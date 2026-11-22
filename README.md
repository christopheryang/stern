# Stern

A local-first, AI-powered secrets manager. Everything stays encrypted on your device — zero cloud, zero telemetry, zero accounts.

Built with Rust + Tauri v2 + Leptos (WASM). Chat with Stern in natural language to store, retrieve, and manage your secrets and documents.

## Architecture

```
Stern/
├── core/                         # Shared business logic (platform-agnostic)
│   ├── stern-core/               # Domain, crypto, storage, AI, keyring
│   └── stern-ipc/                # Wire DTOs (serde, WASM-compatible)
│
├── desktop/                      # Desktop client (macOS/Windows)
│   ├── stern-tauri/              # Tauri v2 shell, commands, plugins
│   ├── stern-app/                # Leptos CSR frontend (WASM)
│   └── stern-ui/                 # Leptos component library
│
├── mobile/                       # Mobile client (Android/iOS) — planned
│
└── Cargo.toml                    # Workspace root
```

## Security

| Layer | Implementation |
|---|---|
| **KDF** | Argon2id (256MB RAM, 3 iter, 4 parallelism) + HKDF-SHA256 |
| **2SKD** | Master password + 16-byte device Secret Key → HKDF preprocess → Argon2id |
| **Key hierarchy** | Master key → HKDF → KEK (wraps per-entry DEKs) |
| **Encryption** | XChaCha20-Poly1305 (AEAD) per entry, fresh DEK per write |
| **Key wrapping** | AES-KW (RFC 3394) — DEK wrapped under KEK |
| **Memory** | `SecretMem<T>` with `mlock` (no swap), `zeroize` on drop |
| **Keychain** | `keyring` crate (macOS Keychain, Windows DPAPI, Linux Secret Service) |
| **Clipboard** | arboard with platform exclusion hints, auto-clear |
| **Verify** | Constant-time hash comparison before decryption |

## AI

Stern includes a built-in chat interface powered by a local text generation model (SmolLM-135M, ONNX). The AI classifies natural language intents:

- **Store** — "save my GitHub password"
- **Retrieve** — "get my Gmail credentials"
- **List** — "show all passwords"
- **Update** — "change my Netflix password"
- **Delete** — "remove my WiFi password"

The model runs entirely on-device via ONNX Runtime, with a rule-based classifier as fallback — chat works even before the model is downloaded.

### Model download & updates

The model is **not bundled** with the app. After installation, the chat view offers a choice of on-device models — SmolLM-135M (~136 MB), SmolLM2-135M-Instruct (~118 MB), SmolLM2-360M-Instruct (~273 MB), or Qwen2.5-0.5B-Instruct (~483 MB) — each downloaded from Hugging Face only when you pick it. The file is verified against a SHA-256 pinned in the app's model catalog before it is used, and stored in the app data folder (`models/<model-id>/<version>/`, with an `installed.json` manifest recording what is installed and which model is active).

Because the model lives outside the app bundle, it can evolve independently: when a new Stern build ships with a newer catalog version, the app detects the difference and offers an in-place update. The new file is downloaded and verified first and only then swapped in, so a failed download never breaks a working install. You can keep several models installed, switch between them, or remove them at any time from the same card in the chat view.

That download is the only network request Stern ever makes, and it happens only when you explicitly start it. Your vault data never leaves the device.

## Tech Stack

- **Backend**: Rust (edition 2021)
- **Desktop**: Tauri v2
- **Frontend**: Leptos 0.8 (CSR → WASM)
- **Database**: SQLite (bundled rusqlite)
- **Crypto**: RustCrypto (chacha20poly1305, aes-kw, argon2, hkdf)
- **AI**: ONNX Runtime (`ort` crate) + rule-based fallback; model downloaded post-install and SHA-256-verified (`ureq`)
- **Build**: Cargo workspace, Trunk (WASM bundler)

## Prerequisites

- [Rust](https://rustup.rs/) (stable, via rustup) with the WASM target:
  `rustup target add wasm32-unknown-unknown`
- [Tauri CLI](https://v2.tauri.app/) v2 — `cargo install tauri-cli --version "^2"`
- [Trunk](https://trunkrs.dev/) — `cargo install trunk --version 0.21.13`
- [Just](https://just.systems/) — `cargo install just`

On Linux, Tauri also needs the WebKitGTK stack (`libwebkit2gtk-4.1-dev`,
`libappindicator3-dev`, `librsvg2-dev`, `libgtk-3-dev`, `libssl-dev`, `patchelf`).

## Development

```bash
git clone https://github.com/christopheryang/stern.git
cd stern

# Run the app in development (frontend hot-reloads on http://localhost:1420)
just dev

# Run the automated tests
just test           # core + desktop
just test-core      # stern-core + stern-ipc only

# Lint and format (the CI gates)
just lint           # clippy, warnings denied
just fmt            # check formatting
just fmt-fix        # apply formatting

# Build release installers
just build
```

### Test-drive the app

1. `just dev` opens the Stern window.
2. On first run, create a vault and choose a master password. Stern also
   generates a device Secret Key; password + Secret Key together unlock the
   vault (see [docs/design/2026-11-21%20-%20Security.md](docs/design/2026-11-21%20-%20Security.md)).
3. Unlock and open the **Chat** tab. Try "show all passwords" or "get my
   Gmail credentials". Storing works too — "save my GitHub password" starts
   the flow — but the secret itself is always entered in the entry form,
   never in chat.
4. The card at the top of the chat view reports "AI model not installed".
   Chat already works via the built-in rules; click **Download model**
   (~136 MB) to fetch the on-device AI model. A progress bar tracks the
   download, and the card then shows the installed version.
5. App data lives in the platform app-data folder for `com.stern.app`
   (macOS: `~/Library/Application Support/com.stern.app/`): the vault is
   `stern.db`, downloaded models live under `models/`. Delete that folder
   to reset to a clean first-run state.

### Packaging

`just build` produces installers under
`desktop/stern-tauri/target/release/bundle/` (with `--target`, under
`target/<triple>/release/bundle/`): `.dmg`/`.app` on macOS, NSIS/MSI on
Windows, `.deb`/AppImage on Linux. Releases are built by CI from `v*` tags
on the `main` branch (see `.github/workflows/release.yml`) for macOS
(Apple Silicon) and Linux (x86_64). Code signing and notarization are not
configured yet, so local builds are unsigned.

## Documentation

- [docs/](docs/README.md) — design documentation index
- [docs/architecture/Architecture.md](docs/architecture/Architecture.md) — how the app is put together
- [docs/design/2026-11-21%20-%20Security.md](docs/design/2026-11-21%20-%20Security.md) — the vault cryptography in detail
- [docs/design/2026-11-21%20-%20AI-and-Models.md](docs/design/2026-11-21%20-%20AI-and-Models.md) — chat, intent classification, and the downloadable model
- [docs/Development.md](docs/Development.md) — build, test, and release workflows in depth
- [docs/adr/](docs/adr/README.md) — architecture decision records
- [AGENTS.md](AGENTS.md) — instructions for AI coding agents working in this repo

## License

MIT
