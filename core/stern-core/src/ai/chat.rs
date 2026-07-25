use crate::ai::intent::{Intent, classify_intent};
use crate::ai::model::AiModel;
use std::sync::Arc;

pub struct ChatHandler {
    _model: Option<Arc<AiModel>>,
}

impl ChatHandler {
    #[must_use]
    pub fn new(model: Option<Arc<AiModel>>) -> Self {
        Self { _model: model }
    }

    #[must_use]
    pub fn process_message(&self, user_input: &str) -> ChatResponse {
        let intent = classify_intent(user_input);

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
                        "I'll store your {kind} \"{name}\" in the vault.{field_desc}\n\nReady to save. Click confirm to encrypt and store it locally."
                    ),
                    action: Some(Action::StoreEntry {
                        name: name.clone(),
                        kind: kind.clone(),
                        fields: fields.clone(),
                    }),
                }
            }
            Intent::RetrieveSecret { ref name } => ChatResponse {
                message: format!(
                    "Searching for \"{name}\" in your vault..."
                ),
                action: Some(Action::SearchEntry {
                    query: name.clone(),
                }),
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
                }
            }
            Intent::DeleteSecret { ref name } => ChatResponse {
                message: format!(
                    "Are you sure you want to delete \"{name}\"? This cannot be undone."
                ),
                action: Some(Action::ConfirmDelete {
                    name: name.clone(),
                }),
            },
            Intent::UpdateSecret { ref name, ref fields } => {
                let field_desc = if fields.is_empty() {
                    "What would you like to update?".to_string()
                } else {
                    let items: Vec<String> = fields
                        .iter()
                        .map(|(k, v)| format!("  {k} → {v}"))
                        .collect();
                    format!("Updating:\n{}", items.join("\n"))
                };
                ChatResponse {
                    message: format!("Updating \"{name}\":\n{field_desc}"),
                    action: Some(Action::UpdateEntry {
                        name: name.clone(),
                        fields: fields.clone(),
                    }),
                }
            }
            Intent::Help => ChatResponse {
                message: "I can help you manage your secrets. Here's what I can do:\n\n\
                    🔐 Store a secret — \"save my GitHub password\"\n\
                    🔍 Find a secret — \"get my Gmail credentials\"\n\
                    📋 List all — \"show all passwords\"\n\
                    ✏️ Update — \"change my Netflix password\"\n\
                    🗑️ Delete — \"remove my WiFi password\"\n\n\
                    Just tell me what you need in natural language."
                    .to_string(),
                action: None,
            },
            Intent::Greeting => ChatResponse {
                message: "Hey! I'm Stern, your local secrets manager. \
                    Everything stays encrypted on your device.\n\n\
                    Ask me to store, find, or manage your secrets."
                    .to_string(),
                action: None,
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
            },
        }
    }
}

pub struct ChatResponse {
    pub message: String,
    pub action: Option<Action>,
}

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
}
