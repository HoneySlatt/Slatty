//! What the interface listens to, as Slatty's own list: frames, timers, keys, the window, gamepads,
//! the pointer. `App::subscription` says what it wants after each message, and `into_iced` hands it
//! to Iced, which keeps what is asked again running and stops what is not.

use std::time::Duration;

use iced::Subscription;

use crate::{Message, couch, pad, sidebar};

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
            Listen::Keys { couch: false } => {
                iced::keyboard::listen().filter_map(crate::key_message)
            }
            Listen::Keys { couch: true } => iced::keyboard::listen().filter_map(crate::couch_key),
            Listen::Window => iced::window::events().filter_map(crate::window_message),
            Listen::CloseRequests => {
                iced::window::close_requests().map(|_| Message::CloseRequested)
            }
            Listen::Resizes => {
                iced::window::resize_events().map(|(_, size)| Message::WindowResized(size))
            }
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
