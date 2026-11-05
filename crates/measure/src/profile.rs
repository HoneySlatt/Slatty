//! The fake profile of 10,000 games that the interface is measured on: a signed-in `[FAKE]`
//! account, its library as GOG's would be cached, a JPEG cover for each game and key art for 200
//! of them, all made here. It lives in a throwaway folder of its own, laid out as a home folder
//! (`home/`), so that the program measured finds nothing of the person running it.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use image::ExtendedColorType;
use image::codecs::jpeg::JpegEncoder;
use slatty_core::db::Db;
use slatty_core::library::{self, LibraryGame, MetadataSource};
use slatty_core::paths::Dirs;

/// The games in the library.
pub const GAMES: usize = 10_000;
/// The games with key art, opened one after the other when memory is measured.
pub const WITH_KEY_ART: usize = 200;
/// GOG's covers: 342×482, about 86 KB (these weigh 87 KB on average).
const COVER: Picture = Picture {
    size: (342, 482),
    grain: 32,
};
/// Key art as GOG serves it for a wide screen, about 1.2 MB.
const KEY_ART: Picture = Picture {
    size: (2560, 1440),
    grain: 12,
};

/// A kind of picture: its size, and the grain that makes it weigh about what GOG's do.
#[derive(Clone, Copy)]
struct Picture {
    size: (u32, u32),
    grain: u8,
}
/// A file that marks a folder this program made, with the version of its layout: only such a folder
/// is measured, never another.
const MARKER: &str = "slatty-measure-profile";
/// The layout's version: a profile of another one is refused, and made again.
const VERSION: &str = "1";
/// The account: a made-up GOG user id, digits as GOG's are.
const USER_ID: &str = "10000000000000042";

/// A fake profile, made by `make`.
pub struct Profile {
    pub root: PathBuf,
}

impl Profile {
    /// The profile in `root`: made there when `root` is missing or empty, else the one this program
    /// made there before. Any other folder is refused, so that nothing of a person's own is ever
    /// measured or changed.
    pub fn open_or_make(root: &Path) -> Result<Self> {
        // Absolute: the runtime folders inside name sockets, which a relative path cannot.
        let root = &std::path::absolute(root)?;
        let marker = root.join(MARKER);
        match std::fs::read_to_string(&marker) {
            Ok(version) if version.trim() == VERSION => Ok(Self {
                root: root.to_path_buf(),
            }),
            Ok(_) => bail!(
                "{} holds a fake profile of another version: delete the folder, it is made again",
                root.display()
            ),
            Err(_) => make(root),
        }
    }

    /// The home folder the program measured is given.
    pub fn home(&self) -> PathBuf {
        self.root.join("home")
    }

    /// The XDG folders the program measured is given, inside its home folder.
    pub fn xdg(&self) -> [(&'static str, PathBuf); 4] {
        let home = self.home();
        [
            ("XDG_CONFIG_HOME", home.join(".config")),
            ("XDG_DATA_HOME", home.join(".local/share")),
            ("XDG_CACHE_HOME", home.join(".cache")),
            ("XDG_STATE_HOME", home.join(".local/state")),
        ]
    }

    /// Slatty's folders, as it finds them from `xdg`.
    fn dirs(&self) -> Dirs {
        let [config, data, cache, state] = self.xdg().map(|(_, path)| path.join("slatty"));
        Dirs {
            config,
            data,
            cache,
            state,
            runtime: None,
        }
    }

    /// Where the runs write their logs and reports.
    pub fn runs(&self) -> PathBuf {
        self.root.join("runs")
    }
}

/// Makes the fake profile in `root`, which must be missing or empty. The marker is written last: a
/// profile cut off half-way is refused, not measured.
fn make(root: &Path) -> Result<Profile> {
    if root.exists() {
        let mut entries =
            std::fs::read_dir(root).with_context(|| format!("reading {}", root.display()))?;
        ensure!(
            entries.next().is_none(),
            "{} is not empty and holds no fake profile: give an empty or missing folder",
            root.display()
        );
    }
    let profile = Profile {
        root: root.to_path_buf(),
    };
    let dirs = profile.dirs();
    eprintln!(
        "making the fake profile of {GAMES} games in {}",
        root.display()
    );
    let db = Db::open(&dirs.db_file())?;
    db.conn().execute(
        "INSERT INTO accounts (user_id, username, added_at) VALUES (?1, ?2, 0)",
        (USER_ID, "[FAKE] Measure"),
    )?;
    db.set_setting("active_user", Some(USER_ID))?;
    let games: Vec<LibraryGame> = (0..GAMES).map(game).collect();
    let cache = dirs.account_cache(USER_ID);
    let covers = cache.join("covers");
    let images = cache.join("images");
    std::fs::create_dir_all(&covers)?;
    std::fs::create_dir_all(&images)?;
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let jobs: Vec<(PathBuf, Picture, u64)> = games
        .iter()
        .enumerate()
        .flat_map(|(n, g)| {
            let cover = (covers.join(format!("{}.jpg", g.id)), COVER, n as u64);
            let art = g.background.as_ref().map(|url| {
                let name = slatty_core::fsutil::sha256_hex(url.as_bytes());
                (images.join(format!("{name}.jpg")), KEY_ART, n as u64)
            });
            std::iter::once(cover).chain(art)
        })
        .collect();
    std::thread::scope(|scope| -> Result<()> {
        let workers: Vec<_> = jobs
            .chunks(jobs.len().div_ceil(threads))
            .map(|chunk| {
                scope.spawn(move || -> Result<()> {
                    for (path, kind, seed) in chunk {
                        let jpeg = picture(*kind, *seed)?;
                        std::fs::write(path, jpeg)
                            .with_context(|| format!("writing {}", path.display()))?;
                    }
                    Ok(())
                })
            })
            .collect();
        for worker in workers {
            worker.join().expect("a picture worker panicked")?;
        }
        Ok(())
    })?;
    library::save_cache(&dirs, USER_ID, games)?;
    std::fs::write(root.join(MARKER), format!("{VERSION}\n"))?;
    Ok(profile)
}

/// The `n`th game. Titles as GOG's can be (long, accented, not only Latin), in no particular
/// order; one game in five has a Linux build. The titles and their order are those of the
/// `library_at_10000_games` benchmark, written again here rather than shared: that benchmark's
/// helpers are the interface's test code, and this program must not depend on the interface.
fn game(n: usize) -> LibraryGame {
    let shuffled = (n * 7919) % GAMES;
    let id = (1_000_000_000 + shuffled).to_string();
    let mut os = vec!["windows".to_string()];
    if shuffled.is_multiple_of(5) {
        os.push("linux".into());
    }
    LibraryGame {
        cover: Some(format!("https://images.gog.com/fake-{id}.jpg")),
        icon: None,
        background: (shuffled < WITH_KEY_ART)
            .then(|| format!("https://images.gog.com/fake-{id}-key-art.jpg")),
        logo: None,
        os,
        metadata: MetadataSource::Gamesdb,
        steam: Some(Vec::new()),
        title: format!("[FAKE] Épopée 世界 {shuffled}: The Long Journey"),
        id,
    }
}

/// A JPEG of `kind`, different for each `seed`: a gradient between two colours, a few blocks and
/// fine grain, so that it weighs about what GOG's do and decodes as slowly.
fn picture(kind: Picture, seed: u64) -> Result<Vec<u8>> {
    let (width, height) = kind.size;
    let mut random = SplitMix(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ u64::from(width));
    let top = random.colour();
    let bottom = random.colour();
    let blocks: Vec<(u32, u32, u32, u32, [u8; 3])> = (0..6)
        .map(|_| {
            let x = random.below(width);
            let y = random.below(height);
            (
                x,
                y,
                random.below(width / 3) + 8,
                random.below(height / 4) + 8,
                random.colour(),
            )
        })
        .collect();
    let mut pixels = Vec::with_capacity((width * height * 3) as usize);
    for y in 0..height {
        for x in 0..width {
            let mut colour = blocks
                .iter()
                .find(|(bx, by, bw, bh, _)| x >= *bx && x < bx + bw && y >= *by && y < by + bh)
                .map(|block| block.4)
                .unwrap_or_else(|| {
                    let t = y as f32 / height as f32;
                    std::array::from_fn(|c| {
                        (f32::from(top[c]) * (1.0 - t) + f32::from(bottom[c]) * t) as u8
                    })
                });
            let spread = u64::from(kind.grain) * 2 + 1;
            let grain = (random.next() % spread) as i16 - i16::from(kind.grain);
            for c in &mut colour {
                *c = (i16::from(*c) + grain).clamp(0, 255) as u8;
            }
            pixels.extend_from_slice(&colour);
        }
    }
    let mut jpeg = Vec::new();
    JpegEncoder::new_with_quality(&mut jpeg, 90).encode(
        &pixels,
        width,
        height,
        ExtendedColorType::Rgb8,
    )?;
    Ok(jpeg)
}

/// SplitMix64: a small, fast generator, the same on every machine, so that every profile is the
/// same.
struct SplitMix(u64);

impl SplitMix {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, bound: u32) -> u32 {
        (self.next() % u64::from(bound.max(1))) as u32
    }

    fn colour(&mut self) -> [u8; 3] {
        let bits = self.next();
        [bits as u8, (bits >> 8) as u8, (bits >> 16) as u8]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_that_is_neither_empty_nor_a_profile_is_refused() {
        let root = std::env::temp_dir().join(format!("slatty-measure-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("someone's file"), "kept").unwrap();
        let refused = Profile::open_or_make(&root);
        assert!(refused.is_err());
        assert_eq!(
            std::fs::read_to_string(root.join("someone's file")).unwrap(),
            "kept"
        );
        assert_eq!(
            std::fs::read_dir(&root).unwrap().count(),
            1,
            "nothing added"
        );
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn games_are_fake_distinct_and_valid_ids() {
        let games: Vec<LibraryGame> = (0..GAMES).map(game).collect();
        let ids: std::collections::HashSet<&str> = games.iter().map(|g| g.id.as_str()).collect();
        assert_eq!(ids.len(), GAMES);
        assert!(games.iter().all(|g| g.title.starts_with("[FAKE] ")));
        assert!(
            games
                .iter()
                .all(|g| slatty_core::paths::check_game_id(&g.id).is_ok())
        );
        assert_eq!(
            games.iter().filter(|g| g.background.is_some()).count(),
            WITH_KEY_ART
        );
    }

    #[test]
    fn pictures_are_the_same_each_time_and_differ_by_seed() {
        let small = Picture {
            size: (64, 48),
            grain: 32,
        };
        let one = picture(small, 1).unwrap();
        assert_eq!(one, picture(small, 1).unwrap());
        assert_ne!(one, picture(small, 2).unwrap());
        assert!(image::load_from_memory(&one).is_ok());
    }
}
