//! Changing how a game looks in the library: its title, the title it is sorted by, its cover and
//! its background. Changes stay in a draft until saved.

use std::path::PathBuf;

use iced::widget::image;
use iced::{Point, Task};
use slatty_core::custom::{self, ImageChange};
use slatty_core::library::LibraryGame;

use crate::{App, Message};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Art {
    Cover,
    Background,
}

/// The edit dialog's state, applied only on Save.
pub struct EditDraft {
    pub game_id: String,
    pub title: String,
    pub sort_title: String,
    pub cover: ImageChange,
    pub background: ImageChange,
}

/// The context menu of a cover, opened by a right click where the pointer was.
#[derive(Debug, Clone, PartialEq)]
pub struct ContextMenu {
    pub game_id: String,
    pub at: Point,
}

pub const MENU_WIDTH: f32 = 190.0;
const MENU_HEIGHT: f32 = 52.0;

#[derive(Debug, Clone)]
pub enum EditMsg {
    /// A cover was right-clicked; the position follows in `At`.
    Menu(String),
    /// Where a right click happened.
    At(Point),
    CloseMenu,
    Open(String),
    Title(String),
    SortTitle(String),
    Pick(Art),
    Picked(Art, Option<PathBuf>),
    /// Back to GOG's title and images (saved with Save).
    Reset,
    Cancel,
    Save,
}

impl App {
    pub fn update_edit(&mut self, msg: EditMsg) -> Task<Message> {
        match msg {
            EditMsg::Menu(game_id) => self.menu_for = Some(game_id),
            EditMsg::At(at) => {
                let window = self.window;
                // A right click anywhere else closes the menu. Near an edge the menu opens inward.
                self.context_menu = self.menu_for.take().map(|game_id| ContextMenu {
                    game_id,
                    at: Point::new(
                        at.x.min(window.width - MENU_WIDTH - 8.0).max(0.0),
                        at.y.min(window.height - MENU_HEIGHT - 8.0).max(0.0),
                    ),
                });
            }
            EditMsg::CloseMenu => self.context_menu = None,
            EditMsg::Open(game_id) => {
                self.context_menu = None;
                self.panel = None;
                let custom = self.customs.get(&game_id).cloned().unwrap_or_default();
                self.edit = Some(EditDraft {
                    title: self.title_of(&game_id),
                    sort_title: custom.sort_title.unwrap_or_default(),
                    cover: ImageChange::Keep,
                    background: ImageChange::Keep,
                    game_id: game_id.clone(),
                });
                let background = self
                    .library
                    .iter()
                    .find(|g| g.id == game_id)
                    .and_then(|g| g.background.clone());
                return self.request_images(background.into_iter().collect());
            }
            EditMsg::Title(v) => {
                if let Some(d) = &mut self.edit {
                    d.title = v;
                }
            }
            EditMsg::SortTitle(v) => {
                if let Some(d) = &mut self.edit {
                    d.sort_title = v;
                }
            }
            EditMsg::Pick(art) => {
                return Task::perform(
                    async move {
                        rfd::AsyncFileDialog::new()
                            .set_title(match art {
                                Art::Cover => "Choose a cover",
                                Art::Background => "Choose a background",
                            })
                            .add_filter("Images", &["png", "jpg", "jpeg", "webp", "gif", "bmp"])
                            .pick_file()
                            .await
                            .map(|f| f.path().to_path_buf())
                    },
                    move |path| Message::Edit(EditMsg::Picked(art, path)),
                );
            }
            EditMsg::Picked(art, Some(path)) => {
                if !custom::is_image(&path) {
                    self.notify_error(format!("{} is not an image.", path.display()));
                } else if let Some(d) = &mut self.edit {
                    *d.image_mut(art) = ImageChange::Set(path);
                }
            }
            EditMsg::Picked(_, None) => {}
            EditMsg::Reset => {
                if let Some(d) = &mut self.edit {
                    d.title = self.gog_titles.get(&d.game_id).cloned().unwrap_or_default();
                    d.sort_title.clear();
                    d.cover = ImageChange::Reset;
                    d.background = ImageChange::Reset;
                }
            }
            EditMsg::Cancel => self.edit = None,
            EditMsg::Save => self.save_edit(),
        }
        Task::none()
    }

    fn save_edit(&mut self) {
        let (Some(core), Some(d)) = (&self.core, &self.edit) else {
            return;
        };
        let gog = self.gog_titles.get(&d.game_id).map_or("", String::as_str);
        // A title left as GOG's is not stored, so a later rename on GOG still shows.
        let title = if d.title.trim() == gog {
            ""
        } else {
            d.title.as_str()
        };
        let sort_title = if d.sort_title.trim() == d.title.trim() {
            ""
        } else {
            d.sort_title.as_str()
        };
        match custom::save(
            &core.db,
            &core.dirs,
            &d.game_id,
            title,
            sort_title,
            d.cover.clone(),
            d.background.clone(),
        ) {
            Ok(saved) => {
                let game_id = d.game_id.clone();
                if saved == custom::Custom::default() {
                    self.customs.remove(&game_id);
                } else {
                    self.customs.insert(game_id, saved);
                }
                self.edit = None;
                self.apply_customs();
            }
            Err(e) => self.notify_error(e.to_string()),
        }
    }

    /// Library titles as the user named them, GOG's otherwise.
    pub fn apply_customs(&mut self) {
        for g in &mut self.library {
            if let Some(title) = self
                .customs
                .get(&g.id)
                .and_then(|c| c.title.clone())
                .or_else(|| self.gog_titles.get(&g.id).cloned())
            {
                g.title = title;
            }
        }
    }

    /// The cover shown for a game: the user's, else GOG's once downloaded.
    pub fn cover(&self, game_id: &str) -> Option<image::Handle> {
        match self.customs.get(game_id).and_then(|c| c.cover.as_ref()) {
            Some(path) => Some(image::Handle::from_path(path)),
            None => self.covers.get(game_id).cloned(),
        }
    }

    /// The key art of the game page: the user's, else GOG's once downloaded.
    pub fn background(&self, g: &LibraryGame) -> Option<image::Handle> {
        match self.customs.get(&g.id).and_then(|c| c.background.as_ref()) {
            Some(path) => Some(image::Handle::from_path(path)),
            None => g
                .background
                .as_ref()
                .and_then(|u| self.images.get(u))
                .cloned(),
        }
    }

    /// What the library is sorted by when sorting by name.
    pub fn sort_name(&self, g: &LibraryGame) -> String {
        self.customs
            .get(&g.id)
            .and_then(|c| c.sort_title.as_deref())
            .unwrap_or(&g.title)
            .to_lowercase()
    }
}

impl EditDraft {
    fn image_mut(&mut self, art: Art) -> &mut ImageChange {
        match art {
            Art::Cover => &mut self.cover,
            Art::Background => &mut self.background,
        }
    }
}
