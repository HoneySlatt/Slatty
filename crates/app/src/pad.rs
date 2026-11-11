//! Gamepads and the keyboard, as the moves of the couch mode. Gamepads are read through gilrs on a
//! thread of their own, made at the first entry into the couch mode, waiting while it is off; a
//! direction held repeats, from the d-pad as from the left stick.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime};

use futures::{SinkExt, Stream};

/// A move of the couch mode. Buttons are named after the Xbox layout, which the hints show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Nav {
    Up,
    Down,
    Left,
    Right,
    /// A, Enter.
    Confirm,
    /// B, Escape.
    Back,
    /// Y, Space: the game's other action (favorite).
    Action,
    /// X, `/`: search.
    Search,
    /// LB and RB, Page Up and Page Down: the tab before or after.
    PrevTab,
    NextTab,
    /// Start (☰), F1.
    Menu,
}

impl Nav {
    fn is_direction(self) -> bool {
        matches!(self, Nav::Up | Nav::Down | Nav::Left | Nav::Right)
    }
}

/// What a gamepad did, before repeats: gilrs's own events cannot be made outside it, in tests.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PadInput {
    Pressed(Nav),
    Released(Nav),
    /// The left stick, from -1 to 1, right and up positive.
    StickX(f32),
    StickY(f32),
    /// A gamepad gone (unplugged, asleep, its battery flat): nothing it held is released.
    Lost,
}

/// A held direction repeats after this, then this often.
const FIRST_REPEAT: Duration = Duration::from_millis(400);
const REPEAT: Duration = Duration::from_millis(110);
/// How far the stick leans to move, and how far back it lets go.
const TILT: f32 = 0.6;
const RELEASE: f32 = 0.35;

/// Turns presses and the stick into moves, a held direction repeating.
#[derive(Debug, Default)]
pub struct Repeater {
    /// The direction held, and when it repeats next.
    held: Option<(Nav, Instant)>,
    stick: (f32, f32),
    /// The direction the stick leans to.
    leaning: Option<Nav>,
}

impl Repeater {
    pub fn input(&mut self, input: PadInput, now: Instant) -> Option<Nav> {
        match input {
            PadInput::Pressed(nav) if nav.is_direction() => {
                self.held = Some((nav, now + FIRST_REPEAT));
                Some(nav)
            }
            PadInput::Pressed(nav) => Some(nav),
            PadInput::Released(nav) => {
                self.held.take_if(|(held, _)| *held == nav);
                None
            }
            PadInput::StickX(x) => {
                self.stick.0 = x;
                self.lean(now)
            }
            PadInput::StickY(y) => {
                self.stick.1 = y;
                self.lean(now)
            }
            PadInput::Lost => {
                *self = Repeater::default();
                None
            }
        }
    }

    /// A repeat of the direction held, once due.
    pub fn tick(&mut self, now: Instant) -> Option<Nav> {
        let (nav, at) = self.held?;
        if now < at {
            return None;
        }
        self.held = Some((nav, now + REPEAT));
        Some(nav)
    }

    /// When the next repeat is due, if a direction is held.
    pub fn next_repeat(&self) -> Option<Instant> {
        self.held.map(|(_, at)| at)
    }

    /// A move when the stick starts leaning another way; a lean held is let go once the stick
    /// comes back near the middle.
    fn lean(&mut self, now: Instant) -> Option<Nav> {
        let (x, y) = self.stick;
        let along = |nav: Nav| match nav {
            Nav::Right => x,
            Nav::Left => -x,
            Nav::Up => y,
            Nav::Down => -y,
            _ => 0.0,
        };
        if let Some(nav) = self.leaning
            && along(nav) >= RELEASE
        {
            return None;
        }
        let toward = if x.abs() >= y.abs() {
            if x > 0.0 { Nav::Right } else { Nav::Left }
        } else if y > 0.0 {
            Nav::Up
        } else {
            Nav::Down
        };
        if along(toward) < TILT {
            if let Some(nav) = self.leaning.take() {
                self.held.take_if(|(held, _)| *held == nav);
            }
            return None;
        }
        self.leaning = Some(toward);
        self.held = Some((toward, now + FIRST_REPEAT));
        Some(toward)
    }
}

/// The move a gamepad button stands for.
pub fn button_nav(button: gilrs::Button) -> Option<Nav> {
    use gilrs::Button;
    Some(match button {
        Button::DPadUp => Nav::Up,
        Button::DPadDown => Nav::Down,
        Button::DPadLeft => Nav::Left,
        Button::DPadRight => Nav::Right,
        Button::South => Nav::Confirm,
        Button::East => Nav::Back,
        Button::North => Nav::Action,
        Button::West => Nav::Search,
        // gilrs's triggers are the shoulder buttons; LT and RT are its second ones.
        Button::LeftTrigger => Nav::PrevTab,
        Button::RightTrigger => Nav::NextTab,
        Button::Start => Nav::Menu,
        _ => return None,
    })
}

fn pad_input(event: gilrs::EventType) -> Option<PadInput> {
    use gilrs::{Axis, EventType};
    match event {
        EventType::ButtonPressed(b, _) => button_nav(b).map(PadInput::Pressed),
        EventType::ButtonReleased(b, _) => button_nav(b).map(PadInput::Released),
        EventType::AxisChanged(Axis::LeftStickX, v, _) => Some(PadInput::StickX(v)),
        EventType::AxisChanged(Axis::LeftStickY, v, _) => Some(PadInput::StickY(v)),
        EventType::Disconnected => Some(PadInput::Lost),
        _ => None,
    }
}

/// How long the reading thread waits for an event when nothing is held: it notices this long, at
/// most, after the couch mode is left.
const IDLE: Duration = Duration::from_secs(1);

/// An input, and when the gamepad made it.
type Timed = (PadInput, SystemTime);

/// Where the reading thread takes the gamepads' input from: gilrs, or a fake in tests.
trait Source {
    /// The next input, waiting at most `wait` for it; `None` once `wait` is over, or as soon as
    /// an event comes that is no input of the couch mode.
    fn next(&mut self, wait: Duration) -> Option<Timed>;

    /// The time now, for the repeats.
    fn now(&self) -> Instant;
}

impl Source for gilrs::Gilrs {
    fn next(&mut self, wait: Duration) -> Option<Timed> {
        let event = self.next_event_blocking(Some(wait))?;
        Some((pad_input(event.event)?, event.time))
    }

    fn now(&self) -> Instant {
        Instant::now()
    }
}

/// The couch mode, open since `since`, which receives the moves.
struct Listener {
    tx: tokio::sync::mpsc::UnboundedSender<Nav>,
    since: SystemTime,
}

/// The thread reading the gamepads for the whole process, started at the first entry into the
/// couch mode: where each entry gives it the listener to serve.
type Reader = std::sync::OnceLock<std::sync::mpsc::Sender<Listener>>;

/// The moves of every gamepad connected, now or later. One gilrs, made once per process, reads
/// them: on Linux each one leaves a thread behind (gilrs-core 0.6.8's hotplug thread, blocked on
/// udev until a device next comes or goes). While the couch mode is closed, the reader waits for
/// the next entry without waking.
pub fn moves() -> impl Stream<Item = Nav> {
    static READER: Reader = std::sync::OnceLock::new();
    listen(&READER, || gilrs::Gilrs::new().ok())
}

/// The moves `reader` reads from the source `open` gives, starting it at the first call.
fn listen<S: Source + 'static>(
    reader: &'static Reader,
    open: fn() -> Option<S>,
) -> impl Stream<Item = Nav> {
    crate::task::channel(32, async move |mut output| {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let listener = Listener {
            tx,
            since: SystemTime::now(),
        };
        if start(reader, open).send(listener).is_err() {
            return;
        }
        while let Some(nav) = rx.recv().await {
            if output.send(nav).await.is_err() {
                break;
            }
        }
    })
}

/// Where to give `reader` a listener, its thread started the first time. A thread that cannot
/// start, or finds no gamepad support (no udev), ends: what it is given is refused, and the
/// keyboard still works.
fn start<S: Source + 'static>(
    reader: &'static Reader,
    open: fn() -> Option<S>,
) -> &'static std::sync::mpsc::Sender<Listener> {
    reader.get_or_init(|| {
        let (tx, listeners) = std::sync::mpsc::channel();
        let _ = std::thread::Builder::new()
            .name("slatty-gamepads".into())
            .spawn(move || {
                if let Some(mut source) = open() {
                    read(&mut source, &listeners);
                }
            });
        tx
    })
}

/// Serves each listener in turn, the latest given when several wait; between them, blocks on
/// `listeners`.
fn read(source: &mut impl Source, listeners: &std::sync::mpsc::Receiver<Listener>) {
    let mut pending = None;
    while let Ok(mut listener) = listeners.recv() {
        while let Ok(later) = listeners.try_recv() {
            listener = later;
        }
        pending = serve(source, &listener, pending);
    }
}

/// Sends `listener` the moves `source` makes, starting with `pending`, until it stops listening;
/// what a gamepad did before it listened is not its. An input read as it stopped is given back,
/// for whoever listens next.
fn serve(
    source: &mut impl Source,
    listener: &Listener,
    mut pending: Option<Timed>,
) -> Option<Timed> {
    let mut repeater = Repeater::default();
    loop {
        if listener.tx.is_closed() {
            return pending;
        }
        let read = pending.take().or_else(|| {
            let wait = repeater
                .next_repeat()
                .map_or(IDLE, |at| at.saturating_duration_since(source.now()));
            source.next(wait)
        });
        if listener.tx.is_closed() {
            return read;
        }
        let input = read
            .filter(|(_, at)| *at >= listener.since)
            .map(|(input, _)| input);
        let away = AWAY.load(Ordering::Relaxed);
        for nav in step(&mut repeater, input, source.now(), away) {
            if listener.tx.send(nav).is_err() {
                return None;
            }
        }
    }
}

/// Whether the player is elsewhere (`App::away`): the window behind another, a game running.
static AWAY: AtomicBool = AtomicBool::new(false);

/// Told by the interface after each message.
pub fn set_away(away: bool) {
    AWAY.store(away, Ordering::Relaxed);
}

/// The moves `input` makes, and the repeat due; none while the player is away: they are the
/// game's, and each would wake the launcher for nothing.
fn step(repeater: &mut Repeater, input: Option<PadInput>, now: Instant, away: bool) -> Vec<Nav> {
    if away {
        *repeater = Repeater::default();
        return Vec::new();
    }
    let moved = input.and_then(|input| repeater.input(input, now));
    moved.into_iter().chain(repeater.tick(now)).collect()
}

/// A key of the couch mode: a move, a character typed into the search, or one taken away.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Nav(Nav),
    Char(char),
    Erase,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    /// What the fake gamepad does at a time.
    #[derive(Clone, Copy)]
    enum Step {
        Input(PadInput),
        /// The couch mode left.
        Leave,
    }

    /// A gamepad that does `steps`, each at its time from the start, on a clock of its own; the
    /// couch mode listening on `listening` is left once they run out.
    struct Fake {
        start: Instant,
        clock: Duration,
        steps: VecDeque<(u64, Step)>,
        listening: tokio::sync::mpsc::UnboundedReceiver<Nav>,
    }

    impl Source for Fake {
        fn next(&mut self, wait: Duration) -> Option<Timed> {
            let until = self.clock + wait;
            while let Some(&(at, step)) = self.steps.front() {
                let at = Duration::from_millis(at);
                if at > until {
                    break;
                }
                self.steps.pop_front();
                self.clock = self.clock.max(at);
                match step {
                    Step::Input(input) => return Some((input, wall(at))),
                    Step::Leave => self.listening.close(),
                }
            }
            if self.steps.is_empty() {
                self.listening.close();
            }
            self.clock = until;
            None
        }

        fn now(&self) -> Instant {
            self.start + self.clock
        }
    }

    /// The wall clock `at` after the fake's start.
    fn wall(at: Duration) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000) + at
    }

    /// A listener that entered `since` ms after the fake's start, and what it receives.
    fn listener(since: u64) -> (Listener, tokio::sync::mpsc::UnboundedReceiver<Nav>) {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let since = wall(Duration::from_millis(since));
        (Listener { tx, since }, rx)
    }

    fn fake(steps: &[(u64, Step)], listening: tokio::sync::mpsc::UnboundedReceiver<Nav>) -> Fake {
        Fake {
            start: Instant::now(),
            clock: Duration::ZERO,
            steps: steps.iter().copied().collect(),
            listening,
        }
    }

    fn received(rx: &mut tokio::sync::mpsc::UnboundedReceiver<Nav>) -> Vec<Nav> {
        std::iter::from_fn(|| rx.try_recv().ok()).collect()
    }

    /// What a gamepad did while the couch mode was closed, waiting in gilrs, is not replayed when
    /// it opens: only what comes after.
    #[test]
    fn moves_made_before_entering_are_not_the_couch_modes() {
        let (couch, rx) = listener(1000);
        let mut pad = fake(
            &[
                (500, Step::Input(PadInput::Pressed(Nav::Down))),
                (1200, Step::Input(PadInput::Pressed(Nav::Confirm))),
            ],
            rx,
        );
        assert_eq!(serve(&mut pad, &couch, None), None);
        assert_eq!(received(&mut pad.listening), [Nav::Confirm]);
    }

    /// The couch mode left and entered again while the reader waits: a move made after the new
    /// entry, read for the old one, goes to the new one.
    #[test]
    fn a_move_made_as_the_couch_mode_is_entered_again_is_not_lost() {
        let (first, rx) = listener(0);
        let mut pad = fake(
            &[
                (300, Step::Leave),
                (800, Step::Input(PadInput::Pressed(Nav::Confirm))),
            ],
            rx,
        );
        let pending = serve(&mut pad, &first, None);
        assert!(received(&mut pad.listening).is_empty());
        let (again, rx) = listener(500);
        pad.listening = rx;
        assert_eq!(serve(&mut pad, &again, pending), None);
        assert_eq!(received(&mut pad.listening), [Nav::Confirm]);
    }

    /// A gamepad fed by the test, in real time.
    struct Pad(std::sync::mpsc::Receiver<Timed>);

    impl Source for Pad {
        fn next(&mut self, wait: Duration) -> Option<Timed> {
            self.0.recv_timeout(wait).ok()
        }

        fn now(&self) -> Instant {
            Instant::now()
        }
    }

    static READER: Reader = std::sync::OnceLock::new();
    static PAD: std::sync::Mutex<Option<std::sync::mpsc::Receiver<Timed>>> =
        std::sync::Mutex::new(None);
    static OPENED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

    fn open_pad() -> Option<Pad> {
        OPENED.fetch_add(1, Ordering::Relaxed);
        PAD.lock().unwrap().take().map(Pad)
    }

    /// Each entry into the couch mode listens to the one reader of the process: gilrs, made
    /// once, leaves no thread of its own behind at each entry.
    #[tokio::test]
    async fn entering_the_couch_mode_again_reuses_its_reader() {
        use futures::{FutureExt, StreamExt};
        let (pad, inputs) = std::sync::mpsc::channel();
        *PAD.lock().unwrap() = Some(inputs);
        let mut first = Box::pin(listen(&READER, open_pad));
        assert_eq!(first.next().now_or_never(), None);
        // Left, then entered again.
        drop(first);
        let mut again = Box::pin(listen(&READER, open_pad));
        assert_eq!(again.next().now_or_never(), None);
        pad.send((PadInput::Pressed(Nav::Confirm), SystemTime::now()))
            .unwrap();
        let next = tokio::time::timeout(Duration::from_secs(5), again.next()).await;
        assert_eq!(next.ok().flatten(), Some(Nav::Confirm));
        assert_eq!(OPENED.load(Ordering::Relaxed), 1);
    }

    fn at(ms: u64, start: Instant) -> Instant {
        start + Duration::from_millis(ms)
    }

    #[test]
    fn a_held_direction_repeats_and_stops_once_released() {
        let start = Instant::now();
        let mut r = Repeater::default();
        assert_eq!(
            r.input(PadInput::Pressed(Nav::Down), start),
            Some(Nav::Down)
        );
        assert_eq!(r.tick(at(300, start)), None, "not before the first delay");
        assert_eq!(r.tick(at(400, start)), Some(Nav::Down));
        assert_eq!(r.tick(at(450, start)), None);
        assert_eq!(r.tick(at(510, start)), Some(Nav::Down));
        assert_eq!(r.input(PadInput::Released(Nav::Down), at(520, start)), None);
        assert_eq!(r.tick(at(2000, start)), None);
        assert_eq!(r.next_repeat(), None);
    }

    /// A gamepad gone while a direction is held sends no release: the repeat stops all the same.
    #[test]
    fn a_direction_held_stops_with_its_gamepad() {
        let start = Instant::now();
        let mut r = Repeater::default();
        r.input(PadInput::Pressed(Nav::Down), start);
        assert_eq!(r.input(PadInput::Lost, at(100, start)), None);
        assert_eq!(r.next_repeat(), None);
        assert_eq!(r.tick(at(2000, start)), None);
        // The stick too, which leans again once back.
        r.input(PadInput::StickX(0.9), at(2100, start));
        r.input(PadInput::Lost, at(2200, start));
        assert_eq!(r.tick(at(4000, start)), None);
        assert_eq!(
            r.input(PadInput::StickX(0.9), at(4100, start)),
            Some(Nav::Right)
        );
        assert_eq!(
            pad_input(gilrs::EventType::Disconnected),
            Some(PadInput::Lost)
        );
    }

    /// While the player is away (the window behind a game), the gamepad's moves are not sent:
    /// they are the game's, and each would wake the launcher. Back, the gamepad moves it at once.
    #[test]
    fn moves_are_not_sent_while_the_player_is_away() {
        let start = Instant::now();
        let mut r = Repeater::default();
        assert!(step(&mut r, Some(PadInput::StickY(0.9)), start, true).is_empty());
        assert_eq!(r.next_repeat(), None);
        assert!(step(&mut r, None, at(2000, start), true).is_empty());
        assert_eq!(
            step(&mut r, Some(PadInput::StickY(0.95)), at(2100, start), false),
            [Nav::Up]
        );
    }

    #[test]
    fn buttons_do_not_repeat() {
        let start = Instant::now();
        let mut r = Repeater::default();
        assert_eq!(
            r.input(PadInput::Pressed(Nav::Confirm), start),
            Some(Nav::Confirm)
        );
        assert_eq!(r.tick(at(2000, start)), None);
    }

    #[test]
    fn the_stick_moves_once_per_lean_and_repeats_while_held() {
        let start = Instant::now();
        let mut r = Repeater::default();
        assert_eq!(r.input(PadInput::StickX(0.3), start), None, "a slight lean");
        assert_eq!(r.input(PadInput::StickX(0.9), start), Some(Nav::Right));
        // Shaking near the end does not move again.
        assert_eq!(r.input(PadInput::StickX(0.5), at(10, start)), None);
        assert_eq!(r.input(PadInput::StickX(0.95), at(20, start)), None);
        assert_eq!(r.tick(at(400, start)), Some(Nav::Right));
        // Back to the middle: let go.
        assert_eq!(r.input(PadInput::StickX(0.1), at(450, start)), None);
        assert_eq!(r.tick(at(2000, start)), None);
        // Up is positive.
        assert_eq!(
            r.input(PadInput::StickY(-0.8), at(2100, start)),
            Some(Nav::Down)
        );
        assert_eq!(r.input(PadInput::StickY(0.0), at(2200, start)), None);
        assert_eq!(
            r.input(PadInput::StickY(0.8), at(2300, start)),
            Some(Nav::Up)
        );
    }

    #[test]
    fn the_stick_leaning_another_way_moves_that_way() {
        let start = Instant::now();
        let mut r = Repeater::default();
        assert_eq!(r.input(PadInput::StickX(0.9), start), Some(Nav::Right));
        assert_eq!(
            r.input(PadInput::StickX(-0.9), at(50, start)),
            Some(Nav::Left)
        );
    }

    #[test]
    fn buttons_stand_for_their_place_on_an_xbox_pad() {
        use gilrs::Button;
        assert_eq!(button_nav(Button::South), Some(Nav::Confirm));
        assert_eq!(button_nav(Button::East), Some(Nav::Back));
        assert_eq!(button_nav(Button::North), Some(Nav::Action));
        assert_eq!(button_nav(Button::West), Some(Nav::Search));
        assert_eq!(button_nav(Button::LeftTrigger), Some(Nav::PrevTab));
        assert_eq!(button_nav(Button::RightTrigger), Some(Nav::NextTab));
        assert_eq!(button_nav(Button::Start), Some(Nav::Menu));
        assert_eq!(button_nav(Button::LeftTrigger2), None);
    }
}
