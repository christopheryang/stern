use crate::AppState;
use stern_core::application::vault::ports::crypto::CryptoProvider;
use stern_core::application::vault::ports::kdf::KeyDerivationProvider;
use stern_core::domain::vault::errors::VaultError;
use stern_core::domain::vault::kdf_params::KdfParams;
use stern_ipc::dto::*;
use tauri::State;

#[tauri::command]
pub async fn vault_status(state: State<'_, AppState>) -> Result<VaultStatus, VaultError> {
    let session = state
        .session
        .lock()
        .map_err(|e| VaultError::Database(e.to_string()))?;
    Ok(VaultStatus {
        is_initialized: state.db_path.exists(),
        is_unlocked: session.is_unlocked,
        entry_count: 0,
    })
}

#[tauri::command]
pub async fn create_vault(
    state: State<'_, AppState>,
    request: CreateVaultRequest,
) -> Result<VaultStatus, VaultError> {
    let params = KdfParams::argon2id_default();
    let salt = state.crypto.generate_vault_salt();
    let secret_key = state.crypto.generate_secret_key();

    let preprocessed = state
        .kdf
        .preprocess_2skd(request.password.as_bytes(), &secret_key)?;
    let master_key = state.kdf.derive_master_key(&preprocessed, &salt, &params)?;
    let kek = state.kdf.derive_kek(&master_key)?;
    let verify = state.kdf.derive_verify_hash(&master_key)?;

    let mut session = state
        .session
        .lock()
        .map_err(|e| VaultError::Database(e.to_string()))?;
    let kek_bytes = *kek;
    session.unlock(kek_bytes, verify);

    Ok(VaultStatus {
        is_initialized: true,
        is_unlocked: true,
        entry_count: 0,
    })
}

#[tauri::command]
pub async fn unlock_vault(
    state: State<'_, AppState>,
    request: UnlockRequest,
) -> Result<VaultStatus, VaultError> {
    let _ = state;
    let _ = request;
    Ok(VaultStatus {
        is_initialized: true,
        is_unlocked: true,
        entry_count: 0,
    })
}

#[tauri::command]
pub async fn lock_vault(state: State<'_, AppState>) -> Result<VaultStatus, VaultError> {
    let mut session = state
        .session
        .lock()
        .map_err(|e| VaultError::Database(e.to_string()))?;
    session.lock();
    Ok(VaultStatus {
        is_initialized: state.db_path.exists(),
        is_unlocked: false,
        entry_count: 0,
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
    provider.set_text(&text).map_err(|e| VaultError::Io(e))
}

#[tauri::command]
pub async fn send_chat_message(
    state: State<'_, AppState>,
    request: SendChatRequest,
) -> Result<ChatResponse, VaultError> {
    let response = state.chat_handler.process_message(&request.content);

    let action_str = response.action.map(|a| {
        match a {
            stern_core::ai::chat::Action::StoreEntry { .. } => "store".to_string(),
            stern_core::ai::chat::Action::SearchEntry { .. } => "search".to_string(),
            stern_core::ai::chat::Action::ListEntries { .. } => "list".to_string(),
            stern_core::ai::chat::Action::ConfirmDelete { .. } => "delete".to_string(),
            stern_core::ai::chat::Action::UpdateEntry { .. } => "update".to_string(),
        }
    });

    Ok(ChatResponse {
        message: response.message,
        action: action_str,
    })
}
