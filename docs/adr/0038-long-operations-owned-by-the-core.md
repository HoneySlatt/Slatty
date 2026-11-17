# 0038. Long operations owned by the core; the interface's state free of any framework

- Status: Accepted; supersedes [0019](0019-the-interfaces-state-free-of-any-framework.md); amends
  [0034](0034-what-slatty-is-to-become.md) (where the downloads' rules live)
- Date: 2026-11-17

## Context

[ADR-0019](0019-the-interfaces-state-free-of-any-framework.md) moved the interface's state out of
the Iced crate into `slatty-app`, which names no interface framework.
[ADR-0034](0034-what-slatty-is-to-become.md) then decided that on uwui the interface's own state,
"the work each part started" included, is written in uwui's signals, over a layer of *pure
models* free of any framework that holds, among others, "the downloads' rules"; it left open where
each part of `slatty-app` goes, until Stage 5's start. uwui disposes a page's state with the page,
and [uwui-needs.md](../uwui-needs.md#checked-against-signals) says "Work that must outlive a page,
a download started from the couch mode, belongs to the core, not to the page's scope".

An audit of the architecture on 2026-11-17
([audits/2026-11-17-architecture.md](../audits/2026-11-17-architecture.md)) read how the long
operations work today, at commit `3d9c028`:

- **What starts is decided in the interface's state.** `App` has 116 public fields. What downloads
  next is decided by `start_next` and the functions it calls (`app/src/downloads.rs`), by
  `hold_for_play`, by the drawers that start a download at once, by the resume at start, and, for
  automatic updates, by `next_update`, `hold_update` and `update_ended` (`app/src/updates.rs`).
  They read about ten fields of their own (`queue`, `goes_on`, `set_aside`, `made_way`,
  `starting_now`, `preempted`, `auto_resume`, `downloads_waiting`, `interrupted`,
  `offline_stopped`) and others the window holds (the running download's token in
  `install_views`, `play`, Steam's running games, the launch being prepared), changed from most
  modules of the state.
- **Two sources of truth.** Install jobs and their states are rows of the database
  (`installer::InstallJob`); offline installers' jobs are another table; the queue's order and the
  downloads set aside are settings holding comma-separated ids. `boot::downloads_from` reconciles
  them at each start.
- **Job states are strings, read and written by the interface.** `queued`, `downloading`, `paused`
  and `failed` are constants of `installer::job`; `updating` and `updating-paused` of
  `maintenance`, and a job is an update by its state alone. The interface tells what each means
  (`Interrupted::of`): a state it does not know is read as a download cut off and resumed at start,
  or, on an installed game, deleted at start.
- **One writer, on purpose.** A second interface is refused while one is open, because "two would
  each keep a download queue and overwrite the other's" (`boot.rs`). The command line has no queue
  and no automatic updates: it installs, updates and repairs one game at a time, in the
  foreground, under the game's lock.
- **Today's limits.** One install or offline installers' download runs at a time; an update,
  change of content, verify, repair or uninstall asked from Manage runs beside it, and updates cut
  off resume at start beside the queue. After a sign-out, the queue behind the running download
  fails entry by entry rather than wait (traced in the code, not reproduced). Closing the window
  asks first (`App::running_work`) and stops every operation it lists: those above, a game added
  through its installer, and Proton builds' downloads and updates.

The decisions taken since need that work to outlive the part of the program that started it: the
tray, where Slatty closed during work stays until the work ends, then quits
([ADR-0034](0034-what-slatty-is-to-become.md)); uwui's pages disposed when they are not shown, the
couch mode's among them; a person's agents acting through a local socket, and Slatty saying why
each waiting download waits ([ADR-0036](0036-slattys-bar-against-the-market.md)).

## Decision

**Three layers, each owning one kind of state.**

- **`slatty-core` owns every long operation and its state**: every operation closing the window
  interrupts today (`App::running_work`: installs and offline installers, updates and changes of
  content, verify, repair and uninstall, cloud syncs, a game added through its installer, Proton
  builds' downloads and updates), game sessions, the queue and its order, and the schedules of
  automatic updates, of games and of Proton builds. The front ends ask it to start, pause, resume,
  reorder or discard work, and read its state and events; they hold no rule of their own on what
  runs.
- **The pure models own the rules of presentation**: the library's lists, sorting, filters and
  search, the wording shown (a download's reason to wait among it), the couch mode's focus rules,
  and what each row of Downloads shows and offers. They read the core's state and events, and own
  no work.
- **The interface's own state**, in uwui's signals in `slatty-gui`, owns what only the window has:
  the page open, focus, selection, scroll, motion, dialogs and what is being typed. Work a page
  starts that changes no data of the person's (an image read, a list made again) belongs to the
  page and is cancelled with it. Work that changes the person's data or files is asked of the core
  and outlives the page and the window.

**No framework below the window.** `slatty-core`, the pure models and `slatty-app`, while it
exists, name no interface framework, Iced or uwui; only `slatty-gui` draws. Slatty's own runtime of
messages, `update`, tasks and subscriptions (`task`, `listen`, `Effect`) goes with Iced, as uwui
gives these (ADR-0034). Where each other part of `slatty-app` goes, the pure models or uwui's
signals, is decided at Stage 5's start.

**The jobs' states are a type of the core.** Each job stores its kind (an install, an update or a
change of content, offline installers) apart from its state. Each kind's states are a closed type,
their transitions checked in one place of the core. A new state comes only with a migration, so
that an older build refuses the database rather than misread it. A state the build does not know
is read as failed all the same: shown and kept, never resumed or deleted by itself; on an installed
game it holds launching and verifying, as an unfinished update does.

**One source of truth for the queue.** The queue's order and the downloads set aside are stored
with the jobs, in the database, and change with a job's state in one transaction; no entry exists
without its job. The migration that moves them out of the settings gives the queue that
`boot::downloads_from` gives today from the same rows.

**The core decides what starts, by a pure function.** It holds the rules that decide today what
starts and what waits, those listed in the Context, and nothing else. Its inputs are every fact
those rules read today, made explicit: the jobs, their states and whether another process holds
their locks; the order; each store signed in and reachable; the games running (Slatty's sessions,
one left running by an earlier start included, and Steam's); a launch being prepared; the
person's choices (**Pause downloads while playing**, **Download now**, a pause on request, a
download set aside, automatic updates on); and what it decided before (a download paused to make
way, one that goes on first). It reads no clock, file or network of its own. It returns what
starts, what pauses and, for each download that waits, why, as a reason of a closed type whose
words the front ends choose. An entry waits while its store is signed out, and says why; it never
fails for it. Whether a queued job keeps the account it was queued for is decided in the
milestone that builds it.

**One process runs the queue, and only it changes it.** It is the interface, which already runs
alone (`locks/interface.lock`). Any process reads the queue; a change another process asks for is
refused, with the reason. **A job changes only under its own lock** (`game-<id>.lock`;
`installers-<id>.lock` for offline installers): a job whose lock another process holds runs there,
and is shown so, never resumed, failed or discarded here. An install, update or repair the command
line runs in the foreground stays outside the queue, beside it, as today. The command line shows
the queue and why each download waits; changing or running it from there waits for a need. There
is no daemon.

**A runner with explicit limits.** It runs what the pure function chooses: one install or
offline installers' download at a time, and beside it what Manage asks and updates cut off, as
today; changing either is a decision of its own. Each operation has a `CancellationToken`. A failure
GOG or Epic may recover from (`Error::is_temporary`) is tried again every minute, as today; any
other leaves the job failed. Each reader of its events gets the jobs' latest states and progress,
never a growing list: one that falls behind skips to the latest. What a front end asks of it never
waits on the interface's thread, nor on the database there: answers come as events. A runner that
stops on a panic ends the process as a crash; its jobs, cut off, resume at the next start. When the
process ends, a job left running is a job cut off, resumed at the next start, as now.

**Closing the window.** Until the tray exists, closing asks first and stops the work, as now. Once
it exists (Stage 5), the process stays while an operation runs or would start now; with the window
closed it starts nothing on a schedule, and a queue that only waits is taken up at the next start.
Whether a download held for a game keeps the process until the game ends is decided with the tray.

**When.** In Stage 5's first milestone, under Iced, before the views move, each step checked alone.
First, today's scenarios of downloads, installs, offline installers and updates, wherever their
tests are, are written to drive the state by its messages and to read only what Downloads and a
game's page show, never `App`'s fields or Iced's views; they pass on today's code. Then the queue's
and automatic updates' rules move to the core, on today's storage, and those tests pass unchanged.
Then the storage changes. Then the fields of `App` go. While the old rules and the core both exist,
one test feeds both the same random sequences of events and finds the same order, states and
starts. The other operations of `App::running_work` move before the tray, each with the scenario
"the window closed while it runs". Iced's figures are measured unchanged
([ADR-0031](0031-iceds-figures-measured-again-during-the-pause.md)). The pause is unchanged:
nothing of this is built before Stage 5 opens ([ADR-0030](0030-ported-second-onto-uwui.md)).

## Alternatives considered

- **The queue as a pure model, the work in the window's state**, as ADR-0034 wrote it: the least
  work, closest to today's code. But the command line and the agents' socket would have no queue,
  or a second copy of its rules, and the tray would need the window's state, where the rules would
  live, to outlive the window. Rejected.
- **A service of its own (a daemon)** that owns the operations, the interface and the command line
  its clients: work outlives everything, and processes no longer need to share. But it brings a
  protocol to version, a socket to secure, a start and a stop of its own, and packages (AppImage,
  Flatpak) to fit around it, for a need nobody has. Rejected; the condition to revisit it is below.
- **Any process changing the queue through the database**: the command line could reorder or
  pause it, but the process running the queue would have to be told of each change, which only
  polling the database gives today, against the budget of no wake-up at rest; and a pause written
  there cannot reach a download whose token lives in another process. Rejected until a need
  appears.
- **Today's split kept**: no work, but the port would write again rules held in fields of the state
  while it writes the views, which ADR-0034 ruled out ("each step checked alone, so that a
  regression has one cause"). Rejected.
- **The whole of `slatty-app` moved into the core**: one place for everything, but the wording and
  the presentation rules would enter the core, against the rule that front ends choose the words.
  Rejected.

## Consequences

- ADR-0019's rule stands, restated whole above.
- ADR-0034's words that put the downloads' rules among the pure models and the work each part
  started in uwui's signals are replaced: scheduling, the jobs' states and the work are the core's;
  what Downloads shows is the pure models'. The rest of ADR-0034 stands. Its open decision, and the
  roadmap's, narrows to where the rest of `slatty-app` goes.
- Stage 5's first milestone grows: a module of the core for the queue, the jobs' states and
  automatic updates, the migrations, and `slatty queue`, which shows the queue
  ([command-line.md](../command-line.md) then). It is the largest move of rules since 0.2.
- Two behaviours change on purpose: an entry waits while its store is signed out instead of
  failing, and a job in a state the build does not know is kept and holds its game instead of
  being resumed or deleted. Each starts with a failing test.
- The timers of automatic updates and of downloads waiting for a store become the core's; uwui's
  timers serve what the window shows ([uwui-needs.md](../uwui-needs.md#work-threads-and-time)).
- The tray and the agents' actions build on the core's operations, not on the window's state.
- A newer build's database is refused by an older one (`db::migrate`). Before the first of these
  migrations, a consistent copy of the database is kept beside it (SQLite's own copy: the file
  alone misses its journal); going back to an older build means restoring that copy, which the
  milestone says in [user-guide.md](../user-guide.md).
- Each scenario of today's tests keeps a test of the same name that passes against the core; the
  core's queue joins the weekly mutation run of
  [ADR-0036](0036-slattys-bar-against-the-market.md).

## Validation and reconsideration

Reviewed reasoning, from the code at `3d9c028`, attacked by a fresh review; nothing measured. The
milestone checks:

| Scenario | Acceptance criterion |
|---|---|
| The window closed while a game downloads, then Slatty started again | The download resumes first, from where it was, as today |
| Slatty killed at each step of a job's change of state | At the next start no job runs twice, none is lost, a pause on request stays paused, the order is kept |
| A 0.2 database whose settings hold a queue (entries without a job, set aside, offline installers', a started download in the order, a queued job missing from it) | The same queue, states and set-aside entries as `downloads_from` gives today from the same rows; a copy of the database kept first |
| A job stored in a state this build does not know | Shown as failed, still there at the next start; on an installed game, launching and verifying refused until it is resolved |
| A game installed from the command line while the interface runs | The interface neither resumes, fails nor discards it; the queue goes on beside it |
| The command line asked to change the queue while the interface runs | Refused, with the reason; nothing changes |
| Signed out of GOG while a game downloads, two more queued | The download goes on; the queue waits, its order kept, says why, and goes on at the next sign-in |
| Epic out of reach, a GOG install queued behind an Epic one | As today: the queue waits, its order kept |
| A game started, Slatty's or Steam's, while a download runs, **Pause downloads while playing** on | The download pauses and goes on first once the game ends |
| Slatty started again while a game it started earlier still runs | Downloads wait for that game |
| **Download now** while a game runs | The row starts; the download it made way for goes on after it |
| An automatic update running when a game starts, the setting off, then on | As today: it goes on (off), or pauses and goes on first after the game (on) |
| An update and an install both cut off, then Slatty started | As today: the update resumes beside the install |
| Install pressed in the couch mode, the couch mode left before the plan answers | The install starts |
| A reader that stops reading during a long download | Its memory stays bounded; it sees the latest state when it reads again |
| The runner panics during a download | The process ends as after a crash; at the next start the download resumes as cut off |
| Each rule the pure function holds | A test of it alone, with no clock, file or network |

To be proposed to the owner again if a need appears for work that must run while no Slatty process
does (a download through a reboot, a schedule kept with the program closed): that is what a daemon
would give; or for the command line to change the queue while the interface runs, which needs the
running process to be told without polling.
