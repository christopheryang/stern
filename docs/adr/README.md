# Architecture Decision Records

An ADR records one significant decision: the context that forced it,
the decision itself, and the consequences — good and bad — that follow.
ADRs are how a future reader (human or agent) learns *why* the code is
shaped the way it is, instead of guessing or re-litigating settled
questions.

## Rules

- One decision per ADR, numbered sequentially (`NNNN-kebab-title.md`).
- ADRs are immutable once accepted. To change a decision, write a new
  ADR that supersedes the old one and update both files' Status lines.
- Use [template.md](template.md). Keep it short: a page is plenty.
- Status values: `Proposed`, `Accepted`, `Deprecated`,
  `Superseded by ADR-NNNN`.

## Index

| ADR | Decision | Status |
|---|---|---|
| [0001](0001-Local-First-No-Cloud.md) | Local-first: no cloud, accounts, or telemetry | Accepted |
| [0002](0002-Hexagonal-Core-Tauri-Leptos.md) | Hexagonal Rust core with a Tauri + Leptos shell | Accepted |
| [0003](0003-Key-Hierarchy-2SKD.md) | Key hierarchy: Argon2id + 2SKD, per-entry DEKs wrapped by a KEK | Accepted |
| [0004](0004-SQLite-Storage.md) | SQLite file for vault storage | Accepted |
| [0005](0005-Rule-Based-Intent-First.md) | Rule-based intent classification first; the LLM is optional | Accepted |
| [0006](0006-Post-Install-Model-Download.md) | AI model delivered by post-install download, not bundled | Accepted |
| [0007](0007-User-Selectable-Models.md) | Users select the AI model from a pinned catalog | Accepted |

ADRs 0001–0005 record decisions already embodied in the codebase
(retroactively documented 2026-11-21). ADR-0006 was recorded when the
decision was made.
