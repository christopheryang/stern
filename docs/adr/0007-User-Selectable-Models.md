# ADR-0007: Users select the AI model from a pinned catalog

- **Status:** Accepted
- **Date:** 2026-11-21
- **Extends:** ADR-0006 (post-install model download)

## Context

ADR-0006 shipped a single catalog model (SmolLM-135M). Small models
trade capability against size very differently, though: a 135M model is
fast but weak at instruction following, a 360M model is noticeably
better for ~2× the download, and a 0.5B model is better still at ~3.5×.
Which trade-off is right depends on the user's hardware and patience,
and Meetily — a comparable local-first Tauri app — demonstrates the
pattern users already understand: a model manager listing each model
with its size and a plain quality/speed note, where the user downloads
the ones they want and selects the active one.

## Decision

- `MODEL_CATALOG` holds several models (initially SmolLM-135M,
  SmolLM2-135M-Instruct, SmolLM2-360M-Instruct, Qwen2.5-0.5B-Instruct),
  each with a `description` carrying the size/quality guidance.
- Several models may be installed at once. The manifest
  (`installed.json`) records all of them plus `active_model_id`;
  exactly one model (or none) is active, and the active model is what
  the chat handler loads.
- The chat view's card becomes a picker: per model — Download /
  Update, Use this model, Remove, progress while downloading, and an
  Active badge. The first model a user installs becomes active
  automatically; installing another never silently switches the
  selection. Removing the active model leaves none active (chat falls
  back to the rule-based classifier).
- Selection changes hot-swap the loaded model in the running app.
- A legacy single-model manifest (the ADR-0006 format) is migrated on
  read: it becomes a one-model manifest with that model active.

## Consequences

- Users control the size/capability trade-off, and trying a bigger
  model no longer risks the working smaller one — both stay installed.
- The catalog remains compiled-in and hash-pinned (ADR-0006 unchanged):
  offering a model is still a reviewed code change, never remote
  config. "Add from Hugging Face URL" (Meetily's advanced escape
  hatch) is deliberately NOT offered — an unpinned download would
  defeat the integrity model.
- Disk use can grow to the sum of installed models (~1 GB if a user
  installs all four); removal is per model and always available.
