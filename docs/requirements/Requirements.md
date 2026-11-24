# Requirements

What Stern must do, collected in one place from the project's own
documents — the README, the ADRs, and the security invariants in
[../design/2026-11-21%20-%20Security.md](../design/2026-11-21%20-%20Security.md). This page records the
requirements; it does not invent new ones. Where a requirement here
and an ADR disagree, the ADR wins.

## Product

- **Local only.** No accounts, no cloud sync, no telemetry
  ([ADR-0001](../adr/0001-Local-First-No-Cloud.md)). The vault and all
  processing stay on the user's device.
- **Chat-first interface.** Vault operations are driven from chat:
  store, retrieve, list, update, and delete secrets, plus create and
  unlock the vault, export, import, and help
  ([ADR-0005](../adr/0005-Rule-Based-Intent-First.md)).
- **Secrets pass through chat only under mask.** Chat classifies
  intent; a guided conversation may collect a secret value, but only
  at a prompt flagged sensitive — the input peek-masks, the value is
  never echoed in a response or shown in the transcript, and the
  model never writes the replies (it only parses, on-device).
- **Destructive actions need confirmation.** Deleting a secret
  requires an explicit confirmation in the UI, whichever classifier
  produced the intent.

## Security

- **Encrypted at rest.** A locked vault is ciphertext only; the
  database file alone reveals nothing
  ([ADR-0004](../adr/0004-SQLite-Storage.md)).
- **Two-factor key derivation.** Unlocking requires the password AND
  the device Secret Key (2SKD), combined through Argon2id
  (256 MiB, 3 iterations, parallelism 4)
  ([ADR-0003](../adr/0003-Key-Hierarchy-2SKD.md)).
- **Per-entry keys.** Every entry is encrypted under its own data
  key (XChaCha20-Poly1305 with context AAD), wrapped by AES-KW.
- **No recovery.** Losing the password or the Secret Key makes the
  vault permanently unopenable — a deliberate consequence of the
  design, stated to the user up front (ADR-0003).
- **Throttled unlocking.** Repeated failed unlock attempts are
  rate-limited.

## AI models

- **Never bundled.** Models are downloaded after install, only when
  the user explicitly starts a download
  ([ADR-0006](../adr/0006-Post-Install-Model-Download.md)).
- **User's choice, by size.** The user picks a size — Small, Medium,
  or Large — never a model name; may install several sizes, selects
  which one is active, and can update or remove any of them
  ([ADR-0007](../adr/0007-User-Selectable-Models.md),
  [ADR-0010](../adr/0010-Model-Size-Tiers.md)).
- **Pinned integrity.** Every download is verified against a SHA-256
  pinned in the compiled-in catalog before use. No remote config and
  no unpinned downloads.
- **Fully functional without a model.** With no model installed (or
  none active), the rule-based classifier covers every intent.
- **One network request.** The model download is the only network
  request the app ever makes.

## Platform and data

- **Desktop app** (Tauri): release bundles are built for macOS
  (Apple Silicon) and Linux (x86_64) — see
  [../Development.md](../Development.md).
- **Data location.** All app data lives in the OS app-data directory
  for `com.stern.app` (the vault database plus the `models/` store);
  deleting that directory resets the app to first-run state.
