# ADR-0010: Models are offered as Small, Medium, and Large tiers, never by name

- **Status:** Accepted
- **Date:** 2026-11-23
- **Amends:** ADR-0007 (user-selectable models — what the user selects, and which models are offered)

## Context

ADR-0007 let users pick from four catalog models by name (SmolLM-135M,
SmolLM2-135M-Instruct, SmolLM2-360M-Instruct, Qwen2.5-0.5B-Instruct).
The names mean nothing to most users, and the choice they actually face
is a trade-off: how much to download, how fast replies are, and how well
varied phrasing is understood. Two of the four entries were near
duplicates (both 135M-class), and the base SmolLM-135M model — the only
one using the few-shot completion prompt rather than ChatML — was the
weakest instruction follower.

## Decision

- The picker offers three **size tiers**: **Small**, **Medium**, and
  **Large**. The UI shows the tier, an approximate download size, and a
  plain-language description. It never shows the underlying model's
  name, version, or quantization.
- The tiers map to: Small → SmolLM2-135M-Instruct, Medium →
  SmolLM2-360M-Instruct, Large → Qwen2.5-0.5B-Instruct. The catalog's
  `display_name` is the tier name, and a test asserts no tier text
  contains a model name.
- **SmolLM-135M is retired.** The default model is now the Small tier
  (`smollm2-135m-instruct`). On startup, `remove_retired_models` removes
  any installed model that the catalog no longer offers, so a retired
  model cannot stay active with no way to manage it.
- The mapping from tier to model is a catalog detail. A tier can be
  repointed to a newer model by editing its catalog entry (new URL,
  hash, and a bumped `version`), without changing what the user sees.
- Model ids, hashes, and the download/verify/select/remove machinery are
  unchanged; so are the pinned SHA-256 rules in AGENTS.md.

## Consequences

- Users choose by what matters to them and are not asked to compare
  model families.
- A user who had SmolLM-135M installed loses it at the next launch and
  has no active model until they pick a tier; chat shows the
  model-required gate and the rule-based classifier is the fallback
  inside the app. Its prompt-style variant (`FewShot`) is no longer used
  by any catalog entry but remains in the code.
- Tier names are vague by design, so the description must carry the
  trade-off honestly; the sizes shown are the real download sizes.
- Anyone debugging a model problem has to map the tier to its model
  through the catalog; this ADR and `model_manager.rs` hold that mapping.
