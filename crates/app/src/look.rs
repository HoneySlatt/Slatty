//! The interface's look as the state keeps it: colours, corners and motion ([`Tokens`]), the
//! built-in theme picked with the theme file over it, and the font family chosen. The interface
//! draws with them (`theme`), in its framework's types.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, OnceLock, RwLock};
use std::time::Duration;

use serde::Deserialize;
use slatty_core::db::Db;
use slatty_core::paths::Dirs;

use crate::presets::Preset;

/// A colour, its channels from 0 to 1 as written (sRGB), alpha included.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const fn from_rgba8(r: u8, g: u8, b: u8, a: f32) -> Color {
        Color {
            r: r as f32 / 255.0,
            g: g as f32 / 255.0,
            b: b as f32 / 255.0,
            a,
        }
    }

    pub const fn from_rgb8(r: u8, g: u8, b: u8) -> Color {
        Color::from_rgba8(r, g, b, 1.0)
    }

    /// `0xrrggbb`, opaque.
    pub const fn from_hex(hex: u32) -> Color {
        Color::from_rgb8((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
    }

    pub fn into_rgba8(self) -> [u8; 4] {
        [
            (self.r * 255.0).round() as u8,
            (self.g * 255.0).round() as u8,
            (self.b * 255.0).round() as u8,
            (self.a * 255.0).round() as u8,
        ]
    }

    pub fn scale_alpha(self, factor: f32) -> Color {
        Color {
            a: self.a * factor,
            ..self
        }
    }
}

/// A colour written `color!(0xrrggbb)`, opaque.
macro_rules! color {
    ($hex:literal) => {
        $crate::look::Color::from_hex($hex)
    };
}
pub(crate) use color;

/// Colours, corner radii and motion of the interface. The defaults are Slatty's own look.
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
    /// How long a new page, a drawer or a dialog takes to come into place, and how far a page rises
    /// meanwhile. Zero turns it off.
    pub transition: Duration,
    pub transition_rise: f32,
    /// How long what the pointer enters takes to show hovered, and to go back once left. Zero
    /// turns it off.
    pub hover: Duration,
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
            hover: Duration::from_millis(160),
        }
    }
}

static TOKENS: LazyLock<RwLock<Arc<Tokens>>> =
    LazyLock::new(|| RwLock::new(Arc::new(Tokens::default())));

/// The tokens in use: the defaults, or those of the theme file once loaded. A new `Arc` once
/// others are applied.
pub fn tokens() -> Arc<Tokens> {
    TOKENS.read().unwrap_or_else(|e| e.into_inner()).clone()
}

/// The theme file the interface reads at start and on Reload.
pub fn file(config_dir: &Path) -> PathBuf {
    config_dir.join("theme.toml")
}

static PRESET: RwLock<Preset> = RwLock::new(Preset::Slatty);

/// The built-in theme chosen in Settings, under the theme file.
pub fn preset() -> Preset {
    *PRESET.read().unwrap_or_else(|e| e.into_inner())
}

pub fn set_preset(preset: Preset) {
    *PRESET.write().unwrap_or_else(|e| e.into_inner()) = preset;
}

/// Reads the theme file again, if there is one, over the chosen preset, and uses the result from
/// the next frame. Keys left out keep the preset's value; a mistake keeps the look in use and is
/// described.
pub fn load(path: &Path) -> Result<(), String> {
    apply(read(path, preset().tokens())?);
    Ok(())
}

/// The look to start with, or once `preset` is picked: the theme file over it, or `preset`
/// alone when the file has a mistake, which is described.
pub fn with_preset(path: &Path, preset: Preset) -> (Tokens, Option<String>) {
    match read(path, preset.tokens()) {
        Ok(tokens) => (tokens, None),
        Err(e) => (preset.tokens(), Some(e)),
    }
}

/// The look the person saved, its built-in theme and font read from `db`, used from the first
/// frame, so that the window never shows another look first. A mistake in the theme file is kept
/// for the page to show (`THEME_ERROR`).
pub fn apply_saved(dirs: &Dirs, db: Option<&Db>) {
    let saved = |read: fn(&Db) -> slatty_core::Result<Option<String>>| {
        db.and_then(|db| read(db).ok().flatten())
    };
    if let Some(preset) =
        saved(slatty_core::settings::interface_theme).and_then(|name| Preset::from_key(&name))
    {
        set_preset(preset);
    }
    let (tokens, mistake) = with_preset(&file(&dirs.config), preset());
    apply(tokens);
    if let Some(e) = mistake {
        let _ = crate::THEME_ERROR.set(e);
    }
    set_font(saved(slatty_core::settings::interface_font).as_deref());
}

/// Uses `tokens` from the next frame.
pub fn apply(tokens: Tokens) {
    *TOKENS.write().unwrap_or_else(|e| e.into_inner()) = Arc::new(tokens);
}

/// The theme file over `base`, or `base` without one.
fn read(path: &Path, base: Tokens) -> Result<Tokens, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => Tokens::from_toml(&text, base).map_err(|e| format!("{}: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(base),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
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
    hover_ms: Option<u64>,
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

/// The longest page transition a theme may ask for: a longer one would leave pages blank for as
/// long.
const MAX_TRANSITION_MS: u64 = 10_000;

impl Tokens {
    /// `base`, overridden by the keys the file sets.
    fn from_toml(text: &str, base: Tokens) -> Result<Tokens, String> {
        let file: ThemeFile = toml::from_str(text).map_err(|e| e.message().to_string())?;
        let mut t = base;
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
        // TOML takes `nan` and `inf`: either in the motion would leave every page blank.
        let number = |key: &str, value: f32| {
            value
                .is_finite()
                .then_some(value)
                .ok_or_else(|| format!("`{key}` must be a number"))
        };
        if let Some(r) = file.shape.radius {
            t.radius = number("radius", r)?.max(0.0);
        }
        if let Some(r) = file.shape.cover_radius {
            t.cover_radius = number("cover_radius", r)?.max(0.0);
        }
        if let Some(ms) = file.motion.transition_ms {
            if ms > MAX_TRANSITION_MS {
                return Err(format!(
                    "`transition_ms` must be {MAX_TRANSITION_MS} or less"
                ));
            }
            t.transition = Duration::from_millis(ms);
        }
        if let Some(rise) = file.motion.transition_rise {
            t.transition_rise = number("transition_rise", rise)?;
        }
        if let Some(ms) = file.motion.hover_ms {
            if ms > MAX_TRANSITION_MS {
                return Err(format!("`hover_ms` must be {MAX_TRANSITION_MS} or less"));
            }
            t.hover = Duration::from_millis(ms);
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
            "# Slatty theme. Every key is optional: one left out keeps its default.\n\
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
            "Pages, drawers, dialogs; 0 turns it off",
        );
        out += &noted(
            format!("transition_rise = {}", self.transition_rise),
            "How far a new page rises, in pixels",
        );
        out += &noted(
            format!("hover_ms = {}", self.hover.as_millis()),
            "Covers hovered or focused; 0 turns it off",
        );
        out
    }
}

/// Geist, the font built into Slatty: the interface looks the same whether or not the system has
/// it.
pub const DEFAULT_FAMILY: &str = "Geist";

static FAMILY: RwLock<Option<String>> = RwLock::new(None);
/// Counts the changes of font, for the interface to see one.
static FAMILY_CHANGES: AtomicU64 = AtomicU64::new(0);

/// The font family chosen in Settings; `None` for Geist.
pub fn font_family() -> Option<String> {
    FAMILY.read().unwrap_or_else(|e| e.into_inner()).clone()
}

/// How many times the font changed: the interface draws with the new one once it sees more.
pub fn font_changes() -> u64 {
    FAMILY_CHANGES.load(Ordering::Acquire)
}

/// Uses `family` from the next frame, or Geist with `None`.
pub fn set_font(family: Option<&str>) {
    *FAMILY.write().unwrap_or_else(|e| e.into_inner()) = family.map(String::from);
    FAMILY_CHANGES.fetch_add(1, Ordering::AcqRel);
}

/// How the interface lists the system's font families; set once, as it starts.
static FONT_LISTER: OnceLock<fn() -> Vec<String>> = OnceLock::new();

pub fn list_fonts_with(lister: fn() -> Vec<String>) {
    let _ = FONT_LISTER.set(lister);
}

/// Font families installed on the system, by name, as the interface can use them; none before the
/// interface said how to list them.
pub fn font_families() -> Vec<String> {
    FONT_LISTER.get().map_or_else(Vec::new, |list| list())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The colours are read as Iced reads them, to the bit: the look stays the same.
    #[test]
    fn colours_are_read_as_iceds() {
        let c = color!(0xc4b1fa);
        assert_eq!(c.into_rgba8(), [0xc4, 0xb1, 0xfa, 0xff]);
        assert_eq!(c, Color::from_rgb8(0xc4, 0xb1, 0xfa));
        assert_eq!(
            (c.r, c.g, c.b),
            (196.0 / 255.0, 177.0 / 255.0, 250.0 / 255.0)
        );
        assert_eq!(c.scale_alpha(0.5).a, 0.5);
    }

    #[test]
    fn a_theme_file_overrides_only_what_it_sets() {
        let t = Tokens::from_toml(
            "[colors]\naccent = \"#ff8800\"\nscrim = \"#00000080\"\n\n[shape]\nradius = 0\n\n\
             [motion]\ntransition_ms = 0\nhover_ms = 0\n",
            Tokens::default(),
        )
        .unwrap();
        assert_eq!(t.hover, Duration::ZERO);
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

    /// A theme file with a mistake leaves the built-in theme picked in use, not Slatty's
    /// default look, at start and when another one is picked.
    #[test]
    fn a_theme_file_with_a_mistake_leaves_the_picked_theme() {
        let dir = std::env::temp_dir().join(format!("slatty-theme-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("theme.toml");
        let picked = Preset::Carbonfox;
        assert_ne!(picked.tokens().background, Tokens::default().background);

        let (tokens, mistake) = with_preset(&file, picked);
        assert_eq!(tokens.background, picked.tokens().background, "no file");
        assert!(mistake.is_none());

        std::fs::write(&file, "[colors]\naccent = \"orange\"\n").unwrap();
        let (tokens, mistake) = with_preset(&file, picked);
        assert_eq!(tokens.background, picked.tokens().background);
        assert!(mistake.unwrap().contains("orange"));

        std::fs::write(&file, "[shape]\nradius = 0\n").unwrap();
        let (tokens, _) = with_preset(&file, picked);
        assert_eq!(
            (tokens.background, tokens.radius),
            (picked.tokens().background, 0.0)
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn mistakes_are_named() {
        let bad_colour =
            Tokens::from_toml("[colors]\naccent = \"orange\"\n", Tokens::default()).unwrap_err();
        assert!(
            bad_colour.contains("`orange` is not a colour"),
            "{bad_colour}"
        );
        let typo =
            Tokens::from_toml("[colors]\naccnet = \"#ffffff\"\n", Tokens::default()).unwrap_err();
        assert!(typo.contains("accnet"), "{typo}");
    }

    /// TOML takes `nan` and `inf` for numbers: either in the motion left every page blank,
    /// Settings included, as would a page that takes years to appear.
    #[test]
    fn numbers_that_would_blank_the_pages_are_named() {
        for (line, key) in [
            ("[motion]\ntransition_rise = nan\n", "transition_rise"),
            ("[motion]\ntransition_rise = inf\n", "transition_rise"),
            ("[shape]\nradius = inf\n", "radius"),
            ("[shape]\ncover_radius = nan\n", "cover_radius"),
            ("[motion]\ntransition_ms = 99999999\n", "transition_ms"),
            ("[motion]\nhover_ms = 99999999\n", "hover_ms"),
        ] {
            let e = Tokens::from_toml(line, Tokens::default()).unwrap_err();
            assert!(e.contains(key), "{line}: {e}");
        }
    }

    #[test]
    fn the_written_file_reads_back_as_the_defaults() {
        let d = Tokens::default();
        let t = Tokens::from_toml(&d.to_toml(), Tokens::default()).unwrap();
        for (a, b) in [
            (t.background, d.background),
            (t.accent, d.accent),
            (t.muted, d.muted),
            (t.scrim, d.scrim),
        ] {
            assert_eq!(a.into_rgba8(), b.into_rgba8());
        }
        assert_eq!(
            (
                t.radius,
                t.cover_radius,
                t.transition,
                t.transition_rise,
                t.hover
            ),
            (
                d.radius,
                d.cover_radius,
                d.transition,
                d.transition_rise,
                d.hover
            )
        );
    }
}
