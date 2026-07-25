use crate::application::vault::ports::keychain::KeychainProvider;
use crate::domain::vault::errors::VaultError;

pub struct OsKeychainProvider;

impl OsKeychainProvider {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Default for OsKeychainProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl KeychainProvider for OsKeychainProvider {
    fn store_key(&self, service: &str, account: &str, key: &[u8]) -> Result<(), VaultError> {
        let entry = keyring::Entry::new(service, account)
            .map_err(|e| VaultError::Keychain(e.to_string()))?;
        entry
            .set_password(
                &base64::Engine::encode(&base64::engine::general_purpose::STANDARD, key),
            )
            .map_err(|e| VaultError::Keychain(e.to_string()))?;
        Ok(())
    }

    fn load_key(&self, service: &str, account: &str) -> Result<Vec<u8>, VaultError> {
        let entry = keyring::Entry::new(service, account)
            .map_err(|e| VaultError::Keychain(e.to_string()))?;
        let password = entry
            .get_password()
            .map_err(|e| VaultError::Keychain(e.to_string()))?;
        base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            &password,
        )
        .map_err(|e| VaultError::Keychain(e.to_string()))
    }

    fn delete_key(&self, service: &str, account: &str) -> Result<(), VaultError> {
        let entry = keyring::Entry::new(service, account)
            .map_err(|e| VaultError::Keychain(e.to_string()))?;
        entry
            .delete_credential()
            .map_err(|e| VaultError::Keychain(e.to_string()))?;
        Ok(())
    }

    fn has_key(&self, service: &str, account: &str) -> bool {
        keyring::Entry::new(service, account)
            .and_then(|e| e.get_password())
            .is_ok()
    }
}
