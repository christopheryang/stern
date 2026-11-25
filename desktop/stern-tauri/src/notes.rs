// Copyright (c) 2026 Christopher Yang
// SPDX-License-Identifier: MIT

//! Notes, edited in the note editor rather than through chat.
//!
//! A note is an entry of kind `Note` with a `title` and a `text` field, the
//! same shape chat stores, so notes saved by chat open in the editor too.
//! Its text is shown in full: notes are not masked.

#![allow(clippy::unreachable)]

use crate::AppState;
use serde::{Deserialize, Serialize};
use stern_core::application::vault::ports::crypto::CryptoProvider;
use stern_core::domain::vault::aad::entry_aad;
use stern_core::domain::vault::crypto_constants::{DEK_WRAPPED_LEN, KEK_LEN};
use stern_core::domain::vault::entry::{EncryptedEntry, EntryKind, EntryPayload, Field};
use stern_core::domain::vault::errors::VaultError;
use stern_core::domain::vault::metadata::{clean_keywords, clean_summary, suggest_keywords};
use stern_core::infrastructure::sqlite::repository::StoredPayload;
use zeroize::Zeroizing;

/// A note as the editor sees it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NoteDto {
    pub id: String,
    pub title: String,
    pub text: String,
    /// "What it's about", or empty.
    pub summary: String,
    /// Words the note can be found by, comma separated for editing.
    pub keywords: String,
    pub version: u64,
    pub updated_at: String,
}

/// The note list entry: enough to choose a note, without its text.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NoteSummary {
    pub id: String,
    pub title: String,
    pub updated_at: String,
}

/// What the editor sends about a note beyond its title and text. `None`
/// for the whole value means "leave what is stored as it is".
#[derive(Debug, Clone, Copy)]
pub struct About<'a> {
    pub summary: &'a str,
    /// Comma separated.
    pub keywords: &'a str,
}

/// Put the description into a payload: cleaned, and when the person left
/// the keywords empty, filled from the text so the note can be found.
fn apply_about(payload: &mut EntryPayload, about: &About<'_>, text: &str) {
    payload.summary = clean_summary(about.summary);
    let typed = clean_keywords(&[about.keywords]);
    payload.keywords = if typed.is_empty() {
        suggest_keywords(text)
    } else {
        typed
    };
}

/// Keywords as the editor shows them: one comma separated line.
fn keywords_line(keywords: &[String]) -> String {
    keywords.join(", ")
}

/// Refuse unless the vault is unlocked.
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

fn unlocked_kek(state: &AppState) -> Result<Zeroizing<[u8; KEK_LEN]>, VaultError> {
    let session = state
        .session
        .lock()
        .map_err(|e| VaultError::Database(e.to_string()))?;
    if !session.is_unlocked {
        return Err(VaultError::VaultLocked);
    }
    Ok(Zeroizing::new(*session.kek()?))
}

fn note_fields(payload: &EntryPayload) -> (String, String) {
    let value = |key: &str| {
        payload
            .fields
            .iter()
            .find(|f| f.key == key)
            .map_or_else(String::new, |f| f.value.clone())
    };
    (value("title"), value("text"))
}

/// Decrypt one note's payload. Not a note, or not decryptable, is an error.
fn open_note(
    state: &AppState,
    kek: &[u8; KEK_LEN],
    entry: &EncryptedEntry,
) -> Result<EntryPayload, VaultError> {
    let wrapped = <[u8; DEK_WRAPPED_LEN]>::try_from(entry.dek_wrapped.as_slice())
        .map_err(|_| VaultError::DecryptionFailed)?;
    let dek = state.crypto.unwrap_dek(&wrapped, kek)?;
    let plaintext = state.crypto.decrypt_entry(
        &dek,
        &entry.nonce,
        &entry.ciphertext,
        &entry_aad(&entry.id, entry.version),
    )?;
    serde_json::from_slice(&plaintext).map_err(|e| VaultError::Serialization(e.to_string()))
}

const fn is_note(entry: &EncryptedEntry) -> bool {
    matches!(entry.kind, EntryKind::Note)
}

/// Every note in the vault, newest first, without their text.
///
/// A note's title is its entry name, which is stored beside the
/// ciphertext, so listing never decrypts a note body.
pub fn list(state: &AppState) -> Result<Vec<NoteSummary>, VaultError> {
    ensure_unlocked(state)?;
    let mut notes: Vec<_> = state
        .repository
        .list_all_entries()?
        .into_iter()
        .filter(is_note)
        .collect();
    // Sort by the timestamp itself: RFC 3339 strings only sort correctly
    // when every offset and fraction has the same shape.
    notes.sort_by_key(|entry| std::cmp::Reverse(entry.updated_at));
    Ok(notes
        .into_iter()
        .map(|entry| NoteSummary {
            id: entry.id,
            title: entry.name,
            updated_at: entry.updated_at.to_rfc3339(),
        })
        .collect())
}

/// The notes whose title matches `query`: an exact match (ignoring case)
/// wins; otherwise every title that contains it. A blank query matches
/// nothing, not everything.
pub fn find(state: &AppState, query: &str) -> Result<Vec<NoteSummary>, VaultError> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return Ok(Vec::new());
    }
    let all = list(state)?;
    let exact: Vec<NoteSummary> = all
        .iter()
        .filter(|note| note.title.to_lowercase() == needle)
        .cloned()
        .collect();
    if !exact.is_empty() {
        return Ok(exact);
    }
    Ok(all
        .into_iter()
        .filter(|note| note.title.to_lowercase().contains(&needle))
        .collect())
}

/// One note, with its text.
pub fn load(state: &AppState, id: &str) -> Result<NoteDto, VaultError> {
    let kek = unlocked_kek(state)?;
    let entry = state
        .repository
        .list_all_entries()?
        .into_iter()
        .find(|e| e.id == id && is_note(e))
        .ok_or_else(|| VaultError::EntryNotFound(id.to_string()))?;
    let payload = open_note(state, &kek, &entry)?;
    let (title, text) = note_fields(&payload);
    // The same fallback `list` shows: the entry name, when the payload
    // has no title field.
    let title = if title.is_empty() {
        entry.name.clone()
    } else {
        title
    };
    Ok(NoteDto {
        id: entry.id.clone(),
        title,
        text,
        summary: payload.summary.clone(),
        keywords: keywords_line(&payload.keywords),
        version: entry.version,
        updated_at: entry.updated_at.to_rfc3339(),
    })
}

/// Set a field's value, adding the field when the payload lacks it.
fn set_field(payload: &mut EntryPayload, key: &str, value: &str) {
    if let Some(field) = payload.fields.iter_mut().find(|f| f.key == key) {
        field.value = value.to_string();
    } else {
        payload.fields.push(Field {
            key: key.to_string(),
            value: value.to_string(),
            hidden: false,
        });
    }
}

/// Create a new note, or save changes to an existing one. A blank title
/// is refused: a note needs a name to be found by.
///
/// Saving an existing note changes only its title and text. Whatever else
/// the entry carries (tags, other fields) is kept as it was.
pub fn save(
    state: &AppState,
    id: Option<&str>,
    title: &str,
    text: &str,
    about: Option<&About<'_>>,
) -> Result<NoteDto, VaultError> {
    let title = title.trim();
    if title.is_empty() {
        return Err(VaultError::Io("A note needs a title.".to_string()));
    }
    let kek = unlocked_kek(state)?;
    let now = chrono::Utc::now();
    match id {
        None => {
            let mut payload = EntryPayload {
                name: title.to_string(),
                kind: EntryKind::Note,
                tags: vec![],
                fields: vec![],
                summary: String::new(),
                keywords: vec![],
            };
            set_field(&mut payload, "title", title);
            set_field(&mut payload, "text", text);
            match about {
                Some(about) => apply_about(&mut payload, about, text),
                None => payload.keywords = suggest_keywords(text),
            }
            // The plaintext JSON is wiped when it goes out of scope.
            let payload_json = Zeroizing::new(
                serde_json::to_vec(&payload)
                    .map_err(|e| VaultError::Serialization(e.to_string()))?,
            );
            let entry_id = uuid::Uuid::new_v4().to_string();
            let version = 1u64;
            let dek = state.crypto.generate_dek();
            let (nonce, ciphertext) =
                state
                    .crypto
                    .encrypt_entry(&dek, &payload_json, &entry_aad(&entry_id, version))?;
            let entry = EncryptedEntry {
                id: entry_id.clone(),
                kind: EntryKind::Note,
                name: title.to_string(),
                tags: vec![],
                dek_wrapped: state.crypto.wrap_dek(&dek, &kek)?.to_vec(),
                nonce,
                ciphertext,
                version,
                created_at: now,
                updated_at: now,
            };
            state.repository.insert_entry(&entry)?;
            load(state, &entry_id)
        }
        Some(existing) => {
            let current = state
                .repository
                .list_all_entries()?
                .into_iter()
                .find(|e| e.id == existing && is_note(e))
                .ok_or_else(|| VaultError::EntryNotFound(existing.to_string()))?;
            // Start from what is stored, so tags and extra fields survive.
            let mut payload = open_note(state, &kek, &current)?;
            payload.name = title.to_string();
            set_field(&mut payload, "title", title);
            set_field(&mut payload, "text", text);
            if let Some(about) = about {
                apply_about(&mut payload, about, text);
            }
            let payload_json = Zeroizing::new(
                serde_json::to_vec(&payload)
                    .map_err(|e| VaultError::Serialization(e.to_string()))?,
            );
            // A new version: the AAD is bound to the version, so an old
            // ciphertext can never be swapped in under the new one.
            let version = current
                .version
                .checked_add(1)
                .ok_or_else(|| VaultError::Database("entry version out of range".to_string()))?;
            let dek = state.crypto.generate_dek();
            let (nonce, ciphertext) =
                state
                    .crypto
                    .encrypt_entry(&dek, &payload_json, &entry_aad(existing, version))?;
            let dek_wrapped = state.crypto.wrap_dek(&dek, &kek)?.to_vec();
            let updated_at = now.to_rfc3339();
            let stored = StoredPayload {
                dek_wrapped: &dek_wrapped,
                nonce: &nonce,
                ciphertext: &ciphertext,
                name: title,
                version,
                updated_at: &updated_at,
            };
            // Only if nobody saved a newer version since we read this one.
            let changed =
                state
                    .repository
                    .update_entry_payload(existing, current.version, &stored)?;
            if !changed {
                return Err(VaultError::Io(
                    "This note was changed somewhere else. Open it again to see the latest."
                        .to_string(),
                ));
            }
            load(state, existing)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_fields_read_title_and_text() {
        let payload = EntryPayload {
            name: "Groceries".to_string(),
            kind: EntryKind::Note,
            tags: vec![],
            fields: vec![
                Field {
                    key: "title".into(),
                    value: "Groceries".into(),
                    hidden: false,
                },
                Field {
                    key: "text".into(),
                    value: "eggs\nmilk".into(),
                    hidden: false,
                },
            ],
            summary: String::new(),
            keywords: vec![],
        };
        assert_eq!(
            note_fields(&payload),
            ("Groceries".into(), "eggs\nmilk".into())
        );
    }
}

#[tauri::command]
pub async fn list_notes(state: tauri::State<'_, AppState>) -> Result<Vec<NoteSummary>, VaultError> {
    list(&state)
}

#[tauri::command]
pub async fn load_note(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<NoteDto, VaultError> {
    load(&state, &id)
}

#[tauri::command]
pub async fn save_note(
    state: tauri::State<'_, AppState>,
    id: Option<String>,
    title: String,
    text: String,
    summary: Option<String>,
    keywords: Option<String>,
) -> Result<NoteDto, VaultError> {
    // Both or neither: an editor that sends only one is out of date.
    let about = summary
        .as_deref()
        .zip(keywords.as_deref())
        .map(|(summary, keywords)| About { summary, keywords });
    save(&state, id.as_deref(), &title, &text, about.as_ref())
}
