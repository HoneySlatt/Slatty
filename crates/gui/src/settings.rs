//! Settings page actions: default installation path and default Proton.

use std::fmt;
use std::path::PathBuf;

use iced::Task;
use slatty_core::settings;

use crate::{App, Message};

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

#[derive(Debug, Clone)]
pub enum SettingsMsg {
    RootInput(String),
    SaveRoot,
    Proton(ProtonChoice),
    /// Proton build of one installed game, used from its next launch.
    GameProton(String, ProtonChoice),
}

impl App {
    pub fn update_settings(&mut self, msg: SettingsMsg) -> Task<Message> {
        let Some(core) = self.core.clone() else {
            return Task::none();
        };
        match msg {
            SettingsMsg::RootInput(v) => self.library_root = v,
            SettingsMsg::SaveRoot => {
                let path = PathBuf::from(self.library_root.trim());
                if !path.is_absolute() {
                    self.notify_error("The default installation path must be absolute.".into());
                } else if let Err(e) = settings::set_library_root(&core.db, &path) {
                    self.notify_error(e.to_string());
                }
            }
            SettingsMsg::Proton(choice) => {
                match settings::set_default_proton(&core.db, &choice.0) {
                    Ok(()) => self.proton = Some(choice.0),
                    Err(e) => self.notify_error(e.to_string()),
                }
            }
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
