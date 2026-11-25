# ADR-0019: Model-suggested summaries that the user approves

- **Status:** Accepted (not yet built; see the
  [design](../design/2026-11-25%20-%20Vault-Assistant.md))
- **Date:** 2026-11-25
- **Amends:** the "no model-generated prose is ever shown" rule in
  ADR-0005 and `AGENTS.md`, for this one case only.

## Context

To find an entry by what it is about, each entry needs a short description
("what it's about"). Typing one for every note and file is a chore, so
most would be empty. A language model can draft one. Until now the model
only parsed commands, and no text it wrote was ever shown, so that a small
model's mistakes could never reach the user as an answer.

## Decision

- **The model may draft a summary.** After a note is saved or a text file
  is added, if a model is installed, it proposes a "what it's about" line
  of at most 160 characters and up to eight keywords.
- **Only as an editable suggestion.** The draft appears in the note editor
  or on the file card as a field the user can accept, edit or clear. It is
  stored only when the user saves. It is never written into a chat reply
  and never saved silently.
- **Input is limited.** The model sees the entry's name and its text and
  non-hidden fields. It never sees hidden fields. Text from a file is
  treated as data, never as instructions, and the model's output is plain
  text that is only ever put into the summary field.
- **Output is checked before it is offered.** A draft is dropped if it
  contains a run of six or more digits, a long token-like string, or any
  value from a hidden field. An empty or dropped draft falls back to
  keywords drawn from the text itself, so there is always something to
  accept or edit.
- **No model, no problem.** Without a model the user types a description,
  or accepts the keyword fallback.
- **Storage.** The summary and keywords are part of the entry's encrypted
  payload, not a plain column.
- **Where it shows.** The summary is shown when the vault is unlocked, in
  the entry view and in search result lists, to tell similar entries
  apart.

## Consequences

- Search by topic works for entries the user never described by hand.
- A small model can write a wrong or odd summary. The user sees it before
  it is kept, and can fix it, and a search does not trust the summary
  alone: the text is searched too.
- A summary is a statement about confidential content, and it appears on
  screen. A user who does not want that can clear it; the entry is still
  found by name and text.
- Drafting runs the model on every save, so it runs in the background and
  never blocks saving.
- `AGENTS.md` and the requirements carry the narrowed rule: model-written
  text is allowed only as an editable summary suggestion.
