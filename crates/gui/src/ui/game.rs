//! Game page: key art, play b        let base: Element<'_, Message> = match self.hero_art(g) {tton, stats, cloud and achievements summaries.

use iced::widget::text::Wrapping;
use iced::widget::{
    Space, button, column, container, image, progress_bar, responsive, row, space, stack, text,
    tooltip,
};
use iced::{Alignment, ContentFit, Element, Length, Padding};
use slatty_core::installer::Progress;
use slatty_core::library::LibraryGame;

use super::format::*;
use super::panels::runner_label;
use super::{inner, note};
use crate::achievements::latest_unlocked;
use crate::icons::{Icon, icon};
use crate::install::{Cancelling, InstallMsg, InstallView};
use crate::theme::{self, BOLD, SEMIBOLD, tokens};
use crate::{App, CloudStatus, Loadable, Message, Panel};

/// Height of the summary cards under the key art.
const CARD_HEIGHT: f32 = 96.0;

impl App {
    /// Key art with the title and Play across the window, the summary cards below it.
    pub fn game_page<'a>(&'a self, g: &'a LibraryGame) -> Element<'a, Message> {
        let mut tools = row![].spacing(6);
        if self.installs.contains_key(&g.id) {
            let tool = |ic, panel| {
                button(icon(ic, 20.0, tokens().text))
                    .padding(8)
                    .on_press(Message::OpenPanel(panel))
                    .style(theme::ghost)
            };
            tools = tools
                .push(tool(Icon::SlidersHorizontal, Panel::GameSettings))
                .push(tool(Icon::EllipsisVertical, Panel::Manage));
        }
        let back = button(
            row![
                icon(Icon::ChevronLeft, 20.0, tokens().text),
                text("Library").size(16)
            ]
            .spacing(12)
            .align_y(Alignment::Center),
        )
        .padding([8, 10])
        .on_press(Message::CloseDetail)
        .style(theme::ghost);
        let header = column![
            container(row![back, space().width(Length::Fill), tools].align_y(Alignment::Center))
                .padding([6, 14])
                .width(Length::Fill),
            container(Space::new())
                .width(Length::Fill)
                .height(1)
                .style(theme::divider),
        ];
        let cards = row![
            container(self.stats(g))
                .padding([0, 18])
                .align_y(Alignment::Center)
                .width(Length::FillPortion(10))
                .height(CARD_HEIGHT)
                .style(theme::block),
            container(self.cloud_card(g))
                .width(Length::FillPortion(7))
                .height(CARD_HEIGHT),
            container(self.achievements_card(g))
                .width(Length::FillPortion(9))
                .height(CARD_HEIGHT),
        ]
        .spacing(14);
        let below = column![cards, self.footer(g)]
            .spacing(12)
            .padding(Padding::new(40.0).top(18.0).bottom(22.0));
        column![header, self.hero(g), below].into()
    }

    /// The key art, or the cover for a game without any. Nothing while the key art downloads: the
    /// cover would show for a moment, stretched, before it.
    pub(crate) fn hero_art(&self, g: &LibraryGame) -> Option<image::Handle> {
        let downloading = g
            .background
            .as_ref()
            .is_some_and(|u| self.images_requested.contains(u));
        match self.background(g) {
            Some(h) => Some(h),
            None if downloading => None,
            None => self.cover(&g.id),
        }
    }

    /// The game's key art, edge to edge, with its title, Play and favorite at the bottom left.
    pub(super) fn hero<'a>(&'a self, g: &'a LibraryGame) -> Element<'a, Message> {
        let base: Element<'_, Message> = match self.hero_art(g) {
            // Enlarged a little: much of GOG's key art ends with a light strip at the bottom.
            Some(h) => image(h)
                .content_fit(ContentFit::Cover)
                .scale(1.06_f32)
                .opacity(self.art_shown.interpolate(0.0, 1.0, self.now))
                .width(Length::Fill)
                .height(Length::Fill)
                .into(),
            None => container(Space::new())
                .width(Length::Fill)
                .height(Length::Fill)
                .style(theme::placeholder)
                .into(),
        };
        let fade = container(Space::new())
            .width(Length::Fill)
            .height(Length::Fill)
            .style(theme::hero_fade);
        let front = container(
            column![
                text(&g.title).size(52).font(BOLD).line_height(1.05),
                self.play_row(g),
                self.session_line(g),
                self.download_line(g),
            ]
            .spacing(16)
            .max_width(720),
        )
        .padding(Padding::new(40.0).bottom(22.0))
        .width(Length::Fill)
        .height(Length::Fill)
        .align_y(Alignment::End);
        stack![base, fade, front]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    pub(super) fn play_row<'a>(&'a self, g: &'a LibraryGame) -> Element<'a, Message> {
        let running = self.play.as_ref().filter(|p| p.running);
        let this_running = running.is_some_and(|p| p.game_id == g.id);
        let big = |ic: Icon, label: String, msg: Option<Message>, danger: bool| {
            let color = if msg.is_some() {
                tokens().on_accent
            } else {
                tokens().muted
            };
            button(
                container(
                    row![icon(ic, 22.0, color), text(label).size(20).font(BOLD)]
                        .spacing(14)
                        .align_y(Alignment::Center),
                )
                .center_x(Length::Fill),
            )
            .padding([16, 0])
            .width(300)
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
                // The download shows under these buttons; the page stays free to leave.
                Some(InstallView::Running { .. }) => big(
                    Icon::Pause,
                    "Pause".into(),
                    Some(Message::Install(InstallMsg::Pause(g.id.clone()))),
                    false,
                ),
                Some(InstallView::Ready(info)) if info.resumable => big(
                    Icon::Download,
                    "Resume".into(),
                    Some(if info.proton.is_some() {
                        Message::Install(InstallMsg::Start(g.id.clone()))
                    } else {
                        Message::OpenPanel(Panel::Install)
                    }),
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
                if favorite {
                    tokens().accent
                } else {
                    tokens().text
                },
            ))
            .center(26),
        )
        .padding(16)
        .on_press(Message::ToggleFavorite(g.id.clone()))
        .style(theme::tonal);
        row![main, heart]
            .spacing(12)
            .align_y(Alignment::Center)
            .into()
    }

    /// The running download of this game, under Pause.
    pub(super) fn download_line<'a>(&'a self, g: &'a LibraryGame) -> Element<'a, Message> {
        match self.install_views.get(&g.id) {
            Some(InstallView::Running {
                progress,
                cancelling,
                rate,
                ..
            }) => download_controls(g, *progress, *cancelling, rate.per_second(), false),
            _ => Space::new().into(),
        }
    }

    pub(super) fn session_line<'a>(&'a self, g: &'a LibraryGame) -> Element<'a, Message> {
        let Some(p) = self.play.as_ref().filter(|p| p.game_id == g.id) else {
            return Space::new().into();
        };
        row![
            text(p.log.last().map(String::as_str).unwrap_or_default())
                .size(14)
                .color(tokens().muted)
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
                container(icon(ic, 20.0, tokens().text))
                    .center(40)
                    .style(theme::circle),
                column![
                    text(value).size(18).font(SEMIBOLD).wrapping(Wrapping::None),
                    text(label)
                        .size(13)
                        .color(tokens().muted)
                        .wrapping(Wrapping::None)
                ]
                .spacing(2),
            ]
            .spacing(12)
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
        let launched = self
            .installs
            .get(&g.id)
            .is_some_and(slatty_core::runner::prefix_ready);
        let (ic, color, status) = if installed {
            match self.cloud.get(&g.id) {
                Some(c) if c.busy => (Icon::RefreshCw, tokens().muted, "Checking…".to_string()),
                Some(c) => match &c.status {
                    Some(CloudStatus::UpToDate) => (
                        Icon::CircleCheck,
                        theme::tokens().success,
                        "Up to date".into(),
                    ),
                    Some(CloudStatus::Pending(n)) if !launched => (
                        Icon::Cloud,
                        tokens().muted,
                        format!("{n} file(s) downloaded at first launch"),
                    ),
                    Some(CloudStatus::Pending(n)) => (
                        Icon::RefreshCw,
                        theme::tokens().warning,
                        format!("{n} file(s) to sync"),
                    ),
                    Some(CloudStatus::Conflict) => (
                        Icon::TriangleAlert,
                        theme::tokens().danger,
                        "Conflict: choose a version".into(),
                    ),
                    Some(CloudStatus::NoCloud) => {
                        (Icon::X, tokens().muted, "Not supported by this game".into())
                    }
                    Some(CloudStatus::Problem) | None => (
                        Icon::TriangleAlert,
                        theme::tokens().warning,
                        "Needs attention".into(),
                    ),
                },
                None => (Icon::RefreshCw, tokens().muted, "Not checked yet".into()),
            }
        } else {
            match self.overview.get(&g.id) {
                Some(o) if o.cloud_saves => {
                    (Icon::Cloud, tokens().muted, "Synced once installed".into())
                }
                Some(_) => (Icon::X, tokens().muted, "Not supported by this game".into()),
                None => (Icon::Cloud, tokens().muted, "Synced once installed".into()),
            }
        };
        // The whole card opens the cloud panel; "Manage" shows when the card has room for it
        // (a drawer narrows the page).
        let card = responsive(move |size| {
            let mut content = row![
                container(icon(Icon::Cloud, 22.0, tokens().text))
                    .center(40)
                    .style(theme::circle),
                column![
                    text("Cloud saves")
                        .size(15)
                        .font(SEMIBOLD)
                        .wrapping(Wrapping::None),
                    // The state icon matters once there are saves to keep in step.
                    row![]
                        .push(installed.then(|| icon(ic, 16.0, color)))
                        .push(
                            text(status.clone())
                                .size(14)
                                .color(tokens().muted)
                                .wrapping(Wrapping::None),
                        )
                        .spacing(8)
                        .align_y(Alignment::Center),
                ]
                .spacing(4)
                .width(Length::Fill),
            ]
            .spacing(14)
            .align_y(Alignment::Center);
            if installed && size.width >= 280.0 {
                content = content.push(
                    button(
                        row![
                            text("Manage").size(15),
                            icon(Icon::ArrowRight, 16.0, tokens().accent)
                        ]
                        .spacing(6)
                        .align_y(Alignment::Center),
                    )
                    .on_press(Message::OpenPanel(Panel::Cloud))
                    .style(theme::link),
                );
            }
            container(content).center_y(Length::Fill).into()
        });
        button(card)
            .padding([0, 18])
            .width(Length::Fill)
            .height(Length::Fill)
            .on_press_maybe(installed.then_some(Message::OpenPanel(Panel::Cloud)))
            .style(theme::tile)
            .into()
    }

    pub(super) fn achievements_card<'a>(&'a self, g: &'a LibraryGame) -> Element<'a, Message> {
        let trophy = container(icon(Icon::Trophy, 22.0, tokens().text))
            .center(40)
            .style(theme::circle);
        let body: Element<'_, Message> = match self.achievements.get(&g.id) {
            None | Some(Loadable::Loading) => text("Loading achievements…")
                .size(14)
                .color(tokens().muted)
                .into(),
            Some(Loadable::Failed(e)) => column![
                text(format!("Unavailable: {e}"))
                    .size(13)
                    .color(tokens().muted),
                button(text("Retry").size(14))
                    .on_press(Message::LoadAchievements(g.id.clone()))
                    .style(theme::link),
            ]
            .spacing(6)
            .into(),
            Some(Loadable::Ready(list)) if list.is_empty() => {
                text("This game has no achievements.")
                    .size(14)
                    .color(tokens().muted)
                    .into()
            }
            Some(Loadable::Ready(list)) => {
                let done = list.iter().filter(|a| a.date_unlocked.is_some()).count();
                let share = done as f32 / list.len() as f32;
                // The latest unlocks, named on hover, only when the card is wide enough for them
                // (the achievements drawer narrows the page).
                responsive(move |size| {
                    let summary = column![
                        text("Achievements").size(15).font(SEMIBOLD),
                        row![
                            text(format!("{done} / {}", list.len())).size(14),
                            space().width(Length::Fill),
                            text(format!("{:.0}%", share * 100.0))
                                .size(14)
                                .color(tokens().muted),
                        ],
                        progress_bar(0.0..=1.0, share)
                            .girth(8)
                            .style(theme::progress),
                    ]
                    .spacing(6)
                    .width(Length::Fill);
                    if size.width < 320.0 {
                        return container(summary).center_y(Length::Fill).into();
                    }
                    let latest = latest_unlocked(list).into_iter().map(|a| {
                        tooltip(
                            container(self.achievement_icon(&a.image_url_unlocked, 44.0))
                                .id(format!("latest-unlock-{}", a.achievement_id)),
                            container(text(&a.name).size(13))
                                .padding([4, 10])
                                .style(theme::block),
                            tooltip::Position::Top,
                        )
                        .into()
                    });
                    row![summary, row(latest).spacing(8)]
                        .spacing(20)
                        .height(Length::Fill)
                        .align_y(Alignment::Center)
                        .into()
                })
                .into()
            }
        };
        let ready =
            matches!(self.achievements.get(&g.id), Some(Loadable::Ready(l)) if !l.is_empty());
        button(
            row![trophy, container(body).width(Length::Fill)]
                .spacing(16)
                .height(Length::Fill)
                .align_y(Alignment::Center),
        )
        .padding([0, 18])
        .width(Length::Fill)
        .height(Length::Fill)
        .on_press_maybe(ready.then_some(Message::OpenPanel(Panel::Achievements)))
        .style(theme::tile)
        .into()
    }

    pub(super) fn footer<'a>(&'a self, g: &'a LibraryGame) -> Element<'a, Message> {
        let Some(install) = self.installs.get(&g.id) else {
            return Space::new().into();
        };
        let item = |ic: Icon, label: String| {
            row![
                icon(ic, 18.0, tokens().muted),
                text(label).size(14).color(tokens().muted)
            ]
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

/// Progress of a running download, with Cancel (asked first) and, when asked for, Pause.
pub(super) fn download_controls<'a>(
    g: &'a LibraryGame,
    progress: Progress,
    cancelling: Cancelling,
    speed: Option<f64>,
    pause: bool,
) -> Element<'a, Message> {
    let msg = |m: fn(String) -> InstallMsg| Message::Install(m(g.id.clone()));
    let mut status = format!(
        "{} / {}",
        human_size(progress.bytes_done),
        human_size(progress.bytes_total)
    );
    if let Some(speed) = speed {
        status += &format!(" · {}/s", human_size(speed as u64));
    }
    let status = note(status);
    let buttons = (cancelling == Cancelling::No).then(|| {
        row![]
            .push(pause.then(|| {
                button(text("Pause").size(14))
                    .padding([10, 18])
                    .on_press(msg(InstallMsg::Pause))
                    .style(theme::tonal)
            }))
            .push(
                button(text("Cancel").size(14))
                    .padding([10, 18])
                    .on_press(msg(InstallMsg::AskCancel))
                    .style(theme::tonal),
            )
            .spacing(10)
    });
    let below: Option<Element<'a, Message>> = match cancelling {
        Cancelling::No => None,
        Cancelling::Asked => Some(
            container(
                column![
                    text(format!(
                        "Cancel the download of {} and delete the {} downloaded so far?",
                        g.title,
                        human_size(progress.bytes_done)
                    ))
                    .size(14),
                    row![
                        button(text("Keep downloading").size(14))
                            .padding([10, 18])
                            .on_press(msg(InstallMsg::KeepDownloading))
                            .style(theme::tonal),
                        button(text("Cancel download").size(14))
                            .padding([10, 18])
                            .on_press(msg(InstallMsg::ConfirmCancel))
                            .style(theme::danger),
                    ]
                    .spacing(10),
                ]
                .spacing(12),
            )
            .padding(16)
            .width(Length::Fill)
            .style(inner)
            .into(),
        ),
        Cancelling::Confirmed => Some(note("Cancelling… the downloaded files are deleted.")),
    };
    column![
        row![
            progress_bar(0.0..=1.0, fraction(progress))
                .girth(8)
                .style(theme::progress),
            text(format!("{:.0} %", fraction(progress) * 100.0))
                .size(14)
                .font(SEMIBOLD)
                // A fixed width, so the bar keeps its length as the digits change.
                .width(48)
                .align_x(Alignment::End),
        ]
        .spacing(14)
        .align_y(Alignment::Center),
        row![container(status).width(Length::Fill)]
            .push(buttons)
            .align_y(Alignment::Center),
    ]
    .push(below)
    .spacing(10)
    .into()
}
