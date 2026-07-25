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

    state.repository.save_vault_meta(
        &salt,
        &verify,
        &secret_key,
        &params,
    )?;

    let mut session = state
        .session
        .lock()
        .map_err(|e| VaultError::Database(e.to_string()))?;
    let kek_bytes = *kek;
    session.unlock(kek_bytes, verify);

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
    let meta = state.repository.load_vault_meta()?
        .ok_or_else(|| VaultError::Database("no vault found — create one first".to_string()))?;

    let params = KdfParams::argon2id_default();
    let preprocessed = state
        .kdf
        .preprocess_2skd(request.password.as_bytes(), &meta.secret_key)?;
    let master_key = state.kdf.derive_master_key(&preprocessed, &meta.salt, &params)?;
    let kek = state.kdf.derive_kek(&master_key)?;
    let verify = state.kdf.derive_verify_hash(&master_key)?;

    if !state.crypto.verify_hash_matches(&verify, &meta.verify_hash) {
        return Err(VaultError::InvalidMasterPassword);
    }

    let mut session = state
        .session
        .lock()
        .map_err(|e| VaultError::Database(e.to_string()))?;
    let kek_bytes = *kek;
    session.unlock(kek_bytes, verify);

    let entry_count = state.repository.count_entries()?;
    Ok(VaultStatus {
        is_initialized: true,
        is_unlocked: true,
        entry_count,
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

    match response.action {
        Some(stern_core::ai::chat::Action::StoreEntry { ref name, ref kind, ref fields }) => {
            let session = state
                .session
                .lock()
                .map_err(|e| VaultError::Database(e.to_string()))?;

            if !session.is_unlocked {
                return Ok(ChatResponse {
                    message: "Vault is locked. Please unlock it first.".to_string(),
                    action: Some("locked".to_string()),
                });
            }

            let kek = session.kek()?.clone();
            drop(session);

            let entry_kind = match kind.as_str() {
                "note" | "document" => stern_core::domain::vault::entry::EntryKind::Note,
                _ => stern_core::domain::vault::entry::EntryKind::Login,
            };

            let entry_fields: Vec<stern_core::domain::vault::entry::Field> = fields
                .iter()
                .map(|(k, v)| stern_core::domain::vault::entry::Field {
                    key: k.clone(),
                    value: v.clone(),
                    hidden: true,
                })
                .collect();

            let entry_id = uuid::Uuid::new_v4().to_string();
            let now = chrono::Utc::now();
            let version = 1u64;

            let payload = stern_core::domain::vault::entry::EntryPayload {
                name: name.clone(),
                kind: entry_kind.clone(),
                tags: vec![],
                fields: entry_fields,
            };

            let payload_json = serde_json::to_vec(&payload)
                .map_err(|e| VaultError::Serialization(e.to_string()))?;

            let aad = stern_core::domain::vault::aad::entry_aad(&entry_id, version);
            let dek = state.crypto.generate_dek();
            let (nonce, ciphertext) =
                state.crypto.encrypt_entry(&dek, &payload_json, &aad)?;
            let dek_wrapped = state.crypto.wrap_dek(&dek, &kek)?.to_vec();

            let entry = stern_core::domain::vault::entry::EncryptedEntry {
                id: entry_id,
                kind: entry_kind,
                name: name.clone(),
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
                message: response.message,
                action: Some("store".to_string()),
            })
        }
        Some(stern_core::ai::chat::Action::ExportVault { ref password }) => {
            match password {
                Some(pw) => {
                    let export_result = export_vault(
                        state,
                        ExportRequest { password: pw.clone() },
                    )
                    .await?;
                    Ok(ChatResponse {
                        message: format!(
                            "Exported {} entries to {}",
                            export_result.entry_count, export_result.path
                        ),
                        action: Some("export".to_string()),
                    })
                }
                None => Ok(ChatResponse {
                    message: response.message,
                    action: Some("export".to_string()),
                }),
            }
        }
        Some(stern_core::ai::chat::Action::ImportVault { ref path, ref password }) => {
            match (path, password) {
                (Some(p), Some(pw)) => {
                    import_vault(
                        state,
                        ImportRequest { password: pw.clone(), path: p.clone() },
                    )
                    .await
                }
                _ => Ok(ChatResponse {
                    message: response.message,
                    action: Some("import".to_string()),
                }),
            }
        }
        Some(stern_core::ai::chat::Action::CreateVault { ref password }) => {
            match password {
                Some(pw) => {
                    let result = create_vault(
                        state,
                        CreateVaultRequest { password: pw.clone() },
                    )
                    .await?;
                    Ok(ChatResponse {
                        message: format!("Vault created and unlocked. {} secrets stored.", result.entry_count),
                        action: Some("create_vault".to_string()),
                    })
                }
                None => Ok(ChatResponse {
                    message: response.message,
                    action: Some("create_vault".to_string()),
                }),
            }
        }
        Some(stern_core::ai::chat::Action::UnlockVault { ref password }) => {
            match password {
                Some(pw) => {
                    let result = unlock_vault(
                        state,
                        UnlockRequest { password: pw.clone(), secret_key: String::new() },
                    )
                    .await?;
                    Ok(ChatResponse {
                        message: format!("Vault unlocked. {} secrets available.", result.entry_count),
                        action: Some("unlock_vault".to_string()),
                    })
                }
                None => Ok(ChatResponse {
                    message: response.message,
                    action: Some("unlock_vault".to_string()),
                }),
            }
        }
        _ => {
            let action_str = response.action.map(|a| {
                match a {
                    stern_core::ai::chat::Action::StoreEntry { .. } => "store".to_string(),
                    stern_core::ai::chat::Action::SearchEntry { .. } => "search".to_string(),
                    stern_core::ai::chat::Action::ListEntries { .. } => "list".to_string(),
                    stern_core::ai::chat::Action::ConfirmDelete { .. } => "delete".to_string(),
                    stern_core::ai::chat::Action::UpdateEntry { .. } => "update".to_string(),
                    stern_core::ai::chat::Action::ExportVault { .. } => "export".to_string(),
                    stern_core::ai::chat::Action::ImportVault { .. } => "import".to_string(),
                    stern_core::ai::chat::Action::CreateVault { .. } => "create_vault".to_string(),
                    stern_core::ai::chat::Action::UnlockVault { .. } => "unlock_vault".to_string(),
                }
            });

            Ok(ChatResponse {
                message: response.message,
                action: action_str,
            })
        }
    }
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
    let kek = session.kek()?.clone();
    drop(session);

    let encrypted_entries = state.repository.list_all_entries()?;

    let mut plaintext_entries: Vec<stern_core::domain::vault::entry::EntryPayload> = Vec::new();
    for enc in &encrypted_entries {
        let aad = stern_core::domain::vault::aad::entry_aad(&enc.id, enc.version);
        let wrapped = <[u8; stern_core::domain::vault::crypto_constants::DEK_WRAPPED_LEN]>::try_from(
            enc.dek_wrapped.as_slice(),
        )
        .map_err(|_| VaultError::DecryptionFailed)?;
        let dek = state.crypto.unwrap_dek(&wrapped, &kek)?;
        let plaintext = state.crypto.decrypt_entry(&dek, &enc.nonce, &enc.ciphertext, &aad)?;
        let payload: stern_core::domain::vault::entry::EntryPayload =
            serde_json::from_slice(&plaintext)
                .map_err(|e| VaultError::Serialization(e.to_string()))?;
        plaintext_entries.push(payload);
    }

    let export_data = serde_json::to_vec_pretty(&plaintext_entries)
        .map_err(|e| VaultError::Serialization(e.to_string()))?;

    let salt = state.crypto.generate_vault_salt();
    let params = stern_core::domain::vault::kdf_params::KdfParams::argon2id_default();
    let password_hash = {
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(request.password.as_bytes());
        let result = hasher.finalize();
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&result);
        arr
    };
    let master_key = state.kdf.derive_master_key(&password_hash, &salt, &params)?;
    let export_kek = state.kdf.derive_kek(&master_key)?;

    let dek = state.crypto.generate_dek();
    let aad = b"stern-export-v1";
    let (nonce, ciphertext) = state.crypto.encrypt_entry(&dek, &export_data, aad)?;
    let dek_wrapped = state.crypto.wrap_dek(&dek, &export_kek)?;

    let export_file = stern_core::domain::vault::entry::EncryptedEntry {
        id: "export".to_string(),
        kind: stern_core::domain::vault::entry::EntryKind::Note,
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

    let export_json = serde_json::to_vec_pretty(&(&salt, &export_file))
        .map_err(|e| VaultError::Serialization(e.to_string()))?;

    std::fs::write(&export_path, &export_json)
        .map_err(|e| VaultError::Io(e.to_string()))?;

    Ok(ExportResponse {
        path: export_path.to_string_lossy().to_string(),
        entry_count: plaintext_entries.len(),
    })
}

#[tauri::command]
pub async fn import_vault(
    state: State<'_, AppState>,
    request: ImportRequest,
) -> Result<ChatResponse, VaultError> {
    let session = state
        .session
        .lock()
        .map_err(|e| VaultError::Database(e.to_string()))?;
    if !session.is_unlocked {
        return Err(VaultError::VaultLocked);
    }
    let kek = session.kek()?.clone();
    drop(session);

    let data = std::fs::read(&request.path)
        .map_err(|e| VaultError::Io(e.to_string()))?;

    let (salt, export_file): (
        [u8; stern_core::domain::vault::crypto_constants::VAULT_SALT_LEN],
        stern_core::domain::vault::entry::EncryptedEntry,
    ) = serde_json::from_slice(&data)
        .map_err(|e| VaultError::Serialization(e.to_string()))?;

    let params = stern_core::domain::vault::kdf_params::KdfParams::argon2id_default();
    let password_hash = {
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(request.password.as_bytes());
        let result = hasher.finalize();
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&result);
        arr
    };
    let master_key = state.kdf.derive_master_key(&password_hash, &salt, &params)?;
    let export_kek = state.kdf.derive_kek(&master_key)?;

    let wrapped = <[u8; stern_core::domain::vault::crypto_constants::DEK_WRAPPED_LEN]>::try_from(
        export_file.dek_wrapped.as_slice(),
    )
    .map_err(|_| VaultError::DecryptionFailed)?;
    let dek = state.crypto.unwrap_dek(&wrapped, &export_kek)?;
    let aad = b"stern-export-v1";
    let plaintext = state.crypto.decrypt_entry(&dek, &export_file.nonce, &export_file.ciphertext, aad)?;

    let entries: Vec<stern_core::domain::vault::entry::EntryPayload> =
        serde_json::from_slice(&plaintext)
            .map_err(|e| VaultError::Serialization(e.to_string()))?;

    let mut imported = 0;
    for payload in entries {
        let entry_id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now();
        let version = 1u64;
        let entry_aad = stern_core::domain::vault::aad::entry_aad(&entry_id, version);

        let payload_json = serde_json::to_vec(&payload)
            .map_err(|e| VaultError::Serialization(e.to_string()))?;

        let entry_dek = state.crypto.generate_dek();
        let (nonce, ciphertext) =
            state.crypto.encrypt_entry(&entry_dek, &payload_json, &entry_aad)?;
        let dek_wrapped = state.crypto.wrap_dek(&entry_dek, &kek)?.to_vec();

        let entry = stern_core::domain::vault::entry::EncryptedEntry {
            id: entry_id,
            kind: payload.kind,
            name: payload.name,
            tags: payload.tags,
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

    Ok(ChatResponse {
        message: format!("Imported {imported} entries from the export file."),
        action: Some("import".to_string()),
    })
}
