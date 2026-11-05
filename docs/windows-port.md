# Porting Slatty to Windows

Slatty runs on Linux today. This page holds what porting it to Windows 10 and 11 (x86-64) needs to
know: how it is built and checked, where Slatty depends on Linux, and the open questions. The
port's goals and limits are decided in [ADR-0022](adr/0022-windows-hosts.md); its milestones and
where each stands are Stage 3 of [PLAN.md](PLAN.md).

## How it is built and checked

- **Cross-compilation from Linux**, in the Nix development shell `nix develop .#windows`: Rust's
  `x86_64-pc-windows-gnu` target with the MinGW-w64 toolchain, from the Nix cache. `cargo check`
  and `cargo build` run there for the Windows target, in `target/windows`; `cargo clippy` does not
  yet (the shell has no Clippy for that target).
- **Tests of the Windows code under Wine**: `cargo test` for the Windows target runs its test
  binaries through Wine, which implements the Windows API the code calls (processes, job objects,
  registry, known folders, reparse points). Wine is not Windows: a passing test there is a first
  check, never a verified behaviour. How to run them, and which tests are Linux's alone:
  [testing.md](testing.md#the-windows-build).
- **Real checks on Windows 10 or 11**, by a person, recorded in [compatibility.md](compatibility.md)
  as the Linux ones are. Nothing about Windows is called verified before.
- Later, a build with Microsoft's toolchain (`x86_64-pc-windows-msvc`), the usual one on Windows,
  if the MinGW build shows a problem it does not have.

## What Windows does differently

Found by running the core's tests under Wine; each broke code that works on Linux.

- A file is flushed to the disk only through a handle that may write it.
- A folder cannot be opened as a file (`File::open`), and Windows has no flush of a whole
  filesystem for a user.
- A file still open cannot be replaced by a rename (Wine, and Windows without POSIX rename
  semantics), nor its folder deleted.
- A read-only file cannot be replaced by a rename, where Linux needs only the folder writable.
- A path made into a string keeps the system's separator: compare `Path`s, not strings.
- Folders ignore case, so two names that differ only by case are one file.
- Creation times cannot tell a file made again from the old one: Windows gives a name made again
  within 15 seconds the old one's, and Wine gives the last write's. NTFS's file index does.

## Where Slatty depends on Linux today

Found by building the core for Windows (109 errors in its first build) and reading the code:

| Area | Linux today | Windows |
|---|---|---|
| Running a game (`runner`, `play`) | umu and Proton, Wine, or a native Linux program; `steam-run` on NixOS | The game's `.exe`, started directly (`Runner::Native` on a Windows host) |
| Following a session (`session`) | A supervisor process: subreaper, process groups, signals, `/proc`, `PR_SET_PDEATHSIG` | The same supervisor process, holding the game in a job object: the job's processes are the session's, they end with it, and it is stopped by closing the game's windows, then ending the job |
| Isolation (`runner`) | bubblewrap through umu's container | None: not offered |
| Data folders (`paths`) | XDG folders, made 0700 | `%APPDATA%\slatty` and `%LOCALAPPDATA%\slatty`, private to the user by the profile's own permissions |
| Files (`fsutil`, `installer`) | `O_NOFOLLOW`, symbolic links, `setpriority` | Reparse points (symbolic links and junctions) refused the same way, `FILE_FLAG_OPEN_REPARSE_POINT`, background thread priority; names Windows reserves (`CON`, `aux.txt`…) refused |
| Locks (`lock`) | `File::try_lock` | The same, on Windows' file locks |
| Secrets (`credentials`) | Secret Service through `keyring` | Windows Credential Manager through `keyring` |
| Saves (`cloud::locations`, `gameinfo`) | GOG's and Epic's folder templates resolved in the game's Wine prefix | The same templates resolved in the user's real folders (Known Folders: Saved Games, Documents, AppData…) |
| GOG's post-install setup (`setup`) | GOG's script interpreter and redistributables run through Proton | Run directly; redistributables need administrator rights: Windows asks (UAC), once the person agreed in Slatty |
| Achievements (`comet`, `galaxy_service`) | Comet, a Linux program; the dummy `GalaxyCommunication` service registered in the prefix | Comet's Windows build, if it has one that works; registering a Windows service needs administrator rights: studied separately |
| Steam (`steam`) | `~/.steam`, `/proc` for the games running, `steam://` through `xdg-open` | Steam's folder from the registry (`HKCU\Software\Valve\Steam`), the game running from the registry or the process list, `steam://` through the shell |
| Epic (`epic::launch`) | Through Proton, the ownership token in the prefix | Directly, with the same arguments, as legendary does on Windows |
| Games added by hand (`added`) | Linux programs, AppImages, `.sh` installers, archives, Windows installers through Proton | `.exe` programs, archives, Windows installers run directly |
| Emulated games (`emulated`) | Linux emulators, found in their usual Linux places | Their Windows builds: later |
| Proton builds, runners (`protons`, `umu`) | Downloaded from GitHub, umu's runtimes | Not offered |
| Notifications (interface) | The desktop's, over D-Bus | None at first; Windows notifications later |
| Gamepads (couch mode) | gilrs through udev | gilrs through Windows' own input APIs |
| File chooser (interface) | rfd through the desktop portal | rfd through Windows' dialogs |
| Packaging | Nix package, desktop entry, AppStream | A zip to unpack first; an installer later |

## Open questions

- Does Comet's Windows build talk to games' Galaxy SDK without GOG Galaxy's own service installed?
- Can a game's session be stopped cleanly on Windows the way it is on Linux (closing its windows
  first), for every kind of game, and launchers that start the game and exit?
- Which redistributables GOG's games ask for are already present on a usual Windows, so that the
  administrator prompt is rare?
- Code signing: an unsigned `.exe` makes Windows SmartScreen warn the first times it starts.
