//! Changing how a game looks in the library: its title, the title it is sorted by, its cover, its
//! background, and whether it is hidden. Pictures come from a file or, once turned on, from
//! SteamGridDB. Changes stay in a draft until saved.

use std::path::PathBuf;

use iced::widget::image;
use iced::{Point, Task};
use slatty_core::custom::{self, ImageChange};
use slatty_core::library::LibraryGame;
use slatty_core::steamgriddb;

use crate::{App, Loadable, Message, err};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Art {
    Cover,
    Background,
}

/// The edit dialog's state, applied only on Save.
pub struct EditDraft {
    pub game_id: String,
    pub title: String,
    pub sort_title: String,
    pub hidden: bool,
    pub cover: ImageChange,
    pub background: ImageChange,
    /// Pictures downloaded from SteamGridDB, ready to show: made once, not at every frame.
    pub previews: std::collections::HashMap<Art, image::Handle>,
    /// What SteamGridDB offers for one of the pictures, once asked.
    pub picker: Option<ArtPicker>,
}

pub struct ArtPicker {
    pub art: Art,
    /// The name to search, the game's title to begin with, and the name last searched.
    pub query: String,
    pub searched: String,
    pub games: Loadable<Vec<steamgriddb::Game>>,
    /// The game whose pictures are shown, and those pictures.
    pub game: Option<u64>,
    pub choices: Option<Loadable<Vec<steamgriddb::Art>>>,
    /// The picture being downloaded.
    pub fetching: Option<usize>,
}

/// The context menu of a cover, opened by a right click where the pointer was.
#[derive(Debug, Clone, PartialEq)]
pub struct ContextMenu {
    pub game_id: String,
    pub at: Point,
}

pub const MENU_WIDTH: f32 = 190.0;
/// The scrolling part of the edit form, brought down to the SteamGridDB search as it opens.
pub const FORM: &str = "edit-form";
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
    Hidden(bool),
    Pick(Art),
    Picked(Art, Option<PathBuf>),
    /// Opens the SteamGridDB search for this picture, with the game's title.
    Browse(Art),
    PickerQuery(String),
    Search,
    /// The games found for a name.
    Found(String, Result<Vec<steamgriddb::Game>, String>),
    /// A game found was chosen: its covers or backgrounds are listed.
    ChooseGame(u64),
    Offered(u64, Result<Vec<steamgriddb::Art>, String>),
    /// One of the pictures was chosen: it is downloaded, then shown in the draft.
    Choose(usize),
    Fetched(Art, Result<Vec<u8>, String>),
    ClosePicker,
    /// Back to GOG's title and images, and shown again (saved with Save).
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
                    hidden: custom.hidden,
                    cover: ImageChange::Keep,
                    background: ImageChange::Keep,
                    previews: Default::default(),
                    picker: None,
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
            EditMsg::Hidden(hidden) => {
                if let Some(d) = &mut self.edit {
                    d.hidden = hidden;
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
            EditMsg::Browse(art) => {
                let Some(d) = &self.edit else {
                    return Task::none();
                };
                let query = self.title_of(&d.game_id);
                if let Some(d) = &mut self.edit {
                    d.picker = Some(ArtPicker {
                        art,
                        query,
                        searched: String::new(),
                        games: Loadable::Loading,
                        game: None,
                        choices: None,
                        fetching: None,
                    });
                }
                return Task::batch([self.update_edit(EditMsg::Search), to_picker()]);
            }
            EditMsg::PickerQuery(query) => {
                if let Some(p) = self.picker_mut() {
                    p.query = query;
                }
            }
            EditMsg::Search => {
                let (Some(core), Some(p)) = (self.core.clone(), self.picker_mut()) else {
                    return Task::none();
                };
                let name = p.query.trim().to_string();
                if name.is_empty() {
                    return Task::none();
                }
                p.searched = name.clone();
                p.games = Loadable::Loading;
                p.game = None;
                p.choices = None;
                return Task::perform(
                    async move {
                        let key = key().await?;
                        steamgriddb::search(&core.http, &key, &name)
                            .await
                            .map_err(refused_key)
                            .map(|games| (name, games))
                    },
                    |r| match r {
                        Ok((name, games)) => Message::Edit(EditMsg::Found(name, Ok(games))),
                        Err(e) => Message::Edit(EditMsg::Found(String::new(), Err(e))),
                    },
                );
            }
            EditMsg::Found(name, result) => {
                let Some(p) = self.picker_mut() else {
                    return Task::none();
                };
                // An answer to an earlier search is too late.
                if result.is_ok() && name != p.searched {
                    return Task::none();
                }
                let first = result.as_ref().ok().and_then(|g| g.first()).map(|g| g.id);
                p.games = match result {
                    Ok(games) => Loadable::Ready(games),
                    Err(e) => Loadable::Failed(e),
                };
                if let Some(id) = first {
                    return self.update_edit(EditMsg::ChooseGame(id));
                }
            }
            EditMsg::ChooseGame(id) => {
                let (Some(core), Some(p)) = (self.core.clone(), self.picker_mut()) else {
                    return Task::none();
                };
                p.game = Some(id);
                p.choices = Some(Loadable::Loading);
                p.fetching = None;
                let kind = match p.art {
                    Art::Cover => steamgriddb::Kind::Cover,
                    Art::Background => steamgriddb::Kind::Background,
                };
                return Task::perform(
                    async move {
                        let key = key().await?;
                        steamgriddb::art(&core.http, &key, id, kind)
                            .await
                            .map_err(refused_key)
                    },
                    move |r| Message::Edit(EditMsg::Offered(id, r)),
                );
            }
            EditMsg::Offered(id, result) => {
                let Some(p) = self.picker_mut().filter(|p| p.game == Some(id)) else {
                    return Task::none();
                };
                let thumbs = match &result {
                    Ok(list) => list.iter().map(|a| a.thumb.clone()).collect(),
                    Err(_) => Vec::new(),
                };
                p.choices = Some(match result {
                    Ok(list) => Loadable::Ready(list),
                    Err(e) => Loadable::Failed(e),
                });
                return Task::batch([self.request_images(thumbs), to_picker()]);
            }
            EditMsg::Choose(i) => {
                let (Some(core), Some(p)) = (self.core.clone(), self.picker_mut()) else {
                    return Task::none();
                };
                let Some(Loadable::Ready(list)) = &p.choices else {
                    return Task::none();
                };
                let Some(url) = list.get(i).map(|a| a.url.clone()) else {
                    return Task::none();
                };
                p.fetching = Some(i);
                let art = p.art;
                return Task::perform(
                    async move { steamgriddb::download(&core.http, &url).await.map_err(err) },
                    move |r| Message::Edit(EditMsg::Fetched(art, r)),
                );
            }
            EditMsg::Fetched(art, result) => {
                let Some(d) = self
                    .edit
                    .as_mut()
                    .filter(|d| d.picker.as_ref().is_some_and(|p| p.art == art))
                else {
                    return Task::none();
                };
                match result {
                    Ok(bytes) if custom::is_image_data(&bytes) => {
                        d.previews
                            .insert(art, image::Handle::from_bytes(bytes.clone()));
                        *d.image_mut(art) = ImageChange::Downloaded(bytes);
                        d.picker = None;
                    }
                    Ok(_) => {
                        d.picker = None;
                        self.notify_error(
                            "SteamGridDB sent something that is not an image.".into(),
                        );
                    }
                    Err(e) => {
                        if let Some(p) = &mut d.picker {
                            p.fetching = None;
                        }
                        self.notify_error(e);
                    }
                }
            }
            EditMsg::ClosePicker => {
                if let Some(d) = &mut self.edit {
                    d.picker = None;
                }
            }
            EditMsg::Reset => {
                if let Some(d) = &mut self.edit {
                    d.title = self.gog_titles.get(&d.game_id).cloned().unwrap_or_default();
                    d.sort_title.clear();
                    d.hidden = false;
                    d.cover = ImageChange::Reset;
                    d.background = ImageChange::Reset;
                    d.picker = None;
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
        let changes = custom::Changes {
            title: title.to_string(),
            sort_title: sort_title.to_string(),
            hidden: d.hidden,
            cover: d.cover.clone(),
            background: d.background.clone(),
        };
        match custom::save(&core.db, &core.dirs, &d.game_id, changes) {
            Ok(saved) => {
                let game_id = d.game_id.clone();
                if saved == custom::Custom::default() {
                    self.customs.remove(&game_id);
                } else {
                    self.customs.insert(game_id, saved);
                }
                self.edit = None;
                self.apply_customs();
                // The last hidden game shown again: its shelf leaves the menu.
                if !self.shelves().contains(&self.shelf) {
                    self.shelf = crate::Shelf::All;
                }
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
    pub fn sort_name(&self, g: &LibraryGame) -> std::rc::Rc<crate::library::NaturalKey> {
        self.sort_keys.of(self
            .customs
            .get(&g.id)
            .and_then(|c| c.sort_title.as_deref())
            .unwrap_or(&g.title))
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

/// The SteamGridDB API key, from the keyring.
async fn key() -> Result<slatty_core::secret::Secret, String> {
    tokio::task::spawn_blocking(|| slatty_core::credentials::load_key(steamgriddb::KEY))
        .await
        .map_err(err)?
        .map_err(err)?
        .ok_or_else(|| "Add your SteamGridDB API key in Settings → Advanced.".to_string())
}

fn refused_key(e: slatty_core::Error) -> String {
    match e {
        slatty_core::Error::Http {
            status: 401 | 403, ..
        } => "SteamGridDB refused the API key: check it in Settings → Advanced.".into(),
        e => e.to_string(),
    }
}

impl App {
    fn picker_mut(&mut self) -> Option<&mut ArtPicker> {
        self.edit.as_mut().and_then(|d| d.picker.as_mut())
    }
}

/// Scrolls the edit form down to the SteamGridDB search, below the pictures.
fn to_picker() -> Task<Message> {
    iced::widget::operation::snap_to_end(FORM)
}
