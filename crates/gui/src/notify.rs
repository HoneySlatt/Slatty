//! Notifications, as Steam shows them: a card in the corner of the window while it has the focus,
//! the desktop's own notification otherwise (a game in front, the window hidden or on another
//! workspace). Each kind can be turned off in Settings → Notifications.

use std::time::{Duration, Instant};

use iced::Task;

use crate::{App, Message};

/// How long a card stays in the window.
pub const SHOWN_FOR: Duration = Duration::from_secs(6);
/// Cards shown at once; an older one gives way to a new one.
const AT_ONCE: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Downloads,
    Updates,
    Achievements,
    Cloud,
}

impl Kind {
    pub const ALL: [Kind; 4] = [
        Kind::Downloads,
        Kind::Updates,
        Kind::Achievements,
        Kind::Cloud,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Kind::Downloads => "downloads",
            Kind::Updates => "updates",
            Kind::Achievements => "achievements",
            Kind::Cloud => "cloud",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Kind::Downloads => "Downloads",
            Kind::Updates => "Game and Proton updates",
            Kind::Achievements => "Achievements",
            Kind::Cloud => "Cloud saves",
        }
    }
}

/// Which notifications are shown: all of them, then each kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Switches {
    pub all: bool,
    pub kinds: [bool; Kind::ALL.len()],
}

impl Default for Switches {
    fn default() -> Self {
        Switches {
            all: true,
            kinds: [true; Kind::ALL.len()],
        }
    }
}

impl Switches {
    pub fn read(db: &slatty_core::db::Db) -> Switches {
        let on = |key| slatty_core::settings::notifications(db, key).unwrap_or(true);
        Switches {
            all: on("all"),
            kinds: Kind::ALL.map(|k| on(k.key())),
        }
    }

    pub fn allows(&self, kind: Kind) -> bool {
        self.all && self.kinds[kind as usize]
    }
}

/// A card in the window's corner.
#[derive(Debug, Clone)]
pub struct Toast {
    pub id: u64,
    pub title: String,
    pub body: String,
    /// The game it is about: a click opens its page, its cover shows beside.
    pub game: Option<String>,
    pub until: Instant,
}

#[derive(Debug, Clone)]
pub enum NotifyMsg {
    /// The window gained or lost the focus.
    Focus(bool),
    /// Cards past their time go.
    Expire(Instant),
    Dismiss(u64),
    Open(u64),
    /// The switch for every kind (`None`) or for one.
    Switch(Option<Kind>, bool),
}

impl App {
    /// Tells the user: a card in the window while it has the focus, the desktop's notification
    /// otherwise. Nothing when that kind is turned off.
    pub fn notify(
        &mut self,
        kind: Kind,
        game: Option<&str>,
        title: impl Into<String>,
        body: impl Into<String>,
    ) -> Task<Message> {
        if !self.notifications.allows(kind) {
            return Task::none();
        }
        let (title, body) = (title.into(), body.into());
        if !self.unfocused {
            self.next_toast += 1;
            self.toasts.push(Toast {
                id: self.next_toast,
                title,
                body,
                game: game.map(String::from),
                until: Instant::now() + SHOWN_FOR,
            });
            if self.toasts.len() > AT_ONCE {
                self.toasts.remove(0);
            }
            return Task::none();
        }
        let image = game.and_then(|id| self.cover_file(id));
        Task::future(async move {
            let _ =
                tokio::task::spawn_blocking(move || desktop(&title, &body, image.as_deref())).await;
        })
        .discard()
    }

    pub fn update_notify(&mut self, msg: NotifyMsg) -> Task<Message> {
        match msg {
            NotifyMsg::Focus(focused) => self.unfocused = !focused,
            NotifyMsg::Expire(now) => self.toasts.retain(|t| t.until > now),
            NotifyMsg::Dismiss(id) => self.toasts.retain(|t| t.id != id),
            NotifyMsg::Open(id) => {
                let game = self
                    .toasts
                    .iter()
                    .find(|t| t.id == id)
                    .and_then(|t| t.game.clone());
                self.toasts.retain(|t| t.id != id);
                if let Some(game) = game {
                    return self.update(Message::Select(game));
                }
            }
            NotifyMsg::Switch(kind, on) => {
                match kind {
                    None => self.notifications.all = on,
                    Some(k) => self.notifications.kinds[k as usize] = on,
                }
                if let Some(core) = &self.core {
                    let key = kind.map_or("all", Kind::key);
                    if let Err(e) = slatty_core::settings::set_notifications(&core.db, key, on) {
                        self.notify_error(e.to_string());
                    }
                }
            }
        }
        Task::none()
    }

    /// The cover of a game as a file, for the desktop's notification.
    fn cover_file(&self, game_id: &str) -> Option<std::path::PathBuf> {
        match self.cover(game_id)? {
            iced::widget::image::Handle::Path(_, path) => Some(path),
            _ => None,
        }
    }
}

/// The desktop's notification, through its notification service (freedesktop.org). Nothing when
/// there is none.
fn desktop(title: &str, body: &str, image: Option<&std::path::Path>) {
    use std::collections::HashMap;
    use zbus::zvariant::Value;
    let Ok(connection) = zbus::blocking::Connection::session() else {
        return;
    };
    let mut hints: HashMap<&str, Value<'_>> = HashMap::new();
    let image = image.map(|p| p.display().to_string());
    if let Some(path) = &image {
        hints.insert("image-path", Value::from(path.as_str()));
    }
    let _ = connection.call_method(
        Some("org.freedesktop.Notifications"),
        "/org/freedesktop/Notifications",
        Some("org.freedesktop.Notifications"),
        "Notify",
        &(
            "SlattyLauncher",
            0u32,
            "",
            title,
            plain(body),
            Vec::<&str>::new(),
            hints,
            -1i32,
        ),
    );
}

/// A notification's body as text: services that read markup there (links, images) would otherwise
/// take it from a title, an achievement name or an error, which games and GOG write.
fn plain(body: &str) -> String {
    body.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_body_is_never_read_as_markup() {
        assert_eq!(
            super::plain("[FAKE] <a href=\"https://x.example\">Q&A</a> <img src=\"/tmp/i.png\"/>"),
            "[FAKE] &lt;a href=\"https://x.example\"&gt;Q&amp;A&lt;/a&gt; &lt;img src=\"/tmp/i.png\"/&gt;"
        );
    }
}
