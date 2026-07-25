use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexEntry {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub tags: Vec<String>,
    pub version: u64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct VaultIndex {
    pub entries: IndexMap<String, IndexEntry>,
}

impl VaultIndex {
    pub fn insert(&mut self, entry: IndexEntry) {
        self.entries.insert(entry.id.clone(), entry);
    }

    pub fn shift_remove(&mut self, id: &str) -> Option<IndexEntry> {
        self.entries.shift_remove(id)
    }

    #[must_use]
    pub fn get(&self, id: &str) -> Option<&IndexEntry> {
        self.entries.get(id)
    }

    #[must_use]
    pub fn search(&self, query: &str) -> Vec<&IndexEntry> {
        let q = query.to_lowercase();
        self.entries
            .values()
            .filter(|e| {
                e.name.to_lowercase().contains(&q)
                    || e.tags.iter().any(|t| t.to_lowercase().contains(&q))
            })
            .collect()
    }
}
