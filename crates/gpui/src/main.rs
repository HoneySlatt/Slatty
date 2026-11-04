//! Slatty's interface drawn with GPUI, over the same state as the Iced one (`slatty-app`). Being
//! built page by page beside `slatty-gui`, which stays the interface until this one matches it.

mod run;
mod theme;

use futures::StreamExt;
use futures::channel::mpsc;
use gpui::{
    App, AppContext, Bounds, Context, FocusHandle, IntoElement, KeyDownEvent, MouseButton,
    MouseMoveEvent, MouseUpEvent, Render, Window, WindowBounds, WindowOptions, div, prelude::*, px,
    size,
};
use gpui_platform::application;
use slatty_app::geometry::Size;
use slatty_app::{Message, couch, look, notify, sidebar};
use slatty_core::db::Db;
use slatty_core::paths::Dirs;

/// The size of a rem at the interface's own scale of 1, as GPUI has it.
const REM: f32 = 16.0;

/// The state, and the window drawing it.
struct Root {
    app: slatty_app::App,
    runner: run::Runner,
    focus: FocusHandle,
    /// The window's scale when last told to the state.
    scale: f32,
}

impl Root {
    fn new(tokio: tokio::runtime::Handle, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (actions, mut received) = mpsc::unbounded();
        cx.spawn_in(window, async move |root, cx| {
            while let Some(action) = received.next().await {
                if root
                    .update_in(cx, |root, window, cx| root.act(action, window, cx))
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        cx.observe_window_bounds(window, |root, window, cx| root.window_changed(window, cx))
            .detach();
        cx.observe_window_activation(window, |root, window, cx| {
            if root.runner.listening.window {
                let focused = window.is_window_active();
                root.message(
                    Message::Notify(notify::NotifyMsg::Focus(focused)),
                    window,
                    cx,
                );
            }
        })
        .detach();
        let this = cx.entity().downgrade();
        window.on_window_should_close(cx, move |window, cx| {
            this.update(cx, |root, cx| root.close_requested(window, cx))
                .unwrap_or(true)
        });
        // The size the first page is laid out for, as Iced tells it once the window opens.
        cx.defer_in(window, |root, window, cx| root.window_changed(window, cx));
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        let (app, task) = slatty_app::App::boot();
        let mut root = Root {
            app,
            runner: run::Runner::new(tokio, actions),
            focus,
            scale: window.scale_factor(),
        };
        root.runner.run(task);
        root.runner.listen(root.app.subscription(), window, cx);
        root
    }

    /// Hands `message` to the state, runs what it returns and listens to what it asks for next.
    fn message(&mut self, message: Message, window: &mut Window, cx: &mut Context<Self>) {
        let task = self.app.update(message);
        self.runner.run(task);
        self.runner.listen(self.app.subscription(), window, cx);
        cx.notify();
    }

    fn act(
        &mut self,
        action: slatty_app::task::Action<Message>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action {
            slatty_app::task::Action::Output(message) => self.message(message, window, cx),
            slatty_app::task::Action::Effect(effect) => run::effect(effect, window, cx),
        }
    }

    /// The window resized, or its scale changed.
    fn window_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let scale = window.scale_factor();
        if scale != self.scale && self.runner.listening.window {
            self.scale = scale;
            self.message(Message::Rescaled(scale), window, cx);
        }
        if self.runner.listening.resizes {
            // In the interface's own units, as Iced gives them.
            let size = window.viewport_size();
            let applied = self.app.applied_scale.get();
            let size = Size::new(
                f32::from(size.width) / applied,
                f32::from(size.height) / applied,
            );
            self.message(Message::WindowResized(size), window, cx);
        }
    }

    /// Whether the window may close: not while the state listens for the request, which it then
    /// answers (asking first when work runs).
    fn close_requested(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if !self.runner.listening.close_requests {
            return true;
        }
        self.message(Message::CloseRequested, window, cx);
        false
    }

    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let message = match self.runner.listening.keys {
            Some(false) => run::window_key(&event.keystroke).map(Message::Key),
            Some(true) => {
                run::pad_key(&event.keystroke).map(|key| Message::Couch(couch::CouchMsg::Key(key)))
            }
            None => None,
        };
        if let Some(message) = message {
            self.message(message, window, cx);
        }
    }

    /// The pointer's place across the window, in the interface's own units.
    fn pointer_x(&self, x: gpui::Pixels) -> f32 {
        f32::from(x) / self.app.applied_scale.get()
    }
}

impl Render for Root {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        window.set_rem_size(px(REM * self.app.scale_factor()));
        if self.runner.listening.frames {
            let this = cx.entity().downgrade();
            window.on_next_frame(move |window, cx| {
                let _ = this.update(cx, |root, cx| root.message(run::frame(), window, cx));
            });
        }
        let tokens = look::tokens();
        let mut page = div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(theme::color(tokens.background))
            .text_color(theme::color(tokens.muted))
            .font_family(theme::font_family())
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|root, event, window, cx| root.key_down(event, window, cx)))
            // Until the pages are drawn: the state started and read the library.
            .child(format!("{} games", self.app.library.len()));
        if self.runner.listening.pointer {
            page = page
                .on_mouse_move(cx.listener(|root, event: &MouseMoveEvent, window, cx| {
                    let x = root.pointer_x(event.position.x);
                    root.message(Message::Sidebar(sidebar::SidebarMsg::Drag(x)), window, cx);
                }))
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|root, _: &MouseUpEvent, window, cx| {
                        root.message(Message::Sidebar(sidebar::SidebarMsg::Drop), window, cx);
                    }),
                );
        }
        page
    }
}

fn main() {
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
    // The state's tasks run here, as under Iced: its file, network and process work is tokio's.
    let tokio = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(tokio) => tokio,
        Err(e) => {
            eprintln!("Slatty could not start its background work: {e}");
            std::process::exit(1);
        }
    };
    let handle = tokio.handle().clone();
    // Before the first frame, so the window never shows another look first.
    if let Ok(dirs) = Dirs::from_system() {
        look::apply_saved(&dirs, Db::open(&dirs.db_file()).ok().as_ref());
    }
    slatty_app::boot::start(|| {});
    application().run(move |cx: &mut App| {
        theme::load_fonts(cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(1440.), px(900.)),
                cx,
            ))),
            window_min_size: Some(size(px(800.), px(560.))),
            app_id: Some(slatty_app::APP_ID.into()),
            ..Default::default()
        };
        if let Err(e) = cx.open_window(options, |window, cx| {
            cx.new(|cx| Root::new(handle, window, cx))
        }) {
            // GPUI draws with the graphics card alone, through Vulkan or OpenGL: without either
            // driver there is no window, where Iced drew with the processor.
            const WHY: &str = "Slatty needs a Vulkan or OpenGL driver to draw its window (Mesa \
                               provides both).";
            eprintln!("Slatty could not open its window: {e:#}. {WHY}");
            notify::desktop("Slatty could not open its window", WHY, None);
            // From a task, which runs once the event loop does: the loop forgets a stop asked
            // before it starts.
            cx.spawn(async |cx| cx.update(|cx| cx.quit())).detach();
        }
    });
    drop(tokio);
}
