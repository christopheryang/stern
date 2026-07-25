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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::vault::ports::keychain::KeychainProvider;

    fn kc() -> MemoryKeychainProvider {
        MemoryKeychainProvider::new()
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
        kc.delete_key("svc", "nope").expect("delete non-existent should succeed");
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
    fn default_impl() {
        let kc = MemoryKeychainProvider::default();
        assert!(!kc.has_key("svc", "acct"));
    }
}
