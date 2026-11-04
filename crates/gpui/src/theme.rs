//! The look as GPUI draws it: the state's colours (`look`) in GPUI's type, the interface's font.

use std::borrow::Cow;
use std::sync::OnceLock;

use gpui::{App, SharedString, rgba};
use slatty_app::look;

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
