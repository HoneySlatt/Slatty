//! The couch mode's pages: large covers, one with the focus, and the gamepad buttons to press
//! shown below them.

use crate::theme::text;
use iced::widget::{
    Column, Row, Space, button, column, container, image, progress_bar, row, space, stack,
};
use iced::{Alignment, ContentFit, Element, Length, Padding};
use slatty_core::library::LibraryGame;

use super::format::{duration, fraction, relative_day};
use super::widgets::logo;
use crate::couch::{
    Couch, CouchMsg, GAP, HINTS, KEYS, KeyAction, MARGIN, MenuItem, SHELVES, TOP_BAR, Tab,
};
use crate::icons::{Icon, icon};
use crate::install::InstallView;
use crate::library::Shelf;
use crate::theme::{self, bold, semibold, tokens};
use crate::{App, Message};

/// The gamepad buttons the hints name.
#[derive(Clone, Copy)]
enum Glyph {
    A,
    B,
    X,
    Y,
    Lb,
    Rb,
    Menu,
}

/// Room the focus ring takes around a cover.
const RING: f32 = 4.0;

impl App {
    pub(super) fn couch_view(&self) -> Element<'_, Message> {
        let Some(couch) = &self.couch else {
            return Space::new().into();
        };
        let game = self.selected_game();
        let page = match game {
            Some(g) => self.couch_game(couch, g),
            None => match couch.tab {
                Tab::Home => self.couch_home(couch),
                Tab::Library => self.couch_library(couch),
                Tab::Downloads => self.couch_downloads(couch),
            },
        };
        let body = column![]
            .push(game.is_none().then(|| self.couch_top_bar(couch)))
            .push(container(page).height(Length::Fill))
            .push(self.couch_hints(couch));
        let keyboard = couch
            .keyboard
            .filter(|_| self.typing())
            .map(|focus| self.couch_keyboard(couch, focus));
        let achievements = game.and_then(|g| self.couch_achievements(couch, g));
        stack![body]
            .push(keyboard)
            .push(achievements)
            .push(couch.menu.map(couch_menu))
            .push(
                self.launch_prompt
                    .as_deref()
                    .map(|id| self.couch_launch(id, couch.launch)),
            )
            .push(
                self.quit_confirm
                    .as_ref()
                    .map(|work| couch_quit(work, couch.quit)),
            )
            .push((!self.toasts.is_empty()).then(|| self.toasts()))
            .into()
    }

    fn couch_top_bar(&self, couch: &Couch) -> Element<'_, Message> {
        let tabs = Tab::ALL.iter().map(|t| {
            button(text(t.label()).size(22).font(semibold()))
                .padding([10, 22])
                .on_press(Message::Couch(CouchMsg::Tab(*t)))
                .style(theme::couch_tab(couch.tab == *t))
                .into()
        });
        // What downloads, wherever the focus is.
        let status = self.download_rows().into_iter().find_map(|r| {
            r.progress
                .map(|p| format!("{} · {:.0}%", r.title, p * 100.0))
        });
        container(
            row![
                logo(40.0),
                glyph(Glyph::Lb),
                Row::with_children(tabs).spacing(8),
                glyph(Glyph::Rb),
                space().width(Length::Fill),
            ]
            .push(status.map(|s| {
                row![
                    icon(Icon::Download, 18.0, tokens().muted),
                    text(s).size(16).color(tokens().muted)
                ]
                .spacing(8)
                .align_y(Alignment::Center)
            }))
            .spacing(16)
            .align_y(Alignment::Center),
        )
        .padding([0.0, MARGIN])
        .width(Length::Fill)
        .height(TOP_BAR)
        .center_y(TOP_BAR)
        .style(theme::couch_bar)
        .into()
    }

    /// What the gamepad's buttons do where the focus is.
    fn couch_hints(&self, couch: &Couch) -> Element<'_, Message> {
        let game = self.selected.is_some();
        let hints: &[(Glyph, &str)] = if self.quit_confirm.is_some() {
            &[(Glyph::A, "Choose"), (Glyph::B, "Keep running")]
        } else if self.launch_prompt.is_some() {
            &[(Glyph::A, "Start"), (Glyph::B, "Cancel")]
        } else if couch.menu.is_some() {
            &[(Glyph::A, "Select"), (Glyph::B, "Close")]
        } else if self.typing() {
            &[
                (Glyph::A, "Type"),
                (Glyph::X, "Erase"),
                (Glyph::Y, "Space"),
                (Glyph::B, "Close"),
            ]
        } else if couch.achievements.is_some() && game {
            &[(Glyph::B, "Close")]
        } else if game {
            &[
                (Glyph::A, "Select"),
                (Glyph::B, "Back"),
                (Glyph::Y, "Favorite"),
            ]
        } else if couch.tab == Tab::Downloads {
            &[(Glyph::A, "Select"), (Glyph::Menu, "Menu")]
        } else if couch.tab == Tab::Library && couch.on_shelves.is_some() {
            &[
                (Glyph::A, "Choose"),
                (Glyph::X, "Search"),
                (Glyph::Menu, "Menu"),
            ]
        } else {
            &[
                (Glyph::A, "Open"),
                (Glyph::Y, "Favorite"),
                (Glyph::X, "Search"),
                (Glyph::Menu, "Menu"),
            ]
        };
        // B closes a notice first.
        let notice = self
            .notice
            .is_some()
            .then_some((Glyph::B, "Close the notice"));
        let hints = notice
            .iter()
            .chain(
                hints
                    .iter()
                    .filter(|(g, _)| notice.is_none() || !matches!(g, Glyph::B)),
            )
            .map(|(g, label)| {
                row![glyph(*g), text(*label).size(16).color(tokens().muted)]
                    .spacing(8)
                    .align_y(Alignment::Center)
                    .into()
            });
        container(Row::with_children(hints).spacing(28))
            .padding([0.0, MARGIN])
            .width(Length::Fill)
            .height(HINTS)
            .center_y(HINTS)
            .style(theme::couch_bar)
            .into()
    }

    /// A game's cover, ringed while it has the focus; pressed, it opens the game.
    fn couch_cover<'a>(
        &'a self,
        g: &'a LibraryGame,
        width: f32,
        focused: bool,
    ) -> Element<'a, Message> {
        let (width, height) = (width - 2.0 * RING, (width - 2.0 * RING) * 4.0 / 3.0);
        let art: Element<'a, Message> = match self.grid_cover(&g.id) {
            Some(h) => image(h)
                .content_fit(ContentFit::Cover)
                .width(width)
                .height(height)
                .border_radius(tokens().cover_radius)
                .into(),
            None => container(text(&g.title).size(16).align_x(Alignment::Center))
                .padding(12)
                .center(width)
                .height(height)
                .style(theme::placeholder)
                .into(),
        };
        button(art)
            .padding(RING)
            .on_press(Message::Select(g.id.clone()))
            .style(theme::couch_card(focused))
            .into()
    }

    /// Play time, last session, achievements and whether it is installed, on one line.
    fn couch_meta(&self, g: &LibraryGame) -> String {
        let mut parts = Vec::new();
        let played = self.played_seconds(&g.id);
        if played > 0 {
            parts.push(format!("{} played", duration(played)));
        }
        if let Some(last) = self.last_played(&g.id).filter(|t| *t > 0) {
            parts.push(format!("Last played {}", relative_day(last).to_lowercase()));
        }
        if let Some((unlocked, total)) = self.achievement_counts(&g.id).filter(|(_, t)| *t > 0) {
            parts.push(format!("{unlocked} / {total} achievements"));
        }
        parts.push(
            if self.is_installed(&g.id) {
                "Installed"
            } else {
                "Not installed"
            }
            .into(),
        );
        parts.join("  ·  ")
    }

    /// The key art of `g`, edge to edge, fading into the page, with its title and `below`.
    fn couch_hero<'a>(
        &'a self,
        g: &'a LibraryGame,
        title_size: f32,
        below: Element<'a, Message>,
    ) -> Element<'a, Message> {
        let art: Element<'a, Message> = match self.hero_art(g) {
            Some(h) => image(h)
                .content_fit(ContentFit::Cover)
                .width(Length::Fill)
                .height(Length::Fill)
                .into(),
            None => Space::new().width(Length::Fill).height(Length::Fill).into(),
        };
        let fade = container(Space::new())
            .width(Length::Fill)
            .height(Length::Fill)
            .style(theme::hero_fade);
        let front = container(
            column![
                text(&g.title).size(title_size).font(bold()),
                text(self.couch_meta(g)).size(18).color(tokens().muted),
                below,
            ]
            .spacing(14),
        )
        .padding(Padding::new(MARGIN).bottom(24.0))
        .width(Length::Fill)
        .height(Length::Fill)
        .align_y(Alignment::End);
        stack![art, fade, front]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    fn couch_home<'a>(&'a self, couch: &'a Couch) -> Element<'a, Message> {
        let rows = self.home_rows();
        let Some((row, col, focused)) = self.home_focus() else {
            return empty_page(
                "No games yet. Sign in to GOG, or show your Steam games, from the desktop mode.",
            );
        };
        let hero = container(self.couch_hero(focused, 44.0, Space::new().into()))
            .height((self.window.height * 0.42).max(220.0))
            .clip(true);
        let card = self.couch_card();
        let across = self.couch_columns();
        // The row with the focus, then the next one, cut by the window's bottom.
        let mut list = Column::new().spacing(GAP);
        for (r, home_row) in rows.iter().enumerate().skip(row).take(2) {
            let games = &home_row.games;
            let at = if r == row {
                col
            } else {
                couch
                    .home_cols
                    .get(r)
                    .copied()
                    .unwrap_or(0)
                    .min(games.len() - 1)
            };
            let start = at
                .saturating_sub(across.saturating_sub(2))
                .min(games.len().saturating_sub(across));
            let covers = games[start..(start + across).min(games.len())]
                .iter()
                .enumerate()
                .map(|(k, &i)| {
                    self.couch_cover(&self.library[i], card, r == row && start + k == at)
                });
            list = list.push(
                column![
                    text(home_row.title).size(22).font(semibold()),
                    Row::with_children(covers).spacing(GAP),
                ]
                .spacing(10),
            );
        }
        column![
            hero,
            container(list)
                .padding(Padding::ZERO.left(MARGIN).right(MARGIN).top(8.0))
                .height(Length::Fill)
                .clip(true),
        ]
        .into()
    }

    fn couch_library<'a>(&'a self, couch: &'a Couch) -> Element<'a, Message> {
        let games = self.couch_games();
        let chip =
            |label: String, ic: Option<Icon>, current: bool, focused: bool, msg: CouchMsg| {
                let color = if focused {
                    tokens().on_accent
                } else {
                    tokens().text
                };
                button(
                    row![]
                        .push(ic.map(|ic| icon(ic, 18.0, color)))
                        .push(text(label).size(18).font(semibold()))
                        .spacing(10)
                        .align_y(Alignment::Center),
                )
                .padding([10, 20])
                .on_press(Message::Couch(msg))
                .style(move |t, s| {
                    if focused {
                        theme::couch_button(true)(t, s)
                    } else {
                        theme::couch_tab(current)(t, s)
                    }
                })
                .into()
            };
        let shelves = Shelf::ALL.iter().enumerate().map(|(i, s)| {
            chip(
                s.to_string(),
                None,
                couch.shelf == *s,
                couch.on_shelves == Some(i),
                CouchMsg::Shelf(*s),
            )
        });
        let search = chip(
            if couch.search.is_empty() {
                "Search".into()
            } else {
                format!("“{}”", couch.search)
            },
            Some(Icon::Search),
            !couch.search.is_empty(),
            couch.on_shelves == Some(Shelf::ALL.len()),
            CouchMsg::OpenSearch,
        );
        let focused = (couch.on_shelves.is_none() && !games.is_empty())
            .then(|| couch.card.min(games.len() - 1));
        let counted = match focused {
            Some(i) => self.library[games[i]].title.clone(),
            None => format!("{} games", games.len()),
        };
        let header = row![
            Row::with_children(shelves).spacing(8),
            search,
            space().width(Length::Fill),
            text(counted).size(18).color(tokens().muted),
        ]
        .spacing(16)
        .align_y(Alignment::Center);
        let grid: Element<'a, Message> = if games.is_empty() {
            empty_page(if couch.search.is_empty() {
                "No games on this shelf."
            } else {
                "No game matches the search."
            })
        } else {
            let (across, down, card) = (self.couch_columns(), self.couch_rows(), self.couch_card());
            // One row more than fits, cut by the bottom: there is more below.
            let rows = (couch.top..=couch.top + down)
                .take_while(|r| r * across < games.len())
                .map(|r| {
                    let covers = (r * across..((r + 1) * across).min(games.len())).map(|k| {
                        self.couch_cover(&self.library[games[k]], card, focused == Some(k))
                    });
                    Row::with_children(covers).spacing(GAP).into()
                });
            Column::with_children(rows).spacing(GAP).into()
        };
        column![
            container(header).height(SHELVES).center_y(SHELVES),
            container(grid).height(Length::Fill).clip(true),
        ]
        .padding([0.0, MARGIN])
        .into()
    }

    /// The keyboard of the search, over the bottom of the library.
    fn couch_keyboard<'a>(
        &'a self,
        couch: &'a Couch,
        (at_row, at_col): (usize, usize),
    ) -> Element<'a, Message> {
        let gap = 8.0;
        let key_width = ((self.window.width * 0.7).min(900.0) - 9.0 * gap) / 10.0;
        let key = |label: String, width: f32, focused: bool, at: (usize, usize)| {
            button(container(text(label).size(20).font(semibold())).center_x(Length::Fill))
                .width(width)
                .padding([12, 0])
                .on_press(Message::Couch(CouchMsg::KeyAt(at.0, at.1)))
                .style(theme::couch_button(focused))
                .into()
        };
        let mut keys = Column::new().spacing(gap);
        for (r, line) in KEYS.iter().enumerate() {
            let line = line.chars().enumerate().map(|(c, ch)| {
                key(
                    ch.to_string(),
                    key_width,
                    (at_row, at_col) == (r, c),
                    (r, c),
                )
            });
            keys = keys.push(Row::with_children(line).spacing(gap));
        }
        let actions = KeyAction::ALL.iter().map(|a| {
            let span = a.span() as f32;
            let focused = at_row == KEYS.len() && KeyAction::at(at_col) == *a;
            key(
                a.label().into(),
                key_width * span + gap * (span - 1.0),
                focused,
                (KEYS.len(), a.start()),
            )
        });
        keys = keys.push(Row::with_children(actions).spacing(gap));
        let typed = if couch.search.is_empty() {
            text("Type a title").size(26).color(tokens().muted)
        } else {
            text(format!("{}▏", couch.search)).size(26)
        };
        let panel = container(column![typed, keys].spacing(18))
            .padding(24)
            .style(theme::couch_panel);
        container(panel)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::End)
            .padding(Padding::ZERO.bottom(HINTS + 16.0))
            .into()
    }

    fn couch_game<'a>(&'a self, couch: &'a Couch, g: &'a LibraryGame) -> Element<'a, Message> {
        let buttons = self.game_buttons(g);
        let focused = couch.button.min(buttons.len().saturating_sub(1));
        let buttons = buttons
            .into_iter()
            .enumerate()
            .map(|(i, (ic, label, msg))| {
                let on = i == focused;
                let color = match (on, msg.is_some()) {
                    (true, true) => tokens().on_accent,
                    (_, false) => tokens().muted,
                    (false, true) => tokens().text,
                };
                button(
                    row![icon(ic, 22.0, color), text(label).size(20).font(semibold())]
                        .spacing(12)
                        .align_y(Alignment::Center),
                )
                .padding([16, 28])
                .on_press_maybe(msg)
                .style(theme::couch_button(on))
                .into()
            });
        let below = column![Row::with_children(buttons).spacing(16)]
            .push(self.couch_game_status(g))
            .spacing(18)
            .into();
        self.couch_hero(g, 52.0, below)
    }

    /// Where an install or a session of the game stands, under its buttons.
    fn couch_game_status<'a>(&'a self, g: &'a LibraryGame) -> Option<Element<'a, Message>> {
        let line = |s: String| Some(text(s).size(18).color(tokens().muted).into());
        match self.install_views.get(&g.id) {
            Some(InstallView::Running { progress, .. }) => {
                let p = fraction(*progress);
                return Some(
                    row![
                        progress_bar(0.0..=1.0, p).girth(10).style(theme::progress),
                        text(format!("{:.0}%", p * 100.0)).size(18),
                    ]
                    .spacing(14)
                    .align_y(Alignment::Center)
                    .width(560)
                    .into(),
                );
            }
            Some(InstallView::Planning) => return line("Getting the install ready…".into()),
            Some(InstallView::Failed(e)) => return line(e.clone()),
            _ => {}
        }
        if self.installs.contains_key(&g.id)
            && slatty_core::store::of(&g.id).is_ok_and(|s| s == slatty_core::store::Store::Gog)
        {
            let (_, _, status) = self.cloud_status(g);
            return line(format!("Cloud saves: {status}"));
        }
        None
    }

    /// The game's achievements, beside its page.
    fn couch_achievements<'a>(
        &'a self,
        couch: &'a Couch,
        g: &'a LibraryGame,
    ) -> Option<Element<'a, Message>> {
        let at = couch.achievements?;
        let list = self.achievement_list(&g.id)?;
        let unlocked = list.iter().filter(|a| a.date_unlocked.is_some()).count();
        let shown = (((self.window.height - 2.0 * MARGIN - 80.0) / 84.0) as usize).max(3);
        let at = at.min(list.len().saturating_sub(1));
        let start = at
            .saturating_sub(shown / 2)
            .min(list.len().saturating_sub(shown));
        let items = list
            .iter()
            .enumerate()
            .skip(start)
            .take(shown)
            .map(|(i, a)| {
                let done = a.date_unlocked.is_some();
                let (name, about) = if a.visible || done {
                    (a.name.as_str(), a.description.as_str())
                } else {
                    ("Hidden achievement", "Unlock it to see what it is.")
                };
                container(
                    row![
                        icon(
                            if done {
                                Icon::CircleCheck
                            } else {
                                Icon::Trophy
                            },
                            22.0,
                            if done {
                                tokens().success
                            } else {
                                tokens().muted
                            },
                        ),
                        column![
                            text(name).size(18).font(semibold()),
                            text(about).size(15).color(tokens().muted),
                        ]
                        .spacing(4),
                    ]
                    .spacing(14)
                    .align_y(Alignment::Center),
                )
                .padding([12, 16])
                .width(Length::Fill)
                .style(theme::couch_item(i == at))
                .into()
            });
        let panel = container(
            column![
                text(format!("Achievements  ·  {unlocked} / {}", list.len()))
                    .size(24)
                    .font(bold()),
                Column::with_children(items).spacing(10),
            ]
            .spacing(18),
        )
        .padding(24)
        .width((self.window.width * 0.42).max(420.0))
        .height(Length::Fill)
        .style(theme::couch_panel);
        Some(
            container(panel)
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(Alignment::End)
                .padding(Padding::new(MARGIN / 2.0).bottom(HINTS + MARGIN / 2.0))
                .into(),
        )
    }

    fn couch_downloads<'a>(&'a self, couch: &'a Couch) -> Element<'a, Message> {
        let rows = self.download_rows();
        if rows.is_empty() {
            return empty_page("Nothing is downloading.");
        }
        let (at, pressed) = couch.download;
        let at = at.min(rows.len() - 1);
        let shown = (((self.window.height - TOP_BAR - HINTS - MARGIN) / 128.0) as usize).max(1);
        let start = at.saturating_sub(shown - 1);
        let items = rows
            .into_iter()
            .enumerate()
            .skip(start)
            .take(shown)
            .map(|(r, row_data)| {
                let focused = r == at;
                let pressed = pressed.min(row_data.buttons.len().saturating_sub(1));
                let buttons = row_data
                    .buttons
                    .into_iter()
                    .enumerate()
                    .map(|(b, (label, msg))| {
                        button(text(label).size(18).font(semibold()))
                            .padding([12, 22])
                            .on_press(msg)
                            .style(theme::couch_button(focused && b == pressed))
                            .into()
                    });
                let cover: Element<'_, Message> = match self.grid_cover(&row_data.game_id) {
                    Some(h) => image(h)
                        .content_fit(ContentFit::Cover)
                        .width(66)
                        .height(88)
                        .border_radius(tokens().cover_radius)
                        .into(),
                    None => container(Space::new())
                        .width(66)
                        .height(88)
                        .style(theme::placeholder)
                        .into(),
                };
                let about = column![
                    text(row_data.title).size(22).font(semibold()),
                    text(row_data.status).size(16).color(tokens().muted),
                ]
                .push(
                    row_data
                        .progress
                        .map(|p| progress_bar(0.0..=1.0, p).girth(8).style(theme::progress)),
                )
                .spacing(8)
                .width(Length::Fill);
                container(
                    row![cover, about, Row::with_children(buttons).spacing(12)]
                        .spacing(20)
                        .align_y(Alignment::Center),
                )
                .padding(16)
                .width(Length::Fill)
                .style(theme::couch_item(focused))
                .into()
            });
        container(Column::with_children(items).spacing(12))
            .padding([MARGIN / 2.0, MARGIN])
            .into()
    }
}

impl App {
    /// Asks how a game that can be started several ways is started, the first time.
    fn couch_launch<'a>(&'a self, game_id: &'a str, at: usize) -> Element<'a, Message> {
        let ways = self
            .launch_options
            .get(game_id)
            .map(Vec::as_slice)
            .unwrap_or_default();
        let choices = ways.iter().enumerate().map(|(i, name)| {
            let focused = i == at;
            let note = if focused {
                tokens().on_accent
            } else {
                tokens().muted
            };
            button(
                row![text(name.as_str()).size(22).font(semibold())]
                    .push((i == 0).then(|| text("Default").size(16).color(note)))
                    .spacing(14)
                    .align_y(Alignment::Center),
            )
            .width(560)
            .padding([16, 24])
            .on_press(Message::LaunchAs(game_id.to_string(), name.clone()))
            .style(theme::couch_button(focused))
            .into()
        });
        container(
            container(
                column![
                    text(format!("Start {}", self.title_of(game_id)))
                        .size(28)
                        .font(bold()),
                    text("Your choice is kept for the next launches.")
                        .size(17)
                        .color(tokens().muted),
                    Column::with_children(choices).spacing(10),
                ]
                .spacing(20)
                .max_width(720),
            )
            .padding(28)
            .style(theme::couch_panel),
        )
        .center(Length::Fill)
        .style(theme::backdrop)
        .into()
    }
}

/// The menu Start opens.
fn couch_menu<'a>(at: usize) -> Element<'a, Message> {
    let items = MenuItem::ALL.iter().enumerate().map(|(i, item)| {
        button(text(item.label()).size(22).font(semibold()))
            .width(380)
            .padding([16, 24])
            .on_press(Message::Couch(CouchMsg::Menu(*item)))
            .style(theme::couch_button(i == at))
            .into()
    });
    container(
        container(Column::with_children(items).spacing(10))
            .padding(24)
            .style(theme::couch_panel),
    )
    .center(Length::Fill)
    .style(theme::backdrop)
    .into()
}

/// The question asked before quitting while work runs.
fn couch_quit<'a>(work: &'a [String], at: usize) -> Element<'a, Message> {
    let choice = |label: &'static str, msg: Message, on: bool| {
        button(text(label).size(20).font(semibold()))
            .padding([14, 28])
            .on_press(msg)
            .style(theme::couch_button(on))
    };
    container(
        container(
            column![
                text("Quit SlattyLauncher?").size(28).font(bold()),
                Column::with_children(
                    work.iter()
                        .map(|w| text(w.as_str()).size(17).color(tokens().muted).into())
                )
                .spacing(8),
                row![
                    choice("Keep running", Message::CancelQuit, at == 0),
                    choice("Quit", Message::ConfirmQuit, at == 1),
                ]
                .spacing(14),
            ]
            .spacing(20)
            .max_width(720),
        )
        .padding(28)
        .style(theme::couch_panel),
    )
    .center(Length::Fill)
    .style(theme::backdrop)
    .into()
}

fn empty_page<'a>(note: &'a str) -> Element<'a, Message> {
    container(text(note).size(20).color(tokens().muted))
        .center(Length::Fill)
        .into()
}

/// A gamepad button, as drawn in the hints and beside the tabs.
fn glyph<'a>(g: Glyph) -> Element<'a, Message> {
    let label = match g {
        Glyph::A => "A",
        Glyph::B => "B",
        Glyph::X => "X",
        Glyph::Y => "Y",
        Glyph::Lb => "LB",
        Glyph::Rb => "RB",
        Glyph::Menu => {
            let bar = || {
                container(Space::new())
                    .width(12)
                    .height(2)
                    .style(theme::couch_glyph_bar)
            };
            return container(column![bar(), bar(), bar()].spacing(3))
                .center(28)
                .style(theme::couch_glyph)
                .into();
        }
    };
    let content = text(label).size(13).font(bold());
    match g {
        Glyph::Lb | Glyph::Rb => container(content).padding([0, 10]).center_y(28),
        _ => container(content).center(28),
    }
    .style(theme::couch_glyph)
    .into()
}
