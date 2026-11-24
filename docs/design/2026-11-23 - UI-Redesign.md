# UI redesign: design system, screens, and brand

Design of the 2026-11-23 frontend redesign. The decision to move styling
into a stylesheet is recorded in
[ADR-0009](../adr/0009-Stylesheet-Design-System.md). No backend, IPC, or
security behavior changed.

## Goals

- A modern, calm look that fits a security product: dark by default,
  light when the OS asks for it.
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
| Theming | CSS custom properties on `:root`; a `prefers-color-scheme: light` block overrides them. No toggle — the app follows the OS. |
| Palette | Deep indigo background with soft radial glows; one accent gradient (sky `#38bdf8` → indigo `#6366f1` → violet `#a855f7`) used for primary buttons, the send button, user bubbles, and progress bars. |
| Surfaces | Translucent "glass" panels with a hairline border and backdrop blur for the top bar and cards. |
| Type | System font stack only (`-apple-system`, Segoe UI, Roboto…). The app never fetches web fonts. |
| Icons | Inline SVG stroke icons (`components/ui.rs`, `IconKind`), colored by `currentColor`. |
| Motion | Short fade/rise on cards and messages, a typing indicator, a spinner. `prefers-reduced-motion` disables it. |
| Status colors | Success (green), danger (rose), warning (amber) — each with a soft background for alerts and tags. |

Components choose class names (`.btn`, `.card`, `.bubble`, …); the only
remaining inline style is the download progress bar's computed width.

## Screens

- **Top bar** — the Stern logo on the left; once unlocked, a "Vault
  unlocked" status pill, a **Lock** button, and a Settings icon button.
  In Settings the right side is a single **Back to chat** button.
- **Setup** — brand mark, "Set up your vault", a live requirements
  checklist (five rules that tick green as the password satisfies them),
  confirm field, show-password toggle. After a valid password, a second
  step shows the no-recovery warning before the vault is created. The
  checklist and the submit-time check share one rule table
  (`password_rules`), so they cannot disagree.
- **Login** — brand mark, "Welcome back", password field, Unlock.
- **Gates** — model-required, model-status error, and vault-status error
  are centered cards with an icon badge and one action.
- **Chat** — an empty state (mark, greeting, three starter chips that
  send ordinary messages: *Save a password*, *Find a secret*, *List
  everything*); then avatars, name labels, gradient user bubbles, glass
  assistant bubbles, pill-shaped option buttons on the newest assistant
  message only, a typing indicator while a reply is pending, and
  auto-scroll to the newest message.
- **Composer** — a pill input with a round send button. When the newest
  assistant message asks for a secret the pill turns amber and shows a
  **Secret** tag; the peek-mask behavior itself (each character visible
  for 2 seconds, then `*`) is unchanged.
- **Settings** — one section card for the AI model picker. Rows show
  name, version/size, description, an **Active** tag, and Download / Use
  this model / Update / Remove; a gradient progress bar shows downloads.
- **Short windows** — below 560 px of height (the app's minimum is
  400 px) the empty state, cards, and composer tighten so nothing is
  clipped. Tall cards scroll rather than overflow.

## Brand

![App icon](2026-11-23%20-%20UI-Redesign%20-%20App%20Icon.png)

![Logo](2026-11-23%20-%20UI-Redesign%20-%20Logo.png)

The mark is a vault ring around a four-point star — the old mark's
vault door and spark, reduced to two plain shapes that stay legible at
16 px. The wordmark is drawn as stroked paths, so
it needs no font; in a light color scheme the logo SVG switches the
wordmark to dark ink.

Sources (the SVGs are the source of truth; the PNGs above are renders):

| File | Use |
| --- | --- |
| `desktop/stern-ui/assets/stern-icon.svg` | App icon: gradient tile with the mark. Rendered to 1024 px and fed to `cargo tauri icon`. Also the browser favicon in dev. |
| `desktop/stern-ui/assets/stern-logo.svg` | Logo (mark + wordmark) in the top bar. |
| `desktop/stern-ui/assets/stern-mark.svg` | Mark only: hero on auth/empty states, assistant avatar. |
| `desktop/stern-tauri/icons/` | Generated `icon.icns`, `icon.ico`, and PNG sizes, listed in `bundle.icon` in `tauri.conf.json`. |

To regenerate the platform icons after editing the icon SVG: render it
to a 1024 × 1024 PNG and run `cargo tauri icon <png>` from
`desktop/stern-tauri/` (then delete the `android/`, `ios/`, and
`Square*` outputs, which the desktop build does not use).

## Verification

- `just lint`, `just fmt`, and `cargo clippy -p stern-ui -p stern-app
  --target wasm32-unknown-unknown -- -D warnings` pass; `trunk build`
  succeeds.
- Each screen was rendered in a browser against a stubbed
  `window.__TAURI__` bridge in both color schemes, including the guided
  store flow and the secret-mode composer (mask confirmed after the
  2-second window).
- Not covered: native rendering inside the Tauri webview (WKWebView on
  macOS, WebKitGTK on Linux). Check it with `just dev` — see the manual
  test pass in [Development.md](../Development.md).
