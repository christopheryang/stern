use super::crypto_constants::{SECRET_KEY_LEN, VAULT_SALT_LEN, VERIFY_HASH_LEN};
use super::kdf_params::KdfParams;

pub struct VaultMeta {
    pub salt: [u8; VAULT_SALT_LEN],
    pub verify_hash: [u8; VERIFY_HASH_LEN],
    pub secret_key: [u8; SECRET_KEY_LEN],
    pub params: KdfParams,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn construction_and_field_access() {
        let salt = [1u8; VAULT_SALT_LEN];
        let verify_hash = [2u8; VERIFY_HASH_LEN];
        let secret_key = [3u8; SECRET_KEY_LEN];
        let params = KdfParams::argon2id_default();

        let meta = VaultMeta {
            salt,
            verify_hash,
            secret_key,
            params,
        };

        assert_eq!(meta.salt, [1u8; VAULT_SALT_LEN]);
        assert_eq!(meta.verify_hash, [2u8; VERIFY_HASH_LEN]);
        assert_eq!(meta.secret_key, [3u8; SECRET_KEY_LEN]);
        assert_eq!(meta.params.alg, "argon2id");
        assert_eq!(meta.params.m, 262_144);
    }

    #[test]
    fn construction_with_fast_params() {
        let meta = VaultMeta {
            salt: [0u8; VAULT_SALT_LEN],
            verify_hash: [0u8; VERIFY_HASH_LEN],
            secret_key: [0u8; SECRET_KEY_LEN],
            params: KdfParams::fast(),
        };
        assert_eq!(meta.params.m, 8);
    }
}
