//! The Game settings drawer: folder, Proton, language and DLC of an installed game.

use crate::theme::text;
use iced::widget::{Column, Space, button, column, container, pick_list, row, scrollable};
use iced::{Alignment, Element, Length, Padding};
use slatty_core::library::LibraryGame;
use slatty_core::maintenance::Change;
use slatty_core::runner::Runner;

use super::format::*;
use super::note;
use super::panels::{dlc_row, runner_label};
use crate::icons::{Icon, icon};
use crate::maintenance::{ContentInfo, MaintenanceMsg};
use crate::settings::{ProtonChoice, SettingsMsg};
use crate::theme::{self, tokens};
use crate::{App, Message};

impl App {
    pub(super) fn game_settings_drawer<'a>(
        &'a self,
        page: Element<'a, Message>,
        g: &'a LibraryGame,
    ) -> Element<'a, Message> {
        let subtitle = text(&g.title).size(18).color(tokens().muted);
        let body = scrollable(container(self.game_settings(g)).padding(Padding::ZERO.right(14)))
            .style(theme::scroller)
            .height(Length::Fill);
        self.drawer(page, "Game settings", subtitle.into(), body.into(), None)
    }

    fn game_settings<'a>(&'a self, g: &'a LibraryGame) -> Element<'a, Message> {
        let Some(install) = self.installs.get(&g.id) else {
            return note("Not installed.");
        };
        let mut sections = vec![section(
            Icon::Folder,
            "Folder",
            text(install.path.display().to_string()).size(15).into(),
        )];
        sections.push(section(
            Icon::SlidersHorizontal,
            "Proton",
            match &install.runner {
                Runner::Umu { proton, .. } => {
                    let id = g.id.clone();
                    column![
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
                        .font(theme::font())
                        .padding([10, 14])
                        .width(Length::Fill),
                        note("Used from the next launch."),
                    ]
                    .spacing(10)
                    .into()
                }
                _ => note(runner_label(install)),
            },
        ));
        let view = self.maintenance.get(&g.id);
        let busy = self.maintenance_busy(&g.id);
        match view.and_then(|v| v.content.as_ref()) {
            Some(c) => {
                sections.push(section(
                    Icon::RefreshCw,
                    "Game version",
                    version(&g.id, c, busy),
                ));
                sections.push(section(Icon::Globe, "Language", language(&g.id, c, busy)));
                sections.push(section(Icon::Puzzle, "DLC", dlcs(&g.id, c, busy)));
            }
            // Reading the languages and DLC, or applying a change to them.
            None => sections.push(
                Column::new()
                    .extend(
                        view.into_iter()
                            .flat_map(|v| &v.lines)
                            .map(|l| note(l.as_str())),
                    )
                    .extend(self.maintenance_progress(&g.id))
                    .spacing(10)
                    .into(),
            ),
        }
        let mut col = Column::new().spacing(22);
        for (i, s) in sections.into_iter().enumerate() {
            if i > 0 {
                col = col.push(
                    container(Space::new())
                        .width(Length::Fill)
                        .height(1)
                        .style(theme::divider),
                );
            }
            col = col.push(s);
        }
        col.into()
    }
}

/// A titled part of the drawer, with its icon.
fn section<'a>(ic: Icon, title: &'a str, content: Element<'a, Message>) -> Element<'a, Message> {
    column![
        row![icon(ic, 22.0, tokens().text), text(title).size(17)]
            .spacing(12)
            .align_y(Alignment::Center),
        content,
    ]
    .spacing(14)
    .into()
}

fn language<'a>(game_id: &'a str, c: &'a ContentInfo, busy: bool) -> Element<'a, Message> {
    if c.languages.len() <= 1 {
        return column![
            text(language_name(&c.language)).size(15),
            note(if c.language == "*" {
                "One download holds every language; choose it in the game's own options."
            } else {
                "GOG offers this game in this language only. Games that hold several languages \
                 in one download let you choose in their own options."
            }),
        ]
        .spacing(10)
        .into();
    }
    let id = game_id.to_string();
    column![
        pick_list(
            c.languages
                .iter()
                .cloned()
                .map(Language)
                .collect::<Vec<_>>(),
            Some(Language(c.chosen_language.clone())),
            move |l| Message::Maintenance(MaintenanceMsg::ChooseLanguage(id.clone(), l.0)),
        )
        .style(theme::select)
        .font(theme::font())
        .padding([10, 14])
        .width(Length::Fill),
        button(text("Switch language").size(14))
            .padding([10, 16])
            .on_press_maybe(
                (!busy && !c.chosen_language.eq_ignore_ascii_case(&c.language)).then(|| {
                    Message::Maintenance(MaintenanceMsg::Apply(
                        game_id.to_string(),
                        Change::Language(c.chosen_language.clone()),
                    ))
                }),
            )
            .style(theme::tonal),
    ]
    .spacing(10)
    .into()
}

/// Only the DLC the account owns: the others cannot be installed.
fn dlcs<'a>(game_id: &'a str, c: &'a ContentInfo, busy: bool) -> Element<'a, Message> {
    if c.dlcs.is_empty() {
        return note("This game has no DLC.");
    }
    if !c.dlcs.iter().any(|d| d.owned) {
        return note("You own none of this game's DLC.");
    }
    Column::with_children(c.dlcs.iter().filter(|d| d.owned).map(|d| {
        let (id, dlc) = (game_id.to_string(), d.id.clone());
        dlc_row(d, c.chosen_dlcs.contains(&d.id), true, move |_| {
            Message::Maintenance(MaintenanceMsg::ToggleContentDlc(id.clone(), dlc.clone()))
        })
    }))
    .push(
        button(text("Apply DLC changes").size(14))
            .padding([10, 16])
            .on_press_maybe((!busy && c.dlcs_changed()).then(|| {
                Message::Maintenance(MaintenanceMsg::Apply(
                    game_id.to_string(),
                    Change::Dlcs(c.chosen_dlcs.clone()),
                ))
            }))
            .style(theme::tonal),
    )
    .spacing(10)
    .into()
}

/// The installed build, and any other GOG offers: older to go back, newer to update.
fn version<'a>(game_id: &'a str, c: &'a ContentInfo, busy: bool) -> Element<'a, Message> {
    let installed = c.versions.iter().find(|v| v.build_id == c.build_id);
    if c.versions.len() <= 1 {
        return text(installed.map_or("", |v| v.label.as_str()))
            .size(15)
            .into();
    }
    let id = game_id.to_string();
    column![
        pick_list(
            c.versions.clone(),
            c.versions.iter().find(|v| v.build_id == c.chosen_build).cloned(),
            move |v| Message::Maintenance(MaintenanceMsg::ChooseVersion(id.clone(), v.build_id)),
        )
        .style(theme::select)
        .font(theme::font())
        .padding([10, 14])
        .width(Length::Fill),
        button(text("Switch version").size(14))
            .padding([10, 16])
            .on_press_maybe((!busy && c.chosen_build != c.build_id).then(|| {
                Message::Maintenance(MaintenanceMsg::Apply(
                    game_id.to_string(),
                    Change::Build(c.chosen_build.clone()),
                ))
            }))
            .style(theme::tonal),
        note("Only the files that differ are downloaded. Saves made with a newer version may not load in an older one."),
    ]
    .spacing(10)
    .into()
}
