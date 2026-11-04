# Architecture decision records

An ADR records one decision: its context, the decision, the alternatives rejected and the
consequences. Once accepted, only its status changes. A later decision that replaces it whole makes
it `Superseded by NNNN`; one that changes part of it makes it `Accepted; amended by NNNN`, and the
newer one `Accepted; amends NNNN (what)`.

New ADRs copy [`0000-template.md`](0000-template.md) and take the next free number.

ADRs 0001 to 0022 record decisions taken before the project kept ADRs. They were written on
2026-11-04 from the design decisions of [ARCHITECTURE.md](../ARCHITECTURE.md), the code and the
documents of the time; the date of each is the day the decision was first written in the
repository, which can be a little after it was taken.

| ADR | Title | Status |
|---|---|---|
| [0001](0001-rust-with-proton-umu-and-comet-outside.md) | Rust for the whole application; Proton, umu and Comet stay external tools | Accepted |
| [0002](0002-gpl-3-0-or-later.md) | GPL-3.0-or-later | Accepted |
| [0003](0003-sqlite-through-rusqlite-behind-a-mutex.md) | The state database: SQLite through rusqlite, behind a mutex | Accepted |
| [0004](0004-tokens-only-in-the-system-keyring.md) | Tokens only in the system keyring | Accepted |
| [0005](0005-every-third-party-off-until-turned-on.md) | Every third party off until turned on; no telemetry | Accepted |
| [0006](0006-sign-in-in-the-system-browser.md) | Sign-in in the system browser, the address pasted back; no web view | Accepted |
| [0007](0007-linux-builds-from-offline-installers.md) | GOG's Linux builds from its offline installers, read file by file | Accepted |
| [0008](0008-sessions-followed-by-a-subreaper-supervisor.md) | Game sessions followed by a subreaper supervisor | Accepted |
| [0009](0009-isolation-through-umus-container.md) | Isolation through umu's container, the tools read-only through bubblewrap | Accepted |
| [0010](0010-proton-builds-downloaded-by-slatty.md) | Proton builds downloaded by Slatty, not by umu | Accepted |
| [0011](0011-comet-supervised-beside-the-game.md) | Comet supervised beside the game, off until turned on | Accepted |
| [0012](0012-cloud-saves-decided-by-content-hashes.md) | Cloud saves decided by content hashes and per-file history, never dates | Accepted |
| [0013](0013-steam-read-from-its-files-played-through-its-client.md) | Steam's games read from its files and played through its client | Accepted |
| [0014](0014-gog-never-told-of-other-stores-games.md) | GOG never told of other stores' games | Accepted |
| [0015](0015-iced-for-the-interface.md) | Iced for the interface | Accepted; amended by 0019 and 0020 |
| [0016](0016-images-shown-from-files-at-the-size-drawn.md) | Images shown from files, made at the size drawn | Accepted |
| [0017](0017-only-the-rows-in-view-are-built.md) | Grids and long lists build only the rows in view | Accepted |
| [0018](0018-a-couch-mode-of-its-own.md) | A couch mode of its own | Accepted |
| [0019](0019-the-interfaces-state-free-of-any-framework.md) | The interface's state free of any framework | Accepted; amends 0015 (the state leaves the Iced crate) |
| [0020](0020-gpui-set-aside-for-slime-ui.md) | GPUI tried and set aside; Slatty is the pilot of SlimeEngine's interface framework | Accepted; amends 0015 (Iced stays until slime-ui runs Slatty) |
| [0021](0021-interface-in-english-first.md) | The interface in English first | Accepted |
| [0022](0022-windows-hosts.md) | Windows hosts: one code base, built from Linux, Linux's features kept Linux's | Accepted |
