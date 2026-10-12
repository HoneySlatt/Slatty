//! Which Steam games run, from the user's processes: the Steam client starts each game under its
//! `reaper`, as `reaper SteamLaunch AppId=<id> -- <the game's command>`, natively or as a Flatpak.
//! The name of each of the user's processes is read, and the arguments of the client's own
//! `reaper` processes alone (named so, and started from the client's folder): other programs'
//! arguments can hold what is none of the launcher's business.

use std::collections::BTreeSet;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

/// The app ids of the Steam games running, from `proc` (`/proc`). A reaper the launcher itself
/// runs under, started from Steam as a non-Steam game (on a Steam Deck), is no game.
pub fn running(proc: &Path) -> BTreeSet<u32> {
    let Ok(me) = std::fs::metadata(proc.join("self")).map(|m| m.uid()) else {
        return BTreeSet::new();
    };
    let above = ancestors(proc);
    std::fs::read_dir(proc)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| {
            e.file_name()
                .to_str()
                .is_some_and(|n| n.bytes().all(|b| b.is_ascii_digit()) && !above.contains(n))
        })
        .filter_map(|e| reaper_app(&e.path(), me))
        .collect()
}

/// The processes the launcher runs under: its parent, the parent's, and so on.
fn ancestors(proc: &Path) -> BTreeSet<String> {
    let parent = |process: &Path| {
        let status = std::fs::read_to_string(process.join("status")).ok()?;
        let pid = status.lines().find_map(|l| l.strip_prefix("PPid:"))?.trim();
        (pid != "0" && pid.bytes().all(|b| b.is_ascii_digit())).then(|| pid.to_string())
    };
    let mut found = BTreeSet::new();
    let mut at = parent(&proc.join("self"));
    while let Some(pid) = at {
        if !found.insert(pid.clone()) {
            break;
        }
        at = parent(&proc.join(&pid));
    }
    found
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
    // Steam's own, from its `ubuntu12_32` folder (`steamrt64` for its 64-bit client), natively or
    // in the Flatpak: another program named so (REAPER, the audio workstation) keeps its arguments
    // to itself.
    let exe = nix::fcntl::readlinkat(&dir, "exe").ok()?;
    let exe = Path::new(&exe);
    if !exe.ends_with("ubuntu12_32/reaper") && !exe.ends_with("steamrt64/reaper") {
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
            std::os::unix::fs::symlink(args[0], dir.join("exe")).unwrap();
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
        // Another program of that name (REAPER, the audio workstation): its arguments are none of
        // the launcher's business, and not read.
        process(
            "105",
            "reaper",
            &["/usr/bin/reaper", "SteamLaunch", "AppId=730", "--", "x"],
        );
        std::fs::create_dir_all(proc.join("not-a-pid")).unwrap();
        assert_eq!(running(&proc), BTreeSet::from([440]));
        std::fs::remove_dir_all(&proc).unwrap();
        assert!(running(&proc).is_empty(), "no /proc, nothing runs");
    }

    /// Started from Steam as a non-Steam game (on a Steam Deck), the launcher runs under a reaper
    /// of its own, which is no game.
    #[test]
    fn the_reaper_the_launcher_runs_under_is_no_game() {
        let proc = std::env::temp_dir().join(format!("slatty-proc-self-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&proc);
        let process = |pid: &str, parent: &str, args: &[&str]| {
            let dir = proc.join(pid);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("comm"), "reaper\n").unwrap();
            std::fs::write(dir.join("cmdline"), format!("{}\0", args.join("\0"))).unwrap();
            std::fs::write(
                dir.join("status"),
                format!("Name:\treaper\nPPid:\t{parent}\n"),
            )
            .unwrap();
            std::os::unix::fs::symlink(args[0], dir.join("exe")).unwrap();
        };
        let reaper = "/home/[FAKE]/.local/share/Steam/ubuntu12_32/reaper";
        // The launcher, child of a wrapper (300), child of its reaper (200).
        std::fs::create_dir_all(proc.join("self")).unwrap();
        std::fs::write(proc.join("self/status"), "Name:\tslatty-gui\nPPid:\t300\n").unwrap();
        std::fs::create_dir_all(proc.join("300")).unwrap();
        std::fs::write(proc.join("300/status"), "Name:\twrapper\nPPid:\t200\n").unwrap();
        let shortcut = [reaper, "SteamLaunch", "AppId=3000000001", "--"];
        process(
            "200",
            "1",
            &[&shortcut[..], &["/[FAKE]/slatty-gui"]].concat(),
        );
        // A game Steam runs beside it still counts.
        process(
            "100",
            "1",
            &[reaper, "SteamLaunch", "AppId=440", "--", "/[FAKE]/game"],
        );
        assert_eq!(running(&proc), BTreeSet::from([440]));
        std::fs::remove_dir_all(&proc).unwrap();
    }
}
