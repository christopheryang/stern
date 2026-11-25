// Security notes:
// - tauri.conf.json CSP uses 'unsafe-inline' for script-src because Leptos WASM hydration
//   requires inline scripts and cannot use nonces. connect-src allows the Tauri IPC bridge
//   and the dev server (localhost:1420).
// - withGlobalTauri is enabled because Leptos WASM builds cannot use @tauri-apps/api;
//   the frontend accesses window.__TAURI__ directly for IPC invocations.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![recursion_limit = "256"]

mod commands;
mod files;
mod notes;
mod organize;
mod trash;

use std::sync::Arc;
use stern_core::ai::model_manager::ModelManager;
use stern_core::ai::{AiModel, ChatHandler};
use stern_core::application::vault::session::VaultSession;
use stern_core::infrastructure::crypto::{Argon2idKdfProvider, XChaCha20CryptoProvider};
use stern_core::infrastructure::keychain::OsKeychainProvider;
use stern_core::infrastructure::sqlite::repository::SqliteRepository;
use tauri::Manager;

use std::sync::Mutex;
use std::time::Instant;

pub struct AppState {
    pub session: Mutex<VaultSession>,
    pub crypto: Arc<XChaCha20CryptoProvider>,
    pub kdf: Arc<Argon2idKdfProvider>,
    pub keychain: Arc<OsKeychainProvider>,
    pub chat_handler: Arc<ChatHandler>,
    pub db_path: std::path::PathBuf,
    pub repository: Arc<SqliteRepository>,
    pub unlock_lockout: Mutex<Option<(u32, Instant)>>,
    pub model_manager: Arc<ModelManager>,
    pub download_progress: Arc<Mutex<Option<(String, u64, u64)>>>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            let _ = app.get_webview_window("main").map(|w| w.set_focus());
        }))
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let app_dir = app
                .path()
                .app_data_dir()
                .map_err(|e| format!("failed to resolve app data dir: {e}"))?;
            std::fs::create_dir_all(&app_dir)
                .map_err(|e| format!("failed to create app data dir: {e}"))?;

            let db_path = app_dir.join("stern.db");
            let repository = Arc::new(
                SqliteRepository::new(&db_path)
                    .map_err(|e| format!("failed to open database: {e}"))?,
            );

            // The AI model — and the ONNX Runtime library itself — are
            // downloaded after install, never bundled; the app ships
            // with no AI at all, and the user picks a model first. The
            // chat handler starts model-less and the verified active
            // model (if one is on disk, with its runtime) is loaded on a
            // background thread: loading a 273–483 MB model
            // synchronously here would block window startup. No download
            // ever happens at startup: a device with a model but no
            // runtime record fetches the runtime when the user next
            // selects or downloads a model.
            let model_manager = Arc::new(ModelManager::new(app_dir.join("models")));
            let chat_handler = Arc::new(ChatHandler::new(None));
            // A model this build no longer offers is cleaned up before
            // the active model is loaded, so it can never be loaded.
            let _removed = model_manager.remove_retired_models();
            {
                let model_manager = model_manager.clone();
                let chat_handler = chat_handler.clone();
                std::thread::spawn(move || {
                    let active_style = model_manager
                        .active_model()
                        .and_then(|active| {
                            stern_core::ai::model_manager::catalog_model(&active.model_id)
                        })
                        .map_or(
                            stern_core::ai::model_manager::PromptStyle::ChatMl,
                            |descriptor| descriptor.prompt_style,
                        );
                    if let (Some(runtime_path), Some(path), Some(tokenizer_path)) = (
                        model_manager.runtime_path(),
                        model_manager.active_model_path(),
                        model_manager.active_tokenizer_path(),
                    ) {
                        match AiModel::new(&runtime_path, &path, &tokenizer_path, active_style) {
                            Ok(model) => chat_handler.set_model(Some(Arc::new(model))),
                            Err(e) => {
                                eprintln!("stern: failed to load AI model: {e}");
                            }
                        }
                    }
                });
            }

            app.manage(AppState {
                session: Mutex::new(VaultSession::new()),
                crypto: Arc::new(XChaCha20CryptoProvider::new()),
                kdf: Arc::new(Argon2idKdfProvider::new()),
                keychain: Arc::new(OsKeychainProvider::new()),
                chat_handler,
                db_path,
                repository,
                unlock_lockout: Mutex::new(None),
                model_manager,
                download_progress: Arc::new(Mutex::new(None)),
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            files::add_file_from_picker,
            files::save_file_to_picker,
            files::delete_stored_file,
            commands::vault_status,
            commands::create_vault,
            commands::unlock_vault,
            commands::lock_vault,
            commands::list_entries,
            notes::list_notes,
            notes::load_note,
            notes::save_note,
            commands::get_entry,
            commands::create_entry,
            commands::update_entry,
            commands::delete_entry,
            commands::search_entries,
            commands::copy_to_clipboard,
            commands::send_chat_message,
            commands::model_status,
            commands::download_model,
            commands::select_model,
            commands::remove_model,
            commands::export_vault,
            commands::import_vault,
        ])
        .run(tauri::generate_context!())
        .unwrap_or_else(|e| {
            eprintln!("error while running tauri application: {e}");
            std::process::exit(1);
        });
}
