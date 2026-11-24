# Development

Building, running, testing, and packaging Stern. The README has the
quick version; this page is the full workflow, following the same
pattern as the topic guides in this folder.

## Environment

| Tool | Version | Install |
|---|---|---|
| Rust (stable, rustup) | current stable | https://rustup.rs |
| WASM target | — | `rustup target add wasm32-unknown-unknown` |
| Tauri CLI | v2 | `cargo install tauri-cli --version "^2"` |
| Trunk | 0.21.13 | `cargo install trunk --version 0.21.13` |
| Just | any recent | `cargo install just` |

The toolchain channel is pinned to `stable` in `rust-toolchain.toml`.
Linux also needs the WebKitGTK stack:
`libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev libgtk-3-dev
libssl-dev patchelf` (the release workflow installs exactly these).

## Running the app

```bash
just dev
```

This runs `cargo tauri dev` in `desktop/stern-tauri`, which starts
Trunk serving the Leptos frontend on http://localhost:1420 and opens
the app window against it. Frontend changes hot-reload; Rust changes
rebuild and relaunch. First launch takes minutes (full dependency
build); later launches are incremental.

Dev builds are unoptimized, except the `argon2` and `blake2` crates,
which the root `Cargo.toml` compiles with `opt-level = 3`
(`[profile.dev.package.*]`). Without that, the vault's Argon2id key
derivation (256 MB, 3 passes) takes about 5 s per unlock instead of
about 0.3 s. It changes speed only — the KDF parameters
([ADR-0003](adr/0003-Key-Hierarchy-2SKD.md)) are untouched, and release
builds are unaffected.

To work on the frontend alone in a browser, `cd desktop/stern-app &&
trunk serve --port 1420` — note the UI expects the Tauri bridge
(`window.__TAURI__`), so vault/chat actions fail outside the app shell;
it is useful for layout work only.

### Manual test pass

1. Fresh state: delete the app data dir (`com.stern.app` — macOS:
   `~/Library/Application Support/com.stern.app/`).
2. `just dev` — the setup screen creates the vault: the checklist
   ticks as you type, Continue is refused until the disclaimer box is
   ticked, and a "Don't lose your password" step follows. Quit and
   relaunch: the login screen now asks for the password before anything
   else.
3. Store an entry in chat (step 4); lock (the top bar's padlock) and
   unlock; ask for the entry and confirm it decrypts. Copy a secret
   and confirm the clipboard auto-clears.
   Open Help (the question mark) and the Disclaimer from the setup
   screen, the login screen, and the chat: Back must return to where
   you were, with anything typed still there. Break the network and
   try a download: the message should be one plain sentence, with the
   technical text as small print beneath it. Any other error should show
   the plain sentence alone.
   Check both themes (Settings → Look; light is the default, and the choice survives a restart) and a short
   window (resize to the 400 px minimum height): nothing should clip
   or overflow, and the setup screen's requirements checklist should
   tick green as you type.
4. Chat: "save a note" (goes straight to the note questions), "find my
   <note title>", "save my GitHub password" (a named store files
   immediately; a bare "store" runs the guided conversation, notes first — secret fields use the
   peek-mask input and never appear in the transcript).
5. First-run screen and Settings (gear) → AI model picker (Small / Large, each with pros and cons; Small is Recommended): Download → progress → installed.
   Remove → the picker shows it gone and chat still answers via rules.
6. Model update drill: bump the catalog `version` string in
   `core/stern-core/src/ai/model_manager.rs` locally, restart — the
   picker offers an update; downloading swaps versions and prunes the
   old directory under `models/<model-id>/`.

## Testing

```bash
just test           # cargo test for core + desktop halves
just test-core      # stern-core + stern-ipc
just test-desktop   # stern-tauri + stern-app + stern-ui
cargo test -p stern-core --test model_manager_tests   # a single file
```

Tests live in per-crate `tests/` directories and are hermetic: temp
dirs for storage, the in-memory keychain instead of the OS keychain,
and in-memory readers instead of the network for model installs.

## Quality gates

All three must pass before a change is done — CI enforces the same:

```bash
just lint     # cargo clippy --workspace -- -D warnings
just fmt      # cargo fmt --all -- --check   (just fmt-fix to apply)
just test
```

The workspace lint set (root `Cargo.toml`) denies `unwrap`/`expect`/
`panic`, indexing/slicing, unchecked arithmetic, and float arithmetic
in library code, and clippy is pedantic — expect to use checked /
saturating arithmetic, `?` propagation, and targeted `#[allow]`s with
precedent in the codebase rather than broad suppressions. Note that
clippy's `--all-targets` is *not* part of the gate: the older test
files predate the pedantic set. New test files should still be
clippy-clean (run `cargo clippy -p <crate> --test <name> -- -D warnings`).

## Regenerating the User's Guide screenshots

The screenshots in [guide/](guide/User-Guide.md) are generated, not
hand-drawn, so they can be redone whenever a screen changes:

```bash
tools/screenshots/capture.sh
```

It builds the frontend, serves a copy with
`tools/screenshots/capture-stub.js` standing in for the Tauri bridge
(no vault, model or network is involved), and takes 900x700 shots, the
real window size, with headless Chrome (`CHROME=...` overrides the
macOS path; Node 22+ is needed). The stub replays the real replies of
the chat handler, so when chat wording changes, update the stub's
replies too. Look at every image before committing; the file names are
`User-Guide - <Image name>.png` and the guide links to them by name.

## Packaging

```bash
just build    # cargo tauri build in desktop/stern-tauri
```

Bundles land in `desktop/stern-tauri/target/release/bundle/` (or
`target/<triple>/release/bundle/` with `--target`): `.dmg` + `.app` on
macOS, NSIS + MSI on Windows, `.deb` + AppImage on Linux — bundle
targets are `"all"` in `tauri.conf.json`. The frontend is built by
Trunk in release mode as part of the Tauri build
(`beforeBuildCommand`).

**Signing is not configured.** Local builds are unsigned; macOS will
require a right-click → Open (or an explicit Gatekeeper exception) on
first launch. Do not ship a build to anyone else until signing and
notarization are set up.

### Versions

- App version: `[workspace.package] version` in the root `Cargo.toml`,
  bumped per the cocogitto rules in AGENTS.md.
- `desktop/stern-tauri/tauri.conf.json` carries its own `version`
  field — keep it in sync with the workspace version when releasing;
  the bundle metadata comes from the config.

## Releasing

Releases are CI-built from tags (`.github/workflows/release.yml`):

1. Land changes on `main` with conventional commits (AGENTS.md).
2. Tag `v<workspace version>` on `main` and push the tag.
3. The workflow verifies the tag is on `main`, builds macOS (Apple
   Silicon, `aarch64-apple-darwin`) and Linux (x86_64) bundles, and
   creates a GitHub Release with generated notes and the bundles
   attached.

Windows is not in the release matrix yet, although the bundle config
supports it.

## Troubleshooting

- **App won't unlock / want a clean slate:** quit the app and delete
  the app data dir (above). The vault is gone with it — export first if
  it matters.
- **Model download fails:** chat keeps working on rules. Check network
  reachability of huggingface.co, then retry from the card; a partial
  download is discarded automatically. To force a fresh start, delete
  the `models/` folder inside the app data dir while the app is closed.
- **`cargo tauri` not found:** the Tauri CLI is a separate install
  (see Environment), not a workspace member.
- **Trunk port conflict:** something else is on :1420 — stop it, or
  serve Trunk on another port and adjust `devUrl` in
  `tauri.conf.json` to match.
