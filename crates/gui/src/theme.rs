//! The interface's look: every colour comes from [`Tokens`], and the widget styles below are built
//! from them only. A custom theme is another `Tokens`; views never name a colour themselves.

use std::sync::OnceLock;

use iced::widget::{button, container, pick_list, progress_bar, scrollable, slider, text_input};
use iced::{Background, Border, Color, Font, Shadow, Theme, border, color, font};

/// Colours of the interface. The defaults are SlattyLauncher's own look.
#[derive(Debug, Clone)]
pub struct Tokens {
    pub background: Color,
    pub surface: Color,
    pub surface_high: Color,
    pub outline: Color,
    pub accent: Color,
    pub accent_hover: Color,
    /// Text and icons drawn on the accent colour.
    pub on_accent: Color,
    pub text: Color,
    pub muted: Color,
    pub success: Color,
    pub warning: Color,
    pub danger: Color,
    pub danger_hover: Color,
    /// Background of error notices.
    pub error_surface: Color,
    /// Darkens the page behind dialogs.
    pub scrim: Color,
    /// Corner radius of controls: tabs, fields, dropdowns, buttons of the new pages.
    pub radius: f32,
    /// Corner radius of game covers.
    pub cover_radius: f32,
}

impl Default for Tokens {
    fn default() -> Self {
        Tokens {
            background: color!(0x14131d),
            surface: color!(0x1e1c2a),
            surface_high: color!(0x2a2739),
            outline: color!(0x353146),
            accent: color!(0xc4b1fa),
            accent_hover: color!(0xd3c4fc),
            on_accent: color!(0x1b1530),
            text: color!(0xf2f0fa),
            muted: color!(0xa19db5),
            success: color!(0x4ade80),
            warning: color!(0xfbbf24),
            danger: color!(0xf87171),
            danger_hover: color!(0xfca5a5),
            error_surface: color!(0x3b1d24),
            scrim: Color {
                a: 0.7,
                ..color!(0x05040a)
            },
            radius: 8.0,
            cover_radius: 6.0,
        }
    }
}

static TOKENS: OnceLock<Tokens> = OnceLock::new();

/// The tokens in use, fixed for the life of the process.
pub fn tokens() -> &'static Tokens {
    TOKENS.get_or_init(Tokens::default)
}

pub const BOLD: Font = Font {
    weight: font::Weight::Bold,
    ..Font::DEFAULT
};
pub const SEMIBOLD: Font = Font {
    weight: font::Weight::Semibold,
    ..Font::DEFAULT
};

pub fn theme() -> Theme {
    Theme::custom(
        "Slatty",
        iced::theme::Palette {
            background: tokens().background,
            text: tokens().text,
            primary: tokens().accent,
            success: tokens().success,
            warning: tokens().warning,
            danger: tokens().danger,
        },
    )
}

fn round(radius: f32) -> Border {
    border::rounded(radius)
}

fn base(background: Option<Color>, text: Color, radius: f32) -> button::Style {
    button::Style {
        background: background.map(Background::Color),
        text_color: text,
        border: round(radius),
        shadow: Shadow::default(),
        snap: true,
    }
}

/// A tab of the top bar: filled with the accent colour when it is the open page.
pub fn nav_tab(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let t = tokens();
        match (active, status) {
            (true, _) => base(Some(t.accent), t.on_accent, t.radius),
            (false, button::Status::Hovered | button::Status::Pressed) => {
                base(Some(t.surface_high), t.text, t.radius)
            }
            (false, _) => base(None, t.text, t.radius),
        }
    }
}

/// A bare icon button that only shows a background when hovered.
pub fn ghost(_: &Theme, status: button::Status) -> button::Style {
    let t = tokens();
    match status {
        button::Status::Hovered | button::Status::Pressed => {
            base(Some(t.surface_high), t.text, t.radius)
        }
        _ => base(None, t.text, t.radius),
    }
}

/// The main call to action (Play, Install, Confirm).
pub fn primary(_: &Theme, status: button::Status) -> button::Style {
    match status {
        button::Status::Hovered | button::Status::Pressed => {
            base(Some(tokens().accent_hover), tokens().on_accent, 18.0)
        }
        button::Status::Disabled => base(Some(tokens().surface_high), tokens().muted, 18.0),
        button::Status::Active => base(Some(tokens().accent), tokens().on_accent, 18.0),
    }
}

pub fn danger(_: &Theme, status: button::Status) -> button::Style {
    match status {
        button::Status::Disabled => base(Some(tokens().surface_high), tokens().muted, 14.0),
        button::Status::Hovered | button::Status::Pressed => {
            base(Some(tokens().danger_hover), tokens().on_accent, 14.0)
        }
        button::Status::Active => base(Some(tokens().danger), tokens().on_accent, 14.0),
    }
}

/// Secondary actions: a filled surface that lightens on hover.
pub fn tonal(_: &Theme, status: button::Status) -> button::Style {
    let t = tokens();
    match status {
        button::Status::Disabled => base(Some(t.surface), t.outline, t.radius),
        button::Status::Hovered | button::Status::Pressed => {
            base(Some(t.outline), t.text, t.radius)
        }
        button::Status::Active => base(Some(t.surface_high), t.text, t.radius),
    }
}

/// A clickable card of a grid (a game on the Achievements tab).
pub fn tile(_: &Theme, status: button::Status) -> button::Style {
    let t = tokens();
    match status {
        button::Status::Hovered | button::Status::Pressed => {
            base(Some(t.surface_high), t.text, t.radius)
        }
        _ => base(Some(t.surface), t.text, t.radius),
    }
}

/// Round icon buttons of the headers.
pub fn icon_button(_: &Theme, status: button::Status) -> button::Style {
    match status {
        button::Status::Hovered | button::Status::Pressed => {
            base(Some(tokens().outline), tokens().text, 999.0)
        }
        _ => base(Some(tokens().surface_high), tokens().text, 999.0),
    }
}

/// Text links ("Manage →").
pub fn link(_: &Theme, status: button::Status) -> button::Style {
    base(
        None,
        match status {
            button::Status::Hovered | button::Status::Pressed => tokens().text,
            _ => tokens().accent,
        },
        8.0,
    )
}

/// Clickable areas that look like their content (cards, covers, rows).
pub fn plain(_: &Theme, status: button::Status) -> button::Style {
    match status {
        button::Status::Hovered | button::Status::Pressed => base(None, tokens().text, 16.0),
        _ => base(None, tokens().text, 16.0),
    }
}

pub fn row_button(_: &Theme, status: button::Status) -> button::Style {
    match status {
        button::Status::Hovered | button::Status::Pressed => {
            base(Some(tokens().surface_high), tokens().text, 16.0)
        }
        _ => base(Some(tokens().surface), tokens().text, 16.0),
    }
}

pub fn card(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(tokens().surface)),
        border: round(20.0),
        text_color: Some(tokens().text),
        ..Default::default()
    }
}

pub fn circle(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(tokens().surface_high)),
        border: round(999.0),
        ..Default::default()
    }
}

pub fn avatar(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(tokens().outline)),
        border: round(999.0),
        text_color: Some(tokens().text),
        ..Default::default()
    }
}

/// Title and buttons shown over a hovered cover.
pub fn cover_overlay(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color {
            a: 0.82,
            ..tokens().background
        })),
        border: border::rounded(border::bottom(tokens().cover_radius)),
        text_color: Some(tokens().text),
        ..Default::default()
    }
}

pub fn cover_frame(_: &Theme) -> container::Style {
    container::Style {
        border: round(tokens().cover_radius)
            .color(tokens().accent)
            .width(2.0),
        ..Default::default()
    }
}

pub fn placeholder(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(tokens().surface_high)),
        border: round(tokens().cover_radius),
        text_color: Some(tokens().muted),
        ..Default::default()
    }
}

pub fn backdrop(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(tokens().scrim)),
        ..Default::default()
    }
}

pub fn notice(error: bool) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(Background::Color(if error {
            tokens().error_surface
        } else {
            tokens().surface_high
        })),
        border: round(14.0),
        text_color: Some(tokens().text),
        ..Default::default()
    }
}

/// A plain block of a list (an achievement), with the controls' corner radius.
pub fn block(_: &Theme) -> container::Style {
    let t = tokens();
    container::Style {
        background: Some(Background::Color(t.surface)),
        border: round(t.radius),
        text_color: Some(t.text),
        ..Default::default()
    }
}

/// An outlined box around a field (the search box).
pub fn outlined(_: &Theme) -> container::Style {
    let t = tokens();
    container::Style {
        background: Some(Background::Color(t.surface)),
        border: round(t.radius).color(t.outline).width(1.0),
        ..Default::default()
    }
}

/// The dot on the avatar saying the account is signed in.
pub fn status_dot(_: &Theme) -> container::Style {
    let t = tokens();
    container::Style {
        background: Some(Background::Color(t.success)),
        border: round(999.0).color(t.background).width(2.0),
        ..Default::default()
    }
}

pub fn divider(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(tokens().outline)),
        ..Default::default()
    }
}

pub fn input(_: &Theme, _: text_input::Status) -> text_input::Style {
    text_input::Style {
        background: Background::Color(Color::TRANSPARENT),
        border: Border::default(),
        icon: tokens().muted,
        placeholder: tokens().muted,
        value: tokens().text,
        selection: Color {
            a: 0.4,
            ..tokens().accent
        },
    }
}

pub fn field(_: &Theme, status: text_input::Status) -> text_input::Style {
    text_input::Style {
        background: Background::Color(tokens().surface_high),
        border: round(12.0).width(1.0).color(match status {
            text_input::Status::Focused { .. } => tokens().accent,
            _ => tokens().outline,
        }),
        icon: tokens().muted,
        placeholder: tokens().muted,
        value: tokens().text,
        selection: Color {
            a: 0.4,
            ..tokens().accent
        },
    }
}

pub fn select(_: &Theme, status: pick_list::Status) -> pick_list::Style {
    pick_list::Style {
        text_color: tokens().text,
        placeholder_color: tokens().muted,
        handle_color: tokens().muted,
        background: Background::Color(match status {
            pick_list::Status::Hovered | pick_list::Status::Opened { .. } => tokens().surface_high,
            _ => tokens().surface,
        }),
        border: round(999.0).color(tokens().outline).width(1.0),
    }
}

/// An outlined drop-down list of the toolbars.
pub fn dropdown(_: &Theme, status: pick_list::Status) -> pick_list::Style {
    let t = tokens();
    pick_list::Style {
        text_color: t.text,
        placeholder_color: t.muted,
        handle_color: t.muted,
        background: Background::Color(t.background),
        border: round(t.radius).width(1.0).color(match status {
            pick_list::Status::Hovered | pick_list::Status::Opened { .. } => t.muted,
            _ => t.outline,
        }),
    }
}

pub fn progress(_: &Theme) -> progress_bar::Style {
    progress_bar::Style {
        background: Background::Color(tokens().surface_high),
        bar: Background::Color(tokens().accent),
        border: round(999.0),
    }
}

pub fn size_slider(_: &Theme, _: slider::Status) -> slider::Style {
    slider::Style {
        rail: slider::Rail {
            backgrounds: (
                Background::Color(tokens().accent),
                Background::Color(tokens().surface_high),
            ),
            width: 4.0,
            border: round(999.0),
        },
        handle: slider::Handle {
            shape: slider::HandleShape::Circle { radius: 8.0 },
            background: Background::Color(tokens().accent),
            border_width: 0.0,
            border_color: Color::TRANSPARENT,
        },
    }
}

pub fn scroller(theme: &Theme, status: scrollable::Status) -> scrollable::Style {
    let mut style = scrollable::default(theme, status);
    for rail in [&mut style.vertical_rail, &mut style.horizontal_rail] {
        rail.background = None;
        rail.border = round(999.0);
        rail.scroller.background = Background::Color(tokens().outline);
        rail.scroller.border = round(999.0);
    }
    style
}
