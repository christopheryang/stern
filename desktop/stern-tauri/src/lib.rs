#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![recursion_limit = "256"]

mod commands;

use std::sync::Arc;
use stern_core::ai::ChatHandler;
use stern_core::application::vault::session::VaultSession;
use stern_core::infrastructure::crypto::{Argon2idKdfProvider, XChaCha20CryptoProvider};
use stern_core::infrastructure::keychain::OsKeychainProvider;
use stern_core::infrastructure::sqlite::repository::SqliteRepository;
use tauri::Manager;

pub struct AppState {
    pub session: std::sync::Mutex<VaultSession>,
    pub crypto: Arc<XChaCha20CryptoProvider>,
    pub kdf: Arc<Argon2idKdfProvider>,
    pub keychain: Arc<OsKeychainProvider>,
    pub chat_handler: Arc<ChatHandler>,
    pub db_path: std::path::PathBuf,
    pub repository: Arc<SqliteRepository>,
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
                .expect("failed to resolve app data dir");
            std::fs::create_dir_all(&app_dir).expect("failed to create app data dir");

            let db_path = app_dir.join("stern.db");
            let repository = Arc::new(
                SqliteRepository::new(&db_path).expect("failed to open database"),
            );

            let chat_handler = Arc::new(ChatHandler::new(None));

            app.manage(AppState {
                session: std::sync::Mutex::new(VaultSession::new()),
                crypto: Arc::new(XChaCha20CryptoProvider::new()),
                kdf: Arc::new(Argon2idKdfProvider::new()),
                keychain: Arc::new(OsKeychainProvider::new()),
                chat_handler,
                db_path,
                repository,
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::vault_status,
            commands::create_vault,
            commands::unlock_vault,
            commands::lock_vault,
            commands::list_entries,
            commands::get_entry,
            commands::create_entry,
            commands::update_entry,
            commands::delete_entry,
            commands::search_entries,
            commands::copy_to_clipboard,
            commands::send_chat_message,
            commands::export_vault,
            commands::import_vault,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
