//! Colours and widget styles of the interface.

use iced::widget::{button, container, pick_list, progress_bar, scrollable, slider, text_input};
use iced::{Background, Border, Color, Font, Shadow, Theme, border, color, font};

pub const BACKGROUND: Color = color!(0x14131d);
pub const SURFACE: Color = color!(0x1e1c2a);
pub const SURFACE_HIGH: Color = color!(0x2a2739);
pub const OUTLINE: Color = color!(0x353146);
pub const ACCENT: Color = color!(0xc4b1fa);
pub const ON_ACCENT: Color = color!(0x1b1530);
pub const TEXT: Color = color!(0xf2f0fa);
pub const MUTED: Color = color!(0xa19db5);
pub const SUCCESS: Color = color!(0x4ade80);
pub const WARNING: Color = color!(0xfbbf24);
pub const DANGER: Color = color!(0xf87171);

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
            background: BACKGROUND,
            text: TEXT,
            primary: ACCENT,
            success: SUCCESS,
            warning: WARNING,
            danger: DANGER,
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

/// A tab or filter of a segmented bar.
pub fn segment(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| match (active, status) {
        (true, _) => base(Some(ACCENT), ON_ACCENT, 999.0),
        (false, button::Status::Hovered | button::Status::Pressed) => {
            base(Some(SURFACE_HIGH), TEXT, 999.0)
        }
        (false, _) => base(None, MUTED, 999.0),
    }
}

/// The main call to action (Play, Install, Confirm).
pub fn primary(_: &Theme, status: button::Status) -> button::Style {
    match status {
        button::Status::Hovered | button::Status::Pressed => {
            base(Some(color!(0xd3c4fc)), ON_ACCENT, 18.0)
        }
        button::Status::Disabled => base(Some(SURFACE_HIGH), MUTED, 18.0),
        button::Status::Active => base(Some(ACCENT), ON_ACCENT, 18.0),
    }
}

pub fn danger(_: &Theme, status: button::Status) -> button::Style {
    match status {
        button::Status::Disabled => base(Some(SURFACE_HIGH), MUTED, 14.0),
        button::Status::Hovered | button::Status::Pressed => {
            base(Some(color!(0xfca5a5)), ON_ACCENT, 14.0)
        }
        button::Status::Active => base(Some(DANGER), ON_ACCENT, 14.0),
    }
}

/// Secondary actions: a filled surface that lightens on hover.
pub fn tonal(_: &Theme, status: button::Status) -> button::Style {
    match status {
        button::Status::Disabled => base(Some(SURFACE), OUTLINE, 14.0),
        button::Status::Hovered | button::Status::Pressed => base(Some(OUTLINE), TEXT, 14.0),
        button::Status::Active => base(Some(SURFACE_HIGH), TEXT, 14.0),
    }
}

/// Round icon buttons of the headers.
pub fn icon_button(_: &Theme, status: button::Status) -> button::Style {
    match status {
        button::Status::Hovered | button::Status::Pressed => base(Some(OUTLINE), TEXT, 999.0),
        _ => base(Some(SURFACE_HIGH), TEXT, 999.0),
    }
}

/// Text links ("Manage →").
pub fn link(_: &Theme, status: button::Status) -> button::Style {
    base(
        None,
        match status {
            button::Status::Hovered | button::Status::Pressed => TEXT,
            _ => ACCENT,
        },
        8.0,
    )
}

/// Clickable areas that look like their content (cards, covers, rows).
pub fn plain(_: &Theme, status: button::Status) -> button::Style {
    match status {
        button::Status::Hovered | button::Status::Pressed => base(None, TEXT, 16.0),
        _ => base(None, TEXT, 16.0),
    }
}

pub fn row_button(_: &Theme, status: button::Status) -> button::Style {
    match status {
        button::Status::Hovered | button::Status::Pressed => base(Some(SURFACE_HIGH), TEXT, 16.0),
        _ => base(Some(SURFACE), TEXT, 16.0),
    }
}

pub fn card(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(SURFACE)),
        border: round(20.0),
        text_color: Some(TEXT),
        ..Default::default()
    }
}

pub fn pill(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(SURFACE)),
        border: round(999.0).color(OUTLINE).width(1.0),
        ..Default::default()
    }
}

pub fn circle(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(SURFACE_HIGH)),
        border: round(999.0),
        ..Default::default()
    }
}

pub fn avatar(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(OUTLINE)),
        border: round(999.0),
        text_color: Some(TEXT),
        ..Default::default()
    }
}

/// Title and buttons shown over a hovered cover.
pub fn cover_overlay(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color {
            a: 0.82,
            ..BACKGROUND
        })),
        border: border::rounded(border::bottom(12.0)),
        text_color: Some(TEXT),
        ..Default::default()
    }
}

pub fn cover_frame(_: &Theme) -> container::Style {
    container::Style {
        border: round(12.0).color(ACCENT).width(2.0),
        ..Default::default()
    }
}

pub fn placeholder(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(SURFACE_HIGH)),
        border: round(12.0),
        text_color: Some(MUTED),
        ..Default::default()
    }
}

pub fn backdrop(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color {
            a: 0.7,
            ..color!(0x05040a)
        })),
        ..Default::default()
    }
}

pub fn notice(error: bool) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(Background::Color(if error {
            color!(0x3b1d24)
        } else {
            SURFACE_HIGH
        })),
        border: round(14.0),
        text_color: Some(TEXT),
        ..Default::default()
    }
}

pub fn divider(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(OUTLINE)),
        ..Default::default()
    }
}

pub fn input(_: &Theme, _: text_input::Status) -> text_input::Style {
    text_input::Style {
        background: Background::Color(Color::TRANSPARENT),
        border: Border::default(),
        icon: MUTED,
        placeholder: MUTED,
        value: TEXT,
        selection: Color { a: 0.4, ..ACCENT },
    }
}

pub fn field(_: &Theme, status: text_input::Status) -> text_input::Style {
    text_input::Style {
        background: Background::Color(SURFACE_HIGH),
        border: round(12.0).width(1.0).color(match status {
            text_input::Status::Focused { .. } => ACCENT,
            _ => OUTLINE,
        }),
        icon: MUTED,
        placeholder: MUTED,
        value: TEXT,
        selection: Color { a: 0.4, ..ACCENT },
    }
}

pub fn select(_: &Theme, status: pick_list::Status) -> pick_list::Style {
    pick_list::Style {
        text_color: TEXT,
        placeholder_color: MUTED,
        handle_color: MUTED,
        background: Background::Color(match status {
            pick_list::Status::Hovered | pick_list::Status::Opened { .. } => SURFACE_HIGH,
            _ => SURFACE,
        }),
        border: round(999.0).color(OUTLINE).width(1.0),
    }
}

pub fn progress(_: &Theme) -> progress_bar::Style {
    progress_bar::Style {
        background: Background::Color(SURFACE_HIGH),
        bar: Background::Color(ACCENT),
        border: round(999.0),
    }
}

pub fn size_slider(_: &Theme, _: slider::Status) -> slider::Style {
    slider::Style {
        rail: slider::Rail {
            backgrounds: (Background::Color(ACCENT), Background::Color(SURFACE_HIGH)),
            width: 4.0,
            border: round(999.0),
        },
        handle: slider::Handle {
            shape: slider::HandleShape::Circle { radius: 8.0 },
            background: Background::Color(ACCENT),
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
        rail.scroller.background = Background::Color(OUTLINE);
        rail.scroller.border = round(999.0);
    }
    style
}
