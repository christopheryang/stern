use serde::Serialize;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum VaultError {
    #[error("key derivation failed: {0}")]
    KeyDerivationFailed(String),

    #[error("encryption failed")]
    EncryptionFailed,

    #[error("decryption failed")]
    DecryptionFailed,

    #[error("invalid master password")]
    InvalidMasterPassword,

    #[error("vault locked")]
    VaultLocked,

    #[error("entry not found: {0}")]
    EntryNotFound(String),

    #[error("database error: {0}")]
    Database(String),

    #[error("keychain error: {0}")]
    Keychain(String),

    #[error("io error: {0}")]
    Io(String),

    #[error("serialization error: {0}")]
    Serialization(String),
}


impl Serialize for VaultError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}
