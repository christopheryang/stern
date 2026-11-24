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

/// Remember the choice for next time. Returns whether it was saved:
/// private modes can refuse storage, and the choice then lasts for this
/// run only, which the caller can say.
#[must_use]
pub fn confirm_lock_save(on: bool) -> bool {
    storage().is_some_and(|storage| {
        storage
            .set_item(CONFIRM_LOCK_KEY, if on { "on" } else { "off" })
            .is_ok()
    })
}

/// The app-wide "ask before locking" signal. `AppShell` provides it once,
/// so the lock button and the Settings choice always agree.
pub type ConfirmLockSignal = leptos::prelude::RwSignal<bool>;

/// The shared signal, or a private one read from storage when none was
/// provided, so a component rendered on its own does not panic.
#[must_use]
pub fn confirm_lock_signal() -> ConfirmLockSignal {
    use leptos::prelude::{use_context, RwSignal};
    use_context::<ConfirmLockSignal>().unwrap_or_else(|| RwSignal::new(confirm_lock_load()))
}
