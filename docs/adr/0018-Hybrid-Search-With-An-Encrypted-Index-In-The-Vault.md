# ADR-0018: Hybrid search with an encrypted index kept in the vault

- **Status:** Accepted. The keyword, date and filter half is built
  (phase 2); the encrypted meaning index is not (phase 3). See the
  [design](../design/2026-11-25%20-%20Vault-Assistant.md).
- **Date:** 2026-11-25

## Context

Chat can only find an entry by matching words against its name. A user who
asks "I stored something about my credit card a couple of months ago" gets
nothing unless the entry happens to be named with those words. The goal is
to find entries by name, by what they are about, and by when they were
saved, without weakening any security invariant.

Facts that shape the choice:

- Every entry has its own data key; names and kinds are stored beside the
  ciphertext, and timestamps are plain columns.
- A vault holds at most a few thousand small entries, so nothing needs a
  database server or an on-disk search engine.
- Embedding vectors can leak the text they came from, so they are as
  sensitive as the text.
- ADR-0001 forbids the network, except for user-initiated downloads that
  are pinned by SHA-256 (ADR-0006, ADR-0008).

## Decision

- **Three signals, merged.** A search combines exact filters (kind, folder,
  date range), keyword ranking (BM25) over text, and meaning ranking
  (embedding similarity) over text. The ranked lists are merged by
  reciprocal-rank fusion. A name match counts for more than a text match.
- **Search reads decrypted text in memory only.** While the vault is
  unlocked, the search builds its index from entry text and non-hidden
  fields. Hidden fields (passwords, card numbers, tokens) are never
  indexed. The in-memory index is wiped when the vault locks.
- **Vectors are saved inside the vault, encrypted.** Embeddings are kept
  in an `entry_index` table in `stern.db`. Each row is encrypted under its
  own data key, wrapped by the key-encrypting key, with authenticated data
  that binds it to the entry id, the entry version and the embedding
  model. A stale or moved row fails to decrypt and is rebuilt. Rows are
  removed with their entry (cascade). They hold offsets, not text. They are
  derived data, so they are left out of exports and rebuilt after an
  import.
- **English-only embedding model, as its own download.** The model is
  `all-MiniLM-L6-v2` (Apache-2.0), int8 quantised, about 23 MB. It is
  downloaded only when the user asks (the "search helper"), pinned by
  SHA-256 like every other download, and recorded in the model manifest.
  The exact file and hash are pinned when it is built. No other
  language is supported yet.
- **Our own small embedder,** on the `ort` and `tokenizers` crates Stern
  already uses: tokenise, run, mean-pool, normalise. The `fastembed` crate
  was considered; it needs `tokenizers` ^0.23 (Stern uses 0.22) and its
  default features download models and binaries on their own, which
  breaks the no-network rule. It can be revisited.
- **Everything works without the search helper.** Name, filter and keyword
  search need no model. The helper only adds meaning matching.
- **Results are candidates, not answers.** A search replies with a short
  list (name, kind, folder, date, and the entry's summary, see ADR-0019).
  The user taps one to open it. No value from a hidden field is shown in a
  list.

## Consequences

- "Credit card" can find "Visa" and "card number" with no hand-written
  synonym list, and new vocabulary needs no code.
- A new table, a new AAD helper and a new pinned download. Stale rows are
  detected by their AAD, so no separate cache-invalidation logic is
  needed.
- The saved index shows, to someone holding the database file, that an
  entry has an index and roughly how long its text is (the number of
  rows is not secret, the number of chunks is a size hint). This is the same
  kind of leak as a blob's size (ADR-0004).
- Indexing a 1 MiB file takes work. It runs in the background, and keyword
  search is available immediately.
- English only: text in other languages still matches by keyword and by
  name, but meaning matching will be poor.
- Quality is measured against an evaluation set (see the design) rather
  than assumed.
