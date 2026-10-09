//! Game page and the tool panels opened over it.

use iced::widget::{
    Column, Space, button, center, checkbox, column, container, image, mouse_area, opaque,
    pick_list, progress_bar, row, scrollable, space, stack, text, text_input,
};
use iced::{Alignment, ContentFit, Element, Length};
use slatty_core::achievements::Achievement;
use slatty_core::cloud::sync::Prefer;
use slatty_core::install::Install;
use slatty_core::installer::DlcChoice;
use slatty_core::library::LibraryGame;
use slatty_core::maintenance::Change;
use slatty_core::runner::Runner;

use crate::icons::{Icon, icon};
use crate::installs::{ContentInfo, InstallMsg, InstallView, MaintenanceMsg, human_size};
use crate::theme::{self, ACCENT, BOLD, MUTED, ON_ACCENT, SEMIBOLD, TEXT};
use crate::view::{duration, fraction, latest_unlocked, logo, relative_day};
use crate::{
    AchievementChange, App, CloudRequest, CloudStatus, Loadable, Message, Panel, PendingChange,
};

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

    fn hero<'a>(&'a self, g: &'a LibraryGame) -> Element<'a, Message> {
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

    fn play_row<'a>(&'a self, g: &'a LibraryGame) -> Element<'a, Message> {
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

    fn session_line<'a>(&'a self, g: &'a LibraryGame) -> Element<'a, Message> {
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

    fn stats<'a>(&'a self, g: &'a LibraryGame) -> Element<'a, Message> {
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

    fn cloud_card<'a>(&'a self, g: &'a LibraryGame) -> Element<'a, Message> {
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

    fn achievements_card<'a>(&'a self, g: &'a LibraryGame) -> Element<'a, Message> {
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

    fn footer<'a>(&'a self, g: &'a LibraryGame) -> Element<'a, Message> {
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

    pub fn with_panel<'a>(
        &'a self,
        page: Element<'a, Message>,
        panel: Panel,
        g: &'a LibraryGame,
    ) -> Element<'a, Message> {
        let (title, content) = match panel {
            Panel::Install => ("Install", self.install_panel(g)),
            Panel::GameSettings => ("Game settings", self.game_settings_panel(g)),
            Panel::Manage => ("Manage", self.manage_panel(g)),
            Panel::Cloud => ("Cloud saves", self.cloud_panel(g)),
            Panel::Achievements => ("Achievements", self.achievements_panel(g)),
            Panel::Session => ("Session", self.session_panel(g)),
        };
        let boxed = container(
            column![
                row![
                    column![
                        text(title).size(24).font(BOLD),
                        text(&g.title).size(14).color(MUTED)
                    ]
                    .spacing(2)
                    .width(Length::Fill),
                    round_button(Icon::X, Message::ClosePanel),
                ]
                .align_y(Alignment::Center),
                scrollable(content).style(theme::scroller).spacing(8),
            ]
            .spacing(18),
        )
        .padding(26)
        .max_width(720)
        .max_height(760)
        .style(theme::card);
        stack![
            page,
            mouse_area(
                container(Space::new())
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .style(theme::backdrop)
            )
            .on_press(Message::ClosePanel),
            center(opaque(boxed)),
        ]
        .into()
    }

    fn install_panel<'a>(&'a self, g: &'a LibraryGame) -> Element<'a, Message> {
        let prepare = |label| {
            button(text(label).size(14))
                .padding([10, 18])
                .on_press(Message::Install(InstallMsg::Prepare(g.id.clone(), None)))
                .style(theme::tonal)
        };
        let mut items: Vec<Element<'_, Message>> = Vec::new();
        match self.install_views.get(&g.id) {
            None | Some(InstallView::Planning) => {
                items.push(note("Reading build information from GOG…"));
            }
            Some(InstallView::Failed(e)) => {
                items.push(note(format!("Failed: {e}")));
                items.push(prepare("Retry").into());
            }
            Some(InstallView::Running { progress, .. }) => {
                items.push(
                    progress_bar(0.0..=1.0, fraction(*progress))
                        .girth(10)
                        .style(theme::progress)
                        .into(),
                );
                items.push(note(format!(
                    "{} / {} · files {}/{}",
                    human_size(progress.bytes_done),
                    human_size(progress.bytes_total),
                    progress.files_done,
                    progress.files_total
                )));
                items.push(
                    button(text("Pause").size(14))
                        .padding([10, 18])
                        .on_press(Message::Install(InstallMsg::Pause(g.id.clone())))
                        .style(theme::tonal)
                        .into(),
                );
            }
            Some(InstallView::Ready(info)) => {
                items.push(
                    text(format!("Version {}", info.version))
                        .size(15)
                        .font(SEMIBOLD)
                        .into(),
                );
                items.push(note(format!(
                    "Download {} · on disk {}",
                    human_size(info.total_download()),
                    human_size(info.total_disk())
                )));
                if info.resumable {
                    items.push(note(format!("Folder: {}", info.folder().display())));
                } else {
                    let id = g.id.clone();
                    items.push(
                        row![
                            text("Install in").size(14).color(MUTED),
                            text_input("/home/…/Games/GOG", &info.root)
                                .on_input(move |v| {
                                    Message::Install(InstallMsg::RootInput(id.clone(), v))
                                })
                                .style(theme::field)
                                .padding([8, 12]),
                            button(
                                row![icon(Icon::FolderOpen, 16.0, TEXT), text("Browse").size(14)]
                                    .spacing(8)
                                    .align_y(Alignment::Center)
                            )
                            .padding([8, 16])
                            .on_press(Message::Install(InstallMsg::Browse(g.id.clone())))
                            .style(theme::tonal),
                        ]
                        .spacing(10)
                        .align_y(Alignment::Center)
                        .into(),
                    );
                    items.push(note(format!("Game folder: {}", info.folder().display())));
                }
                if info.resumable {
                    items.push(note(format!(
                        "Resuming an interrupted install ({}).",
                        info.language
                    )));
                } else if info.languages.len() > 1 {
                    let id = g.id.clone();
                    items.push(
                        row![
                            text("Language").size(14).color(MUTED),
                            pick_list(
                                info.languages.clone(),
                                Some(info.language.clone()),
                                move |l| {
                                    Message::Install(InstallMsg::Prepare(id.clone(), Some(l)))
                                }
                            )
                            .style(theme::select)
                            .padding([8, 16]),
                        ]
                        .spacing(12)
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
                    items.push(note(format!(
                        "Redistributables set up at first launch: {}",
                        info.dependencies.join(", ")
                    )));
                }
                if self.proton.is_none() {
                    items.push(note("Choose a Proton version in Settings."));
                }
                let busy = self.installing().is_some();
                let mut actions = row![
                    button(
                        text(if info.resumable {
                            "Resume install"
                        } else {
                            "Start install"
                        })
                        .size(15)
                        .font(SEMIBOLD)
                    )
                    .padding([12, 22])
                    .on_press_maybe(
                        (!busy && self.proton.is_some())
                            .then(|| Message::Install(InstallMsg::Start(g.id.clone()))),
                    )
                    .style(theme::primary)
                ]
                .spacing(10);
                if info.resumable {
                    actions = actions.push(
                        button(text("Discard download").size(14))
                            .padding([12, 18])
                            .on_press_maybe(
                                (!busy)
                                    .then(|| Message::Install(InstallMsg::Discard(g.id.clone()))),
                            )
                            .style(theme::danger),
                    );
                }
                items.push(actions.into());
            }
        }
        Column::with_children(items).spacing(12).into()
    }

    fn game_settings_panel<'a>(&'a self, g: &'a LibraryGame) -> Element<'a, Message> {
        let Some(install) = self.installs.get(&g.id) else {
            return note("Not installed.");
        };
        let view = self.maintenance.get(&g.id);
        let busy = self.maintenance_busy(&g.id);
        let mut col = column![
            note(format!("Folder: {}", install.path.display())),
            note(runner_label(install)),
        ]
        .spacing(10);
        match view.and_then(|v| v.content.as_ref()) {
            Some(c) => col = col.push(self.content_panel(&g.id, c, busy)),
            None => {
                col = col
                    .extend(
                        view.into_iter()
                            .flat_map(|v| &v.lines)
                            .map(|l| note(l.as_str())),
                    )
                    .extend(self.maintenance_progress(&g.id));
            }
        }
        col.into()
    }

    /// Progress bar and Pause button of a running verify, repair or update.
    fn maintenance_progress(&self, game_id: &str) -> Option<Element<'_, Message>> {
        let v = self.maintenance.get(game_id)?;
        let p = v.progress?;
        Some(
            column![
                progress_bar(0.0..=1.0, fraction(p))
                    .girth(10)
                    .style(theme::progress),
                row![
                    note(format!(
                        "{} / {} · files {}/{}",
                        human_size(p.bytes_done),
                        human_size(p.bytes_total),
                        p.files_done,
                        p.files_total
                    )),
                    space().width(Length::Fill),
                    button(text("Pause").size(14))
                        .padding([8, 16])
                        .on_press(Message::Maintenance(MaintenanceMsg::Pause(
                            game_id.to_string()
                        )))
                        .style(theme::tonal),
                ]
                .align_y(Alignment::Center),
            ]
            .spacing(8)
            .into(),
        )
    }

    fn maintenance_busy(&self, game_id: &str) -> bool {
        self.maintenance.get(game_id).is_some_and(|v| v.busy)
            || self
                .play
                .as_ref()
                .is_some_and(|p| p.running && p.game_id == game_id)
    }

    fn manage_panel<'a>(&'a self, g: &'a LibraryGame) -> Element<'a, Message> {
        let view = self.maintenance.get(&g.id);
        let busy = self.maintenance_busy(&g.id);
        let action = |label, msg: MaintenanceMsg| {
            button(text(label).size(14))
                .padding([10, 16])
                .on_press_maybe((!busy).then(|| Message::Maintenance(msg)))
                .style(theme::tonal)
        };
        let mut items: Vec<Element<'_, Message>> = vec![
            row![
                action("Verify files", MaintenanceMsg::Check(g.id.clone(), false)),
                action("Repair", MaintenanceMsg::Check(g.id.clone(), true)),
                action(
                    "Check for update",
                    MaintenanceMsg::CheckUpdate(g.id.clone())
                ),
                action("Uninstall…", MaintenanceMsg::AskUninstall(g.id.clone())),
            ]
            .spacing(8)
            .wrap()
            .into(),
        ];
        if view.is_some_and(|v| v.update_available) {
            items.push(
                button(text("Update now").size(15).font(SEMIBOLD))
                    .padding([12, 22])
                    .on_press_maybe((!busy).then(|| {
                        Message::Maintenance(MaintenanceMsg::Apply(g.id.clone(), Change::Update))
                    }))
                    .style(theme::primary)
                    .into(),
            );
        }
        if let Some(v) = view {
            items.extend(v.lines.iter().map(|l| note(l.as_str())));
            items.extend(self.maintenance_progress(&g.id));
            if v.confirm_uninstall && !v.busy {
                items.push(
                    container(
                        column![
                            note(
                                "Only files installed by slatty are deleted; anything else in the \
                                 folder is kept. The Wine prefix holds most local saves."
                            ),
                            row![
                                button(text("Uninstall, keep the prefix").size(14))
                                    .padding([10, 16])
                                    .on_press(Message::Maintenance(MaintenanceMsg::Uninstall(
                                        g.id.clone(),
                                        false
                                    )))
                                    .style(theme::danger),
                                button(text("Also delete the prefix (backed up)").size(14))
                                    .padding([10, 16])
                                    .on_press(Message::Maintenance(MaintenanceMsg::Uninstall(
                                        g.id.clone(),
                                        true
                                    )))
                                    .style(theme::danger),
                                button(text("Cancel").size(14))
                                    .padding([10, 16])
                                    .on_press(Message::Maintenance(
                                        MaintenanceMsg::CancelUninstall(g.id.clone())
                                    ))
                                    .style(theme::tonal),
                            ]
                            .spacing(8)
                            .wrap(),
                        ]
                        .spacing(12),
                    )
                    .padding(16)
                    .width(Length::Fill)
                    .style(inner)
                    .into(),
                );
            }
        }
        Column::with_children(items).spacing(12).into()
    }

    fn content_panel<'a>(
        &'a self,
        game_id: &'a str,
        c: &'a ContentInfo,
        busy: bool,
    ) -> Element<'a, Message> {
        let mut col = Column::new().spacing(10);
        if c.languages.len() > 1 {
            let id = game_id.to_string();
            col = col.push(
                row![
                    text("Language").size(14).color(MUTED),
                    pick_list(
                        c.languages.clone(),
                        Some(c.chosen_language.clone()),
                        move |l| {
                            Message::Maintenance(MaintenanceMsg::ChooseLanguage(id.clone(), l))
                        }
                    )
                    .style(theme::select)
                    .padding([8, 16]),
                    button(text("Switch language").size(14))
                        .padding([10, 16])
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
                        .style(theme::tonal),
                ]
                .spacing(12)
                .align_y(Alignment::Center),
            );
        }
        if c.dlcs.is_empty() {
            col = col.push(note("This game has no DLC."));
        } else {
            col = col.push(text("DLC").size(15).font(SEMIBOLD));
            for d in &c.dlcs {
                let (id, dlc) = (game_id.to_string(), d.id.clone());
                col = col.push(dlc_row(d, c.chosen_dlcs.contains(&d.id), true, move |_| {
                    Message::Maintenance(MaintenanceMsg::ToggleContentDlc(id.clone(), dlc.clone()))
                }));
            }
            col = col.push(
                button(text("Apply DLC changes").size(14))
                    .padding([10, 16])
                    .on_press_maybe((!busy && c.dlcs_changed()).then(|| {
                        Message::Maintenance(MaintenanceMsg::Apply(
                            game_id.to_string(),
                            Change::Dlcs(c.chosen_dlcs.clone()),
                        ))
                    }))
                    .style(theme::tonal),
            );
        }
        col.into()
    }

    fn cloud_panel<'a>(&'a self, g: &'a LibraryGame) -> Element<'a, Message> {
        let cloud = self.cloud.get(&g.id);
        let busy = cloud.is_some_and(|c| c.busy);
        let mut items: Vec<Element<'_, Message>> = vec![
            row![
                button(text("Check").size(14))
                    .padding([10, 16])
                    .on_press_maybe(
                        (!busy).then(|| Message::Cloud(g.id.clone(), CloudRequest::Check))
                    )
                    .style(theme::tonal),
                button(text("Sync now").size(14))
                    .padding([10, 16])
                    .on_press_maybe(
                        (!busy).then(|| Message::Cloud(g.id.clone(), CloudRequest::Sync))
                    )
                    .style(theme::tonal),
            ]
            .spacing(8)
            .into(),
        ];
        if let Some(c) = cloud {
            if c.busy {
                items.push(note("Working…"));
            }
            items.extend(c.lines.iter().map(|l| note(l.as_str())));
            if c.conflicts && !c.busy {
                items.push(note(
                    "Both versions changed. Choose the one to keep; \
                     the other one is kept in the backups folder.",
                ));
                items.push(
                    row![
                        button(text("Keep the local version").size(14))
                            .padding([10, 16])
                            .on_press(Message::Cloud(
                                g.id.clone(),
                                CloudRequest::Keep(Prefer::Local)
                            ))
                            .style(theme::primary),
                        button(text("Keep the cloud version").size(14))
                            .padding([10, 16])
                            .on_press(Message::Cloud(
                                g.id.clone(),
                                CloudRequest::Keep(Prefer::Remote)
                            ))
                            .style(theme::primary),
                    ]
                    .spacing(8)
                    .into(),
                );
            }
        }
        Column::with_children(items).spacing(10).into()
    }

    fn achievements_panel<'a>(&'a self, g: &'a LibraryGame) -> Element<'a, Message> {
        let Some(Loadable::Ready(list)) = self.achievements.get(&g.id) else {
            return note("Loading…");
        };
        let unlocked = list.iter().filter(|a| a.date_unlocked.is_some()).count();
        let mut items: Vec<Element<'_, Message>> = vec![
            row![
                text(format!("{unlocked} / {} unlocked", list.len()))
                    .size(15)
                    .font(SEMIBOLD)
                    .width(Length::Fill),
                unlock_all_button(&g.id, list),
            ]
            .align_y(Alignment::Center)
            .into(),
        ];
        items.extend(self.pending_confirmation(&g.id));
        items.extend(list.iter().map(|a| self.achievement_row(&g.id, a)));
        Column::with_children(items).spacing(12).into()
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

    fn session_panel<'a>(&'a self, g: &'a LibraryGame) -> Element<'a, Message> {
        let Some(p) = self.play.as_ref().filter(|p| p.game_id == g.id) else {
            return note("No session yet.");
        };
        let mut col = Column::with_children(p.log.iter().map(|l| note(l.as_str()))).spacing(6);
        if p.running {
            col = col.push(
                button(text("Stop game").size(14))
                    .padding([10, 16])
                    .on_press(Message::StopGame)
                    .style(theme::danger),
            );
        }
        col.into()
    }
}

pub fn round_button<'a>(ic: Icon, msg: Message) -> Element<'a, Message> {
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

/// One DLC line: owned DLC can be ticked, others are shown as not owned.
fn dlc_row<'a>(
    d: &'a DlcChoice,
    checked: bool,
    editable: bool,
    on_toggle: impl Fn(bool) -> Message + 'a,
) -> Element<'a, Message> {
    let label = format!("{} ({})", d.name, human_size(d.disk_size));
    if !d.owned {
        return note(format!("{label} — not owned"));
    }
    checkbox(checked)
        .label(label)
        .text_size(14)
        .on_toggle_maybe(editable.then_some(on_toggle))
        .into()
}

fn change(a: &Achievement, unlock: bool) -> AchievementChange {
    AchievementChange {
        achievement_id: a.achievement_id.clone(),
        name: a.name.clone(),
        unlock,
    }
}

fn confirmation(p: &PendingChange) -> Element<'_, Message> {
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
