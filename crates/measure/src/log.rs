//! The probe's log, as the interface writes it (`docs/testing.md`): one event a line,
//! `<time> <event> [values]`, the time in nanoseconds since the Unix epoch.

use std::io::Read;
use std::path::Path;

use anyhow::{Context, Result, bail};

#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    /// Nanoseconds since the Unix epoch.
    pub at: u128,
    pub name: String,
    pub values: Vec<String>,
}

/// The log of one run, read as it grows.
pub struct Log {
    file: Option<std::fs::File>,
    /// What was read past the last full line.
    partial: String,
    pub events: Vec<Event>,
}

impl Log {
    /// A log that `path` will hold; nothing is read before it exists.
    pub fn new() -> Self {
        Self {
            file: None,
            partial: String::new(),
            events: Vec::new(),
        }
    }

    /// Reads what was added to `path` since the last read.
    pub fn read_more(&mut self, path: &Path) -> Result<()> {
        if self.file.is_none() {
            match std::fs::File::open(path) {
                Ok(file) => self.file = Some(file),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
                Err(e) => return Err(e).with_context(|| format!("opening {}", path.display())),
            }
        }
        let file = self.file.as_mut().expect("just opened");
        file.read_to_string(&mut self.partial)?;
        let Some(end) = self.partial.rfind('\n') else {
            return Ok(());
        };
        let complete: String = self.partial.drain(..=end).collect();
        for line in complete.lines() {
            self.events.push(parse(line)?);
        }
        Ok(())
    }

    /// The first event named `name` (with `value`, when given).
    pub fn first(&self, name: &str, value: Option<&str>) -> Option<&Event> {
        self.events.iter().find(|e| {
            e.name == name && value.is_none_or(|v| e.values.first().map(String::as_str) == Some(v))
        })
    }

    /// The window's last size and scale.
    pub fn window(&self) -> Option<(f64, f64, f64)> {
        let event = self.events.iter().rev().find(|e| e.name == "window")?;
        let number = |i: usize| event.values.get(i)?.parse::<f64>().ok();
        Some((number(0)?, number(1)?, number(2)?))
    }

    /// The times of the frames drawn after `from` and up to `to`.
    pub fn frames(&self, from: u128, to: u128) -> Vec<u128> {
        self.events
            .iter()
            .filter(|e| e.name == "frame" && e.at > from && e.at <= to)
            .map(|e| e.at)
            .collect()
    }

    /// How long each view built between `from` and `to` took, in milliseconds.
    pub fn views(&self, from: u128, to: u128) -> Vec<f64> {
        self.events
            .iter()
            .filter(|e| e.name == "view" && e.at > from && e.at <= to)
            .filter_map(|e| e.values.first()?.parse::<f64>().ok())
            .map(|ns| ns / 1e6)
            .collect()
    }

    /// The events named `name`.
    pub fn count(&self, name: &str) -> usize {
        self.events.iter().filter(|e| e.name == name).count()
    }
}

fn parse(line: &str) -> Result<Event> {
    let mut words = line.split_whitespace();
    let (Some(at), Some(name)) = (words.next(), words.next()) else {
        bail!("malformed line in the probe's log: {line:?}");
    };
    Ok(Event {
        at: at
            .parse()
            .with_context(|| format!("malformed time in the probe's log: {line:?}"))?,
        name: name.to_string(),
        values: words.map(str::to_string).collect(),
    })
}

/// The time between successive `frames`, in milliseconds.
pub fn intervals(frames: &[u128]) -> Vec<f64> {
    frames
        .windows(2)
        .map(|pair| (pair[1] - pair[0]) as f64 / 1e6)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_growing_log_is_read_line_by_line() {
        let path = std::env::temp_dir().join(format!("slatty-measure-log-{}", std::process::id()));
        std::fs::write(&path, "100 start\n200 window 1440 900 1.5\n300 fra").unwrap();
        let mut log = Log::new();
        log.read_more(&path).unwrap();
        assert_eq!(log.events.len(), 2);
        assert_eq!(log.window(), Some((1440.0, 900.0, 1.5)));
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        std::io::Write::write_all(&mut file, b"me\n400 view 2000000\n500 frame\n").unwrap();
        log.read_more(&path).unwrap();
        assert_eq!(log.frames(0, 1000), [300, 500]);
        assert_eq!(log.views(0, 1000), [2.0]);
        assert_eq!(log.first("start", None).unwrap().at, 100);
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn a_malformed_line_is_an_error() {
        assert!(parse("start").is_err());
        assert!(parse("soon start").is_err());
    }

    #[test]
    fn intervals_are_in_milliseconds() {
        assert_eq!(intervals(&[0, 4_000_000, 12_000_000]), [4.0, 8.0]);
        assert!(intervals(&[1]).is_empty());
    }
}
