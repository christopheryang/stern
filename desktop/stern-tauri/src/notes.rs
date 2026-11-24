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
use stern_core::infrastructure::sqlite::repository::StoredPayload;
use zeroize::Zeroizing;

/// A note as the editor sees it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NoteDto {
    pub id: String,
    pub title: String,
    pub text: String,
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
pub fn list(state: &AppState) -> Result<Vec<NoteSummary>, VaultError> {
    let kek = unlocked_kek(state)?;
    let mut notes = Vec::new();
    for entry in state.repository.list_all_entries()? {
        if !is_note(&entry) {
            continue;
        }
        let payload = open_note(state, &kek, &entry)?;
        let (title, _) = note_fields(&payload);
        notes.push(NoteSummary {
            id: entry.id.clone(),
            title: if title.is_empty() {
                entry.name.clone()
            } else {
                title
            },
            updated_at: entry.updated_at.to_rfc3339(),
        });
    }
    notes.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(notes)
}

/// The notes whose title matches `query`: an exact match (ignoring case)
/// wins; otherwise every title that contains it.
pub fn find(state: &AppState, query: &str) -> Result<Vec<NoteSummary>, VaultError> {
    let needle = query.trim().to_lowercase();
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
    Ok(NoteDto {
        id: entry.id.clone(),
        title,
        text,
        version: entry.version,
        updated_at: entry.updated_at.to_rfc3339(),
    })
}

/// Create a new note, or save changes to an existing one. A blank title
/// is refused: a note needs a name to be found by.
pub fn save(
    state: &AppState,
    id: Option<&str>,
    title: &str,
    text: &str,
) -> Result<NoteDto, VaultError> {
    let title = title.trim();
    if title.is_empty() {
        return Err(VaultError::Io("A note needs a title.".to_string()));
    }
    let kek = unlocked_kek(state)?;
    let now = chrono::Utc::now();
    let payload = EntryPayload {
        name: title.to_string(),
        kind: EntryKind::Note,
        tags: vec![],
        fields: vec![
            Field {
                key: "title".to_string(),
                value: title.to_string(),
                hidden: false,
            },
            Field {
                key: "text".to_string(),
                value: text.to_string(),
                hidden: false,
            },
        ],
    };
    let payload_json =
        serde_json::to_vec(&payload).map_err(|e| VaultError::Serialization(e.to_string()))?;

    match id {
        None => {
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
            // A new version: the AAD is bound to the version, so an old
            // ciphertext can never be swapped in under the new one.
            let version = current.version.saturating_add(1);
            let dek = state.crypto.generate_dek();
            let (nonce, ciphertext) =
                state
                    .crypto
                    .encrypt_entry(&dek, &payload_json, &entry_aad(existing, version))?;
            let dek_wrapped = state.crypto.wrap_dek(&dek, &kek)?.to_vec();
            let updated_at = now.to_rfc3339();
            let payload = StoredPayload {
                dek_wrapped: &dek_wrapped,
                nonce: &nonce,
                ciphertext: &ciphertext,
                name: title,
                version,
                updated_at: &updated_at,
            };
            let changed = state.repository.update_entry_payload(existing, &payload)?;
            if !changed {
                return Err(VaultError::EntryNotFound(existing.to_string()));
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
) -> Result<NoteDto, VaultError> {
    save(&state, id.as_deref(), &title, &text)
}
