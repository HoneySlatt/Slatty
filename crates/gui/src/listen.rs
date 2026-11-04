//! What the interface listens to, as Slatty's own list: frames, timers, keys, the window, gamepads,
//! the pointer. `App::subscription` says what it wants after each message, and `into_iced` hands it
//! to Iced, which keeps what is asked again running and stops what is not.

use std::time::Duration;

use iced::Subscription;
use iced::keyboard;
use iced::keyboard::key::Named;

use crate::pad::{Key, Nav};
use crate::{Message, WindowKey, couch, notify, pad, sidebar};

#[derive(Debug, Clone, Copy)]
pub enum Listen {
    /// A message at each frame drawn, while something moves.
    Frames,
    /// The message made by `message`, every `Duration`.
    Every(Duration, fn() -> Message),
    /// The keys the desktop window answers itself, or those the couch mode answers.
    Keys {
        couch: bool,
    },
    /// The window gaining or losing the focus, or changing its scale.
    Window,
    CloseRequests,
    Resizes,
    /// The moves of every gamepad.
    Gamepads,
    /// The pointer moving and let go, while the list beside the library is widened by hand.
    Pointer,
}

/// What Iced runs for `listens`.
pub fn into_iced(listens: Vec<Listen>) -> Subscription<Message> {
    Subscription::batch(listens.into_iter().map(|listen| {
        match listen {
            Listen::Frames => iced::window::frames().map(Message::Frame),
            Listen::Every(every, message) => iced::time::every(every)
                .with(message)
                .map(|(message, _)| message()),
            Listen::Keys { couch: false } => iced::keyboard::listen().filter_map(key_message),
            Listen::Keys { couch: true } => iced::keyboard::listen().filter_map(couch_key),
            Listen::Window => iced::window::events().filter_map(window_message),
            Listen::CloseRequests => {
                iced::window::close_requests().map(|_| Message::CloseRequested)
            }
            Listen::Resizes => iced::window::resize_events()
                .map(|(_, size)| Message::WindowResized(crate::ui::size(size))),
            Listen::Gamepads => {
                Subscription::run(pad::moves).map(|nav| Message::Couch(couch::CouchMsg::Nav(nav)))
            }
            Listen::Pointer => iced::event::listen_with(pointer_message),
        }
    }))
}

/// The pointer moving, or let go, while the list's edge is dragged: anywhere in the window, beyond
/// the edge itself, and out of it, where its release is not told.
fn pointer_message(
    event: iced::Event,
    _: iced::event::Status,
    _: iced::window::Id,
) -> Option<Message> {
    use iced::{Event, mouse};
    match event {
        Event::Mouse(mouse::Event::CursorMoved { position }) => {
            Some(Message::Sidebar(sidebar::SidebarMsg::Drag(position.x)))
        }
        Event::Mouse(
            mouse::Event::ButtonReleased(mouse::Button::Left) | mouse::Event::CursorLeft,
        ) => Some(Message::Sidebar(sidebar::SidebarMsg::Drop)),
        _ => None,
    }
}

/// The window gaining or losing the focus, or changing its scale, as a message; its other events
/// send none.
fn window_message((_, event): (iced::window::Id, iced::window::Event)) -> Option<Message> {
    use iced::window::Event;
    match event {
        Event::Focused => Some(Message::Notify(notify::NotifyMsg::Focus(true))),
        Event::Unfocused => Some(Message::Notify(notify::NotifyMsg::Focus(false))),
        Event::Rescaled(scale) => Some(Message::Rescaled(scale)),
        _ => None,
    }
}

/// The keys the window answers itself (Tab, Shift+Tab, Escape), as a message. The others send
/// none: every message rebuilds the page, and each key a field does not take (its release, a
/// modifier) would.
pub fn key_message(event: keyboard::Event) -> Option<Message> {
    let keyboard::Event::KeyPressed {
        key: keyboard::Key::Named(named),
        modifiers,
        ..
    } = event
    else {
        return None;
    };
    match named {
        Named::Tab if modifiers.shift() => Some(Message::Key(WindowKey::ShiftTab)),
        Named::Tab => Some(Message::Key(WindowKey::Tab)),
        Named::Escape => Some(Message::Key(WindowKey::Escape)),
        _ => None,
    }
}

/// The keys the couch mode answers, as a message.
fn couch_key(event: keyboard::Event) -> Option<Message> {
    pad_key(event).map(|k| Message::Couch(couch::CouchMsg::Key(k)))
}

/// The key pressed, for the couch mode; `None` for the others, so that they send no message.
pub fn pad_key(event: keyboard::Event) -> Option<Key> {
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
    fn keys_move_the_couch_modes_focus_type_and_erase() {
        let named = |n| {
            press(
                keyboard::Key::Named(n),
                None,
                keyboard::Modifiers::default(),
            )
        };
        assert_eq!(pad_key(named(Named::ArrowDown)), Some(Key::Nav(Nav::Down)));
        assert_eq!(pad_key(named(Named::Enter)), Some(Key::Nav(Nav::Confirm)));
        assert_eq!(pad_key(named(Named::Escape)), Some(Key::Nav(Nav::Back)));
        assert_eq!(pad_key(named(Named::Backspace)), Some(Key::Erase));
        assert_eq!(pad_key(named(Named::Space)), Some(Key::Char(' ')));
        let shift_tab = press(
            keyboard::Key::Named(Named::Tab),
            None,
            keyboard::Modifiers::SHIFT,
        );
        assert_eq!(pad_key(shift_tab), Some(Key::Nav(Nav::PrevTab)));
        let typed = |c: &str, m| press(keyboard::Key::Character(c.into()), Some(c), m);
        assert_eq!(
            pad_key(typed("é", keyboard::Modifiers::default())),
            Some(Key::Char('é'))
        );
        assert_eq!(
            pad_key(typed("/", keyboard::Modifiers::default())),
            Some(Key::Nav(Nav::Search))
        );
        assert_eq!(
            pad_key(typed("c", keyboard::Modifiers::CTRL)),
            None,
            "a shortcut types nothing"
        );
        assert_eq!(
            pad_key(keyboard::Event::ModifiersChanged(
                keyboard::Modifiers::SHIFT
            )),
            None
        );
    }

    /// Only the keys the window answers send a message: Tab and Shift+Tab move the focus, Escape
    /// goes back; nothing for a letter, a release or a modifier.
    #[test]
    fn the_window_answers_tab_and_escape_alone() {
        let named = |n, m| press(keyboard::Key::Named(n), None, m);
        let key_of = |event| match key_message(event) {
            Some(Message::Key(k)) => Some(k),
            _ => None,
        };
        let plain = keyboard::Modifiers::empty();
        assert_eq!(key_of(named(Named::Tab, plain)), Some(WindowKey::Tab));
        assert_eq!(
            key_of(named(Named::Tab, keyboard::Modifiers::SHIFT)),
            Some(WindowKey::ShiftTab)
        );
        assert_eq!(key_of(named(Named::Escape, plain)), Some(WindowKey::Escape));
        assert_eq!(key_of(named(Named::Enter, plain)), None);
        assert!(
            key_message(press(
                keyboard::Key::Character("a".into()),
                Some("a"),
                plain
            ))
            .is_none()
        );
    }
}
