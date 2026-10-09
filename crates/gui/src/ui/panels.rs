//! Tool panels opened over the game page.

use iced::widget::{
    Column, Space, button, center, checkbox, column, container, mouse_area, opaque, pick_list,
    progress_bar, row, scrollable, space, stack, text, text_input,
};
use iced::{Alignment, Element, Length, Padding};
use slatty_core::cloud::sync::Prefer;
use slatty_core::install::Install;
use slatty_core::installer::DlcChoice;
use slatty_core::library::LibraryGame;
use slatty_core::maintenance::Change;
use slatty_core::runner::Runner;

use super::achievements::unlock_all_button;
use super::format::*;
use super::game::download_controls;
use super::{inner, note, round_button};
use crate::achievements::by_rarity;
use crate::icons::{Icon, icon};
use crate::install::{InstallMsg, InstallView};
use crate::maintenance::{ContentInfo, MaintenanceMsg};
use crate::settings::{ProtonChoice, SettingsMsg};
use crate::theme::{self, BOLD, SEMIBOLD, tokens};
use crate::{App, CloudRequest, Loadable, Message, Panel};

/// Width of the achievements drawer beside the game page.
const DRAWER_WIDTH: f32 = 500.0;
/// Narrowest game page the drawer opens beside.
const PAGE_BESIDE_DRAWER: f32 = 760.0;

impl App {
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
            Panel::Achievements => return self.achievements_drawer(page, g),
            Panel::Session => ("Session", self.session_panel(g)),
        };
        let boxed = container(
            column![
                row![
                    column![
                        text(title).size(24).font(BOLD),
                        text(&g.title).size(14).color(tokens().muted)
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

    pub(super) fn install_panel<'a>(&'a self, g: &'a LibraryGame) -> Element<'a, Message> {
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
            Some(InstallView::Running {
                progress,
                cancelling,
                ..
            }) => items.push(download_controls(g, *progress, *cancelling, true)),
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
                            text("Install in").size(14).color(tokens().muted),
                            text_input("/home/…/Games/GOG", &info.root)
                                .on_input(move |v| {
                                    Message::Install(InstallMsg::RootInput(id.clone(), v))
                                })
                                .style(theme::field)
                                .padding([8, 12]),
                            button(
                                row![
                                    icon(Icon::FolderOpen, 16.0, tokens().text),
                                    text("Browse").size(14)
                                ]
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
                        language_name(&info.language)
                    )));
                } else if info.languages.len() <= 1 {
                    items.push(note(format!(
                        "Language: {} (the only one GOG offers)",
                        language_name(&info.language)
                    )));
                } else {
                    let id = g.id.clone();
                    items.push(
                        row![
                            text("Language").size(14).color(tokens().muted),
                            pick_list(
                                info.languages
                                    .iter()
                                    .cloned()
                                    .map(Language)
                                    .collect::<Vec<_>>(),
                                Some(Language(info.language.clone())),
                                move |l| {
                                    Message::Install(InstallMsg::Prepare(id.clone(), Some(l.0)))
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
                let id = g.id.clone();
                items.push(
                    row![
                        text("Proton").size(14).color(tokens().muted),
                        pick_list(
                            self.proton_choices
                                .iter()
                                .cloned()
                                .map(ProtonChoice)
                                .collect::<Vec<_>>(),
                            info.proton.clone().map(ProtonChoice),
                            move |c| Message::Install(InstallMsg::Proton(id.clone(), c)),
                        )
                        .placeholder("No Proton build found (Steam or compatibilitytools.d)")
                        .style(theme::select)
                        .padding([8, 16]),
                    ]
                    .spacing(12)
                    .align_y(Alignment::Center)
                    .into(),
                );
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
                        (!busy && info.proton.is_some())
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

    pub(super) fn game_settings_panel<'a>(&'a self, g: &'a LibraryGame) -> Element<'a, Message> {
        let Some(install) = self.installs.get(&g.id) else {
            return note("Not installed.");
        };
        let view = self.maintenance.get(&g.id);
        let busy = self.maintenance_busy(&g.id);
        let runner: Element<'_, Message> = match &install.runner {
            Runner::Umu { proton, .. } => {
                let id = g.id.clone();
                row![
                    text("Proton").size(14).color(tokens().muted),
                    pick_list(
                        self.proton_choices
                            .iter()
                            .cloned()
                            .map(ProtonChoice)
                            .collect::<Vec<_>>(),
                        Some(ProtonChoice(proton.clone())),
                        move |c| Message::Settings(SettingsMsg::GameProton(id.clone(), c)),
                    )
                    .style(theme::select)
                    .padding([8, 16]),
                    note("Used from the next launch."),
                ]
                .spacing(12)
                .align_y(Alignment::Center)
                .into()
            }
            _ => note(runner_label(install)),
        };
        let mut col =
            column![note(format!("Folder: {}", install.path.display())), runner,].spacing(10);
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
    pub(super) fn maintenance_progress(&self, game_id: &str) -> Option<Element<'_, Message>> {
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

    pub(super) fn maintenance_busy(&self, game_id: &str) -> bool {
        self.maintenance.get(game_id).is_some_and(|v| v.busy)
            || self
                .play
                .as_ref()
                .is_some_and(|p| p.running && p.game_id == game_id)
    }

    pub(super) fn manage_panel<'a>(&'a self, g: &'a LibraryGame) -> Element<'a, Message> {
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

    pub(super) fn content_panel<'a>(
        &'a self,
        game_id: &'a str,
        c: &'a ContentInfo,
        busy: bool,
    ) -> Element<'a, Message> {
        let mut col = Column::new().spacing(10);
        if c.languages.len() <= 1 {
            col = col.push(
                row![
                    text("Language").size(14).color(tokens().muted),
                    text(language_name(&c.language)).size(14),
                ]
                .spacing(12),
            );
            col = col.push(note(
                "GOG offers this game in this language only. Games that hold several languages \
                 in one download let you choose in their own options.",
            ));
        } else {
            let id = game_id.to_string();
            col = col.push(
                row![
                    text("Language").size(14).color(tokens().muted),
                    pick_list(
                        c.languages
                            .iter()
                            .cloned()
                            .map(Language)
                            .collect::<Vec<_>>(),
                        Some(Language(c.chosen_language.clone())),
                        move |l| {
                            Message::Maintenance(MaintenanceMsg::ChooseLanguage(id.clone(), l.0))
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

    pub(super) fn cloud_panel<'a>(&'a self, g: &'a LibraryGame) -> Element<'a, Message> {
        let cloud = self.cloud.get(&g.id);
        let busy = cloud.is_some_and(|c| c.busy);
        // Before the first launch Proton has not created the prefix yet: saves can be compared, and
        // are downloaded when the game first starts.
        let launched = self
            .installs
            .get(&g.id)
            .is_some_and(slatty_core::runner::prefix_ready);
        let mut actions = row![
            button(text("Check").size(14))
                .padding([10, 16])
                .on_press_maybe((!busy).then(|| Message::Cloud(g.id.clone(), CloudRequest::Check)))
                .style(theme::tonal),
        ]
        .spacing(8);
        if launched {
            actions = actions.push(
                button(text("Sync now").size(14))
                    .padding([10, 16])
                    .on_press_maybe(
                        (!busy).then(|| Message::Cloud(g.id.clone(), CloudRequest::Sync)),
                    )
                    .style(theme::tonal),
            );
        }
        let mut items: Vec<Element<'_, Message>> = vec![actions.into()];
        if !launched {
            items.push(note(
                "Your cloud saves are downloaded when the game first starts, \
                 before it runs.",
            ));
        }
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

    /// Achievements open in a drawer beside the game page, which stays visible.
    fn achievements_drawer<'a>(
        &'a self,
        page: Element<'a, Message>,
        g: &'a LibraryGame,
    ) -> Element<'a, Message> {
        let rule = || {
            container(Space::new())
                .width(Length::Fill)
                .height(1)
                .style(theme::divider)
        };
        let close = button(container(icon(Icon::X, 20.0, tokens().text)).center(24))
            .padding(10)
            .on_press(Message::ClosePanel)
            .style(theme::tonal);
        let header = row![
            column![
                text("Achievements").size(32).font(BOLD),
                text(&g.title).size(18).color(tokens().accent),
            ]
            .spacing(4)
            .width(Length::Fill),
            close,
        ];
        let body: Element<'_, Message> = match self.achievements.get(&g.id) {
            Some(Loadable::Ready(list)) => {
                let unlocked = list.iter().filter(|a| a.date_unlocked.is_some()).count();
                let mut items = Column::new();
                for (i, a) in by_rarity(list).into_iter().enumerate() {
                    if i > 0 {
                        items = items.push(rule());
                    }
                    items = items
                        .push(container(self.achievement_row(&g.id, a, 56.0)).padding([14, 0]));
                }
                column![
                    row![
                        text(format!("{unlocked} / {} unlocked", list.len()))
                            .size(16)
                            .font(SEMIBOLD)
                            .width(Length::Fill),
                        unlock_all_button(&g.id, list),
                    ]
                    .align_y(Alignment::Center),
                ]
                .extend(self.pending_confirmation(&g.id))
                .push(rule())
                .push(
                    scrollable(container(items).padding(Padding::ZERO.right(14)))
                        .style(theme::scroller)
                        .height(Length::Fill),
                )
                .spacing(14)
                .into()
            }
            _ => note("Loading…"),
        };
        let drawer = container(column![header, body].spacing(22))
            .padding([22, 24])
            .width(DRAWER_WIDTH)
            .height(Length::Fill)
            .style(theme::drawer);
        // Beside the page while it keeps room for its cards, over it in a narrow window.
        if self.window.width - DRAWER_WIDTH < PAGE_BESIDE_DRAWER {
            return stack![
                page,
                mouse_area(
                    container(Space::new())
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .style(theme::backdrop)
                )
                .on_press(Message::ClosePanel),
                container(opaque(drawer)).align_right(Length::Fill),
            ]
            .into();
        }
        row![
            container(page).width(Length::Fill),
            container(Space::new())
                .width(1)
                .height(Length::Fill)
                .style(theme::divider),
            drawer,
        ]
        .into()
    }

    pub(super) fn session_panel<'a>(&'a self, g: &'a LibraryGame) -> Element<'a, Message> {
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

/// One DLC line: owned DLC can be ticked, others are shown as not owned.
pub(super) fn dlc_row<'a>(
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

pub(super) fn runner_label(install: &Install) -> String {
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
