use chrono::{DateTime, Utc};
use stern_ipc::dto::*;

fn ts() -> DateTime<Utc> {
    "2024-06-15T12:00:00Z".parse().unwrap()
}

fn roundtrip<T: serde::Serialize + serde::de::DeserializeOwned>(val: &T) -> T {
    let json = serde_json::to_string(val).unwrap();
    serde_json::from_str(&json).unwrap()
}

// ── VaultStatus ──

#[test]
fn vault_status_roundtrip_true() {
    let v = VaultStatus {
        is_initialized: true,
        is_unlocked: true,
        entry_count: 42,
    };
    let r = roundtrip(&v);
    assert!(r.is_initialized);
    assert!(r.is_unlocked);
    assert_eq!(r.entry_count, 42);
}

#[test]
fn vault_status_roundtrip_false() {
    let v = VaultStatus {
        is_initialized: false,
        is_unlocked: false,
        entry_count: 0,
    };
    let r = roundtrip(&v);
    assert!(!r.is_initialized);
    assert!(!r.is_unlocked);
    assert_eq!(r.entry_count, 0);
}

// ── FieldDto ──

#[test]
fn field_dto_roundtrip_normal() {
    let f = FieldDto {
        key: "user".into(),
        value: "admin".into(),
        hidden: true,
    };
    let r = roundtrip(&f);
    assert_eq!(r.key, "user");
    assert_eq!(r.value, "admin");
    assert!(r.hidden);
}

#[test]
fn field_dto_empty_strings() {
    let f = FieldDto {
        key: String::new(),
        value: String::new(),
        hidden: false,
    };
    let r = roundtrip(&f);
    assert!(r.key.is_empty());
    assert!(r.value.is_empty());
}

#[test]
fn field_dto_unicode() {
    let f = FieldDto {
        key: "日本語".into(),
        value: "🔐🔑".into(),
        hidden: false,
    };
    let r = roundtrip(&f);
    assert_eq!(r.key, "日本語");
    assert_eq!(r.value, "🔐🔑");
}

// ── EntryDto ──

#[test]
fn entry_dto_roundtrip_populated() {
    let e = EntryDto {
        id: "id-1".into(),
        kind: "login".into(),
        name: "GitHub".into(),
        tags: vec!["dev".into(), "work".into()],
        fields: vec![
            FieldDto {
                key: "url".into(),
                value: "https://github.com".into(),
                hidden: false,
            },
            FieldDto {
                key: "pass".into(),
                value: "s3cret".into(),
                hidden: true,
            },
        ],
        version: 3,
        created_at: ts(),
        updated_at: ts(),
    };
    let r = roundtrip(&e);
    assert_eq!(r.id, "id-1");
    assert_eq!(r.tags.len(), 2);
    assert_eq!(r.fields.len(), 2);
    assert_eq!(r.version, 3);
}

#[test]
fn entry_dto_empty_vecs() {
    let e = EntryDto {
        id: String::new(),
        kind: String::new(),
        name: String::new(),
        tags: vec![],
        fields: vec![],
        version: 0,
        created_at: ts(),
        updated_at: ts(),
    };
    let r = roundtrip(&e);
    assert!(r.tags.is_empty());
    assert!(r.fields.is_empty());
}

#[test]
fn entry_dto_unicode() {
    let e = EntryDto {
        id: "日本語id".into(),
        kind: "种类型".into(),
        name: "名称".into(),
        tags: vec!["タグ".into()],
        fields: vec![],
        version: 1,
        created_at: ts(),
        updated_at: ts(),
    };
    let r = roundtrip(&e);
    assert_eq!(r.id, "日本語id");
    assert_eq!(r.tags, vec!["タグ"]);
}

// ── CreateEntryRequest ──

#[test]
fn create_entry_request_roundtrip() {
    let c = CreateEntryRequest {
        kind: "note".into(),
        name: "My Note".into(),
        tags: vec!["personal".into()],
        fields: vec![FieldDto {
            key: "body".into(),
            value: "hello".into(),
            hidden: false,
        }],
    };
    let r = roundtrip(&c);
    assert_eq!(r.kind, "note");
    assert_eq!(r.fields.len(), 1);
}

#[test]
fn create_entry_request_empty() {
    let c = CreateEntryRequest {
        kind: String::new(),
        name: String::new(),
        tags: vec![],
        fields: vec![],
    };
    let r = roundtrip(&c);
    assert!(r.tags.is_empty());
    assert!(r.fields.is_empty());
}

// ── UpdateEntryRequest ──

#[test]
fn update_entry_request_all_some() {
    let u = UpdateEntryRequest {
        id: "id-1".into(),
        name: Some("New Name".into()),
        tags: Some(vec!["a".into()]),
        fields: Some(vec![FieldDto {
            key: "k".into(),
            value: "v".into(),
            hidden: false,
        }]),
    };
    let r = roundtrip(&u);
    assert_eq!(r.id, "id-1");
    assert_eq!(r.name.as_deref(), Some("New Name"));
    assert_eq!(r.tags.as_ref().unwrap().len(), 1);
    assert_eq!(r.fields.as_ref().unwrap().len(), 1);
}

#[test]
fn update_entry_request_all_none() {
    let u = UpdateEntryRequest {
        id: "id-2".into(),
        name: None,
        tags: None,
        fields: None,
    };
    let r = roundtrip(&u);
    assert!(r.name.is_none());
    assert!(r.tags.is_none());
    assert!(r.fields.is_none());
}

#[test]
fn update_entry_request_empty_optionals() {
    let u = UpdateEntryRequest {
        id: String::new(),
        name: Some(String::new()),
        tags: Some(vec![]),
        fields: Some(vec![]),
    };
    let r = roundtrip(&u);
    assert_eq!(r.name.as_deref(), Some(""));
    assert_eq!(r.tags.unwrap().len(), 0);
    assert_eq!(r.fields.unwrap().len(), 0);
}

// ── UnlockRequest ──

#[test]
fn unlock_request_roundtrip() {
    let u = UnlockRequest {
        password: "hunter2".into(),
    };
    let r = roundtrip(&u);
    assert_eq!(r.password, "hunter2");
}

#[test]
fn unlock_request_empty() {
    let u = UnlockRequest {
        password: String::new(),
    };
    let r = roundtrip(&u);
    assert!(r.password.is_empty());
}

#[test]
fn unlock_request_unicode() {
    let u = UnlockRequest {
        password: "パスワード🔐".into(),
    };
    let r = roundtrip(&u);
    assert_eq!(r.password, "パスワード🔐");
}

// ── CreateVaultRequest ──

#[test]
fn create_vault_request_roundtrip() {
    let c = CreateVaultRequest {
        password: "pass123".into(),
    };
    let r = roundtrip(&c);
    assert_eq!(r.password, "pass123");
}

#[test]
fn create_vault_request_empty() {
    let c = CreateVaultRequest {
        password: String::new(),
    };
    let r = roundtrip(&c);
    assert!(r.password.is_empty());
}

// ── ChatMessage ──

#[test]
fn chat_message_roundtrip() {
    let m = ChatMessage {
        id: "msg-1".into(),
        role: "user".into(),
        content: "Hello, world!".into(),
        created_at: ts(),
    };
    let r = roundtrip(&m);
    assert_eq!(r.id, "msg-1");
    assert_eq!(r.role, "user");
    assert_eq!(r.content, "Hello, world!");
}

#[test]
fn chat_message_unicode() {
    let m = ChatMessage {
        id: "msg-2".into(),
        role: "assistant".into(),
        content: "こんにちは🌍".into(),
        created_at: ts(),
    };
    let r = roundtrip(&m);
    assert_eq!(r.content, "こんにちは🌍");
}

#[test]
fn chat_message_empty() {
    let m = ChatMessage {
        id: String::new(),
        role: String::new(),
        content: String::new(),
        created_at: ts(),
    };
    let r = roundtrip(&m);
    assert!(r.id.is_empty());
    assert!(r.role.is_empty());
    assert!(r.content.is_empty());
}

// ── SendChatRequest ──

#[test]
fn send_chat_request_roundtrip() {
    let s = SendChatRequest {
        content: "tell me a joke".into(),
    };
    let r = roundtrip(&s);
    assert_eq!(r.content, "tell me a joke");
}

#[test]
fn send_chat_request_empty() {
    let s = SendChatRequest {
        content: String::new(),
    };
    let r = roundtrip(&s);
    assert!(r.content.is_empty());
}

// ── ChatResponse ──

#[test]
fn chat_response_all_some() {
    let c = ChatResponse {
        message: "Sure!".into(),
        action: Some("create_entry".into()),
        user_message_display: Some("Create entry".into()),
    };
    let r = roundtrip(&c);
    assert_eq!(r.message, "Sure!");
    assert_eq!(r.action.as_deref(), Some("create_entry"));
    assert_eq!(r.user_message_display.as_deref(), Some("Create entry"));
}

#[test]
fn chat_response_all_none() {
    let c = ChatResponse {
        message: String::new(),
        action: None,
        user_message_display: None,
    };
    let r = roundtrip(&c);
    assert!(r.action.is_none());
    assert!(r.user_message_display.is_none());
}

#[test]
fn chat_response_mixed_options() {
    let c = ChatResponse {
        message: "msg".into(),
        action: Some(String::new()),
        user_message_display: None,
    };
    let r = roundtrip(&c);
    assert_eq!(r.action.as_deref(), Some(""));
    assert!(r.user_message_display.is_none());
}

// ── ExportRequest ──

#[test]
fn export_request_roundtrip() {
    let e = ExportRequest {
        password: "export_pass".into(),
    };
    let r = roundtrip(&e);
    assert_eq!(r.password, "export_pass");
}

#[test]
fn export_request_empty() {
    let e = ExportRequest {
        password: String::new(),
    };
    let r = roundtrip(&e);
    assert!(r.password.is_empty());
}

// ── ExportResponse ──

#[test]
fn export_response_roundtrip() {
    let e = ExportResponse {
        path: "/tmp/backup.json".into(),
        entry_count: 10,
    };
    let r = roundtrip(&e);
    assert_eq!(r.path, "/tmp/backup.json");
    assert_eq!(r.entry_count, 10);
}

#[test]
fn export_response_empty_path_zero_count() {
    let e = ExportResponse {
        path: String::new(),
        entry_count: 0,
    };
    let r = roundtrip(&e);
    assert!(r.path.is_empty());
    assert_eq!(r.entry_count, 0);
}

// ── ImportRequest ──

#[test]
fn import_request_roundtrip() {
    let i = ImportRequest {
        password: "import_pass".into(),
        path: "/tmp/backup.json".into(),
    };
    let r = roundtrip(&i);
    assert_eq!(r.password, "import_pass");
    assert_eq!(r.path, "/tmp/backup.json");
}

#[test]
fn import_request_empty() {
    let i = ImportRequest {
        password: String::new(),
        path: String::new(),
    };
    let r = roundtrip(&i);
    assert!(r.password.is_empty());
    assert!(r.path.is_empty());
}

// ── ImportResponse ──

#[test]
fn import_response_roundtrip() {
    let i = ImportResponse { entry_count: 5 };
    let r = roundtrip(&i);
    assert_eq!(r.entry_count, 5);
}

#[test]
fn import_response_zero() {
    let i = ImportResponse { entry_count: 0 };
    let r = roundtrip(&i);
    assert_eq!(r.entry_count, 0);
}

// ── JSON string edge cases ──

#[test]
fn json_deserialize_with_extra_field_is_ok() {
    let json = r#"{"password":"test","extra":true}"#;
    let r: UnlockRequest = serde_json::from_str(json).unwrap();
    assert_eq!(r.password, "test");
}

#[test]
fn json_deserialize_missing_field_fails() {
    let json = r#"{}"#;
    let r = serde_json::from_str::<UnlockRequest>(json);
    assert!(r.is_err());
}

#[test]
fn json_deserialize_wrong_type_fails() {
    let json = r#"{"password":123}"#;
    let r = serde_json::from_str::<UnlockRequest>(json);
    assert!(r.is_err());
}

#[test]
fn json_invalid_syntax_fails() {
    let json = r#"{"password":}"#;
    let r = serde_json::from_str::<UnlockRequest>(json);
    assert!(r.is_err());
}

// ── Debug and Clone ──

#[test]
fn vault_status_debug_and_clone() {
    let v = VaultStatus {
        is_initialized: true,
        is_unlocked: false,
        entry_count: 1,
    };
    let debug_str = format!("{:?}", v);
    assert!(debug_str.contains("VaultStatus"));
    let cloned = v.clone();
    assert_eq!(cloned.entry_count, 1);
}

#[test]
fn entry_dto_clone() {
    let e = EntryDto {
        id: "id".into(),
        kind: "k".into(),
        name: "n".into(),
        tags: vec![],
        fields: vec![],
        version: 0,
        created_at: ts(),
        updated_at: ts(),
    };
    let cloned = e.clone();
    assert_eq!(cloned.id, "id");
}

#[test]
fn chat_message_debug() {
    let m = ChatMessage {
        id: "1".into(),
        role: "user".into(),
        content: "hi".into(),
        created_at: ts(),
    };
    let s = format!("{:?}", m);
    assert!(s.contains("ChatMessage"));
    assert!(s.contains("hi"));
}

// ── Serde JSON format verification ──

#[test]
fn vault_status_json_keys() {
    let v = VaultStatus {
        is_initialized: true,
        is_unlocked: false,
        entry_count: 5,
    };
    let json = serde_json::to_value(&v).unwrap();
    assert!(json.get("is_initialized").is_some());
    assert!(json.get("is_unlocked").is_some());
    assert!(json.get("entry_count").is_some());
    assert_eq!(json["is_initialized"], true);
    assert_eq!(json["entry_count"], 5);
}

#[test]
fn field_dto_json_keys() {
    let f = FieldDto {
        key: "k".into(),
        value: "v".into(),
        hidden: true,
    };
    let json = serde_json::to_value(&f).unwrap();
    assert_eq!(json["key"], "k");
    assert_eq!(json["value"], "v");
    assert_eq!(json["hidden"], true);
}

// ── Large values ──

#[test]
fn large_vec_entry() {
    let tags: Vec<String> = (0..1000).map(|i| format!("tag-{i}")).collect();
    let e = EntryDto {
        id: "big".into(),
        kind: "test".into(),
        name: "Big".into(),
        tags,
        fields: vec![],
        version: u64::MAX,
        created_at: ts(),
        updated_at: ts(),
    };
    let r = roundtrip(&e);
    assert_eq!(r.tags.len(), 1000);
    assert_eq!(r.version, u64::MAX);
}

#[test]
fn usize_max_values() {
    let v = VaultStatus {
        is_initialized: false,
        is_unlocked: false,
        entry_count: usize::MAX,
    };
    let r = roundtrip(&v);
    assert_eq!(r.entry_count, usize::MAX);
}

// ── Whitespace and special characters ──

#[test]
fn string_with_newlines_and_tabs() {
    let s = SendChatRequest {
        content: "line1\nline2\ttab\"quoted\\".into(),
    };
    let r = roundtrip(&s);
    assert_eq!(r.content, "line1\nline2\ttab\"quoted\\");
}

// ── Deserialization from raw JSON ──

#[test]
fn deserialize_entry_dto_from_json() {
    let json = r#"{"id":"a","kind":"b","name":"c","tags":["t"],"fields":[{"key":"k","value":"v","hidden":false}],"version":1,"created_at":"2024-01-01T00:00:00Z","updated_at":"2024-01-01T00:00:00Z"}"#;
    let e: EntryDto = serde_json::from_str(json).unwrap();
    assert_eq!(e.id, "a");
    assert_eq!(e.tags, vec!["t"]);
    assert_eq!(e.fields[0].key, "k");
}

#[test]
fn deserialize_chat_response_from_json() {
    let json = r#"{"message":"ok","action":null,"user_message_display":null}"#;
    let c: ChatResponse = serde_json::from_str(json).unwrap();
    assert_eq!(c.message, "ok");
    assert!(c.action.is_none());
    assert!(c.user_message_display.is_none());
}

#[test]
fn deserialize_chat_response_from_json_with_options() {
    let json = r#"{"message":"ok","action":"do_thing","user_message_display":"Display"}"#;
    let c: ChatResponse = serde_json::from_str(json).unwrap();
    assert_eq!(c.action.as_deref(), Some("do_thing"));
    assert_eq!(c.user_message_display.as_deref(), Some("Display"));
}

#[test]
fn deserialize_update_request_partial_json() {
    let json = r#"{"id":"x"}"#;
    let u: UpdateEntryRequest = serde_json::from_str(json).unwrap();
    assert_eq!(u.id, "x");
    assert!(u.name.is_none());
    assert!(u.tags.is_none());
    assert!(u.fields.is_none());
}
