use crate::ai::intent::Intent;
use crate::ai::model_manager::PromptStyle;
use crate::domain::vault::errors::VaultError;
use ort::session::{builder::GraphOptimizationLevel, Session};
use ort::value::{DynTensor, DynValue, Shape, Tensor, TensorElementType, ValueType};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};
use tokenizers::Tokenizer;

/// One-time ONNX Runtime initialization. The runtime library is not
/// linked into the app (ort is built with `load-dynamic`); it is
/// downloaded on demand and loaded from `runtime_path` here. Only a
/// *successful* initialization is cached, exactly once per process —
/// the first model load wins, and a runtime upgrade takes effect on
/// the next app start. A failure is NOT cached: if loading fails
/// because the runtime file is missing or corrupt and the runtime is
/// later repaired or re-downloaded, the next model load retries
/// instead of replaying the first error until the app restarts.
/// `ORT_INIT_LOCK` serializes initialization attempts so two
/// concurrent loads cannot race `ort::init_from`.
static ORT_INIT: OnceLock<()> = OnceLock::new();
static ORT_INIT_LOCK: Mutex<()> = Mutex::new(());

fn ensure_ort_initialized(runtime_path: &Path) -> Result<(), VaultError> {
    if ORT_INIT.get().is_some() {
        return Ok(());
    }
    let _guard = ORT_INIT_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if ORT_INIT.get().is_some() {
        return Ok(());
    }
    // Never hand `ort::init_from` a path that is not there. In ort
    // rc.12's `load-dynamic` build, a failed library load does not
    // return an error: building the error re-enters the API setup
    // that the in-progress load itself owns, and the call deadlocks
    // (found by the `failed_ort_init_is_not_cached` test hanging
    // inside `ort::setup_api`). Fail here instead, with our own
    // error — the missing-file case is also the realistic one
    // (runtime not downloaded yet, or deleted after install). A
    // file that exists but is corrupt still reaches `init_from`
    // and remains exposed to that ort bug; there is no way to
    // validate a dylib from outside it.
    if !runtime_path.is_file() {
        return Err(VaultError::Model(format!(
            "ONNX Runtime is not installed: no library at {}",
            runtime_path.display()
        )));
    }
    let committed = ort::init_from(runtime_path)
        .map(|builder| builder.with_name("stern-ai").commit())
        .map_err(|e| {
            VaultError::Model(format!(
                "ONNX Runtime failed to load from {}: {e}",
                runtime_path.display()
            ))
        })?;
    if !committed {
        return Err(VaultError::Model(format!(
            "ONNX Runtime failed to load from {}: the environment was not created",
            runtime_path.display()
        )));
    }
    // Holding the lock with the `get` check above makes a lost race
    // unreachable today; map it to success anyway — the environment
    // exists either way, which is all this function promises.
    match ORT_INIT.set(()) {
        Ok(()) | Err(()) => Ok(()),
    }
}

/// The most tokens one parse may generate. A command JSON is short;
/// the cap keeps a rambling model from stalling the chat.
const MAX_NEW_TOKENS: usize = 96;

/// A downloaded model, loaded and ready to parse chat messages.
///
/// The model is a *parser*, never an author: it converts one user
/// message into one structured [`Intent`] (via [`AiModel::parse_command`]),
/// and every word the user reads is produced by Stern's own
/// templates. The decoder drives the standard Hugging Face merged
/// export layout (`input_ids` / `attention_mask` / `position_ids` /
/// `past_key_values.*` in, `logits` / `present.*` out), reading the
/// layer count and KV-cache shape from the session metadata so all
/// four catalog models run through the same code.
pub struct AiModel {
    session: Arc<Mutex<Session>>,
    tokenizer: Tokenizer,
    prompt_style: PromptStyle,
    layers: usize,
    kv_heads: i64,
    kv_head_dim: i64,
    kv_element_type: TensorElementType,
    has_position_ids: bool,
    eos_token_ids: Vec<i64>,
}

impl AiModel {
    /// Load a model, its tokenizer, and (once per process) the ONNX
    /// Runtime library.
    pub fn new(
        runtime_path: &Path,
        model_path: &Path,
        tokenizer_path: &Path,
        prompt_style: PromptStyle,
    ) -> Result<Self, VaultError> {
        ensure_ort_initialized(runtime_path)?;

        let tokenizer = Tokenizer::from_file(tokenizer_path).map_err(|e| {
            VaultError::Model(format!(
                "tokenizer {} failed to load: {e}",
                tokenizer_path.display()
            ))
        })?;

        let session = Session::builder()
            .map_err(|e| VaultError::KeyDerivationFailed(format!("builder: {e}")))?
            // CPU only, deliberately: parsing must behave identically
            // on every machine. ort's session builder defaults the EP
            // selection policy to MaxEfficiency, which on macOS hands
            // the plain SmolLM int8 graph to the CoreML provider —
            // and CoreML rejects the zero-length KV cache a decoder
            // starts with (found by the parse smoke test). Appending
            // the CPU provider alone does not override that policy;
            // `with_auto_device(PreferCPU)` replaces it. These models
            // are small; the CPU is fast enough.
            .with_execution_providers([
                ort::execution_providers::CPUExecutionProvider::default().build()
            ])
            .map_err(|e| VaultError::KeyDerivationFailed(format!("execution providers: {e}")))?
            .with_auto_device(ort::session::builder::AutoDevicePolicy::PreferCPU)
            .map_err(|e| VaultError::KeyDerivationFailed(format!("device policy: {e}")))?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|e| VaultError::KeyDerivationFailed(format!("opt level: {e}")))?
            .with_intra_threads(2)
            .map_err(|e| VaultError::KeyDerivationFailed(format!("threads: {e}")))?
            .commit_from_file(model_path)
            .map_err(|e| VaultError::KeyDerivationFailed(format!("model load: {e}")))?;

        let mut layers = 0usize;
        let mut kv_heads = 0i64;
        let mut kv_head_dim = 0i64;
        let mut kv_element_type = TensorElementType::Float32;
        let mut has_position_ids = false;
        for input in session.inputs() {
            if input.name() == "position_ids" {
                has_position_ids = true;
            }
            if let Some(rest) = input.name().strip_prefix("past_key_values.") {
                if let ValueType::Tensor { ty, shape, .. } = input.dtype() {
                    if rest == "0.key" {
                        kv_element_type = *ty;
                        kv_heads = shape_dim(shape, 1);
                        kv_head_dim = shape_dim(shape, 3);
                    }
                    if rest.rsplit_once('.').is_some_and(|(_, kind)| kind == "key") {
                        layers = layers.saturating_add(1);
                    }
                }
            }
        }
        if layers == 0 || kv_heads <= 0 || kv_head_dim <= 0 {
            return Err(VaultError::Model(
                "model has no usable KV-cache inputs; not a supported decoder export".to_string(),
            ));
        }

        let mut eos_token_ids: Vec<i64> = Vec::new();
        for token in ["<|im_end|>", "<|end_of_text|>", "<|endoftext|>", "</s>"] {
            if let Some(id) = tokenizer.token_to_id(token) {
                let id = i64::from(id);
                if !eos_token_ids.contains(&id) {
                    eos_token_ids.push(id);
                }
            }
        }

        Ok(Self {
            session: Arc::new(Mutex::new(session)),
            tokenizer,
            prompt_style,
            layers,
            kv_heads,
            kv_head_dim,
            kv_element_type,
            has_position_ids,
            eos_token_ids,
        })
    }

    #[must_use]
    pub const fn is_available(&self) -> bool {
        true
    }

    /// Parse one user message into a structured intent.
    ///
    /// `Ok(None)` means the model produced nothing usable — the
    /// caller falls back to the rule classifier. An `Err` is a
    /// mechanical failure (tokenizer or runtime); the caller falls
    /// back the same way.
    pub fn parse_command(&self, user_input: &str) -> Result<Option<Intent>, VaultError> {
        let prompt = match self.prompt_style {
            PromptStyle::ChatMl => chatml_prompt(user_input),
            PromptStyle::FewShot => few_shot_prompt(user_input),
        };
        let raw = self.generate(&prompt)?;
        tracing::debug!("model raw output for {user_input:?}: {raw:?}");
        Ok(parse_command_output(&raw, user_input))
    }

    /// An empty KV cache: one zero-length tensor per layer and kind.
    /// Allocated through the session allocator — `Tensor::from_array`
    /// rejects a zero-sized dimension, but a decoder's first step
    /// needs exactly that.
    fn empty_past(&self, allocator: &ort::memory::Allocator) -> Result<Vec<DynValue>, VaultError> {
        let shape: Vec<i64> = vec![1, self.kv_heads, 0, self.kv_head_dim];
        let mut past = Vec::new();
        for _ in 0..self.layers.saturating_mul(2) {
            let tensor = DynTensor::new(allocator, self.kv_element_type, shape.clone())
                .map_err(|e| VaultError::Model(format!("empty KV tensor: {e}")))?;
            past.push(tensor.into_dyn());
        }
        Ok(past)
    }

    /// Run one decoder step: feed `new_ids` (the whole prompt on the
    /// first step, one token after) plus the KV cache, and return
    /// the greedy next token and the updated cache.
    #[allow(clippy::significant_drop_tightening)]
    fn run_step(
        &self,
        session: &mut Session,
        new_ids: &[i64],
        past_len: usize,
        past: Vec<DynValue>,
    ) -> Result<(i64, Vec<DynValue>), VaultError> {
        let seq_len = new_ids.len();
        let total_len = past_len.saturating_add(seq_len);
        let input_ids = Tensor::from_array((vec![1, seq_len_i64(seq_len)], new_ids.to_vec()))
            .map_err(|e| VaultError::Model(format!("input_ids tensor: {e}")))?;
        let attention_mask =
            Tensor::from_array((vec![1, seq_len_i64(total_len)], vec![1i64; total_len]))
                .map_err(|e| VaultError::Model(format!("attention_mask tensor: {e}")))?;

        let mut inputs = ort::inputs![
            "input_ids" => input_ids,
            "attention_mask" => attention_mask,
        ];
        if self.has_position_ids {
            let positions: Vec<i64> = (0..seq_len)
                .map(|offset| seq_len_i64(past_len.saturating_add(offset)))
                .collect();
            let position_ids = Tensor::from_array((vec![1, seq_len_i64(seq_len)], positions))
                .map_err(|e| VaultError::Model(format!("position_ids tensor: {e}")))?;
            inputs.push(("position_ids".into(), position_ids.into()));
        }
        let mut past_iter = past.into_iter();
        for layer in 0..self.layers {
            for kind in ["key", "value"] {
                let value = past_iter.next().ok_or_else(|| {
                    VaultError::Model("KV cache shorter than the layer count".to_string())
                })?;
                inputs.push((
                    format!("past_key_values.{layer}.{kind}").into(),
                    value.into(),
                ));
            }
        }

        let mut outputs = session
            .run(inputs)
            .map_err(|e| VaultError::Model(format!("model run failed: {e}")))?;

        let logits = outputs
            .remove("logits")
            .ok_or_else(|| VaultError::Model("model produced no logits".to_string()))?;
        let (_shape, data) = logits
            .try_extract_tensor::<f32>()
            .map_err(|e| VaultError::Model(format!("logits extract: {e}")))?;
        let vocab = data.len().checked_div(seq_len).unwrap_or(0).max(1);
        let last = data
            .chunks_exact(vocab)
            .last()
            .ok_or_else(|| VaultError::Model("logits are empty".to_string()))?;
        let (best_index, _) = last
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.total_cmp(b))
            .ok_or_else(|| VaultError::Model("logits row is empty".to_string()))?;
        let token = i64::try_from(best_index)
            .map_err(|_| VaultError::Model("token id out of range".to_string()))?;

        let mut new_past = Vec::new();
        for layer in 0..self.layers {
            for kind in ["key", "value"] {
                let value = outputs
                    .remove(format!("present.{layer}.{kind}"))
                    .ok_or_else(|| {
                        VaultError::Model(format!("model produced no present.{layer}.{kind}"))
                    })?;
                new_past.push(value);
            }
        }
        Ok((token, new_past))
    }

    /// Generate the model's reply to `prompt`, greedily, up to
    /// [`MAX_NEW_TOKENS`] tokens or an end-of-turn token. The session
    /// lock is held for the whole generation on purpose: the session
    /// is shared state, and the lock is what serializes parses.
    #[allow(clippy::significant_drop_tightening)]
    fn generate(&self, prompt: &str) -> Result<String, VaultError> {
        let encoding = self
            .tokenizer
            .encode(prompt, false)
            .map_err(|e| VaultError::Model(format!("tokenize prompt: {e}")))?;
        let prompt_ids: Vec<i64> = encoding.get_ids().iter().map(|id| i64::from(*id)).collect();
        if prompt_ids.is_empty() {
            return Err(VaultError::Model("prompt tokenized to nothing".to_string()));
        }

        let mut session = self
            .session
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let empty = self.empty_past(session.allocator())?;
        let (mut token, mut past) = self.run_step(&mut session, &prompt_ids, 0, empty)?;
        let mut past_len = prompt_ids.len();
        let mut generated: Vec<i64> = Vec::new();
        loop {
            if self.eos_token_ids.contains(&token) {
                break;
            }
            generated.push(token);
            if generated.len() >= MAX_NEW_TOKENS {
                break;
            }
            let (next, new_past) = self.run_step(&mut session, &[token], past_len, past)?;
            past = new_past;
            past_len = past_len.saturating_add(1);
            token = next;
        }

        let ids: Vec<u32> = generated
            .iter()
            .filter_map(|id| u32::try_from(*id).ok())
            .collect();
        self.tokenizer
            .decode(&ids, true)
            .map_err(|e| VaultError::Model(format!("decode output: {e}")))
    }
}

/// A shape dimension as `i64`, or 0 when absent/symbolic.
fn shape_dim(shape: &Shape, index: usize) -> i64 {
    shape.get(index).copied().unwrap_or(0)
}

/// A `usize` length as `i64` (token counts never approach the
/// limit; saturate rather than wrap if one ever did).
fn seq_len_i64(len: usize) -> i64 {
    i64::try_from(len).unwrap_or(i64::MAX)
}

/// Worked examples shared by both prompt styles. Small models
/// follow demonstrations far better than an abstract spec — with a
/// spec-only prompt the 135M models rambled or echoed the spec, and
/// even Qwen copied the kind enumeration literally as a value.
/// These six cover the actions a parse must distinguish.
const PARSE_EXAMPLES: [(&str, &str); 6] = [
    (
        "save my GitHub password is hunter2",
        "{\"action\":\"store\",\"kind\":\"login\",\"name\":\"GitHub\",\"fields\":{\"password\":\"hunter2\"}}",
    ),
    (
        "what's my password for google?",
        "{\"action\":\"retrieve\",\"name\":\"google\",\"field\":\"password\"}",
    ),
    (
        "what is the username for my chase account?",
        "{\"action\":\"retrieve\",\"name\":\"chase\",\"field\":\"username\"}",
    ),
    ("list all my secrets", "{\"action\":\"list\"}"),
    (
        "delete my old netflix entry",
        "{\"action\":\"delete\",\"name\":\"netflix\"}",
    ),
    ("hello", "{\"action\":\"greeting\"}"),
];

/// Wrap the parse request as a `ChatML` conversation: a one-line
/// system prompt, the worked examples as prior turns, then the
/// user's message.
fn chatml_prompt(user_input: &str) -> String {
    let mut prompt = String::from(
        "<|im_start|>system\nYou are a JSON command parser for a password vault. Output exactly one JSON object.<|im_end|>\n",
    );
    for (example_user, example_reply) in PARSE_EXAMPLES {
        prompt.push_str("<|im_start|>user\n");
        prompt.push_str(example_user);
        prompt.push_str("<|im_end|>\n<|im_start|>assistant\n");
        prompt.push_str(example_reply);
        prompt.push_str("<|im_end|>\n");
    }
    prompt.push_str("<|im_start|>user\n");
    prompt.push_str(user_input);
    prompt.push_str("<|im_end|>\n<|im_start|>assistant\n");
    prompt
}

/// The parse request as a plain few-shot completion, for the base
/// model with no instruction tuning: the same worked examples as
/// the `ChatML` prompt, in `Message:` / `Command:` form.
fn few_shot_prompt(user_input: &str) -> String {
    let mut prompt =
        String::from("Convert each message into one JSON command for a secrets manager.\n");
    for (example_user, example_reply) in PARSE_EXAMPLES {
        prompt.push_str("\nMessage: ");
        prompt.push_str(example_user);
        prompt.push_str("\nCommand: ");
        prompt.push_str(example_reply);
        prompt.push('\n');
    }
    prompt.push_str("\nMessage: ");
    prompt.push_str(user_input);
    prompt.push_str("\nCommand:");
    prompt
}

/// The model's JSON reply, deserialized. Every field but `action`
/// is optional; validation happens in [`ParsedCommand::into_intent`].
#[derive(Debug, Deserialize)]
struct ParsedCommand {
    action: String,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    field: Option<String>,
    #[serde(default)]
    fields: BTreeMap<String, String>,
    #[serde(default)]
    category: Option<String>,
    /// A single field's value, when the model answers with
    /// `field`/`value` instead of a `fields` object.
    #[serde(default)]
    value: Option<String>,
}

impl ParsedCommand {
    /// Convert to an intent, or `None` when the command is invalid
    /// or incomplete in a way the guided conversation cannot fix
    /// (the caller then falls back to the rule classifier).
    fn into_intent(self, original: &str) -> Option<Intent> {
        let name = self.name.filter(|n| !n.trim().is_empty());
        let mut fields: Vec<(String, String)> = self.fields.into_iter().collect();
        // Repair a common slip: a single field given as field/value
        // instead of a fields object.
        if fields.is_empty() {
            if let (Some(field), Some(value)) = (&self.field, &self.value) {
                fields.push((field.clone(), value.clone()));
            }
        }
        // Small models often answer with the user's own verb, or say
        // "greeting" while still extracting a name and field (which
        // is a retrieval, whatever they called it).
        let action = match self.action.as_str() {
            "greet" => "greeting",
            "save" | "remember" | "add" => "store",
            "show" => "list",
            "get" | "find" | "fetch" => "retrieve",
            "remove" => "delete",
            "greeting" if self.field.is_some() && name.is_some() => "retrieve",
            other => other,
        };
        match action {
            "store" => {
                let kind = self.kind?;
                if !matches!(
                    kind.as_str(),
                    "login" | "text" | "document" | "card" | "contact"
                ) {
                    return None;
                }
                Some(Intent::StoreSecret {
                    name: name.unwrap_or_default(),
                    kind,
                    fields,
                })
            }
            "retrieve" => Some(Intent::RetrieveSecret {
                name: name?,
                field: self.field,
            }),
            "list" => Some(Intent::ListSecrets {
                category: self.category,
            }),
            "delete" => Some(Intent::DeleteSecret { name: name? }),
            "update" => Some(Intent::UpdateSecret {
                name: name?,
                fields,
            }),
            "help" => Some(Intent::Help),
            "greeting" => Some(Intent::Greeting),
            "unknown" => Some(Intent::Unknown {
                text: original.to_string(),
            }),
            _ => None,
        }
    }
}

/// Extract the first complete JSON object from raw model output:
/// from the first `{` to its matching `}`, respecting strings and
/// escapes. Anything around the object is ignored.
fn extract_json_object(raw: &str) -> Option<&str> {
    let start = raw.find('{')?;
    let bytes = raw.as_bytes();
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for (offset, &byte) in bytes.iter().enumerate().skip(start) {
        if in_string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
            continue;
        }
        match byte {
            b'"' => in_string = true,
            b'{' => depth = depth.saturating_add(1),
            b'}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return raw.get(start..=offset);
                }
            }
            _ => {}
        }
    }
    None
}

/// Parse raw model output into an intent (see [`AiModel::parse_command`]).
fn parse_command_output(raw: &str, original: &str) -> Option<Intent> {
    let json = extract_json_object(raw)?;
    let parsed: ParsedCommand = serde_json::from_str(json).ok()?;
    parsed.into_intent(original)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    /// Regression test for the PR #18 review: a failed `init_from`
    /// must not be cached for the process lifetime. Two loads with
    /// two different bad runtime paths must each report their own
    /// path — if the first error were cached, the second call would
    /// replay it and name the first path.
    #[test]
    fn failed_ort_init_is_not_cached() {
        let first = ensure_ort_initialized(Path::new("/nonexistent/first/libonnxruntime.dylib"))
            .unwrap_err()
            .to_string();
        assert!(first.contains("first"), "unexpected first error: {first}");
        let second = ensure_ort_initialized(Path::new("/nonexistent/second/libonnxruntime.dylib"))
            .unwrap_err()
            .to_string();
        assert!(
            second.contains("second"),
            "init error was cached instead of retried: {second}"
        );
    }

    #[test]
    fn extract_json_object_finds_the_object_in_prose() {
        let raw = "Sure! Here is the command: {\"action\":\"list\"} hope that helps";
        assert_eq!(extract_json_object(raw), Some("{\"action\":\"list\"}"));
    }

    #[test]
    fn extract_json_object_handles_braces_in_strings() {
        let raw = "{\"action\":\"store\",\"name\":\"a}b\"} trailing }";
        assert_eq!(
            extract_json_object(raw),
            Some("{\"action\":\"store\",\"name\":\"a}b\"}")
        );
    }

    #[test]
    fn extract_json_object_rejects_unbalanced_output() {
        assert_eq!(extract_json_object("{\"action\":\"list\""), None);
        assert_eq!(extract_json_object("no json at all"), None);
    }

    #[test]
    fn parse_output_store_with_fields() {
        let intent = parse_command_output(
            "{\"action\":\"store\",\"kind\":\"login\",\"name\":\"GitHub\",\"fields\":{\"password\":\"hunter2\"}}",
            "save my GitHub password is hunter2",
        );
        match intent {
            Some(Intent::StoreSecret { name, kind, fields }) => {
                assert_eq!(name, "GitHub");
                assert_eq!(kind, "login");
                assert_eq!(
                    fields,
                    vec![("password".to_string(), "hunter2".to_string())]
                );
            }
            other => panic!("expected StoreSecret, got: {other:?}"),
        }
    }

    #[test]
    fn parse_output_retrieve_with_field() {
        let intent = parse_command_output(
            "{\"action\":\"retrieve\",\"name\":\"google\",\"field\":\"password\"}",
            "what's my password for google?",
        );
        match intent {
            Some(Intent::RetrieveSecret { name, field }) => {
                assert_eq!(name, "google");
                assert_eq!(field.as_deref(), Some("password"));
            }
            other => panic!("expected RetrieveSecret, got: {other:?}"),
        }
    }

    #[test]
    fn parse_output_rejects_bad_kind_and_unknown_action() {
        assert!(parse_command_output(
            "{\"action\":\"store\",\"kind\":\"crypto\",\"name\":\"x\"}",
            "store x"
        )
        .is_none());
        assert!(parse_command_output("{\"action\":\"explode\"}", "boom").is_none());
        assert!(parse_command_output("not json", "hi").is_none());
    }

    #[test]
    fn parse_output_store_without_name_still_parses() {
        // An empty name is fine: the guided conversation asks.
        let intent = parse_command_output(
            "{\"action\":\"store\",\"kind\":\"login\",\"fields\":{\"username\":\"chris\"}}",
            "store my username chris",
        );
        assert!(matches!(intent, Some(Intent::StoreSecret { name, .. }) if name.is_empty()));
    }
}
