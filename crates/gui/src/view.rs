use iced::widget::{
    Column, button, column, container, grid, image, row, scrollable, text, text_input,
};
use iced::{Alignment, ContentFit, Element, Length};
use slatty_core::achievements::Achievement;
use slatty_core::cloud::plan::Warning;
use slatty_core::cloud::sync::Prefer;
use slatty_core::install::Install;
use slatty_core::library::LibraryGame;
use slatty_core::play::{CloudSummary, PlayEvent};
use slatty_core::runner::Runner;

use crate::{AchievementChange, App, CloudRequest, Loadable, Message, PendingChange};

impl App {
    pub fn view(&self) -> Element<'_, Message> {
        if let Some(e) = &self.fatal {
            return container(text(format!("Démarrage impossible : {e}")).size(18))
                .padding(40)
                .into();
        }
        if self.core.is_none() {
            return container(text("Chargement…")).padding(40).into();
        }
        let body = match &self.account {
            None => self.login_view(),
            Some(_) => self.library_view(),
        };
        let mut page = Column::new();
        if let Some(n) = &self.notice {
            let style = if n.error {
                container::danger
            } else {
                container::secondary
            };
            page = page.push(
                container(
                    row![
                        text(&n.text).width(Length::Fill),
                        button(text("Fermer"))
                            .on_press(Message::DismissNotice)
                            .style(button::text)
                    ]
                    .align_y(Alignment::Center)
                    .spacing(12),
                )
                .padding(10)
                .width(Length::Fill)
                .style(style),
            );
        }
        page.push(body).into()
    }

    fn login_view(&self) -> Element<'_, Message> {
        let busy = self.login_busy;
        let content = column![
            text("Connexion à GOG").size(28),
            text(
                "La connexion se fait dans votre navigateur ; SlattyLauncher ne voit jamais votre mot de passe. \
                 Une fois connecté, le navigateur affiche une page presque vide sur embed.gog.com : \
                 copiez l'adresse complète de cette page et collez-la ci-dessous."
            ),
            button(text("Ouvrir la page de connexion GOG")).on_press(Message::OpenLoginPage),
            row![
                text_input("https://embed.gog.com/on_login_success?…&code=…", &self.login_input)
                    .on_input(Message::LoginInput)
                    .on_submit(Message::SubmitLogin)
                    .width(Length::Fill),
                button(text("Coller")).on_press(Message::PasteLogin).style(button::secondary),
                button(text(if busy { "Connexion…" } else { "Valider" }))
                    .on_press_maybe((!busy && !self.login_input.is_empty()).then_some(Message::SubmitLogin)),
            ]
            .spacing(8),
        ]
        .spacing(16)
        .max_width(720);
        container(content).padding(40).center_x(Length::Fill).into()
    }

    fn library_view(&self) -> Element<'_, Message> {
        let account = self
            .account
            .as_ref()
            .map(|a| a.username.as_str())
            .unwrap_or_default();
        let cache_note = match self.fetched_at {
            Some(ts) => format!("cache du {}", local_time(ts)),
            None => "aucun cache".into(),
        };
        let top = row![
            text_input("Rechercher…", &self.search)
                .on_input(Message::Search)
                .width(Length::FillPortion(3)),
            text(format!("{} jeux · {cache_note}", self.library.len()))
                .width(Length::FillPortion(2)),
            button(text(if self.library_busy {
                "Actualisation…"
            } else {
                "Actualiser"
            }))
            .on_press_maybe((!self.library_busy).then_some(Message::SyncLibrary))
            .style(button::secondary),
            text(account),
            button(text("Déconnexion"))
                .on_press(Message::Logout)
                .style(button::text),
        ]
        .spacing(12)
        .align_y(Alignment::Center);

        let needle = self.search.to_lowercase();
        let cards: Vec<Element<'_, Message>> = self
            .library
            .iter()
            .filter(|g| needle.is_empty() || g.title.to_lowercase().contains(&needle))
            .map(|g| self.card(g))
            .collect();
        let gallery: Element<'_, Message> = if cards.is_empty() {
            container(text(if self.library.is_empty() {
                "Bibliothèque vide : cliquez sur « Actualiser »."
            } else {
                "Aucun jeu ne correspond à la recherche."
            }))
            .padding(20)
            .into()
        } else {
            scrollable(
                grid(cards)
                    .fluid(190)
                    .spacing(14)
                    .height(grid::aspect_ratio(3, 5)),
            )
            .spacing(10)
            .height(Length::Fill)
            .into()
        };

        let main = match self
            .selected
            .as_ref()
            .and_then(|id| self.library.iter().find(|g| &g.id == id))
        {
            Some(game) => row![
                container(gallery).width(Length::FillPortion(3)),
                container(self.detail(game))
                    .width(Length::FillPortion(2))
                    .height(Length::Fill)
            ]
            .spacing(16),
            None => row![gallery],
        };
        column![top, main].spacing(16).padding(16).into()
    }

    fn card<'a>(&'a self, g: &'a LibraryGame) -> Element<'a, Message> {
        let art: Element<'_, Message> = match self.covers.get(&g.id) {
            Some(h) => image(h.clone())
                .content_fit(ContentFit::Cover)
                .width(Length::Fill)
                .height(Length::Fill)
                .into(),
            None => container(text(&g.title).size(14))
                .padding(8)
                .width(Length::Fill)
                .height(Length::Fill)
                .style(container::secondary)
                .into(),
        };
        let installed = self.installs.contains_key(&g.id);
        let label = row![
            text(&g.title).size(13).width(Length::Fill),
            text(if installed { "●" } else { "" }).size(13)
        ];
        let selected = self.selected.as_deref() == Some(g.id.as_str());
        button(column![art, label].spacing(6))
            .on_press(Message::Select(g.id.clone()))
            .padding(4)
            .style(if selected {
                button::primary
            } else {
                button::text
            })
            .into()
    }

    fn detail<'a>(&'a self, g: &'a LibraryGame) -> Element<'a, Message> {
        let mut col = column![
            row![
                text(&g.title).size(24).width(Length::Fill),
                button(text("Fermer"))
                    .on_press(Message::CloseDetail)
                    .style(button::text)
            ]
            .align_y(Alignment::Center),
            text(format!(
                "Plateformes : {}",
                if g.os.is_empty() {
                    "inconnues".into()
                } else {
                    g.os.join(", ")
                }
            ))
            .size(14),
        ]
        .spacing(12);

        let Some(install) = self.installs.get(&g.id) else {
            col = col.push(text(
                "Non installé. L'installation depuis le lanceur n'est pas encore disponible ; \
                 un jeu déjà installé peut être importé avec `slatty import`.",
            ));
            col = col.push(self.achievements_section(g));
            return scrollable(col.padding(8)).height(Length::Fill).into();
        };
        col = col.push(text(format!("Dossier : {}", install.path.display())).size(13));
        col = col.push(text(runner_label(install)).size(13));

        let playing = self.play.as_ref().filter(|p| p.running);
        let this_running = playing.is_some_and(|p| p.game_id == g.id);
        col = col.push(
            row![
                button(text(if this_running { "En cours…" } else { "Jouer" }))
                    .on_press_maybe(playing.is_none().then(|| Message::Play(g.id.clone())))
                    .style(button::success),
                button(text("Arrêter le jeu"))
                    .on_press_maybe(this_running.then_some(Message::StopGame))
                    .style(button::danger),
            ]
            .spacing(8),
        );
        if let Some(p) = self.play.as_ref().filter(|p| p.game_id == g.id) {
            col = col.push(section(
                "Session",
                p.log.iter().map(|l| text(l).size(13).into()).collect(),
            ));
        }

        let cloud = self.cloud.get(&g.id);
        let cloud_busy = cloud.is_some_and(|c| c.busy);
        let mut cloud_items: Vec<Element<'_, Message>> = vec![
            row![
                button(text("Vérifier"))
                    .on_press_maybe(
                        (!cloud_busy).then(|| Message::Cloud(g.id.clone(), CloudRequest::Check))
                    )
                    .style(button::secondary),
                button(text("Synchroniser"))
                    .on_press_maybe(
                        (!cloud_busy).then(|| Message::Cloud(g.id.clone(), CloudRequest::Sync))
                    )
                    .style(button::secondary),
            ]
            .spacing(8)
            .into(),
        ];
        if let Some(c) = cloud {
            cloud_items.extend(c.lines.iter().map(|l| text(l).size(13).into()));
            if c.conflicts && !c.busy {
                cloud_items.push(
                    text(
                        "Les deux versions ont changé. Choisissez celle à garder ; \
                         l'autre est conservée dans le dossier de sauvegardes.",
                    )
                    .size(13)
                    .into(),
                );
                cloud_items.push(
                    row![
                        button(text("Garder la version locale")).on_press(Message::Cloud(
                            g.id.clone(),
                            CloudRequest::Keep(Prefer::Local)
                        )),
                        button(text("Garder la version cloud")).on_press(Message::Cloud(
                            g.id.clone(),
                            CloudRequest::Keep(Prefer::Remote)
                        )),
                    ]
                    .spacing(8)
                    .into(),
                );
            }
        }
        col = col.push(section("Sauvegardes cloud", cloud_items));
        col = col.push(self.achievements_section(g));
        scrollable(col.padding(8)).height(Length::Fill).into()
    }

    fn achievements_section<'a>(&'a self, g: &'a LibraryGame) -> Element<'a, Message> {
        let mut items: Vec<Element<'_, Message>> = Vec::new();
        match self.achievements.get(&g.id) {
            None => items.push(
                button(text("Afficher les achievements"))
                    .on_press(Message::LoadAchievements(g.id.clone()))
                    .style(button::secondary)
                    .into(),
            ),
            Some(Loadable::Loading) => items.push(text("Chargement…").into()),
            Some(Loadable::Failed(e)) => {
                items.push(text(format!("Indisponible : {e}")).size(13).into());
                items.push(
                    button(text("Réessayer"))
                        .on_press(Message::LoadAchievements(g.id.clone()))
                        .into(),
                );
            }
            Some(Loadable::Ready(list)) => {
                let unlocked = list.iter().filter(|a| a.date_unlocked.is_some()).count();
                let locked: Vec<AchievementChange> = list
                    .iter()
                    .filter(|a| a.date_unlocked.is_none())
                    .map(|a| change(a, true))
                    .collect();
                items.push(
                    row![
                        text(format!(
                            "{unlocked} / {} débloqués (données GOG)",
                            list.len()
                        ))
                        .size(14)
                        .width(Length::Fill),
                        button(text("Tout débloquer"))
                            .on_press_maybe((!locked.is_empty()).then(|| {
                                Message::AskAchievementChange(g.id.clone(), locked.clone())
                            }))
                            .style(button::secondary),
                    ]
                    .align_y(Alignment::Center)
                    .into(),
                );
                if let Some(p) = self.pending_change.as_ref().filter(|p| p.game_id == g.id) {
                    items.push(self.confirmation(p));
                }
                for a in list {
                    let done = a.date_unlocked.is_some();
                    let name = if a.visible || done {
                        a.name.as_str()
                    } else {
                        "Achievement caché"
                    };
                    let action =
                        button(text(if done { "Réinitialiser" } else { "Débloquer" }).size(12))
                            .on_press(Message::AskAchievementChange(
                                g.id.clone(),
                                vec![change(a, !done)],
                            ))
                            .style(button::text);
                    items.push(
                        row![
                            text(format!("{} {name}", if done { "✔" } else { "·" }))
                                .size(13)
                                .width(Length::Fill),
                            action
                        ]
                        .align_y(Alignment::Center)
                        .into(),
                    );
                }
            }
        }
        section("Achievements", items)
    }

    fn confirmation<'a>(&'a self, p: &'a PendingChange) -> Element<'a, Message> {
        let names: Vec<&str> = p.changes.iter().map(|c| c.name.as_str()).collect();
        let verb = if p.changes.iter().all(|c| c.unlock) {
            "Débloquer"
        } else if p.changes.iter().all(|c| !c.unlock) {
            "Réinitialiser"
        } else {
            "Modifier"
        };
        container(
            column![
                text(format!("{verb} {} achievement(s) sans jouer : {}", p.changes.len(), names.join(", ")))
                    .size(13),
                text(
                    "Le changement est fait directement sur votre profil GOG public, \
                     avec la date d'aujourd'hui. Il est probablement contraire aux conditions de GOG.",
                )
                .size(12),
                row![
                    button(text("Confirmer")).on_press(Message::ConfirmAchievementChange).style(button::danger),
                    button(text("Annuler")).on_press(Message::CancelAchievementChange).style(button::secondary),
                ]
                .spacing(8),
            ]
            .spacing(6),
        )
        .padding(8)
        .width(Length::Fill)
        .style(container::bordered_box)
        .into()
    }
}

fn change(a: &Achievement, unlock: bool) -> AchievementChange {
    AchievementChange {
        achievement_id: a.achievement_id.clone(),
        name: a.name.clone(),
        unlock,
    }
}

fn section<'a>(title: &'a str, items: Vec<Element<'a, Message>>) -> Element<'a, Message> {
    container(
        column![
            text(title).size(16),
            Column::with_children(items).spacing(4)
        ]
        .spacing(8),
    )
    .padding(10)
    .width(Length::Fill)
    .style(container::rounded_box)
    .into()
}

fn runner_label(install: &Install) -> String {
    match &install.runner {
        Runner::Native => "Lancement natif".into(),
        Runner::Umu { proton, .. } => format!(
            "Proton (umu) : {}",
            proton
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
        ),
        Runner::Wine { wine, .. } => format!("Wine : {}", wine.display()),
    }
}

fn local_time(ts: i64) -> String {
    use chrono::TimeZone;
    chrono::Local
        .timestamp_opt(ts, 0)
        .single()
        .map(|t| t.format("%d/%m/%Y %H:%M").to_string())
        .unwrap_or_default()
}

pub fn describe_warning(w: Warning) -> &'static str {
    match w {
        Warning::LocalRootMissing => {
            "dossier local absent : aucune suppression cloud ne sera faite"
        }
        Warning::LocalEmptyWithHistory => {
            "dossier local vide alors qu'il contenait des sauvegardes : suppressions bloquées"
        }
        Warning::RemoteEmptyWithHistory => {
            "cloud vide alors qu'il contenait des sauvegardes : suppressions locales bloquées"
        }
        Warning::RootChanged => {
            "le dossier de sauvegarde a changé : l'historique précédent est ignoré"
        }
    }
}

fn describe_cloud(prefix: &str, s: &CloudSummary) -> String {
    let mut out = format!(
        "{prefix} : {} envoyé(s), {} téléchargé(s)",
        s.uploaded, s.downloaded
    );
    if !s.conflicts.is_empty() {
        out += &format!(" ; conflits : {}", s.conflicts.join(", "));
    }
    if !s.problems.is_empty() {
        out += &format!(" ; problèmes : {}", s.problems.join(", "));
    }
    out
}

pub fn describe_play_event(e: &PlayEvent) -> String {
    match e {
        PlayEvent::CloudChecked(s) => describe_cloud("Cloud vérifié", s),
        PlayEvent::CloudSkipped(why) => {
            format!("Cloud non vérifié ({why}) ; les sauvegardes locales sont conservées.")
        }
        PlayEvent::Blocked(s) => format!(
            "{}. Lancement annulé : résolvez le conflit dans « Sauvegardes cloud ».",
            describe_cloud("Cloud à vérifier", s)
        ),
        PlayEvent::CometReady => {
            "Comet actif : les achievements obtenus en jeu sont transmis à GOG.".into()
        }
        PlayEvent::CometUnavailable(why) => {
            format!("Achievements indisponibles pour cette session : {why}")
        }
        PlayEvent::Started { pid } => format!("Jeu lancé (pid {pid})."),
        PlayEvent::LauncherExited { code } => {
            format!("Processus de lancement terminé ({code:?}) ; suivi des processus restants…")
        }
        PlayEvent::StopRequested => "Arrêt demandé…".into(),
        PlayEvent::Ended { seconds, clean, .. } => format!(
            "Session terminée après {} min{}.",
            seconds / 60,
            if *clean { "" } else { " (fin incertaine)" }
        ),
        PlayEvent::CloudUploaded(s) => describe_cloud("Cloud après la partie", s),
        PlayEvent::CloudUploadSkipped(why) => {
            format!("Cloud non synchronisé ({why}) ; sauvegardes locales conservées.")
        }
        PlayEvent::Unlocked(names) => {
            format!("Achievements enregistrés sur GOG : {}", names.join(", "))
        }
        PlayEvent::NoNewAchievement => "Aucun nouvel achievement enregistré sur GOG.".into(),
        PlayEvent::AchievementsUnknown => "Impossible de relire les achievements sur GOG.".into(),
    }
}
