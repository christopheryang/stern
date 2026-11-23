// Copyright (c) 2026 Christopher Yang
// SPDX-License-Identifier: MIT

#![allow(clippy::unwrap_used)]

use sha2::{Digest, Sha256};
use std::io::Cursor;
use stern_core::ai::model_manager::{
    version_is_newer, ModelDescriptor, ModelManager, ModelState, PromptStyle,
};

fn fixture_bytes() -> Vec<u8> {
    b"stern test model bytes".to_vec()
}

/// Build a descriptor whose SHA-256 matches `bytes`, as the real catalog
/// entries match their published model files.
fn descriptor_for(bytes: &[u8], version: &'static str) -> ModelDescriptor {
    descriptor_for_id("test-model", bytes, version)
}

fn descriptor_for_id(id: &'static str, bytes: &[u8], version: &'static str) -> ModelDescriptor {
    let digest = data_encoding::HEXLOWER.encode(&Sha256::digest(bytes));
    let sha: &'static str = Box::leak(digest.into_boxed_str());
    ModelDescriptor {
        id,
        display_name: "Test Model",
        description: "A model used by the test suite.",
        version,
        file_name: "model.onnx",
        download_url: "https://example.invalid/model.onnx",
        sha256: sha,
        size_bytes: u64::try_from(bytes.len()).unwrap_or(0),
        tokenizer_url: "https://example.invalid/tokenizer.json",
        tokenizer_sha256: sha,
        tokenizer_size_bytes: 0,
        prompt_style: PromptStyle::ChatMl,
    }
}

fn manager() -> (tempfile::TempDir, ModelManager) {
    let dir = tempfile::tempdir().unwrap();
    let manager = ModelManager::new(dir.path().join("models"));
    (dir, manager)
}

fn install(manager: &ModelManager, descriptor: &ModelDescriptor, bytes: &[u8]) {
    let mut reader = Cursor::new(bytes.to_vec());
    let total = u64::try_from(bytes.len()).unwrap_or(0);
    manager
        .install_from_reader(descriptor, &mut reader, total, &mut |_, _| {})
        .unwrap();
}

#[test]
fn status_not_installed_on_empty_dir() {
    let (_dir, manager) = manager();
    let descriptor = descriptor_for(&fixture_bytes(), "1.0.0");
    let status = manager.status_for(&descriptor);
    assert_eq!(status.state, ModelState::NotInstalled);
    assert!(status.installed.is_none());
    assert!(!status.is_active);
    assert!(manager.active_model_path().is_none());
}

#[test]
fn install_from_reader_success() {
    let (_dir, manager) = manager();
    let bytes = fixture_bytes();
    let descriptor = descriptor_for(&bytes, "1.0.0");

    let mut reader = Cursor::new(bytes.clone());
    let total = u64::try_from(bytes.len()).unwrap_or(0);
    let mut progress_log: Vec<(u64, u64)> = Vec::new();
    let installed = manager
        .install_from_reader(&descriptor, &mut reader, total, &mut |d, t| {
            progress_log.push((d, t));
        })
        .unwrap();

    assert_eq!(installed.model_id, "test-model");
    assert_eq!(installed.version, "1.0.0");
    assert_eq!(installed.size_bytes, total);
    assert_eq!(installed.sha256, descriptor.sha256);
    assert_eq!(progress_log.last().copied(), Some((total, total)));

    let status = manager.status_for(&descriptor);
    assert_eq!(
        status.state,
        ModelState::Installed {
            version: "1.0.0".to_string()
        }
    );
    // The first installed model becomes active automatically.
    assert!(status.is_active);

    let path = manager.active_model_path().unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
}

#[test]
fn install_rejects_wrong_hash_and_keeps_no_partial() {
    let (_dir, manager) = manager();
    let bytes = fixture_bytes();
    // Descriptor pinned to the hash of *different* bytes.
    let descriptor = descriptor_for(b"some other bytes", "1.0.0");

    let mut reader = Cursor::new(bytes);
    let result = manager.install_from_reader(&descriptor, &mut reader, 22, &mut |_, _| {});
    assert!(result.is_err());

    let status = manager.status_for(&descriptor);
    assert_eq!(status.state, ModelState::NotInstalled);
    assert!(manager.active_model_path().is_none());

    // The .part file must not survive a failed verification.
    let version_dir = manager.models_dir().join("test-model").join("1.0.0");
    let leftovers: Vec<_> = std::fs::read_dir(&version_dir)
        .unwrap()
        .filter_map(Result::ok)
        .collect();
    assert!(
        leftovers.is_empty(),
        "partial files left behind: {leftovers:?}"
    );
}

#[test]
fn update_available_when_catalog_is_newer() {
    let (_dir, manager) = manager();
    let bytes = fixture_bytes();
    install(&manager, &descriptor_for(&bytes, "1.0.0"), &bytes);

    let newer = descriptor_for(&bytes, "2.0.0");
    let status = manager.status_for(&newer);
    assert_eq!(
        status.state,
        ModelState::UpdateAvailable {
            installed_version: "1.0.0".to_string(),
            latest_version: "2.0.0".to_string(),
        }
    );
}

#[test]
fn installing_newer_version_prunes_old() {
    let (_dir, manager) = manager();
    let bytes = fixture_bytes();
    install(&manager, &descriptor_for(&bytes, "1.0.0"), &bytes);
    install(&manager, &descriptor_for(&bytes, "2.0.0"), &bytes);

    let old_dir = manager.models_dir().join("test-model").join("1.0.0");
    assert!(!old_dir.exists(), "old version dir should be pruned");

    let status = manager.status_for(&descriptor_for(&bytes, "2.0.0"));
    assert_eq!(
        status.state,
        ModelState::Installed {
            version: "2.0.0".to_string()
        }
    );
    let path = manager.active_model_path().unwrap();
    assert!(path.ends_with("2.0.0/model.onnx"), "active path: {path:?}");
}

#[test]
fn failed_update_keeps_previous_model() {
    let (_dir, manager) = manager();
    let bytes = fixture_bytes();
    install(&manager, &descriptor_for(&bytes, "1.0.0"), &bytes);

    // A v2 download whose bytes do not match its pinned hash must fail
    // without disturbing the working v1 install.
    let bad_v2 = descriptor_for(b"corrupted download", "2.0.0");
    let mut reader = Cursor::new(bytes.clone());
    let result = manager.install_from_reader(&bad_v2, &mut reader, 22, &mut |_, _| {});
    assert!(result.is_err());

    let status = manager.status_for(&descriptor_for(&bytes, "1.0.0"));
    assert_eq!(
        status.state,
        ModelState::Installed {
            version: "1.0.0".to_string()
        }
    );
    let path = manager.active_model_path().unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
}

#[test]
fn remove_model_removes_it() {
    let (_dir, manager) = manager();
    let bytes = fixture_bytes();
    let descriptor = descriptor_for(&bytes, "1.0.0");
    install(&manager, &descriptor, &bytes);

    manager.remove_model("test-model").unwrap();

    assert_eq!(
        manager.status_for(&descriptor).state,
        ModelState::NotInstalled
    );
    assert!(manager.active_model_path().is_none());
    assert!(manager.installed_model("test-model").is_none());
    // Removing the last model removes the manifest too.
    assert!(!manager.models_dir().join("installed.json").exists());
}

#[test]
fn multiple_models_coexist_and_selection_switches() {
    let (_dir, manager) = manager();
    let bytes_a = fixture_bytes();
    let bytes_b = b"a different test model".to_vec();
    let desc_a = descriptor_for_id("model-a", &bytes_a, "1.0.0");
    let desc_b = descriptor_for_id("model-b", &bytes_b, "1.0.0");

    install(&manager, &desc_a, &bytes_a);
    install(&manager, &desc_b, &bytes_b);

    // Both are installed; the first one stays active (installing a second
    // model does not silently switch the user's selection).
    assert_eq!(manager.installed_models().len(), 2);
    assert!(manager.status_for(&desc_a).is_active);
    assert!(!manager.status_for(&desc_b).is_active);
    let path_a = manager.active_model_path().unwrap();
    assert_eq!(std::fs::read(&path_a).unwrap(), bytes_a);

    manager.set_active_model("model-b").unwrap();
    assert!(!manager.status_for(&desc_a).is_active);
    assert!(manager.status_for(&desc_b).is_active);
    let path_b = manager.active_model_path().unwrap();
    assert_eq!(std::fs::read(&path_b).unwrap(), bytes_b);
}

#[test]
fn set_active_requires_an_installed_model() {
    let (_dir, manager) = manager();
    assert!(manager.set_active_model("never-installed").is_err());
}

#[test]
fn removing_the_active_model_clears_the_selection() {
    let (_dir, manager) = manager();
    let bytes_a = fixture_bytes();
    let bytes_b = b"a different test model".to_vec();
    let desc_a = descriptor_for_id("model-a", &bytes_a, "1.0.0");
    let desc_b = descriptor_for_id("model-b", &bytes_b, "1.0.0");
    install(&manager, &desc_a, &bytes_a);
    install(&manager, &desc_b, &bytes_b);

    manager.remove_model("model-a").unwrap();

    assert!(manager.active_model_path().is_none());
    assert!(manager.active_model().is_none());
    // The other model is untouched.
    assert_eq!(
        manager.status_for(&desc_b).state,
        ModelState::Installed {
            version: "1.0.0".to_string()
        }
    );
}

#[test]
fn legacy_single_model_manifest_is_migrated() {
    let (_dir, manager) = manager();
    let bytes = fixture_bytes();
    let descriptor = descriptor_for(&bytes, "1.0.0");
    install(&manager, &descriptor, &bytes);

    // Rewrite installed.json in the legacy bare-InstalledModel format.
    let legacy = manager.installed_model("test-model").unwrap();
    let legacy_json = serde_json::to_vec_pretty(&legacy).unwrap();
    std::fs::write(manager.models_dir().join("installed.json"), legacy_json).unwrap();

    let manifest = manager.manifest();
    assert_eq!(manifest.models.len(), 1);
    assert_eq!(manifest.active_model_id.as_deref(), Some("test-model"));
    assert!(manager.status_for(&descriptor).is_active);
    assert!(manager.active_model_path().is_some());
}

#[test]
fn version_comparison() {
    assert!(version_is_newer("1.0.1", "1.0.0"));
    assert!(version_is_newer("2.0", "1.9.9"));
    assert!(version_is_newer("1.1", "1.0.5"));
    assert!(!version_is_newer("1.0.0", "1.0.0"));
    assert!(!version_is_newer("1.0.0", "1.0.1"));
    assert!(!version_is_newer("1.0", "1.0.1"));
    // Unparseable versions fall back to inequality.
    assert!(version_is_newer("v2", "v1"));
    assert!(!version_is_newer("v1", "v1"));
    // Zero-padding: trailing zero segments do not make a version newer.
    assert!(!version_is_newer("1.0.0", "1.0"));
    assert!(!version_is_newer("1.0", "1.0.0"));
    // Pre-release suffixes sort before the release, never as an update
    // over it, and a release is newer than its own pre-releases.
    assert!(!version_is_newer("1.0.0-rc1", "1.0.0"));
    assert!(version_is_newer("1.0.0", "1.0.0-rc1"));
    assert!(version_is_newer("1.0.0-rc2", "1.0.0-rc1"));
    assert!(!version_is_newer("1.0.0-rc1", "1.0.0-rc2"));
}

#[test]
fn set_active_fails_when_the_model_file_was_deleted_externally() {
    let (_dir, manager) = manager();
    let bytes = fixture_bytes();
    let descriptor = descriptor_for(&bytes, "1.0.0");
    install(&manager, &descriptor, &bytes);
    assert!(manager.installed_model_path("test-model").is_some());

    // Delete the .onnx behind the manifest's back: the record remains,
    // but activation must now fail instead of pointing chat at nothing.
    let path = manager.active_model_path().unwrap();
    std::fs::remove_file(&path).unwrap();
    assert!(manager.installed_model_path("test-model").is_none());
    assert!(manager.set_active_model("test-model").is_err());
}

#[test]
fn remove_model_rejects_path_traversal_and_unknown_ids() {
    let (_dir, manager) = manager();
    for bad in ["..", "../outside", "a/b", "a\\b", "", "no-such-model"] {
        assert!(
            manager.remove_model(bad).is_err(),
            "remove_model({bad:?}) must fail"
        );
    }
}
