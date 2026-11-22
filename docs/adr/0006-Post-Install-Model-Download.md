# ADR-0006: AI model delivered by post-install download, not bundled

- **Status:** Accepted
- **Date:** 2026-11-21

## Context

The AI model is ~136 MB — larger than the rest of the app — and it is
optional: the rule-based classifier covers every intent (ADR-0005).
Model quality in the sub-1B class is improving fast, so the model will
want to change more often than the app ships. Bundling it would inflate
every installer and every app update, couple model iteration to app
releases, and charge the full download cost to users who never enable
the AI. ADR-0001 forbids network calls in general, so any download
needs to be a deliberate, disclosed exception.

Alternatives considered:

- **Bundle the model in the installer.** Simple, works offline from
  first launch. Rejected: size, release coupling, and cost to users who
  never use the model.
- **Download automatically on first launch.** One less click. Rejected:
  a 136 MB transfer the user didn't ask for breaks the consent spirit
  of ADR-0001 and surprises users on metered connections.
- **Remote-config-driven downloads** (catalog fetched from a server).
  Rejected: introduces a server dependency and an unreviewed supply
  chain for executable-adjacent content.

## Decision

- The model is downloaded **after install, only when the user asks**,
  from the chat view's setup card.
- A **catalog compiled into `stern-core`** (`MODEL_CATALOG` in
  `ai/model_manager.rs`) pins each model: id, version, URL, and SHA-256.
  The hash is authoritative and reviewed in source control.
- Installs are verify-then-swap: stream to a `.part` file while
  hashing, rename into `models/<id>/<version>/` only on hash match,
  record an `installed.json` manifest, prune old versions. A failed
  download or update never disturbs a working install.
- A newer catalog version in a new app build surfaces as an in-app
  "update available" prompt; the user can also remove the model at any
  time. The app is fully functional with no model installed.
- This download is the only network request the app makes, and the
  README says so.

## Consequences

- Installers and app updates stay small; models iterate by shipping an
  app build with a bumped catalog entry (the update procedure is in
  AGENTS.md).
- First-run chat is rules-only until the user downloads the model —
  acceptable because ADR-0005 makes the rules a complete classifier,
  not a stub.
- New dependency: model availability depends on the hosting (currently
  Hugging Face) being reachable at download time. Failure degrades to
  rules-only chat, never to a broken app.
- The catalog-in-code choice means a model update cannot ship without
  an app release. That friction is intentional: every model hash users
  will ever verify against has passed code review.
- ADR-0001 is amended in practice, not in spirit: the exception is
  single-purpose, user-initiated, integrity-pinned, and transmits no
  user data.
