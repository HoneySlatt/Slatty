//! The Downloads tab: the install downloading, the queue in the order it will run, and the installs
//! finished since the start.

use crate::theme::text;
use iced::widget::{
    Column, Space, button, column, container, image, mouse_area, progress_bar, row, scrollable,
};
use iced::{Alignment, ContentFit, Element, Length, Padding};

use super::format::*;
use super::game::cancel_prompt;
use super::note;
use super::widgets::{round_button, vertical_rule};
use crate::downloads::{DownloadsMsg, ROW_HEIGHT, ROW_SPACING};
use crate::icons::{Icon, icon};
use crate::install::{Cancelling, InstallMsg, InstallView};
use crate::maintenance::MaintenanceMsg;
use crate::theme::{self, bold, semibold, tokens};
use crate::{App, Interrupted, Message, Panel};

impl App {
    pub(super) fn downloads_page(&self) -> Element<'_, Message> {
        // Downloads stopped by a pause, a closed window or a crash wait at the top too.
        let stopped: Vec<(&str, Interrupted)> = self
            .interrupted
            .iter()
            .filter(|(_, k)| {
                matches!(
                    k,
                    Interrupted::Download | Interrupted::Paused | Interrupted::Failed
                )
            })
            .map(|(id, k)| (id.as_str(), *k))
            .collect();
        let active = usize::from(self.installing().is_some()) + stopped.len();
        let header = column![
            text("Downloads").size(32).font(bold()),
            text(format!(
                "{active} active · {} queued · {} completed",
                self.queue.len(),
                self.completed.len()
            ))
            .size(14)
            .color(tokens().muted),
        ]
        .spacing(4);

        let mut downloading = Column::new().spacing(12);
        if let Some((id, _, _)) = self.installing() {
            downloading = downloading.push(self.download_card(id));
        }
        for (id, kind) in &stopped {
            downloading = downloading.push(self.stopped_card(id, *kind));
        }
        if active == 0 && self.updates.current.is_none() {
            downloading = downloading.push(note(if self.queue.is_empty() {
                "Nothing is downloading. Installs started while another one downloads wait here."
            } else {
                "Nothing is downloading: the queue starts with the next install."
            }));
        }
        let mut content = column![header, heading("Downloading", None), downloading].spacing(16);
        if !self.queue.is_empty() {
            content = content
                .push(heading("Queue", Some(self.queue.len())))
                .push(self.queue_list());
        }
        let updates = usize::from(self.updates.current.is_some()) + self.updates.waiting.len();
        if updates > 0 {
            content = content
                .push(heading("Updates", Some(updates)))
                .push(self.updates_list());
        }
        if !self.completed.is_empty() {
            content = content
                .push(heading("Completed", Some(self.completed.len())))
                .push(
                    Column::with_children(
                        self.completed
                            .iter()
                            .map(|(id, size)| self.completed_row(id, *size)),
                    )
                    .spacing(ROW_SPACING),
                );
        }
        scrollable(container(content).padding(Padding::ZERO.top(10).right(14)))
            .style(theme::scroller)
            .height(Length::Fill)
            .into()
    }

    /// The game updating without asking, with its progress and Pause, then those waiting.
    fn updates_list(&self) -> Element<'_, Message> {
        let mut list = Column::new().spacing(ROW_SPACING);
        if let Some(id) = self.updates.current.as_deref() {
            let progress = self
                .maintenance
                .get(id)
                .and_then(|m| m.progress)
                .unwrap_or_default();
            list = list.push(
                container(
                    column![
                        row![
                            text(self.title_of(id))
                                .size(18)
                                .font(semibold())
                                .width(Length::Fill),
                            round_button(
                                Icon::Pause,
                                Message::Maintenance(MaintenanceMsg::Pause(id.to_string()))
                            ),
                        ]
                        .align_y(Alignment::Center),
                        row![
                            progress_bar(0.0..=1.0, fraction(progress))
                                .girth(10)
                                .style(theme::progress),
                            text(format!("Updating {:.0} %", fraction(progress) * 100.0))
                                .size(15)
                                .width(Length::Shrink),
                        ]
                        .spacing(14)
                        .align_y(Alignment::Center),
                    ]
                    .spacing(12),
                )
                .padding(18)
                .width(Length::Fill)
                .style(theme::card),
            );
        }
        for id in &self.updates.waiting {
            list = list.push(
                container(
                    row![
                        text(self.title_of(id)).size(15).width(Length::Fill),
                        text("Waiting").size(14).color(tokens().muted),
                    ]
                    .align_y(Alignment::Center),
                )
                .padding([14, 18])
                .width(Length::Fill)
                .style(theme::card),
            );
        }
        list.push(note(
            "Games on their newest build are updated one after the other, never while a game \
             runs or an install downloads. A paused update waits until you apply it in Manage.",
        ))
        .into()
    }

    /// The running install: progress, speed, time left, Pause and Cancel.
    fn download_card<'a>(&'a self, id: &'a str) -> Element<'a, Message> {
        let Some(InstallView::Running {
            title,
            folder,
            progress,
            cancelling,
            rate,
            ..
        }) = self.install_views.get(id)
        else {
            return Space::new().into();
        };
        let p = *progress;
        let speed = rate.per_second();
        let left = speed
            .filter(|s| *s > 0.0)
            .map(|s| p.bytes_total.saturating_sub(p.bytes_done) as f64 / s);
        let stat = |s: String| text(s).size(15).width(Length::Fill);
        let msg = |m: fn(String) -> InstallMsg| Message::Install(m(id.to_string()));
        let actions = (*cancelling == Cancelling::No).then(|| {
            row![
                round_button(Icon::Pause, msg(InstallMsg::Pause)),
                round_button(Icon::X, msg(InstallMsg::AskCancel)),
            ]
            .spacing(10)
        });
        let details = column![
            row![
                column![
                    text(title.as_str()).size(20).font(semibold()),
                    row![
                        icon(Icon::Folder, 16.0, tokens().muted),
                        text(folder.display().to_string())
                            .size(14)
                            .color(tokens().muted)
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                ]
                .spacing(6)
                .width(Length::Fill),
            ]
            .push(actions)
            .align_y(Alignment::Start),
            row![
                progress_bar(0.0..=1.0, fraction(p))
                    .girth(10)
                    .style(theme::progress),
                text(format!("{:.0} %", fraction(p) * 100.0))
                    .size(16)
                    .font(semibold())
                    .width(56)
                    .align_x(Alignment::End),
            ]
            .spacing(14)
            .align_y(Alignment::Center),
            row![
                stat(format!(
                    "{} / {}",
                    human_size(p.bytes_done),
                    human_size(p.bytes_total)
                )),
                vertical_rule(18.0),
                stat(speed.map_or_else(|| "—".into(), |s| format!("{}/s", human_size(s as u64)))),
                vertical_rule(18.0),
                stat(left.map_or_else(
                    || "Estimating…".into(),
                    |s| format!("{} remaining", remaining(s))
                )),
            ]
            .spacing(20)
            .align_y(Alignment::Center),
        ]
        .push(cancel_prompt(id, title, p, *cancelling))
        .spacing(16)
        .width(Length::Fill);
        container(
            row![self.cover_art(id, 124.0), details]
                .spacing(24)
                .align_y(Alignment::Center),
        )
        .padding(16)
        .width(Length::Fill)
        .style(theme::card)
        .into()
    }

    /// A download stopped before its end: resumed or discarded from here.
    fn stopped_card<'a>(&'a self, id: &'a str, kind: Interrupted) -> Element<'a, Message> {
        let busy = self.installing().is_some();
        container(
            row![
                self.cover_art(id, 48.0),
                column![
                    text(self.title_of(id)).size(16).font(semibold()),
                    note(match kind {
                        Interrupted::Paused => "Paused. It resumes where it stopped.",
                        Interrupted::Failed => "Failed. It resumes where it stopped.",
                        _ => "Interrupted. It resumes where it stopped.",
                    }),
                ]
                .spacing(4)
                .width(Length::Fill),
                button(text("Resume").size(14))
                    .padding([10, 18])
                    .on_press(Message::OpenDialog(id.to_string(), Panel::Install))
                    .style(theme::primary),
                button(text("Discard").size(14))
                    .padding([10, 18])
                    .on_press_maybe(
                        (!busy).then(|| Message::Install(InstallMsg::Discard(id.to_string()))),
                    )
                    .style(theme::tonal),
            ]
            .spacing(16)
            .align_y(Alignment::Center),
        )
        .padding([12, 16])
        .width(Length::Fill)
        .style(theme::card)
        .into()
    }

    /// The queue, each row moved by its handle. The pointer is followed over the whole list, and
    /// leaving it drops the row where it was.
    fn queue_list(&self) -> Element<'_, Message> {
        let moving = self.drag.map(|d| d.to);
        let rows = self.queue_order().into_iter().enumerate().map(|(i, id)| {
            let size = match self.install_views.get(id) {
                Some(InstallView::Queued(info) | InstallView::Ready(info)) => {
                    human_size(info.total_download())
                }
                _ => "—".into(),
            };
            let handle =
                mouse_area(container(icon(Icon::GripVertical, 20.0, tokens().muted)).center(36))
                    .on_press(Message::Downloads(DownloadsMsg::Grab(i)))
                    .interaction(iced::mouse::Interaction::Grab);
            container(
                row![
                    handle,
                    text((i + 1).to_string())
                        .size(15)
                        .color(tokens().muted)
                        .width(24),
                    self.cover_art(id, 40.0),
                    text(self.title_of(id))
                        .size(16)
                        .font(semibold())
                        .width(Length::Fill),
                    text(size).size(15),
                    round_button(
                        Icon::X,
                        Message::Downloads(DownloadsMsg::Remove(id.to_string()))
                    ),
                ]
                .spacing(16)
                .align_y(Alignment::Center),
            )
            .padding([0, 12])
            .height(ROW_HEIGHT)
            .width(Length::Fill)
            .center_y(ROW_HEIGHT)
            .style(if moving == Some(i) {
                theme::outlined
            } else {
                theme::card
            })
            .into()
        });
        mouse_area(Column::with_children(rows).spacing(ROW_SPACING))
            .on_move(|p| Message::Downloads(DownloadsMsg::Drag(p.y)))
            .on_release(Message::Downloads(DownloadsMsg::Drop))
            .on_exit(Message::Downloads(DownloadsMsg::Drop))
            .into()
    }

    fn completed_row<'a>(&'a self, id: &'a str, size: u64) -> Element<'a, Message> {
        let play = self.installs.contains_key(id).then(|| {
            button(
                row![
                    icon(Icon::Play, 16.0, tokens().accent),
                    text("Play").size(15).font(semibold())
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .padding([10, 22])
            .on_press(Message::Play(id.to_string()))
            .style(theme::accent_outline)
        });
        container(
            row![
                self.cover_art(id, 40.0),
                text(self.title_of(id))
                    .size(16)
                    .font(semibold())
                    .width(Length::Fill),
                text(human_size(size)).size(15).width(110),
                row![
                    icon(Icon::CircleCheck, 18.0, tokens().success),
                    text("Completed").size(14).color(tokens().muted)
                ]
                .spacing(8)
                .width(150)
                .align_y(Alignment::Center),
            ]
            .push(play)
            .spacing(16)
            .align_y(Alignment::Center),
        )
        .padding([0, 12])
        .height(ROW_HEIGHT)
        .center_y(ROW_HEIGHT)
        .width(Length::Fill)
        .style(theme::card)
        .into()
    }

    /// A game's cover at `width`, in its 3:4 shape.
    fn cover_art(&self, id: &str, width: f32) -> Element<'_, Message> {
        let height = width * 4.0 / 3.0;
        match self.cover(id) {
            Some(h) => image(h)
                .content_fit(ContentFit::Cover)
                .width(width)
                .height(height)
                .border_radius(tokens().cover_radius)
                .into(),
            None => container(Space::new())
                .width(width)
                .height(height)
                .style(theme::placeholder)
                .into(),
        }
    }
}

/// A section title with a rule running to the right edge.
fn heading<'a>(title: &'a str, count: Option<usize>) -> Element<'a, Message> {
    let label = match count {
        Some(n) => format!("{title} · {n}"),
        None => title.to_string(),
    };
    row![
        text(label).size(20).font(semibold()),
        container(Space::new())
            .width(Length::Fill)
            .height(1)
            .style(theme::divider),
    ]
    .spacing(16)
    .align_y(Alignment::Center)
    .into()
}

/// Time left, rounded for reading: "8 min", "1 h 05 min", "less than a minute".
fn remaining(seconds: f64) -> String {
    let minutes = (seconds / 60.0).ceil() as u64;
    match (minutes / 60, minutes % 60) {
        (0, 0 | 1) if seconds < 60.0 => "less than a minute".into(),
        (0, m) => format!("{m} min"),
        (h, m) => format!("{h} h {m:02} min"),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn time_left_reads_well() {
        assert_eq!(super::remaining(20.0), "less than a minute");
        assert_eq!(super::remaining(8.0 * 60.0 - 5.0), "8 min");
        assert_eq!(super::remaining(65.0 * 60.0), "1 h 05 min");
    }
}
