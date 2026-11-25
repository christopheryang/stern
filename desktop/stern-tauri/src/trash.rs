// Copyright (c) 2026 Christopher Yang
// SPDX-License-Identifier: MIT

//! The trash: deleting puts an entry here for 30 days, and it can be
//! brought back. Only emptying the trash, or the 30-day purge when the vault
//! is unlocked, removes an entry for good (with its stored file).
//!
//! Trashed entries are left out of every list, search and export, because
//! the repository's normal queries skip them. Like the rest of the
//! organising commands, nothing here shows a secret's value.

#![allow(clippy::unreachable)]

use crate::organize::{matching_entries, Reply};
use crate::AppState;
use stern_core::ai::chat::{DELETE_NO, DELETE_YES, EMPTY_TRASH_MARKER};
use stern_core::domain::vault::entry::EncryptedEntry;
use stern_core::domain::vault::errors::VaultError;

/// How long an entry stays in the trash before it is purged.
pub const TRASH_DAYS: i64 = 30;

fn ensure_unlocked(state: &AppState) -> Result<(), VaultError> {
    let session = state
        .session
        .lock()
        .map_err(|e| VaultError::Database(e.to_string()))?;
    if session.is_unlocked {
        Ok(())
    } else {
        Err(VaultError::VaultLocked)
    }
}

/// Parse a stored RFC 3339 timestamp.
fn parse_time(text: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::parse_from_rfc3339(text)
        .ok()
        .map(|dt| dt.with_timezone(&chrono::Utc))
}

/// Trashed entries with the time they were trashed, newest first.
fn trashed_newest_first(
    state: &AppState,
) -> Result<Vec<(EncryptedEntry, chrono::DateTime<chrono::Utc>)>, VaultError> {
    let stamps = state.repository.trashed_stamps()?;
    let mut found: Vec<(EncryptedEntry, chrono::DateTime<chrono::Utc>)> = state
        .repository
        .list_trashed_entries()?
        .into_iter()
        .filter_map(|entry| {
            let at = stamps
                .iter()
                .find(|(id, _)| *id == entry.id)
                .and_then(|(_, stamp)| parse_time(stamp))?;
            Some((entry, at))
        })
        .collect();
    found.sort_by_key(|(_, at)| std::cmp::Reverse(*at));
    Ok(found)
}

/// "3 days ago", "today": whole days only, no float arithmetic.
fn days_ago(now: chrono::DateTime<chrono::Utc>, then: chrono::DateTime<chrono::Utc>) -> String {
    match now.signed_duration_since(then).num_days() {
        i64::MIN..=0 => "today".to_string(),
        1 => "yesterday".to_string(),
        days => format!("{days} days ago"),
    }
}

/// Put an entry the user confirmed deleting into the trash.
pub fn trash_entry(state: &AppState, entry_id: &str, name: &str) -> Result<Reply, VaultError> {
    ensure_unlocked(state)?;
    let at = chrono::Utc::now().to_rfc3339();
    Ok(Reply::say(
        if state.repository.trash_entry(entry_id, &at)? {
            format!(
                "Deleted \"{name}\". It stays in the trash for {TRASH_DAYS} days. \
             Say \"undo\" to bring it back."
            )
        } else {
            format!("\"{name}\" was already gone.")
        },
    ))
}

/// Bring an entry back: the one named, or the most recently deleted when no
/// name is given.
pub fn restore(state: &AppState, name: &str) -> Result<Reply, VaultError> {
    ensure_unlocked(state)?;
    let trashed = trashed_newest_first(state)?;
    if trashed.is_empty() {
        return Ok(Reply::say(
            "The trash is empty, so there is nothing to bring back.",
        ));
    }
    let entry = if name.trim().is_empty() {
        trashed.first().map(|(entry, _)| entry)
    } else {
        let all: Vec<EncryptedEntry> = trashed.iter().map(|(e, _)| e.clone()).collect();
        let hits = matching_entries(&all, name);
        match hits.as_slice() {
            [] => {
                return Ok(Reply::say(format!(
                    "I couldn't find \"{name}\" in the trash. Say \"show the trash\" to see what is there."
                )))
            }
            [one] => trashed.iter().map(|(e, _)| e).find(|e| e.id == one.id),
            many => {
                return Ok(Reply::say(format!(
                    "More than one deleted entry matches \"{name}\": {}. Say the full name of the one you want back.",
                    many.iter()
                        .map(|e| format!("\"{}\"", e.name))
                        .collect::<Vec<_>>()
                        .join(", ")
                )))
            }
        }
    };
    let Some(entry) = entry else {
        return Ok(Reply::say("I couldn't find that in the trash."));
    };
    Ok(Reply::say(if state.repository.restore_entry(&entry.id)? {
        format!("Brought \"{}\" back.", entry.name)
    } else {
        "That is no longer in the trash.".to_string()
    }))
}

/// List what is in the trash, newest first: names and kinds only.
pub fn show_trash(state: &AppState) -> Result<Reply, VaultError> {
    ensure_unlocked(state)?;
    let trashed = trashed_newest_first(state)?;
    if trashed.is_empty() {
        return Ok(Reply::say("The trash is empty."));
    }
    let now = chrono::Utc::now();
    let lines: Vec<String> = trashed
        .iter()
        .map(|(entry, at)| format!("• {} — deleted {}", entry.name, days_ago(now, *at)))
        .collect();
    Ok(Reply::say(format!(
        "In the trash ({} kept for {TRASH_DAYS} days). Say \"restore <name>\" to bring one back:\n\n{}",
        trashed.len(),
        lines.join("\n")
    )))
}

/// Ask for a yes before emptying the trash.
pub fn confirm_empty(state: &AppState) -> Result<Reply, VaultError> {
    ensure_unlocked(state)?;
    let count = state.repository.list_trashed_entries()?.len();
    if count == 0 {
        return Ok(Reply::say("The trash is already empty."));
    }
    state
        .chat_handler
        .set_pending_delete(EMPTY_TRASH_MARKER, "the trash");
    let noun = if count == 1 { "entry" } else { "entries" };
    Ok(Reply {
        message: format!("Delete the {count} {noun} in the trash for good? This can't be undone."),
        options: vec![DELETE_YES.to_string(), DELETE_NO.to_string()],
    })
}

/// Delete everything in the trash for good, after the user said yes.
pub fn empty(state: &AppState) -> Result<Reply, VaultError> {
    ensure_unlocked(state)?;
    let mut removed = 0usize;
    for entry in state.repository.list_trashed_entries()? {
        if state.repository.delete_entry(&entry.id)? {
            removed = removed.saturating_add(1);
        }
    }
    Ok(Reply::say(match removed {
        0 => "The trash was already empty.".to_string(),
        1 => "Deleted 1 entry for good.".to_string(),
        n => format!("Deleted {n} entries for good."),
    }))
}

/// Remove entries that have been in the trash for more than
/// [`TRASH_DAYS`] days, with their stored files. Returns how many.
pub fn purge_expired(state: &AppState) -> Result<usize, VaultError> {
    purge_expired_at(state, chrono::Utc::now())
}

/// [`purge_expired`] as of `now`, so tests can set the clock.
pub fn purge_expired_at(
    state: &AppState,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<usize, VaultError> {
    // If the date arithmetic ever overflows, purge nothing rather than
    // everything.
    let Some(cutoff) = now.checked_sub_signed(chrono::Duration::days(TRASH_DAYS)) else {
        return Ok(0);
    };
    let mut removed = 0usize;
    for (id, stamp) in state.repository.trashed_stamps()? {
        if parse_time(&stamp).is_some_and(|at| at < cutoff) && state.repository.delete_entry(&id)? {
            removed = removed.saturating_add(1);
        }
    }
    Ok(removed)
}
