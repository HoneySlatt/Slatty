//! Covers of the library grid and the game page's key art made at the size they are drawn. Iced
//! draws an image larger than its place by reading four of its pixels for each one shown, with no
//! smaller copies to read from: a cover two or three times as wide as its card shows grainy, its
//! fine lines broken. And it enlarges one smaller than its place by blending its four nearest
//! pixels, softly: key art of 1920 pixels across a screen of 3840. Each one is made once at the
//! size drawn, in pixels, with Lanczos3 (and AMD's RCAS sharpening, FSR 1's, when enlarged), off the
//! interface thread at the lowest priority, kept in the cache, and shown in place of the original
//! once made.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

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
/// Key art at most this much narrower than its place is shown as it is.
const ENLARGED_FROM: f32 = 1.05;
/// The widths key art is made at: the window's, rounded up to one of these, so that resizing the
/// window makes none again. Wider windows enlarge the widest themselves: one of 3840 pixels takes
/// about 33 MB once decoded.
const ART_WIDTHS: [u32; 3] = [1920, 2560, 3840];
/// The most pixels key art is enlarged to, those of a 4K screen at 16:10: sharpening holds four
/// floats a pixel twice, about 300 MB. Taller key art, of which the page shows a band, is shown as
/// it is.
const ENLARGED_PIXELS: f32 = 3840.0 * 2400.0;
/// The game page draws its key art this much wider than the window (`ui::game::hero`).
const ART_ENLARGED: f32 = 1.06;

#[derive(Default)]
pub struct ScaledCovers {
    /// The window's scale, once known: nothing is made before.
    pub scale: Option<f32>,
    /// Per game: the cover it was made from, and the copy, or `None` to show that cover as it is.
    ready: HashMap<String, (PathBuf, Option<image::Handle>)>,
    /// Covers being made, so that none is asked twice.
    pending: HashSet<PathBuf>,
    limit: Option<Arc<Semaphore>>,
    /// Per key art and width: its copy, or `None` to show it as it is.
    art: HashMap<(PathBuf, u32), Option<image::Handle>>,
    art_pending: HashSet<(PathBuf, u32)>,
    /// When the first key art copy was asked for: those shown since are kept (`trim`).
    session: Option<SystemTime>,
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

    /// The width in pixels key art is made at (`ART_WIDTHS`).
    fn art_width(&self) -> Option<u32> {
        let drawn = self.window.width * self.scaled.scale? * ART_ENLARGED;
        Some(
            ART_WIDTHS
                .into_iter()
                .find(|w| *w as f32 >= drawn)
                .unwrap_or(ART_WIDTHS[ART_WIDTHS.len() - 1]),
        )
    }

    /// The key art drawn for `art`: its copy at the window's width once made.
    pub fn shown_art(&self, art: image::Handle) -> image::Handle {
        let copy = match (&art, self.art_width()) {
            (image::Handle::Path(_, path), Some(width)) => self
                .scaled
                .art
                .get(&(path.clone(), width))
                .cloned()
                .flatten(),
            _ => None,
        };
        copy.unwrap_or(art)
    }

    /// Makes the copy of the open game's key art at the window's width, unless made.
    pub fn scale_art(&mut self) -> Task<Message> {
        let (Some(core), Some(width)) = (self.core.clone(), self.art_width()) else {
            return Task::none();
        };
        let Some(image::Handle::Path(_, from)) =
            self.selected_game().and_then(|g| self.background(g))
        else {
            return Task::none();
        };
        let key = (from.clone(), width);
        if self.scaled.art.contains_key(&key) || !self.scaled.art_pending.insert(key) {
            return Task::none();
        }
        let account = self
            .account
            .as_ref()
            .map(|a| core.dirs.account_cache(&a.user_id));
        let dir = account
            .clone()
            .filter(|c| from.starts_with(c))
            .unwrap_or_else(|| core.dirs.cache.clone())
            .join("art")
            .join(width.to_string());
        let folders: Vec<PathBuf> = [Some(core.dirs.cache.clone()), account]
            .into_iter()
            .flatten()
            .map(|c| c.join("art"))
            .collect();
        // Before the copy's own time, even on a file system that keeps times to two seconds.
        let session = *self
            .scaled
            .session
            .get_or_insert_with(|| SystemTime::now() - Duration::from_secs(10));
        self.with_account(Task::perform(
            {
                let from = from.clone();
                slatty_core::fsutil::in_background(move || {
                    let copy = scaled_copy(&from, width, &dir, true);
                    trim(&folders, ART_BUDGET, session);
                    copy
                })
            },
            move |copy| Message::ArtScaled(from.clone(), width, copy),
        ))
    }

    /// Copies may go into the account's cache: made for an account signed out or forgotten since,
    /// they are dropped, and deleted with its cache (`App::account_task`).
    fn with_account(&self, task: Task<Message>) -> Task<Message> {
        if self.account.is_some() {
            self.account_task(task)
        } else {
            task
        }
    }

    /// Forgets the copies an account's covers were shown with, and stops making those waiting.
    pub fn forget_scaled(&mut self) {
        if let Some(old) = self.scaled.limit.take() {
            old.close();
        }
        self.scaled = ScaledCovers {
            scale: self.scaled.scale,
            ..ScaledCovers::default()
        };
    }

    pub fn art_scaled(&mut self, from: PathBuf, width: u32, copy: Option<PathBuf>) {
        let key = (from, width);
        self.scaled.art_pending.remove(&key);
        self.scaled
            .art
            .insert(key, copy.map(image::Handle::from_path));
    }

    /// The window's scale is known, or changed.
    pub fn rescaled(&mut self, scale: f32) -> Task<Message> {
        if self.scaled.scale == Some(scale) {
            return Task::none();
        }
        self.scaled.scale = Some(scale);
        Task::batch([self.resize_covers(), self.scale_art()])
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
        self.with_account(Task::batch(jobs.chunks(PER_MESSAGE).map(|chunk| {
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
                                let copy = scaled_copy(&from, width, &dir, false);
                                (id, from, copy)
                            })
                            .collect()
                    })
                    .await
                },
                move |made| Message::CoversScaled(width, made),
            )
        })))
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
/// when `source` is shown as it is (`KEPT_UP_TO`, and `ENLARGED_FROM` with `enlarge`; a narrower
/// one without) or cannot be read. Named after the width and the source's path, size and
/// modification time, so a changed cover is made again.
pub fn scaled_copy(source: &Path, width: u32, dir: &Path, enlarge: bool) -> Option<PathBuf> {
    let meta = std::fs::metadata(source).ok()?;
    let id = format!(
        "{width}\n{enlarge}\n{}\n{}\n{:?}",
        source.display(),
        meta.len(),
        meta.modified().ok()
    );
    let name = &slatty_core::fsutil::sha256_hex(id.as_bytes())[..32];
    for ext in ["jpg", "png"] {
        let made = dir.join(format!("{name}.{ext}"));
        // Empty: written just before the computer stopped, never synced.
        if std::fs::metadata(&made).is_ok_and(|m| m.is_file() && m.len() > 0) {
            // Key art shown again is the last `trim` deletes.
            if enlarge {
                let _ = std::fs::File::options()
                    .write(true)
                    .open(&made)
                    .and_then(|f| f.set_modified(std::time::SystemTime::now()));
            }
            return Some(made);
        }
    }
    let reader = || {
        ::image::ImageReader::open(source)
            .ok()?
            .with_guessed_format()
            .ok()
    };
    let (source_width, source_height) = reader()?.into_dimensions().ok()?;
    let ratio = width as f32 / source_width as f32;
    let enlarged = enlarge && ratio > ENLARGED_FROM;
    if !enlarged && ratio * KEPT_UP_TO >= 1.0
        || enlarged && width as f32 * source_height as f32 * ratio > ENLARGED_PIXELS
    {
        return None;
    }
    let small =
        reader()?
            .decode()
            .ok()?
            .resize(width, u32::MAX, ::image::imageops::FilterType::Lanczos3);
    let small = if enlarged { rcas(&small) } else { small };
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

/// The most the key art copies take: about 1 to 2 MB each at 3840 pixels, more than a thousand
/// games' key art, where every game of a library of 10,000 would take 10 to 20 GB.
const ART_BUDGET: u64 = 2 << 30;

/// Deletes the key art copies in `folders` (`<folder>/<width>/<copy>`) shown longest ago, until
/// they take a tenth less than `budget`, once they take more. Those shown since `session` began
/// stay: the session draws them from their file, and would not make them again.
fn trim(folders: &[PathBuf], budget: u64, session: SystemTime) {
    let mut copies: Vec<(std::time::SystemTime, u64, PathBuf)> = folders
        .iter()
        .filter_map(|f| std::fs::read_dir(f).ok())
        .flatten()
        .flatten()
        .filter_map(|width| std::fs::read_dir(width.path()).ok())
        .flatten()
        .flatten()
        .filter_map(|copy| {
            let meta = copy.metadata().ok().filter(|m| m.is_file())?;
            Some((meta.modified().ok()?, meta.len(), copy.path()))
        })
        .collect();
    let mut taken: u64 = copies.iter().map(|(_, len, _)| len).sum();
    if taken <= budget {
        return;
    }
    copies.sort();
    for (shown, len, path) in copies {
        if taken <= budget - budget / 10 || shown >= session {
            break;
        }
        if std::fs::remove_file(&path).is_ok() {
            taken -= len;
        }
    }
}

/// AMD FidelityFX RCAS, FSR 1's sharpening, at its usual strength (0.2 stops): gives an enlarged
/// image back the edges Lanczos3 softens, without the halos of an unsharp mask. Ported from
/// `ffx_fsr1.h` (MIT licence), on the processor. Transparency is kept as it was.
fn rcas(image: &::image::DynamicImage) -> ::image::DynamicImage {
    const LIMIT: f32 = 0.25 - 1.0 / 16.0;
    let strength = (-0.2f32).exp2();
    let src = image.to_rgba32f();
    let (w, h) = src.dimensions();
    let at = |x: i64, y: i64| {
        let x = x.clamp(0, w as i64 - 1) as u32;
        let y = y.clamp(0, h as i64 - 1) as u32;
        src.get_pixel(x, y).0
    };
    let mut out = src.clone();
    for (x, y, pixel) in out.enumerate_pixels_mut() {
        let (x, y) = (x as i64, y as i64);
        let (n, w_, e, s) = (at(x, y - 1), at(x - 1, y), at(x + 1, y), at(x, y + 1));
        let c = pixel.0;
        let mut lobe = f32::MIN;
        for i in 0..3 {
            let min = n[i].min(w_[i]).min(e[i]).min(s[i]);
            let max = n[i].max(w_[i]).max(e[i]).max(s[i]);
            let hit_min = min / (4.0 * max).max(1e-6);
            let hit_max = (1.0 - max) / (4.0 * min - 4.0).min(-1e-6);
            lobe = lobe.max((-hit_min).max(hit_max));
        }
        let lobe = (-LIMIT).max(lobe.min(0.0)) * strength;
        let scale = 1.0 / (4.0 * lobe + 1.0);
        for i in 0..3 {
            pixel.0[i] = ((lobe * (n[i] + w_[i] + e[i] + s[i]) + c[i]) * scale).clamp(0.0, 1.0);
        }
    }
    let sharpened = ::image::DynamicImage::ImageRgba32F(out);
    if image.color().has_alpha() {
        ::image::DynamicImage::ImageRgba8(sharpened.to_rgba8())
    } else {
        ::image::DynamicImage::ImageRgb8(sharpened.to_rgb8())
    }
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

        let made = scaled_copy(&cover, 225, &copies, false).unwrap();
        assert_eq!(::image::image_dimensions(&made).unwrap(), (225, 338));
        let first = std::fs::metadata(&made).unwrap().modified().unwrap();
        assert_eq!(scaled_copy(&cover, 225, &copies, false), Some(made.clone()));
        assert_eq!(
            std::fs::metadata(&made).unwrap().modified().unwrap(),
            first,
            "not made again"
        );

        // A cover changed since: made again, under another name.
        write(&cover, ::image::RgbImage::new(342, 482).into(), jpeg);
        let again = scaled_copy(&cover, 225, &copies, false).unwrap();
        assert_ne!(again, made);
        assert_eq!(::image::image_dimensions(&again).unwrap().0, 225);

        // At most a fifth wider than the card: shown as it is.
        assert_eq!(scaled_copy(&cover, 300, &copies, false), None);

        // Transparency stays.
        let logo = dir.join("2.png");
        let png = ::image::ImageFormat::Png;
        write(&logo, ::image::RgbaImage::new(600, 900).into(), png);
        let made = scaled_copy(&logo, 150, &copies, false).unwrap();
        assert_eq!(made.extension().unwrap(), "png");
        assert!(::image::open(&made).unwrap().color().has_alpha());

        assert_eq!(
            scaled_copy(&dir.join("missing.jpg"), 150, &copies, false),
            None
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn key_art_narrower_than_its_place_is_enlarged_and_sharpened() {
        let dir = std::env::temp_dir().join(format!("slatty-enlarge-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let (art, copies) = (dir.join("art.jpg"), dir.join("art"));
        let jpeg = ::image::ImageFormat::Jpeg;
        write(&art, ::image::RgbImage::new(1920, 620).into(), jpeg);

        let made = scaled_copy(&art, 3840, &copies, true).unwrap();
        assert_eq!(::image::image_dimensions(&made).unwrap(), (3840, 1240));
        // Nearly as wide as its place: shown as it is.
        assert_eq!(scaled_copy(&art, 2000, &copies, true), None);
        // A cover is never enlarged.
        assert_eq!(scaled_copy(&art, 3840, &copies, false), None);
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// Tall key art (a background chosen by hand, a phone's wallpaper) would take gigabytes once
    /// enlarged to the window's width: it is shown as it is.
    #[test]
    fn tall_key_art_is_not_enlarged_past_a_screens_pixels() {
        let dir = std::env::temp_dir().join(format!("slatty-tall-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let (art, copies) = (dir.join("tall.jpg"), dir.join("art"));
        write(
            &art,
            ::image::RgbImage::new(300, 1000).into(),
            ::image::ImageFormat::Jpeg,
        );
        assert_eq!(scaled_copy(&art, 1920, &copies, true), None);
        assert!(!copies.exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// RCAS steepens an edge, leaves a flat area and transparency as they were.
    #[test]
    fn rcas_sharpens_edges_only() {
        let edge = ::image::RgbaImage::from_fn(8, 8, |x, _| {
            let v = if x < 4 { 60 } else { 200 };
            ::image::Rgba([v, v, v, 128])
        });
        let sharp = rcas(&edge.into()).to_rgba8();
        let (dark, light) = (sharp.get_pixel(3, 4), sharp.get_pixel(4, 4));
        assert!(dark[0] < 60 && light[0] > 200, "{dark:?} {light:?}");
        assert_eq!(sharp.get_pixel(0, 4)[0], 60);
        assert_eq!(sharp.get_pixel(7, 4)[0], 200);
        assert_eq!(dark[3], 128);
    }

    /// Over the budget, the key art shown longest ago goes first, until a tenth below it; key art
    /// shown again counts as shown last.
    #[test]
    fn key_art_shown_longest_ago_goes_once_over_the_budget() {
        let dir = std::env::temp_dir().join(format!("slatty-trim-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let (shared, account) = (dir.join("cache/art"), dir.join("cache/1/art"));
        let at = |n: u64| std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(n);
        let copies = [
            (shared.join("3840/old.jpg"), 1),
            (account.join("1920/older.jpg"), 0),
            (account.join("3840/new.jpg"), 3),
            (shared.join("2560/newer.jpg"), 4),
        ];
        for (path, when) in &copies {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, [0u8; 100]).unwrap();
            let f = std::fs::File::options().write(true).open(path).unwrap();
            f.set_modified(at(*when)).unwrap();
        }
        let folders = [shared.clone(), account.clone()];
        trim(&folders, 400, at(10));
        assert!(copies.iter().all(|(p, _)| p.exists()), "within the budget");
        // Shown since the launcher started: the session still draws them from those files.
        trim(&folders, 100, at(1));
        assert!(!copies[1].0.exists(), "shown before");
        assert!(copies[0].0.exists() && copies[2].0.exists() && copies[3].0.exists());
        trim(&folders, 250, at(10));
        // 300 bytes over 250: down to 225, so the one shown longest ago goes.
        assert!(!copies[0].0.exists());
        assert!(copies[2].0.exists() && copies[3].0.exists());

        // Key art shown again: its copy is the last to go.
        let art = dir.join("art.jpg");
        write(
            &art,
            ::image::RgbImage::new(1920, 620).into(),
            ::image::ImageFormat::Jpeg,
        );
        let made = scaled_copy(&art, 3840, &account.join("3840"), true).unwrap();
        std::fs::File::options()
            .write(true)
            .open(&made)
            .unwrap()
            .set_modified(at(2))
            .unwrap();
        scaled_copy(&art, 3840, &account.join("3840"), true).unwrap();
        let shown = std::fs::metadata(&made).unwrap().modified().unwrap();
        assert!(shown > at(1_000_000_000));
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
