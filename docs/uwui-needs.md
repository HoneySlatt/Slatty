# What Slatty asks of uwui

Slatty's interface moves from Iced to uwui, the owner's interface framework, at
[Stage 5](ROADMAP.md#stage-5--the-interface-on-uwui-windows-included--03)
([ADR-0030](adr/0030-ported-second-onto-uwui.md), [ADR-0034](adr/0034-what-slatty-is-to-become.md)).
This file lists what Slatty asks of uwui. It separates the general needs, which uwui may hold, from
Slatty's own, which Slatty builds on them.

It comes from two sources:

- the inventory of what Slatty's interface asks of a framework, made from the code on 2026-11-04
  (commit `b9c6169`), need by need;
- the owner's interview of 2026-11-07 on what Slatty is to become
  ([ADR-0034](adr/0034-what-slatty-is-to-become.md)), before any code.

SlimeEngine's milestone M6d.1b3b chose how an application's state changes uwui's interface on
2026-11-07, from the lists of three other programs: signals
([SlimeEngine's ADR-0205](https://github.com/HoneySlatt/SlimeEngine/blob/main/docs/adr/0205-signals-change-uwuis-interface.md)).
The same day, SlimeEngine's
[ADR-0206](https://github.com/HoneySlatt/SlimeEngine/blob/main/docs/adr/0206-slatty-ported-again-the-daw-later.md)
brought Slatty back among the programs of uwui's first stage, and checks that choice against this
list: [its section below](#checked-against-signals) does the check. Nothing here is measured on
uwui yet.

## How to read it

- uwui takes an application's need only if it is **general**: several kinds of applications need
  it. A need that serves only Slatty, or only one benchmark, stays in Slatty.
- **Mark**: *indispensable* when Slatty cannot meet its parity with Iced, its
  [quality bar](ROADMAP.md#quality-bar) or a decision of ADR-0034 without it; *wished* otherwise.
  Each mark names the stage of Slatty's [roadmap](ROADMAP.md#4-stages) that first needs it.
- **In uwui's requirements**: whether SlimeEngine's
  [ADR-0190](https://github.com/HoneySlatt/SlimeEngine/blob/main/docs/adr/0190-uwuis-requirements.md),
  its ADR-0205 and [uwui's roadmap](https://github.com/HoneySlatt/uwui/blob/main/docs/ROADMAP.md)
  hold the need, as read on 2026-11-07; the rows that SlimeEngine's
  [ADR-0213](https://github.com/HoneySlatt/SlimeEngine/blob/main/docs/adr/0213-uwuis-reach-its-lead-its-themes-and-agents.md),
  [ADR-0215](https://github.com/HoneySlatt/SlimeEngine/blob/main/docs/adr/0215-the-engine-measured-against-the-market.md)
  and [ADR-0218](https://github.com/HoneySlatt/SlimeEngine/blob/main/docs/adr/0218-one-core-for-agents-in-uwui.md)
  touch, and those Slatty's [ADR-0036](adr/0036-slattys-bar-against-the-market.md) adds, as read on
  2026-11-10. *Yes*: they hold it. *Precision*: they hold it in general terms, and this list says
  exactly what is needed. *New*: they do not hold it.

## How Slatty's state changes its interface

Slatty's interface state will be written in uwui's signals, beside a layer of pure models that
names no framework (ADR-0034). These are the changes, from the most frequent to the rarest. In
every case the interface's thread never waits on the core, and nothing is drawn once a change is
over.

- **Every frame while something moves.** The grid of 10,000 covers scrolls at 240 Hz: rows enter
  and leave, covers already made at their size come from the disk. A hovered card's veil fades and
  its cover grows by 5 %. The couch mode's focus moves and its cover grows. Key art fades in. Only
  the rows entering should run Slatty's code.
- **In bursts, from other threads.** Downloads' progress, several at once, at most four updates a
  second. Copies of covers made in the background, 10,000 at a first start over about 21 s.
  A game's session reported by its supervisor; a cloud sync's steps; Steam's client polled while a
  download runs there. Each is gathered into the next frame.
- **A list of 10,000 games changed by key.** A game installed, removed, made a favourite or
  hidden changes its card and may move it between shelves; a sort or a filter reorders the whole
  list; a search filters it as the person types. The item focused, the selection and the scroll are
  kept.
- **By the person.** A page opens and the library keeps its scroll behind it. Drawers, dialogs,
  menus and notices come as layers over the page, which is not built again.
- **Rarely.** The theme, the language or the scale changes; the desktop and the couch mode, two
  sets of pages over the same state, take turns.

## Checked against signals

ADR-0206 asks whether signals, as ADR-0205 chose them, cover each need of Slatty's that is general
and weighs on a state model. On what ADR-0205 says, none is left uncovered; three depend on what it
left open.

| Need | How signals meet it | Covered |
|---|---|---|
| A list of 10,000 games, 100,000 for a large emulated library, changed by key at the cost of the change, the focus, selection and scroll kept | A list makes the rows that come in, each in its scope, and disposes those that leave; only the rows in view are mounted. The games must stay one keyed collection, not a signal each: ADR-0205 finds that signals hold more | Yes, once "a list fed by its changes by key" is built (uwui's first stage) |
| Lists made by the pure models away from the interface's thread (sorting, filtering, searching), then shown whole | One signal set once per new list; the rows' keys keep what stays | Yes |
| Changes from the core's tokio threads, in bursts, gathered into the next frame, the interface's thread never waiting | ADR-0205 leaves how other threads reach signals to M6d.1c, with the executor | Open: M6d.1c |
| Motion at each frame (a hovered card, the couch mode's focus, key art fading) | uwui's motion, or its live values, without Slatty's code at each frame | Yes |
| Relative times ("3 minutes ago") and timers that wake only while shown | ADR-0205 leaves timers at a granularity, read by what is mounted, to uwui's first stage | Open: uwui's first stage |
| Two sets of pages, the desktop's and the couch mode's, over one state, taking turns | Both read the same signals; the pages of one are disposed when the other shows. Work that must outlive a page, a download started from the couch mode, belongs to the core, not to the page's scope | Yes |
| The state kept across a reload of Slatty's code | ADR-0205 leaves migrating a signal's state across a reload to M6d.1c | Open: M6d.1c; wished for Slatty |

## General needs

### The window and the system

| Need | Slatty's use | Mark | In uwui's requirements |
|---|---|---|---|
| A title bar drawn by the application, holding its own controls, with the system's moves, resizes and buttons on Wayland, X11 and Windows; the system's decorations as an option | The search and the tabs in the title bar | Indispensable: Stage 5 | Yes: decorations, the system's or the application's |
| A window blurred over the desktop where the compositor allows it, opaque otherwise, switched at run time | Glass on the desktop; opaque on battery, on the lowest tier, under reduced transparency | Indispensable: Stage 5 | Yes: transparent and blurred windows |
| Full screen turned on and off at run time, and known | The couch mode | Indispensable: Stage 5 | Precision |
| The request to close the window received by the application, which decides | A question, or the tray, while work runs | Indispensable: Stage 5 | Precision |
| An icon in the system's tray with a menu, the program living without a window, the window opened again | Downloads and saves going on after the window is closed | Indispensable: Stage 5 | Yes: the tray |
| One instance, the first one's window brought to the front | A second launch, a launch from a Steam shortcut | Indispensable: Stage 5 | Yes |
| Notifications of the system with an image | A download done, with its cover | Indispensable: Stage 5 | Yes |
| The system's dialogs to choose files and folders, through portals | Games, ROMs, a folder for saves or installers | Indispensable: Stage 5 | Yes |
| The clipboard read and written | Pasting GOG's sign-in address | Indispensable: Stage 5 | Precision |
| Focus of the window gained and lost, as events | Notifications only when Slatty is behind; gamepads only when it is in front | Indispensable: Stage 5 | Precision |
| The display's scale followed, and a scale of the application's own on top | The couch mode sized from a 1920 × 1080 base | Indispensable: Stage 5 | Precision: densities |
| Running under gamescope (the Steam Deck's Game Mode and living-room sessions): detected, full screen, scale 1 under its Xwayland, no decorations | The couch mode on the Deck | Indispensable: Stage 6 | New |
| Addresses of a scheme the application registers (`nxm://`), received by the running instance | Nexus Mods' "Mod manager download" | Indispensable: Stage 10 | Yes: deep links |
| Files dropped from a file manager | A mod's archive, a ROM, a game's folder | Wished: Stage 10 | Yes |
| A start with the session, full screen | A session of Slatty's own on a living room's computer | Wished: after Stage 6 | Yes |

### Input and focus

| Need | Slatty's use | Mark | In uwui's requirements |
|---|---|---|---|
| Gamepads read by uwui: SDL's mappings on Linux, Windows' own APIs, plugged and unplugged, several at once | The couch mode, which today reads them itself through gilrs | Indispensable: Stage 5 | Precision: gamepads from the foundations |
| The kind of each gamepad (Xbox, PlayStation, Nintendo, Steam Deck), so that the buttons shown match it | The buttons to press shown in the couch mode | Indispensable: Stage 5 | New |
| One focus for the mouse, the keyboard and gamepads, moved by directions through a grid of 10,000 virtual items, into rows not built yet, the scroll following | The couch mode's grid and shelves | Indispensable: Stage 5 | Precision: one model of focus |
| The focus kept per page and given back when the person returns | Back from a game's page to the same cover | Indispensable: Stage 5 | Precision |
| A held direction repeated, faster as it is held | Moving through 10,000 covers with a stick | Indispensable: Stage 5 | Precision |
| An event when the focus moves or an element is activated, that the application can hook | The couch mode's sounds, which uwui does not make | Indispensable: Stage 6 | New |
| Chosen gamepad buttons heard while the window is not in front, and the window brought to the front at that moment | The Guide button during a game; on Wayland, bringing a window forward needs the compositor's token (`xdg-activation`), which a window behind does not have; only that button read, not every event of the game's pad | Indispensable: Stage 6 | New |
| Controller mappings given to SDL, read and written per game, without a virtual input device | Controller mappings per game, inside the game's sandbox ([ADR-0036](adr/0036-slattys-bar-against-the-market.md)) | Wished: Stage 6 | New |
| A keyboard on screen driven by a gamepad, read by screen readers | Searching in the couch mode, on a television and on the Deck | Indispensable: Stage 5 | Yes |
| Text fields: masked ones, submitted by Enter, the input method's text | 13 fields, two masked (API keys) | Indispensable: Stage 5 | Yes |
| A click of the right button with its place; menus at that place | A cover's menu | Indispensable: Stage 5 | Yes |
| Hovering an element, entered and left | A card's veil | Indispensable: Stage 5 | Yes |
| A drag that reorders a list, and a drag of an edge, the pointer held outside the window | The queue of downloads; the width of the list beside the library | Indispensable: Stage 5 | Precision |

### Lists and images

| Need | Slatty's use | Mark | In uwui's requirements |
|---|---|---|---|
| A grid of thousands of thumbnails, only the rows in view made, each row as tall as its tallest card, scrolled at the display's rate with no stall when a row of new images enters | The library at 10,000 games; Achievements; the list beside the library | Indispensable: Stage 5 | Precision: Iced stalls about 30 ms at each new row of covers today |
| A list of 10,000 rows changed in place by key, at the cost of the change, sorted and filtered again, the focus, selection and scroll kept | Installs, favourites, sorting, search as one types | Indispensable: Stage 5 | Yes: ADR-0205 leaves it to uwui's first stage |
| Images from files on disk (JPEG, PNG, WebP, GIF, BMP), decoded away from the interface's thread, freed from the GPU's memory once out of view, within a bounded memory | Covers and key art; 140 MB resident and 256 MB of video memory at 10,000 covers | Indispensable: Stage 5 | Precision: the budgets; GPUI kept 455 MB after a scroll |
| An image drawn filling its place, cropped, with rounded corners, an opacity and a scale, and one image fading into another | Cards, hovered covers, key art, the couch mode's Home | Indispensable: Stage 5 | Yes |
| Scrolling driven by the application: to an item, back to where it was, to the end, and the places of sections | The library found as it was left; Settings' sections | Indispensable: Stage 5 | Precision |
| Layers over the page (drawers, dialogs, menus, notices), the page beneath kept whole | Every dialog | Indispensable: Stage 5 | Yes |
| Shelves and carousels | The couch mode's Home | Indispensable: Stage 5 | Yes |
| Video in an element, decoded by the GPU, with sound, started on demand | A game's trailer | Indispensable: Stage 8 | Yes: video, optional |
| Simple charts | Play time per week and per store | Indispensable: Stage 8 | Yes: charts |

### Drawing and motion

| Need | Slatty's use | Mark | In uwui's requirements |
|---|---|---|---|
| An element that flies from one screen to the next, interrupted without a jump | A cover from the grid to its game's page and back | Indispensable: Stage 5 | Yes |
| Motion that runs once, asks for frames only while it runs, takes its durations from the theme, and stops under reduced motion | Pages, drawers, dialogs, notices, the focus | Indispensable: Stage 5 | Yes |
| Blurred shadows, linear gradients, rounded borders, blur behind a panel | Panels, menus, progress bars, key art's fade | Indispensable: Stage 5 | Yes |
| Effects lightened on the lowest tier, on battery, under reduced motion and transparency | Every effect above | Indispensable: Stage 5 | Yes |
| Icons drawn tinted (Lucide), and a logo in SVG | 46 icons and the logo | Indispensable: Stage 5 | Yes |
| The couch density: sizes for a screen across the room | The couch mode | Indispensable: Stage 5 | Yes |

### Text, themes and accessibility

| Need | Slatty's use | Mark | In uwui's requirements |
|---|---|---|---|
| Every writing system, fallback fonts and colour emoji, right to left | Games' titles from every store | Indispensable: Stage 5 | Yes |
| An embedded font, or any font of the system chosen by the person, their list read only when asked; bold standing in for a missing semibold | Geist, or the person's font | Indispensable: Stage 5 | Precision: the person's choice of a system font |
| A theme file of the person's, in uwui's format, read with its errors explained, loaded again at run time | `theme.toml`, converted from Slatty's format | Indispensable: Stage 5 | Precision |
| Translation with plurals, relative times and sizes, the language changed live | French with the port | Indispensable: Stage 5 | Yes: Fluent |
| WCAG 2.2 AA, screen readers on Linux and Windows, a grid of 10,000 virtual items exposed lazily, progress announced | The desktop and the couch mode, read aloud | Indispensable: Stage 5 | Precision: grids exposed lazily |
| Figures of one width | Sizes and progress that do not move as they change | Wished: Stage 5 | Precision: Opium asks the same |

### Work, threads and time

| Need | Slatty's use | Mark | In uwui's requirements |
|---|---|---|---|
| State changed from other threads and tokio's tasks, gathered into the next frame, the interface's thread never waiting | The core's events: downloads, sessions, syncs | Indispensable: Stage 5 | Precision: ADR-0205 leaves it to M6d.1c |
| Signals fed by a state outside uwui, in batches, by parts and by key | The pure models give the lists; the interface shows them | Indispensable: Stage 5 | New |
| Work bound to the part of the interface that started it, cancelled with it, bridged to a cancellation token of the core | Leaving a page stops what it asked | Indispensable: Stage 5 | Yes |
| tokio's adapter for heavy work | The core runs on tokio | Indispensable: Stage 5 | Yes |
| Timers that wake only while they matter, and relative times that change at the granularity shown, only while shown | Updates checked every 6 hours, downloads retried every minute; "3 minutes ago" | Indispensable: Stage 5 | Precision: ADR-0205 leaves it to uwui's first stage |
| A window shown with its first frame within the budget, the state read on a thread started before the window, nothing slow before the first frame | The library shown within 95 ms warm, the compositor's answer included | Indispensable: Stage 5 | Precision: Slatty's budget is stricter than uwui's 100 ms |
| No frame, processor time or wake-up at rest, under Wayland and Xwayland | 0 in 60 s at rest | Indispensable: Stage 5 | Yes |
| uwui's own threads few, and its decoding of images at the lowest priority | Games run beside Slatty on two cores on the lowest tier | Indispensable: Stage 5 | Precision: Magnolia asks for few threads |
| The window's GPU memory and caches freed on demand while the program lives on, down to a blank window's, then shown again within 100 ms, its first screen's images kept decoded in memory | Slatty stepping back while a game runs ([ADR-0036](adr/0036-slattys-bar-against-the-market.md)) | Indispensable: Stage 5 | New |
| Crashes reported on the machine (a panic, an abort, a signal, the GPU lost) and hours of use counted, nothing sent; secrets kept out of core dumps | Slatty's crash reports and its 99.9 % of hours without a crash | Indispensable: Stage 5 | Yes: the host's local crash reports (SlimeEngine's M6d.7) and uwui's hours without a crash; precision: core dumps |

### Tests, measures and platforms

| Need | Slatty's use | Mark | In uwui's requirements |
|---|---|---|---|
| Tests without a window: elements found by text or path, clicks, typing, gamepad input and focus moves simulated, time simulated, no task run | The interface's tests, ported from `iced_test` | Indispensable: Stage 5 | Precision: gamepad input in tests |
| Images of reference for each theme, for the desktop and the couch density, at scales 1 and 2 | Rendering checked in CI | Indispensable: Stage 5 | Yes |
| Agents driving the running program, named and consented to, built into test builds only | Tester agents at each stage's end, on the fake profile and fake servers | Indispensable: Stage 5 | Yes: one core for agents and its MCP server (SlimeEngine's M6d.9a); precision: test builds only |
| Actions declared once for a person's own agents, offered on a local socket only that person can open | Read-only actions on local data first: the library, the saves' state, play time | Wished: Stage 5 | Yes: actions declared once (SlimeEngine's M6d.9b); precision: the socket |
| Missed refreshes counted from the compositor's presentation feedback | No refresh missed on everything that moves | Indispensable: Stage 5 | Precision |
| A blank window measured beside each screen, and input to photon timed by a light sensor | The lead of three times; input to photon equal to the blank window's | Indispensable: Stage 5 | Yes: SlimeEngine's M6d.7 and Stage 6e |
| Video memory read with what an integrated GPU borrows from main memory (GTT) | The lowest tier's and the Deck's budgets | Indispensable: Stage 5 | New |
| HDR images | Key art on the Deck OLED and HDR displays | Wished: when uwui offers it | Yes: after uwui's first stage |
| Linux under Wayland and Xwayland, Windows 10 and 11, with no line of the application written for one | Slatty's platforms; the interface holds no code per platform ([ADR-0036](adr/0036-slattys-bar-against-the-market.md)) | Indispensable: Stage 5 | Yes: uwui's budget of porting |
| The lowest tier smooth at 60 Hz with effects reduced: an Athlon Silver 3050U, its Radeon Vega, 4 GB | ADR-0034's lowest tier, uwui's too | Indispensable: Stage 5 | Yes |
| Drawing without a GPU driver that works, chosen at start or after a failed start | Iced fell back to its CPU renderer; Slatty's "Vulkan only" setting | Wished: Stage 5 | Precision: the CPU renderer, optional |
| Run from an AppImage, its own libraries within, on SteamOS and other distributions | Slatty's package for the Deck and other distributions ([ADR-0034](adr/0034-what-slatty-is-to-become.md)) | Indispensable: Stage 6 | Precision: ADR-0190's packages name AppImage |
| Run inside Flatpak: portals for files, notifications, the tray and addresses | Flathub, only if Flatpak's sandbox can give Slatty's guarantees | Wished: Stage 6 | Precision: Flatpak's sandbox |

## Slatty's own

These stay in Slatty. They ask uwui only for the general needs above.

- Everything `slatty-core` does: the stores, installs, Proton, isolation, saves, Comet, sessions,
  the keyring, the sign-in in the browser.
- The pure models: the library's lists, natural sort keys, filters and shelves, the couch mode's
  rules, the queue's rules, the wording shown.
- The copies of covers and key art at the size drawn, and their cache on disk
  ([ADR-0016](adr/0016-images-shown-from-files-at-the-size-drawn.md)); uwui only shows the files.
- The desktop's pages and the couch mode's pages, Home and a game's page among them.
- The couch mode's sounds, and the sound they play through.
- What the Guide button does; finding gamescope and writing Steam's shortcuts.
- Converting the theme file people already have to uwui's format.
- Where trailers and screenshots come from; uwui plays and shows them.
- The tray's menu and the notifications' wording.

## A screen of reference for Slatty

uwui already measures Slatty's library as one of its screens of reference
([uwui's quality bar](https://github.com/HoneySlatt/uwui/blob/main/docs/ROADMAP.md#screens-of-reference)):
10,000 covers scrolled 16 logical pixels a frame for 1,200 frames, then a game's page opened, its
key art fading in. Its floor is Slatty on Iced, measured on 2026-11-06; its bar, the best of Steam,
gThumb and Nautilus on each criterion
([uwui's measurements](https://github.com/HoneySlatt/uwui/blob/main/docs/measurements.md#slattys-library-with-iced-2026-11-06)).
Slatty's port must beat both (ADR-0034); beating each line is the least. uwui also asks a lead: its
*own cost*, what a program adds above a *blank window* that only clears and presents, at most a third
of the best rival's, and the blank window's figure at rest and from input to present. Slatty measures
it, and a figure missed is debt that does not hold 0.3 back
([ADR-0036](adr/0036-slattys-bar-against-the-market.md)). The couch mode has no screen of reference:
what its gamepad, focus and density cost is measured by Slatty's own budgets, a move shown in the
next frame, and its input to photon against Steam's Game Mode.

## What Stage 5 needs first

Slatty's port starts when uwui's first stage opens it. Every need marked *indispensable: Stage 5*
must exist then, or be built in uwui during the port, under uwui's rule that a need enters it only
if it is general. Those that matter most to the order of uwui's work:

- the grid of thousands of thumbnails, virtual, with a list changed by key at the cost of the
  change;
- images from files, freed from the GPU, within bounded memory;
- gamepads, one focus moved by directions into virtual rows, and the keyboard on screen;
- signals fed by an outside state in batches, and changes from tokio's threads gathered into the
  next frame;
- the title bar drawn by the application, the tray, glass over the desktop;
- the element that flies between screens;
- accessibility of a virtual grid, translation, tests without a window with a gamepad;
- the window's GPU memory freed while a game runs and shown again within 100 ms, missed refreshes
  counted from the compositor, video memory read with what an integrated GPU borrows.

Several of these are in uwui's plan for after its first stage: translation, accessibility at level
AA, the tray. Slatty's port asks for them sooner, as Opium's first release does.

Those for later stages (gamescope, the Guide button heard in the background, focus events for
sounds, the AppImage, deep links, video, charts) are put to uwui when each stage is planned.
