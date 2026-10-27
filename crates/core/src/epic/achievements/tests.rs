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
