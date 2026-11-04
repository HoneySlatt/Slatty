//! What Iced runs for the state: its tasks (`task`) and what it listens to (`subscription`), and the
//! messages read from Iced's events.

use futures::channel::oneshot;
use iced::Subscription;
use iced::keyboard;
use iced::keyboard::key::Named;

use crate::listen::Listen;
use crate::pad::{Key, Nav};
use crate::task::{Action, Effect, Task};
use crate::{Message, WindowKey, couch, notify, pad, sidebar};

/// The task as Iced runs it.
pub fn task<T: Send + 'static>(task: Task<T>) -> iced::Task<T> {
    match task.into_stream() {
        None => iced::Task::none(),
        Some(stream) => iced::Task::run(stream, |action| action).then(|action| match action {
            Action::Output(output) => iced::Task::done(output),
            Action::Effect(effect) => effect_task(effect),
        }),
    }
}

/// What Iced does for `effect`; an answer goes back on its channel.
fn effect_task<T: Send + 'static>(effect: Effect) -> iced::Task<T> {
    use iced::widget::operation;
    use iced::widget::selector::{Target, find, id};
    // Iced's `then` takes a closure it may call more than once: each answer is sent once.
    fn answer<A: Send + 'static, T: Send + 'static>(
        sender: oneshot::Sender<A>,
    ) -> impl FnMut(A) -> iced::Task<T> + Send + 'static {
        let mut sender = Some(sender);
        move |a| {
            if let Some(sender) = sender.take() {
                let _ = sender.send(a);
            }
            iced::Task::none()
        }
    }
    match effect {
        Effect::ScrollTo { id, x, y } => {
            operation::scroll_to(id, operation::AbsoluteOffset { x, y })
        }
        Effect::SnapToEnd(id) => operation::snap_to_end(id),
        Effect::FocusNext => operation::focus_next(),
        Effect::FocusPrevious => operation::focus_previous(),
        Effect::Fullscreen(on) => {
            let mode = if on {
                iced::window::Mode::Fullscreen
            } else {
                iced::window::Mode::Windowed
            };
            iced::window::oldest().and_then(move |window| iced::window::set_mode(window, mode))
        }
        Effect::Exit => iced::exit(),
        Effect::ReadClipboard(sender) => iced::clipboard::read().then(answer(sender)),
        Effect::ScaleFactor(sender) => iced::window::latest()
            .and_then(iced::window::scale_factor)
            .then(answer(sender)),
        Effect::Tops(ids, sender) => {
            let mut found = iced::Task::done(Vec::new());
            for one in ids {
                found = found.then(move |tops: Vec<Option<f32>>| {
                    find(id(one)).map(move |target: Option<Target>| {
                        let mut tops = tops.clone();
                        tops.push(target.map(|t| t.bounds().y));
                        tops
                    })
                });
            }
            found.then(answer(sender))
        }
    }
}

/// What Iced listens to for `listens`: it keeps what is asked again running, and stops the rest.
pub fn subscription(listens: Vec<Listen>) -> Subscription<Message> {
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
