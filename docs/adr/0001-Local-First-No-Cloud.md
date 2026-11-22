# ADR-0001: Local-first — no cloud, accounts, or telemetry

- **Status:** Accepted (recorded 2026-11-21)
- **Date:** 2026-11-21

## Context

Stern stores the user's most sensitive data: every password they own.
Mainstream password managers sync vaults through a vendor cloud, which
creates a breach target, an account-recovery attack surface, and a
trust dependency on the vendor's operational security. Stern's premise
is that none of that is necessary for a personal secrets manager: the
vault is used on the user's own machines, and modern OS keychains plus
strong local encryption cover the threat model (see
[../design/2026-11-21%20-%20Security.md](../design/2026-11-21%20-%20Security.md)).

## Decision

All data lives on the user's device, encrypted. There are:

- no servers operated by the project,
- no user accounts or sign-in,
- no telemetry, analytics, or crash reporting,
- no network calls in the product at all — with one deliberate,
  user-initiated exception carved out by ADR-0006 (the AI model
  download), which transmits nothing about the user or the vault.

Moving a vault between devices is the user's job, via encrypted
export/import.

## Consequences

- The privacy story is simple and verifiable: the binary can be audited
  for network calls, and there is nothing to breach server-side.
- No cross-device sync, no web access, no sharing features. Users who
  need those should use a cloud password manager; Stern does not
  compete on convenience.
- Support and debugging are local-only: no logs leave the device, so
  problems are diagnosed from what the user can see.
- Every future feature must be checked against this ADR. "Just one
  small call home" is how local-first products stop being local-first.
