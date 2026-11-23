use crate::ai::conversation::{Conversation, Step as ConversationStep};
use crate::ai::intent::{classify_intent, Intent};
use crate::ai::model::AiModel;
use std::sync::{Arc, Mutex, RwLock};
use zeroize::Zeroizing;

/// A vault entry offered in a lettered choice.
#[derive(Debug, Clone)]
pub struct EntryCandidate {
    /// The entry's identifier.
    pub id: String,
    /// The label shown for it in the choice.
    pub label: String,
}

/// A lettered entry choice waiting for the user's pick.
struct PendingChoice {
    candidates: Vec<EntryCandidate>,
    field: Option<String>,
}

pub struct ChatHandler {
    model: RwLock<Option<Arc<AiModel>>>,
    /// The guided store conversation in progress, if any. While one
    /// is active it owns every incoming message: its replies are
    /// field values and choices, not new commands.
    conversation: Mutex<Option<Conversation>>,
    /// A lettered entry choice waiting for the user's pick, set
    /// when a search matched several entries.
    pending_choice: Mutex<Option<PendingChoice>>,
    /// Vault readiness as last synced by the app layer. Defaults
    /// to NOT ready — fail closed: until the app syncs the real
    /// state, the handler treats the vault as missing and locked
    /// and runs the gate rather than letting a request through.
    vault_state: Mutex<VaultState>,
    /// The vault setup/unlock conversation in progress, if any.
    /// While one is active it owns every incoming message: its
    /// replies are master passwords, not new commands.
    vault_gate: Mutex<Option<VaultGate>>,
}

/// Whether a vault exists and whether it is unlocked, as synced
/// from the app layer.
#[derive(Clone, Copy)]
struct VaultState {
    exists: bool,
    unlocked: bool,
}

/// How far the vault gate conversation has got.
#[derive(Clone, Copy, PartialEq, Eq)]
enum GateStage {
    SetupPassword,
    SetupConfirm,
    UnlockPassword,
    Completing,
}

/// A vault setup or unlock conversation in progress.
struct VaultGate {
    stage: GateStage,
    /// True for setting up a new vault, false for unlocking.
    setup: bool,
    /// The first typing of the new master password, kept only
    /// until the confirmation matches. Zeroized on drop.
    first_password: Option<Zeroizing<String>>,
    /// The message that was interrupted by the gate, replayed
    /// once the vault is ready.
    stashed: Option<String>,
}

/// The shortest master password the setup conversation accepts.
const MIN_MASTER_PASSWORD_CHARS: usize = 8;

/// What a candidate master password is still missing under the
/// policy (at least 8 characters, a lowercase letter, an
/// uppercase letter, a number, and a symbol), as one brief
/// comma-joined phrase — or nothing when it passes.
fn master_password_gap(password: &str) -> Option<String> {
    let mut missing: Vec<&str> = Vec::new();
    if password.chars().count() < MIN_MASTER_PASSWORD_CHARS {
        missing.push("8+ characters");
    }
    if !password.chars().any(char::is_lowercase) {
        missing.push("a lowercase letter");
    }
    if !password.chars().any(char::is_uppercase) {
        missing.push("an uppercase letter");
    }
    if !password.chars().any(|c| c.is_ascii_digit()) {
        missing.push("a number");
    }
    if !password.chars().any(|c| !c.is_alphanumeric()) {
        missing.push("a symbol");
    }
    if missing.is_empty() {
        None
    } else {
        Some(missing.join(", "))
    }
}

impl ChatHandler {
    #[must_use]
    pub const fn new(model: Option<Arc<AiModel>>) -> Self {
        Self {
            model: RwLock::new(model),
            conversation: Mutex::new(None),
            pending_choice: Mutex::new(None),
            vault_state: Mutex::new(VaultState {
                exists: false,
                unlocked: false,
            }),
            vault_gate: Mutex::new(None),
        }
    }

    /// Replace the loaded model. Called when a model download completes (or
    /// the model is removed) so the change takes effect without a restart.
    ///
    /// A poisoned lock is recovered (with a warning) rather than silently
    /// ignored: ignoring it would leave a stale model active with no trace
    /// of why the replacement never happened.
    pub fn set_model(&self, model: Option<Arc<AiModel>>) {
        let mut guard = self.model.write().unwrap_or_else(|poisoned| {
            tracing::warn!("chat model lock was poisoned; recovering it to replace the model");
            poisoned.into_inner()
        });
        *guard = model;
    }

    /// Whether an AI model is currently loaded. Intent classification falls
    /// back to the rule-based classifier when no model is present.
    ///
    /// Like [`ChatHandler::set_model`], a poisoned lock is recovered rather
    /// than misreporting `false` for a model that is actually loaded.
    #[must_use]
    pub fn has_model(&self) -> bool {
        let guard = self
            .model
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        guard.is_some()
    }

    /// Offer a lettered choice among search matches. The next
    /// message either picks one (answered with a reveal action),
    /// cancels, or — when it is not a choice at all — is processed
    /// normally with the choice forgotten.
    pub fn set_pending_choice(&self, candidates: Vec<EntryCandidate>, field: Option<String>) {
        let mut guard = self
            .pending_choice
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *guard = Some(PendingChoice { candidates, field });
    }

    /// Sync the vault's readiness from the app layer. The app
    /// calls this before every message: when a request needs the
    /// vault and the vault is not ready, the setup/unlock gate
    /// runs first instead of the request failing at save time.
    pub fn set_vault_state(&self, exists: bool, unlocked: bool) {
        let mut guard = self
            .vault_state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *guard = VaultState { exists, unlocked };
    }

    fn vault_state(&self) -> VaultState {
        let guard = self
            .vault_state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *guard
    }

    /// The app created or unlocked the vault the gate asked for:
    /// clear the gate and replay the interrupted message, if any.
    /// Returns the response to show (the replay's response, or a
    /// short confirmation when nothing was interrupted).
    pub fn vault_gate_completed(&self) -> Option<ChatResponse> {
        let gate = {
            let mut guard = self
                .vault_gate
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            guard.take()
        }?;
        if let Some(stashed) = gate.stashed {
            return Some(self.process_message(&stashed));
        }
        let message = if gate.setup {
            "Your vault is set up and unlocked. What would you like to do?"
        } else {
            "Vault unlocked. What would you like to do?"
        };
        Some(plain_response(message))
    }

    /// The app could not complete the gate action (for example,
    /// the vault already existed): abandon the gate.
    pub fn vault_gate_failed(&self) {
        let mut guard = self
            .vault_gate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *guard = None;
    }

    fn keep_gate(&self, gate: VaultGate) {
        let mut guard = self
            .vault_gate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *guard = Some(gate);
    }

    /// Begin the setup or unlock conversation and return its first
    /// prompt. `stashed` is the message to replay once the vault
    /// is ready.
    fn start_gate(&self, setup: bool, stashed: Option<String>) -> ChatResponse {
        let stage = if setup {
            GateStage::SetupPassword
        } else {
            GateStage::UnlockPassword
        };
        self.keep_gate(VaultGate {
            stage,
            setup,
            first_password: None,
            stashed,
        });
        if setup {
            sensitive_prompt(
                "First, set up your vault.\n\n\
                 Choose a master password — at least 8 characters, with a lowercase \
                 letter, an uppercase letter, a number, and a symbol:",
            )
        } else {
            sensitive_prompt(
                "Your vault is locked — I need it open to do that.\n\n\
                 Enter your master password:",
            )
        }
    }

    /// Advance the vault gate with the user's reply.
    fn handle_gate_message(&self, mut gate: VaultGate, user_input: &str) -> ChatResponse {
        if crate::ai::conversation::is_cancel(user_input.trim()) {
            return plain_response("Cancelled.");
        }
        match gate.stage {
            GateStage::SetupPassword => {
                if let Some(gap) = master_password_gap(user_input) {
                    self.keep_gate(gate);
                    return sensitive_prompt(&format!(
                        "That password still needs: {gap}.\n\n\
                         What master password would you like?"
                    ));
                }
                gate.first_password = Some(Zeroizing::new(user_input.to_string()));
                gate.stage = GateStage::SetupConfirm;
                self.keep_gate(gate);
                sensitive_prompt(
                    "⚠ If you forget it, there's no way to recover it — the vault \
                     stays locked forever.\n\n\
                     Type it once more to confirm:",
                )
            }
            GateStage::SetupConfirm => {
                let confirmed = gate
                    .first_password
                    .as_ref()
                    .is_some_and(|password| password.as_str() == user_input);
                if confirmed {
                    let password = gate.first_password.take().unwrap_or_default();
                    gate.stage = GateStage::Completing;
                    self.keep_gate(gate);
                    ChatResponse {
                        message: "Setting up your vault...".to_string(),
                        action: Some(Action::SetupVault { password }),
                        user_message_display: None,
                        options: Vec::new(),
                        input_sensitive: false,
                    }
                } else {
                    gate.first_password = None;
                    gate.stage = GateStage::SetupPassword;
                    self.keep_gate(gate);
                    sensitive_prompt(
                        "Those didn't match — let's start over.\n\n\
                         What master password would you like?",
                    )
                }
            }
            GateStage::UnlockPassword => {
                let password = Zeroizing::new(user_input.to_string());
                self.keep_gate(gate);
                ChatResponse {
                    message: "Unlocking...".to_string(),
                    action: Some(Action::UnlockVaultWith { password }),
                    user_message_display: None,
                    options: Vec::new(),
                    input_sensitive: false,
                }
            }
            GateStage::Completing => {
                self.keep_gate(gate);
                plain_response("One moment...")
            }
        }
    }

    /// Route a create/unlock request: into the gate when the
    /// vault is not ready, a plain answer when it already is.
    fn route_vault_request(&self, intent: &Intent) -> ChatResponse {
        let state = self.vault_state();
        if state.exists && state.unlocked {
            return plain_response(match intent {
                Intent::CreateVault => "Your vault is already set up and unlocked.",
                _ => "Your vault is already unlocked.",
            });
        }
        self.start_gate(!state.exists, None)
    }

    #[must_use]
    pub fn process_message(&self, user_input: &str) -> ChatResponse {
        // A vault gate in progress owns the message FIRST: its
        // replies are master passwords. A pending entry choice
        // must never intercept one — if the vault locks while a
        // choice is waiting, the password would otherwise be read
        // as a choice answer.
        let gate = {
            let mut guard = self
                .vault_gate
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            guard.take()
        };
        if let Some(gate) = gate {
            return self.handle_gate_message(gate, user_input);
        }

        // A lettered entry choice waiting on this message.
        let pending = {
            let mut guard = self
                .pending_choice
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            guard.take()
        };
        if let Some(choice) = pending {
            if crate::ai::conversation::is_cancel(user_input.trim()) {
                return ChatResponse {
                    message: "Cancelled.".to_string(),
                    action: None,
                    user_message_display: None,
                    options: Vec::new(),
                    input_sensitive: false,
                };
            }
            if let Some(picked) = parse_choice(user_input, &choice.candidates) {
                let (entry_id, label) = (picked.id.clone(), picked.label.clone());
                return ChatResponse {
                    message: format!("Showing \"{label}\"..."),
                    action: Some(Action::RevealEntry {
                        entry_id,
                        field: choice.field,
                    }),
                    user_message_display: None,
                    options: Vec::new(),
                    input_sensitive: false,
                };
            }
            // Not a choice: the user moved on, and the message is
            // processed normally below with the choice forgotten.
        }

        // A guided conversation in progress owns the message.
        let active = {
            let mut guard = self
                .conversation
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            guard.take()
        };
        if let Some(mut conversation) = active {
            return match conversation.handle(user_input) {
                ConversationStep::Prompt {
                    message,
                    options,
                    sensitive,
                } => {
                    self.keep_conversation(conversation);
                    ChatResponse {
                        message,
                        action: None,
                        user_message_display: None,
                        options,
                        input_sensitive: sensitive,
                    }
                }
                ConversationStep::Finished(intent) => self.finish(intent),
                ConversationStep::Cancelled => ChatResponse {
                    message: "Cancelled — nothing was saved.".to_string(),
                    action: None,
                    user_message_display: None,
                    options: Vec::new(),
                    input_sensitive: false,
                },
            };
        }

        let intent = self.acquire_intent(user_input);
        self.dispatch_fresh(user_input, intent)
    }

    /// Handle a fresh message once its intent is known: vault
    /// requests route into the gate, guided stores start their
    /// conversation, and everything else settles directly.
    fn dispatch_fresh(&self, user_input: &str, intent: Intent) -> ChatResponse {
        // Vault management requests route into the setup/unlock
        // gate (or a plain answer when the vault is already ready).
        if matches!(intent, Intent::CreateVault | Intent::UnlockVault) {
            return self.route_vault_request(&intent);
        }

        // The guided conversation starts where a parsed store has
        // nothing to file under: a content-free store request gets
        // the full interview (kind first), and a nameless parse —
        // a value with nothing to call it — gets asked for the
        // missing name and fields. A store the parser named
        // executes immediately, exactly as it always has.
        let guided = match &intent {
            Intent::StoreSecret { name, fields, .. } if name.is_empty() && fields.is_empty() => {
                Some(Conversation::start())
            }
            Intent::StoreSecret { name, kind, fields } if name.is_empty() => {
                Conversation::prefilled(kind, name, fields)
            }
            Intent::Unknown { .. } if has_store_verb(user_input) => Some(Conversation::start()),
            _ => None,
        };

        // Anything that touches the vault waits for the vault:
        // run the setup/unlock gate first and replay this message
        // once the vault is ready, instead of collecting a whole
        // secret and failing at save time. Small talk, help, and
        // unknown messages are never gated.
        let state = self.vault_state();
        let needs_vault = guided.is_some()
            || matches!(
                intent,
                Intent::StoreSecret { .. }
                    | Intent::RetrieveSecret { .. }
                    | Intent::ListSecrets { .. }
                    | Intent::DeleteSecret { .. }
                    | Intent::UpdateSecret { .. }
                    | Intent::ExportVault
                    | Intent::ImportVault
            );
        if needs_vault && !(state.exists && state.unlocked) {
            return self.start_gate(!state.exists, Some(user_input.to_string()));
        }

        if let Some((conversation, step)) = guided {
            return match step {
                ConversationStep::Prompt {
                    message,
                    options,
                    sensitive,
                } => {
                    self.keep_conversation(conversation);
                    ChatResponse {
                        message,
                        action: None,
                        user_message_display: None,
                        options,
                        input_sensitive: sensitive,
                    }
                }
                ConversationStep::Finished(intent) => self.finish(intent),
                ConversationStep::Cancelled => ChatResponse {
                    message: "Cancelled — nothing was saved.".to_string(),
                    action: None,
                    user_message_display: None,
                    options: Vec::new(),
                    input_sensitive: false,
                },
            };
        }

        self.finish(intent)
    }

    /// Put an in-progress guided conversation back for the next
    /// message.
    fn keep_conversation(&self, conversation: Conversation) {
        let mut guard = self
            .conversation
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *guard = Some(conversation);
    }

    /// Turn a settled intent into a response, applying the
    /// vault-action display rule.
    fn finish(&self, intent: Intent) -> ChatResponse {
        let mut response = self.intent_to_response(intent);

        let is_vault_action = matches!(
            response.action,
            Some(
                Action::CreateVault
                    | Action::UnlockVault
                    | Action::ExportVault
                    | Action::ImportVault
            )
        );
        if is_vault_action {
            response.user_message_display = None;
        }
        response
    }

    /// Get the intent for a fresh message: the downloaded model
    /// parses first when one is loaded; the rule classifier is the
    /// fallback for no model, a model error, or output that does
    /// not parse.
    fn acquire_intent(&self, user_input: &str) -> Intent {
        match self.model_intent(user_input) {
            // A greeting or help answer is cheap to double-check:
            // small models sometimes say "greeting" for a vault
            // command (measured on SmolLM2-135M: "delete my old
            // netflix entry" came back as a greeting). If the rules
            // see a vault action in the same message, trust them.
            Some(intent @ (Intent::Greeting | Intent::Help)) => match classify_intent(user_input) {
                fallback @ (Intent::StoreSecret { .. }
                | Intent::RetrieveSecret { .. }
                | Intent::ListSecrets { .. }
                | Intent::DeleteSecret { .. }
                | Intent::UpdateSecret { .. }
                | Intent::ExportVault
                | Intent::ImportVault
                | Intent::CreateVault
                | Intent::UnlockVault) => fallback,
                _ => intent,
            },
            Some(intent) => intent,
            None => classify_intent(user_input),
        }
    }

    /// Ask the loaded model to parse the message, if there is one.
    /// Any failure — no model, a mechanical error, unusable output —
    /// yields `None`, and the caller falls back to the rules.
    fn model_intent(&self, user_input: &str) -> Option<Intent> {
        // Clone the Arc and drop the lock before parsing: generation
        // can take seconds, and holding the read lock would stall
        // `set_model` for all of it.
        let model = {
            let guard = self
                .model
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            guard.as_ref().cloned()
        }?;
        match model.parse_command(user_input) {
            Ok(intent) => intent,
            Err(e) => {
                tracing::warn!("model parse failed; falling back to rules: {e}");
                None
            }
        }
    }

    #[allow(
        clippy::unused_self,
        clippy::too_many_lines,
        clippy::needless_pass_by_value
    )]
    fn intent_to_response(&self, intent: Intent) -> ChatResponse {
        match intent {
            Intent::StoreSecret {
                ref name,
                ref kind,
                ref fields,
            } => {
                let field_desc = if fields.is_empty() {
                    String::new()
                } else {
                    let items: Vec<String> =
                        fields.iter().map(|(k, v)| format!("  {k}: {v}")).collect();
                    format!("\nWith fields:\n{}", items.join("\n"))
                };

                ChatResponse {
                    message: format!("Stored your {kind} \"{name}\" in the vault.{field_desc}"),
                    action: Some(Action::StoreEntry {
                        name: name.clone(),
                        kind: kind.clone(),
                        fields: fields.clone(),
                    }),
                    user_message_display: None,
                    options: Vec::new(),
                    input_sensitive: false,
                }
            }
            Intent::RetrieveSecret {
                ref name,
                ref field,
            } => ChatResponse {
                message: format!("Searching for \"{name}\" in your vault..."),
                action: Some(Action::SearchEntry {
                    query: name.clone(),
                    field: field.clone(),
                }),
                user_message_display: None,
                options: Vec::new(),
                input_sensitive: false,
            },
            Intent::ListSecrets { ref category } => {
                let cat_desc = category
                    .as_deref()
                    .map_or_else(String::new, |c| format!(" of type \"{c}\""));
                ChatResponse {
                    message: format!("Here are all your secrets{cat_desc}:"),
                    action: Some(Action::ListEntries {
                        category: category.clone(),
                    }),
                    user_message_display: None,
                    options: Vec::new(),
                    input_sensitive: false,
                }
            }
            Intent::DeleteSecret { ref name } => ChatResponse {
                message: format!(
                    "Are you sure you want to delete \"{name}\"? This cannot be undone."
                ),
                action: Some(Action::ConfirmDelete { name: name.clone() }),
                user_message_display: None,
                options: Vec::new(),
                input_sensitive: false,
            },
            Intent::UpdateSecret {
                ref name,
                ref fields,
            } => {
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
                    options: Vec::new(),
                    input_sensitive: false,
                }
            }
            Intent::ExportVault => ChatResponse {
                message: "Opening secure export dialog...".to_string(),
                action: Some(Action::ExportVault),
                user_message_display: None,
                options: Vec::new(),
                input_sensitive: false,
            },
            Intent::ImportVault => ChatResponse {
                message: "Opening secure import dialog...".to_string(),
                action: Some(Action::ImportVault),
                user_message_display: None,
                options: Vec::new(),
                input_sensitive: false,
            },
            Intent::CreateVault => ChatResponse {
                message: "Opening secure vault creation dialog...".to_string(),
                action: Some(Action::CreateVault),
                user_message_display: None,
                options: Vec::new(),
                input_sensitive: false,
            },
            Intent::UnlockVault => ChatResponse {
                message: "Opening secure unlock dialog...".to_string(),
                action: Some(Action::UnlockVault),
                user_message_display: None,
                options: Vec::new(),
                input_sensitive: false,
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
                options: Vec::new(),
                input_sensitive: false,
            },
            Intent::Greeting => ChatResponse {
                message: "I'm doing well, thanks! I'm Stern, your local secrets manager — \
                    everything stays encrypted on this device.\n\n\
                    I can store a secret for you, find one again, list them all, or update \
                    and delete them. Try \"save my GitHub password\", or ask \"what's my \
                    password for Google?\""
                    .to_string(),
                action: None,
                user_message_display: None,
                options: Vec::new(),
                input_sensitive: false,
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
                options: Vec::new(),
                input_sensitive: false,
            },
        }
    }
}

/// Parse a reply to a lettered entry choice: a letter within
/// range, or the candidate's label.
fn parse_choice<'a>(input: &str, candidates: &'a [EntryCandidate]) -> Option<&'a EntryCandidate> {
    let trimmed = input.trim();
    // A single letter picks by position — but only when it is a
    // letter inside the offered range. Anything else (a digit, a
    // letter past the end) falls through to label matching, so an
    // entry whose name is itself one character stays pickable.
    if trimmed.len() == 1 {
        let byte = *trimmed.as_bytes().first()?;
        if byte.is_ascii_alphabetic() {
            let index = usize::from(byte.to_ascii_lowercase())
                .checked_sub('a' as usize)
                .unwrap_or(usize::MAX);
            if let Some(candidate) = candidates.get(index) {
                return Some(candidate);
            }
        }
    }
    let lower = trimmed.to_lowercase();
    candidates
        .iter()
        .find(|candidate| candidate.label.to_lowercase() == lower)
}

/// Whether the text carries a bare store verb the rule classifier
/// did not turn into a store intent ("store this", "save for
/// later") — those start the guided conversation.
fn has_store_verb(text: &str) -> bool {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .any(|word| matches!(word, "store" | "save" | "add" | "remember"))
}

pub struct ChatResponse {
    pub message: String,
    pub action: Option<Action>,
    pub user_message_display: Option<String>,
    /// Lettered choices accompanying the message (the guided
    /// conversation's kind list, a yes/no confirmation, ...). The UI
    /// renders them as tappable options; empty for plain replies.
    pub options: Vec<String>,
    /// The reply to this message is a secret (a master password, a
    /// secret's sensitive field): the UI hides the typing — each
    /// character shows for two seconds, then turns into a mask.
    pub input_sensitive: bool,
}

/// A response carrying only a message.
fn plain_response(message: &str) -> ChatResponse {
    ChatResponse {
        message: message.to_string(),
        action: None,
        user_message_display: None,
        options: Vec::new(),
        input_sensitive: false,
    }
}

/// A prompt whose answer is a secret: the UI hides the typing.
fn sensitive_prompt(message: &str) -> ChatResponse {
    ChatResponse {
        message: message.to_string(),
        action: None,
        user_message_display: None,
        options: Vec::new(),
        input_sensitive: true,
    }
}

pub enum Action {
    StoreEntry {
        name: String,
        kind: String,
        fields: Vec<(String, String)>,
    },
    SearchEntry {
        query: String,
        /// The field the user asked for, when they named one.
        field: Option<String>,
    },
    RevealEntry {
        entry_id: String,
        /// The field the user asked for, when they named one.
        field: Option<String>,
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
    /// Create the vault with this master password (the setup gate
    /// collected and confirmed it). Executed by the app layer,
    /// which then reports completion back to the handler.
    SetupVault {
        password: Zeroizing<String>,
    },
    /// Unlock the vault with this master password (the unlock
    /// gate collected it). Executed by the app layer with its
    /// lockout protection.
    UnlockVaultWith {
        password: Zeroizing<String>,
    },
}

/// Actions carry secrets — the master passwords, and every field
/// value of an entry being stored or updated — so `Debug` is
/// written by hand: it shows the shape and the field KEYS, never
/// a value. A derived impl would print the secrets into logs,
/// panic messages, and test output.
impl std::fmt::Debug for Action {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        fn keys(fields: &[(String, String)]) -> Vec<&str> {
            fields.iter().map(|(key, _)| key.as_str()).collect()
        }
        match self {
            Self::StoreEntry { name, kind, fields } => f
                .debug_struct("StoreEntry")
                .field("name", name)
                .field("kind", kind)
                .field("field_keys", &keys(fields))
                .finish(),
            Self::SearchEntry { query, field } => f
                .debug_struct("SearchEntry")
                .field("query", query)
                .field("field", field)
                .finish(),
            Self::RevealEntry { entry_id, field } => f
                .debug_struct("RevealEntry")
                .field("entry_id", entry_id)
                .field("field", field)
                .finish(),
            Self::ListEntries { category } => f
                .debug_struct("ListEntries")
                .field("category", category)
                .finish(),
            Self::ConfirmDelete { name } => {
                f.debug_struct("ConfirmDelete").field("name", name).finish()
            }
            Self::UpdateEntry { name, fields } => f
                .debug_struct("UpdateEntry")
                .field("name", name)
                .field("field_keys", &keys(fields))
                .finish(),
            Self::ExportVault => f.write_str("ExportVault"),
            Self::ImportVault => f.write_str("ImportVault"),
            Self::CreateVault => f.write_str("CreateVault"),
            Self::UnlockVault => f.write_str("UnlockVault"),
            Self::SetupVault { .. } => f
                .debug_struct("SetupVault")
                .field("password", &"••••")
                .finish(),
            Self::UnlockVaultWith { .. } => f
                .debug_struct("UnlockVaultWith")
                .field("password", &"••••")
                .finish(),
        }
    }
}
