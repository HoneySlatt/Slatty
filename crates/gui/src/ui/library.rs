//! Library tab: shelves, filters, the cover grid and unfinished work.

use crate::theme::text;
use iced::widget::{
    Column, button, checkbox, column, container, hover, image, mouse_area, pick_list, progress_bar,
    row, scrollable, space,
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
use crate::{App, Filters, Interrupted, Message, Panel, Sort};

/// Space between covers, across and down.
const GRID_SPACING: f32 = 12.0;
/// Width the scrollbar takes beside the grid: its own and the gap before it.
const SCROLLBAR_ROOM: f32 = 18.0;

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
                    "Your library is empty: refresh it in Settings."
                } else {
                    "No game matches."
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
        let busy = self.installing().is_some();
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
        container(Column::with_children(rows).spacing(12))
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
        if (f.achievements || f.cloud_saves)
            && let Some(status) = self.overview_status()
        {
            items = items.push(text(status).size(13).color(tokens().muted));
        }
        container(items)
            .padding([12, 18])
            .width(Length::Fill)
            .style(theme::card)
            .into()
    }

    pub(super) fn card<'a>(&'a self, g: &'a LibraryGame) -> Element<'a, Message> {
        let art: Element<'_, Message> = match self.cover(&g.id) {
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
        let action = if installed {
            button(icon(Icon::Play, 16.0, tokens().on_accent))
                .padding([8, 14])
                .on_press_maybe(self.can_play(&g.id).then(|| Message::Play(g.id.clone())))
                .style(theme::primary)
        } else {
            button(icon(Icon::Download, 16.0, tokens().on_accent))
                .padding([8, 14])
                .on_press(Message::OpenDialog(g.id.clone(), Panel::Install))
                .style(theme::primary)
        };
        let settings = button(icon(Icon::SlidersHorizontal, 16.0, tokens().text))
            .padding(8)
            .on_press(Message::OpenDialog(
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
                    text(&g.title).size(13).font(semibold()),
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
            text(format!("{:.0} %", fraction(p) * 100.0)).size(14),
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
