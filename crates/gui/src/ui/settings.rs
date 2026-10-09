//! Settings tab and the sign-in screen.

use iced::widget::{button, column, container, pick_list, row, scrollable, text, text_input};
use iced::{Alignment, Element, Length, Padding};

use super::card;
use super::format::*;
use super::widgets::logo;
use crate::settings::{ProtonChoice, SettingsMsg};
use crate::theme::{self, BOLD, SEMIBOLD, tokens};
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
            text("Settings").size(30).font(BOLD),
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
                            .padding([8, 12]),
                        button(text("Save").size(14))
                            .padding([8, 16])
                            .on_press(Message::Settings(SettingsMsg::SaveRoot))
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
                        .padding([8, 16]),
                    ]
                    .spacing(10)
                    .align_y(Alignment::Center)
                    .into(),
                ],
            ),
            card(
                "About",
                vec![
                    text(format!(
                        "SlattyLauncher {} · GPL-3.0-or-later · icons by Lucide (ISC)",
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
            .style(theme::scroller)
            .height(Length::Fill)
            .into()
    }

    pub(super) fn login_view(&self) -> Element<'_, Message> {
        let busy = self.login_busy;
        let content = column![
            logo(72.0),
            text("Sign in to GOG").size(30).font(BOLD),
            text(
                "Sign-in happens in your browser; SlattyLauncher never sees your password. \
                 Once signed in, the browser shows an almost blank page on embed.gog.com: \
                 copy that page's full address and paste it below."
            )
            .color(tokens().muted),
            button(text("Open the GOG sign-in page").font(SEMIBOLD))
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
