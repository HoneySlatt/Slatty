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
use crate::edit::{Art, ContextMenu, EditDraft, EditMsg, MENU_WIDTH};
use crate::icons::{Icon, icon};
use crate::theme::{self, bold, tokens};
use crate::{App, Message};

/// The parts of the edit form, laid out differently by the dialog and the drawer.
struct Form<'a> {
    fields: Element<'a, Message>,
    cover: Element<'a, Message>,
    background: Element<'a, Message>,
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
        let dialog = container(
            column![
                header,
                form.fields,
                row![form.cover, form.background].spacing(24),
                note("Click a picture to choose a file."),
                form.actions
            ]
            .spacing(20),
        )
        .padding(26)
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
            column![
                form.fields,
                form.cover,
                form.background,
                note("Click a picture to choose a file."),
            ]
            .spacing(20)
            .padding(Padding::ZERO.right(14)),
        )
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
