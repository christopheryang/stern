# ADR-0002: Hexagonal Rust core with a Tauri + Leptos shell

- **Status:** Accepted (recorded 2026-11-21)
- **Date:** 2026-11-21

## Context

The security-critical logic (key derivation, encryption, vault storage)
needs to be testable without a GUI and reusable if a mobile client
arrives. The desktop shell needs to be small, native-feeling, and
cross-platform (macOS first, Windows/Linux bundles supported). An
Electron-class stack would put the vault's trust boundary inside a
browser runtime and a Node process; a fully native UI per platform
would triple the UI work.

## Decision

- **One Rust workspace.** `stern-core` holds all business logic in a
  hexagonal layout: `domain` (pure types), `application` (use cases
  over port traits), `infrastructure` (adapters: crypto, SQLite,
  keychain, clipboard, backup). The core never depends on Tauri or any
  OS UI API.
- **`stern-ipc`** holds the serde DTOs — the only types that cross the
  shell boundary.
- **Tauri v2** is the desktop shell: a small Rust host (commands,
  plugins, packaging) around a system webview.
- **Leptos (CSR → WASM)** is the frontend, split into `stern-app`
  (entry point) and `stern-ui` (components). The UI invokes commands
  through Tauri's JS bridge.

## Consequences

- The core's test suite runs headlessly (temp dirs, in-memory keychain)
  and covers the crypto and vault behavior without launching the app —
  most of the project's correctness lives where it is cheapest to test.
- A future mobile shell can reuse `stern-core` unchanged; only commands
  and UI would be new.
- Accepted cost: `stern-ui` cannot depend on `stern-ipc` through the
  WASM/Tauri bridge setup, so DTOs are mirrored as local structs in UI
  components. Changes to a DTO must be mirrored by hand (documented in
  AGENTS.md).
- Accepted cost: two build systems to keep green (cargo workspace +
  Trunk for the WASM frontend), and Tauri's webview differences across
  platforms are inherited.
