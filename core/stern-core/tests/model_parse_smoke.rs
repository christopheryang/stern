#![allow(clippy::unwrap_used)]
// Manual smoke test (ignored by default): downloads the real ONNX
// Runtime wheel (~21 MB), loads a real model + tokenizer from the
// paths in STERN_SMOKE_MODEL / STERN_SMOKE_TOKENIZER, and prints how
// the model parses sample chat messages.
use stern_core::ai::model_manager::{ModelManager, PromptStyle};
use stern_core::ai::AiModel;

#[test]
#[ignore = "network + local model files: sets STERN_SMOKE_MODEL and STERN_SMOKE_TOKENIZER"]
fn model_parses_sample_messages() {
    let model_path = std::env::var("STERN_SMOKE_MODEL").unwrap();
    let tokenizer_path = std::env::var("STERN_SMOKE_TOKENIZER").unwrap();

    let dir = tempfile::tempdir().unwrap();
    let manager = ModelManager::new(dir.path().join("models"));
    let runtime_path = manager.ensure_runtime(&mut |_, _| {}).unwrap();

    let style = match std::env::var("STERN_SMOKE_STYLE").as_deref() {
        Ok("fewshot") => PromptStyle::FewShot,
        _ => PromptStyle::ChatMl,
    };
    let load_start = std::time::Instant::now();
    let model = AiModel::new(
        &runtime_path,
        std::path::Path::new(&model_path),
        std::path::Path::new(&tokenizer_path),
        style,
    )
    .unwrap();
    println!("model loaded in {:?}", load_start.elapsed());

    for message in [
        "what's my password for google?",
        "save my GitHub password is hunter2",
        "list all my secrets",
        "hello",
        "store my bank login, username is chris and password is s3cret, the website is chase.com",
        "delete my old netflix entry",
        "what is the username for my chase account?",
        "remember my wifi password is bananas42",
        "show me everything",
        "what's my email for spotify?",
    ] {
        let start = std::time::Instant::now();
        let parsed = model.parse_command(message);
        println!("{message:?}\n  -> {parsed:?} in {:?}", start.elapsed());
    }
}
