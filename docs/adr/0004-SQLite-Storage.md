# ADR-0004: SQLite file for vault storage

- **Status:** Accepted (recorded 2026-11-21)
- **Date:** 2026-11-21

## Context

The vault needs durable, transactional, single-user storage that works
identically on macOS, Windows, and Linux, survives crashes mid-write,
and produces a single artifact the user can back up or move. Entries
are ciphertext blobs plus metadata (ADR-0003), so the store does not
need to understand secrets — it needs reliability and a migration path.

## Decision

Store the vault in a single **SQLite** database (`stern.db` in the app
data directory), accessed through a repository in
`infrastructure/sqlite` with explicit schema migrations. The schema
stores vault metadata (KDF parameters, verification hash) and entries
as ciphertext + wrapped DEK + non-secret metadata.

## Consequences

- One file = one vault: backup, move, and delete are file operations.
- SQLite's maturity and transactional writes protect against corruption
  from crashes or power loss far better than a hand-rolled file format.
- Schema evolution is explicit migrations, tested like code.
- Accepted cost: anyone can copy the file — by design, since ADR-0003
  makes the file useless without the secrets. File permissions are not
  part of the security boundary.
- Accepted cost: SQLite metadata (entry counts, timestamps, sizes) is
  visible to someone holding the file. Field contents are not.
