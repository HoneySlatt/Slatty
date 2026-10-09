//! Built-in themes, picked in Settings. Each is a full set of colours; the theme file can still
//! change any of them on top.

use std::fmt;

use iced::{Color, color};

use crate::theme::Tokens;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Preset {
    #[default]
    Slatty,
    Carbonfox,
    Everforest,
    RosePine,
    PastelGlow,
    GruvboxDark,
    GruvboxLight,
}

impl Preset {
    pub const ALL: [Preset; 7] = [
        Preset::Slatty,
        Preset::Carbonfox,
        Preset::Everforest,
        Preset::RosePine,
        Preset::PastelGlow,
        Preset::GruvboxDark,
        Preset::GruvboxLight,
    ];

    /// How it is stored in the settings.
    pub fn key(self) -> &'static str {
        match self {
            Preset::Slatty => "slatty",
            Preset::Carbonfox => "carbonfox",
            Preset::Everforest => "everforest",
            Preset::RosePine => "rose-pine",
            Preset::PastelGlow => "pastel-glow",
            Preset::GruvboxDark => "gruvbox-dark",
            Preset::GruvboxLight => "gruvbox-light",
        }
    }

    pub fn from_key(key: &str) -> Option<Preset> {
        Preset::ALL.into_iter().find(|p| p.key() == key)
    }

    /// Its colours; corners and motion stay SlattyLauncher's.
    pub fn tokens(self) -> Tokens {
        let base = Tokens::default();
        let scrim = |rgb: Color, a: f32| Color { a, ..rgb };
        match self {
            Preset::Slatty => base,
            // Nightfox's Carbonfox, after IBM's Carbon palette.
            Preset::Carbonfox => Tokens {
                background: color!(0x161616),
                surface: color!(0x1e1e1e),
                surface_high: color!(0x282828),
                outline: color!(0x353535),
                accent: color!(0x78a9ff),
                accent_hover: color!(0x99bdff),
                on_accent: color!(0x0c0c0c),
                text: color!(0xf2f4f8),
                muted: color!(0xb6b8bb),
                success: color!(0x25be6a),
                warning: color!(0x08bdba),
                danger: color!(0xee5396),
                danger_hover: color!(0xff7eb6),
                error_surface: color!(0x3a1a28),
                scrim: scrim(color!(0x0c0c0c), 0.7),
                ..base
            },
            // Everforest, dark, medium contrast.
            Preset::Everforest => Tokens {
                background: color!(0x2d353b),
                surface: color!(0x343f44),
                surface_high: color!(0x3d484d),
                outline: color!(0x475258),
                accent: color!(0xa7c080),
                accent_hover: color!(0xbbd196),
                on_accent: color!(0x232a2e),
                text: color!(0xd3c6aa),
                muted: color!(0x9da9a0),
                success: color!(0x83c092),
                warning: color!(0xdbbc7f),
                danger: color!(0xe67e80),
                danger_hover: color!(0xee9a9c),
                error_surface: color!(0x4c3743),
                scrim: scrim(color!(0x1e2326), 0.7),
                ..base
            },
            // Rosé Pine, main variant.
            Preset::RosePine => Tokens {
                background: color!(0x191724),
                surface: color!(0x1f1d2e),
                surface_high: color!(0x26233a),
                outline: color!(0x403d52),
                accent: color!(0xc4a7e7),
                accent_hover: color!(0xd3bdee),
                on_accent: color!(0x191724),
                text: color!(0xe0def4),
                muted: color!(0x908caa),
                success: color!(0x9ccfd8),
                warning: color!(0xf6c177),
                danger: color!(0xeb6f92),
                danger_hover: color!(0xf08aa7),
                error_surface: color!(0x3b2233),
                scrim: scrim(color!(0x111019), 0.7),
                ..base
            },
            // SlattyLauncher's own: soft pastels glowing on a deep violet night.
            Preset::PastelGlow => Tokens {
                background: color!(0x17151f),
                surface: color!(0x211e2b),
                surface_high: color!(0x2c2838),
                outline: color!(0x3b3549),
                accent: color!(0xffb3d9),
                accent_hover: color!(0xffc8e3),
                on_accent: color!(0x2b1625),
                text: color!(0xf5f0ff),
                muted: color!(0xb4a9c9),
                success: color!(0xa8e6cf),
                warning: color!(0xffd3a5),
                danger: color!(0xff9aa2),
                danger_hover: color!(0xffb7bd),
                error_surface: color!(0x3a1f29),
                scrim: scrim(color!(0x0d0b12), 0.7),
                ..base
            },
            // Gruvbox, dark.
            Preset::GruvboxDark => Tokens {
                background: color!(0x282828),
                surface: color!(0x32302f),
                surface_high: color!(0x3c3836),
                outline: color!(0x504945),
                accent: color!(0xfabd2f),
                accent_hover: color!(0xfcd060),
                on_accent: color!(0x282828),
                text: color!(0xebdbb2),
                muted: color!(0xa89984),
                success: color!(0xb8bb26),
                warning: color!(0xfe8019),
                danger: color!(0xfb4934),
                danger_hover: color!(0xfc6d5d),
                error_surface: color!(0x4a2423),
                scrim: scrim(color!(0x1d2021), 0.7),
                ..base
            },
            // Gruvbox, light.
            Preset::GruvboxLight => Tokens {
                background: color!(0xfbf1c7),
                surface: color!(0xf2e5bc),
                surface_high: color!(0xebdbb2),
                outline: color!(0xd5c4a1),
                accent: color!(0xaf3a03),
                accent_hover: color!(0xc5520f),
                on_accent: color!(0xfbf1c7),
                text: color!(0x3c3836),
                muted: color!(0x7c6f64),
                success: color!(0x79740e),
                warning: color!(0xb57614),
                danger: color!(0x9d0006),
                danger_hover: color!(0xcc241d),
                error_surface: color!(0xf3d3c3),
                scrim: scrim(color!(0x3c3836), 0.45),
                ..base
            },
        }
    }
}

impl fmt::Display for Preset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Preset::Slatty => "Slatty (default)",
            Preset::Carbonfox => "Carbonfox",
            Preset::Everforest => "Everforest",
            Preset::RosePine => "Rosé Pine",
            Preset::PastelGlow => "Pastel Glow",
            Preset::GruvboxDark => "Gruvbox Dark",
            Preset::GruvboxLight => "Gruvbox Light",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_preset_is_stored_by_a_key_and_reads_well() {
        for p in Preset::ALL {
            assert_eq!(Preset::from_key(p.key()), Some(p));
            let t = p.tokens();
            // Text and the text on buttons stand out from what they are drawn on.
            assert!(contrast(t.text, t.background) >= 7.0, "{p}: text");
            assert!(contrast(t.on_accent, t.accent) >= 4.5, "{p}: on accent");
            assert!(contrast(t.muted, t.surface) >= 3.0, "{p}: muted");
        }
        assert_eq!(Preset::from_key("unknown"), None);
    }

    /// WCAG contrast ratio of two colours.
    fn contrast(a: Color, b: Color) -> f32 {
        let luminance = |c: Color| {
            let channel = |v: f32| {
                if v <= 0.03928 {
                    v / 12.92
                } else {
                    ((v + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * channel(c.r) + 0.7152 * channel(c.g) + 0.0722 * channel(c.b)
        };
        let (la, lb) = (luminance(a), luminance(b));
        (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
    }
}
