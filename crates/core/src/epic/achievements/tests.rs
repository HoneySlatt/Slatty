use std::sync::Mutex;

use super::*;

/// Epic's GraphQL API in memory: an answer by query, and the queries asked.
struct Fake {
    definitions: Value,
    progress: Value,
    asked: Mutex<Vec<&'static str>>,
}

impl Graphql for Fake {
    async fn query(&self, query: &'static str, _: Value) -> Result<Value> {
        self.asked.lock().unwrap().push(query);
        Ok(match query {
            DEFINITIONS => self.definitions.clone(),
            _ => self.progress.clone(),
        })
    }
}

fn definition(name: &str, hidden: bool, rarity: Option<f32>) -> Value {
    json!({ "achievement": {
        "name": name,
        "hidden": hidden,
        "unlockedDisplayName": format!("[FAKE] {name}"),
        "lockedDisplayName": if hidden { "[FAKE] Hidden" } else { "" },
        "unlockedDescription": format!("[FAKE] Did {name}"),
        "lockedDescription": "",
        "unlockedIconLink": format!("https://cdn.example/{name}.png"),
        "lockedIconLink": format!("https://cdn.example/{name}-locked.png"),
        "rarity": rarity.map(|p| json!({ "percent": p })),
    }})
}

/// Epic's definitions, in its order, with the account's progress: unlocked ones dated as Slatty
/// keeps dates, hidden ones not visible, an unknown rarity negative.
#[tokio::test]
async fn definitions_and_progress_make_the_achievements() {
    let fake = Fake {
        definitions: json!({ "Achievement": { "productAchievementsRecordBySandbox": {
            "achievements": [
                definition("first", false, Some(85.5)),
                definition("secret", true, None),
                definition("last", false, Some(1.0)),
            ]
        }}}),
        progress: json!({ "PlayerAchievement": { "playerAchievementGameRecordsBySandbox": {
            "records": [{ "playerAchievements": [
                { "playerAchievement": {
                    "achievementName": "first", "unlocked": true,
                    "unlockDate": "2026-10-01T12:34:56.789Z" } },
                { "playerAchievement": {
                    "achievementName": "last", "unlocked": false, "unlockDate": null } },
                { "playerAchievement": {
                    "achievementName": "secret", "unlocked": true, "unlockDate": "[FAKE]" } },
            ]}]
        }}}),
        asked: Mutex::new(Vec::new()),
    };
    let list = fetch(&fake, "fakeaccount", "fakens", "fr").await.unwrap();
    let keys: Vec<&str> = list.iter().map(|a| a.achievement_key.as_str()).collect();
    assert_eq!(keys, ["first", "secret", "last"]);
    assert_eq!(
        list[0].date_unlocked.as_deref(),
        Some("2026-10-01T12:34:56+0000")
    );
    assert_eq!(list[0].name, "[FAKE] first");
    assert_eq!(list[0].rarity, 85.5);
    assert!(list[0].visible);
    assert_eq!(
        list[1].date_unlocked.as_deref(),
        Some(""),
        "unlocked, its date unreadable"
    );
    assert!(!list[1].visible);
    assert_eq!(list[1].rarity, -1.0);
    assert_eq!(list[2].date_unlocked, None);
    assert_eq!(
        list[2].image_url_locked,
        "https://cdn.example/last-locked.png"
    );
}

/// Fields Epic sends as `null` read as having no value, rather than failing the whole game.
#[tokio::test]
async fn null_fields_read_as_empty() {
    let fake = Fake {
        definitions: json!({ "Achievement": { "productAchievementsRecordBySandbox": {
            "achievements": [{ "achievement": {
                "name": "first", "hidden": null,
                "unlockedDisplayName": "[FAKE] First", "lockedDisplayName": null,
                "unlockedDescription": null, "lockedDescription": null,
                "unlockedIconLink": null, "lockedIconLink": null, "rarity": null,
            }}]
        }}}),
        progress: json!({ "PlayerAchievement": { "playerAchievementGameRecordsBySandbox": {
            "records": [{ "playerAchievements": [{ "playerAchievement": {
                "achievementName": "first", "unlocked": null, "unlockDate": null } }] }]
        }}}),
        asked: Mutex::new(Vec::new()),
    };
    let list = fetch(&fake, "fakeaccount", "fakens", "en").await.unwrap();
    assert_eq!(list[0].name, "[FAKE] First");
    assert_eq!(list[0].description, "");
    assert!(list[0].visible);
    assert_eq!(list[0].date_unlocked, None);
}

/// An answer saying something went wrong is an error, even beside data: what Epic could not read
/// would show as no achievements, or none unlocked, and stay so in the cache.
#[test]
fn an_answer_with_errors_is_an_error() {
    let partial = json!({
        "data": { "PlayerAchievement": { "playerAchievementGameRecordsBySandbox": null } },
        "errors": [{ "message": "[FAKE] timed out" }]
    });
    assert!(matches!(data_of(partial), Err(Error::Parse { .. })));
    let fine = json!({ "data": { "Achievement": {} } });
    assert!(data_of(fine).is_ok());
    assert!(data_of(json!({ "data": null })).is_err());
}

/// A game without achievements: none, and the account's progress is not asked for.
#[tokio::test]
async fn a_game_without_achievements_has_none() {
    let fake = Fake {
        definitions: json!({ "Achievement": { "productAchievementsRecordBySandbox": null } }),
        progress: Value::Null,
        asked: Mutex::new(Vec::new()),
    };
    assert!(
        fetch(&fake, "fakeaccount", "fakens", "en")
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(*fake.asked.lock().unwrap(), [DEFINITIONS]);
}

/// The counts kept for an account are read back as written; none before any was kept.
#[test]
fn counts_are_kept_per_account() {
    let root = std::env::temp_dir().join(format!("slatty-epic-counts-{}", std::process::id()));
    let dirs = Dirs::under(&root);
    let account = "0123456789abcdef0123456789abcdef";
    assert!(load_counts(&dirs, account).unwrap().is_empty());
    let counts = Counts::from([
        ("epic-FakeA".to_string(), Some((3, 10))),
        ("epic-FakeB".to_string(), None),
    ]);
    save_counts(&dirs, account, &counts).unwrap();
    assert_eq!(load_counts(&dirs, account).unwrap(), counts);
    assert!(load_counts(&dirs, "../escape").is_err());
    std::fs::remove_dir_all(&root).unwrap();
}

/// A query Epic refuses says why: its messages make the error.
#[test]
fn epics_reasons_for_refusing_a_query_are_kept() {
    assert_eq!(
        refused(br#"{"errors":[{"message":"[FAKE] one"},{"message":"[FAKE] two"}]}"#).as_deref(),
        Some("[FAKE] one; [FAKE] two")
    );
    assert_eq!(refused(b"<html>[FAKE]</html>"), None);
    assert_eq!(refused(br#"{"data":{}}"#), None);
}
