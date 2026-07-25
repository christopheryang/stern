use crate::domain::vault::crypto_constants::{
    DEK_LEN, DEK_WRAPPED_LEN, KEK_LEN, NONCE_LEN, SECRET_KEY_LEN, VAULT_SALT_LEN, VERIFY_HASH_LEN,
};
use crate::domain::vault::errors::VaultError;
use zeroize::Zeroizing;

pub type Nonce = [u8; NONCE_LEN];

pub trait CryptoProvider: Send + Sync {
    fn encrypt_entry(
        &self,
        dek: &[u8; DEK_LEN],
        payload: &[u8],
        aad: &[u8],
    ) -> Result<(Nonce, Vec<u8>), VaultError>;

    fn decrypt_entry(
        &self,
        dek: &[u8; DEK_LEN],
        nonce: &[u8; NONCE_LEN],
        ciphertext: &[u8],
        aad: &[u8],
    ) -> Result<Zeroizing<Vec<u8>>, VaultError>;

    fn wrap_dek(
        &self,
        dek: &[u8; DEK_LEN],
        kek: &[u8; KEK_LEN],
    ) -> Result<[u8; DEK_WRAPPED_LEN], VaultError>;

    fn unwrap_dek(
        &self,
        wrapped: &[u8; DEK_WRAPPED_LEN],
        kek: &[u8; KEK_LEN],
    ) -> Result<Zeroizing<[u8; DEK_LEN]>, VaultError>;

    fn generate_dek(&self) -> Zeroizing<[u8; DEK_LEN]>;
    fn generate_nonce(&self) -> Nonce;
    fn generate_secret_key(&self) -> Zeroizing<[u8; SECRET_KEY_LEN]>;
    fn generate_vault_salt(&self) -> [u8; VAULT_SALT_LEN];

    fn verify_hash_matches(
        &self,
        candidate: &[u8; VERIFY_HASH_LEN],
        expected: &[u8; VERIFY_HASH_LEN],
    ) -> bool;
}
