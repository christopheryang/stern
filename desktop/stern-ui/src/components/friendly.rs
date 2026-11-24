// Copyright (c) 2026 Christopher Yang
// SPDX-License-Identifier: MIT

//! Plain-language errors.
//!
//! The backend reports developer-style messages (`database error: …`,
//! `invalid peer certificate: …`); people should see a short sentence
//! they can act on. The technical text is kept as small print only for
//! download failures, where it helps someone sort out a network or
//! proxy problem; any other error shows the plain sentence alone, so
//! raw backend or database diagnostics never reach the screen.

/// An error ready to show: a plain sentence, plus the original text
/// when it is worth keeping for support.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FriendlyError {
    pub message: String,
    pub detail: Option<String>,
}

/// Strip the `promise error: JsValue("…")` wrapper the bridge adds
/// around a rejected command, so the small print reads cleanly.
fn tidy(raw: &str) -> String {
    let inner = raw
        .strip_prefix("promise error: JsValue(\"")
        .and_then(|rest| rest.strip_suffix("\")"))
        .unwrap_or(raw);
    inner.replace("\\\"", "\"")
}

/// Turn a raw backend/bridge error into a [`FriendlyError`].
#[must_use]
pub fn friendly_error(raw: &str) -> FriendlyError {
    let lower = raw.to_lowercase();
    let known = |message: &str| FriendlyError {
        message: message.to_string(),
        detail: None,
    };
    // Only errors the model manager raised count as download
    // failures, so an unrelated message that merely contains the word
    // (a path under "Downloads", say) is not mistaken for one.
    let tidy_lower = tidy(raw).to_lowercase();
    let is_download_failure = tidy_lower.starts_with("model error:")
        && (tidy_lower.contains("download")
            || tidy_lower.contains("certificate")
            || tidy_lower.contains("unknownissuer"));
    if lower.contains("invalid master password") {
        known("That password isn't right. Please try again.")
    } else if lower.contains("too many failed attempts") {
        known("Too many tries. Please wait a few minutes, then try again.")
    } else if lower.contains("vault locked") {
        known("Stern is locked. Enter your password to continue.")
    } else if lower.contains("a note needs a title") {
        known("Give your note a title, then save it again.")
    } else if lower.contains("changed somewhere else") {
        known("This note was changed somewhere else. Open it again to see the latest.")
    } else if is_download_failure {
        FriendlyError {
            message: "The download didn't work. Check your internet connection and try again."
                .to_string(),
            detail: Some(tidy(raw)),
        }
    } else {
        known("Something went wrong. Please try again.")
    }
}
