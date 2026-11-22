# AI and models

How Stern's chat works, and how the optional on-device models are
delivered, verified, selected, updated, and removed. Decided in
ADR-0005 (rule-based classification first), ADR-0006 (post-install
model download), and ADR-0007 (user-selectable models).

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

## The model

- **Catalog (ADR-0007):** the user picks among four pinned models —
  SmolLM-135M int8 (~136 MB), SmolLM2-135M-Instruct q4f16 (~118 MB),
  SmolLM2-360M-Instruct q4f16 (~273 MB), Qwen2.5-0.5B-Instruct q4f16
  (~483 MB). Each entry carries a plain-language description of its
  size/quality trade-off, shown in the picker.
- **Runtime:** ONNX Runtime via the `ort` crate; `AiModel` wraps the
  session and loads from a file path with `commit_from_file`.
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
- The chat view's card is a model picker in the style of Meetily's
  model manager: one row per catalog model with its description and
  size, a Download button, and — once installed — Use this model and
  Remove. Nothing downloads automatically, ever.
- Storage: `<app_data_dir>/models/<model-id>/<version>/<file>`, plus an
  `installed.json` manifest recording every installed model (id,
  version, file name, size, hash) and the `active_model_id`. Several
  models may be installed at once; exactly one is active.

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
model active. With no model installed (or none active), the app is
fully functional: the rule-based classifier covers every intent.
Removing models is also the way to reclaim disk space; the app never
re-downloads anything unprompted.

![Model lifecycle](2026-11-21%20-%20AI-and-Models%20-%20Model%20Lifecycle.png)

*One catalog model's states. Several models may be installed at once; exactly one is active, and removing the active one leaves none.*

## Network surface

The download (ureq over HTTPS, from the catalog URL — currently
huggingface.co) is the **only** network request Stern makes. It happens
only when the user clicks Download/Update. No telemetry, no update
checks, no remote configuration accompany it, and vault data is never
transmitted.

