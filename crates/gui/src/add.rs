//! The dialog that adds a game from no store, by the program that starts it
//! (`slatty_core::added`).

use std::path::PathBuf;

use iced::Task;
use slatty_core::added::{self, NewGame};
use slatty_core::install::Install;

use crate::settings::ProtonChoice;
use crate::{App, Message, err};

/// What the dialog was given so far.
#[derive(Debug, Clone, Default)]
pub struct AddDraft {
    pub title: String,
    pub program: String,
    pub args: String,
    /// The game's folder, when it is not the program's own.
    pub folder: String,
    pub proton: Option<PathBuf>,
    pub isolated: bool,
    pub busy: bool,
    /// Why the game was not added.
    pub error: Option<String>,
}

impl AddDraft {
    /// Whether the program looks like a Windows one, which runs through Proton. Only shows the
    /// Proton choice: what the program is comes from its first bytes once added.
    pub fn windows(&self) -> bool {
        self.program.to_lowercase().ends_with(".exe")
    }
}

#[derive(Debug, Clone)]
pub enum AddMsg {
    Open,
    Title(String),
    Program(String),
    PickProgram,
    PickedProgram(Option<PathBuf>),
    Args(String),
    Folder(String),
    PickFolder,
    PickedFolder(Option<PathBuf>),
    Proton(ProtonChoice),
    Isolated(bool),
    Save,
    /// The game added, and the GOG game its folder holds, if any.
    Saved(Result<(Install, Option<String>), String>),
    Cancel,
}

impl App {
    pub fn update_add(&mut self, msg: AddMsg) -> Task<Message> {
        match msg {
            AddMsg::Open => {
                self.add_game = Some(AddDraft {
                    proton: self.proton.clone(),
                    isolated: self.isolate_new_games,
                    ..Default::default()
                });
            }
            AddMsg::Cancel => self.add_game = None,
            AddMsg::PickProgram => {
                return Task::perform(
                    async {
                        rfd::AsyncFileDialog::new()
                            .set_title("Choose the program that starts the game")
                            .pick_file()
                            .await
                            .map(|f| f.path().to_path_buf())
                    },
                    |path| Message::Add(AddMsg::PickedProgram(path)),
                );
            }
            AddMsg::PickFolder => {
                return Task::perform(
                    async {
                        rfd::AsyncFileDialog::new()
                            .set_title("Choose the game's folder")
                            .pick_folder()
                            .await
                            .map(|f| f.path().to_path_buf())
                    },
                    |path| Message::Add(AddMsg::PickedFolder(path)),
                );
            }
            AddMsg::Save => {
                let (Some(core), Some(d)) = (self.core.clone(), self.add_game.as_mut()) else {
                    return Task::none();
                };
                d.busy = true;
                d.error = None;
                let text = |s: &str| Some(s.trim()).filter(|s| !s.is_empty()).map(PathBuf::from);
                let folder = text(&d.folder);
                let program = PathBuf::from(d.program.trim());
                let new = NewGame {
                    title: d.title.clone(),
                    args: slatty_core::gameinfo::split_args(&d.args),
                    folder: folder.clone(),
                    working_dir: None,
                    proton: d.proton.clone(),
                    isolated: Some(d.isolated),
                    program,
                };
                return Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            let folder = new
                                .folder
                                .clone()
                                .or_else(|| new.program.parent().map(Into::into));
                            let gog = folder.as_deref().and_then(added::gog_game_in);
                            added::add(&core.db, &core.dirs, new).map(|i| (i, gog))
                        })
                        .await
                        .map_err(err)?
                        .map_err(err)
                    },
                    |r| Message::Add(AddMsg::Saved(r)),
                );
            }
            AddMsg::Saved(Ok((install, gog))) => {
                self.add_game = None;
                let id = install.game_id.clone();
                self.installs.insert(id.clone(), install);
                self.merge_steam();
                if let Some(gog) = gog {
                    self.notice = Some(crate::Notice {
                        error: false,
                        text: format!(
                            "Its folder holds a GOG game ({gog}): installed from GOG, it would \
                             have its cloud saves and achievements."
                        ),
                    });
                }
                return self.update(Message::Select(id));
            }
            AddMsg::Saved(Err(e)) => {
                if let Some(d) = &mut self.add_game {
                    d.busy = false;
                    d.error = Some(e);
                }
            }
            msg => {
                let Some(d) = &mut self.add_game else {
                    return Task::none();
                };
                match msg {
                    AddMsg::Title(v) => d.title = v,
                    AddMsg::Program(v) => d.program = v,
                    AddMsg::PickedProgram(Some(path)) => {
                        // The file's name, as a title to start from.
                        if d.title.trim().is_empty()
                            && let Some(stem) = path.file_stem()
                        {
                            d.title = stem.to_string_lossy().into_owned();
                        }
                        d.program = path.display().to_string();
                    }
                    AddMsg::Args(v) => d.args = v,
                    AddMsg::Folder(v) => d.folder = v,
                    AddMsg::PickedFolder(Some(path)) => d.folder = path.display().to_string(),
                    AddMsg::Proton(choice) => d.proton = Some(choice.0),
                    AddMsg::Isolated(on) => d.isolated = on,
                    _ => {}
                }
            }
        }
        Task::none()
    }
}
