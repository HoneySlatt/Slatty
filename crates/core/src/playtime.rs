//! Play time as GOG records it, and reporting of sessions played through SlattyLauncher, as GOG
//! Galaxy does after each game.

use reqwest::Client;
use rusqlite::params;
use serde::Deserialize;

use crate::auth::Tokens;
use crate::db::Db;
use crate::error::Result;
use crate::http;
use crate::store::Store;

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

/// Sessions of GOG games played with this account that ended cleanly, lasted at least a minute
/// (GOG ignores shorter ones) and were not sent yet.
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
        .filter(|s| crate::store::of(&s.game_id).ok() == Some(Store::Gog))
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

/// Sends every pending session; stops at the first failure so the rest is retried later.
/// Returns the minutes reported.
pub async fn report_pending(db: &Db, http: &Client, tokens: &Tokens) -> Result<i64> {
    let mut minutes = 0;
    for s in unreported(db, &tokens.user_id)? {
        report(http, tokens, &s.game_id, s.started_at, s.minutes).await?;
        mark_reported(db, s.id)?;
        minutes += s.minutes;
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
    fn sessions_of_steam_games_are_never_sent_to_gog() {
        let db = Db::in_memory().unwrap();
        db.conn()
            .execute_batch(
                "INSERT INTO sessions (id, game_id, user_id, started_at, ended_at, state, reported) VALUES
                   (1, 'steam-440', 'me', 1000, 1600, 'ended', 0),
                   (2, '1456487183', 'me', 2000, 2600, 'ended', 0);",
            )
            .unwrap();
        let pending = unreported(&db, "me").unwrap();
        assert_eq!(pending.iter().map(|s| s.id).collect::<Vec<_>>(), [2]);
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
