//! Installing a game: the drawer of the game page, and the dialog the library opens over its grid.
//! Both show what will be downloaded, for which platform, where, in which language, with which
//! Proton.

use crate::theme::text;
use iced::widget::{
    Column, Space, button, column, container, pick_list, row, scrollable, space, text_input,
};
use iced::{Alignment, Element, Length, Padding};
use slatty_core::install::Platform;
use slatty_core::library::LibraryGame;

use super::format::*;
use super::game::download_controls;
use super::note;
use super::panels::dlc_row;
use crate::downloads::DownloadsMsg;
use crate::icons::{Icon, icon};
use crate::install::{InstallMsg, InstallView, PlanInfo};
use crate::settings::{PlatformChoice, ProtonChoice};
use crate::theme::{self, semibold, tokens};
use crate::{App, Message, Page};

/// The parts of the install choices, laid out differently by the drawer and the dialog.
struct Choices<'a> {
    short_of_space: Option<Element<'a, Message>>,
    platform: Element<'a, Message>,
    folder: Element<'a, Message>,
    language: Element<'a, Message>,
    dlcs: Option<Element<'a, Message>>,
    /// For a Windows build only: GOG offers a single version of a Linux build.
    proton: Option<Element<'a, Message>>,
    version: Option<Element<'a, Message>>,
    redistributables: Option<Element<'a, Message>>,
}

impl App {
    pub(super) fn install_drawer<'a>(
        &'a self,
        page: Element<'a, Message>,
        g: &'a LibraryGame,
    ) -> Element<'a, Message> {
        let (body, footer): (Element<'a, Message>, Option<Element<'a, Message>>) =
            match self.install_views.get(&g.id) {
                Some(InstallView::Ready(info)) => (
                    self.drawer_choices(g, info),
                    Some(self.install_actions(g, info, true)),
                ),
                other => (self.install_state(g, other), None),
            };
        let subtitle = text(&g.title).size(18).color(tokens().muted);
        self.drawer(page, "Install", subtitle.into(), body, footer)
    }

    /// The install of a game started from the library: a dialog over the grid, so the library
    /// stays where it was.
    pub(super) fn install_dialog<'a>(
        &'a self,
        page: Element<'a, Message>,
        g: &'a LibraryGame,
    ) -> Element<'a, Message> {
        let info = match self.install_views.get(&g.id) {
            Some(InstallView::Ready(info)) => Some(info),
            _ => None,
        };
        let details: Vec<Element<'a, Message>> = info
            .map(|info| {
                vec![
                    Space::new().height(6).into(),
                    text(format!("Version {}", info.version))
                        .size(15)
                        .font(semibold())
                        .into(),
                ]
            })
            .unwrap_or_default();
        let body: Element<'a, Message> = match info {
            Some(info) => {
                let c = self.choices(g, info);
                let language = section("Language", c.language).width(Length::Fill);
                let mut body = column![
                    sizes(info, true),
                    section("Platform", c.platform),
                    section("Install in", c.folder),
                    match c.proton {
                        Some(proton) =>
                            row![language, section("Proton", proton).width(Length::Fill)]
                                .spacing(20)
                                .into(),
                        None => Element::from(language),
                    },
                ]
                .push(c.version.map(|v| section("Game version", v)))
                .spacing(22);
                if let Some(warning) = c.short_of_space {
                    body = body.push(warning);
                }
                if let Some(dlcs) = c.dlcs {
                    body = body.push(section("DLC", dlcs));
                }
                body.push(c.redistributables).into()
            }
            None => self.install_state(g, self.install_views.get(&g.id)),
        };
        let actions = info.map(|info| self.install_actions(g, info, false));
        self.library_dialog(page, g, "Install", details, body, actions)
    }

    /// Before the choices are known, or once the download runs.
    fn install_state<'a>(
        &'a self,
        g: &'a LibraryGame,
        view: Option<&'a InstallView>,
    ) -> Element<'a, Message> {
        match view {
            Some(InstallView::Failed(e)) => column![
                note(format!("Failed: {e}")),
                button(text("Retry").size(14))
                    .padding([10, 18])
                    .on_press(Message::Install(InstallMsg::Prepare(g.id.clone(), None)))
                    .style(theme::tonal),
            ]
            .spacing(12)
            .into(),
            Some(InstallView::Running {
                progress,
                cancelling,
                rate,
                ..
            }) => download_controls(g, *progress, *cancelling, rate.per_second(), true),
            Some(InstallView::Queued(_)) => {
                let place = self
                    .queue
                    .iter()
                    .position(|id| *id == g.id)
                    .map_or(0, |i| i + 1);
                column![
                    note(format!(
                        "Queued, {place} in line: it starts once the downloads before it are done."
                    )),
                    row![
                        button(text("Show downloads").size(14))
                            .padding([10, 16])
                            .on_press(Message::ShowPage(Page::Downloads))
                            .style(theme::tonal),
                        button(text("Remove from queue").size(14))
                            .padding([10, 16])
                            .on_press(Message::Downloads(DownloadsMsg::Remove(g.id.clone())))
                            .style(theme::danger),
                    ]
                    .spacing(10),
                ]
                .spacing(12)
                .into()
            }
            _ => note("Reading build information from GOG…"),
        }
    }

    /// The choices one under another, as the drawer shows them.
    fn drawer_choices<'a>(
        &'a self,
        g: &'a LibraryGame,
        info: &'a PlanInfo,
    ) -> Element<'a, Message> {
        let c = self.choices(g, info);
        let gap = || Space::new().height(12);
        let mut items = Column::new()
            .spacing(10)
            .push(
                text(format!("Version {}", info.version))
                    .size(16)
                    .font(semibold()),
            )
            .push(Space::new().height(4))
            .push(sizes(info, false))
            .push(c.short_of_space)
            .push(gap())
            .push(section("Platform", c.platform))
            .push(gap())
            .push(section("Install in", c.folder))
            .push(gap())
            .push(section("Language", c.language));
        if let Some(dlcs) = c.dlcs {
            items = items.push(gap()).push(section("DLC", dlcs));
        }
        if let Some(proton) = c.proton {
            items = items.push(gap()).push(section("Proton", proton));
        }
        if let Some(version) = c.version {
            items = items.push(gap()).push(section("Game version", version));
        }
        items = items.push(c.redistributables);
        scrollable(container(items).padding(Padding::ZERO.right(14)))
            .style(theme::scroller)
            .height(Length::Fill)
            .into()
    }

    fn choices<'a>(&'a self, g: &'a LibraryGame, info: &'a PlanInfo) -> Choices<'a> {
        let short_of_space = info.free.filter(|f| *f < info.total_disk()).map(|free| {
            text(format!(
                "Not enough space on this drive: {} free, {} needed.",
                human_size(free),
                human_size(info.total_disk())
            ))
            .size(14)
            .color(tokens().danger)
            .into()
        });

        let folder = if info.resumable {
            note(format!(
                "{} (an interrupted install resumes where it started)",
                info.folder().display()
            ))
        } else {
            let id = g.id.clone();
            let field = row![
                text_input("/home/…/Games/GOG", &info.root)
                    .on_input(move |v| Message::Install(InstallMsg::RootInput(id.clone(), v)))
                    .style(theme::field)
                    .font(theme::font())
                    .padding([10, 14]),
                button(
                    row![
                        icon(Icon::FolderOpen, 16.0, tokens().text),
                        text("Browse").size(14)
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center)
                )
                .padding([10, 16])
                .on_press(Message::Install(InstallMsg::Browse(g.id.clone())))
                .style(theme::tonal),
            ]
            .spacing(10)
            .align_y(Alignment::Center);
            column![
                field,
                note(format!("Game folder: {}", info.folder().display()))
            ]
            .spacing(10)
            .into()
        };

        let language = if info.resumable {
            note(language_name(&info.language))
        } else if info.language == "*" {
            note("Language: one download holds every language; choose it in the game.")
        } else if info.languages.len() <= 1 {
            note(format!(
                "Language: {} (the only one GOG offers)",
                language_name(&info.language)
            ))
        } else {
            let id = g.id.clone();
            pick_list(
                info.languages
                    .iter()
                    .cloned()
                    .map(Language)
                    .collect::<Vec<_>>(),
                Some(Language(info.language.clone())),
                move |l| Message::Install(InstallMsg::Prepare(id.clone(), Some(l.0))),
            )
            .style(theme::select)
            .font(theme::font())
            .padding([10, 14])
            .width(Length::Fill)
            .into()
        };

        // Only the DLC the account owns: the others cannot be installed.
        let dlcs = info.dlcs.iter().any(|d| d.owned).then(|| {
            Column::with_children(info.dlcs.iter().filter(|d| d.owned).map(|d| {
                dlc_row(d, d.selected, !info.resumable, {
                    let (id, dlc) = (g.id.clone(), d.id.clone());
                    move |_| Message::Install(InstallMsg::ToggleDlc(id.clone(), dlc.clone()))
                })
            }))
            .spacing(10)
            .into()
        });

        let chosen = PlatformChoice(info.platform);
        let choice: Element<'a, Message> = if info.resumable {
            note(format!(
                "{chosen} (an interrupted install keeps its platform)"
            ))
        } else if info.platforms.len() <= 1 {
            note(format!("{chosen} (the only build GOG offers)"))
        } else {
            let id = g.id.clone();
            pick_list(
                info.platforms
                    .iter()
                    .map(|p| PlatformChoice(*p))
                    .collect::<Vec<_>>(),
                Some(chosen),
                move |c| Message::Install(InstallMsg::Platform(id.clone(), c.0)),
            )
            .style(theme::select)
            .font(theme::font())
            .padding([10, 14])
            .width(Length::Fill)
            .into()
        };
        let platform = match info.platform {
            Platform::Windows => choice,
            Platform::Linux => column![
                choice,
                note("GOG keeps no cloud saves for Linux builds, and they do not report achievements.")
            ]
            .spacing(10)
            .into(),
        };

        let windows = info.platform == Platform::Windows;
        let id = g.id.clone();
        let proton = windows.then(|| {
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
            .font(theme::font())
            .padding([10, 14])
            .width(Length::Fill)
            .into()
        });

        // The newest version unless another is chosen; an interrupted install keeps its own.
        let current = info
            .versions
            .iter()
            .find(|v| v.build_id == info.build_id)
            .cloned();
        let version = windows.then(|| {
            if info.resumable || info.versions.len() <= 1 {
                note(current.map_or_else(|| info.version.clone(), |v| v.label))
            } else {
                let id = g.id.clone();
                pick_list(info.versions.clone(), current, move |v| {
                    Message::Install(InstallMsg::Version(id.clone(), v.build_id))
                })
                .style(theme::select)
                .font(theme::font())
                .padding([10, 14])
                .width(Length::Fill)
                .into()
            }
        });

        let redistributables = (!info.dependencies.is_empty()).then(|| {
            note(format!(
                "Redistributables set up at first launch: {}",
                info.dependencies.join(", ")
            ))
        });

        Choices {
            short_of_space,
            platform,
            folder,
            language,
            dlcs,
            proton,
            version,
            redistributables,
        }
    }

    /// Start (or Resume) and Discard: across the drawer, or to the right of the dialog.
    fn install_actions<'a>(
        &'a self,
        g: &'a LibraryGame,
        info: &'a PlanInfo,
        wide: bool,
    ) -> Element<'a, Message> {
        let busy = self.installing().is_some();
        let width = if wide { Length::Fill } else { Length::Shrink };
        let start = button(
            container(
                text(if info.resumable {
                    "Resume install"
                } else {
                    "Start install"
                })
                .size(17)
                .font(semibold()),
            )
            .center_x(width),
        )
        .padding(if wide { [16, 0] } else { [14, 56] })
        .width(width)
        .on_press_maybe(
            (!busy && (info.platform == Platform::Linux || info.proton.is_some()))
                .then(|| Message::Install(InstallMsg::Start(g.id.clone()))),
        )
        .style(theme::primary);
        let discard = info.resumable.then(|| {
            button(container(text("Discard download").size(14)).center_x(width))
                .padding(if wide { [12, 0] } else { [14, 22] })
                .width(width)
                .on_press_maybe(
                    (!busy).then(|| Message::Install(InstallMsg::Discard(g.id.clone()))),
                )
                .style(theme::danger)
        });
        // Say why Start is unavailable.
        let why = if let Some((_, title, _)) = self.installing() {
            Some(note(format!(
                "{title} is downloading; this one can start once it is done."
            )))
        } else if info.platform == Platform::Windows && info.proton.is_none() {
            Some(note("Choose a Proton build to start."))
        } else {
            None
        };
        if wide {
            column![]
                .push(why)
                .push(start)
                .push(discard)
                .spacing(10)
                .into()
        } else {
            column![]
                .push(why)
                .push(
                    row![space().width(Length::Fill)]
                        .push(discard)
                        .push(start)
                        .spacing(10),
                )
                .spacing(10)
                .into()
        }
    }
}

/// A label above its control.
fn section<'a>(title: &'a str, content: Element<'a, Message>) -> Column<'a, Message> {
    column![text(title).size(15).color(tokens().muted), content].spacing(10)
}

/// Download, size on disk and free space, side by side; centred in the dialog.
fn sizes<'a>(info: &PlanInfo, centred: bool) -> Element<'a, Message> {
    let short = info.free.is_some_and(|f| f < info.total_disk());
    let align = if centred {
        Alignment::Center
    } else {
        Alignment::Start
    };
    let stat = |ic, label: &'a str, value: String, alert: bool| {
        column![
            icon(ic, 24.0, tokens().text),
            Space::new().height(6),
            text(label).size(14).color(tokens().muted),
            text(value).size(15).font(semibold()).color(if alert {
                tokens().danger
            } else {
                tokens().text
            }),
        ]
        .spacing(4)
        .align_x(align)
        .width(Length::Fill)
    };
    let rule = || {
        container(Space::new())
            .width(1)
            .height(64)
            .style(theme::divider)
    };
    row![
        stat(
            Icon::Download,
            "Download",
            human_size(info.total_download()),
            false
        ),
        rule(),
        stat(
            Icon::Package,
            "On disk",
            human_size(info.total_disk()),
            false
        ),
        rule(),
        stat(
            Icon::HardDrive,
            "Free space",
            info.free.map_or("—".into(), human_size),
            short
        ),
    ]
    .spacing(20)
    .align_y(Alignment::Center)
    .into()
}
