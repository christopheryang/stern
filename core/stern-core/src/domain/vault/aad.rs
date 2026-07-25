#[must_use]
pub fn entry_aad(entry_id: &str, version: u64) -> Vec<u8> {
    let mut aad = entry_id.as_bytes().to_vec();
    aad.extend_from_slice(&version.to_le_bytes());
    aad
}

#[must_use]
pub fn tag_aad(tag_id: &str) -> Vec<u8> {
    tag_id.as_bytes().to_vec()
}

#[must_use]
pub fn blob_aad(entry_id: &str) -> Vec<u8> {
    use crate::domain::vault::crypto_constants::BLOB_AAD_SUFFIX;
    let mut aad = entry_id.as_bytes().to_vec();
    aad.extend_from_slice(BLOB_AAD_SUFFIX);
    aad
}

