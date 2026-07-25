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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(id: &str, name: &str, tags: Vec<&str>) -> IndexEntry {
        IndexEntry {
            id: id.to_string(),
            name: name.to_string(),
            kind: "login".to_string(),
            tags: tags.into_iter().map(str::to_string).collect(),
            version: 1,
            created_at: "2024-01-01T00:00:00Z".to_string(),
            updated_at: "2024-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn insert_and_get() {
        let mut idx = VaultIndex::default();
        let entry = make_entry("a1", "GitHub", vec!["dev"]);
        idx.insert(entry);
        let got = idx.get("a1").expect("entry should exist");
        assert_eq!(got.name, "GitHub");
        assert_eq!(got.tags, vec!["dev"]);
    }

    #[test]
    fn get_missing_returns_none() {
        let idx = VaultIndex::default();
        assert!(idx.get("nope").is_none());
    }

    #[test]
    fn shift_remove_existing() {
        let mut idx = VaultIndex::default();
        idx.insert(make_entry("a1", "GitHub", vec![]));
        let removed = idx.shift_remove("a1").expect("should remove");
        assert_eq!(removed.id, "a1");
        assert!(idx.get("a1").is_none());
    }

    #[test]
    fn shift_remove_missing_returns_none() {
        let mut idx = VaultIndex::default();
        assert!(idx.shift_remove("nope").is_none());
    }

    #[test]
    fn search_by_name_case_insensitive() {
        let mut idx = VaultIndex::default();
        idx.insert(make_entry("a1", "GitHub", vec![]));
        idx.insert(make_entry("a2", "GitLab", vec![]));
        idx.insert(make_entry("a3", "Netflix", vec![]));

        let results = idx.search("git");
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn search_by_tag() {
        let mut idx = VaultIndex::default();
        idx.insert(make_entry("a1", "GitHub", vec!["work", "dev"]));
        idx.insert(make_entry("a2", "Netflix", vec!["personal"]));

        let results = idx.search("work");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "a1");
    }

    #[test]
    fn search_by_tag_case_insensitive() {
        let mut idx = VaultIndex::default();
        idx.insert(make_entry("a1", "GitHub", vec!["Work"]));
        let results = idx.search("work");
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn search_no_results() {
        let mut idx = VaultIndex::default();
        idx.insert(make_entry("a1", "GitHub", vec![]));
        assert!(idx.search("facebook").is_empty());
    }

    #[test]
    fn search_empty_index() {
        let idx = VaultIndex::default();
        assert!(idx.search("anything").is_empty());
    }

    #[test]
    fn serde_roundtrip() {
        let mut idx = VaultIndex::default();
        idx.insert(make_entry("a1", "GitHub", vec!["dev"]));
        idx.insert(make_entry("a2", "Netflix", vec!["personal"]));

        let json = serde_json::to_string(&idx).expect("serialize VaultIndex");
        let back: VaultIndex = serde_json::from_str(&json).expect("deserialize VaultIndex");
        assert_eq!(back.entries.len(), 2);
        assert!(back.get("a1").is_some());
        assert!(back.get("a2").is_some());
    }

    #[test]
    fn insert_overwrites_same_id() {
        let mut idx = VaultIndex::default();
        idx.insert(make_entry("a1", "GitHub v1", vec![]));
        idx.insert(make_entry("a1", "GitHub v2", vec![]));
        let got = idx.get("a1").expect("entry should exist");
        assert_eq!(got.name, "GitHub v2");
    }

    #[test]
    fn default_is_empty() {
        let idx = VaultIndex::default();
        assert!(idx.entries.is_empty());
    }
}
