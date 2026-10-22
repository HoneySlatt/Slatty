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
use slatty_core::emulated::Emulator;

/// An emulator in the dialog's menu, with the system it runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct EmulatorChoice(Emulator);

impl std::fmt::Display for EmulatorChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} · {}",
            self.0.name(),
            super::format::system_name(self.0)
        )
    }
}

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
                text(match d.emulated {
                    true => "From no store: a disc image, a file or a folder, run in an emulator",
                    false =>
                        "From no store: a Windows .exe, or a Linux program, script or AppImage",
                })
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
            toggler(d.emulated)
                .label("Run it in an emulator")
                .on_toggle_maybe(
                    (d.installed_by.is_none() && !d.busy)
                        .then_some(|v| Message::Add(AddMsg::Emulated(v)))
                )
                .size(22)
                .text_size(14)
                .font(theme::font()),
        ]
        .spacing(8);
        if !d.emulated {
            fields = fields.push(
                toggler(d.with_installer)
                    .label("Install it first, from an installer or archive")
                    .on_toggle_maybe(
                        (d.installed_by.is_none() && !d.busy)
                            .then_some(|v| Message::Add(AddMsg::WithInstaller(v))),
                    )
                    .size(22)
                    .text_size(14)
                    .font(theme::font()),
            );
        }
        if d.emulated {
            let folders = d.emulator.is_some_and(|e| e.games_are_folders());
            let emulators: Vec<EmulatorChoice> = Emulator::ALL.map(EmulatorChoice).into();
            fields = fields.extend([
                label("Emulator").into(),
                pick_list(emulators, d.emulator.map(EmulatorChoice), |c| {
                    Message::Add(AddMsg::Emulator(c.0))
                })
                .placeholder("Choose an emulator")
                .style(theme::select)
                .font(theme::font())
                .padding([10, 14])
                .width(Length::Fill)
                .into(),
                label(if folders { "Game folder" } else { "Game file" }).into(),
                row![
                    field(
                        if folders {
                            "/games/PS3/BLUS00000"
                        } else {
                            "/games/PS2/Game.iso"
                        },
                        &d.game,
                        AddMsg::Game
                    )
                    .width(Length::Fill),
                    browse(AddMsg::PickGame),
                ]
                .spacing(10)
                .align_y(Alignment::Center)
                .into(),
                label("Title").into(),
                field(
                    "Read from the game, or its file's name",
                    &d.title,
                    AddMsg::Title,
                )
                .into(),
                note(
                    "Removing it from the library leaves its files as they are. Its emulator \
                     keeps its saves and settings, and is not isolated from your files.",
                ),
            ]);
        } else if d.installer_first() {
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
                    (true, true) => {
                        "Installing: finish the installer in its own window, and install the game \
                         on drive D:, its folder."
                    }
                    (true, false) => "Unpacking…",
                    (false, true) => {
                        "A Windows installer runs in a Wine prefix made for the game, where a \
                         folder named after the title is drive D:: install the game there. Its \
                         program is chosen once it ends. Isolated, the installer can write in \
                         that folder alone."
                    }
                    (false, false) => {
                        "A Linux installer (MojoSetup .sh) or archive (.zip, .tar.gz, .tar.xz) is \
                         unpacked, never run, into a folder named after the title; its program \
                         is chosen there."
                    }
                }))
                .push(label("Title"))
                .push(field("Game", &d.title, AddMsg::Title))
                .push(label("Install into"))
                .push(
                    row![
                        field("/games", &d.into, AddMsg::Into).width(Length::Fill),
                        browse(AddMsg::PickInto),
                    ]
                    .spacing(10)
                    .align_y(Alignment::Center),
                );
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
                    // After an installer, the folder it installed into, when the program is there
                    // or in the game's prefix (`added::add`).
                    field(
                        match d.installed_by.is_some() {
                            true => "The folder installed into, or the program's",
                            false => "The program's folder",
                        },
                        &d.folder,
                        AddMsg::Folder,
                    )
                    .width(Length::Fill),
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
        if !d.emulated && d.windows() {
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
        if !d.emulated {
            fields = fields.push(space().height(4)).push(
                toggler(d.isolated)
                    .label("Isolated from your files")
                    .on_toggle(|v| Message::Add(AddMsg::Isolated(v)))
                    .size(22)
                    .text_size(14)
                    .font(theme::font()),
            );
            fields = fields.push(note(
                "Removing it from the library deletes only what Slatty unpacked for it: your \
                 own files stay. It has no cloud saves or achievements, and GOG is not told of it.",
            ));
        }
        let (label_of, ready, next) = if d.emulated {
            (
                if d.busy { "Adding…" } else { "Add" },
                !d.busy && d.emulator.is_some() && !d.game.trim().is_empty(),
                AddMsg::Save,
            )
        } else if d.installer_first() {
            (
                match (d.busy, d.windows()) {
                    (true, true) => "Installing…",
                    (true, false) => "Unpacking…",
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
        // Why the game was not added, above the buttons: at the end of the form, it would be
        // scrolled out of sight in a short window.
        let error = d
            .error
            .as_ref()
            .map(|e| text(e.as_str()).size(14).color(tokens().danger));
        let dialog = container(column![header, body].push(error).push(actions).spacing(20))
            .padding(Padding::new(26.0).right(12.0))
            .max_width(640)
            .style(theme::card);
        let backdrop = container(Space::new())
            .width(Length::Fill)
            .height(Length::Fill)
            .style(theme::backdrop);
        // Once an installer ran, closing throws away what it installed: not on a click beside.
        let backdrop: Element<'_, Message> = if d.busy || d.installed_by.is_some() {
            opaque(backdrop)
        } else {
            mouse_area(backdrop).on_press(msg(AddMsg::Cancel)).into()
        };
        stack![page, backdrop, center(opaque(dialog))].into()
    }
}
