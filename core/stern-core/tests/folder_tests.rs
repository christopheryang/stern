#![allow(clippy::unwrap_used)]

use stern_core::domain::vault::entry::{EncryptedEntry, EntryKind};
use stern_core::domain::vault::folder::{
    check_placement, depth_of, path_label, render_folder, render_tree, resolve, sibling_exists,
    split_path, validate_name, EncryptedFolder, FolderLink, FolderNode, Resolution, TreeItem,
    MAX_FOLDER_DEPTH,
};
use stern_core::infrastructure::sqlite::repository::SqliteRepository;

fn node(id: &str, parent: Option<&str>, name: &str) -> FolderNode {
    FolderNode {
        id: id.into(),
        parent: parent.map(Into::into),
        name: name.into(),
    }
}

fn links(nodes: &[FolderNode]) -> Vec<FolderLink<'_>> {
    nodes
        .iter()
        .map(|n| FolderLink {
            id: &n.id,
            parent: n.parent.as_deref(),
        })
        .collect()
}

fn sample() -> Vec<FolderNode> {
    vec![
        node("a", None, "Taxes"),
        node("b", Some("a"), "2024"),
        node("c", None, "Home"),
        node("d", Some("c"), "2024"),
    ]
}

#[test]
fn depth_counts_from_one() {
    let nodes = sample();
    assert_eq!(depth_of(&links(&nodes), "a"), Some(1));
    assert_eq!(depth_of(&links(&nodes), "b"), Some(2));
    assert_eq!(depth_of(&links(&nodes), "nope"), None);
}

#[test]
fn a_loop_has_no_depth() {
    let nodes = vec![node("x", Some("y"), "X"), node("y", Some("x"), "Y")];
    assert_eq!(depth_of(&links(&nodes), "x"), None);
}

#[test]
fn a_folder_cannot_go_inside_itself_or_its_own_child() {
    let nodes = sample();
    let l = links(&nodes);
    assert!(check_placement(&l, "a", Some("a")).is_err());
    assert!(check_placement(&l, "a", Some("b")).is_err());
    assert!(check_placement(&l, "b", Some("c")).is_ok());
    assert!(check_placement(&l, "b", None).is_ok());
}

#[test]
fn a_move_cannot_make_the_tree_too_deep() {
    // A chain of MAX_FOLDER_DEPTH folders, and one more folder with a child.
    let mut nodes: Vec<FolderNode> = Vec::new();
    for i in 0..MAX_FOLDER_DEPTH {
        let parent = (i > 0).then(|| format!("f{}", i - 1));
        nodes.push(node(&format!("f{i}"), parent.as_deref(), "x"));
    }
    nodes.push(node("top", None, "Top"));
    nodes.push(node("kid", Some("top"), "Kid"));
    let l = links(&nodes);
    let deepest = format!("f{}", MAX_FOLDER_DEPTH - 1);
    // Top (with its child) under the deepest folder would pass the limit.
    assert!(check_placement(&l, "top", Some(&deepest)).is_err());
    // A single folder under the second-deepest still fits.
    assert!(check_placement(&l, "kid", Some(&format!("f{}", MAX_FOLDER_DEPTH - 2))).is_ok());
}

#[test]
fn names_are_checked() {
    assert_eq!(validate_name("  Taxes ").unwrap(), "Taxes");
    assert!(validate_name("   ").is_err());
    assert!(validate_name("a/b").is_err());
    assert!(validate_name("a > b").is_err());
    assert!(validate_name(&"x".repeat(61)).is_err());
    assert!(validate_name("line\nbreak").is_err());
}

#[test]
fn paths_split_on_arrows_and_slashes() {
    assert_eq!(split_path("Taxes > 2024"), vec!["Taxes", "2024"]);
    assert_eq!(split_path("Taxes/2024"), vec!["Taxes", "2024"]);
    assert!(split_path("  ").is_empty());
}

#[test]
fn resolve_by_name_path_and_ambiguity() {
    let nodes = sample();
    assert_eq!(resolve(&nodes, "taxes"), Resolution::One("a".into()));
    assert_eq!(resolve(&nodes, "Taxes > 2024"), Resolution::One("b".into()));
    assert_eq!(resolve(&nodes, "Home/2024"), Resolution::One("d".into()));
    match resolve(&nodes, "2024") {
        Resolution::Many(paths) => {
            assert_eq!(paths, vec!["Taxes > 2024", "Home > 2024"]);
        }
        other => panic!("expected Many, got {other:?}"),
    }
    assert_eq!(resolve(&nodes, "Nowhere"), Resolution::None);
    // A part of a name still finds a folder when nothing matches exactly.
    assert_eq!(resolve(&nodes, "tax"), Resolution::One("a".into()));
    assert_eq!(path_label(&nodes, "d"), "Home > 2024");
}

#[test]
fn siblings_clash_only_within_one_parent() {
    let nodes = sample();
    assert!(sibling_exists(&nodes, None, "taxes", None));
    assert!(!sibling_exists(&nodes, None, "taxes", Some("a")));
    assert!(sibling_exists(&nodes, Some("a"), "2024", None));
    assert!(!sibling_exists(&nodes, Some("a"), "2025", None));
}

fn item(id: &str, name: &str, kind: &'static str, folder: Option<&str>) -> TreeItem {
    TreeItem {
        id: id.into(),
        name: name.into(),
        kind,
        folder: folder.map(Into::into),
    }
}

#[test]
fn the_tree_shows_names_and_kinds_in_order() {
    let nodes = sample();
    let items = vec![
        item("1", "Return", "document", Some("b")),
        item("2", "Lease", "document", Some("c")),
        item("3", "Netflix", "login", None),
    ];
    let tree = render_tree(&nodes, &items);
    let lines: Vec<&str> = tree.lines().collect();
    assert_eq!(
        lines,
        vec![
            "📁 Home",
            "    📁 2024",
            "    • Lease (document)",
            "📁 Taxes",
            "    📁 2024",
            "        • Return (document)",
            "Not in a folder",
            "    • Netflix (login)",
        ]
    );
}

#[test]
fn one_folder_shows_only_what_is_inside_it() {
    let nodes = sample();
    let items = vec![
        item("1", "Return", "document", Some("b")),
        item("2", "Lease", "document", Some("c")),
    ];
    let inside = render_folder(&nodes, &items, "a");
    assert!(inside.contains("2024"));
    assert!(inside.contains("Return"));
    assert!(!inside.contains("Lease"));
}

#[test]
fn a_loop_in_the_data_cannot_hang_the_tree() {
    let nodes = vec![node("x", Some("y"), "X"), node("y", Some("x"), "Y")];
    let items = vec![item("1", "Lost", "note", Some("x"))];
    // Neither is a top-level folder, so nothing is drawn, and it returns.
    let tree = render_tree(&nodes, &items);
    assert!(!tree.contains('📁'));
}

fn temp_repo() -> (SqliteRepository, tempfile::TempPath) {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let path = tmp.into_temp_path();
    (SqliteRepository::new(&path).unwrap(), path)
}

fn folder(id: &str, parent: Option<&str>) -> EncryptedFolder {
    let now = chrono::Utc::now();
    EncryptedFolder {
        id: id.into(),
        parent_id: parent.map(Into::into),
        dek_wrapped: vec![1; 40],
        nonce: [2; 24],
        ciphertext: vec![3, 4, 5],
        created_at: now,
        updated_at: now,
    }
}

fn entry(id: &str, name: &str) -> EncryptedEntry {
    let now = chrono::Utc::now();
    EncryptedEntry {
        id: id.into(),
        kind: EntryKind::Note,
        name: name.into(),
        tags: vec![],
        dek_wrapped: vec![1; 40],
        nonce: [2; 24],
        ciphertext: vec![9],
        version: 1,
        created_at: now,
        updated_at: now,
    }
}

#[test]
fn folders_round_trip_through_the_database() {
    let (repo, _p) = temp_repo();
    repo.insert_folder(&folder("a", None)).unwrap();
    repo.insert_folder(&folder("b", Some("a"))).unwrap();
    let all = repo.list_folders().unwrap();
    assert_eq!(all.len(), 2);
    let b = all.iter().find(|f| f.id == "b").unwrap();
    assert_eq!(b.parent_id.as_deref(), Some("a"));
    assert_eq!(b.ciphertext, vec![3, 4, 5]);
}

#[test]
fn a_move_that_would_loop_is_refused() {
    let (repo, _p) = temp_repo();
    repo.insert_folder(&folder("a", None)).unwrap();
    repo.insert_folder(&folder("b", Some("a"))).unwrap();
    repo.insert_folder(&folder("c", Some("b"))).unwrap();
    assert!(!repo.move_folder("a", Some("c")).unwrap());
    assert!(!repo.move_folder("a", Some("a")).unwrap());
    assert!(repo.move_folder("c", None).unwrap());
    let c = repo
        .list_folders()
        .unwrap()
        .into_iter()
        .find(|f| f.id == "c")
        .unwrap();
    assert_eq!(c.parent_id, None);
}

#[test]
fn entries_can_be_filed_and_unfiled() {
    let (repo, _p) = temp_repo();
    repo.insert_folder(&folder("a", None)).unwrap();
    repo.insert_entry(&entry("e1", "Groceries")).unwrap();
    assert!(repo.set_entry_folder("e1", Some("a")).unwrap());
    assert_eq!(
        repo.entry_folders().unwrap(),
        vec![("e1".to_string(), "a".to_string())]
    );
    assert!(repo.set_entry_folder("e1", None).unwrap());
    assert!(repo.entry_folders().unwrap().is_empty());
    // An unknown folder or entry is refused, not silently ignored.
    assert!(!repo.set_entry_folder("e1", Some("nope")).unwrap());
    assert!(!repo.set_entry_folder("missing", Some("a")).unwrap());
}

#[test]
fn deleting_a_folder_keeps_what_was_inside_one_level_up() {
    let (repo, _p) = temp_repo();
    repo.insert_folder(&folder("top", None)).unwrap();
    repo.insert_folder(&folder("mid", Some("top"))).unwrap();
    repo.insert_folder(&folder("low", Some("mid"))).unwrap();
    repo.insert_entry(&entry("e1", "Groceries")).unwrap();
    repo.set_entry_folder("e1", Some("mid")).unwrap();

    assert!(repo.delete_folder("mid").unwrap());

    let folders = repo.list_folders().unwrap();
    assert_eq!(folders.len(), 2);
    let low = folders.iter().find(|f| f.id == "low").unwrap();
    assert_eq!(low.parent_id.as_deref(), Some("top"));
    assert_eq!(
        repo.entry_folders().unwrap(),
        vec![("e1".to_string(), "top".to_string())]
    );
    assert_eq!(repo.count_entries().unwrap(), 1);
    assert!(!repo.delete_folder("mid").unwrap());
}

#[test]
fn deleting_a_top_level_folder_unfiles_its_entries() {
    let (repo, _p) = temp_repo();
    repo.insert_folder(&folder("a", None)).unwrap();
    repo.insert_entry(&entry("e1", "Groceries")).unwrap();
    repo.set_entry_folder("e1", Some("a")).unwrap();
    assert!(repo.delete_folder("a").unwrap());
    assert!(repo.entry_folders().unwrap().is_empty());
    assert_eq!(repo.count_entries().unwrap(), 1);
}

#[test]
fn an_old_vault_gains_folders_without_losing_entries() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let path = tmp.into_temp_path();
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE entries (
                 id TEXT PRIMARY KEY, kind TEXT NOT NULL, name TEXT NOT NULL, tags TEXT,
                 dek_wrapped BLOB NOT NULL, nonce BLOB NOT NULL, ciphertext BLOB NOT NULL,
                 version INTEGER NOT NULL DEFAULT 1, created_at TEXT NOT NULL,
                 updated_at TEXT NOT NULL);
             INSERT INTO entries VALUES ('old','note','Old note','[]',x'00',x'00',x'00',1,
                 '2026-01-01T00:00:00Z','2026-01-01T00:00:00Z');",
        )
        .unwrap();
    }
    let repo = SqliteRepository::new(&path).unwrap();
    // Opening it twice must not try to add the column again.
    drop(repo);
    let repo = SqliteRepository::new(&path).unwrap();
    assert_eq!(repo.count_entries().unwrap(), 1);
    repo.insert_folder(&folder("a", None)).unwrap();
    assert!(repo.set_entry_folder("old", Some("a")).unwrap());
}
