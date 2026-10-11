//! Play time as GOG records it, and reporting of sessions played through SlattyLauncher, as GOG
//! Galaxy does after each game.

use reqwest::Client;
use rusqlite::params;
use serde::Deserialize;

use crate::auth::Tokens;
use crate::db::Db;
use crate::error::{Error, Result};
use crate::http;
use crate::paths::Dirs;

fn url(user_id: &str, game_id: &str) -> String {
    format!("https://gameplay.gog.com/games/{game_id}/users/{user_id}/sessions")
}

/// Minutes of play GOG recorded for a game, from every launcher that reports sessions.
pub async fn total_minutes(http: &Client, tokens: &Tokens, game_id: &str) -> Result<u64> {
    #[derive(Deserialize)]
    struct Sessions {
        #[serde(default)]
        time_sum: u64,
    }
    let s: Sessions = http::json(
        http.get(url(&tokens.user_id, game_id))
            .bearer_auth(tokens.access_token.expose()),
        "fetching play time",
    )
    .await?;
    Ok(s.time_sum)
}

/// Adds one session to the game's play time on GOG.
pub async fn report(
    http: &Client,
    tokens: &Tokens,
    game_id: &str,
    started_at: i64,
    minutes: i64,
) -> Result<()> {
    let req = http
        .post(url(&tokens.user_id, game_id))
        .bearer_auth(tokens.access_token.expose())
        .json(&serde_json::json!({ "session_date": started_at, "time": minutes }));
    http::send(req, "reporting play time").await.map(drop)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unreported {
    pub id: i64,
    pub game_id: String,
    pub started_at: i64,
    pub minutes: i64,
}

/// Sessions of this account that ended cleanly, lasted at least a minute (GOG ignores shorter
/// ones) and were not sent yet. A game id that could change the address the session is sent to,
/// with the account's token, is left out.
pub fn unreported(db: &Db, user_id: &str) -> Result<Vec<Unreported>> {
    let conn = db.conn();
    let mut stmt = conn.prepare(
        "SELECT id, game_id, started_at, (ended_at - started_at) / 60 FROM sessions
         WHERE state = 'ended' AND reported = 0 AND user_id = ?1
           AND ended_at - started_at >= 60
         ORDER BY id",
    )?;
    let rows = stmt.query_map([user_id], |r| {
        Ok(Unreported {
            id: r.get(0)?,
            game_id: r.get(1)?,
            started_at: r.get(2)?,
            minutes: r.get(3)?,
        })
    })?;
    let rows = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows
        .into_iter()
        .filter(|s| crate::paths::check_game_id(&s.game_id).is_ok())
        .collect())
}

fn mark_reported(db: &Db, id: i64) -> Result<()> {
    db.conn().execute(
        "UPDATE sessions SET reported = 1 WHERE id = ?1",
        params![id],
    )?;
    Ok(())
}

/// Marks the pending sessions as never to be sent: play time reporting is off, and turning it on
/// again sends only the sessions played after.
pub fn keep_private(db: &Db, user_id: &str) -> Result<()> {
    db.conn().execute(
        "UPDATE sessions SET reported = 2 WHERE state = 'ended' AND reported = 0 AND user_id = ?1",
        params![user_id],
    )?;
    Ok(())
}

/// Sends every pending session; stops at the first failure so the rest is retried later, except a
/// session GOG refuses for good, which is skipped. Returns the minutes reported.
pub async fn report_pending(db: &Db, dirs: &Dirs, http: &Client, tokens: &Tokens) -> Result<i64> {
    send_pending(db, dirs, &tokens.user_id, |s| async move {
        report(http, tokens, &s.game_id, s.started_at, s.minutes).await
    })
    .await
}

async fn send_pending<F: Future<Output = Result<()>>>(
    db: &Db,
    dirs: &Dirs,
    user_id: &str,
    send: impl Fn(Unreported) -> F,
) -> Result<i64> {
    // One process at a time, the sessions read under the lock: two games ending together, one from
    // the interface and one from the command line, would otherwise send the same sessions twice.
    let _sending = crate::lock::acquire(
        &dirs.locks().join(format!("playtime-{user_id}.lock")),
        std::time::Duration::from_secs(60),
    )
    .await?;
    let mut minutes = 0;
    for s in unreported(db, user_id)? {
        let (id, played) = (s.id, s.minutes);
        match send(s).await {
            Ok(()) => {
                mark_reported(db, id)?;
                minutes += played;
            }
            // Refused for good (not a session or token GOG may accept later): sending it again
            // would hold back every session after it.
            Err(Error::Http { status, .. })
                if (400..500).contains(&status) && ![401, 408, 429].contains(&status) =>
            {
                tracing::warn!("play time of session {id} refused by GOG (HTTP {status})");
                db.conn().execute(
                    "UPDATE sessions SET reported = 3 WHERE id = ?1",
                    params![id],
                )?;
            }
            Err(e) => return Err(e),
        }
    }
    Ok(minutes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_finished_sessions_of_a_minute_or_more_of_this_account_are_pending() {
        let db = Db::in_memory().unwrap();
        db.conn()
            .execute_batch(
                "INSERT INTO sessions (id, game_id, user_id, started_at, ended_at, state, reported) VALUES
                   (1, 'g', 'me', 1000, 1600, 'ended', 0),
                   (2, 'g', 'me', 2000, 2030, 'ended', 0),
                   (3, 'g', 'me', 3000, 3600, 'lost', 0),
                   (4, 'g', 'me', 4000, 4600, 'ended', 1),
                   (5, 'g', 'other', 5000, 5600, 'ended', 0),
                   (6, 'g', 'me', 6000, NULL, 'running', 0);",
            )
            .unwrap();
        assert_eq!(
            unreported(&db, "me").unwrap(),
            [Unreported {
                id: 1,
                game_id: "g".into(),
                started_at: 1000,
                minutes: 10
            }]
        );
        mark_reported(&db, 1).unwrap();
        assert!(unreported(&db, "me").unwrap().is_empty());
    }

    #[test]
    fn a_session_whose_game_id_would_change_the_address_is_never_sent() {
        let db = Db::in_memory().unwrap();
        db.conn()
            .execute_batch(
                "INSERT INTO sessions (id, game_id, user_id, started_at, ended_at, state, reported) VALUES
                   (1, '../../users/x', 'me', 1000, 1600, 'ended', 0),
                   (2, '1?a=b', 'me', 2000, 2600, 'ended', 0),
                   (3, '1000000001', 'me', 3000, 3600, 'ended', 0);",
            )
            .unwrap();
        let pending = unreported(&db, "me").unwrap();
        assert_eq!(pending.iter().map(|s| s.id).collect::<Vec<_>>(), [3]);
    }

    /// A session GOG refuses for good does not hold back the ones after it, and is not sent again;
    /// a network error or a server failing for now stops, so that the rest is sent later.
    #[tokio::test]
    async fn a_session_gog_refuses_does_not_hold_back_the_others() {
        let db = Db::in_memory().unwrap();
        db.conn()
            .execute_batch(
                "INSERT INTO sessions (id, game_id, user_id, started_at, ended_at, state, reported) VALUES
                   (1, '1', 'me', 1000, 1600, 'ended', 0),
                   (2, '2', 'me', 2000, 2600, 'ended', 0),
                   (3, '3', 'me', 3000, 3600, 'ended', 0);",
            )
            .unwrap();
        let answer = |status: u16| {
            move |s: Unreported| {
                let refused = s.game_id == "1";
                async move {
                    if refused {
                        Err(crate::error::Error::Http {
                            context: "test",
                            status,
                        })
                    } else {
                        Ok(())
                    }
                }
            }
        };
        let dirs = Dirs::under(&std::env::temp_dir().join("slatty-playtime-unused"));
        assert!(send_pending(&db, &dirs, "me", answer(503)).await.is_err());
        assert_eq!(unreported(&db, "me").unwrap().len(), 3, "tried again later");
        assert_eq!(
            send_pending(&db, &dirs, "me", answer(400)).await.unwrap(),
            20
        );
        assert!(unreported(&db, "me").unwrap().is_empty());
    }

    /// The interface and the command line, each at the end of a game, send the sessions waiting
    /// one after the other: none is added to the play time on GOG twice.
    #[tokio::test]
    async fn two_processes_sending_at_once_send_each_session_once() {
        let root = std::env::temp_dir().join(format!("slatty-playtime-{}", std::process::id()));
        let dirs = Dirs::under(&root);
        let db = Db::in_memory().unwrap();
        db.conn()
            .execute_batch(
                "INSERT INTO sessions (id, game_id, user_id, started_at, ended_at, state, reported) VALUES
                   (1, '1', 'me', 1000, 1600, 'ended', 0),
                   (2, '2', 'me', 2000, 2600, 'ended', 0);",
            )
            .unwrap();
        let sent = std::sync::atomic::AtomicUsize::new(0);
        let send = |_: Unreported| {
            let sent = &sent;
            async move {
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                sent.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Ok(())
            }
        };
        let (a, b) = tokio::join!(
            send_pending(&db, &dirs, "me", send),
            send_pending(&db, &dirs, "me", send)
        );
        assert_eq!(a.unwrap() + b.unwrap(), 20);
        assert_eq!(sent.load(std::sync::atomic::Ordering::SeqCst), 2);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn sessions_played_while_reporting_is_off_are_never_sent() {
        let db = Db::in_memory().unwrap();
        let session = |id: i64| {
            db.conn()
                .execute(
                    "INSERT INTO sessions (id, game_id, user_id, started_at, ended_at, state, reported)
                     VALUES (?1, 'g', 'me', ?2, ?2 + 600, 'ended', 0)",
                    params![id, id * 1000],
                )
                .unwrap();
        };
        session(1);
        keep_private(&db, "me").unwrap();
        assert!(unreported(&db, "me").unwrap().is_empty());
        // Turned on again: only what is played after goes out.
        session(2);
        let pending = unreported(&db, "me").unwrap();
        assert_eq!(pending.iter().map(|s| s.id).collect::<Vec<_>>(), [2]);
    }
}
