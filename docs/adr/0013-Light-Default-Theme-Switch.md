# ADR-0013: Light is the default theme, and users can switch to dark

- **Status:** Accepted
- **Date:** 2026-11-23
- **Amends:** ADR-0009 (stylesheet design system — how the theme is chosen)

## Context

ADR-0009 made dark the default and let the OS switch to light, with no
in-app control. A user whose OS is dark got a dark app whether they
wanted it or not, and no one could pick a look inside Stern.

## Decision

- **Light is the default.** The base `:root` block holds the light
  values. Stern no longer reads `prefers-color-scheme`, so the OS
  setting has no effect.
- **Dark is opt-in.** `:root[data-theme="dark"]` overrides the
  variables. Settings has a *Look* section with Light and Dark; tapping
  one applies it at once and saves it.
- **Storage:** the choice is kept in the webview's local storage under
  `stern-theme`, on this device only. No new Tauri command, no vault
  involvement, no network. A missing or unrecognised value means light.
- **No flash:** a small inline script in `index.html` sets
  `data-theme` before first paint, and the app also applies it on
  mount. The CSP already allows inline scripts.
- **Logo:** the logo is an `<img>`, so a CSS variable cannot recolour
  it. Two files ship — `stern-logo.svg` (dark wordmark, light theme)
  and `stern-logo-dark.svg` — and the stylesheet shows the one that
  matches.

## Consequences

- Anyone who relied on the OS dark mode now sees light until they pick
  dark once.
- The choice is per webview profile, so deleting the app data folder
  resets it to light.
- Every new colour must be defined in both blocks; the stylesheet test
  checks that the light block comes first and that no media query
  reappears.
- Only two themes exist; "follow the system" is not offered.
