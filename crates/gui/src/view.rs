use iced::widget::{
    Column, button, checkbox, column, container, grid, image, pick_list, progress_bar, row,
    scrollable, text, text_input,
};
use iced::{Alignment, ContentFit, Element, Length};
use slatty_core::achievements::Achievement;
use slatty_core::cloud::plan::Warning;
use slatty_core::cloud::sync::Prefer;
use slatty_core::install::Install;
use slatty_core::installer::{DlcChoice, Progress};
use slatty_core::library::LibraryGame;
use slatty_core::maintenance::Change;
use slatty_core::play::{CloudSummary, PlayEvent};
use slatty_core::runner::Runner;

use crate::installs::{
    ContentInfo, InstallMsg, InstallView, MaintenanceMsg, ProtonChoice, SettingsMsg, human_size,
};
use crate::{AchievementChange, App, CloudRequest, Loadable, Message, PendingChange};

fn fraction(p: Progress) -> f32 {
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
            return container(text("Loading…")).padding(40).into();
        }
        let body = match &self.account {
            None => self.login_view(),
            Some(_) => self.library_view(),
        };
        let mut page = Column::new();
        if let Some(n) = &self.notice {
            let style = if n.error {
                container::danger
            } else {
                container::secondary
            };
            page = page.push(
                container(
                    row![
                        text(&n.text).width(Length::Fill),
                        button(text("Close"))
                            .on_press(Message::DismissNotice)
                            .style(button::text)
                    ]
                    .align_y(Alignment::Center)
                    .spacing(12),
                )
                .padding(10)
                .width(Length::Fill)
                .style(style),
            );
        }
        page.push(body).into()
    }

    fn login_view(&self) -> Element<'_, Message> {
        let busy = self.login_busy;
        let content = column![
            text("Sign in to GOG").size(28),
            text(
                "Sign-in happens in your browser; SlattyLauncher never sees your password. \
                 Once signed in, the browser shows an almost blank page on embed.gog.com: \
                 copy that page's full address and paste it below."
            ),
            button(text("Open the GOG sign-in page")).on_press(Message::OpenLoginPage),
            row![
                text_input(
                    "https://embed.gog.com/on_login_success?…&code=…",
                    &self.login_input
                )
                .on_input(Message::LoginInput)
                .on_submit(Message::SubmitLogin)
                .width(Length::Fill),
                button(text("Paste"))
                    .on_press(Message::PasteLogin)
                    .style(button::secondary),
                button(text(if busy { "Signing in…" } else { "Sign in" })).on_press_maybe(
                    (!busy && !self.login_input.is_empty()).then_some(Message::SubmitLogin)
                ),
            ]
            .spacing(8),
        ]
        .spacing(16)
        .max_width(720);
        container(content).padding(40).center_x(Length::Fill).into()
    }

    fn library_view(&self) -> Element<'_, Message> {
        let account = self
            .account
            .as_ref()
            .map(|a| a.username.as_str())
            .unwrap_or_default();
        let cache_note = match self.fetched_at {
            Some(ts) => format!("cached {}", local_time(ts)),
            None => "no cache".into(),
        };
        let top = row![
            text_input("Search…", &self.search)
                .on_input(Message::Search)
                .width(Length::FillPortion(3)),
            text(format!("{} games · {cache_note}", self.library.len()))
                .width(Length::FillPortion(2)),
            button(text(if self.library_busy {
                "Refreshing…"
            } else {
                "Refresh"
            }))
            .on_press_maybe((!self.library_busy).then_some(Message::SyncLibrary))
            .style(button::secondary),
            text(account),
            button(text("Settings"))
                .on_press(Message::Settings(SettingsMsg::Toggle))
                .style(button::text),
            button(text("Log out"))
                .on_press(Message::Logout)
                .style(button::text),
        ]
        .spacing(12)
        .align_y(Alignment::Center);
        let mut header = column![top].spacing(10);
        if let Some((id, title, p)) = self.installing() {
            header = header.push(
                button(
                    row![
                        text(format!("Downloading {title}"))
                            .size(13)
                            .width(Length::Fill),
                        progress_bar(0.0..=1.0, fraction(p)).length(240).girth(8),
                        text(format!("{:.0} %", fraction(p) * 100.0)).size(13),
                    ]
                    .spacing(10)
                    .align_y(Alignment::Center),
                )
                .on_press(Message::Select(id.to_string()))
                .style(button::text),
            );
        }
        if self.settings_open {
            header = header.push(self.settings_panel());
        }

        let needle = self.search.to_lowercase();
        let cards: Vec<Element<'_, Message>> = self
            .library
            .iter()
            .filter(|g| needle.is_empty() || g.title.to_lowercase().contains(&needle))
            .map(|g| self.card(g))
            .collect();
        let gallery: Element<'_, Message> = if cards.is_empty() {
            container(text(if self.library.is_empty() {
                "Your library is empty: click Refresh."
            } else {
                "No game matches the search."
            }))
            .padding(20)
            .into()
        } else {
            scrollable(
                grid(cards)
                    .fluid(190)
                    .spacing(14)
                    .height(grid::aspect_ratio(3, 5)),
            )
            .spacing(10)
            .height(Length::Fill)
            .into()
        };

        let main = match self
            .selected
            .as_ref()
            .and_then(|id| self.library.iter().find(|g| &g.id == id))
        {
            Some(game) => row![
                container(gallery).width(Length::FillPortion(3)),
                container(self.detail(game))
                    .width(Length::FillPortion(2))
                    .height(Length::Fill)
            ]
            .spacing(16),
            None => row![gallery],
        };
        column![header, main].spacing(16).padding(16).into()
    }

    fn settings_panel(&self) -> Element<'_, Message> {
        let choices: Vec<ProtonChoice> = self
            .proton_choices
            .iter()
            .cloned()
            .map(ProtonChoice)
            .collect();
        let selected = self.proton.clone().map(ProtonChoice);
        section(
            "Settings",
            vec![
                row![
                    text("Games folder").size(14).width(160),
                    text_input("/home/…/Games/GOG", &self.library_root)
                        .on_input(|v| Message::Settings(SettingsMsg::RootInput(v)))
                        .on_submit(Message::Settings(SettingsMsg::SaveRoot)),
                    button(text("Enregistrer"))
                        .on_press(Message::Settings(SettingsMsg::SaveRoot))
                        .style(button::secondary),
                ]
                .spacing(8)
                .align_y(Alignment::Center)
                .into(),
                row![
                    text("Proton").size(14).width(160),
                    pick_list(choices, selected, |c| Message::Settings(
                        SettingsMsg::Proton(c)
                    ))
                    .placeholder("No Proton found in compatibilitytools.d"),
                ]
                .spacing(8)
                .align_y(Alignment::Center)
                .into(),
            ],
        )
    }

    fn install_section<'a>(&'a self, g: &'a LibraryGame) -> Element<'a, Message> {
        let prepare = |label| {
            button(text(label))
                .on_press(Message::Install(InstallMsg::Prepare(g.id.clone(), None)))
                .style(button::secondary)
        };
        let mut items: Vec<Element<'_, Message>> = Vec::new();
        match self.install_views.get(&g.id) {
            None => {
                items.push(text("Not installed.").size(14).into());
                items.push(prepare("Prepare install").into());
            }
            Some(InstallView::Planning) => {
                items.push(text("Reading build information from GOG…").into())
            }
            Some(InstallView::Failed(e)) => {
                items.push(text(format!("Failed: {e}")).size(13).into());
                items.push(prepare("Retry").into());
            }
            Some(InstallView::Running { progress, .. }) => {
                items.push(
                    progress_bar(0.0..=1.0, fraction(*progress))
                        .girth(10)
                        .into(),
                );
                items.push(
                    text(format!(
                        "{} / {} · fichiers {}/{}",
                        human_size(progress.bytes_done),
                        human_size(progress.bytes_total),
                        progress.files_done,
                        progress.files_total
                    ))
                    .size(13)
                    .into(),
                );
                items.push(
                    button(text("Pause"))
                        .on_press(Message::Install(InstallMsg::Pause(g.id.clone())))
                        .style(button::secondary)
                        .into(),
                );
            }
            Some(InstallView::Ready(info)) => {
                items.push(text(format!("Version {}", info.version)).size(14).into());
                items.push(
                    text(format!(
                        "Download {} · on disk {}",
                        human_size(info.total_download()),
                        human_size(info.total_disk())
                    ))
                    .size(13)
                    .into(),
                );
                items.push(
                    text(format!("Folder: {}", info.folder.display()))
                        .size(13)
                        .into(),
                );
                if info.resumable {
                    items.push(
                        text(format!(
                            "Resuming an interrupted install ({}).",
                            info.language
                        ))
                        .size(13)
                        .into(),
                    );
                } else if info.languages.len() > 1 {
                    let id = g.id.clone();
                    items.push(
                        row![
                            text("Language").size(13),
                            pick_list(
                                info.languages.clone(),
                                Some(info.language.clone()),
                                move |l| {
                                    Message::Install(InstallMsg::Prepare(id.clone(), Some(l)))
                                }
                            ),
                        ]
                        .spacing(8)
                        .align_y(Alignment::Center)
                        .into(),
                    );
                }
                for d in &info.dlcs {
                    items.push(dlc_row(d, d.selected, !info.resumable, {
                        let (id, dlc) = (g.id.clone(), d.id.clone());
                        move |_| Message::Install(InstallMsg::ToggleDlc(id.clone(), dlc.clone()))
                    }));
                }
                if !info.dependencies.is_empty() {
                    items.push(
                        text(format!(
                            "Redistributables not installed by slatty: {}",
                            info.dependencies.join(", ")
                        ))
                        .size(12)
                        .into(),
                    );
                }
                if self.proton.is_none() {
                    items.push(text("Choose a Proton version in Settings.").size(13).into());
                }
                let busy = self.installing().is_some();
                items.push(
                    button(text(if info.resumable {
                        "Resume install"
                    } else {
                        "Install"
                    }))
                    .on_press_maybe(
                        (!busy && self.proton.is_some())
                            .then(|| Message::Install(InstallMsg::Start(g.id.clone()))),
                    )
                    .style(button::success)
                    .into(),
                );
            }
        }
        section("Installation", items)
    }

    fn card<'a>(&'a self, g: &'a LibraryGame) -> Element<'a, Message> {
        let art: Element<'_, Message> = match self.covers.get(&g.id) {
            Some(h) => image(h.clone())
                .content_fit(ContentFit::Cover)
                .width(Length::Fill)
                .height(Length::Fill)
                .into(),
            None => container(text(&g.title).size(14))
                .padding(8)
                .width(Length::Fill)
                .height(Length::Fill)
                .style(container::secondary)
                .into(),
        };
        let installed = self.installs.contains_key(&g.id);
        let label = row![
            text(&g.title).size(13).width(Length::Fill),
            text(if installed { "●" } else { "" }).size(13)
        ];
        let selected = self.selected.as_deref() == Some(g.id.as_str());
        button(column![art, label].spacing(6))
            .on_press(Message::Select(g.id.clone()))
            .padding(4)
            .style(if selected {
                button::primary
            } else {
                button::text
            })
            .into()
    }

    fn detail<'a>(&'a self, g: &'a LibraryGame) -> Element<'a, Message> {
        let mut col = column![
            row![
                text(&g.title).size(24).width(Length::Fill),
                button(text("Close"))
                    .on_press(Message::CloseDetail)
                    .style(button::text)
            ]
            .align_y(Alignment::Center),
            text(format!(
                "Platforms: {}",
                if g.os.is_empty() {
                    "unknown".into()
                } else {
                    g.os.join(", ")
                }
            ))
            .size(14),
        ]
        .spacing(12);

        let Some(install) = self.installs.get(&g.id) else {
            col = col.push(self.install_section(g));
            col = col.push(self.achievements_section(g));
            return scrollable(col.padding(8)).height(Length::Fill).into();
        };
        col = col.push(text(format!("Folder: {}", install.path.display())).size(13));
        col = col.push(text(runner_label(install)).size(13));

        let playing = self.play.as_ref().filter(|p| p.running);
        let this_running = playing.is_some_and(|p| p.game_id == g.id);
        col = col.push(
            row![
                button(text(if this_running { "Running…" } else { "Play" }))
                    .on_press_maybe(playing.is_none().then(|| Message::Play(g.id.clone())))
                    .style(button::success),
                button(text("Stop game"))
                    .on_press_maybe(this_running.then_some(Message::StopGame))
                    .style(button::danger),
            ]
            .spacing(8),
        );
        if let Some(p) = self.play.as_ref().filter(|p| p.game_id == g.id) {
            col = col.push(section(
                "Session",
                p.log.iter().map(|l| text(l).size(13).into()).collect(),
            ));
        }

        let cloud = self.cloud.get(&g.id);
        let cloud_busy = cloud.is_some_and(|c| c.busy);
        let mut cloud_items: Vec<Element<'_, Message>> = vec![
            row![
                button(text("Check"))
                    .on_press_maybe(
                        (!cloud_busy).then(|| Message::Cloud(g.id.clone(), CloudRequest::Check))
                    )
                    .style(button::secondary),
                button(text("Sync"))
                    .on_press_maybe(
                        (!cloud_busy).then(|| Message::Cloud(g.id.clone(), CloudRequest::Sync))
                    )
                    .style(button::secondary),
            ]
            .spacing(8)
            .into(),
        ];
        if let Some(c) = cloud {
            cloud_items.extend(c.lines.iter().map(|l| text(l).size(13).into()));
            if c.conflicts && !c.busy {
                cloud_items.push(
                    text(
                        "Both versions changed. Choose the one to keep; \
                         the other one is kept in the backups folder.",
                    )
                    .size(13)
                    .into(),
                );
                cloud_items.push(
                    row![
                        button(text("Keep the local version")).on_press(Message::Cloud(
                            g.id.clone(),
                            CloudRequest::Keep(Prefer::Local)
                        )),
                        button(text("Keep the cloud version")).on_press(Message::Cloud(
                            g.id.clone(),
                            CloudRequest::Keep(Prefer::Remote)
                        )),
                    ]
                    .spacing(8)
                    .into(),
                );
            }
        }
        col = col.push(section("Cloud saves", cloud_items));
        col = col.push(self.achievements_section(g));
        col = col.push(self.maintenance_section(g));
        scrollable(col.padding(8)).height(Length::Fill).into()
    }

    fn maintenance_section<'a>(&'a self, g: &'a LibraryGame) -> Element<'a, Message> {
        let view = self.maintenance.get(&g.id);
        let busy = view.is_some_and(|v| v.busy)
            || self
                .play
                .as_ref()
                .is_some_and(|p| p.running && p.game_id == g.id);
        let action = |label, msg: MaintenanceMsg| {
            button(text(label))
                .on_press_maybe((!busy).then(|| Message::Maintenance(msg)))
                .style(button::secondary)
        };
        let mut items: Vec<Element<'_, Message>> = vec![
            row![
                action("Verify files", MaintenanceMsg::Check(g.id.clone(), false)),
                action("Repair", MaintenanceMsg::Check(g.id.clone(), true)),
                action(
                    "Check for update",
                    MaintenanceMsg::CheckUpdate(g.id.clone())
                ),
                action("Language & DLC…", MaintenanceMsg::LoadContent(g.id.clone())),
                action("Uninstall…", MaintenanceMsg::AskUninstall(g.id.clone())),
            ]
            .spacing(8)
            .wrap()
            .into(),
        ];
        if view.is_some_and(|v| v.update_available) {
            items.push(
                button(text("Update now"))
                    .on_press_maybe((!busy).then(|| {
                        Message::Maintenance(MaintenanceMsg::Apply(g.id.clone(), Change::Update))
                    }))
                    .style(button::success)
                    .into(),
            );
        }
        if let Some(v) = view {
            items.extend(v.lines.iter().map(|l| text(l).size(13).into()));
            if let Some(c) = &v.content {
                items.push(self.content_panel(&g.id, c, busy));
            }
            if v.confirm_uninstall && !v.busy {
                items.push(
                    container(
                        column![
                            text(
                                "Only files installed by slatty are deleted; anything else in the folder is kept. \
                                 The Wine prefix holds most local saves.",
                            )
                            .size(13),
                            column![
                                button(text("Uninstall, keep the prefix"))
                                    .on_press(Message::Maintenance(MaintenanceMsg::Uninstall(g.id.clone(), false)))
                                    .style(button::danger),
                                button(text("Also delete the prefix (backed up)"))
                                    .on_press(Message::Maintenance(MaintenanceMsg::Uninstall(g.id.clone(), true)))
                                    .style(button::danger),
                                button(text("Cancel"))
                                    .on_press(Message::Maintenance(MaintenanceMsg::CancelUninstall(g.id.clone())))
                                    .style(button::secondary),
                            ]
                            .spacing(8),
                        ]
                        .spacing(6),
                    )
                    .padding(8)
                    .width(Length::Fill)
                    .style(container::bordered_box)
                    .into(),
                );
            }
        }
        section("Maintenance", items)
    }

    fn content_panel<'a>(
        &'a self,
        game_id: &'a str,
        c: &'a ContentInfo,
        busy: bool,
    ) -> Element<'a, Message> {
        let mut col = Column::new().spacing(6);
        if c.languages.len() > 1 {
            let id = game_id.to_string();
            col = col.push(
                row![
                    text("Language").size(13),
                    pick_list(
                        c.languages.clone(),
                        Some(c.chosen_language.clone()),
                        move |l| {
                            Message::Maintenance(MaintenanceMsg::ChooseLanguage(id.clone(), l))
                        }
                    ),
                    button(text("Switch language"))
                        .on_press_maybe(
                            (!busy && !c.chosen_language.eq_ignore_ascii_case(&c.language)).then(
                                || {
                                    Message::Maintenance(MaintenanceMsg::Apply(
                                        game_id.to_string(),
                                        Change::Language(c.chosen_language.clone()),
                                    ))
                                }
                            )
                        )
                        .style(button::secondary),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            );
        }
        if c.dlcs.is_empty() {
            col = col.push(text("This game has no DLC.").size(13));
        } else {
            for d in &c.dlcs {
                let (id, dlc) = (game_id.to_string(), d.id.clone());
                col = col.push(dlc_row(d, c.chosen_dlcs.contains(&d.id), true, move |_| {
                    Message::Maintenance(MaintenanceMsg::ToggleContentDlc(id.clone(), dlc.clone()))
                }));
            }
            col = col.push(
                button(text("Apply DLC changes"))
                    .on_press_maybe((!busy && c.dlcs_changed()).then(|| {
                        Message::Maintenance(MaintenanceMsg::Apply(
                            game_id.to_string(),
                            Change::Dlcs(c.chosen_dlcs.clone()),
                        ))
                    }))
                    .style(button::secondary),
            );
        }
        container(col)
            .padding(8)
            .width(Length::Fill)
            .style(container::bordered_box)
            .into()
    }
}

/// One DLC line: owned DLC can be ticked, others are shown as not owned.
fn dlc_row<'a>(
    d: &'a DlcChoice,
    checked: bool,
    editable: bool,
    on_toggle: impl Fn(bool) -> Message + 'a,
) -> Element<'a, Message> {
    let label = format!("{} ({})", d.name, human_size(d.disk_size));
    if !d.owned {
        return text(format!("{label} — not owned")).size(13).into();
    }
    checkbox(checked)
        .label(label)
        .text_size(13)
        .on_toggle_maybe(editable.then_some(on_toggle))
        .into()
}

impl App {
    fn achievements_section<'a>(&'a self, g: &'a LibraryGame) -> Element<'a, Message> {
        let mut items: Vec<Element<'_, Message>> = Vec::new();
        match self.achievements.get(&g.id) {
            None => items.push(
                button(text("Show achievements"))
                    .on_press(Message::LoadAchievements(g.id.clone()))
                    .style(button::secondary)
                    .into(),
            ),
            Some(Loadable::Loading) => items.push(text("Loading…").into()),
            Some(Loadable::Failed(e)) => {
                items.push(text(format!("Indisponible : {e}")).size(13).into());
                items.push(
                    button(text("Retry"))
                        .on_press(Message::LoadAchievements(g.id.clone()))
                        .into(),
                );
            }
            Some(Loadable::Ready(list)) => {
                let unlocked = list.iter().filter(|a| a.date_unlocked.is_some()).count();
                let locked: Vec<AchievementChange> = list
                    .iter()
                    .filter(|a| a.date_unlocked.is_none())
                    .map(|a| change(a, true))
                    .collect();
                items.push(
                    row![
                        text(format!("{unlocked} / {} unlocked (GOG data)", list.len()))
                            .size(14)
                            .width(Length::Fill),
                        button(text("Unlock all"))
                            .on_press_maybe((!locked.is_empty()).then(|| {
                                Message::AskAchievementChange(g.id.clone(), locked.clone())
                            }))
                            .style(button::secondary),
                    ]
                    .align_y(Alignment::Center)
                    .into(),
                );
                if let Some(p) = self.pending_change.as_ref().filter(|p| p.game_id == g.id) {
                    items.push(self.confirmation(p));
                }
                for a in list {
                    let done = a.date_unlocked.is_some();
                    let name = if a.visible || done {
                        a.name.as_str()
                    } else {
                        "Hidden achievement"
                    };
                    let action = button(text(if done { "Clear" } else { "Unlock" }).size(12))
                        .on_press(Message::AskAchievementChange(
                            g.id.clone(),
                            vec![change(a, !done)],
                        ))
                        .style(button::text);
                    items.push(
                        row![
                            text(format!("{} {name}", if done { "✔" } else { "·" }))
                                .size(13)
                                .width(Length::Fill),
                            action
                        ]
                        .align_y(Alignment::Center)
                        .into(),
                    );
                }
            }
        }
        section("Achievements", items)
    }

    fn confirmation<'a>(&'a self, p: &'a PendingChange) -> Element<'a, Message> {
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
                .size(13),
                text(
                    "The change is written directly to your public GOG profile, \
                     dated today. It is probably against GOG's terms.",
                )
                .size(12),
                row![
                    button(text("Confirm"))
                        .on_press(Message::ConfirmAchievementChange)
                        .style(button::danger),
                    button(text("Cancel"))
                        .on_press(Message::CancelAchievementChange)
                        .style(button::secondary),
                ]
                .spacing(8),
            ]
            .spacing(6),
        )
        .padding(8)
        .width(Length::Fill)
        .style(container::bordered_box)
        .into()
    }
}

fn change(a: &Achievement, unlock: bool) -> AchievementChange {
    AchievementChange {
        achievement_id: a.achievement_id.clone(),
        name: a.name.clone(),
        unlock,
    }
}

fn section<'a>(title: &'a str, items: Vec<Element<'a, Message>>) -> Element<'a, Message> {
    container(
        column![
            text(title).size(16),
            Column::with_children(items).spacing(4)
        ]
        .spacing(8),
    )
    .padding(10)
    .width(Length::Fill)
    .style(container::rounded_box)
    .into()
}

fn runner_label(install: &Install) -> String {
    match &install.runner {
        Runner::Native => "Native".into(),
        Runner::Umu { proton, .. } => format!(
            "Proton (umu): {}",
            proton
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
        ),
        Runner::Wine { wine, .. } => format!("Wine: {}", wine.display()),
    }
}

fn local_time(ts: i64) -> String {
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
