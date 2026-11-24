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
//! The ONNX Runtime library itself is also not bundled: the app binary
//! is built with ort's `load-dynamic` feature and ships with no ONNX
//! code at all. The first model download also fetches the pinned
//! runtime build for the current platform (see [`RUNTIME_CATALOG`]),
//! verifies it the same way, and extracts just the shared library;
//! [`ModelManager::runtime_path`] then hands it to `AiModel`, which
//! dlopens it. Until a model (and therefore the runtime) is present,
//! there is no AI in the app at all.
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

/// How a model's chat prompt is formatted for parsing commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptStyle {
    /// `ChatML` turns (`<|im_start|>` / `<|im_end|>`), for the
    /// instruction-tuned models.
    ChatMl,
    /// A plain few-shot completion, for the base model, which has
    /// no instruction tuning to lean on.
    FewShot,
}

/// The file name every model's tokenizer is stored under, beside
/// the model file in its version directory.
pub const TOKENIZER_FILE_NAME: &str = "tokenizer.json";

/// A downloadable AI model known to this build of Stern.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelDescriptor {
    /// Stable model identifier (directory name, manifest key).
    pub id: &'static str,
    /// The size tier shown in the UI ("Small", "Medium", "Large").
    /// Deliberately not the underlying model's name: users choose a
    /// size, not a model (ADR-0010).
    pub display_name: &'static str,
    /// Plain-language guidance shown next to the tier in the picker:
    /// what it is good at and what it costs (size, speed). Never names
    /// the underlying model.
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
    /// Canonical download URL of the model's `tokenizer.json`.
    pub tokenizer_url: &'static str,
    /// Lowercase hex SHA-256 of the tokenizer file.
    pub tokenizer_sha256: &'static str,
    /// Size in bytes of the tokenizer file.
    pub tokenizer_size_bytes: u64,
    /// How this model's parsing prompt is formatted.
    pub prompt_style: PromptStyle,
    /// Approximate size in bytes, as published by the model host (the
    /// host's displayed megabyte figure is rounded). Used for UI display
    /// and as the progress total when the server does not send a
    /// `Content-Length`; the SHA-256 above is the real verification.
    pub size_bytes: u64,
}

/// The model selected automatically when the user has not chosen one:
/// the Small tier.
pub const DEFAULT_MODEL_ID: &str = "smollm2-135m-instruct";

/// Every model this build of Stern offers, smallest first. The UI
/// shows each as a size tier (`display_name`), never by model name.
///
/// One entry per model id: bump `version` (and the URL/hash) to offer an
/// upgrade to existing installs. All SHA-256 values were taken from the
/// files' Hugging Face pages. A model dropped from this list is removed
/// from devices that still have it at startup
/// ([`ModelManager::remove_retired_models`]).
pub static MODEL_CATALOG: &[ModelDescriptor] = &[
    // Small: SmolLM2-135M-Instruct.
    ModelDescriptor {
        id: DEFAULT_MODEL_ID,
        display_name: "Small",
        description: "Fastest and lightest, with the smallest download. Best for \
                      simple, direct commands.",
        version: "1.0.0",
        file_name: "model_q4f16.onnx",
        download_url: "https://huggingface.co/HuggingFaceTB/SmolLM2-135M-Instruct/resolve/main/onnx/model_q4f16.onnx",
        sha256: "9358cd4ce037c304621f8c194a525607ae7c5ea73239fcae4c21bd02f2e34ff7",
        size_bytes: 118_000_000, // 118 MB as listed on the model page
        tokenizer_url: "https://huggingface.co/HuggingFaceTB/SmolLM2-135M-Instruct/resolve/main/tokenizer.json",
        tokenizer_sha256: "9ca9acddb6525a194ec8ac7a87f24fbba7232a9a15ffa1af0c1224fcd888e47c",
        tokenizer_size_bytes: 2_104_556,
        prompt_style: PromptStyle::ChatMl,
    },
    // Medium: SmolLM2-360M-Instruct.
    ModelDescriptor {
        id: "smollm2-360m-instruct",
        display_name: "Medium",
        description: "Recommended for most people. Handles varied phrasing \
                      noticeably better, and is still fast.",
        version: "1.0.0",
        file_name: "model_q4f16.onnx",
        download_url: "https://huggingface.co/HuggingFaceTB/SmolLM2-360M-Instruct/resolve/main/onnx/model_q4f16.onnx",
        sha256: "cc63370efc2aca6d5307518b85162777132cc5b8d68eeb8154ea9b5fce09ad46",
        size_bytes: 273_000_000, // 273 MB as listed on the model page
        tokenizer_url: "https://huggingface.co/HuggingFaceTB/SmolLM2-360M-Instruct/resolve/main/tokenizer.json",
        tokenizer_sha256: "9ca9acddb6525a194ec8ac7a87f24fbba7232a9a15ffa1af0c1224fcd888e47c",
        tokenizer_size_bytes: 2_104_556,
        prompt_style: PromptStyle::ChatMl,
    },
    // Large: Qwen2.5-0.5B-Instruct.
    ModelDescriptor {
        id: "qwen2.5-0.5b-instruct",
        display_name: "Large",
        description: "The most capable, with the widest understanding of how \
                      you phrase things. The biggest download and the slowest \
                      to respond.",
        version: "1.0.0",
        file_name: "model_q4f16.onnx",
        download_url: "https://huggingface.co/onnx-community/Qwen2.5-0.5B-Instruct/resolve/main/onnx/model_q4f16.onnx",
        sha256: "b11c1dd99efd57e6c6e5bc4443a019931a5fbd5dd500d48644d8225f5ce0b2cb",
        size_bytes: 483_000_000, // 483 MB as listed on the model page
        tokenizer_url: "https://huggingface.co/onnx-community/Qwen2.5-0.5B-Instruct/resolve/main/tokenizer.json",
        tokenizer_sha256: "a8506e7111b80c6d8635951a02eab0f4e1a8e4e5772da83846579e97b16f61bf",
        tokenizer_size_bytes: 7_031_673,
        prompt_style: PromptStyle::ChatMl,
    },
];

/// The catalog entry for a model id, if this build knows it.
#[must_use]
pub fn catalog_model(id: &str) -> Option<&'static ModelDescriptor> {
    MODEL_CATALOG.iter().find(|m| m.id == id)
}

/// The ONNX Runtime version this build loads. It must match the API
/// level the `ort` crate is compiled against (api-24 for
/// ort 2.0.0-rc.12, whose own bundled distribution is 1.31.0).
pub const RUNTIME_VERSION: &str = "1.31.0";

/// A downloadable ONNX Runtime build for one platform.
///
/// The archive is the official `onnxruntime` Python wheel for the
/// platform, used purely as a distribution container: exactly one
/// entry — the shared library — is extracted from it, and the wheel's
/// SHA-256 (as published by the `PyPI` JSON API) is pinned here. The
/// wheel is ~21–24 MB; the extracted library is what stays on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeDescriptor {
    /// `std::env::consts::OS` value this build targets ("macos", "linux").
    pub platform_os: &'static str,
    /// `std::env::consts::ARCH` value this build targets.
    pub platform_arch: &'static str,
    /// Runtime version (also the version directory name).
    pub version: &'static str,
    /// Canonical download URL of the wheel archive.
    pub archive_url: &'static str,
    /// Lowercase hex SHA-256 of the wheel archive.
    pub sha256: &'static str,
    /// Size of the wheel archive in bytes (progress totals/display).
    pub size_bytes: u64,
    /// Path of the shared library inside the wheel.
    pub archive_entry: &'static str,
    /// File name the library is stored under in its version directory.
    pub lib_file_name: &'static str,
}

/// Every platform this build can fetch an ONNX Runtime for — the
/// platforms Stern ships (macOS Apple Silicon, Linux `x86_64`).
pub static RUNTIME_CATALOG: &[RuntimeDescriptor] = &[
    RuntimeDescriptor {
        platform_os: "macos",
        platform_arch: "aarch64",
        version: RUNTIME_VERSION,
        archive_url: "https://files.pythonhosted.org/packages/e0/2b/117f94d73a3bac4276c285c47e384e1b3ea67b191aa4c7592df9d3f4a136/onnxruntime-1.31.0-cp313-cp313-macosx_14_0_arm64.whl",
        sha256: "0ba02a44acb6203040354d9a1f160e3f37a43feac7bb05caa3e0ea545efed505",
        size_bytes: 20_881_803,
        archive_entry: "onnxruntime/capi/libonnxruntime.1.31.0.dylib",
        lib_file_name: "libonnxruntime.1.31.0.dylib",
    },
    RuntimeDescriptor {
        platform_os: "linux",
        platform_arch: "x86_64",
        version: RUNTIME_VERSION,
        archive_url: "https://files.pythonhosted.org/packages/0d/ac/67ebbaab4b3083f2a6b27ee6c4aa400c7f8d6c72b5499aac7e4cd6ba74f5/onnxruntime-1.31.0-cp313-cp313-manylinux_2_28_x86_64.whl",
        sha256: "37fd78cee5160c7a43a1730ccb3682ffd880af9c9e80385d625c0c2f8b125809",
        size_bytes: 23_760_708,
        archive_entry: "onnxruntime/capi/libonnxruntime.so.1.31.0",
        lib_file_name: "libonnxruntime.so.1.31.0",
    },
];

/// The runtime build for the platform this binary is running on, if
/// the catalog carries one.
#[must_use]
pub fn current_runtime() -> Option<&'static RuntimeDescriptor> {
    RUNTIME_CATALOG.iter().find(|r| {
        r.platform_os == std::env::consts::OS && r.platform_arch == std::env::consts::ARCH
    })
}

/// Record of the ONNX Runtime library that has been downloaded,
/// verified, and extracted on this device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledRuntime {
    pub version: String,
    pub lib_file_name: String,
    /// Hex SHA-256 of the downloaded archive (matches the catalog).
    pub sha256: String,
    /// Actual number of archive bytes downloaded.
    pub size_bytes: u64,
    /// RFC 3339 timestamp of the install.
    pub installed_at: String,
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
    /// The downloaded ONNX Runtime library, if one is installed.
    /// Absent in manifests written before the runtime became a
    /// download, hence the default.
    #[serde(default)]
    pub runtime: Option<InstalledRuntime>,
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
    /// Serializes a whole runtime install (download, extraction, and
    /// the manifest update) so concurrent `ensure_runtime` callers
    /// cannot download to and rename the same `.part` archive or race
    /// the runtime record in `installed.json`.
    runtime_lock: Mutex<()>,
}

impl ModelManager {
    #[must_use]
    pub const fn new(models_dir: PathBuf) -> Self {
        Self {
            models_dir,
            manifest_lock: Mutex::new(()),
            runtime_lock: Mutex::new(()),
        }
    }

    /// Lock the manifest for a read-modify-write cycle, recovering the
    /// guard if a previous holder panicked while holding it.
    fn lock_manifest(&self) -> std::sync::MutexGuard<'_, ()> {
        self.manifest_lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Lock the runtime install for one `ensure_runtime` run,
    /// recovering the guard if a previous holder panicked.
    fn lock_runtime(&self) -> std::sync::MutexGuard<'_, ()> {
        self.runtime_lock
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
                runtime: None,
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

    /// Remove every installed model that this build's catalog no longer
    /// offers, returning the ids removed. A retired model would
    /// otherwise sit on disk (and stay active) with no way to manage it
    /// from the picker. Removal failures are skipped, not fatal: the
    /// model simply stays until the next launch.
    pub fn remove_retired_models(&self) -> Vec<String> {
        self.manifest()
            .models
            .into_iter()
            .map(|m| m.model_id)
            .filter(|id| catalog_model(id).is_none())
            .filter(|id| self.remove_model(id).is_ok())
            .collect()
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

    fn runtime_version_dir(&self, version: &str) -> PathBuf {
        self.models_dir.join("runtime").join(version)
    }

    /// Path of the installed ONNX Runtime shared library, if the
    /// runtime recorded in the manifest *is the catalog's pinned
    /// build* and is still present on disk. This is the library
    /// `AiModel` dlopens; without it no model can load. A record
    /// whose version (or library name) does not match the current
    /// catalog — e.g. after a pinned-version bump — counts as not
    /// installed, so `ensure_runtime` fetches the pinned build
    /// instead of handing `AiModel` the older library.
    #[must_use]
    pub fn runtime_path(&self) -> Option<PathBuf> {
        let runtime = self.manifest().runtime?;
        let descriptor = current_runtime()?;
        if runtime.version != descriptor.version
            || runtime.lib_file_name != descriptor.lib_file_name
        {
            return None;
        }
        let path = self
            .runtime_version_dir(descriptor.version)
            .join(descriptor.lib_file_name);
        path.exists().then_some(path)
    }

    /// Make sure the ONNX Runtime library for this platform is
    /// installed, downloading, verifying, and extracting it if not.
    /// Returns the library path. Called as the first phase of a model
    /// download, and on its own when a model is selected on a device
    /// whose manifest predates the runtime download (models installed,
    /// runtime missing).
    pub fn ensure_runtime(
        &self,
        progress: &mut dyn FnMut(u64, u64),
    ) -> Result<PathBuf, VaultError> {
        // One install at a time: concurrent callers would otherwise
        // stream to the same `.part` archive, rename it over each
        // other, and race the manifest's runtime record. The second
        // caller re-checks inside the lock and reuses the first
        // caller's install.
        let _runtime_guard = self.lock_runtime();
        if let Some(path) = self.runtime_path() {
            return Ok(path);
        }
        let Some(descriptor) = current_runtime() else {
            return Err(VaultError::Model(format!(
                "no ONNX Runtime build is catalogued for {}-{}",
                std::env::consts::OS,
                std::env::consts::ARCH
            )));
        };

        let version_dir = self.runtime_version_dir(descriptor.version);
        fs::create_dir_all(&version_dir)
            .map_err(|e| VaultError::Model(format!("create runtime dir: {e}")))?;
        let archive_path = version_dir.join("onnxruntime.whl.part");
        let lib_path = version_dir.join(descriptor.lib_file_name);

        let installed = (|| -> Result<InstalledRuntime, VaultError> {
            let (mut reader, total) =
                Self::open_download(descriptor.archive_url, descriptor.size_bytes)?;
            let (digest, downloaded) = Self::stream_hash_to_file(
                &mut reader,
                total,
                progress,
                &archive_path,
                "runtime archive",
            )?;
            if !digest.eq_ignore_ascii_case(descriptor.sha256) {
                return Err(VaultError::Model(format!(
                    "runtime integrity check failed: expected SHA-256 {}, got {digest}",
                    descriptor.sha256
                )));
            }
            Self::extract_zip_entry(&archive_path, descriptor.archive_entry, &lib_path)?;
            Ok(InstalledRuntime {
                version: descriptor.version.to_string(),
                lib_file_name: descriptor.lib_file_name.to_string(),
                sha256: digest,
                size_bytes: downloaded,
                installed_at: chrono::Utc::now().to_rfc3339(),
            })
        })();
        if let Err(e) = installed {
            if let Err(cleanup) = fs::remove_file(&archive_path) {
                tracing::warn!("failed to remove partial runtime archive: {cleanup}");
            }
            return Err(e);
        }
        let installed = installed?;
        if let Err(e) = fs::remove_file(&archive_path) {
            tracing::warn!("failed to remove runtime archive after extraction: {e}");
        }

        let _guard = self.lock_manifest();
        let mut manifest = self.manifest();
        manifest.runtime = Some(installed);
        self.write_manifest(&manifest)?;
        Ok(lib_path)
    }

    /// Extract one entry of a ZIP archive (a wheel is a ZIP) to `dest`,
    /// via a `.part` file and an atomic rename, and mark it executable
    /// on Unix — a dlopen target needs the execute bit.
    ///
    /// The reader is hand-rolled on purpose: the archive reaching this
    /// point has already been SHA-256-verified against the pinned
    /// catalog hash, and a wheel entry is only ever stored or deflated,
    /// so the small reader below is all Stern needs — no ZIP
    /// dependency (the `zip` crate carried six `RustSec` advisories in
    /// its general-purpose extraction APIs, none of which Stern used).
    /// Entry CRCs are not checked; the whole-archive hash is the
    /// integrity guarantee. ZIP64 archives are rejected — a wheel is
    /// tens of megabytes, orders of magnitude below that limit.
    fn extract_zip_entry(
        archive_path: &Path,
        entry_name: &str,
        dest: &Path,
    ) -> Result<(), VaultError> {
        let bytes = fs::read(archive_path)
            .map_err(|e| VaultError::Model(format!("open runtime archive: {e}")))?;
        let (reader, uncompressed_size) = Self::zip_entry_data(&bytes, entry_name)?;
        let part_path = dest.with_extension("part");
        let result = Self::write_zip_entry(reader, uncompressed_size, &part_path, dest);
        if let Err(e) = result {
            // No orphaned `.part` library on any failure path (copy,
            // size check, flush, chmod, rename) — the same cleanup
            // discipline as the archive `.part` above.
            if let Err(cleanup) = fs::remove_file(&part_path) {
                if cleanup.kind() != std::io::ErrorKind::NotFound {
                    tracing::warn!("failed to remove partial runtime library: {cleanup}");
                }
            }
            return Err(e);
        }
        Ok(())
    }

    /// Stream one extracted entry to `part_path` (capped at the size
    /// the central directory declares, so a corrupt deflate stream
    /// cannot write unbounded data), mark it executable on Unix — a
    /// dlopen target needs the execute bit — and rename it onto
    /// `dest`. The caller removes `part_path` if this returns an
    /// error.
    fn write_zip_entry(
        reader: Box<dyn Read + '_>,
        uncompressed_size: u64,
        part_path: &Path,
        dest: &Path,
    ) -> Result<(), VaultError> {
        let mut out = fs::File::create(part_path)
            .map_err(|e| VaultError::Model(format!("create runtime library file: {e}")))?;
        let written = std::io::copy(&mut reader.take(uncompressed_size), &mut out)
            .map_err(|e| VaultError::Model(format!("extract runtime library: {e}")))?;
        if written != uncompressed_size {
            return Err(VaultError::Model(format!(
                "runtime library size mismatch: directory says {uncompressed_size} bytes, extracted {written}"
            )));
        }
        out.flush()
            .map_err(|e| VaultError::Model(format!("flush runtime library: {e}")))?;
        drop(out);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(part_path, fs::Permissions::from_mode(0o755))
                .map_err(|e| VaultError::Model(format!("chmod runtime library: {e}")))?;
        }
        fs::rename(part_path, dest)
            .map_err(|e| VaultError::Model(format!("move runtime library into place: {e}")))?;
        Ok(())
    }

    /// Locate `entry_name` in a ZIP archive's central directory and
    /// return a reader over its decompressed bytes plus the declared
    /// uncompressed size. Only stored (0) and deflated (8) entries are
    /// supported — the two a wheel can contain.
    // Offset arithmetic is inherent to parsing a binary format; every
    // offset is bounds-checked through the helpers below (a miss is a
    // `malformed` error, never a panic), and all directory fields are
    // u32/u16, so no offset can overflow a 64-bit usize.
    #[allow(clippy::arithmetic_side_effects)]
    fn zip_entry_data<'a>(
        archive: &'a [u8],
        entry_name: &str,
    ) -> Result<(Box<dyn Read + 'a>, u64), VaultError> {
        fn malformed(what: &str) -> VaultError {
            VaultError::Model(format!("read runtime archive: malformed {what}"))
        }
        fn u16_at(b: &[u8], at: usize) -> Option<u16> {
            b.get(at..at + 2)
                .and_then(|s| <[u8; 2]>::try_from(s).ok())
                .map(u16::from_le_bytes)
        }
        fn u32_at(b: &[u8], at: usize) -> Option<u32> {
            b.get(at..at + 4)
                .and_then(|s| <[u8; 4]>::try_from(s).ok())
                .map(u32::from_le_bytes)
        }
        fn sig_at(b: &[u8], at: usize, sig: [u8; 4]) -> bool {
            b.get(at..at + 4).is_some_and(|s| s == sig)
        }

        if archive.len() < 22 {
            return Err(malformed("archive (smaller than an end record)"));
        }
        // The end-of-central-directory record is the last thing in
        // the file; scan backwards for its signature (a trailing
        // comment can add up to 64 KiB after it).
        let window = archive.len().min(22 + usize::from(u16::MAX));
        let eocd = (archive.len() - window..=archive.len() - 22)
            .rev()
            .find(|&i| sig_at(archive, i, [0x50, 0x4b, 0x05, 0x06]))
            .ok_or_else(|| malformed("end of central directory"))?;
        let cd_count_raw =
            u16_at(archive, eocd + 10).ok_or_else(|| malformed("end of central directory"))?;
        let cd_count = usize::from(cd_count_raw);
        let cd_size =
            u32_at(archive, eocd + 12).ok_or_else(|| malformed("end of central directory"))?;
        let cd_offset =
            u32_at(archive, eocd + 16).ok_or_else(|| malformed("end of central directory"))?;
        // ZIP64 sentinel values in the end record: any of them means
        // the real value lives in a ZIP64 extension record, which
        // this reader does not parse.
        if cd_count_raw == u16::MAX || cd_size == u32::MAX || cd_offset == u32::MAX {
            return Err(VaultError::Model(
                "read runtime archive: ZIP64 archives are unsupported".to_string(),
            ));
        }
        let mut pos = cd_offset as usize;
        for _ in 0..cd_count {
            if !sig_at(archive, pos, [0x50, 0x4b, 0x01, 0x02]) {
                return Err(malformed("central directory"));
            }
            let fields = || -> Option<(u16, usize, usize, usize, usize, usize, usize)> {
                Some((
                    u16_at(archive, pos + 10)?,
                    u32_at(archive, pos + 20)? as usize,
                    u32_at(archive, pos + 24)? as usize,
                    usize::from(u16_at(archive, pos + 28)?),
                    usize::from(u16_at(archive, pos + 30)?),
                    usize::from(u16_at(archive, pos + 32)?),
                    u32_at(archive, pos + 42)? as usize,
                ))
            };
            let (method, compressed, uncompressed, name_len, extra_len, comment_len, local_offset) =
                fields().ok_or_else(|| malformed("central directory"))?;
            // ZIP64 sentinels in a central-directory entry: the real
            // sizes/offset would live in a ZIP64 extra field, which
            // this reader does not parse.
            if compressed == u32::MAX as usize
                || uncompressed == u32::MAX as usize
                || local_offset == u32::MAX as usize
            {
                return Err(VaultError::Model(
                    "read runtime archive: ZIP64 archives are unsupported".to_string(),
                ));
            }
            let name = archive
                .get(pos + 46..pos + 46 + name_len)
                .ok_or_else(|| malformed("central directory"))?;
            if name == entry_name.as_bytes() {
                if !sig_at(archive, local_offset, [0x50, 0x4b, 0x03, 0x04]) {
                    return Err(malformed("local file header"));
                }
                let local_name = usize::from(
                    u16_at(archive, local_offset + 26)
                        .ok_or_else(|| malformed("local file header"))?,
                );
                let local_extra = usize::from(
                    u16_at(archive, local_offset + 28)
                        .ok_or_else(|| malformed("local file header"))?,
                );
                let start = local_offset + 30 + local_name + local_extra;
                let data = archive
                    .get(start..start + compressed)
                    .ok_or_else(|| malformed("entry data"))?;
                let reader: Box<dyn Read> = match method {
                    0 => Box::new(std::io::Cursor::new(data)),
                    8 => Box::new(flate2::read::DeflateDecoder::new(std::io::Cursor::new(
                        data,
                    ))),
                    _ => {
                        return Err(VaultError::Model(format!(
                            "unsupported runtime archive compression: {method}"
                        )));
                    }
                };
                return Ok((reader, uncompressed as u64));
            }
            pos += 46 + name_len + extra_len + comment_len;
        }
        Err(VaultError::Model(format!(
            "runtime library missing from archive: {entry_name}"
        )))
    }

    /// Path of one installed model's verified file, if it is recorded in
    /// the manifest *and* still present on disk. This is the checked view
    /// [`ModelManager::status_for`] and [`ModelManager::active_model_path`]
    /// use; a manifest record whose file was deleted externally yields
    /// `None` here.
    #[must_use]
    pub fn installed_model_path(&self, model_id: &str) -> Option<PathBuf> {
        let installed = self.installed_model(model_id)?;
        let path = self.installed_file_path(&installed);
        path.exists().then_some(path)
    }

    /// Path of one installed model's tokenizer file, if the model is
    /// recorded in the manifest *and* the tokenizer is present on
    /// disk. Models installed before tokenizers were downloaded do
    /// not have one; the chat falls back to the rule classifier for
    /// them until the model is downloaded again.
    #[must_use]
    pub fn installed_tokenizer_path(&self, model_id: &str) -> Option<PathBuf> {
        let installed = self.installed_model(model_id)?;
        let path = self
            .installed_file_path(&installed)
            .with_file_name(TOKENIZER_FILE_NAME);
        path.exists().then_some(path)
    }

    /// Path of the active model's tokenizer file, if it is present
    /// on disk.
    #[must_use]
    pub fn active_tokenizer_path(&self) -> Option<PathBuf> {
        let installed = self.active_model()?;
        let path = self
            .installed_file_path(&installed)
            .with_file_name(TOKENIZER_FILE_NAME);
        path.exists().then_some(path)
    }

    /// Make an installed model the active one. The model's file must
    /// still be present on disk — a manifest record alone (e.g. after the
    /// `.onnx` was deleted externally) is not enough.
    pub fn set_active_model(&self, model_id: &str) -> Result<(), VaultError> {
        let _guard = self.lock_manifest();
        let mut manifest = self.manifest();
        let Some(installed) = manifest
            .models
            .iter()
            .find(|m| m.model_id == model_id)
            .cloned()
        else {
            return Err(VaultError::Model(format!(
                "model {model_id} is not installed"
            )));
        };
        if !self.installed_file_path(&installed).exists() {
            return Err(VaultError::Model(format!(
                "model {model_id} is recorded as installed but its file is missing"
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

    /// The HTTP agent for downloads, with explicit timeouts so a
    /// stalled download fails instead of hanging forever: connecting
    /// and receiving the response headers are short, and
    /// `timeout_per_read` restarts on every read, so a large file on a
    /// slow link is fine as long as bytes keep arriving — 60 seconds
    /// without a single successful read aborts the download.
    ///
    /// Certificates are verified against the operating system's trust
    /// store rather than ureq's bundled Mozilla roots, so a corporate
    /// TLS-inspecting proxy whose root CA the machine already trusts
    /// does not fail the download with `UnknownIssuer`. Integrity never
    /// depends on TLS: every file is checked against its pinned SHA-256.
    fn download_agent() -> ureq::Agent {
        ureq::Agent::config_builder()
            .tls_config(
                ureq::tls::TlsConfig::builder()
                    .root_certs(ureq::tls::RootCerts::PlatformVerifier)
                    .build(),
            )
            .timeout_connect(Some(Duration::from_secs(15)))
            .timeout_recv_response(Some(Duration::from_secs(30)))
            .timeout_per_read(Some(Duration::from_mins(1)))
            .build()
            .into()
    }

    /// Open an HTTPS download and return its body reader plus the total
    /// byte count (the `Content-Length` when the server sends a usable
    /// one, otherwise `fallback_total`).
    fn open_download(url: &str, fallback_total: u64) -> Result<(Box<dyn Read>, u64), VaultError> {
        let response = Self::download_agent()
            .get(url)
            .call()
            .map_err(|e| VaultError::Model(format!("download failed: {e}")))?;

        if !response.status().is_success() {
            return Err(VaultError::Model(format!(
                "download failed: HTTP {}",
                response.status()
            )));
        }

        let total = response
            .headers()
            .get("content-length")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok())
            .filter(|&n| n > 0)
            .unwrap_or(fallback_total);

        Ok((Box::new(response.into_body().into_reader()), total))
    }

    /// Stream `reader` into `part_path` while hashing, reporting
    /// progress. Returns the hex SHA-256 and the byte count; the caller
    /// compares the digest against its catalog pin.
    fn stream_hash_to_file(
        reader: &mut dyn Read,
        total_bytes: u64,
        progress: &mut dyn FnMut(u64, u64),
        part_path: &Path,
        what: &str,
    ) -> Result<(String, u64), VaultError> {
        let mut file = fs::File::create(part_path)
            .map_err(|e| VaultError::Model(format!("create {what} file: {e}")))?;

        let mut hasher = Sha256::new();
        let mut downloaded: u64 = 0;
        let mut buffer = vec![0u8; 64 * 1024].into_boxed_slice();
        loop {
            let n = reader
                .read(&mut buffer)
                .map_err(|e| VaultError::Model(format!("read {what} download: {e}")))?;
            if n == 0 {
                break;
            }
            let chunk = buffer
                .get(..n)
                .ok_or_else(|| VaultError::Model("read buffer overflow".to_string()))?;
            hasher.update(chunk);
            file.write_all(chunk)
                .map_err(|e| VaultError::Model(format!("write {what} file: {e}")))?;
            let n_u64 = u64::try_from(n)
                .map_err(|_| VaultError::Model("read size overflow".to_string()))?;
            downloaded = downloaded.saturating_add(n_u64);
            progress(downloaded, total_bytes);
        }
        file.flush()
            .map_err(|e| VaultError::Model(format!("flush {what} file: {e}")))?;
        drop(file);

        Ok((
            data_encoding::HEXLOWER.encode(&hasher.finalize()),
            downloaded,
        ))
    }

    /// Download the catalog model over HTTPS and install it, fetching
    /// the ONNX Runtime first if this device does not have it yet.
    ///
    /// `progress` is called with `(downloaded_bytes, total_bytes)` as
    /// the bytes stream in; the totals span all three phases (runtime,
    /// model, tokenizer), so the bar runs once from empty to full.
    /// These are the only network calls in Stern.
    pub fn download(
        &self,
        descriptor: &ModelDescriptor,
        progress: &mut dyn FnMut(u64, u64),
    ) -> Result<InstalledModel, VaultError> {
        // Phase 1: the runtime, if missing. Its size is folded into the
        // progress totals so the three phases read as one download.
        let runtime_size = match current_runtime() {
            Some(runtime) if self.runtime_path().is_none() => runtime.size_bytes,
            _ => 0,
        };
        let combined_total = runtime_size
            .saturating_add(descriptor.size_bytes)
            .saturating_add(descriptor.tokenizer_size_bytes);
        if runtime_size > 0 {
            self.ensure_runtime(&mut |downloaded, _| progress(downloaded, combined_total))?;
        }

        // Phase 2: the model itself, offset by the runtime bytes.
        let (mut reader, model_total) =
            Self::open_download(descriptor.download_url, descriptor.size_bytes)?;
        let installed = self.install_from_reader(
            descriptor,
            &mut reader,
            model_total,
            &mut |downloaded, _| {
                progress(runtime_size.saturating_add(downloaded), combined_total);
            },
        )?;

        // Phase 3: the tokenizer, beside the model file. Without it
        // the model cannot run, so a failure here fails the download
        // (the model stays installed; a retry re-fetches both).
        let tokenizer_base = runtime_size.saturating_add(model_total);
        let (mut reader, tokenizer_total) =
            Self::open_download(descriptor.tokenizer_url, descriptor.tokenizer_size_bytes)?;
        self.install_tokenizer(
            descriptor,
            &mut reader,
            tokenizer_total,
            &mut |downloaded, _| {
                progress(tokenizer_base.saturating_add(downloaded), combined_total);
            },
        )?;
        Ok(installed)
    }

    /// Install a model's tokenizer beside its model file: stream to a
    /// `.part` file while hashing, verify the SHA-256 against the
    /// catalog, then atomically move it into place. On any failure
    /// the partial file is removed.
    fn install_tokenizer(
        &self,
        descriptor: &ModelDescriptor,
        reader: &mut dyn Read,
        total_bytes: u64,
        progress: &mut dyn FnMut(u64, u64),
    ) -> Result<(), VaultError> {
        let version_dir = self.model_version_dir(descriptor.id, descriptor.version);
        fs::create_dir_all(&version_dir)
            .map_err(|e| VaultError::Model(format!("create model dir: {e}")))?;

        let part_path = version_dir.join(format!("{TOKENIZER_FILE_NAME}.part"));
        let final_path = version_dir.join(TOKENIZER_FILE_NAME);

        let result =
            Self::stream_hash_to_file(reader, total_bytes, progress, &part_path, "tokenizer")
                .and_then(|(digest, _)| {
                    if !digest.eq_ignore_ascii_case(descriptor.tokenizer_sha256) {
                        return Err(VaultError::Model(format!(
                            "tokenizer integrity check failed: expected SHA-256 {}, got {digest}",
                            descriptor.tokenizer_sha256
                        )));
                    }
                    fs::rename(&part_path, &final_path)
                        .map_err(|e| VaultError::Model(format!("move tokenizer into place: {e}")))
                });
        if let Err(e) = result {
            if let Err(cleanup) = fs::remove_file(&part_path) {
                tracing::warn!("failed to remove partial tokenizer file: {cleanup}");
            }
            return Err(e);
        }
        Ok(())
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
        let (digest, downloaded) =
            Self::stream_hash_to_file(reader, total_bytes, progress, part_path, "model")?;
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
        fs::write(&tmp_path, &json).map_err(|e| {
            // Best-effort cleanup so a failed write leaves no temp file.
            if let Err(cleanup) = fs::remove_file(&tmp_path) {
                tracing::warn!("failed to remove manifest temp file: {cleanup}");
            }
            VaultError::Model(format!("write model manifest: {e}"))
        })?;
        fs::rename(&tmp_path, self.manifest_path()).map_err(|e| {
            // A failed rename must not leave the temp file behind either.
            if let Err(cleanup) = fs::remove_file(&tmp_path) {
                tracing::warn!("failed to remove manifest temp file: {cleanup}");
            }
            VaultError::Model(format!("replace model manifest: {e}"))
        })?;
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
        if manifest.models.is_empty()
            && manifest.active_model_id.is_none()
            && manifest.runtime.is_none()
        {
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn runtime_catalog_covers_this_platform() {
        let runtime = current_runtime().expect("this platform must be catalogued");
        assert_eq!(runtime.version, RUNTIME_VERSION);
        assert_eq!(runtime.sha256.len(), 64);
        assert!(runtime.archive_entry.ends_with(
            runtime
                .lib_file_name
                .strip_prefix("libonnxruntime")
                .unwrap_or("")
        ));
        assert!(runtime.archive_entry.contains(RUNTIME_VERSION));
    }

    #[test]
    fn manifest_round_trips_the_runtime_record() {
        let manifest = ModelManifest {
            models: Vec::new(),
            active_model_id: None,
            runtime: Some(InstalledRuntime {
                version: RUNTIME_VERSION.to_string(),
                lib_file_name: "libonnxruntime.1.31.0.dylib".to_string(),
                sha256: "ab".repeat(32),
                size_bytes: 42,
                installed_at: "2026-11-22T00:00:00+00:00".to_string(),
            }),
        };
        let json = serde_json::to_string(&manifest).unwrap();
        let parsed: ModelManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, manifest);

        // A manifest written before the runtime record existed parses
        // with no runtime installed.
        let legacy: ModelManifest =
            serde_json::from_str(r#"{"models":[],"active_model_id":null}"#).unwrap();
        assert_eq!(legacy.runtime, None);
    }

    /// Build a minimal ZIP archive (local headers, central directory,
    /// end record) around `entries` — the test counterpart of the
    /// hand-rolled reader in `ModelManager::zip_entry_data`. Entry
    /// CRCs are zero: the reader does not check them (the production
    /// caller verifies the whole archive's SHA-256 first). Each entry
    /// is `(name, payload, deflate)`.
    fn build_test_wheel(entries: &[(&str, &[u8], bool)]) -> Vec<u8> {
        fn push_u16(out: &mut Vec<u8>, v: u16) {
            out.extend_from_slice(&v.to_le_bytes());
        }
        fn push_u32(out: &mut Vec<u8>, v: u32) {
            out.extend_from_slice(&v.to_le_bytes());
        }
        let mut out: Vec<u8> = Vec::new();
        let mut central: Vec<u8> = Vec::new();
        for (name, payload, deflate) in entries {
            let compressed = if *deflate {
                let mut enc =
                    flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::fast());
                enc.write_all(payload).unwrap();
                enc.finish().unwrap()
            } else {
                payload.to_vec()
            };
            let method: u16 = if *deflate { 8 } else { 0 };
            let local_offset = out.len() as u32;
            out.extend_from_slice(&[0x50, 0x4b, 0x03, 0x04]);
            push_u16(&mut out, 20);
            push_u16(&mut out, 0);
            push_u16(&mut out, method);
            push_u16(&mut out, 0);
            push_u16(&mut out, 0);
            push_u32(&mut out, 0);
            push_u32(&mut out, compressed.len() as u32);
            push_u32(&mut out, payload.len() as u32);
            push_u16(&mut out, name.len() as u16);
            push_u16(&mut out, 0);
            out.extend_from_slice(name.as_bytes());
            out.extend_from_slice(&compressed);
            central.extend_from_slice(&[0x50, 0x4b, 0x01, 0x02]);
            push_u16(&mut central, 20);
            push_u16(&mut central, 20);
            push_u16(&mut central, 0);
            push_u16(&mut central, method);
            push_u16(&mut central, 0);
            push_u16(&mut central, 0);
            push_u32(&mut central, 0);
            push_u32(&mut central, compressed.len() as u32);
            push_u32(&mut central, payload.len() as u32);
            push_u16(&mut central, name.len() as u16);
            push_u16(&mut central, 0);
            push_u16(&mut central, 0);
            push_u16(&mut central, 0);
            push_u16(&mut central, 0);
            push_u32(&mut central, 0);
            push_u32(&mut central, local_offset);
            central.extend_from_slice(name.as_bytes());
        }
        let cd_offset = out.len() as u32;
        out.extend_from_slice(&central);
        out.extend_from_slice(&[0x50, 0x4b, 0x05, 0x06]);
        push_u16(&mut out, 0);
        push_u16(&mut out, 0);
        push_u16(&mut out, entries.len() as u16);
        push_u16(&mut out, entries.len() as u16);
        push_u32(&mut out, central.len() as u32);
        push_u32(&mut out, cd_offset);
        push_u16(&mut out, 0);
        out
    }

    #[test]
    fn extract_zip_entry_writes_the_library_file() {
        let dir = tempfile::tempdir().unwrap();
        let archive_path = dir.path().join("runtime.whl");
        let payload = b"fake onnxruntime library bytes";
        // Real wheels deflate their entries; cover the deflated path,
        // a stored entry alongside it, and a missing entry.
        let wheel = build_test_wheel(&[
            (
                "onnxruntime/capi/libonnxruntime.1.31.0.dylib",
                payload,
                true,
            ),
            ("onnxruntime/__init__.py", b"# stub", false),
        ]);
        fs::write(&archive_path, &wheel).unwrap();
        let dest = dir.path().join("libonnxruntime.1.31.0.dylib");
        ModelManager::extract_zip_entry(
            &archive_path,
            "onnxruntime/capi/libonnxruntime.1.31.0.dylib",
            &dest,
        )
        .unwrap();
        assert_eq!(fs::read(&dest).unwrap(), payload);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&dest).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o755);
        }
        let stored_dest = dir.path().join("__init__.py");
        ModelManager::extract_zip_entry(&archive_path, "onnxruntime/__init__.py", &stored_dest)
            .unwrap();
        assert_eq!(fs::read(&stored_dest).unwrap(), b"# stub");
        assert!(ModelManager::extract_zip_entry(
            &archive_path,
            "onnxruntime/capi/absent.dylib",
            &dir.path().join("absent.dylib"),
        )
        .is_err());
    }

    #[test]
    fn runtime_path_rejects_a_stale_runtime_version() {
        let dir = tempfile::tempdir().unwrap();
        let manager = ModelManager::new(dir.path().join("models"));
        let descriptor = current_runtime().unwrap();
        // Record an older runtime in the manifest and place its file
        // on disk: it must NOT be handed to AiModel once the catalog
        // pins a different version.
        let stale = ModelManifest {
            models: Vec::new(),
            active_model_id: None,
            runtime: Some(InstalledRuntime {
                version: "0.0.0-stale".to_string(),
                lib_file_name: descriptor.lib_file_name.to_string(),
                sha256: "ab".repeat(32),
                size_bytes: 42,
                installed_at: "2026-11-22T00:00:00+00:00".to_string(),
            }),
        };
        manager.write_manifest(&stale).unwrap();
        let stale_dir = manager.runtime_version_dir("0.0.0-stale");
        fs::create_dir_all(&stale_dir).unwrap();
        fs::write(stale_dir.join(descriptor.lib_file_name), b"old library").unwrap();
        assert_eq!(manager.runtime_path(), None);

        // The catalog build itself, recorded and on disk, resolves.
        let current = ModelManifest {
            runtime: Some(InstalledRuntime {
                version: descriptor.version.to_string(),
                ..stale.runtime.clone().unwrap()
            }),
            ..stale
        };
        manager.write_manifest(&current).unwrap();
        let current_dir = manager.runtime_version_dir(descriptor.version);
        fs::create_dir_all(&current_dir).unwrap();
        let lib_path = current_dir.join(descriptor.lib_file_name);
        fs::write(&lib_path, b"current library").unwrap();
        assert_eq!(manager.runtime_path(), Some(lib_path));
    }

    #[test]
    fn zip_entry_data_rejects_zip64_sentinels() {
        let payload = b"fake onnxruntime library bytes";
        let wheel = build_test_wheel(&[("lib.dylib", payload, false)]);
        // Central-directory uncompressed-size sentinel (offset 24 in
        // the central header).
        let cd = wheel
            .windows(4)
            .position(|w| w == [0x50, 0x4b, 0x01, 0x02])
            .unwrap();
        let mut patched = wheel.clone();
        patched[cd + 24..cd + 28].copy_from_slice(&u32::MAX.to_le_bytes());
        let err = ModelManager::zip_entry_data(&patched, "lib.dylib")
            .err()
            .unwrap()
            .to_string();
        assert!(err.contains("ZIP64"), "unexpected error: {err}");
        // End-record entry-count sentinel (the end record is the
        // last 22 bytes; its total-count field is at +10).
        let mut patched = wheel;
        let eocd = patched.len() - 22;
        patched[eocd + 10..eocd + 12].copy_from_slice(&u16::MAX.to_le_bytes());
        let err = ModelManager::zip_entry_data(&patched, "lib.dylib")
            .err()
            .unwrap()
            .to_string();
        assert!(err.contains("ZIP64"), "unexpected error: {err}");
    }

    #[test]
    fn extract_failure_leaves_no_part_file() {
        let dir = tempfile::tempdir().unwrap();
        let archive_path = dir.path().join("runtime.whl");
        let payload = b"fake onnxruntime library bytes";
        let mut wheel = build_test_wheel(&[("lib.dylib", payload, false)]);
        // Lie about the uncompressed size so extraction fails its
        // size check after writing the .part file.
        let cd = wheel
            .windows(4)
            .position(|w| w == [0x50, 0x4b, 0x01, 0x02])
            .unwrap();
        let bigger = u32::try_from(payload.len() + 5).unwrap();
        wheel[cd + 24..cd + 28].copy_from_slice(&bigger.to_le_bytes());
        fs::write(&archive_path, &wheel).unwrap();
        let dest = dir.path().join("lib.dylib");
        assert!(ModelManager::extract_zip_entry(&archive_path, "lib.dylib", &dest).is_err());
        assert!(!dest.exists());
        assert!(!dest.with_extension("part").exists());
    }

    #[test]
    fn fresh_manager_has_no_runtime() {
        let dir = tempfile::tempdir().unwrap();
        let manager = ModelManager::new(dir.path().join("models"));
        assert_eq!(manager.runtime_path(), None);
    }
}
