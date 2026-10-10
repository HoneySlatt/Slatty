//! What the user changed about how a game looks in the library: its title, the title it is sorted
//! by, its cover, its background, and whether it is hidden. Kept apart from GOG's data so that refreshing the library never
//! undoes it; chosen images are copied into slatty's data folder, so moving or deleting the original
//! file breaks nothing.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use rusqlite::{OptionalExtension, params};

use crate::db::Db;
use crate::error::{Error, Result};
use crate::fsutil;
use crate::paths::Dirs;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Custom {
    pub title: Option<String>,
    pub sort_title: Option<String>,
    pub cover: Option<PathBuf>,
    pub background: Option<PathBuf>,
    /// Left out of the library's shelves, shown only with the hidden games.
    pub hidden: bool,
}

/// What the edit form saves. Empty titles mean GOG's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Changes {
    pub title: String,
    pub sort_title: String,
    pub hidden: bool,
    pub cover: ImageChange,
    pub background: ImageChange,
}

/// What saving does to one of the game's images.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageChange {
    Keep,
    /// Use a copy of this image file.
    Set(PathBuf),
    /// Back to GOG's image.
    Reset,
}

/// Every game the user customised.
pub fn all(db: &Db) -> Result<HashMap<String, Custom>> {
    let conn = db.conn();
    let mut stmt = conn
        .prepare("SELECT game_id, title, sort_title, cover, background, hidden FROM game_custom")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, read(r, 1)?)))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

fn get(db: &Db, game_id: &str) -> Result<Custom> {
    Ok(db
        .conn()
        .query_row(
            "SELECT title, sort_title, cover, background, hidden FROM game_custom
             WHERE game_id = ?1",
            [game_id],
            |r| read(r, 0),
        )
        .optional()?
        .unwrap_or_default())
}

/// A `Custom` from the columns starting at `first`.
fn read(r: &rusqlite::Row<'_>, first: usize) -> rusqlite::Result<Custom> {
    Ok(Custom {
        title: r.get(first)?,
        sort_title: r.get(first + 1)?,
        cover: r.get::<_, Option<String>>(first + 2)?.map(PathBuf::from),
        background: r.get::<_, Option<String>>(first + 3)?.map(PathBuf::from),
        hidden: r.get(first + 4)?,
    })
}

/// Saves a game's customisation. A game with nothing left customised is forgotten, along with its
/// copied images.
pub fn save(db: &Db, dirs: &Dirs, game_id: &str, changes: Changes) -> Result<Custom> {
    if game_id.is_empty() || !game_id.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(Error::Refused(format!("unexpected game id `{game_id}`")));
    }
    let old = get(db, game_id)?;
    let folder = dirs.data.join("custom").join(game_id);
    let text = |s: &str| Some(s.trim().to_string()).filter(|s| !s.is_empty());
    let cover = apply(&folder, "cover", old.cover.as_deref(), changes.cover)?;
    let background = apply(
        &folder,
        "background",
        old.background.as_deref(),
        changes.background,
    )
    .inspect_err(|_| remove_replaced(&folder, cover.as_deref(), old.cover.as_deref()))?;
    let custom = Custom {
        title: text(&changes.title),
        sort_title: text(&changes.sort_title),
        cover,
        background,
        hidden: changes.hidden,
    };
    let saved = if custom == Custom::default() {
        db.conn()
            .execute("DELETE FROM game_custom WHERE game_id = ?1", [game_id])
    } else {
        let path = |p: &Option<PathBuf>| p.as_ref().map(|p| p.to_string_lossy().into_owned());
        db.conn().execute(
            "INSERT INTO game_custom (game_id, title, sort_title, cover, background, hidden)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(game_id) DO UPDATE SET title = excluded.title,
                sort_title = excluded.sort_title, cover = excluded.cover,
                background = excluded.background, hidden = excluded.hidden",
            params![
                game_id,
                custom.title,
                custom.sort_title,
                path(&custom.cover),
                path(&custom.background),
                custom.hidden
            ],
        )
    };
    if let Err(e) = saved {
        remove_replaced(&folder, custom.cover.as_deref(), old.cover.as_deref());
        remove_replaced(
            &folder,
            custom.background.as_deref(),
            old.background.as_deref(),
        );
        return Err(e.into());
    }
    // The database owns the new copies now; failures above never remove its previous images.
    remove_replaced(&folder, old.cover.as_deref(), custom.cover.as_deref());
    remove_replaced(
        &folder,
        old.background.as_deref(),
        custom.background.as_deref(),
    );
    if custom == Custom::default() {
        let _ = std::fs::remove_dir(&folder);
    }
    Ok(custom)
}

/// The image to keep after `change`. A new image gets a new file name, so nothing keeps showing
/// the previous one from a cache. The previous copy stays until the database write succeeds.
fn apply(
    folder: &Path,
    art: &str,
    old: Option<&Path>,
    change: ImageChange,
) -> Result<Option<PathBuf>> {
    let new = match change {
        ImageChange::Keep => return Ok(old.map(Path::to_path_buf)),
        ImageChange::Reset => None,
        ImageChange::Set(source) => {
            let bytes = std::fs::read(&source)
                .map_err(|e| Error::io(format!("read {}", source.display()), e))?;
            let ext = image_extension(&bytes).ok_or_else(|| {
                Error::Refused(format!(
                    "{} is not a PNG, JPEG, WebP, GIF or BMP image",
                    source.display()
                ))
            })?;
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let dest = folder.join(format!("{art}-{stamp}.{ext}"));
            fsutil::write_atomic(&dest, &bytes)?;
            Some(dest)
        }
    };
    Ok(new)
}

fn remove_replaced(folder: &Path, old: Option<&Path>, kept: Option<&Path>) {
    if let Some(old) = old.filter(|o| Some(*o) != kept && o.starts_with(folder)) {
        let _ = std::fs::remove_file(old);
    }
}

fn image_extension(bytes: &[u8]) -> Option<&'static str> {
    match bytes {
        [0x89, b'P', b'N', b'G', ..] => Some("png"),
        [0xFF, 0xD8, 0xFF, ..] => Some("jpg"),
        [
            b'R',
            b'I',
            b'F',
            b'F',
            _,
            _,
            _,
            _,
            b'W',
            b'E',
            b'B',
            b'P',
            ..,
        ] => Some("webp"),
        [b'G', b'I', b'F', b'8', ..] => Some("gif"),
        [b'B', b'M', ..] => Some("bmp"),
        _ => None,
    }
}

/// Whether a file looks like an image the interface can show, from its first bytes.
pub fn is_image(path: &Path) -> bool {
    let mut head = [0u8; 16];
    std::fs::File::open(path)
        .and_then(|mut f| f.read(&mut head))
        .is_ok_and(|n| image_extension(&head[..n]).is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n[FAKE] not a real picture";

    struct Env {
        root: PathBuf,
        dirs: Dirs,
        db: Db,
    }

    impl Env {
        fn new(name: &str) -> Env {
            let root =
                std::env::temp_dir().join(format!("slatty-custom-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(&root).unwrap();
            Env {
                dirs: Dirs::under(&root.join("app")),
                db: Db::in_memory().unwrap(),
                root,
            }
        }

        fn picture(&self, name: &str, bytes: &[u8]) -> PathBuf {
            let path = self.root.join(name);
            std::fs::write(&path, bytes).unwrap();
            path
        }
    }

    impl Drop for Env {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    /// Changes that only touch the cover.
    fn cover(change: ImageChange) -> Changes {
        Changes {
            title: String::new(),
            sort_title: String::new(),
            hidden: false,
            cover: change,
            background: ImageChange::Keep,
        }
    }

    #[test]
    fn titles_and_images_are_kept_and_survive_the_original_file() {
        let env = Env::new("save");
        let source = env.picture("mine.png", PNG);
        let changes = Changes {
            title: "  My Game ".into(),
            sort_title: "Game, My".into(),
            ..cover(ImageChange::Set(source.clone()))
        };
        let custom = save(&env.db, &env.dirs, "42", changes).unwrap();
        assert_eq!(custom.title.as_deref(), Some("My Game"));
        assert_eq!(custom.sort_title.as_deref(), Some("Game, My"));
        let cover = custom.cover.clone().unwrap();
        assert!(cover.starts_with(env.dirs.data.join("custom/42")));
        std::fs::remove_file(source).unwrap();
        assert_eq!(std::fs::read(&cover).unwrap(), PNG, "a copy, not a link");
        assert_eq!(all(&env.db).unwrap()["42"], custom);
    }

    #[test]
    fn replacing_or_resetting_an_image_deletes_the_previous_copy() {
        let env = Env::new("replace");
        let set = |name| cover(ImageChange::Set(env.picture(name, PNG)));
        let first = save(&env.db, &env.dirs, "42", set("a.png"))
            .unwrap()
            .cover
            .unwrap();
        let second = save(&env.db, &env.dirs, "42", set("b.png"))
            .unwrap()
            .cover
            .unwrap();
        assert_ne!(
            first, second,
            "a new name, so no cache shows the old picture"
        );
        assert!(!first.exists());
        let custom = save(&env.db, &env.dirs, "42", cover(ImageChange::Reset)).unwrap();
        assert_eq!(custom, Custom::default());
        assert!(!second.exists());
        assert!(
            all(&env.db).unwrap().is_empty(),
            "nothing left: the game is forgotten"
        );
    }

    #[test]
    fn a_hidden_game_stays_hidden_until_shown_again() {
        let env = Env::new("hidden");
        let hide = |hidden| Changes {
            hidden,
            ..cover(ImageChange::Keep)
        };
        save(&env.db, &env.dirs, "42", hide(true)).unwrap();
        assert!(all(&env.db).unwrap()["42"].hidden, "kept with nothing else");
        save(&env.db, &env.dirs, "42", hide(false)).unwrap();
        assert!(all(&env.db).unwrap().is_empty());
    }

    #[test]
    fn only_images_are_accepted() {
        let env = Env::new("reject");
        let text = env.picture("notes.txt", b"[FAKE] just text");
        assert!(matches!(
            save(&env.db, &env.dirs, "42", cover(ImageChange::Set(text))),
            Err(Error::Refused(_))
        ));
        let renamed = Changes {
            title: "T".into(),
            ..cover(ImageChange::Keep)
        };
        assert!(matches!(
            save(&env.db, &env.dirs, "../x", renamed),
            Err(Error::Refused(_))
        ));
        assert!(is_image(&env.picture("ok.png", PNG)));
    }

    #[test]
    fn a_failed_background_change_preserves_the_previous_cover() {
        let env = Env::new("partial-failure");
        let first = save(
            &env.db,
            &env.dirs,
            "42",
            cover(ImageChange::Set(env.picture("first.png", PNG))),
        )
        .unwrap();
        let result = save(
            &env.db,
            &env.dirs,
            "42",
            Changes {
                background: ImageChange::Set(env.picture("invalid.txt", b"[FAKE] not an image")),
                ..cover(ImageChange::Set(env.picture("second.png", PNG)))
            },
        );
        assert!(result.is_err());
        assert_eq!(all(&env.db).unwrap()["42"], first);
        assert!(first.cover.unwrap().exists());
        assert_eq!(
            std::fs::read_dir(env.dirs.data.join("custom/42"))
                .unwrap()
                .count(),
            1
        );
    }

    #[test]
    fn a_failed_database_write_preserves_custom_images() {
        let env = Env::new("db-failure");
        let first = save(
            &env.db,
            &env.dirs,
            "42",
            cover(ImageChange::Set(env.picture("first.png", PNG))),
        )
        .unwrap();
        env.db
            .conn()
            .execute_batch("PRAGMA query_only = ON")
            .unwrap();
        assert!(save(&env.db, &env.dirs, "42", cover(ImageChange::Reset)).is_err());
        assert_eq!(all(&env.db).unwrap()["42"], first);
        assert!(first.cover.unwrap().exists());
    }
}
