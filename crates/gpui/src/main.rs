//! Slatty's interface drawn with GPUI, over the same state as the Iced one (`slatty-app`). Being
//! built page by page beside `slatty-gui`, which stays the interface until this one matches it.

use gpui::{
    App, AppContext, Bounds, Context, IntoElement, Render, Window, WindowBounds, WindowOptions,
    div, prelude::*, px, rgba, size,
};
use gpui_platform::application;
use slatty_app::look;

/// The window's content.
struct Root;

impl Render for Root {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().bg(color(look::tokens().background))
    }
}

/// A colour of the look, as GPUI draws it.
fn color(c: look::Color) -> gpui::Rgba {
    let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u32;
    rgba(byte(c.r) << 24 | byte(c.g) << 16 | byte(c.b) << 8 | byte(c.a))
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
    application().run(|cx: &mut App| {
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
        if let Err(e) = cx.open_window(options, |_, cx| cx.new(|_| Root)) {
            // GPUI draws with the graphics card alone, through Vulkan or OpenGL: without either
            // driver there is no window, where Iced drew with the processor.
            const WHY: &str = "Slatty needs a Vulkan or OpenGL driver to draw its window (Mesa \
                               provides both).";
            eprintln!("Slatty could not open its window: {e:#}. {WHY}");
            slatty_app::notify::desktop("Slatty could not open its window", WHY, None);
            // From a task, which runs once the event loop does: the loop forgets a stop asked
            // before it starts.
            cx.spawn(async |cx| cx.update(|cx| cx.quit())).detach();
        }
    });
}
