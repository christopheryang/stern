// Copyright (c) 2026 Christopher Yang
// SPDX-License-Identifier: MIT

//! Stored files: add a file to the vault, and get one back out.
//!
//! The shell reads and encrypts the file, so its bytes never cross to
//! the web frontend. Only plain text is accepted (ADR-0017). Size is checked before the read (see
//! [`read_file_capped`]), and each file gets its own data key, bound to
//! its entry through the blob AAD.

#![allow(clippy::unreachable)]

use crate::AppState;
use std::io::Read;
use std::path::{Path, PathBuf};
use stern_core::application::vault::ports::crypto::CryptoProvider;
use stern_core::domain::vault::aad::{blob_aad, entry_aad};
use stern_core::domain::vault::crypto_constants::DEK_WRAPPED_LEN;
use stern_core::domain::vault::entry::{EncryptedEntry, EntryKind, EntryPayload};
use stern_core::domain::vault::errors::VaultError;
use stern_core::domain::vault::file::{
    check_name, check_text, classify, within_limit, Refusal, MAX_FILE_BYTES,
};
use stern_core::domain::vault::metadata::suggest_keywords;
use stern_core::infrastructure::sqlite::repository::EncryptedBlob;
use zeroize::Zeroizing;

/// Read a file of at most [`MAX_FILE_BYTES`], refusing anything larger
/// without reading it.
///
/// The file is opened once and every check uses that handle, and the
/// read is capped at one byte past the limit, so a file that grows during
/// the read is still refused.
pub fn read_file_capped(path: &Path) -> Result<Zeroizing<Vec<u8>>, VaultError> {
    read_file_limited(path, MAX_FILE_BYTES)
}

#[cfg(test)]
pub fn read_file_limited_for_test(
    path: &Path,
    limit: u64,
) -> Result<Zeroizing<Vec<u8>>, VaultError> {
    read_file_limited(path, limit)
}

fn read_file_limited(path: &Path, limit: u64) -> Result<Zeroizing<Vec<u8>>, VaultError> {
    let open_error = |e: std::io::Error| {
        tracing::warn!("file could not be opened: {e}");
        VaultError::Io("Couldn't open that file.".to_string())
    };
    let file = std::fs::File::open(path).map_err(open_error)?;
    let meta = file.metadata().map_err(open_error)?;
    if !meta.is_file() {
        return Err(VaultError::Io("That is not a file.".to_string()));
    }
    if !within_limit(meta.len()) || meta.len() > limit {
        return Err(VaultError::Io(file_too_big_message()));
    }
    let mut data = Zeroizing::new(Vec::new());
    file.take(limit.saturating_add(1))
        .read_to_end(&mut data)
        .map_err(|e| {
            tracing::warn!("file could not be read: {e}");
            VaultError::Io("Couldn't read that file.".to_string())
        })?;
    if u64::try_from(data.len()).unwrap_or(u64::MAX) > limit {
        return Err(VaultError::Io(file_too_big_message()));
    }
    Ok(data)
}

/// The plain-language refusal for an oversized file. Shared with the
/// frontend's wording (see the Help page).
#[must_use]
pub fn file_too_big_message() -> String {
    Refusal::TooBig.message().to_string()
}

/// A refusal as the error the user sees.
fn refuse(refusal: Refusal) -> VaultError {
    VaultError::Io(refusal.message().to_string())
}

/// The extensions the file picker offers first.
const TEXT_EXTENSIONS: &[&str] = &[
    "txt", "md", "html", "htm", "csv", "tsv", "json", "xml", "yaml", "yml", "toml", "ini", "log",
    "conf", "env",
];

/// What the user is told about a file that was just stored. No contents.
#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
pub struct StoredFile {
    pub id: String,
    pub name: String,
    pub size_bytes: u64,
    pub is_text: bool,
}

/// Store the file at `path` in the vault under its own name. Requires an
/// unlocked vault; returns the new entry's id with the name and size to
/// show the user.
pub fn add_file(state: &AppState, path: &Path) -> Result<StoredFile, VaultError> {
    let kek = {
        let session = state
            .session
            .lock()
            .map_err(|e| VaultError::Database(e.to_string()))?;
        if !session.is_unlocked {
            return Err(VaultError::VaultLocked);
        }
        Zeroizing::new(*session.kek()?)
    };

    // Refuse by name before the file is even opened, then an oversized
    // file before anything is read into memory, then by content.
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file")
        .to_string();
    check_name(&name).map_err(refuse)?;
    let raw = read_file_capped(path)?;
    // What is stored is the text without a byte-order mark. `raw` is wiped
    // when it goes out of scope.
    let text = check_text(&raw).map_err(refuse)?;
    let bytes = text.as_bytes();
    let info = classify(&name, bytes);

    let entry_id = uuid::Uuid::new_v4().to_string();
    let version = 1u64;
    let now = chrono::Utc::now();

    // The bytes are one blob under their own DEK, bound to the entry.
    let dek = state.crypto.generate_dek();
    let (nonce, ciphertext) = state
        .crypto
        .encrypt_entry(&dek, bytes, &blob_aad(&entry_id))?;
    let blob = EncryptedBlob {
        dek_wrapped: state.crypto.wrap_dek(&dek, &kek)?.to_vec(),
        nonce,
        ciphertext,
        byte_len: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
        mime_type: info.mime_type,
        is_text: info.is_text,
    };

    // The entry carries only what the list view shows: its name. Its own
    // key fields are unused for a file and hold an empty, encrypted payload
    // so every entry row has the same shape.
    let payload = EntryPayload {
        name: name.clone(),
        kind: EntryKind::Document,
        tags: vec![],
        fields: vec![],
        summary: String::new(),
        // Words from the text, so the file can be found by what it says.
        keywords: suggest_keywords(text),
    };
    let payload_json =
        serde_json::to_vec(&payload).map_err(|e| VaultError::Serialization(e.to_string()))?;
    let entry_dek = state.crypto.generate_dek();
    let (entry_nonce, entry_ciphertext) =
        state
            .crypto
            .encrypt_entry(&entry_dek, &payload_json, &entry_aad(&entry_id, version))?;
    let entry = EncryptedEntry {
        id: entry_id.clone(),
        kind: EntryKind::Document,
        name,
        tags: vec![],
        dek_wrapped: state.crypto.wrap_dek(&entry_dek, &kek)?.to_vec(),
        nonce: entry_nonce,
        ciphertext: entry_ciphertext,
        version,
        created_at: now,
        updated_at: now,
    };
    state.repository.insert_file(&entry, &blob)?;
    Ok(StoredFile {
        id: entry_id,
        name: entry.name,
        size_bytes: blob.byte_len,
        is_text: blob.is_text,
    })
}

/// Where a retrieved file is written: a path the user chose. Never
/// replaces an existing file.
pub fn write_retrieved(destination: &Path, bytes: &[u8]) -> Result<PathBuf, VaultError> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(destination).map_err(|e| {
        tracing::warn!("retrieved file could not be created: {e}");
        VaultError::Io("Couldn't save the file there. Pick another place.".to_string())
    })?;
    if file.write_all(bytes).and_then(|()| file.flush()).is_err() {
        drop(file);
        drop(std::fs::remove_file(destination));
        return Err(VaultError::Io("Couldn't write the file.".to_string()));
    }
    Ok(destination.to_path_buf())
}

/// Decrypt a stored file. Fails with a decryption error when the blob
/// has been altered or moved to another entry.
pub fn read_stored(state: &AppState, entry_id: &str) -> Result<Zeroizing<Vec<u8>>, VaultError> {
    let kek = {
        let session = state
            .session
            .lock()
            .map_err(|e| VaultError::Database(e.to_string()))?;
        if !session.is_unlocked {
            return Err(VaultError::VaultLocked);
        }
        Zeroizing::new(*session.kek()?)
    };
    let blob = state
        .repository
        .get_blob(entry_id)?
        .ok_or_else(|| VaultError::EntryNotFound(entry_id.to_string()))?;
    let wrapped = <[u8; DEK_WRAPPED_LEN]>::try_from(blob.dek_wrapped.as_slice())
        .map_err(|_| VaultError::DecryptionFailed)?;
    let dek = state.crypto.unwrap_dek(&wrapped, &kek)?;
    let plaintext =
        state
            .crypto
            .decrypt_entry(&dek, &blob.nonce, &blob.ciphertext, &blob_aad(entry_id))?;
    Ok(Zeroizing::new(plaintext.to_vec()))
}

/// Ask the user for a file with the native picker and store it. Returns
/// what was stored, or `None` when the user cancels.
#[tauri::command]
pub async fn add_file_from_picker(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<Option<StoredFile>, VaultError> {
    use tauri_plugin_dialog::DialogExt;
    // The filter only suggests text formats; the content check is the gate.
    let Some(picked) = app
        .dialog()
        .file()
        .add_filter("Text files", TEXT_EXTENSIONS)
        .add_filter("All files", &["*"])
        .blocking_pick_file()
    else {
        return Ok(None);
    };
    let path = picked
        .into_path()
        .map_err(|_| VaultError::Io("Pick a file on this computer.".to_string()))?;
    add_file(&state, &path).map(Some)
}

/// Ask the user where to save a stored file, then write it there.
/// Returns the saved path, or `None` when the user cancels.
#[tauri::command]
pub async fn save_file_to_picker(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    entry_id: String,
    suggested_name: String,
) -> Result<Option<String>, VaultError> {
    use tauri_plugin_dialog::DialogExt;
    let bytes = read_stored(&state, &entry_id)?;
    let Some(destination) = app
        .dialog()
        .file()
        .set_file_name(&suggested_name)
        .blocking_save_file()
    else {
        return Ok(None);
    };
    let destination = destination
        .into_path()
        .map_err(|_| VaultError::Io("Choose a place on this computer.".to_string()))?;
    write_retrieved(&destination, &bytes).map(|p| Some(p.to_string_lossy().to_string()))
}

/// Move a stored file to the trash, like any other entry. It is removed
/// for good when the trash is emptied or after 30 days.
#[tauri::command]
pub async fn delete_stored_file(
    state: tauri::State<'_, AppState>,
    entry_id: String,
) -> Result<bool, VaultError> {
    {
        let session = state
            .session
            .lock()
            .map_err(|e| VaultError::Database(e.to_string()))?;
        if !session.is_unlocked {
            return Err(VaultError::VaultLocked);
        }
    }
    state
        .repository
        .trash_entry(&entry_id, &chrono::Utc::now().to_rfc3339())
}
