use aes_kw::{KeyInit as AesKwKeyInit, KwAes256};
use chacha20poly1305::aead::{Aead, KeyInit as AeadKeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use rand::RngCore;
use rand::rngs::OsRng;
use subtle::ConstantTimeEq;
use zeroize::Zeroizing;

use crate::application::vault::ports::crypto::{CryptoProvider, Nonce};
use crate::domain::vault::crypto_constants::{
    DEK_LEN, DEK_WRAPPED_LEN, KEK_LEN, NONCE_LEN, SECRET_KEY_LEN, VAULT_SALT_LEN,
    VERIFY_HASH_LEN,
};
use crate::domain::vault::errors::VaultError;

pub struct XChaCha20CryptoProvider;

impl XChaCha20CryptoProvider {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Default for XChaCha20CryptoProvider {
    fn default() -> Self {
        Self::new()
    }
}

fn cipher(key: &[u8; 32]) -> XChaCha20Poly1305 {
    XChaCha20Poly1305::new(key.into())
}

pub(crate) fn aead_encrypt(
    key: &[u8; 32],
    payload: &[u8],
    aad: &[u8],
) -> Result<(Nonce, Vec<u8>), VaultError> {
    let mut nonce_bytes = [0u8; NONCE_LEN];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = XNonce::from_slice(&nonce_bytes);
    let ct = cipher(key)
        .encrypt(nonce, Payload { msg: payload, aad })
        .map_err(|_| VaultError::EncryptionFailed)?;
    Ok((nonce_bytes, ct))
}

pub(crate) fn aead_decrypt(
    key: &[u8; 32],
    nonce: &[u8; NONCE_LEN],
    ct: &[u8],
    aad: &[u8],
) -> Result<Zeroizing<Vec<u8>>, VaultError> {
    let xnonce = XNonce::from_slice(nonce);
    let pt = cipher(key)
        .decrypt(xnonce, Payload { msg: ct, aad })
        .map_err(|_| VaultError::DecryptionFailed)?;
    Ok(Zeroizing::new(pt))
}

impl CryptoProvider for XChaCha20CryptoProvider {
    fn encrypt_entry(
        &self,
        dek: &[u8; DEK_LEN],
        payload: &[u8],
        aad: &[u8],
    ) -> Result<(Nonce, Vec<u8>), VaultError> {
        aead_encrypt(dek, payload, aad)
    }

    fn decrypt_entry(
        &self,
        dek: &[u8; DEK_LEN],
        nonce: &[u8; NONCE_LEN],
        ciphertext: &[u8],
        aad: &[u8],
    ) -> Result<Zeroizing<Vec<u8>>, VaultError> {
        aead_decrypt(dek, nonce, ciphertext, aad)
    }

    fn wrap_dek(
        &self,
        dek: &[u8; DEK_LEN],
        kek: &[u8; KEK_LEN],
    ) -> Result<[u8; DEK_WRAPPED_LEN], VaultError> {
        let kw = KwAes256::new(kek.into());
        let mut out = [0u8; DEK_WRAPPED_LEN];
        kw.wrap_key(dek, &mut out)
            .map_err(|_| VaultError::EncryptionFailed)?;
        Ok(out)
    }

    fn unwrap_dek(
        &self,
        wrapped: &[u8; DEK_WRAPPED_LEN],
        kek: &[u8; KEK_LEN],
    ) -> Result<Zeroizing<[u8; DEK_LEN]>, VaultError> {
        let kw = KwAes256::new(kek.into());
        let mut out = [0u8; DEK_LEN];
        kw.unwrap_key(wrapped, &mut out)
            .map_err(|_| VaultError::DecryptionFailed)?;
        Ok(Zeroizing::new(out))
    }

    fn generate_dek(&self) -> Zeroizing<[u8; DEK_LEN]> {
        let mut k = [0u8; DEK_LEN];
        OsRng.fill_bytes(&mut k);
        Zeroizing::new(k)
    }

    fn generate_nonce(&self) -> Nonce {
        let mut n = [0u8; NONCE_LEN];
        OsRng.fill_bytes(&mut n);
        n
    }

    fn generate_secret_key(&self) -> Zeroizing<[u8; SECRET_KEY_LEN]> {
        let mut k = [0u8; SECRET_KEY_LEN];
        OsRng.fill_bytes(&mut k);
        Zeroizing::new(k)
    }

    fn generate_vault_salt(&self) -> [u8; VAULT_SALT_LEN] {
        let mut s = [0u8; VAULT_SALT_LEN];
        OsRng.fill_bytes(&mut s);
        s
    }

    fn verify_hash_matches(
        &self,
        candidate: &[u8; VERIFY_HASH_LEN],
        expected: &[u8; VERIFY_HASH_LEN],
    ) -> bool {
        candidate.ct_eq(expected).into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::vault::ports::crypto::CryptoProvider;

    fn provider() -> XChaCha20CryptoProvider {
        XChaCha20CryptoProvider::new()
    }

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let p = provider();
        let dek = *p.generate_dek();
        let plaintext = b"hello, world!";
        let aad = b"test-aad";

        let (nonce, ct) = p.encrypt_entry(&dek, plaintext, aad).expect("encrypt");
        assert_ne!(&ct[..], plaintext);
        assert_eq!(ct.len(), plaintext.len() + 16);

        let pt = p.decrypt_entry(&dek, &nonce, &ct, aad).expect("decrypt");
        assert_eq!(&pt[..], plaintext);
    }

    #[test]
    fn decrypt_wrong_key_fails() {
        let p = provider();
        let dek1 = *p.generate_dek();
        let dek2 = *p.generate_dek();
        let plaintext = b"secret data";
        let aad = b"aad";

        let (nonce, ct) = p.encrypt_entry(&dek1, plaintext, aad).expect("encrypt");
        let result = p.decrypt_entry(&dek2, &nonce, &ct, aad);
        assert!(result.is_err(), "decrypt with wrong key should fail");
    }

    #[test]
    fn decrypt_wrong_aad_fails() {
        let p = provider();
        let dek = *p.generate_dek();
        let plaintext = b"secret data";

        let (nonce, ct) = p.encrypt_entry(&dek, plaintext, b"aad1").expect("encrypt");
        let result = p.decrypt_entry(&dek, &nonce, &ct, b"aad2");
        assert!(result.is_err(), "decrypt with wrong AAD should fail");
    }

    #[test]
    fn wrap_unwrap_dek_roundtrip() {
        let p = provider();
        let dek = *p.generate_dek();
        let kek = [0x42u8; KEK_LEN];

        let wrapped = p.wrap_dek(&dek, &kek).expect("wrap");
        assert_eq!(wrapped.len(), DEK_WRAPPED_LEN);

        let unwrapped = p.unwrap_dek(&wrapped, &kek).expect("unwrap");
        assert_eq!(*unwrapped, dek);
    }

    #[test]
    fn unwrap_dek_wrong_kek_fails() {
        let p = provider();
        let dek = *p.generate_dek();
        let kek1 = [0x11u8; KEK_LEN];
        let kek2 = [0x22u8; KEK_LEN];

        let wrapped = p.wrap_dek(&dek, &kek1).expect("wrap");
        let result = p.unwrap_dek(&wrapped, &kek2);
        assert!(result.is_err(), "unwrap with wrong KEK should fail");
    }

    #[test]
    fn generate_dek_correct_length() {
        let p = provider();
        let dek = p.generate_dek();
        assert_eq!(dek.len(), DEK_LEN);
    }

    #[test]
    fn generate_nonce_correct_length() {
        let p = provider();
        let nonce = p.generate_nonce();
        assert_eq!(nonce.len(), NONCE_LEN);
    }

    #[test]
    fn generate_secret_key_correct_length() {
        let p = provider();
        let sk = p.generate_secret_key();
        assert_eq!(sk.len(), SECRET_KEY_LEN);
    }

    #[test]
    fn generate_vault_salt_correct_length() {
        let p = provider();
        let salt = p.generate_vault_salt();
        assert_eq!(salt.len(), VAULT_SALT_LEN);
    }

    #[test]
    fn verify_hash_matches_true() {
        let p = provider();
        let h = [0xABu8; VERIFY_HASH_LEN];
        assert!(p.verify_hash_matches(&h, &h));
    }

    #[test]
    fn verify_hash_matches_false() {
        let p = provider();
        let h1 = [0xABu8; VERIFY_HASH_LEN];
        let h2 = [0xCDu8; VERIFY_HASH_LEN];
        assert!(!p.verify_hash_matches(&h1, &h2));
    }

    #[test]
    fn encrypt_empty_plaintext() {
        let p = provider();
        let dek = *p.generate_dek();

        let (nonce, ct) = p.encrypt_entry(&dek, b"", b"").expect("encrypt empty");
        let pt = p.decrypt_entry(&dek, &nonce, &ct, b"").expect("decrypt empty");
        assert!(pt.is_empty());
    }

    #[test]
    fn default_impl() {
        let p = XChaCha20CryptoProvider::default();
        let dek = p.generate_dek();
        assert_eq!(dek.len(), DEK_LEN);
    }

    #[test]
    fn encrypt_produces_unique_nonces() {
        let p = provider();
        let dek = *p.generate_dek();
        let (n1, _) = p.encrypt_entry(&dek, b"data", b"").expect("encrypt1");
        let (n2, _) = p.encrypt_entry(&dek, b"data", b"").expect("encrypt2");
        assert_ne!(n1, n2, "nonces should be random and unique");
    }
}
