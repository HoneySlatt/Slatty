# 0039. Rust toolchain review at resumption

- Status: Accepted
- Date: 2026-11-18

## Context

The workspace declares Rust 1.90 as its minimum. The Nix shells supply the tools used by
developers and CI. [Rust 1.100](https://blog.rust-lang.org/2026/11/12/Rust-1.100.0/) brings new
standard library APIs and Cargo checks, but Slatty has no current requirement for them. The
toolchain review was static.

## Decision

Keep the current toolchain and the declared minimum during the pause. When Stage 5 starts,
review the stable release available then and the requirements of the uwui port.

## Alternatives considered

- Try 1.100 with the current minimum. This could reveal incompatibilities early, but the stable
  release and dependency requirements may change before work resumes.
- Raise the minimum to 1.100 after validation. This would make its new APIs available throughout
  the workspace and require contributors to upgrade even without a current need for those APIs.

## Consequences

At resumption, assess any candidate toolchain with the Linux and Windows checks. Check the
declared minimum separately against the locked dependencies. A toolchain update and a change to
the minimum are separate decisions.
