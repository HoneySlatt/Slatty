# Glossary

The terms this project defines, in alphabetical order. Each says what it means here, and links to
where it is decided or built.

**Build id.** Which build of a game is installed: GOG's build id for a Windows build from the content
system, `linux:<version>` for a Linux build from an offline installer
([ADR-0007](adr/0007-linux-builds-from-offline-installers.md)).

**Card.** A game's cover in the library's grid, with its title and buttons over it while hovered;
its width is the cover size chosen in Settings.

**Copy at the size drawn.** A cover or key art made once at the size the window draws it, from which
the interface shows it ([ADR-0016](adr/0016-images-shown-from-files-at-the-size-drawn.md),
`app/src/scaled.rs`).

**Couch mode.** The interface's pages for a gamepad and a television or a Steam Deck, full screen,
with a focus of their own ([ADR-0018](adr/0018-a-couch-mode-of-its-own.md)).

**Fake profile.** A data, cache and configuration folder of made-up games and a made-up account,
used to run and measure the program without touching a person's own data
([testing.md](testing.md#the-application-at-10000-games)).

**Galaxy service.** Comet's dummy `GalaxyCommunication` Windows service, registered in a game's
prefix so that Galaxy SDK versions that look for it reach Comet
([ADR-0011](adr/0011-comet-supervised-beside-the-game.md)).

**Game id.** What names a game in Slatty: GOG's product id, `steam-<app id>`, `epic-<app name>`,
`local-<number>` for a game added by hand, `emu-<number>` for an emulated one; `store::of` tells
which ([AGENTS.md](../AGENTS.md#files-and-data-safety)).

**Home (of a game).** The home folder an isolated game sees instead of the person's, `homes/<id>`
([ADR-0009](adr/0009-isolation-through-umus-container.md)).

**Interrupted.** An install, update or offline installer stopped before its end (paused, failed, or
cut off by a close or a crash), shown above the library to be resumed or dropped.

**Isolation.** Running a game in umu's container with a home of its own and the tools read-only, so
that it sees nothing of the person's files and cannot change what other games run
([ADR-0009](adr/0009-isolation-through-umus-container.md)).

**Key art.** A game's wide background image, shown behind its page and on the couch mode's Home.

**Kept lists.** The games each tab lists, kept between messages until one that can change them
(`library::Lists`, `Message::keeps_lists`; [ADR-0017](adr/0017-only-the-rows-in-view-are-built.md)).

**List beside the library.** The list of every game's name and icon beside the grid, as in Steam
(`app/src/sidebar.rs`).

**Milestone.** A small, verifiable step of a stage, with its goal, what it delivers, when it is done
and how it is tested ([PLAN.md](PLAN.md)).

**Offline installer.** GOG's installer of a whole game, a file to download and keep; Linux builds are
read from them file by file ([ADR-0007](adr/0007-linux-builds-from-offline-installers.md)).

**Overview.** Per game, what the library's filters and cards read beside the game itself: its
achievement counts, whether it has cloud saves, its play time; cached per account
(`core/src/overview.rs`).

**Prefix.** The Wine folder a Windows game runs in, with its own `C:` drive, registry and saves.

**Proton build.** A build of Proton (GE-Proton, Proton-CachyOS, UMU-Proton) that Slatty downloads
into a folder of its own, followed through `<project>-latest`
([ADR-0010](adr/0010-proton-builds-downloaded-by-slatty.md)).

**Queue.** The downloads waiting their turn, in an order the person sets by dragging, kept across
restarts.

**Session.** A game's run, from its start until its last process ends, followed by a supervisor;
its length is the play time reported
([ADR-0008](adr/0008-sessions-followed-by-a-subreaper-supervisor.md)).

**Shelf.** A part of the library the toolbar picks: all, installed, favorites, hidden.

**Stage.** A step of the roadmap, with its goal and its exit criterion ([ROADMAP.md](ROADMAP.md)).

**Supervisor.** The copy of Slatty's own binary that starts a game and follows its session until no
descendant is left ([ADR-0008](adr/0008-sessions-followed-by-a-subreaper-supervisor.md)).

**Twin.** A Steam game that is also a GOG game the person owns; the library shows them as one
([ADR-0013](adr/0013-steam-read-from-its-files-played-through-its-client.md)).
