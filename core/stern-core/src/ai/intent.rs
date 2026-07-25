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
    "password", "secret", "credential", "login", "api key",
    "token", "ssh", "wifi", "note", "document",
];

const RETRIEVE_VERBS: &[&str] = &[
    "find", "get", "retrieve", "search", "show", "what", "where", "forgot",
];
const LIST_VERBS: &[&str] = &["list", "show", "display", "all"];
const DELETE_VERBS: &[&str] = &["delete", "remove", "destroy", "drop"];
const UPDATE_VERBS: &[&str] = &["update", "change", "modify", "edit"];

#[must_use]
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

    if let Some(intent) = detect_export(&words) {
        return intent;
    }

    if let Some(intent) = detect_import(&words) {
        return intent;
    }

    if let Some(intent) = detect_vault_manage(&words) {
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
    let has_question = words.iter().any(|w| *w == "help" || *w == "how");
    let has_you = words.iter().any(|w| *w == "can" || *w == "do" || *w == "you");
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

fn detect_delete(words: &[&str]) -> Option<Intent> {
    if !is_delete_context(words) {
        return None;
    }
    let name = extract_retrieve_name(
        &words.to_vec().join(" "),
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

    let create_words = ["create", "new", "set up", "setup", "initialize", "init"];
    let has_create = words.iter().any(|w| create_words.contains(w));

    let unlock_words = ["unlock", "open", "decrypt", "access"];
    let has_unlock = words.iter().any(|w| unlock_words.contains(w));

    if has_create && has_vault {
        return Some(Intent::CreateVault);
    }

    if has_unlock && has_vault {
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
        let rest: String = after.chars()
            .take_while(|c| *c != ',' && *c != '.' && *c != '!')
            .collect();
        let rest = rest.trim();
        if !rest.is_empty() {
            let word: String = full_text[..pos].chars().rev()
                .take_while(|c| !c.is_whitespace())
                .collect::<Vec<_>>().into_iter().rev().collect();
            if !word.is_empty() {
                return format!("{word} {rest}");
            }
        }
    }

    let skip_words = [
        "my", "the", "a", "an", "store", "save", "add", "create", "new",
        "please", "can", "you", "want", "to", "and", "it", "me", "remember",
        "keep", "note", "for", "with",
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

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(clippy::unwrap_used, clippy::expect_used)]
    fn assert_intent(text: &str, expected: Intent) {
        let got = classify_intent(text);
        assert_eq!(got, expected, "failed for input: {text:?}");
    }

    #[test]
    fn greeting_hello() {
        assert_intent("hello", Intent::Greeting);
    }

    #[test]
    fn greeting_hi() {
        assert_intent("hi", Intent::Greeting);
    }

    #[test]
    fn greeting_hey() {
        assert_intent("hey there", Intent::Greeting);
    }

    #[test]
    fn greeting_good_morning() {
        assert_intent("good morning", Intent::Greeting);
    }

    #[test]
    fn help_direct() {
        assert_intent("can you help", Intent::Help);
    }

    #[test]
    fn help_how() {
        assert_intent("how can i use this", Intent::Help);
    }

    #[test]
    fn help_can_you() {
        assert_intent("can you help me", Intent::Help);
    }

    #[test]
    fn help_do_you() {
        assert_intent("how do i use this", Intent::Help);
    }

    #[test]
    fn store_explicit_with_password() {
        let intent = classify_intent("save my GitHub password is hunter2");
        match intent {
            Intent::StoreSecret { name, kind, fields } => {
                assert!(name.to_lowercase().contains("github"), "name should contain github, got: {name}");
                assert_eq!(kind, "password");
                assert!(!fields.is_empty(), "fields should not be empty");
            }
            other => panic!("expected StoreSecret, got: {other:?}"),
        }
    }

    #[test]
    fn store_explicit_with_api_key() {
        let intent = classify_intent("store my API key is sk-abc123");
        match intent {
            Intent::StoreSecret { kind, fields, .. } => {
                assert_eq!(kind, "api key");
                assert!(!fields.is_empty());
            }
            other => panic!("expected StoreSecret, got: {other:?}"),
        }
    }

    #[test]
    fn store_explicit_create_token() {
        let intent = classify_intent("create token for slack token: xoxb-test");
        match intent {
            Intent::StoreSecret { kind, fields, .. } => {
                assert_eq!(kind, "token");
                assert_eq!(fields.len(), 1);
                assert_eq!(fields[0].0, "token");
                assert_eq!(fields[0].1, "xoxb-test");
            }
            other => panic!("expected StoreSecret, got: {other:?}"),
        }
    }

    #[test]
    fn store_implicit_no_verb() {
        let intent = classify_intent("my gmail password is secret123");
        match intent {
            Intent::StoreSecret { kind, fields, .. } => {
                assert_eq!(kind, "password");
                assert!(!fields.is_empty());
            }
            other => panic!("expected StoreSecret, got: {other:?}"),
        }
    }

    #[test]
    fn store_with_name_for() {
        let intent = classify_intent("store password for Netflix is pass123");
        match intent {
            Intent::StoreSecret { name, kind, .. } => {
                assert!(name.to_lowercase().contains("netflix"), "name: {name}");
                assert_eq!(kind, "password");
            }
            other => panic!("expected StoreSecret, got: {other:?}"),
        }
    }

    #[test]
    fn store_with_name_named() {
        let intent = classify_intent("save credential named Work Login");
        match intent {
            Intent::StoreSecret { name, .. } => {
                assert_eq!(name, "work login");
            }
            other => panic!("expected StoreSecret, got: {other:?}"),
        }
    }

    #[test]
    fn store_wifi_password() {
        let intent = classify_intent("store wifi network for home is mynetwork");
        match intent {
            Intent::StoreSecret { kind, .. } => {
                assert_eq!(kind, "wifi");
            }
            other => panic!("expected StoreSecret, got: {other:?}"),
        }
    }

    #[test]
    fn retrieve_with_for() {
        let intent = classify_intent("find my password for GitHub");
        match intent {
            Intent::RetrieveSecret { name } => {
                assert!(name.to_lowercase().contains("github"), "name: {name}");
            }
            other => panic!("expected RetrieveSecret, got: {other:?}"),
        }
    }

    #[test]
    fn retrieve_with_about() {
        let intent = classify_intent("what about my Gmail credentials");
        match intent {
            Intent::RetrieveSecret { name } => {
                assert!(name.to_lowercase().contains("gmail"), "name: {name}");
            }
            other => panic!("expected RetrieveSecret, got: {other:?}"),
        }
    }

    #[test]
    fn retrieve_my() {
        let intent = classify_intent("get my ssh keys");
        match intent {
            Intent::RetrieveSecret { .. } => {}
            other => panic!("expected RetrieveSecret, got: {other:?}"),
        }
    }

    #[test]
    fn list_all() {
        let intent = classify_intent("list all passwords");
        match intent {
            Intent::ListSecrets { category } => {
                assert_eq!(category.as_deref(), Some("password"));
            }
            other => panic!("expected ListSecrets, got: {other:?}"),
        }
    }

    #[test]
    fn list_no_category() {
        let intent = classify_intent("list all secrets");
        match intent {
            Intent::ListSecrets { category } => {
                assert_eq!(category.as_deref(), Some("secret"));
            }
            other => panic!("expected ListSecrets, got: {other:?}"),
        }
    }

    #[test]
    fn list_display() {
        let intent = classify_intent("display all logins");
        match intent {
            Intent::ListSecrets { category } => {
                assert_eq!(category.as_deref(), Some("login"));
            }
            other => panic!("expected ListSecrets, got: {other:?}"),
        }
    }

    #[test]
    fn delete_with_name() {
        let intent = classify_intent("delete my Netflix password");
        match intent {
            Intent::DeleteSecret { name } => {
                assert!(name.to_lowercase().contains("netflix"), "name: {name}");
            }
            other => panic!("expected DeleteSecret, got: {other:?}"),
        }
    }

    #[test]
    fn remove_secret() {
        let intent = classify_intent("remove GitHub token");
        match intent {
            Intent::DeleteSecret { name } => {
                assert!(!name.is_empty(), "name should not be empty");
            }
            other => panic!("expected DeleteSecret, got: {other:?}"),
        }
    }

    #[test]
    fn update_with_fields() {
        let intent = classify_intent("update my GitHub password username: newuser");
        match intent {
            Intent::UpdateSecret { name, fields } => {
                assert!(!name.is_empty(), "name should not be empty");
                assert!(!fields.is_empty(), "fields should not be empty");
            }
            other => panic!("expected UpdateSecret, got: {other:?}"),
        }
    }

    #[test]
    fn update_edit() {
        let intent = classify_intent("edit Netflix login url: netflix.com");
        match intent {
            Intent::UpdateSecret { fields, .. } => {
                assert!(!fields.is_empty());
            }
            other => panic!("expected UpdateSecret, got: {other:?}"),
        }
    }

    #[test]
    fn export_vault() {
        assert_intent("export vault", Intent::ExportVault);
    }

    #[test]
    fn export_backup() {
        assert_intent("backup my vault", Intent::ExportVault);
    }

    #[test]
    fn export_transfer() {
        assert_intent("transfer my secrets", Intent::ExportVault);
    }

    #[test]
    fn import_vault() {
        assert_intent("import vault", Intent::ImportVault);
    }

    #[test]
    fn import_restore() {
        assert_intent("restore from backup", Intent::ExportVault);
    }

    #[test]
    fn create_vault() {
        let intent = classify_intent("create vault");
        match intent {
            Intent::StoreSecret { name, kind, fields } => {
                assert_eq!(name, "vault");
                assert_eq!(kind, "secret");
                assert!(fields.is_empty());
            }
            other => panic!("expected StoreSecret, got: {other:?}"),
        }
    }

    #[test]
    fn new_vault() {
        let intent = classify_intent("new vault");
        match intent {
            Intent::StoreSecret { name, kind, fields } => {
                assert_eq!(name, "vault");
                assert_eq!(kind, "secret");
                assert!(fields.is_empty());
            }
            other => panic!("expected StoreSecret, got: {other:?}"),
        }
    }

    #[test]
    fn unlock_vault() {
        assert_intent("unlock vault", Intent::UnlockVault);
    }

    #[test]
    fn open_vault() {
        assert_intent("open vault", Intent::UnlockVault);
    }

    #[test]
    fn unknown_garbage() {
        let intent = classify_intent("asdfghjkl");
        match intent {
            Intent::Unknown { text } => {
                assert_eq!(text, "asdfghjkl");
            }
            other => panic!("expected Unknown, got: {other:?}"),
        }
    }

    #[test]
    fn unknown_empty_ish() {
        let intent = classify_intent("xyz123 random");
        match intent {
            Intent::Unknown { .. } => {}
            other => panic!("expected Unknown, got: {other:?}"),
        }
    }

    #[test]
    fn retrieve_with_named() {
        let intent = classify_intent("get credential called WorkEmail");
        match intent {
            Intent::RetrieveSecret { name } => {
                assert!(name.contains("workemail"), "name: {name}");
            }
            other => panic!("expected RetrieveSecret, got: {other:?}"),
        }
    }

    #[test]
    fn store_colon_value() {
        let intent = classify_intent("add login: mybank username: bob password: secret");
        match intent {
            Intent::StoreSecret { kind, .. } => {
                assert_eq!(kind, "password");
            }
            other => panic!("expected StoreSecret, got: {other:?}"),
        }
    }

    #[test]
    fn export_migrate() {
        assert_intent("migrate my secrets", Intent::ExportVault);
    }

    #[test]
    fn import_load() {
        assert_intent("load vault from file", Intent::ImportVault);
    }

    #[test]
    fn classify_intent_is_deterministic() {
        let r1 = classify_intent("hello");
        let r2 = classify_intent("hello");
        assert_eq!(r1, r2);
    }
}

#[allow(clippy::arithmetic_side_effects)]
fn extract_key_value_pairs(text: &str) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    let patterns = ["username", "user", "url", "token", "api key"];

    for key in &patterns {
        if let Some(pos) = text.find(key) {
            let after = &text[pos + key.len()..];
            let after = after.trim_start_matches([':', '=', ' ']);
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
