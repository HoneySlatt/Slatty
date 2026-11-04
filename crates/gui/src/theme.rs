//! The interface's look: every colour comes from [`Tokens`], and the widget styles below are built
//! from them only. A custom theme is another `Tokens`; views never name a colour themselves. The
//! state keeps the look (`look`); these are its tokens in Iced's types.

use std::sync::{Arc, LazyLock, RwLock};

use crate::look;

use iced::widget::{button, container, pick_list, progress_bar, scrollable, text_input};
use iced::{Background, Border, Color, Font, Shadow, Theme, border, font};

/// The look's colours, corner radii and motion, in Iced's colours.
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
    /// How far a new page rises as it comes into place (the motion's durations are the state's).
    pub transition_rise: f32,
}

impl Tokens {
    fn of(t: &look::Tokens) -> Self {
        let c = |c: look::Color| Color::from_rgba(c.r, c.g, c.b, c.a);
        Tokens {
            background: c(t.background),
            surface: c(t.surface),
            surface_high: c(t.surface_high),
            outline: c(t.outline),
            accent: c(t.accent),
            accent_hover: c(t.accent_hover),
            on_accent: c(t.on_accent),
            text: c(t.text),
            muted: c(t.muted),
            success: c(t.success),
            warning: c(t.warning),
            danger: c(t.danger),
            danger_hover: c(t.danger_hover),
            error_surface: c(t.error_surface),
            scrim: c(t.scrim),
            radius: t.radius,
            cover_radius: t.cover_radius,
            transition_rise: t.transition_rise,
        }
    }
}

/// The look's tokens last seen, and their copy in Iced's colours.
static TOKENS: LazyLock<RwLock<(Arc<look::Tokens>, Arc<Tokens>)>> = LazyLock::new(|| {
    let look = look::tokens();
    let tokens = Arc::new(Tokens::of(&look));
    RwLock::new((look, tokens))
});

/// The tokens in use, in Iced's colours; made again only once the look changed.
pub fn tokens() -> Arc<Tokens> {
    let look = look::tokens();
    {
        let seen = TOKENS.read().unwrap_or_else(|e| e.into_inner());
        if Arc::ptr_eq(&seen.0, &look) {
            return seen.1.clone();
        }
    }
    let tokens = Arc::new(Tokens::of(&look));
    *TOKENS.write().unwrap_or_else(|e| e.into_inner()) = (look, tokens.clone());
    tokens
}

/// Geist (SIL Open Font License, see assets/fonts/OFL.txt), built into the binary: the
/// interface looks the same whether or not the system has it.
pub const FONTS: [&[u8]; 3] = [
    include_bytes!("../assets/fonts/Geist-Regular.ttf"),
    include_bytes!("../assets/fonts/Geist-SemiBold.ttf"),
    include_bytes!("../assets/fonts/Geist-Bold.ttf"),
];

/// The font drawn with, and how many changes of the look's font it follows (`look::font_changes`).
static FONT: RwLock<(u64, Font)> = RwLock::new((0, Font::with_name(look::DEFAULT_FAMILY)));
/// The weight drawn for "semibold": many families only come in regular and bold. Unknown for a
/// chosen family until first drawn, so that the system's fonts are not read before the window.
static SEMIBOLD: RwLock<Option<font::Weight>> = RwLock::new(Some(font::Weight::Semibold));

/// The font family of the interface, chosen in Settings; Geist by default.
pub fn font() -> Font {
    let changes = look::font_changes();
    {
        let (seen, font) = *FONT.read().unwrap_or_else(|e| e.into_inner());
        if seen == changes {
            return font;
        }
    }
    let family = look::font_family();
    let font = match &family {
        // Font names live as long as the process; a few bytes per change of font.
        Some(name) => Font::with_name(Box::leak(name.clone().into_boxed_str())),
        None => Font::with_name(look::DEFAULT_FAMILY),
    };
    *FONT.write().unwrap_or_else(|e| e.into_inner()) = (changes, font);
    *SEMIBOLD.write().unwrap_or_else(|e| e.into_inner()) =
        family.is_none().then_some(font::Weight::Semibold);
    font
}

/// Whether the system has `family` in this weight (400 regular, 600 semibold, 700 bold).
fn has_weight(family: &str, weight: u16) -> bool {
    let mut system = iced::advanced::graphics::text::font_system()
        .write()
        .unwrap_or_else(|e| e.into_inner());
    system
        .raw()
        .db()
        .faces()
        .any(|f| f.weight.0 == weight && f.families.iter().any(|(name, _)| name == family))
}

pub fn bold() -> Font {
    Font {
        weight: font::Weight::Bold,
        ..font()
    }
}

pub fn semibold() -> Font {
    let font = font();
    let known = *SEMIBOLD.read().unwrap_or_else(|e| e.into_inner());
    let weight = known.unwrap_or_else(|| {
        let weight = match font.family {
            font::Family::Name(name) if !has_weight(name, 600) => font::Weight::Bold,
            _ => font::Weight::Semibold,
        };
        *SEMIBOLD.write().unwrap_or_else(|e| e.into_inner()) = Some(weight);
        weight
    });
    Font { weight, ..font }
}

/// Text in the interface's font: use it instead of `iced::widget::text`.
pub fn text<'a>(content: impl iced::widget::text::IntoFragment<'a>) -> iced::widget::Text<'a> {
    iced::widget::text(content).font(font())
}

/// Font families installed on the system, by name, as the interface can use them.
pub fn font_families() -> Vec<String> {
    let mut system = iced::advanced::graphics::text::font_system()
        .write()
        .unwrap_or_else(|e| e.into_inner());
    let mut names: Vec<String> = system
        .raw()
        .db()
        .faces()
        .filter_map(|f| f.families.first().map(|(name, _)| name.clone()))
        // Symbol and icon fonts have no letters to show.
        .filter(|n| !["Icons", "Emoji", "Symbol"].iter().any(|s| n.contains(s)))
        .collect();
    names.sort_by_key(|n| n.to_lowercase());
    names.dedup();
    names
}

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

/// An outlined button, filled when hovered (Check in the Cloud saves drawer).
pub fn action_row(_: &Theme, status: button::Status) -> button::Style {
    let t = tokens();
    let (background, text) = match status {
        button::Status::Hovered | button::Status::Pressed => (t.surface_high, t.text),
        button::Status::Disabled => (t.surface, t.muted),
        button::Status::Active => (t.surface, t.text),
    };
    button::Style {
        border: Border {
            color: t.outline,
            width: 1.0,
            radius: t.radius.into(),
        },
        ..base(Some(background), text, t.radius)
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
        button::Status::Hovered | button::Status::Pressed => base(
            Some(tokens().accent_hover),
            tokens().on_accent,
            tokens().radius,
        ),
        button::Status::Disabled => {
            base(Some(tokens().surface_high), tokens().muted, tokens().radius)
        }
        button::Status::Active => base(Some(tokens().accent), tokens().on_accent, tokens().radius),
    }
}

pub fn danger(_: &Theme, status: button::Status) -> button::Style {
    match status {
        button::Status::Disabled => {
            base(Some(tokens().surface_high), tokens().muted, tokens().radius)
        }
        button::Status::Hovered | button::Status::Pressed => base(
            Some(tokens().danger_hover),
            tokens().on_accent,
            tokens().radius,
        ),
        button::Status::Active => base(Some(tokens().danger), tokens().on_accent, tokens().radius),
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

/// An entry of a side list: tinted with the accent when it is the one shown.
pub fn side_entry(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let t = tokens();
        match (active, status) {
            (true, _) => base(
                Some(Color {
                    a: 0.14,
                    ..t.accent
                }),
                t.accent,
                t.radius,
            ),
            (false, button::Status::Hovered | button::Status::Pressed) => {
                base(Some(t.surface_high), t.text, t.radius)
            }
            (false, _) => base(None, t.text, t.radius),
        }
    }
}

/// The bar beside the entry of a side list that is shown.
pub fn side_marker(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(tokens().accent)),
        border: round(2.0),
        ..container::Style::default()
    }
}

/// An action that stands out without filling: outlined and labelled with the accent.
pub fn accent_outline(_: &Theme, status: button::Status) -> button::Style {
    let t = tokens();
    let fill = match status {
        button::Status::Hovered | button::Status::Pressed => Some(Color {
            a: 0.12,
            ..t.accent
        }),
        _ => None,
    };
    let mut style = base(fill, t.accent, t.radius);
    style.border = style.border.color(t.accent).width(1.0);
    style
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

pub fn avatar(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(tokens().outline)),
        border: round(999.0),
        text_color: Some(tokens().text),
        ..Default::default()
    }
}

/// Title and buttons shown over a hovered cover, `shown` from 0 (clear) to 1 (hovered).
pub fn cover_overlay(shown: f32) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(Background::Color(Color {
            a: 0.82 * shown,
            ..tokens().background
        })),
        border: border::rounded(border::bottom(tokens().cover_radius)),
        text_color: Some(tokens().text.scale_alpha(shown)),
        ..Default::default()
    }
}

pub fn cover_frame(shown: f32) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        border: round(tokens().cover_radius)
            .color(tokens().accent.scale_alpha(shown))
            .width(2.0),
        ..Default::default()
    }
}

/// `style` with every colour `shown` times as opaque: buttons fading in and out with what holds
/// them.
pub fn faded(
    style: fn(&Theme, button::Status) -> button::Style,
    shown: f32,
) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |theme, status| {
        let s = style(theme, status);
        button::Style {
            background: s.background.map(|b| b.scale_alpha(shown)),
            text_color: s.text_color.scale_alpha(shown),
            border: Border {
                color: s.border.color.scale_alpha(shown),
                ..s.border
            },
            shadow: Shadow {
                color: s.shadow.color.scale_alpha(shown),
                ..s.shadow
            },
            ..s
        }
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

/// Covers a page with the background colour, `amount` from 0 (clear) to 1 (hidden).
pub fn veil(amount: f32) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(Background::Color(tokens().background.scale_alpha(amount))),
        ..Default::default()
    }
}

/// A cover of the couch mode: ringed with the accent while it has the focus, `focused` from 0 to 1
/// as the focus comes and goes.
pub fn couch_card(focused: f32) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, _| {
        let t = tokens();
        button::Style {
            background: None,
            text_color: t.text,
            border: Border {
                color: t.accent.scale_alpha(focused),
                width: 3.0,
                radius: (t.cover_radius + 4.0).into(),
            },
            shadow: Shadow::default(),
            snap: true,
        }
    }
}

/// A button of the couch mode: filled with the accent while it has the focus.
pub fn couch_button(focused: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let t = tokens();
        match (focused, status) {
            // Something that cannot be done now (an update running) still shows the focus.
            (true, button::Status::Disabled) => {
                let mut style = base(Some(t.surface_high), t.muted, t.radius);
                style.border = style.border.color(t.accent).width(3.0);
                style
            }
            (true, _) => base(Some(t.accent), t.on_accent, t.radius),
            (false, button::Status::Disabled) => base(Some(t.surface_high), t.muted, t.radius),
            (false, button::Status::Hovered | button::Status::Pressed) => {
                base(Some(t.outline), t.text, t.radius)
            }
            (false, _) => base(Some(t.surface_high), t.text, t.radius),
        }
    }
}

/// A tab of the couch mode's top bar.
pub fn couch_tab(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, _| {
        let t = tokens();
        if active {
            base(Some(t.surface_high), t.text, t.radius)
        } else {
            base(None, t.muted, t.radius)
        }
    }
}

/// The couch mode's bars, above and below its pages.
pub fn couch_bar(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(tokens().background)),
        text_color: Some(tokens().text),
        ..Default::default()
    }
}

/// A row of a couch mode's list (a download, an achievement): ringed while it has the focus.
pub fn couch_item(focused: bool) -> impl Fn(&Theme) -> container::Style {
    move |_| {
        let t = tokens();
        container::Style {
            background: Some(Background::Color(t.surface)),
            border: round(20.0)
                .color(if focused {
                    t.accent
                } else {
                    Color::TRANSPARENT
                })
                .width(2.0),
            text_color: Some(t.text),
            ..Default::default()
        }
    }
}

/// A gamepad button, as the couch mode's hints draw it.
pub fn couch_glyph(_: &Theme) -> container::Style {
    let t = tokens();
    container::Style {
        background: Some(Background::Color(t.surface_high)),
        border: round(999.0).color(t.outline).width(1.0),
        text_color: Some(t.text),
        ..Default::default()
    }
}

/// The bars of a glyph drawn for the Menu button.
pub fn couch_glyph_bar(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(tokens().text)),
        border: round(1.0),
        ..Default::default()
    }
}

/// A panel over the couch mode's page: its menu, keyboard, achievements and questions.
pub fn couch_panel(_: &Theme) -> container::Style {
    let t = tokens();
    container::Style {
        background: Some(Background::Color(t.surface)),
        border: round(20.0).color(t.outline).width(1.0),
        text_color: Some(t.text),
        shadow: Shadow {
            color: Color {
                a: 0.5,
                ..Color::BLACK
            },
            offset: iced::Vector::new(0.0, 10.0),
            blur_radius: 30.0,
        },
        ..Default::default()
    }
}

pub fn backdrop(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(tokens().scrim)),
        ..Default::default()
    }
}

/// The backdrop fading in, `shown` from 0 (clear) to 1.
pub fn backdrop_shown(shown: f32) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(Background::Color(tokens().scrim.scale_alpha(shown))),
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

/// Darkens the bottom of a game's key art so the title and buttons over it stay readable.
pub fn hero_fade(_: &Theme) -> container::Style {
    let t = tokens();
    let clear = Color {
        a: 0.0,
        ..t.background
    };
    container::Style {
        background: Some(Background::Gradient(iced::Gradient::Linear(
            iced::gradient::Linear::new(iced::Radians(std::f32::consts::PI))
                .add_stop(0.0, clear)
                .add_stop(0.5, clear)
                .add_stop(
                    1.0,
                    Color {
                        a: 0.85,
                        ..t.background
                    },
                ),
        ))),
        ..Default::default()
    }
}

/// A panel docked to the right of the page.
pub fn drawer(_: &Theme) -> container::Style {
    let t = tokens();
    container::Style {
        background: Some(Background::Color(t.surface)),
        text_color: Some(t.text),
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

/// A small menu floating over the page.
pub fn menu(_: &Theme) -> container::Style {
    let t = tokens();
    container::Style {
        background: Some(Background::Color(t.surface)),
        border: round(t.radius).color(t.outline).width(1.0),
        text_color: Some(t.text),
        shadow: Shadow {
            color: Color {
                a: 0.4,
                ..Color::BLACK
            },
            offset: iced::Vector::new(0.0, 6.0),
            blur_radius: 18.0,
        },
        ..Default::default()
    }
}

/// A notification card floating in the window's corner, like a menu; lighter under the pointer.
pub fn toast(_: &Theme, status: button::Status) -> button::Style {
    let t = tokens();
    let fill = match status {
        button::Status::Hovered | button::Status::Pressed => t.surface_high,
        _ => t.surface,
    };
    button::Style {
        background: Some(Background::Color(fill)),
        text_color: t.text,
        border: round(t.radius).color(t.outline).width(1.0),
        shadow: Shadow {
            color: Color {
                a: 0.4,
                ..Color::BLACK
            },
            offset: iced::Vector::new(0.0, 6.0),
            blur_radius: 18.0,
        },
        snap: true,
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
        border: round(tokens().radius).width(1.0).color(match status {
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
        border: round(tokens().radius).color(tokens().outline).width(1.0),
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

/// The bar of a game whose achievements are all unlocked: the accent, brightening along it.
pub fn progress_lit(_: &Theme) -> progress_bar::Style {
    let t = tokens();
    progress_bar::Style {
        background: Background::Color(t.surface_high),
        bar: Background::Gradient(iced::Gradient::Linear(
            iced::gradient::Linear::new(iced::Radians(std::f32::consts::FRAC_PI_2))
                .add_stop(0.0, t.accent.scale_alpha(0.5))
                .add_stop(0.6, t.accent)
                .add_stop(1.0, t.accent_hover),
        )),
        border: round(999.0),
    }
}

/// A halo of the accent around what a game whose achievements are all unlocked shows.
fn halo(t: &Tokens) -> Shadow {
    Shadow {
        color: t.accent.scale_alpha(0.45),
        offset: iced::Vector::ZERO,
        blur_radius: 12.0,
    }
}

/// Around the lit bar: its halo.
pub fn glow(_: &Theme) -> container::Style {
    container::Style {
        border: round(999.0),
        shadow: halo(&tokens()),
        ..Default::default()
    }
}

/// Completed, said of a game whose achievements are all unlocked: the accent, tinted over the card
/// and ringed.
pub fn lit_badge(_: &Theme) -> container::Style {
    let t = tokens();
    container::Style {
        background: Some(Background::Color(t.accent.scale_alpha(0.14))),
        border: round(999.0).color(t.accent.scale_alpha(0.5)).width(1.0),
        text_color: Some(t.accent_hover),
        ..Default::default()
    }
}

/// A soft light behind the trophy of a game whose achievements are all unlocked.
pub fn trophy_glow(_: &Theme) -> container::Style {
    let t = tokens();
    container::Style {
        background: Some(Background::Color(t.accent.scale_alpha(0.3))),
        border: round(999.0),
        shadow: Shadow {
            color: t.accent.scale_alpha(0.6),
            offset: iced::Vector::ZERO,
            blur_radius: 18.0,
        },
        ..Default::default()
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The look applied, or a font chosen, by the state is what the interface draws with from the
    /// next view. The same values are applied as every test draws with.
    #[test]
    fn the_look_and_the_font_the_state_applies_are_drawn() {
        let before = tokens();
        look::apply(look::Tokens::default());
        let after = tokens();
        assert!(!Arc::ptr_eq(&before, &after), "made again");
        let accent = look::Tokens::default().accent;
        assert_eq!(
            after.accent,
            Color::from_rgba(accent.r, accent.g, accent.b, accent.a)
        );

        look::set_font(None);
        assert_eq!(font(), Font::with_name(look::DEFAULT_FAMILY));
    }
}
