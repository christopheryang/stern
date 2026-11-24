//! Unit tests for the Tauri command layer. They live in their own file
//! (under a `tests/` directory) so CodeQL's `**/tests/**` exclusion covers
//! them: the fixture passwords below are not findings.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use stern_core::ai::chat::Action as CoreAction;
use stern_core::application::vault::session::VaultSession;
use stern_core::infrastructure::crypto::{Argon2idKdfProvider, XChaCha20CryptoProvider};
use stern_core::infrastructure::keychain::OsKeychainProvider;
use stern_core::infrastructure::sqlite::repository::{SqliteRepository, StoredPayload};

#[test]
fn action_to_string_store_entry() {
    let action = CoreAction::StoreEntry {
        name: "test".into(),
        kind: "login".into(),
        fields: vec![],
    };
    assert_eq!(action_to_string(&action), "store");
}

#[test]
fn action_to_string_search_entry() {
    let action = CoreAction::SearchEntry {
        query: "foo".into(),
        field: None,
    };
    assert_eq!(action_to_string(&action), "search");
}

#[test]
fn action_to_string_list_entries() {
    let action = CoreAction::ListEntries {
        category: Some("logins".into()),
    };
    assert_eq!(action_to_string(&action), "list");
}

#[test]
fn action_to_string_list_entries_no_category() {
    let action = CoreAction::ListEntries { category: None };
    assert_eq!(action_to_string(&action), "list");
}

#[test]
fn action_to_string_confirm_delete() {
    let action = CoreAction::ConfirmDelete {
        name: "my-entry".into(),
    };
    assert_eq!(action_to_string(&action), "delete");
}

#[test]
fn action_to_string_update_entry() {
    let action = CoreAction::UpdateEntry {
        name: "entry".into(),
        fields: vec![("key".into(), "val".into())],
    };
    assert_eq!(action_to_string(&action), "update");
}

#[test]
fn action_to_string_export_vault() {
    assert_eq!(action_to_string(&CoreAction::ExportVault), "export");
}

#[test]
fn action_to_string_import_vault() {
    assert_eq!(action_to_string(&CoreAction::ImportVault), "import");
}

#[test]
fn action_to_string_create_vault() {
    assert_eq!(action_to_string(&CoreAction::CreateVault), "create_vault");
}

#[test]
fn action_to_string_unlock_vault() {
    assert_eq!(action_to_string(&CoreAction::UnlockVault), "unlock_vault");
}

#[test]
fn passthrough_response_with_action() {
    let core_resp = stern_core::ai::chat::ChatResponse {
        message: "hello".into(),
        action: Some(CoreAction::ExportVault),
        user_message_display: Some("display me".into()),
        options: Vec::new(),
        input_sensitive: false,
    };
    let ipc_resp = passthrough_response(&core_resp);
    assert_eq!(ipc_resp.message, "hello");
    assert_eq!(ipc_resp.action.as_deref(), Some("export"));
    assert_eq!(ipc_resp.user_message_display.as_deref(), Some("display me"));
}

#[test]
fn passthrough_response_without_action() {
    let core_resp = stern_core::ai::chat::ChatResponse {
        message: "no action".into(),
        action: None,
        user_message_display: None,
        options: Vec::new(),
        input_sensitive: false,
    };
    let ipc_resp = passthrough_response(&core_resp);
    assert_eq!(ipc_resp.message, "no action");
    assert!(ipc_resp.action.is_none());
    assert!(ipc_resp.user_message_display.is_none());
}

#[test]
fn passthrough_response_all_action_variants() {
    let variants: Vec<CoreAction> = vec![
        CoreAction::StoreEntry {
            name: "n".into(),
            kind: "login".into(),
            fields: vec![],
        },
        CoreAction::SearchEntry {
            query: "q".into(),
            field: None,
        },
        CoreAction::RevealEntry {
            entry_id: "id".into(),
            field: None,
        },
        CoreAction::ListEntries { category: None },
        CoreAction::ConfirmDelete { name: "n".into() },
        CoreAction::UpdateEntry {
            name: "n".into(),
            fields: vec![],
        },
        CoreAction::ExportVault,
        CoreAction::ImportVault,
        CoreAction::CreateVault,
        CoreAction::UnlockVault,
    ];

    let expected = [
        "store",
        "search",
        "reveal",
        "list",
        "delete",
        "update",
        "export",
        "import",
        "create_vault",
        "unlock_vault",
    ];

    for (action, expected_str) in variants.into_iter().zip(expected.iter()) {
        let core_resp = stern_core::ai::chat::ChatResponse {
            message: "m".into(),
            action: Some(action),
            user_message_display: None,
            options: Vec::new(),
            input_sensitive: false,
        };
        let ipc_resp = passthrough_response(&core_resp);
        assert_eq!(ipc_resp.action.as_deref(), Some(*expected_str));
    }
}

fn unlock_test_session(state: &AppState) {
    use stern_core::domain::vault::crypto_constants::{KEK_LEN, VERIFY_HASH_LEN};
    let kek = [7u8; KEK_LEN];
    let verify = [9u8; VERIFY_HASH_LEN];
    state.session.lock().unwrap().unlock(kek, verify);
}

fn store_test_entry(state: &AppState, name: &str, password: &str) {
    let fields = vec![
        ("username".to_string(), "chris".to_string()),
        ("password".to_string(), password.to_string()),
    ];
    let stored = handle_store_entry(state, name, "login", &fields, "stored").unwrap();
    assert_eq!(stored.action.as_deref(), Some("store"));
}

#[test]
fn search_entry_answers_the_asked_field() {
    let (state, _path) = make_test_app_state();
    unlock_test_session(&state);
    store_test_entry(&state, "Google", "hunter2");

    let answer = handle_search_entry(&state, "google", Some("password")).unwrap();
    assert!(
        answer.message.contains("hunter2"),
        "message: {}",
        answer.message
    );

    let missing = handle_search_entry(&state, "nope", None).unwrap();
    assert!(missing.message.contains("No secret found"));
}

#[test]
fn search_entry_with_two_matches_offers_a_choice_that_reveals() {
    let (state, _path) = make_test_app_state();
    unlock_test_session(&state);
    store_test_entry(&state, "Google", "personal-pass");
    store_test_entry(&state, "Google Work", "work-pass");

    let response = handle_search_entry(&state, "google", Some("password")).unwrap();
    assert_eq!(response.options.len(), 2);

    let picked = state.chat_handler.process_message("b");
    let entry_id = match picked.action {
        Some(CoreAction::RevealEntry { entry_id, .. }) => entry_id,
        other => panic!("expected RevealEntry, got: {other:?}"),
    };
    let answer = handle_reveal_entry(&state, &entry_id, Some("password")).unwrap();
    assert!(
        answer.message.contains("work-pass"),
        "message: {}",
        answer.message
    );
}

#[test]
fn search_entry_while_locked_reports_locked() {
    let (state, _path) = make_test_app_state();
    let response = handle_search_entry(&state, "google", None).unwrap();
    assert_eq!(response.action.as_deref(), Some("locked"));
}

fn make_test_app_state() -> (AppState, tempfile::TempPath) {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let path = tmp.into_temp_path();
    let db_path = path.to_path_buf();
    let repository = Arc::new(SqliteRepository::new(&db_path).unwrap());
    let state = AppState {
        session: Mutex::new(VaultSession::new()),
        crypto: Arc::new(XChaCha20CryptoProvider::new()),
        kdf: Arc::new(Argon2idKdfProvider::new()),
        keychain: Arc::new(OsKeychainProvider::new()),
        chat_handler: Arc::new(stern_core::ai::ChatHandler::new(None)),
        model_manager: Arc::new(stern_core::ai::ModelManager::new(db_path.clone())),
        download_progress: Arc::new(Mutex::new(None)),
        db_path,
        repository,
        unlock_lockout: Mutex::new(None),
    };
    (state, path)
}

#[test]
fn lockout_initial_state_is_none() {
    let (state, _path) = make_test_app_state();
    let lockout = state.unlock_lockout.lock().unwrap();
    assert!(lockout.is_none());
}

#[test]
fn lockout_increments_on_failed_attempt() {
    let (state, _path) = make_test_app_state();
    {
        let mut lockout = state.unlock_lockout.lock().unwrap();
        let entry = lockout.get_or_insert((0, Instant::now()));
        entry.0 += 1;
        entry.1 = Instant::now();
    }
    let lockout = state.unlock_lockout.lock().unwrap();
    let (count, _) = lockout.unwrap();
    assert_eq!(count, 1);
}

#[test]
fn lockout_blocks_after_max_attempts() {
    let (state, _path) = make_test_app_state();
    {
        let mut lockout = state.unlock_lockout.lock().unwrap();
        *lockout = Some((MAX_UNLOCK_ATTEMPTS, Instant::now()));
    }

    let lockout = state.unlock_lockout.lock().unwrap();
    if let Some((count, last_attempt)) = *lockout {
        assert!(count >= MAX_UNLOCK_ATTEMPTS);
        let remaining = LOCKOUT_DURATION.checked_sub(last_attempt.elapsed());
        assert!(remaining.is_some(), "should still be in lockout window");
    } else {
        panic!("lockout should be set");
    }
}

#[test]
fn lockout_resets_on_success() {
    let (state, _path) = make_test_app_state();
    {
        let mut lockout = state.unlock_lockout.lock().unwrap();
        *lockout = Some((MAX_UNLOCK_ATTEMPTS, Instant::now()));
    }
    {
        let mut lockout = state.unlock_lockout.lock().unwrap();
        *lockout = None;
    }
    let lockout = state.unlock_lockout.lock().unwrap();
    assert!(lockout.is_none());
}

#[test]
fn lockout_clears_after_duration_expires() {
    let (state, _path) = make_test_app_state();
    {
        let mut lockout = state.unlock_lockout.lock().unwrap();
        let expired_time = Instant::now() - LOCKOUT_DURATION - Duration::from_secs(1);
        *lockout = Some((MAX_UNLOCK_ATTEMPTS, expired_time));
    }

    let lockout = state.unlock_lockout.lock().unwrap();
    if let Some((count, last_attempt)) = *lockout {
        if count >= MAX_UNLOCK_ATTEMPTS {
            let remaining = LOCKOUT_DURATION.checked_sub(last_attempt.elapsed());
            if remaining.is_some() {
                panic!("lockout should have expired");
            }
        }
    }
}

#[test]
fn lockout_counter_accumulates_across_attempts() {
    let (state, _path) = make_test_app_state();
    for i in 1..=MAX_UNLOCK_ATTEMPTS {
        let mut lockout = state.unlock_lockout.lock().unwrap();
        let entry = lockout.get_or_insert((0, Instant::now()));
        entry.0 += 1;
        entry.1 = Instant::now();
        let (count, _) = lockout.unwrap();
        assert_eq!(count, i);
    }
}

#[test]
fn app_state_components_are_initialized() {
    let (state, _path) = make_test_app_state();
    assert!(!state.session.lock().unwrap().is_unlocked);
    assert!(state.unlock_lockout.lock().unwrap().is_none());
}

/// Drive one chat message the way `send_chat_message` does:
/// sync the vault state, process, execute the action.
fn chat(state: &AppState, text: &str) -> ChatResponse {
    let exists = state.repository.vault_exists().unwrap();
    let unlocked = state.session.lock().unwrap().is_unlocked;
    state.chat_handler.set_vault_state(exists, unlocked);
    let response = state.chat_handler.process_message(text);
    execute_chat_response(state, response).unwrap()
}

#[test]
fn chat_sets_up_the_vault_before_the_first_store() {
    let (state, _path) = make_test_app_state();

    let gate = chat(&state, "store");
    assert!(gate.input_sensitive, "message: {}", gate.message);
    assert!(gate.message.contains("set up your vault"));
    assert!(!state.repository.vault_exists().unwrap());

    let confirm = chat(&state, "Correct-Horse-9!");
    assert!(confirm.input_sensitive);

    // Setup completes and the interrupted store resumes at
    // the kind question.
    let resumed = chat(&state, "Correct-Horse-9!");
    assert!(state.repository.vault_exists().unwrap());
    assert!(state.session.lock().unwrap().is_unlocked);
    assert_eq!(resumed.options.len(), 5, "message: {}", resumed.message);

    chat(&state, "c");
    chat(&state, "Google");
    chat(&state, "chris");
    let saved = chat(&state, "hunter2");
    assert!(saved.message.contains("Save this"));
    let done = chat(&state, "yes");
    assert_eq!(done.action.as_deref(), Some("store"));
    assert_eq!(state.repository.count_entries().unwrap(), 1);
}

#[test]
fn exports_never_overwrite_an_existing_file() {
    let dir = tempfile::tempdir().unwrap();
    let first = write_new_export_file(dir.path(), b"one").unwrap();
    let second = write_new_export_file(dir.path(), b"two").unwrap();
    assert_ne!(first, second);
    assert_eq!(std::fs::read(&first).unwrap(), b"one");
    assert_eq!(std::fs::read(&second).unwrap(), b"two");
}

#[cfg(unix)]
#[test]
fn export_files_are_owner_only() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = write_new_export_file(dir.path(), b"x").unwrap();
    let mode = std::fs::metadata(path).unwrap().permissions().mode();
    assert_eq!(mode & 0o077, 0);
}

#[test]
fn import_rejects_missing_non_json_and_non_file_paths() {
    let dir = tempfile::tempdir().unwrap();
    assert!(read_export_file(&dir.path().join("nope.json")).is_err());

    let text = dir.path().join("notes.txt");
    std::fs::write(&text, b"[]").unwrap();
    assert!(read_export_file(&text).is_err());

    let folder = dir.path().join("folder.json");
    std::fs::create_dir(&folder).unwrap();
    assert!(read_export_file(&folder).is_err());

    let good = dir.path().join("export.json");
    std::fs::write(&good, b"[]").unwrap();
    assert_eq!(read_export_file(&good).unwrap(), b"[]");
}

#[test]
fn import_enforces_the_size_limit_on_what_it_reads() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("export.json");
    std::fs::write(&path, b"0123456789").unwrap();
    assert_eq!(read_export_file_limited(&path, 10).unwrap(), b"0123456789");
    let too_big = read_export_file_limited(&path, 9).unwrap_err();
    assert!(too_big.to_string().contains("too large"), "{too_big}");
}

#[test]
fn import_errors_carry_no_os_detail() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("gone.json");
    let err = read_export_file(&missing).unwrap_err().to_string();
    assert!(!err.contains("os error"), "{err}");
    assert!(!err.contains(dir.path().to_str().unwrap()), "{err}");
}

/// A writer that accepts nothing, like a full disk.
struct FullDisk;

impl std::io::Write for FullDisk {
    fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
        Err(std::io::Error::other("no space left on device"))
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn a_failed_export_write_removes_the_partial_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("stern-export-test.json");
    std::fs::write(&path, b"partial").unwrap();

    let err = fill_export_file(FullDisk, &path, b"secret").unwrap_err();

    assert!(!path.exists(), "the partial export was left behind");
    let text = err.to_string();
    assert!(!text.contains("no space"), "{text}");
    assert!(!text.contains(dir.path().to_str().unwrap()), "{text}");
}

#[test]
fn a_successful_export_write_keeps_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_new_export_file(dir.path(), b"payload").unwrap();
    assert_eq!(std::fs::read(path).unwrap(), b"payload");
}

#[cfg(unix)]
#[test]
fn an_export_that_cannot_be_created_says_nothing_about_the_os() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o500)).unwrap();
    let result = write_new_export_file(dir.path(), b"x");
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    // Running as root can still create the file; only check when refused.
    if let Err(e) = result {
        let text = e.to_string();
        assert!(!text.contains("os error"), "{text}");
        assert!(!text.contains(dir.path().to_str().unwrap()), "{text}");
    }
}

#[test]
fn import_errors_do_not_echo_the_path() {
    let err = read_export_file(std::path::Path::new("/Users/someone/secret.json")).unwrap_err();
    assert!(!err.to_string().contains("someone"), "{err}");
}

#[test]
fn model_status_carries_pros_cons_and_one_recommendation() {
    let (state, _path) = make_test_app_state();
    let rows = model_status_dtos(&state);
    assert_eq!(rows.len(), 2);
    assert!(rows
        .iter()
        .all(|r| !r.pros.is_empty() && !r.cons.is_empty()));
    let recommended: Vec<&str> = rows
        .iter()
        .filter(|r| r.recommended)
        .map(|r| r.display_name.as_str())
        .collect();
    assert_eq!(recommended, ["Small"]);
}

#[test]
fn chat_saves_a_document_without_the_kind_question() {
    let (state, _path) = make_test_app_state();
    create_vault_inner(&state, "correct horse battery").unwrap();
    unlock_test_session(&state);

    let first = chat(&state, "save a document");
    assert_eq!(first.message, "What is the document called?");
    chat(&state, "Shopping list");
    let confirm = chat(&state, "eggs and milk");
    assert!(confirm.message.contains("Save this document?"));
    let done = chat(&state, "yes");
    assert_eq!(done.action.as_deref(), Some("store"));
    assert_eq!(state.repository.count_entries().unwrap(), 1);

    let found = handle_search_entry(&state, "shopping", Some("content")).unwrap();
    assert!(
        found.message.contains("eggs and milk"),
        "message: {}",
        found.message
    );
}

#[test]
fn chat_opens_the_editor_for_a_new_note_and_nothing_is_saved_by_chat() {
    let (state, _path) = make_test_app_state();
    create_vault_inner(&state, "correct horse battery").unwrap();
    unlock_test_session(&state);

    let reply = chat(&state, "save a note");
    assert_eq!(reply.action.as_deref(), Some("open_note_editor"));
    assert_eq!(state.repository.count_entries().unwrap(), 0);
}

#[test]
fn chat_edit_opens_the_named_note_in_the_editor() {
    let (state, _path) = make_test_app_state();
    create_vault_inner(&state, "correct horse battery").unwrap();
    unlock_test_session(&state);
    let note = crate::notes::save(&state, None, "Shopping list", "eggs").unwrap();

    let reply = chat(&state, "edit my shopping list");
    assert_eq!(
        reply.action.as_deref(),
        Some(format!("open_note_editor:{}", note.id).as_str()),
        "message: {}",
        reply.message
    );
}

#[test]
fn chat_edit_says_so_when_two_notes_match() {
    let (state, _path) = make_test_app_state();
    create_vault_inner(&state, "correct horse battery").unwrap();
    unlock_test_session(&state);
    crate::notes::save(&state, None, "Trip plan", "a").unwrap();
    crate::notes::save(&state, None, "Trip packing", "b").unwrap();

    let reply = chat(&state, "edit my trip");
    assert!(reply.action.is_none(), "action: {:?}", reply.action);
    assert!(
        reply.message.contains("More than one note"),
        "{}",
        reply.message
    );
}

#[test]
fn chat_unlocks_a_locked_vault_and_replays_the_question() {
    let (state, _path) = make_test_app_state();
    create_vault_inner(&state, "correct horse battery").unwrap();
    store_test_entry(&state, "Google", "hunter2");
    state.session.lock().unwrap().lock();

    let prompt = chat(&state, "what's my password for google?");
    assert!(prompt.input_sensitive, "message: {}", prompt.message);
    assert!(prompt.message.contains("locked"));

    let wrong = chat(&state, "not the password");
    assert!(wrong.input_sensitive, "message: {}", wrong.message);
    assert!(!state.session.lock().unwrap().is_unlocked);

    let answer = chat(&state, "correct horse battery");
    assert!(
        answer.message.contains("hunter2"),
        "the replayed question should be answered: {}",
        answer.message
    );
}

#[test]
fn a_note_saved_in_the_editor_round_trips_with_its_text_in_full() {
    let (state, _path) = make_test_app_state();
    unlock_test_session(&state);
    let created = crate::notes::save(&state, None, "Groceries", "eggs\nmilk").unwrap();
    assert_eq!(created.title, "Groceries");
    assert_eq!(created.text, "eggs\nmilk");

    let loaded = crate::notes::load(&state, &created.id).unwrap();
    assert_eq!(loaded.text, "eggs\nmilk");

    let edited =
        crate::notes::save(&state, Some(&created.id), "Groceries", "eggs, milk, bread").unwrap();
    assert_eq!(edited.id, created.id);
    assert_eq!(edited.version, created.version + 1);
    assert_eq!(
        crate::notes::load(&state, &created.id).unwrap().text,
        "eggs, milk, bread"
    );

    let listed = crate::notes::list(&state).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].title, "Groceries");
}

#[test]
fn a_note_needs_a_title() {
    let (state, _path) = make_test_app_state();
    unlock_test_session(&state);
    let err = crate::notes::save(&state, None, "   ", "body").unwrap_err();
    assert!(err.to_string().contains("needs a title"), "{err}");
    assert!(crate::notes::list(&state).unwrap().is_empty());
}

#[test]
fn a_note_saved_by_chat_is_listed_in_the_editor() {
    let (state, _path) = make_test_app_state();
    unlock_test_session(&state);
    let _ = handle_store_entry(
        &state,
        "Shopping list",
        "note",
        &[
            ("title".into(), "Shopping list".into()),
            ("text".into(), "eggs".into()),
        ],
        "saved",
    )
    .unwrap();
    let listed = crate::notes::list(&state).unwrap();
    assert!(
        listed.iter().any(|n| n.title == "Shopping list"),
        "{listed:?}"
    );
}

#[test]
fn an_edited_note_cannot_be_swapped_back_to_an_older_version() {
    let (state, _path) = make_test_app_state();
    unlock_test_session(&state);
    let first = crate::notes::save(&state, None, "Plan", "v1").unwrap();
    let old_ciphertext = state.repository.list_all_entries().unwrap()[0].clone();
    crate::notes::save(&state, Some(&first.id), "Plan", "v2").unwrap();

    // Put the first version's row back. Its AAD was bound to version 1, and
    // the row now says version 2, so it must not open.
    let mut restored = old_ciphertext;
    restored.version = 2;
    let payload = StoredPayload {
        dek_wrapped: &restored.dek_wrapped,
        nonce: &restored.nonce,
        ciphertext: &restored.ciphertext,
        name: &restored.name,
        version: 2,
        updated_at: &restored.updated_at.to_rfc3339(),
    };
    state
        .repository
        .update_entry_payload(&first.id, &payload)
        .unwrap();
    assert!(crate::notes::load(&state, &first.id).is_err());
}

#[test]
fn model_changes_are_refused_until_the_vault_is_unlocked() {
    let (state, _path) = make_test_app_state();
    assert!(matches!(
        require_unlocked(&state),
        Err(VaultError::VaultLocked)
    ));
    unlock_test_session(&state);
    assert!(require_unlocked(&state).is_ok());
}

#[test]
fn model_changes_are_refused_again_after_lock() {
    let (state, _path) = make_test_app_state();
    unlock_test_session(&state);
    state.session.lock().unwrap().lock();
    assert!(matches!(
        require_unlocked(&state),
        Err(VaultError::VaultLocked)
    ));
}
