//! The look as GPUI draws it: the state's colours (`look`) in GPUI's type, the interface's font,
//! and what each kind of button looks like ([`Look`]), from the tokens only, as `slatty-gui`'s
//! styles are: views never name a colour.

use std::borrow::Cow;
use std::sync::{Arc, LazyLock, OnceLock, RwLock};

use gpui::{App, Pixels, Rgba, SharedString, px, rgba};
use slatty_app::look;

/// The look's colours and corners the views draw with so far, in GPUI's types; the others join
/// as the views that need them come.
pub struct Tokens {
    pub background: Rgba,
    pub surface: Rgba,
    pub surface_high: Rgba,
    pub outline: Rgba,
    pub accent: Rgba,
    /// Text and icons drawn on the accent colour.
    pub on_accent: Rgba,
    pub text: Rgba,
    pub muted: Rgba,
    pub success: Rgba,
    /// Corner radius of controls: tabs, fields, dropdowns, buttons.
    pub radius: Pixels,
}

impl Tokens {
    fn of(t: &look::Tokens) -> Self {
        Tokens {
            background: color(t.background),
            surface: color(t.surface),
            surface_high: color(t.surface_high),
            outline: color(t.outline),
            accent: color(t.accent),
            on_accent: color(t.on_accent),
            text: color(t.text),
            muted: color(t.muted),
            success: color(t.success),
            radius: px(t.radius),
        }
    }
}

/// The look's tokens last seen, and their copy in GPUI's types.
static TOKENS: LazyLock<RwLock<(Arc<look::Tokens>, Arc<Tokens>)>> = LazyLock::new(|| {
    let look = look::tokens();
    let tokens = Arc::new(Tokens::of(&look));
    RwLock::new((look, tokens))
});

/// The tokens in use, in GPUI's types; made again only once the look changed.
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

/// What a button looks like at rest and under the pointer: `slatty-gui`'s button styles, whose
/// pressed state looks hovered.
#[derive(Clone, Copy)]
pub struct Look {
    pub fill: Option<Rgba>,
    pub text: Rgba,
    pub hover_fill: Option<Rgba>,
    pub hover_text: Rgba,
    pub radius: Pixels,
}

impl Look {
    fn base(fill: Option<Rgba>, text: Rgba, radius: Pixels) -> Self {
        Look {
            fill,
            text,
            hover_fill: fill,
            hover_text: text,
            radius,
        }
    }

    fn hovered(self, fill: Option<Rgba>, text: Rgba) -> Self {
        Look {
            hover_fill: fill,
            hover_text: text,
            ..self
        }
    }
}

/// A tab of the top bar: filled with the accent colour when it is the open page.
pub fn nav_tab(active: bool) -> Look {
    let t = tokens();
    if active {
        Look::base(Some(t.accent), t.on_accent, t.radius)
    } else {
        Look::base(None, t.text, t.radius).hovered(Some(t.surface_high), t.text)
    }
}

/// Clickable areas that look like their content (cards, covers, rows).
pub fn plain() -> Look {
    Look::base(None, tokens().text, px(16.))
}

/// Geist (SIL Open Font License, see crates/gui/assets/fonts/OFL.txt), built into the binary: the
/// interface looks the same whether or not the system has it.
const FONTS: [&[u8]; 3] = [
    include_bytes!("../../gui/assets/fonts/Geist-Regular.ttf"),
    include_bytes!("../../gui/assets/fonts/Geist-SemiBold.ttf"),
    include_bytes!("../../gui/assets/fonts/Geist-Bold.ttf"),
];

/// The system's font families, read once GPUI has them (`load_fonts`).
static FAMILIES: OnceLock<Vec<String>> = OnceLock::new();

/// Adds Geist to the fonts GPUI draws with, notes the system's families for Settings, and tells
/// the state how to list them.
pub fn load_fonts(cx: &mut App) {
    if let Err(e) = cx
        .text_system()
        .add_fonts(FONTS.iter().map(|font| Cow::Borrowed(*font)).collect())
    {
        tracing::warn!("Geist could not be loaded: {e:#}");
    }
    let _ = FAMILIES.set(families(cx.text_system().all_font_names()));
    look::list_fonts_with(font_families);
}

/// Font families installed on the system, by name, as the interface can use them.
fn font_families() -> Vec<String> {
    FAMILIES.get().cloned().unwrap_or_default()
}

/// The families of `names` a person may pick: sorted, each once.
fn families(names: Vec<String>) -> Vec<String> {
    let mut names: Vec<String> = names
        .into_iter()
        // GPUI's own aliases (".SystemUIFont"), and symbol and icon fonts, which have no letters
        // to show.
        .filter(|n| !n.starts_with('.'))
        .filter(|n| !["Icons", "Emoji", "Symbol"].iter().any(|s| n.contains(s)))
        .collect();
    names.sort_by_key(|n| n.to_lowercase());
    names.dedup();
    names
}

/// The font family of the interface, chosen in Settings; Geist by default.
pub fn font_family() -> SharedString {
    look::font_family()
        .unwrap_or_else(|| look::DEFAULT_FAMILY.into())
        .into()
}

/// A colour of the look, as GPUI draws it.
pub fn color(c: look::Color) -> gpui::Rgba {
    let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u32;
    rgba(byte(c.r) << 24 | byte(c.g) << 16 | byte(c.b) << 8 | byte(c.a))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_colour_keeps_its_channels() {
        let c = color(look::Color::from_rgba8(0x14, 0x13, 0x1d, 1.0));
        assert_eq!(c, rgba(0x14131dff));
        let half = color(look::Color::from_rgba8(255, 0, 0, 0.5));
        assert_eq!(half, rgba(0xff000080));
    }

    #[test]
    fn families_are_sorted_once_without_aliases_or_symbols() {
        let names = [
            "Noto Sans",
            ".SystemUIFont",
            "abel",
            "Noto Sans",
            "Noto Color Emoji",
        ];
        let families = families(names.iter().map(|n| n.to_string()).collect());
        assert_eq!(families, ["abel", "Noto Sans"]);
    }
}
