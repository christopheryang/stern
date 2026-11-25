# ADR-0016: Nested folders that show structure, never secrets

- **Status:** Accepted
- **Date:** 2026-11-25

## Context

A vault of notes, documents and passwords gets hard to scan as it grows.
Users asked to organise it into folders, with folders inside folders, and
to see that structure at a glance without any secret value appearing on
screen. Every entry is encrypted under its own data key, and an entry's
name is stored beside its ciphertext (the note list relies on this).

The choices that were genuinely open:

- Whether an entry can sit in several folders (tags) or exactly one.
- Whether folder names are plain or encrypted.
- Where the tree lives: a table of its own, or inside each entry's payload.
- What happens to the contents when a folder is deleted.

## Decision

- **One folder per entry; folders nest** up to 8 levels. An entry in no
  folder is at the top level.
- **Folder names are encrypted.** Each folder name is encrypted under its
  own data key (wrapped by the key-encrypting key) with the folder's id as
  authenticated data (`folder_aad`), so a name cannot be swapped onto
  another folder.
- **The tree shape is plain structure**, in a `folders` table (`id`,
  `parent_id`) and a nullable `entries.folder_id`. Which folder is inside
  which, and which entry is in which folder, are not secret values; the
  alternative, putting the folder in each entry's encrypted payload, would
  force decrypting every entry to draw the tree.
- **Showing the tree reads names and kinds only.** The tree and list
  views are built from entry names and kinds. They never decrypt an
  entry's fields, so no secret value can reach them.
- **Deleting a folder keeps what is inside it.** Sub-folders and entries
  move up to the deleted folder's parent, in one transaction.
- **A folder can never be placed inside itself or one of its own
  children.** This is checked both in code and in the database move.
- **Deleting an entry asks first.** A yes-or-no answer is required, and any
  other message cancels the request (ADR-0005: destructive actions need
  explicit confirmation).
- Existing vaults are migrated by adding the `folder_id` column when it is
  missing; every existing entry stays outside any folder.

## Consequences

- The tree is cheap to draw and cannot leak a value, because it never
  decrypts one.
- The folder structure is visible to someone who has the database file:
  they can see how many folders exist, how deep, and how many entries are
  in each, though not the folder names. This is the same kind of leak as an
  entry's plain name and approximate size, and is accepted for the same
  reason (ADR-0004).
- One folder per entry keeps "move X to Y" unambiguous. Users who want an
  entry under two headings cannot have that; tags could be added later.
- A folder's name is not searchable by the database, only after decrypting
  every folder. The count is small, so the shell decrypts them all.
- Export and import do not yet carry folders. That belongs with the wider
  import and export work.
