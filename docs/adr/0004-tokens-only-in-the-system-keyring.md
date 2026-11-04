# 0004. Tokens only in the system keyring

- Status: Accepted
- Date: 2026-10-09

## Context

A GOG refresh token gives access to an account's library and profile for good; Epic's tokens and the
keys of SteamGridDB and Steam's Web API are secrets too. Files, logs, error messages and process
arguments are all read by other programs or end up in bug reports.

## Decision

**Tokens and keys live only in the system keyring** (Secret Service on Linux, through the `keyring`
crate; modules `credentials` and `account`). Sensitive values are wrapped in `secret::Secret`,
whose `Debug` hides them. Network errors are stripped of their address (`Error::network`), since
some GOG endpoints carry tokens in the query string. The token is sent only to `https://` addresses
on GOG's domains that a GOG answer names (`http::gog_only`). The one exception is Comet's handoff
file ([0011](0011-comet-supervised-beside-the-game.md)): 0600, in a private folder, removed as
soon as Comet listens.

## Alternatives considered

- **A file of Slatty's own, encrypted**: the key would have to live somewhere the same user can
  read, which protects nothing the keyring does not.
- **Asking for the password**: GOG's sign-in is a web page; Slatty never sees a password
  ([0006](0006-sign-in-in-the-system-browser.md)).

## Consequences

- Slatty needs a running Secret Service; `slatty doctor` reports when there is none.
- Tests never read the keyring; agents never read, print or paste a token (AGENTS.md).
- On Windows, the same rule uses Windows Credential Manager ([0022](0022-windows-hosts.md)).
