//! Gamepads and the keyboard, as the moves of the couch mode. Gamepads are read through gilrs on a
//! thread of their own, while the couch mode is on; a direction held repeats, from the d-pad as
//! from the left stick.

use std::time::{Duration, Instant};

use iced::futures::{SinkExt, Stream};
use iced::keyboard;

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
        _ => None,
    }
}

/// How long the reading thread waits for an event when nothing is held: it ends this long, at
/// most, after the couch mode is left.
const IDLE: Duration = Duration::from_secs(1);

/// The moves of every gamepad connected, now or later.
pub fn moves() -> impl Stream<Item = Nav> {
    iced::stream::channel(32, async |mut output| {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let started = std::thread::Builder::new()
            .name("slatty-gamepads".into())
            .spawn(move || read(&tx));
        if started.is_err() {
            return;
        }
        while let Some(nav) = rx.recv().await {
            if output.send(nav).await.is_err() {
                break;
            }
        }
    })
}

fn read(tx: &tokio::sync::mpsc::UnboundedSender<Nav>) {
    // Without udev, no gamepad: the keyboard still works.
    let Ok(mut gilrs) = gilrs::Gilrs::new() else {
        return;
    };
    let mut repeater = Repeater::default();
    while !tx.is_closed() {
        let wait = repeater
            .next_repeat()
            .map_or(IDLE, |at| at.saturating_duration_since(Instant::now()));
        let event = gilrs.next_event_blocking(Some(wait));
        let now = Instant::now();
        let moved = event
            .and_then(|e| pad_input(e.event))
            .and_then(|input| repeater.input(input, now));
        for nav in moved.into_iter().chain(repeater.tick(now)) {
            if tx.send(nav).is_err() {
                return;
            }
        }
    }
}

/// A key of the couch mode: a move, a character typed into the search, or one taken away.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Nav(Nav),
    Char(char),
    Erase,
}

/// The key pressed, for the couch mode; `None` for the others, so that they send no message.
pub fn key(event: keyboard::Event) -> Option<Key> {
    use keyboard::key::Named;
    let keyboard::Event::KeyPressed {
        key,
        modifiers,
        text,
        ..
    } = event
    else {
        return None;
    };
    Some(match key {
        keyboard::Key::Named(named) => match named {
            Named::ArrowUp => Key::Nav(Nav::Up),
            Named::ArrowDown => Key::Nav(Nav::Down),
            Named::ArrowLeft => Key::Nav(Nav::Left),
            Named::ArrowRight => Key::Nav(Nav::Right),
            Named::Enter => Key::Nav(Nav::Confirm),
            Named::Escape => Key::Nav(Nav::Back),
            Named::Backspace => Key::Erase,
            Named::PageUp => Key::Nav(Nav::PrevTab),
            Named::PageDown => Key::Nav(Nav::NextTab),
            Named::Tab if modifiers.shift() => Key::Nav(Nav::PrevTab),
            Named::Tab => Key::Nav(Nav::NextTab),
            Named::F1 => Key::Nav(Nav::Menu),
            Named::Space => Key::Char(' '),
            _ => return None,
        },
        _ if modifiers.control() || modifiers.alt() || modifiers.logo() => return None,
        _ => match text
            .and_then(|t| t.chars().next())
            .filter(|c| !c.is_control())
        {
            Some('/') => Key::Nav(Nav::Search),
            Some(c) => Key::Char(c),
            None => return None,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn press(
        k: keyboard::Key,
        text: Option<&str>,
        modifiers: keyboard::Modifiers,
    ) -> keyboard::Event {
        keyboard::Event::KeyPressed {
            key: k.clone(),
            modified_key: k,
            physical_key: keyboard::key::Physical::Unidentified(
                keyboard::key::NativeCode::Unidentified,
            ),
            location: keyboard::Location::Standard,
            modifiers,
            text: text.map(Into::into),
            repeat: false,
        }
    }

    #[test]
    fn keys_move_the_focus_type_and_erase() {
        use keyboard::key::Named;
        let named = |n| {
            press(
                keyboard::Key::Named(n),
                None,
                keyboard::Modifiers::default(),
            )
        };
        assert_eq!(key(named(Named::ArrowDown)), Some(Key::Nav(Nav::Down)));
        assert_eq!(key(named(Named::Enter)), Some(Key::Nav(Nav::Confirm)));
        assert_eq!(key(named(Named::Escape)), Some(Key::Nav(Nav::Back)));
        assert_eq!(key(named(Named::Backspace)), Some(Key::Erase));
        assert_eq!(key(named(Named::Space)), Some(Key::Char(' ')));
        let shift_tab = press(
            keyboard::Key::Named(Named::Tab),
            None,
            keyboard::Modifiers::SHIFT,
        );
        assert_eq!(key(shift_tab), Some(Key::Nav(Nav::PrevTab)));
        let typed = |c: &str, m| press(keyboard::Key::Character(c.into()), Some(c), m);
        assert_eq!(
            key(typed("é", keyboard::Modifiers::default())),
            Some(Key::Char('é'))
        );
        assert_eq!(
            key(typed("/", keyboard::Modifiers::default())),
            Some(Key::Nav(Nav::Search))
        );
        assert_eq!(
            key(typed("c", keyboard::Modifiers::CTRL)),
            None,
            "a shortcut types nothing"
        );
        assert_eq!(
            key(keyboard::Event::ModifiersChanged(
                keyboard::Modifiers::SHIFT
            )),
            None
        );
    }
}
