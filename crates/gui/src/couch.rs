//! The couch mode: the library for a gamepad, full screen, for a television or a Steam Deck. It
//! shows the same games and starts the same work as the desktop window, through the same
//! messages; only the way around differs, a focus moved with the d-pad, the left stick or the
//! arrow keys. Settings stay in the desktop window.

use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};

use iced::{Size, Task};
use slatty_core::installer::Progress;
use slatty_core::library::LibraryGame;

use crate::downloads::{DownloadsMsg, Preempt};
use crate::install::{InstallMsg, InstallView};
use crate::library::{Filters, Shelf, Sort};
use crate::offline::{OfflineMsg, of_entry};
use crate::pad::{Key, Nav};
use crate::{App, Interrupted, Message, Page, Panel};

/// Asked for as the program starts: with `--couch`, or in a gamescope session such as the Steam
/// Deck's Game Mode.
static AT_START: AtomicBool = AtomicBool::new(false);

pub fn ask_at_start() -> bool {
    let wanted = std::env::args_os().any(|a| a == "--couch")
        || std::env::var_os("XDG_CURRENT_DESKTOP")
            .is_some_and(|d| d.eq_ignore_ascii_case("gamescope"))
        || std::env::var_os("GAMESCOPE_WAYLAND_DISPLAY").is_some();
    AT_START.store(wanted, Ordering::Relaxed);
    wanted
}

pub fn at_start() -> bool {
    AT_START.load(Ordering::Relaxed)
}

/// Space around the pages and between covers, and the heights of the bars above and below them.
pub const MARGIN: f32 = 48.0;
pub const GAP: f32 = 20.0;
pub const TOP_BAR: f32 = 84.0;
pub const HINTS: f32 = 60.0;
/// Height of the shelves above the library's covers.
pub const SHELVES: f32 = 72.0;
/// Height of a notice of one line, above the pages while it shows.
pub const NOTICE: f32 = 64.0;
/// Room the focus ring takes around a cover.
pub const RING: f32 = 4.0;
/// How many games the Continue row holds.
const RECENT: usize = 20;
/// How long the focus stays on a game of Home before its key art's copy is made.
const REST: std::time::Duration = std::time::Duration::from_millis(400);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    Home,
    Library,
    Downloads,
}

impl Tab {
    pub const ALL: [Tab; 3] = [Tab::Home, Tab::Library, Tab::Downloads];

    pub fn label(self) -> &'static str {
        match self {
            Tab::Home => "Home",
            Tab::Library => "Library",
            Tab::Downloads => "Downloads",
        }
    }

    fn step(self, forward: bool) -> Tab {
        let i = Tab::ALL.iter().position(|t| *t == self).unwrap_or(0);
        let i = if forward {
            (i + 1).min(Tab::ALL.len() - 1)
        } else {
            i.saturating_sub(1)
        };
        Tab::ALL[i]
    }
}

/// The items of the menu Start opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuItem {
    Desktop,
    Quit,
}

impl MenuItem {
    pub const ALL: [MenuItem; 2] = [MenuItem::Desktop, MenuItem::Quit];

    pub fn label(self) -> &'static str {
        match self {
            MenuItem::Desktop => "Desktop mode",
            MenuItem::Quit => "Quit SlattyLauncher",
        }
    }
}

/// The on-screen keyboard: rows of characters, then a row of actions.
pub const KEYS: [&str; 4] = ["1234567890", "qwertyuiop", "asdfghjkl'", "zxcvbnm-.&"];
/// Actions of the keyboard's last row, and the columns each spans.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyAction {
    Space,
    Erase,
    Clear,
    Done,
}

impl KeyAction {
    pub const ALL: [KeyAction; 4] = [
        KeyAction::Space,
        KeyAction::Erase,
        KeyAction::Clear,
        KeyAction::Done,
    ];
    /// First column of each, out of ten.
    const STARTS: [usize; 4] = [0, 4, 6, 8];

    pub fn label(self) -> &'static str {
        match self {
            KeyAction::Space => "Space",
            KeyAction::Erase => "⌫",
            KeyAction::Clear => "Clear",
            KeyAction::Done => "Done",
        }
    }

    /// Columns it spans, out of ten.
    pub fn span(self) -> usize {
        match self {
            KeyAction::Space => 4,
            _ => 2,
        }
    }

    /// The action at column `col` of the last row.
    pub fn at(col: usize) -> KeyAction {
        let i = KeyAction::STARTS
            .iter()
            .rposition(|s| *s <= col)
            .unwrap_or(0);
        KeyAction::ALL[i]
    }

    pub fn start(self) -> usize {
        KeyAction::STARTS[KeyAction::ALL.iter().position(|a| *a == self).unwrap_or(0)]
    }
}

/// Where the focus is in each tab and over them, kept when moving between tabs.
#[derive(Debug, Default)]
pub struct Couch {
    pub tab: Tab,
    /// Home: the row with the focus, and the cover each row was left on.
    pub home_row: usize,
    pub home_cols: Vec<usize>,
    /// Home: the games those places hold, with their row's title, as last seen. When the rows are
    /// made again (a favorite toggled, an install ended, a game played), the focus finds them where
    /// they went (`App::refocus_couch`); a move empties them, to be read again from the places.
    pub home_games: Vec<(&'static str, String)>,
    /// Whether the player moved or used Home's focus since the couch mode opened: until then it
    /// stays on the first cover of the first row, whatever comes to fill the rows.
    pub home_chosen: bool,
    /// Home: the game the focus rested on a moment, whose key art's copy is made (`REST`).
    pub rested: Option<String>,
    /// Library: the shelf shown, the cover with the focus and the first row shown; on the
    /// shelves above, the one with the focus (the search after them).
    pub shelf: Shelf,
    pub card: usize,
    /// The game at `card` as last seen, found again when the shelf's games are made again; none
    /// after a move, or another shelf or search, which start from `card`.
    pub card_game: Option<String>,
    /// Whether the player moved or used the focus on this shelf: until then it stays on its first
    /// cover, whatever comes to fill the shelf.
    pub card_chosen: bool,
    pub top: usize,
    pub on_shelves: Option<usize>,
    pub search: String,
    /// The on-screen keyboard, while open: the key with the focus (row, column out of ten).
    pub keyboard: Option<(usize, usize)>,
    /// Whether letters came from a real keyboard since its focus last moved: Enter then ends the
    /// search, rather than press the key with the focus (a remote's arrows and OK).
    pub typed: bool,
    /// Downloads: the row and its button with the focus.
    pub download: (usize, usize),
    /// The game page: its button with the focus, and the achievement with the focus while the
    /// list is open.
    pub button: usize,
    pub achievements: Option<usize>,
    /// The menu Start opens, while open: the item with the focus.
    pub menu: Option<usize>,
    /// The button with the focus in the quit question.
    pub quit: usize,
    /// The way with the focus, while asked how a game is started (`App::launch_prompt`).
    pub launch: usize,
}

/// A row of covers of the home page.
#[derive(Debug, Clone)]
pub struct HomeRow {
    pub title: &'static str,
    /// Places in `library`.
    pub games: Rc<[usize]>,
}

/// A row of the Downloads tab, and its buttons.
pub struct DownloadRow {
    /// The game it is for.
    pub game_id: String,
    pub title: String,
    pub status: String,
    /// From 0 to 1, while it downloads.
    pub progress: Option<f32>,
    pub buttons: Vec<(&'static str, Message)>,
}

#[derive(Debug, Clone)]
pub enum CouchMsg {
    Enter,
    Leave,
    Nav(Nav),
    Key(Key),
    /// What a click or a tap chose.
    Tab(Tab),
    Shelf(Shelf),
    OpenSearch,
    /// A key of the on-screen keyboard (row, column).
    KeyAt(usize, usize),
    /// Installs a game with the default choices.
    Install(String),
    /// The focus has rested on a game of Home a moment: its key art's copy is made.
    Rested(String),
    Achievements,
    OpenMenu,
    Menu(MenuItem),
}

impl App {
    pub fn update_couch(&mut self, msg: CouchMsg) -> Task<Message> {
        match msg {
            CouchMsg::Enter => {
                let screen = self.screen();
                self.couch = Some(Couch::default());
                self.fit_screen(screen);
                self.selected = None;
                self.panel = None;
                self.dialog = None;
                self.edit = None;
                self.context_menu = None;
                self.add_game = None;
                return Task::batch([fullscreen(true), self.home_art()]);
            }
            CouchMsg::Leave => {
                let screen = self.screen();
                self.couch = None;
                self.fit_screen(screen);
                return Task::batch([fullscreen(false), self.scale_art()]);
            }
            // A gamepad is read whatever window is in front: a game's moves are not the launcher's.
            CouchMsg::Nav(_) if self.away() => {}
            // Typed on a real keyboard, the search ends with Enter, as it would elsewhere.
            CouchMsg::Key(Key::Nav(Nav::Confirm))
                if self.typing() && self.couch.as_ref().is_some_and(|c| c.typed) =>
            {
                if let Some(couch) = &mut self.couch {
                    couch.keyboard = None;
                }
            }
            CouchMsg::Nav(nav) | CouchMsg::Key(Key::Nav(nav)) => return self.couch_nav(nav),
            CouchMsg::Key(Key::Char(c)) => return self.typed(c),
            CouchMsg::Key(Key::Erase) => {
                if !self.typing() {
                    // Back, but not at a tab, where Back opens the menu: one Backspace too many
                    // after a search would open it, and the next Enter leave the couch mode.
                    let at_tab = self.selected.is_none()
                        && self.notice.is_none()
                        && self.quit_confirm.is_none()
                        && self.launch_prompt.is_none()
                        && self.couch.as_ref().is_some_and(|c| c.menu.is_none());
                    if at_tab {
                        return Task::none();
                    }
                    return self.couch_nav(Nav::Back);
                }
                if let Some(couch) = &mut self.couch {
                    couch.search.pop();
                }
                self.searched();
            }
            CouchMsg::Tab(tab) => {
                self.selected = None;
                if let Some(couch) = &mut self.couch {
                    couch.tab = tab;
                }
                // Its rows may have changed meanwhile, its focus on another game.
                if tab == Tab::Home {
                    return self.home_art();
                }
            }
            CouchMsg::Shelf(shelf) => {
                if let Some(couch) = &mut self.couch {
                    couch.shelf = shelf;
                    couch.card = 0;
                    couch.card_game = None;
                    couch.card_chosen = false;
                    couch.top = 0;
                }
            }
            CouchMsg::OpenSearch => self.open_search(),
            CouchMsg::KeyAt(row, col) => {
                if let Some(couch) = &mut self.couch {
                    couch.keyboard = Some((row, col));
                }
                return self.press_key();
            }
            CouchMsg::Install(game_id) => {
                self.start_planned.insert(game_id.clone());
                return self.update_install(InstallMsg::Prepare(game_id, None));
            }
            CouchMsg::Rested(game_id) => {
                if self.home_focus().is_some_and(|(_, _, g)| g.id == game_id) {
                    if let Some(couch) = &mut self.couch {
                        couch.rested = Some(game_id);
                    }
                    return self.scale_art();
                }
            }
            CouchMsg::Achievements => {
                if let Some(couch) = &mut self.couch {
                    couch.achievements = Some(0);
                }
            }
            CouchMsg::OpenMenu => {
                if let Some(couch) = &mut self.couch {
                    couch.menu = Some(0);
                }
            }
            CouchMsg::Menu(item) => {
                if let Some(couch) = &mut self.couch {
                    couch.menu = None;
                }
                return match item {
                    MenuItem::Desktop => self.update_couch(CouchMsg::Leave),
                    MenuItem::Quit => self.update(Message::CloseRequested),
                };
            }
        }
        Task::none()
    }

    /// Whether the player is elsewhere: the window behind another, or a game running.
    pub(crate) fn away(&self) -> bool {
        self.unfocused
            || self.play.as_ref().is_some_and(|p| p.running)
            || !self.steam.running.is_empty()
    }

    fn couch_nav(&mut self, nav: Nav) -> Task<Message> {
        let Some(couch) = &self.couch else {
            return Task::none();
        };
        // The notice's own Close is out of the focus's way.
        if nav == Nav::Back && self.notice.take().is_some() {
            return Task::none();
        }
        if self.quit_confirm.is_some() {
            return self.quit_nav(nav);
        }
        if let Some(game_id) = self.launch_prompt.clone() {
            return self.launch_nav(game_id, nav);
        }
        if couch.menu.is_some() {
            return self.menu_nav(nav);
        }
        if self.typing() {
            return self.keyboard_nav(nav);
        }
        if let Some(id) = self.selected.clone() {
            return self.game_nav(&id, nav);
        }
        match nav {
            Nav::PrevTab | Nav::NextTab => {
                let couch = self.couch.as_mut().expect("couch mode");
                couch.tab = couch.tab.step(nav == Nav::NextTab);
                if couch.tab == Tab::Home {
                    return self.home_art();
                }
                Task::none()
            }
            // At a tab, Back has nowhere to go but the menu, and its way out.
            Nav::Menu | Nav::Back => self.update_couch(CouchMsg::OpenMenu),
            Nav::Search => {
                self.open_search();
                Task::none()
            }
            _ => match couch.tab {
                Tab::Home => self.home_nav(nav),
                Tab::Library => self.library_nav(nav),
                Tab::Downloads => self.downloads_nav(nav),
            },
        }
    }

    fn home_nav(&mut self, nav: Nav) -> Task<Message> {
        let rows = self.home_rows();
        let Some(couch) = &mut self.couch else {
            return Task::none();
        };
        if rows.is_empty() {
            return Task::none();
        }
        couch.home_chosen = true;
        let row = couch.home_row.min(rows.len() - 1);
        if couch.home_cols.len() < rows.len() {
            couch.home_cols.resize(rows.len(), 0);
        }
        let len = rows[row].games.len();
        let col = couch.home_cols[row].min(len.saturating_sub(1));
        let game = |col: usize| self.library[rows[row].games[col]].id.clone();
        match nav {
            Nav::Up => couch.home_row = row.saturating_sub(1),
            Nav::Down => couch.home_row = (row + 1).min(rows.len() - 1),
            Nav::Left => couch.home_cols[row] = col.saturating_sub(1),
            Nav::Right => couch.home_cols[row] = (col + 1).min(len.saturating_sub(1)),
            Nav::Confirm if len > 0 => return self.update(Message::Select(game(col))),
            // The favorite toggled may change the rows: the focus finds its game again
            // (`App::refocus_couch`).
            Nav::Action if len > 0 => {
                let toggled = self.update(Message::ToggleFavorite(game(col)));
                return Task::batch([toggled, self.home_art()]);
            }
            _ => return Task::none(),
        }
        // Moved: which games the focus is on is read again from its places.
        couch.home_games.clear();
        self.home_art()
    }

    /// Asks for the key art of the game with the focus on the home page, unless it is kept, and
    /// for its copy at the window's width once the focus rests there: a direction held over a row
    /// would ask for one per game passed over, each long work in the background.
    pub fn home_art(&mut self) -> Task<Message> {
        if self.couch.is_none() {
            return Task::none();
        }
        // Its rows may have been made again since the end of the last message (another tab
        // showed, or a favorite was just toggled): its focus is found again first.
        self.refocus_couch();
        let Some((_, _, g)) = self.home_focus() else {
            return Task::none();
        };
        let (id, art) = (g.id.clone(), g.background.clone());
        let rest = Task::perform(async { tokio::time::sleep(REST).await }, move |_| {
            Message::Couch(CouchMsg::Rested(id.clone()))
        });
        Task::batch([self.request_images(art.into_iter().collect()), rest])
    }

    fn library_nav(&mut self, nav: Nav) -> Task<Message> {
        let games = self.couch_games();
        let (columns, rows) = (self.couch_columns(), self.couch_rows());
        let Some(couch) = &mut self.couch else {
            return Task::none();
        };
        if let Some(i) = couch.on_shelves {
            let last = Shelf::ALL.len();
            match nav {
                Nav::Left => couch.on_shelves = Some(i.saturating_sub(1)),
                Nav::Right => couch.on_shelves = Some((i + 1).min(last)),
                Nav::Down if !games.is_empty() => couch.on_shelves = None,
                Nav::Confirm if i == last => self.open_search(),
                Nav::Confirm => {
                    couch.shelf = Shelf::ALL[i];
                    couch.card = 0;
                    couch.card_game = None;
                    couch.card_chosen = false;
                    couch.top = 0;
                }
                _ => {}
            }
            return Task::none();
        }
        couch.card_chosen = true;
        let len = games.len();
        let card = couch.card.min(len.saturating_sub(1));
        match nav {
            Nav::Up if card < columns => {
                couch.on_shelves = Some(
                    Shelf::ALL
                        .iter()
                        .position(|s| *s == couch.shelf)
                        .unwrap_or(0),
                );
            }
            Nav::Up => couch.card = card - columns,
            Nav::Down if card / columns < len.saturating_sub(1) / columns => {
                couch.card = (card + columns).min(len - 1);
            }
            Nav::Left if card % columns > 0 => couch.card = card - 1,
            Nav::Right if card % columns + 1 < columns && card + 1 < len => couch.card = card + 1,
            Nav::Confirm if len > 0 => {
                let id = self.library[games[card]].id.clone();
                return self.update(Message::Select(id));
            }
            Nav::Action if len > 0 => {
                let id = self.library[games[card]].id.clone();
                return self.update(Message::ToggleFavorite(id));
            }
            _ => {}
        }
        // Moved: which game the focus is on is read again from its place.
        couch.card_game = None;
        // The row with the focus stays in view.
        let row = couch.card / columns;
        if row < couch.top {
            couch.top = row;
        } else if row >= couch.top + rows {
            couch.top = row + 1 - rows;
        }
        Task::none()
    }

    fn downloads_nav(&mut self, nav: Nav) -> Task<Message> {
        let rows = self.download_rows();
        let Some(couch) = &mut self.couch else {
            return Task::none();
        };
        if rows.is_empty() {
            return Task::none();
        }
        let row = couch.download.0.min(rows.len() - 1);
        let buttons = rows[row].buttons.len();
        let button = couch.download.1.min(buttons.saturating_sub(1));
        match nav {
            Nav::Up => couch.download = (row.saturating_sub(1), 0),
            Nav::Down => couch.download = ((row + 1).min(rows.len() - 1), 0),
            Nav::Left => couch.download.1 = button.saturating_sub(1),
            Nav::Right => couch.download.1 = (button + 1).min(buttons.saturating_sub(1)),
            Nav::Confirm if buttons > 0 => {
                let msg = rows[row].buttons[button].1.clone();
                return self.update(msg);
            }
            _ => {}
        }
        Task::none()
    }

    fn game_nav(&mut self, game_id: &str, nav: Nav) -> Task<Message> {
        let Some(g) = self.library.iter().find(|g| g.id == game_id).cloned() else {
            return self.update(Message::CloseDetail);
        };
        let buttons = self.game_buttons(&g);
        let achievements = self.achievement_list(game_id).map_or(0, |a| a.len());
        let Some(couch) = &mut self.couch else {
            return Task::none();
        };
        if let Some(i) = couch.achievements {
            match nav {
                Nav::Up => couch.achievements = Some(i.saturating_sub(1)),
                Nav::Down => couch.achievements = Some((i + 1).min(achievements.saturating_sub(1))),
                Nav::Back | Nav::Confirm => couch.achievements = None,
                _ => {}
            }
            return Task::none();
        }
        let button = couch.button.min(buttons.len().saturating_sub(1));
        match nav {
            Nav::Left => couch.button = button.saturating_sub(1),
            Nav::Right => couch.button = (button + 1).min(buttons.len().saturating_sub(1)),
            Nav::Back => {
                couch.button = 0;
                return self.update(Message::CloseDetail);
            }
            Nav::Action => return self.update(Message::ToggleFavorite(g.id.clone())),
            Nav::Confirm => {
                if let Some(msg) = buttons.get(button).and_then(|b| b.2.clone()) {
                    return self.update(msg);
                }
            }
            _ => {}
        }
        Task::none()
    }

    fn menu_nav(&mut self, nav: Nav) -> Task<Message> {
        let Some(couch) = &mut self.couch else {
            return Task::none();
        };
        let i = couch.menu.unwrap_or(0);
        match nav {
            Nav::Up => couch.menu = Some(i.saturating_sub(1)),
            Nav::Down => couch.menu = Some((i + 1).min(MenuItem::ALL.len() - 1)),
            Nav::Back | Nav::Menu => couch.menu = None,
            Nav::Confirm => return self.update_couch(CouchMsg::Menu(MenuItem::ALL[i])),
            _ => {}
        }
        Task::none()
    }

    /// The question asked before quitting while work runs: Keep running, or Quit.
    fn quit_nav(&mut self, nav: Nav) -> Task<Message> {
        let Some(couch) = &mut self.couch else {
            return Task::none();
        };
        match nav {
            Nav::Left => couch.quit = 0,
            Nav::Right => couch.quit = 1,
            Nav::Back => {
                couch.quit = 0;
                return self.update(Message::CancelQuit);
            }
            Nav::Confirm => {
                let quit = std::mem::take(&mut couch.quit) == 1;
                return self.update(if quit {
                    Message::ConfirmQuit
                } else {
                    Message::CancelQuit
                });
            }
            _ => {}
        }
        Task::none()
    }

    /// The launch question answered, however: the next one starts on its first way.
    pub fn forget_launch_focus(&mut self) {
        if let Some(couch) = &mut self.couch {
            couch.launch = 0;
        }
    }

    /// The question asked the first time a game that can be started several ways is played.
    fn launch_nav(&mut self, game_id: String, nav: Nav) -> Task<Message> {
        let ways = self
            .launch_options
            .get(&game_id)
            .cloned()
            .unwrap_or_default();
        let Some(couch) = &mut self.couch else {
            return Task::none();
        };
        let at = couch.launch.min(ways.len().saturating_sub(1));
        match nav {
            Nav::Up => couch.launch = at.saturating_sub(1),
            Nav::Down => couch.launch = (at + 1).min(ways.len().saturating_sub(1)),
            Nav::Back => return self.update(Message::CancelLaunch),
            Nav::Confirm => {
                return self.update(match ways.into_iter().nth(at) {
                    Some(way) => Message::LaunchAs(game_id, way),
                    None => Message::CancelLaunch,
                });
            }
            _ => {}
        }
        Task::none()
    }

    fn keyboard_nav(&mut self, nav: Nav) -> Task<Message> {
        let Some(couch) = &mut self.couch else {
            return Task::none();
        };
        let (row, col) = couch.keyboard.unwrap_or((1, 0));
        let last = KEYS.len();
        if matches!(nav, Nav::Up | Nav::Down | Nav::Left | Nav::Right) {
            couch.typed = false;
        }
        match nav {
            Nav::Up => couch.keyboard = Some((row.saturating_sub(1), col)),
            Nav::Down => couch.keyboard = Some(((row + 1).min(last), col)),
            // On the row of actions, a move goes from one action to the next.
            Nav::Left if row == last => {
                let i = KeyAction::ALL
                    .iter()
                    .position(|a| *a == KeyAction::at(col))
                    .unwrap_or(0);
                couch.keyboard = Some((row, KeyAction::ALL[i.saturating_sub(1)].start()));
            }
            Nav::Right if row == last => {
                let i = KeyAction::ALL
                    .iter()
                    .position(|a| *a == KeyAction::at(col))
                    .unwrap_or(0);
                let next = KeyAction::ALL[(i + 1).min(KeyAction::ALL.len() - 1)];
                couch.keyboard = Some((row, next.start()));
            }
            Nav::Left => couch.keyboard = Some((row, col.saturating_sub(1))),
            Nav::Right => couch.keyboard = Some((row, (col + 1).min(9))),
            Nav::Confirm => return self.press_key(),
            Nav::Search => {
                couch.search.pop();
                self.searched();
            }
            Nav::Action => {
                couch.search.push(' ');
                self.searched();
            }
            Nav::Back | Nav::Menu => couch.keyboard = None,
            _ => {}
        }
        Task::none()
    }

    /// The key of the on-screen keyboard with the focus, pressed.
    fn press_key(&mut self) -> Task<Message> {
        let Some(couch) = &mut self.couch else {
            return Task::none();
        };
        let Some((row, col)) = couch.keyboard else {
            return Task::none();
        };
        match KEYS.get(row) {
            Some(keys) => {
                if let Some(c) = keys.chars().nth(col) {
                    couch.search.push(c);
                }
            }
            None => match KeyAction::at(col) {
                KeyAction::Space => couch.search.push(' '),
                KeyAction::Erase => {
                    couch.search.pop();
                }
                KeyAction::Clear => couch.search.clear(),
                KeyAction::Done => couch.keyboard = None,
            },
        }
        self.searched();
        Task::none()
    }

    /// A character typed on a keyboard: into the search, which it opens from a list.
    fn typed(&mut self, c: char) -> Task<Message> {
        let typing = self.typing();
        let Some(couch) = &mut self.couch else {
            return Task::none();
        };
        if typing {
            couch.search.push(c);
            couch.typed = true;
            self.searched();
            return Task::none();
        }
        if c == ' ' {
            return self.couch_nav(Nav::Action);
        }
        let browsing = self.selected.is_none()
            && couch.menu.is_none()
            && couch.tab != Tab::Downloads
            && self.quit_confirm.is_none();
        if browsing {
            self.open_search();
            if let Some(couch) = &mut self.couch {
                couch.search.push(c);
                couch.typed = true;
            }
            self.searched();
        }
        Task::none()
    }

    /// Whether the on-screen keyboard shows, over the library, and so takes the presses: open, it
    /// is hidden by a game's page or another tab.
    pub(crate) fn typing(&self) -> bool {
        self.selected.is_none()
            && self
                .couch
                .as_ref()
                .is_some_and(|c| c.keyboard.is_some() && c.tab == Tab::Library)
    }

    /// The library, with the keyboard over it.
    fn open_search(&mut self) {
        if let Some(couch) = &mut self.couch {
            couch.tab = Tab::Library;
            couch.on_shelves = None;
            couch.keyboard = Some((1, 0));
            couch.typed = false;
        }
    }

    /// The search changed: the covers shown start again from the first.
    fn searched(&mut self) {
        if let Some(couch) = &mut self.couch {
            couch.card = 0;
            couch.card_game = None;
            couch.card_chosen = false;
            couch.top = 0;
        }
    }

    /// The games of the couch library: its shelf, matching its search, by name.
    pub fn couch_games(&self) -> Rc<[usize]> {
        let Some(couch) = &self.couch else {
            return Rc::from([]);
        };
        let key = (
            couch.search.clone(),
            couch.shelf,
            Filters::default(),
            Sort::NameAsc,
            self.library.len(),
        );
        let mut kept = self.lists.couch.borrow_mut();
        match &*kept {
            Some((k, games)) if *k == key => games.clone(),
            _ => {
                let games: Rc<[usize]> = self
                    .find_games(
                        &couch.search,
                        couch.shelf,
                        Filters::default(),
                        Sort::NameAsc,
                    )
                    .into();
                *kept = Some((key, games.clone()));
                games
            }
        }
    }

    /// The rows of the home page: the games played lately, the favorites, the games installed;
    /// the whole library when there is none of these yet.
    pub fn home_rows(&self) -> Rc<[HomeRow]> {
        if let Some(rows) = &*self.lists.couch_home.borrow() {
            return rows.clone();
        }
        let shown: Vec<usize> = (0..self.library.len())
            .filter(|&i| !self.is_hidden(&self.library[i].id))
            .collect();
        let by_name = |mut games: Vec<usize>| -> Rc<[usize]> {
            games.sort_by_cached_key(|&i| self.sort_name(&self.library[i]));
            games.into()
        };
        let mut recent: Vec<(i64, usize)> = shown
            .iter()
            .filter_map(|&i| Some((self.last_played(&self.library[i].id)?, i)))
            .filter(|(t, _)| *t > 0)
            .collect();
        recent.sort_by_key(|(t, _)| std::cmp::Reverse(*t));
        let recent: Rc<[usize]> = recent.into_iter().take(RECENT).map(|(_, i)| i).collect();
        let favorites = by_name(
            shown
                .iter()
                .copied()
                .filter(|&i| self.favorites.contains(&self.library[i].id))
                .collect(),
        );
        let installed = by_name(
            shown
                .iter()
                .copied()
                .filter(|&i| self.is_installed(&self.library[i].id))
                .collect(),
        );
        let mut rows: Vec<HomeRow> = [
            ("Continue playing", recent),
            ("Favorites", favorites),
            ("Installed", installed),
        ]
        .into_iter()
        .filter(|(_, games)| !games.is_empty())
        .map(|(title, games)| HomeRow { title, games })
        .collect();
        if rows.is_empty() && !shown.is_empty() {
            rows.push(HomeRow {
                title: "Your library",
                games: by_name(shown),
            });
        }
        let rows: Rc<[HomeRow]> = rows.into();
        *self.lists.couch_home.borrow_mut() = Some(rows.clone());
        rows
    }

    /// The game with the focus on the home page.
    pub fn home_focus(&self) -> Option<(usize, usize, &LibraryGame)> {
        let couch = self.couch.as_ref()?;
        let rows = self.home_rows();
        let row = couch.home_row.min(rows.len().checked_sub(1)?);
        let games = &rows[row].games;
        let col = couch
            .home_cols
            .get(row)
            .copied()
            .unwrap_or(0)
            .min(games.len().checked_sub(1)?);
        Some((row, col, &self.library[games[col]]))
    }

    /// Lists made again may have moved the games the focus was on: it finds them where they went,
    /// and keeps its place where its game left. Run after each message, so that the focus is right
    /// before the next view and the next press; not while a game's page shows, which draws no
    /// list: the games last seen are found once it closes, a favorite taken out and put back on
    /// the page included.
    pub(crate) fn refocus_couch(&mut self) {
        if self.selected.is_some() {
            return;
        }
        match self.couch.as_ref().map(|c| c.tab) {
            Some(Tab::Home) => self.refocus_home(),
            Some(Tab::Library) => self.refocus_library(),
            _ => {}
        }
    }

    /// Home's part of `refocus_couch`. Each row's focus goes with its game; the row with the focus
    /// goes with its game too when it is gone (its last favorite taken out), else keeps its place.
    fn refocus_home(&mut self) {
        let rows = self.home_rows();
        let library = &self.library;
        let Some(couch) = &mut self.couch else {
            return;
        };
        let at = |row: &HomeRow, col: usize| game_at(library, row, col);
        let place = |row: &HomeRow, id: &str| row.games.iter().position(|&i| library[i].id == id);
        let unchanged = couch.home_games.len() == rows.len()
            && rows
                .iter()
                .zip(&couch.home_games)
                .enumerate()
                .all(|(r, (row, seen))| {
                    let col = couch.home_cols.get(r).copied().unwrap_or(0);
                    row.title == seen.0 && at(row, col) == seen.1
                });
        if unchanged {
            return;
        }
        let seen = std::mem::take(&mut couch.home_games);
        if couch.home_chosen
            && let Some((title, id)) = seen.get(couch.home_row).cloned()
        {
            let mut cols: Vec<usize> = rows
                .iter()
                .map(|row| match seen.iter().position(|(t, _)| *t == row.title) {
                    Some(k) => place(row, &seen[k].1)
                        .unwrap_or_else(|| couch.home_cols.get(k).copied().unwrap_or(0)),
                    None => 0,
                })
                .collect();
            couch.home_row = match rows.iter().position(|row| row.title == title) {
                Some(r) => r,
                None => match rows.iter().position(|row| place(row, &id).is_some()) {
                    Some(r) => {
                        cols[r] = place(&rows[r], &id).unwrap_or(0);
                        r
                    }
                    None => couch.home_row.min(rows.len().saturating_sub(1)),
                },
            };
            couch.home_cols = cols;
        }
        if couch.home_cols.len() < rows.len() {
            couch.home_cols.resize(rows.len(), 0);
        }
        couch.home_games = rows
            .iter()
            .zip(&couch.home_cols)
            .map(|(row, &col)| (row.title, at(row, col).to_owned()))
            .collect();
    }

    /// The library's part of `refocus_couch`: once the player moved or used it, the focus goes with
    /// its game, and keeps its place where its game left the shelf.
    fn refocus_library(&mut self) {
        let games = self.couch_games();
        let (columns, rows) = (self.couch_columns(), self.couch_rows());
        let library = &self.library;
        let Some(couch) = &mut self.couch else {
            return;
        };
        let Some(last) = games.len().checked_sub(1) else {
            return;
        };
        let at = |card: usize| &library[games[card.min(last)]].id;
        if couch.card_chosen
            && let Some(id) = &couch.card_game
            && at(couch.card) != id
            && let Some(k) = games.iter().position(|&i| library[i].id == *id)
        {
            couch.card = k;
            // Its row in view, where a move would have left the rows shown.
            let row = k / columns;
            couch.top = couch.top.min(row).max((row + 1).saturating_sub(rows));
        }
        if couch.card_game.as_ref() != Some(at(couch.card)) {
            couch.card_game = Some(at(couch.card).clone());
        }
    }

    /// The buttons of a game's page: its main action, favorite, achievements.
    pub fn game_buttons(
        &self,
        g: &LibraryGame,
    ) -> Vec<(crate::icons::Icon, String, Option<Message>)> {
        use crate::icons::Icon;
        let main = self.main_action(g);
        // What opens a drawer in the desktop window happens here at once, with the defaults.
        let message = match main.message {
            Some(Message::OpenPanel(Panel::Install)) => {
                Some(Message::Couch(CouchMsg::Install(g.id.clone())))
            }
            Some(Message::ShowPage(Page::Downloads)) => {
                Some(Message::Couch(CouchMsg::Tab(Tab::Downloads)))
            }
            other => other,
        };
        let favorite = self.favorites.contains(&g.id);
        let mut buttons = vec![
            (main.icon, main.label, message),
            (
                if favorite {
                    Icon::HeartFilled
                } else {
                    Icon::Heart
                },
                if favorite {
                    "Favorite"
                } else {
                    "Add to favorites"
                }
                .into(),
                Some(Message::ToggleFavorite(g.id.clone())),
            ),
        ];
        if self.achievement_list(&g.id).is_some_and(|a| !a.is_empty()) {
            buttons.push((
                Icon::Trophy,
                "Achievements".into(),
                Some(Message::Couch(CouchMsg::Achievements)),
            ));
        }
        buttons
    }

    /// A GOG game's achievements, once read.
    pub fn achievement_list(
        &self,
        game_id: &str,
    ) -> Option<&[slatty_core::achievements::Achievement]> {
        match self.achievements.get(game_id)? {
            crate::Loadable::Ready(list) => Some(list),
            _ => None,
        }
    }

    /// What the Downloads tab lists: the download running, those stopped, then the queue.
    pub fn download_rows(&self) -> Vec<DownloadRow> {
        let mut rows = Vec::new();
        let percent = |done: u64, total: u64| {
            if total == 0 {
                0.0
            } else {
                done as f32 / total as f32
            }
        };
        // A download the launcher pauses takes a moment to stop: said at once, with nothing to
        // press meanwhile.
        let running = |entry: &str, p: Progress, pause: Message| match self
            .preempted
            .as_ref()
            .filter(|(e, _)| e == entry)
        {
            Some((_, Preempt::StartNow)) => ("Pausing…".to_string(), Vec::new()),
            Some((_, Preempt::Play)) => ("Pausing while a game runs…".to_string(), Vec::new()),
            None => (
                format!(
                    "Downloading {:.0}%",
                    percent(p.bytes_done, p.bytes_total) * 100.0
                ),
                vec![("Pause", pause)],
            ),
        };
        if let Some((id, title, p)) = self.installing() {
            let (status, buttons) =
                running(id, p, Message::Install(InstallMsg::Pause(id.to_string())));
            rows.push(DownloadRow {
                game_id: id.to_string(),
                title: title.to_string(),
                status,
                progress: Some(percent(p.bytes_done, p.bytes_total)),
                buttons,
            });
        }
        if let Some(run) = &self.offline_run {
            let p = run.progress;
            let (status, buttons) = running(
                &crate::offline::entry(&run.game_id),
                p,
                Message::Offline(OfflineMsg::Pause(run.game_id.clone())),
            );
            rows.push(DownloadRow {
                game_id: run.game_id.clone(),
                title: format!("{} · offline installers", self.title_of(&run.game_id)),
                status,
                progress: Some(percent(p.bytes_done, p.bytes_total)),
                buttons,
            });
        }
        for (id, kind) in &self.interrupted {
            let status = match kind {
                Interrupted::Paused => "Paused",
                Interrupted::Failed => "Failed",
                Interrupted::Download => "Interrupted",
                _ => continue,
            };
            rows.push(DownloadRow {
                game_id: id.clone(),
                title: self.title_of(id),
                status: status.into(),
                progress: None,
                buttons: vec![
                    ("Resume", Message::Couch(CouchMsg::Install(id.clone()))),
                    ("Discard", Message::Install(InstallMsg::Discard(id.clone()))),
                ],
            });
        }
        for (id, failed) in &self.offline_stopped {
            rows.push(DownloadRow {
                game_id: id.clone(),
                title: format!("{} · offline installers", self.title_of(id)),
                status: if failed.is_some() { "Failed" } else { "Paused" }.into(),
                progress: None,
                buttons: vec![
                    ("Resume", Message::Offline(OfflineMsg::Resume(id.clone()))),
                    ("Remove", Message::Offline(OfflineMsg::Remove(id.clone()))),
                ],
            });
        }
        // While the download running stops for Download now, the row asked for is starting, and
        // no other is asked for meanwhile.
        let starting = matches!(self.preempted, Some((_, Preempt::StartNow)));
        for (place, entry) in self.queue.iter().enumerate() {
            let (id, title) = match of_entry(entry) {
                Some(id) => (
                    id.to_string(),
                    format!("{} · offline installers", self.title_of(id)),
                ),
                None => (entry.clone(), self.title_of(entry)),
            };
            let size = match self.install_views.get(entry) {
                Some(InstallView::Queued(info) | InstallView::Ready(info)) => {
                    format!(
                        " · {}",
                        crate::ui::format::human_size(info.total_download())
                    )
                }
                _ => String::new(),
            };
            let remove = (
                "Remove",
                Message::Downloads(DownloadsMsg::Remove(entry.clone())),
            );
            let asked = self.starting_now.as_ref() == Some(entry);
            let (status, buttons) = match (starting, asked) {
                (true, true) => ("Starting…".to_string(), Vec::new()),
                (true, false) => (format!("Queued, {} in line{size}", place + 1), vec![remove]),
                (false, _) => (
                    format!("Queued, {} in line{size}", place + 1),
                    vec![
                        (
                            "Download now",
                            Message::Downloads(DownloadsMsg::StartNow(entry.clone())),
                        ),
                        remove,
                    ],
                ),
            };
            rows.push(DownloadRow {
                game_id: id,
                title,
                status,
                progress: None,
                buttons,
            });
        }
        rows
    }

    /// The window's size at the screen's own scale.
    pub fn screen(&self) -> Size {
        Size::new(
            self.window.width * self.ui_scale,
            self.window.height * self.ui_scale,
        )
    }

    /// Lays the interface out for a window of `screen`'s size: the couch mode, made for 1920×1080,
    /// is enlarged to fill a larger screen (a 4K television at scale 1 shows it as a 1080p one
    /// does), drawn at the screen's pixels; smaller ones, a Steam Deck's, show it as it is.
    pub fn fit_screen(&mut self, screen: Size) {
        self.ui_scale = match self.couch {
            Some(_) => (screen.width / 1920.0).min(screen.height / 1080.0).max(1.0),
            None => 1.0,
        };
        self.window = Size::new(screen.width / self.ui_scale, screen.height / self.ui_scale);
    }

    /// The width a cover is drawn at, inside its focus ring.
    pub fn couch_cover_width(&self) -> f32 {
        self.couch_card() - 2.0 * RING
    }

    /// The first row of covers the library shows: the one kept, moved so that the row with the
    /// focus shows, whatever changed since the last move (the window grew, the shelf has fewer
    /// games).
    pub(crate) fn library_top(&self, couch: &Couch, focused: Option<usize>) -> usize {
        let row = focused.unwrap_or(0) / self.couch_columns();
        couch
            .top
            .min(row)
            .max((row + 1).saturating_sub(self.couch_rows()))
    }

    /// The games whose covers the page shown draws, as places in `library`.
    pub fn couch_in_view(&self) -> Vec<usize> {
        let Some(couch) = self.couch.as_ref().filter(|_| self.selected.is_none()) else {
            return Vec::new();
        };
        let across = self.couch_columns();
        match couch.tab {
            Tab::Home => {
                let Some((row, col, _)) = self.home_focus() else {
                    return Vec::new();
                };
                let (_, shown, _) = self.home_layout();
                let rows = self.home_rows();
                let mut games = Vec::new();
                for (r, home_row) in rows.iter().enumerate().skip(row).take(shown) {
                    let len = home_row.games.len();
                    let at = match r == row {
                        true => col,
                        false => couch.home_cols.get(r).copied().unwrap_or(0).min(len - 1),
                    };
                    let start = home_start(at, across, len);
                    games.extend_from_slice(&home_row.games[start..(start + across).min(len)]);
                }
                games
            }
            Tab::Library => {
                let games = self.couch_games();
                let focused = (couch.on_shelves.is_none() && !games.is_empty())
                    .then(|| couch.card.min(games.len() - 1));
                let top = self.library_top(couch, focused);
                let first = (top * across).min(games.len());
                let last = ((top + self.couch_rows()) * across).min(games.len());
                games[first..last].to_vec()
            }
            Tab::Downloads => Vec::new(),
        }
    }

    /// Width of a cover: about eight across the window, the row filling it.
    pub fn couch_card(&self) -> f32 {
        let columns = self.couch_columns() as f32;
        (self.window.width - 2.0 * MARGIN - (columns - 1.0) * GAP) / columns
    }

    /// Covers across the window.
    pub fn couch_columns(&self) -> usize {
        let wanted = (self.window.width / 8.0).clamp(140.0, 280.0);
        (((self.window.width - 2.0 * MARGIN + GAP) / (wanted + GAP)).floor() as usize).max(1)
    }

    /// Rows of covers the library shows at once.
    pub fn couch_rows(&self) -> usize {
        let row = self.couch_card() * 4.0 / 3.0 + GAP;
        let room = self.window.height - TOP_BAR - HINTS - SHELVES - self.notice_room();
        (((room + GAP) / row).floor() as usize).max(1)
    }

    /// The room a notice takes above the pages while it shows.
    pub fn notice_room(&self) -> f32 {
        if self.notice.is_some() { NOTICE } else { 0.0 }
    }
}

/// The first cover a row of Home shows, of `len`, `across` at once: the one with the focus as far
/// right as one from the end, the row full when it can be.
pub(crate) fn home_start(at: usize, across: usize, len: usize) -> usize {
    at.saturating_sub(across.saturating_sub(2))
        .min(len.saturating_sub(across))
}

/// The id of the game at `col` in `row`, the last one past its end; empty for an empty row.
fn game_at<'a>(library: &'a [LibraryGame], row: &HomeRow, col: usize) -> &'a str {
    let last = row.games.len().saturating_sub(1);
    row.games
        .get(col.min(last))
        .map_or("", |&i| library[i].id.as_str())
}

fn fullscreen(on: bool) -> Task<Message> {
    let mode = if on {
        iced::window::Mode::Fullscreen
    } else {
        iced::window::Mode::Windowed
    };
    iced::window::oldest().and_then(move |id| iced::window::set_mode(id, mode))
}
