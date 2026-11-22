// Copyright (c) 2026 Christopher Yang
// SPDX-License-Identifier: MIT

//! Post-install AI model management.
//!
//! The AI models are **not** bundled with the application. After install,
//! the user picks a model from the compiled-in catalog (the pattern
//! popularized by Meetily's model manager: a list with sizes and plain
//! quality/speed guidance) and downloads it on demand. Models live under
//! the app's data directory (`<app_data>/models/<model-id>/<version>/`)
//! with an `installed.json` manifest recording what is on disk and which
//! installed model is the active one.
//!
//! The catalog is compiled into the app. Shipping a new app build with a
//! bumped catalog version is how model upgrades are offered:
//! [`ModelManager::status_for`] compares the installed version against
//! the catalog and reports [`ModelState::UpdateAvailable`], and
//! [`ModelManager::download`] fetches, verifies, and atomically swaps in
//! the new file.
//!
//! Integrity: every download is verified against the SHA-256 pinned in
//! the catalog before it becomes usable. A mismatch deletes the partial
//! file and leaves any previously installed model untouched.
//!
//! The only network access in Stern is the download performed here, and
//! it happens only when the user explicitly asks for it.

use crate::domain::vault::errors::VaultError;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

/// A downloadable AI model known to this build of Stern.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelDescriptor {
    /// Stable model identifier (directory name, manifest key).
    pub id: &'static str,
    /// Human-readable name shown in the UI.
    pub display_name: &'static str,
    /// Plain-language guidance shown next to the model in the picker:
    /// what it is good at and what it costs (size, speed).
    pub description: &'static str,
    /// Catalog version of this model build (dotted numeric, e.g. "1.0.0").
    pub version: &'static str,
    /// File name the model is stored under inside its version directory.
    pub file_name: &'static str,
    /// Canonical download URL.
    pub download_url: &'static str,
    /// Lowercase hex SHA-256 of the model file. This is the authoritative
    /// integrity check for every download.
    pub sha256: &'static str,
    /// Approximate size in bytes, as published by the model host (the
    /// host's displayed megabyte figure is rounded). Used for UI display
    /// and as the progress total when the server does not send a
    /// `Content-Length`; the SHA-256 above is the real verification.
    pub size_bytes: u64,
}

/// The model selected automatically when the user has not chosen one.
pub const DEFAULT_MODEL_ID: &str = "smollm-135m";

/// Every model this build of Stern offers.
///
/// One entry per model id: bump `version` (and the URL/hash) to offer an
/// upgrade to existing installs. All SHA-256 values were taken from the
/// files' Hugging Face pages.
pub static MODEL_CATALOG: &[ModelDescriptor] = &[
    ModelDescriptor {
        id: DEFAULT_MODEL_ID,
        display_name: "SmolLM-135M",
        description: "The original Stern model. Smallest family, fast responses; \
                      best for simple, direct commands.",
        version: "1.0.0",
        file_name: "model_int8.onnx",
        download_url:
            "https://huggingface.co/HuggingFaceTB/SmolLM-135M/resolve/main/onnx/model_int8.onnx",
        sha256: "9fc89b8ea13d2e8ee3edcc19e728af1eb556ae4945824998a61d9cbbb8029df2",
        size_bytes: 142_606_336, // ~136 MiB as listed on the model page
    },
    ModelDescriptor {
        id: "smollm2-135m-instruct",
        display_name: "SmolLM2-135M-Instruct",
        description: "Newer 135M generation with better instruction following, \
                      in a slightly smaller download.",
        version: "1.0.0",
        file_name: "model_q4f16.onnx",
        download_url: "https://huggingface.co/HuggingFaceTB/SmolLM2-135M-Instruct/resolve/main/onnx/model_q4f16.onnx",
        sha256: "9358cd4ce037c304621f8c194a525607ae7c5ea73239fcae4c21bd02f2e34ff7",
        size_bytes: 118_000_000, // 118 MB as listed on the model page
    },
    ModelDescriptor {
        id: "smollm2-360m-instruct",
        display_name: "SmolLM2-360M-Instruct",
        description: "Recommended for most users. Noticeably better at varied \
                      phrasing, still fast on CPU.",
        version: "1.0.0",
        file_name: "model_q4f16.onnx",
        download_url: "https://huggingface.co/HuggingFaceTB/SmolLM2-360M-Instruct/resolve/main/onnx/model_q4f16.onnx",
        sha256: "cc63370efc2aca6d5307518b85162777132cc5b8d68eeb8154ea9b5fce09ad46",
        size_bytes: 273_000_000, // 273 MB as listed on the model page
    },
    ModelDescriptor {
        id: "qwen2.5-0.5b-instruct",
        display_name: "Qwen2.5-0.5B-Instruct",
        description: "The most capable model in this list, with the longest \
                      context. Biggest download and the slowest on CPU.",
        version: "1.0.0",
        file_name: "model_q4f16.onnx",
        download_url: "https://huggingface.co/onnx-community/Qwen2.5-0.5B-Instruct/resolve/main/onnx/model_q4f16.onnx",
        sha256: "b11c1dd99efd57e6c6e5bc4443a019931a5fbd5dd500d48644d8225f5ce0b2cb",
        size_bytes: 483_000_000, // 483 MB as listed on the model page
    },
];

/// The catalog entry for a model id, if this build knows it.
#[must_use]
pub fn catalog_model(id: &str) -> Option<&'static ModelDescriptor> {
    MODEL_CATALOG.iter().find(|m| m.id == id)
}

/// Record of a model that has been downloaded and verified on this device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledModel {
    pub model_id: String,
    pub version: String,
    pub file_name: String,
    /// Hex SHA-256 computed over the downloaded bytes (matches the catalog).
    pub sha256: String,
    /// Actual number of bytes downloaded.
    pub size_bytes: u64,
    /// RFC 3339 timestamp of the install.
    pub installed_at: String,
}

/// The `installed.json` manifest: every verified model on disk, and which
/// of them is active. Several models may be installed at once; exactly one
/// (or none) is active.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelManifest {
    pub models: Vec<InstalledModel>,
    #[serde(default)]
    pub active_model_id: Option<String>,
}

/// Where an installed model stands relative to the catalog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelState {
    /// No verified model file on disk.
    NotInstalled,
    /// The catalog version is installed.
    Installed { version: String },
    /// An older version is installed and the catalog offers a newer one.
    UpdateAvailable {
        installed_version: String,
        latest_version: String,
    },
}

/// Full status for one catalog model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelStatusInfo {
    pub descriptor: ModelDescriptor,
    pub state: ModelState,
    pub installed: Option<InstalledModel>,
    /// Whether this model is the manifest's active model.
    pub is_active: bool,
}

/// Owns the on-disk model store rooted at a caller-provided directory
/// (the Tauri shell passes `<app_data_dir>/models`).
pub struct ModelManager {
    models_dir: PathBuf,
    /// Serializes every manifest read-modify-write cycle (and the matching
    /// filesystem changes) so concurrent installs, removals, and active-model
    /// changes cannot interleave and leave `installed.json` out of sync
    /// with the model directories on disk.
    manifest_lock: Mutex<()>,
}

impl ModelManager {
    #[must_use]
    pub const fn new(models_dir: PathBuf) -> Self {
        Self {
            models_dir,
            manifest_lock: Mutex::new(()),
        }
    }

    /// Lock the manifest for a read-modify-write cycle, recovering the
    /// guard if a previous holder panicked while holding it.
    fn lock_manifest(&self) -> std::sync::MutexGuard<'_, ()> {
        self.manifest_lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Reject a caller-supplied model id that is not safe to use as a
    /// path component: it must be a model this build's catalog knows, or
    /// one already recorded in the manifest, and it must never contain
    /// path separators or traversal segments.
    fn validate_stored_model_id(&self, model_id: &str) -> Result<(), VaultError> {
        if model_id.is_empty()
            || model_id == "."
            || model_id == ".."
            || model_id.contains('/')
            || model_id.contains('\\')
            || model_id.contains("..")
        {
            return Err(VaultError::Model(format!("invalid model id: {model_id}")));
        }
        let known = catalog_model(model_id).is_some()
            || self
                .manifest()
                .models
                .iter()
                .any(|m| m.model_id == model_id);
        if known {
            Ok(())
        } else {
            Err(VaultError::Model(format!("unknown model: {model_id}")))
        }
    }

    #[must_use]
    pub fn models_dir(&self) -> &Path {
        &self.models_dir
    }

    /// The descriptor of the default model.
    pub fn default_descriptor(&self) -> Result<ModelDescriptor, VaultError> {
        catalog_model(DEFAULT_MODEL_ID)
            .copied()
            .ok_or_else(|| VaultError::Model("model catalog is empty".to_string()))
    }

    fn manifest_path(&self) -> PathBuf {
        self.models_dir.join("installed.json")
    }

    fn model_version_dir(&self, model_id: &str, version: &str) -> PathBuf {
        self.models_dir.join(model_id).join(version)
    }

    fn installed_file_path(&self, installed: &InstalledModel) -> PathBuf {
        self.model_version_dir(&installed.model_id, &installed.version)
            .join(&installed.file_name)
    }

    /// The manifest as recorded on disk. A missing or corrupt manifest
    /// reads as "nothing installed". A legacy manifest holding a single
    /// bare [`InstalledModel`] (the pre-selection format) is migrated in
    /// memory to a one-model manifest with that model active.
    #[must_use]
    pub fn manifest(&self) -> ModelManifest {
        let Ok(bytes) = fs::read(self.manifest_path()) else {
            return ModelManifest::default();
        };
        if let Ok(manifest) = serde_json::from_slice::<ModelManifest>(&bytes) {
            return manifest;
        }
        if let Ok(legacy) = serde_json::from_slice::<InstalledModel>(&bytes) {
            return ModelManifest {
                active_model_id: Some(legacy.model_id.clone()),
                models: vec![legacy],
            };
        }
        ModelManifest::default()
    }

    /// Every model recorded as installed (whether or not its file is
    /// still present; use [`ModelManager::status_for`] for the checked
    /// per-model view).
    #[must_use]
    pub fn installed_models(&self) -> Vec<InstalledModel> {
        self.manifest().models
    }

    /// The install record for one model, if present.
    #[must_use]
    pub fn installed_model(&self, model_id: &str) -> Option<InstalledModel> {
        self.manifest()
            .models
            .into_iter()
            .find(|m| m.model_id == model_id)
    }

    /// The active model's install record, if one is selected and present.
    #[must_use]
    pub fn active_model(&self) -> Option<InstalledModel> {
        let manifest = self.manifest();
        let active_id = manifest.active_model_id?;
        manifest
            .models
            .into_iter()
            .find(|m| m.model_id == active_id)
    }

    /// Path of the active model's verified file, if it is installed and
    /// still present on disk. This is the file the app should load.
    #[must_use]
    pub fn active_model_path(&self) -> Option<PathBuf> {
        let installed = self.active_model()?;
        let path = self.installed_file_path(&installed);
        path.exists().then_some(path)
    }

    /// Make an installed model the active one.
    pub fn set_active_model(&self, model_id: &str) -> Result<(), VaultError> {
        let _guard = self.lock_manifest();
        let mut manifest = self.manifest();
        if !manifest.models.iter().any(|m| m.model_id == model_id) {
            return Err(VaultError::Model(format!(
                "model {model_id} is not installed"
            )));
        }
        manifest.active_model_id = Some(model_id.to_string());
        self.write_manifest(&manifest)
    }

    /// Status of the default model.
    pub fn status(&self) -> Result<ModelStatusInfo, VaultError> {
        Ok(self.status_for(&self.default_descriptor()?))
    }

    /// Status of an arbitrary catalog model.
    #[must_use]
    pub fn status_for(&self, descriptor: &ModelDescriptor) -> ModelStatusInfo {
        let manifest = self.manifest();
        let installed = manifest
            .models
            .iter()
            .find(|i| i.model_id == descriptor.id)
            .filter(|i| self.installed_file_path(i).exists())
            .cloned();

        let state = match &installed {
            None => ModelState::NotInstalled,
            Some(inst) if version_is_newer(descriptor.version, &inst.version) => {
                ModelState::UpdateAvailable {
                    installed_version: inst.version.clone(),
                    latest_version: descriptor.version.to_string(),
                }
            }
            Some(inst) => ModelState::Installed {
                version: inst.version.clone(),
            },
        };

        let is_active =
            installed.is_some() && manifest.active_model_id.as_deref() == Some(descriptor.id);

        ModelStatusInfo {
            descriptor: *descriptor,
            state,
            installed,
            is_active,
        }
    }

    /// Download the catalog model over HTTPS and install it.
    ///
    /// `progress` is called with `(downloaded_bytes, total_bytes)` as the
    /// body streams in. This is the only network call in Stern.
    pub fn download(
        &self,
        descriptor: &ModelDescriptor,
        progress: &mut dyn FnMut(u64, u64),
    ) -> Result<InstalledModel, VaultError> {
        // Explicit timeouts so a stalled download fails instead of hanging
        // forever: connecting and receiving the response headers are
        // short, and `timeout_per_read` restarts on every read, so a large
        // model on a slow link is fine as long as bytes keep arriving —
        // 60 seconds without a single successful read aborts the download.
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_connect(Some(Duration::from_secs(15)))
            .timeout_recv_response(Some(Duration::from_secs(30)))
            .timeout_per_read(Some(Duration::from_mins(1)))
            .build()
            .into();
        let response = agent
            .get(descriptor.download_url)
            .call()
            .map_err(|e| VaultError::Model(format!("model download failed: {e}")))?;

        if !response.status().is_success() {
            return Err(VaultError::Model(format!(
                "model download failed: HTTP {}",
                response.status()
            )));
        }

        let total = response
            .headers()
            .get("content-length")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok())
            .filter(|&n| n > 0)
            .unwrap_or(descriptor.size_bytes);

        let mut reader = response.into_body().into_reader();
        self.install_from_reader(descriptor, &mut reader, total, progress)
    }

    /// Install a model from any byte stream: stream to a `.part` file while
    /// hashing, verify the SHA-256 against the catalog, then atomically move
    /// the file into place and update the manifest. On any failure the
    /// partial file is removed and every previously installed model is left
    /// exactly as it was. If no model is active afterwards, the newly
    /// installed model becomes active.
    pub fn install_from_reader(
        &self,
        descriptor: &ModelDescriptor,
        reader: &mut dyn Read,
        total_bytes: u64,
        progress: &mut dyn FnMut(u64, u64),
    ) -> Result<InstalledModel, VaultError> {
        let version_dir = self.model_version_dir(descriptor.id, descriptor.version);
        fs::create_dir_all(&version_dir)
            .map_err(|e| VaultError::Model(format!("create model dir: {e}")))?;

        let part_path = version_dir.join(format!("{}.part", descriptor.file_name));
        let final_path = version_dir.join(descriptor.file_name);

        let installed =
            Self::install_into_part_file(descriptor, reader, total_bytes, progress, &part_path)
                .and_then(|installed| {
                    fs::rename(&part_path, &final_path)
                        .map_err(|e| VaultError::Model(format!("move model into place: {e}")))?;
                    Ok(installed)
                });
        let installed = match installed {
            Ok(installed) => installed,
            Err(e) => {
                if let Err(cleanup) = fs::remove_file(&part_path) {
                    tracing::warn!("failed to remove partial model file: {cleanup}");
                }
                return Err(e);
            }
        };

        let _guard = self.lock_manifest();
        let mut manifest = self.manifest();
        manifest.models.retain(|m| m.model_id != installed.model_id);
        manifest.models.push(installed.clone());
        if manifest.active_model_id.is_none() {
            manifest.active_model_id = Some(installed.model_id.clone());
        }
        self.write_manifest(&manifest)?;
        self.prune_old_versions(descriptor.id, descriptor.version);
        Ok(installed)
    }

    fn install_into_part_file(
        descriptor: &ModelDescriptor,
        reader: &mut dyn Read,
        total_bytes: u64,
        progress: &mut dyn FnMut(u64, u64),
        part_path: &Path,
    ) -> Result<InstalledModel, VaultError> {
        let mut file = fs::File::create(part_path)
            .map_err(|e| VaultError::Model(format!("create model file: {e}")))?;

        let mut hasher = Sha256::new();
        let mut downloaded: u64 = 0;
        let mut buffer = vec![0u8; 64 * 1024].into_boxed_slice();
        loop {
            let n = reader
                .read(&mut buffer)
                .map_err(|e| VaultError::Model(format!("read model download: {e}")))?;
            if n == 0 {
                break;
            }
            let chunk = buffer
                .get(..n)
                .ok_or_else(|| VaultError::Model("read buffer overflow".to_string()))?;
            hasher.update(chunk);
            file.write_all(chunk)
                .map_err(|e| VaultError::Model(format!("write model file: {e}")))?;
            let n_u64 = u64::try_from(n)
                .map_err(|_| VaultError::Model("read size overflow".to_string()))?;
            downloaded = downloaded.saturating_add(n_u64);
            progress(downloaded, total_bytes);
        }
        file.flush()
            .map_err(|e| VaultError::Model(format!("flush model file: {e}")))?;
        drop(file);

        let digest = data_encoding::HEXLOWER.encode(&hasher.finalize());
        if !digest.eq_ignore_ascii_case(descriptor.sha256) {
            return Err(VaultError::Model(format!(
                "model integrity check failed: expected SHA-256 {}, got {digest}",
                descriptor.sha256
            )));
        }

        Ok(InstalledModel {
            model_id: descriptor.id.to_string(),
            version: descriptor.version.to_string(),
            file_name: descriptor.file_name.to_string(),
            sha256: digest,
            size_bytes: downloaded,
            installed_at: chrono::Utc::now().to_rfc3339(),
        })
    }

    fn write_manifest(&self, manifest: &ModelManifest) -> Result<(), VaultError> {
        fs::create_dir_all(&self.models_dir)
            .map_err(|e| VaultError::Model(format!("create models dir: {e}")))?;
        let json = serde_json::to_vec_pretty(manifest)
            .map_err(|e| VaultError::Serialization(e.to_string()))?;
        // Unique per process, so two Stern processes cannot clobber each
        // other's temp file; within one process the manifest lock already
        // serializes writers.
        let tmp_path = self
            .models_dir
            .join(format!("installed.json.{}.tmp", std::process::id()));
        fs::write(&tmp_path, &json)
            .map_err(|e| VaultError::Model(format!("write model manifest: {e}")))?;
        fs::rename(&tmp_path, self.manifest_path())
            .map_err(|e| VaultError::Model(format!("replace model manifest: {e}")))?;
        Ok(())
    }

    /// Remove superseded version directories of a model after a successful
    /// install, so updates do not accumulate stale model files.
    fn prune_old_versions(&self, model_id: &str, keep_version: &str) {
        let model_dir = self.models_dir.join(model_id);
        let Ok(entries) = fs::read_dir(&model_dir) else {
            return;
        };
        for entry in entries.flatten() {
            let is_other_version = entry
                .file_name()
                .to_str()
                .is_some_and(|name| name != keep_version);
            if is_other_version && entry.path().is_dir() {
                if let Err(e) = fs::remove_dir_all(entry.path()) {
                    tracing::warn!("failed to prune old model version: {e}");
                }
            }
        }
    }

    /// Remove one installed model (all its versions) and its manifest
    /// record. If it was the active model, no model is active afterwards.
    pub fn remove_model(&self, model_id: &str) -> Result<(), VaultError> {
        // Validate before touching the filesystem: `model_id` comes from
        // the UI/IPC layer, and joining an unchecked id (e.g. one with
        // `..` or a path separator) would let `remove_dir_all` escape
        // the models directory.
        self.validate_stored_model_id(model_id)?;
        let _guard = self.lock_manifest();
        let model_dir = self.models_dir.join(model_id);
        if model_dir.exists() {
            fs::remove_dir_all(&model_dir)
                .map_err(|e| VaultError::Model(format!("remove model: {e}")))?;
        }
        let mut manifest = self.manifest();
        manifest.models.retain(|m| m.model_id != model_id);
        if manifest.active_model_id.as_deref() == Some(model_id) {
            manifest.active_model_id = None;
        }
        if manifest.models.is_empty() && manifest.active_model_id.is_none() {
            let path = self.manifest_path();
            if path.exists() {
                fs::remove_file(&path)
                    .map_err(|e| VaultError::Model(format!("remove model manifest: {e}")))?;
            }
        } else {
            self.write_manifest(&manifest)?;
        }
        Ok(())
    }
}

/// Is `latest` a strictly newer dotted-numeric version than `installed`?
///
/// The numeric core is compared segment by segment with the shorter side
/// zero-padded, so `1.0` and `1.0.0` are equal — neither is newer. A
/// `-suffix` (e.g. `-rc1`) marks a pre-release: it sorts *before* the same
/// core without a suffix, and two suffixes on the same core compare
/// lexically. That keeps a downgrade (including release → pre-release of
/// the same core) from ever reading as an update. Versions whose core is
/// not dotted-numeric at all fall back to plain inequality.
#[must_use]
pub fn version_is_newer(latest: &str, installed: &str) -> bool {
    fn parse(v: &str) -> Option<(Vec<u64>, Option<&str>)> {
        let (core, suffix) = match v.split_once('-') {
            Some((core, suffix)) => (core, Some(suffix)),
            None => (v, None),
        };
        let segments: Option<Vec<u64>> = core
            .split('.')
            .map(|part| part.parse::<u64>().ok())
            .collect();
        segments.map(|segments| (segments, suffix))
    }
    match (parse(latest), parse(installed)) {
        (Some((mut l, l_suffix)), Some((mut i, i_suffix))) => {
            let len = l.len().max(i.len());
            l.resize(len, 0);
            i.resize(len, 0);
            match l.cmp(&i) {
                std::cmp::Ordering::Greater => true,
                std::cmp::Ordering::Less => false,
                std::cmp::Ordering::Equal => match (l_suffix, i_suffix) {
                    (None, Some(_)) => true,
                    (Some(l), Some(i)) => l > i,
                    _ => false,
                },
            }
        }
        _ => latest != installed,
    }
}
