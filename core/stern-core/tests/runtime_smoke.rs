#![allow(clippy::unwrap_used)]
// Manual smoke test (ignored by default): downloads the real ONNX
// Runtime wheel from PyPI, verifies + extracts it, then proves the
// library dlopens by initializing ort from it.
use stern_core::ai::model_manager::ModelManager;
use stern_core::ai::AiModel;

#[test]
#[ignore = "network test: downloads ~21 MB from PyPI"]
fn ensure_runtime_downloads_and_dlopen_works() {
    let dir = tempfile::tempdir().unwrap();
    let manager = ModelManager::new(dir.path().join("models"));
    let path = manager.ensure_runtime(&mut |_, _| {}).unwrap();
    assert!(path.exists());
    // A bogus model path must fail at the *model* stage, not the
    // runtime stage: reaching the session error proves init_from
    // loaded the downloaded library.
    let msg = match AiModel::new(&path, std::path::Path::new("/nonexistent/model.onnx")) {
        Ok(_) => panic!("a nonexistent model must not load"),
        Err(e) => format!("{e}"),
    };
    assert!(
        !msg.contains("ONNX Runtime failed to load"),
        "runtime load failed: {msg}"
    );
    println!(
        "runtime ok at {}; model-stage error as expected: {msg}",
        path.display()
    );
}
