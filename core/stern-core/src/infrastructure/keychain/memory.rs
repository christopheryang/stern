use crate::application::vault::ports::keychain::KeychainProvider;
use crate::domain::vault::errors::VaultError;
use std::collections::HashMap;
use std::sync::Mutex;

pub struct MemoryKeychainProvider {
    store: Mutex<HashMap<String, Vec<u8>>>,
}

impl MemoryKeychainProvider {
    #[must_use]
    pub fn new() -> Self {
        Self {
            store: Mutex::new(HashMap::new()),
        }
    }
}

impl Default for MemoryKeychainProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl KeychainProvider for MemoryKeychainProvider {
    #[allow(clippy::significant_drop_tightening)]
    fn store_key(&self, service: &str, account: &str, key: &[u8]) -> Result<(), VaultError> {
        let mut store = self
            .store
            .lock()
            .map_err(|e| VaultError::Keychain(e.to_string()))?;
        store.insert(format!("{service}:{account}"), key.to_vec());
        Ok(())
    }

    fn load_key(&self, service: &str, account: &str) -> Result<Vec<u8>, VaultError> {
        let store = self
            .store
            .lock()
            .map_err(|e| VaultError::Keychain(e.to_string()))?;
        store
            .get(&format!("{service}:{account}"))
            .cloned()
            .ok_or_else(|| VaultError::Keychain("key not found".to_owned()))
    }

    #[allow(clippy::significant_drop_tightening)]
    fn delete_key(&self, service: &str, account: &str) -> Result<(), VaultError> {
        let mut store = self
            .store
            .lock()
            .map_err(|e| VaultError::Keychain(e.to_string()))?;
        store.remove(&format!("{service}:{account}"));
        Ok(())
    }

    fn has_key(&self, service: &str, account: &str) -> bool {
        self.store
            .lock()
            .is_ok_and(|s| s.contains_key(&format!("{service}:{account}")))
    }
}
