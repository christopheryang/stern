use crate::domain::vault::crypto_constants::NONCE_LEN;
use crate::domain::vault::crypto_constants::{SECRET_KEY_LEN, VAULT_SALT_LEN, VERIFY_HASH_LEN};
use crate::domain::vault::entry::EncryptedEntry;
use crate::domain::vault::errors::VaultError;
use crate::domain::vault::folder::EncryptedFolder;
use crate::domain::vault::kdf_params::KdfParams;
use crate::domain::vault::vault_meta::VaultMeta;
use rusqlite::Connection;
use std::path::Path;
use std::sync::Mutex;

pub struct SqliteRepository {
    conn: Mutex<Connection>,
}

/// The encrypted fields of an entry that an update replaces.
pub struct StoredPayload<'a> {
    pub dek_wrapped: &'a [u8],
    pub nonce: &'a [u8; crate::domain::vault::crypto_constants::NONCE_LEN],
    pub ciphertext: &'a [u8],
    pub name: &'a str,
    pub version: u64,
    pub updated_at: &'a str,
}

/// The encrypted bytes of a stored file, kept beside its entry.
#[derive(Debug, Clone)]
pub struct EncryptedBlob {
    pub dek_wrapped: Vec<u8>,
    pub nonce: [u8; NONCE_LEN],
    pub ciphertext: Vec<u8>,
    pub byte_len: u64,
    pub mime_type: String,
    pub is_text: bool,
}

/// Insert one entry row. Shared by `insert_entry` and the file
/// transaction, so both write the same shape.
fn insert_entry_row(conn: &Connection, entry: &EncryptedEntry) -> Result<(), VaultError> {
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
            entry.version.cast_signed(),
            entry.created_at.to_rfc3339(),
            entry.updated_at.to_rfc3339(),
        ],
    )
    .map_err(|e| VaultError::Database(e.to_string()))?;
    Ok(())
}

/// Add `column` to `table` unless it is already there.
///
/// Names and the definition are this file's own constants, never user text.
fn add_column_if_missing(
    conn: &Connection,
    table: &'static str,
    column: &'static str,
    definition: &'static str,
) -> Result<(), VaultError> {
    let present = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .and_then(|mut stmt| {
            let names = stmt.query_map([], |row| row.get::<_, String>(1))?;
            for name in names {
                if name? == column {
                    return Ok(true);
                }
            }
            Ok(false)
        })
        .map_err(|e| VaultError::Database(e.to_string()))?;
    if !present {
        conn.execute_batch(&format!(
            "ALTER TABLE {table} ADD COLUMN {column} {definition}"
        ))
        .map_err(|e| VaultError::Database(e.to_string()))?;
    }
    Ok(())
}

fn into_rusqlite(e: VaultError) -> rusqlite::Error {
    rusqlite::Error::ToSqlConversionFailure(Box::new(e))
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

            CREATE TABLE IF NOT EXISTS blobs (
                entry_id TEXT PRIMARY KEY REFERENCES entries(id) ON DELETE CASCADE,
                dek_wrapped BLOB NOT NULL,
                nonce BLOB NOT NULL,
                ciphertext BLOB NOT NULL,
                byte_len INTEGER NOT NULL,
                mime_type TEXT NOT NULL,
                is_text INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS folders (
                id TEXT PRIMARY KEY,
                parent_id TEXT REFERENCES folders(id) ON DELETE SET NULL,
                dek_wrapped BLOB NOT NULL,
                nonce BLOB NOT NULL,
                ciphertext BLOB NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            ",
        )
        .map_err(|e| VaultError::Database(e.to_string()))?;

        // Migrations: columns added after the first release. Each is added
        // only when missing, so opening a vault twice is harmless, and every
        // existing entry keeps its place: outside any folder, not trashed.
        add_column_if_missing(
            &conn,
            "entries",
            "folder_id",
            "TEXT REFERENCES folders(id) ON DELETE SET NULL",
        )?;
        add_column_if_missing(&conn, "entries", "trashed_at", "TEXT")?;

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

    /// Store an encrypted file: its entry and its blob, in one
    /// transaction, so a file is either fully stored or not stored.
    pub fn insert_file(
        &self,
        entry: &EncryptedEntry,
        blob: &EncryptedBlob,
    ) -> Result<(), VaultError> {
        let mut guard = self
            .conn
            .lock()
            .map_err(|e| VaultError::Database(e.to_string()))?;
        let tx = guard
            .transaction()
            .map_err(|e| VaultError::Database(e.to_string()))?;
        insert_entry_row(&tx, entry)?;
        tx.execute(
            "INSERT INTO blobs (entry_id, dek_wrapped, nonce, ciphertext, byte_len, mime_type, is_text)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                entry.id,
                blob.dek_wrapped,
                blob.nonce.as_slice(),
                blob.ciphertext,
                i64::try_from(blob.byte_len).unwrap_or(i64::MAX),
                blob.mime_type,
                i64::from(blob.is_text),
            ],
        )
        .map_err(|e| VaultError::Database(e.to_string()))?;
        tx.commit()
            .map_err(|e| VaultError::Database(e.to_string()))?;
        drop(guard);
        Ok(())
    }

    /// Remove an entry. Its blob, if it has one, goes with it (the
    /// `blobs` table cascades on delete).
    pub fn delete_entry(&self, entry_id: &str) -> Result<bool, VaultError> {
        self.execute(|conn| {
            let removed = conn.execute(
                "DELETE FROM entries WHERE id = ?1",
                rusqlite::params![entry_id],
            )?;
            Ok(removed > 0)
        })
    }

    /// Replace an entry's encrypted payload, name and version, but only if
    /// the stored version is still `expected_version`. The new version is
    /// `payload.version`, which must follow it. Returns false when no entry
    /// has that id and version: it was deleted, or another save got there
    /// first, so the caller's copy is out of date and nothing is written.
    pub fn update_entry_payload(
        &self,
        entry_id: &str,
        expected_version: u64,
        payload: &StoredPayload<'_>,
    ) -> Result<bool, VaultError> {
        let new_version = i64::try_from(payload.version)
            .map_err(|_| VaultError::Database("entry version out of range".to_string()))?;
        let old_version = i64::try_from(expected_version)
            .map_err(|_| VaultError::Database("entry version out of range".to_string()))?;
        self.execute(|conn| {
            let changed = conn.execute(
                "UPDATE entries
                 SET dek_wrapped = ?2, nonce = ?3, ciphertext = ?4, name = ?5,
                     version = ?6, updated_at = ?7
                 WHERE id = ?1 AND version = ?8",
                rusqlite::params![
                    entry_id,
                    payload.dek_wrapped,
                    payload.nonce.as_slice(),
                    payload.ciphertext,
                    payload.name,
                    new_version,
                    payload.updated_at,
                    old_version,
                ],
            )?;
            Ok(changed > 0)
        })
    }

    /// Store a new folder.
    pub fn insert_folder(&self, folder: &EncryptedFolder) -> Result<(), VaultError> {
        self.execute(|conn| {
            conn.execute(
                "INSERT INTO folders (id, parent_id, dek_wrapped, nonce, ciphertext, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                rusqlite::params![
                    folder.id,
                    folder.parent_id,
                    folder.dek_wrapped,
                    folder.nonce.as_slice(),
                    folder.ciphertext,
                    folder.created_at.to_rfc3339(),
                    folder.updated_at.to_rfc3339(),
                ],
            )?;
            Ok(())
        })
    }

    /// Every folder, oldest first.
    pub fn list_folders(&self) -> Result<Vec<EncryptedFolder>, VaultError> {
        self.execute(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, parent_id, dek_wrapped, nonce, ciphertext, created_at, updated_at
                 FROM folders ORDER BY created_at, id",
            )?;
            let rows = stmt.query_map([], |row| {
                let nonce: Vec<u8> = row.get(3)?;
                let nonce: [u8; NONCE_LEN] = nonce
                    .as_slice()
                    .try_into()
                    .map_err(|_| rusqlite::Error::InvalidQuery)?;
                let created_at: String = row.get(5)?;
                let updated_at: String = row.get(6)?;
                let parse = |text: &str| {
                    chrono::DateTime::parse_from_rfc3339(text)
                        .map_or_else(|_| chrono::Utc::now(), |dt| dt.with_timezone(&chrono::Utc))
                };
                Ok(EncryptedFolder {
                    id: row.get(0)?,
                    parent_id: row.get(1)?,
                    dek_wrapped: row.get(2)?,
                    nonce,
                    ciphertext: row.get(4)?,
                    created_at: parse(&created_at),
                    updated_at: parse(&updated_at),
                })
            })?;
            rows.collect()
        })
    }

    /// Replace a folder's encrypted name. False when no such folder.
    pub fn update_folder_name(
        &self,
        folder_id: &str,
        dek_wrapped: &[u8],
        nonce: &[u8; NONCE_LEN],
        ciphertext: &[u8],
        updated_at: &str,
    ) -> Result<bool, VaultError> {
        self.execute(|conn| {
            let changed = conn.execute(
                "UPDATE folders SET dek_wrapped = ?2, nonce = ?3, ciphertext = ?4, updated_at = ?5
                 WHERE id = ?1",
                rusqlite::params![
                    folder_id,
                    dek_wrapped,
                    nonce.as_slice(),
                    ciphertext,
                    updated_at
                ],
            )?;
            Ok(changed > 0)
        })
    }

    /// Move a folder under another (or to the top level with `None`).
    /// Refused, with `Ok(false)`, when either folder is missing or the move
    /// would put a folder inside itself, so the tree can never loop.
    pub fn move_folder(
        &self,
        folder_id: &str,
        new_parent: Option<&str>,
    ) -> Result<bool, VaultError> {
        self.execute(|conn| {
            if let Some(parent) = new_parent {
                // UNION (not UNION ALL) stops the walk on any existing loop.
                let inside: i64 = conn.query_row(
                    "WITH RECURSIVE sub(id) AS (
                         SELECT ?1
                         UNION
                         SELECT f.id FROM folders f JOIN sub ON f.parent_id = sub.id
                     )
                     SELECT COUNT(*) FROM sub WHERE id = ?2",
                    rusqlite::params![folder_id, parent],
                    |row| row.get(0),
                )?;
                if inside > 0 {
                    return Ok(false);
                }
            }
            let changed = conn.execute(
                "UPDATE folders SET parent_id = ?2 WHERE id = ?1",
                rusqlite::params![folder_id, new_parent],
            )?;
            Ok(changed > 0)
        })
    }

    /// Delete a folder. What was inside it is kept: its sub-folders and
    /// entries move up to the folder's own parent. One transaction, so a
    /// failure leaves everything where it was.
    pub fn delete_folder(&self, folder_id: &str) -> Result<bool, VaultError> {
        let mut guard = self
            .conn
            .lock()
            .map_err(|e| VaultError::Database(e.to_string()))?;
        let tx = guard
            .transaction()
            .map_err(|e| VaultError::Database(e.to_string()))?;
        let parent: Option<Option<String>> = tx
            .query_row(
                "SELECT parent_id FROM folders WHERE id = ?1",
                rusqlite::params![folder_id],
                |row| row.get(0),
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })
            .map_err(|e| VaultError::Database(e.to_string()))?;
        let Some(parent) = parent else {
            return Ok(false);
        };
        tx.execute(
            "UPDATE folders SET parent_id = ?2 WHERE parent_id = ?1",
            rusqlite::params![folder_id, parent],
        )
        .map_err(|e| VaultError::Database(e.to_string()))?;
        tx.execute(
            "UPDATE entries SET folder_id = ?2 WHERE folder_id = ?1",
            rusqlite::params![folder_id, parent],
        )
        .map_err(|e| VaultError::Database(e.to_string()))?;
        tx.execute(
            "DELETE FROM folders WHERE id = ?1",
            rusqlite::params![folder_id],
        )
        .map_err(|e| VaultError::Database(e.to_string()))?;
        tx.commit()
            .map_err(|e| VaultError::Database(e.to_string()))?;
        drop(guard);
        Ok(true)
    }

    /// File an entry in a folder, or take it out of its folder with `None`.
    /// False when the entry (or the folder) does not exist.
    pub fn set_entry_folder(
        &self,
        entry_id: &str,
        folder_id: Option<&str>,
    ) -> Result<bool, VaultError> {
        self.execute(|conn| {
            if let Some(folder) = folder_id {
                let known: i64 = conn.query_row(
                    "SELECT COUNT(*) FROM folders WHERE id = ?1",
                    rusqlite::params![folder],
                    |row| row.get(0),
                )?;
                if known == 0 {
                    return Ok(false);
                }
            }
            let changed = conn.execute(
                "UPDATE entries SET folder_id = ?2 WHERE id = ?1",
                rusqlite::params![entry_id, folder_id],
            )?;
            Ok(changed > 0)
        })
    }

    /// The folder of every entry that is in one, as (entry id, folder id).
    pub fn entry_folders(&self) -> Result<Vec<(String, String)>, VaultError> {
        self.execute(|conn| {
            let mut stmt =
                conn.prepare("SELECT id, folder_id FROM entries WHERE folder_id IS NOT NULL")?;
            let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
            rows.collect()
        })
    }

    /// Put an entry in the trash, stamped with `at` (RFC 3339). False when
    /// there is no such entry, or it is already in the trash.
    pub fn trash_entry(&self, entry_id: &str, at: &str) -> Result<bool, VaultError> {
        self.execute(|conn| {
            let changed = conn.execute(
                "UPDATE entries SET trashed_at = ?2 WHERE id = ?1 AND trashed_at IS NULL",
                rusqlite::params![entry_id, at],
            )?;
            Ok(changed > 0)
        })
    }

    /// Take an entry out of the trash. False when it is not in it.
    pub fn restore_entry(&self, entry_id: &str) -> Result<bool, VaultError> {
        self.execute(|conn| {
            let changed = conn.execute(
                "UPDATE entries SET trashed_at = NULL WHERE id = ?1 AND trashed_at IS NOT NULL",
                rusqlite::params![entry_id],
            )?;
            Ok(changed > 0)
        })
    }

    /// When each trashed entry was put there, as (entry id, RFC 3339 time).
    pub fn trashed_stamps(&self) -> Result<Vec<(String, String)>, VaultError> {
        self.execute(|conn| {
            let mut stmt =
                conn.prepare("SELECT id, trashed_at FROM entries WHERE trashed_at IS NOT NULL")?;
            let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
            rows.collect()
        })
    }

    /// Mark a stored file as binary, as an earlier version would have stored
    /// it. Only for tests of how older vaults are handled.
    #[doc(hidden)]
    pub fn mark_file_binary_for_test(&self, entry_id: &str) -> Result<(), VaultError> {
        self.execute(|conn| {
            conn.execute(
                "UPDATE blobs SET is_text = 0, mime_type = 'application/pdf' WHERE entry_id = ?1",
                rusqlite::params![entry_id],
            )?;
            Ok(())
        })
    }

    /// Set when an entry was saved, so a test can ask about "a couple of
    /// months ago" without waiting for it.
    #[doc(hidden)]
    pub fn backdate_entry_for_test(&self, entry_id: &str, when: &str) -> Result<(), VaultError> {
        self.execute(|conn| {
            conn.execute(
                "UPDATE entries SET created_at = ?2, updated_at = ?2 WHERE id = ?1",
                rusqlite::params![entry_id, when],
            )?;
            Ok(())
        })
    }

    /// The ids of entries that hold a stored file that is not text. Files
    /// from earlier versions may be binary; new ones never are. Read from
    /// the blob's plain flag, so nothing is decrypted.
    pub fn binary_file_ids(&self) -> Result<Vec<String>, VaultError> {
        self.execute(|conn| {
            let mut stmt = conn.prepare("SELECT entry_id FROM blobs WHERE is_text = 0")?;
            let rows = stmt.query_map([], |row| row.get(0))?;
            rows.collect()
        })
    }

    /// The encrypted blob stored for an entry, if it has one.
    pub fn get_blob(&self, entry_id: &str) -> Result<Option<EncryptedBlob>, VaultError> {
        self.execute(|conn| {
            let mut stmt = conn.prepare(
                "SELECT dek_wrapped, nonce, ciphertext, byte_len, mime_type, is_text
                 FROM blobs WHERE entry_id = ?1",
            )?;
            let mut rows = stmt.query(rusqlite::params![entry_id])?;
            let Some(row) = rows.next()? else {
                return Ok(None);
            };
            let nonce: Vec<u8> = row.get(1)?;
            let nonce: [u8; crate::domain::vault::crypto_constants::NONCE_LEN] = nonce
                .as_slice()
                .try_into()
                .map_err(|_| rusqlite::Error::InvalidQuery)?;
            let byte_len: i64 = row.get(3)?;
            let is_text: i64 = row.get(5)?;
            Ok(Some(EncryptedBlob {
                dek_wrapped: row.get(0)?,
                nonce,
                ciphertext: row.get(2)?,
                byte_len: u64::try_from(byte_len).unwrap_or(0),
                mime_type: row.get(4)?,
                is_text: is_text != 0,
            }))
        })
    }

    pub fn insert_entry(&self, entry: &EncryptedEntry) -> Result<(), VaultError> {
        self.execute(|conn| insert_entry_row(conn, entry).map_err(into_rusqlite))
    }

    /// Every entry that is not in the trash, by name.
    pub fn list_all_entries(&self) -> Result<Vec<EncryptedEntry>, VaultError> {
        self.query_entries("trashed_at IS NULL")
    }

    /// Every entry in the trash, by name.
    pub fn list_trashed_entries(&self) -> Result<Vec<EncryptedEntry>, VaultError> {
        self.query_entries("trashed_at IS NOT NULL")
    }

    /// Entries matching a fixed condition. `condition` is always one of
    /// this file's own constants, never user text.
    #[allow(clippy::indexing_slicing)]
    fn query_entries(&self, condition: &'static str) -> Result<Vec<EncryptedEntry>, VaultError> {
        self.execute(|conn| {
            let mut stmt = conn.prepare(&format!(
                "SELECT id, kind, name, tags, dek_wrapped, nonce, ciphertext, version, created_at, updated_at
                 FROM entries WHERE {condition} ORDER BY name"
            ))?;

            let rows = stmt.query_map([], |row| {
                let id: String = row.get(0)?;
                let kind_str: String = row.get(1)?;
                let name: String = row.get(2)?;
                let tags_json: String = row.get(3)?;
                let dek_wrapped: Vec<u8> = row.get(4)?;
                let nonce: Vec<u8> = row.get(5)?;
                let ciphertext: Vec<u8> = row.get(6)?;
                let version: u64 = row.get::<_, i64>(7)?.cast_unsigned();
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
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM entries WHERE trashed_at IS NULL",
                [],
                |row| row.get(0),
            )?;
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
