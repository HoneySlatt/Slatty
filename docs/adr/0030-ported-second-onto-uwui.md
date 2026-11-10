# 0030. Slatty ported onto uwui second, after the music player

- Status: Accepted; amends [0020](0020-gpui-set-aside-for-slime-ui.md) (Slatty is no longer the
  pilot: it is the second of four programs ported, onto the framework now named uwui) and
  [0025](0025-paused-after-m3-3-until-slime-ui.md) (the pause lasts until Slatty's turn in
  SlimeEngine's Stage 6d; 0.3 is released once Stage 5's exit holds); amended by
  [0034](0034-what-slatty-is-to-become.md) (ported whole in uwui's first stage; Stage 5's open
  decisions taken; the market's bar in its exit) and
  [0036](0036-slattys-bar-against-the-market.md) (build times measured during the pause; macOS an
  open decision)
- Date: 2026-11-05

## Context

[ADR-0020](0020-gpui-set-aside-for-slime-ui.md) made Slatty the pilot of SlimeEngine's interface
framework, then called slime-ui: the first program ported onto it, during SlimeEngine's Stage 6d.
[ADR-0025](0025-paused-after-m3-3-until-slime-ui.md) paused Slatty after M3.3 until that stage
"can run an application", then released the interface on it as 0.3, which the roadmap placed after
SlimeEngine 0.8, the end of that stage.

On 2026-11-05 the owner took decisions in SlimeEngine that change this. Their sources are
[SlimeEngine's ADRs](https://github.com/HoneySlatt/SlimeEngine/tree/main/docs/adr) 0175 to 0182,
above all ADR-0181, the framework's scope:

- The framework is named **uwui** (SlimeEngine's ADR-0179). Its crates, `slime-ui` and
  `slime-ui-render`, become `uwui` and `uwui-render` in SlimeEngine's milestone M6c.N.
- uwui is the base of all the owner's interfaces, in a repository of its own from Stage 6d, once it
  is decoupled from the engine. Until then it lives in SlimeEngine's repository.
- Four programs are ported before uwui's API is declared stable, in this order: Opium, the owner's
  music player, on the desktop; Slatty; Opium's Android prototype; Magnolia, a media application.
  Each has a checkpoint where the owner uses it.
- Each runs on uwui at least as well as on its present framework, on figures measured first. For
  Slatty, those of SlimeEngine's ADR-0170, measured with Iced: 10,000 games shown in 86 to 91 ms,
  scrolling at 4.15 ms a frame by median with no frame over 16.7 ms, no CPU time at rest, 116 to
  135 MB of memory ([measurements.md](../measurements.md)).
- uwui is embedded in a host and never owns the loop. On the desktop a host on winit, which uwui
  provides, runs the whole program. A project's logic never depends on uwui.
- Its targets: the desktop on Linux and Windows, Android, the web in a canvas; macOS and iOS within
  reach, without a port. Mouse, keyboard, touch and gamepads from its foundations, with one model of
  focus (Tab and directions). Its look comes from themes in TOML files, read through `serde` behind
  a feature; without it, a theme is written in code.
- Its lower desktop tier for applications, not measured: a laptop of 2015, two cores, an Intel
  HD 520, 4 GB, 1080p at 60 Hz.

Before uwui's API is declared stable, SlimeEngine's Stage 6d decouples it, moves it to its own
repository and runs two short trials, Android and the web, before the ports begin.

## Decision

**Slatty is ported second.** Opium's port on the desktop comes first. Slatty's port, Stage 5, is
done in this repository during SlimeEngine's Stage 6d, with a checkpoint where the owner uses Slatty
day to day.

**The pause lasts until Slatty's turn.** No milestone starts until the plan of SlimeEngine's
Stage 6d opens Slatty's port, after Opium's port on the desktop. CI's weekly run goes on. The only
exceptions are M3.3's check on Windows and its fixes, and the measurement of
[ADR-0031](0031-iceds-figures-measured-again-during-the-pause.md).

**The exit: at least as well as on Iced, then the quality bar.** Slatty runs on uwui at least as
well as on Iced on each figure measured first: the library shown, scrolling, rest under Wayland and
Xwayland, memory. At the port, Iced and uwui are measured side by side by the same command
([ADR-0031](0031-iceds-figures-measured-again-during-the-pause.md)). Slatty's own
[quality bar](../ROADMAP.md#quality-bar) applies on top, and it is stricter where it says so: for
example, no frame above 6.25 ms while scrolling, against 16.7 ms.

**0.3 is released once Stage 5's exit holds**, without waiting for uwui's API to be declared
stable. It depends on a pinned revision of uwui. Slatty then follows the API's changes that the
later ports bring.

**Where uwui comes from.** From SlimeEngine's repository until uwui moves to its own, under the
names `uwui` and `uwui-render` from M6c.N; then from its own repository. Slatty takes it at its
port, which SlimeEngine plans after the move. Adding it follows
[CONTRIBUTING.md](../../CONTRIBUTING.md#dependencies), and `deny.toml` will have to allow its git
source.

**Slatty's logic stays its own.** `slatty-core` and `slatty-app` never depend on uwui, as
[ADR-0019](0019-the-interfaces-state-free-of-any-framework.md) already requires. Only `slatty-gui`
draws with it, on uwui's desktop host.

**Slatty's platforms do not change.** It stays on Linux and Windows
([ADR-0022](0022-windows-hosts.md)). uwui reaching Android, the web, macOS and iOS gives Slatty no
target of its own. Slatty's lowest tier ([ADR-0024](0024-hardware-tiers-and-the-budgets-of-the-whole-loop.md))
belongs to the same class of machine as uwui's lower desktop tier: the same GPU, cores, memory and
display, with a hard disk.

**In prose, the framework is uwui.** Living documents name it uwui. Earlier ADRs keep slime-ui, the
name it had when they were written.

## Alternatives considered

- **Slatty as the pilot, as ADR-0020 decided**: rejected in SlimeEngine (ADR-0178, ADR-0181). The
  music player carries the effects that are hardest to build, and it is the program the owner uses
  most.
- **Work outside the interface while Opium is ported** (Stage 4's packages, fuzzing, the keyring's
  wake-ups): Slatty would progress meanwhile, but this is what ADR-0025 had rejected. The owner's
  effort stays on uwui. Rejected by the owner.
- **0.3 after uwui's API is declared stable**, as "after SlimeEngine 0.8" meant: a release on a
  stable API, but Slatty ported and not released during two more ports, and Windows (0.4) pushed
  back further. Rejected by the owner.

## Consequences

- The pause is longer: it covers uwui's decoupling, its move, the two trials and Opium's port.
  Windows (0.4) and the packages (0.5) come later accordingly.
- Slatty is ported onto a uwui that Opium has already tried, in a repository of its own.
- 0.3 rests on an API that will still change: the ports of Opium on Android and of Magnolia may ask
  Slatty for changes after its release.
- Stage 5 decides at its start what uwui's foundations change in Slatty: the theme file that people
  already have (`theme.toml`, [theming.md](../theming.md)) kept or migrated to uwui's themes, and the
  couch mode's gamepads (gilrs, `slatty-app`'s `pad`) and focus, kept or taken from uwui.
