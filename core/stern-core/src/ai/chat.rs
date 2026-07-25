use crate::ai::intent::{Intent, classify_intent};
use crate::ai::model::AiModel;
use std::sync::Arc;

pub struct ChatHandler {
    _model: Option<Arc<AiModel>>,
}

impl ChatHandler {
    #[must_use]
    pub const fn new(model: Option<Arc<AiModel>>) -> Self {
        Self { _model: model }
    }

    #[must_use]
    pub fn process_message(&self, user_input: &str) -> ChatResponse {
        let intent = classify_intent(user_input);
        let mut response = self.intent_to_response(intent);

        let is_vault_action = matches!(
            response.action,
            Some(Action::CreateVault | Action::UnlockVault | Action::ExportVault | Action::ImportVault)
        );
        if is_vault_action {
            response.user_message_display = None;
        }
        response
    }

    #[allow(clippy::unused_self, clippy::too_many_lines, clippy::needless_pass_by_value)]
    fn intent_to_response(&self, intent: Intent) -> ChatResponse {
        match intent {
            Intent::StoreSecret { ref name, ref kind, ref fields } => {
                let field_desc = if fields.is_empty() {
                    String::new()
                } else {
                    let items: Vec<String> = fields
                        .iter()
                        .map(|(k, v)| format!("  {k}: {v}"))
                        .collect();
                    format!("\nWith fields:\n{}", items.join("\n"))
                };

                ChatResponse {
                    message: format!(
                        "Stored your {kind} \"{name}\" in the vault.{field_desc}"
                    ),
                    action: Some(Action::StoreEntry {
                        name: name.clone(),
                        kind: kind.clone(),
                        fields: fields.clone(),
                    }),
                    user_message_display: None,
                }
            }
            Intent::RetrieveSecret { ref name } => ChatResponse {
                message: format!(
                    "Searching for \"{name}\" in your vault..."
                ),
                action: Some(Action::SearchEntry {
                    query: name.clone(),
                }),
                user_message_display: None,
            },
            Intent::ListSecrets { ref category } => {
                let cat_desc = category
                    .as_deref()
                    .map(|c| format!(" of type \"{c}\""))
                    .unwrap_or_default();
                ChatResponse {
                    message: format!(
                        "Here are all your secrets{cat_desc}:"
                    ),
                    action: Some(Action::ListEntries {
                        category: category.clone(),
                    }),
                    user_message_display: None,
                }
            }
            Intent::DeleteSecret { ref name } => ChatResponse {
                message: format!(
                    "Are you sure you want to delete \"{name}\"? This cannot be undone."
                ),
                action: Some(Action::ConfirmDelete {
                    name: name.clone(),
                }),
                user_message_display: None,
            },
            Intent::UpdateSecret { ref name, ref fields } => {
                let field_desc = if fields.is_empty() {
                    "What would you like to update?".to_string()
                } else {
                    let items: Vec<String> = fields
                        .iter()
                        .map(|(k, v)| format!("  {k} -> {v}"))
                        .collect();
                    format!("Updating:\n{}", items.join("\n"))
                };
                ChatResponse {
                    message: format!("Updating \"{name}\":\n{field_desc}"),
                    action: Some(Action::UpdateEntry {
                        name: name.clone(),
                        fields: fields.clone(),
                    }),
                    user_message_display: None,
                }
            }
            Intent::ExportVault => ChatResponse {
                message: "Opening secure export dialog...".to_string(),
                action: Some(Action::ExportVault),
                user_message_display: None,
            },
            Intent::ImportVault => ChatResponse {
                message: "Opening secure import dialog...".to_string(),
                action: Some(Action::ImportVault),
                user_message_display: None,
            },
            Intent::CreateVault => ChatResponse {
                message: "Opening secure vault creation dialog...".to_string(),
                action: Some(Action::CreateVault),
                user_message_display: None,
            },
            Intent::UnlockVault => ChatResponse {
                message: "Opening secure unlock dialog...".to_string(),
                action: Some(Action::UnlockVault),
                user_message_display: None,
            },
            Intent::Help => ChatResponse {
                message: "I can help you manage your secrets. Here's what I can do:\n\n\
                    \u{1f510} Store a secret -- \"save my GitHub password\"\n\
                    \u{1f50d} Find a secret -- \"get my Gmail credentials\"\n\
                    \u{1f4cb} List all -- \"show all passwords\"\n\
                    \u{270f}\u{fe0f} Update -- \"change my Netflix password\"\n\
                    \u{1f5d1}\u{fe0f} Delete -- \"remove my WiFi password\"\n\
                    \u{1f4e6} Export -- \"export my vault\"\n\
                    \u{1f4e5} Import -- \"import vault\"\n\n\
                    Just tell me what you need in natural language."
                    .to_string(),
                action: None,
                user_message_display: None,
            },
            Intent::Greeting => ChatResponse {
                message: "Hey! I'm Stern, your local secrets manager. \
                    Everything stays encrypted on your device.\n\n\
                    Ask me to store, find, or manage your secrets."
                    .to_string(),
                action: None,
                user_message_display: None,
            },
            Intent::Unknown { ref text } => ChatResponse {
                message: format!(
                    "I'm not sure what you mean by \"{text}\".\n\n\
                    Try something like:\n\
                    - \"save my GitHub password\"\n\
                    - \"find my API keys\"\n\
                    - \"list all secrets\"\n\
                    - \"help\""
                ),
                action: None,
                user_message_display: None,
            },
        }
    }
}

pub struct ChatResponse {
    pub message: String,
    pub action: Option<Action>,
    pub user_message_display: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(resp.message.contains("secrets"), "message: {}", resp.message);
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
            Action::SearchEntry { query } => {
                assert!(!query.is_empty());
            }
            other => panic!("expected SearchEntry, got: {other:?}"),
        }
    }

    #[test]
    fn list_response() {
        let resp = handler().process_message("list all passwords");
        assert!(resp.message.contains("password"), "message: {}", resp.message);
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
            Action::SearchEntry { query } => {
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
        assert!(matches!(resp.action.expect("should have action"), Action::ExportVault));
        assert!(resp.user_message_display.is_none());
    }

    #[test]
    fn import_response() {
        let resp = handler().process_message("import vault");
        assert!(resp.message.contains("import"), "message: {}", resp.message);
        assert!(matches!(resp.action.expect("should have action"), Action::ImportVault));
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
        let resp = handler().process_message("unlock vault");
        assert!(resp.message.contains("unlock"), "message: {}", resp.message);
        assert!(matches!(resp.action.expect("should have action"), Action::UnlockVault));
        assert!(resp.user_message_display.is_none());
    }

    #[test]
    fn unknown_response() {
        let resp = handler().process_message("asdfghjkl");
        assert!(resp.message.contains("not sure"), "message: {}", resp.message);
        assert!(resp.action.is_none());
    }

    #[test]
    fn vault_actions_clear_display() {
        for input in &["create vault", "unlock vault", "export vault", "import vault"] {
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
}

#[derive(Debug)]
pub enum Action {
    StoreEntry {
        name: String,
        kind: String,
        fields: Vec<(String, String)>,
    },
    SearchEntry {
        query: String,
    },
    ListEntries {
        category: Option<String>,
    },
    ConfirmDelete {
        name: String,
    },
    UpdateEntry {
        name: String,
        fields: Vec<(String, String)>,
    },
    ExportVault,
    ImportVault,
    CreateVault,
    UnlockVault,
}
