# ADR-0008: The ONNX Runtime is downloaded on demand, chat requires a model, and the UI is chat-only

- **Status:** Accepted
- **Date:** 2026-11-22
- **Extends:** ADR-0006 (post-install model download), ADR-0007 (user-selectable models)
- **Amends:** the user-facing optionality of ADR-0005 (the rule-based classifier remains the classification engine; "no model" is no longer a chat mode)

## Context

ADR-0006 and ADR-0007 took the model out of the package, but the ONNX
Runtime stayed statically linked into the binary: the shipped app
carried the entire runtime (the single largest component in the
binary) for an AI feature a fresh install had not yet chosen to use,
and the runtime's version was coupled to app releases. At the same
time, chat ran without any model via the rule-based classifier, so the
model read as an optional extra rather than the chat's engine, and the
UI carried a sidebar whose Secrets/Documents tabs were placeholders.

## Decision

- The `ort` crate is built with its `load-dynamic` feature: **no ONNX
  code or runtime ships in the binary.** `AiModel` dlopens the runtime
  library from the app data directory at first model load
  (`ort::init_from`, once per process).
- A compiled-in `RUNTIME_CATALOG` pins one runtime build per shipped
  platform (ONNX Runtime 1.31.0 — the version matching ort
  2.0.0-rc.12's api-24 — for macOS arm64 and Linux x86_64). The
  distribution container is the official `onnxruntime` wheel from
  PyPI, pinned by the SHA-256 the PyPI JSON API publishes. The first
  model download fetches the wheel, verifies the hash, extracts
  exactly one entry — the shared library — into
  `<app_data_dir>/models/runtime/<version>/`, and records it in
  `installed.json`. Selecting an already-installed model on a device
  that has no runtime yet (e.g. a manifest written before this ADR)
  fetches the runtime as part of that user action. Nothing downloads
  at startup, ever.
- **Chat requires a model.** With no active model (or no runtime), the
  app shows a model-required gate instead of the chat, routing the
  user to Settings to pick one. The first model a user downloads is
  therefore also their first-run setup step.
- **The UI is a chat box and a gear.** The sidebar is removed. The app
  is a single chat view under a slim header (app name + gear button);
  the gear opens the Settings view, which holds the model picker
  (download / select / update / remove, unchanged from ADR-0007).

## Consequences

- The release binary drops from 38 MB to 16 MB (the DMG from
  12 MB to 5.2 MB), and the built binary contains zero ONNX Runtime
  symbols: the statically linked runtime was the dead weight. What
  remains is the Tauri shell and the frontend.
- First run now has a mandatory download before chat: the runtime
  wheel (~21–24 MB) plus the chosen model. An offline first run
  cannot chat at all — accepted, because the model *is* the chat
  interface; the vault's chat surface is unavailable until setup
  completes.
- The runtime version is pinned to the `ort` crate's API level.
  Bumping `ort` means bumping `RUNTIME_VERSION`, the wheel URLs, and
  their hashes in the same reviewed change — the same discipline
  ADR-0006 already imposes on model updates.
- Platforms without a catalogued runtime (anything beyond macOS
  arm64 / Linux x86_64 today) cannot load models; `ensure_runtime`
  fails with an explicit unsupported-platform error.
- The wheel is a container, not an install: one library file is
  extracted, nothing is executed, and the archive is deleted after
  extraction. PyPI joins Hugging Face as a download source; both are
  reached only from explicit user actions (Download, or selecting a
  model that still needs the runtime).
- A runtime upgrade only takes effect on the next app start (the
  library is initialized once per process); model hot-swap is
  unaffected.
