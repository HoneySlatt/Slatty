//! What Linux tells of a process and of the graphics card: its memory, its processor time, its
//! wake-ups, its video memory, the card's power; and dropping files from the disk cache.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result};

/// The resident memory of `pid`, in bytes.
pub fn resident(pid: u32) -> Result<u64> {
    let status = std::fs::read_to_string(format!("/proc/{pid}/status"))?;
    kib_field(&status, "VmRSS:").context("no VmRSS in /proc/<pid>/status")
}

/// The video memory `pid` holds, in bytes: `drm-memory-vram` of each graphics client it opened
/// (or `drm-total-vram`, its newer name), counted once each (`drm-client-id`). `None` when the
/// driver reports none, as only some do (amdgpu does).
pub fn video_memory(pid: u32) -> Result<Option<u64>> {
    let mut clients: HashMap<String, u64> = HashMap::new();
    for entry in std::fs::read_dir(format!("/proc/{pid}/fdinfo"))? {
        let Ok(info) = std::fs::read_to_string(entry?.path()) else {
            continue;
        };
        let (Some(client), Some(vram)) = (
            info.lines()
                .find_map(|l| l.strip_prefix("drm-client-id:"))
                .map(|id| id.trim().to_string()),
            kib_field(&info, "drm-memory-vram:").or_else(|| kib_field(&info, "drm-total-vram:")),
        ) else {
            continue;
        };
        clients.insert(client, vram);
    }
    Ok((!clients.is_empty()).then(|| clients.values().sum()))
}

/// A field of `/proc` given in KiB, in bytes.
fn kib_field(text: &str, name: &str) -> Option<u64> {
    let value = text.lines().find_map(|l| l.strip_prefix(name))?;
    let kib: u64 = value.split_whitespace().next()?.parse().ok()?;
    Some(kib * 1024)
}

/// The processor time `pid` used so far, user and system, in clock ticks (a hundredth of a second
/// on Linux).
pub fn ticks(pid: u32) -> Result<u64> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat"))?;
    // The command's name, in parentheses, may hold spaces: fields are counted after it.
    let rest = stat
        .rsplit_once(')')
        .context("malformed /proc/<pid>/stat")?
        .1;
    let fields: Vec<&str> = rest.split_whitespace().collect();
    // utime and stime are fields 14 and 15 of the line; the 12th and 13th after the name.
    let utime: u64 = fields.get(11).context("no utime")?.parse()?;
    let stime: u64 = fields.get(12).context("no stime")?.parse()?;
    Ok(utime + stime)
}

/// The wake-ups of every thread of `pid` so far: its context switches, those it asked for and those
/// it did not, per thread id. A thread that ended takes its count with it.
pub fn wakeups(pid: u32) -> Result<HashMap<u32, u64>> {
    let mut counts = HashMap::new();
    for entry in std::fs::read_dir(format!("/proc/{pid}/task"))? {
        let entry = entry?;
        let Ok(tid) = entry.file_name().to_string_lossy().parse::<u32>() else {
            continue;
        };
        let Ok(status) = std::fs::read_to_string(entry.path().join("status")) else {
            continue;
        };
        let count: u64 = ["voluntary_ctxt_switches:", "nonvoluntary_ctxt_switches:"]
            .iter()
            .filter_map(|name| {
                status
                    .lines()
                    .find_map(|l| l.strip_prefix(name))
                    .and_then(|v| v.trim().parse::<u64>().ok())
            })
            .sum();
        counts.insert(tid, count);
    }
    Ok(counts)
}

/// The wake-ups between two counts of `wakeups`: a thread born between them counts from zero.
pub fn wakeups_between(before: &HashMap<u32, u64>, after: &HashMap<u32, u64>) -> u64 {
    after
        .iter()
        .map(|(tid, count)| count - before.get(tid).copied().unwrap_or(0).min(*count))
        .sum()
}

/// The files `pid` has mapped: its program and its libraries.
pub fn mapped_files(pid: u32) -> Result<HashSet<PathBuf>> {
    let maps = std::fs::read_to_string(format!("/proc/{pid}/maps"))?;
    Ok(maps
        .lines()
        .filter_map(|l| l.split_whitespace().nth(5))
        .filter(|p| p.starts_with('/'))
        .map(PathBuf::from)
        .filter(|p| p.is_file())
        .collect())
}

/// Every file under `folder`.
pub fn files_under(folder: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut folders = vec![folder.to_path_buf()];
    while let Some(folder) = folders.pop() {
        for entry in std::fs::read_dir(&folder).into_iter().flatten().flatten() {
            match entry.file_type() {
                Ok(t) if t.is_dir() => folders.push(entry.path()),
                Ok(t) if t.is_file() => files.push(entry.path()),
                _ => {}
            }
        }
    }
    files
}

/// Drops `files` from the disk cache, as `dd iflag=nocache count=0` does (`posix_fadvise`), which
/// needs no privilege. A page another process maps stays: the libraries the desktop itself uses
/// stay in memory, as they do after a real start of the computer. What is still to be written (the
/// database's journal of the run before) is written first: the cache keeps a page it has not.
pub fn drop_from_cache(files: &[PathBuf]) -> Result<()> {
    Command::new("sync").status().context("running sync")?;
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    std::thread::scope(|scope| {
        let workers: Vec<_> = files
            .chunks(files.len().div_ceil(threads).max(1))
            .map(|chunk| {
                scope.spawn(move || -> Result<()> {
                    for file in chunk {
                        Command::new("dd")
                            .arg(format!("if={}", file.display()))
                            .args(["iflag=nocache", "count=0", "status=none"])
                            .stdout(Stdio::null())
                            .stderr(Stdio::null())
                            .status()
                            .context("running dd")?;
                    }
                    Ok(())
                })
            })
            .collect();
        workers
            .into_iter()
            .try_for_each(|w| w.join().expect("a cache worker panicked"))
    })
}

/// The power the graphics card draws now, in watts, from its sensor (`power1_average`, or
/// `power1_input`), with the sensor's path; `None` when no card reports it.
pub fn gpu_power() -> Option<(f64, PathBuf)> {
    let mut cards: Vec<PathBuf> = std::fs::read_dir("/sys/class/drm")
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("card") && !n.contains('-'))
        })
        .collect();
    cards.sort();
    cards.iter().find_map(|card| {
        let hwmons = std::fs::read_dir(card.join("device/hwmon")).ok()?;
        hwmons.flatten().find_map(|hwmon| {
            ["power1_average", "power1_input"].iter().find_map(|name| {
                let path = hwmon.path().join(name);
                let microwatts: f64 = std::fs::read_to_string(&path).ok()?.trim().parse().ok()?;
                Some((microwatts / 1e6, path))
            })
        })
    })
}

/// The processor's name, from `/proc/cpuinfo`.
pub fn processor() -> String {
    std::fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|info| {
            info.lines()
                .find_map(|l| l.strip_prefix("model name"))
                .map(|v| v.trim_start_matches([' ', '\t', ':']).to_string())
        })
        .unwrap_or_else(|| "unknown processor".into())
}

/// The kernel's version.
pub fn kernel() -> String {
    std::fs::read_to_string("/proc/sys/kernel/osrelease")
        .map(|v| v.trim().to_string())
        .unwrap_or_else(|_| "unknown".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_process_is_measured() {
        let pid = std::process::id();
        assert!(resident(pid).unwrap() > 0);
        ticks(pid).unwrap();
        let counts = wakeups(pid).unwrap();
        assert!(counts.contains_key(&pid));
        assert!(
            mapped_files(pid)
                .unwrap()
                .iter()
                .any(|p| p.to_string_lossy().contains("slatty_measure"))
        );
    }

    #[test]
    fn wake_ups_count_new_threads_from_zero() {
        let before = HashMap::from([(1, 10), (2, 5)]);
        let after = HashMap::from([(1, 12), (3, 4)]);
        assert_eq!(wakeups_between(&before, &after), 2 + 4);
    }

    #[test]
    fn kib_fields_are_read_in_bytes() {
        assert_eq!(
            kib_field("drm-memory-vram:\t1234 KiB\n", "drm-memory-vram:"),
            Some(1234 * 1024)
        );
        assert_eq!(kib_field("VmRSS:  12 kB\n", "VmRSS:"), Some(12 * 1024));
        assert_eq!(kib_field("nothing\n", "VmRSS:"), None);
    }
}
