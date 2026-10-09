//! The dialog that changes a game's title, sorting title, cover and background.

use iced::widget::{
    Space, button, center, column, container, image, mouse_area, opaque, pin, row, space, stack,
    text, text_input,
};
use iced::{Alignment, ContentFit, Element, Length};
use slatty_core::custom::ImageChange;

use super::note;
use crate::edit::{Art, ContextMenu, EditDraft, EditMsg, MENU_WIDTH};
use crate::icons::{Icon, icon};
use crate::theme::{self, BOLD, tokens};
use crate::{App, Message};

impl App {
    pub(super) fn edit_dialog<'a>(
        &'a self,
        page: Element<'a, Message>,
        d: &'a EditDraft,
    ) -> Element<'a, Message> {
        let gog_title = self.gog_titles.get(&d.game_id).map_or("", String::as_str);
        let msg = |m| Message::Edit(m);
        let label = |t| text(t).size(14).color(tokens().muted);
        let close = button(container(icon(Icon::X, 20.0, tokens().text)).center(24))
            .padding(10)
            .on_press(msg(EditMsg::Cancel))
            .style(theme::tonal);
        let header = row![
            column![
                text("Edit game").size(26).font(BOLD),
                text(gog_title).size(14).color(tokens().muted),
            ]
            .spacing(4)
            .width(Length::Fill),
            close,
        ];
        let fields = column![
            label("Title"),
            text_input(gog_title, &d.title)
                .on_input(|v| Message::Edit(EditMsg::Title(v)))
                .style(theme::field)
                .padding([10, 14]),
            label("Sorting title"),
            text_input(&d.title, &d.sort_title)
                .on_input(|v| Message::Edit(EditMsg::SortTitle(v)))
                .style(theme::field)
                .padding([10, 14]),
            note("Used when the library is sorted by name."),
        ]
        .spacing(8);
        let picture = |art: Art, width: f32| {
            let content: Element<'_, Message> = match self.edit_preview(d, art) {
                Some(h) => image(h)
                    .content_fit(ContentFit::Cover)
                    .width(width)
                    .height(PICTURE_HEIGHT)
                    .border_radius(tokens().cover_radius)
                    .into(),
                None => container(text("Choose an image").size(14).color(tokens().muted))
                    .center_x(width)
                    .center_y(PICTURE_HEIGHT)
                    .style(theme::placeholder)
                    .into(),
            };
            button(content)
                .padding(0)
                .on_press(msg(EditMsg::Pick(art)))
                .style(theme::plain)
        };
        let art = row![
            column![
                label("Cover"),
                picture(Art::Cover, PICTURE_HEIGHT * 3.0 / 4.0)
            ]
            .spacing(8),
            column![
                label("Background"),
                picture(Art::Background, PICTURE_HEIGHT * 16.0 / 9.0)
            ]
            .spacing(8),
        ]
        .spacing(24);
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
        let dialog = container(
            column![
                header,
                fields,
                art,
                note("Click a picture to choose a file."),
                actions
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
            .on_press(msg(EditMsg::Cancel)),
            center(opaque(dialog)),
        ]
        .into()
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
