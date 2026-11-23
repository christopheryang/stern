use crate::domain::vault::errors::VaultError;
use ort::environment::Environment;
use ort::session::{builder::GraphOptimizationLevel, Session};
use ort::value::Tensor;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

/// One-time ONNX Runtime initialization. The runtime library is not
/// linked into the app (ort is built with `load-dynamic`); it is
/// downloaded on demand and loaded from `runtime_path` here, and
/// the `Environment` it creates is held for the process lifetime —
/// since ort rc.15, sessions are built against an explicit
/// environment instead of a global one. Only a *successful*
/// initialization is cached, exactly once per process — the first
/// model load wins, and a runtime upgrade takes effect on the next
/// app start. A failure is NOT cached: if loading fails because the
/// runtime file is missing or corrupt and the runtime is later
/// repaired or re-downloaded, the next model load retries instead
/// of replaying the first error until the app restarts.
/// `ORT_INIT_LOCK` serializes initialization attempts so two
/// concurrent loads cannot race `ort::init_from`.
static ORT_ENV: OnceLock<Environment> = OnceLock::new();
static ORT_INIT_LOCK: Mutex<()> = Mutex::new(());

fn ensure_ort_initialized(runtime_path: &Path) -> Result<(), VaultError> {
    if ORT_ENV.get().is_some() {
        return Ok(());
    }
    let _guard = ORT_INIT_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if ORT_ENV.get().is_some() {
        return Ok(());
    }
    // Fail on a missing runtime file with our own error, naming the
    // path: it is the realistic failure (runtime not downloaded
    // yet, or deleted after install) and deserves a clearer message
    // than the loader's. (In ort rc.12 this check was load-bearing:
    // a failed `init_from` deadlocked instead of returning an
    // error, because building the error re-entered the API setup
    // the in-progress load owned. rc.15 fixed that upstream — load
    // failures now return a plain `LoadError` — so a corrupt file
    // that reaches `init_from` simply errors.)
    if !runtime_path.is_file() {
        return Err(VaultError::Model(format!(
            "ONNX Runtime is not installed: no library at {}",
            runtime_path.display()
        )));
    }
    let environment = ort::init_from(runtime_path)
        .map_err(|e| {
            VaultError::Model(format!(
                "ONNX Runtime failed to load from {}: {e}",
                runtime_path.display()
            ))
        })?
        .with_name("stern-ai")
        .build()
        .map_err(|e| VaultError::Model(format!("ONNX Runtime environment failed to start: {e}")))?;
    // Holding the lock with the `get` check above makes a lost race
    // unreachable today; map it to success anyway — the environment
    // exists either way, which is all this function promises.
    match ORT_ENV.set(environment) {
        Ok(()) | Err(_) => Ok(()),
    }
}

/// The process-wide ONNX Runtime environment, once
/// `ensure_ort_initialized` has succeeded.
fn ort_environment() -> Result<&'static Environment, VaultError> {
    ORT_ENV
        .get()
        .ok_or_else(|| VaultError::Model("ONNX Runtime is not initialized".to_string()))
}

pub struct AiModel {
    session: Arc<Mutex<Session>>,
}

impl AiModel {
    pub fn new(runtime_path: &Path, model_path: &Path) -> Result<Self, VaultError> {
        ensure_ort_initialized(runtime_path)?;

        let session = Session::builder(ort_environment()?)
            .map_err(|e| VaultError::KeyDerivationFailed(format!("builder: {e}")))?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|e| VaultError::KeyDerivationFailed(format!("opt level: {e}")))?
            .with_intra_threads(2)
            .map_err(|e| VaultError::KeyDerivationFailed(format!("threads: {e}")))?
            .commit_from_file(model_path)
            .map_err(|e| VaultError::KeyDerivationFailed(format!("model load: {e}")))?;

        Ok(Self {
            session: Arc::new(Mutex::new(session)),
        })
    }

    #[must_use]
    pub const fn is_available(&self) -> bool {
        true
    }

    #[allow(clippy::significant_drop_tightening)]
    pub fn run_inference(
        &self,
        input_ids: &[i64],
        attention_mask: &[i64],
    ) -> Result<Vec<f32>, VaultError> {
        let input_ids_tensor =
            Tensor::from_array((vec![1usize, input_ids.len()], input_ids.to_vec()))
                .map_err(|e| VaultError::KeyDerivationFailed(format!("tensor: {e}")))?;

        let attention_mask_tensor =
            Tensor::from_array((vec![1usize, attention_mask.len()], attention_mask.to_vec()))
                .map_err(|e| VaultError::KeyDerivationFailed(format!("tensor: {e}")))?;

        let mut session = self
            .session
            .lock()
            .map_err(|e| VaultError::KeyDerivationFailed(format!("lock: {e}")))?;

        let _outputs = session
            .run(ort::inputs![
                "input_ids" => input_ids_tensor,
                "attention_mask" => attention_mask_tensor,
            ])
            .map_err(|e| VaultError::KeyDerivationFailed(format!("inference: {e}")))?;

        Ok(vec![])
    }
}

pub struct SimpleTokenizer {
    vocab: std::collections::HashMap<String, i64>,
    unk_token_id: i64,
}

impl SimpleTokenizer {
    #[must_use]
    #[allow(clippy::cast_possible_wrap)]
    pub fn new() -> Self {
        let mut vocab = std::collections::HashMap::new();

        let special_tokens = [
            "[PAD]",
            "[UNK]",
            "[CLS]",
            "[SEP]",
            "[MASK]",
            "store",
            "save",
            "add",
            "create",
            "new",
            "find",
            "get",
            "retrieve",
            "search",
            "show",
            "list",
            "display",
            "delete",
            "remove",
            "destroy",
            "password",
            "secret",
            "credential",
            "login",
            "api",
            "key",
            "token",
            "document",
            "file",
            "note",
            "text",
            "spreadsheet",
            "csv",
            "pdf",
            "update",
            "change",
            "modify",
            "edit",
            "help",
            "what",
            "how",
            "can",
            "my",
            "the",
            "a",
            "an",
            "for",
            "with",
            "to",
            "is",
            "github",
            "gmail",
            "netflix",
            "ssh",
            "wifi",
            "bank",
            "email",
            "username",
            "user",
            "pass",
            "url",
            "website",
            "server",
            "database",
        ];

        for (i, token) in special_tokens.iter().enumerate() {
            vocab.insert(token.to_string(), i as i64);
        }

        Self {
            vocab,
            unk_token_id: 1,
        }
    }

    #[must_use]
    pub fn encode(&self, text: &str) -> (Vec<i64>, Vec<i64>) {
        let tokens: Vec<i64> = text
            .to_lowercase()
            .split_whitespace()
            .map(|word| *self.vocab.get(word).unwrap_or(&self.unk_token_id))
            .collect();

        let attention_mask = vec![1i64; tokens.len()];
        (tokens, attention_mask)
    }
}

impl Default for SimpleTokenizer {
    fn default() -> Self {
        Self::new()
    }
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
}
