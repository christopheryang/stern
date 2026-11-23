#![allow(clippy::unreachable, clippy::let_underscore_must_use)]

use crate::AppState;
use std::sync::Arc;
use stern_core::ai::model_manager::{catalog_model, current_runtime, ModelState, MODEL_CATALOG};
use stern_core::ai::AiModel;
use stern_core::application::vault::ports::crypto::CryptoProvider;
use stern_core::application::vault::ports::kdf::KeyDerivationProvider;
use stern_core::domain::vault::aad::entry_aad;
use stern_core::domain::vault::crypto_constants::{
    DEK_WRAPPED_LEN, KEK_LEN, SECRET_KEY_LEN, VAULT_SALT_LEN,
};
use stern_core::domain::vault::entry::{EncryptedEntry, EntryKind, EntryPayload, Field};
use stern_core::domain::vault::errors::VaultError;
use stern_core::domain::vault::kdf_params::KdfParams;
use stern_ipc::dto::{
    ChatResponse, CreateEntryRequest, CreateVaultRequest, EntryDto, ExportRequest, ExportResponse,
    ImportRequest, ImportResponse, ModelStatusDto, SendChatRequest, UnlockRequest,
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
    create_vault_inner(&state, &request.password)
}

/// Create the vault and leave the session unlocked. Shared by
/// the `create_vault` command and the chat vault gate.
fn create_vault_inner(state: &AppState, password: &str) -> Result<VaultStatus, VaultError> {
    if state.repository.vault_exists()? {
        return Err(VaultError::Database("vault already exists".to_string()));
    }

    let params = KdfParams::argon2id_default();
    let salt = state.crypto.generate_vault_salt();
    let secret_key = state.crypto.generate_secret_key();

    let preprocessed = state
        .kdf
        .preprocess_2skd(password.as_bytes(), &secret_key)?;
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
pub async fn unlock_vault(
    state: State<'_, AppState>,
    request: UnlockRequest,
) -> Result<VaultStatus, VaultError> {
    unlock_vault_inner(&state, &request.password)
}

/// Unlock the vault, with the failed-attempt lockout. Shared by
/// the `unlock_vault` command and the chat vault gate.
#[allow(clippy::arithmetic_side_effects)]
fn unlock_vault_inner(state: &AppState, password: &str) -> Result<VaultStatus, VaultError> {
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
        .preprocess_2skd(password.as_bytes(), &meta.secret_key)?;
    let master_key = state
        .kdf
        .derive_master_key(&preprocessed, &meta.salt, &params)?;
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

pub async fn list_entries(_state: State<'_, AppState>) -> Result<Vec<EntryDto>, VaultError> {
    Ok(vec![])
}

#[tauri::command]

pub async fn get_entry(_state: State<'_, AppState>, id: String) -> Result<EntryDto, VaultError> {
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

pub async fn delete_entry(_state: State<'_, AppState>, _id: String) -> Result<(), VaultError> {
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
    // Sync the vault's readiness so the handler can run its
    // setup/unlock gate before a request that needs the vault.
    let exists = state.repository.vault_exists()?;
    let unlocked = {
        let session = state
            .session
            .lock()
            .map_err(|e| VaultError::Database(e.to_string()))?;
        session.is_unlocked
    };
    state.chat_handler.set_vault_state(exists, unlocked);

    let response = state.chat_handler.process_message(&request.content);
    execute_chat_response(&state, response)
}

/// Execute a chat response's action, if it carries one.
fn execute_chat_response(
    state: &AppState,
    response: stern_core::ai::chat::ChatResponse,
) -> Result<ChatResponse, VaultError> {
    match response.action {
        Some(stern_core::ai::chat::Action::StoreEntry {
            ref name,
            ref kind,
            ref fields,
        }) => handle_store_entry(state, name, kind, fields, &response.message),
        Some(stern_core::ai::chat::Action::SearchEntry {
            ref query,
            ref field,
        }) => handle_search_entry(state, query, field.as_deref()),
        Some(stern_core::ai::chat::Action::RevealEntry {
            ref entry_id,
            ref field,
        }) => handle_reveal_entry(state, entry_id, field.as_deref()),
        Some(stern_core::ai::chat::Action::SetupVault { ref password }) => {
            handle_setup_vault(state, password)
        }
        Some(stern_core::ai::chat::Action::UnlockVaultWith { ref password }) => {
            handle_unlock_vault(state, password)
        }
        Some(
            stern_core::ai::chat::Action::ListEntries { .. }
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
            options: response.options,
            input_sensitive: response.input_sensitive,
        }),
    }
}

/// The setup gate collected a confirmed master password: create
/// the vault, then let the handler replay the interrupted
/// message (or confirm the setup when nothing was interrupted).
fn handle_setup_vault(state: &AppState, password: &str) -> Result<ChatResponse, VaultError> {
    match create_vault_inner(state, password) {
        Ok(_) => {
            state.chat_handler.set_vault_state(true, true);
            state.chat_handler.vault_gate_completed().map_or_else(
                || {
                    Ok(plain_chat_response(
                        "Your vault is set up and unlocked.".to_string(),
                    ))
                },
                |replayed| execute_chat_response(state, replayed),
            )
        }
        Err(e) => {
            state.chat_handler.vault_gate_failed();
            Ok(plain_chat_response(format!(
                "I couldn't set up the vault: {e}"
            )))
        }
    }
}

/// The unlock gate collected a master password: unlock (with
/// the lockout protection), then replay the interrupted
/// message. A wrong password re-prompts — the gate stays open.
fn handle_unlock_vault(state: &AppState, password: &str) -> Result<ChatResponse, VaultError> {
    match unlock_vault_inner(state, password) {
        Ok(_) => {
            state.chat_handler.set_vault_state(true, true);
            state.chat_handler.vault_gate_completed().map_or_else(
                || Ok(plain_chat_response("Vault unlocked.".to_string())),
                |replayed| execute_chat_response(state, replayed),
            )
        }
        Err(e) => Ok(ChatResponse {
            message: format!("{e}\n\nEnter your master password:"),
            action: None,
            user_message_display: None,
            options: Vec::new(),
            input_sensitive: true,
        }),
    }
}

fn passthrough_response(response: &stern_core::ai::chat::ChatResponse) -> ChatResponse {
    let action_str = response.action.as_ref().map(action_to_string);
    ChatResponse {
        message: response.message.clone(),
        action: action_str,
        user_message_display: response.user_message_display.clone(),
        options: response.options.clone(),
        input_sensitive: response.input_sensitive,
    }
}

fn action_to_string(action: &stern_core::ai::chat::Action) -> String {
    match action {
        stern_core::ai::chat::Action::StoreEntry { .. } => "store".to_string(),
        stern_core::ai::chat::Action::SearchEntry { .. } => "search".to_string(),
        stern_core::ai::chat::Action::RevealEntry { .. } => "reveal".to_string(),
        stern_core::ai::chat::Action::ListEntries { .. } => "list".to_string(),
        stern_core::ai::chat::Action::ConfirmDelete { .. } => "delete".to_string(),
        stern_core::ai::chat::Action::UpdateEntry { .. } => "update".to_string(),
        stern_core::ai::chat::Action::ExportVault => "export".to_string(),
        stern_core::ai::chat::Action::ImportVault => "import".to_string(),
        stern_core::ai::chat::Action::CreateVault => "create_vault".to_string(),
        stern_core::ai::chat::Action::UnlockVault => "unlock_vault".to_string(),
        stern_core::ai::chat::Action::SetupVault { .. } => "setup_vault".to_string(),
        stern_core::ai::chat::Action::UnlockVaultWith { .. } => "unlock_vault_with".to_string(),
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
            options: Vec::new(),
            input_sensitive: false,
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
        options: Vec::new(),
        input_sensitive: false,
    })
}

/// The chat response used when the vault is locked.
fn locked_chat_response() -> ChatResponse {
    ChatResponse {
        message: "Vault is locked. Please unlock it first.".to_string(),
        action: Some("locked".to_string()),
        user_message_display: None,
        options: Vec::new(),
        input_sensitive: false,
    }
}

/// A chat response carrying only a message.
const fn plain_chat_response(message: String) -> ChatResponse {
    ChatResponse {
        message,
        action: None,
        user_message_display: None,
        options: Vec::new(),
        input_sensitive: false,
    }
}

/// All entries plus the session KEK, or `None` when the vault is
/// locked.
type UnlockedVault = Option<(Vec<EncryptedEntry>, [u8; KEK_LEN])>;

fn unlocked_entries(state: &AppState) -> Result<UnlockedVault, VaultError> {
    let session = state
        .session
        .lock()
        .map_err(|e| VaultError::Database(e.to_string()))?;

    if !session.is_unlocked {
        return Ok(None);
    }

    let kek = *session.kek()?;
    drop(session);

    let entries = state.repository.list_all_entries()?;
    Ok(Some((entries, kek)))
}

/// An entry's decrypted fields, in stored order.
fn decrypt_entry_fields(
    state: &AppState,
    entry: &EncryptedEntry,
    kek: &[u8; KEK_LEN],
) -> Result<Vec<(String, String)>, VaultError> {
    let aad = entry_aad(&entry.id, entry.version);
    let wrapped = <[u8; DEK_WRAPPED_LEN]>::try_from(entry.dek_wrapped.as_slice())
        .map_err(|_| VaultError::DecryptionFailed)?;
    let dek = state.crypto.unwrap_dek(&wrapped, kek)?;
    let plaintext = state
        .crypto
        .decrypt_entry(&dek, &entry.nonce, &entry.ciphertext, &aad)?;
    let payload: EntryPayload =
        serde_json::from_slice(&plaintext).map_err(|e| VaultError::Serialization(e.to_string()))?;
    Ok(payload
        .fields
        .iter()
        .map(|field| (field.key.clone(), field.value.clone()))
        .collect())
}

/// Answer from one entry: the asked-for field, or every field when
/// none was asked for.
fn answer_entry(
    state: &AppState,
    entry: &EncryptedEntry,
    kek: &[u8; KEK_LEN],
    field: Option<&str>,
) -> Result<ChatResponse, VaultError> {
    let fields = decrypt_entry_fields(state, entry, kek)?;
    let message = field.map_or_else(
        || {
            if fields.is_empty() {
                format!("\"{}\" has no fields stored.", entry.name)
            } else {
                let lines: Vec<String> = fields
                    .iter()
                    .map(|(key, value)| format!("  {key}: {value}"))
                    .collect();
                format!("\"{}\":\n{}", entry.name, lines.join("\n"))
            }
        },
        |wanted| {
            fields
                .iter()
                .find(|(key, _)| key.eq_ignore_ascii_case(wanted))
                .map_or_else(
                    || {
                        let keys: Vec<&str> = fields.iter().map(|(key, _)| key.as_str()).collect();
                        format!(
                            "\"{}\" doesn't have a {wanted} field. It has: {}.",
                            entry.name,
                            keys.join(", ")
                        )
                    },
                    |(key, value)| format!("{} — {key}: {value}", entry.name),
                )
        },
    );
    Ok(plain_chat_response(message))
}

/// A label for a choice list, made unique with a " (2)", " (3)",
/// ... suffix when the name is already taken.
fn unique_label(name: &str, taken: &[String]) -> String {
    if !taken.iter().any(|label| label == name) {
        return name.to_string();
    }
    let mut suffix = 2usize;
    loop {
        let candidate = format!("{name} ({suffix})");
        if !taken.contains(&candidate) {
            return candidate;
        }
        suffix = suffix.saturating_add(1);
    }
}

/// Execute a chat search: match the query against entry names and
/// answer directly, or offer a lettered choice when several
/// entries match.
fn handle_search_entry(
    state: &AppState,
    query: &str,
    field: Option<&str>,
) -> Result<ChatResponse, VaultError> {
    let Some((entries, kek)) = unlocked_entries(state)? else {
        return Ok(locked_chat_response());
    };
    let needle = query.to_lowercase();
    let matches: Vec<&EncryptedEntry> = entries
        .iter()
        .filter(|entry| entry.name.to_lowercase().contains(&needle))
        .collect();
    match matches.as_slice() {
        [] => Ok(plain_chat_response(format!(
            "No secret found for \"{query}\"."
        ))),
        [entry] => answer_entry(state, entry, &kek, field),
        _ => {
            let mut labels: Vec<String> = Vec::new();
            let mut candidates: Vec<stern_core::ai::chat::EntryCandidate> = Vec::new();
            for entry in &matches {
                let label = unique_label(&entry.name, &labels);
                labels.push(label.clone());
                candidates.push(stern_core::ai::chat::EntryCandidate {
                    id: entry.id.clone(),
                    label,
                });
            }
            state
                .chat_handler
                .set_pending_choice(candidates, field.map(str::to_string));
            Ok(ChatResponse {
                message: format!("More than one match for \"{query}\". Which one?"),
                action: None,
                user_message_display: None,
                options: labels,
                input_sensitive: false,
            })
        }
    }
}

/// Execute a reveal for the entry the user picked from a lettered
/// choice.
fn handle_reveal_entry(
    state: &AppState,
    entry_id: &str,
    field: Option<&str>,
) -> Result<ChatResponse, VaultError> {
    let Some((entries, kek)) = unlocked_entries(state)? else {
        return Ok(locked_chat_response());
    };
    let Some(entry) = entries.iter().find(|entry| entry.id == entry_id) else {
        return Ok(plain_chat_response(
            "That entry is no longer available.".to_string(),
        ));
    };
    answer_entry(state, entry, &kek, field)
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
        let plaintext = state
            .crypto
            .decrypt_entry(&dek, &enc.nonce, &enc.ciphertext, &aad)?;
        let payload: EntryPayload = serde_json::from_slice(&plaintext)
            .map_err(|e| VaultError::Serialization(e.to_string()))?;
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

    std::fs::write(&export_path, &export_json).map_err(|e| VaultError::Io(e.to_string()))?;

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

    let data = std::fs::read(&path).map_err(|e| VaultError::Io(e.to_string()))?;

    let (salt, secret_key, export_file): (
        [u8; VAULT_SALT_LEN],
        [u8; SECRET_KEY_LEN],
        EncryptedEntry,
    ) = serde_json::from_slice(&data).map_err(|e| VaultError::Serialization(e.to_string()))?;

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
        state
            .crypto
            .decrypt_entry(&dek, &export_file.nonce, &export_file.ciphertext, aad)?;

    let entries: Vec<EntryPayload> =
        serde_json::from_slice(&plaintext).map_err(|e| VaultError::Serialization(e.to_string()))?;

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
            state
                .crypto
                .encrypt_entry(&entry_dek, &payload_json, &entry_aad)?;
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use std::time::Instant;
    use stern_core::ai::chat::Action as CoreAction;
    use stern_core::application::vault::session::VaultSession;
    use stern_core::infrastructure::crypto::{Argon2idKdfProvider, XChaCha20CryptoProvider};
    use stern_core::infrastructure::keychain::OsKeychainProvider;
    use stern_core::infrastructure::sqlite::repository::SqliteRepository;

    #[test]
    fn action_to_string_store_entry() {
        let action = CoreAction::StoreEntry {
            name: "test".into(),
            kind: "login".into(),
            fields: vec![],
        };
        assert_eq!(action_to_string(&action), "store");
    }

    #[test]
    fn action_to_string_search_entry() {
        let action = CoreAction::SearchEntry {
            query: "foo".into(),
            field: None,
        };
        assert_eq!(action_to_string(&action), "search");
    }

    #[test]
    fn action_to_string_list_entries() {
        let action = CoreAction::ListEntries {
            category: Some("logins".into()),
        };
        assert_eq!(action_to_string(&action), "list");
    }

    #[test]
    fn action_to_string_list_entries_no_category() {
        let action = CoreAction::ListEntries { category: None };
        assert_eq!(action_to_string(&action), "list");
    }

    #[test]
    fn action_to_string_confirm_delete() {
        let action = CoreAction::ConfirmDelete {
            name: "my-entry".into(),
        };
        assert_eq!(action_to_string(&action), "delete");
    }

    #[test]
    fn action_to_string_update_entry() {
        let action = CoreAction::UpdateEntry {
            name: "entry".into(),
            fields: vec![("key".into(), "val".into())],
        };
        assert_eq!(action_to_string(&action), "update");
    }

    #[test]
    fn action_to_string_export_vault() {
        assert_eq!(action_to_string(&CoreAction::ExportVault), "export");
    }

    #[test]
    fn action_to_string_import_vault() {
        assert_eq!(action_to_string(&CoreAction::ImportVault), "import");
    }

    #[test]
    fn action_to_string_create_vault() {
        assert_eq!(action_to_string(&CoreAction::CreateVault), "create_vault");
    }

    #[test]
    fn action_to_string_unlock_vault() {
        assert_eq!(action_to_string(&CoreAction::UnlockVault), "unlock_vault");
    }

    #[test]
    fn passthrough_response_with_action() {
        let core_resp = stern_core::ai::chat::ChatResponse {
            message: "hello".into(),
            action: Some(CoreAction::ExportVault),
            user_message_display: Some("display me".into()),
            options: Vec::new(),
            input_sensitive: false,
        };
        let ipc_resp = passthrough_response(&core_resp);
        assert_eq!(ipc_resp.message, "hello");
        assert_eq!(ipc_resp.action.as_deref(), Some("export"));
        assert_eq!(ipc_resp.user_message_display.as_deref(), Some("display me"));
    }

    #[test]
    fn passthrough_response_without_action() {
        let core_resp = stern_core::ai::chat::ChatResponse {
            message: "no action".into(),
            action: None,
            user_message_display: None,
            options: Vec::new(),
            input_sensitive: false,
        };
        let ipc_resp = passthrough_response(&core_resp);
        assert_eq!(ipc_resp.message, "no action");
        assert!(ipc_resp.action.is_none());
        assert!(ipc_resp.user_message_display.is_none());
    }

    #[test]
    fn passthrough_response_all_action_variants() {
        let variants: Vec<CoreAction> = vec![
            CoreAction::StoreEntry {
                name: "n".into(),
                kind: "login".into(),
                fields: vec![],
            },
            CoreAction::SearchEntry {
                query: "q".into(),
                field: None,
            },
            CoreAction::RevealEntry {
                entry_id: "id".into(),
                field: None,
            },
            CoreAction::ListEntries { category: None },
            CoreAction::ConfirmDelete { name: "n".into() },
            CoreAction::UpdateEntry {
                name: "n".into(),
                fields: vec![],
            },
            CoreAction::ExportVault,
            CoreAction::ImportVault,
            CoreAction::CreateVault,
            CoreAction::UnlockVault,
        ];

        let expected = [
            "store",
            "search",
            "reveal",
            "list",
            "delete",
            "update",
            "export",
            "import",
            "create_vault",
            "unlock_vault",
        ];

        for (action, expected_str) in variants.into_iter().zip(expected.iter()) {
            let core_resp = stern_core::ai::chat::ChatResponse {
                message: "m".into(),
                action: Some(action),
                user_message_display: None,
                options: Vec::new(),
                input_sensitive: false,
            };
            let ipc_resp = passthrough_response(&core_resp);
            assert_eq!(ipc_resp.action.as_deref(), Some(*expected_str));
        }
    }

    fn unlock_test_session(state: &AppState) {
        use stern_core::domain::vault::crypto_constants::{KEK_LEN, VERIFY_HASH_LEN};
        let kek = [7u8; KEK_LEN];
        let verify = [9u8; VERIFY_HASH_LEN];
        state.session.lock().unwrap().unlock(kek, verify);
    }

    fn store_test_entry(state: &AppState, name: &str, password: &str) {
        let fields = vec![
            ("username".to_string(), "chris".to_string()),
            ("password".to_string(), password.to_string()),
        ];
        let stored = handle_store_entry(state, name, "login", &fields, "stored").unwrap();
        assert_eq!(stored.action.as_deref(), Some("store"));
    }

    #[test]
    fn search_entry_answers_the_asked_field() {
        let (state, _path) = make_test_app_state();
        unlock_test_session(&state);
        store_test_entry(&state, "Google", "hunter2");

        let answer = handle_search_entry(&state, "google", Some("password")).unwrap();
        assert!(
            answer.message.contains("hunter2"),
            "message: {}",
            answer.message
        );

        let missing = handle_search_entry(&state, "nope", None).unwrap();
        assert!(missing.message.contains("No secret found"));
    }

    #[test]
    fn search_entry_with_two_matches_offers_a_choice_that_reveals() {
        let (state, _path) = make_test_app_state();
        unlock_test_session(&state);
        store_test_entry(&state, "Google", "personal-pass");
        store_test_entry(&state, "Google Work", "work-pass");

        let response = handle_search_entry(&state, "google", Some("password")).unwrap();
        assert_eq!(response.options.len(), 2);

        let picked = state.chat_handler.process_message("b");
        let entry_id = match picked.action {
            Some(CoreAction::RevealEntry { entry_id, .. }) => entry_id,
            other => panic!("expected RevealEntry, got: {other:?}"),
        };
        let answer = handle_reveal_entry(&state, &entry_id, Some("password")).unwrap();
        assert!(
            answer.message.contains("work-pass"),
            "message: {}",
            answer.message
        );
    }

    #[test]
    fn search_entry_while_locked_reports_locked() {
        let (state, _path) = make_test_app_state();
        let response = handle_search_entry(&state, "google", None).unwrap();
        assert_eq!(response.action.as_deref(), Some("locked"));
    }

    fn make_test_app_state() -> (AppState, tempfile::TempPath) {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let path = tmp.into_temp_path();
        let db_path = path.to_path_buf();
        let repository = Arc::new(SqliteRepository::new(&db_path).unwrap());
        let state = AppState {
            session: Mutex::new(VaultSession::new()),
            crypto: Arc::new(XChaCha20CryptoProvider::new()),
            kdf: Arc::new(Argon2idKdfProvider::new()),
            keychain: Arc::new(OsKeychainProvider::new()),
            chat_handler: Arc::new(stern_core::ai::ChatHandler::new(None)),
            model_manager: Arc::new(stern_core::ai::ModelManager::new(db_path.clone())),
            download_progress: Arc::new(Mutex::new(None)),
            db_path,
            repository,
            unlock_lockout: Mutex::new(None),
        };
        (state, path)
    }

    #[test]
    fn lockout_initial_state_is_none() {
        let (state, _path) = make_test_app_state();
        let lockout = state.unlock_lockout.lock().unwrap();
        assert!(lockout.is_none());
    }

    #[test]
    fn lockout_increments_on_failed_attempt() {
        let (state, _path) = make_test_app_state();
        {
            let mut lockout = state.unlock_lockout.lock().unwrap();
            let entry = lockout.get_or_insert((0, Instant::now()));
            entry.0 += 1;
            entry.1 = Instant::now();
        }
        let lockout = state.unlock_lockout.lock().unwrap();
        let (count, _) = lockout.unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn lockout_blocks_after_max_attempts() {
        let (state, _path) = make_test_app_state();
        {
            let mut lockout = state.unlock_lockout.lock().unwrap();
            *lockout = Some((MAX_UNLOCK_ATTEMPTS, Instant::now()));
        }

        let lockout = state.unlock_lockout.lock().unwrap();
        if let Some((count, last_attempt)) = *lockout {
            assert!(count >= MAX_UNLOCK_ATTEMPTS);
            let remaining = LOCKOUT_DURATION.checked_sub(last_attempt.elapsed());
            assert!(remaining.is_some(), "should still be in lockout window");
        } else {
            panic!("lockout should be set");
        }
    }

    #[test]
    fn lockout_resets_on_success() {
        let (state, _path) = make_test_app_state();
        {
            let mut lockout = state.unlock_lockout.lock().unwrap();
            *lockout = Some((MAX_UNLOCK_ATTEMPTS, Instant::now()));
        }
        {
            let mut lockout = state.unlock_lockout.lock().unwrap();
            *lockout = None;
        }
        let lockout = state.unlock_lockout.lock().unwrap();
        assert!(lockout.is_none());
    }

    #[test]
    fn lockout_clears_after_duration_expires() {
        let (state, _path) = make_test_app_state();
        {
            let mut lockout = state.unlock_lockout.lock().unwrap();
            let expired_time = Instant::now() - LOCKOUT_DURATION - Duration::from_secs(1);
            *lockout = Some((MAX_UNLOCK_ATTEMPTS, expired_time));
        }

        let lockout = state.unlock_lockout.lock().unwrap();
        if let Some((count, last_attempt)) = *lockout {
            if count >= MAX_UNLOCK_ATTEMPTS {
                let remaining = LOCKOUT_DURATION.checked_sub(last_attempt.elapsed());
                if remaining.is_some() {
                    panic!("lockout should have expired");
                }
            }
        }
    }

    #[test]
    fn lockout_counter_accumulates_across_attempts() {
        let (state, _path) = make_test_app_state();
        for i in 1..=MAX_UNLOCK_ATTEMPTS {
            let mut lockout = state.unlock_lockout.lock().unwrap();
            let entry = lockout.get_or_insert((0, Instant::now()));
            entry.0 += 1;
            entry.1 = Instant::now();
            let (count, _) = lockout.unwrap();
            assert_eq!(count, i);
        }
    }

    #[test]
    fn app_state_components_are_initialized() {
        let (state, _path) = make_test_app_state();
        assert!(!state.session.lock().unwrap().is_unlocked);
        assert!(state.unlock_lockout.lock().unwrap().is_none());
    }

    /// Drive one chat message the way `send_chat_message` does:
    /// sync the vault state, process, execute the action.
    fn chat(state: &AppState, text: &str) -> ChatResponse {
        let exists = state.repository.vault_exists().unwrap();
        let unlocked = state.session.lock().unwrap().is_unlocked;
        state.chat_handler.set_vault_state(exists, unlocked);
        let response = state.chat_handler.process_message(text);
        execute_chat_response(state, response).unwrap()
    }

    #[test]
    fn chat_sets_up_the_vault_before_the_first_store() {
        let (state, _path) = make_test_app_state();

        let gate = chat(&state, "store");
        assert!(gate.input_sensitive, "message: {}", gate.message);
        assert!(gate.message.contains("set up your vault"));
        assert!(!state.repository.vault_exists().unwrap());

        let confirm = chat(&state, "Correct-Horse-9!");
        assert!(confirm.input_sensitive);

        // Setup completes and the interrupted store resumes at
        // the kind question.
        let resumed = chat(&state, "Correct-Horse-9!");
        assert!(state.repository.vault_exists().unwrap());
        assert!(state.session.lock().unwrap().is_unlocked);
        assert_eq!(resumed.options.len(), 5, "message: {}", resumed.message);

        chat(&state, "a");
        chat(&state, "Google");
        chat(&state, "chris");
        let saved = chat(&state, "hunter2");
        assert!(saved.message.contains("Save this"));
        let done = chat(&state, "yes");
        assert_eq!(done.action.as_deref(), Some("store"));
        assert_eq!(state.repository.count_entries().unwrap(), 1);
    }

    #[test]
    fn chat_unlocks_a_locked_vault_and_replays_the_question() {
        let (state, _path) = make_test_app_state();
        create_vault_inner(&state, "correct horse battery").unwrap();
        store_test_entry(&state, "Google", "hunter2");
        state.session.lock().unwrap().lock();

        let prompt = chat(&state, "what's my password for google?");
        assert!(prompt.input_sensitive, "message: {}", prompt.message);
        assert!(prompt.message.contains("locked"));

        let wrong = chat(&state, "not the password");
        assert!(wrong.input_sensitive, "message: {}", wrong.message);
        assert!(!state.session.lock().unwrap().is_unlocked);

        let answer = chat(&state, "correct horse battery");
        assert!(
            answer.message.contains("hunter2"),
            "the replayed question should be answered: {}",
            answer.message
        );
    }
}

// ---------------------------------------------------------------------------
// AI model management: models are downloaded after install, never bundled.
// The user picks from the compiled-in catalog (Meetily-style model
// manager); exactly one installed model is active at a time.
// ---------------------------------------------------------------------------

fn model_status_dtos(state: &AppState) -> Vec<ModelStatusDto> {
    let progress = state
        .download_progress
        .lock()
        .ok()
        .and_then(|guard| guard.clone());
    let runtime_installed = state.model_manager.runtime_path().is_some();
    MODEL_CATALOG
        .iter()
        .map(|descriptor| {
            let info = state.model_manager.status_for(descriptor);
            let (state_str, installed_version) = match &info.state {
                ModelState::NotInstalled => ("not_installed", None),
                ModelState::Installed { version } => ("installed", Some(version.clone())),
                ModelState::UpdateAvailable {
                    installed_version, ..
                } => ("update_available", Some(installed_version.clone())),
            };
            let (is_downloading, downloaded_bytes, total_bytes) = match &progress {
                Some((id, downloaded, total)) if id == descriptor.id => (true, *downloaded, *total),
                _ => (false, 0, descriptor.size_bytes),
            };
            ModelStatusDto {
                model_id: descriptor.id.to_string(),
                display_name: descriptor.display_name.to_string(),
                description: descriptor.description.to_string(),
                state: state_str.to_string(),
                installed_version,
                latest_version: descriptor.version.to_string(),
                size_bytes: descriptor.size_bytes,
                is_active: info.is_active,
                runtime_installed,
                is_downloading,
                downloaded_bytes,
                total_bytes,
            }
        })
        .collect()
}

/// Clears the download-progress slot when dropped — including when the
/// download command is cancelled or panics — so a stuck `Some(...)` can
/// never block every later download.
struct DownloadProgressGuard {
    progress: Arc<std::sync::Mutex<Option<(String, u64, u64)>>>,
}

impl Drop for DownloadProgressGuard {
    fn drop(&mut self) {
        if let Ok(mut guard) = self.progress.lock() {
            *guard = None;
        }
    }
}

/// The installed ONNX Runtime library path, installing the runtime
/// first if this device does not have it. The install is a ~21–24 MB
/// download, so it runs on the blocking pool; it only ever happens as
/// part of a user-initiated model action (download/select), never at
/// startup. Devices whose manifest predates the runtime download have
/// models installed but no runtime — this is where they catch up.
async fn ensure_runtime_path(
    state: &AppState,
    progress_id: Option<&str>,
) -> Result<std::path::PathBuf, VaultError> {
    if let Some(path) = state.model_manager.runtime_path() {
        return Ok(path);
    }
    // Report the runtime download through the same progress slot a
    // model download uses, keyed by the model the user just picked,
    // so the UI's poller shows the ~21-24 MB fetch instead of a
    // silent wait. The slot is only reserved when it is free (a
    // model download may already own it), and only a slot reserved
    // here is cleared here — the guard never steals another
    // download's progress.
    let total = current_runtime().map_or(0, |runtime| runtime.size_bytes);
    let reserved = match progress_id {
        Some(id) => {
            let mut guard = state
                .download_progress
                .lock()
                .map_err(|e| VaultError::Model(e.to_string()))?;
            if guard.is_none() {
                *guard = Some((id.to_string(), 0, total));
                true
            } else {
                false
            }
        }
        None => false,
    };
    let _progress_guard = reserved.then(|| DownloadProgressGuard {
        progress: state.download_progress.clone(),
    });
    let progress = state.download_progress.clone();
    let progress_key = progress_id.map(str::to_string);
    let manager = state.model_manager.clone();
    tauri::async_runtime::spawn_blocking(move || {
        manager.ensure_runtime(&mut |downloaded, total_bytes| {
            if let Some(key) = &progress_key {
                if let Ok(mut guard) = progress.lock() {
                    // Only ever update the slot while it still
                    // belongs to this action's model.
                    if guard.as_ref().is_some_and(|(id, _, _)| id == key) {
                        *guard = Some((key.clone(), downloaded, total_bytes));
                    }
                }
            }
        })
    })
    .await
    .map_err(|e| VaultError::Model(format!("runtime install task failed: {e}")))?
}

/// Load one model file on the blocking pool. Loading a 100–500 MB model
/// must never run on a Tokio worker, and a failure is returned to the
/// caller (and shown in the UI) instead of being swallowed.
async fn load_model_at(
    runtime_path: std::path::PathBuf,
    path: std::path::PathBuf,
    tokenizer_path: std::path::PathBuf,
    prompt_style: stern_core::ai::model_manager::PromptStyle,
) -> Result<Arc<AiModel>, VaultError> {
    tauri::async_runtime::spawn_blocking(move || {
        AiModel::new(&runtime_path, &path, &tokenizer_path, prompt_style)
            .map(Arc::new)
            .map_err(|e| VaultError::Model(format!("model {} failed to load: {e}", path.display())))
    })
    .await
    .map_err(|e| VaultError::Model(format!("model load task failed: {e}")))?
}

/// Point the chat handler at the manifest's active model (or none),
/// loading it fresh from disk. An active model without a tokenizer
/// (installed before tokenizers were downloaded) leaves the chat
/// model-less — the rule classifier keeps working — rather than
/// failing the sync.
async fn sync_chat_model(state: &AppState) -> Result<(), VaultError> {
    match (
        state.model_manager.active_model_path(),
        state.model_manager.active_tokenizer_path(),
    ) {
        (Some(path), Some(tokenizer_path)) => {
            let active = state.model_manager.active_model();
            let active_id = active.as_ref().map(|active| active.model_id.clone());
            let prompt_style = active_id
                .as_deref()
                .and_then(stern_core::ai::model_manager::catalog_model)
                .map_or(
                    stern_core::ai::model_manager::PromptStyle::ChatMl,
                    |descriptor| descriptor.prompt_style,
                );
            let runtime_path = ensure_runtime_path(state, active_id.as_deref()).await?;
            let model = load_model_at(runtime_path, path, tokenizer_path, prompt_style).await?;
            state.chat_handler.set_model(Some(model));
        }
        _ => state.chat_handler.set_model(None),
    }
    Ok(())
}

#[tauri::command]
pub async fn model_status(state: State<'_, AppState>) -> Result<Vec<ModelStatusDto>, VaultError> {
    Ok(model_status_dtos(&state))
}

#[tauri::command]
pub async fn download_model(
    state: State<'_, AppState>,
    model_id: String,
) -> Result<Vec<ModelStatusDto>, VaultError> {
    let descriptor = catalog_model(&model_id)
        .copied()
        .ok_or_else(|| VaultError::Model(format!("unknown model: {model_id}")))?;

    // Reserve the progress slot *before* dropping the lock: checking and
    // reserving in the same critical section closes the TOCTOU window in
    // which two rapid calls could both pass the check and download to the
    // same `.part` file concurrently.
    {
        let mut guard = state
            .download_progress
            .lock()
            .map_err(|e| VaultError::Model(e.to_string()))?;
        if guard.is_some() {
            return Err(VaultError::Model(
                "a model download is already in progress".to_string(),
            ));
        }
        *guard = Some((model_id.clone(), 0, descriptor.size_bytes));
    }
    // RAII: the slot is cleared when this guard drops, on every exit path
    // below (success, error, cancellation, panic).
    let _progress_guard = DownloadProgressGuard {
        progress: state.download_progress.clone(),
    };

    let manager = state.model_manager.clone();
    let progress = state.download_progress.clone();
    let progress_id = model_id.clone();

    let result = tauri::async_runtime::spawn_blocking(move || {
        manager.download(&descriptor, &mut |downloaded, total| {
            if let Ok(mut guard) = progress.lock() {
                *guard = Some((progress_id.clone(), downloaded, total));
            }
        })
    })
    .await;

    match result {
        Ok(Ok(_installed)) => {}
        Ok(Err(e)) => return Err(e),
        Err(e) => {
            return Err(VaultError::Model(format!(
                "model download task failed: {e}"
            )));
        }
    }

    // Hot-load only when the downloaded model is the active one (first
    // install, or an update of the active model). Downloading a
    // non-active model must not reload the active model from disk.
    let is_active = state
        .model_manager
        .active_model()
        .is_some_and(|active| active.model_id == descriptor.id);
    if is_active {
        sync_chat_model(&state).await?;
    }
    Ok(model_status_dtos(&state))
}

#[tauri::command]
pub async fn select_model(
    state: State<'_, AppState>,
    model_id: String,
) -> Result<Vec<ModelStatusDto>, VaultError> {
    // Load *before* persisting the selection: if the load fails, the
    // manifest, the UI's active badge, and the chat handler all stay on
    // the previously active model instead of desyncing.
    let path = state
        .model_manager
        .installed_model_path(&model_id)
        .ok_or_else(|| {
            VaultError::Model(format!(
                "model {model_id} is not installed or its file is missing"
            ))
        })?;
    let tokenizer_path = state
        .model_manager
        .installed_tokenizer_path(&model_id)
        .ok_or_else(|| {
            VaultError::Model(format!(
                "model {model_id} has no tokenizer; download it again to repair the install"
            ))
        })?;
    let prompt_style = stern_core::ai::model_manager::catalog_model(&model_id).map_or(
        stern_core::ai::model_manager::PromptStyle::ChatMl,
        |descriptor| descriptor.prompt_style,
    );
    let runtime_path = ensure_runtime_path(&state, Some(&model_id)).await?;
    let model = load_model_at(runtime_path, path, tokenizer_path, prompt_style).await?;
    state.model_manager.set_active_model(&model_id)?;
    state.chat_handler.set_model(Some(model));
    Ok(model_status_dtos(&state))
}

#[tauri::command]
pub async fn remove_model(
    state: State<'_, AppState>,
    model_id: String,
) -> Result<Vec<ModelStatusDto>, VaultError> {
    let was_active = state
        .model_manager
        .active_model()
        .is_some_and(|active| active.model_id == model_id);
    state.model_manager.remove_model(&model_id)?;
    // Only the removal of the active model affects the chat handler
    // (it becomes model-less); removing any other model leaves the
    // loaded model untouched instead of reloading it from disk.
    if was_active {
        sync_chat_model(&state).await?;
    }
    Ok(model_status_dtos(&state))
}
