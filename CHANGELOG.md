# Changelog

All notable changes are listed here. Versions follow [Semantic Versioning](https://semver.org);
until 1.0, a minor version may change commands and on-disk formats.

## 0.1.0 (not released yet)

The first public version.

### Features

- Sign-in to GOG in the system browser; session tokens in the system keyring.
- Library with covers, cached for offline use, fluid at 10,000 games.
- Install Windows builds from GOG's Galaxy content system (pause, resume, integrity checks, binary
  patches) and native Linux builds from GOG's offline installers.
- Launch through umu-launcher and Proton (or natively), following the session until the last
  process exits; launch options; post-install setup (GOG's scripts, redistributables).
- Windows games isolated from your files by default, in umu's container inside bubblewrap.
- Cloud saves: three-way sync on content hashes, conflicts that never resolve themselves, a copy
  of every save replaced or deleted, locally and in the cloud.
- Achievements: listed, reported as games unlock them through Comet, or unlocked by hand.
- Verify, repair, update (automatically for games on their newest build), uninstall; DLC and
  language changes.
- Proton builds downloaded from GitHub and kept up to date when asked (off until turned on).
- Edit a game's title, sorting title, cover and background (SteamGridDB when turned on); hide it.
- Play time reported to GOG as Galaxy does (can be turned off).
- Steam games in the same library (off until turned on), read from the Steam client's own files
  and played, installed, verified and uninstalled through the client; no GOG account needed for
  them. GOG is recommended where you have the choice. Images the client has not saved come from
  Steam's image servers once turned on.
- Command line (`slatty`) and interface (`slatty-gui`) on the same core.

### Security and privacy

Reviewed before publication. Among what changed:

- Nothing is written through a link or into a pipe a game left in its folder, prefix or save
  folders; files read from a game folder, network answers, manifests and chunks are bounded in
  size before memory is taken.
- The account's token goes only to GOG's own addresses, never leaves in a `Referer`, and stays out
  of Comet's log. Comet gets it through a folder of the user's alone, is taken as ready only when
  it runs as the user, and stops with the launcher even when the launcher is killed. Comet only
  runs once **Achievements in game** is turned on: while it runs, any local program can get the
  access token from it.
- Isolated games can no longer change umu's runtime, the Proton build or the runtime tool other
  games run through, nor see or signal the user's other programs.
- umu's database is read from the Proton build's own copy; asking umu's server is off until
  turned on. The runtime refresh no longer has umu ask GitHub.
- The cloud version of a save is kept before an upload replaces it or a deletion removes it.
- **Forget account** deletes what is kept of an account on the computer; saves stay.
- [SECURITY.md](SECURITY.md) lists everything that leaves the computer, and how to report a
  vulnerability privately.
- Where bubblewrap cannot hold an isolated game (Ubuntu 23.10 and later without an AppArmor
  profile for it), the launch is refused with the reason and the way out, never run without
  isolation; `slatty doctor` checks it too.

Known limits are listed in [SECURITY.md](SECURITY.md) and [docs/compatibility.md](docs/compatibility.md).
