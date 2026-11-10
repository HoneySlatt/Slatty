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
| [0009](0009-isolation-through-umus-container.md) | Isolation through umu's container, the tools read-only through bubblewrap | Accepted; amended by 0034 and 0036 |
| [0010](0010-proton-builds-downloaded-by-slatty.md) | Proton builds downloaded by Slatty, not by umu | Accepted |
| [0011](0011-comet-supervised-beside-the-game.md) | Comet supervised beside the game, off until turned on | Accepted |
| [0012](0012-cloud-saves-decided-by-content-hashes.md) | Cloud saves decided by content hashes and per-file history, never dates | Accepted |
| [0013](0013-steam-read-from-its-files-played-through-its-client.md) | Steam's games read from its files and played through its client | Accepted; amended by 0034 |
| [0014](0014-gog-never-told-of-other-stores-games.md) | GOG never told of other stores' games | Accepted |
| [0015](0015-iced-for-the-interface.md) | Iced for the interface | Accepted; amended by 0019 and 0020 |
| [0016](0016-images-shown-from-files-at-the-size-drawn.md) | Images shown from files, made at the size drawn | Accepted |
| [0017](0017-only-the-rows-in-view-are-built.md) | Grids and long lists build only the rows in view | Accepted |
| [0018](0018-a-couch-mode-of-its-own.md) | A couch mode of its own | Accepted; amended by 0034 and 0036 |
| [0019](0019-the-interfaces-state-free-of-any-framework.md) | The interface's state free of any framework | Accepted; amends 0015 (the state leaves the Iced crate); amended by 0034 |
| [0020](0020-gpui-set-aside-for-slime-ui.md) | GPUI tried and set aside; Slatty is the pilot of SlimeEngine's interface framework | Accepted; amends 0015 (Iced stays until slime-ui runs Slatty); amended by 0025, 0030 and 0031 |
| [0021](0021-interface-in-english-first.md) | The interface in English first | Accepted; amended by 0034 |
| [0022](0022-windows-hosts.md) | Windows hosts: one code base, built from Linux, Linux's features kept Linux's | Accepted; amended by 0034 |
| [0023](0023-a-quality-bar-close-to-what-is-measured.md) | A quality bar close to what is measured, its gaps closed in Stage 4 | Accepted; amended by 0034 and 0036 |
| [0024](0024-hardware-tiers-and-the-budgets-of-the-whole-loop.md) | Hardware tiers from a 2016 laptop, and budgets for the whole working loop | Accepted; extends 0023; amended by 0026, 0034 and 0036 |
| [0025](0025-paused-after-m3-3-until-slime-ui.md) | Slatty paused after M3.3 until slime-ui runs it; then slime-ui, Windows, packages | Accepted; amends 0020 (the work outside the interface no longer goes on meanwhile); amended by 0030, 0031 and 0034 |
| [0026](0026-ci-runs-the-checks-in-three-parts-at-once.md) | CI runs `scripts/check.sh` in three parts at once, with a Cargo cache | Accepted; amends 0024 (how the CI's budget is reached); amended by 0033 |
| [0027](0027-linux-features-refused-on-windows-not-compiled-out.md) | Linux's features refused on Windows where they start, not compiled out, until Stage 5 | Accepted |
| [0028](0028-windows-asks-before-gogs-setup-at-launch.md) | On Windows, GOG's setup runs once the person agreed, asked at launch | Accepted |
| [0029](0029-every-installed-file-flushed-on-windows.md) | On Windows every file an install writes is flushed to the disk before it takes its name | Accepted |
| [0030](0030-ported-second-onto-uwui.md) | Slatty ported onto uwui second, after the music player | Accepted; amends 0020 (no longer the pilot) and 0025 (the pause lasts until Slatty's turn; 0.3 once Stage 5's exit holds); amended by 0034 and 0036 |
| [0031](0031-iceds-figures-measured-again-during-the-pause.md) | Iced's figures measured again, by a command, during the pause | Accepted; amends 0020 (the bar is Iced's figures measured again) and 0025 (one milestone during the pause) |
| [0032](0032-gpl-kept-on-a-permissive-framework.md) | GPL-3.0-or-later kept, on a framework under MIT OR Apache-2.0 | Accepted |
| [0033](0033-a-push-of-markdown-alone-checks-only-the-links.md) | A push of Markdown alone checks only the links | Accepted; amends 0026 (a fourth part, not every part in every run) |
| [0034](0034-what-slatty-is-to-become.md) | What Slatty is to become | Accepted; amends 0009, 0013, 0018, 0019, 0021, 0022, 0023, 0024, 0025 and 0030; amended by 0036 |
| [0035](0035-slattys-emblem.md) | Slatty's emblem: its author keeps its rights; in the program at Stage 5 | Accepted |
| [0036](0036-slattys-bar-against-the-market.md) | Slatty's bar set against the market: rivals on the moments of play, a lead by family, features added | Accepted; amends 0009, 0018, 0023, 0024, 0030 and 0034 |
