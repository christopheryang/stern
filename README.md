# Stern

A local-first, AI-powered secrets manager. Everything stays encrypted on your device — zero cloud, zero telemetry, zero accounts.

Built with Rust + Tauri v2 + Leptos (WASM). Chat with Stern in natural language to store, retrieve, and manage your secrets and documents.

**New to Stern? Read the [User's Guide](docs/guide/User-Guide.md)** — a step-by-step walkthrough with screenshots, from creating your password to saving and finding your first note.

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
│   └── stern-ui/                 # Leptos component library, stylesheet, brand assets
│
├── mobile/                       # Mobile client (Android/iOS) — planned
│
├── tools/screenshots/            # Regenerates the User's Guide screenshots
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

Stern's chat is powered by a local text generation model (downloaded after install — see below). The model only *parses*: it turns a message into a structured intent, and deterministic code does everything else — the model never writes a reply you see. When a request is missing pieces, a guided conversation asks for them (kind first, then the fields it needs, then a masked confirmation):

- **Store** — "save a note", "save a document", "save my GitHub password" (notes and documents come first; naming the kind skips the kind question)
- **Retrieve** — "what's my password for Gmail?"
- **List** — "show my secrets", "show my notes" (names and kinds only, never values)
- **Folders** — "create folder Taxes", "move Lease to Home", "show my folders" (nested; each entry sits in one folder; folder names are encrypted)
- **Rename** — "rename Netflix to Netflix Family"
- **Update** — "change my Netflix password"
- **Delete** — "delete my old Netflix entry" (always asks yes or no first; goes to a trash for 30 days, and "undo" or "restore <name>" brings it back)
- **Files** — "save a document" opens a picker for a plain text file (.txt, .md, .html, .csv and the like) of up to 1 MB; PDFs, Office files and pictures are not accepted
- **Describe** — a note can carry a short "what it's about" line and keywords, kept encrypted with it

Everyday wording works too ("I forgot my router password", "jot down a note"), and small talk gets a short fixed reply.

The model runs entirely on-device via ONNX Runtime. A rule-based classifier covers for it — no model yet, a model error, output that doesn't parse — so chat works even before a model is downloaded.

### Model download & updates

The model is **not bundled** with the app. After installation, Settings (the gear in the header) offers a choice of two on-device AI models — Small (~118 MB) or Large (~483 MB), each shown with its pros and cons — each downloaded from Hugging Face only when you pick it. The file is verified against a SHA-256 pinned in the app's model catalog before it is used, and stored in the app data folder (`models/<model-id>/<version>/`, with an `installed.json` manifest recording what is installed and which model is active). Each model also downloads its tokenizer file, verified the same way, and the first download also fetches the ONNX Runtime library the model runs on.

Because the model lives outside the app bundle, it can evolve independently: when a new Stern build ships with a newer catalog version, the app detects the difference and offers an in-place update. The new file is downloaded and verified first and only then swapped in, so a failed download never breaks a working install. You can keep several models installed, switch between them, or remove them at any time from the Settings picker. Removing asks you to confirm, then deletes the model's files; removing the last model also deletes the downloaded runtime.

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
2. On first run, the setup screen asks you to create a password
   (8+ characters, with a small letter, a capital letter, a number, and a
   symbol — a checklist ticks off as you type) and to tick a box agreeing
   to the [disclaimer](docs/Disclaimer.md). A short "don't lose your
   password" step follows. There is no account and no username. Stern also
   generates a device Secret Key; password + Secret Key together unlock the
   vault (see [docs/design/2026-11-21%20-%20Security.md](docs/design/2026-11-21%20-%20Security.md)).
3. On later launches, the login screen asks for the password
   before anything else is reachable. After unlocking, choose an AI model
   (Small or Large) on the first-run screen or in Settings (the gear,
   top right); changing the model needs the vault unlocked, and the first
   download also fetches the ONNX Runtime. Settings also holds the light
   and dark choice, which works before you sign in too, and whether
   locking asks "Lock Stern?" first. The padlock at the top right shows
   locked or unlocked and locks the vault. The help button
   (the question mark) opens [plain-language help](docs/Help.md) at any
   time, including before you unlock.
4. In chat, try "show all passwords", "what's my password for Gmail?",
   or "save a note". Storing runs as a short guided
   conversation — kind first (unless you named it), then the fields it
   needs. Secret fields
   are typed into a peek-mask input (each character visible for
   2 seconds, then `*`) and never appear in the transcript.
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

- [docs/guide/User-Guide.md](docs/guide/User-Guide.md) — the user's guide, with screenshots
- [docs/](docs/README.md) — design documentation index
- [docs/architecture/Architecture.md](docs/architecture/Architecture.md) — how the app is put together
- [docs/design/2026-11-21%20-%20Security.md](docs/design/2026-11-21%20-%20Security.md) — the vault cryptography in detail
- [docs/design/2026-11-21%20-%20AI-and-Models.md](docs/design/2026-11-21%20-%20AI-and-Models.md) — chat, intent classification, and the downloadable model
- [docs/design/2026-11-22%20-%20Model-Backed-Chat.md](docs/design/2026-11-22%20-%20Model-Backed-Chat.md) — model-backed chat: parsing, guided conversations, retrieval Q&A, the vault gate, and the auth front door
- [docs/design/2026-11-23%20-%20UI-Redesign.md](docs/design/2026-11-23%20-%20UI-Redesign.md) — the frontend design system, screens, and brand
- [docs/Help.md](docs/Help.md) — the in-app help page, in plain language
- [docs/Disclaimer.md](docs/Disclaimer.md) — the in-app disclaimer and limit of liability
- [docs/Development.md](docs/Development.md) — build, test, and release workflows in depth
- [docs/adr/](docs/adr/README.md) — architecture decision records
- [AGENTS.md](AGENTS.md) — instructions for AI coding agents working in this repo

## License

MIT
