//! The interface: the window shell (top bar, notices, quit dialog) and widgets shared by pages.

mod achievements;
pub mod format;
mod game;
mod library;
mod panels;
mod settings;

use iced::widget::{
    Column, Space, button, center, column, container, image, mouse_area, opaque, row, space, stack,
    text, text_input,
};
use iced::{Alignment, ContentFit, Element, Length, Padding};
use slatty_core::library::LibraryGame;

use crate::icons::{Icon, icon};
use crate::theme::{self, ACCENT, BOLD, MUTED, SEMIBOLD, TEXT};
use crate::{App, Message, Page};

impl App {
    pub fn view(&self) -> Element<'_, Message> {
        if let Some(e) = &self.fatal {
            return container(text(format!("Cannot start: {e}")).size(18))
                .padding(40)
                .into();
        }
        if self.core.is_none() {
            return container(text("Loading…").color(MUTED)).padding(40).into();
        }
        let Some(account) = &self.account else {
            return self.with_notice(self.login_view());
        };
        let body: Element<'_, Message> = match self.selected_game() {
            Some(game) => self.game_page(game),
            None => column![
                self.top_bar(&account.username),
                match self.page {
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
                }
            ]
            .spacing(14)
            .into(),
        };
        let page = self.with_notice(container(body).padding(Padding::new(24.0).top(16.0)).into());
        let page = match (self.panel, self.selected_game()) {
            (Some(panel), Some(game)) => self.with_panel(page, panel, game),
            _ => page,
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
                        theme::DANGER
                    } else {
                        theme::SUCCESS
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

    fn top_bar(&self, username: &str) -> Element<'_, Message> {
        let tab = |label, page: Page| {
            button(text(label).size(15).font(SEMIBOLD))
                .padding([8, 22])
                .on_press(Message::ShowPage(page))
                .style(theme::segment(self.page == page && self.selected.is_none()))
        };
        let tabs = container(
            row![
                tab("Library", Page::Library),
                tab("Achievements", Page::Achievements),
                tab("Settings", Page::Settings),
            ]
            .spacing(4),
        )
        .padding(4)
        .style(theme::pill);
        let search = container(
            row![
                icon(Icon::Search, 18.0, MUTED),
                text_input("Search a game", &self.search)
                    .on_input(Message::Search)
                    .style(theme::input)
                    .size(15)
                    .padding(0),
            ]
            .spacing(10)
            .align_y(Alignment::Center),
        )
        .padding([10, 16])
        .width(320)
        .style(theme::pill);
        let initial = username
            .chars()
            .next()
            .map(|c| c.to_lowercase().to_string())
            .unwrap_or_default();
        let picture: Element<'_, Message> =
            match self.avatar.as_ref().and_then(|u| self.images.get(u)) {
                Some(h) => image(h.clone())
                    .content_fit(ContentFit::Cover)
                    .width(38)
                    .height(38)
                    .border_radius(19)
                    .into(),
                None => container(text(initial).size(17).font(BOLD))
                    .center(38)
                    .style(theme::avatar)
                    .into(),
            };
        let avatar = button(picture)
            .padding(0)
            .on_press(Message::ShowPage(Page::Settings))
            .style(theme::plain);
        row![
            logo(),
            Space::new().width(18),
            tabs,
            space().width(Length::Fill),
            search,
            avatar,
        ]
        .spacing(14)
        .align_y(Alignment::Center)
        .into()
    }
}

pub(super) fn logo<'a>() -> Element<'a, Message> {
    text("slatty").size(30).font(BOLD).color(ACCENT).into()
}

/// A titled block of the settings page or of a panel.
pub(super) fn card<'a>(title: &'a str, items: Vec<Element<'a, Message>>) -> Element<'a, Message> {
    container(
        column![
            text(title).size(17).font(SEMIBOLD),
            Column::with_children(items).spacing(10)
        ]
        .spacing(14),
    )
    .padding(20)
    .width(Length::Fill)
    .style(theme::card)
    .into()
}

pub(super) fn round_button<'a>(ic: Icon, msg: Message) -> Element<'a, Message> {
    button(container(icon(ic, 20.0, TEXT)).center(24))
        .padding(10)
        .on_press(msg)
        .style(theme::icon_button)
        .into()
}

fn note<'a>(s: impl text::IntoFragment<'a>) -> Element<'a, Message> {
    text(s).size(14).color(MUTED).into()
}

fn inner(_: &iced::Theme) -> container::Style {
    container::Style {
        background: Some(iced::Background::Color(theme::SURFACE_HIGH)),
        border: iced::border::rounded(14),
        ..Default::default()
    }
}

/// Asks before closing the window while something is still running.
fn quit_dialog<'a>(page: Element<'a, Message>, work: &'a [String]) -> Element<'a, Message> {
    let items = work.iter().map(|w| {
        row![
            icon(Icon::TriangleAlert, 18.0, theme::WARNING),
            text(w).size(14).color(MUTED).width(Length::Fill)
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
