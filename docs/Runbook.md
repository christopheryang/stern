# Runbook

Operating and troubleshooting a running Stern install. For building
and releasing the app, see [Development.md](Development.md); for how
things work, see [architecture/Architecture.md](architecture/Architecture.md).

## Key locations

All state lives under the OS app-data directory for `com.stern.app`
(on macOS: `~/Library/Application Support/com.stern.app/`).

| Path | What it is |
| --- | --- |
| `stern.db` | The vault database (ciphertext only while locked) |
| `models/` | Downloaded AI models, one `<model-id>/<version>/` per install |
| `models/installed.json` | Model manifest: what is installed, and which model is active |

## Healthy looks like

- The app opens to the lock screen; unlocking shows the vault.
- Chat answers commands even with **no model installed** — the
  rule-based classifier is always on duty. A model is an enhancement,
  never a requirement.
- The chat view's AI model card shows the catalog; an installed,
  selected model carries an **Active** badge.

## Common problems

### A model download fails immediately

Most likely the model host (Hugging Face) is unreachable from this
network. Nothing is kept from a failed attempt — partial files are
deleted automatically — so just retry from the picker when the
network allows. No other part of the app is affected.

### "Model integrity check failed"

The downloaded bytes do not match the SHA-256 pinned in the catalog.
**Do not bypass this.** Re-download once; if it fails again, the
upstream file has changed — the catalog entry must be re-verified
against the file's Hugging Face page and updated by a developer
(AGENTS.md, "Shipping a model update").

### A model is downloaded but chat behaves the same

Expected today: classification is rule-based, and the downloaded
model is loaded and held ready (see
[design/2026-11-21%20-%20AI-and-Models.md](design/2026-11-21%20-%20AI-and-Models.md)). Also check the
picker: the model only counts when it is the **Active** one — use
**Use this model** to switch; the change takes effect immediately.

### A model shows as not installed, but files are on disk

`models/installed.json` is the source of truth, and a missing or
corrupt manifest reads as "nothing installed". Recovery: quit the
app, delete the contents of `models/`, relaunch, and download again.

### The password is forgotten

There is no recovery, by design
([adr/0003-Key-Hierarchy-2SKD.md](adr/0003-Key-Hierarchy-2SKD.md)):
without the password and the device Secret Key the vault cannot be
opened, by anyone. The only way forward is a reset (below), which
destroys the vault's contents.

### Unlocking is temporarily refused

Repeated failed attempts trigger a lockout. Wait for it to expire
and retry; do not delete anything.

## Reset to first-run state

1. Quit Stern.
2. Delete the whole app-data directory (see Key locations).
3. Relaunch — the app behaves as freshly installed: no vault, no
   models.

## Before reporting a problem

Collect: the app version, the OS, what the AI model card shows, the
contents of `models/installed.json` (model metadata only — it
contains no secrets), and any console output mentioning a model that
failed to load.
