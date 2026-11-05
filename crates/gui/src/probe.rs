//! The measuring probe, built with the `probe` feature only (off by default): it writes what
//! `slatty-measure` reads, and acts by itself, so that no input enters the session. Without
//! `SLATTY_PROBE` in the environment it does nothing at all.
//!
//! The log's format names no framework, so that another interface writes the same
//! (`docs/testing.md` describes it): one event a line, `<time> <event> [values]`, the time in
//! nanoseconds since the Unix epoch, so that the measuring program can compare it with its own.
//!
//! - `start`: the probe began, before the window opens;
//! - `window <width> <height> <scale>`: the window's size in its units, and its scale;
//! - `library`: the state holds the library, before any frame shows it; the time from here to
//!   `shown` is the window's own: waiting for the compositor and the graphics card;
//! - `shown`: the first frame drawn with the library in;
//! - `covers`: the first frame drawn with every cover made at the size it is drawn;
//! - `frame`: a frame drawn; `view <nanoseconds>`: how long building the view took;
//! - `begin <step>` and `end <step>`: a step of `SLATTY_PROBE_STEP`;
//! - `no-art <game id>`: a game page whose key art did not show within `ART_WAIT`.
//!
//! The steps, one a run, begin a second after `covers`:
//!
//! - `scroll`: scrolls the library's grid by `SCROLL_STEP` each frame, for `SCROLLED_FRAMES` frames;
//! - `grid`: scrolls it a screen each frame, down to its end;
//! - `pages`: opens the page of `PAGES` games with key art, one after the other, each until its key
//!   art's copy has been drawn twice, or `ART_WAIT` passed.
//!
//! Lines are written as they come, but during a step, where they wait until its end: writing
//! 480 lines a second would weigh on the frames measured.

use std::collections::VecDeque;
use std::fs::File;
use std::io::Write;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use iced::advanced::layout::{self, Layout};
use iced::advanced::widget::{Operation, Tree, Widget};
use iced::advanced::{Clipboard, Shell, mouse, overlay, renderer};
use iced::widget::operation::{self, AbsoluteOffset};
use iced::{Element, Event, Length, Rectangle, Size, Subscription, Vector};

use crate::{App, Message};

/// The frames scrolled by the `scroll` step.
const SCROLLED_FRAMES: usize = 1200;
/// How far the `scroll` step scrolls each frame, in the window's units: 3,840 a second at 240 Hz,
/// a brisk flick.
const SCROLL_STEP: f32 = 16.0;
/// The game pages the `pages` step opens.
const PAGES: usize = 200;
/// How long a game page waits for its key art before the next opens.
const ART_WAIT: Duration = Duration::from_secs(5);
/// How long the probe waits after `covers` before its step, for the start's work to end.
const SETTLE: Duration = Duration::from_secs(1);

static PROBE: Mutex<Option<Probe>> = Mutex::new(None);

struct Probe {
    log: File,
    /// Lines waiting for the end of the step.
    held: Vec<String>,
    step: Option<Step>,
    phase: Phase,
    /// The covers found in the cache at start.
    covers_found: Option<usize>,
    /// The covers made, at the width they were last made at.
    covers_made: (u32, usize),
    /// The frames drawn so far.
    frames: usize,
    /// Whether the state holds the library.
    library_in: bool,
    /// The window's size and scale, as last told.
    window: (f32, f32, f32),
    /// The grid's last scroll: its offset, the height shown and the content's height.
    grid: Option<(f32, f32, f32)>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Step {
    Scroll,
    Grid,
    Pages,
}

impl Step {
    fn name(self) -> &'static str {
        match self {
            Step::Scroll => "scroll",
            Step::Grid => "grid",
            Step::Pages => "pages",
        }
    }
}

enum Phase {
    /// Until the library shows.
    Starting,
    /// Until every cover shows.
    Shown,
    /// Every cover shown; waiting `SETTLE` before the step.
    Settling(Instant),
    /// The step runs: the frame its last action was taken at.
    Running(usize, Running),
    Done,
}

enum Running {
    Scroll {
        from: usize,
    },
    Grid,
    Pages {
        /// The games left to open, with the name of their key art's file.
        left: VecDeque<(String, String)>,
        open: Option<Page>,
    },
}

struct Page {
    id: String,
    /// The name of its key art's file in the cache, as `library::image` keeps it: copies of key art
    /// asked for by a page that waited too long are not this page's.
    art: String,
    since: Instant,
    /// The frame its key art arrived at.
    art_at: Option<usize>,
}

/// Starts the probe when `SLATTY_PROBE` names its log. A step it does not know, or a log it cannot
/// write, leaves it off, said on the standard error: the program runs as without it.
pub fn start() {
    let Some(path) = std::env::var_os("SLATTY_PROBE") else {
        return;
    };
    let step = match std::env::var("SLATTY_PROBE_STEP").as_deref() {
        Err(_) | Ok("") => None,
        Ok("scroll") => Some(Step::Scroll),
        Ok("grid") => Some(Step::Grid),
        Ok("pages") => Some(Step::Pages),
        Ok(other) => {
            eprintln!("probe off: unknown SLATTY_PROBE_STEP {other:?}");
            return;
        }
    };
    let log = match File::create(&path) {
        Ok(log) => log,
        Err(e) => {
            eprintln!("probe off: cannot write {}: {e}", path.display());
            return;
        }
    };
    let mut probe = Probe {
        log,
        held: Vec::new(),
        step,
        phase: Phase::Starting,
        covers_found: None,
        covers_made: (0, 0),
        frames: 0,
        library_in: false,
        window: (0.0, 0.0, 1.0),
        grid: None,
    };
    probe.write("start");
    *PROBE.lock().unwrap_or_else(|e| e.into_inner()) = Some(probe);
}

/// With the probe: what it does with this state.
fn with<T>(f: impl FnOnce(&mut Probe) -> T) -> Option<T> {
    PROBE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_mut()
        .map(f)
}

/// The state's update, seen by the probe, then its next action.
pub fn update(app: &mut App, message: Message) -> iced::Task<Message> {
    let frame = matches!(message, Message::Frame(_));
    with(|probe| probe.see(&message));
    let task = crate::run::task(app.update(message));
    let next = with(|probe| {
        if !probe.library_in && !app.library.is_empty() {
            probe.library_in = true;
            probe.write("library");
        }
        probe.next(app, frame)
    })
    .unwrap_or_else(iced::Task::none);
    iced::Task::batch([task, next])
}

/// The view, timed, inside a widget that tells the probe each frame drawn.
pub fn view(app: &App) -> Element<'_, Message> {
    if with(|_| ()).is_none() {
        return app.view();
    }
    let began = Instant::now();
    let view = app.view();
    let took = began.elapsed().as_nanos();
    with(|probe| probe.write(&format!("view {took}")));
    Element::new(Drawn { content: view })
}

/// The frames, from the library shown to the step's end, in a run with a step: they bring the
/// messages the probe acts on, which nothing else may send once every cover shows. A run with a
/// step measures no start, so frames drawn before `covers` change no figure.
pub fn subscription() -> Subscription<Message> {
    let wanted = |probe: &mut Probe| {
        probe.step.is_some()
            && matches!(
                probe.phase,
                Phase::Shown | Phase::Settling(_) | Phase::Running(..)
            )
    };
    match with(wanted) {
        Some(true) => iced::window::frames().map(Message::Frame),
        _ => Subscription::none(),
    }
}

impl Probe {
    fn write(&mut self, event: &str) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let line = format!("{now} {event}\n");
        if matches!(self.phase, Phase::Running(..)) {
            self.held.push(line);
        } else {
            self.flush();
            let _ = self.log.write_all(line.as_bytes());
        }
    }

    fn flush(&mut self) {
        if !self.held.is_empty() {
            let _ = self.log.write_all(self.held.concat().as_bytes());
            self.held.clear();
        }
    }

    /// Notes what `message` tells of the window, the covers, the grid and key art.
    fn see(&mut self, message: &Message) {
        match message {
            Message::AccountResult(_, inner) => self.see(inner),
            Message::CachedCovers(found) => self.covers_found = Some(found.len()),
            Message::CoversScaled(width, made) => {
                if self.covers_made.0 != *width {
                    self.covers_made = (*width, 0);
                }
                self.covers_made.1 += made.len();
            }
            Message::WindowResized(size) => {
                self.window.0 = size.width;
                self.window.1 = size.height;
                self.write_window();
            }
            Message::Rescaled(scale) => {
                self.window.2 = *scale;
                self.write_window();
            }
            Message::GridScrolled(scroll) => {
                self.grid = Some((scroll.offset.y, scroll.shown.height, scroll.content.height));
            }
            Message::ArtScaled(from, ..) => {
                if let Phase::Running(
                    _,
                    Running::Pages {
                        open: Some(page), ..
                    },
                ) = &mut self.phase
                    && from
                        .file_stem()
                        .is_some_and(|stem| stem == page.art.as_str())
                {
                    page.art_at.get_or_insert(self.frames);
                }
            }
            _ => {}
        }
    }

    fn write_window(&mut self) {
        let (width, height, scale) = self.window;
        self.write(&format!("window {width} {height} {scale}"));
    }

    /// A frame drawn: the marks of the start, as they come.
    fn drawn(&mut self) {
        self.frames += 1;
        self.write("frame");
        match self.phase {
            Phase::Starting if self.library_in => {
                self.write("shown");
                self.phase = Phase::Shown;
                self.check_covers();
            }
            Phase::Shown => self.check_covers(),
            _ => {}
        }
    }

    fn check_covers(&mut self) {
        if self
            .covers_found
            .is_some_and(|found| found > 0 && self.covers_made.1 >= found)
        {
            self.write("covers");
            self.phase = match self.step {
                Some(_) => Phase::Settling(Instant::now() + SETTLE),
                None => Phase::Done,
            };
        }
    }

    /// What the probe does after a message: on each frame of a step, its next action.
    fn next(&mut self, app: &App, frame: bool) -> iced::Task<Message> {
        if !frame {
            return iced::Task::none();
        }
        match &self.phase {
            Phase::Settling(until) if Instant::now() >= *until => {
                let step = self.step.expect("settling for a step");
                let running = match step {
                    Step::Scroll => Running::Scroll { from: self.frames },
                    Step::Grid => Running::Grid,
                    Step::Pages => Running::Pages {
                        left: app
                            .library
                            .iter()
                            .filter_map(|g| {
                                let url = g.background.as_ref()?;
                                let art = slatty_core::fsutil::sha256_hex(url.as_bytes());
                                Some((g.id.clone(), art))
                            })
                            .take(PAGES)
                            .collect(),
                        open: None,
                    },
                };
                self.write(&format!("begin {}", step.name()));
                self.phase = Phase::Running(self.frames, running);
                iced::Task::none()
            }
            // One action a frame drawn: two frame messages may come for one frame.
            Phase::Running(acted_at, _) if *acted_at != self.frames => self.act(),
            _ => iced::Task::none(),
        }
    }

    /// The step's action on a new frame, and its end once it is over.
    fn act(&mut self) -> iced::Task<Message> {
        let (frames, grid) = (self.frames, self.grid);
        let Phase::Running(acted_at, running) = &mut self.phase else {
            return iced::Task::none();
        };
        *acted_at = frames;
        let mut missing_art = None;
        let (task, over) = match running {
            Running::Scroll { from } => (scroll(SCROLL_STEP), frames - *from >= SCROLLED_FRAMES),
            Running::Grid => match grid {
                Some((offset, shown, content)) if offset + shown >= content - 1.0 => {
                    (iced::Task::none(), true)
                }
                Some((_, shown, _)) => (scroll(shown), false),
                None => (scroll(1.0), false),
            },
            Running::Pages { left, open } => match open {
                // Its key art drawn twice: the next page.
                Some(page)
                    if page.art_at.is_some_and(|at| frames >= at + 2)
                        || page.since.elapsed() >= ART_WAIT =>
                {
                    missing_art = page.art_at.is_none().then(|| page.id.clone());
                    *open = None;
                    (iced::Task::done(Message::CloseDetail), false)
                }
                Some(_) => (iced::Task::none(), false),
                None => match left.pop_front() {
                    Some((id, art)) => {
                        *open = Some(Page {
                            id: id.clone(),
                            art,
                            since: Instant::now(),
                            art_at: None,
                        });
                        (iced::Task::done(Message::Select(id)), false)
                    }
                    None => (iced::Task::none(), true),
                },
            },
        };
        if let Some(id) = missing_art {
            self.write(&format!("no-art {id}"));
        }
        if over {
            let step = self.step.expect("running a step");
            self.phase = Phase::Done;
            self.write(&format!("end {}", step.name()));
        }
        task
    }
}

/// Scrolls the library's grid down by `by`, in the window's units.
fn scroll(by: f32) -> iced::Task<Message> {
    operation::scroll_by(crate::library::GRID, AbsoluteOffset { x: 0.0, y: by })
}

/// The whole window, which tells the probe each frame it draws.
struct Drawn<'a> {
    content: Element<'a, Message>,
}

impl Widget<Message, iced::Theme, iced::Renderer> for Drawn<'_> {
    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn Operation,
    ) {
        self.content
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &iced::Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
        with(Probe::drawn);
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, iced::Theme, iced::Renderer>> {
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}
