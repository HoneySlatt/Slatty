//! A Steam game's achievements as the Steam client keeps them for the account, read and never
//! changed: their names, descriptions and icons from `appcache/stats/UserGameStatsSchema_<app>.bin`,
//! which are unlocked and when from `appcache/stats/UserGameStats_<account>_<app>.bin` (binary
//! KeyValues, as Steam Achievement Manager reads them), and how common some of them are from the
//! client's library cache (`userdata/<account>/config/librarycache/<app>.json`). The client writes
//! these files as it shows or runs a game: a game it never did has none.

use std::collections::HashMap;
use std::path::Path;

use super::local::{Steam, SteamUser};
use super::vdf::Vdf;
use crate::achievements::Achievement;

/// The largest schema seen holds every language in 3.5 MB; larger is not Steam's.
const MAX_FILE: u64 = 64 << 20;
/// The library cache's files are a few tens of KB.
const MAX_CACHE_FILE: u64 = 4 << 20;
/// Where Steam's image servers keep a game's achievement icons.
const ICONS: &str = "https://shared.steamstatic.com/community_assets/images/apps/";
/// More achievements than any game has: larger counts are cut to it, so that comparing shares
/// (products of counts) cannot overflow.
const MAX_ACHIEVEMENTS: u64 = 100_000;

/// The achievements of `app_id` for the account, as the client knows them; none when it keeps no
/// schema for the game. With `icons`, each names its icons on Steam's image servers. How common
/// one is, in percent, is negative when the client's cache does not say.
pub fn achievements(
    steam: &Steam,
    user: &SteamUser,
    app_id: u32,
    icons: bool,
) -> Option<Vec<Achievement>> {
    let stats = steam.root.join("appcache/stats");
    let schema = read(&stats.join(format!("UserGameStatsSchema_{app_id}.bin")))?;
    let mine = read(&stats.join(format!("UserGameStats_{}_{app_id}.bin", user.account_id())));
    let cache = mine.as_ref().and_then(|m| m.get("cache"));
    let rarity = rarity(library_cache(steam, user, app_id).as_ref());
    let game = schema.entries().first().map(|(_, game)| game)?;
    let mut list = Vec::new();
    for (stat_id, stat) in game.get("stats").map_or(&[][..], Vdf::entries) {
        let Some(bits) = stat.get("bits") else {
            continue;
        };
        let mine = cache.and_then(|c| c.get(stat_id));
        let data = mine
            .and_then(|s| s.text("data"))
            .and_then(|d| d.parse::<i64>().ok())
            .unwrap_or(0) as u32;
        let times = mine.and_then(|s| s.get("AchievementTimes"));
        for (bit, a) in bits.entries() {
            let (Ok(bit), Some(key)) = (bit.parse::<u32>(), a.text("name")) else {
                continue;
            };
            let display = a.get("display");
            let text = |field| {
                display
                    .and_then(|d| d.get(field))
                    .map(localized)
                    .unwrap_or_default()
            };
            let icon = |field| {
                display
                    .and_then(|d| d.text(field))
                    .filter(|f| icons && safe_name(f))
                    .map(|f| format!("{ICONS}{app_id}/{f}"))
                    .unwrap_or_default()
            };
            let unlocked = bit < 32 && data & (1 << bit) != 0;
            let at = times
                .and_then(|t| t.text(&bit.to_string()))
                .and_then(|t| t.parse::<i64>().ok())
                .filter(|t| *t > 0);
            list.push(Achievement {
                achievement_id: key.to_string(),
                achievement_key: key.to_string(),
                name: text("name"),
                description: text("desc"),
                visible: display.and_then(|d| d.text("hidden")) != Some("1"),
                date_unlocked: unlocked.then(|| at.map(date).unwrap_or_default()),
                rarity: rarity.get(key).copied().unwrap_or(-1.0),
                image_url_unlocked: icon("icon"),
                image_url_locked: icon("icon_gray"),
            });
        }
    }
    Some(list)
}

/// Unlocked and total achievements of each game of `apps` that has any, by app id: from the
/// client's library cache, else from its stats files.
pub fn counts(steam: &Steam, user: &SteamUser, apps: &[u32]) -> HashMap<u32, (usize, usize)> {
    apps.iter()
        .filter_map(|&app| {
            let cached = library_cache(steam, user, app).and_then(|c| {
                let n = |key: &str| c[key].as_u64().map(|n| n.min(MAX_ACHIEVEMENTS) as usize);
                let (done, total) = (n("nAchieved")?, n("nTotal")?);
                Some((done.min(total), total)).filter(|(_, total)| *total > 0)
            });
            let counted = cached
                .or_else(|| crate::overview::counts(&achievements(steam, user, app, false)?))?;
            Some((app, counted))
        })
        .collect()
}

/// What the client's library cache keeps of a game's achievements (`data` of its `achievements`).
fn library_cache(steam: &Steam, user: &SteamUser, app_id: u32) -> Option<serde_json::Value> {
    let path = steam
        .root
        .join("userdata")
        .join(user.account_id().to_string())
        .join("config/librarycache")
        .join(format!("{app_id}.json"));
    let file = super::open(&path).ok()?;
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(&mut std::io::Read::take(file, MAX_CACHE_FILE), &mut bytes).ok()?;
    let all: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    all.as_array()?
        .iter()
        .find(|section| section[0] == "achievements")
        .map(|section| section[1]["data"].clone())
}

/// How common the achievements the library cache shows are, in percent, by key: a dozen of those
/// unlocked and a dozen of the others at most.
fn rarity(cache: Option<&serde_json::Value>) -> HashMap<String, f32> {
    let Some(cache) = cache else {
        return HashMap::new();
    };
    ["vecHighlight", "vecUnachieved", "vecAchievedHidden"]
        .into_iter()
        .filter_map(|list| cache[list].as_array())
        .flatten()
        .filter_map(|a| {
            let share = a["flAchieved"]
                .as_f64()
                .filter(|s| (0.0..=100.0).contains(s))?;
            Some((a["strID"].as_str()?.to_string(), share as f32))
        })
        .collect()
}

/// A text the schema gives in each language: English, else the first one given.
fn localized(value: &Vdf) -> String {
    match value {
        Vdf::Text(text) => text.clone(),
        Vdf::Section(_) => value
            .text("english")
            .or_else(|| {
                value.entries().iter().find_map(|(k, v)| match v {
                    Vdf::Text(t) if !k.eq_ignore_ascii_case("token") => Some(t.as_str()),
                    _ => None,
                })
            })
            .unwrap_or_default()
            .to_string(),
    }
}

/// An icon's file name, fit for an address: letters, digits and `._-`.
fn safe_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}

/// A date as GOG writes the time an achievement was unlocked.
fn date(seconds: i64) -> String {
    chrono::DateTime::from_timestamp(seconds, 0)
        .map(|t| t.format("%Y-%m-%dT%H:%M:%S+0000").to_string())
        .unwrap_or_default()
}

fn read(path: &Path) -> Option<Vdf> {
    let file = super::open(path).ok()?;
    let mut data = Vec::new();
    std::io::Read::read_to_end(&mut std::io::Read::take(file, MAX_FILE), &mut data).ok()?;
    super::appinfo::keyvalues(&data)
        .inspect_err(|e| tracing::warn!("{}: {e}", path.display()))
        .ok()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::steam::local::tests::FakeSteam;

    /// A value of binary KeyValues.
    pub(crate) enum Kv {
        Text(&'static str),
        Int(i32),
        Section(Vec<(String, Kv)>),
    }

    pub(crate) fn section(entries: Vec<(&str, Kv)>) -> Kv {
        Kv::Section(
            entries
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
        )
    }

    /// Binary KeyValues with their keys written in place, as the client's stats files are.
    pub(crate) fn encode(root: &[(String, Kv)]) -> Vec<u8> {
        fn entries(out: &mut Vec<u8>, all: &[(String, Kv)]) {
            for (key, value) in all {
                let kind = match value {
                    Kv::Section(_) => 0x00,
                    Kv::Text(_) => 0x01,
                    Kv::Int(_) => 0x02,
                };
                out.push(kind);
                out.extend(key.as_bytes());
                out.push(0);
                match value {
                    Kv::Section(inner) => entries(out, inner),
                    Kv::Text(t) => {
                        out.extend(t.as_bytes());
                        out.push(0);
                    }
                    Kv::Int(n) => out.extend(n.to_le_bytes()),
                }
            }
            out.push(0x08);
        }
        let mut out = Vec::new();
        entries(&mut out, root);
        out
    }

    fn achievement(key: &'static str, name: &'static str, hidden: i32) -> Kv {
        section(vec![
            ("name", Kv::Text(key)),
            (
                "display",
                section(vec![
                    (
                        "name",
                        section(vec![
                            ("french", Kv::Text("[FAKE] en français")),
                            ("english", Kv::Text(name)),
                            ("token", Kv::Text("NEW_ACHIEVEMENT_1_0_NAME")),
                        ]),
                    ),
                    (
                        "desc",
                        section(vec![
                            ("token", Kv::Text("NEW_ACHIEVEMENT_1_0_DESC")),
                            ("german", Kv::Text("[FAKE] Beschreibung")),
                        ]),
                    ),
                    ("hidden", Kv::Int(hidden)),
                    ("icon", Kv::Text("0123abcd.jpg")),
                    ("icon_gray", Kv::Text("../../gray.jpg")),
                ]),
            ),
        ])
    }

    /// Writes the stats files of app 70 for the fake account: three achievements in a stat of
    /// bits, two of them unlocked (one with its time), beside a stat that is a number.
    pub(crate) fn write_stats(fake: &FakeSteam) {
        let stats = fake.steam.root.join("appcache/stats");
        std::fs::create_dir_all(&stats).unwrap();
        let schema = vec![(
            "70".to_string(),
            section(vec![
                ("gamename", Kv::Text("[FAKE] Life")),
                (
                    "stats",
                    section(vec![
                        (
                            "1",
                            section(vec![
                                ("type", Kv::Text("ACHIEVEMENTS")),
                                (
                                    "bits",
                                    section(vec![
                                        ("0", achievement("ACH_WAKE", "[FAKE] Wake up", 0)),
                                        ("1", achievement("ACH_SECRET", "[FAKE] Secret", 1)),
                                        ("2", achievement("ACH_LAST", "[FAKE] Last", 0)),
                                    ]),
                                ),
                            ]),
                        ),
                        (
                            "2",
                            section(vec![("type", Kv::Text("INT")), ("name", Kv::Text("kills"))]),
                        ),
                    ]),
                ),
            ]),
        )];
        std::fs::write(stats.join("UserGameStatsSchema_70.bin"), encode(&schema)).unwrap();
        let mine = vec![(
            "cache".to_string(),
            section(vec![
                ("crc", Kv::Int(1)),
                (
                    "1",
                    section(vec![
                        ("data", Kv::Int(0b011)),
                        (
                            "AchievementTimes",
                            section(vec![("0", Kv::Int(1_760_000_000))]),
                        ),
                    ]),
                ),
                ("2", section(vec![("data", Kv::Int(42))])),
            ]),
        )];
        std::fs::write(stats.join("UserGameStats_39734274_70.bin"), encode(&mine)).unwrap();
    }

    #[test]
    fn achievements_are_read_from_the_clients_stats_files() {
        let fake = FakeSteam::new("stats");
        write_stats(&fake);
        let cache = fake
            .steam
            .root
            .join("userdata/39734274/config/librarycache/70.json");
        std::fs::write(
            &cache,
            r#"[["achievements",{"version":1,"data":{"nAchieved":0,"nTotal":0,
                "vecHighlight":[{"strID":"ACH_WAKE","flAchieved":61.5}],
                "vecUnachieved":[{"strID":"ACH_LAST","flAchieved":250.0}],
                "vecAchievedHidden":[]}}]]"#,
        )
        .unwrap();
        let user = crate::steam::local::signed_in_user(&fake.steam).unwrap();

        let list = achievements(&fake.steam, &user, 70, false).unwrap();
        let keys: Vec<&str> = list.iter().map(|a| a.achievement_key.as_str()).collect();
        assert_eq!(
            keys,
            ["ACH_WAKE", "ACH_SECRET", "ACH_LAST"],
            "the stat of bits alone"
        );
        let (wake, secret, last) = (&list[0], &list[1], &list[2]);
        assert_eq!(wake.name, "[FAKE] Wake up", "in English");
        assert_eq!(wake.description, "[FAKE] Beschreibung", "else as given");
        assert_eq!(
            wake.date_unlocked.as_deref(),
            Some("2025-10-09T08:53:20+0000")
        );
        assert_eq!(
            secret.date_unlocked.as_deref(),
            Some(""),
            "unlocked, time unknown"
        );
        assert!(!secret.visible && wake.visible);
        assert_eq!(last.date_unlocked, None);
        assert_eq!(wake.rarity, 61.5);
        assert!(last.rarity < 0.0 && secret.rarity < 0.0, "not known");
        assert!(wake.image_url_unlocked.is_empty(), "no icons to download");

        let list = achievements(&fake.steam, &user, 70, true).unwrap();
        assert_eq!(
            list[0].image_url_unlocked,
            "https://shared.steamstatic.com/community_assets/images/apps/70/0123abcd.jpg"
        );
        assert!(
            list[0].image_url_locked.is_empty(),
            "a name that climbs out"
        );
        assert!(achievements(&fake.steam, &user, 440, false).is_none());

        // Counted from the stats files, the library cache knowing none.
        assert_eq!(
            counts(&fake.steam, &user, &[70, 440]),
            HashMap::from([(70, (2, 3))])
        );
        std::fs::write(
            &cache,
            r#"[["achievements",{"version":1,"data":{"nAchieved":5,"nTotal":9}}]]"#,
        )
        .unwrap();
        assert_eq!(
            counts(&fake.steam, &user, &[70]),
            HashMap::from([(70, (5, 9))])
        );
        std::fs::write(
            &cache,
            r#"[["achievements",{"version":1,"data":{"nAchieved":18446744073709551615,"nTotal":4}}]]"#,
        )
        .unwrap();
        assert_eq!(
            counts(&fake.steam, &user, &[70]),
            HashMap::from([(70, (4, 4))]),
            "never more unlocked than there are"
        );
    }

    #[test]
    fn damaged_stats_files_are_left_out() {
        let fake = FakeSteam::new("stats-damaged");
        write_stats(&fake);
        let user = crate::steam::local::signed_in_user(&fake.steam).unwrap();
        let schema = fake
            .steam
            .root
            .join("appcache/stats/UserGameStatsSchema_70.bin");
        let whole = std::fs::read(&schema).unwrap();
        for cut in [0, 1, 5, whole.len() / 2, whole.len() - 1] {
            std::fs::write(&schema, &whole[..cut]).unwrap();
            assert!(
                achievements(&fake.steam, &user, 70, false).is_none(),
                "cut at {cut}"
            );
        }
        // Deeper than any Steam file.
        let mut deep = Kv::Text("x");
        for _ in 0..40 {
            deep = section(vec![("a", deep)]);
        }
        std::fs::write(&schema, encode(&[("70".to_string(), deep)])).unwrap();
        assert!(achievements(&fake.steam, &user, 70, false).is_none());
        // The account's file damaged: nothing unlocked, rather than a guess.
        std::fs::write(&schema, &whole).unwrap();
        std::fs::write(
            fake.steam
                .root
                .join("appcache/stats/UserGameStats_39734274_70.bin"),
            [0x00, b'c'],
        )
        .unwrap();
        let list = achievements(&fake.steam, &user, 70, false).unwrap();
        assert!(list.iter().all(|a| a.date_unlocked.is_none()));
    }
}
