//! The interface: the window shell (top bar, notices, quit dialog) and widgets shared by pages.

mod achievements;
mod edit;
pub mod format;
mod game;
mod library;
mod panels;
mod settings;
mod widgets;

use iced::widget::{
    Column, Space, button, center, column, container, mouse_area, opaque, row, space, stack, text,
    text_input,
};
use iced::{Alignment, Element, Length, Padding};
use slatty_core::library::LibraryGame;

use crate::icons::{Icon, icon};
use crate::theme::{self, BOLD, tokens};
use crate::{App, Message, Page};
use widgets::{avatar, icon_tab, logo, nav_tab, vertical_rule};
pub(super) use widgets::{card, inner, note, round_button};

impl App {
    pub fn view(&self) -> Element<'_, Message> {
        if let Some(e) = &self.fatal {
            return container(text(format!("Cannot start: {e}")).size(18))
                .padding(40)
                .into();
        }
        if self.core.is_none() {
            return container(text("Loading…").color(tokens().muted))
                .padding(40)
                .into();
        }
        let Some(account) = &self.account else {
            return self.with_notice(self.login_view());
        };
        let body: Element<'_, Message> = match self.selected_game() {
            Some(game) => self.game_page(game),
            None => column![
                self.top_bar(&account.username),
                container(match self.page {
                    Page::Library => self.library_page(),
                    Page::Achievements => match self
                        .achievements_game
                        .as_ref()
                        .and_then(|id| self.library.iter().find(|g| &g.id == id))
                    {
                        Some(game) => self.achievements_game_page(game),
                        None => self.achievements_page(),
                    },
                    Page::Settings => self.settings_page(),
                })
                .padding(Padding::new(20.0).top(16.0)),
            ]
            .into(),
        };
        let page = self.with_notice(body);
        let page = match (self.panel, self.selected_game()) {
            (Some(panel), Some(game)) => self.with_panel(page, panel, game),
            _ => page,
        };
        let page = match &self.edit {
            Some(d) => self.edit_dialog(page, d),
            None => page,
        };
        match &self.quit_confirm {
            Some(work) => quit_dialog(page, work),
            None => page,
        }
    }

    fn selected_game(&self) -> Option<&LibraryGame> {
        let id = self.selected.as_ref()?;
        self.library.iter().find(|g| &g.id == id)
    }

    fn with_notice<'a>(&'a self, body: Element<'a, Message>) -> Element<'a, Message> {
        let Some(n) = &self.notice else {
            return body;
        };
        let banner = container(
            row![
                icon(
                    if n.error {
                        Icon::TriangleAlert
                    } else {
                        Icon::CircleCheck
                    },
                    18.0,
                    if n.error {
                        theme::tokens().danger
                    } else {
                        theme::tokens().success
                    }
                ),
                text(&n.text).size(14).width(Length::Fill),
                button(text("Close").size(13))
                    .on_press(Message::DismissNotice)
                    .style(theme::link),
            ]
            .align_y(Alignment::Center)
            .spacing(12),
        )
        .padding([10, 16])
        .width(Length::Fill)
        .style(theme::notice(n.error));
        column![container(banner).padding([8, 20]), body].into()
    }

    /// Logo, page tabs, search and account, above every page but the game page.
    fn top_bar(&self, username: &str) -> Element<'_, Message> {
        let tab = |ic, label, page| nav_tab(ic, label, self.page == page, Message::ShowPage(page));
        let tabs = row![
            tab(Icon::LayoutGrid, "Library", Page::Library),
            tab(Icon::Trophy, "Achievements", Page::Achievements),
            vertical_rule(24.0),
            icon_tab(
                Icon::Settings,
                "settings-tab",
                self.page == Page::Settings,
                Message::ShowPage(Page::Settings)
            ),
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        let search = container(
            row![
                icon(Icon::Search, 18.0, tokens().muted),
                text_input("Search a game", &self.search)
                    .on_input(Message::Search)
                    .style(theme::input)
                    .size(15)
                    .padding(0),
            ]
            .spacing(10)
            .align_y(Alignment::Center),
        )
        .padding([9, 14])
        .width(360)
        .style(theme::outlined);
        let initial = username
            .chars()
            .next()
            .map(|c| c.to_uppercase().to_string())
            .unwrap_or_default();
        let picture = self
            .avatar
            .as_ref()
            .and_then(|u| self.images.get(u))
            .cloned();
        let account = button(avatar(picture, initial, 34.0))
            .padding(0)
            .on_press(Message::ShowPage(Page::Settings))
            .style(theme::plain);
        let bar = row![
            logo(44.0),
            Space::new().width(12),
            tabs,
            space().width(Length::Fill),
            search,
            account,
        ]
        .spacing(16)
        .align_y(Alignment::Center);
        column![
            container(bar).padding([7, 20]).width(Length::Fill),
            container(Space::new())
                .width(Length::Fill)
                .height(1)
                .style(theme::divider),
        ]
        .into()
    }
}

/// Asks before closing the window while something is still running.
fn quit_dialog<'a>(page: Element<'a, Message>, work: &'a [String]) -> Element<'a, Message> {
    let items = work.iter().map(|w| {
        row![
            icon(Icon::TriangleAlert, 18.0, theme::tokens().warning),
            text(w).size(14).color(tokens().muted).width(Length::Fill)
        ]
        .spacing(12)
        .into()
    });
    let dialog = container(
        column![
            text("Quit SlattyLauncher?").size(24).font(BOLD),
            Column::with_children(items).spacing(10),
            row![
                space().width(Length::Fill),
                button(text("Keep running").size(14))
                    .padding([10, 18])
                    .on_press(Message::CancelQuit)
                    .style(theme::tonal),
                button(text("Quit anyway").size(14))
                    .padding([10, 18])
                    .on_press(Message::ConfirmQuit)
                    .style(theme::danger),
            ]
            .spacing(10),
        ]
        .spacing(18),
    )
    .padding(26)
    .max_width(560)
    .style(theme::card);
    stack![
        page,
        mouse_area(
            container(Space::new())
                .width(Length::Fill)
                .height(Length::Fill)
                .style(theme::backdrop)
        )
        .on_press(Message::CancelQuit),
        center(opaque(dialog)),
    ]
    .into()
}
