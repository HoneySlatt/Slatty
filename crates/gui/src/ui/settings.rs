//! Settings tab and the sign-in screen.

use crate::theme::text;
use iced::widget::{
    Column, Space, button, column, container, pick_list, row, scrollable, text_input, toggler,
};
use iced::{Alignment, Element, Length, Padding};

use super::format::*;
use super::note;
use super::widgets::logo;
use crate::icons::{Icon, icon};
use crate::runners::RunnersMsg;
use crate::settings::{
    COVER_SIZES, CoverSize, FontChoice, PlatformChoice, ProtonChoice, SETTINGS_SCROLL, Section,
    SettingsMsg,
};
use crate::theme::{self, bold, semibold, tokens};
use crate::updates::UpdatesMsg;
use crate::{App, Message};
use slatty_core::protons::{Release, Source, Stage};

impl App {
    /// A side list of the parts of the page, and their cards; an entry brings its card to the top.
    pub(super) fn settings_page(&self) -> Element<'_, Message> {
        let entries = Section::ALL.map(|s| {
            let active = self.settings_view.section == s;
            let color = if active {
                tokens().accent
            } else {
                tokens().text
            };
            let marker: Element<'_, Message> = if active {
                container(Space::new())
                    .width(4)
                    .height(28)
                    .style(theme::side_marker)
                    .into()
            } else {
                Space::new().width(4).into()
            };
            button(
                row![
                    marker,
                    icon(section_icon(s), 20.0, color),
                    text(s.title()).size(15).color(color)
                ]
                .spacing(14)
                .align_y(Alignment::Center),
            )
            .padding(Padding::from([10, 12]).left(0))
            .width(Length::Fill)
            .on_press(Message::Settings(SettingsMsg::Show(s)))
            .style(theme::side_entry(active))
            .into()
        });
        let side = container(Column::with_children(entries).spacing(6))
            .padding(10)
            .width(SIDE_WIDTH)
            .height(Length::Fill)
            .style(theme::card);
        // Room for a name, a control and what follows it side by side, once the side list, the
        // margins and the indent are taken from the window.
        let wide = self.window.width - SIDE_WIDTH - 172.0 >= LABEL_WIDTH + AFTER_WIDTH + 330.0;
        let cards = Section::ALL.map(|s| {
            let rows = match s {
                Section::Account => self.account_rows(wide),
                Section::Library => self.library_rows(wide),
                Section::Installs => self.installs_rows(wide),
                Section::Runners => self.runners_rows(wide),
                Section::Appearance => self.appearance_rows(wide),
                Section::Privacy => self.privacy_rows(wide),
                Section::Advanced => self.advanced_rows(wide),
                Section::About => vec![note(format!(
                    "SlattyLauncher {} · GPL-3.0-or-later · icons by Lucide (ISC) · Geist font (OFL)",
                    env!("CARGO_PKG_VERSION")
                ))],
            };
            container(
                column![
                    row![
                        icon(section_icon(s), 24.0, tokens().text),
                        text(s.title()).size(18).font(semibold())
                    ]
                    .spacing(14)
                    .align_y(Alignment::Center),
                    // Under the title rather than under the icon.
                    container(Column::with_children(rows).spacing(12))
                        .padding(Padding::ZERO.left(38)),
                ]
                .spacing(14),
            )
            .id(s.id())
            .padding(20)
            .width(Length::Fill)
            .style(theme::card)
            .into()
        });
        let content = scrollable(
            container(Column::with_children(cards).spacing(14)).padding(Padding::ZERO.right(14)),
        )
        .id(SETTINGS_SCROLL)
        .on_scroll(|v| {
            Message::Settings(SettingsMsg::Scrolled {
                offset: v.absolute_offset().y,
                max: (v.content_bounds().height - v.bounds().height).max(0.0),
            })
        })
        .spacing(8)
        .style(theme::scroller)
        .height(Length::Fill);
        column![
            text("Settings").size(32).font(bold()),
            row![side, content].spacing(20)
        ]
        .spacing(18)
        .padding(Padding::ZERO.top(10))
        .into()
    }

    pub(super) fn login_view(&self) -> Element<'_, Message> {
        let busy = self.login_busy;
        let content = column![
            logo(72.0),
            text("Sign in to GOG").size(30).font(bold()),
            text(
                "Sign-in happens in your browser; SlattyLauncher never sees your password. \
                 Once signed in, the browser shows an almost blank page on embed.gog.com: \
                 copy that page's full address and paste it below."
            )
            .color(tokens().muted),
            button(text("Open the GOG sign-in page").font(semibold()))
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
                .font(theme::font())
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
impl App {
    /// What leaves the computer besides GOG's own services, each with its switch.
    fn privacy_rows(&self, wide: bool) -> Vec<Element<'_, Message>> {
        vec![
            setting(
                wide,
                "Proton fixes",
                switch(
                    self.umu_lookup,
                    SettingsMsg::UmuLookup,
                    "At a game's first launch, sends its GOG id to umu's public database to pick \
                     its community fixes.",
                ),
                None,
            ),
            setting(
                wide,
                "Achievements in game",
                switch(
                    self.game_achievements,
                    SettingsMsg::GameAchievements,
                    "Starts Comet while a game that uses GOG's Galaxy runs, so what it unlocks \
                     reaches GOG. Comet acts for your account and listens on this computer until \
                     the game ends. Unlocking by hand (Advanced) does not need it.",
                ),
                None,
            ),
            setting(
                wide,
                "Isolate new Windows games",
                switch(
                    self.isolate_new_games,
                    SettingsMsg::IsolateNewGames,
                    "Windows games installed from now on run without access to your files: they \
                     see their own folder and a home folder of their own. Each game's Game \
                     settings changes it.",
                ),
                None,
            ),
            setting(
                wide,
                "Play time on GOG",
                switch(
                    self.report_playtime,
                    SettingsMsg::ReportPlaytime,
                    "Sends each session to GOG, as Galaxy does, so it counts on your profile. \
                     Sessions played while off are never sent.",
                ),
                None,
            ),
            note(
                "No telemetry: SlattyLauncher talks to GOG, to the download servers GOG names, \
                 to umu's database when allowed above, to GitHub once Proton downloads are \
                 turned on in Runners, and to SteamGridDB once turned on in Advanced.",
            ),
        ]
    }

    /// What most people never need, off until turned on.
    fn advanced_rows(&self, wide: bool) -> Vec<Element<'_, Message>> {
        let mut rows = vec![
            self.manual_achievements_row(wide),
            self.steamgriddb_row(wide),
        ];
        if self.steamgriddb {
            rows.push(self.steamgriddb_key_row(wide));
        }
        rows
    }

    fn steamgriddb_row(&self, wide: bool) -> Element<'_, Message> {
        setting(
            wide,
            "SteamGridDB",
            switch(
                self.steamgriddb,
                SettingsMsg::SteamGridDb,
                "Offers covers and backgrounds from SteamGridDB, a community database of game \
                 art, in Edit game. Searching sends the name you type and your API key to \
                 steamgriddb.com.",
            ),
            None,
        )
    }

    /// The key to SteamGridDB's API: typed once, then kept in the keyring and never shown.
    fn steamgriddb_key_row(&self, wide: bool) -> Element<'_, Message> {
        let msg = |m| Message::Settings(m);
        let (control, after): (Element<'_, Message>, Element<'_, Message>) =
            if self.steamgriddb_key == Some(true) {
                (
                    text("Saved in the system keyring").size(14).into(),
                    button(text("Remove").size(14))
                        .padding([8, 14])
                        .on_press(msg(SettingsMsg::RemoveSteamGridDbKey))
                        .style(theme::tonal)
                        .into(),
                )
            } else {
                let typed = !self.steamgriddb_key_input.trim().is_empty();
                (
                    column![
                        text_input("Paste your API key", &self.steamgriddb_key_input)
                            .secure(true)
                            .on_input(|v| Message::Settings(SettingsMsg::SteamGridDbKeyInput(v)))
                            .on_submit(msg(SettingsMsg::SaveSteamGridDbKey))
                            .style(theme::field)
                            .font(theme::font())
                            .padding([8, 12]),
                        button(text("Create a key on steamgriddb.com").size(14))
                            .padding(0)
                            .on_press(msg(SettingsMsg::OpenSteamGridDbKeyPage))
                            .style(theme::link),
                    ]
                    .spacing(8)
                    .into(),
                    button(text("Save").size(14))
                        .padding([8, 14])
                        .on_press_maybe(typed.then(|| msg(SettingsMsg::SaveSteamGridDbKey)))
                        .style(theme::tonal)
                        .into(),
                )
            };
        setting(wide, "API key", control, Some(after))
    }

    fn manual_achievements_row(&self, wide: bool) -> Element<'_, Message> {
        setting(
            wide,
            "Manual achievements",
            switch(
                self.manual_achievements,
                SettingsMsg::ManualAchievements,
                "Offers Unlock, Clear and Unlock all beside a game's achievements. A change goes \
                 straight to your public GOG profile, dated today, and is probably against GOG's \
                 terms.",
            ),
            None,
        )
    }
}

impl App {
    /// Proton builds from GitHub: the switch, the newest build of each project once asked for,
    /// and the builds already downloaded.
    fn runners_rows(&self, wide: bool) -> Vec<Element<'_, Message>> {
        let r = &self.runners;
        let msg = |m| Message::Runners(m);
        let mut rows = vec![setting(
            wide,
            "Proton downloads",
            column![
                toggler(r.downloads)
                    .on_toggle(move |on| msg(RunnersMsg::Downloads(on)))
                    .size(22),
                note(
                    "Lists and downloads GE-Proton, Proton-CachyOS and UMU-Proton from their \
                     GitHub releases when you ask, and at start once Keep up to date is on. Each \
                     build is checked against the sum its release publishes.",
                ),
            ]
            .spacing(8)
            .into(),
            None,
        )];
        if r.downloads {
            let update: Element<'_, Message> = match &r.installing {
                Some((name, stage, _)) if r.updating => column![
                    text(if name.is_empty() {
                        "Checking GitHub…".to_string()
                    } else {
                        format!("{name}: {}", stage_text(*stage))
                    })
                    .size(14),
                    button(text("Stop").size(14))
                        .padding([8, 16])
                        .on_press(msg(RunnersMsg::Cancel))
                        .style(theme::tonal),
                ]
                .spacing(8)
                .into(),
                _ => button(text("Update now").size(14))
                    .padding([8, 16])
                    .on_press_maybe(r.installing.is_none().then_some(msg(RunnersMsg::Update)))
                    .style(theme::tonal)
                    .into(),
            };
            rows.push(setting(
                wide,
                "Keep up to date",
                column![
                    toggler(r.updates)
                        .on_toggle(move |on| msg(RunnersMsg::Updates(on)))
                        .size(22),
                    note(
                        "Once a day at start, downloads the newest build of each project a game \
                         or the default Proton follows as <project>-latest (chosen in the Proton \
                         menus). The build before is kept to go back to; older ones no game uses \
                         are deleted.",
                    ),
                ]
                .spacing(8)
                .into(),
                Some(update),
            ));
            rows.push(setting(
                wide,
                "Newest builds",
                note(if r.releases.is_empty() {
                    "Not checked yet."
                } else {
                    "From GitHub, as of the last check."
                }),
                Some(
                    button(
                        text(if r.checking {
                            "Checking…"
                        } else {
                            "Check GitHub"
                        })
                        .size(14),
                    )
                    .padding([8, 16])
                    .on_press_maybe((!r.checking).then_some(msg(RunnersMsg::Check)))
                    .style(theme::tonal)
                    .into(),
                ),
            ));
            for (source, release) in &r.releases {
                rows.push(self.release_row(wide, *source, release));
            }
        }
        if !r.downloaded.is_empty() {
            let builds = r.downloaded.iter().map(|path| {
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                row![
                    text(name.into_owned()).size(15).width(Length::Fill),
                    button(text("Delete").size(14))
                        .padding([6, 14])
                        .on_press(msg(RunnersMsg::Remove(path.clone())))
                        .style(theme::tonal),
                ]
                .spacing(12)
                .align_y(Alignment::Center)
                .into()
            });
            rows.push(setting(
                wide,
                "Downloaded",
                Column::with_children(builds).spacing(8).into(),
                Some(note("Kept until deleted. A build a game uses stays.")),
            ));
        }
        rows
    }

    fn release_row<'a>(
        &'a self,
        wide: bool,
        source: Source,
        release: &'a Result<Release, String>,
    ) -> Element<'a, Message> {
        let r = &self.runners;
        let release = match release {
            Ok(release) => release,
            Err(e) => return setting(wide, source.name(), note(e.clone()), None),
        };
        let what = format!("{} · {}", release.name, human_size(release.size));
        let installing = r
            .installing
            .as_ref()
            .filter(|(name, _, _)| *name == release.name);
        let downloaded = self
            .core
            .as_ref()
            .is_some_and(|c| r.downloaded.contains(&release.path(&c.dirs)));
        let (control, after): (Element<'_, Message>, Element<'_, Message>) =
            if let Some((_, stage, _)) = installing {
                (
                    text(stage_text(*stage)).size(15).into(),
                    button(text("Stop").size(14))
                        .padding([8, 16])
                        .on_press(Message::Runners(RunnersMsg::Cancel))
                        .style(theme::tonal)
                        .into(),
                )
            } else if downloaded {
                (text(what).size(15).into(), note("Installed"))
            } else {
                (
                    text(what).size(15).into(),
                    button(text("Install").size(14))
                        .padding([8, 16])
                        .on_press_maybe(
                            r.installing
                                .is_none()
                                .then(|| Message::Runners(RunnersMsg::Install(release.clone()))),
                        )
                        .style(theme::tonal)
                        .into(),
                )
            };
        setting(wide, source.name(), control, Some(after))
    }
}

/// Where a download stands, in words.
fn stage_text(stage: Option<Stage>) -> String {
    let percent = |done: u64, total: u64| (done * 100).checked_div(total).unwrap_or(100);
    match stage {
        None => "Starting…".into(),
        Some(Stage::Downloading { done, total }) => {
            format!("Downloading {} %", percent(done, total))
        }
        Some(Stage::Verifying) => "Checking the sum…".into(),
        Some(Stage::Unpacking { done, total }) => format!("Unpacking {} %", percent(done, total)),
    }
}

/// A switch, with what it does below it.
fn switch<'a>(on: bool, msg: fn(bool) -> SettingsMsg, what: &'static str) -> Element<'a, Message> {
    column![
        toggler(on)
            .on_toggle(move |v| Message::Settings(msg(v)))
            .size(22),
        note(what),
    ]
    .spacing(8)
    .into()
}

impl App {
    fn account_rows(&self, wide: bool) -> Vec<Element<'_, Message>> {
        let name = self.account.as_ref().map(|a| a.username.as_str());
        vec![setting(
            wide,
            "Signed in as",
            text(name.unwrap_or_default()).size(15).into(),
            Some(action("Log out", Message::Logout)),
        )]
    }

    fn library_rows(&self, wide: bool) -> Vec<Element<'_, Message>> {
        let refreshed = match self.fetched_at {
            Some(ts) => format!("Last refreshed {}", local_time(ts)),
            None => "Never refreshed".into(),
        };
        let refresh = button(
            text(if self.library_busy {
                "Refreshing…"
            } else {
                "Refresh library"
            })
            .size(14),
        )
        .padding([8, 16])
        .on_press_maybe((!self.library_busy).then_some(Message::SyncLibrary))
        .style(theme::tonal);
        vec![
            setting(
                wide,
                "Games",
                text(format!("{} owned · {refreshed}", self.library.len()))
                    .size(15)
                    .into(),
                Some(refresh.into()),
            ),
            setting(
                wide,
                "Cover size",
                select(pick_list(
                    COVER_SIZES.map(CoverSize),
                    Some(CoverSize::of(self.card_width)),
                    |s| Message::Settings(SettingsMsg::CoverSize(s)),
                )),
                None,
            ),
        ]
    }

    fn installs_rows(&self, wide: bool) -> Vec<Element<'_, Message>> {
        let protons: Vec<ProtonChoice> = self
            .proton_choices
            .iter()
            .cloned()
            .map(ProtonChoice)
            .collect();
        let browse = button(
            row![
                icon(Icon::FolderOpen, 16.0, tokens().text),
                text("Browse").size(14)
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        )
        .padding([8, 16])
        .on_press(Message::Settings(SettingsMsg::BrowseRoot))
        .style(theme::tonal);
        vec![
            setting(
                wide,
                "Default installation path",
                text_input("/home/…/Games/GOG", &self.library_root)
                    .on_input(|v| Message::Settings(SettingsMsg::RootInput(v)))
                    .on_submit(Message::Settings(SettingsMsg::SaveRoot))
                    .style(theme::field)
                    .font(theme::font())
                    .padding([8, 12])
                    .into(),
                Some(browse.into()),
            ),
            setting(
                wide,
                "Default platform",
                select(pick_list(
                    PlatformChoice::ALL,
                    Some(PlatformChoice(self.default_platform)),
                    |c| Message::Settings(SettingsMsg::Platform(c)),
                )),
                Some(note("For games GOG offers on both.")),
            ),
            setting(
                wide,
                "Default Proton",
                select(
                    pick_list(protons, self.proton.clone().map(ProtonChoice), |c| {
                        Message::Settings(SettingsMsg::Proton(c))
                    })
                    .placeholder("No Proton build found (Steam or compatibilitytools.d)"),
                ),
                None,
            ),
            setting(
                wide,
                "Update games automatically",
                column![
                    toggler(self.updates.on)
                        .on_toggle(|on| Message::Updates(UpdatesMsg::Toggle(on)))
                        .size(22),
                    note(
                        "Checks at start and every six hours, and updates games on their newest \
                         build one after the other, never while a game runs. A game on an older \
                         version you chose stays on it.",
                    ),
                ]
                .spacing(8)
                .into(),
                None,
            ),
        ]
    }

    /// The built-in themes, the font and the theme file, with the way to create, edit and apply it.
    fn appearance_rows(&self, wide: bool) -> Vec<Element<'_, Message>> {
        let Some(core) = &self.core else {
            return Vec::new();
        };
        let path = crate::theme::file(&core.dirs.config);
        let exists = path.exists();
        let file_actions: Element<'_, Message> = if exists {
            row![
                action("Edit", Message::Settings(SettingsMsg::EditTheme)),
                action("Reload", Message::Settings(SettingsMsg::ReloadTheme)),
            ]
            .spacing(8)
            .into()
        } else {
            action(
                "Create theme file",
                Message::Settings(SettingsMsg::CreateTheme),
            )
        };
        let current = match theme::font().family {
            iced::font::Family::Name(name) if name != theme::DEFAULT_FAMILY => {
                FontChoice::Family(name.to_string())
            }
            _ => FontChoice::Default,
        };
        let fonts: Vec<FontChoice> = std::iter::once(FontChoice::Default)
            .chain(
                self.font_families
                    .iter()
                    .filter(|f| *f != theme::DEFAULT_FAMILY)
                    .cloned()
                    .map(FontChoice::Family),
            )
            .collect();
        vec![
            setting(
                wide,
                "Theme",
                select(pick_list(
                    crate::presets::Preset::ALL,
                    Some(theme::preset()),
                    |p| Message::Settings(SettingsMsg::Preset(p)),
                )),
                None,
            ),
            setting(
                wide,
                "Font",
                select(pick_list(fonts, Some(current), |f| {
                    Message::Settings(SettingsMsg::Font(f))
                })),
                None,
            ),
            setting(
                wide,
                "Theme file",
                container(text(path.display().to_string()).size(14))
                    .padding([9, 12])
                    .width(Length::Fill)
                    .style(theme::outlined)
                    .into(),
                Some(file_actions),
            ),
            note(if exists {
                "Colours, corners and the page transition come from this file. Edit it, then \
                 Reload to see the change."
            } else {
                "The default look is in use. Create the theme file to change colours, corners and \
                 the page transition."
            }),
        ]
    }
}

/// Width of the side list of the Settings page.
const SIDE_WIDTH: f32 = 236.0;
/// Width of the labels, and of the space for what follows a setting, so the settings line up.
const LABEL_WIDTH: f32 = 240.0;
const AFTER_WIDTH: f32 = 250.0;

fn section_icon(s: Section) -> Icon {
    match s {
        Section::Account => Icon::User,
        Section::Library => Icon::LayoutGrid,
        Section::Installs => Icon::Download,
        Section::Runners => Icon::Package,
        Section::Appearance => Icon::Palette,
        Section::Privacy => Icon::Shield,
        Section::Advanced => Icon::Wrench,
        Section::About => Icon::Info,
    }
}

/// A setting: its name, its value or control, then a button or a note. On a narrow page the name
/// goes above the rest.
fn setting<'a>(
    wide: bool,
    label: &'a str,
    control: Element<'a, Message>,
    after: Option<Element<'a, Message>>,
) -> Element<'a, Message> {
    let label = text(label).size(15).color(tokens().muted);
    if !wide {
        return column![
            label,
            row![container(control).width(Length::Fill)]
                .push(after)
                .spacing(12)
                .align_y(Alignment::Center),
        ]
        .spacing(8)
        .into();
    }
    row![
        label.width(LABEL_WIDTH),
        container(control).width(Length::Fill),
        container(after.unwrap_or_else(|| Space::new().into())).width(AFTER_WIDTH),
    ]
    .spacing(16)
    .align_y(Alignment::Center)
    .into()
}

fn select<'a, T, L, V>(list: iced::widget::PickList<'a, T, L, V, Message>) -> Element<'a, Message>
where
    T: ToString + PartialEq + Clone + 'a,
    L: std::borrow::Borrow<[T]> + 'a,
    V: std::borrow::Borrow<T> + 'a,
{
    list.style(theme::select)
        .font(theme::font())
        .padding([8, 16])
        .width(Length::Fill)
        .into()
}

fn action<'a>(label: &'a str, msg: Message) -> Element<'a, Message> {
    button(text(label).size(14))
        .padding([8, 16])
        .on_press(msg)
        .style(theme::tonal)
        .into()
}
