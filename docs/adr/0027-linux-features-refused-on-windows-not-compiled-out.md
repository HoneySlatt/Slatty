# 0027. Linux's features refused on Windows where they start, not compiled out, until Stage 5

- Status: Accepted
- Date: 2026-11-05

## Context

[ADR-0022](0022-windows-hosts.md) keeps what only Linux has Linux's: umu and Proton, Wine,
isolation, GOG's Linux builds, Proton builds, the Galaxy service in a Wine prefix. The PLAN asked
M3.3 to build those modules on Linux alone, their callers on Windows getting
`Error::Unsupported`. Before M3.3 they built for Windows and failed there late: a game set to umu
failed on a missing `umu-run`.

The core is not their only caller. `slatty-app` and the Iced interface use `protons` and the
Proton settings: `#[cfg(unix)]` on the modules would have to reach Iced's views and messages,
which receive bug fixes only and go with Stage 5
([ADR-0020](0020-gpui-set-aside-for-slime-ui.md)).

## Decision

**The modules build everywhere; each way into them refuses on Windows**, through one function,
`platform::linux_only`, which returns `Error::Unsupported` on Windows and nothing on Linux. It is
asked:

- before a launch command through Proton or Wine (`runner::windows_spec`), or for a Linux game
  (`runner::launch_spec`);
- before installing a GOG Linux build (`installer::install`) or importing a Windows build with
  Wine or Proton (`install::from_dir`);
- before setting a game's Proton build or isolating it (`install::set_proton`, `set_isolated`);
- before downloading a Proton build (`protons::check_allowed`, `protons::install`);
- before registering the Galaxy service in a prefix (`galaxy_service::ensure`);
- by the command line for `--proton` and its `proton` commands;
- by umu's game id lookup and runtime refresh before a launch (`umu::resolve`,
  `runner::refresh_runtimes`), which then do nothing, the launch refusing after them.

Tests under Wine check each refusal that returns an error; the tests of those features run on Linux
alone.

**Compiling them out waits for Stage 5**, when the interface is written again on slime-ui and can
leave them out of Windows from the start.

## Alternatives considered

- **`#[cfg(unix)]` on the modules and every caller**, as the PLAN said: the compiler would prove
  that no path on Windows reaches them, and the Windows program would be smaller, but `cfg` would
  spread through Iced code about to be replaced, against ADR-0020's rule. Rejected by the owner.
- **A mix**: compiled out where only the core and the command line call (umu, the Galaxy service),
  refused at the entry for the rest. Two ways for one rule. Rejected.

## Consequences

- Linux code that never runs there is in the Windows program, until Stage 5.
- A new way into one of these features must ask `platform::linux_only`; a review checks it.
