//! The Manage drawer: verify, repair, update and uninstall an installed game, and download GOG's
//! offline installers of any GOG game.

use crate::theme::text;
use iced::widget::{Column, button, column, container, row, scrollable};
use iced::{Element, Length, Padding};
use slatty_core::library::LibraryGame;
use slatty_core::maintenance::Change;

use super::game_settings::{stacked, titled};
use super::note;
use crate::icons::Icon;
use crate::maintenance::MaintenanceMsg;
use crate::theme::{self, tokens};
use crate::{App, Message, Panel};

type Style = fn(&iced::Theme, button::Status) -> button::Style;

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
        if slatty_core::store::is_local(&g.id) {
            return self.manage_added(g, view.is_some_and(|v| v.confirm_uninstall), busy);
        }
        if !self.installs.contains_key(&g.id) {
            return self.offline_section(g);
        }
        let on = |msg| (!busy).then(|| Message::Maintenance(msg));
        let files = column![
            row![
                action(
                    "Verify files",
                    on(MaintenanceMsg::Check(id(), false)),
                    theme::tonal
                ),
                action(
                    "Repair",
                    on(MaintenanceMsg::Check(id(), true)),
                    theme::tonal
                ),
            ]
            .spacing(10),
            note(
                "Verify checks every file against GOG's checksums; Repair downloads again the \
                 missing or damaged ones."
            ),
        ]
        .spacing(10);
        let mut updates = row![action(
            "Check for update",
            on(MaintenanceMsg::CheckUpdate(id())),
            theme::tonal
        )]
        .spacing(10);
        if view.is_some_and(|v| v.update_available) {
            updates = updates.push(action(
                "Update now",
                on(MaintenanceMsg::Apply(id(), Change::Update)),
                theme::primary,
            ));
        }
        let updates =
            column![updates, note("Only the files that changed are downloaded.")].spacing(10);
        let mut sections = vec![
            titled(Icon::FileCheck, "Files", files.into(), 0.0),
            titled(Icon::RefreshCw, "Updates", updates.into(), 0.0),
        ];
        // What the last action found, and the progress of the running one.
        if let Some(v) = view.filter(|v| !v.lines.is_empty() || v.progress.is_some()) {
            sections.push(
                Column::new()
                    .extend(v.lines.iter().map(|l| note(l.as_str())))
                    .extend(self.maintenance_progress(&g.id))
                    .spacing(10)
                    .into(),
            );
        }
        sections.push(self.offline_section(g));
        let uninstall = if view.is_some_and(|v| v.confirm_uninstall && !v.busy) {
            column![
                note(
                    "Only files installed by Slatty are deleted; anything else in the \
                     folder is kept. The Wine prefix holds most local saves."
                ),
                choice(
                    "Uninstall, keep the prefix",
                    MaintenanceMsg::Uninstall(id(), false),
                    theme::danger,
                ),
                choice(
                    "Also delete the prefix (backed up)",
                    MaintenanceMsg::Uninstall(id(), true),
                    theme::danger,
                ),
                choice(
                    "Cancel",
                    MaintenanceMsg::CancelUninstall(id()),
                    theme::tonal
                ),
            ]
        } else {
            column![
                action(
                    "Uninstall…",
                    on(MaintenanceMsg::AskUninstall(id())),
                    theme::tonal
                ),
                note("Only files Slatty installed are deleted."),
            ]
        };
        sections.push(titled(
            Icon::Trash,
            "Uninstall",
            uninstall.spacing(10).into(),
            0.0,
        ));
        stacked(sections)
    }

    /// GOG's offline installers, opened in their own drawer, and where their download stands.
    fn offline_section<'a>(&'a self, g: &'a LibraryGame) -> Element<'a, Message> {
        let status = if self.offline_run.as_ref().is_some_and(|r| r.game_id == g.id) {
            "Downloading, in the Downloads tab."
        } else if self.queue.contains(&crate::offline::entry(&g.id)) {
            "Waiting in the Downloads tab."
        } else if self.offline_stopped.iter().any(|(id, _)| *id == g.id) {
            "Stopped: resume it in the Downloads tab."
        } else {
            "GOG's setup programs, kept in a folder to install the game without GOG."
        };
        titled(
            Icon::Download,
            "Offline installers",
            column![
                action(
                    "Download installers…",
                    Some(Message::OpenPanel(Panel::Installers)),
                    theme::tonal
                ),
                note(status),
            ]
            .spacing(10)
            .into(),
            0.0,
        )
    }

    /// A game added by hand: nothing to verify or update from GOG, only taking it out of the
    /// library, its files left where they are.
    fn manage_added<'a>(
        &'a self,
        g: &'a LibraryGame,
        confirm: bool,
        busy: bool,
    ) -> Element<'a, Message> {
        let id = || g.id.clone();
        let mut col = Column::new();
        if confirm && !busy {
            let install = self.installs.get(&g.id);
            let prefix = install.and_then(|i| i.runner.prefix());
            // Its installer put the game itself in its prefix.
            let in_prefix = install
                .zip(prefix)
                .is_some_and(|(i, p)| i.path.starts_with(p));
            col = col
                .push(note(match (prefix.is_some(), in_prefix) {
                    (_, true) => {
                        "Only what Slatty unpacked for it is deleted. The game is in \
                         the Wine prefix Slatty made for it: deleting the prefix deletes \
                         the game, copied to the backups first with the prefix's saves."
                    }
                    (true, false) => {
                        "Only what Slatty unpacked for it is deleted: your own files stay \
                         where they are. The Wine prefix Slatty made for it holds most of \
                         its saves."
                    }
                    (false, _) => {
                        "Only what Slatty unpacked for it is deleted: your own files stay \
                         where they are."
                    }
                }))
                .push(choice(
                    if prefix.is_some() {
                        "Remove, keep the prefix"
                    } else {
                        "Remove from library"
                    },
                    MaintenanceMsg::Uninstall(id(), false),
                    theme::danger,
                ));
            if prefix.is_some() {
                col = col.push(choice(
                    "Also delete the prefix (backed up)",
                    MaintenanceMsg::Uninstall(id(), true),
                    theme::danger,
                ));
            }
            col = col.push(choice(
                "Cancel",
                MaintenanceMsg::CancelUninstall(id()),
                theme::tonal,
            ));
        } else {
            col = col
                .push(action(
                    "Remove from library…",
                    (!busy).then(|| Message::Maintenance(MaintenanceMsg::AskUninstall(id()))),
                    theme::tonal,
                ))
                .push(note("Your own files stay where they are."));
        }
        if let Some(v) = self.maintenance.get(&g.id) {
            col = col.extend(v.lines.iter().map(|l| note(l.as_str())));
        }
        titled(Icon::Trash, "Remove", col.spacing(10).into(), 0.0)
    }
}

/// A button of a section, as wide as its label.
fn action(label: &str, on_press: Option<Message>, style: Style) -> Element<'_, Message> {
    button(text(label).size(14))
        .padding([10, 16])
        .on_press_maybe(on_press)
        .style(style)
        .into()
}

/// One of the answers to an uninstall, across the drawer.
fn choice(label: &str, msg: MaintenanceMsg, style: Style) -> Element<'_, Message> {
    button(container(text(label).size(14)).center_x(Length::Fill))
        .padding([12, 0])
        .width(Length::Fill)
        .on_press(Message::Maintenance(msg))
        .style(style)
        .into()
}
