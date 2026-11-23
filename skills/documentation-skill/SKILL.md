---
name: documentation-skill
description: Keep Stern's documentation fresh and put new docs in the right place. Use whenever you create or modify a pull request, change any behavior, interface, workflow, or structure, or add a document.
---

# Documentation skill

Stern's rule, from AGENTS.md: **a PR with stale docs is not ready.**
This skill is the working checklist behind that rule — how to check
freshness, and where a new doc goes when no existing doc covers a
change.

## When to use this skill

- Before creating a pull request, and again before updating one.
- Whenever a change alters behavior, an interface (DTO, command,
  catalog format), a workflow (build, CI, release), or the project
  structure.
- Whenever you add, rename, or move a document.

## The freshness checklist

1. **List what the PR changes** — behavior, interfaces, workflows,
   structure — from the diff, not from memory.
2. **For each change, find the doc that describes it today.** Search;
   don't guess:

   ```bash
   grep -rn "<the thing that changed>" README.md AGENTS.md docs/
   ```

3. **Update that doc in the same PR.** The doc must describe the
   system as the PR leaves it, not as it was. Pay attention to the
   quiet casualties: step-by-step walkthroughs (README "Test-drive",
   Development "Manual test pass"), troubleshooting entries
   (Runbook), invariants (AGENTS.md "Security invariants",
   Requirements), and flow descriptions (Architecture "Key flows").
4. **If no doc covers it, create one** (see "Where a new doc goes")
   and link it from the index that lists its siblings.
5. **Fix links both ways.** A new doc is linked from its index
   (README's Documentation list, `docs/README.md`'s table, or
   `docs/adr/README.md` for an ADR). A renamed or moved doc gets
   every link to it fixed in the same change.
6. **Re-read the changed docs against the diff** before calling the
   PR ready. If a sentence would mislead someone testing the new
   build, it is stale.

### Change type → docs to check

| The change… | Check these docs |
| --- | --- |
| User-visible behavior or flow | README (incl. Test-drive), `docs/architecture/Architecture.md` (Key flows), the feature's `docs/design/` doc, `docs/requirements/Requirements.md`, `docs/Runbook.md` |
| Interface: DTO, Tauri command, catalog/manifest format | `docs/architecture/Architecture.md`, the matching `docs/design/` doc, AGENTS.md (Gotchas, shipping sections) |
| Workflow: build, test, CI, release | AGENTS.md (Commands, CI), `docs/Development.md` |
| Structure: crates, directories, files moved | README (Architecture tree), `docs/architecture/Architecture.md` (Workspace map), AGENTS.md (Repository layout) |
| A significant or hard-to-reverse decision | A new ADR (below) plus the docs it supersedes |
| Security property or invariant | AGENTS.md (Security invariants), `docs/requirements/Requirements.md`, `docs/design/2026-11-21 - Security.md` |

## Where a new doc goes

- **`docs/design/`** — the detailed design of one feature
  implementation: how it works, the trade-offs, the measurements.
  One doc per feature; this is where detail belongs.
- **`docs/architecture/`** — the current snapshot of the whole
  system. Higher level than `design/`, never as detailed. Update
  it when the system's shape changes; don't write feature detail
  here.
- **`docs/adr/`** — one file per significant or hard-to-reverse
  decision, from `docs/adr/Template.md`, with the next free
  number, linked from `docs/adr/README.md`.
- **`docs/requirements/Requirements.md`** — the product's agreed
  behavior in one place. Edit the entry when the agreed behavior
  itself changes; this file records requirements, it doesn't
  invent them.
- **`docs/Development.md`** — build, test, and release workflows,
  including the manual test pass.
- **`docs/Runbook.md`** — operating and troubleshooting a running
  install.
- **`README.md`** — what a new user needs: what Stern is, how to
  install/run it, and an honest test-drive of the current build.
- **`AGENTS.md`** — rules and workflows for agents working in this
  repo (commands, conventions, gotchas, invariants).

If a change fits an existing doc, extend that doc — a new file is
for a topic nothing covers.

## Naming and format

- Directory names are always **lowercase** (`design/`,
  `architecture/`, `adr/`).
- Document file names have the **first letter capitalized**, and
  acronyms are always capitalized (`AI-and-Models.md`,
  `Runbook.md`).
- Every file in `docs/design/` is prefixed with its date:
  `YYYY-MM-DD - Name.md`.
- Links to a doc whose name contains spaces use the %20-encoded
  relative path (`design/2026-11-22%20-%20Model-Backed-Chat.md`).

## Images and diagrams

- Add a diagram wherever it clarifies a doc: a system diagram for
  the architecture snapshot, data-flow or sequence diagrams for
  flows and multi-step interactions, a schema diagram for data
  models. Keep the diagram's source so it can be regenerated, and
  update the diagram in the same change as the behavior it
  depicts.
- **Never embed images as base64 in Markdown.** Each image is a
  separate file alongside the doc that references it, named
  `<Doc name> - <Image name>.<ext>` (e.g.
  `Architecture - System Architecture.png`), linked with a
  %20-encoded relative path.

## Before you call the PR ready

- [ ] Every change in the diff has its describing doc updated, or
      a new doc created and linked.
- [ ] Walkthroughs, troubleshooting entries, and invariants were
      re-read, not just the feature's design doc.
- [ ] New/renamed docs follow the naming rules and are linked
      from their index.
- [ ] No doc still describes the pre-change behavior.
