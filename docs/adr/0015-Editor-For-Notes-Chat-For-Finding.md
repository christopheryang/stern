# ADR-0015: Notes are written in an editor inside the chat; chat creates and finds them

- **Status:** Accepted
- **Date:** 2026-11-24

## Context

Chat was the only way to write a note. A note's body was collected
through the masked, guided conversation, so the text a user typed
could not be read back, corrected, or extended, and long notes were
awkward to enter one prompt at a time. Notes hold free text, not
credentials, so masking them hides what the user just wrote without
protecting anything.

A first version put the editor on its own Notes page, opened from a
Notes button in the top bar. That made the editor a second place to go
to, apart from the conversation that starts a note.

## Decision

- **Chat** starts a note and finds notes. "Save a note", or choosing
  note from the kind list, opens the **note editor** in the chat: a
  title field and a multi-line text area, with Save note and Cancel.
- To change a note, the user says so ("edit my shopping list"). The
  same editor opens with that note's title and text. More than one
  match gets a plain reply naming them, not a guess.
- There is **no Notes page and no Notes button**. The editor exists
  only where a note is created or opened for editing.
- **Notes are not masked.** The note title and text are shown in full
  in chat and in the editor. Passwords, card numbers, and contact
  fields stay masked. Documents keep the guided questions, also unmasked.
- Each save writes a new version. The version is bound into the
  ciphertext's associated data, so an older row cannot be restored in
  place of a newer one. A save names the version it read, and is refused
  if the note has been saved since, so two writers cannot silently
  overwrite each other.
- Saving changes only the title and text. Tags and any other fields the
  note carries are kept as they were.
- Only one note is open at a time. A request to open another while one
  is open is answered with a plain message, so typed text is never
  replaced without the user's say.

## Consequences

- A note is written once, in one place, with room to see and fix it.
- Chat no longer collects a note one prompt at a time, so a note can't
  be saved by chat alone. This is the point of the change.
- The editor opens inline, so it scrolls with the transcript and is
  closed by saving or cancelling; there is no list of notes to browse.
  Finding a note stays a chat request.
- Note text is visible on screen in full. Users who need hidden text
  should store it as a login or other masked entry.
- Documents (uploaded files) follow the same rule as notes for display;
  their binary content is never shown in chat.
