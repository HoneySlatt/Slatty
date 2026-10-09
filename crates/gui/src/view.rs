use iced::widget::{
    Column, Space, button, checkbox, column, container, grid, hover, image, pick_list,
    progress_bar, row, scrollable, slider, space, text, text_input,
};
use iced::{Alignment, ContentFit, Element, Length, Padding};
use slatty_core::achievements::Achievement;
use slatty_core::cloud::plan::Warning;
use slatty_core::installer::Progress;
use slatty_core::library::LibraryGame;
use slatty_core::play::{CloudSummary, PlayEvent};

use crate::icons::{Icon, icon};
use crate::installs::{ProtonChoice, SettingsMsg};
use crate::theme::{self, ACCENT, BOLD, MUTED, ON_ACCENT, SEMIBOLD, TEXT};
use crate::{App, Filters, Message, Page, Panel, Shelf, Sort};

pub fn fraction(p: Progress) -> f32 {
    if p.bytes_total == 0 {
        0.0
    } else {
        p.bytes_done as f32 / p.bytes_total as f32
    }
}

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
                    Page::Achievements => self.achievements_page(),
                    Page::Settings => self.settings_page(),
                }
            ]
            .spacing(14)
            .into(),
        };
        let page = self.with_notice(container(body).padding(Padding::new(24.0).top(16.0)).into());
        match (self.panel, self.selected_game()) {
            (Some(panel), Some(game)) => self.with_panel(page, panel, game),
            _ => page,
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
        let avatar = button(
            container(text(initial).size(17).font(BOLD))
                .center(38)
                .style(theme::avatar),
        )
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

    fn library_page(&self) -> Element<'_, Message> {
        let games = self.visible_games();
        let shelf = |label, s: Shelf| {
            button(text(label).size(14).font(SEMIBOLD))
                .padding([8, 20])
                .on_press(Message::ShowShelf(s))
                .style(theme::segment(self.shelf == s))
        };
        let shelves = container(
            row![
                shelf("All", Shelf::All),
                shelf("Installed", Shelf::Installed),
                shelf("Favorites", Shelf::Favorites),
            ]
            .spacing(4),
        )
        .padding(4)
        .style(theme::pill);
        let filter_button = button(icon(
            Icon::ListFilter,
            18.0,
            if self.filters.any() { ON_ACCENT } else { TEXT },
        ))
        .padding(11)
        .on_press(Message::ToggleFilters)
        .style(if self.filters.any() {
            theme::segment(true)
        } else {
            theme::segment(false)
        });
        let toolbar = row![
            shelves,
            text(format!("{} games", games.len())).size(14).color(MUTED),
            space().width(Length::Fill),
            pick_list(Sort::ALL, Some(self.sort), Message::SortBy)
                .style(theme::select)
                .padding([9, 18])
                .text_size(14),
            Space::new().width(4),
            icon(Icon::LayoutGrid, 20.0, MUTED),
            slider(110.0..=240.0, self.card_width, Message::CardWidth)
                .width(150)
                .style(theme::size_slider),
            container(filter_button).style(theme::pill),
        ]
        .spacing(14)
        .align_y(Alignment::Center);

        let mut page = column![toolbar].spacing(14);
        if self.filters_open {
            page = page.push(self.filter_bar());
        }
        if let Some((id, title, p)) = self.installing() {
            page = page.push(download_banner(id, title, p));
        }

        let cards: Vec<Element<'_, Message>> = games.iter().map(|g| self.card(g)).collect();
        let gallery: Element<'_, Message> = if cards.is_empty() {
            container(
                text(if self.library.is_empty() {
                    "Your library is empty: refresh it in Settings."
                } else {
                    "No game matches."
                })
                .color(MUTED),
            )
            .padding(20)
            .into()
        } else {
            scrollable(
                grid(cards)
                    .fluid(self.card_width)
                    .spacing(14)
                    .height(grid::aspect_ratio(3, 4)),
            )
            .spacing(8)
            .style(theme::scroller)
            .height(Length::Fill)
            .into()
        };
        page.push(gallery).into()
    }

    fn filter_bar(&self) -> Element<'_, Message> {
        let f = self.filters;
        let check = |label, on: bool, set: fn(Filters, bool) -> Filters| {
            checkbox(on)
                .label(label)
                .text_size(14)
                .on_toggle(move |v| Message::SetFilters(set(f, v)))
        };
        let mut items = row![
            check("Windows", f.windows, |f, v| Filters { windows: v, ..f }),
            check("Linux", f.linux, |f, v| Filters { linux: v, ..f }),
            check("Has achievements", f.achievements, |f, v| Filters {
                achievements: v,
                ..f
            }),
            check("Has cloud saves", f.cloud_saves, |f, v| Filters {
                cloud_saves: v,
                ..f
            }),
        ]
        .spacing(24)
        .align_y(Alignment::Center);
        if (f.achievements || f.cloud_saves) && !self.overview_complete() {
            items = items.push(
                text(format!(
                    "Reading GOG data… {}/{} games",
                    self.overview.len().min(self.library.len()),
                    self.library.len()
                ))
                .size(13)
                .color(MUTED),
            );
        }
        container(items)
            .padding([12, 18])
            .width(Length::Fill)
            .style(theme::card)
            .into()
    }

    /// Games of the current shelf matching search and filters, in the chosen order.
    pub fn visible_games(&self) -> Vec<&LibraryGame> {
        let needle = self.search.to_lowercase();
        let f = self.filters;
        let mut games: Vec<&LibraryGame> = self
            .library
            .iter()
            .filter(|g| needle.is_empty() || g.title.to_lowercase().contains(&needle))
            .filter(|g| match self.shelf {
                Shelf::All => true,
                Shelf::Installed => self.installs.contains_key(&g.id),
                Shelf::Favorites => self.favorites.contains(&g.id),
            })
            .filter(|g| !f.windows || g.os.iter().any(|o| o == "windows"))
            .filter(|g| !f.linux || g.os.iter().any(|o| o == "linux"))
            .filter(|g| {
                let o = self.overview.get(&g.id);
                (!f.achievements || o.is_some_and(|o| o.achievements.is_some()))
                    && (!f.cloud_saves || o.is_some_and(|o| o.cloud_saves))
            })
            .collect();
        let played = |g: &LibraryGame| self.playtime.get(&g.id).copied().unwrap_or_default();
        match self.sort {
            Sort::NameAsc => games.sort_by_key(|g| g.title.to_lowercase()),
            Sort::NameDesc => {
                games.sort_by_key(|g| std::cmp::Reverse(g.title.to_lowercase()));
            }
            Sort::RecentlyPlayed => {
                games.sort_by_key(|g| {
                    (
                        std::cmp::Reverse(played(g).last_played),
                        g.title.to_lowercase(),
                    )
                });
            }
            Sort::MostPlayed => {
                games.sort_by_key(|g| {
                    (std::cmp::Reverse(played(g).seconds), g.title.to_lowercase())
                });
            }
        }
        games
    }

    fn card<'a>(&'a self, g: &'a LibraryGame) -> Element<'a, Message> {
        let art: Element<'_, Message> = match self.covers.get(&g.id) {
            Some(h) => image(h.clone())
                .content_fit(ContentFit::Cover)
                .width(Length::Fill)
                .height(Length::Fill)
                .border_radius(12)
                .into(),
            None => container(text(&g.title).size(14).font(SEMIBOLD))
                .padding(12)
                .width(Length::Fill)
                .height(Length::Fill)
                .style(theme::placeholder)
                .into(),
        };
        let base = button(art)
            .padding(0)
            .on_press(Message::Select(g.id.clone()))
            .style(theme::plain);
        let installed = self.installs.contains_key(&g.id);
        let running = self.play.as_ref().is_some_and(|p| p.running);
        let action = if installed {
            button(icon(Icon::Play, 16.0, ON_ACCENT))
                .padding([8, 14])
                .on_press_maybe((!running).then(|| Message::Play(g.id.clone())))
                .style(theme::primary)
        } else {
            button(icon(Icon::Download, 16.0, ON_ACCENT))
                .padding([8, 14])
                .on_press(Message::SelectWith(g.id.clone(), Panel::Install))
                .style(theme::primary)
        };
        let settings = button(icon(Icon::SlidersHorizontal, 16.0, TEXT))
            .padding(8)
            .on_press(Message::SelectWith(
                g.id.clone(),
                if installed {
                    Panel::GameSettings
                } else {
                    Panel::Install
                },
            ))
            .style(theme::plain);
        let overlay = column![
            space().height(Length::Fill),
            container(
                column![
                    text(&g.title).size(13).font(SEMIBOLD),
                    row![settings, space().width(Length::Fill), action].align_y(Alignment::Center),
                ]
                .spacing(4),
            )
            .padding([8, 10])
            .width(Length::Fill)
            .style(theme::cover_overlay),
        ];
        hover(
            base,
            container(overlay)
                .width(Length::Fill)
                .height(Length::Fill)
                .style(theme::cover_frame),
        )
    }

    fn achievements_page(&self) -> Element<'_, Message> {
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
        let scanning = if self.overview_busy || !self.overview_complete() {
            format!(
                "Reading GOG data… {}/{} games",
                self.overview.len().min(self.library.len()),
                self.library.len()
            )
        } else {
            format!("{} games with achievements", games.len())
        };
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
                .on_press(Message::SelectWith(g.id.clone(), Panel::Achievements))
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

    fn settings_page(&self) -> Element<'_, Message> {
        let account = self.account.as_ref();
        let choices: Vec<ProtonChoice> = self
            .proton_choices
            .iter()
            .cloned()
            .map(ProtonChoice)
            .collect();
        let selected = self.proton.clone().map(ProtonChoice);
        let label = |t| text(t).size(15).width(180).color(MUTED);
        let cache_note = match self.fetched_at {
            Some(ts) => format!("Last refreshed {}", local_time(ts)),
            None => "Never refreshed".into(),
        };
        let content = column![
            text("Settings").size(30).font(BOLD),
            card(
                "Account",
                vec![
                    row![
                        label("Signed in as"),
                        text(account.map(|a| a.username.as_str()).unwrap_or_default())
                            .size(15)
                            .width(Length::Fill),
                        button(text("Log out").size(14))
                            .padding([8, 16])
                            .on_press(Message::Logout)
                            .style(theme::tonal),
                    ]
                    .align_y(Alignment::Center)
                    .into(),
                ],
            ),
            card(
                "Library",
                vec![
                    row![
                        label("Games"),
                        text(format!("{} owned · {cache_note}", self.library.len()))
                            .size(15)
                            .width(Length::Fill),
                        button(
                            text(if self.library_busy {
                                "Refreshing…"
                            } else {
                                "Refresh library"
                            })
                            .size(14)
                        )
                        .padding([8, 16])
                        .on_press_maybe((!self.library_busy).then_some(Message::SyncLibrary))
                        .style(theme::tonal),
                    ]
                    .align_y(Alignment::Center)
                    .into(),
                ],
            ),
            card(
                "Installs",
                vec![
                    row![
                        label("Games folder"),
                        text_input("/home/…/Games/GOG", &self.library_root)
                            .on_input(|v| Message::Settings(SettingsMsg::RootInput(v)))
                            .on_submit(Message::Settings(SettingsMsg::SaveRoot))
                            .style(theme::field)
                            .padding([8, 12]),
                        button(text("Save").size(14))
                            .padding([8, 16])
                            .on_press(Message::Settings(SettingsMsg::SaveRoot))
                            .style(theme::tonal),
                    ]
                    .spacing(10)
                    .align_y(Alignment::Center)
                    .into(),
                    row![
                        label("Proton"),
                        pick_list(choices, selected, |c| Message::Settings(
                            SettingsMsg::Proton(c)
                        ))
                        .placeholder("No Proton found in compatibilitytools.d")
                        .style(theme::select)
                        .padding([8, 16]),
                    ]
                    .spacing(10)
                    .align_y(Alignment::Center)
                    .into(),
                ],
            ),
            card(
                "About",
                vec![
                    text(format!(
                        "SlattyLauncher {} · GPL-3.0-or-later · icons by Lucide (ISC)",
                        env!("CARGO_PKG_VERSION")
                    ))
                    .size(14)
                    .color(MUTED)
                    .into(),
                ],
            ),
        ]
        .spacing(16)
        .max_width(860);
        scrollable(container(content).padding(Padding::ZERO.top(10)))
            .style(theme::scroller)
            .height(Length::Fill)
            .into()
    }

    fn login_view(&self) -> Element<'_, Message> {
        let busy = self.login_busy;
        let content = column![
            logo(),
            text("Sign in to GOG").size(30).font(BOLD),
            text(
                "Sign-in happens in your browser; SlattyLauncher never sees your password. \
                 Once signed in, the browser shows an almost blank page on embed.gog.com: \
                 copy that page's full address and paste it below."
            )
            .color(MUTED),
            button(text("Open the GOG sign-in page").font(SEMIBOLD))
                .padding([12, 22])
                .on_press(Message::OpenLoginPage)
                .style(theme::primary),
            row![
                text_input(
                    "https://embed.gog.com/on_login_success?…&code=…",
                    &self.login_input
                )
                .on_input(Message::LoginInput)
                .on_submit(Message::SubmitLogin)
                .style(theme::field)
                .padding([10, 14])
                .width(Length::Fill),
                button(text("Paste"))
                    .padding([10, 16])
                    .on_press(Message::PasteLogin)
                    .style(theme::tonal),
                button(text(if busy { "Signing in…" } else { "Sign in" }))
                    .padding([10, 16])
                    .on_press_maybe(
                        (!busy && !self.login_input.is_empty()).then_some(Message::SubmitLogin)
                    )
                    .style(theme::tonal),
            ]
            .spacing(8),
        ]
        .spacing(18)
        .max_width(720);
        container(content).padding(60).center_x(Length::Fill).into()
    }
}

pub fn logo<'a>() -> Element<'a, Message> {
    text("slatty").size(30).font(BOLD).color(ACCENT).into()
}

fn download_banner<'a>(id: &str, title: &str, p: Progress) -> Element<'a, Message> {
    button(
        row![
            icon(Icon::Download, 16.0, ACCENT),
            text(format!("Downloading {title}"))
                .size(14)
                .width(Length::Fill),
            progress_bar(0.0..=1.0, fraction(p))
                .length(260)
                .girth(8)
                .style(theme::progress),
            text(format!("{:.0} %", fraction(p) * 100.0)).size(14),
        ]
        .spacing(12)
        .align_y(Alignment::Center),
    )
    .padding([10, 16])
    .width(Length::Fill)
    .on_press(Message::SelectWith(id.to_string(), Panel::Install))
    .style(theme::row_button)
    .into()
}

/// A titled block of the settings page or of a panel.
pub fn card<'a>(title: &'a str, items: Vec<Element<'a, Message>>) -> Element<'a, Message> {
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

/// Unlocked achievements, newest first.
pub fn latest_unlocked(list: &[Achievement]) -> Vec<&Achievement> {
    let mut done: Vec<&Achievement> = list.iter().filter(|a| a.date_unlocked.is_some()).collect();
    done.sort_by(|a, b| b.date_unlocked.cmp(&a.date_unlocked));
    done.truncate(3);
    done
}

pub fn duration(seconds: i64) -> String {
    let minutes = seconds / 60;
    match (minutes / 60, minutes % 60) {
        (0, m) => format!("{m} m"),
        (h, m) => format!("{h} h {m} m"),
    }
}

/// "Today", "Yesterday", "3 days ago", or the date.
pub fn relative_day(ts: i64) -> String {
    use chrono::{Local, TimeZone};
    let Some(then) = Local.timestamp_opt(ts, 0).single() else {
        return String::new();
    };
    match (Local::now().date_naive() - then.date_naive()).num_days() {
        0 => "Today".into(),
        1 => "Yesterday".into(),
        d @ 2..=6 => format!("{d} days ago"),
        _ => then.format("%-d %b %Y").to_string(),
    }
}

pub fn local_time(ts: i64) -> String {
    use chrono::TimeZone;
    chrono::Local
        .timestamp_opt(ts, 0)
        .single()
        .map(|t| t.format("%d/%m/%Y %H:%M").to_string())
        .unwrap_or_default()
}

pub fn describe_warning(w: Warning) -> &'static str {
    match w {
        Warning::LocalRootMissing => "local folder missing: no cloud file will be deleted",
        Warning::LocalEmptyWithHistory => {
            "local folder empty although it held saves: deletions blocked"
        }
        Warning::RemoteEmptyWithHistory => {
            "cloud empty although it held saves: local deletions blocked"
        }
        Warning::RootChanged => "the save folder changed: previous history is ignored",
    }
}

fn describe_cloud(prefix: &str, s: &CloudSummary) -> String {
    let mut out = format!(
        "{prefix}: {} uploaded, {} downloaded",
        s.uploaded, s.downloaded
    );
    if !s.conflicts.is_empty() {
        out += &format!("; conflicts: {}", s.conflicts.join(", "));
    }
    if !s.problems.is_empty() {
        out += &format!("; problems: {}", s.problems.join(", "));
    }
    out
}

pub fn describe_play_event(e: &PlayEvent) -> String {
    match e {
        PlayEvent::PreparingPrefix => "First launch: creating the Wine prefix…".into(),
        PlayEvent::SetupStep(step) => format!("Setup: {step}…"),
        PlayEvent::SetupWarning(w) => format!("Setup warning: {w}"),
        PlayEvent::SetupSkipped(why) => {
            format!("Setup not run ({why}); it will be retried at the next launch.")
        }
        PlayEvent::CloudChecked(s) => describe_cloud("Cloud checked", s),
        PlayEvent::CloudSkipped(why) => {
            format!("Cloud not checked ({why}); local saves are kept.")
        }
        PlayEvent::Blocked(s) => format!(
            "{}. Launch cancelled: resolve the conflict under Cloud saves.",
            describe_cloud("Cloud needs attention", s)
        ),
        PlayEvent::CometReady => {
            "Comet running: achievements earned in game are sent to GOG.".into()
        }
        PlayEvent::CometUnavailable(why) => {
            format!("Achievements unavailable for this session: {why}")
        }
        PlayEvent::Started { pid } => format!("Game started (pid {pid})."),
        PlayEvent::LauncherExited { code } => {
            format!("Launcher exited ({code:?}); following remaining game processes…")
        }
        PlayEvent::StopRequested => "Stopping…".into(),
        PlayEvent::Ended { seconds, clean, .. } => format!(
            "Session ended after {} min{}.",
            seconds / 60,
            if *clean { "" } else { " (end uncertain)" }
        ),
        PlayEvent::CloudUploaded(s) => describe_cloud("Cloud after playing", s),
        PlayEvent::CloudUploadSkipped(why) => {
            format!("Cloud not synced ({why}); local saves are kept.")
        }
        PlayEvent::Unlocked(names) => {
            format!("Achievements recorded on GOG: {}", names.join(", "))
        }
        PlayEvent::NoNewAchievement => "No new achievement recorded on GOG.".into(),
        PlayEvent::AchievementsUnknown => "Could not read achievements back from GOG.".into(),
    }
}
