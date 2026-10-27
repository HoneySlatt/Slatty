//! The list of every game beside the library, as Steam's: its own search, closed and opened at will
//! (the choice is kept), a game opened with a click. Only the rows in view are built, and only
//! their icons asked for.

use std::rc::Rc;

use iced::Task;
use iced::widget::scrollable::Viewport;

use crate::library::{Filters, GridWindow, Shelf, Sort};
use crate::{App, Message, Page};

/// Width of the open list, and height of a game's row in it.
pub const WIDTH: f32 = 270.0;
pub const ROW: f32 = 30.0;

#[derive(Debug, Default)]
pub struct Sidebar {
    pub open: bool,
    pub search: String,
    /// Where the list is scrolled to and how high it shows.
    pub view: Option<Viewport>,
    /// Whether the list was open when the cover grid last told its size: opened or closed since,
    /// the grid's width is another.
    pub open_when_measured: bool,
}

#[derive(Debug, Clone)]
pub enum SidebarMsg {
    Toggle,
    Search(String),
    Scrolled(Viewport),
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
        }
        Task::none()
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
            true => WIDTH,
            false => 0.0,
        }
    }

    /// The cover grid's last size and scroll, its width only when the list was as it is now.
    pub fn grid_measured(&self) -> (Option<f32>, Option<Viewport>) {
        let view = self.grid_view;
        let width = view
            .filter(|_| self.sidebar.open_when_measured == self.sidebar.open)
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
        GridWindow::new(count, WIDTH, WIDTH, 0.0, Some(ROW), offset, height)
    }

    /// Asks for the icons of the rows in view not shown yet: a library of thousands of games asks
    /// only for those scrolled to. After every message, as the list may have moved or changed.
    pub fn sidebar_icons(&mut self) -> Task<Message> {
        if !self.sidebar_shown() {
            return Task::none();
        }
        let games = self.sidebar_games();
        let window = self.sidebar_window(games.len());
        let wanted: Vec<String> = window
            .shown
            .filter_map(|row| games.get(row))
            .filter_map(|&i| self.library[i].icon.clone())
            .filter(|icon| !self.images.contains_key(icon) && !self.images_requested.contains(icon))
            .collect();
        if wanted.is_empty() {
            return Task::none();
        }
        // A file of Steam's client is shown as it is, never downloaded: one gone is not looked
        // for again after every message.
        self.images_requested
            .extend(wanted.iter().filter(|w| w.starts_with('/')).cloned());
        self.request_images(wanted)
    }
}
