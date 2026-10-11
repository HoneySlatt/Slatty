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
    /// Whether the game is installed first, from which installer or archive, and where a Linux
    /// one is unpacked.
    pub with_installer: bool,
    pub installer: String,
    pub into: String,
    /// The game the installer ran for, once it ended, and where it put it: the program is there.
    pub installed_by: Option<String>,
    pub installed_in: Option<PathBuf>,
}

impl AddDraft {
    /// Whether the program looks like a Windows one, which runs through Proton. Only shows the
    /// Proton choice: what the program is comes from its first bytes once added.
    pub fn windows(&self) -> bool {
        let exe = |s: &str| s.to_lowercase().ends_with(".exe");
        if self.installer_first() {
            exe(&self.installer)
        } else {
            exe(&self.program)
        }
    }

    /// Whether the installer is still to run before the program can be picked.
    pub fn installer_first(&self) -> bool {
        self.with_installer && self.installed_by.is_none()
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
    WithInstaller(bool),
    Installer(String),
    PickInstaller,
    PickedInstaller(Option<PathBuf>),
    RunInstaller,
    /// Where a Linux installer or archive is unpacked.
    Into(String),
    PickInto,
    PickedInto(Option<PathBuf>),
    /// The installer ended: the game it ran for, and where it put it.
    InstallerEnded(Result<(String, PathBuf), String>),
    /// What the installer put in a prefix, thrown away as the dialog closed.
    Discarded(Result<(), String>),
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
                    into: self.library_root.clone(),
                    ..Default::default()
                });
            }
            // Not while the installer runs: what it installs would be left behind.
            AddMsg::Cancel if self.add_game.as_ref().is_some_and(|d| d.busy) => {}
            AddMsg::Cancel => {
                // What an installer put in the game's prefix goes with it.
                let installed = self.add_game.take().and_then(|d| d.installed_by);
                if let (Some(core), Some(id)) = (self.core.clone(), installed) {
                    return Task::perform(
                        async move {
                            tokio::task::spawn_blocking(move || {
                                added::discard_installer(&core.db, &core.dirs, &id)
                            })
                            .await
                            .map_err(err)?
                            .map_err(err)
                        },
                        |r| Message::Add(AddMsg::Discarded(r)),
                    );
                }
            }
            AddMsg::Discarded(Ok(())) => {}
            AddMsg::Discarded(Err(e)) => self.notify_error(e),
            AddMsg::PickProgram => {
                // Where the installer put the game, once it ran.
                let start = self.add_game.as_ref().and_then(|d| d.installed_in.clone());
                return Task::perform(
                    async move {
                        let mut dialog = rfd::AsyncFileDialog::new()
                            .set_title("Choose the program that starts the game");
                        if let Some(start) = start {
                            dialog = dialog.set_directory(start);
                        }
                        dialog.pick_file().await.map(|f| f.path().to_path_buf())
                    },
                    |path| Message::Add(AddMsg::PickedProgram(path)),
                );
            }
            AddMsg::PickInstaller => {
                return Task::perform(
                    async {
                        rfd::AsyncFileDialog::new()
                            .set_title("Choose the game's installer or archive")
                            .pick_file()
                            .await
                            .map(|f| f.path().to_path_buf())
                    },
                    |path| Message::Add(AddMsg::PickedInstaller(path)),
                );
            }
            AddMsg::PickInto => {
                return Task::perform(
                    async {
                        rfd::AsyncFileDialog::new()
                            .set_title("Choose where to unpack the game")
                            .pick_folder()
                            .await
                            .map(|f| f.path().to_path_buf())
                    },
                    |path| Message::Add(AddMsg::PickedInto(path)),
                );
            }
            AddMsg::RunInstaller => {
                let (Some(core), Some(d)) = (self.core.clone(), self.add_game.as_mut()) else {
                    return Task::none();
                };
                d.busy = true;
                d.error = None;
                let installer = PathBuf::from(d.installer.trim());
                let into = PathBuf::from(d.into.trim());
                let (title, proton, isolated) = (d.title.clone(), d.proton.clone(), d.isolated);
                return Task::perform(
                    async move {
                        // A Linux installer or archive is unpacked; a Windows one runs.
                        let installer2 = installer.clone();
                        let archive =
                            tokio::task::spawn_blocking(move || added::is_archive(&installer2))
                                .await
                                .map_err(err)?;
                        if archive {
                            std::fs::create_dir_all(&into)
                                .map_err(|e| format!("{}: {e}", into.display()))?;
                            return added::unpack(
                                &core.db,
                                &core.dirs,
                                &installer,
                                &into,
                                &title,
                                Default::default(),
                                &|_, _| {},
                            )
                            .await
                            .map_err(err);
                        }
                        let id = added::run_installer(
                            &core.db,
                            &core.dirs,
                            &slatty_core::session::supervisor_exe(),
                            &installer,
                            proton,
                            isolated,
                        )
                        .await
                        .map_err(err)?;
                        let drive = core.dirs.data.join("prefixes").join(&id).join("drive_c");
                        Ok((id, drive))
                    },
                    |r| Message::Add(AddMsg::InstallerEnded(r)),
                );
            }
            AddMsg::InstallerEnded(r) => {
                if let Some(d) = &mut self.add_game {
                    d.busy = false;
                    match r {
                        Ok((id, folder)) => {
                            d.installed_by = Some(id);
                            d.installed_in = Some(folder);
                        }
                        Err(e) => d.error = Some(e),
                    }
                }
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
                    installed_by: d.installed_by.clone(),
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
                    // Once it ran, the game is in the prefix made for it.
                    AddMsg::WithInstaller(on) if d.installed_by.is_none() => {
                        d.with_installer = on;
                    }
                    AddMsg::Installer(v) => d.installer = v,
                    AddMsg::Into(v) => d.into = v,
                    AddMsg::PickedInto(Some(path)) => d.into = path.display().to_string(),
                    AddMsg::PickedInstaller(Some(path)) => {
                        if d.title.trim().is_empty()
                            && let Some(stem) = path.file_stem()
                        {
                            let stem = stem.to_string_lossy();
                            // `game.tar.gz`'s stem is `game.tar`.
                            d.title = stem.strip_suffix(".tar").unwrap_or(&stem).to_string();
                        }
                        d.installer = path.display().to_string();
                    }
                    _ => {}
                }
            }
        }
        Task::none()
    }
}
