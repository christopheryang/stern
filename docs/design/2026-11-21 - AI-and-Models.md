# AI and models

How Stern's chat works, and how the on-device models are delivered,
verified, selected, updated, and removed. Decided in ADR-0005
(rule-based classification first), ADR-0006 (post-install model
download), ADR-0007 (user-selectable models), and ADR-0008 (the
runtime is downloaded on demand too, chat requires a model, and the
UI is chat-only).

## Chat pipeline

```
user message
  → classify_intent()        rule-based keyword scoring (ai/intent.rs)
  → Intent                   Store | Retrieve | List | Update | Delete |
                             CreateVault | UnlockVault | ExportVault |
                             ImportVault | Help | Unknown
  → ChatHandler              maps intent → response text + Action
  → shell                    performs the Action via native UI flows
```

The rule-based classifier is deterministic, fully testable, and has no
dependencies — it is the production classifier today. The ONNX model
(below) is downloaded, verified, and loaded into an inference session
held by the `ChatHandler`, ready to take over classification; routing
real traffic through it ships only when it beats the rules on the intent
test corpus. Destructive intents (delete) require confirmation in the UI
regardless of which classifier produced them, and secret values are
never part of chat (see [2026-11-21 - Security.md](2026-11-21%20-%20Security.md)).

The chat itself is gated (ADR-0008): the app ships with no model and
no runtime, so until the user has downloaded a model and it is active,
the chat area shows a model-required gate that routes to Settings.
Picking the first model is the first-run setup step.

## The model

- **Catalog (ADR-0007):** the user picks among four pinned models —
  SmolLM-135M int8 (~136 MB), SmolLM2-135M-Instruct q4f16 (~118 MB),
  SmolLM2-360M-Instruct q4f16 (~273 MB), Qwen2.5-0.5B-Instruct q4f16
  (~483 MB). Each entry carries a plain-language description of its
  size/quality trade-off, shown in the picker.
- **Runtime:** ONNX Runtime via the `ort` crate, built with
  `load-dynamic` — the runtime is **not linked into the binary**
  (ADR-0008). `AiModel` dlopens the downloaded library once per
  process (`ort::init_from`) and wraps the session, loaded from a
  file path with `commit_from_file`. The runtime itself is a pinned
  download like the models: see "The runtime" under Delivery.
- **Why so small:** the job is classifying a sentence into a handful of
  intents, not open-ended generation — but phrasing varies, and the
  larger entries follow instructions measurably better. Letting the
  user choose (rather than shipping one compromise) is the point of
  ADR-0007.

## Delivery: post-install download (ADR-0006)

The model is **not bundled** with the app. Bundling ~136 MB into every
installer and every app update — for an optional component that improves
on a schedule independent of the app — was rejected in favor of a
managed download:

- The **catalog** is compiled into `stern-core`
  (`ai/model_manager.rs::MODEL_CATALOG`): id, display name, description,
  version, file name, download URL, **pinned SHA-256**, approximate
  size. The hash is authoritative; the size is for the UI's "~273 MB"
  labels.
- The Settings view (opened from the gear button in the top bar — the
  UI is a single chat view, no sidebar) holds a model picker in the
  style of Meetily's model manager: one row per catalog model with
  its description and size, a Download button, and — once installed —
  Use this model and Remove. Nothing downloads automatically, ever.
- Storage: `<app_data_dir>/models/<model-id>/<version>/<file>`, plus an
  `installed.json` manifest recording every installed model (id,
  version, file name, size, hash), the `active_model_id`, and the
  installed runtime (below). Several models may be installed at once;
  exactly one is active.

### The runtime

The ONNX Runtime library is a pinned download too (ADR-0008).
`RUNTIME_CATALOG` in the same module pins one build per shipped
platform — ONNX Runtime 1.31.0 (the version matching the `ort`
crate's api-24) for macOS arm64 and Linux x86_64 — distributed as the
official `onnxruntime` wheel from PyPI, pinned by the SHA-256 the
PyPI JSON API publishes. The first model download fetches the wheel
first (the progress bar spans both phases), verifies its hash,
extracts exactly one entry — the shared library — into
`<app_data_dir>/models/runtime/<version>/`, deletes the archive, and
records the install in the manifest. Selecting an already-installed
model on a device whose manifest predates the runtime record fetches
the runtime as part of that click (with progress, through the same
progress slot a model download uses). A recorded runtime whose
version does not match the pinned catalog version counts as not
installed and is fetched again. A runtime upgrade takes effect on
the next app start; the library initializes once per process — a
failed initialization is not cached, so a repaired or re-downloaded
runtime loads on the next attempt without a restart. Before calling
into ort, the loader checks the library file exists and fails with
its own error if it does not — the clearer message is worth it, and
in ort rc.12's `load-dynamic` build the check was load-bearing: a
failed library load deadlocked inside ort's error construction
instead of returning an error. ort rc.15 fixed that upstream (load
failures return a plain `LoadError`), so a present-but-corrupt
library now simply errors. rc.15 also made the environment
explicit: `init_from(..).build()` yields an `Environment` the app
holds for the process lifetime, and sessions are built against it
(`Session::builder(&env)`).

### Install integrity

1. The file streams to `model.onnx.part` while a SHA-256 is computed
   incrementally; progress `(downloaded, total)` is reported to the UI.
2. The digest must equal the catalog's pinned hash. On mismatch the
   `.part` file is deleted and the previous state — including a working
   older model — is untouched.
3. On match, the file is renamed into place (atomic on the same
   filesystem), the manifest is written via temp-file + rename, and
   older version directories for that model are pruned.
4. If the installed model is (or becomes) the active one, the shell
   loads it into the running app (hot-load via the `ChatHandler`'s
   `RwLock`); no restart is needed.

![Model download and verification](2026-11-21%20-%20AI-and-Models%20-%20Model%20Download%20Sequence.png)

*A download installs only after the pinned SHA-256 matches; any failure deletes the partial file and leaves the previous state untouched.*

### Updates

Update detection is a version comparison between the manifest and the
catalog: when a new Stern build ships a catalog entry with a newer
`version`, the card reports **update available** and offers the upgrade.
The upgrade is the same verified pipeline as a fresh install — new file
first, swap only after verification — so a failed or corrupted download
never breaks a working install. Shipping a model update is therefore a
code change to the catalog (see AGENTS.md, "Shipping a model update"),
which keeps every distributed model hash reviewed and pinned in source
control rather than fetched from a remote config.

### Selection

The first model a user installs becomes active automatically.
Installing another model never changes the selection by itself —
switching is the explicit **Use this model** action, which records the
choice in the manifest and hot-swaps the loaded model. Trying a bigger
model is therefore risk-free: the previous one stays installed and one
click away.

### Removal and absence

Each row's **Remove** action deletes that model's directory tree and
manifest entry; removing the active model unloads it and leaves no
model active. With no model installed (or none active), the app shows
the model-required gate: chat — the app's interface to the vault — is
unavailable until the user picks a model in Settings (ADR-0008). The
runtime library stays installed when models are removed; it is shared
by all of them. Removing models is the way to reclaim disk space; the
app never re-downloads anything unprompted.

![Model lifecycle](2026-11-21%20-%20AI-and-Models%20-%20Model%20Lifecycle.png)

*One catalog model's states. Several models may be installed at once; exactly one is active, and removing the active one leaves none.*

## Network surface

The downloads (ureq over HTTPS — models from the catalog URLs,
currently huggingface.co; the runtime wheel from PyPI) are the
**only** network requests Stern makes. They happen only when the user
clicks Download/Update, or selects a model on a device that still
needs the runtime. No telemetry, no update checks, no remote
configuration accompany them, and vault data is never transmitted.

TLS certificates are verified against the **operating system's trust
store** (ureq's `platform-verifier`), not a bundled root list, so
downloads work behind corporate TLS-inspecting proxies whose root CA
the machine already trusts. This does not weaken integrity: every
downloaded file is checked against its pinned SHA-256 before use, so
a tampered download is rejected whatever the transport trusted.

