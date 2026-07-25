use crate::domain::vault::crypto_constants::{
    KEK_LEN, MASTER_KEY_LEN, SECRET_KEY_LEN, VAULT_SALT_LEN, VERIFY_HASH_LEN,
};
use crate::domain::vault::errors::VaultError;
use crate::domain::vault::kdf_params::KdfParams;
use zeroize::Zeroizing;

pub trait KeyDerivationProvider: Send + Sync {
    fn preprocess_2skd(
        &self,
        master_password: &[u8],
        secret_key: &[u8; SECRET_KEY_LEN],
    ) -> Result<Zeroizing<[u8; 32]>, VaultError>;

    fn derive_master_key(
        &self,
        input: &[u8; 32],
        vault_salt: &[u8; VAULT_SALT_LEN],
        params: &KdfParams,
    ) -> Result<Zeroizing<[u8; MASTER_KEY_LEN]>, VaultError>;

    fn derive_kek(
        &self,
        master_key: &[u8; MASTER_KEY_LEN],
    ) -> Result<Zeroizing<[u8; KEK_LEN]>, VaultError>;

    fn derive_verify_hash(
        &self,
        master_key: &[u8; MASTER_KEY_LEN],
    ) -> Result<[u8; VERIFY_HASH_LEN], VaultError>;
}
