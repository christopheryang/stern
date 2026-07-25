use crate::domain::vault::errors::VaultError;

pub trait KeychainProvider: Send + Sync {
    fn store_key(&self, service: &str, account: &str, key: &[u8]) -> Result<(), VaultError>;
    fn load_key(&self, service: &str, account: &str) -> Result<Vec<u8>, VaultError>;
    fn delete_key(&self, service: &str, account: &str) -> Result<(), VaultError>;
    fn has_key(&self, service: &str, account: &str) -> bool;
}
