//! Achievements tab, the per-game achievements page, and the widgets they share with the game page.

use iced::widget::{
    Column, Space, button, column, container, image, progress_bar, row, scrollable, space, text,
};
use iced::{Alignment, ContentFit, Element, Length, Padding};
use slatty_core::achievements::Achievement;
use slatty_core::library::LibraryGame;

use super::{inner, note, round_button};
use crate::icons::{Icon, icon};
use crate::theme::{self, ACCENT, BOLD, MUTED, SEMIBOLD, TEXT};
use crate::{AchievementChange, App, Loadable, Message, Page, PendingChange};

impl App {
    pub(super) fn achievements_page(&self) -> Element<'_, Message> {
        let mut games: Vec<(&LibraryGame, usize, usize)> = self
            .library
            .iter()
            .filter_map(|g| {
                let (done, total) = self.overview.get(&g.id)?.achievements?;
                Some((g, done, total))
            })
            .collect();
        games.sort_by(|a, b| {
            let pa = a.1 as f32 / a.2 as f32;
            let pb = b.1 as f32 / b.2 as f32;
            pb.total_cmp(&pa)
                .then_with(|| a.0.title.to_lowercase().cmp(&b.0.title.to_lowercase()))
        });
        let unlocked: usize = games.iter().map(|g| g.1).sum();
        let total: usize = games.iter().map(|g| g.2).sum();
        let perfect = games.iter().filter(|g| g.1 == g.2).count();
        let scanning = self
            .overview_status()
            .unwrap_or_else(|| format!("{} games with achievements", games.len()));
        let header = row![
            column![
                text("Achievements").size(30).font(BOLD),
                text(format!(
                    "{unlocked} / {total} unlocked · {perfect} completed · {scanning}"
                ))
                .size(14)
                .color(MUTED),
            ]
            .spacing(4)
            .width(Length::Fill),
            button(
                row![icon(Icon::RefreshCw, 16.0, TEXT), text("Refresh").size(14)]
                    .spacing(8)
                    .align_y(Alignment::Center)
            )
            .padding([10, 16])
            .on_press_maybe((!self.overview_busy).then_some(Message::ScanOverview))
            .style(theme::tonal),
        ]
        .align_y(Alignment::Center);
        let rows: Vec<Element<'_, Message>> = games
            .into_iter()
            .map(|(g, done, total)| {
                let thumb: Element<'_, Message> = match self.covers.get(&g.id) {
                    Some(h) => image(h.clone())
                        .content_fit(ContentFit::Cover)
                        .width(48)
                        .height(64)
                        .border_radius(8)
                        .into(),
                    None => container(Space::new())
                        .width(48)
                        .height(64)
                        .style(theme::placeholder)
                        .into(),
                };
                let share = done as f32 / total as f32;
                button(
                    row![
                        thumb,
                        column![
                            text(&g.title).size(16).font(SEMIBOLD),
                            progress_bar(0.0..=1.0, share)
                                .girth(8)
                                .style(theme::progress),
                        ]
                        .spacing(10)
                        .width(Length::Fill),
                        text(format!("{done} / {total}"))
                            .size(15)
                            .font(SEMIBOLD)
                            .width(90)
                            .align_x(Alignment::End),
                        text(format!("{:.0}%", share * 100.0))
                            .size(14)
                            .color(MUTED)
                            .width(50)
                            .align_x(Alignment::End),
                    ]
                    .spacing(18)
                    .align_y(Alignment::Center),
                )
                .padding([10, 16])
                .width(Length::Fill)
                .on_press(Message::OpenAchievements(g.id.clone()))
                .style(theme::row_button)
                .into()
            })
            .collect();
        column![
            header,
            scrollable(Column::with_children(rows).spacing(8))
                .spacing(8)
                .style(theme::scroller)
                .height(Length::Fill)
        ]
        .spacing(20)
        .padding(Padding::ZERO.top(10))
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
                    icon(Icon::ArrowRight, 16.0, ACCENT)
                ]
                .spacing(6)
                .align_y(Alignment::Center)
            )
            .on_press(Message::Select(g.id.clone()))
            .style(theme::link),
        ]
        .spacing(12)
        .align_y(Alignment::Center);
        let cover: Element<'_, Message> = match self.covers.get(&g.id) {
            Some(h) => image(h.clone())
                .content_fit(ContentFit::Cover)
                .width(72)
                .height(96)
                .border_radius(10)
                .into(),
            None => container(Space::new())
                .width(72)
                .height(96)
                .style(theme::placeholder)
                .into(),
        };
        let body: Element<'_, Message> = match self.achievements.get(&g.id) {
            None | Some(Loadable::Loading) => text("Loading achievements…").color(MUTED).into(),
            Some(Loadable::Failed(e)) => column![
                text(format!("Unavailable: {e}")).color(MUTED),
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
                        text(&g.title).size(30).font(BOLD),
                        row![
                            text(format!("{done} / {} unlocked", list.len()))
                                .size(15)
                                .font(SEMIBOLD),
                            text(format!("{:.0}%", share * 100.0)).size(14).color(MUTED),
                        ]
                        .spacing(16),
                        progress_bar(0.0..=1.0, share)
                            .girth(8)
                            .style(theme::progress),
                    ]
                    .spacing(10)
                    .width(Length::Fill),
                    unlock_all_button(&g.id, list),
                ]
                .spacing(20)
                .align_y(Alignment::Center);
                let mut rows = Column::new().spacing(8);
                rows = rows.extend(self.pending_confirmation(&g.id));
                rows = rows.extend(list.iter().map(|a| {
                    container(self.achievement_row(&g.id, a))
                        .padding([12, 16])
                        .width(Length::Fill)
                        .style(theme::card)
                        .into()
                }));
                column![
                    summary,
                    scrollable(rows)
                        .spacing(8)
                        .style(theme::scroller)
                        .height(Length::Fill)
                ]
                .spacing(20)
                .into()
            }
        };
        column![header, body]
            .spacing(18)
            .padding(Padding::ZERO.top(10))
            .into()
    }

    pub fn achievement_icon<'a>(&'a self, url: &str, size: f32) -> Element<'a, Message> {
        match self.images.get(url) {
            Some(h) => image(h.clone())
                .width(size)
                .height(size)
                .border_radius(10)
                .into(),
            None => container(icon(Icon::Trophy, size / 2.5, MUTED))
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
            .size(15)
            .font(SEMIBOLD)
            .color(if done { TEXT } else { MUTED })
        ]
        .spacing(2)
        .width(Length::Fill);
        if shown && !a.description.is_empty() {
            details = details.push(text(&a.description).size(13).color(MUTED));
        }
        details = details.push(
            text(format!("{:.1}% of players", a.rarity))
                .size(12)
                .color(MUTED),
        );
        row![
            self.achievement_icon(url, 48.0),
            details,
            button(text(if done { "Clear" } else { "Unlock" }).size(13))
                .padding([6, 12])
                .on_press(Message::AskAchievementChange(
                    game_id.to_string(),
                    vec![change(a, !done)],
                ))
                .style(theme::tonal),
        ]
        .spacing(14)
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
        .padding([8, 16])
        .on_press_maybe(
            (!locked.is_empty()).then(|| Message::AskAchievementChange(game_id, locked)),
        )
        .style(theme::tonal)
        .into()
}
