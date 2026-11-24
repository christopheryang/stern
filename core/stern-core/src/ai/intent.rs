use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Intent {
    StoreSecret {
        name: String,
        kind: String,
        fields: Vec<(String, String)>,
    },
    RetrieveSecret {
        name: String,
        /// The field the user asked for ("password" in "what's my
        /// password for Google?"), when they named one.
        field: Option<String>,
    },
    ListSecrets {
        category: Option<String>,
    },
    DeleteSecret {
        name: String,
    },
    UpdateSecret {
        name: String,
        fields: Vec<(String, String)>,
    },
    /// Make a folder, at the top level or inside `parent`. An empty
    /// `name` means the request did not say what to call it.
    CreateFolder {
        name: String,
        parent: Option<String>,
    },
    /// File an entry in a folder; `folder` of `None` takes it out of its
    /// folder.
    MoveEntry {
        name: String,
        folder: Option<String>,
    },
    /// Show the folder tree, or what is inside one folder.
    ShowFolders {
        folder: Option<String>,
    },
    /// Give an entry (or, with `folder`, a folder) a new name. An empty
    /// `new_name` means the request did not say what to call it.
    Rename {
        name: String,
        new_name: String,
        folder: bool,
    },
    DeleteFolder {
        name: String,
    },
    ExportVault,
    ImportVault,
    CreateVault,
    UnlockVault,
    Help,
    Greeting,
    Unknown {
        text: String,
    },
}

const SECRET_TYPES: &[&str] = &[
    "password",
    "secret",
    "credential",
    "login",
    "api key",
    "token",
    "ssh",
    "wifi",
    "note",
    "document",
];

const RETRIEVE_VERBS: &[&str] = &[
    "find", "get", "retrieve", "search", "show", "what", "where", "forgot",
];
const LIST_VERBS: &[&str] = &["list", "show", "display", "all"];
const DELETE_VERBS: &[&str] = &["delete", "remove", "destroy", "drop"];
const UPDATE_VERBS: &[&str] = &["update", "change", "modify", "edit"];

/// Polite lead-ins that carry no meaning of their own, longest first so
/// "can you please" goes before "can you".
const POLITE_LEADS: &[&str] = &[
    "can you please",
    "could you please",
    "would you please",
    "will you please",
    "can you",
    "could you",
    "would you",
    "will you",
    "please",
    "kindly",
];

/// The request without its polite lead-in: "please edit my note" is the
/// same request as "edit my note", and every rule keys on the verb being
/// first. Stripping repeats ("please, could you ..."). When nothing would
/// be left, the text is kept as it was.
fn strip_polite_lead(lower: &str) -> &str {
    let mut rest = lower.trim_start();
    loop {
        let stripped = POLITE_LEADS.iter().find_map(|lead| {
            let after = rest.strip_prefix(lead)?;
            // Whole words only: "pleasant" does not start with "please".
            (after.is_empty() || after.starts_with([' ', ',']))
                .then(|| after.trim_start_matches([' ', ',']))
        });
        match stripped {
            Some(after) if !after.is_empty() => rest = after,
            _ => return rest,
        }
    }
}

#[must_use]
pub fn classify_intent(text: &str) -> Intent {
    // Expand a sentence-initial question contraction, so "what's
    // my password for Google?" parses like "what is ..." — the
    // detectors key on the separate words. Only the leading
    // contraction is touched: the same words inside a stored value
    // keep their original text.
    let raw_lower = text.to_lowercase();
    let lower = if raw_lower.starts_with("what's ") {
        raw_lower.replacen("what's", "what is", 1)
    } else if raw_lower.starts_with("where's ") {
        raw_lower.replacen("where's", "where is", 1)
    } else {
        strip_polite_lead(&raw_lower).to_string()
    };
    let words: Vec<&str> = lower.split_whitespace().collect();
    // The greeting/help checks compare whole words, so strip the
    // punctuation a sentence leaves on its last word ("you?" is
    // still "you").
    let clean_words: Vec<&str> = words
        .iter()
        .map(|word| word.trim_end_matches(['?', '!', '.', ',']))
        .collect();

    // Folder requests are exact words and cannot be mistaken for small
    // talk or a help question, so they are read first.
    if let Some(intent) = classify_folder_intent(text) {
        return intent;
    }

    if is_greeting(&clean_words, &lower) {
        return Intent::Greeting;
    }

    if is_help(&clean_words) {
        return Intent::Help;
    }

    // The vault itself is not something to store: "create vault" and
    // "unlock the vault" are about the vault, whatever store words they
    // contain.
    if let Some(intent) = detect_vault_manage(&words) {
        return intent;
    }

    // A leading edit verb is an edit, whatever the rest says: "edit my
    // shopping list" is not a list request, and "edit my note" is not a
    // store request, though both contain words those rules look for.
    if words.first().is_some_and(|w| UPDATE_VERBS.contains(w)) {
        if let Some(intent) = detect_update(&words, &lower) {
            return intent;
        }
    }

    if let Some(intent) = detect_store(&words, &lower) {
        return intent;
    }

    if let Some(intent) = detect_plural_list(&clean_words) {
        return intent;
    }

    if let Some(intent) = detect_retrieve(&words, &lower) {
        return intent;
    }

    if let Some(intent) = detect_list(&words, &lower) {
        return intent;
    }

    if let Some(intent) = detect_delete(&words) {
        return intent;
    }

    if let Some(intent) = detect_update(&words, &lower) {
        return intent;
    }

    if let Some(intent) = detect_export(&words) {
        return intent;
    }

    if let Some(intent) = detect_import(&words) {
        return intent;
    }

    Intent::Unknown {
        text: text.to_string(),
    }
}

fn is_greeting(words: &[&str], lower: &str) -> bool {
    let greetings = [
        "hello",
        "hi",
        "hey",
        "good",
        "morning",
        "evening",
        "afternoon",
        "greetings",
    ];
    if words.iter().any(|w| greetings.contains(w)) {
        return true;
    }
    // Gratitude counts as small talk only when it is (nearly) the
    // whole message — "save my password, thanks" is a store, not a
    // greeting.
    let gratitude = ["thanks", "thank"];
    if words.len() <= 4 && words.iter().any(|w| gratitude.contains(w)) {
        return true;
    }
    // Small-talk phrases the word list cannot express.
    let phrases = [
        "how are you",
        "how's it going",
        "how is it going",
        "how do you do",
        "how have you been",
        "what's up",
        "whats up",
    ];
    phrases.iter().any(|phrase| lower.contains(phrase))
}

fn is_help(words: &[&str]) -> bool {
    if matches!(words, ["help"]) {
        return true;
    }
    let has_question = words.iter().any(|w| *w == "help" || *w == "how");
    let has_you = words
        .iter()
        .any(|w| *w == "can" || *w == "do" || *w == "you");
    let has_subject = words.iter().any(|w| *w == "i" || *w == "me" || *w == "you");
    (has_subject || has_you) && has_question
}

fn has_secret_type(text: &str) -> bool {
    SECRET_TYPES.iter().any(|t| text.contains(t))
}

fn has_value_indicator(text: &str) -> bool {
    text.contains(" is ") || text.contains(" is: ") || text.contains(": ")
}

fn is_retrieve_context(words: &[&str]) -> bool {
    words.iter().any(|w| RETRIEVE_VERBS.contains(w))
}

fn is_list_context(words: &[&str]) -> bool {
    words.iter().any(|w| LIST_VERBS.contains(w))
}

fn is_delete_context(words: &[&str]) -> bool {
    words.iter().any(|w| DELETE_VERBS.contains(w))
}

fn is_update_context(words: &[&str]) -> bool {
    words.iter().any(|w| UPDATE_VERBS.contains(w))
}

fn detect_store(words: &[&str], full_text: &str) -> Option<Intent> {
    let has_store_verb = words.iter().any(|w| {
        [
            "store", "save", "add", "create", "new", "remember", "keep", "note",
        ]
        .contains(w)
    });

    let implicit_store = has_secret_type(full_text)
        && has_value_indicator(full_text)
        && !is_retrieve_context(words)
        && !is_list_context(words)
        && !is_delete_context(words)
        && !is_update_context(words);

    if !has_store_verb && !implicit_store {
        return None;
    }

    let kind = SECRET_TYPES
        .iter()
        .find(|t| full_text.contains(*t))
        .map_or_else(|| "secret".to_string(), |t| (*t).to_string());

    let name = extract_store_name(full_text);
    let fields = extract_store_value(full_text, &kind);

    Some(Intent::StoreSecret { name, kind, fields })
}

fn detect_retrieve(words: &[&str], full_text: &str) -> Option<Intent> {
    if !is_retrieve_context(words) {
        return None;
    }

    let has_secret = has_secret_type(full_text);
    let has_name_hint = full_text.contains(" for ")
        || full_text.contains(" called ")
        || full_text.contains(" named ")
        || full_text.contains("'s");

    if !has_secret && !has_name_hint && !words.iter().any(|w| ["my", "me"].contains(w)) {
        return None;
    }

    let name = extract_retrieve_name(full_text);
    if name.is_empty() {
        return None;
    }
    Some(Intent::RetrieveSecret {
        name,
        field: extract_field_request(full_text),
    })
}

/// The field a retrieval question asks for, if it names one
/// ("password" in "what's my password for Google?").
fn extract_field_request(full_text: &str) -> Option<String> {
    const FIELD_WORDS: &[(&str, &str)] = &[
        ("card number", "number"),
        ("username", "username"),
        ("user name", "username"),
        ("password", "password"),
        ("email", "email"),
        ("phone", "phone"),
        ("address", "address"),
        ("cvv", "cvv"),
        ("expiry", "expiry"),
        ("website", "website"),
    ];
    FIELD_WORDS
        .iter()
        .find(|(word, _)| full_text.contains(word))
        .map(|(_, field)| (*field).to_string())
}

fn detect_list(words: &[&str], full_text: &str) -> Option<Intent> {
    if !is_list_context(words) {
        return None;
    }
    let has_all = words.iter().any(|w| *w == "all" || *w == "every");
    let has_secret = has_secret_type(full_text);
    let has_list_verb = words.iter().any(|w| LIST_VERBS.contains(w));

    if !has_all && !has_secret && !has_list_verb {
        return None;
    }

    let category = SECRET_TYPES
        .iter()
        .find(|t| full_text.contains(*t))
        .map(|c| (*c).to_string());

    Some(Intent::ListSecrets { category })
}

/// Words that only say what kind of thing a delete request points at.
const DELETE_FILLER: &[&str] = &[
    "my",
    "the",
    "a",
    "an",
    "old",
    "saved",
    "stored",
    "entry",
    "entries",
    "item",
    "secret",
    "password",
    "login",
    "note",
    "document",
    "credential",
    "token",
    "named",
    "called",
];

/// The name a delete request points at: "delete my old netflix entry"
/// gives "netflix".
fn extract_delete_name(words: &[&str]) -> String {
    let named: Vec<&str> = words
        .iter()
        .copied()
        .filter(|w| !DELETE_VERBS.contains(w) && !DELETE_FILLER.contains(w))
        .map(|w| w.trim_matches(['.', '!', '?', ',', '"', '\'']))
        .filter(|w| !w.is_empty())
        .collect();
    if named.is_empty() {
        extract_retrieve_name(&words.join(" "))
    } else {
        named.join(" ")
    }
}

/// "show my notes", "list all my passwords": a plural kind after a show
/// verb asks for the whole list, not for one entry named "notes".
fn detect_plural_list(words: &[&str]) -> Option<Intent> {
    const PLURALS: &[(&str, &str)] = &[
        ("notes", "note"),
        ("documents", "document"),
        ("passwords", "password"),
        ("logins", "login"),
        ("secrets", "secret"),
        ("credentials", "credential"),
        ("tokens", "token"),
        ("files", "document"),
    ];
    if !matches!(
        words.first(),
        Some(&("show" | "list" | "display" | "see" | "view"))
    ) {
        return None;
    }
    let last = words.last()?;
    PLURALS
        .iter()
        .find(|(plural, _)| plural == last)
        .map(|(_, kind)| Intent::ListSecrets {
            category: Some((*kind).to_string()),
        })
}

fn detect_delete(words: &[&str]) -> Option<Intent> {
    if !is_delete_context(words) {
        return None;
    }
    let name = extract_delete_name(words);
    if name.is_empty() {
        return None;
    }
    Some(Intent::DeleteSecret { name })
}

fn detect_update(words: &[&str], full_text: &str) -> Option<Intent> {
    if !is_update_context(words) {
        return None;
    }
    let fields = extract_key_value_pairs(full_text);
    // With field values the request names a login to change; without
    // any it names the note or entry to open for editing.
    let name = if fields.is_empty() {
        extract_update_name(full_text)
    } else {
        extract_retrieve_name(full_text)
    };
    Some(Intent::UpdateSecret { name, fields })
}

fn detect_export(words: &[&str]) -> Option<Intent> {
    let export_words = ["export", "backup", "back up", "transfer", "migrate"];
    if !words.iter().any(|w| export_words.contains(w)) {
        return None;
    }

    Some(Intent::ExportVault)
}

fn detect_import(words: &[&str]) -> Option<Intent> {
    let import_words = ["import", "restore", "load"];
    let has_import = words.iter().any(|w| import_words.contains(w));
    if !has_import {
        return None;
    }

    Some(Intent::ImportVault)
}

fn detect_vault_manage(words: &[&str]) -> Option<Intent> {
    let has_vault = words.contains(&"vault");

    if !has_vault {
        return None;
    }

    // Something stored *in* the vault ("save a note in my vault") is a
    // store, not a request about the vault.
    let stores_into_it = words.iter().any(|w| matches!(*w, "in" | "into" | "to"))
        && words
            .iter()
            .any(|w| matches!(*w, "save" | "store" | "add" | "remember" | "keep"));
    if stores_into_it {
        return None;
    }

    let create_words = ["create", "new", "set up", "setup", "initialize", "init"];
    let has_create = words.iter().any(|w| create_words.contains(w));

    let unlock_words = ["unlock", "open", "decrypt", "access"];
    let has_unlock = words.iter().any(|w| unlock_words.contains(w));

    if has_create {
        return Some(Intent::CreateVault);
    }

    if has_unlock {
        return Some(Intent::UnlockVault);
    }

    None
}

#[allow(clippy::arithmetic_side_effects, clippy::option_if_let_else)]
fn extract_store_name(full_text: &str) -> String {
    let lower = full_text.to_lowercase();

    for prefix in &["for ", "called ", "named "] {
        if let Some(pos) = lower.find(prefix) {
            let after = &full_text[pos + prefix.len()..];
            let stop_chars = [',', '.', '!', '?', ':', '='];
            let mut end = after.len();
            for (i, c) in after.char_indices() {
                if stop_chars.contains(&c) {
                    end = i;
                    break;
                }
            }
            let truncated = &after[..end];
            if let Some(is_pos) = truncated.to_lowercase().find(" is ") {
                let name = truncated[..is_pos].trim();
                if !name.is_empty() {
                    return name.to_string();
                }
            }
            if let Some(is_pos) = truncated.to_lowercase().find(" is: ") {
                let name = truncated[..is_pos].trim();
                if !name.is_empty() {
                    return name.to_string();
                }
            }
            let name = truncated.trim().to_string();
            if !name.is_empty() {
                return name;
            }
        }
    }

    if let Some(pos) = lower.find("'s ") {
        let after = &full_text[pos + 3..];
        let rest: String = after
            .chars()
            .take_while(|c| *c != ',' && *c != '.' && *c != '!')
            .collect();
        let rest = rest.trim();
        if !rest.is_empty() {
            let word: String = full_text[..pos]
                .chars()
                .rev()
                .take_while(|c| !c.is_whitespace())
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            if !word.is_empty() {
                return format!("{word} {rest}");
            }
        }
    }

    let skip_words = [
        "my", "the", "a", "an", "store", "save", "add", "create", "new", "please", "can", "you",
        "want", "to", "and", "it", "me", "remember", "keep", "note", "for", "with",
    ];

    let secret_words: Vec<&str> = SECRET_TYPES.to_vec();

    let before_is = if let Some(pos) = lower.find(" is ") {
        &full_text[..pos]
    } else if let Some(pos) = lower.find(" is: ") {
        &full_text[..pos]
    } else {
        full_text
    };

    let name_words: Vec<&str> = before_is
        .split_whitespace()
        .filter(|w| {
            let lw = w.to_lowercase();
            !skip_words.contains(&lw.as_str()) && !secret_words.contains(&lw.as_str())
        })
        .collect();

    if !name_words.is_empty() {
        return name_words.join(" ");
    }

    full_text
        .split_whitespace()
        .filter(|w| {
            let lw = w.to_lowercase();
            !skip_words.contains(&lw.as_str()) && !secret_words.contains(&lw.as_str())
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[allow(clippy::arithmetic_side_effects)]
fn extract_store_value(full_text: &str, kind: &str) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    let lower = full_text.to_lowercase();

    let kv_patterns: &[(&[&str], &str)] = &[
        (&["password is:", "password is "], "password"),
        (&["secret is:", "secret is "], "secret"),
        (&["credential is:", "credential is "], "credential"),
        (&["token is:", "token is "], "token"),
        (&["api key is:", "api key is "], "api key"),
        (&["wifi password is:", "wifi password is "], "wifi"),
    ];

    for (suffixes, field_key) in kv_patterns {
        for suffix in *suffixes {
            if let Some(pos) = lower.find(suffix) {
                let after = &full_text[pos + suffix.len()..];
                let value: String = after
                    .chars()
                    .take_while(|c| *c != ',' && *c != '.' && *c != '!' && *c != '?')
                    .collect();
                let value = value.trim().to_string();
                if !value.is_empty() {
                    pairs.push((field_key.to_string(), value));
                    return pairs;
                }
            }
        }
    }

    let colon_patterns: &[&str] = &["password:", "secret:", "credential:", "token:", "api key:"];
    for pattern in colon_patterns {
        if let Some(pos) = lower.find(pattern) {
            let after = &full_text[pos + pattern.len()..];
            let value: String = after
                .chars()
                .take_while(|c| *c != ',' && *c != '.' && *c != '!' && *c != '?')
                .collect();
            let value = value.trim().to_string();
            if !value.is_empty() {
                pairs.push((pattern.trim_end_matches(':').to_string(), value));
                return pairs;
            }
        }
    }

    if let Some(pos) = lower.rfind(" is ") {
        let after = &full_text[pos + 4..];
        let value: String = after
            .chars()
            .take_while(|c| *c != ',' && *c != '.' && *c != '!' && *c != '?')
            .collect();
        let value = value.trim().to_string();
        if !value.is_empty() {
            pairs.push((kind.to_string(), value));
        }
    }

    if pairs.is_empty() {
        if let Some(pos) = lower.rfind(" is: ") {
            let after = &full_text[pos + 5..];
            let value: String = after
                .chars()
                .take_while(|c| *c != ',' && *c != '.' && *c != '!' && *c != '?')
                .collect();
            let value = value.trim().to_string();
            if !value.is_empty() {
                pairs.push((kind.to_string(), value));
            }
        }
    }

    pairs
}

/// The name an edit request points at: what follows the verb, without
/// the filler words in front ("edit my note called Groceries" gives
/// "Groceries"). Falls back to the general name rules when nothing is left.
fn extract_update_name(full_text: &str) -> String {
    const FILLER: &[&str] = &[
        "my", "the", "a", "an", "note", "document", "entry", "called", "named", "for", "of",
    ];
    let mut words = full_text.split_whitespace();
    let leading_verb = words
        .next()
        .is_some_and(|w| UPDATE_VERBS.contains(&w.to_lowercase().as_str()));
    let rest: Vec<&str> = if leading_verb {
        words.collect()
    } else {
        full_text.split_whitespace().collect()
    };
    let start = rest
        .iter()
        .position(|w| !FILLER.contains(&w.to_lowercase().as_str()))
        .unwrap_or(rest.len());
    let name = rest
        .get(start..)
        .unwrap_or_default()
        .join(" ")
        .trim_end_matches(['.', '!', '?'])
        .trim()
        .to_string();
    if name.is_empty() {
        extract_retrieve_name(full_text)
    } else {
        name
    }
}

#[allow(clippy::arithmetic_side_effects)]
fn extract_retrieve_name(full_text: &str) -> String {
    let lower = full_text.to_lowercase();

    for prefix in &["for ", "about ", "called ", "named "] {
        if let Some(pos) = lower.find(prefix) {
            let after = &full_text[pos + prefix.len()..];
            let name: String = after
                .chars()
                .take_while(|c| *c != ',' && *c != '.' && *c != '!' && *c != '?')
                .collect();
            let name = name.trim().to_string();
            if !name.is_empty() {
                return name;
            }
        }
    }

    let skip_words = [
        "find",
        "get",
        "retrieve",
        "search",
        "show",
        "what",
        "where",
        "my",
        "the",
        "a",
        "an",
        "all",
        "list",
        "display",
        "forgot",
        "password",
        "secret",
        "credential",
        "login",
        "api",
        "key",
        "token",
        "ssh",
        "wifi",
        "note",
        "document",
        "do",
        "you",
        "have",
    ];

    let name_words: Vec<&str> = full_text
        .split_whitespace()
        .filter(|w| !skip_words.contains(&w.to_lowercase().as_str()))
        .collect();

    if !name_words.is_empty() {
        return name_words.join(" ");
    }

    full_text.to_string()
}

#[allow(clippy::arithmetic_side_effects)]
fn extract_key_value_pairs(text: &str) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    let patterns = ["username", "user", "url", "token", "api key"];

    for key in &patterns {
        if let Some(pos) = text.find(key) {
            let after = &text[pos + key.len()..];
            let after = after.trim_start_matches([':', '=', ' ']);
            let value: String = after.chars().take_while(|c| !c.is_whitespace()).collect();
            if !value.is_empty() {
                pairs.push((key.to_string(), value));
            }
        }
    }

    pairs
}

// ---------------------------------------------------------------------
// Folders. These rules read the words as typed, not lowercased, because a
// folder name keeps its capitals: "create folder Taxes" makes "Taxes".
// ---------------------------------------------------------------------

/// Lead-in words with no meaning of their own, removed word by word.
fn strip_lead_words<'a, 'b>(words: &'b [&'a str]) -> &'b [&'a str] {
    const LEADS: &[&[&str]] = &[
        &["can", "you", "please"],
        &["could", "you", "please"],
        &["would", "you", "please"],
        &["will", "you", "please"],
        &["can", "you"],
        &["could", "you"],
        &["would", "you"],
        &["will", "you"],
        &["please"],
        &["kindly"],
    ];
    let mut rest = words;
    'again: loop {
        for lead in LEADS {
            let hit = rest.len() > lead.len()
                && rest
                    .iter()
                    .zip(lead.iter())
                    .all(|(w, l)| w.trim_end_matches(',').eq_ignore_ascii_case(l));
            if hit {
                rest = rest.get(lead.len()..).unwrap_or(rest);
                continue 'again;
            }
        }
        return rest;
    }
}

const PARENT_PREPS: &[&str] = &["in", "inside", "under", "within"];
const TARGET_PREPS: &[&str] = &["to", "into", "in", "under", "inside"];
const REF_FILLER: &[&str] = &["my", "the", "a", "an", "new", "called", "named", "folder"];

fn clean_word(word: &str) -> &str {
    word.trim_matches(['.', '!', '?', ',', '"', '\'', ':', ';'])
}

/// The words as one reference, minus the filler in front and the
/// punctuation around it.
fn reference(words: &[&str]) -> String {
    let kept: Vec<&str> = words
        .iter()
        .map(|w| clean_word(w))
        .filter(|w| !w.is_empty())
        .collect();
    // Filler goes only while something else remains: a folder called "A"
    // is still a folder called "A".
    let start = kept
        .iter()
        .position(|w| !REF_FILLER.contains(&w.to_lowercase().as_str()))
        .unwrap_or_else(|| kept.len().saturating_sub(1));
    let mut kept = kept.get(start..).unwrap_or_default().to_vec();
    if kept
        .last()
        .is_some_and(|w| w.eq_ignore_ascii_case("folder") || w.eq_ignore_ascii_case("folders"))
    {
        kept.pop();
    }
    kept.join(" ")
}

/// An entry reference: the same, without the words that only say what kind
/// of thing it is ("netflix login" is the entry called Netflix).
fn entry_reference(words: &[&str]) -> String {
    let plain = reference(words);
    let trimmed: Vec<&str> = plain
        .split_whitespace()
        .filter(|w| {
            !DELETE_FILLER.contains(&w.to_lowercase().as_str()) || w.eq_ignore_ascii_case("old")
        })
        .collect();
    if trimmed.is_empty() {
        plain
    } else {
        trimmed.join(" ")
    }
}

fn lowered(words: &[&str]) -> Vec<String> {
    words.iter().map(|w| clean_word(w).to_lowercase()).collect()
}

fn position_of(lw: &[String], any: &[&str], from: usize) -> Option<usize> {
    lw.iter()
        .enumerate()
        .skip(from)
        .find(|(_, w)| any.contains(&w.as_str()))
        .map(|(i, _)| i)
}

fn last_position_of(lw: &[String], any: &[&str]) -> Option<usize> {
    lw.iter()
        .enumerate()
        .rev()
        .find(|(_, w)| any.contains(&w.as_str()))
        .map(|(i, _)| i)
}

/// Folder requests: make, rename, delete, move into, and show. `None`
/// when the text is not about folders.
#[must_use]
pub fn classify_folder_intent(text: &str) -> Option<Intent> {
    let all: Vec<&str> = text.split_whitespace().collect();
    let words = strip_lead_words(&all);
    let lw = lowered(words);
    let first = lw.first()?.as_str();
    let mentions_folder = lw.iter().any(|w| w == "folder" || w == "folders");

    if first == "rename" {
        return Some(parse_rename(words, &lw));
    }
    if matches!(first, "delete" | "remove") && mentions_folder {
        return Some(Intent::DeleteFolder {
            name: reference(words.get(1..).unwrap_or_default()),
        });
    }
    // "add folder Receipts in Taxes" makes a folder; "add receipts to the
    // Taxes folder" files an entry. Where "folder" comes tells them apart.
    let names_new_folder = matches!(first, "create" | "make" | "new")
        || (first == "add"
            && lw
                .iter()
                .position(|w| w == "folder")
                .is_some_and(|i| i <= 2));
    if names_new_folder {
        if let Some(intent) = parse_create_folder(words, &lw) {
            return Some(intent);
        }
    }
    if matches!(first, "move" | "put" | "file" | "add") {
        if let Some(intent) = parse_move(words, &lw) {
            return Some(intent);
        }
    }
    parse_show_folders(words, &lw)
}

fn parse_rename(words: &[&str], lw: &[String]) -> Intent {
    let to = last_position_of(lw, &["to", "as"]).filter(|i| *i > 0);
    let folder = lw.iter().skip(1).take(3).any(|w| w == "folder");
    let (left, right): (&[&str], &[&str]) = to.map_or_else(
        || (words.get(1..).unwrap_or_default(), &[][..]),
        |i| {
            (
                words.get(1..i).unwrap_or_default(),
                words.get(i.saturating_add(1)..).unwrap_or_default(),
            )
        },
    );
    let name = if folder {
        reference(left)
    } else {
        entry_reference(left)
    };
    let new_name = right
        .iter()
        .map(|w| clean_word(w))
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    Intent::Rename {
        name,
        new_name,
        folder,
    }
}

fn parse_move(words: &[&str], lw: &[String]) -> Option<Intent> {
    // "take it out of its folder": no folder at all.
    let out_of = lw
        .iter()
        .position(|w| w == "out")
        .filter(|i| lw.get(i.saturating_add(1)).is_some_and(|next| next == "of"));
    if let Some(i) = out_of {
        return Some(Intent::MoveEntry {
            name: entry_reference(words.get(1..i).unwrap_or_default()),
            folder: None,
        });
    }
    let prep = last_position_of(lw, TARGET_PREPS).filter(|i| *i > 1)?;
    let target = reference(words.get(prep.saturating_add(1)..).unwrap_or_default());
    // "add a note to my vault" or "put it in the vault" is not a folder.
    if target.is_empty()
        || target.eq_ignore_ascii_case("vault")
        || target.eq_ignore_ascii_case("my vault")
    {
        return None;
    }
    let verb = lw.first()?.as_str();
    let named_folder = lw.iter().any(|w| w == "folder");
    // "add" and "put" are everyday words: only a folder is a reason to
    // read them as a move.
    if matches!(verb, "add" | "put") && !named_folder {
        return None;
    }
    let name = entry_reference(words.get(1..prep).unwrap_or_default());
    if name.is_empty() {
        return None;
    }
    let lower_target = target.to_lowercase();
    let folder = if matches!(
        lower_target.as_str(),
        "top level" | "the top level" | "top" | "no folder" | "root"
    ) {
        None
    } else {
        Some(target)
    };
    Some(Intent::MoveEntry { name, folder })
}

fn parse_create_folder(words: &[&str], lw: &[String]) -> Option<Intent> {
    let first = lw.first()?.as_str();
    let folder_at = position_of(lw, &["folder"], 1)?;
    if !matches!(first, "create" | "make" | "new" | "add") {
        return None;
    }
    let after = words.get(folder_at.saturating_add(1)..).unwrap_or_default();
    let after_lw = lw.get(folder_at.saturating_add(1)..).unwrap_or_default();
    let prep = position_of(after_lw, PARENT_PREPS, 0);
    let (name_words, parent_words): (&[&str], &[&str]) = prep.map_or((after, &[][..]), |i| {
        (
            after.get(..i).unwrap_or_default(),
            after.get(i.saturating_add(1)..).unwrap_or_default(),
        )
    });
    let parent = reference(parent_words);
    Some(Intent::CreateFolder {
        name: reference(name_words),
        parent: (!parent.is_empty()).then_some(parent),
    })
}

fn parse_show_folders(words: &[&str], lw: &[String]) -> Option<Intent> {
    let first = lw.first()?.as_str();
    let asks = matches!(
        first,
        "show" | "list" | "display" | "view" | "open" | "see" | "what" | "what's" | "which" | "how"
    );
    if !asks {
        return None;
    }
    let overview = lw.iter().any(|w| {
        matches!(
            w.as_str(),
            "folders" | "tree" | "hierarchy" | "structure" | "organized" | "organised" | "layout"
        )
    });
    if overview {
        return Some(Intent::ShowFolders { folder: None });
    }
    // A single folder needs the word "folder": "what's in the Taxes
    // folder", "open the Taxes folder", "show folder Taxes". Without it
    // "what's in my shopping list" is a question about a note.
    let at = lw.iter().position(|w| w == "folder")?;
    let inside = lw
        .iter()
        .position(|w| matches!(w.as_str(), "in" | "inside"))
        .filter(|i| *i < at);
    // "show folder Taxes" names it after; "open the Taxes folder" before.
    let after = reference(words.get(at.saturating_add(1)..).unwrap_or_default());
    let target = if after.is_empty() {
        let from = inside.map_or(1, |i| i.saturating_add(1));
        reference(words.get(from..at).unwrap_or_default())
    } else {
        after
    };
    let lower = target.to_lowercase();
    if target.is_empty() || matches!(lower.as_str(), "vault" | "my vault" | "the vault") {
        return Some(Intent::ShowFolders { folder: None });
    }
    Some(Intent::ShowFolders {
        folder: Some(target),
    })
}
