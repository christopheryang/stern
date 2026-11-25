# ADR-0017: Text-only files with a 1 MB limit

- **Status:** Accepted (not yet built; see the
  [design](../design/2026-11-25%20-%20Vault-Assistant.md))
- **Date:** 2026-11-25

## Context

File upload (see [File Upload](../design/2026-11-24%20-%20File-Upload.md))
stores any file up to 10 MB as an encrypted blob, text or binary. Stern's
purpose is small, highly confidential text: it must be able to find an
entry by what it is *about*, and that needs the text of every entry.

A PDF, a Word document or a spreadsheet cannot be searched without
extracting its text, which needs a parser per format, and a scan needs OCR.
Each parser is attack surface that reads untrusted bytes inside the
process that holds the keys. A file Stern can store but not describe or
find works against the goal.

## Decision

- **Plain text only.** A file is accepted only if it is valid UTF-8 with no
  NUL byte. The content decides, never the extension (unchanged from the
  existing rule). A byte-order mark is removed.
- **Limit: 1 MiB (1,048,576 bytes),** shown to users as "1 MB". The size
  is checked on the open file before any of it is read, and the read is
  capped one byte past the limit (the existing `read_file_capped`
  behaviour, with the constant lowered).
- **Known binary extensions are refused up front** (pdf, doc, docx, xls,
  xlsx, ppt, pptx, png, jpg, gif, zip, and similar) with a plain message
  ("Stern keeps plain text files only, like .txt, .md, .html, .csv or
  .json"), even if the bytes happen to be valid UTF-8.
- **The file picker suggests text extensions** (txt, md, html, htm, csv,
  tsv, json, xml, yaml, yml, toml, ini, log, conf, env) but does not
  restrict the user to them; the content check is the gate.
- **No new parsers.** HTML is stored as it is and indexed by its visible
  text, using a small tag stripper that skips script and style content.
  CSV and the rest are indexed as text.
- **Existing binary entries stay.** An entry stored by an earlier build as
  binary remains readable, deletable and renamable, is marked as an older
  file, and is not indexed. Nothing is deleted or converted.
- The limit is one constant, so it can be raised later, after measuring
  indexing time.

## Consequences

- Smaller attack surface: no document parsers, no OCR, no image decoders.
- A text file of at most 1 MiB is a bounded amount to index, so indexing
  can run in the background with a progress note.
- People who wanted to keep PDFs or scans in Stern cannot. The message says
  so up front rather than storing something Stern cannot search.
- Text in another encoding (UTF-16, Latin-1) is refused; the message
  suggests saving it as UTF-8. Supporting more encodings can be added
  later.
- The help text, user guide and file-upload design currently say 10 MB and
  allow binary files. They change in the same release that changes the
  code, not before.
