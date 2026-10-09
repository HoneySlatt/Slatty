# Security

## Reporting a vulnerability

Please do not open a public issue for a security problem. Contact the maintainer privately through
the repository's security advisory feature once the project is published. Until then, contact the
author directly.

Include the steps to reproduce and the version or commit you tested.

## How SlattyLauncher handles your account

- **No password.** You sign in on GOG's own page, in your browser.
- **Tokens stay in the keyring.** Session tokens are stored only in the system keyring (Secret
  Service). Without a keyring, SlattyLauncher refuses to store them rather than writing a plain file.
- **Tokens stay out of logs and process lists.**
  - They never appear in logs, error messages or process arguments.
  - Some GOG endpoints take tokens in the URL, so network errors are reported without their URL.
- **Comet handoff.** Comet receives tokens through a file readable only by you, in a private runtime
  directory. The file is deleted as soon as Comet has read it.
- **Logs.** Comet's log may contain game client identifiers. Review it before sharing.

## Scope

SlattyLauncher talks only to GOG services and to the CDN addresses GOG returns. It runs games with
your user rights; it does not sandbox them beyond what umu and Proton provide.
