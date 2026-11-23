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

    /// Path of the installed ONNX Runtime shared library, if one is
    /// recorded in the manifest *and* still present on disk. This is
    /// the library `AiModel` dlopens; without it no model can load.
    #[must_use]
    pub fn runtime_path(&self) -> Option<PathBuf> {
        let runtime = self.manifest().runtime?;
        let path = self
            .runtime_version_dir(&runtime.version)
            .join(&runtime.lib_file_name);
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
    fn extract_zip_entry(
        archive_path: &Path,
        entry_name: &str,
        dest: &Path,
    ) -> Result<(), VaultError> {
        let file = fs::File::open(archive_path)
            .map_err(|e| VaultError::Model(format!("open runtime archive: {e}")))?;
        let mut archive = zip::ZipArchive::new(file)
            .map_err(|e| VaultError::Model(format!("read runtime archive: {e}")))?;
        let mut entry = archive
            .by_name(entry_name)
            .map_err(|e| VaultError::Model(format!("runtime library missing from archive: {e}")))?;
        let part_path = dest.with_extension("part");
        let mut out = fs::File::create(&part_path)
            .map_err(|e| VaultError::Model(format!("create runtime library file: {e}")))?;
        std::io::copy(&mut entry, &mut out)
            .map_err(|e| VaultError::Model(format!("extract runtime library: {e}")))?;
        out.flush()
            .map_err(|e| VaultError::Model(format!("flush runtime library: {e}")))?;
        drop(out);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&part_path, fs::Permissions::from_mode(0o755))
                .map_err(|e| VaultError::Model(format!("chmod runtime library: {e}")))?;
        }
        fs::rename(&part_path, dest)
            .map_err(|e| VaultError::Model(format!("move runtime library into place: {e}")))?;
        Ok(())
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
    fn download_agent() -> ureq::Agent {
        ureq::Agent::config_builder()
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
    /// the bytes stream in; the totals span both phases (runtime, then
    /// model), so the bar runs once from empty to full. These are the
    /// only network calls in Stern.
    pub fn download(
        &self,
        descriptor: &ModelDescriptor,
        progress: &mut dyn FnMut(u64, u64),
    ) -> Result<InstalledModel, VaultError> {
        // Phase 1: the runtime, if missing. Its size is folded into the
        // progress totals so the two phases read as one download.
        let runtime_size = match current_runtime() {
            Some(runtime) if self.runtime_path().is_none() => runtime.size_bytes,
            _ => 0,
        };
        if runtime_size > 0 {
            let combined = runtime_size.saturating_add(descriptor.size_bytes);
            self.ensure_runtime(&mut |downloaded, _| progress(downloaded, combined))?;
        }

        // Phase 2: the model itself, offset by the runtime bytes.
        let (mut reader, model_total) =
            Self::open_download(descriptor.download_url, descriptor.size_bytes)?;
        let combined_total = runtime_size.saturating_add(model_total);
        self.install_from_reader(
            descriptor,
            &mut reader,
            model_total,
            &mut |downloaded, _| {
                progress(runtime_size.saturating_add(downloaded), combined_total);
            },
        )
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

    #[test]
    fn extract_zip_entry_writes_the_library_file() {
        let dir = tempfile::tempdir().unwrap();
        let archive_path = dir.path().join("runtime.whl");
        let payload = b"fake onnxruntime library bytes";
        {
            let file = fs::File::create(&archive_path).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let options = zip::write::SimpleFileOptions::default();
            zip.start_file("onnxruntime/capi/libonnxruntime.1.31.0.dylib", options)
                .unwrap();
            zip.write_all(payload).unwrap();
            zip.finish().unwrap();
        }
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
    }

    #[test]
    fn fresh_manager_has_no_runtime() {
        let dir = tempfile::tempdir().unwrap();
        let manager = ModelManager::new(dir.path().join("models"));
        assert_eq!(manager.runtime_path(), None);
    }
}
