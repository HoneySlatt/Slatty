//! The pictures built into the binary, as GPUI reads them: Lucide's icons, by their file's name,
//! and Slatty's emblem.

use std::borrow::Cow;

use gpui::{AssetSource, SharedString};
use slatty_app::icons::Icon;

pub struct Assets;

/// The path an icon is drawn from: its name, which lives as long as the program, so drawing it
/// allocates nothing.
pub fn icon_path(icon: Icon) -> SharedString {
    SharedString::new_static(icon.name())
}

/// The emblem's path; no icon has that name.
pub const LOGO: &str = "slatty-logo";

impl AssetSource for Assets {
    fn load(&self, path: &str) -> gpui::Result<Option<Cow<'static, [u8]>>> {
        if path == LOGO {
            return Ok(Some(Cow::Borrowed(include_bytes!(
                "../../gui/assets/logo.svg"
            ))));
        }
        Ok(Icon::ALL
            .iter()
            .find(|i| i.name() == path)
            .map(|icon| Cow::Borrowed(icon.svg())))
    }

    fn list(&self, _: &str) -> gpui::Result<Vec<SharedString>> {
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_and_the_logo_load_from_their_paths() {
        for icon in Icon::ALL {
            let svg = Assets.load(&icon_path(icon)).unwrap().unwrap();
            assert_eq!(&*svg, icon.svg());
        }
        assert!(Assets.load(LOGO).unwrap().is_some());
        assert!(Assets.load("nothing").unwrap().is_none());
    }
}
