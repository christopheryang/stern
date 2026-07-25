use crate::domain::vault::errors::VaultError;

pub struct BackupProvider;

impl BackupProvider {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    pub fn create_backup(&self, data: &[u8], path: &std::path::Path) -> Result<(), VaultError> {
        use std::io::Write;
        let hash = blake3::hash(data);
        let mut file = std::fs::File::create(path).map_err(|e| VaultError::Io(e.to_string()))?;
        file.write_all(hash.as_bytes())
            .map_err(|e| VaultError::Io(e.to_string()))?;
        file.write_all(data)
            .map_err(|e| VaultError::Io(e.to_string()))?;
        Ok(())
    }
}

impl Default for BackupProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn create_backup_writes_hash_and_data() {
        let provider = BackupProvider::new();
        let data = b"hello, backup world!";
        let tmp = tempfile::NamedTempFile::new().expect("temp file");
        let path = tmp.into_temp_path();

        provider.create_backup(data, &path).expect("create_backup");

        let mut file = std::fs::File::open(&path).expect("open backup");
        let mut contents = Vec::new();
        file.read_to_end(&mut contents).expect("read");

        let expected_hash = blake3::hash(data);
        let hash_bytes = expected_hash.as_bytes();
        assert_eq!(&contents[..hash_bytes.len()], hash_bytes, "first 32 bytes should be blake3 hash");
        assert_eq!(&contents[hash_bytes.len()..], data, "rest should be original data");
    }

    #[test]
    fn create_backup_total_size() {
        let provider = BackupProvider::new();
        let data = b"test data for backup size check";
        let tmp = tempfile::NamedTempFile::new().expect("temp file");
        let path = tmp.into_temp_path();

        provider.create_backup(data, &path).expect("create_backup");

        let meta = std::fs::metadata(&path).expect("metadata");
        assert_eq!(meta.len(), 32 + data.len() as u64);
    }

    #[test]
    fn create_backup_empty_data() {
        let provider = BackupProvider::new();
        let data = b"";
        let tmp = tempfile::NamedTempFile::new().expect("temp file");
        let path = tmp.into_temp_path();

        provider.create_backup(data, &path).expect("create_backup");

        let contents = std::fs::read(&path).expect("read");
        assert_eq!(contents.len(), 32);
        let expected_hash = blake3::hash(data);
        assert_eq!(&contents, expected_hash.as_bytes());
    }

    #[test]
    fn default_impl() {
        let _p = BackupProvider::default();
    }
}
