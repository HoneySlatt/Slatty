//! Text shown to the user: sizes, durations, dates and descriptions of core events.

use slatty_core::achievements::Unlock;
use slatty_core::cloud::plan::Warning;
use slatty_core::installer::Progress;
use slatty_core::play::{CloudSummary, PlayEvent};

pub fn fraction(p: Progress) -> f32 {
    if p.bytes_total == 0 {
        0.0
    } else {
        p.bytes_done as f32 / p.bytes_total as f32
    }
}

/// "1.2 GiB / 3.00 GiB"; before the files are listed, when the total is not known yet, what is
/// being done instead of "0 KiB / 0 KiB".
pub fn amount(p: Progress) -> String {
    if p.bytes_total == 0 {
        return "Listing files…".into();
    }
    format!(
        "{} / {}",
        human_size(p.bytes_done),
        human_size(p.bytes_total)
    )
}

pub fn human_size(bytes: u64) -> String {
    slatty_core::units::size(bytes)
}

/// Where an install or update Steam makes is: "Downloading 45%" (`progress`, done of), paused, or
/// waiting; `writing` when Steam writes it but its size is not known yet.
pub fn steam_download(
    u: &slatty_core::steam::local::Update,
    progress: Option<(u64, u64)>,
    writing: bool,
    waiting: bool,
) -> String {
    match progress {
        _ if u.paused => "Paused in Steam".into(),
        _ if waiting => "Queued in Steam".into(),
        Some((done, of)) => format!("Downloading {:.0}%", done as f64 * 100.0 / of as f64),
        None if writing => "Downloading".into(),
        None => "Queued in Steam".into(),
    }
}

pub fn duration(seconds: i64) -> String {
    let minutes = seconds / 60;
    match (minutes / 60, minutes % 60) {
        (0, m) => format!("{m} m"),
        (h, m) => format!("{h} h {m} m"),
    }
}

/// The day dates are told from: always 10 October 2026 in tests, so that what they show, and
/// their snapshots, do not change from one day to the next.
fn today() -> chrono::NaiveDate {
    if cfg!(test) {
        chrono::NaiveDate::from_ymd_opt(2026, 10, 10).expect("a valid date")
    } else {
        chrono::Local::now().date_naive()
    }
}

/// A moment in the local time zone: in tests always UTC+2 (Paris in summer), so that the times and
/// days they show do not depend on the computer's zone.
fn local(ts: i64) -> Option<chrono::DateTime<chrono::FixedOffset>> {
    use chrono::TimeZone;
    if cfg!(test) {
        chrono::FixedOffset::east_opt(2 * 3600).and_then(|zone| zone.timestamp_opt(ts, 0).single())
    } else {
        chrono::Local
            .timestamp_opt(ts, 0)
            .single()
            .map(|t| t.fixed_offset())
    }
}

/// A path as shown. In tests their throwaway folder, named differently each run, shows as
/// `/tmp/slatty-test`, so that the snapshots showing it do not change from one run to the next.
pub fn shown_path(path: &std::path::Path) -> String {
    #[cfg(test)]
    if let Ok(rest) = path.strip_prefix(crate::tests::root()) {
        return std::path::Path::new("/tmp/slatty-test")
            .join(rest)
            .display()
            .to_string();
    }
    path.display().to_string()
}

/// "Today", "Yesterday", "3 days ago", or the date.
pub fn relative_day(ts: i64) -> String {
    let Some(then) = local(ts) else {
        return String::new();
    };
    match (today() - then.date_naive()).num_days() {
        0 => "Today".into(),
        1 => "Yesterday".into(),
        d @ 2..=6 => format!("{d} days ago"),
        _ => then.format("%-d %b %Y").to_string(),
    }
}

pub fn local_time(ts: i64) -> String {
    local(ts)
        .map(|t| t.format("%d/%m/%Y %H:%M").to_string())
        .unwrap_or_default()
}

/// A language code of GOG's builds, named for a pick list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Language(pub String);

impl std::fmt::Display for Language {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&language_name(&self.0))
    }
}

/// "French" for "fr-FR"; the code itself when it is not known.
pub fn language_name(code: &str) -> String {
    let name = match code.to_ascii_lowercase().as_str() {
        // Builds without language packs: the game holds its languages and offers them itself.
        "*" => "Chosen in the game",
        "en-us" | "en" => "English",
        "en-gb" => "English (UK)",
        "fr-fr" | "fr" => "French",
        "de-de" | "de" => "German",
        "es-es" | "es" => "Spanish",
        "es-mx" => "Spanish (Latin America)",
        "it-it" | "it" => "Italian",
        "pl-pl" | "pl" => "Polish",
        "ru-ru" | "ru" => "Russian",
        "pt-br" => "Portuguese (Brazil)",
        "pt-pt" | "pt" => "Portuguese",
        "zh-hans" | "zh-cn" => "Chinese (Simplified)",
        "zh-hant" | "zh-tw" => "Chinese (Traditional)",
        "ja-jp" | "ja" => "Japanese",
        "ko-kr" | "ko" => "Korean",
        "cs-cz" | "cs" => "Czech",
        "hu-hu" | "hu" => "Hungarian",
        "tr-tr" | "tr" => "Turkish",
        "uk-ua" | "uk" => "Ukrainian",
        "nl-nl" | "nl" => "Dutch",
        "sv-se" | "sv" => "Swedish",
        "da-dk" | "da" => "Danish",
        "fi-fi" | "fi" => "Finnish",
        "nb-no" | "no" => "Norwegian",
        "el-gr" | "el" => "Greek",
        "ro-ro" | "ro" => "Romanian",
        "bg-bg" | "bg" => "Bulgarian",
        "sk-sk" | "sk" => "Slovak",
        "ar" | "ar-sa" => "Arabic",
        "he-il" | "he" => "Hebrew",
        "th-th" | "th" => "Thai",
        "vi-vn" | "vi" => "Vietnamese",
        "id-id" | "id" => "Indonesian",
        _ => return code.to_string(),
    };
    name.to_string()
}

pub fn describe_warning(w: Warning) -> &'static str {
    match w {
        Warning::LocalRootMissing => "local folder missing: no cloud file will be deleted",
        Warning::LocalEmptyWithHistory => {
            "local folder empty although it held saves: deletions blocked"
        }
        Warning::RemoteEmptyWithHistory => {
            "cloud empty although it held saves: local deletions blocked"
        }
        Warning::RootChanged => {
            "the save folder changed, or another account synced it: previous history is ignored"
        }
    }
}

fn describe_cloud(prefix: &str, s: &CloudSummary) -> String {
    let mut out = format!(
        "{prefix}: {} uploaded, {} downloaded",
        s.uploaded, s.downloaded
    );
    if !s.conflicts.is_empty() {
        out += &format!("; conflicts: {}", s.conflicts.join(", "));
    }
    if !s.problems.is_empty() {
        out += &format!("; problems: {}", s.problems.join(", "));
    }
    if !s.left_out.is_empty() {
        out += &format!(
            "; left out (unsafe name, or a case-only duplicate): {}",
            s.left_out.join(", ")
        );
    }
    if !s.unreachable.is_empty() {
        out += &format!(
            "; GOG's cloud not reached, local saves kept as they are: {}",
            s.unreachable.join(", ")
        );
    }
    out
}

pub fn describe_play_event(e: &PlayEvent) -> String {
    let names = |new: &[Unlock]| {
        let names: Vec<&str> = new.iter().map(|u| u.name.as_str()).collect();
        names.join(", ")
    };
    match e {
        PlayEvent::PreparingPrefix => "First launch: creating the Wine prefix…".into(),
        PlayEvent::SetupStep(step) => format!("Setup: {step}…"),
        PlayEvent::SetupWarning(w) => format!("Setup warning: {w}"),
        PlayEvent::SetupSkipped(why) => {
            format!("Setup not run ({why}); it will be retried at the next launch.")
        }
        PlayEvent::CloudChecked(s) => describe_cloud("Cloud checked", s),
        PlayEvent::CloudSkipped(why) => {
            format!("Cloud not checked ({why}); local saves are kept.")
        }
        PlayEvent::Blocked(s) => format!(
            "{}. Launch cancelled: resolve the conflict under Cloud saves.",
            describe_cloud("Cloud needs attention", s)
        ),
        PlayEvent::CometReady => {
            "Comet running: achievements earned in game are sent to GOG.".into()
        }
        PlayEvent::CometUnavailable(why) => {
            format!("Achievements unavailable for this session: {why}")
        }
        PlayEvent::Isolated => {
            "Isolated from your files: the game sees its own folder and its own home.".into()
        }
        PlayEvent::Started { pid } => format!("Game started (pid {pid})."),
        PlayEvent::LauncherExited { code } => {
            format!("Launcher exited ({code:?}); following remaining game processes…")
        }
        PlayEvent::StopRequested => "Stopping…".into(),
        PlayEvent::StoppedBeforeStart => "Stopped before the game started.".into(),
        PlayEvent::Ended { seconds, clean, .. } => format!(
            "Session ended after {} min{}.",
            seconds / 60,
            if *clean { "" } else { " (end uncertain)" }
        ),
        PlayEvent::CloudUploaded(s) => describe_cloud("Cloud after playing", s),
        PlayEvent::CloudUploadSkipped(why) => {
            format!("Cloud not synced ({why}); local saves are kept.")
        }
        PlayEvent::PlaytimeReported(m) => format!("Play time sent to GOG: {m} min."),
        PlayEvent::PlaytimeNotReported(why) => {
            format!("Play time not sent to GOG ({why}); it will be sent after the next session.")
        }
        PlayEvent::Achieved(new) => format!("Achievement unlocked: {}", names(new)),
        PlayEvent::Unlocked(new) => format!("Achievements recorded on GOG: {}", names(new)),
        PlayEvent::NoNewAchievement => "No new achievement recorded on GOG.".into(),
        PlayEvent::AchievementsUnknown => "Could not read achievements back from GOG.".into(),
    }
}
