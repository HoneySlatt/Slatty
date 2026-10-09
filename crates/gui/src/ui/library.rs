//! Library tab: shelves, filters, the cover grid and unfinished work.

use iced::widget::{
    Column, Space, button, checkbox, column, container, grid, hover, image, pick_list,
    progress_bar, row, scrollable, slider, space, text,
};
use iced::{Alignment, ContentFit, Element, Length};
use slatty_core::installer::Progress;
use slatty_core::library::LibraryGame;
use slatty_core::maintenance::Change;

use super::format::*;
use crate::icons::{Icon, icon};
use crate::install::InstallMsg;
use crate::maintenance::MaintenanceMsg;
use crate::theme::{self, ACCENT, MUTED, ON_ACCENT, SEMIBOLD, TEXT};
use crate::{App, Filters, Interrupted, Message, Panel, Shelf, Sort};

impl App {
    pub(super) fn library_page(&self) -> Element<'_, Message> {
        let games = self.visible_games();
        let shelf = |label, s: Shelf| {
            button(text(label).size(14).font(SEMIBOLD))
                .padding([8, 20])
                .on_press(Message::ShowShelf(s))
                .style(theme::segment(self.shelf == s))
        };
        let shelves = container(
            row![
                shelf("All", Shelf::All),
                shelf("Installed", Shelf::Installed),
                shelf("Favorites", Shelf::Favorites),
            ]
            .spacing(4),
        )
        .padding(4)
        .style(theme::pill);
        let filter_button = button(icon(
            Icon::ListFilter,
            18.0,
            if self.filters.any() { ON_ACCENT } else { TEXT },
        ))
        .padding(11)
        .on_press(Message::ToggleFilters)
        .style(if self.filters.any() {
            theme::segment(true)
        } else {
            theme::segment(false)
        });
        let toolbar = row![
            shelves,
            text(format!("{} games", games.len())).size(14).color(MUTED),
            space().width(Length::Fill),
            pick_list(Sort::ALL, Some(self.sort), Message::SortBy)
                .style(theme::select)
                .padding([9, 18])
                .text_size(14),
            Space::new().width(4),
            icon(Icon::LayoutGrid, 20.0, MUTED),
            slider(110.0..=240.0, self.card_width, Message::CardWidth)
                .width(150)
                .style(theme::size_slider),
            container(filter_button).style(theme::pill),
        ]
        .spacing(14)
        .align_y(Alignment::Center);

        let mut page = column![toolbar].spacing(14);
        if self.filters_open {
            page = page.push(self.filter_bar());
        }
        if let Some((id, title, p)) = self.installing() {
            page = page.push(download_banner(id, title, p));
        }
        if !self.interrupted.is_empty() {
            page = page.push(self.interrupted_card());
        }

        let cards: Vec<Element<'_, Message>> = games.iter().map(|g| self.card(g)).collect();
        let gallery: Element<'_, Message> = if cards.is_empty() {
            container(
                text(if self.library.is_empty() {
                    "Your library is empty: refresh it in Settings."
                } else {
                    "No game matches."
                })
                .color(MUTED),
            )
            .padding(20)
            .into()
        } else {
            scrollable(
                grid(cards)
                    .fluid(self.card_width)
                    .spacing(14)
                    .height(grid::aspect_ratio(3, 4)),
            )
            .spacing(8)
            .style(theme::scroller)
            .height(Length::Fill)
            .into()
        };
        page.push(gallery).into()
    }

    /// Downloads and updates left unfinished, with the way to resume or drop each one.
    pub(super) fn interrupted_card(&self) -> Element<'_, Message> {
        let busy = self.installing().is_some();
        let rows = self.interrupted.iter().map(|(id, kind)| {
            let title = self.title_of(id);
            let working = self.maintenance.get(id).is_some_and(|m| m.busy);
            let (status, actions): (&str, Element<'_, Message>) = match kind {
                Interrupted::Download => (
                    "Download interrupted. It resumes where it stopped.",
                    row![
                        button(text("Resume").size(14))
                            .padding([8, 16])
                            .on_press_maybe(
                                (!busy).then(|| Message::SelectWith(id.clone(), Panel::Install)),
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
                Interrupted::Update => (
                    if working {
                        "Finishing the update…"
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
                icon(Icon::TriangleAlert, 18.0, theme::WARNING),
                column![
                    text(title).size(15).font(SEMIBOLD),
                    text(status).size(13).color(MUTED)
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
            items = items.push(text(status).size(13).color(MUTED));
        }
        container(items)
            .padding([12, 18])
            .width(Length::Fill)
            .style(theme::card)
            .into()
    }

    pub(super) fn card<'a>(&'a self, g: &'a LibraryGame) -> Element<'a, Message> {
        let art: Element<'_, Message> = match self.covers.get(&g.id) {
            Some(h) => image(h.clone())
                .content_fit(ContentFit::Cover)
                .width(Length::Fill)
                .height(Length::Fill)
                .border_radius(12)
                .into(),
            None => container(text(&g.title).size(14).font(SEMIBOLD))
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
            button(icon(Icon::Play, 16.0, ON_ACCENT))
                .padding([8, 14])
                .on_press_maybe(self.can_play(&g.id).then(|| Message::Play(g.id.clone())))
                .style(theme::primary)
        } else {
            button(icon(Icon::Download, 16.0, ON_ACCENT))
                .padding([8, 14])
                .on_press(Message::SelectWith(g.id.clone(), Panel::Install))
                .style(theme::primary)
        };
        let settings = button(icon(Icon::SlidersHorizontal, 16.0, TEXT))
            .padding(8)
            .on_press(Message::SelectWith(
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
                    text(&g.title).size(13).font(SEMIBOLD),
                    row![settings, space().width(Length::Fill), action].align_y(Alignment::Center),
                ]
                .spacing(4),
            )
            .padding([8, 10])
            .width(Length::Fill)
            .style(theme::cover_overlay),
        ];
        hover(
            base,
            container(overlay)
                .width(Length::Fill)
                .height(Length::Fill)
                .style(theme::cover_frame),
        )
    }
}

pub(super) fn download_banner<'a>(id: &str, title: &str, p: Progress) -> Element<'a, Message> {
    button(
        row![
            icon(Icon::Download, 16.0, ACCENT),
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
    .on_press(Message::SelectWith(id.to_string(), Panel::Install))
    .style(theme::row_button)
    .into()
}
