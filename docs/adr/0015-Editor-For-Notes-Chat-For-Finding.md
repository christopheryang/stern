# ADR-0015: The note editor writes and edits notes; chat creates and finds them

- **Status:** Accepted
- **Date:** 2026-11-24

## Context

Chat was the only way to write a note. A note's body was collected
through the masked, guided conversation, so the text a user typed
could not be read back, corrected, or extended, and long notes were
awkward to enter one prompt at a time. Notes hold free text, not
credentials, so masking them hides what the user just wrote without
protecting anything.

## Decision

- **Chat** creates notes and retrieves them. It does not edit them.
- A **Notes page** (header button, unlocked vault only) lists note
  titles and opens a title field and a multi-line text area. Save
  creates a new note or saves changes to an existing one.
- **Notes are not masked.** The note title and text are shown in full
  in chat and in the editor. Passwords, card numbers, and contact
  fields stay masked.
- Each save writes a new version. The version is bound into the
  ciphertext's associated data, so an older row cannot be restored in
  place of a newer one.

## Consequences

- Users can review and change what they wrote without retyping it.
- Notes stored in chat before this change keep their text, and they
  appear in the editor list only if they were stored as notes.
- The chat no longer changes an existing note, so two ways of editing
  do not need to agree.
- Note text is visible on screen in full. Users who need hidden text
  should store it as a login or other masked entry.
- Documents (uploaded files) follow the same rule as notes for display;
  their binary content is never shown in chat.
