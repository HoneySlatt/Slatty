//! The interface's look: every colour comes from [`Tokens`], and the widget styles below are built
//! from them only. A custom theme is another `Tokens`; views never name a colour themselves.

use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, RwLock};
use std::time::Duration;

use serde::Deserialize;

use iced::widget::{button, container, pick_list, progress_bar, scrollable, slider, text_input};
use iced::{Background, Border, Color, Font, Shadow, Theme, border, color, font};

/// Colours, corner radii and motion of the interface. The defaults are SlattyLauncher's own look.
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
    /// How long a new page takes to appear, and how far it rises meanwhile. Zero turns it off.
    pub transition: Duration,
    pub transition_rise: f32,
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
            transition: Duration::from_millis(180),
            transition_rise: 8.0,
        }
    }
}

static TOKENS: LazyLock<RwLock<Arc<Tokens>>> =
    LazyLock::new(|| RwLock::new(Arc::new(Tokens::default())));

/// The tokens in use: the defaults, or those of the theme file once loaded.
pub fn tokens() -> Arc<Tokens> {
    TOKENS.read().unwrap_or_else(|e| e.into_inner()).clone()
}

/// The theme file the interface reads at start and on Reload.
pub fn file(config_dir: &Path) -> PathBuf {
    config_dir.join("theme.toml")
}

/// Reads the theme file, if there is one, and uses it from the next frame. Keys left out keep
/// their default; a mistake keeps the look in use and is described.
pub fn load(path: &Path) -> Result<(), String> {
    let tokens = match std::fs::read_to_string(path) {
        Ok(text) => Tokens::from_toml(&text).map_err(|e| format!("{}: {e}", path.display()))?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Tokens::default(),
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    *TOKENS.write().unwrap_or_else(|e| e.into_inner()) = Arc::new(tokens);
    Ok(())
}

/// The theme file as written by the user: three tables, every key optional.
#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct ThemeFile {
    colors: ColorsFile,
    shape: ShapeFile,
    motion: MotionFile,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct ColorsFile {
    background: Option<Hex>,
    surface: Option<Hex>,
    surface_high: Option<Hex>,
    outline: Option<Hex>,
    accent: Option<Hex>,
    accent_hover: Option<Hex>,
    on_accent: Option<Hex>,
    text: Option<Hex>,
    muted: Option<Hex>,
    success: Option<Hex>,
    warning: Option<Hex>,
    danger: Option<Hex>,
    danger_hover: Option<Hex>,
    error_surface: Option<Hex>,
    scrim: Option<Hex>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct ShapeFile {
    radius: Option<f32>,
    cover_radius: Option<f32>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct MotionFile {
    transition_ms: Option<u64>,
    transition_rise: Option<f32>,
}

/// `#rrggbb`, or `#rrggbbaa` with transparency.
#[derive(Debug, Clone, Copy)]
struct Hex(Color);

impl<'de> Deserialize<'de> for Hex {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        parse_hex(&s).map(Hex).ok_or_else(|| {
            serde::de::Error::custom(format!("`{s}` is not a colour like #c4b1fa or #05040ab3"))
        })
    }
}

fn parse_hex(s: &str) -> Option<Color> {
    let digits = s.strip_prefix('#')?;
    if !matches!(digits.len(), 6 | 8) || !digits.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&digits[i..i + 2], 16).ok();
    let alpha = if digits.len() == 8 { byte(6)? } else { 255 };
    Some(Color::from_rgba8(
        byte(0)?,
        byte(2)?,
        byte(4)?,
        alpha as f32 / 255.0,
    ))
}

fn hex(c: Color) -> String {
    let [r, g, b, a] = c.into_rgba8();
    if a == 255 {
        format!("#{r:02x}{g:02x}{b:02x}")
    } else {
        format!("#{r:02x}{g:02x}{b:02x}{a:02x}")
    }
}

impl Tokens {
    /// The defaults, overridden by the keys the file sets.
    fn from_toml(text: &str) -> Result<Tokens, String> {
        let file: ThemeFile = toml::from_str(text).map_err(|e| e.message().to_string())?;
        let mut t = Tokens::default();
        let c = file.colors;
        for (slot, value) in [
            (&mut t.background, c.background),
            (&mut t.surface, c.surface),
            (&mut t.surface_high, c.surface_high),
            (&mut t.outline, c.outline),
            (&mut t.accent, c.accent),
            (&mut t.accent_hover, c.accent_hover),
            (&mut t.on_accent, c.on_accent),
            (&mut t.text, c.text),
            (&mut t.muted, c.muted),
            (&mut t.success, c.success),
            (&mut t.warning, c.warning),
            (&mut t.danger, c.danger),
            (&mut t.danger_hover, c.danger_hover),
            (&mut t.error_surface, c.error_surface),
            (&mut t.scrim, c.scrim),
        ] {
            if let Some(Hex(color)) = value {
                *slot = color;
            }
        }
        if let Some(r) = file.shape.radius {
            t.radius = r.max(0.0);
        }
        if let Some(r) = file.shape.cover_radius {
            t.cover_radius = r.max(0.0);
        }
        if let Some(ms) = file.motion.transition_ms {
            t.transition = Duration::from_millis(ms);
        }
        if let Some(rise) = file.motion.transition_rise {
            t.transition_rise = rise;
        }
        Ok(t)
    }

    /// The whole theme as a file, to start a custom one from.
    pub fn to_toml(&self) -> String {
        let colors = [
            ("background", self.background, "Window background"),
            ("surface", self.surface, "Cards, drawers"),
            (
                "surface_high",
                self.surface_high,
                "Fields, hovered controls",
            ),
            ("outline", self.outline, "Borders, dividers"),
            ("accent", self.accent, "Primary buttons, selection"),
            ("accent_hover", self.accent_hover, ""),
            ("on_accent", self.on_accent, "Text on the accent colour"),
            ("text", self.text, ""),
            ("muted", self.muted, "Secondary text"),
            ("success", self.success, ""),
            ("warning", self.warning, ""),
            ("danger", self.danger, ""),
            ("danger_hover", self.danger_hover, ""),
            (
                "error_surface",
                self.error_surface,
                "Background of error notices",
            ),
            (
                "scrim",
                self.scrim,
                "Darkens the page behind dialogs (#rrggbbaa)",
            ),
        ];
        let mut out = String::from(
            "# SlattyLauncher theme. Every key is optional: one left out keeps its default.\n\
             # Settings → Appearance → Reload applies changes without restarting.\n\n[colors]\n",
        );
        for (key, color, note) in colors {
            let line = format!("{key} = \"{}\"", hex(color));
            if note.is_empty() {
                out += &format!("{line}\n");
            } else {
                out += &format!("{line:<28}# {note}\n");
            }
        }
        let noted = |line: String, note: &str| format!("{line:<28}# {note}\n");
        out += "\n[shape]\n";
        out += &noted(format!("radius = {}", self.radius), "Corners of controls");
        out += &noted(
            format!("cover_radius = {}", self.cover_radius),
            "Corners of game covers",
        );
        out += "\n[motion]\n";
        out += &noted(
            format!("transition_ms = {}", self.transition.as_millis()),
            "Page transition; 0 turns it off",
        );
        out += &noted(
            format!("transition_rise = {}", self.transition_rise),
            "How far a new page rises, in pixels",
        );
        out
    }
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

/// A row of the Manage drawer: outlined, filled when hovered.
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

/// Covers a page with the background colour, `amount` from 0 (clear) to 1 (hidden).
pub fn veil(amount: f32) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(Background::Color(tokens().background.scale_alpha(amount))),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_theme_file_overrides_only_what_it_sets() {
        let t = Tokens::from_toml(
            "[colors]\naccent = \"#ff8800\"\nscrim = \"#00000080\"\n\n[shape]\nradius = 0\n\n\
             [motion]\ntransition_ms = 0\n",
        )
        .unwrap();
        assert_eq!(t.accent, Color::from_rgb8(0xff, 0x88, 0x00));
        assert!((t.scrim.a - 128.0 / 255.0).abs() < 1e-6);
        assert_eq!(t.radius, 0.0);
        assert_eq!(t.transition, Duration::ZERO);
        let d = Tokens::default();
        assert_eq!(
            (t.background, t.cover_radius),
            (d.background, d.cover_radius)
        );
    }

    #[test]
    fn mistakes_are_named() {
        let bad_colour = Tokens::from_toml("[colors]\naccent = \"orange\"\n").unwrap_err();
        assert!(
            bad_colour.contains("`orange` is not a colour"),
            "{bad_colour}"
        );
        let typo = Tokens::from_toml("[colors]\naccnet = \"#ffffff\"\n").unwrap_err();
        assert!(typo.contains("accnet"), "{typo}");
    }

    #[test]
    fn the_written_file_reads_back_as_the_defaults() {
        let d = Tokens::default();
        let t = Tokens::from_toml(&d.to_toml()).unwrap();
        for (a, b) in [
            (t.background, d.background),
            (t.accent, d.accent),
            (t.muted, d.muted),
            (t.scrim, d.scrim),
        ] {
            assert_eq!(a.into_rgba8(), b.into_rgba8());
        }
        assert_eq!(
            (t.radius, t.cover_radius, t.transition, t.transition_rise),
            (d.radius, d.cover_radius, d.transition, d.transition_rise)
        );
    }
}
