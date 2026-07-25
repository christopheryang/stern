use crate::domain::vault::crypto_constants::{KEK_LEN, VERIFY_HASH_LEN};
use crate::infrastructure::crypto::secret_mem::SecretMem;
use zeroize::Zeroize;

pub struct VaultSession {
    kek: SecretMem<[u8; KEK_LEN]>,
    verify_hash: [u8; VERIFY_HASH_LEN],
    pub is_unlocked: bool,
}

impl VaultSession {
    pub fn new() -> Self {
        Self {
            kek: SecretMem::new([0u8; KEK_LEN]),
            verify_hash: [0u8; VERIFY_HASH_LEN],
            is_unlocked: false,
        }
    }

    pub fn unlock(
        &mut self,
        kek: [u8; KEK_LEN],
        verify_hash: [u8; VERIFY_HASH_LEN],
    ) {
        *self.kek.get_mut() = kek;
        self.verify_hash = verify_hash;
        self.is_unlocked = true;
    }

    pub fn kek(&self) -> Result<&[u8; KEK_LEN], crate::domain::vault::errors::VaultError> {
        if !self.is_unlocked {
            return Err(crate::domain::vault::errors::VaultError::VaultLocked);
        }
        Ok(self.kek.get())
    }

    pub fn verify_hash(&self) -> &[u8; VERIFY_HASH_LEN] {
        &self.verify_hash
    }

    pub fn lock(&mut self) {
        self.kek.get_mut().zeroize();
        self.verify_hash.zeroize();
        self.is_unlocked = false;
    }
}

impl Default for VaultSession {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for VaultSession {
    fn drop(&mut self) {
        self.lock();
    }
}
