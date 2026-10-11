//! Which Steam games run, from the user's processes: the Steam client starts each game under its
//! `reaper`, as `reaper SteamLaunch AppId=<id> -- <the game's command>`, natively or as a Flatpak.
//! The name of each of the user's processes is read, and the arguments of their `reaper` processes
//! alone: other programs' arguments can hold what is none of the launcher's business.

use std::collections::BTreeSet;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

/// The app ids of the Steam games running, from `proc` (`/proc`).
pub fn running(proc: &Path) -> BTreeSet<u32> {
    let Ok(me) = std::fs::metadata(proc.join("self")).map(|m| m.uid()) else {
        return BTreeSet::new();
    };
    std::fs::read_dir(proc)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| {
            e.file_name()
                .to_str()
                .is_some_and(|n| n.bytes().all(|b| b.is_ascii_digit()))
        })
        .filter_map(|e| reaper_app(&e.path(), me))
        .collect()
}

/// The app id a process's folder (`/proc/<pid>`) tells, when it is one of the user's `reaper`
/// processes. The folder is opened once and its files read through it: a process that ends
/// meanwhile, and another one given its number, cannot be read in its place.
fn reaper_app(process: &Path, me: u32) -> Option<u32> {
    use nix::fcntl::{OFlag, open, openat};
    use nix::sys::stat::{Mode, fstat};
    let dir = open(
        process,
        OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_CLOEXEC,
        Mode::empty(),
    )
    .ok()?;
    if fstat(&dir).ok()?.st_uid != me {
        return None;
    }
    let read = |name: &str| {
        let file = openat(
            &dir,
            name,
            OFlag::O_RDONLY | OFlag::O_CLOEXEC,
            Mode::empty(),
        )
        .ok()?;
        let mut bytes = Vec::new();
        let mut file = std::io::Read::take(std::fs::File::from(file), 64 << 10);
        std::io::Read::read_to_end(&mut file, &mut bytes).ok()?;
        Some(bytes)
    };
    if read("comm")?.trim_ascii() != b"reaper" {
        return None;
    }
    app_of(&read("cmdline")?)
}

/// The app id of the game a `reaper` command line starts, from its arguments before `--`; what
/// follows is the game's own.
fn app_of(cmdline: &[u8]) -> Option<u32> {
    let own: Vec<&[u8]> = cmdline
        .split(|b| *b == 0)
        .take_while(|arg| *arg != b"--")
        .collect();
    if !own.contains(&&b"SteamLaunch"[..]) {
        return None;
    }
    own.iter()
        .find_map(|arg| arg.strip_prefix(b"AppId="))
        .and_then(|id| std::str::from_utf8(id).ok()?.parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_game_runs_while_steams_reaper_for_it_runs() {
        let proc = std::env::temp_dir().join(format!("slatty-proc-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&proc);
        let process = |pid: &str, comm: &str, args: &[&str]| {
            let dir = proc.join(pid);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("comm"), format!("{comm}\n")).unwrap();
            let mut cmdline = args.join("\0");
            cmdline.push('\0');
            std::fs::write(dir.join("cmdline"), cmdline).unwrap();
        };
        std::fs::create_dir_all(proc.join("self")).unwrap();
        let reaper = "/home/[FAKE]/.local/share/Steam/ubuntu12_32/reaper";
        process(
            "100",
            "reaper",
            &[
                reaper,
                "SteamLaunch",
                "AppId=440",
                "--",
                "/[FAKE]/game",
                "AppId=7",
            ],
        );
        // Steam's own reaper, and processes that only look alike.
        process("101", "reaper", &[reaper, "SteamLaunch", "--", "AppId=620"]);
        process("102", "game", &["/[FAKE]/game", "SteamLaunch", "AppId=70"]);
        process("103", "reaper", &[reaper, "AppId=220"]);
        process(
            "104",
            "reaper",
            &[reaper, "SteamLaunch", "AppId=not-a-number"],
        );
        std::fs::create_dir_all(proc.join("not-a-pid")).unwrap();
        assert_eq!(running(&proc), BTreeSet::from([440]));
        std::fs::remove_dir_all(&proc).unwrap();
        assert!(running(&proc).is_empty(), "no /proc, nothing runs");
    }
}
