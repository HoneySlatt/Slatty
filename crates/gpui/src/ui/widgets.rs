//! Building blocks shared by the pages, as `slatty-gui`'s: they take their colours and shapes from
//! the theme's tokens only.

use gpui::{
    Div, FontWeight, ObjectFit, Rgba, SharedString, Stateful, Svg, div, img, prelude::*, px, svg,
};
use slatty_app::icons::Icon;
use slatty_app::picture::Picture;

use crate::assets::{LOGO, icon_path};
use crate::theme::{self, Look, tokens};

/// A Lucide icon, `size` pixels square, in `color`.
pub fn icon(which: Icon, size: f32, color: Rgba) -> Svg {
    svg()
        .path(icon_path(which))
        .size(px(size))
        .flex_none()
        .text_color(color)
}

/// A button looking as `look` says. `id` names it for its hover and for tests, and its group, so
/// that an icon in it can follow the hover (`icon_in`).
pub fn button(id: &'static str, look: Look) -> Stateful<Div> {
    let button = div()
        .id(id)
        .group(id)
        .flex()
        .items_center()
        .flex_none()
        .rounded(look.radius)
        .text_color(look.text)
        .cursor_pointer();
    let button = match look.fill {
        Some(fill) => button.bg(fill),
        None => button,
    };
    button.hover(move |style| {
        let style = style.text_color(look.hover_text);
        match look.hover_fill {
            Some(fill) => style.bg(fill),
            None => style,
        }
    })
}

/// An icon in the button `id` of `look`, taking its text colour, hovered too.
pub fn icon_in(id: &'static str, look: Look, which: Icon, size: f32) -> Svg {
    icon(which, size, look.text).group_hover(id, move |style| style.text_color(look.hover_text))
}

/// Slatty's emblem, in the accent colour.
pub fn logo(height: f32) -> Svg {
    // The emblem's view box is 682 wide and 1,247 high.
    svg()
        .path(LOGO)
        .h(px(height))
        .w(px(height * 682.0 / 1247.0))
        .flex_none()
        .text_color(tokens().accent)
}

/// A tab of the top bar: icon and label, filled when its page is open.
pub fn nav_tab(id: &'static str, which: Icon, label: &'static str, active: bool) -> Stateful<Div> {
    let look = theme::nav_tab(active);
    button(id, look)
        .px(px(16.))
        .py(px(8.))
        .gap(px(10.))
        .child(icon_in(id, look, which, 18.))
        .child(
            div()
                .text_size(px(15.))
                .font_weight(FontWeight::SEMIBOLD)
                .child(label),
        )
}

/// A tab shown as an icon only.
pub fn icon_tab(id: &'static str, which: Icon, active: bool) -> Stateful<Div> {
    let look = theme::nav_tab(active);
    button(id, look).p(px(8.)).child(
        div()
            .size(px(22.))
            .flex()
            .items_center()
            .justify_center()
            .child(icon_in(id, look, which, 20.)),
    )
}

/// A thin vertical line between groups of a bar.
pub fn vertical_rule(height: f32) -> Div {
    div()
        .w(px(1.))
        .h(px(height))
        .flex_none()
        .bg(tokens().outline)
}

/// A thin horizontal line across its parent.
pub fn divider() -> Div {
    div().w_full().h(px(1.)).flex_none().bg(tokens().outline)
}

/// The account picture (or its initial), with the signed-in dot once signed in to GOG.
pub fn avatar(picture: Option<&Picture>, initial: String, size: f32, signed_in: bool) -> Div {
    let t = tokens();
    let face = match picture.and_then(Picture::path) {
        Some(path) => img(path.to_path_buf())
            .object_fit(ObjectFit::Cover)
            .size(px(size))
            .rounded_full()
            .into_any_element(),
        None => div()
            .size(px(size))
            .rounded_full()
            .bg(t.outline)
            .text_color(t.text)
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(size * 0.45))
            .font_weight(FontWeight::BOLD)
            .child(SharedString::from(initial))
            .into_any_element(),
    };
    let dot = size * 0.3;
    div()
        .relative()
        .size(px(size))
        .flex_none()
        .child(face)
        .when(signed_in, |avatar| {
            avatar.child(
                div()
                    .id("signed-in")
                    .absolute()
                    .right_0()
                    .bottom_0()
                    .size(px(dot))
                    .rounded_full()
                    .bg(t.success)
                    .border_2()
                    .border_color(t.background),
            )
        })
}
