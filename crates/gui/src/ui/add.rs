//! The dialog adding a game from no store, over the library.

use crate::theme::text;
use iced::widget::{
    Space, button, center, column, container, mouse_area, opaque, pick_list, row, scrollable,
    space, stack, text_input, toggler,
};
use iced::{Alignment, Element, Length, Padding};

use super::note;
use crate::add::{AddDraft, AddMsg};
use crate::icons::{Icon, icon};
use crate::settings::ProtonChoice;
use crate::theme::{self, bold, tokens};
use crate::{App, Message};

impl App {
    pub(super) fn add_dialog<'a>(
        &'a self,
        page: Element<'a, Message>,
        d: &'a AddDraft,
    ) -> Element<'a, Message> {
        let msg = |m| Message::Add(m);
        let label = |t| text(t).size(14).color(tokens().muted);
        let field = |placeholder, value: &'a str, on: fn(String) -> AddMsg| {
            text_input(placeholder, value)
                .on_input(move |v| Message::Add(on(v)))
                .style(theme::field)
                .font(theme::font())
                .padding([10, 14])
        };
        let browse = |m| {
            button(
                row![
                    icon(Icon::FolderOpen, 16.0, tokens().text),
                    text("Browse").size(14)
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .padding([10, 16])
            .on_press_maybe((!d.busy).then(|| msg(m)))
            .style(theme::tonal)
        };
        let close = button(container(icon(Icon::X, 20.0, tokens().text)).center(24))
            .padding(10)
            .on_press(msg(AddMsg::Cancel))
            .style(theme::tonal);
        let header = row![
            column![
                text("Add a game").size(26).font(bold()),
                text("From no store: a Windows .exe, or a Linux program, script or AppImage")
                    .size(14)
                    .color(tokens().muted),
            ]
            .spacing(4)
            .width(Length::Fill),
            close,
        ];
        let mut fields = column![
            label("Program"),
            row![
                field("/games/Game/game.exe", &d.program, AddMsg::Program).width(Length::Fill),
                browse(AddMsg::PickProgram),
            ]
            .spacing(10)
            .align_y(Alignment::Center),
            label("Title"),
            field("Game", &d.title, AddMsg::Title),
            label("Arguments"),
            field("-windowed", &d.args, AddMsg::Args),
            label("Game folder"),
            row![
                field("The program's folder", &d.folder, AddMsg::Folder).width(Length::Fill),
                browse(AddMsg::PickFolder),
            ]
            .spacing(10)
            .align_y(Alignment::Center),
            note(
                "The folder the game is in, when the program sits deeper in it. An isolated game \
                 can write in it and nowhere else of yours."
            ),
        ]
        .spacing(8);
        if d.windows() {
            fields = fields.push(label("Proton")).push(
                pick_list(
                    self.proton_offers(),
                    d.proton.clone().map(ProtonChoice),
                    |c| Message::Add(AddMsg::Proton(c)),
                )
                .placeholder("Choose a Proton build")
                .style(theme::select)
                .font(theme::font())
                .padding([10, 14])
                .width(Length::Fill),
            );
        }
        fields = fields.push(space().height(4)).push(
            toggler(d.isolated)
                .label("Isolated from your files")
                .on_toggle(|v| Message::Add(AddMsg::Isolated(v)))
                .size(22)
                .text_size(14)
                .font(theme::font()),
        );
        fields = fields.push(note(
            "Its files stay yours: removing it from the library leaves them. It has no cloud \
             saves or achievements, and GOG is not told of it.",
        ));
        if let Some(e) = &d.error {
            fields = fields.push(text(e.as_str()).size(14).color(tokens().danger));
        }
        let ready = !d.busy && !d.program.trim().is_empty() && !d.title.trim().is_empty();
        let actions = row![
            space().width(Length::Fill),
            button(text("Cancel").size(14))
                .padding([10, 18])
                .on_press(msg(AddMsg::Cancel))
                .style(theme::tonal),
            button(text(if d.busy { "Adding…" } else { "Add" }).size(14))
                .padding([10, 22])
                .on_press_maybe(ready.then(|| msg(AddMsg::Save)))
                .style(theme::primary),
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        // The form scrolls between its title and its buttons in a short window.
        const AROUND: f32 = 48.0 + 52.0 + 80.0 + 52.0 + 40.0;
        let body =
            container(scrollable(fields.padding(Padding::ZERO.right(14))).style(theme::scroller))
                .max_height((self.window.height - AROUND).max(160.0));
        let dialog = container(column![header, body, actions].spacing(20))
            .padding(Padding::new(26.0).right(12.0))
            .max_width(640)
            .style(theme::card);
        stack![
            page,
            mouse_area(
                container(Space::new())
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .style(theme::backdrop)
            )
            .on_press(msg(AddMsg::Cancel)),
            center(opaque(dialog)),
        ]
        .into()
    }
}
