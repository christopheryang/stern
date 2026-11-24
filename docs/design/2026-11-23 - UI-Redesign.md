# UI redesign: design system, screens, and brand

Design of the 2026-11-23 frontend redesign. The decision to move styling
into a stylesheet is recorded in
[ADR-0009](../adr/0009-Stylesheet-Design-System.md); the plain-language
copy, Help page, and disclaimer in
[ADR-0011](../adr/0011-Plain-Language-Help-And-Disclaimer.md). No backend,
IPC, or security behavior changed.

## Goals

- A modern, calm look that fits a security product: light by default,
  dark when the user picks it in Settings.
- One place to change the look. Before this change every component
  carried its own inline `style="..."` strings and hex colors.
- A real identity: an app icon for the dock / task bar and a logo inside
  the app (the old icon was a 1×1 placeholder).
- No new network access, fonts, or runtime dependencies.

## Design system

All styling lives in
[`desktop/stern-ui/styles/stern.css`](../../desktop/stern-ui/styles/stern.css),
loaded by Trunk from `desktop/stern-app/index.html`.

| Piece | Choice |
| --- | --- |
| Theme switch | Settings → Look holds the light/dark choice. Settings opens from the top bar on every screen, including login and setup, so the theme can be changed before authenticating. |
| Theming | CSS custom properties on `:root`; a `:root[data-theme="dark"]` block overrides them. Light is the default; Settings → Look switches to dark, saved in the webview's local storage and applied before first paint by a small script in `index.html` ([ADR-0013](../adr/0013-Light-Default-Theme-Switch.md)). |
| Palette | Deep indigo background with soft radial glows; one accent gradient (sky `#38bdf8` → indigo `#6366f1` → violet `#a855f7`) used for primary buttons, the send button, user bubbles, and progress bars. |
| Surfaces | Translucent "glass" panels with a hairline border and backdrop blur for the top bar and cards. |
| Type | System font stack only (`-apple-system`, Segoe UI, Roboto…). The app never fetches web fonts. |
| Icons | Inline SVG stroke icons (`components/ui.rs`, `IconKind`), colored by `currentColor`. |
| Motion | Short fade/rise on cards and messages, a typing indicator, a spinner. `prefers-reduced-motion` disables it. |
| Status colors | Success (green), danger (rose), warning (amber) — each with a soft background for alerts and tags. |

Components choose class names (`.btn`, `.card`, `.bubble`, …); the only
remaining inline style is the download progress bar's computed width.

## Screens

- **Top bar** — the Stern logo on the left; on the right a
  **padlock** icon, a Help icon, and a Settings icon. The padlock is
  open and green when the vault is unlocked; clicking it locks the
  vault, after a "Lock Stern?" dialog that Settings → Locking can turn
  off. When the vault is locked it is closed, and clicking it moves to
  the password box. The first-run setup screen has no padlock, since
  there is no vault to lock yet. On Settings, Help, or the
  Disclaimer the right side is a single **Back** button; the main view
  stays mounted underneath, so nothing typed there is lost.
- **Setup** — brand mark, "Welcome to Stern", a live requirements
  checklist (five rules that tick green as the password satisfies them),
  a confirm field, a show-password toggle, and a checkbox agreeing to the
  disclaimer (with a link to read it). After the form is valid, a
  "Don't lose your password" step comes before the vault is created. The
  checklist and the submit-time check share one rule table
  (`password_rules`), so they cannot disagree.
- **Login** — "Welcome back" (no mark: the top bar already shows the
  logo), a password field, **Open**, and a small Disclaimer link.
- **Gates** — "Choose an AI model to get started" and one shared "Something
  went wrong" card (used for both the vault and the helper status
  checks) are centered cards with an icon badge and one action.
- **Chat** — an empty state (mark, "How can I help?", three starter chips
  that send ordinary messages: *Save a note*, *Save a document*, *Save a password*, and a tip that plain text files are the best long-term store
  ([ADR-0012](../adr/0012-Notes-First-And-Plain-Text-Advice.md));
  then avatars, name labels, gradient user bubbles, glass assistant
  bubbles, pill-shaped option buttons on the newest assistant message
  only, a typing indicator while a reply is pending, and auto-scroll to
  the newest message. Errors in chat are one plain sentence.
- **Composer** — a pill input with a round send button. When the newest
  assistant message asks for a secret the pill turns amber and shows a
  **Private** tag; the peek-mask behavior itself (each character visible
  for 2 seconds, then `*`) is unchanged.
- **Settings and first run** — one "AI model" card that says what an AI
  model is and suggests Small if unsure. Rows show the tier (Small or
  Large), its download size, a short description, a **Recommended** tag,
  what you gain and what you give up (ADR-0014), an **Active** tag, and
  Download / Use this / Update / Remove. The first-run gate shows the
  same card, so a new user picks and downloads right there; a percentage and a gradient bar show downloads. A failed
  download shows one plain sentence with the technical text as small
  print (other errors show the sentence alone).
- **Help and Disclaimer** — Help is a list of collapsible questions
  (the first open) built from `docs/Help.md`; the Disclaimer is a plain
  document built from `docs/Disclaimer.md`. Both are embedded at compile
  time, so the docs and the app cannot disagree.
- **Short windows** — below 560 px of height (the app's minimum is
  400 px) the empty state, cards, and composer tighten so nothing is
  clipped. Tall cards scroll rather than overflow.

## Brand

![App icon](2026-11-23%20-%20UI-Redesign%20-%20App%20Icon.png)

![Logo](2026-11-23%20-%20UI-Redesign%20-%20Logo.png)

The mark is a vault ring around a four-point star — the old mark's
vault door and spark, reduced to two plain shapes that stay legible at
16 px. The wordmark is drawn as stroked paths, so
it needs no font. The logo ships twice, with a dark-ink wordmark
(`stern-logo.svg`, light theme) and a pale one (`stern-logo-dark.svg`);
the stylesheet shows the one that matches the chosen theme.

Sources (the SVGs are the source of truth; the PNGs above are renders):

| File | Use |
| --- | --- |
| `desktop/stern-ui/assets/stern-icon.svg` | App icon: gradient tile with the mark. Rendered to 1024 px and fed to `cargo tauri icon`. Also the browser favicon in dev. |
| `desktop/stern-ui/assets/stern-logo.svg` | Logo (mark + wordmark) in the top bar, light theme. |
| `desktop/stern-ui/assets/stern-logo-dark.svg` | The same logo with a pale wordmark, dark theme. |
| `desktop/stern-ui/assets/stern-mark.svg` | Mark only: hero on auth/empty states, assistant avatar. |
| `desktop/stern-tauri/icons/` | Generated `icon.icns`, `icon.ico`, and PNG sizes, listed in `bundle.icon` in `tauri.conf.json`. |

To regenerate the platform icons after editing the icon SVG: render it
to a 1024 × 1024 PNG and run `cargo tauri icon <png>` from
`desktop/stern-tauri/` (then delete the `android/`, `ios/`, and
`Square*` outputs, which the desktop build does not use).

## Plain-language copy

The rules the copy follows (ADR-0011): one short sentence per label; say
"password", not "master password"; say "helper" and "size", not "AI
model"; never name a technology or a model; put anything longer in Help.
Raw errors are mapped by `friendly_error`
(`desktop/stern-ui/src/components/friendly.rs`) — known cases (wrong
password, lockout, locked, download) get a tailored sentence, anything
else gets "Something went wrong. Please try again." Only a failed
model download keeps its original text as small print; other errors
never show backend diagnostics.

## Verification

- `just lint`, `just fmt`, and `cargo clippy -p stern-ui -p stern-app
  --target wasm32-unknown-unknown -- -D warnings` pass; `trunk build`
  succeeds.
- Each screen was rendered in a browser against a stubbed
  `window.__TAURI__` bridge in both themes, including the guided
  store flow and the secret-mode composer (mask confirmed after the
  2-second window).
- Not covered: native rendering inside the Tauri webview (WKWebView on
  macOS, WebKitGTK on Linux). Check it with `just dev` — see the manual
  test pass in [Development.md](../Development.md).
