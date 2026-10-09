//! Settings page actions: default installation path and default Proton.

use std::fmt;
use std::path::PathBuf;

use iced::Task;
use slatty_core::settings;

use crate::{App, Message, theme};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtonChoice(pub PathBuf);

impl fmt::Display for ProtonChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = self
            .0
            .file_name()
            .map(|n| n.to_string_lossy())
            .unwrap_or_default();
        f.write_str(&name)
    }
}

/// A font family to pick, or the system's default.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FontChoice {
    Default,
    Family(String),
}

impl fmt::Display for FontChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FontChoice::Default => write!(f, "{} (default)", crate::theme::DEFAULT_FAMILY),
            FontChoice::Family(name) => f.write_str(name),
        }
    }
}

#[derive(Debug, Clone)]
pub enum SettingsMsg {
    RootInput(String),
    /// Enter in the field, or a folder chosen with Browse.
    SaveRoot,
    BrowseRoot,
    BrowsedRoot(Option<PathBuf>),
    Proton(ProtonChoice),
    /// Writes the default theme to the theme file, to start a custom one from.
    CreateTheme,
    /// Opens the theme file in the desktop's editor.
    EditTheme,
    ReloadTheme,
    /// The interface font, from the families installed on the system.
    Font(FontChoice),
    /// A built-in theme, under the theme file.
    Preset(crate::presets::Preset),
    /// Proton build of one installed game, used from its next launch.
    GameProton(String, ProtonChoice),
}

impl App {
    pub fn update_settings(&mut self, msg: SettingsMsg) -> Task<Message> {
        let Some(core) = self.core.clone() else {
            return Task::none();
        };
        match msg {
            // Kept as typed, without a Save button: an absolute path is saved quietly, and Enter
            // says what is wrong with any other.
            SettingsMsg::RootInput(v) => {
                let path = PathBuf::from(v.trim());
                self.library_root = v;
                if path.is_absolute() {
                    let _ = settings::set_library_root(&core.db, &path);
                }
            }
            SettingsMsg::SaveRoot => {
                let path = PathBuf::from(self.library_root.trim());
                if !path.is_absolute() {
                    self.notify_error("The default installation path must be absolute.".into());
                } else if let Err(e) = settings::set_library_root(&core.db, &path) {
                    self.notify_error(e.to_string());
                }
            }
            SettingsMsg::BrowseRoot => {
                let start = PathBuf::from(self.library_root.trim());
                return Task::perform(
                    async move {
                        let mut dialog =
                            rfd::AsyncFileDialog::new().set_title("Default installation path");
                        if start.is_dir() {
                            dialog = dialog.set_directory(&start);
                        }
                        dialog.pick_folder().await.map(|f| f.path().to_path_buf())
                    },
                    |picked| Message::Settings(SettingsMsg::BrowsedRoot(picked)),
                );
            }
            SettingsMsg::BrowsedRoot(Some(folder)) => {
                self.library_root = folder.display().to_string();
                return self.update_settings(SettingsMsg::SaveRoot);
            }
            SettingsMsg::BrowsedRoot(None) => {}
            SettingsMsg::Proton(choice) => {
                match settings::set_default_proton(&core.db, &choice.0) {
                    Ok(()) => self.proton = Some(choice.0),
                    Err(e) => self.notify_error(e.to_string()),
                }
            }
            SettingsMsg::CreateTheme => {
                let path = theme::file(&core.dirs.config);
                if !path.exists() {
                    let written = slatty_core::fsutil::write_atomic(
                        &path,
                        theme::tokens().to_toml().as_bytes(),
                    );
                    if let Err(e) = written {
                        self.notify_error(e.to_string());
                    }
                }
            }
            SettingsMsg::EditTheme => {
                let opened = std::process::Command::new("xdg-open")
                    .arg(theme::file(&core.dirs.config))
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn();
                if let Err(e) = opened {
                    self.notify_error(format!("Could not open the theme file: {e}"));
                }
            }
            SettingsMsg::Preset(preset) => {
                if let Err(e) = settings::set_interface_theme(&core.db, preset.key()) {
                    self.notify_error(e.to_string());
                }
                theme::set_preset(preset);
                if let Err(e) = theme::load(&theme::file(&core.dirs.config)) {
                    self.notify_error(format!("Theme file not used: {e}"));
                }
            }
            SettingsMsg::Font(choice) => {
                let family = match &choice {
                    FontChoice::Default => None,
                    FontChoice::Family(name) => Some(name.as_str()),
                };
                match settings::set_interface_font(&core.db, family) {
                    Ok(()) => theme::set_font(family),
                    Err(e) => self.notify_error(e.to_string()),
                }
            }
            SettingsMsg::ReloadTheme => match theme::load(&theme::file(&core.dirs.config)) {
                Ok(()) => {
                    self.notice = Some(crate::Notice {
                        error: false,
                        text: "Theme reloaded.".into(),
                    });
                }
                Err(e) => self.notify_error(format!("Theme file not used: {e}")),
            },
            SettingsMsg::GameProton(game_id, choice) => {
                match slatty_core::install::set_proton(&core.db, &game_id, &choice.0) {
                    Ok(install) => {
                        self.installs.insert(game_id, install);
                    }
                    Err(e) => self.notify_error(e.to_string()),
                }
            }
        }
        Task::none()
    }
}
