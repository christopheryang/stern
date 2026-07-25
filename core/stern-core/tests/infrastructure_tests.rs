use std::io::Read;
use stern_core::application::vault::ports::keychain::KeychainProvider;
use stern_core::domain::vault::crypto_constants::*;
use stern_core::domain::vault::entry::{EncryptedEntry, EntryKind};
use stern_core::domain::vault::errors::VaultError;
use stern_core::domain::vault::kdf_params::KdfParams;
use stern_core::infrastructure::backup::BackupProvider;
#[allow(clippy::unwrap_used)]
use stern_core::infrastructure::keychain::memory::MemoryKeychainProvider;
use stern_core::infrastructure::sqlite::repository::SqliteRepository;

fn kc() -> MemoryKeychainProvider {
    MemoryKeychainProvider::new()
}

fn temp_repo() -> (SqliteRepository, tempfile::TempPath) {
    let tmp = tempfile::NamedTempFile::new().expect("create temp file");
    let path = tmp.into_temp_path();
    let repo = SqliteRepository::new(&path).expect("open repo");
    (repo, path)
}

#[test]
fn store_and_load() {
    let kc = kc();
    kc.store_key("svc", "acct", b"key-data").expect("store");
    let loaded = kc.load_key("svc", "acct").expect("load");
    assert_eq!(loaded, b"key-data");
}

#[test]
fn has_key_true() {
    let kc = kc();
    kc.store_key("svc", "acct", b"data").expect("store");
    assert!(kc.has_key("svc", "acct"));
}

#[test]
fn has_key_false() {
    let kc = kc();
    assert!(!kc.has_key("svc", "acct"));
}

#[test]
fn load_missing_key_error() {
    let kc = kc();
    let result = kc.load_key("svc", "nope");
    assert!(result.is_err());
    match result.unwrap_err() {
        VaultError::Keychain(msg) => assert!(msg.contains("key not found")),
        other => panic!("expected Keychain error, got: {other:?}"),
    }
}

#[test]
fn delete_key() {
    let kc = kc();
    kc.store_key("svc", "acct", b"data").expect("store");
    assert!(kc.has_key("svc", "acct"));
    kc.delete_key("svc", "acct").expect("delete");
    assert!(!kc.has_key("svc", "acct"));
}

#[test]
fn delete_nonexistent_key_no_error() {
    let kc = kc();
    kc.delete_key("svc", "nope")
        .expect("delete non-existent should succeed");
}

#[test]
fn overwrite_key() {
    let kc = kc();
    kc.store_key("svc", "acct", b"v1").expect("store1");
    kc.store_key("svc", "acct", b"v2").expect("store2");
    let loaded = kc.load_key("svc", "acct").expect("load");
    assert_eq!(loaded, b"v2");
}

#[test]
fn different_service_account_independent() {
    let kc = kc();
    kc.store_key("s1", "a1", b"data1").expect("store");
    kc.store_key("s2", "a2", b"data2").expect("store");
    assert_eq!(kc.load_key("s1", "a1").expect("load"), b"data1");
    assert_eq!(kc.load_key("s2", "a2").expect("load"), b"data2");
}

#[test]
fn keychain_default_impl() {
    let kc = MemoryKeychainProvider::default();
    assert!(!kc.has_key("svc", "acct"));
}

#[test]
fn create_backup_writes_hash_and_data() {
    let provider = BackupProvider::new();
    let data = b"hello, backup world!";
    let tmp = tempfile::NamedTempFile::new().expect("temp file");
    let path = tmp.into_temp_path();

    provider.create_backup(data, &path).expect("create_backup");

    let mut file = std::fs::File::open(&path).expect("open backup");
    let mut contents = Vec::new();
    file.read_to_end(&mut contents).expect("read");

    let expected_hash = blake3::hash(data);
    let hash_bytes = expected_hash.as_bytes();
    assert_eq!(
        &contents[..hash_bytes.len()],
        hash_bytes,
        "first 32 bytes should be blake3 hash"
    );
    assert_eq!(
        &contents[hash_bytes.len()..],
        data,
        "rest should be original data"
    );
}

#[test]
fn create_backup_total_size() {
    let provider = BackupProvider::new();
    let data = b"test data for backup size check";
    let tmp = tempfile::NamedTempFile::new().expect("temp file");
    let path = tmp.into_temp_path();

    provider.create_backup(data, &path).expect("create_backup");

    let meta = std::fs::metadata(&path).expect("metadata");
    assert_eq!(meta.len(), 32 + data.len() as u64);
}

#[test]
fn create_backup_empty_data() {
    let provider = BackupProvider::new();
    let data = b"";
    let tmp = tempfile::NamedTempFile::new().expect("temp file");
    let path = tmp.into_temp_path();

    provider.create_backup(data, &path).expect("create_backup");

    let contents = std::fs::read(&path).expect("read");
    assert_eq!(contents.len(), 32);
    let expected_hash = blake3::hash(data);
    assert_eq!(&contents, expected_hash.as_bytes());
}

#[test]
fn backup_default_impl() {
    let _p = BackupProvider::default();
}

#[test]
fn new_creates_db() {
    let (repo, _path) = temp_repo();
    let count = repo.count_entries().expect("count");
    assert_eq!(count, 0);
}

#[test]
fn insert_and_list_entry() {
    let (repo, _path) = temp_repo();
    let entry = EncryptedEntry {
        id: "id-1".to_string(),
        kind: EntryKind::Login,
        name: "GitHub".to_string(),
        tags: vec!["dev".to_string()],
        dek_wrapped: vec![1, 2, 3],
        nonce: [4u8; NONCE_LEN],
        ciphertext: vec![5, 6, 7],
        version: 1,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    repo.insert_entry(&entry).expect("insert");
    let entries = repo.list_all_entries().expect("list");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].id, "id-1");
    assert_eq!(entries[0].name, "GitHub");
    assert!(matches!(entries[0].kind, EntryKind::Login));
    assert_eq!(entries[0].tags, vec!["dev"]);
}

#[test]
fn count_entries() {
    let (repo, _path) = temp_repo();
    assert_eq!(repo.count_entries().expect("count"), 0);

    let entry = EncryptedEntry {
        id: "e1".to_string(),
        kind: EntryKind::Note,
        name: "Note".to_string(),
        tags: vec![],
        dek_wrapped: vec![],
        nonce: [0u8; NONCE_LEN],
        ciphertext: vec![],
        version: 1,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    repo.insert_entry(&entry).expect("insert");
    assert_eq!(repo.count_entries().expect("count"), 1);
}

#[test]
fn vault_exists_false_initially() {
    let (repo, _path) = temp_repo();
    assert!(!repo.vault_exists().expect("vault_exists"));
}

#[test]
fn save_and_load_vault_meta() {
    let (repo, _path) = temp_repo();
    let salt = [1u8; VAULT_SALT_LEN];
    let verify_hash = [2u8; VERIFY_HASH_LEN];
    let secret_key = [3u8; SECRET_KEY_LEN];
    let params = KdfParams::argon2id_default();

    repo.save_vault_meta(&salt, &verify_hash, &secret_key, &params)
        .expect("save");
    assert!(repo.vault_exists().expect("vault_exists"));

    let meta = repo.load_vault_meta().expect("load").expect("should exist");
    assert_eq!(meta.salt, salt);
    assert_eq!(meta.verify_hash, verify_hash);
    assert_eq!(meta.secret_key, secret_key);
    assert_eq!(meta.params, params);
}

#[test]
fn load_vault_meta_none_when_empty() {
    let (repo, _path) = temp_repo();
    let meta = repo.load_vault_meta().expect("load");
    assert!(meta.is_none());
}

#[test]
fn list_entries_returns_sorted_by_name() {
    let (repo, _path) = temp_repo();
    for (id, name) in [("c", "Charlie"), ("a", "Alice"), ("b", "Bob")] {
        let entry = EncryptedEntry {
            id: id.to_string(),
            kind: EntryKind::Login,
            name: name.to_string(),
            tags: vec![],
            dek_wrapped: vec![],
            nonce: [0u8; NONCE_LEN],
            ciphertext: vec![],
            version: 1,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        repo.insert_entry(&entry).expect("insert");
    }
    let entries = repo.list_all_entries().expect("list");
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].name, "Alice");
    assert_eq!(entries[1].name, "Bob");
    assert_eq!(entries[2].name, "Charlie");
}

#[test]
fn insert_all_entry_kinds() {
    let (repo, _path) = temp_repo();
    for (i, kind) in [EntryKind::Login, EntryKind::Note, EntryKind::Document]
        .into_iter()
        .enumerate()
    {
        let entry = EncryptedEntry {
            id: format!("e{i}"),
            kind,
            name: format!("Entry {i}"),
            tags: vec![],
            dek_wrapped: vec![],
            nonce: [0u8; NONCE_LEN],
            ciphertext: vec![],
            version: 1,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        repo.insert_entry(&entry).expect("insert");
    }
    let entries = repo.list_all_entries().expect("list");
    assert_eq!(entries.len(), 3);
}
