# Architecture

How Stern is put together: a hexagonal Rust core that owns everything
security-relevant, a thin Tauri shell that owns the OS, and a Leptos
frontend that owns nothing but pixels. See [adr/](../adr/README.md) for the
decisions behind this shape — especially ADR-0001 (local-first) and
ADR-0002 (hexagonal core + Tauri/Leptos).

![Stern system architecture](Architecture%20-%20System%20Architecture.png)

*The whole app on one page: the frontend owns pixels, the shell owns the OS, and stern-core owns everything security-relevant. The blue arrow is the only network call in the product.*

## Principles

1. **Local-first, always.** No servers, no accounts, no telemetry. The
   only network call in the product is the user-initiated AI model
   download ([2026-11-21 - AI-and-Models.md](../design/2026-11-21%20-%20AI-and-Models.md), ADR-0006).
2. **The core is platform-agnostic.** `stern-core` never imports Tauri,
   a windowing toolkit, or an OS API directly; OS capabilities arrive
   through ports. This keeps the crypto and vault logic unit-testable
   headlessly and leaves the door open to the planned mobile client.
3. **The shell is thin.** Tauri commands validate, delegate to the core,
   and translate results to DTOs. No business logic lives in the shell.
4. **Secrets have one path.** Key material and plaintext entries exist
   only inside the core's session, behind `SecretMem` (mlock + zeroize).
   The frontend only ever sees DTOs for entries it asked to reveal.

## Workspace map

```
core/
  stern-core/                 All business logic
    src/domain/               Pure domain types, no I/O
      vault/                  Entry, VaultMeta, SecretKey, KdfParams,
                              AAD construction, crypto constants, errors
      shared/                 Ids and shared value types
    src/application/          Use cases over ports
      vault/                  Vault service + session (holds keys while
                              unlocked); ports/ = crypto, kdf, keychain
                              traits the domain needs from the OS
    src/infrastructure/       Adapters implementing the ports
      crypto/                 Argon2id KDF, XChaCha20-Poly1305, SecretMem
      sqlite/                 Repository + migrations over stern.db
      keychain/               OS keychain (keyring crate) + in-memory
                              implementation for tests
      backup.rs, clipboard.rs Encrypted export/import; clipboard with
                              auto-clear
    src/ai/                   Chat handler, intent classifier, ONNX model
                              wrapper, model manager (download/updates)
  stern-ipc/                  Serde DTOs for the Tauri boundary — the
                              only types allowed to cross it
desktop/
  stern-tauri/                Tauri v2 shell: commands.rs (the command
                              surface), lib.rs (AppState, setup),
                              capabilities/, tauri.conf.json (packaging)
  stern-app/                  Leptos CSR entry point; Trunk serves it in
                              dev (port 1420) and builds dist/ for release
  stern-ui/                   Leptos components (chat, sidebar, model
                              setup card, app shell). Depends on no other
                              crate in the workspace — it talks to the
                              shell through window.__TAURI__.core.invoke
                              and mirrors the DTOs it needs locally.
```

## Runtime structure

The shell owns a single `AppState`:

- `vault` — the core vault service over `<app_data_dir>/stern.db`
- `chat_handler` — intent classification; holds the optional AI model
  behind an `RwLock` so a finished download hot-loads without a restart
- `model_manager` — the on-disk AI model store (`<app_data_dir>/models`)
- `download_progress` — `(downloaded, total)` bytes while a model
  download runs; the frontend polls `model_status` instead of using an
  event bus
- unlock-attempt tracking for brute-force throttling

Commands are synchronous and short, except `download_model`, which runs
the blocking download on `tauri::async_runtime::spawn_blocking`.

## Key flows

**Unlock.** Frontend → `unlock_vault` command → vault service derives
keys from password + device Secret Key (see [2026-11-21 - Security.md](../design/2026-11-21%20-%20Security.md))
→ constant-time verify against `VaultMeta` → session holds the master
key in `SecretMem`. Locking drops the session and zeroizes the keys.

**Store / retrieve an entry.** Commands delegate to the vault service,
which asks the repository for the entry's ciphertext, unwraps its DEK
with the session KEK, and decrypts (or the reverse on write, with a fresh
DEK per write). Plaintext exists only for the duration of the call.

![How an entry moves](Architecture%20-%20Entry%20Data%20Flow.png)

*Storing an entry: the value crosses into the core as a DTO and reaches the database as ciphertext; retrieval returns the revealed value along the dashed path.*

**Chat.** Frontend → `send_chat` → `ChatHandler` classifies the message
(rule-based today; [2026-11-21 - AI-and-Models.md](../design/2026-11-21%20-%20AI-and-Models.md)) → maps the
intent to an `Action` → the shell performs vault actions through native
UI flows. Secret values are never entered in, or echoed to, chat: for
vault actions the response suppresses the user's message text.

**Model download.** Frontend card → `download_model` → `ModelManager`
streams the catalog file to a `.part` file while hashing → SHA-256 must
match the pinned catalog hash → atomic rename into
`models/<id>/<version>/`, manifest written, old versions pruned → the
shell loads the model into the `ChatHandler`. Any failure before the
rename leaves the previous state untouched.

## On-disk layout (app data dir, `com.stern.app`)

```
stern.db                              SQLite vault (ciphertext entries)
models/
  installed.json                      Manifest: which model + version
  <model-id>/<version>/model.onnx     Verified model files, one dir
                                      per version so updates are swaps
```

Deleting the whole folder returns the app to first-run state.

## Error handling

One error type crosses every boundary: `VaultError` (domain errors,
crypto/storage failures, and `VaultError::Model` for model download /
verification problems). Tauri commands return `Result<T, VaultError>`;
its `Serialize` impl renders it as a plain string for the frontend.

## Testing

Tests live in per-crate `tests/` directories and run against temp dirs
and the in-memory keychain — no OS keychain, no network. The model
manager's tests drive installs from in-memory readers, which is how the
hash-verification, failed-update, and pruning semantics are pinned
without downloading anything. See [Development.md](../Development.md) for
the commands and gates.

