//! The Installers drawer: GOG's offline installers of a game, chosen by system and languages, to
//! download and keep.

use crate::theme::text;
use iced::widget::{Column, button, checkbox, column, container, pick_list, row, scrollable};
use iced::{Alignment, Element, Length, Padding};
use slatty_core::library::LibraryGame;
use slatty_core::offline;

use super::format::*;
use super::game_settings::{stacked, titled};
use super::note;
use crate::icons::Icon;
use crate::offline::{Choice, OfflineMsg, OfflineView};
use crate::theme::{self, semibold, tokens};
use crate::{App, Message};

/// A system as offered, by its name.
#[derive(Debug, Clone, PartialEq, Eq)]
struct System(String);

impl std::fmt::Display for System {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self.0.as_str() {
            "windows" => "Windows",
            "linux" => "Linux",
            "mac" => "macOS",
            other => other,
        })
    }
}

impl App {
    pub(super) fn installers_drawer<'a>(
        &'a self,
        page: Element<'a, Message>,
        g: &'a LibraryGame,
    ) -> Element<'a, Message> {
        let subtitle = text(&g.title).size(18).color(tokens().muted);
        let (body, footer) = match self.offline_views.get(&g.id) {
            Some(OfflineView::Ready(c)) => {
                (self.installers(g, c), Some(self.installers_footer(g, c)))
            }
            Some(OfflineView::Failed(e)) => (
                column![
                    note(e.as_str()),
                    button(text("Try again").size(14))
                        .padding([10, 16])
                        .on_press(Message::Offline(OfflineMsg::Load(g.id.clone())))
                        .style(theme::tonal),
                ]
                .spacing(12)
                .into(),
                None,
            ),
            _ => (note("Asking GOG for its installers…"), None),
        };
        let body = scrollable(container(body).padding(Padding::ZERO.right(14)))
            .style(theme::scroller)
            .height(Length::Fill);
        self.drawer(
            page,
            "Offline installers",
            subtitle.into(),
            body.into(),
            footer,
        )
    }

    fn installers<'a>(&'a self, g: &'a LibraryGame, c: &'a Choice) -> Element<'a, Message> {
        let id = g.id.clone();
        let systems: Vec<System> = c
            .offer
            .systems()
            .into_iter()
            .map(|s| System(s.into()))
            .collect();
        let system = pick_list(systems, Some(System(c.os.clone())), move |s| {
            Message::Offline(OfflineMsg::System(id.clone(), s.0))
        })
        .style(theme::select)
        .font(theme::font())
        .padding([10, 14])
        .width(Length::Fill);
        // GOG often makes one set of installers holding every language, listed in one.
        let offered = c.offer.languages(&c.os);
        let language: Element<'a, Message> = match offered.as_slice() {
            [only] => column![
                text(language_name(only)).size(15),
                note(
                    "GOG offers these installers in this language only. Many hold every \
                     language of the game, chosen in its own options."
                ),
            ]
            .spacing(10)
            .into(),
            _ => Column::with_children(offered.into_iter().map(|l| {
                let (id, language) = (g.id.clone(), l.to_string());
                checkbox(c.languages.iter().any(|c| c == l))
                    .label(language_name(l))
                    .text_size(14)
                    .font(theme::font())
                    .on_toggle(move |_| {
                        Message::Offline(OfflineMsg::ToggleLanguage(id.clone(), language.clone()))
                    })
                    .into()
            }))
            .push(note("A DLC without these languages comes in English."))
            .spacing(10)
            .into(),
        };
        let mut sections = vec![
            titled(
                Icon::Package,
                "System",
                column![
                    system,
                    note("GOG's setup programs, to install the game without GOG or a launcher."),
                ]
                .spacing(10)
                .into(),
                0.0,
            ),
            titled(Icon::Globe, "Languages", language, 0.0),
        ];
        let dlcs: Vec<&offline::Dlc> = c.offer.dlcs_for(&c.os).collect();
        if !dlcs.is_empty() {
            sections.push(titled(
                Icon::Puzzle,
                "DLC",
                Column::with_children(dlcs.into_iter().map(|d| {
                    let (id, dlc) = (g.id.clone(), d.id.clone());
                    checkbox(c.dlcs.contains(&d.id))
                        .label(d.title.as_str())
                        .text_size(14)
                        .font(theme::font())
                        .on_toggle(move |_| {
                            Message::Offline(OfflineMsg::ToggleDlc(id.clone(), dlc.clone()))
                        })
                        .into()
                }))
                .spacing(10)
                .into(),
                0.0,
            ));
        }
        let root = std::path::Path::new(self.installers_root.trim());
        sections.push(titled(
            Icon::Folder,
            "Folder",
            column![
                text(
                    offline::folder_for(root, &c.offer.title)
                        .display()
                        .to_string()
                )
                .size(15),
                note("The installers folder is chosen in Settings → Installs."),
            ]
            .spacing(10)
            .into(),
            0.0,
        ));
        stacked(sections)
    }

    /// The size, and Download (or Add to queue while something downloads).
    fn installers_footer<'a>(&'a self, g: &'a LibraryGame, c: &'a Choice) -> Element<'a, Message> {
        let queued = self.queue.contains(&crate::offline::entry(&g.id))
            || self.offline_run.as_ref().is_some_and(|r| r.game_id == g.id);
        let label = match (queued, self.downloading()) {
            (true, _) => "In the Downloads tab",
            (false, true) => "Add to queue",
            (false, false) => "Download",
        };
        column![
            row![
                text("Download size").size(14).color(tokens().muted),
                text(human_size(c.size())).size(14),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
            button(container(text(label).size(17).font(semibold())).center_x(Length::Fill))
                .padding([16, 0])
                .width(Length::Fill)
                .on_press_maybe(
                    (!queued).then(|| Message::Offline(OfflineMsg::Download(g.id.clone()))),
                )
                .style(theme::primary),
        ]
        .spacing(12)
        .into()
    }
}
