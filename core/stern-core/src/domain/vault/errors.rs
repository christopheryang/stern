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

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;

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
}

impl Serialize for VaultError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}
