//! The list of every game beside the library, as Steam's: its own search, closed and opened at will
//! and its edge dragged to widen it (both kept), a game opened with a click. Only the rows in view
//! are built, and only their icons asked for.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use iced::widget::image;
use iced::widget::scrollable::Viewport;
use iced::{Event, Subscription, Task, mouse};
use slatty_core::custom::ImageChange;
use slatty_core::library::LibraryGame;

use crate::library::{Filters, GridWindow, Shelf, Sort};
use crate::{App, Message, Page};

/// Width of the open list until dragged, the widths it can be dragged to, height of a game's row in
/// it and side of its icon.
pub const WIDTH: f32 = 270.0;
pub const WIDTHS: (f32, f32) = (180.0, 600.0);
pub const ROW: f32 = 30.0;
pub const ICON: f32 = 20.0;

#[derive(Debug)]
pub struct Sidebar {
    pub open: bool,
    pub width: f32,
    /// Whether its edge is being dragged.
    pub dragging: bool,
    pub search: String,
    /// Where the list is scrolled to and how high it shows.
    pub view: Option<Viewport>,
    /// The room the list took when the cover grid last told its size: opened, closed or dragged
    /// since, the grid's width is another.
    pub room_when_measured: f32,
    /// The images whose download failed: an icon among them gives way to the next one, rather than
    /// be asked again after every message.
    pub failed: HashSet<String>,
}

impl Default for Sidebar {
    fn default() -> Self {
        Sidebar {
            open: false,
            width: WIDTH,
            dragging: false,
            search: String::new(),
            view: None,
            room_when_measured: 0.0,
            failed: HashSet::new(),
        }
    }
}

/// Where a game's icon stands.
enum IconStep {
    Shown(image::Handle),
    /// Being downloaded or made.
    Waiting,
    Download(String),
    /// Its copy to make from this file, so many pixels square, and whether the file must be square.
    Make(PathBuf, u32, bool),
    None,
}

#[derive(Debug, Clone)]
pub enum SidebarMsg {
    Toggle,
    Search(String),
    Scrolled(Viewport),
    /// Its edge pressed, moved to the window's `x`, let go.
    Grab,
    Drag(f32),
    Drop,
}

impl App {
    pub fn update_sidebar(&mut self, msg: SidebarMsg) -> Task<Message> {
        match msg {
            SidebarMsg::Toggle => {
                self.sidebar.open = !self.sidebar.open;
                if let Some(core) = &self.core
                    && let Err(e) = slatty_core::settings::set_sidebar(&core.db, self.sidebar.open)
                {
                    self.notify_error(e.to_string());
                }
            }
            SidebarMsg::Search(search) => {
                self.sidebar.search = search;
                self.sidebar.view = None;
            }
            SidebarMsg::Scrolled(view) => self.sidebar.view = Some(view),
            SidebarMsg::Grab => self.sidebar.dragging = true,
            // The list starts at the window's left edge: its edge follows the pointer.
            SidebarMsg::Drag(x) if self.sidebar.dragging => {
                self.sidebar.width = x.clamp(WIDTHS.0, WIDTHS.1);
            }
            SidebarMsg::Drag(_) => {}
            SidebarMsg::Drop => {
                self.sidebar.dragging = false;
                if let Some(core) = &self.core
                    && let Err(e) =
                        slatty_core::settings::set_sidebar_width(&core.db, self.sidebar.width)
                {
                    self.notify_error(e.to_string());
                }
            }
        }
        Task::none()
    }

    /// The pointer's moves and its button let go, while the list's edge is dragged: they happen
    /// anywhere in the window, beyond the edge itself.
    pub fn sidebar_drag(&self) -> Subscription<Message> {
        if !self.sidebar.dragging {
            return Subscription::none();
        }
        iced::event::listen_with(|event, _, _| match event {
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                Some(Message::Sidebar(SidebarMsg::Drag(position.x)))
            }
            // Let go out of the window, where its release is not told.
            Event::Mouse(
                mouse::Event::ButtonReleased(mouse::Button::Left) | mouse::Event::CursorLeft,
            ) => Some(Message::Sidebar(SidebarMsg::Drop)),
            _ => None,
        })
    }

    /// Whether the list shows: open, on the library's page.
    pub fn sidebar_shown(&self) -> bool {
        self.sidebar.open && self.on_library_page()
    }

    /// Whether the list has its place: beside the library's page, open or not.
    pub fn on_library_page(&self) -> bool {
        self.page == Page::Library && self.selected.is_none() && self.couch.is_none()
    }

    /// The width the list takes beside the library: none once closed.
    pub fn sidebar_room(&self) -> f32 {
        match self.sidebar_shown() {
            true => self.sidebar.width,
            false => 0.0,
        }
    }

    /// The cover grid's last size and scroll, its width only when the list was as it is now.
    pub fn grid_measured(&self) -> (Option<f32>, Option<Viewport>) {
        let view = self.grid_view;
        let width = view
            .filter(|_| self.sidebar.room_when_measured == self.sidebar_room())
            .map(|v| v.bounds().width);
        (width, view)
    }

    /// The games the list shows, as places in `library`: every one not hidden, A to Z, those
    /// whose title holds what it is searched for. Kept until a message that can change it.
    pub fn sidebar_games(&self) -> Rc<[usize]> {
        if let Some((search, games)) = self.lists.sidebar.borrow().as_ref()
            && *search == self.sidebar.search
        {
            return games.clone();
        }
        let games: Rc<[usize]> = self
            .find_games(
                self.sidebar.search.trim(),
                Shelf::All,
                Filters::default(),
                Sort::NameAsc,
            )
            .into();
        *self.lists.sidebar.borrow_mut() = Some((self.sidebar.search.clone(), games.clone()));
        games
    }

    /// The rows of the list in view: from its last scroll, else from the window.
    pub fn sidebar_window(&self, count: usize) -> GridWindow {
        let view = self.sidebar.view;
        let height = view.map_or(self.window.height, |v| v.bounds().height);
        let offset = view.map_or(0.0, |v| v.absolute_offset().y);
        let width = self.sidebar.width;
        GridWindow::new(count, width, width, 0.0, Some(ROW), offset, height)
    }

    /// Asks for the icons in view not shown yet: those of the list's rows (a library of thousands
    /// of games asks only for those scrolled to), and the one Edit game shows. After every
    /// message, as the list may have moved or changed.
    pub fn wanted_icons(&mut self) -> Task<Message> {
        let mut steps = Vec::new();
        if self.sidebar_shown()
            && let Some(pixels) = self.icon_pixels(ICON)
        {
            let games = self.sidebar_games();
            let window = self.sidebar_window(games.len());
            steps.extend(
                window
                    .shown
                    .filter_map(|row| games.get(row))
                    .map(|&i| self.icon_step(&self.library[i], pixels, true)),
            );
        }
        if let Some(d) = &self.edit
            && let Some(game) = self.library.iter().find(|g| g.id == d.game_id)
            && let Some(pixels) = self.icon_pixels(crate::edit::ICON_PICTURE)
        {
            match d.icon {
                ImageChange::Keep => steps.push(self.icon_step(game, pixels, true)),
                ImageChange::Reset => steps.push(self.icon_step(game, pixels, false)),
                _ => {}
            }
        }
        let (mut downloads, mut copies) = (Vec::new(), Vec::new());
        for step in steps {
            match step {
                IconStep::Download(url) => downloads.push(url),
                IconStep::Make(file, pixels, square) => copies.push((file, pixels, square)),
                _ => {}
            }
        }
        let downloads = match downloads.is_empty() {
            true => Task::none(),
            false => self.request_images(downloads),
        };
        Task::batch([downloads, self.scale_icons(copies)])
    }

    /// A game's icon drawn `side` wide, once its copy is made: the one chosen in Edit game (with
    /// `chosen`), else its store's, else, for a game shown as GOG's, its Steam twin's.
    pub fn icon_at(&self, g: &LibraryGame, side: f32, chosen: bool) -> Option<image::Handle> {
        match self.icon_step(g, self.icon_pixels(side)?, chosen) {
            IconStep::Shown(handle) => Some(handle),
            _ => None,
        }
    }

    /// The pixels of a side `side` long in the window's units; none until its scale is known.
    fn icon_pixels(&self, side: f32) -> Option<u32> {
        Some((side * self.scaled.scale? * self.ui_scale).round() as u32)
    }

    /// The first of a game's icons that is shown, or what it waits for; one that cannot be read,
    /// downloaded or is no icon gives way to the next.
    fn icon_step(&self, g: &LibraryGame, pixels: u32, chosen: bool) -> IconStep {
        let custom = self
            .customs
            .get(&g.id)
            .and_then(|c| c.icon.as_deref())
            .filter(|_| chosen)
            .map(|path| Ok((path.to_path_buf(), false)));
        let stores = [
            g.icon.as_deref(),
            self.steam.twin_icons.get(&g.id).map(String::as_str),
        ];
        let stores = stores.into_iter().flatten().map(|key| self.icon_file(key));
        for file in custom.into_iter().chain(stores) {
            let (file, square) = match file {
                Ok(file) => file,
                Err(Some(step)) => return step,
                Err(None) => continue,
            };
            let key = (file, pixels);
            match self.scaled.icons.get(&key) {
                Some(Some(handle)) => return IconStep::Shown(handle.clone()),
                Some(None) => continue,
                None if self.scaled.icons_pending.contains(&key) => return IconStep::Waiting,
                None => return IconStep::Make(key.0, pixels, square),
            }
        }
        IconStep::None
    }

    /// The file of a store's icon: a file of Steam's client as it is, else the one downloaded.
    fn icon_file(&self, key: &str) -> Result<(PathBuf, bool), Option<IconStep>> {
        if Path::new(key).is_absolute() {
            return Ok((PathBuf::from(key), true));
        }
        match self.images.get(key) {
            Some(image::Handle::Path(_, path)) => Ok((path.clone(), true)),
            _ if self.sidebar.failed.contains(key) => Err(None),
            _ if self.images_requested.contains(key) => Err(Some(IconStep::Waiting)),
            _ => Err(Some(IconStep::Download(key.to_string()))),
        }
    }
}
