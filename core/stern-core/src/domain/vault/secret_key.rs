use hmac::digest::KeyInit;
use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

const CHECKSUM_LEN: usize = 4;

#[derive(Debug, Clone)]
pub struct SecretKey {
    pub key: [u8; 16],
    pub checksum: [u8; CHECKSUM_LEN],
}

impl SecretKey {
    #[must_use]
    #[allow(clippy::expect_used)]
    pub fn new(key: [u8; 16]) -> Self {
        let checksum = Self::compute_checksum(&key);
        Self { key, checksum }
    }

    #[allow(clippy::expect_used, clippy::indexing_slicing)]
    fn compute_checksum(key: &[u8; 16]) -> [u8; CHECKSUM_LEN] {
        let mut mac = HmacSha256::new_from_slice(b"stern-secret-key-checksum")
            .expect("HMAC can take key of any size");
        mac.update(key);
        let result = mac.finalize().into_bytes();
        let mut checksum = [0u8; CHECKSUM_LEN];
        checksum.copy_from_slice(&result[..CHECKSUM_LEN]);
        checksum
    }

    #[must_use]
    pub fn encode(&self) -> String {
        let mut data = Vec::with_capacity(16 + CHECKSUM_LEN);
        data.extend_from_slice(&self.key);
        data.extend_from_slice(&self.checksum);
        let encoded = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &data);
        format!("SK1-{encoded}")
    }

    #[must_use]
    #[allow(clippy::indexing_slicing)]
    pub fn decode(s: &str) -> Option<Self> {
        let s = s.strip_prefix("SK1-")?;
        let data = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, s).ok()?;
        if data.len() != 16 + CHECKSUM_LEN {
            return None;
        }
        let mut key = [0u8; 16];
        let mut checksum = [0u8; CHECKSUM_LEN];
        key.copy_from_slice(&data[..16]);
        checksum.copy_from_slice(&data[16..]);

        let expected = Self::compute_checksum(&key);
        if checksum != expected {
            return None;
        }

        Some(Self { key, checksum })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_decode_roundtrip() {
        let key = [42u8; 16];
        let sk = SecretKey::new(key);
        let encoded = sk.encode();
        let decoded = SecretKey::decode(&encoded).expect("decode should succeed");
        assert_eq!(decoded.key, sk.key);
        assert_eq!(decoded.checksum, sk.checksum);
    }

    #[test]
    fn encode_has_prefix() {
        let sk = SecretKey::new([1u8; 16]);
        let encoded = sk.encode();
        assert!(encoded.starts_with("SK1-"), "encoded: {encoded}");
    }

    #[test]
    fn decode_wrong_prefix() {
        assert!(SecretKey::decode("XX1-abc").is_none());
    }

    #[test]
    fn decode_no_prefix() {
        assert!(SecretKey::decode("abc").is_none());
    }

    #[test]
    fn decode_truncated_data() {
        let short = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &[0u8; 5]);
        assert!(SecretKey::decode(&format!("SK1-{short}")).is_none());
    }

    #[test]
    fn decode_too_long_data() {
        let long = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &[0u8; 30]);
        assert!(SecretKey::decode(&format!("SK1-{long}")).is_none());
    }

    #[test]
    fn decode_wrong_checksum() {
        let key = [99u8; 16];
        let mut data = Vec::with_capacity(16 + CHECKSUM_LEN);
        data.extend_from_slice(&key);
        data.extend_from_slice(&[0xFF; CHECKSUM_LEN]);
        let encoded = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &data);
        assert!(SecretKey::decode(&format!("SK1-{encoded}")).is_none());
    }

    #[test]
    fn new_computes_checksum() {
        let key = [1u8; 16];
        let sk = SecretKey::new(key);
        let expected = SecretKey::compute_checksum(&key);
        assert_eq!(sk.checksum, expected);
    }

    #[test]
    fn different_keys_different_checksums() {
        let sk1 = SecretKey::new([1u8; 16]);
        let sk2 = SecretKey::new([2u8; 16]);
        assert_ne!(sk1.checksum, sk2.checksum);
    }

    #[test]
    fn decode_empty_string() {
        assert!(SecretKey::decode("").is_none());
    }

    #[test]
    fn decode_sk1_only() {
        assert!(SecretKey::decode("SK1-").is_none());
    }

    #[test]
    fn encode_decode_various_keys() {
        for seed in [0u8, 127, 255] {
            let key = [seed; 16];
            let sk = SecretKey::new(key);
            let encoded = sk.encode();
            let decoded = SecretKey::decode(&encoded).expect("roundtrip should work");
            assert_eq!(decoded.key, key);
        }
    }
}
