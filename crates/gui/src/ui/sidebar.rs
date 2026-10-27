//! The list of every game beside the library, or the strip it closes to.

use crate::theme::text;
use iced::widget::{
    Space, button, column, container, image, mouse_area, row, scrollable, text_input,
};
use iced::{Alignment, Element, Length, Padding, mouse};
use slatty_core::library::LibraryGame;

use super::widgets::grid_rows;
use crate::icons::{Icon, icon};
use crate::sidebar::{ROW, SidebarMsg};
use crate::theme::{self, tokens};
use crate::{App, Message};

/// Size of a game's icon in the list, and width of its edge that can be grabbed.
const ICON: f32 = 20.0;
const EDGE: f32 = 6.0;

impl App {
    pub(super) fn sidebar_view(&self) -> Element<'_, Message> {
        let toggle = |glyph| {
            button(icon(glyph, 18.0, tokens().muted))
                .padding(7)
                .on_press(Message::Sidebar(SidebarMsg::Toggle))
                .style(theme::ghost)
        };
        // Closed, it takes no room: its place stays, so the page beside it keeps its own (the
        // grid's scroll); the library's toolbar opens it again.
        if !self.sidebar.open {
            return Space::new().width(0).into();
        }
        let search = container(
            row![
                icon(Icon::Search, 16.0, tokens().muted),
                text_input("Search", &self.sidebar.search)
                    .on_input(|s| Message::Sidebar(SidebarMsg::Search(s)))
                    .style(theme::input)
                    .font(theme::font())
                    .size(14)
                    .padding(0),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        )
        .padding([7, 12])
        .width(Length::Fill)
        .style(theme::outlined);
        let head = row![search, toggle(Icon::PanelLeftClose)]
            .spacing(6)
            .align_y(Alignment::Center);
        let games = self.sidebar_games();
        let list: Element<'_, Message> = if games.is_empty() {
            container(text("No game matches.").size(13).color(tokens().muted))
                .padding([4, 8])
                .into()
        } else {
            // Only the rows in view are built, one child a row so each keeps its place.
            let window = self.sidebar_window(games.len());
            let rows = grid_rows(&window, games.len(), 0.0, |i| {
                self.sidebar_row(&self.library[games[i]])
            });
            scrollable(rows)
                .on_scroll(|v| Message::Sidebar(SidebarMsg::Scrolled(v)))
                .style(theme::scroller)
                .height(Length::Fill)
                .into()
        };
        // Its edge, a line a few pixels wide to grab, dragged to widen the list.
        let edge = mouse_area(
            container(
                container(Space::new())
                    .width(1)
                    .height(Length::Fill)
                    .style(theme::divider),
            )
            .center_x(EDGE)
            .height(Length::Fill),
        )
        .on_press(Message::Sidebar(SidebarMsg::Grab))
        .interaction(mouse::Interaction::ResizingHorizontally);
        row![
            container(column![head, list].spacing(10))
                .padding(Padding::new(10.0).top(16.0).left(12.0).right(10.0 - EDGE))
                .width(Length::Fill)
                .height(Length::Fill),
            edge,
        ]
        .width(self.sidebar.width)
        .height(Length::Fill)
        .into()
    }

    /// A game's icon, when it has one, and its name, dimmed when not installed, as Steam's list
    /// shows them; a click opens its page.
    fn sidebar_row<'a>(&'a self, g: &'a LibraryGame) -> Element<'a, Message> {
        let shown_icon: Element<'_, Message> = match self.icon(g) {
            Some(handle) => image(handle).width(ICON).height(ICON).into(),
            None => Space::new().width(ICON).height(ICON).into(),
        };
        let color = match self.is_installed(&g.id) {
            true => tokens().text,
            false => tokens().muted,
        };
        let name = container(
            text(&g.title)
                .size(14)
                .color(color)
                .wrapping(iced::widget::text::Wrapping::None),
        )
        .width(Length::Fill)
        .clip(true);
        button(
            row![shown_icon, name]
                .spacing(10)
                .align_y(Alignment::Center),
        )
        .padding([0, 8])
        .width(Length::Fill)
        .height(ROW)
        .on_press(Message::Select(g.id.clone()))
        .style(theme::side_entry(false))
        .into()
    }
}
