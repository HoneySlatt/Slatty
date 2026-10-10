//! What the user changed about how a game looks in the library: its title, the title it is sorted
//! by, its cover and its background. Kept apart from GOG's data so that refreshing the library never
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
    let mut stmt =
        conn.prepare("SELECT game_id, title, sort_title, cover, background FROM game_custom")?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            Custom {
                title: r.get(1)?,
                sort_title: r.get(2)?,
                cover: r.get::<_, Option<String>>(3)?.map(PathBuf::from),
                background: r.get::<_, Option<String>>(4)?.map(PathBuf::from),
            },
        ))
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

fn get(db: &Db, game_id: &str) -> Result<Custom> {
    Ok(db
        .conn()
        .query_row(
            "SELECT title, sort_title, cover, background FROM game_custom WHERE game_id = ?1",
            [game_id],
            |r| {
                Ok(Custom {
                    title: r.get(0)?,
                    sort_title: r.get(1)?,
                    cover: r.get::<_, Option<String>>(2)?.map(PathBuf::from),
                    background: r.get::<_, Option<String>>(3)?.map(PathBuf::from),
                })
            },
        )
        .optional()?
        .unwrap_or_default())
}

/// Saves a game's customisation. Empty titles mean GOG's; a game with nothing left customised is
/// forgotten, along with its copied images.
pub fn save(
    db: &Db,
    dirs: &Dirs,
    game_id: &str,
    title: &str,
    sort_title: &str,
    cover: ImageChange,
    background: ImageChange,
) -> Result<Custom> {
    if game_id.is_empty() || !game_id.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(Error::Refused(format!("unexpected game id `{game_id}`")));
    }
    let old = get(db, game_id)?;
    let folder = dirs.data.join("custom").join(game_id);
    let text = |s: &str| Some(s.trim().to_string()).filter(|s| !s.is_empty());
    let custom = Custom {
        title: text(title),
        sort_title: text(sort_title),
        cover: apply(&folder, "cover", old.cover.as_deref(), cover)?,
        background: apply(&folder, "background", old.background.as_deref(), background)?,
    };
    if custom == Custom::default() {
        db.conn()
            .execute("DELETE FROM game_custom WHERE game_id = ?1", [game_id])?;
        let _ = std::fs::remove_dir(&folder);
    } else {
        let path = |p: &Option<PathBuf>| p.as_ref().map(|p| p.to_string_lossy().into_owned());
        db.conn().execute(
            "INSERT INTO game_custom (game_id, title, sort_title, cover, background)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(game_id) DO UPDATE SET title = excluded.title,
                sort_title = excluded.sort_title, cover = excluded.cover,
                background = excluded.background",
            params![
                game_id,
                custom.title,
                custom.sort_title,
                path(&custom.cover),
                path(&custom.background)
            ],
        )?;
    }
    Ok(custom)
}

/// The image to keep after `change`. A new image gets a new file name, so nothing keeps showing
/// the previous one from a cache; the replaced copy is deleted.
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
    if let Some(old) = old.filter(|o| o.starts_with(folder)) {
        let _ = std::fs::remove_file(old);
    }
    Ok(new)
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

    #[test]
    fn titles_and_images_are_kept_and_survive_the_original_file() {
        let env = Env::new("save");
        let source = env.picture("mine.png", PNG);
        let custom = save(
            &env.db,
            &env.dirs,
            "42",
            "  My Game ",
            "Game, My",
            ImageChange::Set(source.clone()),
            ImageChange::Keep,
        )
        .unwrap();
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
        let first = save(
            &env.db,
            &env.dirs,
            "42",
            "",
            "",
            ImageChange::Set(env.picture("a.png", PNG)),
            ImageChange::Keep,
        )
        .unwrap()
        .cover
        .unwrap();
        let second = save(
            &env.db,
            &env.dirs,
            "42",
            "",
            "",
            ImageChange::Set(env.picture("b.png", PNG)),
            ImageChange::Keep,
        )
        .unwrap()
        .cover
        .unwrap();
        assert_ne!(
            first, second,
            "a new name, so no cache shows the old picture"
        );
        assert!(!first.exists());
        let custom = save(
            &env.db,
            &env.dirs,
            "42",
            "",
            "",
            ImageChange::Reset,
            ImageChange::Keep,
        )
        .unwrap();
        assert_eq!(custom, Custom::default());
        assert!(!second.exists());
        assert!(
            all(&env.db).unwrap().is_empty(),
            "nothing left: the game is forgotten"
        );
    }

    #[test]
    fn only_images_are_accepted() {
        let env = Env::new("reject");
        let text = env.picture("notes.txt", b"[FAKE] just text");
        assert!(matches!(
            save(
                &env.db,
                &env.dirs,
                "42",
                "",
                "",
                ImageChange::Set(text),
                ImageChange::Keep
            ),
            Err(Error::Refused(_))
        ));
        assert!(matches!(
            save(
                &env.db,
                &env.dirs,
                "../x",
                "T",
                "",
                ImageChange::Keep,
                ImageChange::Keep
            ),
            Err(Error::Refused(_))
        ));
        assert!(is_image(&env.picture("ok.png", PNG)));
    }
}
