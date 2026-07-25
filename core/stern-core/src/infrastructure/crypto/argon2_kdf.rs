use argon2::{Algorithm, Argon2, Params, Version};
use hkdf::Hkdf;
use sha2::Sha256;
use zeroize::Zeroizing;

use crate::application::vault::ports::kdf::KeyDerivationProvider;
use crate::domain::vault::crypto_constants::{
    HKDF_INFO_2SKD, HKDF_INFO_KEK, HKDF_INFO_VERIFY, KEK_LEN, MASTER_KEY_LEN, SECRET_KEY_LEN,
    VAULT_SALT_LEN, VERIFY_HASH_LEN,
};
use crate::domain::vault::errors::VaultError;
use crate::domain::vault::kdf_params::KdfParams;

pub struct Argon2idKdfProvider;

impl Argon2idKdfProvider {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Default for Argon2idKdfProvider {
    fn default() -> Self {
        Self::new()
    }
}

fn hkdf_expand<const N: usize>(ikm: &[u8], info: &[u8]) -> Result<[u8; N], VaultError> {
    let hk = Hkdf::<Sha256>::new(None, ikm);
    let mut out = [0u8; N];
    hk.expand(info, &mut out)
        .map_err(|e| VaultError::KeyDerivationFailed(format!("hkdf expand: {e}")))?;
    Ok(out)
}

impl KeyDerivationProvider for Argon2idKdfProvider {
    fn preprocess_2skd(
        &self,
        master_password: &[u8],
        secret_key: &[u8; SECRET_KEY_LEN],
    ) -> Result<Zeroizing<[u8; 32]>, VaultError> {
        let hk = Hkdf::<Sha256>::new(Some(secret_key), master_password);
        let mut out = [0u8; 32];
        hk.expand(HKDF_INFO_2SKD, &mut out)
            .map_err(|e| VaultError::KeyDerivationFailed(format!("hkdf expand: {e}")))?;
        Ok(Zeroizing::new(out))
    }

    fn derive_master_key(
        &self,
        input: &[u8; 32],
        vault_salt: &[u8; VAULT_SALT_LEN],
        params: &KdfParams,
    ) -> Result<Zeroizing<[u8; MASTER_KEY_LEN]>, VaultError> {
        if params.alg != "argon2id" {
            return Err(VaultError::KeyDerivationFailed(format!(
                "unsupported kdf alg: {}",
                params.alg
            )));
        }
        let a2_params = Params::new(params.m, params.t, params.p, Some(MASTER_KEY_LEN))
            .map_err(|e| VaultError::KeyDerivationFailed(e.to_string()))?;
        let a2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, a2_params);
        let mut out = [0u8; MASTER_KEY_LEN];
        a2.hash_password_into(input, vault_salt, &mut out)
            .map_err(|e| VaultError::KeyDerivationFailed(e.to_string()))?;
        Ok(Zeroizing::new(out))
    }

    fn derive_kek(
        &self,
        master_key: &[u8; MASTER_KEY_LEN],
    ) -> Result<Zeroizing<[u8; KEK_LEN]>, VaultError> {
        Ok(Zeroizing::new(hkdf_expand::<KEK_LEN>(
            master_key,
            HKDF_INFO_KEK,
        )?))
    }

    fn derive_verify_hash(
        &self,
        master_key: &[u8; MASTER_KEY_LEN],
    ) -> Result<[u8; VERIFY_HASH_LEN], VaultError> {
        hkdf_expand::<VERIFY_HASH_LEN>(master_key, HKDF_INFO_VERIFY)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::vault::ports::kdf::KeyDerivationProvider;

    fn kdf() -> Argon2idKdfProvider {
        Argon2idKdfProvider::new()
    }

    #[test]
    fn preprocess_2skd_deterministic() {
        let kdf = kdf();
        let password = b"test-password";
        let secret_key = [42u8; SECRET_KEY_LEN];

        let r1 = kdf.preprocess_2skd(password, &secret_key).expect("preprocess 1");
        let r2 = kdf.preprocess_2skd(password, &secret_key).expect("preprocess 2");
        assert_eq!(*r1, *r2, "preprocess_2skd should be deterministic");
    }

    #[test]
    fn preprocess_2skd_different_passwords_differ() {
        let kdf = kdf();
        let sk = [0u8; SECRET_KEY_LEN];
        let r1 = kdf.preprocess_2skd(b"pass1", &sk).expect("ok");
        let r2 = kdf.preprocess_2skd(b"pass2", &sk).expect("ok");
        assert_ne!(*r1, *r2);
    }

    #[test]
    fn preprocess_2skd_output_length() {
        let kdf = kdf();
        let sk = [0u8; SECRET_KEY_LEN];
        let out = kdf.preprocess_2skd(b"password", &sk).expect("ok");
        assert_eq!(out.len(), 32);
    }

    #[test]
    fn derive_master_key_succeeds() {
        let kdf = kdf();
        let input = [1u8; 32];
        let salt = [2u8; VAULT_SALT_LEN];
        let params = KdfParams::fast();

        let key = kdf.derive_master_key(&input, &salt, &params).expect("derive_master_key");
        assert_eq!(key.len(), MASTER_KEY_LEN);
    }

    #[test]
    fn derive_master_key_wrong_alg_fails() {
        let kdf = kdf();
        let input = [0u8; 32];
        let salt = [0u8; VAULT_SALT_LEN];
        let params = KdfParams {
            alg: "scrypt".to_string(),
            m: 8,
            t: 1,
            p: 1,
            version: 1,
        };
        let result = kdf.derive_master_key(&input, &salt, &params);
        assert!(result.is_err(), "unsupported alg should fail");
    }

    #[test]
    fn derive_kek_correct_length() {
        let kdf = kdf();
        let master_key = [3u8; MASTER_KEY_LEN];
        let kek = kdf.derive_kek(&master_key).expect("derive_kek");
        assert_eq!(kek.len(), KEK_LEN);
    }

    #[test]
    fn derive_kek_deterministic() {
        let kdf = kdf();
        let mk = [5u8; MASTER_KEY_LEN];
        let r1 = kdf.derive_kek(&mk).expect("ok");
        let r2 = kdf.derive_kek(&mk).expect("ok");
        assert_eq!(*r1, *r2);
    }

    #[test]
    fn derive_verify_hash_correct_length() {
        let kdf = kdf();
        let master_key = [7u8; MASTER_KEY_LEN];
        let vh = kdf.derive_verify_hash(&master_key).expect("derive_verify_hash");
        assert_eq!(vh.len(), VERIFY_HASH_LEN);
    }

    #[test]
    fn derive_verify_hash_deterministic() {
        let kdf = kdf();
        let mk = [9u8; MASTER_KEY_LEN];
        let r1 = kdf.derive_verify_hash(&mk).expect("ok");
        let r2 = kdf.derive_verify_hash(&mk).expect("ok");
        assert_eq!(r1, r2);
    }

    #[test]
    fn full_chain_consistency() {
        let kdf = kdf();
        let password = b"my-secure-password";
        let secret_key = [0xAAu8; SECRET_KEY_LEN];
        let salt = [0xBBu8; VAULT_SALT_LEN];
        let params = KdfParams::fast();

        let input = *kdf.preprocess_2skd(password, &secret_key).expect("preprocess");
        let mk = *kdf.derive_master_key(&input, &salt, &params).expect("master key");
        let kek1 = *kdf.derive_kek(&mk).expect("kek");
        let vh1 = kdf.derive_verify_hash(&mk).expect("verify hash");

        let mk2 = *kdf.derive_master_key(&input, &salt, &params).expect("master key 2");
        let kek2 = *kdf.derive_kek(&mk2).expect("kek 2");
        let vh2 = kdf.derive_verify_hash(&mk2).expect("verify hash 2");

        assert_eq!(kek1, kek2);
        assert_eq!(vh1, vh2);
    }

    #[test]
    fn default_impl() {
        let _kdf = Argon2idKdfProvider::default();
    }
}
