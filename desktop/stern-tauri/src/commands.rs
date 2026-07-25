#![allow(clippy::unreachable, clippy::let_underscore_must_use)]

use crate::AppState;
use stern_core::application::vault::ports::crypto::CryptoProvider;
use stern_core::application::vault::ports::kdf::KeyDerivationProvider;
use stern_core::domain::vault::aad::entry_aad;
use stern_core::domain::vault::crypto_constants::{DEK_WRAPPED_LEN, SECRET_KEY_LEN, VAULT_SALT_LEN};
use stern_core::domain::vault::entry::{EncryptedEntry, EntryKind, EntryPayload, Field};
use stern_core::domain::vault::errors::VaultError;
use stern_core::domain::vault::kdf_params::KdfParams;
use stern_ipc::dto::{
    ChatResponse, CreateEntryRequest, CreateVaultRequest, EntryDto, ExportRequest,
    ExportResponse, ImportRequest, ImportResponse, SendChatRequest, UnlockRequest,
    UpdateEntryRequest, VaultStatus,
};
use tauri::State;

use std::time::{Duration, Instant};

const MAX_UNLOCK_ATTEMPTS: u32 = 5;
const LOCKOUT_DURATION: Duration = Duration::from_mins(5);

#[tauri::command]
pub async fn vault_status(state: State<'_, AppState>) -> Result<VaultStatus, VaultError> {
    let session = state
        .session
        .lock()
        .map_err(|e| VaultError::Database(e.to_string()))?;
    let entry_count = state.repository.count_entries()?;
    Ok(VaultStatus {
        is_initialized: state.repository.vault_exists()?,
        is_unlocked: session.is_unlocked,
        entry_count,
    })
}

#[tauri::command]
pub async fn create_vault(
    state: State<'_, AppState>,
    request: CreateVaultRequest,
) -> Result<VaultStatus, VaultError> {
    if state.repository.vault_exists()? {
        return Err(VaultError::Database("vault already exists".to_string()));
    }

    let params = KdfParams::argon2id_default();
    let salt = state.crypto.generate_vault_salt();
    let secret_key = state.crypto.generate_secret_key();

    let preprocessed = state
        .kdf
        .preprocess_2skd(request.password.as_bytes(), &secret_key)?;
    let master_key = state.kdf.derive_master_key(&preprocessed, &salt, &params)?;
    let kek = state.kdf.derive_kek(&master_key)?;
    let verify = state.kdf.derive_verify_hash(&master_key)?;

    state
        .repository
        .save_vault_meta(&salt, &verify, &secret_key, &params)?;

    let kek_bytes = *kek;
    {
        let mut session = state
            .session
            .lock()
            .map_err(|e| VaultError::Database(e.to_string()))?;
        session.unlock(kek_bytes, verify);
    }

    let entry_count = state.repository.count_entries()?;
    Ok(VaultStatus {
        is_initialized: true,
        is_unlocked: true,
        entry_count,
    })
}

#[tauri::command]
#[allow(clippy::arithmetic_side_effects)]
pub async fn unlock_vault(
    state: State<'_, AppState>,
    request: UnlockRequest,
) -> Result<VaultStatus, VaultError> {
    {
        let mut lockout = state
            .unlock_lockout
            .lock()
            .map_err(|e| VaultError::Database(e.to_string()))?;
        if let Some((count, last_attempt)) = *lockout {
            if count >= MAX_UNLOCK_ATTEMPTS {
                if let Some(remaining) = LOCKOUT_DURATION.checked_sub(last_attempt.elapsed()) {
                    return Err(VaultError::Database(format!(
                        "Too many failed attempts. Try again in {} seconds.",
                        remaining.as_secs()
                    )));
                }
                *lockout = None;
            }
        }
    }

    let meta = state
        .repository
        .load_vault_meta()?
        .ok_or_else(|| VaultError::Database("no vault found -- create one first".to_string()))?;

    let params = KdfParams::argon2id_default();
    let preprocessed = state
        .kdf
        .preprocess_2skd(request.password.as_bytes(), &meta.secret_key)?;
    let master_key = state.kdf.derive_master_key(&preprocessed, &meta.salt, &params)?;
    let kek = state.kdf.derive_kek(&master_key)?;
    let verify = state.kdf.derive_verify_hash(&master_key)?;

    if !state.crypto.verify_hash_matches(&verify, &meta.verify_hash) {
        let mut lockout = state
            .unlock_lockout
            .lock()
            .map_err(|e| VaultError::Database(e.to_string()))?;
        let entry = lockout.get_or_insert((0, Instant::now()));
        entry.0 += 1;
        entry.1 = Instant::now();
        drop(lockout);
        return Err(VaultError::InvalidMasterPassword);
    }

    {
        let mut lockout = state
            .unlock_lockout
            .lock()
            .map_err(|e| VaultError::Database(e.to_string()))?;
        *lockout = None;
    }

    let kek_bytes = *kek;
    {
        let mut session = state
            .session
            .lock()
            .map_err(|e| VaultError::Database(e.to_string()))?;
        session.unlock(kek_bytes, verify);
    }

    let entry_count = state.repository.count_entries()?;
    Ok(VaultStatus {
        is_initialized: true,
        is_unlocked: true,
        entry_count,
    })
}

#[tauri::command]

pub async fn lock_vault(state: State<'_, AppState>) -> Result<VaultStatus, VaultError> {
    let entry_count = state.repository.count_entries()?;
    {
        let mut session = state
            .session
            .lock()
            .map_err(|e| VaultError::Database(e.to_string()))?;
        session.lock();
    }
    Ok(VaultStatus {
        is_initialized: state.repository.vault_exists()?,
        is_unlocked: false,
        entry_count,
    })
}

#[tauri::command]

pub async fn list_entries(
    _state: State<'_, AppState>,
) -> Result<Vec<EntryDto>, VaultError> {
    Ok(vec![])
}

#[tauri::command]

pub async fn get_entry(
    _state: State<'_, AppState>,
    id: String,
) -> Result<EntryDto, VaultError> {
    Err(VaultError::EntryNotFound(id))
}

#[tauri::command]

pub async fn create_entry(
    _state: State<'_, AppState>,
    _request: CreateEntryRequest,
) -> Result<EntryDto, VaultError> {
    Err(VaultError::VaultLocked)
}

#[tauri::command]

pub async fn update_entry(
    _state: State<'_, AppState>,
    _request: UpdateEntryRequest,
) -> Result<EntryDto, VaultError> {
    Err(VaultError::VaultLocked)
}

#[tauri::command]

pub async fn delete_entry(
    _state: State<'_, AppState>,
    _id: String,
) -> Result<(), VaultError> {
    Err(VaultError::VaultLocked)
}

#[tauri::command]

pub async fn search_entries(
    _state: State<'_, AppState>,
    _query: String,
) -> Result<Vec<EntryDto>, VaultError> {
    Ok(vec![])
}

#[tauri::command]

pub async fn copy_to_clipboard(
    _state: State<'_, AppState>,
    text: String,
) -> Result<(), VaultError> {
    let provider = stern_core::infrastructure::clipboard::ClipboardProvider::new();
    provider.set_text(&text).map_err(VaultError::Io)
}

#[tauri::command]

pub async fn send_chat_message(
    state: State<'_, AppState>,
    request: SendChatRequest,
) -> Result<ChatResponse, VaultError> {
    let response = state.chat_handler.process_message(&request.content);

    match response.action {
        Some(stern_core::ai::chat::Action::StoreEntry {
            ref name,
            ref kind,
            ref fields,
        }) => handle_store_entry(&state, name, kind, fields, &response.message),
        Some(
            stern_core::ai::chat::Action::SearchEntry { .. }
            | stern_core::ai::chat::Action::ListEntries { .. }
            | stern_core::ai::chat::Action::ConfirmDelete { .. }
            | stern_core::ai::chat::Action::UpdateEntry { .. }
            | stern_core::ai::chat::Action::ExportVault
            | stern_core::ai::chat::Action::ImportVault
            | stern_core::ai::chat::Action::CreateVault
            | stern_core::ai::chat::Action::UnlockVault,
        ) => Ok(passthrough_response(&response)),
        None => Ok(ChatResponse {
            message: response.message,
            action: None,
            user_message_display: None,
        }),
    }
}

fn passthrough_response(response: &stern_core::ai::chat::ChatResponse) -> ChatResponse {
    let action_str = response.action.as_ref().map(action_to_string);
    ChatResponse {
        message: response.message.clone(),
        action: action_str,
        user_message_display: response.user_message_display.clone(),
    }
}

fn action_to_string(action: &stern_core::ai::chat::Action) -> String {
    match action {
        stern_core::ai::chat::Action::StoreEntry { .. } => "store".to_string(),
        stern_core::ai::chat::Action::SearchEntry { .. } => "search".to_string(),
        stern_core::ai::chat::Action::ListEntries { .. } => "list".to_string(),
        stern_core::ai::chat::Action::ConfirmDelete { .. } => "delete".to_string(),
        stern_core::ai::chat::Action::UpdateEntry { .. } => "update".to_string(),
        stern_core::ai::chat::Action::ExportVault => "export".to_string(),
        stern_core::ai::chat::Action::ImportVault => "import".to_string(),
        stern_core::ai::chat::Action::CreateVault => "create_vault".to_string(),
        stern_core::ai::chat::Action::UnlockVault => "unlock_vault".to_string(),
    }
}

fn handle_store_entry(
    state: &AppState,
    name: &str,
    kind: &str,
    fields: &[(String, String)],
    message: &str,
) -> Result<ChatResponse, VaultError> {
    let session = state
        .session
        .lock()
        .map_err(|e| VaultError::Database(e.to_string()))?;

    if !session.is_unlocked {
        return Ok(ChatResponse {
            message: "Vault is locked. Please unlock it first.".to_string(),
            action: Some("locked".to_string()),
            user_message_display: None,
        });
    }

    let kek = *session.kek()?;
    drop(session);

    let entry_kind = match kind {
        "note" | "document" => EntryKind::Note,
        _ => EntryKind::Login,
    };

    let entry_fields: Vec<Field> = fields
        .iter()
        .map(|(k, v)| Field {
            key: k.clone(),
            value: v.clone(),
            hidden: true,
        })
        .collect();

    let entry_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now();
    let version = 1u64;

    let payload = EntryPayload {
        name: name.to_string(),
        kind: entry_kind,
        tags: vec![],
        fields: entry_fields,
    };

    let payload_json =
        serde_json::to_vec(&payload).map_err(|e| VaultError::Serialization(e.to_string()))?;

    let aad = entry_aad(&entry_id, version);
    let dek = state.crypto.generate_dek();
    let (nonce, ciphertext) = state.crypto.encrypt_entry(&dek, &payload_json, &aad)?;
    let dek_wrapped = state.crypto.wrap_dek(&dek, &kek)?.to_vec();

    let entry = EncryptedEntry {
        id: entry_id,
        kind: entry_kind,
        name: name.to_string(),
        tags: vec![],
        dek_wrapped,
        nonce,
        ciphertext,
        version,
        created_at: now,
        updated_at: now,
    };

    state.repository.insert_entry(&entry)?;

    Ok(ChatResponse {
        message: message.to_string(),
        action: Some("store".to_string()),
        user_message_display: None,
    })
}

#[tauri::command]

pub async fn export_vault(
    state: State<'_, AppState>,
    request: ExportRequest,
) -> Result<ExportResponse, VaultError> {
    let session = state
        .session
        .lock()
        .map_err(|e| VaultError::Database(e.to_string()))?;
    if !session.is_unlocked {
        return Err(VaultError::VaultLocked);
    }
    let kek = *session.kek()?;
    drop(session);

    let encrypted_entries = state.repository.list_all_entries()?;
    let mut plaintext_entries: Vec<EntryPayload> = Vec::new();
    for enc in &encrypted_entries {
        let aad = entry_aad(&enc.id, enc.version);
        let wrapped = <[u8; DEK_WRAPPED_LEN]>::try_from(enc.dek_wrapped.as_slice())
            .map_err(|_| VaultError::DecryptionFailed)?;
        let dek = state.crypto.unwrap_dek(&wrapped, &kek)?;
        let plaintext =
            state.crypto.decrypt_entry(&dek, &enc.nonce, &enc.ciphertext, &aad)?;
        let payload: EntryPayload =
            serde_json::from_slice(&plaintext).map_err(|e| VaultError::Serialization(e.to_string()))?;
        plaintext_entries.push(payload);
    }

    let export_data = serde_json::to_vec_pretty(&plaintext_entries)
        .map_err(|e| VaultError::Serialization(e.to_string()))?;

    let salt = state.crypto.generate_vault_salt();
    let secret_key = state.crypto.generate_secret_key();
    let params = KdfParams::argon2id_default();
    let preprocessed = state
        .kdf
        .preprocess_2skd(request.password.as_bytes(), &secret_key)?;
    let master_key = state.kdf.derive_master_key(&preprocessed, &salt, &params)?;
    let export_kek = state.kdf.derive_kek(&master_key)?;

    let dek = state.crypto.generate_dek();
    let aad = b"stern-export-v1";
    let (nonce, ciphertext) = state.crypto.encrypt_entry(&dek, &export_data, aad)?;
    let dek_wrapped = state.crypto.wrap_dek(&dek, &export_kek)?;

    let export_file = EncryptedEntry {
        id: "export".to_string(),
        kind: EntryKind::Note,
        name: "stern-export".to_string(),
        tags: vec![],
        dek_wrapped: dek_wrapped.to_vec(),
        nonce,
        ciphertext,
        version: 1,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    let export_path = state
        .db_path
        .parent()
        .unwrap_or(&state.db_path)
        .join("stern-export.json");

    let export_json = serde_json::to_vec_pretty(&(&salt, &*secret_key, &export_file))
        .map_err(|e| VaultError::Serialization(e.to_string()))?;

    std::fs::write(&export_path, &export_json)
        .map_err(|e| VaultError::Io(e.to_string()))?;

    Ok(ExportResponse {
        path: export_path.to_string_lossy().to_string(),
        entry_count: plaintext_entries.len(),
    })
}

#[tauri::command]
#[allow(clippy::arithmetic_side_effects)]
pub async fn import_vault(
    state: State<'_, AppState>,
    request: ImportRequest,
) -> Result<ImportResponse, VaultError> {
    let session = state
        .session
        .lock()
        .map_err(|e| VaultError::Database(e.to_string()))?;
    if !session.is_unlocked {
        return Err(VaultError::VaultLocked);
    }
    let kek = *session.kek()?;
    drop(session);

    let path = std::path::PathBuf::from(&request.path);
    if !path.exists() {
        return Err(VaultError::Io(format!(
            "Import file not found: {}",
            request.path
        )));
    }
    if !path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("json"))
    {
        return Err(VaultError::Io(format!(
            "Import file must be a .json file, got: {}",
            request.path
        )));
    }

    let data =
        std::fs::read(&path).map_err(|e| VaultError::Io(e.to_string()))?;

    let (salt, secret_key, export_file): (
        [u8; VAULT_SALT_LEN],
        [u8; SECRET_KEY_LEN],
        EncryptedEntry,
    ) = serde_json::from_slice(&data)
        .map_err(|e| VaultError::Serialization(e.to_string()))?;

    let params = KdfParams::argon2id_default();
    let preprocessed = state
        .kdf
        .preprocess_2skd(request.password.as_bytes(), &secret_key)?;
    let master_key = state.kdf.derive_master_key(&preprocessed, &salt, &params)?;
    let export_kek = state.kdf.derive_kek(&master_key)?;

    let wrapped = <[u8; DEK_WRAPPED_LEN]>::try_from(export_file.dek_wrapped.as_slice())
        .map_err(|_| VaultError::DecryptionFailed)?;
    let dek = state.crypto.unwrap_dek(&wrapped, &export_kek)?;
    let aad = b"stern-export-v1";
    let plaintext =
        state.crypto.decrypt_entry(&dek, &export_file.nonce, &export_file.ciphertext, aad)?;

    let entries: Vec<EntryPayload> = serde_json::from_slice(&plaintext)
        .map_err(|e| VaultError::Serialization(e.to_string()))?;

    let mut imported = 0;
    for payload in entries {
        let entry_id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now();
        let version = 1u64;
        let entry_aad = entry_aad(&entry_id, version);

        let payload_json =
            serde_json::to_vec(&payload).map_err(|e| VaultError::Serialization(e.to_string()))?;

        let entry_dek = state.crypto.generate_dek();
        let (nonce, ciphertext) =
            state.crypto.encrypt_entry(&entry_dek, &payload_json, &entry_aad)?;
        let dek_wrapped = state.crypto.wrap_dek(&entry_dek, &kek)?.to_vec();

        let entry = EncryptedEntry {
            id: entry_id,
            kind: payload.kind,
            name: payload.name.clone(),
            tags: payload.tags.clone(),
            dek_wrapped,
            nonce,
            ciphertext,
            version,
            created_at: now,
            updated_at: now,
        };

        state.repository.insert_entry(&entry)?;
        imported += 1;
    }

    Ok(ImportResponse {
        entry_count: imported,
    })
}
