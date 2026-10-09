//! The Manage drawer: verify, repair, update and uninstall an installed game.

use iced::widget::{Column, Space, button, column, container, row, scrollable, text};
use iced::{Alignment, Element, Length, Padding};
use slatty_core::library::LibraryGame;
use slatty_core::maintenance::Change;

use super::{inner, note};
use crate::icons::{Icon, icon};
use crate::maintenance::MaintenanceMsg;
use crate::theme::{self, SEMIBOLD, tokens};
use crate::{App, Message};

impl App {
    pub(super) fn manage_drawer<'a>(
        &'a self,
        page: Element<'a, Message>,
        g: &'a LibraryGame,
    ) -> Element<'a, Message> {
        let subtitle = text(&g.title).size(18).color(tokens().muted);
        let body = scrollable(container(self.manage(g)).padding(Padding::ZERO.right(14)))
            .style(theme::scroller)
            .height(Length::Fill);
        self.drawer(page, "Manage", subtitle.into(), body.into(), None)
    }

    fn manage<'a>(&'a self, g: &'a LibraryGame) -> Element<'a, Message> {
        let view = self.maintenance.get(&g.id);
        let busy = self.maintenance_busy(&g.id);
        let id = || g.id.clone();
        let action = |ic, label, msg: MaintenanceMsg| {
            action_row(ic, label, (!busy).then(|| Message::Maintenance(msg)), false)
        };
        let mut col = column![
            action(
                Icon::FileCheck,
                "Verify files",
                MaintenanceMsg::Check(id(), false)
            ),
            action(Icon::Wrench, "Repair", MaintenanceMsg::Check(id(), true)),
            action(
                Icon::RefreshCw,
                "Check for update",
                MaintenanceMsg::CheckUpdate(id())
            ),
        ]
        .spacing(12);
        if view.is_some_and(|v| v.update_available) {
            col = col.push(
                button(
                    container(text("Update now").size(16).font(SEMIBOLD)).center_x(Length::Fill),
                )
                .padding([14, 0])
                .width(Length::Fill)
                .on_press_maybe(
                    (!busy)
                        .then(|| Message::Maintenance(MaintenanceMsg::Apply(id(), Change::Update))),
                )
                .style(theme::primary),
            );
        }
        // What the last action found, and the progress of the running one.
        if let Some(v) = view {
            col = col
                .extend(v.lines.iter().map(|l| note(l.as_str())))
                .extend(self.maintenance_progress(&g.id));
        }
        col = col.push(Space::new().height(4)).push(
            container(Space::new())
                .width(Length::Fill)
                .height(1)
                .style(theme::divider),
        );
        col = col.push(Space::new().height(4)).push(action_row(
            Icon::Trash,
            "Uninstall…",
            (!busy).then(|| Message::Maintenance(MaintenanceMsg::AskUninstall(id()))),
            true,
        ));
        if view.is_some_and(|v| v.confirm_uninstall && !v.busy) {
            let choice =
                |label,
                 msg: MaintenanceMsg,
                 style: fn(&iced::Theme, button::Status) -> button::Style| {
                    button(container(text(label).size(14)).center_x(Length::Fill))
                        .padding([12, 0])
                        .width(Length::Fill)
                        .on_press(Message::Maintenance(msg))
                        .style(style)
                };
            col = col.push(
                container(
                    Column::new()
                        .push(note(
                            "Only files installed by slatty are deleted; anything else in the \
                             folder is kept. The Wine prefix holds most local saves.",
                        ))
                        .push(choice(
                            "Uninstall, keep the prefix",
                            MaintenanceMsg::Uninstall(id(), false),
                            theme::danger,
                        ))
                        .push(choice(
                            "Also delete the prefix (backed up)",
                            MaintenanceMsg::Uninstall(id(), true),
                            theme::danger,
                        ))
                        .push(choice(
                            "Cancel",
                            MaintenanceMsg::CancelUninstall(id()),
                            theme::tonal,
                        ))
                        .spacing(10),
                )
                .padding(16)
                .width(Length::Fill)
                .style(inner),
            );
        }
        col.into()
    }
}

/// One action of the drawer: its icon, its name, and a chevron.
fn action_row<'a>(
    ic: Icon,
    label: &'a str,
    on_press: Option<Message>,
    danger: bool,
) -> Element<'a, Message> {
    let t = tokens();
    let color = match (on_press.is_some(), danger) {
        (false, _) => t.muted,
        (true, true) => t.danger,
        (true, false) => t.text,
    };
    button(
        row![
            icon(ic, 24.0, if danger { t.danger } else { t.accent }),
            text(label).size(16).color(color).width(Length::Fill),
            icon(Icon::ChevronRight, 18.0, t.muted),
        ]
        .spacing(18)
        .align_y(Alignment::Center),
    )
    .padding([18, 20])
    .width(Length::Fill)
    .on_press_maybe(on_press)
    .style(theme::action_row)
    .into()
}
