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
        // The game is installed first: a Windows installer runs in the prefix made for it, a Linux
        // one or an archive is unpacked; its program is then picked where it was put.
        let mut fields = column![
            toggler(d.with_installer)
                .label("Install it first, from an installer or archive")
                .on_toggle_maybe(
                    (d.installed_by.is_none() && !d.busy)
                        .then_some(|v| Message::Add(AddMsg::WithInstaller(v)))
                )
                .size(22)
                .text_size(14)
                .font(theme::font()),
        ]
        .spacing(8);
        if d.installer_first() {
            fields = fields
                .push(label("Installer"))
                .push(
                    row![
                        field("/downloads/setup.exe", &d.installer, AddMsg::Installer)
                            .width(Length::Fill),
                        browse(AddMsg::PickInstaller),
                    ]
                    .spacing(10)
                    .align_y(Alignment::Center),
                )
                .push(note(match (d.busy, d.windows()) {
                    (true, true) => "Installing: finish the installer in its own window.",
                    (true, false) => "Unpacking…",
                    (false, true) => {
                        "A Windows installer installs the game into a Wine prefix made for it; \
                         its program is chosen there once it ends. Its folder is the one it can \
                         write in when isolated."
                    }
                    (false, false) => {
                        "A Linux installer (MojoSetup .sh) or archive (.zip, .tar.gz, .tar.xz) is \
                         unpacked, never run, into a folder named after the title; its program \
                         is chosen there."
                    }
                }))
                .push(label("Title"))
                .push(field("Game", &d.title, AddMsg::Title));
            if !d.windows() {
                fields = fields.push(label("Unpack into")).push(
                    row![
                        field("/games", &d.into, AddMsg::Into).width(Length::Fill),
                        browse(AddMsg::PickInto),
                    ]
                    .spacing(10)
                    .align_y(Alignment::Center),
                );
            }
        } else {
            if d.installed_by.is_some() {
                fields = fields.push(note(
                    "The installer ended: choose the game's program where it installed it.",
                ));
            }
            fields = fields.extend([
                label("Program").into(),
                row![
                    field("/games/Game/game.exe", &d.program, AddMsg::Program).width(Length::Fill),
                    browse(AddMsg::PickProgram),
                ]
                .spacing(10)
                .align_y(Alignment::Center)
                .into(),
                label("Title").into(),
                field("Game", &d.title, AddMsg::Title).into(),
                label("Arguments").into(),
                field("-windowed", &d.args, AddMsg::Args).into(),
                label("Game folder").into(),
                row![
                    field("The program's folder", &d.folder, AddMsg::Folder).width(Length::Fill),
                    browse(AddMsg::PickFolder),
                ]
                .spacing(10)
                .align_y(Alignment::Center)
                .into(),
                note(
                    "The folder the game is in, when the program sits deeper in it. An isolated \
                     game can write in it and nowhere else of yours.",
                ),
            ]);
        }
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
            "Removing it from the library deletes only what SlattyLauncher unpacked for it: your \
             own files stay. It has no cloud saves or achievements, and GOG is not told of it.",
        ));
        if let Some(e) = &d.error {
            fields = fields.push(text(e.as_str()).size(14).color(tokens().danger));
        }
        let (label_of, ready, next) = if d.installer_first() {
            (
                match (d.busy, d.windows()) {
                    (true, _) => "Installing…",
                    (false, true) => "Run installer",
                    (false, false) => "Unpack",
                },
                // A Linux one is unpacked into a folder named after the title.
                !d.busy
                    && !d.installer.trim().is_empty()
                    && (d.windows() || !d.title.trim().is_empty()),
                AddMsg::RunInstaller,
            )
        } else {
            (
                if d.busy { "Adding…" } else { "Add" },
                !d.busy && !d.program.trim().is_empty() && !d.title.trim().is_empty(),
                AddMsg::Save,
            )
        };
        let actions = row![
            space().width(Length::Fill),
            button(text("Cancel").size(14))
                .padding([10, 18])
                .on_press_maybe((!d.busy).then(|| msg(AddMsg::Cancel)))
                .style(theme::tonal),
            button(text(label_of).size(14))
                .padding([10, 22])
                .on_press_maybe(ready.then(|| msg(next)))
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
