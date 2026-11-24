# ADR-0009: Frontend styling lives in a themed stylesheet, not inline styles

- **Status:** Accepted
- **Date:** 2026-11-23
- **Extends:** ADR-0008 (chat-first UI)

## Context

Every Leptos component carried its own inline `style="..."` strings and
literal hex colors — about eighty of them across five files, with the
same constants (button, input, card) redefined per file. Changing the
look meant editing every component, there was no light theme, no hover
or focus styling (inline styles cannot express `:hover`, `:focus-visible`,
media queries, or animations), and the app's icon was a 1×1 placeholder.

Alternatives considered: a CSS framework or utility classes (Tailwind)
would add a Node build step to a Rust-only toolchain; a Rust styling
crate (Stylers, Turf) would add a proc-macro dependency for little gain
at this size; keeping inline styles cannot express the interactions a
modern UI needs.

## Decision

- One plain stylesheet, `desktop/stern-ui/styles/stern.css`, loaded by
  Trunk's built-in `css` asset handling. Components use class names only.
- Theming through CSS custom properties: dark by default, light via
  `prefers-color-scheme`. No in-app toggle.
- System fonts only and inline SVG icons, so the redesign adds no
  network access and no dependencies — the no-network invariant in
  AGENTS.md is untouched.
- Brand assets (icon, logo, mark) are SVG sources in
  `desktop/stern-ui/assets/`, copied next to the app by Trunk; the
  platform icon set is generated from the icon with `cargo tauri icon`.
- Presentation only: no change to commands, DTOs, or the secret-handling
  path. The peek-mask composer logic is unchanged; only its markup moved
  into a `Composer` component.

## Consequences

- One file controls the look; a new theme is a new block of variables.
- `stern-ui` now has non-Rust assets and `stern-app`'s Trunk watch list
  includes them.
- Class names are not checked by the compiler: a typo silently drops a
  style. Mitigation is the manual pass in `docs/Development.md`.
- The look follows the OS color scheme; users cannot override it inside
  the app.
- Native webview rendering (WKWebView, WebKitGTK) can differ from the
  browser used to check the redesign — notably `backdrop-filter`
  support — so `just dev` remains the acceptance check.
