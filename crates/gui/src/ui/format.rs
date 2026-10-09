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
