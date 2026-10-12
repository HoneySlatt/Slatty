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
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use iced::Task;
use iced::widget::image;
use tokio::sync::Semaphore;

use crate::upscale::{Enlarge, Stopped, Upscaler};
use crate::{App, Message};

/// Covers made at once, each on a thread of its own.
const AT_ONCE: usize = 4;
/// Covers made by one task, which sends them in one message.
const PER_MESSAGE: usize = 16;
/// The same with AI: about 0.1 s a cover, after a second for the upscaler to start.
const PER_RUN: usize = 64;
/// A cover at most this much wider than its card is shown as it is: drawn smaller by less than
/// that, it loses no pixel.
const KEPT_UP_TO: f32 = 1.2;
/// Key art at most this much narrower than its place is shown as it is.
const ENLARGED_FROM: f32 = 1.05;
/// The widths key art is made at: the window's, rounded up to one of these, so that resizing the
/// window makes none again. Wider windows enlarge the widest themselves: one of 3840 pixels takes
/// about 33 MB once decoded.
const ART_WIDTHS: [u32; 3] = [1920, 2560, 3840];
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
    /// The upscaler found at start, and whether Enhance images with AI is on.
    pub upscaler: Option<Arc<Upscaler>>,
    pub enhance: bool,
    /// Whether a game SlattyLauncher started runs, which stops the upscaler.
    pub game: Arc<AtomicBool>,
    /// Whether a failure of the upscaler was told, once.
    told: bool,
    /// Whether images left for a game are already to be asked again.
    again: bool,
}

/// Images made by one task, sent in one message.
#[derive(Debug, Clone)]
pub struct Batch {
    pub width: u32,
    pub ai: bool,
    /// Per game (empty for key art): the source, and its copy, not made when a game stopped the
    /// upscaler (outer `None`).
    pub made: Vec<(String, PathBuf, Option<Option<PathBuf>>)>,
    pub failed: Option<String>,
}

/// Images left for a game that ran are asked again after this.
const AGAIN_AFTER: std::time::Duration = std::time::Duration::from_secs(60);

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
        let (Some(_), Some(width)) = (&self.core, self.art_width()) else {
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
        let job = Job {
            dir: self.cache_of(&from).join("art").join(self.folder(width)),
            source: from,
            width,
            enlarge: true,
        };
        let ai = self.ai();
        let with_ai = ai.is_some();
        let shared = self.core.as_ref().map(|c| c.dirs.cache.clone());
        let folders: Vec<PathBuf> = [shared, self.account_cache()]
            .into_iter()
            .flatten()
            .map(|c| c.join("art"))
            .collect();
        Task::perform(
            slatty_core::fsutil::in_background(move || {
                let (mut made, failed) = make_copies(std::slice::from_ref(&job), ai.as_ref());
                trim(&folders, ART_BUDGET);
                let made = vec![(String::new(), job.source, made.pop().flatten())];
                Batch {
                    width,
                    ai: with_ai,
                    made,
                    failed,
                }
            }),
            Message::ArtScaled,
        )
    }

    pub fn art_scaled(&mut self, batch: Batch) -> Task<Message> {
        let current = batch.ai == self.ai_on();
        let mut left = false;
        for (_, from, copy) in batch.made {
            let key = (from, batch.width);
            self.scaled.art_pending.remove(&key);
            match copy {
                Some(copy) if current => {
                    self.scaled
                        .art
                        .insert(key, copy.map(image::Handle::from_path));
                }
                Some(_) => {}
                None => left = true,
            }
        }
        self.after_batch(batch.failed, left)
    }

    /// Tells the upscaler's first failure, and asks again for what a game left.
    fn after_batch(&mut self, failed: Option<String>, left: bool) -> Task<Message> {
        if let Some(e) = failed
            && !std::mem::replace(&mut self.scaled.told, true)
        {
            self.notify_error(format!("Images not enhanced: {e}"));
        }
        if !left || std::mem::replace(&mut self.scaled.again, true) {
            return Task::none();
        }
        Task::perform(async { tokio::time::sleep(AGAIN_AFTER).await }, |()| {
            Message::ScaleAgain
        })
    }

    /// Asks again for the images a game left.
    pub fn scale_again(&mut self) -> Task<Message> {
        self.scaled.again = false;
        Task::batch([self.scale_covers(None), self.scale_art()])
    }

    fn ai_on(&self) -> bool {
        self.scaled.enhance && self.scaled.upscaler.is_some()
    }

    /// The folder of the copies made at `width`, apart for those made with AI, so that pruning
    /// the others deletes them.
    fn folder(&self, width: u32) -> String {
        if self.ai_on() {
            format!("{width}-ai")
        } else {
            width.to_string()
        }
    }

    fn account_cache(&self) -> Option<PathBuf> {
        let core = self.core.as_ref()?;
        Some(core.dirs.account_cache(&self.account.as_ref()?.user_id))
    }

    /// The cache an image's copies go to: the account's for GOG's images, which forgetting the
    /// account deletes; the shared one otherwise.
    fn cache_of(&self, source: &Path) -> PathBuf {
        let Some(core) = &self.core else {
            return PathBuf::new();
        };
        self.account
            .as_ref()
            .map(|a| core.dirs.account_cache(&a.user_id))
            .filter(|c| source.starts_with(c))
            .unwrap_or_else(|| core.dirs.cache.clone())
    }

    /// What enhances images with AI, once turned on and found: a working folder of its own per
    /// batch, and whether a game runs, SlattyLauncher's or one the Steam client started (read
    /// once a second while the upscaler works).
    fn ai(&self) -> Option<Ai> {
        static BATCH: AtomicU32 = AtomicU32::new(0);
        let (core, upscaler) = (self.core.as_ref()?, self.scaled.upscaler.clone()?);
        if !self.scaled.enhance {
            return None;
        }
        let n = BATCH.fetch_add(1, Ordering::Relaxed);
        let work = work_folder(&core.dirs).join(n.to_string());
        let (ours, asked, steam) = (
            self.scaled.game.clone(),
            AtomicU32::new(0),
            AtomicBool::new(false),
        );
        let game = move || {
            if asked.fetch_add(1, Ordering::Relaxed) % 10 == 0 {
                let running = slatty_core::steam::running::running(Path::new("/proc"));
                steam.store(!running.is_empty(), Ordering::Relaxed);
            }
            ours.load(Ordering::Relaxed) || steam.load(Ordering::Relaxed)
        };
        Some(Ai {
            upscaler,
            work,
            shaders: core.dirs.cache.join("upscale-shaders"),
            game: Arc::new(game),
        })
    }

    /// Enhance images with AI turned on or off: every copy is made again.
    pub fn set_enhance(&mut self, on: bool) -> Task<Message> {
        self.scaled.enhance = on;
        self.scaled.told = false;
        self.scaled.art.clear();
        self.scaled.art_pending.clear();
        Task::batch([self.resize_covers(), self.scale_art()])
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
        let keep = self.folder(width);
        Task::future(async move {
            let _ = tokio::task::spawn_blocking(move || prune(&folders, &keep)).await;
        })
        .discard()
    }

    /// Makes the copies missing, of `ids` or of every cover, those the grid shows first first.
    pub fn scale_covers(&mut self, ids: Option<Vec<String>>) -> Task<Message> {
        let (Some(_), Some(width)) = (&self.core, self.scaled_width()) else {
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
        let with_ai = self.ai_on();
        // With AI, more covers to a run of the upscaler, which takes about a second to start.
        let (at_once, per_task) = if with_ai {
            (2, PER_RUN)
        } else {
            (AT_ONCE, PER_MESSAGE)
        };
        let limit = self
            .scaled
            .limit
            .get_or_insert_with(|| Arc::new(Semaphore::new(at_once)))
            .clone();
        let folder = self.folder(width);
        Task::batch(jobs.chunks(per_task).map(|chunk| {
            let ids: Vec<String> = chunk.iter().map(|(id, _)| id.clone()).collect();
            let jobs: Vec<Job> = chunk
                .iter()
                .map(|(_, from)| Job {
                    source: from.clone(),
                    width,
                    dir: self.cache_of(from).join("scaled").join(&folder),
                    enlarge: false,
                })
                .collect();
            let (limit, ai) = (limit.clone(), self.ai());
            // Not made, the size having changed since: nothing to tell.
            Task::future(async move {
                let Ok(_permit) = limit.acquire_owned().await else {
                    return None;
                };
                let batch = slatty_core::fsutil::in_background(move || {
                    let (made, failed) = make_copies(&jobs, ai.as_ref());
                    let made = ids
                        .into_iter()
                        .zip(jobs)
                        .zip(made)
                        .map(|((id, job), copy)| (id, job.source, copy))
                        .collect();
                    Batch {
                        width,
                        ai: with_ai,
                        made,
                        failed,
                    }
                });
                Some(batch.await)
            })
            .and_then(|batch| Task::done(Message::CoversScaled(batch)))
        }))
    }

    pub fn covers_scaled(&mut self, batch: Batch) -> Task<Message> {
        if self.scaled_width() != Some(batch.width) || batch.ai != self.ai_on() {
            return Task::none();
        }
        let mut left = false;
        for (id, from, copy) in batch.made {
            self.scaled.pending.remove(&from);
            match copy {
                Some(copy) => {
                    let copy = copy.map(image::Handle::from_path);
                    self.scaled.ready.insert(id, (from, copy));
                }
                None => left = true,
            }
        }
        self.after_batch(batch.failed, left)
    }
}

/// The most the key art copies take: about 1 to 2 MB each at 3840 pixels, more than a thousand
/// games' key art, where every game of a library of 10,000 would take 10 to 20 GB.
const ART_BUDGET: u64 = 2 << 30;

/// Deletes the key art copies in `folders` (`<folder>/<width>/<copy>`) shown longest ago, until
/// they take a tenth less than `budget`, once they take more.
fn trim(folders: &[PathBuf], budget: u64) {
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
    for (_, len, path) in copies {
        if taken <= budget - budget / 10 {
            break;
        }
        if std::fs::remove_file(&path).is_ok() {
            taken -= len;
        }
    }
}

/// Where the upscaler works, a folder per batch.
pub fn work_folder(dirs: &slatty_core::paths::Dirs) -> PathBuf {
    dirs.cache.join("upscale")
}

/// Deletes, in each of `folders`, the copies made at another width, or another way, than those in
/// `keep`.
fn prune(folders: &[PathBuf], keep: &str) {
    let entries = folders.iter().filter_map(|f| std::fs::read_dir(f).ok());
    for entry in entries.flatten().flatten() {
        if entry.file_name() != keep {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

/// A copy to make: of `source`, `width` pixels wide, in `dir`; key art (`enlarge`) is enlarged
/// when narrower, a cover never.
pub struct Job {
    pub source: PathBuf,
    pub width: u32,
    pub dir: PathBuf,
    pub enlarge: bool,
}

/// What enlarges images with AI for `make_copies`.
pub struct Ai {
    pub upscaler: Arc<dyn Enlarge>,
    /// A folder of its own for each batch.
    pub work: PathBuf,
    pub shaders: PathBuf,
    /// Whether a game runs, which stops it.
    pub game: Arc<dyn Fn() -> bool + Send + Sync>,
}

enum Plan {
    Done(Option<PathBuf>),
    Make {
        /// The copy's path without its extension.
        name: PathBuf,
        ai: bool,
    },
}

/// The copy of `source` `width` pixels wide in `dir`, made unless an earlier one is there; `None`
/// when `source` is shown as it is or cannot be read (see `make_copies`).
#[cfg(test)]
fn scaled_copy(source: &Path, width: u32, dir: &Path, enlarge: bool) -> Option<PathBuf> {
    let job = Job {
        source: source.to_path_buf(),
        width,
        dir: dir.to_path_buf(),
        enlarge,
    };
    make_copies(&[job], None).0.pop().flatten().flatten()
}

/// Per job, its copy; not made (outer `None`) when a game stopped the upscaler.
pub type Copies = Vec<Option<Option<PathBuf>>>;

/// Makes the copies of `jobs`, with `ai` when given, in one run of the upscaler. For each: the copy;
/// `None` when the source is shown as it is (a cover at most `KEPT_UP_TO` wider than its place
/// without AI, key art at most `ENLARGED_FROM` narrower or `KEPT_UP_TO` wider) or cannot be read;
/// not made (outer `None`) when a game stopped the upscaler, to be made once it ends. A copy is
/// named after the width, how it was made and its source's path, size and modification time, so a
/// changed image is made again. With AI, a cover is enlarged twice first and made smaller again,
/// which cleans it (unless already twice as wide as its place), and key art narrower than its place
/// is enlarged with it rather than with Lanczos3 and RCAS. Also returns why the upscaler failed, if
/// it did: those it was to enlarge show as they are until the next start.
pub fn make_copies(jobs: &[Job], ai: Option<&Ai>) -> (Copies, Option<String>) {
    let plans: Vec<Plan> = jobs.iter().map(|j| plan(j, ai.is_some())).collect();
    let wants_ai = |p: &Plan| matches!(p, Plan::Make { ai: true, .. });
    let enlarged = match ai {
        Some(ai) if plans.iter().any(wants_ai) => enlarge(jobs, &plans, ai),
        _ => Some(Ok(())),
    };
    let made = jobs
        .iter()
        .zip(&plans)
        .enumerate()
        .map(|(i, (job, plan))| match plan {
            Plan::Done(copy) => Some(copy.clone()),
            Plan::Make { ai: true, .. } if enlarged.is_none() => None,
            Plan::Make { ai: true, .. } if matches!(enlarged, Some(Err(_))) => Some(None),
            Plan::Make { name, ai: wanted } => {
                // One with transparency, which the upscaler would lose, is not given to it.
                let from = match (wanted, ai) {
                    (true, Some(ai)) => ai.work.join("out").join(format!("{i}.png")),
                    _ => job.source.clone(),
                };
                let from = if from.is_file() {
                    from
                } else {
                    job.source.clone()
                };
                Some(finish(&from, job.width, name))
            }
        })
        .collect();
    if let Some(ai) = ai {
        let _ = std::fs::remove_dir_all(&ai.work);
    }
    (made, enlarged.and_then(Result::err))
}

/// Enlarges twice, in one run of the upscaler, the sources `plans` want AI for, from PNGs written
/// here (the upscaler never reads a downloaded file), into `ai.work/out/<index>.png`. `None` when
/// a game stopped it or runs.
fn enlarge(jobs: &[Job], plans: &[Plan], ai: &Ai) -> Option<Result<(), String>> {
    if (ai.game)() {
        return None;
    }
    let input = ai.work.join("in");
    if let Err(e) = std::fs::create_dir_all(&input) {
        return Some(Err(e.to_string()));
    }
    for (i, (job, plan)) in jobs.iter().zip(plans).enumerate() {
        if let Plan::Make { ai: true, .. } = plan {
            let image = reader(&job.source).and_then(|r| r.decode().ok());
            if let Some(image) = image.filter(|i| !i.color().has_alpha()) {
                let _ = image.to_rgb8().save(input.join(format!("{i}.png")));
            }
        }
    }
    // One run on the graphics card at a time.
    static GPU: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _gpu = GPU.lock().unwrap_or_else(|e| e.into_inner());
    match ai.upscaler.enlarge(&ai.work, &ai.shaders, &*ai.game) {
        Ok(()) => Some(Ok(())),
        Err(Stopped::ForAGame) => None,
        Err(Stopped::Failed(e)) => Some(Err(e)),
    }
}

fn plan(job: &Job, ai: bool) -> Plan {
    let Ok(meta) = std::fs::metadata(&job.source) else {
        return Plan::Done(None);
    };
    let id = format!(
        "{}\n{}\n{ai}\n{}\n{}\n{:?}",
        job.width,
        job.enlarge,
        job.source.display(),
        meta.len(),
        meta.modified().ok()
    );
    let name = job
        .dir
        .join(&slatty_core::fsutil::sha256_hex(id.as_bytes())[..32]);
    for ext in ["jpg", "png"] {
        let made = name.with_extension(ext);
        // Empty: written just before the computer stopped, never synced.
        if std::fs::metadata(&made).is_ok_and(|m| m.is_file() && m.len() > 0) {
            // Key art shown again is the last `trim` deletes.
            if job.enlarge {
                let _ = std::fs::File::options()
                    .write(true)
                    .open(&made)
                    .and_then(|f| f.set_modified(std::time::SystemTime::now()));
            }
            return Plan::Done(Some(made));
        }
    }
    let Some((source_width, _)) = reader(&job.source).and_then(|r| r.into_dimensions().ok()) else {
        return Plan::Done(None);
    };
    let ratio = job.width as f32 / source_width as f32;
    let enlarged = job.enlarge && ratio > ENLARGED_FROM;
    let ai = ai && if job.enlarge { enlarged } else { ratio > 0.5 };
    if !ai && !enlarged && ratio * KEPT_UP_TO >= 1.0 {
        return Plan::Done(None);
    }
    Plan::Make { name, ai }
}

fn reader(path: &Path) -> Option<::image::ImageReader<std::io::BufReader<std::fs::File>>> {
    ::image::ImageReader::open(path)
        .ok()?
        .with_guessed_format()
        .ok()
}

/// Makes `from` `width` pixels wide with Lanczos3, sharpened with RCAS when enlarged, and keeps it
/// at `name`.
fn finish(from: &Path, width: u32, name: &Path) -> Option<PathBuf> {
    let image = reader(from)?.decode().ok()?;
    let enlarged = width as f32 > image.width() as f32 * ENLARGED_FROM;
    let small = image.resize(width, u32::MAX, ::image::imageops::FilterType::Lanczos3);
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
    let made = name.with_extension(ext);
    let tmp = slatty_core::fsutil::temp_sibling(&made);
    slatty_core::paths::ensure_dir(name.parent()?).ok()?;
    let written = std::fs::write(&tmp, &out).and_then(|()| std::fs::rename(&tmp, &made));
    if written.is_err() {
        let _ = std::fs::remove_file(&tmp);
        return None;
    }
    Some(made)
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

    /// Stands for the upscaler: makes each image it is given twice as large and green, so a copy
    /// made from its work shows it.
    struct FakeAi {
        runs: std::sync::atomic::AtomicUsize,
        fails: bool,
    }

    impl Enlarge for FakeAi {
        fn enlarge(&self, work: &Path, _: &Path, stop: &dyn Fn() -> bool) -> Result<(), Stopped> {
            self.runs.fetch_add(1, Ordering::Relaxed);
            if stop() {
                return Err(Stopped::ForAGame);
            }
            if self.fails {
                return Err(Stopped::Failed("[FAKE] no device".into()));
            }
            std::fs::create_dir_all(work.join("out")).unwrap();
            for entry in std::fs::read_dir(work.join("in")).unwrap() {
                let path = entry.unwrap().path();
                let (w, h) = ::image::image_dimensions(&path).unwrap();
                ::image::RgbImage::from_pixel(w * 2, h * 2, ::image::Rgb([0, 255, 0]))
                    .save(work.join("out").join(path.file_name().unwrap()))
                    .unwrap();
            }
            Ok(())
        }
    }

    fn green(path: &Path) -> bool {
        let image = ::image::open(path).unwrap().to_rgb8();
        let p = image.get_pixel(image.width() / 2, image.height() / 2);
        p[1] > 200 && p[0] < 50
    }

    /// With AI, covers are cleaned by the upscaler (one run for the whole batch) and made
    /// smaller; one with transparency, or already twice as wide as its card, is not given to it.
    #[test]
    fn covers_are_enhanced_in_one_run_of_the_upscaler() {
        let dir = std::env::temp_dir().join(format!("slatty-ai-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let jpeg = ::image::ImageFormat::Jpeg;
        let png = ::image::ImageFormat::Png;
        write(
            &dir.join("a.jpg"),
            ::image::RgbImage::new(342, 482).into(),
            jpeg,
        );
        write(
            &dir.join("b.jpg"),
            ::image::RgbImage::new(300, 450).into(),
            jpeg,
        );
        write(
            &dir.join("c.png"),
            ::image::RgbaImage::new(342, 482).into(),
            png,
        );
        write(
            &dir.join("d.jpg"),
            ::image::RgbImage::new(700, 1000).into(),
            jpeg,
        );
        let jobs: Vec<Job> = ["a.jpg", "b.jpg", "c.png", "d.jpg"]
            .iter()
            .map(|f| Job {
                source: dir.join(f),
                width: 327,
                dir: dir.join("scaled"),
                enlarge: false,
            })
            .collect();
        let fake = Arc::new(FakeAi {
            runs: Default::default(),
            fails: false,
        });
        let ai = Ai {
            upscaler: fake.clone(),
            work: dir.join("work"),
            shaders: dir.join("shaders"),
            game: Arc::new(|| false),
        };
        let (made, failed) = make_copies(&jobs, Some(&ai));
        assert_eq!(failed, None);
        assert_eq!(fake.runs.load(Ordering::Relaxed), 1);
        let copies: Vec<PathBuf> = made.into_iter().map(|m| m.unwrap().unwrap()).collect();
        for copy in &copies {
            assert_eq!(::image::image_dimensions(copy).unwrap().0, 327);
        }
        assert!(green(&copies[0]) && green(&copies[1]));
        assert!(!green(&copies[2]), "transparency is not given to it");
        assert!(!green(&copies[3]), "twice as wide: made smaller alone");
        assert!(!dir.join("work").exists(), "its working folder goes");
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// A game running stops the upscaler: what it was to make is left for later, the rest made.
    /// A failure leaves them shown as they are, and says why.
    #[test]
    fn a_game_or_a_failure_leaves_images_unenhanced() {
        let dir = std::env::temp_dir().join(format!("slatty-ai-stop-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let jpeg = ::image::ImageFormat::Jpeg;
        write(
            &dir.join("art.jpg"),
            ::image::RgbImage::new(1920, 620).into(),
            jpeg,
        );
        write(
            &dir.join("big.jpg"),
            ::image::RgbImage::new(700, 1000).into(),
            jpeg,
        );
        let jobs = [
            Job {
                source: dir.join("art.jpg"),
                width: 3840,
                dir: dir.join("art"),
                enlarge: true,
            },
            Job {
                source: dir.join("big.jpg"),
                width: 327,
                dir: dir.join("scaled"),
                enlarge: false,
            },
        ];
        let ai = |fails, game: bool| Ai {
            upscaler: Arc::new(FakeAi {
                runs: Default::default(),
                fails,
            }),
            work: dir.join("work"),
            shaders: dir.join("shaders"),
            game: Arc::new(move || game),
        };
        let (made, failed) = make_copies(&jobs, Some(&ai(false, true)));
        assert_eq!(failed, None);
        assert_eq!(made[0], None, "left for later");
        assert!(made[1].clone().unwrap().is_some());

        let (made, failed) = make_copies(&jobs, Some(&ai(true, false)));
        assert_eq!(failed.as_deref(), Some("[FAKE] no device"));
        assert_eq!(made[0], Some(None), "shown as it is");
        std::fs::remove_dir_all(dir).unwrap();
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
            (account.join("1920-ai/older.jpg"), 0),
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
        trim(&folders, 400);
        assert!(copies.iter().all(|(p, _)| p.exists()), "within the budget");
        trim(&folders, 300);
        // 400 bytes over 300: down to 270, so the two shown longest ago go.
        assert!(!copies[1].0.exists() && !copies[0].0.exists());
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
        prune(
            &[shared.clone(), account.clone(), dir.join("missing")],
            "300",
        );
        for folder in [&shared, &account] {
            assert!(!folder.join("225").exists());
            assert!(folder.join("300/a.jpg").exists());
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
}
