# Stern documentation

The design record for Stern: how the system is put together, why it is
built this way, and how to work on it. Start with the overview below,
then follow the guide to the document you need.

## System overview

Stern is a **totally local, AI-powered secrets manager** for the
desktop. Passwords, keys, and notes live in an encrypted vault on the
user's own machine. There are no accounts, no cloud sync, and no
telemetry — the only network request the app ever makes is the AI
model download the user explicitly starts.

**The pieces** (details in [architecture/Architecture.md](architecture/Architecture.md)):

- **stern-core** — the hexagonal Rust core: the vault domain, the
  cryptography, SQLite storage, the OS keyring, and the AI layer
  (intent classification plus the downloadable ONNX models).
- **stern-ipc** — the DTOs that cross the boundary between core and
  shell.
- **stern-tauri** — the Tauri v2 desktop shell: commands, session
  state, and the model download machinery.
- **stern-ui / stern-app** — the Leptos (WASM) frontend: the vault
  views and the chat interface.

**Unlocking** combines the user's password with a device Secret Key
(2SKD) through Argon2id to derive the master key, which unwraps a
key-encryption key; every vault entry is encrypted under its own data
key (XChaCha20-Poly1305, wrapped with AES-KW). A locked vault is
ciphertext only. See [design/2026-11-21%20-%20Security.md](design/2026-11-21%20-%20Security.md).

**Chat is the front door.** Typing "store my GitHub token" is enough:
a classifier maps the sentence to an intent and the shell performs the
action through native UI flows. Classification is rule-based today;
an optional downloaded model is verified and held ready to take over.
Secret values never travel through chat. See
[design/2026-11-21%20-%20AI-and-Models.md](design/2026-11-21%20-%20AI-and-Models.md).

**AI models are not bundled.** The app ships a catalog of pinned
models (118–483 MB); the user picks one, downloads it after install,
and can switch or remove models at any time. Every download is
SHA-256-verified against the catalog before it is used. See
[design/2026-11-21%20-%20AI-and-Models.md](design/2026-11-21%20-%20AI-and-Models.md), ADR-0006, and ADR-0007.

## Navigate the docs

| Document | What it covers | Read it when… |
| --- | --- | --- |
| [architecture/Architecture.md](architecture/Architecture.md) | Crate layout, layering, runtime structure, data flows, on-disk layout | You need to know where something lives or how a request flows |
| [design/2026-11-21%20-%20Security.md](design/2026-11-21%20-%20Security.md) | Threat model and the vault cryptography in detail | You touch keys, encryption, or the vault format |
| [design/2026-11-21%20-%20AI-and-Models.md](design/2026-11-21%20-%20AI-and-Models.md) | Chat pipeline, the model catalog, and the download / verify / update / select / remove lifecycle | You work on chat or the AI models |
| [design/2026-11-22%20-%20Model-Backed-Chat.md](design/2026-11-22%20-%20Model-Backed-Chat.md) | Model-backed chat: parsing, guided conversations, retrieval Q&A, the vault gate, and the auth front door | You work on the chat pipeline or the app shell |
| [design/2026-11-23%20-%20UI-Redesign.md](design/2026-11-23%20-%20UI-Redesign.md) | The design system, screens, and brand (icon, logo) of the redesigned frontend | You change the UI's look or the app icon |
| [guide/User-Guide.md](guide/User-Guide.md) | The user's guide: a screenshot walkthrough from first launch to saving and finding a note | You change a screen or a step a user sees (the screenshots need regenerating) |
| [Help.md](Help.md) | The in-app help page: short questions and answers for people with no technical background | You change what the app does, or what a user sees |
| [Disclaimer.md](Disclaimer.md) | The in-app disclaimer and limit of liability | You change what Stern stores, sends, or promises |
| [Development.md](Development.md) | Environment, build / run / test / package workflows, release process | You build, test, or ship the app |
| [requirements/Requirements.md](requirements/Requirements.md) | What Stern must do, collected from the README, ADRs, and security invariants | You need the agreed behavior in one place |
| [Runbook.md](Runbook.md) | Operating and troubleshooting a running install | Something is wrong on a live install |
| [adr/](adr/README.md) | Architecture decision records — one file per significant decision, with its context and consequences | You want to know *why* something is the way it is, or you are about to change it |
| [../AGENTS.md](../AGENTS.md) | Instructions for coding agents working in this repo | You are an agent starting work |
| [../README.md](../README.md) | Install, run, and test-drive instructions | You are setting Stern up as a user |

## Architecture decision records

Significant or hard-to-reverse decisions are recorded as ADRs in
[adr/](adr/README.md): one decision per file, immutable once accepted
(a changed decision gets a new ADR that supersedes the old one). Use
[adr/Template.md](adr/Template.md) for new records and link them from
the ADR index.

## Keeping the docs current

- Behavior changes update the matching page in this directory, in the
  same change.
- Significant decisions get an ADR (see above).
- **Naming:** topic subdirectories are lowercase (`architecture/`,
  `design/`, `requirements/`, `adr/`); file names start with a
  capital letter, and acronyms are always capitalized. Files in
  `design/` — the detailed design of one feature implementation —
  are prefixed with their date: `design/2026-11-21 - AI-and-Models.md`.
  `architecture/` is the current system snapshot and stays
  higher-level than `design/`. When you rename or move a file, fix
  every link to it in the same change.
