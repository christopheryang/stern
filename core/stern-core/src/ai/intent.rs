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
    ExportVault {
        password: Option<String>,
    },
    ImportVault {
        path: Option<String>,
        password: Option<String>,
    },
    CreateVault {
        password: Option<String>,
    },
    UnlockVault {
        password: Option<String>,
    },
    Help,
    Greeting,
    Unknown {
        text: String,
    },
}

const SECRET_TYPES: &[&str] = &[
    "password", "secret", "credential", "login", "api key",
    "token", "ssh", "wifi", "note", "document",
];

const RETRIEVE_VERBS: &[&str] = &[
    "find", "get", "retrieve", "search", "show", "what", "where", "forgot",
];
const LIST_VERBS: &[&str] = &["list", "show", "display", "all"];
const DELETE_VERBS: &[&str] = &["delete", "remove", "destroy", "drop"];
const UPDATE_VERBS: &[&str] = &["update", "change", "modify", "edit"];

pub fn classify_intent(text: &str) -> Intent {
    let lower = text.to_lowercase();
    let words: Vec<&str> = lower.split_whitespace().collect();

    if is_greeting(&words) {
        return Intent::Greeting;
    }
    if is_help(&words) {
        return Intent::Help;
    }

    if let Some(intent) = detect_store(&words, &lower) {
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
    if let Some(intent) = detect_export(&words, &lower) {
        return intent;
    }
    if let Some(intent) = detect_import(&words, &lower) {
        return intent;
    }
    if let Some(intent) = detect_vault_manage(&words, &lower) {
        return intent;
    }

    Intent::Unknown {
        text: text.to_string(),
    }
}

fn is_greeting(words: &[&str]) -> bool {
    let greetings = ["hello", "hi", "hey", "good", "morning", "evening", "afternoon"];
    words.iter().any(|w| greetings.contains(w))
}

fn is_help(words: &[&str]) -> bool {
    let has_question = words.iter().any(|w| *w == "help" || *w == "how" || *w == "what");
    let has_you = words.iter().any(|w| *w == "can" || *w == "do" || *w == "you");
    has_question || (has_you && words.iter().any(|w| *w == "i" || *w == "me"))
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
        ["store", "save", "add", "create", "new", "remember", "keep", "note"].contains(w)
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
        .map(|t| (*t).to_string())
        .unwrap_or_else(|| "secret".to_string());

    let name = extract_store_name(full_text);
    let fields = extract_store_value(full_text, &kind);

    Some(Intent::StoreSecret { name, kind, fields })
}

fn detect_retrieve(words: &[&str], full_text: &str) -> Option<Intent> {
    if !is_retrieve_context(words) {
        return None;
    }

    let has_secret = has_secret_type(full_text);
    let has_name_hint = full_text.contains(" for ") || full_text.contains(" called ")
        || full_text.contains(" named ") || full_text.contains("'s");

    if !has_secret && !has_name_hint && !words.iter().any(|w| ["my", "me"].contains(w)) {
        return None;
    }

    let name = extract_retrieve_name(full_text);
    if name.is_empty() {
        return None;
    }
    Some(Intent::RetrieveSecret { name })
}

fn detect_list(words: &[&str], full_text: &str) -> Option<Intent> {
    if !is_list_context(words) {
        return None;
    }
    let has_all = words.iter().any(|w| *w == "all" || *w == "every" || *w == "every");
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

fn detect_delete(words: &[&str]) -> Option<Intent> {
    if !is_delete_context(words) {
        return None;
    }
    let name = extract_retrieve_name(
        &words.iter().copied().collect::<Vec<_>>().join(" "),
    );
    if name.is_empty() {
        return None;
    }
    Some(Intent::DeleteSecret { name })
}

fn detect_update(words: &[&str], full_text: &str) -> Option<Intent> {
    if !is_update_context(words) {
        return None;
    }
    let name = extract_retrieve_name(full_text);
    let fields = extract_key_value_pairs(full_text);
    Some(Intent::UpdateSecret { name, fields })
}

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
        let rest: String = after.chars()
            .take_while(|c| *c != ',' && *c != '.' && *c != '!')
            .collect();
        let rest = rest.trim();
        if !rest.is_empty() {
            let word: String = full_text[..pos].chars().rev()
                .take_while(|c| !c.is_whitespace())
                .collect::<Vec<_>>().into_iter().rev().collect();
            if !word.is_empty() {
                return format!("{} {}", word, rest);
            }
        }
    }

    let skip_words = [
        "my", "the", "a", "an", "store", "save", "add", "create", "new",
        "please", "can", "you", "want", "to", "and", "it", "me", "remember",
        "keep", "note", "for", "with",
    ];

    let secret_words: Vec<&str> = SECRET_TYPES.iter().copied().collect();

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
            !skip_words.contains(&lw.as_str())
                && !secret_words.contains(&lw.as_str())
        })
        .collect();

    if !name_words.is_empty() {
        return name_words.join(" ");
    }

    full_text
        .split_whitespace()
        .filter(|w| {
            let lw = w.to_lowercase();
            !skip_words.contains(&lw.as_str())
                && !secret_words.contains(&lw.as_str())
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn extract_store_value(full_text: &str, kind: &str) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    let lower = full_text.to_lowercase();

    for (suffix, field_key) in &[
        ("password is:", "password"),
        ("password is ", "password"),
        ("secret is:", "secret"),
        ("secret is ", "secret"),
        ("credential is:", "credential"),
        ("credential is ", "credential"),
        ("token is:", "token"),
        ("token is ", "token"),
        ("api key is:", "api key"),
        ("api key is ", "api key"),
        ("wifi password is:", "wifi"),
        ("wifi password is ", "wifi"),
    ] {
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

    for pattern in &["password:", "secret:", "credential:", "token:", "api key:"] {
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
        "find", "get", "retrieve", "search", "show", "what", "where",
        "my", "the", "a", "an", "all", "list", "display", "forgot",
        "password", "secret", "credential", "login", "api", "key",
        "token", "ssh", "wifi", "note", "document", "do", "you", "have",
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

fn extract_key_value_pairs(text: &str) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    let patterns = ["username", "user", "url", "token", "api key"];

    for key in &patterns {
        if let Some(pos) = text.find(key) {
            let after = &text[pos + key.len()..];
            let after = after.trim_start_matches(|c: char| c == ':' || c == '=' || c == ' ');
            let value: String = after
                .chars()
                .take_while(|c| !c.is_whitespace())
                .collect();
            if !value.is_empty() {
                pairs.push((key.to_string(), value));
            }
        }
    }

    pairs
}

fn detect_export(words: &[&str], full_text: &str) -> Option<Intent> {
    let export_words = ["export", "backup", "back up", "copy", "transfer", "migrate"];
    if !words.iter().any(|w| export_words.contains(w)) {
        return None;
    }

    let password = extract_password(full_text);
    Some(Intent::ExportVault { password })
}

fn detect_import(words: &[&str], full_text: &str) -> Option<Intent> {
    let import_words = ["import", "restore", "load", "transfer"];
    let has_import = words.iter().any(|w| import_words.contains(w));
    if !has_import {
        return None;
    }

    let path = if let Some(pos) = full_text.find(".json") {
        let before = &full_text[..pos + 5];
        let start = before.rfind(|c: char| c == '/' || c == '\\' || c == ' ')
            .map(|p| p + 1)
            .unwrap_or(0);
        Some(before[start..].to_string())
    } else {
        None
    };

    let password = extract_password(full_text);
    Some(Intent::ImportVault { path, password })
}

fn extract_password(text: &str) -> Option<String> {
    let lower = text.to_lowercase();

    for pattern in &["password ", "pass ", "pwd ", "with password ", "using password "] {
        if let Some(pos) = lower.find(pattern) {
            let after = &text[pos + pattern.len()..];
            let pw: String = after
                .chars()
                .take_while(|c| *c != ',' && *c != '.' && *c != '!' && *c != '?')
                .collect();
            let pw = pw.trim().to_string();
            if !pw.is_empty() {
                return Some(pw);
            }
        }
    }

    None
}

fn detect_vault_manage(words: &[&str], full_text: &str) -> Option<Intent> {
    let has_vault = words.iter().any(|w| *w == "vault");

    let create_words = ["create", "new", "set up", "setup", "initialize", "init"];
    let has_create = words.iter().any(|w| create_words.contains(w));

    let unlock_words = ["unlock", "open", "decrypt", "access"];
    let has_unlock = words.iter().any(|w| unlock_words.contains(w));

    if has_create && has_vault {
        let password = extract_password(full_text);
        return Some(Intent::CreateVault { password });
    }

    if has_unlock && has_vault {
        let password = extract_password(full_text);
        return Some(Intent::UnlockVault { password });
    }

    None
}
