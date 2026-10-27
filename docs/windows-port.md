# Porting Slatty to Windows

Slatty runs on Linux today. This page plans running it on Windows 10 and 11 (x86-64): what has to
change, in which order, and how each step is checked. It is updated as the work goes on; the
[roadmap](roadmap.md) lists the port as planned until it is done.

## Goals and limits

- One code base: the same crates build for Linux and Windows. Linux behaves exactly as before at
  every step; its tests and benchmarks keep passing.
- On Windows, Windows games run directly, without Proton or Wine. GOG's Windows builds, Epic's
  games, Steam's games (through the Steam client) and games added by hand are the scope of a first
  version; emulated games follow.
- Safety comes first, as on Linux: tokens only in the system's secret storage (Windows Credential
  Manager), nothing written through a link leading out of a game folder, saves never lost.
- What only Linux has stays Linux's: umu and Proton, the isolation from your files (umu's
  container), `steam-run`, GOG's native Linux builds and their offline installers, Proton
  builds. On Windows the interface does not offer them.
- Not planned for the first version: isolating Windows games from your files (Windows has no
  equivalent of umu's container a launcher can use without administrator rights), an installer
  signed with a code-signing certificate (Windows SmartScreen warns about unsigned programs).

## How it is built and checked

- **Cross-compilation from Linux**, in the Nix development shell `nix develop .#windows`: Rust's
  `x86_64-pc-windows-gnu` target with the MinGW-w64 toolchain, from the Nix cache. `cargo check`,
  `cargo clippy` and `cargo build` run there for the Windows target, in `target/windows`.
- **Tests of the Windows code under Wine**: `cargo test` for the Windows target runs its test
  binaries through Wine, which implements the Windows API the code calls (processes, job objects,
  registry, known folders, reparse points). Wine is not Windows: a passing test there is a first
  check, never a verified behaviour.
- **Real checks on Windows 10 or 11**, by a person, recorded in [compatibility.md](compatibility.md)
  as the Linux ones are. Nothing about Windows is called verified before.
- Later, a build with Microsoft's toolchain (`x86_64-pc-windows-msvc`), the usual one on Windows,
  if the MinGW build shows a problem it does not have.

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

## Steps

Each step is a series of small commits; the Linux build, clippy and tests pass after each, and the
Windows target builds as far as the step reached.

1. **The Windows build environment** (done when the shell builds `slatty-core`'s dependencies):
   the `windows` development shell in `flake.nix`, its Wine test runner, and this page.
2. **Everything compiles for Windows.** A `platform` module in the core gathers what differs by
   system (processes, priority, private folders, opening without following links, known folders,
   finding programs). Linux-only modules (Proton, umu, isolation, Linux installers, Proton builds,
   the Galaxy service in a prefix) build on Linux alone; their callers on Windows get
   `Error::Unsupported`. Done when `slatty`, `slatty-gui` and the core build for Windows, and the
   core's tests that do not need Linux pass under Wine.
3. **Play a GOG game.** Install a Windows build (the download code is shared), start it directly,
   follow and stop its session through a job object, record play time, run GOG's setup with the
   person's consent for administrator rights. Cloud saves in the real folders.
4. **Epic and Steam.** Epic's games launched directly and signed in; Steam found from the registry,
   its games listed, played, installed and uninstalled through the client, the game running found.
5. **Games added by hand**: programs, archives and installers on Windows.
6. **The interface on Windows**: Linux-only settings hidden, file chooser, gamepads, a window icon,
   paths shown the Windows way.
7. **Achievements**: Comet on Windows, and whether the Galaxy service is needed there.
8. **Packaging**: a zip with `slatty.exe`, `slatty-gui.exe` and the licences, built by Nix; then an
   installer, and a release published for Windows only once the real checks pass.
9. **Real checks on Windows 10 and 11**, then the documentation: the user guide's Windows parts,
   SECURITY.md (where secrets and files live on Windows), compatibility.md, the README.

## Open questions

- Does Comet's Windows build talk to games' Galaxy SDK without GOG Galaxy's own service installed?
- Can a game's session be stopped cleanly on Windows the way it is on Linux (closing its windows
  first), for every kind of game, and launchers that start the game and exit?
- Which redistributables GOG's games ask for are already present on a usual Windows, so that the
  administrator prompt is rare?
- Code signing: an unsigned `.exe` makes Windows SmartScreen warn the first times it starts.
