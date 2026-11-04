//! What the interface listens to, as Slatty's own list: frames, timers, keys, the window, gamepads,
//! the pointer. `App::subscription` says what it wants after each message, and the interface
//! listens to it (`run::subscription`).

use std::time::Duration;

use crate::Message;

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
