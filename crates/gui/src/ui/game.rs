//! Game page: key art, play button, stats, cloud and achievements summaries.

use crate::theme::text;
use iced::widget::text::Wrapping;
use iced::widget::{
    Space, button, column, container, image, mouse_area, opaque, progress_bar, responsive, row,
    space, stack, tooltip,
};
use iced::{Alignment, ContentFit, Element, Length, Padding};
use slatty_core::install::Platform;
use slatty_core::installer::Progress;
use slatty_core::library::{LibraryGame, NotInstallable};
use slatty_core::store::Store;

use super::achievements::{achievement_bar, achievement_share, completed};
use super::format::*;
use super::panels::runner_label;
use super::{inner, note};
use crate::achievements::latest_unlocked;
use crate::edit::EditMsg;
use crate::icons::{Icon, icon};
use crate::install::{Cancelling, InstallMsg, InstallView};
use crate::theme::{self, bold, semibold, tokens};
use crate::{App, CloudStatus, Loadable, Message, Page, Panel};

/// Height of the summary cards under the key art.
const CARD_HEIGHT: f32 = 96.0;

/// A game's main button: what it says, and what it does when it can be pressed.
pub struct MainAction {
    pub icon: Icon,
    pub label: String,
    pub message: Option<Message>,
    /// Stop, which ends the game.
    pub danger: bool,
}
/// Width of the row of cards under which play time and the last session lose their icons.
const NARROW_CARDS: f32 = 1100.0;
/// Window height under which a game's title is set smaller, for the key art is short.
const SHORT_WINDOW: f32 = 800.0;

impl App {
    /// Key art with the title and Play across the window, the summary cards below it.
    pub fn game_page<'a>(&'a self, g: &'a LibraryGame) -> Element<'a, Message> {
        let mut tools = row![].spacing(6);
        let tool = |ic, panel| {
            button(icon(ic, 20.0, tokens().text))
                .padding(8)
                .on_press(Message::OpenPanel(panel))
                .style(theme::ghost)
        };
        if self.installs.contains_key(&g.id) {
            tools = tools
                .push(tool(Icon::SlidersHorizontal, Panel::GameSettings))
                .push(tool(Icon::EllipsisVertical, Panel::Manage));
        } else if slatty_core::store::of(&g.id).is_ok_and(|s| s == Store::Gog) {
            // Not installed: its offline installers can still be kept.
            tools = tools.push(tool(Icon::EllipsisVertical, Panel::Manage));
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
        // On a narrow page (a small window, a drawer beside it) play time and the last session get
        // the room their values need, without their icons.
        let steam = self.steam_game(&g.id).is_some();
        let added = slatty_core::store::is_local(&g.id);
        let cards = container(responsive(move |size| {
            let narrow = size.width < NARROW_CARDS;
            let (stats, achievements) = if narrow { (11, 8) } else { (10, 9) };
            // Neither cloud saves nor achievements: what it is in their place.
            if added {
                return row![
                    container(self.stats(g, narrow))
                        .padding([0, 18])
                        .align_y(Alignment::Center)
                        .width(Length::FillPortion(stats))
                        .height(CARD_HEIGHT)
                        .style(theme::block),
                    container(self.added_card(g))
                        .width(Length::FillPortion(7 + achievements))
                        .height(CARD_HEIGHT),
                ]
                .spacing(14)
                .into();
            }
            if steam {
                return row![
                    container(self.stats(g, narrow))
                        .padding([0, 18])
                        .align_y(Alignment::Center)
                        .width(Length::FillPortion(stats))
                        .height(CARD_HEIGHT)
                        .style(theme::block),
                    container(self.steam_card())
                        .width(Length::FillPortion(7))
                        .height(CARD_HEIGHT),
                    container(self.achievements_card(g))
                        .width(Length::FillPortion(achievements))
                        .height(CARD_HEIGHT),
                ]
                .spacing(14)
                .into();
            }
            row![
                container(self.stats(g, narrow))
                    .padding([0, 18])
                    .align_y(Alignment::Center)
                    .width(Length::FillPortion(stats))
                    .height(CARD_HEIGHT)
                    .style(theme::block),
                container(self.cloud_card(g))
                    .width(Length::FillPortion(7))
                    .height(CARD_HEIGHT),
                container(self.achievements_card(g))
                    .width(Length::FillPortion(achievements))
                    .height(CARD_HEIGHT),
            ]
            .spacing(14)
            .into()
        }))
        .height(CARD_HEIGHT);
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
            Some(h) => Some(self.shown_art(h)),
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
        // A long title in a short window gives way to Play and the lines below it: smaller, and cut
        // from the top as a last resort, never pushing them out.
        let title = text(&g.title)
            .size(if self.window.height < SHORT_WINDOW {
                40
            } else {
                52
            })
            .font(bold())
            .line_height(1.05);
        // What is drawn over the art keeps its right clicks from the art's menu.
        let front = container(
            column![
                container(opaque(title))
                    .height(Length::Fill)
                    .align_y(Alignment::End)
                    .clip(true),
                opaque(self.play_row(g)),
                opaque(self.not_installable_line(g)),
                opaque(self.session_line(g)),
                opaque(self.download_line(g)),
            ]
            .spacing(16)
            .max_width(720)
            .height(Length::Fill),
        )
        .padding(Padding::new(40.0).bottom(22.0))
        .width(Length::Fill)
        .height(Length::Fill)
        .align_y(Alignment::End);
        // A right click on the art opens the same menu as on a cover in the library.
        mouse_area(
            stack![base, fade, front]
                .width(Length::Fill)
                .height(Length::Fill),
        )
        .on_right_press(Message::Edit(EditMsg::Menu(g.id.clone())))
        .into()
    }

    pub(super) fn play_row<'a>(&'a self, g: &'a LibraryGame) -> Element<'a, Message> {
        let main = self.main_action(g);
        let color = if main.message.is_some() {
            tokens().on_accent
        } else {
            tokens().muted
        };
        let main = button(
            container(
                row![
                    icon(main.icon, 22.0, color),
                    text(main.label).size(20).font(bold())
                ]
                .spacing(14)
                .align_y(Alignment::Center),
            )
            .center_x(Length::Fill),
        )
        .padding([16, 0])
        .width(300)
        .on_press_maybe(main.message)
        .style(if main.danger {
            theme::danger
        } else {
            theme::primary
        });
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

    /// The game's main button, as the game page and the couch mode show it: Play, Install, Stop,
    /// or where its download or update stands.
    pub(crate) fn main_action(&self, g: &LibraryGame) -> MainAction {
        let running = self.play.as_ref().filter(|p| p.running);
        let this_running = running.is_some_and(|p| p.game_id == g.id);
        let big = |icon: Icon, label: String, message: Option<Message>, danger: bool| MainAction {
            icon,
            label,
            message,
            danger,
        };
        if self.steam_game(&g.id).is_some() {
            use crate::steam::SteamMsg;
            use slatty_core::steam::client::Action;
            let ask = |action| Some(Message::Steam(SteamMsg::Ask(g.id.clone(), action)));
            match self.steam.installed.get(&g.id) {
                // Steam has no way to be asked to stop a game.
                _ if self.steam.running.contains(&g.id) => {
                    big(Icon::Play, "Running".into(), None, false)
                }
                Some(installed) if installed.update.is_some_and(|u| u.started) => big(
                    Icon::Download,
                    self.steam_status(&g.id, installed.update.as_ref().expect("just checked")),
                    None,
                    false,
                ),
                Some(installed) if installed.ready => {
                    big(Icon::Play, "Play".into(), ask(Action::Play), false)
                }
                Some(_) => big(Icon::Download, "Queued in Steam".into(), None, false),
                None => big(
                    Icon::Download,
                    "Install".into(),
                    ask(Action::Install),
                    false,
                ),
            }
        } else if self.installs.contains_key(&g.id) {
            if this_running {
                big(Icon::Stop, "Stop".into(), Some(Message::StopGame), true)
            } else if self.updates.current.as_deref() == Some(g.id.as_str()) {
                // Playable again once the update is done, or paused from Downloads.
                let progress = self.maintenance.get(&g.id).and_then(|m| m.progress);
                big(
                    Icon::Download,
                    format!(
                        "Updating {:.0}%",
                        progress.map(fraction).unwrap_or_default() * 100.0
                    ),
                    None,
                    false,
                )
            } else if self.launch_reading.as_deref() == Some(g.id.as_str()) {
                big(Icon::Play, "Starting…".into(), None, false)
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
                    Some(
                        if info.platform == Platform::Linux || info.proton.is_some() {
                            Message::Install(InstallMsg::Start(g.id.clone()))
                        } else {
                            Message::OpenPanel(Panel::Install)
                        },
                    ),
                    false,
                ),
                Some(InstallView::Queued(_)) => big(
                    Icon::Clock,
                    "Queued".into(),
                    Some(Message::ShowPage(Page::Downloads)),
                    false,
                ),
                _ if self.not_installable(&g.id).is_some() => {
                    big(Icon::Download, "Not installable".into(), None, false)
                }
                _ => big(
                    Icon::Download,
                    "Install".into(),
                    Some(Message::OpenPanel(Panel::Install)),
                    false,
                ),
            }
        }
    }

    /// Why Install is unavailable, when GOG offers nothing to install.
    fn not_installable_line<'a>(&'a self, g: &'a LibraryGame) -> Element<'a, Message> {
        match self.not_installable(&g.id) {
            Some(why) => text(match why {
                NotInstallable::Pack => {
                    "GOG offers nothing to install for this product: it is a pack, whose games \
                     are in your library on their own."
                }
                NotInstallable::Other => {
                    "GOG offers nothing to install for this product. Its downloads, if any, are \
                     on gog.com, in your account."
                }
            })
            .size(14)
            .color(tokens().warning)
            .into(),
            None => Space::new().into(),
        }
    }

    /// Why GOG offers nothing to install for a game not installed, once known.
    pub(super) fn not_installable(&self, game_id: &str) -> Option<NotInstallable> {
        if self.installs.contains_key(game_id) {
            return None;
        }
        self.installability.get(game_id).copied().flatten()
    }

    /// The running download of this game, under Pause.
    pub(super) fn download_line<'a>(&'a self, g: &'a LibraryGame) -> Element<'a, Message> {
        match self.install_views.get(&g.id) {
            Some(InstallView::Running {
                progress,
                cancelling,
                rate,
                ..
            }) => download_controls(g, *progress, *cancelling, rate.received_per_second(), false),
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

    pub(super) fn stats<'a>(&'a self, g: &'a LibraryGame, narrow: bool) -> Element<'a, Message> {
        let last_played = self.last_played(&g.id);
        let stat = |ic: Icon, value: String, label: &'a str| {
            row![]
                .push((!narrow).then(|| {
                    container(icon(ic, 20.0, tokens().text))
                        .center(40)
                        .style(theme::circle)
                }))
                .push(
                    column![
                        text(value)
                            .size(18)
                            .font(semibold())
                            .wrapping(Wrapping::None),
                        text(label)
                            .size(13)
                            .color(tokens().muted)
                            .wrapping(Wrapping::None)
                    ]
                    .spacing(2),
                )
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
                last_played.map_or("—".into(), relative_day),
                "Last played",
            ),
        ]
        .spacing(18)
        .align_y(Alignment::Center)
        .into()
    }

    pub(super) fn cloud_card<'a>(&'a self, g: &'a LibraryGame) -> Element<'a, Message> {
        // Nothing to manage for a Linux build: GOG keeps no cloud saves for them.
        let installed = self
            .installs
            .get(&g.id)
            .is_some_and(|i| i.platform == Platform::Windows);
        let (ic, color, status) = self.cloud_status(g);
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
                        .font(semibold())
                        .wrapping(Wrapping::None),
                    // The state icon matters once there are saves to keep in step.
                    row![]
                        .push(installed.then(|| icon(ic, 16.0, color)))
                        .push(
                            // Wraps rather than run over the next card when a drawer narrows it.
                            text(status.clone())
                                .size(14)
                                .color(tokens().muted)
                                .width(Length::Fill),
                        )
                        .spacing(8)
                        .width(Length::Fill)
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

    /// The cloud state of a game in a few words, with its icon and colour.
    pub(super) fn cloud_status(&self, g: &LibraryGame) -> (Icon, iced::Color, String) {
        let launched = self.cloud.get(&g.id).is_some_and(|c| c.launched);
        if self
            .installs
            .get(&g.id)
            .is_some_and(|i| i.platform == Platform::Linux)
        {
            return (
                Icon::X,
                tokens().muted,
                "Not available for Linux builds".into(),
            );
        }
        if self.installs.contains_key(&g.id) {
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
                Some(o) if o.checked && !o.cloud_saves => {
                    (Icon::X, tokens().muted, "Not supported by this game".into())
                }
                _ => (Icon::Cloud, tokens().muted, "Synced once installed".into()),
            }
        }
    }

    pub(super) fn achievements_card<'a>(&'a self, g: &'a LibraryGame) -> Element<'a, Message> {
        let lit = matches!(
            self.achievements.get(&g.id),
            Some(Loadable::Ready(l)) if completed(
                l.iter().filter(|a| a.date_unlocked.is_some()).count(),
                l.len(),
            )
        );
        let trophy = if lit {
            container(icon(Icon::Trophy, 22.0, tokens().accent_hover))
                .center(40)
                .style(theme::lit_circle)
        } else {
            container(icon(Icon::Trophy, 22.0, tokens().text))
                .center(40)
                .style(theme::circle)
        };
        let body: Element<'_, Message> = match self.achievements.get(&g.id) {
            None | Some(Loadable::Loading) => text("Loading achievements…")
                .size(14)
                .color(tokens().muted)
                .into(),
            // The reason can be long (a keyring error): it shows on hover, the card keeps its size.
            Some(Loadable::Failed(e)) => column![
                tooltip(
                    text("Achievements unavailable")
                        .size(14)
                        .color(tokens().muted),
                    container(text(e).size(13))
                        .padding([4, 10])
                        .max_width(360)
                        .style(theme::block),
                    tooltip::Position::Top,
                ),
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
                // The latest unlocks, named on hover, only when the card is wide enough for them
                // (the achievements drawer narrows the page).
                responsive(move |size| {
                    let summary = column![
                        text("Achievements").size(15).font(semibold()),
                        row![
                            text(format!("{done} / {}", list.len())).size(14),
                            space().width(Length::Fill),
                            achievement_share(done, list.len(), 14.0),
                        ]
                        .align_y(Alignment::Center),
                        achievement_bar(done, list.len(), 8.0),
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

    /// What a game added by hand is: the user's files, isolated or not, and nothing of GOG's.
    fn added_card<'a>(&'a self, g: &'a LibraryGame) -> Element<'a, Message> {
        let isolated = self.installs.get(&g.id).is_some_and(|i| i.isolated);
        let card = responsive(move |size| {
            let content = row![]
                .push((size.width >= 240.0).then(|| {
                    container(icon(Icon::Folder, 22.0, tokens().text))
                        .center(40)
                        .style(theme::circle)
                }))
                .push(
                    column![
                        text("Added by hand")
                            .size(15)
                            .font(semibold())
                            .wrapping(Wrapping::None),
                        text(if isolated {
                            "Isolated from your files"
                        } else {
                            "Not isolated from your files"
                        })
                        .size(14)
                        .color(tokens().muted)
                        .width(Length::Fill),
                    ]
                    .spacing(4)
                    .width(Length::Fill),
                )
                .spacing(14)
                .align_y(Alignment::Center);
            container(content).center_y(Length::Fill).into()
        });
        let why = container(
            text(
                "Its files are yours: removing it from the library leaves them, except those \
                 Slatty unpacked. It has no cloud saves or achievements, and GOG is not \
                 told of it: its play time stays on this computer.",
            )
            .size(13),
        )
        .padding([4, 10])
        .max_width(360)
        .style(theme::block);
        tooltip(
            container(card)
                .padding([0, 18])
                .width(Length::Fill)
                .height(Length::Fill)
                .style(theme::block),
            why,
            tooltip::Position::Top,
        )
        .into()
    }

    /// What a Steam game's page says of Steam, in place of GOG's cloud and achievements cards:
    /// what runs it, in a few words as the cloud card says its state; the reason the GOG build is
    /// the safer choice on hover. Its icon goes when a drawer narrows the card.
    fn steam_card(&self) -> Element<'_, Message> {
        let card = responsive(move |size| {
            let content = row![]
                .push((size.width >= 240.0).then(|| {
                    container(icon(Icon::Globe, 22.0, tokens().text))
                        .center(40)
                        .style(theme::circle)
                }))
                .push(
                    column![
                        text("Runs through Steam")
                            .size(15)
                            .font(semibold())
                            .wrapping(Wrapping::None),
                        text("Not isolated from your files")
                            .size(14)
                            .color(tokens().muted)
                            .width(Length::Fill),
                    ]
                    .spacing(4)
                    .width(Length::Fill),
                )
                .spacing(14)
                .align_y(Alignment::Center);
            container(content).center_y(Length::Fill).into()
        });
        let why = container(
            text(
                "Steam's client plays, updates and syncs it, and it cannot run isolated from your \
                 files. A GOG build runs isolated and needs no client running: the safer choice \
                 for your privacy and security.",
            )
            .size(13),
        )
        .padding([4, 10])
        .max_width(360)
        .style(theme::block);
        tooltip(
            container(card)
                .padding([0, 18])
                .width(Length::Fill)
                .height(Length::Fill)
                .style(theme::block),
            why,
            tooltip::Position::Top,
        )
        .into()
    }

    pub(super) fn footer<'a>(&'a self, g: &'a LibraryGame) -> Element<'a, Message> {
        let item = |ic: Icon, label: String| {
            row![
                icon(ic, 18.0, tokens().muted),
                text(label).size(14).color(tokens().muted)
            ]
            .spacing(10)
            .align_y(Alignment::Center)
        };
        if let Some(installed) = self.steam.installed.get(&g.id) {
            use crate::steam::SteamMsg;
            use slatty_core::steam::client::Action;
            let ask = |label, action| {
                button(text(label).size(14))
                    .padding([6, 12])
                    .on_press(Message::Steam(SteamMsg::Ask(g.id.clone(), action)))
                    .style(theme::link)
            };
            return row![
                item(Icon::HardDrive, human_size(installed.size)),
                space().width(Length::Fill),
                ask("Verify files in Steam", Action::Verify),
                ask("Uninstall in Steam", Action::Uninstall),
            ]
            .spacing(18)
            .align_y(Alignment::Center)
            .into();
        }
        let twin = self.steam_twin(&g.id).map(|twin| self.steam_twin_row(twin));
        let Some(install) = self.installs.get(&g.id) else {
            return match twin {
                Some(twin) => row![space().width(Length::Fill), twin].into(),
                None => Space::new().into(),
            };
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
        content
            .push(twin.map(|twin| row![space().width(Length::Fill), twin]))
            .spacing(18)
            .align_y(Alignment::Center)
            .into()
    }

    /// A GOG game also in the Steam library: its Steam build can be played or installed through
    /// Steam, the GOG build staying the one the page is about.
    fn steam_twin_row<'a>(&'a self, twin: &'a str) -> Element<'a, Message> {
        use crate::steam::SteamMsg;
        use slatty_core::steam::client::Action;
        let installed = self.steam.installed.get(twin);
        let (label, action) = match installed {
            _ if self.steam.running.contains(twin) => ("Running in Steam".to_string(), None),
            Some(i) if i.update.is_some_and(|u| u.started) => (
                self.steam_status(twin, i.update.as_ref().expect("just checked")),
                None,
            ),
            Some(i) if i.ready => ("Play the Steam version".to_string(), Some(Action::Play)),
            Some(_) => ("Queued in Steam".to_string(), None),
            None => (
                "Install the Steam version".to_string(),
                Some(Action::Install),
            ),
        };
        // Where both builds are offered, why the GOG one comes first.
        let why = container(
            text(
                "This GOG build runs isolated from your files and needs no client running: the \
                 safer choice. The Steam build runs through Steam's client.",
            )
            .size(13),
        )
        .padding([4, 10])
        .max_width(360)
        .style(theme::block);
        row![
            icon(Icon::Globe, 18.0, tokens().muted),
            tooltip(
                text("Also in your Steam library")
                    .size(14)
                    .color(tokens().muted),
                why,
                tooltip::Position::Top,
            ),
            button(text(label).size(14))
                .padding([6, 12])
                .on_press_maybe(action.map(|a| Message::Steam(SteamMsg::Ask(twin.to_string(), a))))
                .style(theme::link),
        ]
        .spacing(10)
        .align_y(Alignment::Center)
        .into()
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
    let mut status = amount(progress);
    if let Some(speed) = speed {
        status += &format!(" · {}", slatty_core::units::speed(speed));
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
    let below = cancel_prompt(&g.id, &g.title, progress, cancelling);
    column![
        row![
            progress_bar(0.0..=1.0, fraction(progress))
                .girth(8)
                .style(theme::progress),
            text(format!("{:.0}%", fraction(progress) * 100.0))
                .size(14)
                .font(semibold())
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

/// Asks before a running download is cancelled, since its files are deleted.
pub(super) fn cancel_prompt<'a>(
    game_id: &str,
    title: &str,
    progress: Progress,
    cancelling: Cancelling,
) -> Option<Element<'a, Message>> {
    let msg = |m: fn(String) -> InstallMsg| Message::Install(m(game_id.to_string()));
    match cancelling {
        Cancelling::No => None,
        Cancelling::Asked => Some(
            container(
                column![
                    text(format!(
                        "Cancel the download of {title} and delete the {} downloaded so far?",
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
    }
}
