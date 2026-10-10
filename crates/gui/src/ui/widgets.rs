//! Building blocks shared by the pages. They take their colours and shapes from the theme
//! tokens only, so a custom theme changes them everywhere at once.

use crate::theme::text;
use iced::widget::{Column, Space, button, container, image, row, stack, svg};
use iced::{Alignment, ContentFit, Element, Length};

use crate::Message;
use crate::icons::{Icon, icon};
use crate::library::GridWindow;
use crate::theme::{self, bold, semibold, tokens};

/// SlattyLauncher's emblem, in the accent colour.
pub fn logo<'a>(height: f32) -> Element<'a, Message> {
    svg(svg::Handle::from_memory(
        include_bytes!("../../assets/logo.svg").as_slice(),
    ))
    .height(height)
    .width(Length::Shrink)
    .style(|_, _| svg::Style {
        color: Some(tokens().accent),
    })
    .into()
}

/// A tab of the top bar: icon and label, filled when its page is open.
pub fn nav_tab<'a>(ic: Icon, label: &'a str, active: bool, msg: Message) -> Element<'a, Message> {
    let color = if active {
        tokens().on_accent
    } else {
        tokens().text
    };
    button(
        row![
            icon(ic, 18.0, color),
            text(label).size(15).font(semibold()).color(color)
        ]
        .spacing(10)
        .align_y(Alignment::Center),
    )
    .padding([8, 16])
    .on_press(msg)
    .style(theme::nav_tab(active))
    .into()
}

/// A tab shown as an icon only. `id` lets tests and assistive tools find it.
pub fn icon_tab<'a>(
    ic: Icon,
    id: &'static str,
    active: bool,
    msg: Message,
) -> Element<'a, Message> {
    let color = if active {
        tokens().on_accent
    } else {
        tokens().text
    };
    button(container(icon(ic, 20.0, color)).center(22).id(id))
        .padding(8)
        .on_press(msg)
        .style(theme::nav_tab(active))
        .into()
}

/// A thin vertical line between groups of a bar.
pub fn vertical_rule<'a>(height: f32) -> Element<'a, Message> {
    container(Space::new())
        .width(1)
        .height(height)
        .style(theme::divider)
        .into()
}

/// The account picture (or its initial) with the signed-in dot.
pub fn avatar<'a>(
    picture: Option<image::Handle>,
    initial: String,
    size: f32,
) -> Element<'a, Message> {
    let face: Element<'a, Message> = match picture {
        Some(h) => image(h)
            .content_fit(ContentFit::Cover)
            .width(size)
            .height(size)
            .border_radius(size / 2.0)
            .into(),
        None => container(text(initial).size(size * 0.45).font(bold()))
            .center(size)
            .style(theme::avatar)
            .into(),
    };
    let dot = container(Space::new())
        .width(size * 0.3)
        .height(size * 0.3)
        .style(theme::status_dot);
    stack![
        face,
        container(dot)
            .width(size)
            .height(size)
            .align_right(size)
            .align_bottom(size),
    ]
    .into()
}

/// A titled block of the settings page or of a panel.
pub fn round_button<'a>(ic: Icon, msg: Message) -> Element<'a, Message> {
    button(container(icon(ic, 20.0, tokens().text)).center(24))
        .padding(10)
        .on_press(msg)
        .style(theme::icon_button)
        .into()
}

pub fn note<'a>(s: impl iced::widget::text::IntoFragment<'a>) -> Element<'a, Message> {
    text(s).size(14).color(tokens().muted).into()
}

pub fn inner(_: &iced::Theme) -> container::Style {
    container::Style {
        background: Some(iced::Background::Color(tokens().surface_high)),
        border: iced::border::rounded(14),
        ..Default::default()
    }
}

/// A grid of `count` cards laid out by `w`, one child of the column per row of the whole grid: the
/// rows in view hold their cards, the others are spaces of the same height. A row still in view
/// after a scroll is then the same child as before, and Iced keeps what it laid out for it (the
/// titles above all, slow to shape in scripts the interface font lacks).
pub fn grid_rows<'a>(
    w: &GridWindow,
    count: usize,
    spacing: f32,
    card: impl Fn(usize) -> Element<'a, Message>,
) -> Column<'a, Message> {
    Column::with_children((0..w.rows).map(|r| {
        if !w.shown.contains(&r) {
            return Space::new().height(w.row_height).into();
        }
        let cards = w.cards(r, count);
        let empty = w.columns - cards.len();
        row(cards
            .map(|i| {
                container(card(i))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .into()
            })
            .chain((0..empty).map(|_| Space::new().width(Length::Fill).into())))
        .spacing(spacing)
        .height(w.row_height)
        .into()
    }))
    .spacing(spacing)
}
