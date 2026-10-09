//! Settings tab and the sign-in screen.

use crate::theme::text;
use iced::widget::{
    Space, button, column, container, pick_list, row, scrollable, slider, text_input,
};
use iced::{Alignment, Element, Length, Padding};

use super::card;
use super::format::*;
use super::widgets::logo;
use crate::icons::{Icon, icon};
use crate::settings::{FontChoice, ProtonChoice, SettingsMsg};
use crate::theme::{self, bold, semibold, tokens};
use crate::{App, Message};

impl App {
    pub(super) fn settings_page(&self) -> Element<'_, Message> {
        let account = self.account.as_ref();
        let choices: Vec<ProtonChoice> = self
            .proton_choices
            .iter()
            .cloned()
            .map(ProtonChoice)
            .collect();
        let selected = self.proton.clone().map(ProtonChoice);
        let label = |t| text(t).size(15).width(210).color(tokens().muted);
        let cache_note = match self.fetched_at {
            Some(ts) => format!("Last refreshed {}", local_time(ts)),
            None => "Never refreshed".into(),
        };
        let content = column![
            text("Settings").size(30).font(bold()),
            card(
                "Account",
                vec![
                    row![
                        label("Signed in as"),
                        text(account.map(|a| a.username.as_str()).unwrap_or_default())
                            .size(15)
                            .width(Length::Fill),
                        button(text("Log out").size(14))
                            .padding([8, 16])
                            .on_press(Message::Logout)
                            .style(theme::tonal),
                    ]
                    .align_y(Alignment::Center)
                    .into(),
                ],
            ),
            card(
                "Library",
                vec![
                    row![
                        label("Games"),
                        text(format!("{} owned · {cache_note}", self.library.len()))
                            .size(15)
                            .width(Length::Fill),
                        button(
                            text(if self.library_busy {
                                "Refreshing…"
                            } else {
                                "Refresh library"
                            })
                            .size(14)
                        )
                        .padding([8, 16])
                        .on_press_maybe((!self.library_busy).then_some(Message::SyncLibrary))
                        .style(theme::tonal),
                    ]
                    .align_y(Alignment::Center)
                    .into(),
                ],
            ),
            card(
                "Installs",
                vec![
                    row![
                        label("Default installation path"),
                        text_input("/home/…/Games/GOG", &self.library_root)
                            .on_input(|v| Message::Settings(SettingsMsg::RootInput(v)))
                            .on_submit(Message::Settings(SettingsMsg::SaveRoot))
                            .style(theme::field)
                            .font(theme::font())
                            .padding([8, 12]),
                        button(
                            row![
                                icon(Icon::FolderOpen, 16.0, tokens().text),
                                text("Browse").size(14)
                            ]
                            .spacing(8)
                            .align_y(Alignment::Center)
                        )
                        .padding([8, 16])
                        .on_press(Message::Settings(SettingsMsg::BrowseRoot))
                        .style(theme::tonal),
                    ]
                    .spacing(10)
                    .align_y(Alignment::Center)
                    .into(),
                    row![
                        label("Default Proton"),
                        pick_list(choices, selected, |c| Message::Settings(
                            SettingsMsg::Proton(c)
                        ))
                        .placeholder("No Proton build found (Steam or compatibilitytools.d)")
                        .style(theme::select)
                        .font(theme::font())
                        .padding([8, 16]),
                    ]
                    .spacing(10)
                    .align_y(Alignment::Center)
                    .into(),
                ],
            ),
            card("Appearance", vec![self.appearance()]),
            card(
                "About",
                vec![
                    text(format!(
                        "SlattyLauncher {} · GPL-3.0-or-later · icons by Lucide (ISC) · Geist font (OFL)",
                        env!("CARGO_PKG_VERSION")
                    ))
                    .size(14)
                    .color(tokens().muted)
                    .into(),
                ],
            ),
        ]
        .spacing(16)
        .max_width(860);
        scrollable(container(content).padding(Padding::ZERO.top(10)))
            .spacing(8)
            .style(theme::scroller)
            .height(Length::Fill)
            .into()
    }

    pub(super) fn login_view(&self) -> Element<'_, Message> {
        let busy = self.login_busy;
        let content = column![
            logo(72.0),
            text("Sign in to GOG").size(30).font(bold()),
            text(
                "Sign-in happens in your browser; SlattyLauncher never sees your password. \
                 Once signed in, the browser shows an almost blank page on embed.gog.com: \
                 copy that page's full address and paste it below."
            )
            .color(tokens().muted),
            button(text("Open the GOG sign-in page").font(semibold()))
                .padding([12, 22])
                .on_press(Message::OpenLoginPage)
                .style(theme::primary),
            row![
                text_input(
                    "https://embed.gog.com/on_login_success?…&code=…",
                    &self.login_input
                )
                .on_input(Message::LoginInput)
                .on_submit(Message::SubmitLogin)
                .style(theme::field)
                .font(theme::font())
                .padding([10, 14])
                .width(Length::Fill),
                button(text("Paste"))
                    .padding([10, 16])
                    .on_press(Message::PasteLogin)
                    .style(theme::tonal),
                button(text(if busy { "Signing in…" } else { "Sign in" }))
                    .padding([10, 16])
                    .on_press_maybe(
                        (!busy && !self.login_input.is_empty()).then_some(Message::SubmitLogin)
                    )
                    .style(theme::tonal),
            ]
            .spacing(8),
        ]
        .spacing(18)
        .max_width(720);
        container(content).padding(60).center_x(Length::Fill).into()
    }
}

impl App {
    /// The theme file: where it is, and the way to create, edit and apply it.
    fn appearance(&self) -> Element<'_, Message> {
        let Some(core) = &self.core else {
            return Space::new().into();
        };
        let path = crate::theme::file(&core.dirs.config);
        let exists = path.exists();
        let action = |label, msg| {
            button(text(label).size(14))
                .padding([8, 16])
                .on_press(Message::Settings(msg))
                .style(theme::tonal)
        };
        let buttons = if exists {
            row![
                action("Edit", SettingsMsg::EditTheme),
                action("Reload", SettingsMsg::ReloadTheme),
            ]
        } else {
            row![action("Create theme file", SettingsMsg::CreateTheme)]
        };
        let current = match theme::font().family {
            iced::font::Family::Name(name) if name != theme::DEFAULT_FAMILY => {
                FontChoice::Family(name.to_string())
            }
            _ => FontChoice::Default,
        };
        let fonts: Vec<FontChoice> = std::iter::once(FontChoice::Default)
            .chain(
                self.font_families
                    .iter()
                    .filter(|f| *f != theme::DEFAULT_FAMILY)
                    .cloned()
                    .map(FontChoice::Family),
            )
            .collect();
        column![
            row![
                text("Theme").size(15).width(210).color(tokens().muted),
                pick_list(crate::presets::Preset::ALL, Some(theme::preset()), |p| {
                    Message::Settings(SettingsMsg::Preset(p))
                })
                .style(theme::select)
                .font(theme::font())
                .padding([8, 16])
                .width(Length::Fill),
            ]
            .spacing(10)
            .align_y(Alignment::Center),
            row![
                text("Cover size").size(15).width(210).color(tokens().muted),
                icon(Icon::LayoutGrid, 20.0, tokens().muted),
                slider(
                    crate::COVER_WIDTHS.0..=crate::COVER_WIDTHS.1,
                    self.card_width,
                    Message::CardWidth
                )
                .on_release(Message::Settings(SettingsMsg::SaveCoverWidth))
                .width(Length::Fill)
                .style(theme::size_slider),
            ]
            .spacing(12)
            .align_y(Alignment::Center),
            row![
                text("Font").size(15).width(210).color(tokens().muted),
                pick_list(fonts, Some(current), |f| Message::Settings(
                    SettingsMsg::Font(f)
                ))
                .style(theme::select)
                .font(theme::font())
                .padding([8, 16])
                .width(Length::Fill),
            ]
            .spacing(10)
            .align_y(Alignment::Center),
            row![
                text("Theme file").size(15).width(210).color(tokens().muted),
                text(path.display().to_string())
                    .size(14)
                    .width(Length::Fill),
                buttons.spacing(8),
            ]
            .spacing(10)
            .align_y(Alignment::Center),
            text(if exists {
                "Colours, corners and the page transition come from this file. Edit it, then \
                 Reload to see the change."
            } else {
                "The default look is in use. Create the theme file to change colours, corners and \
                 the page transition."
            })
            .size(13)
            .color(tokens().muted),
        ]
        .spacing(10)
        .into()
    }
}
