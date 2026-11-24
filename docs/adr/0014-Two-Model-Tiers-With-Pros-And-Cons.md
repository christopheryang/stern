# ADR-0014: Two AI model tiers with pros and cons, chosen on the first-run screen

- **Status:** Accepted
- **Date:** 2026-11-23
- **Amends:** ADR-0010 (size tiers — which tiers exist and how they are presented)

## Context

ADR-0010 offered Small, Medium and Large. In practice Medium sat so
close to Small that it did not help anyone choose: a few extra
megabytes and no change anyone could describe. The first-run screen
said only "Choose a size" and sent the user to Settings, with no
explanation of what an AI model is or why one matters, and the picker
offered one-line descriptions that did not say what a user gives up.

## Decision

- **Two tiers:** **Small** (SmolLM2-135M-Instruct) and **Large**
  (Qwen2.5-0.5B-Instruct). Medium (SmolLM2-360M-Instruct) is removed
  from the catalog. A device that has it installed loses it at the next
  launch through `remove_retired_models`, and picks Small or Large.
- **Pros and cons are data, not UI text.** `ModelDescriptor` carries
  `pros` and `cons` lists in plain words. They reach the UI through
  `ModelStatusDto`, together with a `recommended` flag (true for the
  default, Small). A test bans technical words in them and the model
  names, as ADR-0010 does for names.
- **The picker explains itself.** It says what an AI model is in one
  sentence, that it runs on the user's own computer, and suggests Small
  to anyone unsure. Each row shows what the user gains and what they
  give up, and Small is tagged *Recommended*.
- **Choose in place.** The first-run gate is titled "Choose an AI model
  to get started" and shows the same picker, so a new user picks and
  downloads without a detour to Settings. When the download finishes
  chat opens.
- The model is never named, and it is still a user-initiated download
  verified against a pinned SHA-256 (ADR-0006, ADR-0008).

## Consequences

- Anyone who had Medium has no active model after the next launch and
  sees the first-run screen again.
- The honest trade-off is now visible: Small is quick but may misread
  unusual wording; Large understands more but is slower, bigger and
  heavier on memory. If the models are repointed, the pros and cons
  must be re-checked against the new ones.
- `ModelStatusDto` gained three fields. Because stern-ui keeps its own
  copy of the DTO (AGENTS.md), `model_setup.rs` was updated to match,
  with defaults so an older backend still parses.
