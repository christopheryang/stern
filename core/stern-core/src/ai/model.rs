use crate::domain::vault::errors::VaultError;
use ort::session::{builder::GraphOptimizationLevel, Session};
use ort::value::Tensor;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

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

pub struct AiModel {
    session: Arc<Mutex<Session>>,
}

impl AiModel {
    pub fn new(runtime_path: &Path, model_path: &Path) -> Result<Self, VaultError> {
        ensure_ort_initialized(runtime_path)?;

        let session = Session::builder()
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
