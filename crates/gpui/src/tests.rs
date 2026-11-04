//! Interface tests: the real `update` and views in GPUI's test window, over a state with fake
//! data (`slatty_app::testing`). As slatty-gui's, they never run the state's tasks.

use std::path::PathBuf;

use gpui::{Entity, Modifiers, TestAppContext, VisualTestContext};
use slatty_app::task::Task;
use slatty_app::{Page, testing};

use crate::Root;

/// The throwaway folder the tests work in, its own for each run.
fn folder() -> PathBuf {
    std::env::temp_dir().join(format!("slatty-gpui-test-{}", std::process::id()))
}

/// The window over `app`, as `main` opens it, its tasks dropped.
fn open(cx: &mut TestAppContext, app: slatty_app::App) -> (Entity<Root>, &mut VisualTestContext) {
    cx.update(|cx| {
        crate::theme::load_fonts(cx);
        crate::ui::bind_keys(cx);
    });
    let (root, cx) =
        cx.add_window_view(|window, cx| Root::new(None, app, Task::none(), window, cx));
    cx.run_until_parked();
    (root, cx)
}

/// The window over a library of fourteen `[FAKE]` games, signed in.
fn library(cx: &mut TestAppContext) -> (Entity<Root>, &mut VisualTestContext) {
    open(cx, testing::library(testing::core(&folder())))
}

/// Clicks the middle of what the debug selector `selector` names.
fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} is not drawn"));
    cx.simulate_click(bounds.center(), Modifiers::default());
}

#[gpui::test]
fn a_tab_of_the_top_bar_opens_its_page(cx: &mut TestAppContext) {
    let (root, cx) = library(cx);
    click(cx, "achievements-tab");
    root.read_with(cx, |root, _| assert_eq!(root.app.page, Page::Achievements));
    click(cx, "settings-tab");
    root.read_with(cx, |root, _| assert_eq!(root.app.page, Page::Settings));
    click(cx, "library-tab");
    root.read_with(cx, |root, _| assert_eq!(root.app.page, Page::Library));
}

/// What is typed in the search box is the state's search, and what the state holds is shown.
#[gpui::test]
fn the_search_box_types_into_the_states_search(cx: &mut TestAppContext) {
    let (root, cx) = library(cx);
    click(cx, "search");
    cx.simulate_input("Game 1");
    root.read_with(cx, |root, _| assert_eq!(root.app.search, "Game 1"));
    cx.simulate_keystrokes("backspace backspace");
    root.read_with(cx, |root, _| assert_eq!(root.app.search, "Game"));
    root.update(cx, |root, cx| {
        root.app.search = "Ω".into();
        cx.notify();
    });
    cx.run_until_parked();
    cx.simulate_input("mega");
    root.read_with(cx, |root, _| {
        assert_eq!(
            root.app.search, "Ωmega",
            "typed after what the state put there"
        )
    });
}

/// Escape goes back, as with Iced: here it closes the question asked before quitting.
#[gpui::test]
fn escape_goes_back(cx: &mut TestAppContext) {
    let (root, cx) = library(cx);
    root.update(cx, |root, _| {
        root.app.quit_confirm = Some(vec!["[FAKE] Game 3".into()])
    });
    cx.simulate_keystrokes("escape");
    root.read_with(cx, |root, _| assert!(root.app.quit_confirm.is_none()));
}
