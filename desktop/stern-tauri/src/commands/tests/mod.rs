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
fn chat_shows_the_file_picker_for_a_document_and_saves_nothing_itself() {
    let (state, _path) = make_test_app_state();
    create_vault_inner(&state, "correct horse battery").unwrap();
    unlock_test_session(&state);

    for text in ["store a document", "save a document"] {
        let reply = chat(&state, text);
        assert_eq!(
            reply.action.as_deref(),
            Some("open_file_picker"),
            "{text:?}: {}",
            reply.message
        );
        assert!(!reply.message.contains("Saved"), "{}", reply.message);
    }
    assert_eq!(state.repository.count_entries().unwrap(), 0);
}

#[test]
fn a_picked_file_is_reported_by_name_and_size_without_its_contents() {
    let (state, _path) = make_test_app_state();
    unlock_test_session(&state);
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("lease.txt");
    std::fs::write(&src, "the tenant agrees to pay").unwrap();

    let stored = crate::files::add_file(&state, &src).unwrap();
    assert_eq!(stored.name, "lease.txt");
    assert_eq!(stored.size_bytes, 24);
    assert!(stored.is_text);
    let shown = serde_json::to_string(&stored).unwrap();
    assert!(!shown.contains("tenant"), "contents leaked: {shown}");
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
    let note = crate::notes::save(&state, None, "Shopping list", "eggs", None).unwrap();

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
    crate::notes::save(&state, None, "Trip plan", "a", None).unwrap();
    crate::notes::save(&state, None, "Trip packing", "b", None).unwrap();

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
    let created = crate::notes::save(&state, None, "Groceries", "eggs\nmilk", None).unwrap();
    assert_eq!(created.title, "Groceries");
    assert_eq!(created.text, "eggs\nmilk");

    let loaded = crate::notes::load(&state, &created.id).unwrap();
    assert_eq!(loaded.text, "eggs\nmilk");

    let edited = crate::notes::save(
        &state,
        Some(&created.id),
        "Groceries",
        "eggs, milk, bread",
        None,
    )
    .unwrap();
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
    let err = crate::notes::save(&state, None, "   ", "body", None).unwrap_err();
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
    let first = crate::notes::save(&state, None, "Plan", "v1", None).unwrap();
    let old_ciphertext = state.repository.list_all_entries().unwrap()[0].clone();
    crate::notes::save(&state, Some(&first.id), "Plan", "v2", None).unwrap();

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
        .update_entry_payload(&first.id, 2, &payload)
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

/// Put a note in the vault with tags and an extra field, the way an
/// older build or another tool might have written it.
fn insert_rich_note(state: &AppState) -> String {
    use stern_core::domain::vault::aad::entry_aad;
    use stern_core::domain::vault::entry::{EncryptedEntry, EntryKind, EntryPayload, Field};
    let kek = *state.session.lock().unwrap().kek().unwrap();
    let payload = EntryPayload {
        name: "Trip".to_string(),
        kind: EntryKind::Note,
        tags: vec!["travel".to_string(), "2026".to_string()],
        fields: vec![
            Field {
                key: "title".into(),
                value: "Trip".into(),
                hidden: false,
            },
            Field {
                key: "text".into(),
                value: "pack socks".into(),
                hidden: false,
            },
            Field {
                key: "booking".into(),
                value: "ABC123".into(),
                hidden: true,
            },
        ],
        summary: String::new(),
        keywords: vec![],
    };
    let id = "note-rich".to_string();
    let json = serde_json::to_vec(&payload).unwrap();
    let dek = state.crypto.generate_dek();
    let (nonce, ciphertext) = state
        .crypto
        .encrypt_entry(&dek, &json, &entry_aad(&id, 1))
        .unwrap();
    let now = chrono::Utc::now();
    state
        .repository
        .insert_entry(&EncryptedEntry {
            id: id.clone(),
            kind: EntryKind::Note,
            name: "Trip".to_string(),
            tags: vec!["travel".to_string(), "2026".to_string()],
            dek_wrapped: state.crypto.wrap_dek(&dek, &kek).unwrap().to_vec(),
            nonce,
            ciphertext,
            version: 1,
            created_at: now,
            updated_at: now,
        })
        .unwrap();
    id
}

#[test]
fn saving_a_note_keeps_its_tags_and_other_fields() {
    let (state, _path) = make_test_app_state();
    unlock_test_session(&state);
    let id = insert_rich_note(&state);

    crate::notes::save(
        &state,
        Some(&id),
        "Trip to Rome",
        "pack socks and a hat",
        None,
    )
    .unwrap();

    let entry = state
        .repository
        .list_all_entries()
        .unwrap()
        .into_iter()
        .find(|e| e.id == id)
        .unwrap();
    assert_eq!(entry.tags, ["travel", "2026"]);
    let kek = *state.session.lock().unwrap().kek().unwrap();
    let wrapped = <[u8; stern_core::domain::vault::crypto_constants::DEK_WRAPPED_LEN]>::try_from(
        entry.dek_wrapped.as_slice(),
    )
    .unwrap();
    let dek = state.crypto.unwrap_dek(&wrapped, &kek).unwrap();
    let plain = state
        .crypto
        .decrypt_entry(
            &dek,
            &entry.nonce,
            &entry.ciphertext,
            &stern_core::domain::vault::aad::entry_aad(&id, entry.version),
        )
        .unwrap();
    let payload: stern_core::domain::vault::entry::EntryPayload =
        serde_json::from_slice(&plain).unwrap();
    assert_eq!(payload.tags, ["travel", "2026"]);
    let booking = payload.fields.iter().find(|f| f.key == "booking").unwrap();
    assert_eq!(booking.value, "ABC123");
    assert!(booking.hidden);
    let note = crate::notes::load(&state, &id).unwrap();
    assert_eq!(
        (note.title.as_str(), note.text.as_str()),
        ("Trip to Rome", "pack socks and a hat")
    );
}

#[test]
fn a_save_from_an_out_of_date_copy_is_refused() {
    let (state, _path) = make_test_app_state();
    unlock_test_session(&state);
    let first = crate::notes::save(&state, None, "Plan", "v1", None).unwrap();
    crate::notes::save(&state, Some(&first.id), "Plan", "v2", None).unwrap();

    // The repository itself refuses a write that names a stale version.
    let payload = StoredPayload {
        dek_wrapped: &[0u8; 40],
        nonce: &[0u8; 24],
        ciphertext: b"x",
        name: "Plan",
        version: 2,
        updated_at: "2026-11-24T00:00:00+00:00",
    };
    let applied = state
        .repository
        .update_entry_payload(&first.id, 1, &payload)
        .unwrap();
    assert!(!applied, "version 1 is no longer the stored version");
    assert_eq!(crate::notes::load(&state, &first.id).unwrap().text, "v2");
}

#[test]
fn an_entry_version_that_does_not_fit_is_an_error_not_a_clamp() {
    let (state, _path) = make_test_app_state();
    unlock_test_session(&state);
    let note = crate::notes::save(&state, None, "Plan", "v1", None).unwrap();
    let payload = StoredPayload {
        dek_wrapped: &[0u8; 40],
        nonce: &[0u8; 24],
        ciphertext: b"x",
        name: "Plan",
        version: u64::MAX,
        updated_at: "2026-11-24T00:00:00+00:00",
    };
    let err = state
        .repository
        .update_entry_payload(&note.id, 1, &payload)
        .unwrap_err();
    assert!(err.to_string().contains("out of range"), "{err}");
    assert_eq!(crate::notes::load(&state, &note.id).unwrap().text, "v1");
}

#[test]
fn listing_notes_orders_by_time_and_does_not_open_their_text() {
    let (state, _path) = make_test_app_state();
    unlock_test_session(&state);
    let older = crate::notes::save(&state, None, "Older", "a", None).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(5));
    let newer = crate::notes::save(&state, None, "Newer", "b", None).unwrap();

    let listed = crate::notes::list(&state).unwrap();
    let ids: Vec<&str> = listed.iter().map(|n| n.id.as_str()).collect();
    assert_eq!(ids, [newer.id.as_str(), older.id.as_str()]);

    // Corrupt one note's ciphertext: listing needs only the stored title,
    // so it still works; opening the note is what fails.
    let mut broken = state
        .repository
        .list_all_entries()
        .unwrap()
        .into_iter()
        .find(|e| e.id == older.id)
        .unwrap();
    broken.ciphertext[0] ^= 0xff;
    let payload = StoredPayload {
        dek_wrapped: &broken.dek_wrapped,
        nonce: &broken.nonce,
        ciphertext: &broken.ciphertext,
        name: "Older",
        version: broken.version,
        updated_at: &broken.updated_at.to_rfc3339(),
    };
    state
        .repository
        .update_entry_payload(&older.id, broken.version, &payload)
        .unwrap();
    assert_eq!(crate::notes::list(&state).unwrap().len(), 2);
    assert!(crate::notes::load(&state, &older.id).is_err());
}

#[test]
fn finding_notes_with_a_blank_query_matches_nothing() {
    let (state, _path) = make_test_app_state();
    unlock_test_session(&state);
    crate::notes::save(&state, None, "Plan", "a", None).unwrap();
    assert!(crate::notes::find(&state, "").unwrap().is_empty());
    assert!(crate::notes::find(&state, "   ").unwrap().is_empty());
    assert_eq!(crate::notes::find(&state, "plan").unwrap().len(), 1);
}

#[test]
fn locking_cancels_a_download_that_started_before_it() {
    let (state, _path) = make_test_app_state();
    let epoch = state.model_manager.download_epoch();
    state.model_manager.cancel_downloads();
    assert_ne!(state.model_manager.download_epoch(), epoch);
}

#[test]
fn a_text_file_is_stored_and_comes_back_byte_for_byte() {
    let (state, _path) = make_test_app_state();
    unlock_test_session(&state);
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("notes.csv");
    let text = "name,card\nChase,héllo\n".repeat(200);
    std::fs::write(&src, &text).unwrap();

    let id = crate::files::add_file(&state, &src).unwrap().id;
    let back = crate::files::read_stored(&state, &id).unwrap();
    assert_eq!(back.as_slice(), text.as_bytes());

    let out = dir.path().join("restored.csv");
    crate::files::write_retrieved(&out, &back).unwrap();
    assert_eq!(std::fs::read(&out).unwrap(), text.as_bytes());
}

#[test]
fn a_byte_order_mark_is_not_stored() {
    let (state, _path) = make_test_app_state();
    unlock_test_session(&state);
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("a.txt");
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(b"hello");
    std::fs::write(&src, &bytes).unwrap();
    let stored = crate::files::add_file(&state, &src).unwrap();
    assert_eq!(stored.size_bytes, 5);
    let back = crate::files::read_stored(&state, &stored.id).unwrap();
    assert_eq!(back.as_slice(), b"hello");
}

#[test]
fn files_that_are_not_plain_text_are_refused_and_nothing_is_stored() {
    let (state, _path) = make_test_app_state();
    unlock_test_session(&state);
    let dir = tempfile::tempdir().unwrap();
    let write = |name: &str, bytes: &[u8]| {
        let path = dir.path().join(name);
        std::fs::write(&path, bytes).unwrap();
        path
    };
    // A PDF, even when its bytes happen to be valid UTF-8, is refused by name.
    let pdf = write("lease.pdf", b"just text");
    let err = crate::files::add_file(&state, &pdf).unwrap_err();
    assert!(err.to_string().contains("plain text files only"), "{err}");
    // Binary content under a friendly name.
    let png = write("notes.txt", &[0x89, b'P', b'N', b'G', 0, 1, 2]);
    let err = crate::files::add_file(&state, &png).unwrap_err();
    assert!(err.to_string().contains("isn't plain text"), "{err}");
    // UTF-16 text.
    let utf16 = write("w.txt", &[0xFF, 0xFE, b'h', 0, b'i', 0]);
    let err = crate::files::add_file(&state, &utf16).unwrap_err();
    assert!(err.to_string().contains("UTF-16"), "{err}");
    assert_eq!(state.repository.count_entries().unwrap(), 0);
}

#[test]
fn the_limit_is_one_mebibyte_to_the_byte() {
    let (state, _path) = make_test_app_state();
    unlock_test_session(&state);
    let dir = tempfile::tempdir().unwrap();
    let exact = dir.path().join("exact.txt");
    std::fs::write(&exact, vec![b'a'; 1024 * 1024]).unwrap();
    assert!(crate::files::add_file(&state, &exact).is_ok());
    let over = dir.path().join("over.txt");
    std::fs::write(&over, vec![b'a'; 1024 * 1024 + 1]).unwrap();
    let err = crate::files::add_file(&state, &over).unwrap_err();
    assert!(err.to_string().contains("1 MB"), "{err}");
    assert_eq!(state.repository.count_entries().unwrap(), 1);
}

#[test]
fn a_text_file_is_stored_as_text() {
    let (state, _path) = make_test_app_state();
    unlock_test_session(&state);
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("shopping.txt");
    std::fs::write(&src, "eggs, milk and bread").unwrap();

    let id = crate::files::add_file(&state, &src).unwrap().id;
    let blob = state.repository.get_blob(&id).unwrap().unwrap();
    assert!(blob.is_text);
    assert_eq!(blob.mime_type, "text/plain");
    assert_eq!(blob.byte_len, "eggs, milk and bread".len() as u64);
}

#[test]
fn a_file_over_the_limit_is_refused_before_it_is_stored() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("big.bin");
    std::fs::write(&src, vec![0u8; 64]).unwrap();
    let err = crate::files::read_file_limited_for_test(&src, 63).unwrap_err();
    assert!(err.to_string().contains("too big"), "{err}");
    assert!(crate::files::read_file_limited_for_test(&src, 64).is_ok());
}

#[test]
fn a_file_cannot_be_read_while_the_vault_is_locked() {
    let (state, _path) = make_test_app_state();
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("a.txt");
    std::fs::write(&src, "x").unwrap();
    assert!(crate::files::add_file(&state, &src).is_err());
}

#[test]
fn a_stored_file_cannot_be_moved_to_another_entry() {
    let (state, _path) = make_test_app_state();
    unlock_test_session(&state);
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("a.txt");
    std::fs::write(&src, "secret contents").unwrap();
    let first = crate::files::add_file(&state, &src).unwrap().id;
    let second = crate::files::add_file(&state, &src).unwrap().id;

    // Put the first file's blob (ciphertext, DEK, nonce) under the second
    // entry's id. Its AAD was bound to the first id, so it must not open.
    let first_blob = state.repository.get_blob(&first).unwrap().unwrap();
    state.repository.delete_entry(&second).unwrap();
    let entry = state
        .repository
        .list_all_entries()
        .unwrap()
        .into_iter()
        .find(|e| e.id == first)
        .unwrap();
    let mut moved = entry;
    moved.id = second.clone();
    state.repository.insert_file(&moved, &first_blob).unwrap();

    let err = crate::files::read_stored(&state, &second).unwrap_err();
    assert!(err.to_string().contains("decryption"), "{err}");
}

// ---- folders, listing, rename and delete from chat ----

fn unlocked_state_with_entries() -> (AppState, tempfile::TempPath) {
    let (state, path) = make_test_app_state();
    create_vault_inner(&state, "correct horse battery").unwrap();
    unlock_test_session(&state);
    store_test_entry(&state, "Netflix", "hunter2-secret");
    store_test_entry(&state, "GitHub", "ghp-token-value");
    (state, path)
}

#[test]
fn folders_can_be_made_nested_and_shown_without_values() {
    let (state, _path) = unlocked_state_with_entries();
    assert!(chat(&state, "create folder Streaming")
        .message
        .contains("Created the folder"));
    let nested = chat(&state, "create folder Kids in Streaming");
    assert!(
        nested.message.contains("inside \"Streaming\""),
        "message: {}",
        nested.message
    );
    let moved = chat(&state, "move Netflix to Streaming > Kids");
    assert!(
        moved.message.contains("Streaming > Kids"),
        "message: {}",
        moved.message
    );

    let tree = chat(&state, "show my folders");
    assert!(tree.message.contains("📁 Streaming"), "{}", tree.message);
    assert!(tree.message.contains("📁 Kids"), "{}", tree.message);
    assert!(tree.message.contains("Netflix (login)"), "{}", tree.message);
    assert!(tree.message.contains("GitHub (login)"), "{}", tree.message);
    // Structure only: never a stored value.
    assert!(!tree.message.contains("hunter2-secret"));
    assert!(!tree.message.contains("ghp-token-value"));
}

#[test]
fn folder_names_are_encrypted_in_the_database() {
    let (state, _path) = unlocked_state_with_entries();
    chat(&state, "create folder Divorce Papers");
    let stored = state.repository.list_folders().unwrap();
    assert_eq!(stored.len(), 1);
    let raw = String::from_utf8_lossy(&stored[0].ciphertext).to_string();
    assert!(!raw.contains("Divorce"));
    // And a name moved onto another folder fails to decrypt.
    chat(&state, "create folder Other");
    let both = state.repository.list_folders().unwrap();
    let kek = *state.session.lock().unwrap().kek().unwrap();
    let mut swapped = both[0].clone();
    swapped.ciphertext = both[1].ciphertext.clone();
    swapped.nonce = both[1].nonce;
    swapped.dek_wrapped = both[1].dek_wrapped.clone();
    assert!(crate::organize::folder_name_for_test(&state, &kek, &swapped).is_err());
}

#[test]
fn a_duplicate_or_misplaced_folder_is_explained() {
    let (state, _path) = unlocked_state_with_entries();
    chat(&state, "create folder Taxes");
    assert!(chat(&state, "create folder taxes")
        .message
        .contains("already a folder"));
    assert!(chat(&state, "create folder Sub in Nowhere")
        .message
        .contains("don't see a folder"));
    assert!(chat(&state, "create a folder")
        .message
        .contains("What should the folder be called"));
    assert!(chat(&state, "move Netflix to Nowhere")
        .message
        .contains("don't see a folder"));
    assert!(chat(&state, "move Missing to Taxes")
        .message
        .contains("couldn't find"));
}

#[test]
fn an_entry_can_come_back_out_of_a_folder() {
    let (state, _path) = unlocked_state_with_entries();
    chat(&state, "create folder Streaming");
    chat(&state, "move Netflix to Streaming");
    let out = chat(&state, "move Netflix out of its folder");
    assert!(out.message.contains("out of its folder"), "{}", out.message);
    assert!(state.repository.entry_folders().unwrap().is_empty());
}

#[test]
fn deleting_a_folder_keeps_its_entries() {
    let (state, _path) = unlocked_state_with_entries();
    chat(&state, "create folder Streaming");
    chat(&state, "move Netflix to Streaming");
    let gone = chat(&state, "delete the Streaming folder");
    assert!(
        gone.message.contains("Deleted the folder"),
        "{}",
        gone.message
    );
    assert_eq!(state.repository.count_entries().unwrap(), 2);
    assert!(state.repository.list_folders().unwrap().is_empty());
}

#[test]
fn listing_shows_names_never_values() {
    let (state, _path) = unlocked_state_with_entries();
    let all = chat(&state, "list all my secrets");
    assert!(all.message.contains("Netflix"), "{}", all.message);
    assert!(all.message.contains("GitHub"), "{}", all.message);
    assert!(!all.message.contains("hunter2-secret"));
    let none = chat(&state, "show all my documents");
    assert!(none.message.contains("no document"), "{}", none.message);
}

#[test]
fn deleting_an_entry_asks_first_and_only_a_yes_deletes() {
    let (state, _path) = unlocked_state_with_entries();
    let ask = chat(&state, "delete my old netflix entry");
    assert!(
        ask.message.contains("Delete \"Netflix\""),
        "{}",
        ask.message
    );
    assert_eq!(ask.options.len(), 2);
    assert_eq!(state.repository.count_entries().unwrap(), 2);

    let kept = chat(&state, "No, keep it");
    assert!(kept.message.contains("kept"));
    assert_eq!(state.repository.count_entries().unwrap(), 2);

    chat(&state, "delete Netflix");
    let done = chat(&state, "Yes, delete it");
    assert!(
        done.message.contains("Deleted \"Netflix\""),
        "{}",
        done.message
    );
    assert_eq!(state.repository.count_entries().unwrap(), 1);
}

#[test]
fn deleting_something_that_is_not_there_says_so() {
    let (state, _path) = unlocked_state_with_entries();
    let reply = chat(&state, "delete my Hulu login");
    assert!(reply.message.contains("couldn't find"), "{}", reply.message);
    assert!(reply.options.is_empty());
}

#[test]
fn renaming_an_entry_keeps_its_secrets_readable() {
    let (state, _path) = unlocked_state_with_entries();
    let renamed = chat(&state, "rename Netflix to Netflix Family");
    assert!(
        renamed.message.contains("Renamed \"Netflix\""),
        "{}",
        renamed.message
    );
    let found = chat(&state, "what's my password for Netflix Family");
    assert!(
        found.message.contains("hunter2-secret"),
        "{}",
        found.message
    );
    assert!(chat(&state, "rename Netflix Family")
        .message
        .contains("What should"));
}

#[test]
fn renaming_a_folder_works_and_clashes_are_refused() {
    let (state, _path) = unlocked_state_with_entries();
    chat(&state, "create folder Taxes");
    chat(&state, "create folder Home");
    assert!(chat(&state, "rename folder Taxes to Home")
        .message
        .contains("already a folder"));
    let ok = chat(&state, "rename the Taxes folder to Tax Docs");
    assert!(ok.message.contains("Renamed the folder"), "{}", ok.message);
    assert!(chat(&state, "show my folders").message.contains("Tax Docs"));
}

#[test]
fn a_folder_cannot_be_moved_into_its_own_child() {
    let (state, _path) = unlocked_state_with_entries();
    chat(&state, "create folder A");
    chat(&state, "create folder B in A");
    let refused = chat(&state, "move A to B");
    assert!(
        refused.message.contains("inside itself"),
        "{}",
        refused.message
    );
}

#[test]
fn the_organising_commands_refuse_a_locked_vault() {
    let (state, _path) = make_test_app_state();
    let reply = crate::organize::show_folders(&state, None);
    assert!(matches!(reply, Err(VaultError::VaultLocked)));
    let reply = handle_organize(&state, &CoreAction::ShowFolders { folder: None });
    assert_eq!(reply.action.as_deref(), Some("locked"));
}

#[test]
fn asking_what_a_listed_note_is_shows_the_note() {
    let (state, _path) = make_test_app_state();
    create_vault_inner(&state, "correct horse battery").unwrap();
    unlock_test_session(&state);
    let id = insert_rich_note(&state);
    assert!(!id.is_empty());
    let shown = chat(&state, "what is the trip?");
    assert!(shown.message.contains("pack socks"), "{}", shown.message);
    assert!(!shown.message.contains("No secret found"));
}

#[test]
fn an_older_binary_file_stays_listed_readable_and_deletable() {
    let (state, _path) = make_test_app_state();
    create_vault_inner(&state, "correct horse battery").unwrap();
    unlock_test_session(&state);
    // Store a real file, then mark it binary the way an earlier build would
    // have stored a PDF.
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("scan.txt");
    std::fs::write(&src, "pretend this is a scan").unwrap();
    let stored = crate::files::add_file(&state, &src).unwrap();
    state
        .repository
        .mark_file_binary_for_test(&stored.id)
        .unwrap();

    let list = chat(&state, "show my secrets");
    assert!(
        list.message.contains("scan.txt (document, older file)"),
        "{}",
        list.message
    );
    // Still readable.
    assert!(crate::files::read_stored(&state, &stored.id).is_ok());
    let renamed = chat(&state, "rename scan.txt to lease scan");
    assert!(renamed.message.contains("Renamed"), "{}", renamed.message);
    chat(&state, "delete lease scan");
    chat(&state, "Yes, delete it");
    assert_eq!(state.repository.count_entries().unwrap(), 0);
}

fn note_about<'a>(summary: &'a str, keywords: &'a str) -> crate::notes::About<'a> {
    crate::notes::About { summary, keywords }
}

#[test]
fn a_note_keeps_the_summary_and_keywords_it_was_given() {
    let (state, _path) = make_test_app_state();
    unlock_test_session(&state);
    let about = note_about("  my   Chase   card  ", "Credit Card, visa");
    let saved =
        crate::notes::save(&state, None, "Chase", "card ends in 4417", Some(&about)).unwrap();
    assert_eq!(saved.summary, "my Chase card");
    assert_eq!(saved.keywords, "credit card, visa");
    let again = crate::notes::load(&state, &saved.id).unwrap();
    assert_eq!(again.summary, "my Chase card");
}

#[test]
fn empty_keywords_are_filled_from_the_text() {
    let (state, _path) = make_test_app_state();
    unlock_test_session(&state);
    let about = note_about("", "");
    let saved = crate::notes::save(
        &state,
        None,
        "Trip",
        "passport renewal appointment, passport photo needed",
        Some(&about),
    )
    .unwrap();
    assert!(saved.keywords.contains("passport"), "{}", saved.keywords);
    // A note saved with no description at all gets them too.
    let plain = crate::notes::save(&state, None, "Rome", "colosseum tickets", None).unwrap();
    assert!(plain.keywords.contains("colosseum"), "{}", plain.keywords);
}

#[test]
fn saving_without_a_description_leaves_the_stored_one_alone() {
    let (state, _path) = make_test_app_state();
    unlock_test_session(&state);
    let about = note_about("my card", "visa");
    let first = crate::notes::save(&state, None, "Chase", "v1", Some(&about)).unwrap();
    let edited = crate::notes::save(&state, Some(&first.id), "Chase", "v2", None).unwrap();
    assert_eq!(edited.summary, "my card");
    assert_eq!(edited.keywords, "visa");
    assert_eq!(edited.text, "v2");
    // And sending an empty summary clears it.
    let cleared = note_about("", "visa");
    let cleared =
        crate::notes::save(&state, Some(&first.id), "Chase", "v2", Some(&cleared)).unwrap();
    assert_eq!(cleared.summary, "");
}

#[test]
fn a_stored_text_file_gets_keywords_from_its_text_and_no_secret_words() {
    let (state, _path) = make_test_app_state();
    unlock_test_session(&state);
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("lease.txt");
    std::fs::write(
        &src,
        "apartment lease agreement. apartment rent due monthly. wifi password mangosteen99",
    )
    .unwrap();
    let id = crate::files::add_file(&state, &src).unwrap().id;
    let entry = state
        .repository
        .list_all_entries()
        .unwrap()
        .into_iter()
        .find(|e| e.id == id)
        .unwrap();
    let kek = *state.session.lock().unwrap().kek().unwrap();
    let wrapped = <[u8; 40]>::try_from(entry.dek_wrapped.as_slice()).unwrap();
    let dek = state.crypto.unwrap_dek(&wrapped, &kek).unwrap();
    let plain = state
        .crypto
        .decrypt_entry(
            &dek,
            &entry.nonce,
            &entry.ciphertext,
            &stern_core::domain::vault::aad::entry_aad(&entry.id, entry.version),
        )
        .unwrap();
    let payload: EntryPayload = serde_json::from_slice(&plain).unwrap();
    assert!(
        payload.keywords.contains(&"apartment".to_string()),
        "{:?}",
        payload.keywords
    );
    assert!(
        !payload.keywords.iter().any(|w| w.contains("mangosteen")),
        "{:?}",
        payload.keywords
    );
}

// ---- trash ----

fn two_notes() -> (AppState, tempfile::TempPath) {
    let (state, path) = make_test_app_state();
    create_vault_inner(&state, "correct horse battery").unwrap();
    unlock_test_session(&state);
    crate::notes::save(&state, None, "Chase card", "visa ending 4417", None).unwrap();
    crate::notes::save(&state, None, "Shopping", "eggs and milk", None).unwrap();
    (state, path)
}

#[test]
fn deleting_puts_the_entry_in_the_trash_and_out_of_sight() {
    let (state, _path) = two_notes();
    chat(&state, "delete Chase card");
    let done = chat(&state, "Yes, delete it");
    assert!(done.message.contains("Deleted"), "{}", done.message);
    assert!(done.message.contains("undo"), "{}", done.message);

    // Not listed, not found, not counted, not exported.
    assert!(!chat(&state, "show my secrets").message.contains("Chase"));
    assert!(chat(&state, "find Chase card")
        .message
        .contains("No secret found"));
    assert_eq!(state.repository.count_entries().unwrap(), 1);
    assert!(crate::notes::list(&state)
        .unwrap()
        .iter()
        .all(|n| n.title != "Chase card"));
    // But it is still stored, ready to come back.
    assert_eq!(state.repository.list_trashed_entries().unwrap().len(), 1);
    let shown = chat(&state, "show the trash");
    assert!(shown.message.contains("Chase card"), "{}", shown.message);
    assert!(shown.message.contains("today"), "{}", shown.message);
    assert!(!shown.message.contains("4417"));
}

#[test]
fn undo_brings_back_the_last_deleted_entry_with_its_text() {
    let (state, _path) = two_notes();
    chat(&state, "delete Chase card");
    chat(&state, "Yes, delete it");
    chat(&state, "delete Shopping");
    chat(&state, "Yes, delete it");

    let back = chat(&state, "undo");
    assert!(
        back.message.contains("Brought \"Shopping\""),
        "{}",
        back.message
    );
    let by_name = chat(&state, "restore Chase card");
    assert!(
        by_name.message.contains("Brought \"Chase card\""),
        "{}",
        by_name.message
    );
    let found = crate::notes::find(&state, "chase").unwrap();
    let note = crate::notes::load(&state, &found[0].id).unwrap();
    assert_eq!(note.text, "visa ending 4417");
    assert_eq!(state.repository.count_entries().unwrap(), 2);
    assert!(chat(&state, "undo")
        .message
        .contains("nothing to bring back"));
}

#[test]
fn a_restored_entry_keeps_its_folder() {
    let (state, _path) = two_notes();
    chat(&state, "create folder Money");
    chat(&state, "move Chase card to Money");
    chat(&state, "delete Chase card");
    chat(&state, "Yes, delete it");
    chat(&state, "undo");
    let tree = chat(&state, "show my folders");
    assert!(tree.message.contains("Chase card"), "{}", tree.message);
    assert_eq!(state.repository.entry_folders().unwrap().len(), 1);
}

#[test]
fn emptying_the_trash_asks_then_removes_for_good() {
    let (state, _path) = two_notes();
    assert!(chat(&state, "empty the trash")
        .message
        .contains("already empty"));
    chat(&state, "delete Shopping");
    chat(&state, "Yes, delete it");
    let ask = chat(&state, "empty the trash");
    assert!(ask.message.contains("for good"), "{}", ask.message);
    assert_eq!(ask.options.len(), 2);
    assert_eq!(state.repository.list_trashed_entries().unwrap().len(), 1);

    let kept = chat(&state, "No, keep it");
    assert!(kept.message.contains("kept"), "{}", kept.message);
    assert_eq!(state.repository.list_trashed_entries().unwrap().len(), 1);

    chat(&state, "empty the trash");
    let gone = chat(&state, "Yes, delete it");
    assert!(gone.message.contains("for good"), "{}", gone.message);
    assert!(state.repository.list_trashed_entries().unwrap().is_empty());
    assert!(chat(&state, "undo")
        .message
        .contains("nothing to bring back"));
}

#[test]
fn the_purge_removes_only_old_entries_and_their_stored_files() {
    let (state, _path) = make_test_app_state();
    create_vault_inner(&state, "correct horse battery").unwrap();
    unlock_test_session(&state);
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("lease.txt");
    std::fs::write(&src, "apartment lease").unwrap();
    let old_file = crate::files::add_file(&state, &src).unwrap().id;
    let recent = crate::notes::save(&state, None, "Recent", "x", None)
        .unwrap()
        .id;
    let kept = crate::notes::save(&state, None, "Kept", "y", None)
        .unwrap()
        .id;

    let now = chrono::Utc::now();
    let stamp = |days: i64| (now - chrono::Duration::days(days)).to_rfc3339();
    assert!(state.repository.trash_entry(&old_file, &stamp(31)).unwrap());
    assert!(state.repository.trash_entry(&recent, &stamp(29)).unwrap());

    assert_eq!(crate::trash::purge_expired_at(&state, now).unwrap(), 1);
    // The old file and its stored bytes are gone.
    assert!(state.repository.get_blob(&old_file).unwrap().is_none());
    let left: Vec<String> = state
        .repository
        .list_trashed_entries()
        .unwrap()
        .into_iter()
        .map(|e| e.id)
        .collect();
    assert_eq!(left, vec![recent.clone()]);
    // An entry that was never trashed is untouched.
    assert!(state
        .repository
        .list_all_entries()
        .unwrap()
        .iter()
        .any(|e| e.id == kept));
}

#[test]
fn unlocking_purges_the_expired_trash() {
    let (state, _path) = make_test_app_state();
    create_vault_inner(&state, "correct horse battery").unwrap();
    let id = {
        unlock_test_session(&state);
        let id = crate::notes::save(&state, None, "Old", "x", None)
            .unwrap()
            .id;
        let long_ago = (chrono::Utc::now() - chrono::Duration::days(45)).to_rfc3339();
        state.repository.trash_entry(&id, &long_ago).unwrap();
        id
    };
    state.session.lock().unwrap().lock();
    unlock_vault_inner(&state, "correct horse battery").unwrap();
    assert!(state
        .repository
        .list_trashed_entries()
        .unwrap()
        .iter()
        .all(|e| e.id != id));
}

#[test]
fn the_trash_commands_refuse_a_locked_vault() {
    let (state, _path) = make_test_app_state();
    assert!(matches!(
        crate::trash::show_trash(&state),
        Err(VaultError::VaultLocked)
    ));
    assert!(matches!(
        crate::trash::restore(&state, ""),
        Err(VaultError::VaultLocked)
    ));
    assert!(matches!(
        crate::trash::empty(&state),
        Err(VaultError::VaultLocked)
    ));
}

#[test]
fn an_older_vault_gains_the_trash_column_and_keeps_its_entries() {
    use stern_core::infrastructure::sqlite::repository::SqliteRepository;
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let path = tmp.into_temp_path();
    {
        let repo = SqliteRepository::new(&path).unwrap();
        let now = chrono::Utc::now();
        repo.insert_entry(&stern_core::domain::vault::entry::EncryptedEntry {
            id: "e".into(),
            kind: stern_core::domain::vault::entry::EntryKind::Note,
            name: "Old".into(),
            tags: vec![],
            dek_wrapped: vec![1; 40],
            nonce: [2; 24],
            ciphertext: vec![3],
            version: 1,
            created_at: now,
            updated_at: now,
        })
        .unwrap();
    }
    // Reopening runs the migrations again and must not fail or lose data.
    let repo = SqliteRepository::new(&path).unwrap();
    assert_eq!(repo.count_entries().unwrap(), 1);
    assert!(repo.list_trashed_entries().unwrap().is_empty());
}

#[test]
fn deleting_a_stored_file_goes_to_the_trash_not_straight_out() {
    let (state, _path) = make_test_app_state();
    unlock_test_session(&state);
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("a.txt");
    std::fs::write(&src, "keep me").unwrap();
    let id = crate::files::add_file(&state, &src).unwrap().id;
    assert!(state
        .repository
        .trash_entry(&id, &chrono::Utc::now().to_rfc3339())
        .unwrap());
    // Still stored, with its bytes, until the trash is emptied.
    assert!(state.repository.get_blob(&id).unwrap().is_some());
    assert_eq!(state.repository.count_entries().unwrap(), 0);
    crate::trash::empty(&state).unwrap();
    assert!(state.repository.get_blob(&id).unwrap().is_none());
}
