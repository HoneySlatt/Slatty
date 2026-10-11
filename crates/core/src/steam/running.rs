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
        .filter(|e| e.metadata().is_ok_and(|m| m.uid() == me))
        .filter(|e| std::fs::read(e.path().join("comm")).is_ok_and(|c| c.trim_ascii() == b"reaper"))
        .filter_map(|e| app_of(&std::fs::read(e.path().join("cmdline")).ok()?))
        .collect()
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
