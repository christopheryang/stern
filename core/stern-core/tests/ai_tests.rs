#![allow(clippy::unwrap_used)]

use stern_core::ai::chat::{Action, ChatHandler, EntryCandidate};
use stern_core::ai::intent::{classify_intent, Intent};

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
            assert!(
                name.to_lowercase().contains("github"),
                "name should contain github, got: {name}"
            );
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
        Intent::RetrieveSecret { name, .. } => {
            assert!(name.to_lowercase().contains("github"), "name: {name}");
        }
        other => panic!("expected RetrieveSecret, got: {other:?}"),
    }
}

#[test]
fn retrieve_with_about() {
    let intent = classify_intent("what about my Gmail credentials");
    match intent {
        Intent::RetrieveSecret { name, .. } => {
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
        Intent::RetrieveSecret { name, .. } => {
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

fn handler() -> ChatHandler {
    ChatHandler::new(None)
}

#[test]
fn greeting_response() {
    let resp = handler().process_message("hello");
    assert!(resp.message.contains("Stern"), "message: {}", resp.message);
    assert!(resp.action.is_none());
    assert!(resp.user_message_display.is_none());
}

#[test]
fn help_response() {
    let resp = handler().process_message("help");
    assert!(
        resp.message.contains("secrets"),
        "message: {}",
        resp.message
    );
    assert!(resp.action.is_none());
}

#[test]
fn store_response() {
    let resp = handler().process_message("save my GitHub password is hunter2");
    assert!(resp.message.contains("github"), "message: {}", resp.message);
    match resp.action.expect("should have action") {
        Action::StoreEntry { name, kind, fields } => {
            assert!(name.to_lowercase().contains("github"));
            assert_eq!(kind, "password");
            assert!(!fields.is_empty());
        }
        other => panic!("expected StoreEntry, got: {other:?}"),
    }
    assert!(resp.user_message_display.is_none());
}

#[test]
fn retrieve_response() {
    let resp = handler().process_message("find my password for GitHub");
    assert!(resp.message.contains("github"), "message: {}", resp.message);
    match resp.action.expect("should have action") {
        Action::SearchEntry { query, .. } => {
            assert!(!query.is_empty());
        }
        other => panic!("expected SearchEntry, got: {other:?}"),
    }
}

#[test]
fn list_response() {
    let resp = handler().process_message("list all passwords");
    assert!(
        resp.message.contains("password"),
        "message: {}",
        resp.message
    );
    match resp.action.expect("should have action") {
        Action::ListEntries { category } => {
            assert!(category.is_some());
        }
        other => panic!("expected ListEntries, got: {other:?}"),
    }
}

#[test]
fn list_no_category_response() {
    let resp = handler().process_message("show all secrets");
    match resp.action.expect("should have action") {
        Action::SearchEntry { query, .. } => {
            assert!(!query.is_empty());
        }
        other => panic!("expected SearchEntry, got: {other:?}"),
    }
}

#[test]
fn delete_response() {
    let resp = handler().process_message("delete my Netflix password");
    assert!(resp.message.contains("delete"), "message: {}", resp.message);
    match resp.action.expect("should have action") {
        Action::ConfirmDelete { name } => {
            assert!(!name.is_empty());
        }
        other => panic!("expected ConfirmDelete, got: {other:?}"),
    }
}

#[test]
fn update_response() {
    let resp = handler().process_message("update my GitHub username: newuser");
    assert!(resp.message.contains("github"), "message: {}", resp.message);
    match resp.action.expect("should have action") {
        Action::UpdateEntry { name, fields } => {
            assert!(!name.is_empty());
            assert!(!fields.is_empty());
        }
        other => panic!("expected UpdateEntry, got: {other:?}"),
    }
}

#[test]
fn update_no_fields_response() {
    let resp = handler().process_message("change my Netflix login");
    match resp.action.expect("should have action") {
        Action::UpdateEntry { .. } => {}
        other => panic!("expected UpdateEntry, got: {other:?}"),
    }
}

#[test]
fn export_response() {
    let resp = handler().process_message("export vault");
    assert!(resp.message.contains("export"), "message: {}", resp.message);
    assert!(matches!(
        resp.action.expect("should have action"),
        Action::ExportVault
    ));
    assert!(resp.user_message_display.is_none());
}

#[test]
fn import_response() {
    let resp = handler().process_message("import vault");
    assert!(resp.message.contains("import"), "message: {}", resp.message);
    assert!(matches!(
        resp.action.expect("should have action"),
        Action::ImportVault
    ));
    assert!(resp.user_message_display.is_none());
}

#[test]
fn create_vault_response() {
    let resp = handler().process_message("create vault");
    assert!(resp.message.contains("vault"), "message: {}", resp.message);
    match resp.action.expect("should have action") {
        Action::StoreEntry { name, kind, .. } => {
            assert_eq!(name, "vault");
            assert_eq!(kind, "secret");
        }
        other => panic!("expected StoreEntry, got: {other:?}"),
    }
    assert!(resp.user_message_display.is_none());
}

#[test]
fn unlock_vault_response() {
    // The handler's default vault state is ready (the app syncs
    // the real state per message), so an unlock request is
    // answered plainly — no dialog fiction, no action.
    let resp = handler().process_message("unlock vault");
    assert!(
        resp.message.contains("already unlocked"),
        "message: {}",
        resp.message
    );
    assert!(resp.action.is_none());
    assert!(resp.user_message_display.is_none());
}

#[test]
fn unknown_response() {
    let resp = handler().process_message("asdfghjkl");
    assert!(
        resp.message.contains("not sure"),
        "message: {}",
        resp.message
    );
    assert!(resp.action.is_none());
}

#[test]
fn vault_actions_clear_display() {
    for input in &[
        "create vault",
        "unlock vault",
        "export vault",
        "import vault",
    ] {
        let resp = handler().process_message(input);
        assert!(
            resp.user_message_display.is_none(),
            "user_message_display should be None for vault action: {input}"
        );
    }
}

#[test]
fn non_vault_actions_have_no_display() {
    for input in &["hello", "help", "asdfghjkl"] {
        let resp = handler().process_message(input);
        assert!(resp.user_message_display.is_none());
    }
}

#[test]
fn guided_store_flow_collects_a_login_field_by_field() {
    let handler = ChatHandler::new(None);
    let first = handler.process_message("store");
    assert_eq!(first.message, "What is this secret for?");
    assert_eq!(first.options.len(), 5);
    let second = handler.process_message("a");
    assert_eq!(
        second.message,
        "What website is this username/password for?"
    );
    let _ = handler.process_message("Google");
    let _ = handler.process_message("chris");
    let confirm = handler.process_message("hunter2");
    assert_eq!(confirm.options, vec!["yes".to_string(), "no".to_string()]);
    let done = handler.process_message("yes");
    assert!(matches!(done.action, Some(Action::StoreEntry { .. })));
}

#[test]
fn cancelling_a_guided_store_returns_to_normal_classification() {
    let handler = ChatHandler::new(None);
    let _ = handler.process_message("store");
    let cancelled = handler.process_message("cancel");
    assert!(cancelled.message.starts_with("Cancelled"));
    let greeting = handler.process_message("hello");
    assert!(greeting.message.contains("I'm Stern"));
}

#[test]
fn retrieve_question_extracts_the_asked_field() {
    match classify_intent("what's my password for google?") {
        Intent::RetrieveSecret { name, field } => {
            assert!(name.to_lowercase().contains("google"), "name: {name}");
            assert_eq!(field.as_deref(), Some("password"));
        }
        other => panic!("expected RetrieveSecret, got: {other:?}"),
    }
}

#[test]
fn pending_choice_resolves_by_letter() {
    let handler = ChatHandler::new(None);
    handler.set_pending_choice(
        vec![
            EntryCandidate {
                id: "id-a".to_string(),
                label: "Google".to_string(),
            },
            EntryCandidate {
                id: "id-b".to_string(),
                label: "Google Work".to_string(),
            },
        ],
        Some("password".to_string()),
    );
    let response = handler.process_message("b");
    match response.action {
        Some(Action::RevealEntry { entry_id, field }) => {
            assert_eq!(entry_id, "id-b");
            assert_eq!(field.as_deref(), Some("password"));
        }
        other => panic!("expected RevealEntry, got: {other:?}"),
    }
}

#[test]
fn pending_choice_falls_through_on_other_messages() {
    let handler = ChatHandler::new(None);
    handler.set_pending_choice(
        vec![EntryCandidate {
            id: "id-a".to_string(),
            label: "Google".to_string(),
        }],
        None,
    );
    let greeting = handler.process_message("hello");
    assert!(greeting.message.contains("I'm Stern"));
    // The choice is forgotten: "a" is no longer a pick.
    let after = handler.process_message("a");
    assert!(!matches!(after.action, Some(Action::RevealEntry { .. })));
}

#[test]
fn greeting_how_are_you() {
    assert_intent("how are you?", Intent::Greeting);
    let handler = ChatHandler::new(None);
    let response = handler.process_message("how are you?");
    assert!(
        response.message.contains("store a secret"),
        "message: {}",
        response.message
    );
    assert!(!response.message.contains("not sure what you mean"));
}

#[test]
fn greeting_thanks_alone_but_not_after_a_command() {
    assert_intent("thanks", Intent::Greeting);
    assert_intent("thank you", Intent::Greeting);
    // "save my password, thanks" is a store, not small talk.
    assert_ne!(
        classify_intent("save my GitHub password is hunter2, thanks"),
        Intent::Greeting
    );
}

#[test]
fn sensitive_field_prompts_are_flagged() {
    let handler = ChatHandler::new(None);
    let kind = handler.process_message("store");
    assert!(!kind.input_sensitive);
    let _ = handler.process_message("a");
    let _ = handler.process_message("Google");
    let password_prompt = handler.process_message("chris");
    assert!(
        password_prompt.input_sensitive,
        "the password question must hide the typing: {}",
        password_prompt.message
    );
    let confirm = handler.process_message("hunter2");
    assert!(!confirm.input_sensitive);
}

#[test]
fn gate_setup_flow_replays_the_interrupted_store() {
    let handler = ChatHandler::new(None);
    handler.set_vault_state(false, false);

    let first = handler.process_message("store");
    assert!(first.input_sensitive, "message: {}", first.message);
    assert!(first.message.contains("set up your vault"));
    assert!(first.action.is_none());

    let weak = handler.process_message("short");
    assert!(weak.input_sensitive);
    assert!(weak.message.contains("still needs"));

    let confirm = handler.process_message("Correct horse 9!");
    assert!(confirm.input_sensitive);
    assert!(confirm.message.contains("confirm"));

    let mismatch = handler.process_message("Correct horse 9?");
    assert!(mismatch.input_sensitive);
    assert!(mismatch.message.contains("didn't match"));

    let _ = handler.process_message("Correct horse 9!");
    let ready = handler.process_message("Correct horse 9!");
    match ready.action {
        Some(Action::SetupVault { password }) => {
            assert_eq!(password, "Correct horse 9!");
        }
        other => panic!("expected SetupVault, got: {other:?}"),
    }

    handler.set_vault_state(true, true);
    let replayed = handler.vault_gate_completed().unwrap();
    assert_eq!(
        replayed.options.len(),
        5,
        "the interrupted store should resume at the kind question: {}",
        replayed.message
    );
}

#[test]
fn gate_unlock_flow_replays_the_interrupted_search() {
    let handler = ChatHandler::new(None);
    handler.set_vault_state(true, false);

    let prompt = handler.process_message("what's my password for google?");
    assert!(prompt.input_sensitive, "message: {}", prompt.message);
    assert!(prompt.message.contains("locked"));

    let attempt = handler.process_message("hunter22");
    match attempt.action {
        Some(Action::UnlockVaultWith { password }) => {
            assert_eq!(password, "hunter22");
        }
        other => panic!("expected UnlockVaultWith, got: {other:?}"),
    }

    handler.set_vault_state(true, true);
    let replayed = handler.vault_gate_completed().unwrap();
    assert!(
        matches!(replayed.action, Some(Action::SearchEntry { .. })),
        "the interrupted search should replay, got: {:?}",
        replayed.action
    );
}

#[test]
fn gate_never_blocks_small_talk() {
    let handler = ChatHandler::new(None);
    handler.set_vault_state(false, false);
    let response = handler.process_message("how are you?");
    assert!(!response.input_sensitive);
    assert!(response.message.contains("I'm Stern"));
}

#[test]
fn gate_cancel_abandons_the_setup() {
    let handler = ChatHandler::new(None);
    handler.set_vault_state(false, false);
    let _ = handler.process_message("store");
    let cancelled = handler.process_message("cancel");
    assert_eq!(cancelled.message, "Cancelled.");
    assert!(handler.vault_gate_completed().is_none());
}
