//! Text shown to the user: sizes, durations, dates and descriptions of core events.

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

pub fn human_size(bytes: u64) -> String {
    match bytes {
        b if b >= 1 << 30 => format!("{:.2} GiB", b as f64 / (1u64 << 30) as f64),
        b if b >= 1 << 20 => format!("{:.1} MiB", b as f64 / (1u64 << 20) as f64),
        b => format!("{} KiB", b >> 10),
    }
}

pub fn duration(seconds: i64) -> String {
    let minutes = seconds / 60;
    match (minutes / 60, minutes % 60) {
        (0, m) => format!("{m} m"),
        (h, m) => format!("{h} h {m} m"),
    }
}

/// "Today", "Yesterday", "3 days ago", or the date.
pub fn relative_day(ts: i64) -> String {
    use chrono::{Local, TimeZone};
    let Some(then) = Local.timestamp_opt(ts, 0).single() else {
        return String::new();
    };
    match (Local::now().date_naive() - then.date_naive()).num_days() {
        0 => "Today".into(),
        1 => "Yesterday".into(),
        d @ 2..=6 => format!("{d} days ago"),
        _ => then.format("%-d %b %Y").to_string(),
    }
}

pub fn local_time(ts: i64) -> String {
    use chrono::TimeZone;
    chrono::Local
        .timestamp_opt(ts, 0)
        .single()
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
        Warning::RootChanged => "the save folder changed: previous history is ignored",
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
    out
}

pub fn describe_play_event(e: &PlayEvent) -> String {
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
        PlayEvent::Started { pid } => format!("Game started (pid {pid})."),
        PlayEvent::LauncherExited { code } => {
            format!("Launcher exited ({code:?}); following remaining game processes…")
        }
        PlayEvent::StopRequested => "Stopping…".into(),
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
        PlayEvent::Unlocked(names) => {
            format!("Achievements recorded on GOG: {}", names.join(", "))
        }
        PlayEvent::NoNewAchievement => "No new achievement recorded on GOG.".into(),
        PlayEvent::AchievementsUnknown => "Could not read achievements back from GOG.".into(),
    }
}
