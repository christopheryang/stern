use crate::domain::vault::crypto_constants::NONCE_LEN;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
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

impl Drop for DecryptedEntry {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        for field in &mut self.fields {
            field.value.zeroize();
        }
    }
}
