# AGENTS.md — working in the Stern repository

Instructions for AI coding agents (and humans) changing this codebase.
Read this file before your first change; read the linked docs before
touching their area.

Stern is a **local-first, AI-powered secrets manager**: Rust + Tauri v2 +
Leptos (WASM). Everything is encrypted on-device — zero cloud, zero
telemetry, zero accounts. Design documentation lives in
[docs/](docs/README.md); the decisions behind it are recorded in
[docs/adr/](docs/adr/README.md).

## Repository layout

```
core/
  stern-core/     All business logic, platform-agnostic (hexagonal:
                  domain / application ports / infrastructure adapters)
  stern-ipc/      Wire DTOs shared across the Tauri boundary (serde only)
desktop/
  stern-tauri/    Tauri v2 shell: commands, plugins, AppState, packaging
  stern-app/      Leptos CSR entry point (Trunk serves this on :1420)
  stern-ui/       Leptos component library (depends on nothing internal
                  except via the Tauri JS bridge — see Gotchas)
mobile/           Planned; does not exist yet
```

## Setup

- Rust stable via rustup, plus `rustup target add wasm32-unknown-unknown`
- `cargo install tauri-cli --version "^2"`, `cargo install trunk --version 0.21.13`,
  `cargo install just`
- Linux additionally needs the WebKitGTK stack (see README Prerequisites).

## Commands

| Task | Command |
|---|---|
| Run the app (dev) | `just dev` |
| All tests | `just test` (`test-core` / `test-desktop` for halves) |
| One test file | `cargo test -p stern-core --test <name>` |
| Lint gate | `just lint` → `cargo clippy --workspace -- -D warnings` |
| Format check / fix | `just fmt` / `just fmt-fix` |
| Release build | `just build` → bundles in `desktop/stern-tauri/target/release/bundle/` |

Before declaring work done, all three must pass: `just lint`,
`just fmt`, and the affected crates' tests. The workspace lints are
strict (see the root `Cargo.toml`): `unwrap`/`expect`/`panic`, indexing,
unchecked arithmetic, and float arithmetic are **denied** in library code,
and clippy pedantic warnings fail CI. Use `?` with `VaultError`, checked /
saturating arithmetic, and `#[must_use]` where the existing code does.
Tests may use `unwrap` behind a file-level
`#![allow(clippy::unwrap_used)]`.

## Commit conventions (cocogitto)

Commits are Conventional Commits enforced by cocogitto (`cog.toml`); the
full rules are in `.agents/skills/repo-skills.md`. In short:

- One scope per line; every component touched gets its own
  `{action}({scope}):` line in the body. Scopes: `core`, `ipc`, `tauri`,
  `app`, `ui`, `crypto`, `ai`, `vault`, `db`, `build`, `ci`, `docs`, `deps`.
- The subject line's type drives the version bump in the root
  `Cargo.toml` (`[workspace.package] version`): `feat` → minor,
  `fix` → patch, `feat!` → major; docs/build/ci/refactor/test/chore → none.

## Gotchas

- **stern-ui does not depend on stern-ipc.** DTOs are duplicated as local
  serde structs in the UI components and invoked through
  `window.__TAURI__.core.invoke`. If you change a DTO in
  `core/stern-ipc/src/dto.rs`, mirror it in the UI component that uses it
  (e.g. `desktop/stern-ui/src/components/chat.rs`, `model_setup.rs`).
- **Tauri commands** live in `desktop/stern-tauri/src/commands.rs`, are
  registered in `lib.rs`'s `invoke_handler!`, return
  `Result<T, VaultError>` (which serializes as a string), and get shared
  state from `AppState`. Long-running work (downloads) belongs on
  `tauri::async_runtime::spawn_blocking`, with progress kept in `AppState`
  for the frontend to poll — there is no event bus.
- **Tests** live in per-crate `tests/` directories, not in `src/`.
- **App data** for manual debugging: the platform app-data dir for
  `com.stern.app` (macOS: `~/Library/Application Support/com.stern.app/`).
  The vault is `stern.db`; downloaded AI models live under `models/`.
  Deleting the folder resets the app to first-run state.

## Security invariants — do not break these

- **No network access** anywhere except the user-initiated AI model
  download (ADR-0006). No telemetry, no update checks, no remote config.
- **Secrets never travel through chat.** Chat classifies intent only;
  secret values are entered and shown in the entry UI, and chat responses
  to vault actions suppress the echoed user message.
- Key material lives in `SecretMem<T>` (mlock + zeroize). Never log,
  serialize, or clone secrets or keys into plain types.
- Do not weaken the crypto parameters in
  `core/stern-core/src/domain/vault/crypto_constants.rs` or the KDF
  configuration (ADR-0003) without a new ADR and a migration story.
- Downloaded model files are executed only after SHA-256 verification
  against the pinned catalog hash.

## Shipping a model update

The AI model is not bundled (ADR-0006); users download it post-install
and are offered updates in-app. To ship a new model:

1. Publish the ONNX file (Hugging Face) and note its exact SHA-256 from
   the file page — the hash is authoritative; `size_bytes` is approximate.
2. In `core/stern-core/src/ai/model_manager.rs`, update the
   `MODEL_CATALOG` entry: `version` (bump it — version comparison drives
   the update prompt), `download_url`, `sha256`, `size_bytes`.
3. Existing installs then report `UpdateAvailable`; the new file is
   downloaded and verified before it replaces the old one.

To **add** a model to the picker (ADR-0007), append a new catalog entry
with a unique `id` and the SHA-256 from its Hugging Face file page; the
UI, commands, and manifest handle the rest. Never add an entry without a
verified pinned hash — an unverifiable model must not ship in the
catalog.

## CI

- `core-quality-check.yml` / `desktop-quality-check.yml` — fmt, clippy,
  tests for the two halves of the workspace.
- `precommit-check.yml` — workspace clippy, cargo-deny, commit messages.
- `quality-check.yml` — the always-on PR gate: fmt, workspace clippy,
  stern-ui clippy for wasm32, all test suites, the Trunk frontend
  build, a cargo-audit pass over Cargo.lock (RustSec advisories), and
  a gitleaks secret scan, on every pull request with no path filters.
- `codeql.yml` — CodeQL code scanning (actions + rust) on pushes and
  pull requests to main plus a weekly schedule; findings land in the
  repo's Security tab.
- `release.yml` — on `v*` tags on `main`: builds macOS (Apple Silicon)
  and Linux (x86_64) bundles and attaches them to a GitHub Release.
  Signing/notarization are not configured.

## Documentation duties

- Behavior changes update the matching page in `docs/`.
- **Before creating or updating a pull request:** check doc freshness
  — every behavior, interface, workflow, or structure change in the PR
  must be reflected in `docs/` (and in the README or this file where
  they describe it). Create new docs when a change introduces
  something no existing doc covers. A PR with stale docs is not
  ready.
- Significant or hard-to-reverse decisions get an ADR in `docs/adr/`
  (use `docs/adr/Template.md`, next free number, link it from
  `docs/adr/README.md`).
- **Docs naming (Christopher's rule):** directories are always
  lowercase (`architecture/`, `design/`, `requirements/`, `adr/`);
  document names under `docs/` always have the first letter
  capitalized, and acronyms are always capitalized
  (`AI-and-Models.md`, `Runbook.md`). `design/` holds the detailed
  design of a particular feature implementation, and every file in
  it is prefixed with its date: `YYYY-MM-DD - Name.md`.
  `architecture/` holds the current snapshot of the system and stays
  at a higher level — never as detailed as `design/`. When you
  rename or move a doc, fix every link to it in the same change.
- **Diagrams:** add diagrams wherever they clarify a doc — a system
  diagram for the architecture snapshot, data-flow and sequence
  diagrams for flows and multi-step interactions in `design/`, and a
  schema diagram for the storage design. Keep each diagram's source
  so it can be regenerated when the system changes, and update the
  diagram in the same change as the behavior it depicts.
- **Images are separate files, never embedded.** Do not base64-embed
  images in Markdown. Each image lives alongside the doc that
  references it, named `<Doc name> - <Image name>.<ext>` (e.g.
  `Architecture - System Architecture.png`), and the doc links to it
  with a %20-encoded relative path.

