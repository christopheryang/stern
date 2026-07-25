use crate::domain::vault::errors::VaultError;
use ort::session::{Session, builder::GraphOptimizationLevel};
use ort::value::Tensor;
use std::path::Path;
use std::sync::{Arc, Mutex};

pub struct AiModel {
    session: Arc<Mutex<Session>>,
}

impl AiModel {
    pub fn new(model_path: &Path) -> Result<Self, VaultError> {
        let _env = ort::environment::init()
            .with_name("stern-ai");

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
        let input_ids_tensor = Tensor::from_array((
            vec![1usize, input_ids.len()],
            input_ids.to_vec(),
        ))
        .map_err(|e| VaultError::KeyDerivationFailed(format!("tensor: {e}")))?;

        let attention_mask_tensor = Tensor::from_array((
            vec![1usize, attention_mask.len()],
            attention_mask.to_vec(),
        ))
        .map_err(|e| VaultError::KeyDerivationFailed(format!("tensor: {e}")))?;

        let mut session = self.session
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
            "[PAD]", "[UNK]", "[CLS]", "[SEP]", "[MASK]",
            "store", "save", "add", "create", "new",
            "find", "get", "retrieve", "search", "show", "list", "display",
            "delete", "remove", "destroy",
            "password", "secret", "credential", "login", "api", "key", "token",
            "document", "file", "note", "text", "spreadsheet", "csv", "pdf",
            "update", "change", "modify", "edit",
            "help", "what", "how", "can",
            "my", "the", "a", "an", "for", "with", "to", "is",
            "github", "gmail", "netflix", "ssh", "wifi", "bank", "email",
            "username", "user", "pass", "url", "website", "server", "database",
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
            .map(|word| {
                *self
                    .vocab
                    .get(word)
                    .unwrap_or(&self.unk_token_id)
            })
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
