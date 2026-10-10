//! The dialog (library) and the drawer (game page) that change a game's title, sorting title,
//! cover and background, or hide it.

use crate::theme::text;
use iced::widget::{
    Space, button, center, column, container, image, mouse_area, opaque, pin, row, scrollable,
    space, stack, text_input, toggler,
};
use iced::{Alignment, ContentFit, Element, Length, Padding};
use slatty_core::custom::ImageChange;

use super::note;
use super::panels::DRAWER_WIDTH;
use crate::edit::{Art, ArtPicker, ContextMenu, EditDraft, EditMsg, FORM, MENU_WIDTH};
use crate::icons::{Icon, icon};
use crate::theme::{self, bold, tokens};
use crate::{App, Loadable, Message};

/// The parts of the edit form, laid out differently by the dialog and the drawer.
struct Form<'a> {
    fields: Element<'a, Message>,
    cover: Element<'a, Message>,
    background: Element<'a, Message>,
    /// What SteamGridDB offers for one of the pictures, once asked.
    picker: Option<Element<'a, Message>>,
    actions: Element<'a, Message>,
}

impl App {
    /// Over the library, opened from a cover.
    pub(super) fn edit_dialog<'a>(
        &'a self,
        page: Element<'a, Message>,
        d: &'a EditDraft,
    ) -> Element<'a, Message> {
        let gog_title = self.gog_titles.get(&d.game_id).map_or("", String::as_str);
        let close = button(container(icon(Icon::X, 20.0, tokens().text)).center(24))
            .padding(10)
            .on_press(Message::Edit(EditMsg::Cancel))
            .style(theme::tonal);
        let header = row![
            column![
                text("Edit game").size(26).font(bold()),
                text(gog_title).size(14).color(tokens().muted),
            ]
            .spacing(4)
            .width(Length::Fill),
            close,
        ];
        let form = self.edit_form(
            d,
            (PICTURE_HEIGHT * 3.0 / 4.0, PICTURE_HEIGHT),
            (PICTURE_HEIGHT * 16.0 / 9.0, PICTURE_HEIGHT),
        );
        // The form scrolls between its title and its buttons once SteamGridDB's choices make it
        // taller than the window: it has the room the margins, padding, title and buttons leave.
        const AROUND: f32 = 48.0 + 52.0 + 80.0 + 52.0 + 40.0;
        let body = container(
            scrollable(
                column![form.fields, row![form.cover, form.background].spacing(24)]
                    .push(form.picker)
                    .push(note("Click a picture to choose a file."))
                    .spacing(20)
                    .padding(Padding::ZERO.right(14)),
            )
            .id(FORM)
            .style(theme::scroller),
        )
        .max_height((self.window.height - AROUND).max(160.0));
        let dialog = container(column![header, body, form.actions].spacing(20))
            .padding(Padding::new(26.0).right(12.0))
            .max_width(720)
            .style(theme::card);
        stack![
            page,
            mouse_area(
                container(Space::new())
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .style(theme::backdrop)
            )
            .on_press(Message::Edit(EditMsg::Cancel)),
            center(opaque(dialog)),
        ]
        .into()
    }

    /// Beside the game page, opened from its key art.
    pub(super) fn edit_drawer<'a>(
        &'a self,
        page: Element<'a, Message>,
        d: &'a EditDraft,
    ) -> Element<'a, Message> {
        let gog_title = self.gog_titles.get(&d.game_id).map_or("", String::as_str);
        let subtitle = text(gog_title).size(18).color(tokens().muted);
        let form = self.edit_form(
            d,
            (DRAWER_PICTURE * 0.3, DRAWER_PICTURE * 0.4),
            (DRAWER_PICTURE, DRAWER_PICTURE * 9.0 / 16.0),
        );
        let body = scrollable(
            column![form.fields, form.cover, form.background]
                .push(form.picker)
                .push(note("Click a picture to choose a file."))
                .spacing(20)
                .padding(Padding::ZERO.right(14)),
        )
        .id(FORM)
        .style(theme::scroller)
        .height(Length::Fill);
        self.drawer(
            page,
            "Edit game",
            subtitle.into(),
            body.into(),
            Some(form.actions),
        )
    }

    /// Cover and background pictures are `(width, height)`.
    fn edit_form<'a>(
        &'a self,
        d: &'a EditDraft,
        cover: (f32, f32),
        background: (f32, f32),
    ) -> Form<'a> {
        let gog_title = self.gog_titles.get(&d.game_id).map_or("", String::as_str);
        let msg = |m| Message::Edit(m);
        let label = |t| text(t).size(14).color(tokens().muted);
        let fields = column![
            label("Title"),
            text_input(gog_title, &d.title)
                .on_input(|v| Message::Edit(EditMsg::Title(v)))
                .style(theme::field)
                .font(theme::font())
                .padding([10, 14]),
            label("Sorting title"),
            text_input(&d.title, &d.sort_title)
                .on_input(|v| Message::Edit(EditMsg::SortTitle(v)))
                .style(theme::field)
                .font(theme::font())
                .padding([10, 14]),
            note("Used when the library is sorted by name."),
            space().height(4),
            toggler(d.hidden)
                .label("Hide game")
                .on_toggle(|v| Message::Edit(EditMsg::Hidden(v)))
                .size(22)
                .text_size(14)
                .font(theme::font()),
            note("Shown only under Hidden games in the library."),
        ]
        .spacing(8);
        let picture = |art: Art, title, (width, height): (f32, f32)| -> Element<'a, Message> {
            let content: Element<'_, Message> = match self.edit_preview(d, art) {
                Some(h) => image(h)
                    .content_fit(ContentFit::Cover)
                    .width(width)
                    .height(height)
                    .border_radius(tokens().cover_radius)
                    .into(),
                None => container(text("Choose an image").size(14).color(tokens().muted))
                    .center_x(width)
                    .center_y(height)
                    .style(theme::placeholder)
                    .into(),
            };
            column![
                label(title),
                button(content)
                    .padding(0)
                    .on_press(msg(EditMsg::Pick(art)))
                    .style(theme::plain)
            ]
            .push(self.steamgriddb.then(|| {
                button(text("From SteamGridDB").size(14))
                    .padding(0)
                    .on_press(msg(EditMsg::Browse(art)))
                    .style(theme::link)
            }))
            .spacing(8)
            .into()
        };
        let actions = row![
            button(text("Reset to default").size(14))
                .padding([10, 16])
                .on_press(msg(EditMsg::Reset))
                .style(theme::tonal),
            space().width(Length::Fill),
            button(text("Cancel").size(14))
                .padding([10, 18])
                .on_press(msg(EditMsg::Cancel))
                .style(theme::tonal),
            button(text("Save").size(14))
                .padding([10, 22])
                .on_press(msg(EditMsg::Save))
                .style(theme::primary),
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        Form {
            fields: fields.into(),
            cover: picture(Art::Cover, "Cover", cover),
            background: picture(Art::Background, "Background", background),
            picker: d.picker.as_ref().map(|p| self.art_picker(p)),
            actions: actions.into(),
        }
    }

    /// The picture the game would have once saved.
    fn edit_preview(&self, d: &EditDraft, art: Art) -> Option<image::Handle> {
        let game = self.library.iter().find(|g| g.id == d.game_id)?;
        let change = match art {
            Art::Cover => &d.cover,
            Art::Background => &d.background,
        };
        match (change, art) {
            (ImageChange::Set(path), _) => Some(image::Handle::from_path(path)),
            (ImageChange::Downloaded(_), _) => d.previews.get(&art).cloned(),
            (ImageChange::Keep, Art::Cover) => self.cover(&game.id),
            (ImageChange::Keep, Art::Background) => self.background(game),
            (ImageChange::Reset, Art::Cover) => self.covers.get(&game.id).cloned(),
            (ImageChange::Reset, Art::Background) => game
                .background
                .as_ref()
                .and_then(|u| self.images.get(u))
                .cloned(),
        }
    }
}

const PICTURE_HEIGHT: f32 = 200.0;
/// Width of the background in the drawer: the drawer less its padding and the room of its scrollbar.
const DRAWER_PICTURE: f32 = DRAWER_WIDTH - 48.0 - 14.0;

/// The menu a right click on a cover opens, at the pointer. A click beside it closes it.
pub(super) fn context_menu<'a>(
    page: Element<'a, Message>,
    menu: &'a ContextMenu,
) -> Element<'a, Message> {
    let edit = button(
        row![
            icon(Icon::Pencil, 16.0, tokens().text),
            text("Edit game").size(14)
        ]
        .spacing(10)
        .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .padding([8, 12])
    .on_press(Message::Edit(EditMsg::Open(menu.game_id.clone())))
    .style(theme::ghost);
    stack![
        page,
        mouse_area(
            container(Space::new())
                .width(Length::Fill)
                .height(Length::Fill)
        )
        .on_press(Message::Edit(EditMsg::CloseMenu)),
        pin(container(edit)
            .padding(6)
            .width(MENU_WIDTH)
            .style(theme::menu))
        .x(menu.at.x)
        .y(menu.at.y),
    ]
    .into()
}

impl App {
    /// The SteamGridDB search for one picture: a name to search, the games found, then that
    /// game's pictures in a row to scroll; a picture clicked is downloaded into the draft.
    fn art_picker<'a>(&'a self, p: &'a ArtPicker) -> Element<'a, Message> {
        let msg = |m| Message::Edit(m);
        let (title, empty, (width, height)) = match p.art {
            Art::Cover => (
                "Cover from SteamGridDB",
                "SteamGridDB has no cover for this game.",
                (96.0, 144.0),
            ),
            Art::Background => (
                "Background from SteamGridDB",
                "SteamGridDB has no background for this game.",
                (248.0, 80.0),
            ),
        };
        let close = button(icon(Icon::X, 16.0, tokens().text))
            .padding(6)
            .on_press(msg(EditMsg::ClosePicker))
            .style(theme::icon_button);
        let search = row![
            text_input("Name of the game", &p.query)
                .on_input(|v| Message::Edit(EditMsg::PickerQuery(v)))
                .on_submit(msg(EditMsg::Search))
                .style(theme::field)
                .font(theme::font())
                .padding([8, 12]),
            button(text("Search").size(14))
                .padding([8, 14])
                .on_press_maybe((!p.query.trim().is_empty()).then(|| msg(EditMsg::Search)))
                .style(theme::tonal),
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        let games: Element<'_, Message> = match &p.games {
            Loadable::Loading => note("Searching SteamGridDB…"),
            Loadable::Failed(e) => note(e.as_str()),
            Loadable::Ready(list) if list.is_empty() => {
                note("No game of that name on SteamGridDB.")
            }
            Loadable::Ready(list) => row(list.iter().map(|g| {
                let name = match g
                    .release_date
                    .and_then(|t| chrono::DateTime::from_timestamp(t, 0))
                {
                    Some(d) => format!("{} ({})", g.name, chrono::Datelike::year(&d)),
                    None => g.name.clone(),
                };
                button(text(name).size(13))
                    .padding([6, 12])
                    .on_press(msg(EditMsg::ChooseGame(g.id)))
                    .style(if p.game == Some(g.id) {
                        theme::accent_outline
                    } else {
                        theme::tonal
                    })
                    .into()
            }))
            .spacing(8)
            .wrap()
            .vertical_spacing(8)
            .into(),
        };
        let pictures: Option<Element<'_, Message>> = p.choices.as_ref().map(|c| match c {
            Loadable::Loading => note("Loading its pictures…"),
            Loadable::Failed(e) => note(e.as_str()),
            Loadable::Ready(list) if list.is_empty() => note(empty),
            Loadable::Ready(list) => {
                let previews = list.iter().enumerate().map(|(i, a)| {
                    let picture: Element<'_, Message> = match self.images.get(&a.thumb) {
                        Some(h) => image(h.clone())
                            .content_fit(ContentFit::Cover)
                            .width(width)
                            .height(height)
                            .border_radius(tokens().cover_radius)
                            .into(),
                        None => container(Space::new())
                            .width(width)
                            .height(height)
                            .style(theme::placeholder)
                            .into(),
                    };
                    button(picture)
                        .padding(0)
                        .on_press_maybe(p.fetching.is_none().then_some(msg(EditMsg::Choose(i))))
                        .style(theme::plain)
                        .into()
                });
                scrollable(row(previews).spacing(10).padding(Padding::ZERO.bottom(12)))
                    .direction(scrollable::Direction::Horizontal(
                        scrollable::Scrollbar::default(),
                    ))
                    .style(theme::scroller)
                    .into()
            }
        });
        column![
            row![
                text(title)
                    .size(14)
                    .color(tokens().muted)
                    .width(Length::Fill),
                close
            ]
            .align_y(Alignment::Center),
            search,
            games,
        ]
        .push(pictures)
        .push(p.fetching.map(|_| note("Downloading…")))
        .spacing(10)
        .into()
    }
}
