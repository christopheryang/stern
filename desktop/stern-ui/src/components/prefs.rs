// Copyright (c) 2026 Christopher Yang
// SPDX-License-Identifier: MIT

//! Small on-device preferences that are not secrets.
//!
//! Like the theme, they live in the webview's local storage on this
//! device only.

/// The local-storage key for "ask before locking".
pub const CONFIRM_LOCK_KEY: &str = "stern-confirm-lock";

/// Read a stored value: asking is the default, and only an explicit
/// `"off"` turns it off.
#[must_use]
pub fn confirm_lock_from_stored(value: Option<&str>) -> bool {
    value != Some("off")
}

fn storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}

/// Whether locking asks for confirmation first.
#[must_use]
pub fn confirm_lock_load() -> bool {
    let stored = storage().and_then(|s| s.get_item(CONFIRM_LOCK_KEY).ok().flatten());
    confirm_lock_from_stored(stored.as_deref())
}

/// Remember the choice for next time.
pub fn confirm_lock_save(on: bool) {
    if let Some(storage) = storage() {
        // Private modes can refuse storage; the choice then lasts for this run only.
        drop(storage.set_item(CONFIRM_LOCK_KEY, if on { "on" } else { "off" }));
    }
}

/// The app-wide "ask before locking" signal. `AppShell` provides it once,
/// so the Lock switch and the Settings choice always agree.
pub type ConfirmLockSignal = leptos::prelude::RwSignal<bool>;
