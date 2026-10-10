//! Settings page actions: default installation path and default Proton.

use std::fmt;
use std::path::PathBuf;

use iced::Task;
use slatty_core::install::Platform;
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

/// Cover sizes offered, in percent of the usual width.
pub const COVER_SIZES: [u16; 7] = [75, 85, 100, 115, 130, 145, 160];
/// Width of a cover at 100 %, in pixels.
const COVER_WIDTH: f32 = 150.0;

/// A cover size, in percent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CoverSize(pub u16);

impl CoverSize {
    pub fn width(self) -> f32 {
        COVER_WIDTH * f32::from(self.0) / 100.0
    }

    /// The size offered nearest to `width`.
    pub fn of(width: f32) -> CoverSize {
        let percent = width / COVER_WIDTH * 100.0;
        CoverSize(
            COVER_SIZES
                .into_iter()
                .min_by_key(|p| (f32::from(*p) - percent).abs() as u32)
                .unwrap_or(100),
        )
    }
}

impl fmt::Display for CoverSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} %", self.0)
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

/// The parts of the Settings page, listed beside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Section {
    #[default]
    Account,
    Library,
    Installs,
    Appearance,
    About,
}

impl Section {
    pub const ALL: [Section; 5] = [
        Section::Account,
        Section::Library,
        Section::Installs,
        Section::Appearance,
        Section::About,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Section::Account => "Account",
            Section::Library => "Library",
            Section::Installs => "Installs",
            Section::Appearance => "Appearance",
            Section::About => "About",
        }
    }

    /// The id of its card, to find where it is.
    pub fn id(self) -> &'static str {
        match self {
            Section::Account => "settings-account",
            Section::Library => "settings-library",
            Section::Installs => "settings-installs",
            Section::Appearance => "settings-appearance",
            Section::About => "settings-about",
        }
    }
}

/// The id of the page's scrolling area.
pub const SETTINGS_SCROLL: &str = "settings-scroll";

/// Windows or Linux, named for a pick list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlatformChoice(pub Platform);

impl PlatformChoice {
    pub const ALL: [PlatformChoice; 2] = [
        PlatformChoice(Platform::Windows),
        PlatformChoice(Platform::Linux),
    ];
}

impl fmt::Display for PlatformChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self.0 {
            Platform::Windows => "Windows",
            Platform::Linux => "Linux (native)",
        })
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
    /// Size of the library covers, kept for the next start.
    CoverSize(CoverSize),
    /// A built-in theme, under the theme file.
    Preset(crate::presets::Preset),
    /// Build installed when a game has both.
    Platform(PlatformChoice),
    /// A part of the page, chosen in its side list: shown at the top.
    Show(Section),
    /// Where that part starts, below the first one, once measured.
    ScrollTo(Option<f32>),
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
            SettingsMsg::Platform(choice) => {
                self.default_platform = choice.0;
                if let Err(e) = settings::set_default_platform(&core.db, choice.0) {
                    self.notify_error(e.to_string());
                }
            }
            SettingsMsg::Show(section) => {
                use iced::widget::selector::{find, id};
                self.settings_section = section;
                // Its offset in the page: how far its card is below the first one.
                return find(id(section.id())).then(|card| {
                    find(id(Section::Account.id())).map(move |first| {
                        let top = |t: &Option<_>| {
                            t.as_ref()
                                .map(|t: &iced::widget::selector::Target| t.bounds().y)
                        };
                        Message::Settings(SettingsMsg::ScrollTo(
                            top(&card).zip(top(&first)).map(|(c, f)| c - f),
                        ))
                    })
                });
            }
            SettingsMsg::ScrollTo(Some(y)) => {
                return iced::widget::operation::scroll_to(
                    SETTINGS_SCROLL,
                    iced::widget::operation::AbsoluteOffset {
                        x: None,
                        y: Some(y),
                    },
                );
            }
            SettingsMsg::ScrollTo(None) => {}
            SettingsMsg::CoverSize(size) => {
                self.card_width = size.width();
                if let Err(e) = settings::set_cover_width(&core.db, self.card_width) {
                    self.notify_error(e.to_string());
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
