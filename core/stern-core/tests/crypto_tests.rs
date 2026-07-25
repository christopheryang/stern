use stern_core::application::vault::ports::crypto::CryptoProvider;
use stern_core::application::vault::ports::kdf::KeyDerivationProvider;
use stern_core::domain::vault::crypto_constants::*;
use stern_core::domain::vault::kdf_params::KdfParams;
use stern_core::infrastructure::crypto::argon2_kdf::Argon2idKdfProvider;
use stern_core::infrastructure::crypto::secret_mem::SecretMem;
#[allow(clippy::unwrap_used)]
use stern_core::infrastructure::crypto::xchacha20::XChaCha20CryptoProvider;
fn provider() -> XChaCha20CryptoProvider {
    XChaCha20CryptoProvider::new()
}

fn kdf() -> Argon2idKdfProvider {
    Argon2idKdfProvider::new()
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
    let pt = p
        .decrypt_entry(&dek, &nonce, &ct, b"")
        .expect("decrypt empty");
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

#[test]
fn preprocess_2skd_deterministic() {
    let kdf = kdf();
    let password = b"test-password";
    let secret_key = [42u8; SECRET_KEY_LEN];

    let r1 = kdf
        .preprocess_2skd(password, &secret_key)
        .expect("preprocess 1");
    let r2 = kdf
        .preprocess_2skd(password, &secret_key)
        .expect("preprocess 2");
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

    let key = kdf
        .derive_master_key(&input, &salt, &params)
        .expect("derive_master_key");
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
    let vh = kdf
        .derive_verify_hash(&master_key)
        .expect("derive_verify_hash");
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

    let input = *kdf
        .preprocess_2skd(password, &secret_key)
        .expect("preprocess");
    let mk = *kdf
        .derive_master_key(&input, &salt, &params)
        .expect("master key");
    let kek1 = *kdf.derive_kek(&mk).expect("kek");
    let vh1 = kdf.derive_verify_hash(&mk).expect("verify hash");

    let mk2 = *kdf
        .derive_master_key(&input, &salt, &params)
        .expect("master key 2");
    let kek2 = *kdf.derive_kek(&mk2).expect("kek 2");
    let vh2 = kdf.derive_verify_hash(&mk2).expect("verify hash 2");

    assert_eq!(kek1, kek2);
    assert_eq!(vh1, vh2);
}

#[test]
fn kdf_default_impl() {
    let _kdf = Argon2idKdfProvider::default();
}

#[test]
fn new_and_get() {
    let sm = SecretMem::new(42u64);
    assert_eq!(*sm.get(), 42);
}

#[test]
fn get_mut_and_set() {
    let mut sm = SecretMem::new(10u64);
    *sm.get_mut() = 20;
    assert_eq!(*sm.get(), 20);
}

#[test]
fn new_default_value() {
    let sm = SecretMem::<u32>::new(0);
    assert_eq!(*sm.get(), 0);
}

#[test]
fn works_with_u8() {
    let mut sm = SecretMem::new(0xFFu8);
    assert_eq!(*sm.get(), 0xFF);
    *sm.get_mut() = 0x00;
    assert_eq!(*sm.get(), 0x00);
}

#[test]
fn works_with_i32() {
    let sm = SecretMem::new(-42i32);
    assert_eq!(*sm.get(), -42);
}
