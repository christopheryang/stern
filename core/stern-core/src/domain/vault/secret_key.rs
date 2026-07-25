use hmac::{Hmac, Mac};
use hmac::digest::KeyInit;
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

const CHECKSUM_LEN: usize = 4;

#[derive(Debug, Clone)]
pub struct SecretKey {
    pub key: [u8; 16],
    pub checksum: [u8; CHECKSUM_LEN],
}

impl SecretKey {
    pub fn new(key: [u8; 16]) -> Self {
        let checksum = Self::compute_checksum(&key);
        Self { key, checksum }
    }

    fn compute_checksum(key: &[u8; 16]) -> [u8; CHECKSUM_LEN] {
        let mut mac = HmacSha256::new_from_slice(b"stern-secret-key-checksum")
            .expect("HMAC can take key of any size");
        mac.update(key);
        let result = mac.finalize().into_bytes();
        let mut checksum = [0u8; CHECKSUM_LEN];
        checksum.copy_from_slice(&result[..CHECKSUM_LEN]);
        checksum
    }

    pub fn encode(&self) -> String {
        let mut data = Vec::with_capacity(16 + CHECKSUM_LEN);
        data.extend_from_slice(&self.key);
        data.extend_from_slice(&self.checksum);
        let encoded = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            &data,
        );
        format!("SK1-{}", encoded)
    }

    pub fn decode(s: &str) -> Option<Self> {
        let s = s.strip_prefix("SK1-")?;
        let data = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            s,
        ).ok()?;
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
