# ADR-0012: Notes and documents come first, and plain text files are the recommended long-term store

- **Status:** Accepted
- **Date:** 2026-11-23

## Context

Stern's users are people who do not trust large companies to hold their
secrets in the cloud. What they mostly want to keep are notes and
documents, not website logins. The guided store flow and every starter
prompt nevertheless led with "username/password", which pointed new users
at the least typical use. Separately, no application, Stern included,
can promise to be readable in thirty years; a plain text file can.

## Decision

- Wherever the app asks the user what to do, notes and documents come
  before passwords. The kind question is "What would you like to save?"
  and offers, in order: note, document, username/password, credit card,
  contact info. The starter chips are *Save a note*, *Save a document*,
  *Save a password*. Chat help and the unknown-request reply suggest
  notes first.
- A request that already names the kind ("save a note", "store a
  document") skips the kind question and goes straight to that kind's
  questions (`Conversation::start_for_kind`). A bare "store" and kinds
  the guided flow does not know still ask.
- Guided notes (kind `text`) are filed as notes, not logins.
- The app advises users that the best way to keep something forever is a
  plain text file (.txt) kept on their own computer: it needs no
  third-party software to open or edit and nobody can take it away.
  This appears as a tip on the empty chat, in the chat help reply, and
  as a Help page section. Stern is positioned as the place for things
  that must be locked; the text file as the place for things that must
  never be lost.

## Consequences

- Letters in the kind question changed (a = note, c = username/password).
  Anyone relying on the old letters gets a different kind; labels still
  work. Tests pin the new order.
- The product now recommends something other than itself for some uses,
  deliberately. The advice is general, not a feature: Stern does not
  create, import or export text files, and there is no file attachment
  support — "document" means pasted text.
