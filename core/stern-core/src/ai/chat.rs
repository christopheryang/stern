use crate::ai::conversation::{
    has_own_screen, is_placeholder_name, is_sensitive_field, kind_word, Conversation,
    Step as ConversationStep,
};
use crate::ai::intent::{
    classify_folder_intent, classify_intent, classify_trash_intent, Chatter, Intent,
};
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

/// A delete waiting for the user to confirm it.
struct PendingDelete {
    entry_id: String,
    name: String,
}

/// The entry id a pending confirmation uses to mean "empty the trash".
pub const EMPTY_TRASH_MARKER: &str = "\u{0}empty-trash";

/// The two answers offered with a delete confirmation.
pub const DELETE_YES: &str = "Yes, delete it";
pub const DELETE_NO: &str = "No, keep it";

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
    /// An entry waiting for the user's yes or no before it is deleted.
    pending_delete: Mutex<Option<PendingDelete>>,
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
    // A space is not a symbol: the rule is about punctuation and the like.
    if !password
        .chars()
        .any(|c| !c.is_alphanumeric() && !c.is_whitespace())
    {
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
            pending_delete: Mutex::new(None),
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

    /// Ask the user to confirm deleting this entry. The next message is
    /// the answer: a yes deletes it, a no keeps it, and anything else
    /// means the user moved on, so nothing is deleted.
    pub fn set_pending_delete(&self, entry_id: &str, name: &str) {
        let mut guard = self
            .pending_delete
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *guard = Some(PendingDelete {
            entry_id: entry_id.to_string(),
            name: name.to_string(),
        });
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

        if let Some(response) = self.answer_pending_delete(user_input) {
            return response;
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
                ConversationStep::NoteEditor => Self::note_editor_response(),
                ConversationStep::FilePicker => Self::file_picker_response(),
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

    /// The reply to a yes or no that a delete confirmation is waiting for,
    /// or `None` when no delete is waiting or the user moved on (in which
    /// case nothing is deleted and the message is handled normally).
    fn answer_pending_delete(&self, user_input: &str) -> Option<ChatResponse> {
        let delete = {
            let mut guard = self
                .pending_delete
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            guard.take()
        }?;
        if parse_yes_no(user_input)? {
            let action = if delete.entry_id == EMPTY_TRASH_MARKER {
                Action::EmptyTrash
            } else {
                Action::DeleteEntry {
                    entry_id: delete.entry_id,
                    name: delete.name,
                }
            };
            Some(Self::vault_action(String::new(), action))
        } else {
            Some(plain_response(&format!(
                "Okay, \"{}\" is kept.",
                delete.name
            )))
        }
    }

    /// Process a message whose intent a parser (the AI model) has already
    /// produced. Everything after parsing is the same as for
    /// [`ChatHandler::process_message`], so the model's output is held to
    /// the same rules as the classifier's.
    #[must_use]
    pub fn process_parsed(&self, user_input: &str, intent: Intent) -> ChatResponse {
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

        // A lookup is read from the user's own words, not from the name the
        // classifier cut out of them, so topics and times count too. A
        // message nothing understood gets a search before a shrug, but only
        // when the vault is open: gibberish must never ask for a password.
        let ready = {
            let state = self.vault_state();
            state.exists && state.unlocked
        };
        let intent = match intent {
            Intent::RetrieveSecret { field, .. } => Intent::Find {
                text: user_input.to_string(),
                field,
                fallback: false,
            },
            Intent::Unknown { .. }
                if ready
                    && !has_store_verb(user_input)
                    && !crate::ai::query::query_from_text(user_input)
                        .terms
                        .is_empty() =>
            {
                Intent::Find {
                    text: user_input.to_string(),
                    field: None,
                    fallback: true,
                }
            }
            other => other,
        };

        // The guided conversation starts where a parsed store has
        // nothing to file under: a content-free store request gets
        // the full interview — kind first, unless the request already
        // named one ("save a note") — and a nameless parse — a value
        // with nothing to call it — gets asked for the missing name
        // and fields. A store the parser named executes immediately,
        // exactly as it always has.
        let guided = match &intent {
            // A store with nothing to keep is a request to start one, not
            // something to save. This holds whatever name the parser gave
            // it: a model answers "store a document" with the name
            // "document", and filing that would save an empty entry. A
            // note or document is filled in on its own screen, so a name
            // does not skip it; any other kind keeps a name that someone
            // really chose and asks for the rest.
            Intent::StoreSecret { name, kind, fields } if fields.is_empty() => {
                if is_placeholder_name(name, kind) || has_own_screen(kind) {
                    Some(Conversation::start_for_kind(kind))
                } else {
                    Conversation::prefilled(kind, name, fields)
                        .or_else(|| Some(Conversation::start_for_kind(kind)))
                }
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
                    | Intent::Find { .. }
                    | Intent::ListSecrets { .. }
                    | Intent::DeleteSecret { .. }
                    | Intent::UpdateSecret { .. }
                    | Intent::CreateFolder { .. }
                    | Intent::MoveEntry { .. }
                    | Intent::ShowFolders { .. }
                    | Intent::Rename { .. }
                    | Intent::DeleteFolder { .. }
                    | Intent::Restore { .. }
                    | Intent::ShowTrash
                    | Intent::EmptyTrash
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
                ConversationStep::NoteEditor => Self::note_editor_response(),
                ConversationStep::FilePicker => Self::file_picker_response(),
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

    /// What Stern says when it does not understand a message and a search
    /// found nothing either.
    #[must_use]
    pub fn not_sure_reply(text: &str) -> String {
        format!(
            "I'm not sure what you mean by \"{text}\".\n\n\
            Here are some things I can do:\n\
            - \"save a note\" or \"save a document\"\n\
            - \"find my shopping list\"\n\
            - \"show my secrets\" to see what you have\n\
            - \"show my folders\" to see how it is organised\n\
            - \"help\" for everything else"
        )
    }

    /// A reply that carries an action for the app layer to carry out. The
    /// app layer writes the message, since it is the one that knows what
    /// happened.
    const fn vault_action(message: String, action: Action) -> ChatResponse {
        ChatResponse {
            message,
            action: Some(action),
            user_message_display: None,
            options: Vec::new(),
            input_sensitive: false,
        }
    }

    /// The reply that shows the file picker for a document.
    fn file_picker_response() -> ChatResponse {
        ChatResponse {
            message: "Choose a plain text file from this computer to keep in your vault, \
                like .txt, .md, .html or .csv. The limit is 1 MB."
                .to_string(),
            action: Some(Action::OpenFilePicker),
            user_message_display: None,
            options: Vec::new(),
            input_sensitive: false,
        }
    }

    /// The reply that opens the note editor on a new note.
    fn note_editor_response() -> ChatResponse {
        ChatResponse {
            message: "Write your note below, then tap Save.".to_string(),
            action: Some(Action::OpenNoteEditor),
            user_message_display: None,
            options: Vec::new(),
            input_sensitive: false,
        }
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
        // Folder requests are exact words ("move X to Y"), which the
        // rules read reliably and a small model does not know at all.
        if let Some(folder_intent) = classify_folder_intent(user_input) {
            return folder_intent;
        }
        if let Some(trash_intent) = classify_trash_intent(user_input) {
            return trash_intent;
        }
        match self.model_intent(user_input) {
            // A greeting, help or "unknown" answer is cheap to
            // double-check: small models sometimes say "greeting" for a
            // vault command (measured on SmolLM2-135M: "delete my old
            // netflix entry" came back as a greeting), or give up on a
            // phrasing the rules understand. If the rules see a vault
            // action or small talk in the same message, trust them.
            Some(intent @ (Intent::Greeting | Intent::Help | Intent::Unknown { .. })) => {
                match classify_intent(user_input) {
                    fallback @ (Intent::StoreSecret { .. }
                    | Intent::RetrieveSecret { .. }
                    | Intent::ListSecrets { .. }
                    | Intent::DeleteSecret { .. }
                    | Intent::UpdateSecret { .. }
                    | Intent::ExportVault
                    | Intent::ImportVault
                    | Intent::CreateVault
                    | Intent::UnlockVault
                    | Intent::Chatter(_)) => fallback,
                    Intent::Help if matches!(intent, Intent::Unknown { .. }) => Intent::Help,
                    _ => intent,
                }
            }
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
                // A stored secret is never echoed back: secret fields
                // show as a mask, everything else as typed.
                let field_desc = if fields.is_empty() {
                    String::new()
                } else {
                    let items: Vec<String> = fields
                        .iter()
                        .map(|(k, v)| {
                            if is_sensitive_field(kind, k) {
                                format!("  {k}: ••••")
                            } else {
                                format!("  {k}: {v}")
                            }
                        })
                        .collect();
                    format!("\n{}", items.join("\n"))
                };

                ChatResponse {
                    message: format!("Saved your {} \"{name}\".{field_desc}", kind_word(kind)),
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
                    fallback: false,
                }),
                user_message_display: None,
                options: Vec::new(),
                input_sensitive: false,
            },
            Intent::Find {
                text,
                field,
                fallback,
            } => ChatResponse {
                message: if fallback {
                    Self::not_sure_reply(&text)
                } else {
                    "Searching your vault...".to_string()
                },
                action: Some(Action::SearchEntry {
                    query: text,
                    field,
                    fallback,
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
            Intent::CreateFolder { name, parent } => {
                Self::vault_action(String::new(), Action::CreateFolder { name, parent })
            }
            Intent::MoveEntry { name, folder } => {
                Self::vault_action(String::new(), Action::MoveEntry { name, folder })
            }
            Intent::ShowFolders { folder } => {
                Self::vault_action(String::new(), Action::ShowFolders { folder })
            }
            Intent::Rename {
                name,
                new_name,
                folder,
            } => Self::vault_action(
                String::new(),
                Action::RenameItem {
                    name,
                    new_name,
                    folder,
                },
            ),
            Intent::DeleteFolder { name } => {
                Self::vault_action(String::new(), Action::DeleteFolder { name })
            }
            Intent::Restore { name } => Self::vault_action(String::new(), Action::Restore { name }),
            Intent::ShowTrash => Self::vault_action(String::new(), Action::ShowTrash),
            Intent::EmptyTrash => Self::vault_action(String::new(), Action::ConfirmEmptyTrash),
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
            Intent::Help => plain_response(HELP_REPLY),
            Intent::Greeting => plain_response(GREETING_REPLY),
            Intent::Chatter(chatter) => plain_response(chatter_reply(chatter)),
            Intent::Unknown { ref text } => plain_response(&Self::not_sure_reply(text)),
        }
    }
}

/// A yes or no answer, or `None` when the text is neither.
fn parse_yes_no(input: &str) -> Option<bool> {
    let lower = input.trim().trim_end_matches(['.', '!']).to_lowercase();
    if lower == DELETE_YES.to_lowercase()
        || matches!(
            lower.as_str(),
            "yes" | "y" | "yep" | "yeah" | "sure" | "ok" | "okay" | "confirm" | "delete" | "do it"
        )
    {
        return Some(true);
    }
    if lower == DELETE_NO.to_lowercase()
        || matches!(
            lower.as_str(),
            "no" | "n" | "nope" | "cancel" | "stop" | "keep" | "keep it" | "never mind"
        )
    {
        return Some(false);
    }
    None
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

/// What Stern says when asked for help or what it can do.
const HELP_REPLY: &str = "I keep your notes and documents safe, and everything \
stays on this computer. Here is what you can ask me:\n\n\
\u{1f4dd} Save something -- \"save a note\", \"save a document\", \"save my GitHub password\"\n\
\u{1f50d} Find it again -- \"find my shopping list\", \"the note about my credit card\", \"what I saved last March\"\n\
\u{1f4cb} See what you have -- \"show my secrets\" (names only, never the secrets themselves)\n\
\u{1f4c1} Organise -- \"create folder Taxes\", \"move Lease to Home\", \"show my folders\"\n\
\u{270f}\u{fe0f} Tidy up -- \"rename Netflix to Netflix Family\", \"delete my old Netflix entry\"\n\
\u{21a9}\u{fe0f} Changed your mind -- \"undo\" brings back what you deleted in the last 30 days\n\n\
Tip: for notes you want to keep forever, a plain text file (.txt) is best. Any \
computer can open it, with no special app.";

/// What Stern says to a greeting.
const GREETING_REPLY: &str = "Hello! I'm Stern. I keep your notes and documents private, and \
everything stays on this computer.\n\n\
I can save a note or a document, find it again, and keep your things in folders. \
Try \"save a note\", \"show my secrets\", or ask \"what can you do?\"";

/// What Stern says to small talk that is not a greeting.
const fn chatter_reply(chatter: Chatter) -> &'static str {
    match chatter {
        Chatter::Thanks => "You're welcome! Anything else you'd like to save or find?",
        Chatter::Goodbye => {
            "Goodbye! Your vault stays locked up safe. Use the padlock if you'd like to lock it now."
        }
        Chatter::About => {
            "I'm Stern, your private vault assistant. I keep notes, documents and passwords \
            on this computer, encrypted, and I can find, organise and tidy them for you. \
            Ask \"what can you do?\" for ideas."
        }
        Chatter::Acknowledge => {
            "Okay. Say \"save a note\", \"find <name>\" or \"show my secrets\" whenever you're ready."
        }
    }
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
    /// Open the note editor on a new, empty note.
    OpenNoteEditor,
    /// Show the file picker, to keep a file from this computer as a
    /// document.
    OpenFilePicker,
    StoreEntry {
        name: String,
        kind: String,
        fields: Vec<(String, String)>,
    },
    SearchEntry {
        /// The user's own words. The search reads names, topics and time out
        /// of them.
        query: String,
        /// The field the user asked for, when they named one.
        field: Option<String>,
        /// The message was not understood as anything else; if the search
        /// finds nothing, reply with [`ChatHandler::not_sure_reply`].
        fallback: bool,
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
    /// Make a folder (inside `parent`, or at the top level).
    CreateFolder {
        name: String,
        parent: Option<String>,
    },
    /// File an entry in a folder, or take it out of its folder.
    MoveEntry {
        name: String,
        folder: Option<String>,
    },
    /// Show the folder tree, or what is inside one folder.
    ShowFolders {
        folder: Option<String>,
    },
    /// Rename an entry, or a folder.
    RenameItem {
        name: String,
        new_name: String,
        folder: bool,
    },
    /// Delete a folder. What is inside it is kept.
    DeleteFolder {
        name: String,
    },
    /// Bring an entry back from the trash (the most recent when `name` is
    /// empty).
    Restore {
        name: String,
    },
    /// Show what is in the trash.
    ShowTrash,
    /// Ask before emptying the trash.
    ConfirmEmptyTrash,
    /// Delete everything in the trash for good, after the user said yes.
    EmptyTrash,
    /// Delete this entry, after the user said yes.
    DeleteEntry {
        entry_id: String,
        name: String,
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
            Self::SearchEntry {
                query,
                field,
                fallback,
            } => f
                .debug_struct("SearchEntry")
                .field("query", query)
                .field("field", field)
                .field("fallback", fallback)
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
            Self::CreateFolder { name, parent } => f
                .debug_struct("CreateFolder")
                .field("name", name)
                .field("parent", parent)
                .finish(),
            Self::MoveEntry { name, folder } => f
                .debug_struct("MoveEntry")
                .field("name", name)
                .field("folder", folder)
                .finish(),
            Self::ShowFolders { folder } => f
                .debug_struct("ShowFolders")
                .field("folder", folder)
                .finish(),
            Self::RenameItem {
                name,
                new_name,
                folder,
            } => f
                .debug_struct("RenameItem")
                .field("name", name)
                .field("new_name", new_name)
                .field("folder", folder)
                .finish(),
            Self::DeleteFolder { name } => {
                f.debug_struct("DeleteFolder").field("name", name).finish()
            }
            Self::DeleteEntry { entry_id, name } => f
                .debug_struct("DeleteEntry")
                .field("entry_id", entry_id)
                .field("name", name)
                .finish(),
            Self::Restore { name } => f.debug_struct("Restore").field("name", name).finish(),
            Self::ShowTrash => f.write_str("ShowTrash"),
            Self::ConfirmEmptyTrash => f.write_str("ConfirmEmptyTrash"),
            Self::EmptyTrash => f.write_str("EmptyTrash"),
            Self::OpenNoteEditor => f.write_str("OpenNoteEditor"),
            Self::OpenFilePicker => f.write_str("OpenFilePicker"),
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
