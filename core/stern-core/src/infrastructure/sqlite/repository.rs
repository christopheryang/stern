use crate::domain::vault::errors::VaultError;
use rusqlite::Connection;
use std::path::Path;
use std::sync::Mutex;

pub struct SqliteRepository {
    conn: Mutex<Connection>,
}

impl SqliteRepository {
    pub fn new(path: &Path) -> Result<Self, VaultError> {
        let conn =
            Connection::open(path).map_err(|e| VaultError::Database(e.to_string()))?;

        conn.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA foreign_keys = ON;

            CREATE TABLE IF NOT EXISTS vault_meta (
                id TEXT PRIMARY KEY,
                vault_salt BLOB NOT NULL,
                verify_hash BLOB NOT NULL,
                encrypted_index BLOB NOT NULL,
                kdf_params TEXT NOT NULL,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS entries (
                id TEXT PRIMARY KEY,
                kind TEXT NOT NULL,
                name TEXT NOT NULL,
                tags TEXT,
                dek_wrapped BLOB NOT NULL,
                nonce BLOB NOT NULL,
                ciphertext BLOB NOT NULL,
                version INTEGER NOT NULL DEFAULT 1,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            ",
        )
        .map_err(|e| VaultError::Database(e.to_string()))?;

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn execute<F, R>(&self, f: F) -> Result<R, VaultError>
    where
        F: FnOnce(&Connection) -> Result<R, rusqlite::Error>,
    {
        let conn = self
            .conn
            .lock()
            .map_err(|e| VaultError::Database(e.to_string()))?;
        f(&conn).map_err(|e| VaultError::Database(e.to_string()))
    }
}
