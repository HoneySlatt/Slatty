mod graphics;
pub mod icons;
#[cfg(feature = "probe")]
mod probe;
mod run;
#[cfg(test)]
mod tests;
mod theme;
mod ui;

use std::ops::{Deref, DerefMut};

use slatty_core::db::Db;
use slatty_core::paths::Dirs;

// The state's modules and types, by the names the views and the tests use; `App` and `icons` are
// the interface's own.
pub use slatty_app::*;

/// The state (`slatty_app::App`), and the views drawn from it (`ui`): they read it as if it were
/// their own.
#[derive(Default)]
pub struct App(pub slatty_app::App);

impl Deref for App {
    type Target = slatty_app::App;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for App {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl App {
    /// The state's `update`. A method of the interface's own, so that the message given may read
    /// the state as it is made (`app.update(Message::Frame(app.now))`), as it could before.
    pub fn update(&mut self, message: Message) -> task::Task<Message> {
        self.0.update(message)
    }

    fn theme(&self) -> iced::Theme {
        theme::theme()
    }
}

fn main() -> iced::Result {
    // First, for the session supervisor too: a crash must not write the tokens to the disk.
    if let Err(e) = slatty_core::platform::no_core_dumps() {
        eprintln!("slatty: core dumps could not be turned off: {e}");
    }
    if std::env::args_os()
        .nth(1)
        .is_some_and(|a| a == slatty_core::session::SUPERVISE_ARG)
    {
        std::process::exit(slatty_core::session::supervise_main());
    }
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_env("SLATTY_LOG"))
        .with_writer(std::io::stderr)
        .init();
    let mut vulkan_only = false;
    // Before the first frame, so the window never shows another look first.
    if let Ok(dirs) = Dirs::from_system() {
        let db = Db::open(&dirs.db_file()).ok();
        vulkan_only = db
            .as_ref()
            .is_some_and(|db| slatty_core::settings::vulkan_only(db).unwrap_or(false));
        look::apply_saved(&dirs, db.as_ref());
    }
    look::list_fonts_with(theme::font_families);
    let backends = graphics::backends(std::env::var_os("WGPU_BACKEND").as_deref(), vulkan_only);
    if backends == Some(wgpu::Backends::VULKAN) {
        // SAFETY: no other thread runs yet; `boot::start` begins the first one.
        unsafe { std::env::set_var("WGPU_BACKEND", "vulkan") };
    }
    if let Some(scale) = couch::x11_scale(
        couch::under_gamescope(),
        std::env::var_os("WINIT_X11_SCALE_FACTOR").as_deref(),
    ) {
        // SAFETY: as above.
        unsafe { std::env::set_var("WINIT_X11_SCALE_FACTOR", scale) };
    }
    boot::start(move || {
        // Which backends the next start can do with, once this one is over.
        if let Some(backends) = backends {
            std::thread::sleep(graphics::CHECK_AFTER);
            if let Ok(dirs) = Dirs::from_system()
                && let Ok(db) = Db::open(&dirs.db_file())
            {
                futures::executor::block_on(graphics::check(&db, backends));
            }
        }
    });
    // Iced reads them once the graphics driver is set up: up to a tenth of a second more before
    // the first frame when they are not in the disk cache yet, as after the computer starts.
    std::thread::spawn(|| {
        iced::advanced::graphics::text::font_system();
    });
    #[cfg(feature = "probe")]
    probe::start();
    #[cfg(not(feature = "probe"))]
    let (update, view) = (
        |app: &mut App, message| run::task(app.update(message)),
        App::view,
    );
    #[cfg(feature = "probe")]
    let (update, view) = (probe::update, probe::view);
    let mut app = iced::application(
        || {
            let (app, task) = slatty_app::App::boot();
            (App(app), run::task(task))
        },
        update,
        view,
    );
    for font in theme::FONTS {
        app = app.font(font);
    }
    app.title("Slatty")
        .subscription(|app: &App| {
            let subscription = run::subscription(app.subscription());
            #[cfg(feature = "probe")]
            let subscription = iced::Subscription::batch([subscription, probe::subscription()]);
            subscription
        })
        .theme(App::theme)
        .scale_factor(|app: &App| app.scale_factor())
        .window(window_settings(couch::ask_at_start()))
        .default_font(theme::font())
        .exit_on_close_request(false)
        .run()
}

/// The window, named after the desktop entry so that the desktop shows the entry's name and icon
/// for it.
fn window_settings(fullscreen: bool) -> iced::window::Settings {
    iced::window::Settings {
        size: iced::Size::new(1440.0, 900.0),
        fullscreen,
        min_size: Some(iced::Size::new(800.0, 560.0)),
        #[cfg(unix)]
        platform_specific: iced::window::settings::PlatformSpecific {
            application_id: APP_ID.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}
