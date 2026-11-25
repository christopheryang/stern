//! Folders: a tree that organises entries without exposing them.
//!
//! A folder is a name and a place in the tree. Its name is encrypted like
//! any other vault text; the tree shape (which folder is inside which) and
//! which entry sits in which folder are plain structure, kept beside the
//! ciphertext the way an entry's own name is. Nothing here touches a secret
//! value, so showing the tree never shows one.

use crate::domain::vault::crypto_constants::NONCE_LEN;
use crate::domain::vault::errors::VaultError;

/// The deepest a folder may sit: a top-level folder is depth 1.
pub const MAX_FOLDER_DEPTH: usize = 8;

/// The longest folder name, in characters.
pub const MAX_FOLDER_NAME_CHARS: usize = 60;

/// A folder as stored: its name encrypted under its own data key.
#[derive(Debug, Clone)]
pub struct EncryptedFolder {
    pub id: String,
    /// The folder this one is inside, or `None` for a top-level folder.
    pub parent_id: Option<String>,
    pub dek_wrapped: Vec<u8>,
    pub nonce: [u8; NONCE_LEN],
    pub ciphertext: Vec<u8>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// One folder's place in the tree.
#[derive(Debug, Clone, Copy)]
pub struct FolderLink<'a> {
    pub id: &'a str,
    pub parent: Option<&'a str>,
}

fn link_of<'a, 'b>(links: &'b [FolderLink<'a>], id: &str) -> Option<&'b FolderLink<'a>> {
    links.iter().find(|link| link.id == id)
}

/// How deep a folder sits (a top-level folder is 1). `None` when the folder
/// is unknown or its parents loop back on themselves.
#[must_use]
pub fn depth_of(links: &[FolderLink<'_>], id: &str) -> Option<usize> {
    let mut depth = 1usize;
    let mut current = link_of(links, id)?.parent;
    while let Some(parent) = current {
        depth = depth.saturating_add(1);
        if depth > links.len().saturating_add(1) {
            return None;
        }
        current = link_of(links, parent)?.parent;
    }
    Some(depth)
}

/// Whether `candidate` is `ancestor` itself or sits somewhere inside it.
#[must_use]
pub fn is_within(links: &[FolderLink<'_>], candidate: &str, ancestor: &str) -> bool {
    let mut current = Some(candidate);
    let mut steps = 0usize;
    while let Some(id) = current {
        if id == ancestor {
            return true;
        }
        steps = steps.saturating_add(1);
        if steps > links.len() {
            return false;
        }
        current = link_of(links, id).and_then(|link| link.parent);
    }
    false
}

/// How many levels a folder and everything inside it span (a folder with
/// nothing inside is 1).
#[must_use]
pub fn height_of(links: &[FolderLink<'_>], id: &str) -> usize {
    fn walk(links: &[FolderLink<'_>], id: &str, budget: usize) -> usize {
        if budget == 0 {
            return 0;
        }
        let deepest_child = links
            .iter()
            .filter(|link| link.parent == Some(id))
            .map(|link| walk(links, link.id, budget.saturating_sub(1)))
            .max()
            .unwrap_or(0);
        deepest_child.saturating_add(1)
    }
    walk(links, id, MAX_FOLDER_DEPTH.saturating_add(1))
}

/// Check that a folder may be placed under `new_parent` (`None` for the top
/// level): not inside itself, and not so deep that the tree passes
/// [`MAX_FOLDER_DEPTH`].
pub fn check_placement(
    links: &[FolderLink<'_>],
    id: &str,
    new_parent: Option<&str>,
) -> Result<(), VaultError> {
    let parent_depth = match new_parent {
        None => 0,
        Some(parent) => {
            if is_within(links, parent, id) {
                return Err(VaultError::Io(
                    "A folder can't go inside itself.".to_string(),
                ));
            }
            depth_of(links, parent)
                .ok_or_else(|| VaultError::Io("That folder isn't there any more.".to_string()))?
        }
    };
    if parent_depth.saturating_add(height_of(links, id)) > MAX_FOLDER_DEPTH {
        return Err(VaultError::Io(format!(
            "Folders can only go {MAX_FOLDER_DEPTH} levels deep."
        )));
    }
    Ok(())
}

/// A folder name ready to store: trimmed, not empty, not too long, and
/// free of the characters a folder path uses to separate names.
pub fn validate_name(name: &str) -> Result<String, VaultError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(VaultError::Io("A folder needs a name.".to_string()));
    }
    if name.chars().count() > MAX_FOLDER_NAME_CHARS {
        return Err(VaultError::Io(format!(
            "Folder names can be up to {MAX_FOLDER_NAME_CHARS} characters."
        )));
    }
    if name.chars().any(|c| c.is_control() || c == '/' || c == '>') {
        return Err(VaultError::Io(
            "A folder name can't contain / or >.".to_string(),
        ));
    }
    Ok(name.to_string())
}

/// A folder with its name in the clear, for building and showing the tree.
/// Only the shell makes these, after decrypting a folder's name.
#[derive(Debug, Clone)]
pub struct FolderNode {
    pub id: String,
    pub parent: Option<String>,
    pub name: String,
}

/// An entry as the tree shows it: its name and kind, never its contents.
#[derive(Debug, Clone)]
pub struct TreeItem {
    pub id: String,
    pub name: String,
    pub kind: &'static str,
    pub folder: Option<String>,
    /// A stored file that is not text, kept from an earlier version.
    pub older_file: bool,
}

/// What a folder reference in a request points at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    /// No folder has that name.
    None,
    /// Exactly one folder.
    One(String),
    /// Several folders share the name; each is given by its full path so
    /// the user can say which.
    Many(Vec<String>),
}

/// The separators a folder path may use in a request: "Taxes > 2024" and
/// "Taxes/2024" mean the same.
#[must_use]
pub fn split_path(reference: &str) -> Vec<String> {
    reference
        .split(['>', '/'])
        .map(|part| part.trim().to_string())
        .filter(|part| !part.is_empty())
        .collect()
}

/// The names from the top-level folder down to this one.
#[must_use]
pub fn path_segments(nodes: &[FolderNode], id: &str) -> Vec<String> {
    let mut segments = Vec::new();
    let mut current = Some(id);
    while let Some(folder_id) = current {
        let Some(node) = nodes.iter().find(|node| node.id == folder_id) else {
            break;
        };
        segments.push(node.name.clone());
        if segments.len() > nodes.len() {
            break;
        }
        current = node.parent.as_deref();
    }
    segments.reverse();
    segments
}

/// A folder's full path, for showing: "Taxes > 2024".
#[must_use]
pub fn path_label(nodes: &[FolderNode], id: &str) -> String {
    path_segments(nodes, id).join(" > ")
}

/// Find the folder a request names.
///
/// A path ("Taxes > 2024") matches the folders whose path ends that way; a
/// bare name matches every folder with that name (ignoring case), and when
/// none is named exactly, the folders whose name contains it.
#[must_use]
pub fn resolve(nodes: &[FolderNode], reference: &str) -> Resolution {
    let wanted: Vec<String> = split_path(reference)
        .iter()
        .map(|part| part.to_lowercase())
        .collect();
    let Some(last) = wanted.last() else {
        return Resolution::None;
    };
    let ends_with_path = |id: &str| {
        let have: Vec<String> = path_segments(nodes, id)
            .iter()
            .map(|part| part.to_lowercase())
            .collect();
        have.len() >= wanted.len()
            && have
                .iter()
                .rev()
                .zip(wanted.iter().rev())
                .all(|(have, want)| have == want)
    };
    let mut hits: Vec<&FolderNode> = nodes.iter().filter(|n| ends_with_path(&n.id)).collect();
    if hits.is_empty() && wanted.len() == 1 {
        hits = nodes
            .iter()
            .filter(|n| n.name.to_lowercase().contains(last.as_str()))
            .collect();
    }
    match hits.as_slice() {
        [] => Resolution::None,
        [one] => Resolution::One(one.id.clone()),
        many => Resolution::Many(many.iter().map(|n| path_label(nodes, &n.id)).collect()),
    }
}

/// Whether a folder already holds a sub-folder with this name (ignoring
/// case). `parent` of `None` means the top level; `except` skips one
/// folder, so renaming a folder to its own name is not a clash.
#[must_use]
pub fn sibling_exists(
    nodes: &[FolderNode],
    parent: Option<&str>,
    name: &str,
    except: Option<&str>,
) -> bool {
    let wanted = name.trim().to_lowercase();
    nodes.iter().any(|node| {
        node.parent.as_deref() == parent
            && Some(node.id.as_str()) != except
            && node.name.to_lowercase() == wanted
    })
}

/// The most lines a rendered tree shows before it says how many more.
pub const MAX_TREE_LINES: usize = 200;

/// Draw the folders and what is filed in them as indented text: names and
/// kinds only, never a secret's value. Entries in no folder come last.
#[must_use]
pub fn render_tree(nodes: &[FolderNode], items: &[TreeItem]) -> String {
    let mut lines: Vec<String> = Vec::new();
    let mut seen: Vec<&str> = Vec::new();
    draw_level(nodes, items, None, 0, &mut seen, &mut lines);
    let loose: Vec<&TreeItem> = items
        .iter()
        .filter(|item| {
            item.folder.is_none() || !nodes.iter().any(|n| Some(&n.id) == item.folder.as_ref())
        })
        .collect();
    if !loose.is_empty() && !nodes.is_empty() {
        lines.push("Not in a folder".to_string());
    }
    let indent = usize::from(!nodes.is_empty());
    for item in loose {
        lines.push(item_line(item, indent));
    }
    cap_lines(lines)
}

/// What is directly inside one folder: its sub-folders and entries.
#[must_use]
pub fn render_folder(nodes: &[FolderNode], items: &[TreeItem], id: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    let mut seen: Vec<&str> = Vec::new();
    draw_level(nodes, items, Some(id), 0, &mut seen, &mut lines);
    cap_lines(lines)
}

fn item_line(item: &TreeItem, depth: usize) -> String {
    let kind = if item.older_file {
        format!("{}, older file", item.kind)
    } else {
        item.kind.to_string()
    };
    format!("{}• {} ({kind})", "    ".repeat(depth), item.name)
}

fn draw_level<'a>(
    nodes: &'a [FolderNode],
    items: &[TreeItem],
    parent: Option<&str>,
    depth: usize,
    seen: &mut Vec<&'a str>,
    lines: &mut Vec<String>,
) {
    if depth > MAX_FOLDER_DEPTH || lines.len() > MAX_TREE_LINES {
        return;
    }
    let mut here: Vec<&FolderNode> = nodes
        .iter()
        .filter(|node| node.parent.as_deref() == parent)
        .collect();
    here.sort_by_key(|node| node.name.to_lowercase());
    for node in here {
        if seen.contains(&node.id.as_str()) {
            continue;
        }
        seen.push(node.id.as_str());
        lines.push(format!("{}📁 {}", "    ".repeat(depth), node.name));
        draw_level(
            nodes,
            items,
            Some(&node.id),
            depth.saturating_add(1),
            seen,
            lines,
        );
    }
    let mut inside: Vec<&TreeItem> = items
        .iter()
        .filter(|item| item.folder.as_deref() == parent && parent.is_some())
        .collect();
    inside.sort_by_key(|item| item.name.to_lowercase());
    for item in inside {
        lines.push(item_line(item, depth));
    }
}

fn cap_lines(mut lines: Vec<String>) -> String {
    if lines.len() > MAX_TREE_LINES {
        let more = lines.len().saturating_sub(MAX_TREE_LINES);
        lines.truncate(MAX_TREE_LINES);
        lines.push(format!("…and {more} more"));
    }
    lines.join("\n")
}
