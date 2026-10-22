//! Games run in an emulator: a disc image, a file or a game's folder the user has, started by the
//! emulator of its system, found in `PATH` or chosen in Settings. Its files are the user's: Slatty
//! never writes in them, and removing such a game only takes it out of the library. Emulators keep
//! their saves and settings themselves.
//!
//! The game is an install (`installs`) whose id is `emu-<number>` (`store::emulated_id`), never
//! given twice, and whose folder is the one holding its file, which other games may share, or the
//! game's own folder. Which emulator starts it, and with what, is kept in `emulated/<id>.json`.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::db::Db;
use crate::error::{Error, Result};
use crate::install::{Install, Platform};
use crate::installer::leads_inside;
use crate::paths::Dirs;
use crate::runner::{LaunchSpec, Runner};

/// The setting counting the games ever emulated, from which each takes its number.
const EMULATED: &str = "emulated_games";

/// How deep a scan looks in the folders it is given: a system's folder, a folder per game, and the
/// files of a game in a folder of their own.
const SCAN_DEPTH: usize = 4;

/// The emulators Slatty starts games in, one per system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Emulator {
    DuckStation,
    Pcsx2,
    Ppsspp,
    Rpcs3,
    ShadPs4,
    Kyty,
    Dolphin,
    Eden,
}

impl Emulator {
    pub const ALL: [Emulator; 8] = [
        Emulator::DuckStation,
        Emulator::Pcsx2,
        Emulator::Ppsspp,
        Emulator::Rpcs3,
        Emulator::ShadPs4,
        Emulator::Kyty,
        Emulator::Dolphin,
        Emulator::Eden,
    ];

    /// How it is named in the settings.
    pub fn key(self) -> &'static str {
        match self {
            Emulator::DuckStation => "duckstation",
            Emulator::Pcsx2 => "pcsx2",
            Emulator::Ppsspp => "ppsspp",
            Emulator::Rpcs3 => "rpcs3",
            Emulator::ShadPs4 => "shadps4",
            Emulator::Kyty => "kyty",
            Emulator::Dolphin => "dolphin",
            Emulator::Eden => "eden",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Emulator::DuckStation => "DuckStation",
            Emulator::Pcsx2 => "PCSX2",
            Emulator::Ppsspp => "PPSSPP",
            Emulator::Rpcs3 => "RPCS3",
            Emulator::ShadPs4 => "shadPS4",
            Emulator::Kyty => "Kyty",
            Emulator::Dolphin => "Dolphin",
            Emulator::Eden => "Eden",
        }
    }

    /// The system it emulates, as the library lists it among a game's systems.
    pub fn system(self) -> &'static str {
        match self {
            Emulator::DuckStation => "ps1",
            Emulator::Pcsx2 => "ps2",
            Emulator::Ppsspp => "psp",
            Emulator::Rpcs3 => "ps3",
            Emulator::ShadPs4 => "ps4",
            Emulator::Kyty => "ps5",
            Emulator::Dolphin => "wii",
            Emulator::Eden => "switch",
        }
    }

    /// The names its program goes by, as distributions package it.
    fn programs(self) -> &'static [&'static str] {
        match self {
            Emulator::DuckStation => &["duckstation-qt", "duckstation"],
            Emulator::Pcsx2 => &["pcsx2-qt", "pcsx2"],
            Emulator::Ppsspp => &["ppsspp", "PPSSPPSDL", "PPSSPPQt"],
            Emulator::Rpcs3 => &["rpcs3"],
            // The emulator itself: its Qt launcher starts it only once told which one to use.
            Emulator::ShadPs4 => &["shadps4"],
            Emulator::Kyty => &["kyty_emulator"],
            Emulator::Dolphin => &["dolphin-emu"],
            Emulator::Eden => &["eden"],
        }
    }

    /// The extensions, lowercase, of the files it starts. Those that start a game's folder start
    /// it from what `folder_game` finds there.
    pub fn extensions(self) -> &'static [&'static str] {
        match self {
            Emulator::DuckStation => &["cue", "chd", "pbp", "m3u", "iso", "img", "ecm"],
            Emulator::Pcsx2 => &["iso", "chd", "cso", "zso", "cue"],
            Emulator::Ppsspp => &["iso", "cso", "chd", "pbp"],
            Emulator::Rpcs3 | Emulator::ShadPs4 => &[],
            Emulator::Kyty => &["zar"],
            Emulator::Dolphin => &[
                "iso", "gcm", "rvz", "gcz", "wbfs", "ciso", "wia", "wad", "dol", "elf",
            ],
            Emulator::Eden => &["nsp", "xci", "nro"],
        }
    }

    /// Whether its games are folders rather than files (a Kyty game may also be a ZArchive).
    pub fn games_are_folders(self) -> bool {
        matches!(self, Emulator::Rpcs3 | Emulator::ShadPs4 | Emulator::Kyty)
    }

    /// What it starts of `dir` when that is a game's folder, relative to it: the folder itself
    /// when empty.
    fn folder_game(self, dir: &Path) -> Option<PathBuf> {
        let has = |rel: &str| dir.join(rel).is_file();
        match self {
            Emulator::Rpcs3 => ["PS3_GAME/USRDIR/EBOOT.BIN", "USRDIR/EBOOT.BIN"]
                .into_iter()
                .find(|rel| has(rel))
                .map(PathBuf::from),
            Emulator::ShadPs4 => {
                (has("eboot.bin") && has("sce_sys/param.sfo")).then(|| "eboot.bin".into())
            }
            Emulator::Kyty => (has("eboot.bin") && has("sce_sys/param.json")).then(PathBuf::new),
            _ => None,
        }
    }

    /// What it is given to start `game`, full screen, and to quit as the game ends.
    fn args(self, game: &str) -> Vec<String> {
        let args: &[&str] = match self {
            // `--`: what follows is the file, even one whose name starts with a dash.
            Emulator::DuckStation | Emulator::Pcsx2 => &["-batch", "-fullscreen", "--"],
            Emulator::Ppsspp => &["--fullscreen"],
            Emulator::Rpcs3 => &["--no-gui", "--fullscreen"],
            Emulator::ShadPs4 => &["--fullscreen", "true", "-g"],
            Emulator::Kyty => &["--fullscreen", "--game"],
            Emulator::Dolphin => &["-b", "-e"],
            Emulator::Eden => &["-f", "-g"],
        };
        args.iter()
            .map(|a| a.to_string())
            .chain([game.to_string()])
            .collect()
    }

    /// Its program: the one chosen in Settings, else the first of its names in `PATH`.
    pub fn program(self, db: &Db) -> Result<Option<PathBuf>> {
        if let Some(chosen) = crate::settings::emulator_program(db, self)? {
            return Ok(Some(chosen));
        }
        Ok(self
            .programs()
            .iter()
            .find_map(|name| crate::doctor::find_in_path(name)))
    }
}

/// What starts an emulated game.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Game {
    pub emulator: Emulator,
    /// What the emulator is given, in the game's folder: a file, or the folder itself when empty.
    pub start: PathBuf,
}

/// A game to add.
#[derive(Debug, Clone)]
pub struct NewGame {
    /// Its title: read from the game, or from its file's name, when not given.
    pub title: Option<String>,
    /// Its file, or its folder for a system whose games are folders.
    pub path: PathBuf,
    pub emulator: Emulator,
}

/// The emulators that can start `path`: by its extension, or by what its folder holds.
pub fn emulators_for(path: &Path) -> Vec<Emulator> {
    Emulator::ALL
        .into_iter()
        .filter(|e| match path.is_dir() {
            true => e.folder_game(path).is_some(),
            false => has_extension(path, e.extensions()),
        })
        .collect()
}

fn has_extension(path: &Path, extensions: &[&str]) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| extensions.contains(&e.to_ascii_lowercase().as_str()))
}

/// The folder a game's install names and what its emulator is given in it, for `path`, as it is
/// on disk; refused when `emulator` cannot start it.
fn placed(path: &Path, emulator: Emulator) -> Result<(PathBuf, PathBuf)> {
    let path = path
        .canonicalize()
        .map_err(|e| Error::io(format!("find {}", path.display()), e))?;
    if path.to_str().is_none() {
        // Emulators are given it as an argument, which is text.
        return Err(Error::Refused(format!(
            "{} is not a name an emulator can be given; rename it",
            path.display()
        )));
    }
    if path.is_dir() {
        if let Some(start) = emulator.folder_game(&path) {
            return Ok((path, start));
        }
    } else if path.is_file()
        && has_extension(&path, emulator.extensions())
        && let (Some(folder), Some(name)) = (path.parent(), path.file_name())
    {
        return Ok((folder.to_path_buf(), PathBuf::from(name)));
    }
    Err(Error::Refused(format!(
        "{} does not start {}",
        emulator.name(),
        path.display()
    )))
}

/// Adds a game, which takes the next number never given.
pub fn add(db: &Db, dirs: &Dirs, new: NewGame) -> Result<Install> {
    let (folder, start) = placed(&new.path, new.emulator)?;
    let full = whole(&folder, &start);
    if let Some(other) = listed(db, dirs)?.get(&full) {
        return Err(Error::Refused(format!(
            "{} is in the library already",
            other.title
        )));
    }
    let title = match new.title.as_deref().map(str::trim) {
        Some(title) if !title.is_empty() => title.to_string(),
        _ => title_of(&full, new.emulator),
    };
    let game_id = next_id(db, dirs)?;
    let game = Game {
        emulator: new.emulator,
        start,
    };
    let install = Install {
        game_id,
        title,
        platform: Platform::Linux,
        path: folder,
        client_id: None,
        runner: Runner::Native,
        umu_id: None,
        // Emulators run as the user's programs do; their saves are in the user's folders.
        isolated: false,
    };
    save_game(dirs, &install.game_id, &game)?;
    if let Err(e) = install.save(db) {
        let _ = std::fs::remove_file(game_file(dirs, &install.game_id));
        return Err(e);
    }
    // Added again on purpose: a scan may find it again too.
    let mut forgotten = crate::settings::forgotten_emulated(db)?;
    if forgotten.remove(&full) {
        crate::settings::set_forgotten_emulated(db, &forgotten)?;
    }
    Ok(install)
}

/// The games in the library run in an emulator, by what their emulator is given.
fn listed(db: &Db, dirs: &Dirs) -> Result<HashMap<PathBuf, Install>> {
    let games = games(dirs)?;
    Ok(Install::list(db)?
        .into_iter()
        .filter_map(|i| {
            let game = games.get(&i.game_id)?;
            Some((whole(&i.path, &game.start), i))
        })
        .collect())
}

/// What starts each emulated game, by game id; one whose file cannot be read is left out.
pub fn games(dirs: &Dirs) -> Result<HashMap<String, Game>> {
    let folder = dirs.data.join("emulated");
    let entries = match std::fs::read_dir(&folder) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(HashMap::new()),
        Err(e) => return Err(Error::io(format!("read {}", folder.display()), e)),
    };
    Ok(entries
        .filter_map(|entry| {
            let name = entry.ok()?.file_name().into_string().ok()?;
            let id = name.strip_suffix(".json")?;
            crate::store::is_emulated(id)
                .then(|| load_game(dirs, id).ok())
                .flatten()
                .map(|game| (id.to_string(), game))
        })
        .collect())
}

/// The emulated games, as the library lists them: with their system, and no image of their own
/// (the user's edits give them some).
pub fn library(db: &Db, dirs: &Dirs) -> Result<Vec<crate::library::LibraryGame>> {
    let games = games(dirs)?;
    Ok(Install::list(db)?
        .iter()
        .filter_map(|i| Some(library_game(i, games.get(&i.game_id)?.emulator)))
        .collect())
}

/// An emulated game as the library lists it.
pub fn library_game(install: &Install, emulator: Emulator) -> crate::library::LibraryGame {
    crate::library::LibraryGame {
        id: install.game_id.clone(),
        title: install.title.clone(),
        cover: None,
        icon: None,
        background: None,
        logo: None,
        os: vec![emulator.system().into()],
        metadata: crate::library::MetadataSource::Missing,
        steam: Some(Vec::new()),
    }
}

/// Takes an emulated game out of the library; its files are left as they are, and a scan does not
/// add it again.
pub fn remove(db: &Db, dirs: &Dirs, game_id: &str) -> Result<()> {
    if !crate::store::is_emulated(game_id) {
        return Err(Error::Refused(format!("{game_id} is not emulated")));
    }
    let _busy = crate::lock::game(dirs, game_id)?;
    let install = Install::get(db, game_id)?
        .ok_or_else(|| Error::NotFound(format!("{game_id} is not in the library")))?;
    if let Ok(game) = load_game(dirs, game_id) {
        let mut forgotten = crate::settings::forgotten_emulated(db)?;
        forgotten.insert(whole(&install.path, &game.start));
        crate::settings::set_forgotten_emulated(db, &forgotten)?;
    }
    Install::remove(db, game_id)?;
    match std::fs::remove_file(game_file(dirs, game_id)) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            return Err(Error::io("delete the emulated game's record", e));
        }
        _ => {}
    }
    crate::custom::forget(db, dirs, game_id)?;
    crate::logs::remove_game(dirs, game_id)?;
    Ok(())
}

/// Finds an emulated game at `path`, where it was moved: a file or a folder its emulator starts.
pub fn locate(db: &Db, dirs: &Dirs, game_id: &str, path: &Path) -> Result<Install> {
    let mut install = Install::get(db, game_id)?
        .filter(|i| crate::store::is_emulated(&i.game_id))
        .ok_or_else(|| Error::NotFound(format!("{game_id} is not emulated")))?;
    let mut game = load_game(dirs, game_id)?;
    let (folder, start) = placed(path, game.emulator)?;
    if let Some(other) = listed(db, dirs)?
        .get(&whole(&folder, &start))
        .filter(|other| other.game_id != game_id)
    {
        return Err(Error::Refused(format!(
            "{} is in the library already",
            other.title
        )));
    }
    game.start = start;
    install.path = folder;
    save_game(dirs, game_id, &game)?;
    install.save(db)?;
    Ok(install)
}

/// How an emulated game starts: its emulator, given its file or folder, checked to be there still.
pub fn launch_spec(db: &Db, dirs: &Dirs, install: &Install) -> Result<LaunchSpec> {
    let game = load_game(dirs, &install.game_id)?;
    let start = whole(&install.path, &game.start);
    if !leads_inside(&install.path, &start) {
        return Err(Error::Refused(format!(
            "{} leads out of the game's folder {}",
            start.display(),
            install.path.display()
        )));
    }
    if !start.exists() {
        return Err(Error::NotFound(format!("{} is missing", start.display())));
    }
    let program = game.emulator.program(db)?.ok_or_else(|| {
        Error::NotFound(format!(
            "{} is not installed: install it, or choose its program in Settings → Emulators",
            game.emulator.name()
        ))
    })?;
    let start = start.to_str().ok_or_else(|| {
        Error::Refused(format!(
            "{} is not a name an emulator can be given",
            start.display()
        ))
    })?;
    Ok(LaunchSpec {
        program,
        args: game.emulator.args(start),
        cwd: install.path.clone(),
        env: Vec::new(),
    })
}

/// A game a scan found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub path: PathBuf,
    pub title: String,
}

/// The games `emulator` starts in `folders` and the library does not list, nor listed until the
/// user removed them; each once, as it is on disk. Files a playlist or a cue sheet names are the
/// discs or tracks of that game, not games of their own.
pub fn scan(db: &Db, dirs: &Dirs, emulator: Emulator, folders: &[PathBuf]) -> Result<Vec<Found>> {
    let known: HashSet<PathBuf> = listed(db, dirs)?
        .into_keys()
        .chain(crate::settings::forgotten_emulated(db)?)
        .collect();
    let mut seen = HashSet::new();
    let mut files = Vec::new();
    let mut named = HashSet::new();
    for folder in folders {
        walk(
            folder, emulator, SCAN_DEPTH, &mut seen, &mut files, &mut named,
        );
    }
    let mut found: Vec<Found> = files
        .into_iter()
        .filter(|path| !named.contains(path) && !known.contains(path))
        .map(|path| Found {
            title: title_of(&path, emulator),
            path,
        })
        .collect();
    found.sort_by(|a, b| a.path.cmp(&b.path));
    found.dedup();
    Ok(found)
}

/// Adds the games a scan found, those that cannot be added left out with why.
pub fn add_found(
    db: &Db,
    dirs: &Dirs,
    emulator: Emulator,
    found: Vec<Found>,
) -> (Vec<Install>, Vec<(PathBuf, Error)>) {
    let mut added = Vec::new();
    let mut failed = Vec::new();
    for f in found {
        let new = NewGame {
            title: Some(f.title),
            // A folder game is added by its folder, its file being what the scan found.
            path: folder_of(&f.path, emulator),
            emulator,
        };
        match add(db, dirs, new) {
            Ok(install) => added.push(install),
            Err(e) => failed.push((f.path, e)),
        }
    }
    (added, failed)
}

/// The folder of a game whose emulator starts it from its folder, from what it starts there; any
/// other path as it is. The outermost: a disc's `PS3_GAME` holds what a downloaded game does.
fn folder_of(path: &Path, emulator: Emulator) -> PathBuf {
    path.ancestors()
        .filter(|dir| {
            emulator
                .folder_game(dir)
                .is_some_and(|start| whole(dir, &start) == path)
        })
        .last()
        .unwrap_or(path)
        .to_path_buf()
}

/// What an emulator is given: `start` in `folder`, or the folder itself.
fn whole(folder: &Path, start: &Path) -> PathBuf {
    match start.as_os_str().is_empty() {
        true => folder.to_path_buf(),
        false => folder.join(start),
    }
}

/// Looks through `dir`, `depth` folders deep, for what `emulator` starts: kept in `files` as it
/// is on disk, with the files playlists and cue sheets name in `named`.
fn walk(
    dir: &Path,
    emulator: Emulator,
    depth: usize,
    seen: &mut HashSet<PathBuf>,
    files: &mut Vec<PathBuf>,
    named: &mut HashSet<PathBuf>,
) {
    // A link back to a folder already seen ends there.
    let Ok(dir) = dir.canonicalize() else { return };
    if depth == 0 || !seen.insert(dir.clone()) {
        return;
    }
    if let Some(start) = emulator.folder_game(&dir) {
        files.push(whole(&dir, &start));
        return;
    }
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, emulator, depth - 1, seen, files, named);
        } else if path.is_file() && has_extension(&path, emulator.extensions()) {
            let Ok(path) = path.canonicalize() else {
                continue;
            };
            named.extend(parts_named(&path));
            files.push(path);
        }
    }
}

/// The files a playlist (`.m3u`, a disc on each line) or a cue sheet (`FILE "…" BINARY`) names,
/// beside it, as they are on disk.
fn parts_named(path: &Path) -> Vec<PathBuf> {
    let (Some(folder), Some(ext)) = (path.parent(), path.extension().and_then(|e| e.to_str()))
    else {
        return Vec::new();
    };
    let ext = ext.to_ascii_lowercase();
    if ext != "m3u" && ext != "cue" {
        return Vec::new();
    }
    // Sheets are small: a large file of that name is not one.
    let Some(text) = read_small(path, 64 * 1024) else {
        return Vec::new();
    };
    let text = String::from_utf8_lossy(&text);
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            if ext == "m3u" {
                (!line.is_empty() && !line.starts_with('#')).then_some(line)
            } else {
                let rest = line.strip_prefix("FILE ")?;
                let quoted = rest.strip_prefix('"')?;
                Some(&quoted[..quoted.find('"')?])
            }
        })
        .filter_map(|name| folder.join(name).canonicalize().ok())
        .collect()
}

fn read_small(path: &Path, limit: u64) -> Option<Vec<u8>> {
    use std::io::Read;
    let file = std::fs::File::open(path).ok()?;
    if file.metadata().ok()?.len() > limit {
        return None;
    }
    let mut bytes = Vec::new();
    file.take(limit).read_to_end(&mut bytes).ok()?;
    Some(bytes)
}

/// A game's title: the one it gives itself, for the systems whose games are folders, else its
/// file's or folder's name without the tags that sets of discs add ("(USA)", "[!]").
fn title_of(path: &Path, emulator: Emulator) -> String {
    let folder = folder_of(path, emulator);
    let given = match emulator {
        Emulator::Rpcs3 => ["PS3_GAME/PARAM.SFO", "PARAM.SFO"]
            .iter()
            .find_map(|rel| sfo_title(&folder.join(rel))),
        Emulator::ShadPs4 => sfo_title(&folder.join("sce_sys/param.sfo")),
        Emulator::Kyty => json_title(&folder.join("sce_sys/param.json")),
        _ => None,
    };
    let name = match folder == path && !path.is_dir() {
        true => path.file_stem(),
        false => folder.file_name(),
    };
    given
        .or_else(|| name.map(|n| clean_title(&n.to_string_lossy())))
        .unwrap_or_default()
}

/// `name` without the tags at its end, its underscores as spaces.
fn clean_title(name: &str) -> String {
    let mut title = name.trim_end();
    while let Some(open) = match title.chars().last() {
        Some(')') => title.rfind('('),
        Some(']') => title.rfind('['),
        _ => None,
    } {
        title = title[..open].trim_end();
    }
    let title = title.replace('_', " ");
    let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
    match title.is_empty() {
        true => name.to_string(),
        false => title,
    }
}

/// The `TITLE` a PlayStation `PARAM.SFO` gives.
fn sfo_title(path: &Path) -> Option<String> {
    let sfo = read_small(path, 1024 * 1024)?;
    let u16_at = |at: usize| Some(u16::from_le_bytes(sfo.get(at..at + 2)?.try_into().ok()?));
    let u32_at =
        |at: usize| Some(u32::from_le_bytes(sfo.get(at..at + 4)?.try_into().ok()?) as usize);
    if sfo.get(..4)? != b"\0PSF" {
        return None;
    }
    let (keys, data, count) = (u32_at(8)?, u32_at(12)?, u32_at(16)?);
    let text_at = |at: usize, len: usize| {
        let bytes = sfo.get(at..at.checked_add(len)?.min(sfo.len()))?;
        let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
        std::str::from_utf8(&bytes[..end]).ok()
    };
    (0..count.min(256)).find_map(|i| {
        let entry = 20 + i * 16;
        let key_at = keys.checked_add(u16_at(entry)? as usize)?;
        let key = text_at(key_at, 64)?;
        if key != "TITLE" {
            return None;
        }
        let title = text_at(data.checked_add(u32_at(entry + 12)?)?, u32_at(entry + 4)?)?;
        let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
        (!title.is_empty()).then_some(title)
    })
}

/// The title a PlayStation 5 `param.json` gives in its default language.
fn json_title(path: &Path) -> Option<String> {
    let json: serde_json::Value = serde_json::from_slice(&read_small(path, 1024 * 1024)?).ok()?;
    let names = json.get("localizedParameters")?;
    let language = names.get("defaultLanguage")?.as_str()?;
    let title = names.get(language)?.get("titleName")?.as_str()?.trim();
    (!title.is_empty()).then(|| title.to_string())
}

/// The next number never given to an emulated game, counted in the database at once, so that two
/// processes adding games never give the same.
fn next_number(db: &Db) -> Result<u64> {
    let n: String = db.conn().query_row(
        "INSERT INTO settings (key, value) VALUES (?1, '1')
         ON CONFLICT(key) DO UPDATE SET value = CAST(value AS INTEGER) + 1
         RETURNING value",
        [EMULATED],
        |r| r.get(0),
    )?;
    n.parse()
        .map_err(|_| Error::Refused(format!("the count of games emulated is `{n}`")))
}

/// A number never given, whose id nothing is left of either (a database made again while the
/// data folder stayed would count from 1 again).
fn next_id(db: &Db, dirs: &Dirs) -> Result<String> {
    loop {
        let id = crate::store::emulated_id(next_number(db)?);
        if std::fs::symlink_metadata(game_file(dirs, &id)).is_err()
            && Install::get(db, &id)?.is_none()
        {
            return Ok(id);
        }
    }
}

fn game_file(dirs: &Dirs, game_id: &str) -> PathBuf {
    dirs.data.join("emulated").join(format!("{game_id}.json"))
}

fn save_game(dirs: &Dirs, game_id: &str, game: &Game) -> Result<()> {
    crate::paths::check_owned_id(game_id)?;
    let json = serde_json::to_vec_pretty(game).map_err(|e| Error::parse("emulated game", e))?;
    crate::fsutil::write_atomic(&game_file(dirs, game_id), &json)
}

fn load_game(dirs: &Dirs, game_id: &str) -> Result<Game> {
    crate::paths::check_owned_id(game_id)?;
    let path = game_file(dirs, game_id);
    let bytes =
        std::fs::read(&path).map_err(|e| Error::io(format!("read {}", path.display()), e))?;
    serde_json::from_slice(&bytes).map_err(|e| Error::parse("emulated game", e))
}

#[cfg(test)]
mod tests;
