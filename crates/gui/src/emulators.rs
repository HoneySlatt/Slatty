//! Settings → Emulators: the program of each emulator and the folders its games are looked for
//! in, and the scan that adds the games found there to the library.

use std::collections::HashMap;
use std::path::PathBuf;

use iced::Task;
use slatty_core::db::Db;
use slatty_core::emulated::{self, Emulator};
use slatty_core::install::Install;

use crate::{App, Message, Notice, err};

#[derive(Debug, Clone, Default)]
pub struct EmulatorsView {
    /// The program of each emulator found, the one chosen or else the one in `PATH`.
    pub programs: HashMap<Emulator, PathBuf>,
    /// The emulators whose program was chosen rather than found.
    pub chosen: Vec<Emulator>,
    /// Where each emulator's games are looked for.
    pub folders: HashMap<Emulator, Vec<PathBuf>>,
    pub scanning: bool,
}

impl EmulatorsView {
    /// Read off the interface thread: `PATH` is looked through.
    pub fn read(db: &Db) -> slatty_core::Result<EmulatorsView> {
        let mut view = EmulatorsView::default();
        for e in Emulator::ALL {
            if let Some(program) = e.program(db)? {
                view.programs.insert(e, program);
            }
            if slatty_core::settings::emulator_program(db, e)?.is_some() {
                view.chosen.push(e);
            }
            let folders = slatty_core::settings::rom_folders(db, e)?;
            if !folders.is_empty() {
                view.folders.insert(e, folders);
            }
        }
        Ok(view)
    }
}

#[derive(Debug, Clone)]
pub enum EmulatorsMsg {
    PickProgram(Emulator),
    PickedProgram(Emulator, Option<PathBuf>),
    /// Goes back to the program found in `PATH`.
    FindProgram(Emulator),
    AddFolder(Emulator),
    PickedFolder(Emulator, Option<PathBuf>),
    RemoveFolder(Emulator, PathBuf),
    /// Looks through every emulator's folders and adds the games found.
    Scan,
    /// The games added, with their emulators, and how many could not be.
    Scanned(Result<(Vec<(Install, Emulator)>, usize), String>),
    /// What the emulators are, read again once one changed.
    Read(Result<EmulatorsView, String>),
}

impl App {
    pub fn update_emulators(&mut self, msg: EmulatorsMsg) -> Task<Message> {
        let Some(core) = self.core.clone() else {
            return Task::none();
        };
        match msg {
            EmulatorsMsg::PickProgram(e) => {
                return Task::perform(
                    async move {
                        rfd::AsyncFileDialog::new()
                            .set_title(format!("Choose the program of {}", e.name()))
                            .pick_file()
                            .await
                            .map(|f| f.path().to_path_buf())
                    },
                    move |path| Message::Emulators(EmulatorsMsg::PickedProgram(e, path)),
                );
            }
            EmulatorsMsg::PickedProgram(_, None) => {}
            EmulatorsMsg::PickedProgram(e, Some(program)) => {
                return self.change_emulator(move |db| {
                    slatty_core::settings::set_emulator_program(db, e, Some(&program))
                });
            }
            EmulatorsMsg::FindProgram(e) => {
                return self.change_emulator(move |db| {
                    slatty_core::settings::set_emulator_program(db, e, None)
                });
            }
            EmulatorsMsg::AddFolder(e) => {
                return Task::perform(
                    async move {
                        rfd::AsyncFileDialog::new()
                            .set_title(format!("Choose a folder of {} games", e.name()))
                            .pick_folder()
                            .await
                            .map(|f| f.path().to_path_buf())
                    },
                    move |path| Message::Emulators(EmulatorsMsg::PickedFolder(e, path)),
                );
            }
            EmulatorsMsg::PickedFolder(_, None) => {}
            EmulatorsMsg::PickedFolder(e, Some(folder)) => {
                let mut folders = self.emulators.folders.get(&e).cloned().unwrap_or_default();
                if !folders.contains(&folder) {
                    folders.push(folder);
                }
                return self.change_emulator(move |db| {
                    slatty_core::settings::set_rom_folders(db, e, &folders)
                });
            }
            EmulatorsMsg::RemoveFolder(e, folder) => {
                let mut folders = self.emulators.folders.get(&e).cloned().unwrap_or_default();
                folders.retain(|f| *f != folder);
                return self.change_emulator(move |db| {
                    slatty_core::settings::set_rom_folders(db, e, &folders)
                });
            }
            EmulatorsMsg::Scan if self.emulators.scanning => {}
            EmulatorsMsg::Scan => {
                self.emulators.scanning = true;
                let folders = self.emulators.folders.clone();
                return Task::perform(
                    // Folders of many games, maybe on a disk that wakes up for them.
                    slatty_core::fsutil::in_background(move || {
                        let mut added = Vec::new();
                        let mut failed = 0;
                        for (e, folders) in folders {
                            let found =
                                emulated::scan(&core.db, &core.dirs, e, &folders).map_err(err)?;
                            let (ok, bad) = emulated::add_found(&core.db, &core.dirs, e, found);
                            added.extend(ok.into_iter().map(|i| (i, e)));
                            failed += bad.len();
                        }
                        Ok((added, failed))
                    }),
                    |r| Message::Emulators(EmulatorsMsg::Scanned(r)),
                );
            }
            EmulatorsMsg::Scanned(r) => {
                self.emulators.scanning = false;
                match r {
                    Ok((added, failed)) => {
                        let text = match (added.len(), failed) {
                            (0, 0) => "No new games were found.".to_string(),
                            (1, 0) => "1 game was added to the library.".to_string(),
                            (n, 0) => format!("{n} games were added to the library."),
                            (n, f) => {
                                format!("{n} game(s) added to the library; {f} could not be added.")
                            }
                        };
                        let any = !added.is_empty();
                        for (install, e) in added {
                            self.emulated.insert(install.game_id.clone(), e);
                            self.installs.insert(install.game_id.clone(), install);
                        }
                        if any {
                            self.merge_steam();
                        }
                        self.notice = Some(Notice {
                            error: failed > 0,
                            text,
                        });
                    }
                    Err(e) => self.notify_error(e),
                }
            }
            EmulatorsMsg::Read(Ok(view)) => {
                self.emulators = EmulatorsView {
                    scanning: self.emulators.scanning,
                    ..view
                };
            }
            EmulatorsMsg::Read(Err(e)) => self.notify_error(e),
        }
        Task::none()
    }

    /// Saves a change of an emulator's settings, then reads them all again.
    fn change_emulator(
        &self,
        change: impl FnOnce(&Db) -> slatty_core::Result<()> + Send + 'static,
    ) -> Task<Message> {
        let Some(core) = self.core.clone() else {
            return Task::none();
        };
        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || {
                    change(&core.db).map_err(err)?;
                    EmulatorsView::read(&core.db).map_err(err)
                })
                .await
                .map_err(err)?
            },
            |r| Message::Emulators(EmulatorsMsg::Read(r)),
        )
    }
}
