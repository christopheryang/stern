// Copyright (c) 2026 Christopher Yang
// SPDX-License-Identifier: MIT

//! The light/dark theme choice.
//!
//! Light is the default. The choice is stored in the webview's local
//! storage (on this device only) under [`STORAGE_KEY`] and applied as
//! a `data-theme` attribute on `<html>`; `index.html` applies a saved
//! dark choice before first paint.

/// The local-storage key holding the saved choice.
pub const STORAGE_KEY: &str = "stern-theme";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Theme {
    #[default]
    Light,
    Dark,
}

impl Theme {
    /// The stored form of the choice.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }

    /// Read a stored value; anything unknown or missing is the default.
    #[must_use]
    pub fn from_stored(value: Option<&str>) -> Self {
        match value {
            Some("dark") => Self::Dark,
            _ => Self::Light,
        }
    }
}

fn storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}

/// The saved theme, or light when nothing valid is saved.
#[must_use]
pub fn load() -> Theme {
    let stored = storage().and_then(|s| s.get_item(STORAGE_KEY).ok().flatten());
    Theme::from_stored(stored.as_deref())
}

/// Show `theme` now, without saving it.
pub fn apply(theme: Theme) {
    let root = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.document_element());
    if let Some(root) = root {
        // A failed write only means the old look stays; nothing to recover.
        drop(root.set_attribute("data-theme", theme.as_str()));
    }
}

/// Show `theme` and remember it for next time.
pub fn set(theme: Theme) {
    apply(theme);
    if let Some(storage) = storage() {
        // Private modes can refuse storage; the choice then lasts for this run only.
        drop(storage.set_item(STORAGE_KEY, theme.as_str()));
    }
}

/// The app-wide theme signal. `AppShell` provides it once, so the
/// header toggle and the Settings choice always agree.
pub type ThemeSignal = leptos::prelude::RwSignal<Theme>;

/// Switch to `theme` everywhere: the shared signal, the page, and storage.
pub fn choose(signal: ThemeSignal, theme: Theme) {
    use leptos::prelude::Set;
    set(theme);
    signal.set(theme);
}

impl Theme {
    /// The other theme.
    #[must_use]
    pub const fn other(self) -> Self {
        match self {
            Self::Light => Self::Dark,
            Self::Dark => Self::Light,
        }
    }
}
