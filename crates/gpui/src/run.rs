//! What GPUI runs for the state: its tasks, on a tokio runtime, what they ask of the window, and
//! what it listens to (`Listen`), started and stopped after each message as the state asks.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use futures::StreamExt;
use futures::channel::mpsc;
use gpui::{Context, Keystroke, Window};
use slatty_app::listen::Listen;
use slatty_app::pad::{Key, Nav};
use slatty_app::task::{Action, Effect, Task};
use slatty_app::{Message, WindowKey, couch, pad};

use crate::Root;

/// The state's tasks, run on tokio, and the window's side of them.
pub struct Runner {
    tokio: tokio::runtime::Handle,
    /// Where the tasks send their outputs and requests, read on the window's thread (`Root::new`).
    actions: mpsc::UnboundedSender<Action<Message>>,
    pub listening: Listening,
}

/// What the window listens to after the last message.
#[derive(Default)]
pub struct Listening {
    pub frames: bool,
    /// The keys the window answers, or the couch mode's (`Some(true)`).
    pub keys: Option<bool>,
    pub window: bool,
    pub close_requests: bool,
    pub resizes: bool,
    pub pointer: bool,
    /// One per timer, by its period and the function making its message; dropping one stops it.
    timers: HashMap<(Duration, usize), gpui::Task<()>>,
    gamepads: Option<tokio::task::JoinHandle<()>>,
}

impl Runner {
    pub fn new(
        tokio: tokio::runtime::Handle,
        actions: mpsc::UnboundedSender<Action<Message>>,
    ) -> Self {
        Runner {
            tokio,
            actions,
            listening: Listening::default(),
        }
    }

    /// Runs `task` on tokio; its outputs and requests come back to the window's thread.
    pub fn run(&self, task: Task<Message>) {
        if let Some(stream) = task.into_stream() {
            let actions = self.actions.clone();
            self.tokio.spawn(stream.for_each(move |action| {
                let _ = actions.unbounded_send(action);
                std::future::ready(())
            }));
        }
    }

    /// Starts what `listens` asks for that does not run yet, and stops the rest.
    pub fn listen(&mut self, listens: Vec<Listen>, window: &Window, cx: &mut Context<Root>) {
        let had = std::mem::take(&mut self.listening);
        let mut timers = had.timers;
        let mut gamepads = had.gamepads;
        let mut kept = HashSet::new();
        let mut wants_gamepads = false;
        for listen in listens {
            match listen {
                Listen::Frames => self.listening.frames = true,
                Listen::Every(every, message) => {
                    let key = (every, message as usize);
                    kept.insert(key);
                    timers
                        .entry(key)
                        .or_insert_with(|| every_period(every, message, window, cx));
                }
                Listen::Keys { couch } => self.listening.keys = Some(couch),
                Listen::Window => self.listening.window = true,
                Listen::CloseRequests => self.listening.close_requests = true,
                Listen::Resizes => self.listening.resizes = true,
                Listen::Gamepads => wants_gamepads = true,
                Listen::Pointer => self.listening.pointer = true,
            }
        }
        timers.retain(|key, _| kept.contains(key));
        self.listening.timers = timers;
        if !wants_gamepads && let Some(reading) = gamepads.take() {
            // The reading thread sees its channel closed and ends.
            reading.abort();
        }
        if wants_gamepads && gamepads.is_none() {
            let actions = self.actions.clone();
            gamepads = Some(self.tokio.spawn(pad::moves().for_each(move |nav| {
                let message = Message::Couch(couch::CouchMsg::Nav(nav));
                let _ = actions.unbounded_send(Action::Output(message));
                std::future::ready(())
            })));
        }
        self.listening.gamepads = gamepads;
    }
}

/// The message `message` makes, every `every`, while the task lives.
fn every_period(
    every: Duration,
    message: fn() -> Message,
    window: &Window,
    cx: &mut Context<Root>,
) -> gpui::Task<()> {
    cx.spawn_in(window, async move |root, cx| {
        loop {
            cx.background_executor().timer(every).await;
            if root
                .update_in(cx, |root, window, cx| root.message(message(), window, cx))
                .is_err()
            {
                break;
            }
        }
    })
}

/// Does what `effect` asks of the window; an answer goes back on its channel.
pub fn effect(effect: Effect, window: &mut Window, cx: &mut Context<Root>) {
    match effect {
        // No part of the page scrolls yet.
        Effect::ScrollTo { .. } | Effect::SnapToEnd(_) => {}
        Effect::FocusNext => window.focus_next(cx),
        Effect::FocusPrevious => window.focus_prev(cx),
        Effect::Fullscreen(on) => {
            if window.is_fullscreen() != on {
                window.toggle_fullscreen();
            }
        }
        Effect::Exit => cx.quit(),
        Effect::ReadClipboard(answer) => {
            let _ = answer.send(cx.read_from_clipboard().and_then(|item| item.text()));
        }
        Effect::ScaleFactor(answer) => {
            let _ = answer.send(window.scale_factor());
        }
        // No part of the page with an id is drawn yet.
        Effect::Tops(ids, answer) => {
            let _ = answer.send(vec![None; ids.len()]);
        }
    }
}

/// The message for a frame drawn, while something moves.
pub fn frame() -> Message {
    Message::Frame(Instant::now())
}

/// The keys the window answers itself (Tab, Shift+Tab, Escape); `None` for the others, so that
/// they send no message.
pub fn window_key(keystroke: &Keystroke) -> Option<WindowKey> {
    match keystroke.key.as_str() {
        "tab" if keystroke.modifiers.shift => Some(WindowKey::ShiftTab),
        "tab" => Some(WindowKey::Tab),
        "escape" => Some(WindowKey::Escape),
        _ => None,
    }
}

/// The key pressed, for the couch mode; `None` for the others, so that they send no message.
pub fn pad_key(keystroke: &Keystroke) -> Option<Key> {
    let modifiers = &keystroke.modifiers;
    Some(match keystroke.key.as_str() {
        "up" => Key::Nav(Nav::Up),
        "down" => Key::Nav(Nav::Down),
        "left" => Key::Nav(Nav::Left),
        "right" => Key::Nav(Nav::Right),
        "enter" => Key::Nav(Nav::Confirm),
        "escape" => Key::Nav(Nav::Back),
        "backspace" => Key::Erase,
        "pageup" => Key::Nav(Nav::PrevTab),
        "pagedown" => Key::Nav(Nav::NextTab),
        "tab" if modifiers.shift => Key::Nav(Nav::PrevTab),
        "tab" => Key::Nav(Nav::NextTab),
        "f1" => Key::Nav(Nav::Menu),
        "space" => Key::Char(' '),
        _ if modifiers.control || modifiers.alt || modifiers.platform => return None,
        _ => match keystroke
            .key_char
            .as_deref()
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
    use gpui::Modifiers;

    use super::*;

    fn press(key: &str, key_char: Option<&str>, modifiers: Modifiers) -> Keystroke {
        Keystroke {
            modifiers,
            key: key.into(),
            key_char: key_char.map(Into::into),
        }
    }

    fn plain(key: &str) -> Keystroke {
        press(key, None, Modifiers::default())
    }

    fn shift() -> Modifiers {
        Modifiers {
            shift: true,
            ..Default::default()
        }
    }

    #[test]
    fn keys_move_the_couch_modes_focus_type_and_erase() {
        assert_eq!(pad_key(&plain("down")), Some(Key::Nav(Nav::Down)));
        assert_eq!(pad_key(&plain("enter")), Some(Key::Nav(Nav::Confirm)));
        assert_eq!(pad_key(&plain("escape")), Some(Key::Nav(Nav::Back)));
        assert_eq!(pad_key(&plain("backspace")), Some(Key::Erase));
        assert_eq!(pad_key(&plain("space")), Some(Key::Char(' ')));
        assert_eq!(
            pad_key(&press("tab", None, shift())),
            Some(Key::Nav(Nav::PrevTab))
        );
        assert_eq!(
            pad_key(&press("e", Some("é"), Modifiers::default())),
            Some(Key::Char('é'))
        );
        assert_eq!(
            pad_key(&press(";", Some("/"), shift())),
            Some(Key::Nav(Nav::Search)),
            "the character typed, whatever the key"
        );
        let control = Modifiers {
            control: true,
            ..Default::default()
        };
        assert_eq!(
            pad_key(&press("c", Some("c"), control)),
            None,
            "a shortcut types nothing"
        );
        assert_eq!(pad_key(&plain("shift")), None);
    }

    /// Only the keys the window answers send a message: Tab and Shift+Tab move the focus, Escape
    /// goes back; nothing for a letter or Enter.
    #[test]
    fn the_window_answers_tab_and_escape_alone() {
        assert_eq!(window_key(&plain("tab")), Some(WindowKey::Tab));
        assert_eq!(
            window_key(&press("tab", None, shift())),
            Some(WindowKey::ShiftTab)
        );
        assert_eq!(window_key(&plain("escape")), Some(WindowKey::Escape));
        assert_eq!(window_key(&plain("enter")), None);
        assert_eq!(
            window_key(&press("a", Some("a"), Modifiers::default())),
            None
        );
    }
}
