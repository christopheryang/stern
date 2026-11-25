// Copyright (c) 2026 Christopher Yang
// SPDX-License-Identifier: MIT

//! Finding entries from chat by what the user says about them: a name, a
//! topic, a folder, a time. The words are read by the core's search; this
//! module only gathers what the search may look at and shows what it found.
//!
//! What the search reads is an entry's name, its summary and keywords, its
//! folder, and its text: a note's body, a file's text, and the fields of a
//! login that are not marked hidden. A hidden field (a password, a card
//! number) is never put in front of the search and never shown in a list.

#![allow(clippy::unreachable)]

use crate::commands::{
    answer_entry, locked_chat_response, plain_chat_response, unique_label, unlocked_entries,
};
use crate::notes::{note_fields, open_note};
use crate::organize::{folder_nodes, show_folders};
use crate::AppState;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use stern_core::ai::chat::{ChatHandler, EntryCandidate};
use stern_core::domain::vault::entry::{EncryptedEntry, EntryKind, EntryPayload};
use stern_core::domain::vault::errors::VaultError;
use stern_core::domain::vault::folder::{path_label, path_segments, FolderNode};
use stern_core::domain::vault::query::describe;
use stern_core::domain::vault::text::searchable_text;
use stern_core::search::{find, FolderProblem, Hit, Kind, SearchDoc, MAX_RESULTS};
use stern_ipc::dto::ChatResponse;
use zeroize::{Zeroize, Zeroizing};

/// How much of a file is shown when one is opened in chat.
const EXCERPT_CHARS: usize = 600;

const fn kind_of(kind: EntryKind) -> Kind {
    match kind {
        EntryKind::Login => Kind::Login,
        EntryKind::Note => Kind::Note,
        EntryKind::Document => Kind::Document,
    }
}

/// The text of an entry the search may read. Hidden fields are left out.
fn readable_text(state: &AppState, entry: &EncryptedEntry, payload: &EntryPayload) -> String {
    match entry.kind {
        EntryKind::Note => note_fields(payload).1,
        EntryKind::Document => crate::files::read_stored(state, &entry.id)
            .ok()
            .and_then(|bytes| String::from_utf8(bytes.to_vec()).ok())
            .map(|text| searchable_text(&entry.name, &text))
            .unwrap_or_default(),
        EntryKind::Login => payload
            .fields
            .iter()
            .filter(|field| !field.hidden)
            .map(|field| format!("{} {}", field.key, field.value))
            .collect::<Vec<_>>()
            .join(" "),
    }
}

/// What the search may look at, for every entry in the vault.
fn search_docs(
    state: &AppState,
    entries: &[EncryptedEntry],
    kek: &[u8; stern_core::domain::vault::crypto_constants::KEK_LEN],
    nodes: &[FolderNode],
) -> Result<Vec<SearchDoc>, VaultError> {
    let filed: HashMap<String, String> = state.repository.entry_folders()?.into_iter().collect();
    let mut docs = Vec::with_capacity(entries.len());
    for entry in entries {
        // An entry that cannot be read is left out, not fatal.
        let Ok(payload) = open_note(state, kek, entry) else {
            continue;
        };
        docs.push(SearchDoc {
            id: entry.id.clone(),
            name: entry.name.clone(),
            kind: kind_of(entry.kind),
            folder: filed
                .get(&entry.id)
                .map(|folder| path_segments(nodes, folder))
                .unwrap_or_default(),
            summary: payload.summary.clone(),
            keywords: payload.keywords.clone(),
            text: readable_text(state, entry, &payload),
            created_at: entry.created_at,
            updated_at: entry.updated_at,
        });
    }
    Ok(docs)
}

/// Wipe what was decrypted for the search.
fn wipe(docs: &mut [SearchDoc]) {
    for doc in docs {
        doc.text.zeroize();
        doc.summary.zeroize();
    }
}

fn day(when: DateTime<Utc>) -> String {
    when.format("%d %b %Y").to_string()
}

/// One line about a candidate: name, kind, folder, date, and the summary
/// the user wrote. Never a value.
fn describe_hit(doc: &SearchDoc) -> String {
    let place = if doc.folder.is_empty() {
        String::new()
    } else {
        format!(", in {}", doc.folder.join(" > "))
    };
    let about = if doc.summary.is_empty() {
        String::new()
    } else {
        format!(": {}", doc.summary)
    };
    format!(
        "• {} ({}{place}, saved {}){about}",
        doc.name,
        doc.kind.word(),
        day(doc.created_at)
    )
}

/// The words that say which dates were used.
fn time_note(outcome: &stern_core::search::Outcome) -> Option<String> {
    if outcome.ignored_time {
        return Some("Nothing matched those dates, so I looked at every date.".to_string());
    }
    let range = outcome.range.as_ref()?;
    Some(if outcome.widened {
        format!(
            "Nothing matched exactly, so I widened the dates to {}.",
            describe(range)
        )
    } else {
        format!("Looking {}.", describe(range))
    })
}

fn folder_problem_reply(problem: &FolderProblem) -> ChatResponse {
    plain_chat_response(match problem {
        FolderProblem::Unknown(name) => format!(
            "I don't see a folder called \"{name}\". Say \"show my folders\" to see what you have."
        ),
        FolderProblem::Ambiguous(paths) => format!(
            "More than one folder matches: {}. Say the whole path, like \"{}\".",
            paths.join(", "),
            paths.first().cloned().unwrap_or_default()
        ),
    })
}

/// The start of a file's text, for showing in chat.
fn document_reply(state: &AppState, entry: &EncryptedEntry) -> ChatResponse {
    let Some(text) = crate::files::read_stored(state, &entry.id)
        .ok()
        .map(|bytes| Zeroizing::new(String::from_utf8(bytes.to_vec()).ok()))
    else {
        return plain_chat_response(format!("I couldn't open \"{}\".", entry.name));
    };
    let Some(text) = text.as_deref() else {
        return plain_chat_response(format!(
            "\"{}\" is an older file that isn't text, so I can't show it here.",
            entry.name
        ));
    };
    let shown = searchable_text(&entry.name, text);
    let mut excerpt: String = shown.chars().take(EXCERPT_CHARS).collect();
    if shown.chars().count() > EXCERPT_CHARS {
        excerpt.push('…');
    }
    plain_chat_response(format!("\"{}\":\n\n{excerpt}", entry.name))
}

/// Show one entry the way the user asked: a field if they named one,
/// otherwise a file's start, or the fields of a note or login (a note's
/// are its title and text).
pub fn open_entry(
    state: &AppState,
    entry: &EncryptedEntry,
    kek: &[u8; stern_core::domain::vault::crypto_constants::KEK_LEN],
    field: Option<&str>,
) -> Result<ChatResponse, VaultError> {
    if field.is_some() {
        return answer_entry(state, entry, kek, field);
    }
    match entry.kind {
        EntryKind::Document => Ok(document_reply(state, entry)),
        EntryKind::Note | EntryKind::Login => answer_entry(state, entry, kek, None),
    }
}

/// Offer the candidates as tappable choices.
fn candidates_reply(
    state: &AppState,
    docs: &[SearchDoc],
    hits: &[Hit],
    field: Option<&str>,
    heading: &str,
) -> ChatResponse {
    let mut labels: Vec<String> = Vec::new();
    let mut candidates: Vec<EntryCandidate> = Vec::new();
    let mut lines: Vec<String> = Vec::new();
    for hit in hits.iter().take(MAX_RESULTS) {
        let Some(doc) = docs.iter().find(|doc| doc.id == hit.id) else {
            continue;
        };
        let label = unique_label(&doc.name, &labels);
        labels.push(label.clone());
        candidates.push(EntryCandidate {
            id: doc.id.clone(),
            label,
        });
        lines.push(describe_hit(doc));
    }
    state
        .chat_handler
        .set_pending_choice(candidates, field.map(str::to_string));
    ChatResponse {
        message: format!("{heading}\n\n{}\n\nTap the one you mean.", lines.join("\n")),
        action: None,
        user_message_display: None,
        options: labels,
        input_sensitive: false,
    }
}

/// Search the vault for what `text` asks for and say what was found.
///
/// `field` is a field the user named ("my password for google"): a single
/// match then answers with that field. `fallback` is set when nothing else
/// understood the message; finding nothing is then answered with the "I'm
/// not sure" reply rather than "no match".
pub fn find_entries(
    state: &AppState,
    text: &str,
    field: Option<&str>,
    fallback: bool,
) -> Result<ChatResponse, VaultError> {
    let Some((entries, kek)) = unlocked_entries(state)? else {
        return Ok(locked_chat_response());
    };
    let nodes = folder_nodes(state, &kek)?;
    let mut docs = search_docs(state, &entries, &kek, &nodes)?;
    let found = find(&docs, &nodes, text, Utc::now());
    let reply = reply_for(
        state, &entries, &kek, &nodes, &docs, &found, text, field, fallback,
    );
    wipe(&mut docs);
    reply
}

#[allow(clippy::too_many_arguments)]
fn reply_for(
    state: &AppState,
    entries: &[EncryptedEntry],
    kek: &[u8; stern_core::domain::vault::crypto_constants::KEK_LEN],
    nodes: &[FolderNode],
    docs: &[SearchDoc],
    found: &stern_core::search::Found,
    text: &str,
    field: Option<&str>,
    fallback: bool,
) -> Result<ChatResponse, VaultError> {
    if let Some(problem) = &found.folder_problem {
        return Ok(folder_problem_reply(problem));
    }
    if let Some(folder_id) = &found.folder_view {
        let reply = show_folders(state, Some(&path_label(nodes, folder_id)))?;
        return Ok(ChatResponse {
            message: reply.message,
            action: None,
            user_message_display: None,
            options: reply.options,
            input_sensitive: false,
        });
    }
    let hits = &found.outcome.hits;
    if hits.is_empty() {
        if fallback {
            return Ok(plain_chat_response(ChatHandler::not_sure_reply(text)));
        }
        let mut message = String::from("I couldn't find anything that matches that.");
        if let Some(note) = time_note(&found.outcome) {
            message = format!("{message} {note}");
        }
        message.push_str(
            " Try different words, or say \"show my secrets\" to see everything you have.",
        );
        return Ok(plain_chat_response(message));
    }
    let single = match hits.as_slice() {
        // A guess from a message nothing understood is never opened outright:
        // it is offered, so a stray word cannot show a secret.
        [only] if !fallback => entries.iter().find(|entry| entry.id == only.id),
        _ => None,
    };
    if let Some(entry) = single {
        let mut reply = open_entry(state, entry, kek, field)?;
        if let Some(note) = time_note(&found.outcome) {
            reply.message = format!("{note}\n\n{}", reply.message);
        }
        return Ok(reply);
    }
    let mut heading = if hits.len() == 1 {
        "This might be what you mean.".to_string()
    } else {
        format!("I found {} possible matches.", hits.len().min(MAX_RESULTS))
    };
    if let Some(note) = time_note(&found.outcome) {
        heading = format!("{heading} {note}");
    }
    Ok(candidates_reply(state, docs, hits, field, &heading))
}
