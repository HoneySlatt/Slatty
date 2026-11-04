//! Lucide icons (ISC license, see assets/icons/LICENSE), tinted with the given colour.

use iced::widget::{Svg, svg};
use iced::{Color, Theme};

pub use slatty_app::icons::Icon;

pub fn icon<'a>(which: Icon, size: f32, color: Color) -> Svg<'a, Theme> {
    svg(svg::Handle::from_memory(which.svg()))
        .width(size)
        .height(size)
        .style(move |_, _| svg::Style { color: Some(color) })
}
