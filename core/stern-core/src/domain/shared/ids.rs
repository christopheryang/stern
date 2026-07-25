use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EntryId(pub String);

impl EntryId {
    #[must_use]
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }
}

impl Default for EntryId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for EntryId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TagId(pub String);

impl TagId {
    #[must_use]
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }
}

impl Default for TagId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct VaultId(pub String);

impl VaultId {
    #[must_use]
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }
}

impl Default for VaultId {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_id_new_non_empty() {
        let id = EntryId::new();
        assert!(!id.0.is_empty(), "EntryId should be non-empty");
    }

    #[test]
    fn entry_id_default_non_empty() {
        let id = EntryId::default();
        assert!(!id.0.is_empty());
    }

    #[test]
    fn entry_id_unique() {
        let a = EntryId::new();
        let b = EntryId::new();
        assert_ne!(a.0, b.0, "two EntryIds should be different");
    }

    #[test]
    fn entry_id_display() {
        let id = EntryId("test-123".to_string());
        assert_eq!(format!("{id}"), "test-123");
    }

    #[test]
    fn entry_id_serde_roundtrip() {
        let id = EntryId::new();
        let json = serde_json::to_string(&id).expect("serialize");
        let back: EntryId = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(id, back);
    }

    #[test]
    fn tag_id_new_non_empty() {
        let id = TagId::new();
        assert!(!id.0.is_empty());
    }

    #[test]
    fn tag_id_unique() {
        let a = TagId::new();
        let b = TagId::new();
        assert_ne!(a.0, b.0);
    }

    #[test]
    fn tag_id_serde_roundtrip() {
        let id = TagId::new();
        let json = serde_json::to_string(&id).expect("serialize");
        let back: TagId = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(id, back);
    }

    #[test]
    fn vault_id_new_non_empty() {
        let id = VaultId::new();
        assert!(!id.0.is_empty());
    }

    #[test]
    fn vault_id_unique() {
        let a = VaultId::new();
        let b = VaultId::new();
        assert_ne!(a.0, b.0);
    }

    #[test]
    fn vault_id_serde_roundtrip() {
        let id = VaultId::new();
        let json = serde_json::to_string(&id).expect("serialize");
        let back: VaultId = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(id, back);
    }

    #[test]
    fn ids_are_valid_uuid_format() {
        let id = EntryId::new();
        assert_eq!(id.0.len(), 36, "UUID v4 should be 36 chars: {}", id.0);
        assert_eq!(id.0.chars().filter(|c| *c == '-').count(), 4);
    }
}
