//! Covers of the library grid made at the size they are drawn. Iced draws an image larger than its
//! place by reading four of its pixels for each one shown, with no smaller copies to read from: a
//! cover two or three times as wide as its card shows grainy, its fine lines broken. Each one is
//! made once at the card's widest, in pixels, with Lanczos3, off the interface thread at the lowest
//! priority, kept in the cache, and shown in place of the original once made.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use iced::Task;
use iced::widget::image;
use tokio::sync::Semaphore;

use crate::{App, Message};

/// Covers made at once, each on a thread of its own.
const AT_ONCE: usize = 4;
/// Covers made by one task, which sends them in one message.
const PER_MESSAGE: usize = 16;
/// A cover at most this much wider than its card is shown as it is: drawn smaller by less than
/// that, it loses no pixel.
const KEPT_UP_TO: f32 = 1.2;

#[derive(Default)]
pub struct ScaledCovers {
    /// The window's scale, once known: nothing is made before.
    pub scale: Option<f32>,
    /// Per game: the cover it was made from, and the copy, or `None` to show that cover as it is.
    ready: HashMap<String, (PathBuf, Option<image::Handle>)>,
    /// Covers being made, so that none is asked twice.
    pending: HashSet<PathBuf>,
    limit: Option<Arc<Semaphore>>,
}

impl App {
    /// The width in pixels the grid's covers are made at: the card's widest. Narrower cards, in a
    /// window the columns do not fill exactly, draw them smaller by less than half.
    fn scaled_width(&self) -> Option<u32> {
        Some((self.card_width * self.scaled.scale?).round() as u32)
    }

    /// The cover the grid shows for a game: its copy at the card's size once made.
    pub fn grid_cover(&self, game_id: &str) -> Option<image::Handle> {
        let cover = self.cover(game_id)?;
        match (self.scaled.ready.get(game_id), &cover) {
            (Some((from, Some(copy))), image::Handle::Path(_, path)) if from == path => {
                Some(copy.clone())
            }
            _ => Some(cover),
        }
    }

    /// The window's scale is known, or changed.
    pub fn rescaled(&mut self, scale: f32) -> Task<Message> {
        if self.scaled.scale == Some(scale) {
            return Task::none();
        }
        self.scaled.scale = Some(scale);
        self.resize_covers()
    }

    /// The covers' size changed (the window's scale, Settings → Library): all are made again.
    pub fn resize_covers(&mut self) -> Task<Message> {
        self.scaled.ready.clear();
        self.scaled.pending.clear();
        // Those waiting to be made at the old size are not; those being made are dropped.
        if let Some(old) = self.scaled.limit.take() {
            old.close();
        }
        Task::batch([self.prune_covers(), self.scale_covers(None)])
    }

    /// Deletes the copies made at other sizes: about 37 KB a cover at 225 pixels, 350 MB at 10,000
    /// games.
    fn prune_covers(&self) -> Task<Message> {
        let (Some(core), Some(width)) = (&self.core, self.scaled_width()) else {
            return Task::none();
        };
        let mut folders = vec![core.dirs.cache.join("scaled")];
        if let Some(account) = &self.account {
            folders.push(core.dirs.account_cache(&account.user_id).join("scaled"));
        }
        Task::future(async move {
            let _ = tokio::task::spawn_blocking(move || prune(&folders, width)).await;
        })
        .discard()
    }

    /// Makes the copies missing, of `ids` or of every cover, those the grid shows first first.
    pub fn scale_covers(&mut self, ids: Option<Vec<String>>) -> Task<Message> {
        let (Some(core), Some(width)) = (self.core.clone(), self.scaled_width()) else {
            return Task::none();
        };
        let ids: Vec<String> = match ids {
            Some(ids) => ids,
            None => self
                .covers
                .keys()
                .chain(
                    self.customs
                        .iter()
                        .filter(|(_, c)| c.cover.is_some())
                        .map(|(id, _)| id),
                )
                .cloned()
                .collect::<HashSet<_>>()
                .into_iter()
                .collect(),
        };
        let mut jobs: Vec<(String, PathBuf)> = ids
            .into_iter()
            .filter_map(|id| {
                let image::Handle::Path(_, path) = self.cover(&id)? else {
                    return None;
                };
                let made = self
                    .scaled
                    .ready
                    .get(&id)
                    .is_some_and(|(from, _)| *from == path);
                (!made && self.scaled.pending.insert(path.clone())).then_some((id, path))
            })
            .collect();
        if jobs.is_empty() {
            return Task::none();
        }
        let order: HashMap<&str, usize> = self
            .visible_games()
            .iter()
            .enumerate()
            .map(|(i, g)| (g.id.as_str(), i))
            .collect();
        jobs.sort_by_key(|(id, _)| order.get(id.as_str()).copied().unwrap_or(usize::MAX));
        let limit = self
            .scaled
            .limit
            .get_or_insert_with(|| Arc::new(Semaphore::new(AT_ONCE)))
            .clone();
        let user_cache = self
            .account
            .as_ref()
            .map(|a| core.dirs.account_cache(&a.user_id));
        let shared = core.dirs.cache.clone();
        Task::batch(jobs.chunks(PER_MESSAGE).map(|chunk| {
            let (chunk, limit) = (chunk.to_vec(), limit.clone());
            let (user_cache, shared) = (user_cache.clone(), shared.clone());
            Task::perform(
                async move {
                    let Ok(_permit) = limit.acquire_owned().await else {
                        return Vec::new();
                    };
                    slatty_core::fsutil::in_background(move || {
                        chunk
                            .into_iter()
                            .map(|(id, from)| {
                                // A GOG cover's copy stays with the account's cache, which
                                // forgetting the account deletes.
                                let cache = match &user_cache {
                                    Some(c) if from.starts_with(c) => c,
                                    _ => &shared,
                                };
                                let dir = cache.join("scaled").join(width.to_string());
                                let copy = scaled_copy(&from, width, &dir);
                                (id, from, copy)
                            })
                            .collect()
                    })
                    .await
                },
                move |made| Message::CoversScaled(width, made),
            )
        }))
    }

    pub fn covers_scaled(&mut self, width: u32, made: Vec<(String, PathBuf, Option<PathBuf>)>) {
        if self.scaled_width() != Some(width) {
            return;
        }
        for (id, from, copy) in made {
            self.scaled.pending.remove(&from);
            let copy = copy.map(image::Handle::from_path);
            self.scaled.ready.insert(id, (from, copy));
        }
    }
}

/// Deletes, in each of `folders`, the copies made at another width than `width`.
fn prune(folders: &[PathBuf], width: u32) {
    let keep = width.to_string();
    let entries = folders.iter().filter_map(|f| std::fs::read_dir(f).ok());
    for entry in entries.flatten().flatten() {
        if entry.file_name() != keep.as_str() {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

/// The copy of `source` `width` pixels wide in `dir`, made unless an earlier one is there; `None`
/// when `source` is shown as it is (`KEPT_UP_TO`) or cannot be read. Named after the width and the
/// source's path, size and modification time, so a changed cover is made again.
pub fn scaled_copy(source: &Path, width: u32, dir: &Path) -> Option<PathBuf> {
    let meta = std::fs::metadata(source).ok()?;
    let id = format!(
        "{width}\n{}\n{}\n{:?}",
        source.display(),
        meta.len(),
        meta.modified().ok()
    );
    let name = &slatty_core::fsutil::sha256_hex(id.as_bytes())[..32];
    for ext in ["jpg", "png"] {
        let made = dir.join(format!("{name}.{ext}"));
        // Empty: written just before the computer stopped, never synced.
        if std::fs::metadata(&made).is_ok_and(|m| m.is_file() && m.len() > 0) {
            return Some(made);
        }
    }
    let reader = || {
        ::image::ImageReader::open(source)
            .ok()?
            .with_guessed_format()
            .ok()
    };
    let (source_width, _) = reader()?.into_dimensions().ok()?;
    if source_width as f32 <= width as f32 * KEPT_UP_TO {
        return None;
    }
    let small =
        reader()?
            .decode()
            .ok()?
            .resize(width, u32::MAX, ::image::imageops::FilterType::Lanczos3);
    let mut out = Vec::new();
    let (written, ext) = if small.color().has_alpha() {
        let png = small.write_to(
            &mut std::io::Cursor::new(&mut out),
            ::image::ImageFormat::Png,
        );
        (png, "png")
    } else {
        let encoder = ::image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 92);
        (small.to_rgb8().write_with_encoder(encoder), "jpg")
    };
    written.ok()?;
    // A copy is made again when lost: not synced to disk, which a hard disk would take 10 ms each
    // for, file after file.
    let made = dir.join(format!("{name}.{ext}"));
    let tmp = slatty_core::fsutil::temp_sibling(&made);
    slatty_core::paths::ensure_dir(dir).ok()?;
    let written = std::fs::write(&tmp, &out).and_then(|()| std::fs::rename(&tmp, &made));
    if written.is_err() {
        let _ = std::fs::remove_file(&tmp);
        return None;
    }
    Some(made)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, image: ::image::DynamicImage, format: ::image::ImageFormat) {
        let mut out = Vec::new();
        image
            .write_to(&mut std::io::Cursor::new(&mut out), format)
            .unwrap();
        std::fs::write(path, out).unwrap();
    }

    #[test]
    fn a_cover_is_made_once_at_the_cards_width_unless_already_about_that_wide() {
        let dir = std::env::temp_dir().join(format!("slatty-scaled-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let copies = dir.join("scaled");
        let cover = dir.join("1.jpg");
        let jpeg = ::image::ImageFormat::Jpeg;
        write(&cover, ::image::RgbImage::new(600, 900).into(), jpeg);

        let made = scaled_copy(&cover, 225, &copies).unwrap();
        assert_eq!(::image::image_dimensions(&made).unwrap(), (225, 338));
        let first = std::fs::metadata(&made).unwrap().modified().unwrap();
        assert_eq!(scaled_copy(&cover, 225, &copies), Some(made.clone()));
        assert_eq!(
            std::fs::metadata(&made).unwrap().modified().unwrap(),
            first,
            "not made again"
        );

        // A cover changed since: made again, under another name.
        write(&cover, ::image::RgbImage::new(342, 482).into(), jpeg);
        let again = scaled_copy(&cover, 225, &copies).unwrap();
        assert_ne!(again, made);
        assert_eq!(::image::image_dimensions(&again).unwrap().0, 225);

        // At most a fifth wider than the card: shown as it is.
        assert_eq!(scaled_copy(&cover, 300, &copies), None);

        // Transparency stays.
        let logo = dir.join("2.png");
        let png = ::image::ImageFormat::Png;
        write(&logo, ::image::RgbaImage::new(600, 900).into(), png);
        let made = scaled_copy(&logo, 150, &copies).unwrap();
        assert_eq!(made.extension().unwrap(), "png");
        assert!(::image::open(&made).unwrap().color().has_alpha());

        assert_eq!(scaled_copy(&dir.join("missing.jpg"), 150, &copies), None);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn copies_made_at_other_sizes_are_deleted() {
        let dir = std::env::temp_dir().join(format!("slatty-prune-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let (shared, account) = (dir.join("cache/scaled"), dir.join("cache/1/scaled"));
        for folder in [&shared, &account] {
            for width in ["225", "300"] {
                std::fs::create_dir_all(folder.join(width)).unwrap();
                std::fs::write(folder.join(width).join("a.jpg"), b"jpg").unwrap();
            }
        }
        prune(&[shared.clone(), account.clone(), dir.join("missing")], 300);
        for folder in [&shared, &account] {
            assert!(!folder.join("225").exists());
            assert!(folder.join("300/a.jpg").exists());
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
}
