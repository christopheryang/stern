# ADR-0005: Rule-based intent classification first; the LLM is optional

- **Status:** Accepted (recorded 2026-11-21)
- **Date:** 2026-11-21

## Context

Stern's chat turns sentences like "get my Gmail credentials" into vault
actions, including destructive ones (delete). The classifier therefore
sits on a safety-critical path. Small local LLMs (the ~135M class that
fits Stern's size budget) are demonstrably unreliable at structured
output and instruction following, and any LLM introduces
nondeterminism into a path where a misclassified "delete" is a real
loss. Meanwhile the intent space is tiny — store, retrieve, list,
update, delete, plus vault lifecycle actions — and keyword scoring
covers the phrasings users actually type.

## Decision

- A **deterministic, rule-based classifier** (`ai/intent.rs`, keyword
  scoring) owns intent classification in production.
- The ONNX model is an optional enhancement layered behind the same
  `ChatHandler` interface; it takes over only when it beats the rules
  on the intent test corpus.
- **Chat never carries secret values.** Intents reference entries;
  secrets are entered and displayed in the entry UI. Chat responses to
  vault actions suppress the echoed user message.
- Destructive actions require explicit confirmation in the UI no matter
  which classifier produced the intent.

## Consequences

- Classification is fully testable and reproducible; the intent test
  corpus is the contract both classifiers are judged against.
- The app is fully functional with no model installed, which is what
  makes ADR-0006's optional download viable.
- Accepted cost: phrasing coverage is limited to what the rules know.
  Unrecognized phrasings return `Unknown` and a help response rather
  than a guess — the safe failure mode, at some UX cost.
- The LLM work is not wasted: the model pipeline (download, verify,
  load, hot-swap) is exercised in production, so swapping the
  classifier later is a small change behind a stable interface.
