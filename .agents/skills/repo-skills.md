---
name: repo-skills
description: How to write conventional commit messages for the stern repository using cocogitto
---

# Commit Message Conventions

This project uses [cocogitto](https://docs.cocogitto.io/) to enforce [Conventional Commits](https://www.conventionalcommits.org/).

## Allowed Scopes

From `cog.toml`, the valid scopes are:

- **core** — stern-core (domain, application, infrastructure)
- **ipc** — stern-ipc (shared IPC message types)
- **tauri** — stern-tauri (Tauri desktop shell)
- **app** — stern-app (Leptos WASM frontend)
- **ui** — stern-ui (Leptos component library)
- **crypto** — encryption, key derivation, secrets handling
- **ai** — AI/ML inference, ONNX runtime
- **vault** — vault management, keyring integration
- **db** — SQLite, SeaORM, database schema
- **build** — Cargo workspace, Justfile, Trunk, Tauri config
- **ci** — GitHub Actions, PR templates, CI config
- **docs** — documentation
- **deps** — dependency updates

## Rules

### Rule 1: One scope per commit line

cocogitto only allows **one** scope per commit line. Commas are not allowed. When a commit touches multiple components, each component gets its own line in the message body with the `{action}({scope}):` prefix.

```
# WRONG — rejected by cocogitto
fix(core,tauri): address security review findings

# RIGHT — each component gets its own line
fix(core): remove passwords from chat intent enums
fix(tauri): add brute-force protection on unlock
fix(ipc): add ImportResponse DTO
```

### Rule 2: Every component touched gets its own line

Every file changed should be covered by a line in the commit message. Each line starts with `{action}({scope}):` followed by a brief description.

```
# WRONG — misses components
fix: address security review findings

# RIGHT — covers every component touched
fix(core): remove passwords from chat intent enums
fix(ai): sanitize user messages for vault actions
fix(ipc): add user_message_display field to ChatResponse DTO
fix(tauri): add path validation to import_vault command
```

### Rule 3: The first line is the subject

The first line is the conventional commit subject. Use it for the primary or broadest change. Additional lines in the body follow the same `{action}({scope}):` format.

```
fix: sanitize chat messages and add import validation

fix(core): set user_message_display to None for vault intents
fix(ai): remove dead path field from Intent::ImportVault
fix(tauri): add file existence and extension check on import
```

### Rule 4: Keep each line brief

Each `{action}({scope}):` line should be short and descriptive. No sentences, no punctuation at the end.

```
# WRONG — verbose sentences
fix(core): remove the password extraction from the Intent and Action enums so that
vault operations use native dialogs instead

# RIGHT — concise
fix(core): remove password extraction from Intent/Action enums
```

### Rule 5: Group by intent, not by file

When multiple files in the same component are changed for the same reason, one line suffices:

```
fix(core): remove passwords from chat intent enums
```

This covers `intent.rs`, `chat.rs`, and any related test files in `core`.

## Commit Types

| Type | When to use |
|------|------------|
| `feat` | New user-facing functionality |
| `fix` | Bug fix or security patch |
| `build` | Build system, CI, tooling |
| `ci` | CI/CD configuration only |
| `docs` | Documentation only |
| `refactor` | Code restructuring without behavior change |
| `style` | Formatting, whitespace, no logic change |
| `test` | Adding or updating tests |
| `chore` | Maintenance tasks |
| `perf` | Performance improvement |
| `revert` | Reverting a previous commit |
