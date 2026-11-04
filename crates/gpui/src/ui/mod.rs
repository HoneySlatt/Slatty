//! The views: what the window shows of the state, page by page, as `slatty-gui`'s `ui` does.

mod widgets;

use gpui::{AnyElement, App, ClickEvent, Context, Div, IntoElement, Window, div, prelude::*, px};
use slatty_app::icons::Icon;
use slatty_app::{Message, Page, couch};

use crate::Root;
use crate::theme::{self, tokens};
use widgets::{avatar, divider, icon, icon_tab, nav_tab, vertical_rule};

impl Root {
    /// What the window shows.
    pub fn view(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let app = &self.app;
        if let Some(e) = &app.fatal {
            return div()
                .p(px(40.))
                .text_size(px(18.))
                .child(format!("Cannot start: {e}"))
                .into_any_element();
        }
        if app.core.is_none() {
            return div()
                .p(px(40.))
                .text_color(tokens().muted)
                .child("Loading…")
                .into_any_element();
        }
        // Steam's games and those added by hand show without GOG; GOG's sign-in stays one click
        // away in Settings.
        let added = app
            .installs
            .keys()
            .any(|id| slatty_core::store::from_no_store(id));
        let username = match (&app.account, &app.steam.found) {
            (Some(account), _) if !app.login_open => Some(account.username.clone()),
            (None, Some(name)) if app.steam.on && !app.login_open => Some(name.clone()),
            (None, None) if app.steam.on && !app.login_open => Some("Steam".into()),
            (None, _) if app.epic.account.is_some() && !app.login_open => {
                Some(app.epic.name.clone().unwrap_or_else(|| "Epic Games".into()))
            }
            (None, _) if added && !app.login_open => Some(String::new()),
            _ => None,
        };
        let Some(username) = username else {
            // The sign-in page is not drawn yet.
            return div().into_any_element();
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(self.top_bar(&username, cx))
            .child(
                // Until the pages are drawn: the state started and read the library.
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(tokens().muted)
                    .child(format!("{} games", self.app.library.len())),
            )
            .into_any_element()
    }

    /// Sends `message` when clicked.
    fn press(
        &self,
        message: Message,
        cx: &mut Context<Self>,
    ) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
        cx.listener(move |root, _: &ClickEvent, window, cx| {
            root.message(message.clone(), window, cx)
        })
    }

    /// Logo, page tabs, search and account, above every page but the game page.
    fn top_bar(&self, username: &str, cx: &mut Context<Self>) -> Div {
        let t = tokens();
        let page = self.app.page;
        let mut tab = |id, which, label, to: Page| {
            nav_tab(id, which, label, page == to).on_click(self.press(Message::ShowPage(to), cx))
        };
        let tabs = div()
            .flex()
            .items_center()
            .gap(px(8.))
            .child(tab(
                "library-tab",
                Icon::LayoutGrid,
                "Library",
                Page::Library,
            ))
            .child(tab(
                "achievements-tab",
                Icon::Trophy,
                "Achievements",
                Page::Achievements,
            ))
            .child(vertical_rule(24.))
            .child(
                icon_tab("downloads-tab", Icon::Download, page == Page::Downloads)
                    .on_click(self.press(Message::ShowPage(Page::Downloads), cx)),
            )
            .child(
                icon_tab("settings-tab", Icon::Settings, page == Page::Settings)
                    .on_click(self.press(Message::ShowPage(Page::Settings), cx)),
            )
            .child(
                icon_tab("couch-tab", Icon::Gamepad, false)
                    .on_click(self.press(Message::Couch(couch::CouchMsg::Enter), cx)),
            );
        // Not a field yet: typing comes with the text field.
        let search = div()
            .flex()
            .items_center()
            .gap(px(10.))
            .px(px(14.))
            .py(px(9.))
            // Narrower in a small window, so the account button stays in view.
            .w(px((self.app.window.width - 580.0).clamp(180.0, 360.0)))
            .flex_none()
            .bg(t.surface)
            .border_1()
            .border_color(t.outline)
            .rounded(t.radius)
            .child(icon(Icon::Search, 18., t.muted))
            .child(
                div()
                    .text_size(px(15.))
                    .text_color(t.muted)
                    .child("Search a game"),
            );
        let initial = username
            .chars()
            .next()
            .map(|c| c.to_uppercase().to_string())
            .unwrap_or_default();
        let picture = self
            .app
            .avatar
            .as_ref()
            .and_then(|u| self.app.images.get(u));
        let account = widgets::button("account", theme::plain())
            .child(avatar(picture, initial, 34., self.app.account.is_some()))
            .on_click(self.press(Message::ShowPage(Page::Settings), cx));
        let bar = div()
            .flex()
            .items_center()
            .gap(px(16.))
            .px(px(20.))
            .py(px(7.))
            .child(widgets::logo(44.))
            .child(tabs)
            .child(div().flex_1())
            .child(search)
            .child(account);
        div().flex().flex_col().child(bar).child(divider())
    }
}
