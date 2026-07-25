// Tests moved from domain modules
#![allow(clippy::unwrap_used)]

use std::error::Error;
use stern_core::domain::shared::ids::{EntryId, TagId, VaultId};
use stern_core::domain::vault::aad::{blob_aad, entry_aad, tag_aad};
use stern_core::domain::vault::crypto_constants::{
    BLOB_AAD_SUFFIX, NONCE_LEN, SECRET_KEY_LEN, VAULT_SALT_LEN, VERIFY_HASH_LEN,
};
use stern_core::domain::vault::entry::{
    DecryptedEntry, EncryptedEntry, EntryKind, EntryPayload, Field,
};
use stern_core::domain::vault::errors::VaultError;
use stern_core::domain::vault::index::{IndexEntry, VaultIndex};
use stern_core::domain::vault::kdf_params::KdfParams;
use stern_core::domain::vault::vault_meta::VaultMeta;

// ---------------------------------------------------------------------------
// aad tests
// ---------------------------------------------------------------------------

#[test]
fn entry_aad_length() {
    let id = "test-id-123";
    let aad = entry_aad(id, 1);
    assert_eq!(aad.len(), id.len() + 8);
}

#[test]
fn entry_aad_content() {
    let id = "abc";
    let aad = entry_aad(id, 42);
    assert_eq!(&aad[..3], b"abc");
    assert_eq!(&aad[3..], &42_u64.to_le_bytes());
}

#[test]
fn entry_aad_version_zero() {
    let id = "x";
    let aad = entry_aad(id, 0);
    assert_eq!(aad, b"x\0\0\0\0\0\0\0\0");
}

#[test]
fn tag_aad_is_just_bytes() {
    let tag = "my-tag";
    let aad = tag_aad(tag);
    assert_eq!(aad, tag.as_bytes());
    assert_eq!(aad.len(), tag.len());
}

#[test]
fn blob_aad_suffix() {
    let id = "entry1";
    let aad = blob_aad(id);
    let expected_len = id.len() + BLOB_AAD_SUFFIX.len();
    assert_eq!(aad.len(), expected_len);
    assert_eq!(&aad[..id.len()], id.as_bytes());
    assert_eq!(&aad[id.len()..], BLOB_AAD_SUFFIX);
}

#[test]
fn entry_aad_different_versions_differ() {
    let a1 = entry_aad("id", 1);
    let a2 = entry_aad("id", 2);
    assert_ne!(a1, a2);
}

#[test]
fn entry_aad_different_ids_differ() {
    let a1 = entry_aad("id1", 1);
    let a2 = entry_aad("id2", 1);
    assert_ne!(a1, a2);
}

#[test]
fn blob_aad_empty_id() {
    let aad = blob_aad("");
    assert_eq!(&aad, BLOB_AAD_SUFFIX);
}

// ---------------------------------------------------------------------------
// entry tests
// ---------------------------------------------------------------------------

#[test]
fn entry_kind_copy() {
    let k = EntryKind::Login;
    let k2 = k;
    assert!(matches!(k2, EntryKind::Login));
    assert!(matches!(k, EntryKind::Login));
}

#[test]
fn entry_kind_serde_roundtrip() {
    for kind in [EntryKind::Login, EntryKind::Note, EntryKind::Document] {
        let json = serde_json::to_string(&kind).expect("serialize EntryKind");
        let back: EntryKind = serde_json::from_str(&json).expect("deserialize EntryKind");
        assert_eq!(format!("{kind:?}"), format!("{back:?}"));
    }
}

#[test]
fn entry_payload_serde_roundtrip() {
    let payload = EntryPayload {
        name: "test".to_string(),
        kind: EntryKind::Login,
        tags: vec!["tag1".to_string(), "tag2".to_string()],
        fields: vec![
            Field {
                key: "user".to_string(),
                value: "admin".to_string(),
                hidden: false,
            },
            Field {
                key: "pass".to_string(),
                value: "secret".to_string(),
                hidden: true,
            },
        ],
    };
    let json = serde_json::to_string(&payload).expect("serialize EntryPayload");
    let back: EntryPayload = serde_json::from_str(&json).expect("deserialize EntryPayload");
    assert_eq!(back.name, "test");
    assert!(matches!(back.kind, EntryKind::Login));
    assert_eq!(back.tags.len(), 2);
    assert_eq!(back.fields.len(), 2);
    assert_eq!(back.fields[0].key, "user");
    assert!(!back.fields[0].hidden);
    assert!(back.fields[1].hidden);
}

#[test]
fn field_serde_roundtrip() {
    let field = Field {
        key: "username".to_string(),
        value: "admin".to_string(),
        hidden: true,
    };
    let json = serde_json::to_string(&field).expect("serialize Field");
    let back: Field = serde_json::from_str(&json).expect("deserialize Field");
    assert_eq!(back.key, "username");
    assert_eq!(back.value, "admin");
    assert!(back.hidden);
}

#[test]
fn encrypted_entry_serde_roundtrip() {
    let entry = EncryptedEntry {
        id: "abc-123".to_string(),
        kind: EntryKind::Note,
        name: "My Note".to_string(),
        tags: vec!["personal".to_string()],
        dek_wrapped: vec![1, 2, 3, 4],
        nonce: [0u8; NONCE_LEN],
        ciphertext: vec![10, 20, 30],
        version: 1,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    let json = serde_json::to_string(&entry).expect("serialize EncryptedEntry");
    let back: EncryptedEntry = serde_json::from_str(&json).expect("deserialize EncryptedEntry");
    assert_eq!(back.id, "abc-123");
    assert!(matches!(back.kind, EntryKind::Note));
    assert_eq!(back.name, "My Note");
    assert_eq!(back.tags, vec!["personal"]);
    assert_eq!(back.dek_wrapped, vec![1, 2, 3, 4]);
    assert_eq!(back.ciphertext, vec![10, 20, 30]);
    assert_eq!(back.version, 1);
}

#[test]
fn decrypted_entry_has_correct_fields() {
    let entry = DecryptedEntry {
        id: "id-1".to_string(),
        kind: EntryKind::Document,
        name: "doc".to_string(),
        tags: vec![],
        fields: vec![],
        version: 2,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    assert_eq!(entry.id, "id-1");
    assert!(matches!(entry.kind, EntryKind::Document));
    assert_eq!(entry.version, 2);
}

// ---------------------------------------------------------------------------
// errors tests
// ---------------------------------------------------------------------------

#[test]
fn display_key_derivation_failed() {
    let err = VaultError::KeyDerivationFailed("bad params".to_string());
    assert_eq!(err.to_string(), "key derivation failed: bad params");
}

#[test]
fn display_encryption_failed() {
    let err = VaultError::EncryptionFailed;
    assert_eq!(err.to_string(), "encryption failed");
}

#[test]
fn display_decryption_failed() {
    let err = VaultError::DecryptionFailed;
    assert_eq!(err.to_string(), "decryption failed");
}

#[test]
fn display_invalid_master_password() {
    let err = VaultError::InvalidMasterPassword;
    assert_eq!(err.to_string(), "invalid master password");
}

#[test]
fn display_vault_locked() {
    let err = VaultError::VaultLocked;
    assert_eq!(err.to_string(), "vault locked");
}

#[test]
fn display_entry_not_found() {
    let err = VaultError::EntryNotFound("abc".to_string());
    assert_eq!(err.to_string(), "entry not found: abc");
}

#[test]
fn display_database() {
    let err = VaultError::Database("disk full".to_string());
    assert_eq!(err.to_string(), "database error: disk full");
}

#[test]
fn display_keychain() {
    let err = VaultError::Keychain("no key".to_string());
    assert_eq!(err.to_string(), "keychain error: no key");
}

#[test]
fn display_io() {
    let err = VaultError::Io("not found".to_string());
    assert_eq!(err.to_string(), "io error: not found");
}

#[test]
fn display_serialization() {
    let err = VaultError::Serialization("bad json".to_string());
    assert_eq!(err.to_string(), "serialization error: bad json");
}

#[test]
fn serialize_all_variants() {
    let errors: Vec<VaultError> = vec![
        VaultError::KeyDerivationFailed("x".to_string()),
        VaultError::EncryptionFailed,
        VaultError::DecryptionFailed,
        VaultError::InvalidMasterPassword,
        VaultError::VaultLocked,
        VaultError::EntryNotFound("x".to_string()),
        VaultError::Database("x".to_string()),
        VaultError::Keychain("x".to_string()),
        VaultError::Io("x".to_string()),
        VaultError::Serialization("x".to_string()),
    ];
    for err in &errors {
        let json = serde_json::to_string(err).expect("VaultError should serialize");
        let s: String = serde_json::from_str(&json).expect("should deserialize as string");
        assert_eq!(s, err.to_string());
    }
}

#[test]
fn error_is_send() {
    fn assert_send<T: Send>() {}
    assert_send::<VaultError>();
}

#[test]
fn error_is_sync() {
    fn assert_sync<T: Sync>() {}
    assert_sync::<VaultError>();
}

#[test]
fn error_is_std_error() {
    let err: Box<dyn Error> = Box::new(VaultError::VaultLocked);
    assert!(err.to_string().contains("vault locked"));
}

// ---------------------------------------------------------------------------
// index tests
// ---------------------------------------------------------------------------

fn make_entry(id: &str, name: &str, tags: Vec<&str>) -> IndexEntry {
    IndexEntry {
        id: id.to_string(),
        name: name.to_string(),
        kind: "login".to_string(),
        tags: tags.into_iter().map(str::to_string).collect(),
        version: 1,
        created_at: "2024-01-01T00:00:00Z".to_string(),
        updated_at: "2024-01-01T00:00:00Z".to_string(),
    }
}

#[test]
fn insert_and_get() {
    let mut idx = VaultIndex::default();
    let entry = make_entry("a1", "GitHub", vec!["dev"]);
    idx.insert(entry);
    let got = idx.get("a1").expect("entry should exist");
    assert_eq!(got.name, "GitHub");
    assert_eq!(got.tags, vec!["dev"]);
}

#[test]
fn get_missing_returns_none() {
    let idx = VaultIndex::default();
    assert!(idx.get("nope").is_none());
}

#[test]
fn shift_remove_existing() {
    let mut idx = VaultIndex::default();
    idx.insert(make_entry("a1", "GitHub", vec![]));
    let removed = idx.shift_remove("a1").expect("should remove");
    assert_eq!(removed.id, "a1");
    assert!(idx.get("a1").is_none());
}

#[test]
fn shift_remove_missing_returns_none() {
    let mut idx = VaultIndex::default();
    assert!(idx.shift_remove("nope").is_none());
}

#[test]
fn search_by_name_case_insensitive() {
    let mut idx = VaultIndex::default();
    idx.insert(make_entry("a1", "GitHub", vec![]));
    idx.insert(make_entry("a2", "GitLab", vec![]));
    idx.insert(make_entry("a3", "Netflix", vec![]));

    let results = idx.search("git");
    assert_eq!(results.len(), 2);
}

#[test]
fn search_by_tag() {
    let mut idx = VaultIndex::default();
    idx.insert(make_entry("a1", "GitHub", vec!["work", "dev"]));
    idx.insert(make_entry("a2", "Netflix", vec!["personal"]));

    let results = idx.search("work");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "a1");
}

#[test]
fn search_by_tag_case_insensitive() {
    let mut idx = VaultIndex::default();
    idx.insert(make_entry("a1", "GitHub", vec!["Work"]));
    let results = idx.search("work");
    assert_eq!(results.len(), 1);
}

#[test]
fn search_no_results() {
    let mut idx = VaultIndex::default();
    idx.insert(make_entry("a1", "GitHub", vec![]));
    assert!(idx.search("facebook").is_empty());
}

#[test]
fn search_empty_index() {
    let idx = VaultIndex::default();
    assert!(idx.search("anything").is_empty());
}

#[test]
fn serde_roundtrip() {
    let mut idx = VaultIndex::default();
    idx.insert(make_entry("a1", "GitHub", vec!["dev"]));
    idx.insert(make_entry("a2", "Netflix", vec!["personal"]));

    let json = serde_json::to_string(&idx).expect("serialize VaultIndex");
    let back: VaultIndex = serde_json::from_str(&json).expect("deserialize VaultIndex");
    assert_eq!(back.entries.len(), 2);
    assert!(back.get("a1").is_some());
    assert!(back.get("a2").is_some());
}

#[test]
fn insert_overwrites_same_id() {
    let mut idx = VaultIndex::default();
    idx.insert(make_entry("a1", "GitHub v1", vec![]));
    idx.insert(make_entry("a1", "GitHub v2", vec![]));
    let got = idx.get("a1").expect("entry should exist");
    assert_eq!(got.name, "GitHub v2");
}

#[test]
fn default_is_empty() {
    let idx = VaultIndex::default();
    assert!(idx.entries.is_empty());
}

// ---------------------------------------------------------------------------
// kdf_params tests
// ---------------------------------------------------------------------------

#[test]
fn argon2id_default_values() {
    let p = KdfParams::argon2id_default();
    assert_eq!(p.alg, "argon2id");
    assert_eq!(p.m, 262_144);
    assert_eq!(p.t, 3);
    assert_eq!(p.p, 4);
    assert_eq!(p.version, 1);
}

#[test]
fn fast_values_are_small() {
    let p = KdfParams::fast();
    assert_eq!(p.alg, "argon2id");
    assert_eq!(p.m, 8);
    assert_eq!(p.t, 1);
    assert_eq!(p.p, 1);
    assert_eq!(p.version, 1);
}

#[test]
fn serde_roundtrip_kdf_params() {
    let params = KdfParams::argon2id_default();
    let json = serde_json::to_string(&params).expect("serialize KdfParams");
    let back: KdfParams = serde_json::from_str(&json).expect("deserialize KdfParams");
    assert_eq!(params, back);
}

#[test]
fn serde_roundtrip_fast() {
    let params = KdfParams::fast();
    let json = serde_json::to_string(&params).expect("serialize");
    let back: KdfParams = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(params, back);
}

#[test]
fn clone_eq() {
    let p1 = KdfParams::argon2id_default();
    let p2 = p1.clone();
    assert_eq!(p1, p2);
}

// ---------------------------------------------------------------------------
// vault_meta tests
// ---------------------------------------------------------------------------

#[test]
fn construction_and_field_access() {
    let salt = [1u8; VAULT_SALT_LEN];
    let verify_hash = [2u8; VERIFY_HASH_LEN];
    let secret_key = [3u8; SECRET_KEY_LEN];
    let params = KdfParams::argon2id_default();

    let meta = VaultMeta {
        salt,
        verify_hash,
        secret_key,
        params,
    };

    assert_eq!(meta.salt, [1u8; VAULT_SALT_LEN]);
    assert_eq!(meta.verify_hash, [2u8; VERIFY_HASH_LEN]);
    assert_eq!(meta.secret_key, [3u8; SECRET_KEY_LEN]);
    assert_eq!(meta.params.alg, "argon2id");
    assert_eq!(meta.params.m, 262_144);
}

#[test]
fn construction_with_fast_params() {
    let meta = VaultMeta {
        salt: [0u8; VAULT_SALT_LEN],
        verify_hash: [0u8; VERIFY_HASH_LEN],
        secret_key: [0u8; SECRET_KEY_LEN],
        params: KdfParams::fast(),
    };
    assert_eq!(meta.params.m, 8);
}

// ---------------------------------------------------------------------------
// ids tests
// ---------------------------------------------------------------------------

#[test]
fn entry_id_new_non_empty() {
    let id = EntryId::new();
    assert!(!id.0.is_empty(), "EntryId should be non-empty");
}

#[test]
fn entry_id_default_non_empty() {
    let id = EntryId::default();
    assert!(!id.0.is_empty());
}

#[test]
fn entry_id_unique() {
    let a = EntryId::new();
    let b = EntryId::new();
    assert_ne!(a.0, b.0, "two EntryIds should be different");
}

#[test]
fn entry_id_display() {
    let id = EntryId("test-123".to_string());
    assert_eq!(format!("{id}"), "test-123");
}

#[test]
fn entry_id_serde_roundtrip() {
    let id = EntryId::new();
    let json = serde_json::to_string(&id).expect("serialize");
    let back: EntryId = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(id, back);
}

#[test]
fn tag_id_new_non_empty() {
    let id = TagId::new();
    assert!(!id.0.is_empty());
}

#[test]
fn tag_id_unique() {
    let a = TagId::new();
    let b = TagId::new();
    assert_ne!(a.0, b.0);
}

#[test]
fn tag_id_serde_roundtrip() {
    let id = TagId::new();
    let json = serde_json::to_string(&id).expect("serialize");
    let back: TagId = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(id, back);
}

#[test]
fn vault_id_new_non_empty() {
    let id = VaultId::new();
    assert!(!id.0.is_empty());
}

#[test]
fn vault_id_unique() {
    let a = VaultId::new();
    let b = VaultId::new();
    assert_ne!(a.0, b.0);
}

#[test]
fn vault_id_serde_roundtrip() {
    let id = VaultId::new();
    let json = serde_json::to_string(&id).expect("serialize");
    let back: VaultId = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(id, back);
}

#[test]
fn ids_are_valid_uuid_format() {
    let id = EntryId::new();
    assert_eq!(id.0.len(), 36, "UUID v4 should be 36 chars: {}", id.0);
    assert_eq!(id.0.chars().filter(|c| *c == '-').count(), 4);
}
