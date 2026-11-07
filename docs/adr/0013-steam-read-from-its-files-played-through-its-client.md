# 0013. Steam's games read from its files and played through its client

- Status: Accepted; amended by [0034](0034-what-slatty-is-to-become.md) (Slatty's own games written
  into Steam's shortcuts, guarded)
- Date: 2026-10-12

## Context

A person's library spans stores; Steam is the largest. Most Steam games need the Steam client
running (DRM, Steamworks), and Steam owns their files. The client keeps what Slatty needs on disk:
the account's games, those installed, play times, images and achievements.

## Decision

**Slatty reads the local Steam client's files and never writes to them** (`core/src/steam`): it
finds the client, its libraries and the account, decodes `appinfo.vdf` for the apps asked for, and
reads play time from `localconfig.vdf`, keeping nothing else of it. **Games are played, installed,
verified and uninstalled through the client**, by `steam://` addresses. Steam's servers are asked
(images the client lacks, the Web API) only once turned on
([0005](0005-every-third-party-off-until-turned-on.md)).

## Alternatives considered

- **Starting Steam games directly**: most fail without the client.
- **Steam's servers for the library**: needs a key or a sign-in, and tells Valve more than reading
  the client's files does.
- **Managing Steam's files** (installing, moving): Steam would not know of it.

## Consequences

- A Steam game's files are Steam's: `paths::check_owned_id` refuses Steam's ids wherever Slatty
  installs, records or deletes a game's files (AGENTS.md).
- Every file is read bounded in size, without following links, without waiting.
- A game owned on both GOG and Steam shows once.
