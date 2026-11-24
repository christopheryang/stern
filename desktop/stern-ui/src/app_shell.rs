// Copyright (c) 2026 Christopher Yang
// SPDX-License-Identifier: MIT

//! App layout: a single chat view under a slim top bar — the Stern
//! logo, a vault status pill, and Lock / Settings buttons. There is
//! no sidebar. All styling lives in `styles/stern.css`.
//!
//! Two gates stand in front of the chat, in this order:
//!
//! 1. The vault. First run shows the setup screen (choose a master
//!    password); every later launch shows the login screen until
//!    the vault is unlocked. Nothing else — not even Settings — is
//!    reachable before that.
//! 2. The model. Stern ships with no AI model (and no ONNX
//!    Runtime) at all, so until the user has downloaded a model and
//!    it is the active one, the chat area shows a model-required
//!    gate that routes to the configuration view, where the model
//!    picker lives. The gate check is `model_status`: an active
//!    model whose runtime is installed.

use crate::components::auth::{AuthPhase, LoginView, SetupView};
use crate::components::chat::ChatView;
use crate::components::config::ConfigView;
use crate::components::ui::{BrandLogo, Icon, IconKind, Loading};
use leptos::prelude::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;

#[allow(clippy::future_not_send)]
async fn invoke_model_status() -> Result<serde_json::Value, String> {
    let args = serde_json::json!({});
    let args_js = serde_wasm_bindgen::to_value(&args).map_err(|e| e.to_string())?;

    let window = web_sys::window().ok_or("no window")?;
    let tauri: JsValue = js_sys::Reflect::get(&window, &JsValue::from_str("__TAURI__"))
        .map_err(|e| format!("no __TAURI__: {e:?}"))?;
    if tauri.is_undefined() {
        return Err("window.__TAURI__ is undefined — not running in Tauri?".into());
    }
    let core: JsValue = js_sys::Reflect::get(&tauri, &JsValue::from_str("core"))
        .map_err(|e| format!("no core: {e:?}"))?;
    if core.is_undefined() {
        return Err("window.__TAURI__.core is undefined".into());
    }
    let invoke_fn = js_sys::Reflect::get(&core, &JsValue::from_str("invoke"))
        .map_err(|e| format!("no invoke: {e:?}"))?;
    let invoke_fn: js_sys::Function = invoke_fn
        .dyn_into()
        .map_err(|_| "invoke is not a function")?;

    let result = invoke_fn
        .call2(&core, &JsValue::from_str("model_status"), &args_js)
        .map_err(|e| format!("invoke error: {e:?}"))?;

    let result = wasm_bindgen_futures::JsFuture::from(js_sys::Promise::resolve(&result))
        .await
        .map_err(|e| format!("promise error: {e:?}"))?;

    serde_wasm_bindgen::from_value(result).map_err(|e| e.to_string())
}

/// True when some catalog model is active *and* the ONNX Runtime is
/// installed — the two things chat needs before it can run a model.
/// An invoke or parse failure is an `Err`, not "not ready": the
/// caller surfaces it instead of leaving the UI on "Loading…" with
/// no way out.
#[allow(clippy::future_not_send)]
async fn fetch_model_ready() -> Result<bool, String> {
    let value = invoke_model_status().await?;
    let rows = value
        .as_array()
        .ok_or_else(|| "model status was not a list".to_string())?;
    Ok(rows.iter().any(|row| {
        row.get("is_active").and_then(serde_json::Value::as_bool) == Some(true)
            && row
                .get("runtime_installed")
                .and_then(serde_json::Value::as_bool)
                == Some(true)
    }))
}

fn spawn_vault_refresh(
    set_auth: WriteSignal<Option<AuthPhase>>,
    set_auth_error: WriteSignal<Option<String>>,
    cancelled: Arc<AtomicBool>,
) {
    spawn_local(async move {
        match crate::components::auth::fetch_vault_phase().await {
            Ok(phase) => {
                if !cancelled.load(Ordering::Relaxed) {
                    set_auth_error.set(None);
                    set_auth.set(Some(phase));
                }
            }
            Err(error) => {
                if !cancelled.load(Ordering::Relaxed) {
                    set_auth_error.set(Some(error));
                }
            }
        }
    });
}

fn spawn_refresh(
    set_model_ready: WriteSignal<Option<bool>>,
    set_status_error: WriteSignal<Option<String>>,
    cancelled: Arc<AtomicBool>,
) {
    spawn_local(async move {
        match fetch_model_ready().await {
            Ok(ready) => {
                if !cancelled.load(Ordering::Relaxed) {
                    set_status_error.set(None);
                    set_model_ready.set(Some(ready));
                }
            }
            Err(error) => {
                if !cancelled.load(Ordering::Relaxed) {
                    set_status_error.set(Some(error));
                }
            }
        }
    });
}

/// The gate shown when no model is ready: Stern ships without one,
/// so the first run routes the user to Settings to pick one.
#[component]
fn ModelRequiredGate(set_in_config: WriteSignal<bool>) -> impl IntoView {
    view! {
        <div class="stage">
            <div class="card">
                <div class="badge-icon"><Icon kind=IconKind::Chip /></div>
                <h2 class="title">"Choose an AI model to get started"</h2>
                <p class="subtitle">
                    "Stern ships without an AI model. Pick one in Settings \
                    and it downloads to this device only — then you can chat."
                </p>
                <div class="btn-row">
                    <button class="btn btn-primary" on:click=move |_| set_in_config.set(true)>
                        <Icon kind=IconKind::Download class="icon-sm" />
                        "Choose a model"
                    </button>
                </div>
            </div>
        </div>
    }
}

/// The gate shown when the model-status check itself failed: the
/// error and a retry, instead of a "Loading…" that never ends.
#[component]
fn StatusErrorGate(
    error: String,
    set_model_ready: WriteSignal<Option<bool>>,
    set_status_error: WriteSignal<Option<String>>,
    cancelled: Arc<AtomicBool>,
) -> impl IntoView {
    view! {
        <div class="stage">
            <div class="card">
                <div class="badge-icon badge-icon--danger"><Icon kind=IconKind::Alert /></div>
                <h2 class="title">"Couldn't check whether a model is ready"</h2>
                <p class="subtitle">{error}</p>
                <div class="btn-row">
                    <button
                        class="btn btn-primary"
                        on:click=move |_| {
                            set_status_error.set(None);
                            spawn_refresh(set_model_ready, set_status_error, cancelled.clone());
                        }
                    >
                        <Icon kind=IconKind::Refresh class="icon-sm" />
                        "Retry"
                    </button>
                </div>
            </div>
        </div>
    }
}

/// The gate shown when the vault-status check itself failed:
/// the error and a retry, instead of a "Loading…" that never ends.
#[component]
fn VaultStatusErrorGate(
    error: String,
    set_auth: WriteSignal<Option<AuthPhase>>,
    set_auth_error: WriteSignal<Option<String>>,
    cancelled: Arc<AtomicBool>,
) -> impl IntoView {
    view! {
        <div class="stage">
            <div class="card">
                <div class="badge-icon badge-icon--danger"><Icon kind=IconKind::Alert /></div>
                <h2 class="title">"Couldn't check the vault"</h2>
                <p class="subtitle">{error}</p>
                <div class="btn-row">
                    <button
                        class="btn btn-primary"
                        on:click=move |_| {
                            set_auth_error.set(None);
                            spawn_vault_refresh(set_auth, set_auth_error, cancelled.clone());
                        }
                    >
                        <Icon kind=IconKind::Refresh class="icon-sm" />
                        "Retry"
                    </button>
                </div>
            </div>
        </div>
    }
}

/// What the main area shows once the vault is unlocked: the
/// configuration view, the chat, or the model-required gate.
#[allow(clippy::too_many_arguments)]
fn unlocked_main(
    in_config: bool,
    model_ready: Option<bool>,
    status_error: Option<String>,
    set_in_config: WriteSignal<bool>,
    set_model_ready: WriteSignal<Option<bool>>,
    set_status_error: WriteSignal<Option<String>>,
    cancelled: Arc<AtomicBool>,
) -> AnyView {
    if in_config {
        view! { <ConfigView /> }.into_any()
    } else if model_ready == Some(true) {
        view! { <ChatView /> }.into_any()
    } else if model_ready == Some(false) {
        view! { <ModelRequiredGate set_in_config=set_in_config /> }.into_any()
    } else if let Some(error) = status_error {
        view! {
            <StatusErrorGate
                error=error
                set_model_ready=set_model_ready
                set_status_error=set_status_error
                cancelled=cancelled
            />
        }
        .into_any()
    } else {
        view! { <Loading /> }.into_any()
    }
}

/// The header's right-hand side: nothing before authentication,
/// a back button in Settings, and Lock + the gear in the chat.
#[allow(clippy::too_many_arguments)]
fn header_actions(
    auth: Option<AuthPhase>,
    in_config: bool,
    locking: bool,
    lock_error: Option<String>,
    set_in_config: WriteSignal<bool>,
    set_auth: WriteSignal<Option<AuthPhase>>,
    set_model_ready: WriteSignal<Option<bool>>,
    set_status_error: WriteSignal<Option<String>>,
    set_locking: WriteSignal<bool>,
    set_lock_error: WriteSignal<Option<String>>,
    cancelled: Arc<AtomicBool>,
) -> AnyView {
    if auth != Some(AuthPhase::Unlocked) {
        // No header actions before authentication: Settings (and
        // its model downloads) wait for an unlocked vault.
        return ().into_any();
    }
    if in_config {
        view! {
            <button
                class="btn btn-sm"
                on:click=move |_| {
                    set_in_config.set(false);
                    // The user may have just downloaded or
                    // selected a model — re-check the gate.
                    spawn_refresh(set_model_ready, set_status_error, cancelled.clone());
                }
            >
                <Icon kind=IconKind::ArrowLeft class="icon-sm" />
                "Back to chat"
            </button>
        }
        .into_any()
    } else {
        view! {
            <div class="topbar-actions">
                {lock_error.map(|error| view! { <span class="inline-error">{error}</span> })}
                <span class="pill">
                    <span class="pill-dot"></span>
                    "Vault unlocked"
                </span>
                <button
                    class="btn btn-sm"
                    prop:disabled=locking
                    on:click=move |_| {
                        if locking {
                            return;
                        }
                        set_locking.set(true);
                        set_lock_error.set(None);
                        spawn_local(async move {
                            match crate::components::auth::lock_vault().await {
                                Ok(()) => set_auth.set(Some(AuthPhase::Login)),
                                Err(error) => {
                                    set_lock_error.set(Some(format!(
                                        "Couldn't lock the vault: {error}"
                                    )));
                                    set_locking.set(false);
                                }
                            }
                        });
                    }
                >
                    <Icon kind=IconKind::Lock class="icon-sm" />
                    "Lock"
                </button>
                <button
                    class="btn-icon"
                    aria-label="Settings"
                    title="Settings"
                    on:click=move |_| set_in_config.set(true)
                >
                    <Icon kind=IconKind::Settings />
                </button>
            </div>
        }
        .into_any()
    }
}

#[component]
pub fn AppShell() -> impl IntoView {
    let (in_config, set_in_config) = signal(false);
    // The outermost gate: the vault. None = still checking. Until
    // this is Some(Unlocked), neither chat nor Settings (and so no
    // model download) is reachable — vault setup comes first.
    let (auth, set_auth) = signal(Option::<AuthPhase>::None);
    let (auth_error, set_auth_error) = signal(Option::<String>::None);
    // None = still checking. Chat stays gated until this is Some(true).
    let (model_ready, set_model_ready) = signal(Option::<bool>::None);
    // A failed status check, kept apart from `model_ready` so the UI
    // can show the error with a retry instead of "Loading…" forever.
    let (status_error, set_status_error) = signal(Option::<String>::None);
    // The Lock button's in-flight flag (a second click while a
    // lock is running must not start another) and its failure,
    // shown next to the button instead of swallowed.
    let (locking, set_locking) = signal(false);
    let (lock_error, set_lock_error) = signal(Option::<String>::None);
    // Set on unmount so a late status fetch never writes a dead
    // signal. An `Arc<AtomicBool>`, not a signal: the flag itself must
    // stay readable after this component is disposed, when reading a
    // signal it owned would access a disposed signal. (The
    // `Rc<Cell<bool>>` pattern `model_setup.rs` uses for its
    // per-action stop flags cannot serve here: `on_cleanup` requires
    // a Send + Sync closure.)
    let cancelled = Arc::new(AtomicBool::new(false));
    let cancelled_on_cleanup = cancelled.clone();
    on_cleanup(move || cancelled_on_cleanup.store(true, Ordering::Relaxed));

    spawn_vault_refresh(set_auth, set_auth_error, cancelled.clone());
    spawn_refresh(set_model_ready, set_status_error, cancelled.clone());
    // One clone per view closure below: each `move` closure takes
    // its own handle to the same flag.
    let cancelled_header = cancelled.clone();
    let cancelled_main = cancelled.clone();
    let cancelled_auth = cancelled;

    view! {
        <div class="app">
            <header class="topbar">
                <BrandLogo />
                {move || header_actions(
                    auth.get(),
                    in_config.get(),
                    locking.get(),
                    lock_error.get(),
                    set_in_config,
                    set_auth,
                    set_model_ready,
                    set_status_error,
                    set_locking,
                    set_lock_error,
                    cancelled_header.clone(),
                )}
            </header>
            <main class="main">
                {move || -> AnyView {
                    match auth.get() {
                        Some(AuthPhase::Setup) => {
                            view! { <SetupView set_auth=set_auth /> }.into_any()
                        }
                        Some(AuthPhase::Login) => {
                            view! { <LoginView set_auth=set_auth /> }.into_any()
                        }
                        Some(AuthPhase::Unlocked) => unlocked_main(
                            in_config.get(),
                            model_ready.get(),
                            status_error.get(),
                            set_in_config,
                            set_model_ready,
                            set_status_error,
                            cancelled_main.clone(),
                        ),
                        None => auth_error.get().map_or_else(
                            || view! { <Loading /> }.into_any(),
                            |error| {
                                view! {
                                    <VaultStatusErrorGate
                                        error=error
                                        set_auth=set_auth
                                        set_auth_error=set_auth_error
                                        cancelled=cancelled_auth.clone()
                                    />
                                }.into_any()
                            },
                        ),
                    }
                }}
            </main>
        </div>
    }
}
