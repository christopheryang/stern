use crate::domain::vault::crypto_constants::{SECRET_KEY_LEN, VAULT_SALT_LEN, VERIFY_HASH_LEN};
use crate::domain::vault::entry::EncryptedEntry;
use crate::domain::vault::errors::VaultError;
use crate::domain::vault::kdf_params::KdfParams;
use crate::domain::vault::vault_meta::VaultMeta;
use rusqlite::Connection;
use std::path::Path;
use std::sync::Mutex;

pub struct SqliteRepository {
    conn: Mutex<Connection>,
}

impl SqliteRepository {
    pub fn new(path: &Path) -> Result<Self, VaultError> {
        let conn = Connection::open(path).map_err(|e| VaultError::Database(e.to_string()))?;

        conn.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA foreign_keys = ON;

            CREATE TABLE IF NOT EXISTS vault_meta (
                id TEXT PRIMARY KEY,
                vault_salt BLOB NOT NULL,
                verify_hash BLOB NOT NULL,
                secret_key BLOB NOT NULL,
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

        // Migration: rename legacy 'encrypted_index' column to 'secret_key' if it exists.
        let has_old_col: bool = conn
            .prepare("PRAGMA table_info(vault_meta)")
            .and_then(|mut stmt| {
                let rows = stmt.query_map([], |row| {
                    let name: String = row.get(1)?;
                    Ok(name)
                })?;
                for row in rows {
                    if row? == "encrypted_index" {
                        return Ok(true);
                    }
                }
                Ok(false)
            })
            .unwrap_or(false);
        if has_old_col {
            conn.execute_batch(
                "ALTER TABLE vault_meta RENAME COLUMN encrypted_index TO secret_key",
            )
            .map_err(|e| VaultError::Database(e.to_string()))?;
        }

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

    pub fn insert_entry(&self, entry: &EncryptedEntry) -> Result<(), VaultError> {
        self.execute(|conn| {
            let kind_str = match entry.kind {
                crate::domain::vault::entry::EntryKind::Login => "login",
                crate::domain::vault::entry::EntryKind::Note => "note",
                crate::domain::vault::entry::EntryKind::Document => "document",
            };
            let tags_json = serde_json::to_string(&entry.tags).unwrap_or_default();
            conn.execute(
                "INSERT INTO entries (id, kind, name, tags, dek_wrapped, nonce, ciphertext, version, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                rusqlite::params![
                    entry.id,
                    kind_str,
                    entry.name,
                    tags_json,
                    entry.dek_wrapped,
                    entry.nonce.as_slice(),
                    entry.ciphertext,
                    entry.version,
                    entry.created_at.to_rfc3339(),
                    entry.updated_at.to_rfc3339(),
                ],
            )?;
            Ok(())
        })
    }

    #[allow(clippy::indexing_slicing)]
    pub fn list_all_entries(&self) -> Result<Vec<EncryptedEntry>, VaultError> {
        self.execute(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, kind, name, tags, dek_wrapped, nonce, ciphertext, version, created_at, updated_at
                 FROM entries ORDER BY name",
            )?;

            let rows = stmt.query_map([], |row| {
                let id: String = row.get(0)?;
                let kind_str: String = row.get(1)?;
                let name: String = row.get(2)?;
                let tags_json: String = row.get(3)?;
                let dek_wrapped: Vec<u8> = row.get(4)?;
                let nonce: Vec<u8> = row.get(5)?;
                let ciphertext: Vec<u8> = row.get(6)?;
                let version: u64 = row.get(7)?;
                let created_at: String = row.get(8)?;
                let updated_at: String = row.get(9)?;

                let kind = match kind_str.as_str() {
                    "note" => crate::domain::vault::entry::EntryKind::Note,
                    "document" => crate::domain::vault::entry::EntryKind::Document,
                    _ => crate::domain::vault::entry::EntryKind::Login,
                };
                let tags: Vec<String> =
                    serde_json::from_str(&tags_json).unwrap_or_default();
                let mut nonce_arr = [0u8; crate::domain::vault::crypto_constants::NONCE_LEN];
                let len = nonce.len().min(nonce_arr.len());
                nonce_arr[..len].copy_from_slice(&nonce[..len]);

                Ok(EncryptedEntry {
                    id,
                    kind,
                    name,
                    tags,
                    dek_wrapped,
                    nonce: nonce_arr,
                    ciphertext,
                    version,
                    created_at: chrono::DateTime::parse_from_rfc3339(&created_at)
                        .map_or_else(|_| chrono::Utc::now(), |dt| dt.with_timezone(&chrono::Utc)),
                    updated_at: chrono::DateTime::parse_from_rfc3339(&updated_at)
                        .map_or_else(|_| chrono::Utc::now(), |dt| dt.with_timezone(&chrono::Utc)),
                })
            })?;

            let mut entries = Vec::new();
            for row in rows {
                entries.push(row?);
            }
            Ok(entries)
        })
    }

    pub fn count_entries(&self) -> Result<usize, VaultError> {
        self.execute(|conn| {
            let count: i64 =
                conn.query_row("SELECT COUNT(*) FROM entries", [], |row| row.get(0))?;
            Ok(usize::try_from(count).unwrap_or(usize::MAX))
        })
    }

    pub fn vault_exists(&self) -> Result<bool, VaultError> {
        self.execute(|conn| {
            let count: i64 =
                conn.query_row("SELECT COUNT(*) FROM vault_meta", [], |row| row.get(0))?;
            Ok(count > 0)
        })
    }

    pub fn save_vault_meta(
        &self,
        salt: &[u8; VAULT_SALT_LEN],
        verify_hash: &[u8; VERIFY_HASH_LEN],
        secret_key: &[u8; SECRET_KEY_LEN],
        params: &KdfParams,
    ) -> Result<(), VaultError> {
        self.execute(|conn| {
            let params_json = serde_json::to_string(params)
                .map_err(|e| rusqlite::Error::InvalidParameterName(e.to_string()))?;
            conn.execute(
                "INSERT OR REPLACE INTO vault_meta (id, vault_salt, verify_hash, secret_key, kdf_params, created_at)
                 VALUES ('default', ?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![
                    &salt[..],
                    &verify_hash[..],
                    &secret_key[..],
                    params_json,
                    chrono::Utc::now().to_rfc3339(),
                ],
            )?;
            Ok(())
        })
    }

    #[allow(clippy::indexing_slicing)]
    pub fn load_vault_meta(&self) -> Result<Option<VaultMeta>, VaultError> {
        self.execute(|conn| {
            let result = conn.query_row(
                "SELECT vault_salt, verify_hash, secret_key, kdf_params FROM vault_meta WHERE id = 'default'",
                [],
                |row| {
                    let salt: Vec<u8> = row.get(0)?;
                    let verify_hash: Vec<u8> = row.get(1)?;
                    let secret_key: Vec<u8> = row.get(2)?;
                    let params_json: String = row.get(3)?;
                    Ok((salt, verify_hash, secret_key, params_json))
                },
            );

            match result {
                Ok((salt, verify_hash, secret_key, params_json)) => {
                    let mut salt_arr = [0u8; VAULT_SALT_LEN];
                    let len = salt.len().min(salt_arr.len());
                    salt_arr[..len].copy_from_slice(&salt[..len]);

                    let mut verify_arr = [0u8; VERIFY_HASH_LEN];
                    let len = verify_hash.len().min(verify_arr.len());
                    verify_arr[..len].copy_from_slice(&verify_hash[..len]);

                    let mut sk_arr = [0u8; SECRET_KEY_LEN];
                    let len = secret_key.len().min(sk_arr.len());
                    sk_arr[..len].copy_from_slice(&secret_key[..len]);

                    let params: KdfParams = serde_json::from_str(&params_json)
                        .map_err(|e| rusqlite::Error::InvalidParameterName(e.to_string()))?;

                    Ok(Some(VaultMeta {
                        salt: salt_arr,
                        verify_hash: verify_arr,
                        secret_key: sk_arr,
                        params,
                    }))
                }
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(e),
            }
        })
    }
}
