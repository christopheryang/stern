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

