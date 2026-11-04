# 0003. The state database: SQLite through rusqlite, behind a mutex

- Status: Accepted
- Date: 2026-10-09

## Context

Slatty keeps settings, install records, jobs, sessions, play time and cloud-sync history. The
command line and the interface both read and write them, sometimes at once, and the data must
survive crashes. The interface's background work runs on tokio, whose futures must be `Send`.

## Decision

**SQLite through rusqlite, one connection per process behind a mutex** (`db::Db`). Queries are
simple and synchronous. The schema changes only by appending a migration to `db::MIGRATIONS`;
settings are key/value rows with a getter and a setter each (`core/src/settings.rs`).

## Alternatives considered

- **An async SQL layer (sqlx)**: compile-time checks and async queries, for a database with one
  writer and small queries; more dependencies and build time for no measured need.
- **Files (TOML, JSON) per kind of record**: no transactions, and the command line and the
  interface writing at once would need locks of their own.
- **A connection pool**: several writers to one SQLite file only wait on each other.

## Consequences

- One writer per process; a long query blocks others in that process, so queries stay short.
- Existing migrations are never edited or reordered: users' databases already ran them (AGENTS.md).
- Tests use `Db::in_memory()`.
