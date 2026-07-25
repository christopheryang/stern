use crate::domain::vault::crypto_constants::NONCE_LEN;
use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum EntryKind {
    Login,
    Note,
    Document,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedEntry {
    pub id: String,
    pub kind: EntryKind,
    pub name: String,
    pub tags: Vec<String>,
    pub dek_wrapped: Vec<u8>,
    pub nonce: [u8; NONCE_LEN],
    pub ciphertext: Vec<u8>,
    pub version: u64,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntryPayload {
    pub name: String,
    pub kind: EntryKind,
    pub tags: Vec<String>,
    pub fields: Vec<Field>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Field {
    pub key: String,
    pub value: String,
    pub hidden: bool,
}

#[derive(Debug)]
pub struct DecryptedEntry {
    pub id: String,
    pub kind: EntryKind,
    pub name: String,
    pub tags: Vec<String>,
    pub fields: Vec<Field>,
    pub version: u64,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl Drop for EntryPayload {
    fn drop(&mut self) {
        self.name.zeroize();
        for field in &mut self.fields {
            field.value.zeroize();
        }
        for tag in &mut self.tags {
            tag.zeroize();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
                Field { key: "user".to_string(), value: "admin".to_string(), hidden: false },
                Field { key: "pass".to_string(), value: "secret".to_string(), hidden: true },
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
}

impl Drop for DecryptedEntry {
    fn drop(&mut self) {
        self.name.zeroize();
        for field in &mut self.fields {
            field.value.zeroize();
        }
        for tag in &mut self.tags {
            tag.zeroize();
        }
    }
}
