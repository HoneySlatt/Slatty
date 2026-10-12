//! Library tab: shelves, filters, the cover grid and unfinished work.

use crate::theme::text;
use iced::widget::{
    Column, button, checkbox, column, container, hover, image, mouse_area, pick_list, progress_bar,
    row, scrollable, space, tooltip,
};
use iced::{Alignment, ContentFit, Element, Length};
use slatty_core::installer::Progress;
use slatty_core::library::LibraryGame;
use slatty_core::maintenance::Change;

use super::format::*;
use super::widgets::{grid_rows, vertical_rule};
use crate::edit::EditMsg;
use crate::icons::{Icon, icon};
use crate::install::InstallMsg;
use crate::library::GridWindow;
use crate::maintenance::MaintenanceMsg;
use crate::theme::{self, semibold, tokens};
use crate::{App, Filters, Interrupted, Message, Panel, Shelf, Sort};

/// Space between covers, across and down.
const GRID_SPACING: f32 = 12.0;
/// Width the scrollbar takes beside the grid: its own and the gap before it.
const SCROLLBAR_ROOM: f32 = 18.0;
/// Height of a stopped download or update above the grid, and the space between two.
const STOPPED_ROW: f32 = 40.0;
const STOPPED_SPACING: f32 = 12.0;
/// Window height under which fewer of them show at once.
const SHORT_WINDOW: f32 = 720.0;

impl App {
    pub(super) fn library_page(&self) -> Element<'_, Message> {
        let games = self.visible_games();
        let filter_button = button(icon(
            Icon::ListFilter,
            20.0,
            if self.filters.any() {
                tokens().accent
            } else {
                tokens().text
            },
        ))
        .padding(8)
        .on_press(Message::ToggleFilters)
        .style(theme::ghost);
        let add_button = tooltip(
            button(icon(Icon::Plus, 20.0, tokens().text))
                .padding(8)
                .on_press(Message::Add(crate::add::AddMsg::Open))
                .style(theme::ghost),
            container(text("Add a game from no store").size(13))
                .padding([4, 10])
                .style(theme::block),
            tooltip::Position::Bottom,
        );
        let toolbar = row![
            pick_list(self.shelves(), Some(self.shelf), Message::ShowShelf)
                .style(theme::dropdown)
                .font(theme::font())
                .padding([9, 16])
                .text_size(14)
                .width(150),
            vertical_rule(22.0),
            text(format!("{} games", games.len()))
                .size(14)
                .color(tokens().muted),
            space().width(Length::Fill),
            pick_list(Sort::ALL, Some(self.sort), Message::SortBy)
                .style(theme::dropdown)
                .font(theme::font())
                .padding([9, 16])
                .text_size(14)
                .width(170),
            filter_button,
            add_button,
        ]
        .spacing(14)
        .align_y(Alignment::Center);

        // What shows above the grid, in one column: the grid keeps its place in the page, and with
        // it its scroll position, when a bar or a banner comes or goes.
        let mut head = column![toolbar].spacing(14);
        if self.filters_open {
            head = head.push(self.filter_bar());
        }
        if let Some((id, title, p)) = self.installing() {
            head = head.push(download_banner(id, title, p));
        }
        if !self.interrupted.is_empty() {
            head = head.push(self.interrupted_card());
        }

        let gallery: Element<'_, Message> = if games.is_empty() {
            container(
                text(if self.library.is_empty() {
                    if self.library_busy {
                        "Reading your library from GOG…"
                    } else if self.account.is_none() {
                        "Your library is empty: sign in to GOG in Settings."
                    } else {
                        "Your library is empty: refresh it in Settings."
                    }
                } else if !self.search.trim().is_empty() || self.filters.any() {
                    "No game matches."
                } else {
                    match self.shelf {
                        Shelf::All => "No game matches.",
                        Shelf::Installed => "No game installed yet.",
                        Shelf::Favorites => {
                            "No favorite yet: the heart beside Play on a game's page adds it here."
                        }
                        Shelf::Hidden => "No hidden game.",
                    }
                })
                .color(tokens().muted),
            )
            .padding(20)
            .into()
        } else {
            // Only the rows in view are built; the area's size comes from its last scroll, or from the
            // window until then (less the page's margins and the scrollbar's room).
            let view = self.grid_view;
            let width =
                view.map_or(self.window.width - 40.0, |v| v.bounds().width) - SCROLLBAR_ROOM;
            let height = view.map_or(self.window.height, |v| v.bounds().height);
            let offset = view.map_or(0.0, |v| v.absolute_offset().y);
            let w = GridWindow::new(
                games.len(),
                width,
                self.card_width,
                GRID_SPACING,
                None,
                offset,
                height,
            );
            scrollable(grid_rows(&w, games.len(), GRID_SPACING, |i| {
                self.card(games[i])
            }))
            .spacing(8)
            .id(crate::library::GRID)
            .on_scroll(Message::GridScrolled)
            .style(theme::scroller)
            .height(Length::Fill)
            .into()
        };
        column![head, gallery].spacing(14).into()
    }

    /// Downloads and updates left unfinished, with the way to resume or drop each one.
    pub(super) fn interrupted_card(&self) -> Element<'_, Message> {
        let busy = self.downloading();
        let rows = self.interrupted.iter().map(|(id, kind)| {
            let title = self.title_of(id);
            let working = self.maintenance.get(id).is_some_and(|m| m.busy);
            let (status, actions): (&str, Element<'_, Message>) = match kind {
                Interrupted::Download | Interrupted::Paused | Interrupted::Failed => (
                    match kind {
                        Interrupted::Paused => "Download paused. It resumes where it stopped.",
                        Interrupted::Failed => "Download failed. It resumes where it stopped.",
                        _ => "Download interrupted. It resumes where it stopped.",
                    },
                    row![
                        button(text("Resume").size(14))
                            .padding([8, 16])
                            .on_press_maybe(
                                (!busy).then(|| Message::OpenDialog(id.clone(), Panel::Install)),
                            )
                            .style(theme::primary),
                        button(text("Discard").size(14))
                            .padding([8, 16])
                            .on_press_maybe(
                                (!busy).then(|| Message::Install(InstallMsg::Discard(id.clone()))),
                            )
                            .style(theme::tonal),
                    ]
                    .spacing(8)
                    .into(),
                ),
                Interrupted::Update | Interrupted::UpdatePaused => (
                    if working {
                        "Finishing the update…"
                    } else if *kind == Interrupted::UpdatePaused {
                        "Update paused. The game cannot start until it is finished."
                    } else {
                        "Update interrupted. The game cannot start until it is finished."
                    },
                    button(text("Finish update").size(14))
                        .padding([8, 16])
                        .on_press_maybe((!working).then(|| {
                            Message::Maintenance(MaintenanceMsg::Apply(id.clone(), Change::Update))
                        }))
                        .style(theme::primary)
                        .into(),
                ),
            };
            row![
                icon(Icon::TriangleAlert, 18.0, theme::tokens().warning),
                column![
                    text(title).size(15).font(semibold()),
                    text(status).size(13).color(tokens().muted)
                ]
                .spacing(2)
                .width(Length::Fill),
                actions,
            ]
            .spacing(14)
            .align_y(Alignment::Center)
            .into()
        });
        // Many of them scroll in the card, so that the grid keeps its room: in a short window one,
        // and the top of the next to show there are more; four otherwise.
        let shown = if self.window.height < SHORT_WINDOW {
            1.5
        } else {
            4.0
        };
        container(
            container(
                scrollable(Column::with_children(rows).spacing(STOPPED_SPACING))
                    .spacing(8)
                    .style(theme::scroller),
            )
            .max_height(shown * STOPPED_ROW + (shown - 1.0) * STOPPED_SPACING),
        )
        .padding([14, 18])
        .width(Length::Fill)
        .style(theme::card)
        .into()
    }

    pub(super) fn filter_bar(&self) -> Element<'_, Message> {
        let f = self.filters;
        let check = |label, on: bool, set: fn(Filters, bool) -> Filters| {
            checkbox(on)
                .label(label)
                .text_size(14)
                .font(theme::font())
                .on_toggle(move |v| Message::SetFilters(set(f, v)))
        };
        let mut items = row![
            check("GOG", f.gog, |f, v| Filters { gog: v, ..f }),
            check("Steam", f.steam, |f, v| Filters { steam: v, ..f }),
            check("Added", f.added, |f, v| Filters { added: v, ..f }),
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
        .spacing(18)
        .align_y(Alignment::Center);
        if (f.achievements || f.cloud_saves)
            && let Some(status) = self.overview_status()
        {
            items = items.push(text(status).size(13).color(tokens().muted));
        }
        // Seven of them are wider than the smallest window.
        container(items.wrap().vertical_spacing(10))
            .padding([12, 18])
            .width(Length::Fill)
            .style(theme::card)
            .into()
    }

    pub(super) fn card<'a>(&'a self, g: &'a LibraryGame) -> Element<'a, Message> {
        let art: Element<'_, Message> = match self.grid_cover(&g.id) {
            Some(h) => image(h)
                .content_fit(ContentFit::Cover)
                .width(Length::Fill)
                .height(Length::Fill)
                .border_radius(tokens().cover_radius)
                .into(),
            None => container(text(&g.title).size(14).font(semibold()))
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
        let installable = self.not_installable(&g.id).is_none();
        let action = if self.steam_game(&g.id).is_some() {
            use crate::steam::SteamMsg;
            use slatty_core::steam::client::Action;
            let installed = self.steam.installed.get(&g.id);
            let downloading = installed.is_some_and(|i| i.update.is_some_and(|u| u.started));
            let (ic, action) = match installed.map(|i| i.ready) {
                _ if self.steam.running.contains(&g.id) => (Icon::Play, None),
                _ if downloading => (Icon::Download, None),
                Some(true) => (Icon::Play, Some(Action::Play)),
                Some(false) => (Icon::Download, None),
                None => (Icon::Download, Some(Action::Install)),
            };
            button(icon(ic, 16.0, tokens().on_accent))
                .padding([8, 14])
                .on_press_maybe(action.map(|a| Message::Steam(SteamMsg::Ask(g.id.clone(), a))))
                .style(theme::primary)
        } else if installed {
            button(icon(Icon::Play, 16.0, tokens().on_accent))
                .padding([8, 14])
                .on_press_maybe(self.can_play(&g.id).then(|| Message::Play(g.id.clone())))
                .style(theme::primary)
        } else {
            button(icon(Icon::Download, 16.0, tokens().on_accent))
                .padding([8, 14])
                .on_press_maybe(
                    installable.then(|| Message::OpenDialog(g.id.clone(), Panel::Install)),
                )
                .style(theme::primary)
        };
        // Steam keeps a Steam game's settings: its page says what can be asked of Steam. A product
        // GOG offers nothing to install for has none: its page says why.
        let settings = button(icon(Icon::SlidersHorizontal, 16.0, tokens().text))
            .padding(8)
            .on_press(if self.steam_game(&g.id).is_some() || !installable {
                Message::Select(g.id.clone())
            } else {
                Message::OpenDialog(
                    g.id.clone(),
                    if installed {
                        Panel::GameSettings
                    } else {
                        Panel::Install
                    },
                )
            })
            .style(theme::plain);
        let overlay = column![
            space().height(Length::Fill),
            container(
                column![
                    // Two lines at most: a longer title would push the buttons off the cover.
                    container(text(&g.title).size(13).font(semibold()))
                        .max_height(2.0 * 13.0 * 1.3)
                        .clip(true),
                    row![settings, space().width(Length::Fill), action].align_y(Alignment::Center),
                ]
                .spacing(4),
            )
            .padding([8, 10])
            .width(Length::Fill)
            .style(theme::cover_overlay),
        ];
        // A right click opens the cover's menu (edit the title, sorting title, cover, background).
        mouse_area(hover(
            base,
            container(overlay)
                .width(Length::Fill)
                .height(Length::Fill)
                .style(theme::cover_frame),
        ))
        .on_right_press(Message::Edit(EditMsg::Menu(g.id.clone())))
        .into()
    }
}

pub(super) fn download_banner<'a>(id: &str, title: &str, p: Progress) -> Element<'a, Message> {
    button(
        row![
            icon(Icon::Download, 16.0, tokens().accent),
            text(format!("Downloading {title}"))
                .size(14)
                .width(Length::Fill),
            progress_bar(0.0..=1.0, fraction(p))
                .length(260)
                .girth(8)
                .style(theme::progress),
            text(format!("{:.0}%", fraction(p) * 100.0)).size(14),
        ]
        .spacing(12)
        .align_y(Alignment::Center),
    )
    .padding([10, 16])
    .width(Length::Fill)
    .on_press(Message::Select(id.to_string()))
    .style(theme::row_button)
    .into()
}
