//! The Install drawer: what will be downloaded, where, in which language, with which Proton.

use crate::theme::text;
use iced::widget::{
    Column, Space, button, column, container, pick_list, row, scrollable, text_input,
};
use iced::{Alignment, Element, Length, Padding};
use slatty_core::library::LibraryGame;

use super::format::*;
use super::game::download_controls;
use super::note;
use super::panels::dlc_row;
use crate::icons::{Icon, icon};
use crate::install::{InstallMsg, InstallView, PlanInfo};
use crate::settings::ProtonChoice;
use crate::theme::{self, semibold, tokens};
use crate::{App, Message};

impl App {
    pub(super) fn install_drawer<'a>(
        &'a self,
        page: Element<'a, Message>,
        g: &'a LibraryGame,
    ) -> Element<'a, Message> {
        let (body, footer): (Element<'a, Message>, Option<Element<'a, Message>>) =
            match self.install_views.get(&g.id) {
                None | Some(InstallView::Planning) => {
                    (note("Reading build information from GOG…"), None)
                }
                Some(InstallView::Failed(e)) => (
                    column![
                        note(format!("Failed: {e}")),
                        button(text("Retry").size(14))
                            .padding([10, 18])
                            .on_press(Message::Install(InstallMsg::Prepare(g.id.clone(), None)))
                            .style(theme::tonal),
                    ]
                    .spacing(12)
                    .into(),
                    None,
                ),
                Some(InstallView::Running {
                    progress,
                    cancelling,
                    rate,
                    ..
                }) => (
                    download_controls(g, *progress, *cancelling, rate.per_second(), true),
                    None,
                ),
                Some(InstallView::Ready(info)) => (
                    self.install_choices(g, info),
                    Some(self.install_actions(g, info)),
                ),
            };
        let subtitle = text(&g.title).size(18).color(tokens().muted);
        self.drawer(page, "Install", subtitle.into(), body, footer)
    }

    fn install_choices<'a>(
        &'a self,
        g: &'a LibraryGame,
        info: &'a PlanInfo,
    ) -> Element<'a, Message> {
        let label = |t| text(t).size(15).color(tokens().muted);
        let mut items = Column::new().spacing(10);
        items = items.push(
            text(format!("Version {}", info.version))
                .size(16)
                .font(semibold()),
        );
        items = items.push(Space::new().height(4));
        items = items.push(sizes(info));
        if let Some(free) = info.free.filter(|f| *f < info.total_disk()) {
            items = items.push(
                text(format!(
                    "Not enough space on this drive: {} free, {} needed.",
                    human_size(free),
                    human_size(info.total_disk())
                ))
                .size(14)
                .color(tokens().danger),
            );
        }

        let section =
            |title, content: Element<'a, Message>| column![label(title), content].spacing(10);
        items = items.push(Space::new().height(12));
        if info.resumable {
            items = items.push(section(
                "Install in",
                note(format!(
                    "{} (an interrupted install resumes where it started)",
                    info.folder().display()
                )),
            ));
        } else {
            let id = g.id.clone();
            let folder = row![
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
            items = items.push(section(
                "Install in",
                column![
                    folder,
                    note(format!("Game folder: {}", info.folder().display()))
                ]
                .spacing(10)
                .into(),
            ));
        }

        items = items.push(Space::new().height(12));
        let language: Element<'a, Message> = if info.resumable {
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
        items = items.push(section("Language", language));

        // Only the DLC the account owns: the others cannot be installed.
        if info.dlcs.iter().any(|d| d.owned) {
            items = items.push(Space::new().height(12));
            let dlcs = Column::with_children(info.dlcs.iter().filter(|d| d.owned).map(|d| {
                dlc_row(d, d.selected, !info.resumable, {
                    let (id, dlc) = (g.id.clone(), d.id.clone());
                    move |_| Message::Install(InstallMsg::ToggleDlc(id.clone(), dlc.clone()))
                })
            }))
            .spacing(10);
            items = items.push(section("DLC", dlcs.into()));
        }

        items = items.push(Space::new().height(12));
        let id = g.id.clone();
        items = items.push(section(
            "Proton",
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
            .into(),
        ));

        // The newest version unless another is chosen; an interrupted install keeps its own.
        items = items.push(Space::new().height(12));
        let current = info
            .versions
            .iter()
            .find(|v| v.build_id == info.build_id)
            .cloned();
        let version: Element<'a, Message> = if info.resumable || info.versions.len() <= 1 {
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
        };
        items = items.push(section("Game version", version));
        if !info.dependencies.is_empty() {
            items = items.push(note(format!(
                "Redistributables set up at first launch: {}",
                info.dependencies.join(", ")
            )));
        }
        scrollable(container(items).padding(Padding::ZERO.right(14)))
            .style(theme::scroller)
            .height(Length::Fill)
            .into()
    }

    fn install_actions<'a>(
        &'a self,
        g: &'a LibraryGame,
        info: &'a PlanInfo,
    ) -> Element<'a, Message> {
        let busy = self.installing().is_some();
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
            .center_x(Length::Fill),
        )
        .padding([16, 0])
        .width(Length::Fill)
        .on_press_maybe(
            (!busy && info.proton.is_some())
                .then(|| Message::Install(InstallMsg::Start(g.id.clone()))),
        )
        .style(theme::primary);
        let mut actions = column![].spacing(10);
        // Say why Start is unavailable.
        if let Some((_, title, _)) = self.installing() {
            actions = actions.push(note(format!(
                "{title} is downloading; this one can start once it is done."
            )));
        } else if info.proton.is_none() {
            actions = actions.push(note("Choose a Proton build to start."));
        }
        actions = actions.push(start);
        if info.resumable {
            actions = actions.push(
                button(container(text("Discard download").size(14)).center_x(Length::Fill))
                    .padding([12, 0])
                    .width(Length::Fill)
                    .on_press_maybe(
                        (!busy).then(|| Message::Install(InstallMsg::Discard(g.id.clone()))),
                    )
                    .style(theme::danger),
            );
        }
        actions.into()
    }
}

/// Download, size on disk and free space, side by side.
fn sizes<'a>(info: &PlanInfo) -> Element<'a, Message> {
    let short = info.free.is_some_and(|f| f < info.total_disk());
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
