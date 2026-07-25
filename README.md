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

Stern includes a built-in chat interface powered by a local text generation model (SmolLM-135M ONNX Q4, <50MB). The AI classifies natural language intents:

- **Store** — "save my GitHub password"
- **Retrieve** — "get my Gmail credentials"
- **List** — "show all passwords"
- **Update** — "change my Netflix password"
- **Delete** — "remove my WiFi password"

The model runs entirely on-device via ONNX Runtime. No network calls.

## Tech Stack

- **Backend**: Rust (edition 2021)
- **Desktop**: Tauri v2
- **Frontend**: Leptos 0.8 (CSR → WASM)
- **Database**: SQLite (bundled rusqlite)
- **Crypto**: RustCrypto (chacha20poly1305, aes-kw, argon2, hkdf)
- **AI**: ONNX Runtime (`ort` crate) + rule-based fallback
- **Build**: Cargo workspace, Trunk (WASM bundler)

## Prerequisites

- [Rust](https://rustup.rs/) (stable)
- [Trunk](https://trunkrs.dev/) — `cargo install trunk --version 0.21.13`
- [Just](https://just.systems/) — `cargo install just`

## Getting Started

```bash
# Clone
git clone https://github.com/yourname/stern.git
cd stern

# Run in development
just dev

# Build for release
just build

# Lint
just lint
```

## License

MIT
