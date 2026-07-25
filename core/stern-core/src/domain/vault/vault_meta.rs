use super::crypto_constants::{SECRET_KEY_LEN, VAULT_SALT_LEN, VERIFY_HASH_LEN};
use super::kdf_params::KdfParams;

pub struct VaultMeta {
    pub salt: [u8; VAULT_SALT_LEN],
    pub verify_hash: [u8; VERIFY_HASH_LEN],
    pub secret_key: [u8; SECRET_KEY_LEN],
    pub params: KdfParams,
}

