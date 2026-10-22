use super::*;

struct Fake {
    root: PathBuf,
    dirs: Dirs,
    db: Db,
    roms: PathBuf,
}

/// A folder of games, and Slatty's data, in a temporary folder.
fn fake(name: &str) -> Fake {
    let root = std::env::temp_dir().join(format!("slatty-emulated-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let roms = root.join("roms");
    std::fs::create_dir_all(&roms).unwrap();
    Fake {
        dirs: Dirs::under(&root.join("slatty")),
        db: Db::in_memory().unwrap(),
        roms: roms.canonicalize().unwrap(),
        root,
    }
}

impl Fake {
    /// A file in the games' folder, with its folders.
    fn file(&self, rel: &str, bytes: &[u8]) -> PathBuf {
        let path = self.roms.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, bytes).unwrap();
        path
    }

    fn add(&self, path: &Path, emulator: Emulator) -> Result<Install> {
        add(
            &self.db,
            &self.dirs,
            NewGame {
                title: None,
                path: path.to_path_buf(),
                emulator,
            },
        )
    }

    fn scan(&self, emulator: Emulator) -> Vec<Found> {
        scan(
            &self.db,
            &self.dirs,
            emulator,
            std::slice::from_ref(&self.roms),
        )
        .unwrap()
    }
}

impl Drop for Fake {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// A `PARAM.SFO` giving `title`, laid out as PlayStation consoles write it.
fn sfo(title: &str) -> Vec<u8> {
    let keys = b"CATEGORY\0TITLE\0\0\0";
    let (keys_at, data_at) = (20 + 2 * 16, 20 + 2 * 16 + keys.len());
    let mut out = b"\0PSF".to_vec();
    for n in [0x0101u32, keys_at as u32, data_at as u32, 2] {
        out.extend(n.to_le_bytes());
    }
    let title = format!("{title}\0");
    // CATEGORY, then TITLE: key offset, format, length, room, data offset.
    out.extend(0u16.to_le_bytes());
    out.extend(0x0204u16.to_le_bytes());
    out.extend([3u32, 4, 0].iter().flat_map(|n| n.to_le_bytes()));
    out.extend(9u16.to_le_bytes());
    out.extend(0x0204u16.to_le_bytes());
    out.extend(
        [title.len() as u32, 128, 4]
            .iter()
            .flat_map(|n| n.to_le_bytes()),
    );
    out.extend(keys);
    out.extend(b"DG\0\0");
    out.extend(title.as_bytes());
    out
}

#[test]
fn emulators_are_told_by_the_file_or_folder() {
    let f = fake("kinds");
    let xci = f.file("[FAKE] Game.XCI", b"");
    assert_eq!(emulators_for(&xci), vec![Emulator::Eden]);
    let iso = f.file("[FAKE] Game.iso", b"");
    assert_eq!(
        emulators_for(&iso),
        vec![
            Emulator::DuckStation,
            Emulator::Pcsx2,
            Emulator::Ppsspp,
            Emulator::Dolphin
        ]
    );
    f.file("ps3/[FAKE] Game/PS3_GAME/USRDIR/EBOOT.BIN", b"");
    assert_eq!(
        emulators_for(&f.roms.join("ps3/[FAKE] Game")),
        vec![Emulator::Rpcs3]
    );
    f.file("ps4/CUSA00000/eboot.bin", b"");
    f.file("ps4/CUSA00000/sce_sys/param.sfo", &sfo("[FAKE] Game"));
    assert_eq!(
        emulators_for(&f.roms.join("ps4/CUSA00000")),
        vec![Emulator::ShadPs4]
    );
    f.file("ps5/PPSA00000/eboot.bin", b"");
    f.file("ps5/PPSA00000/sce_sys/param.json", b"{}");
    assert_eq!(
        emulators_for(&f.roms.join("ps5/PPSA00000")),
        vec![Emulator::Kyty]
    );
    assert!(emulators_for(&f.file("readme.txt", b"")).is_empty());
}

#[test]
fn a_game_is_added_from_its_file_and_titled_from_its_name() {
    let f = fake("add");
    let rom = f.file("ps2/[FAKE] Some Game (USA) [!].chd", b"[FAKE]");
    let install = f.add(&rom, Emulator::Pcsx2).unwrap();
    assert!(crate::store::is_emulated(&install.game_id));
    assert_eq!(install.title, "[FAKE] Some Game");
    assert_eq!(install.path, f.roms.join("ps2"));
    assert_eq!(install.runner, Runner::Native);
    assert!(!install.isolated);
    let library = library(&f.db, &f.dirs).unwrap();
    assert_eq!(library.len(), 1);
    assert_eq!(library[0].os, vec!["ps2".to_string()]);
    // Once only, and only by an emulator that starts it.
    assert!(matches!(
        f.add(&rom, Emulator::Pcsx2),
        Err(Error::Refused(_))
    ));
    assert!(matches!(
        f.add(&rom, Emulator::Eden),
        Err(Error::Refused(_))
    ));
    let named = add(
        &f.db,
        &f.dirs,
        NewGame {
            title: Some("  [FAKE] Named  ".into()),
            path: f.file("ps2/other.iso", b""),
            emulator: Emulator::Pcsx2,
        },
    )
    .unwrap();
    assert_eq!(named.title, "[FAKE] Named");
    assert_ne!(named.game_id, install.game_id);
}

#[test]
fn the_emulator_is_given_the_game_full_screen() {
    let f = fake("launch");
    let rom = f.file("switch/[FAKE] Game.nsp", b"");
    let install = f.add(&rom, Emulator::Eden).unwrap();
    let program = f.file("bin/eden", b"#!/bin/sh\n");
    crate::settings::set_emulator_program(&f.db, Emulator::Eden, Some(&program)).unwrap();
    let spec = crate::runner::game_launch_spec(&f.db, &f.dirs, &install).unwrap();
    assert_eq!(spec.program, program);
    assert_eq!(
        spec.args,
        vec!["-f", "-g", rom.to_str().unwrap()]
            .into_iter()
            .map(String::from)
            .collect::<Vec<_>>()
    );
    assert_eq!(spec.cwd, f.roms.join("switch"));
    assert!(spec.env.is_empty());
    // A file gone is said to be.
    std::fs::remove_file(&rom).unwrap();
    assert!(matches!(
        launch_spec(&f.db, &f.dirs, &install),
        Err(Error::NotFound(_))
    ));
}

#[test]
fn a_record_leading_out_of_the_game_folder_is_refused() {
    let f = fake("outside");
    let rom = f.file("psp/[FAKE] Game.cso", b"");
    let install = f.add(&rom, Emulator::Ppsspp).unwrap();
    f.file("elsewhere.cso", b"");
    let game = Game {
        emulator: Emulator::Ppsspp,
        start: "../elsewhere.cso".into(),
    };
    save_game(&f.dirs, &install.game_id, &game).unwrap();
    assert!(matches!(
        launch_spec(&f.db, &f.dirs, &install),
        Err(Error::Refused(_))
    ));
}

#[test]
fn removing_a_game_keeps_its_files_and_scans_leave_it_out() {
    let f = fake("remove");
    let rom = f.file("gc/[FAKE] Game.rvz", b"[FAKE]");
    let install = f.add(&rom, Emulator::Dolphin).unwrap();
    assert!(f.scan(Emulator::Dolphin).is_empty(), "listed already");
    remove(&f.db, &f.dirs, &install.game_id).unwrap();
    assert!(rom.is_file(), "the user's file stays");
    assert!(Install::get(&f.db, &install.game_id).unwrap().is_none());
    assert!(!game_file(&f.dirs, &install.game_id).exists());
    assert!(f.scan(Emulator::Dolphin).is_empty(), "removed on purpose");
    // Added by hand again, a scan would find it again once removed no longer.
    let again = f.add(&rom, Emulator::Dolphin).unwrap();
    assert_ne!(again.game_id, install.game_id, "an id is never given twice");
    assert!(
        crate::settings::forgotten_emulated(&f.db)
            .unwrap()
            .is_empty()
    );
    // What is not emulated is not removed here.
    assert!(matches!(
        remove(&f.db, &f.dirs, "local-1"),
        Err(Error::Refused(_))
    ));
}

#[test]
fn a_scan_finds_each_game_once() {
    let f = fake("scan");
    f.file("ps1/[FAKE] One (USA).chd", b"");
    // A game on two discs, each a cue sheet and its track, and the playlist of both.
    for disc in 1..=2 {
        f.file(&format!("ps1/Two/[FAKE] Two (Disc {disc}).bin"), b"");
        f.file(
            &format!("ps1/Two/[FAKE] Two (Disc {disc}).cue"),
            format!("FILE \"[FAKE] Two (Disc {disc}).bin\" BINARY\n  TRACK 01 MODE2/2352\n")
                .as_bytes(),
        );
    }
    f.file(
        "ps1/Two/[FAKE] Two.m3u",
        b"# discs\n[FAKE] Two (Disc 1).cue\n[FAKE] Two (Disc 2).cue\n",
    );
    f.file("ps1/notes.txt", b"");
    // A link back to a folder already seen.
    std::os::unix::fs::symlink(&f.roms, f.roms.join("ps1/loop")).unwrap();
    let found = f.scan(Emulator::DuckStation);
    let mut titles: Vec<&str> = found.iter().map(|g| g.title.as_str()).collect();
    titles.sort();
    assert_eq!(titles, vec!["[FAKE] One", "[FAKE] Two"]);
    let (added, failed) = add_found(&f.db, &f.dirs, Emulator::DuckStation, found);
    assert_eq!((added.len(), failed.len()), (2, 0));
    assert!(f.scan(Emulator::DuckStation).is_empty());
}

#[test]
fn folder_games_are_found_with_the_title_they_give() {
    let f = fake("folders");
    f.file("ps3/BLUS00000/PS3_GAME/USRDIR/EBOOT.BIN", b"");
    f.file("ps3/BLUS00000/PS3_GAME/PARAM.SFO", &sfo("[FAKE] Third"));
    f.file("ps4/CUSA00000/eboot.bin", b"");
    f.file("ps4/CUSA00000/sce_sys/param.sfo", &sfo("[FAKE] Fourth"));
    f.file("ps5/PPSA00000/eboot.bin", b"");
    f.file(
        "ps5/PPSA00000/sce_sys/param.json",
        br#"{"localizedParameters":{"defaultLanguage":"en-US","en-US":{"titleName":"[FAKE] Fifth"}}}"#,
    );
    for (emulator, title, folder) in [
        (Emulator::Rpcs3, "[FAKE] Third", "ps3/BLUS00000"),
        (Emulator::ShadPs4, "[FAKE] Fourth", "ps4/CUSA00000"),
        (Emulator::Kyty, "[FAKE] Fifth", "ps5/PPSA00000"),
    ] {
        let found = f.scan(emulator);
        assert_eq!(found.len(), 1, "{emulator:?}");
        assert_eq!(found[0].title, title);
        let (added, failed) = add_found(&f.db, &f.dirs, emulator, found);
        assert!(failed.is_empty(), "{failed:?}");
        assert_eq!(added[0].path, f.roms.join(folder));
        assert!(f.scan(emulator).is_empty(), "{emulator:?} listed already");
    }
}

/// Neither isolated, which its emulator is not, nor forgotten as a GOG game is: removed.
#[test]
fn an_emulated_game_is_neither_isolated_nor_forgotten() {
    let f = fake("refused");
    let install = f
        .add(&f.file("[FAKE] Game.iso", b""), Emulator::Pcsx2)
        .unwrap();
    assert!(matches!(
        crate::install::set_isolated(&f.db, &install.game_id, true),
        Err(Error::Refused(_))
    ));
    assert!(matches!(
        crate::maintenance::forget(&f.db, &f.dirs, &install.game_id),
        Err(Error::Refused(_))
    ));
    assert!(Install::get(&f.db, &install.game_id).unwrap().is_some());
}

#[test]
fn a_moved_game_is_found_again() {
    let f = fake("locate");
    let rom = f.file("ps2/[FAKE] Game.iso", b"");
    let install = f.add(&rom, Emulator::Pcsx2).unwrap();
    let moved = f.file("elsewhere/[FAKE] Game.iso", b"");
    std::fs::remove_file(&rom).unwrap();
    let found = locate(&f.db, &f.dirs, &install.game_id, &moved).unwrap();
    assert_eq!(found.path, f.roms.join("elsewhere"));
    assert_eq!(found.title, install.title);
    assert!(
        locate(&f.db, &f.dirs, &install.game_id, &f.file("x.txt", b"")).is_err(),
        "not a game of its emulator"
    );
}

#[test]
fn titles_lose_the_tags_of_disc_sets() {
    assert_eq!(clean_title("Some_Game (USA) (En,Fr) [!]"), "Some Game");
    assert_eq!(clean_title("(Prototype)"), "(Prototype)");
}
