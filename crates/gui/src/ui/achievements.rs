//! Achievements tab, the per-game achievements page, and the widgets they share with the game page.

use iced::widget::{
    Column, Space, button, column, container, grid, image, progress_bar, row, scrollable, space,
    text,
};
use iced::{Alignment, ContentFit, Element, Length, Padding};
use slatty_core::achievements::Achievement;
use slatty_core::library::LibraryGame;

use super::{inner, note, round_button};
use crate::achievements::by_rarity;
use crate::icons::{Icon, icon};
use crate::theme::{self, BOLD, SEMIBOLD, tokens};
use crate::{AchievementChange, App, Loadable, Message, Page, PendingChange};

/// Height of a game card on the Achievements tab.
const TILE_HEIGHT: f32 = 186.0;
/// Height of an achievement on a game's achievements page.
const ACHIEVEMENT_HEIGHT: f32 = 112.0;

impl App {
    pub(super) fn achievements_page(&self) -> Element<'_, Message> {
        let games = self.games_by_achievements();
        let unlocked: usize = games.iter().map(|g| g.1).sum();
        let total: usize = games.iter().map(|g| g.2).sum();
        let perfect = games.iter().filter(|g| g.1 == g.2).count();
        let scanning = self
            .overview_status()
            .unwrap_or_else(|| format!("{} games", games.len()));
        let header = row![
            column![
                text("Achievements").size(32).font(BOLD),
                text(format!(
                    "{unlocked} / {total} unlocked · {scanning} · {perfect} completed"
                ))
                .size(14)
                .color(tokens().muted),
            ]
            .spacing(4)
            .width(Length::Fill),
            button(
                row![
                    icon(Icon::RefreshCw, 16.0, tokens().text),
                    text("Refresh").size(14)
                ]
                .spacing(8)
                .align_y(Alignment::Center)
            )
            .padding([10, 16])
            .on_press_maybe((!self.overview_busy).then_some(Message::ScanOverview))
            .style(theme::tonal),
        ]
        .align_y(Alignment::Center);
        let cards: Vec<Element<'_, Message>> = games
            .into_iter()
            .map(|(g, done, total)| self.achievement_tile(g, done, total))
            .collect();
        column![
            header,
            scrollable(grid(cards).fluid(540).spacing(14).height(Length::Shrink))
                .spacing(8)
                .style(theme::scroller)
                .height(Length::Fill)
        ]
        .spacing(20)
        .padding(Padding::ZERO.top(4))
        .into()
    }

    /// One game of the Achievements tab: cover, title and progress.
    fn achievement_tile<'a>(
        &'a self,
        g: &'a LibraryGame,
        done: usize,
        total: usize,
    ) -> Element<'a, Message> {
        let cover_height = TILE_HEIGHT - 16.0;
        let cover_width = cover_height * 3.0 / 4.0;
        let cover: Element<'_, Message> = match self.cover(&g.id) {
            Some(h) => image(h)
                .content_fit(ContentFit::Cover)
                .width(cover_width)
                .height(cover_height)
                .border_radius(tokens().cover_radius)
                .into(),
            None => container(Space::new())
                .width(cover_width)
                .height(cover_height)
                .style(theme::placeholder)
                .into(),
        };
        let share = done as f32 / total as f32;
        let details = column![
            text(&g.title).size(17).font(SEMIBOLD),
            space().height(Length::Fill),
            row![
                text(format!("{done} / {total}")).size(16),
                space().width(Length::Fill),
                text(format!("{:.0}%", share * 100.0))
                    .size(15)
                    .color(tokens().muted),
            ],
            progress_bar(0.0..=1.0, share)
                .girth(8)
                .style(theme::progress),
        ]
        .spacing(10)
        .padding(Padding::new(16.0).left(6.0).bottom(30.0))
        .height(Length::Fill);
        button(row![cover, details].spacing(14).height(Length::Fill))
            .padding(8)
            .width(Length::Fill)
            .height(TILE_HEIGHT)
            .on_press(Message::OpenAchievements(g.id.clone()))
            .style(theme::tile)
            .into()
    }

    pub(super) fn achievements_game_page<'a>(&'a self, g: &'a LibraryGame) -> Element<'a, Message> {
        let header = row![
            round_button(Icon::ChevronLeft, Message::ShowPage(Page::Achievements)),
            text("All games").size(16),
            space().width(Length::Fill),
            button(
                row![
                    text("Game page").size(14),
                    icon(Icon::ArrowRight, 16.0, tokens().accent)
                ]
                .spacing(6)
                .align_y(Alignment::Center)
            )
            .on_press(Message::Select(g.id.clone()))
            .style(theme::link),
        ]
        .spacing(12)
        .align_y(Alignment::Center);
        let cover: Element<'_, Message> = match self.cover(&g.id) {
            Some(h) => image(h)
                .content_fit(ContentFit::Cover)
                .width(84)
                .height(112)
                .border_radius(tokens().cover_radius)
                .into(),
            None => container(Space::new())
                .width(84)
                .height(112)
                .style(theme::placeholder)
                .into(),
        };
        let body: Element<'_, Message> = match self.achievements.get(&g.id) {
            None | Some(Loadable::Loading) => {
                text("Loading achievements…").color(tokens().muted).into()
            }
            Some(Loadable::Failed(e)) => column![
                text(format!("Unavailable: {e}")).color(tokens().muted),
                button(text("Retry").size(14))
                    .on_press(Message::LoadAchievements(g.id.clone()))
                    .style(theme::link),
            ]
            .spacing(8)
            .into(),
            Some(Loadable::Ready(list)) => {
                let done = list.iter().filter(|a| a.date_unlocked.is_some()).count();
                let share = if list.is_empty() {
                    0.0
                } else {
                    done as f32 / list.len() as f32
                };
                let summary = row![
                    cover,
                    column![
                        text(&g.title).size(34).font(BOLD),
                        row![
                            text(format!("{done} / {} unlocked", list.len()))
                                .size(16)
                                .font(SEMIBOLD),
                            text(format!("{:.0}%", share * 100.0))
                                .size(15)
                                .color(tokens().muted),
                        ]
                        .spacing(16),
                        progress_bar(0.0..=1.0, share)
                            .girth(10)
                            .style(theme::progress),
                    ]
                    .spacing(10)
                    .width(Length::Fill),
                    unlock_all_button(&g.id, list),
                ]
                .spacing(22)
                .align_y(Alignment::Center);
                let cards: Vec<Element<'_, Message>> = by_rarity(list)
                    .into_iter()
                    .map(|a| {
                        container(self.achievement_row(&g.id, a, 64.0))
                            .padding([14, 18])
                            .width(Length::Fill)
                            .height(ACHIEVEMENT_HEIGHT)
                            .align_y(Alignment::Center)
                            .style(theme::block)
                            .into()
                    })
                    .collect();
                let mut list_view = Column::new().spacing(14);
                list_view = list_view.extend(self.pending_confirmation(&g.id));
                list_view =
                    list_view.push(grid(cards).fluid(1100).spacing(14).height(Length::Shrink));
                column![
                    summary,
                    scrollable(list_view)
                        .spacing(8)
                        .style(theme::scroller)
                        .height(Length::Fill)
                ]
                .spacing(22)
                .into()
            }
        };
        column![header, body]
            .spacing(18)
            .padding(Padding::ZERO.top(4))
            .into()
    }

    pub fn achievement_icon<'a>(&'a self, url: &str, size: f32) -> Element<'a, Message> {
        match self.images.get(url) {
            Some(h) => image(h.clone())
                .width(size)
                .height(size)
                .border_radius(tokens().cover_radius)
                .into(),
            None => container(icon(Icon::Trophy, size / 2.5, tokens().muted))
                .center(size)
                .style(theme::placeholder)
                .into(),
        }
    }

    pub fn pending_confirmation(&self, game_id: &str) -> Option<Element<'_, Message>> {
        self.pending_change
            .as_ref()
            .filter(|p| p.game_id == game_id)
            .map(confirmation)
    }

    /// One achievement with its icon, description, rarity and Unlock or Clear.
    pub fn achievement_row<'a>(
        &'a self,
        game_id: &'a str,
        a: &'a Achievement,
        icon_size: f32,
    ) -> Element<'a, Message> {
        let done = a.date_unlocked.is_some();
        let shown = a.visible || done;
        let url = if done {
            &a.image_url_unlocked
        } else {
            &a.image_url_locked
        };
        let mut details = column![
            text(if shown {
                a.name.as_str()
            } else {
                "Hidden achievement"
            })
            .size(16)
            .font(SEMIBOLD)
            .color(if done { tokens().text } else { tokens().muted })
        ]
        .spacing(3)
        .width(Length::Fill);
        if shown && !a.description.is_empty() {
            details = details.push(text(&a.description).size(14).color(tokens().muted));
        }
        details = details.push(
            text(format!("{:.1}% of players", a.rarity))
                .size(13)
                .color(tokens().muted),
        );
        row![
            self.achievement_icon(url, icon_size),
            details,
            button(text(if done { "Clear" } else { "Unlock" }).size(14))
                .padding([8, 18])
                .on_press(Message::AskAchievementChange(
                    game_id.to_string(),
                    vec![change(a, !done)],
                ))
                .style(theme::tonal),
        ]
        .spacing(18)
        .align_y(Alignment::Center)
        .into()
    }
}

pub(super) fn change(a: &Achievement, unlock: bool) -> AchievementChange {
    AchievementChange {
        achievement_id: a.achievement_id.clone(),
        name: a.name.clone(),
        unlock,
    }
}

pub(super) fn confirmation(p: &PendingChange) -> Element<'_, Message> {
    let names: Vec<&str> = p.changes.iter().map(|c| c.name.as_str()).collect();
    let verb = if p.changes.iter().all(|c| c.unlock) {
        "Unlock"
    } else if p.changes.iter().all(|c| !c.unlock) {
        "Clear"
    } else {
        "Change"
    };
    container(
        column![
            text(format!(
                "{verb} {} achievement(s) without playing: {}",
                p.changes.len(),
                names.join(", ")
            ))
            .size(14),
            note(
                "The change is written directly to your public GOG profile, \
                 dated today. It is probably against GOG's terms."
            ),
            row![
                button(text("Confirm").size(14))
                    .padding([10, 16])
                    .on_press(Message::ConfirmAchievementChange)
                    .style(theme::danger),
                button(text("Cancel").size(14))
                    .padding([10, 16])
                    .on_press(Message::CancelAchievementChange)
                    .style(theme::tonal),
            ]
            .spacing(8),
        ]
        .spacing(10),
    )
    .padding(16)
    .width(Length::Fill)
    .style(inner)
    .into()
}

pub fn unlock_all_button<'a>(game_id: &str, list: &[Achievement]) -> Element<'a, Message> {
    let locked: Vec<AchievementChange> = list
        .iter()
        .filter(|a| a.date_unlocked.is_none())
        .map(|a| change(a, true))
        .collect();
    let game_id = game_id.to_string();
    button(text("Unlock all").size(14))
        .padding([10, 18])
        .on_press_maybe(
            (!locked.is_empty()).then(|| Message::AskAchievementChange(game_id, locked)),
        )
        .style(theme::tonal)
        .into()
}
