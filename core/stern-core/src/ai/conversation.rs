// Copyright (c) 2026 Christopher Yang
// SPDX-License-Identifier: MIT

//! Guided store conversation.
//!
//! When a store request does not carry everything a secret needs,
//! the chat collects it field by field instead of guessing: first
//! what kind of secret it is (a short lettered list), then one
//! question per field, then a confirmation. The user can answer a
//! choice with its letter or its label, skip optional fields, and
//! cancel at any point.
//!
//! The engine is deliberately deterministic — no model output is
//! involved — so the questions, their order, and the resulting
//! [`Intent`] are exactly testable. Model-based parsing feeds the
//! same entry points: a parsed store that is missing required
//! fields lands in [`Conversation::prefilled`], and a bare store
//! request lands in [`Conversation::start`].

use crate::ai::intent::Intent;

/// One field of a secret kind.
pub struct FieldSpec {
    /// Key the value is stored under.
    pub key: &'static str,
    /// The question asked to collect it.
    pub prompt: &'static str,
    /// Whether the conversation may finish without it. Optional
    /// fields can be skipped by answering "skip".
    pub required: bool,
    /// Whether the value is masked in the confirmation summary.
    pub sensitive: bool,
}

/// A kind of secret the guided conversation can store. The first
/// field doubles as the entry's name (the website for a login, the
/// title for a text, and so on).
pub struct KindSpec {
    /// Stable identifier, stored as the entry kind.
    pub id: &'static str,
    /// Label shown in the kind question.
    pub label: &'static str,
    /// The field that receives a parsed store's bare value (the
    /// password for a login, the text for a short text, ...).
    pub value_key: &'static str,
    /// Fields in the order they are asked.
    pub fields: &'static [FieldSpec],
}

const LOGIN_FIELDS: &[FieldSpec] = &[
    FieldSpec {
        key: "website",
        prompt: "What website is this username/password for?",
        required: true,
        sensitive: false,
    },
    FieldSpec {
        key: "username",
        prompt: "What is the username?",
        required: true,
        sensitive: false,
    },
    FieldSpec {
        key: "password",
        prompt: "What is the password?",
        required: true,
        sensitive: true,
    },
];

const TEXT_FIELDS: &[FieldSpec] = &[
    FieldSpec {
        key: "title",
        prompt: "What should I call this text?",
        required: true,
        sensitive: false,
    },
    FieldSpec {
        key: "text",
        prompt: "What is the text?",
        required: true,
        sensitive: true,
    },
];

const DOCUMENT_FIELDS: &[FieldSpec] = &[
    FieldSpec {
        key: "title",
        prompt: "What is the document called?",
        required: true,
        sensitive: false,
    },
    FieldSpec {
        key: "content",
        prompt: "Paste the document's text.",
        required: true,
        sensitive: true,
    },
];

const CARD_FIELDS: &[FieldSpec] = &[
    FieldSpec {
        key: "label",
        prompt: "What should I call this card? (for example, Chase Sapphire)",
        required: true,
        sensitive: false,
    },
    FieldSpec {
        key: "number",
        prompt: "What is the card number?",
        required: true,
        sensitive: true,
    },
    FieldSpec {
        key: "expiry",
        prompt: "What is the expiry date? (MM/YY)",
        required: true,
        sensitive: false,
    },
    FieldSpec {
        key: "cvv",
        prompt: "What is the CVV?",
        required: true,
        sensitive: true,
    },
];

const CONTACT_FIELDS: &[FieldSpec] = &[
    FieldSpec {
        key: "name",
        prompt: "Whose contact info is this?",
        required: true,
        sensitive: false,
    },
    FieldSpec {
        key: "phone",
        prompt: "What is the phone number?",
        required: true,
        sensitive: false,
    },
    FieldSpec {
        key: "email",
        prompt: "What is the email address? (or type skip)",
        required: false,
        sensitive: false,
    },
    FieldSpec {
        key: "address",
        prompt: "What is the address? (or type skip)",
        required: false,
        sensitive: false,
    },
];

/// The kinds the kind question offers, in the order it offers them.
pub const KINDS: &[KindSpec] = &[
    KindSpec {
        id: "login",
        label: "username/password",
        value_key: "password",
        fields: LOGIN_FIELDS,
    },
    KindSpec {
        id: "text",
        label: "short text",
        value_key: "text",
        fields: TEXT_FIELDS,
    },
    KindSpec {
        id: "document",
        label: "document",
        value_key: "content",
        fields: DOCUMENT_FIELDS,
    },
    KindSpec {
        id: "card",
        label: "credit card",
        value_key: "number",
        fields: CARD_FIELDS,
    },
    KindSpec {
        id: "contact",
        label: "contact info",
        value_key: "phone",
        fields: CONTACT_FIELDS,
    },
];

/// The kind a rule-classified store kind string belongs to, if it
/// maps onto one of the guided kinds.
fn kind_for_intent_kind(intent_kind: &str) -> Option<&'static KindSpec> {
    let id = match intent_kind {
        "password" | "secret" | "credential" | "login" | "api key" | "token" | "ssh" | "wifi" => {
            "login"
        }
        "note" | "text" => "text",
        "document" => "document",
        "card" | "credit card" => "card",
        "contact" | "contact info" => "contact",
        _ => return None,
    };
    KINDS.iter().find(|kind| kind.id == id)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    AwaitingKind,
    Collecting,
    Confirming,
}

/// One turn of a guided conversation.
#[derive(Debug)]
pub enum Step {
    /// Ask the user something. `options` are the lettered choices
    /// for choice questions (empty for free-value questions).
    /// `sensitive` marks a question whose answer is a secret value:
    /// the UI hides the typing (each character shows briefly, then
    /// turns into a mask character).
    Prompt {
        message: String,
        options: Vec<String>,
        sensitive: bool,
    },
    /// The conversation collected a complete store request.
    Finished(Intent),
    /// The user cancelled, or declined at the confirmation.
    Cancelled,
}

/// A guided store conversation in progress.
pub struct Conversation {
    stage: Stage,
    kind: Option<&'static KindSpec>,
    /// Keys of fields already answered or skipped, in answer order.
    answered: Vec<&'static str>,
    /// The collected values, in field order.
    fields: Vec<(String, String)>,
    /// Whether optional fields are asked. A fresh conversation asks
    /// them; one prefilled from a parsed command does not — the
    /// command already said everything it had to say.
    ask_optional: bool,
}

impl Conversation {
    /// Start a fresh conversation with the kind question.
    #[must_use]
    pub fn start() -> (Self, Step) {
        let conversation = Self {
            stage: Stage::AwaitingKind,
            kind: None,
            answered: Vec::new(),
            fields: Vec::new(),
            ask_optional: true,
        };
        let step = Self::kind_prompt("What is this secret for?");
        (conversation, step)
    }

    /// Start a conversation prefilled from a parsed store request,
    /// or return `None` when nothing the conversation would ask is
    /// missing (the caller then executes the request as-is).
    #[must_use]
    pub fn prefilled(
        intent_kind: &str,
        name: &str,
        parsed_fields: &[(String, String)],
    ) -> Option<(Self, Step)> {
        let kind = kind_for_intent_kind(intent_kind)?;
        let mut conversation = Self {
            stage: Stage::Collecting,
            kind: Some(kind),
            answered: Vec::new(),
            fields: Vec::new(),
            ask_optional: false,
        };
        // The parsed name answers the name field (the website for a
        // login, the title for a text, ...).
        let name_key = kind.fields.first()?.key;
        if !name.is_empty() {
            conversation.record(name_key, name.to_string());
        }
        for (key, value) in parsed_fields {
            if let Some(spec) = kind.fields.iter().find(|field| field.key == key) {
                conversation.record(spec.key, value.clone());
            } else if key == intent_kind || key == "value" {
                conversation.record(kind.value_key, value.clone());
            }
        }
        conversation.next_field()?;
        let step = conversation.next_prompt();
        Some((conversation, step))
    }

    /// Advance the conversation with the user's reply.
    pub fn handle(&mut self, input: &str) -> Step {
        let trimmed = input.trim();
        if is_cancel(trimmed) {
            return Step::Cancelled;
        }
        match self.stage {
            Stage::AwaitingKind => self.handle_kind_choice(trimmed),
            Stage::Collecting => self.handle_field_value(trimmed),
            Stage::Confirming => self.handle_confirmation(trimmed),
        }
    }

    fn handle_kind_choice(&mut self, input: &str) -> Step {
        let Some(kind) = parse_kind_choice(input) else {
            return Self::kind_prompt("Please pick one of these:");
        };
        self.kind = Some(kind);
        self.stage = Stage::Collecting;
        self.next_prompt()
    }

    fn handle_field_value(&mut self, input: &str) -> Step {
        let Some(field) = self.next_field() else {
            return self.next_prompt();
        };
        if !field.required && input.eq_ignore_ascii_case("skip") {
            self.answered.push(field.key);
            return self.next_prompt();
        }
        if field.required && input.is_empty() {
            return Step::Prompt {
                message: field.prompt.to_string(),
                options: Vec::new(),
                sensitive: field.sensitive,
            };
        }
        self.record(field.key, input.to_string());
        self.next_prompt()
    }

    fn handle_confirmation(&mut self, input: &str) -> Step {
        let lower = input.to_lowercase();
        match lower.as_str() {
            "yes" | "y" | "a" => {
                let Some(kind) = self.kind else {
                    return Step::Cancelled;
                };
                let name = self
                    .fields
                    .first()
                    .map_or_else(String::new, |(_, value)| value.clone());
                Step::Finished(Intent::StoreSecret {
                    name,
                    kind: kind.id.to_string(),
                    fields: std::mem::take(&mut self.fields),
                })
            }
            "no" | "n" | "b" => Step::Cancelled,
            _ => Step::Prompt {
                message: "Please answer yes or no:".to_string(),
                options: vec!["yes".to_string(), "no".to_string()],
                sensitive: false,
            },
        }
    }

    /// Record a value for a field, unless it is already answered.
    fn record(&mut self, key: &'static str, value: String) {
        if self.answered.contains(&key) {
            return;
        }
        self.answered.push(key);
        self.fields.push((key.to_string(), value));
    }

    /// The first field still to ask, in spec order. Optional fields
    /// are only asked when this conversation asks them at all.
    fn next_field(&self) -> Option<&'static FieldSpec> {
        let kind = self.kind?;
        kind.fields.iter().find(|field| {
            !self.answered.contains(&field.key) && (field.required || self.ask_optional)
        })
    }

    /// The prompt for the next unanswered field, or the
    /// confirmation when nothing is left to ask.
    fn next_prompt(&mut self) -> Step {
        if let Some(field) = self.next_field() {
            return Step::Prompt {
                message: field.prompt.to_string(),
                options: Vec::new(),
                sensitive: field.sensitive,
            };
        }
        self.stage = Stage::Confirming;
        let Some(kind) = self.kind else {
            return Step::Cancelled;
        };
        let mut lines = vec![format!("Save this {}?", kind.label)];
        for (key, value) in &self.fields {
            let sensitive = kind
                .fields
                .iter()
                .find(|field| field.key == key)
                .is_some_and(|field| field.sensitive);
            if sensitive {
                lines.push(format!("  {key}: ••••"));
            } else {
                lines.push(format!("  {key}: {value}"));
            }
        }
        Step::Prompt {
            message: lines.join("\n"),
            options: vec!["yes".to_string(), "no".to_string()],
            sensitive: false,
        }
    }

    fn kind_prompt(message: &str) -> Step {
        Step::Prompt {
            message: message.to_string(),
            options: KINDS.iter().map(|kind| kind.label.to_string()).collect(),
            sensitive: false,
        }
    }
}

/// Whether the reply cancels the conversation, at any stage.
pub(crate) fn is_cancel(input: &str) -> bool {
    matches!(
        input.to_lowercase().as_str(),
        "cancel" | "stop" | "quit" | "never mind"
    )
}

/// Parse a kind answer: a letter (`a`–`e`), a kind label, or a kind
/// identifier.
fn parse_kind_choice(input: &str) -> Option<&'static KindSpec> {
    let lower = input.to_lowercase();
    if lower.len() == 1 {
        let byte = lower.as_bytes().first()?;
        let index = usize::from(*byte).checked_sub('a' as usize)?;
        return KINDS.get(index);
    }
    KINDS
        .iter()
        .find(|kind| kind.label == lower || kind.id == lower)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn start_at_fields() -> Conversation {
        let (conversation, step) = Conversation::start();
        assert!(matches!(step, Step::Prompt { .. }));
        conversation
    }

    #[test]
    fn kind_question_lists_the_five_kinds_in_order() {
        let (_, step) = Conversation::start();
        match step {
            Step::Prompt {
                message, options, ..
            } => {
                assert_eq!(message, "What is this secret for?");
                assert_eq!(
                    options,
                    vec![
                        "username/password",
                        "short text",
                        "document",
                        "credit card",
                        "contact info"
                    ]
                );
            }
            _ => panic!("expected a prompt"),
        }
    }

    #[test]
    fn kind_choice_accepts_a_letter_or_a_label() {
        let mut by_letter = start_at_fields();
        match by_letter.handle("a") {
            Step::Prompt { message, .. } => {
                assert_eq!(message, "What website is this username/password for?");
            }
            _ => panic!("expected the website prompt"),
        }
        let mut by_label = start_at_fields();
        match by_label.handle("Credit Card") {
            Step::Prompt { message, .. } => {
                assert!(message.contains("call this card"));
            }
            _ => panic!("expected the card label prompt"),
        }
    }

    #[test]
    fn unknown_kind_answer_reprompts() {
        let mut conversation = start_at_fields();
        match conversation.handle("banana") {
            Step::Prompt {
                message, options, ..
            } => {
                assert_eq!(message, "Please pick one of these:");
                assert_eq!(options.len(), 5);
            }
            _ => panic!("expected a re-prompt"),
        }
    }

    #[test]
    fn full_login_flow_finishes_with_a_store_intent() {
        let mut conversation = start_at_fields();
        conversation.handle("a");
        conversation.handle("Google");
        conversation.handle("chris");
        match conversation.handle("hunter2") {
            Step::Prompt {
                message, options, ..
            } => {
                assert!(message.starts_with("Save this username/password?"));
                assert!(message.contains("website: Google"));
                assert!(message.contains("password: ••••"));
                assert_eq!(options, vec!["yes", "no"]);
            }
            _ => panic!("expected the confirmation prompt"),
        }
        match conversation.handle("yes") {
            Step::Finished(Intent::StoreSecret { name, kind, fields }) => {
                assert_eq!(name, "Google");
                assert_eq!(kind, "login");
                assert_eq!(
                    fields,
                    vec![
                        ("website".to_string(), "Google".to_string()),
                        ("username".to_string(), "chris".to_string()),
                        ("password".to_string(), "hunter2".to_string()),
                    ]
                );
            }
            _ => panic!("expected a finished store intent"),
        }
    }

    #[test]
    fn declining_the_confirmation_cancels() {
        let mut conversation = start_at_fields();
        conversation.handle("b");
        conversation.handle("Shopping list");
        conversation.handle("eggs and milk");
        assert!(matches!(conversation.handle("no"), Step::Cancelled));
    }

    #[test]
    fn cancel_works_at_any_stage() {
        let mut conversation = start_at_fields();
        assert!(matches!(conversation.handle("cancel"), Step::Cancelled));
        let mut conversation = start_at_fields();
        conversation.handle("a");
        conversation.handle("Google");
        assert!(matches!(conversation.handle("stop"), Step::Cancelled));
    }

    #[test]
    fn optional_fields_can_be_skipped() {
        let mut conversation = start_at_fields();
        conversation.handle("e");
        conversation.handle("Mom");
        conversation.handle("555-0100");
        conversation.handle("skip");
        match conversation.handle("skip") {
            Step::Prompt { message, .. } => {
                assert!(message.starts_with("Save this contact info?"));
                assert!(!message.contains("email"));
            }
            _ => panic!("expected the confirmation prompt"),
        }
    }

    #[test]
    fn prefilled_login_asks_only_the_missing_username() {
        let parsed = vec![("password".to_string(), "hunter2".to_string())];
        let (mut conversation, step) =
            Conversation::prefilled("password", "GitHub", &parsed).unwrap();
        match step {
            Step::Prompt { message, .. } => {
                assert_eq!(message, "What is the username?");
            }
            _ => panic!("expected the username prompt"),
        }
        conversation.handle("chris");
        match conversation.handle("yes") {
            Step::Finished(Intent::StoreSecret { name, kind, fields }) => {
                assert_eq!(name, "GitHub");
                assert_eq!(kind, "login");
                assert!(fields.contains(&("password".to_string(), "hunter2".to_string())));
                assert!(fields.contains(&("username".to_string(), "chris".to_string())));
            }
            _ => panic!("expected a finished store intent"),
        }
    }

    #[test]
    fn prefilled_complete_request_returns_none() {
        let parsed = vec![
            ("username".to_string(), "chris".to_string()),
            ("password".to_string(), "hunter2".to_string()),
        ];
        assert!(Conversation::prefilled("password", "GitHub", &parsed).is_none());
    }

    #[test]
    fn prefilled_unknown_kind_returns_none() {
        assert!(Conversation::prefilled("biometric", "Face", &[]).is_none());
    }
}
