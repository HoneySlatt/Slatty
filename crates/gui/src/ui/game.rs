//! Game page: key art, play button, stats, cloud and achievements summaries.

use iced::widget::{
    Space, button, column, container, image, progress_bar, row, scrollable, space, stack, text,
};
use iced::{Alignment, ContentFit, Element, Length};
use slatty_core::library::LibraryGame;

use super::format::*;
use super::panels::runner_label;
use super::{logo, round_button};
use crate::achievements::latest_unlocked;
use crate::icons::{Icon, icon};
use crate::install::InstallView;
use crate::theme::{self, ACCENT, BOLD, MUTED, ON_ACCENT, SEMIBOLD, TEXT};
use crate::{App, CloudStatus, Loadable, Message, Panel};

impl App {
    pub fn game_page<'a>(&'a self, g: &'a LibraryGame) -> Element<'a, Message> {
        let installed = self.installs.contains_key(&g.id);
        let mut tools = row![].spacing(10);
        if installed {
            tools = tools
                .push(round_button(
                    Icon::SlidersHorizontal,
                    Message::OpenPanel(Panel::GameSettings),
                ))
                .push(round_button(
                    Icon::EllipsisVertical,
                    Message::OpenPanel(Panel::Manage),
                ));
        }
        let header = row![
            logo(),
            Space::new().width(10),
            round_button(Icon::ChevronLeft, Message::CloseDetail),
            text("Library").size(16),
            space().width(Length::Fill),
            tools,
        ]
        .spacing(12)
        .align_y(Alignment::Center);

        let side = column![
            text(&g.title).size(52).font(BOLD).line_height(1.05),
            self.play_row(g),
        ]
        .push(self.session_line(g))
        .push(self.stats(g))
        .push(self.cloud_card(g))
        .push(self.achievements_card(g))
        .push(self.footer(g))
        .spacing(18)
        .width(470);
        let body = row![
            self.hero(g),
            scrollable(side).style(theme::scroller).height(Length::Fill)
        ]
        .spacing(36)
        .height(Length::Fill);
        column![header, body].spacing(18).into()
    }

    pub(super) fn hero<'a>(&'a self, g: &'a LibraryGame) -> Element<'a, Message> {
        let art = g
            .background
            .as_ref()
            .and_then(|u| self.images.get(u))
            .or_else(|| self.covers.get(&g.id));
        let base: Element<'_, Message> = match art {
            Some(h) => image(h.clone())
                .content_fit(ContentFit::Cover)
                .width(Length::Fill)
                .height(Length::Fill)
                .border_radius(22)
                .into(),
            None => container(Space::new())
                .width(Length::Fill)
                .height(Length::Fill)
                .style(theme::placeholder)
                .into(),
        };
        let mut layers = stack![base];
        if let Some(h) = g.logo.as_ref().and_then(|u| self.images.get(u)) {
            layers = layers.push(
                container(
                    image(h.clone())
                        .content_fit(ContentFit::Contain)
                        .width(320)
                        .height(140),
                )
                .padding(32)
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(Alignment::Start)
                .align_y(Alignment::End),
            );
        }
        container(layers)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    pub(super) fn play_row<'a>(&'a self, g: &'a LibraryGame) -> Element<'a, Message> {
        let running = self.play.as_ref().filter(|p| p.running);
        let this_running = running.is_some_and(|p| p.game_id == g.id);
        let big = |ic: Icon, label: String, msg: Option<Message>, danger: bool| {
            let color = if msg.is_some() { ON_ACCENT } else { MUTED };
            button(
                container(
                    row![icon(ic, 22.0, color), text(label).size(20).font(BOLD)]
                        .spacing(14)
                        .align_y(Alignment::Center),
                )
                .center_x(Length::Fill),
            )
            .padding([18, 0])
            .width(Length::Fill)
            .on_press_maybe(msg)
            .style(if danger {
                theme::danger
            } else {
                theme::primary
            })
        };
        let main = if self.installs.contains_key(&g.id) {
            if this_running {
                big(Icon::Stop, "Stop".into(), Some(Message::StopGame), true)
            } else {
                big(
                    Icon::Play,
                    "Play".into(),
                    self.can_play(&g.id).then(|| Message::Play(g.id.clone())),
                    false,
                )
            }
        } else {
            match self.install_views.get(&g.id) {
                Some(InstallView::Running { progress, .. }) => big(
                    Icon::Download,
                    format!("Downloading {:.0} %", fraction(*progress) * 100.0),
                    Some(Message::OpenPanel(Panel::Install)),
                    false,
                ),
                _ => big(
                    Icon::Download,
                    "Install".into(),
                    Some(Message::OpenPanel(Panel::Install)),
                    false,
                ),
            }
        };
        let favorite = self.favorites.contains(&g.id);
        let heart = button(
            container(icon(
                if favorite {
                    Icon::HeartFilled
                } else {
                    Icon::Heart
                },
                24.0,
                if favorite { ACCENT } else { TEXT },
            ))
            .center(30),
        )
        .padding(17)
        .on_press(Message::ToggleFavorite(g.id.clone()))
        .style(theme::tonal);
        row![main, heart]
            .spacing(12)
            .align_y(Alignment::Center)
            .into()
    }

    pub(super) fn session_line<'a>(&'a self, g: &'a LibraryGame) -> Element<'a, Message> {
        let Some(p) = self.play.as_ref().filter(|p| p.game_id == g.id) else {
            return Space::new().into();
        };
        row![
            text(p.log.last().map(String::as_str).unwrap_or_default())
                .size(14)
                .color(MUTED)
                .width(Length::Fill),
            button(text("Details").size(14))
                .on_press(Message::OpenPanel(Panel::Session))
                .style(theme::link),
        ]
        .align_y(Alignment::Center)
        .into()
    }

    pub(super) fn stats<'a>(&'a self, g: &'a LibraryGame) -> Element<'a, Message> {
        let played = self.playtime.get(&g.id);
        let stat = |ic: Icon, value: String, label: &'a str| {
            row![
                container(icon(ic, 20.0, TEXT))
                    .center(44)
                    .style(theme::circle),
                column![
                    text(value).size(18).font(SEMIBOLD),
                    text(label).size(13).color(MUTED)
                ]
                .spacing(2),
            ]
            .spacing(14)
            .align_y(Alignment::Center)
            .width(Length::Fill)
        };
        row![
            stat(
                Icon::Clock,
                match self.played_seconds(&g.id) {
                    s if s >= 60 => duration(s),
                    _ => "—".into(),
                },
                "Time played",
            ),
            container(Space::new())
                .width(1)
                .height(40)
                .style(theme::divider),
            stat(
                Icon::Calendar,
                played.map_or("—".into(), |p| relative_day(p.last_played)),
                "Last played",
            ),
        ]
        .spacing(18)
        .align_y(Alignment::Center)
        .into()
    }

    pub(super) fn cloud_card<'a>(&'a self, g: &'a LibraryGame) -> Element<'a, Message> {
        let installed = self.installs.contains_key(&g.id);
        let (ic, color, status) = if installed {
            match self.cloud.get(&g.id) {
                Some(c) if c.busy => (Icon::RefreshCw, MUTED, "Checking…".to_string()),
                Some(c) => match &c.status {
                    Some(CloudStatus::UpToDate) => {
                        (Icon::CircleCheck, theme::SUCCESS, "Up to date".into())
                    }
                    Some(CloudStatus::Pending(n)) => (
                        Icon::RefreshCw,
                        theme::WARNING,
                        format!("{n} file(s) to sync"),
                    ),
                    Some(CloudStatus::Conflict) => (
                        Icon::TriangleAlert,
                        theme::DANGER,
                        "Conflict: choose a version".into(),
                    ),
                    Some(CloudStatus::NoCloud) => {
                        (Icon::X, MUTED, "Not supported by this game".into())
                    }
                    Some(CloudStatus::Problem) | None => (
                        Icon::TriangleAlert,
                        theme::WARNING,
                        "Needs attention".into(),
                    ),
                },
                None => (Icon::RefreshCw, MUTED, "Not checked yet".into()),
            }
        } else {
            match self.overview.get(&g.id) {
                Some(o) if o.cloud_saves => (Icon::Cloud, MUTED, "Synced once installed".into()),
                Some(_) => (Icon::X, MUTED, "Not supported by this game".into()),
                None => (Icon::Cloud, MUTED, "Synced once installed".into()),
            }
        };
        let mut content = row![
            container(icon(Icon::Cloud, 22.0, TEXT))
                .center(48)
                .style(theme::circle),
            column![
                text("Cloud saves").size(15).font(SEMIBOLD),
                row![icon(ic, 16.0, color), text(status).size(14).color(MUTED)]
                    .spacing(8)
                    .align_y(Alignment::Center),
            ]
            .spacing(4)
            .width(Length::Fill),
        ]
        .spacing(16)
        .align_y(Alignment::Center);
        if installed {
            content = content.push(
                button(
                    row![
                        text("Manage").size(15),
                        icon(Icon::ArrowRight, 16.0, ACCENT)
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center),
                )
                .on_press(Message::OpenPanel(Panel::Cloud))
                .style(theme::link),
            );
        }
        container(content)
            .padding(18)
            .width(Length::Fill)
            .style(theme::card)
            .into()
    }

    pub(super) fn achievements_card<'a>(&'a self, g: &'a LibraryGame) -> Element<'a, Message> {
        let trophy = container(icon(Icon::Trophy, 22.0, TEXT))
            .center(48)
            .style(theme::circle);
        let body: Element<'_, Message> = match self.achievements.get(&g.id) {
            None | Some(Loadable::Loading) => {
                text("Loading achievements…").size(14).color(MUTED).into()
            }
            Some(Loadable::Failed(e)) => column![
                text(format!("Unavailable: {e}")).size(13).color(MUTED),
                button(text("Retry").size(14))
                    .on_press(Message::LoadAchievements(g.id.clone()))
                    .style(theme::link),
            ]
            .spacing(6)
            .into(),
            Some(Loadable::Ready(list)) if list.is_empty() => {
                text("This game has no achievements.")
                    .size(14)
                    .color(MUTED)
                    .into()
            }
            Some(Loadable::Ready(list)) => {
                let done = list.iter().filter(|a| a.date_unlocked.is_some()).count();
                let share = done as f32 / list.len() as f32;
                let latest: Vec<Element<'_, Message>> = latest_unlocked(list)
                    .into_iter()
                    .map(|a| {
                        column![
                            self.achievement_icon(&a.image_url_unlocked, 52.0),
                            text(&a.name)
                                .size(13)
                                .color(MUTED)
                                .align_x(Alignment::Center)
                        ]
                        .spacing(8)
                        .align_x(Alignment::Center)
                        .width(Length::Fill)
                        .into()
                    })
                    .collect();
                column![
                    row![
                        text("Achievements").size(15).font(SEMIBOLD),
                        text(format!("{done} / {}", list.len()))
                            .size(15)
                            .font(SEMIBOLD),
                        space().width(Length::Fill),
                        text(format!("{:.0}%", share * 100.0)).size(14).color(MUTED),
                    ]
                    .spacing(24)
                    .align_y(Alignment::Center),
                    progress_bar(0.0..=1.0, share)
                        .girth(8)
                        .style(theme::progress),
                    row(latest).spacing(8),
                ]
                .spacing(14)
                .into()
            }
        };
        let ready =
            matches!(self.achievements.get(&g.id), Some(Loadable::Ready(l)) if !l.is_empty());
        button(
            row![trophy, container(body).width(Length::Fill)]
                .spacing(16)
                .align_y(Alignment::Start),
        )
        .padding(18)
        .width(Length::Fill)
        .on_press_maybe(ready.then_some(Message::OpenPanel(Panel::Achievements)))
        .style(theme::row_button)
        .into()
    }

    pub(super) fn footer<'a>(&'a self, g: &'a LibraryGame) -> Element<'a, Message> {
        let Some(install) = self.installs.get(&g.id) else {
            return Space::new().into();
        };
        let item = |ic: Icon, label: String| {
            row![icon(ic, 18.0, MUTED), text(label).size(14).color(MUTED)]
                .spacing(10)
                .align_y(Alignment::Center)
        };
        let content = match self.records.get(&g.id) {
            Some(r) => row![
                item(Icon::HardDrive, human_size(r.size)),
                container(Space::new())
                    .width(1)
                    .height(20)
                    .style(theme::divider),
                item(Icon::Settings, format!("Version {}", r.version)),
            ],
            None => row![item(Icon::Settings, runner_label(install))],
        };
        content.spacing(18).align_y(Alignment::Center).into()
    }
}
