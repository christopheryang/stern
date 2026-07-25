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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::vault::crypto_constants::BLOB_AAD_SUFFIX;

    #[test]
    fn entry_aad_length() {
        let id = "test-id-123";
        let aad = entry_aad(id, 1);
        assert_eq!(aad.len(), id.len() + 8);
    }

    #[test]
    fn entry_aad_content() {
        let id = "abc";
        let aad = entry_aad(id, 42);
        assert_eq!(&aad[..3], b"abc");
        assert_eq!(&aad[3..], &42_u64.to_le_bytes());
    }

    #[test]
    fn entry_aad_version_zero() {
        let id = "x";
        let aad = entry_aad(id, 0);
        assert_eq!(aad, b"x\0\0\0\0\0\0\0\0");
    }

    #[test]
    fn tag_aad_is_just_bytes() {
        let tag = "my-tag";
        let aad = tag_aad(tag);
        assert_eq!(aad, tag.as_bytes());
        assert_eq!(aad.len(), tag.len());
    }

    #[test]
    fn blob_aad_suffix() {
        let id = "entry1";
        let aad = blob_aad(id);
        let expected_len = id.len() + BLOB_AAD_SUFFIX.len();
        assert_eq!(aad.len(), expected_len);
        assert_eq!(&aad[..id.len()], id.as_bytes());
        assert_eq!(&aad[id.len()..], BLOB_AAD_SUFFIX);
    }

    #[test]
    fn entry_aad_different_versions_differ() {
        let a1 = entry_aad("id", 1);
        let a2 = entry_aad("id", 2);
        assert_ne!(a1, a2);
    }

    #[test]
    fn entry_aad_different_ids_differ() {
        let a1 = entry_aad("id1", 1);
        let a2 = entry_aad("id2", 1);
        assert_ne!(a1, a2);
    }

    #[test]
    fn blob_aad_empty_id() {
        let aad = blob_aad("");
        assert_eq!(&aad, BLOB_AAD_SUFFIX);
    }
}
