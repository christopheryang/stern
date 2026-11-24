# ADR-0011: Plain-language copy, an in-app Help page, and a disclaimer agreed to at setup

- **Status:** Accepted
- **Date:** 2026-11-23
- **Extends:** ADR-0008 (chat-first UI), ADR-0009 (stylesheet design system), ADR-0010 (size tiers)

## Context

The first redesign kept the product's technical vocabulary on screen:
"master password", "vault", "ONNX Runtime", "peek-mask", raw backend
errors (`model error: download failed: io: invalid peer certificate`),
and paragraphs explaining downloads. The intended user has no technical
background. There was also nowhere to explain the product in more detail
and no statement of the limits of what Stern promises — for a program
that holds people's passwords and whose password cannot be recovered.

## Decision

- **Labels are short and plain.** Screens say "password" (not "master
  password"), "helper size" (not "AI model"), "Open" (not "Unlock"),
  and name no technology: no ONNX Runtime, no model names, no sizes in
  prose beyond the tier's download size. Settings has a one-line
  explanation; everything longer lives in Help.
- **Errors are sentences, not codes.** `friendly_error` maps backend and
  bridge errors to one plain sentence. Only a failed model download keeps
  its original text as small print, because that is where it helps
  someone sort out a network or proxy problem; every other error shows
  the sentence alone, so backend or database diagnostics never reach the
  screen. A message counts as a download failure only when the model
  manager raised it, so an unrelated error that merely mentions a
  "Downloads" folder is not mistaken for one. The chat's `Error: …`
  replies use the same mapping.
- **A Help page** (`docs/Help.md`) of short questions and answers,
  reachable from the top bar at any time — including before the vault
  is set up or unlocked, since it is static text.
- **A Disclaimer** (`docs/Disclaimer.md`): use at your own risk, no
  warranty, no recovery of a lost password, no guarantee of security,
  the assistant can make mistakes, third-party downloads, a limit of
  liability, and the MIT license. Setup requires ticking a box
  agreeing to it before the vault is created; the page is linked from
  the setup and login screens and from Help.
- **One copy of the text.** Both pages are Markdown under `docs/`,
  embedded in the app at compile time with `include_str!` and drawn by a
  small reader that supports only headings, bullets, and paragraphs. A
  test fails if either file uses inline markup the reader would show
  literally, or if Help mentions technical terms it exists to avoid.
- Help and the Disclaimer open as pages over the main view, which stays
  mounted (hidden), so Back never loses what was typed.

## Consequences

- Users are not asked to understand the technology, and the full
  explanation is one tap away.
- The agreement is a checkbox on the setup screen, not a stored record:
  Stern has no accounts or server, and the checkbox is enforced by the
  UI only. It shows the user was asked; it is not tamper-proof proof of
  consent.
- **The disclaimer is not legal advice and has not been reviewed by a
  lawyer.** Enforceability varies by jurisdiction; it should be reviewed
  before a public release.
- Hiding detail has a cost: someone debugging sees less in the UI. The
  technical text remains as small print on download failures only, and
  the Runbook still maps symptoms to causes. Anything else must be found
  in the log.
- The reader is deliberately minimal; adding bold, links, or tables to
  Help or the Disclaimer means extending it (the test will say so).
