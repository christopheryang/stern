// Copyright (c) 2026 Christopher Yang
// SPDX-License-Identifier: MIT

//! Organising the vault from chat: folders, listing, renaming, deleting.
//!
//! Everything here shows names and kinds, never a secret's value. Folder
//! names are encrypted; an entry's own name is stored beside its
//! ciphertext, as it is for the note list. Every function needs an
//! unlocked vault and fails with `VaultLocked` otherwise.

#![allow(clippy::unreachable)]

use crate::AppState;
use stern_core::ai::chat::{DELETE_NO, DELETE_YES};
use stern_core::application::vault::ports::crypto::CryptoProvider;
use stern_core::domain::vault::aad::{entry_aad, folder_aad};
use stern_core::domain::vault::crypto_constants::{DEK_WRAPPED_LEN, KEK_LEN};
use stern_core::domain::vault::entry::{EncryptedEntry, EntryKind, EntryPayload};
use stern_core::domain::vault::errors::VaultError;
use stern_core::domain::vault::folder::{
    check_placement, depth_of, path_label, render_folder, render_tree, resolve, sibling_exists,
    validate_name, EncryptedFolder, FolderLink, FolderNode, Resolution, TreeItem, MAX_FOLDER_DEPTH,
};
use stern_core::infrastructure::sqlite::repository::StoredPayload;
use zeroize::Zeroizing;

/// What the chat says back, with tappable choices when there are some.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    pub message: String,
    pub options: Vec<String>,
}

impl Reply {
    fn say(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            options: Vec::new(),
        }
    }
}

fn unlocked_kek(state: &AppState) -> Result<Zeroizing<[u8; KEK_LEN]>, VaultError> {
    let session = state
        .session
        .lock()
        .map_err(|e| VaultError::Database(e.to_string()))?;
    if !session.is_unlocked {
        return Err(VaultError::VaultLocked);
    }
    Ok(Zeroizing::new(*session.kek()?))
}

/// A folder with its name encrypted under a fresh data key.
fn seal_folder(
    state: &AppState,
    kek: &[u8; KEK_LEN],
    id: &str,
    parent_id: Option<String>,
    name: &str,
) -> Result<EncryptedFolder, VaultError> {
    let dek = state.crypto.generate_dek();
    let (nonce, ciphertext) = state
        .crypto
        .encrypt_entry(&dek, name.as_bytes(), &folder_aad(id))?;
    let now = chrono::Utc::now();
    Ok(EncryptedFolder {
        id: id.to_string(),
        parent_id,
        dek_wrapped: state.crypto.wrap_dek(&dek, kek)?.to_vec(),
        nonce,
        ciphertext,
        created_at: now,
        updated_at: now,
    })
}

fn decrypt_folder(
    state: &AppState,
    kek: &[u8; KEK_LEN],
    folder: &EncryptedFolder,
) -> Result<FolderNode, VaultError> {
    let wrapped = <[u8; DEK_WRAPPED_LEN]>::try_from(folder.dek_wrapped.as_slice())
        .map_err(|_| VaultError::DecryptionFailed)?;
    let dek = state.crypto.unwrap_dek(&wrapped, kek)?;
    let plain = state.crypto.decrypt_entry(
        &dek,
        &folder.nonce,
        &folder.ciphertext,
        &folder_aad(&folder.id),
    )?;
    let name = String::from_utf8(plain.to_vec()).map_err(|_| VaultError::DecryptionFailed)?;
    Ok(FolderNode {
        id: folder.id.clone(),
        parent: folder.parent_id.clone(),
        name,
    })
}

/// Decrypt a folder's name, for the tests that tamper with a stored one.
#[cfg(test)]
pub fn folder_name_for_test(
    state: &AppState,
    kek: &[u8; KEK_LEN],
    folder: &EncryptedFolder,
) -> Result<FolderNode, VaultError> {
    decrypt_folder(state, kek, folder)
}

/// Every folder with its name in the clear.
fn folder_nodes(state: &AppState, kek: &[u8; KEK_LEN]) -> Result<Vec<FolderNode>, VaultError> {
    state
        .repository
        .list_folders()?
        .iter()
        .map(|folder| decrypt_folder(state, kek, folder))
        .collect()
}

fn links(nodes: &[FolderNode]) -> Vec<FolderLink<'_>> {
    nodes
        .iter()
        .map(|node| FolderLink {
            id: &node.id,
            parent: node.parent.as_deref(),
        })
        .collect()
}

const fn kind_label(kind: EntryKind) -> &'static str {
    match kind {
        EntryKind::Login => "login",
        EntryKind::Note => "note",
        EntryKind::Document => "document",
    }
}

fn tree_items(state: &AppState, entries: &[EncryptedEntry]) -> Result<Vec<TreeItem>, VaultError> {
    let filed = state.repository.entry_folders()?;
    Ok(entries
        .iter()
        .map(|entry| TreeItem {
            id: entry.id.clone(),
            name: entry.name.clone(),
            kind: kind_label(entry.kind),
            folder: filed
                .iter()
                .find(|(id, _)| *id == entry.id)
                .map(|(_, folder)| folder.clone()),
        })
        .collect())
}

/// The entries whose name is `query` (ignoring case); when none is, those
/// that contain it. A blank query matches nothing.
fn matching_entries<'a>(entries: &'a [EncryptedEntry], query: &str) -> Vec<&'a EncryptedEntry> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return Vec::new();
    }
    let exact: Vec<&EncryptedEntry> = entries
        .iter()
        .filter(|entry| entry.name.to_lowercase() == needle)
        .collect();
    if !exact.is_empty() {
        return exact;
    }
    entries
        .iter()
        .filter(|entry| entry.name.to_lowercase().contains(&needle))
        .collect()
}

/// One entry for `query`, or the reply that says why there is not exactly
/// one.
fn one_entry<'a>(entries: &'a [EncryptedEntry], query: &str) -> Result<&'a EncryptedEntry, Reply> {
    let hits = matching_entries(entries, query);
    match hits.as_slice() {
        [] => Err(Reply::say(format!(
            "I couldn't find anything called \"{query}\". Say \"show my secrets\" to see what you have."
        ))),
        [one] => Ok(one),
        many => Err(Reply::say(format!(
            "More than one entry matches \"{query}\": {}. Say the full name of the one you mean.",
            many.iter()
                .map(|entry| format!("\"{}\"", entry.name))
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    }
}

/// One folder for `reference`, or the reply that says why not.
fn one_folder(nodes: &[FolderNode], reference: &str) -> Result<String, Reply> {
    match resolve(nodes, reference) {
        Resolution::One(id) => Ok(id),
        Resolution::None => Err(Reply::say(format!(
            "I don't see a folder called \"{reference}\". Say \"create folder {reference}\" to make it."
        ))),
        Resolution::Many(paths) => Err(Reply::say(format!(
            "More than one folder matches \"{reference}\": {}. Say the whole path, like \"{}\".",
            paths.join(", "),
            paths.first().cloned().unwrap_or_default()
        ))),
    }
}

/// Make a folder, inside `parent` or at the top level.
pub fn create_folder(
    state: &AppState,
    name: &str,
    parent: Option<&str>,
) -> Result<Reply, VaultError> {
    let kek = unlocked_kek(state)?;
    if name.trim().is_empty() {
        return Ok(Reply::say(
            "What should the folder be called? Say, for example, \"create folder Taxes\".",
        ));
    }
    let name = validate_name(name).map_or_else(|e| Err(e.to_string()), Ok);
    let name = match name {
        Ok(name) => name,
        Err(message) => return Ok(Reply::say(message)),
    };
    let nodes = folder_nodes(state, &kek)?;
    let parent_id = match parent {
        None => None,
        Some(reference) => match one_folder(&nodes, reference) {
            Ok(id) => Some(id),
            Err(reply) => return Ok(reply),
        },
    };
    if let Some(parent_id) = parent_id.as_deref() {
        let depth = depth_of(&links(&nodes), parent_id).unwrap_or(MAX_FOLDER_DEPTH);
        if depth >= MAX_FOLDER_DEPTH {
            return Ok(Reply::say(format!(
                "Folders can only go {MAX_FOLDER_DEPTH} levels deep."
            )));
        }
    }
    if sibling_exists(&nodes, parent_id.as_deref(), &name, None) {
        return Ok(Reply::say(format!(
            "There is already a folder called \"{name}\" there."
        )));
    }
    let id = uuid::Uuid::new_v4().to_string();
    state
        .repository
        .insert_folder(&seal_folder(state, &kek, &id, parent_id.clone(), &name)?)?;
    let place = parent_id
        .as_deref()
        .map(|id| path_label(&nodes, id))
        .map_or_else(String::new, |path| format!(" inside \"{path}\""));
    Ok(Reply::say(format!(
        "Created the folder \"{name}\"{place}. Say \"move <name> to {name}\" to put something in it."
    )))
}

/// File an entry (or move a folder) into a folder; `None` takes an entry
/// out of its folder.
pub fn move_item(state: &AppState, name: &str, folder: Option<&str>) -> Result<Reply, VaultError> {
    let kek = unlocked_kek(state)?;
    let nodes = folder_nodes(state, &kek)?;
    let entries = state.repository.list_all_entries()?;
    let target = match folder {
        None => None,
        Some(reference) => match one_folder(&nodes, reference) {
            Ok(id) => Some(id),
            Err(reply) => return Ok(reply),
        },
    };
    if matching_entries(&entries, name).is_empty() {
        if let Resolution::One(folder_id) = resolve(&nodes, name) {
            return move_folder(state, &nodes, &folder_id, target.as_deref());
        }
    }
    let entry = match one_entry(&entries, name) {
        Ok(entry) => entry,
        Err(reply) => return Ok(reply),
    };
    if !state
        .repository
        .set_entry_folder(&entry.id, target.as_deref())?
    {
        return Ok(Reply::say("That entry is no longer there."));
    }
    Ok(Reply::say(target.as_deref().map_or_else(
        || format!("Took \"{}\" out of its folder.", entry.name),
        |id| {
            format!(
                "Moved \"{}\" into \"{}\".",
                entry.name,
                path_label(&nodes, id)
            )
        },
    )))
}

fn move_folder(
    state: &AppState,
    nodes: &[FolderNode],
    folder_id: &str,
    target: Option<&str>,
) -> Result<Reply, VaultError> {
    if let Err(e) = check_placement(&links(nodes), folder_id, target) {
        return Ok(Reply::say(e.to_string()));
    }
    let moved = nodes.iter().find(|n| n.id == folder_id);
    if let Some(node) = moved {
        if sibling_exists(nodes, target, &node.name, Some(folder_id)) {
            return Ok(Reply::say(format!(
                "There is already a folder called \"{}\" there.",
                node.name
            )));
        }
    }
    if !state.repository.move_folder(folder_id, target)? {
        return Ok(Reply::say("I couldn't move that folder."));
    }
    let name = path_label(nodes, folder_id);
    Ok(Reply::say(target.map_or_else(
        || format!("Moved the folder \"{name}\" to the top level."),
        |id| {
            format!(
                "Moved the folder \"{name}\" into \"{}\".",
                path_label(nodes, id)
            )
        },
    )))
}

/// Show the folder tree, or what is directly inside one folder. Names and
/// kinds only.
pub fn show_folders(state: &AppState, folder: Option<&str>) -> Result<Reply, VaultError> {
    let kek = unlocked_kek(state)?;
    let nodes = folder_nodes(state, &kek)?;
    let entries = state.repository.list_all_entries()?;
    let items = tree_items(state, &entries)?;
    if let Some(reference) = folder {
        let id = match one_folder(&nodes, reference) {
            Ok(id) => id,
            Err(reply) => return Ok(reply),
        };
        let path = path_label(&nodes, &id);
        let inside = render_folder(&nodes, &items, &id);
        return Ok(Reply::say(if inside.is_empty() {
            format!("📁 {path} is empty.")
        } else {
            format!("📁 {path}\n{inside}")
        }));
    }
    if nodes.is_empty() {
        return Ok(Reply::say(
            "You don't have any folders yet. Say \"create folder Taxes\" to make one, \
             then \"move <name> to Taxes\" to put something in it.",
        ));
    }
    Ok(Reply::say(format!(
        "Here is how your vault is organised (names only, no secrets shown):\n\n{}",
        render_tree(&nodes, &items)
    )))
}

/// Does this stored kind belong to the category the user named?
fn in_category(kind: EntryKind, category: &str) -> bool {
    match category {
        "note" => matches!(kind, EntryKind::Note),
        "document" => matches!(kind, EntryKind::Document),
        "secret" => true,
        _ => matches!(kind, EntryKind::Login),
    }
}

/// List what is in the vault: names and kinds, grouped by folder. Never
/// values.
pub fn list_entries(state: &AppState, category: Option<&str>) -> Result<Reply, VaultError> {
    let kek = unlocked_kek(state)?;
    let nodes = folder_nodes(state, &kek)?;
    let entries = state.repository.list_all_entries()?;
    let shown: Vec<EncryptedEntry> = entries
        .into_iter()
        .filter(|entry| category.is_none_or(|c| in_category(entry.kind, c)))
        .collect();
    if shown.is_empty() {
        return Ok(Reply::say(category.map_or_else(
            || "Your vault is empty. Try \"save a note\" or \"save a document\".".to_string(),
            |c| format!("You have no {c} entries yet."),
        )));
    }
    let items = tree_items(state, &shown)?;
    let count = shown.len();
    let noun = if count == 1 { "entry" } else { "entries" };
    // Everything: the folder tree. One category: a flat list that says
    // where each entry is filed.
    let body = if category.is_some() {
        items
            .iter()
            .map(|item| {
                let place = item
                    .folder
                    .as_deref()
                    .map(|id| path_label(&nodes, id))
                    .filter(|path| !path.is_empty())
                    .map_or_else(String::new, |path| format!(" — in {path}"));
                format!("• {} ({}){place}", item.name, item.kind)
            })
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        render_tree(&nodes, &items)
    };
    Ok(Reply::say(format!(
        "You have {count} {noun} (names only, no secrets shown):\n\n{body}"
    )))
}

/// Ask for a yes before deleting an entry.
pub fn confirm_delete(state: &AppState, name: &str) -> Result<Reply, VaultError> {
    unlocked_kek(state)?;
    let entries = state.repository.list_all_entries()?;
    let entry = match one_entry(&entries, name) {
        Ok(entry) => entry,
        Err(reply) => return Ok(reply),
    };
    state
        .chat_handler
        .set_pending_delete(&entry.id, &entry.name);
    Ok(Reply {
        message: format!("Delete \"{}\"? This can't be undone.", entry.name),
        options: vec![DELETE_YES.to_string(), DELETE_NO.to_string()],
    })
}

/// Delete an entry the user confirmed.
pub fn delete_entry(state: &AppState, entry_id: &str, name: &str) -> Result<Reply, VaultError> {
    unlocked_kek(state)?;
    Ok(Reply::say(if state.repository.delete_entry(entry_id)? {
        format!("Deleted \"{name}\".")
    } else {
        format!("\"{name}\" was already gone.")
    }))
}

/// Delete a folder. What was inside it is kept and moves up a level.
pub fn delete_folder(state: &AppState, name: &str) -> Result<Reply, VaultError> {
    let kek = unlocked_kek(state)?;
    let nodes = folder_nodes(state, &kek)?;
    let id = match one_folder(&nodes, name) {
        Ok(id) => id,
        Err(reply) => return Ok(reply),
    };
    let path = path_label(&nodes, &id);
    state.repository.delete_folder(&id)?;
    Ok(Reply::say(format!(
        "Deleted the folder \"{path}\". Everything that was inside it is still in your vault, one level up."
    )))
}

/// Rename an entry, or a folder.
pub fn rename_item(
    state: &AppState,
    name: &str,
    new_name: &str,
    folder: bool,
) -> Result<Reply, VaultError> {
    let kek = unlocked_kek(state)?;
    if new_name.trim().is_empty() {
        return Ok(Reply::say(format!(
            "What should \"{name}\" be called? Say, for example, \"rename {name} to <new name>\"."
        )));
    }
    let nodes = folder_nodes(state, &kek)?;
    let entries = state.repository.list_all_entries()?;
    if folder || matching_entries(&entries, name).is_empty() {
        if let Resolution::One(id) = resolve(&nodes, name) {
            return rename_folder(state, &kek, &nodes, &id, new_name);
        }
        if folder {
            return Ok(one_folder(&nodes, name).map_or_else(|reply| reply, |_| Reply::say("")));
        }
    }
    let entry = match one_entry(&entries, name) {
        Ok(entry) => entry,
        Err(reply) => return Ok(reply),
    };
    rename_entry(state, &kek, entry, new_name)
}

fn rename_folder(
    state: &AppState,
    kek: &[u8; KEK_LEN],
    nodes: &[FolderNode],
    id: &str,
    new_name: &str,
) -> Result<Reply, VaultError> {
    let new_name = match validate_name(new_name) {
        Ok(name) => name,
        Err(e) => return Ok(Reply::say(e.to_string())),
    };
    let parent = nodes
        .iter()
        .find(|n| n.id == id)
        .and_then(|n| n.parent.as_deref());
    if sibling_exists(nodes, parent, &new_name, Some(id)) {
        return Ok(Reply::say(format!(
            "There is already a folder called \"{new_name}\" there."
        )));
    }
    let sealed = seal_folder(state, kek, id, None, &new_name)?;
    let updated_at = sealed.updated_at.to_rfc3339();
    if !state.repository.update_folder_name(
        id,
        &sealed.dek_wrapped,
        &sealed.nonce,
        &sealed.ciphertext,
        &updated_at,
    )? {
        return Ok(Reply::say("That folder is no longer there."));
    }
    Ok(Reply::say(format!(
        "Renamed the folder \"{}\" to \"{new_name}\".",
        path_label(nodes, id)
    )))
}

fn rename_entry(
    state: &AppState,
    kek: &[u8; KEK_LEN],
    entry: &EncryptedEntry,
    new_name: &str,
) -> Result<Reply, VaultError> {
    let new_name = new_name.trim();
    let wrapped = <[u8; DEK_WRAPPED_LEN]>::try_from(entry.dek_wrapped.as_slice())
        .map_err(|_| VaultError::DecryptionFailed)?;
    let dek = state.crypto.unwrap_dek(&wrapped, kek)?;
    let plain = state.crypto.decrypt_entry(
        &dek,
        &entry.nonce,
        &entry.ciphertext,
        &entry_aad(&entry.id, entry.version),
    )?;
    let mut payload: EntryPayload =
        serde_json::from_slice(&plain).map_err(|e| VaultError::Serialization(e.to_string()))?;
    payload.name = new_name.to_string();
    // A note's title is a field of its own; keep it in step with the name.
    if let Some(title) = payload.fields.iter_mut().find(|f| f.key == "title") {
        title.value = new_name.to_string();
    }
    let json = Zeroizing::new(
        serde_json::to_vec(&payload).map_err(|e| VaultError::Serialization(e.to_string()))?,
    );
    let version = entry
        .version
        .checked_add(1)
        .ok_or_else(|| VaultError::Database("entry version out of range".to_string()))?;
    let new_dek = state.crypto.generate_dek();
    let (nonce, ciphertext) =
        state
            .crypto
            .encrypt_entry(&new_dek, &json, &entry_aad(&entry.id, version))?;
    let dek_wrapped = state.crypto.wrap_dek(&new_dek, kek)?.to_vec();
    let updated_at = chrono::Utc::now().to_rfc3339();
    let stored = StoredPayload {
        dek_wrapped: &dek_wrapped,
        nonce: &nonce,
        ciphertext: &ciphertext,
        name: new_name,
        version,
        updated_at: &updated_at,
    };
    if !state
        .repository
        .update_entry_payload(&entry.id, entry.version, &stored)?
    {
        return Ok(Reply::say(
            "That entry was changed somewhere else. Try again.",
        ));
    }
    Ok(Reply::say(format!(
        "Renamed \"{}\" to \"{new_name}\".",
        entry.name
    )))
}
