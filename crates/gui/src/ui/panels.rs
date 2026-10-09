//! Tool panels opened over the game page.

use crate::theme::text;
use iced::widget::{
    Column, Space, button, center, checkbox, column, container, mouse_area, opaque, progress_bar,
    row, scrollable, space, stack,
};
use iced::{Alignment, Element, Length, Padding};
use slatty_core::install::Install;
use slatty_core::installer::DlcChoice;
use slatty_core::library::LibraryGame;
use slatty_core::runner::Runner;

use super::achievements::unlock_all_button;
use super::format::*;
use super::{note, round_button};
use crate::achievements::by_rarity;
use crate::icons::{Icon, icon};
use crate::maintenance::MaintenanceMsg;
use crate::theme::{self, bold, semibold, tokens};
use crate::{App, Loadable, Message, Panel};

/// Width of the drawers beside the game page.
pub(super) const DRAWER_WIDTH: f32 = 500.0;
/// Narrowest game page the drawer opens beside.
const PAGE_BESIDE_DRAWER: f32 = 760.0;

impl App {
    pub fn with_panel<'a>(
        &'a self,
        page: Element<'a, Message>,
        panel: Panel,
        g: &'a LibraryGame,
    ) -> Element<'a, Message> {
        let (title, content) = match panel {
            Panel::Install => return self.install_drawer(page, g),
            Panel::GameSettings => return self.game_settings_drawer(page, g),
            Panel::Manage => return self.manage_drawer(page, g),
            Panel::Cloud => return self.cloud_drawer(page, g),
            Panel::Achievements => return self.achievements_drawer(page, g),
            Panel::Session => ("Session", self.session_panel(g)),
        };
        let boxed = container(
            column![
                row![
                    column![
                        text(title).size(24).font(bold()),
                        text(&g.title).size(14).color(tokens().muted)
                    ]
                    .spacing(2)
                    .width(Length::Fill),
                    round_button(Icon::X, Message::ClosePanel),
                ]
                .align_y(Alignment::Center),
                scrollable(content).style(theme::scroller).spacing(8),
            ]
            .spacing(18),
        )
        .padding(26)
        .max_width(720)
        .max_height(760)
        .style(theme::card);
        stack![
            page,
            mouse_area(
                container(Space::new())
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .style(theme::backdrop)
            )
            .on_press(Message::ClosePanel),
            center(opaque(boxed)),
        ]
        .into()
    }

    /// Progress bar and Pause button of a running verify, repair or update.
    pub(super) fn maintenance_progress(&self, game_id: &str) -> Option<Element<'_, Message>> {
        let v = self.maintenance.get(game_id)?;
        let p = v.progress?;
        Some(
            column![
                progress_bar(0.0..=1.0, fraction(p))
                    .girth(10)
                    .style(theme::progress),
                row![
                    note(format!(
                        "{} / {} · files {}/{}",
                        human_size(p.bytes_done),
                        human_size(p.bytes_total),
                        p.files_done,
                        p.files_total
                    )),
                    space().width(Length::Fill),
                    button(text("Pause").size(14))
                        .padding([8, 16])
                        .on_press(Message::Maintenance(MaintenanceMsg::Pause(
                            game_id.to_string()
                        )))
                        .style(theme::tonal),
                ]
                .align_y(Alignment::Center),
            ]
            .spacing(8)
            .into(),
        )
    }

    pub(super) fn maintenance_busy(&self, game_id: &str) -> bool {
        self.maintenance.get(game_id).is_some_and(|v| v.busy)
            || self
                .play
                .as_ref()
                .is_some_and(|p| p.running && p.game_id == game_id)
    }

    /// Achievements open in a drawer beside the game page, which stays visible.
    fn achievements_drawer<'a>(
        &'a self,
        page: Element<'a, Message>,
        g: &'a LibraryGame,
    ) -> Element<'a, Message> {
        let rule = || {
            container(Space::new())
                .width(Length::Fill)
                .height(1)
                .style(theme::divider)
        };
        let body: Element<'_, Message> = match self.achievements.get(&g.id) {
            Some(Loadable::Ready(list)) => {
                let unlocked = list.iter().filter(|a| a.date_unlocked.is_some()).count();
                let mut items = Column::new();
                for (i, a) in by_rarity(list).into_iter().enumerate() {
                    if i > 0 {
                        items = items.push(rule());
                    }
                    items = items
                        .push(container(self.achievement_row(&g.id, a, 56.0)).padding([14, 0]));
                }
                column![
                    row![
                        text(format!("{unlocked} / {} unlocked", list.len()))
                            .size(16)
                            .font(semibold())
                            .width(Length::Fill),
                        unlock_all_button(&g.id, list),
                    ]
                    .align_y(Alignment::Center),
                ]
                .extend(self.pending_confirmation(&g.id))
                .push(rule())
                .push(
                    scrollable(container(items).padding(Padding::ZERO.right(14)))
                        .style(theme::scroller)
                        .height(Length::Fill),
                )
                .spacing(14)
                .into()
            }
            _ => note("Loading…"),
        };
        let subtitle = text(&g.title).size(18).color(tokens().accent);
        self.drawer(page, "Achievements", subtitle.into(), body, None)
    }

    /// A panel docked to the right of the game page: beside it while the page keeps room for its
    /// cards, over it in a narrow window. The footer stays at the bottom, under a rule.
    pub(super) fn drawer<'a>(
        &self,
        page: Element<'a, Message>,
        title: &'a str,
        subtitle: Element<'a, Message>,
        body: Element<'a, Message>,
        footer: Option<Element<'a, Message>>,
    ) -> Element<'a, Message> {
        let close = button(container(icon(Icon::X, 20.0, tokens().text)).center(24))
            .padding(10)
            .on_press(Message::ClosePanel)
            .style(theme::tonal);
        let header = row![
            column![text(title).size(32).font(bold()), subtitle]
                .spacing(4)
                .width(Length::Fill),
            close,
        ];
        let mut content = column![header, container(body).height(Length::Fill)].spacing(22);
        if let Some(footer) = footer {
            content = content.push(
                column![
                    container(Space::new())
                        .width(Length::Fill)
                        .height(1)
                        .style(theme::divider),
                    footer,
                ]
                .spacing(22),
            );
        }
        let drawer = container(content)
            .padding([22, 24])
            .width(DRAWER_WIDTH)
            .height(Length::Fill)
            .style(theme::drawer);
        // Beside the page while it keeps room for its cards, over it in a narrow window.
        if self.window.width - DRAWER_WIDTH < PAGE_BESIDE_DRAWER {
            return stack![
                page,
                mouse_area(
                    container(Space::new())
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .style(theme::backdrop)
                )
                .on_press(Message::ClosePanel),
                container(opaque(drawer)).align_right(Length::Fill),
            ]
            .into();
        }
        row![
            container(page).width(Length::Fill),
            container(Space::new())
                .width(1)
                .height(Length::Fill)
                .style(theme::divider),
            drawer,
        ]
        .into()
    }

    pub(super) fn session_panel<'a>(&'a self, g: &'a LibraryGame) -> Element<'a, Message> {
        let Some(p) = self.play.as_ref().filter(|p| p.game_id == g.id) else {
            return note("No session yet.");
        };
        let mut col = Column::with_children(p.log.iter().map(|l| note(l.as_str()))).spacing(6);
        if p.running {
            col = col.push(
                button(text("Stop game").size(14))
                    .padding([10, 16])
                    .on_press(Message::StopGame)
                    .style(theme::danger),
            );
        }
        col.into()
    }
}

/// One DLC line: owned DLC can be ticked, others are shown as not owned.
pub(super) fn dlc_row<'a>(
    d: &'a DlcChoice,
    checked: bool,
    editable: bool,
    on_toggle: impl Fn(bool) -> Message + 'a,
) -> Element<'a, Message> {
    checkbox(checked)
        .label(format!("{} ({})", d.name, human_size(d.disk_size)))
        .text_size(14)
        .font(theme::font())
        .on_toggle_maybe(editable.then_some(on_toggle))
        .into()
}

pub(super) fn runner_label(install: &Install) -> String {
    match &install.runner {
        Runner::Native => "Native".into(),
        Runner::Umu { proton, .. } => format!(
            "Proton (umu): {}",
            proton
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
        ),
        Runner::Wine { wine, .. } => format!("Wine: {}", wine.display()),
    }
}

impl App {
    /// A game's dialog over the library: its cover with a title and subtitle beside it, a close
    /// button, then `body`. The library stays where it was underneath.
    pub(super) fn library_dialog<'a>(
        &'a self,
        page: Element<'a, Message>,
        g: &'a LibraryGame,
        title: &'a str,
        details: Vec<Element<'a, Message>>,
        body: Element<'a, Message>,
    ) -> Element<'a, Message> {
        const COVER: (f32, f32) = (94.0, 125.0);
        let cover: Element<'a, Message> = match self.cover(&g.id) {
            Some(h) => iced::widget::image(h)
                .content_fit(iced::ContentFit::Cover)
                .width(COVER.0)
                .height(COVER.1)
                .border_radius(tokens().cover_radius)
                .into(),
            None => container(Space::new())
                .width(COVER.0)
                .height(COVER.1)
                .style(theme::placeholder)
                .into(),
        };
        let header = Column::new()
            .push(text(title).size(30).font(bold()))
            .push(text(&g.title).size(18).color(tokens().muted))
            .extend(details)
            .spacing(4);
        // The title and details centred against the cover; the close button stays at the top.
        let top = row![
            cover,
            container(header).center_y(COVER.1).width(Length::Fill),
            round_button(Icon::X, Message::CloseDialog)
        ]
        .spacing(22)
        .align_y(Alignment::Start);
        let dialog = container(
            scrollable(
                column![top, body]
                    .spacing(22)
                    .padding(Padding::ZERO.right(10)),
            )
            .style(theme::scroller),
        )
        .padding(28)
        .max_width(640)
        .max_height(self.window.height - 60.0)
        .style(theme::card);
        stack![
            page,
            mouse_area(
                container(Space::new())
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .style(theme::backdrop)
            )
            .on_press(Message::CloseDialog),
            center(opaque(dialog)),
        ]
        .into()
    }
}
