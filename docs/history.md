# History

The stages that are done, newest first: what each delivered, its release, and the ADRs that hold its
decisions. The current stage is in [`PLAN.md`](PLAN.md); what changed for users is in
[`CHANGELOG.md`](../CHANGELOG.md).

## Stage 2 — Beyond GOG → 0.2.0 (2026-10-27)

One library for every game a person owns: Steam's and Epic's games, games added by hand, emulated
games, the list of games beside the library, and the couch mode for a gamepad. Not yet tried on a
Steam Deck (Stage 6), nor emulated games in each emulator ([debt.md](debt.md)).
Decisions: [ADR-0013](adr/0013-steam-read-from-its-files-played-through-its-client.md),
[ADR-0014](adr/0014-gog-never-told-of-other-stores-games.md),
[ADR-0018](adr/0018-a-couch-mode-of-its-own.md).

## Stage 1 — GOG on Linux → 0.1.0 (2026-10-21)

Sign-in, library, installs of Windows and Linux builds, updates, isolation, cloud saves and
achievements, from the command line and the interface, checked against GOG in
[compatibility.md](compatibility.md). Decisions: [ADR-0001](adr/0001-rust-with-proton-umu-and-comet-outside.md)
to [ADR-0012](adr/0012-cloud-saves-decided-by-content-hashes.md), and the interface's
[ADR-0015](adr/0015-iced-for-the-interface.md) to [ADR-0017](adr/0017-only-the-rows-in-view-are-built.md).
