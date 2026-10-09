//! The Cloud saves drawer: where the saves are, what a sync would do, and the sync itself.

use iced::widget::{Column, Space, button, column, container, row, scrollable, text};
use iced::{Alignment, Element, Length, Padding};
use slatty_core::cloud::sync::Prefer;
use slatty_core::library::LibraryGame;

use super::note;
use crate::cloud::{CloudRequest, Counts, SaveLocation};
use crate::icons::{Icon, icon};
use crate::theme::{self, SEMIBOLD, tokens};
use crate::{App, Message};

impl App {
    pub(super) fn cloud_drawer<'a>(
        &'a self,
        page: Element<'a, Message>,
        g: &'a LibraryGame,
    ) -> Element<'a, Message> {
        let subtitle = text(&g.title).size(18).color(tokens().muted);
        let body = scrollable(container(self.cloud_details(g)).padding(Padding::ZERO.right(14)))
            .style(theme::scroller)
            .height(Length::Fill);
        self.drawer(
            page,
            "Cloud saves",
            subtitle.into(),
            body.into(),
            Some(self.cloud_actions(g)),
        )
    }

    fn cloud_details<'a>(&'a self, g: &'a LibraryGame) -> Element<'a, Message> {
        let t = tokens();
        let cloud = self.cloud.get(&g.id);
        let launched = self
            .installs
            .get(&g.id)
            .is_some_and(slatty_core::runner::prefix_ready);
        let (ic, color, status) = self.cloud_status(g);
        let mut col = Column::new().spacing(22).push(
            // The state's own icon, unless it is the cloud already.
            row![icon(Icon::Cloud, 36.0, t.text)]
                .push((ic != Icon::Cloud).then(|| icon(ic, 22.0, color)))
                .push(text(status).size(17))
                .spacing(14)
                .align_y(Alignment::Center),
        );
        if !launched {
            col = col.push(note(
                "Your cloud saves are downloaded when the game first starts, before it runs.",
            ));
        }
        let Some(c) = cloud else {
            return col.into();
        };
        col = col.extend(c.lines.iter().map(|l| note(l.as_str())));
        let several = c.locations.len() > 1;
        for l in &c.locations {
            col = col.push(location(l, several));
        }
        if let Some(last) = &c.last_sync {
            col = col.push(note(last.as_str()));
        }
        if c.conflicts && !c.busy {
            let keep = |label, side| {
                button(container(text(label).size(14)).center_x(Length::Fill))
                    .padding([12, 0])
                    .width(Length::Fill)
                    .on_press(Message::Cloud(g.id.clone(), CloudRequest::Keep(side)))
                    .style(theme::primary)
            };
            col = col.push(
                column![
                    note(
                        "Both versions changed. Choose the one to keep; the other one is kept \
                         in the backups folder."
                    ),
                    keep("Keep the local version", Prefer::Local),
                    keep("Keep the cloud version", Prefer::Remote),
                ]
                .spacing(10),
            );
        }
        col.into()
    }

    /// Check and Sync now, across the bottom of the drawer.
    fn cloud_actions<'a>(&'a self, g: &'a LibraryGame) -> Element<'a, Message> {
        let busy = self.cloud.get(&g.id).is_some_and(|c| c.busy);
        let launched = self
            .installs
            .get(&g.id)
            .is_some_and(slatty_core::runner::prefix_ready);
        let labelled = |ic, label, color| {
            row![icon(ic, 20.0, color), text(label).size(16).font(SEMIBOLD)]
                .spacing(12)
                .align_y(Alignment::Center)
        };
        let check = button(labelled(Icon::Search, "Check", tokens().text))
            .padding([14, 22])
            .on_press_maybe((!busy).then(|| Message::Cloud(g.id.clone(), CloudRequest::Check)))
            .style(theme::action_row);
        // Before the first launch, the game's first start downloads the saves.
        let sync = button(
            container(labelled(Icon::RefreshCw, "Sync now", tokens().on_accent))
                .center_x(Length::Fill),
        )
        .padding([14, 22])
        .width(Length::Fill)
        .on_press_maybe(
            (!busy && launched).then(|| Message::Cloud(g.id.clone(), CloudRequest::Sync)),
        )
        .style(theme::primary);
        row![check, sync]
            .spacing(12)
            .align_y(Alignment::Center)
            .into()
    }
}

/// A save folder, then what a sync would do in it.
fn location<'a>(l: &'a SaveLocation, named: bool) -> Element<'a, Message> {
    let t = tokens();
    let title = if named {
        format!("Save folder · {}", l.name)
    } else {
        "Save folder".to_string()
    };
    let mut col = column![
        row![icon(Icon::Folder, 22.0, t.text), text(title).size(16)]
            .spacing(12)
            .align_y(Alignment::Center),
        container(text(l.folder.as_str()).size(14))
            .padding([12, 14])
            .width(Length::Fill)
            .style(theme::outlined),
    ]
    .spacing(12);
    if let Some(c) = l.counts {
        col = col.push(Space::new().height(6)).push(
            container(Space::new())
                .width(Length::Fill)
                .height(1)
                .style(theme::divider),
        );
        col = col.push(counts(c));
    }
    col.extend(l.notes.iter().map(|n| note(n.as_str()))).into()
}

fn counts<'a>(c: Counts) -> Element<'a, Message> {
    let t = tokens();
    let line = |ic, label: &'a str, n: usize| {
        row![
            icon(ic, 22.0, t.muted),
            text(label).size(15).width(Length::Fill),
            if n > 0 {
                text(n.to_string()).size(15).font(SEMIBOLD)
            } else {
                text(n.to_string()).size(15).color(t.muted)
            },
        ]
        .spacing(16)
        .align_y(Alignment::Center)
    };
    column![
        line(Icon::ArrowUp, "To upload", c.upload),
        line(Icon::ArrowDown, "To download", c.download),
        line(Icon::ArrowRightLeft, "To compare", c.compare),
        line(Icon::CircleCheck, "Unchanged", c.unchanged),
        line(Icon::Trash, "Deleted on one side", c.deleted),
    ]
    .spacing(16)
    .into()
}
