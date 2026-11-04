# 0006. Sign-in in the system browser, the address pasted back; no web view

- Status: Accepted
- Date: 2026-10-12

## Context

GOG signs a person in on its own web page and accepts only GOG Galaxy's redirect address,
`https://embed.gog.com/on_login_success`; no local redirect (a loopback address, a custom scheme)
is known to work. The code that ends the sign-in comes back in that address.

## Decision

**The system browser opens GOG's sign-in page, and the person pastes back the address they land
on**; Slatty takes the code from it and exchanges it for tokens, which go to the keyring
([0004](0004-tokens-only-in-the-system-keyring.md)). The same applies to Epic's sign-in.

## Alternatives considered

- **An embedded web view** catching the redirect: one step less, at the cost of a browser engine
  inside a native application, and of a sign-in page the person cannot check is GOG's.
- **A local redirect server**: GOG refuses any redirect but Galaxy's.

## Consequences

- One manual step at sign-in, explained on the page; pasting is helped by a button that reads the
  clipboard.
- The address carries the sign-in code: it is never logged or shown, and agents never handle it
  (AGENTS.md).
