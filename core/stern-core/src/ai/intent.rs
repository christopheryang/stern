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
    Help,
    Greeting,
    Unknown {
        text: String,
    },
}

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

    if let Some(intent) = detect_retrieve(&words) {
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

    Intent::Unknown {
        text: text.to_string(),
    }
}

fn is_greeting(words: &[&str]) -> bool {
    let greetings = ["hello", "hi", "hey", "good morning", "good evening"];
    words.iter().any(|w| greetings.contains(w))
}

fn is_help(words: &[&str]) -> bool {
    words.iter().any(|w| *w == "help" || *w == "what" || *w == "how")
        && words.iter().any(|w| *w == "can" || *w == "do" || *w == "you")
}

fn detect_store(words: &[&str], full_text: &str) -> Option<Intent> {
    let store_verbs = ["store", "save", "add", "create", "new"];
    let has_store_verb = words.iter().any(|w| store_verbs.contains(w));
    if !has_store_verb {
        return None;
    }

    let secret_types = [
        "password", "secret", "credential", "login", "api key",
        "token", "ssh", "wifi", "note", "document",
    ];
    let kind = secret_types
        .iter()
        .find(|t| full_text.contains(*t))
        .map(|t| (*t).to_string())
        .unwrap_or_else(|| "secret".to_string());

    let name = extract_name(words, &store_verbs);
    let fields = extract_key_value_pairs(full_text);

    Some(Intent::StoreSecret {
        name,
        kind,
        fields,
    })
}

fn detect_retrieve(words: &[&str]) -> Option<Intent> {
    let retrieve_verbs = ["find", "get", "retrieve", "search", "show", "what", "where"];
    let has_retrieve = words.iter().any(|w| retrieve_verbs.contains(w));
    if !has_retrieve {
        return None;
    }

    let name = extract_name(words, &retrieve_verbs);
    Some(Intent::RetrieveSecret { name })
}

fn detect_list(words: &[&str], full_text: &str) -> Option<Intent> {
    let list_verbs = ["list", "show", "display", "all"];
    let has_list = words.iter().any(|w| list_verbs.contains(w));
    if !has_list {
        return None;
    }

    let categories = ["password", "secret", "credential", "api", "ssh", "note", "document"];
    let category = categories
        .iter()
        .find(|c| full_text.contains(*c))
        .map(|c| (*c).to_string());

    Some(Intent::ListSecrets { category })
}

fn detect_delete(words: &[&str]) -> Option<Intent> {
    let delete_verbs = ["delete", "remove", "destroy", "drop"];
    let has_delete = words.iter().any(|w| delete_verbs.contains(w));
    if !has_delete {
        return None;
    }

    let name = extract_name(words, &delete_verbs);
    Some(Intent::DeleteSecret { name })
}

fn detect_update(words: &[&str], full_text: &str) -> Option<Intent> {
    let update_verbs = ["update", "change", "modify", "edit"];
    let has_update = words.iter().any(|w| update_verbs.contains(w));
    if !has_update {
        return None;
    }

    let name = extract_name(words, &update_verbs);
    let fields = extract_key_value_pairs(full_text);
    Some(Intent::UpdateSecret { name, fields })
}

fn extract_name<'a>(words: &[&'a str], skip_verbs: &[&str]) -> String {
    let skip = [
        "store", "save", "add", "create", "new", "find", "get",
        "retrieve", "search", "show", "list", "display", "all",
        "delete", "remove", "destroy", "drop", "update", "change",
        "modify", "edit", "my", "the", "a", "an", "for", "with",
        "password", "secret", "credential", "login", "api", "key",
        "token", "ssh", "wifi", "note", "document", "called", "named",
    ];

    words
        .iter()
        .filter(|w| !skip.contains(w) && !skip_verbs.contains(w))
        .copied()
        .collect::<Vec<_>>()
        .join(" ")
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
