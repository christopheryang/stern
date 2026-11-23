// Copyright (c) 2026 Christopher Yang
// SPDX-License-Identifier: MIT

//! App layout: a single chat view under a slim header — the app name
//! and a gear button that opens the configuration view. There is no
//! sidebar.
//!
//! Stern ships with no AI model (and no ONNX Runtime) at all, so until
//! the user has downloaded a model and it is the active one, there is
//! nothing to chat with: the chat area shows a model-required gate
//! that routes to the configuration view, where the model picker
//! lives. The gate check is `model_status`: an active model whose
//! runtime is installed.

use crate::components::chat::ChatView;
use crate::components::config::ConfigView;
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
        <div style="flex:1;display:flex;flex-direction:column;align-items:center;justify-content:center;gap:12px;padding:20px;text-align:center;">
            <div style="font-size:16px;color:#e6e6e6;">
                "Choose an AI model to get started"
            </div>
            <p style="color:#a0a0b0;font-size:14px;max-width:420px;line-height:1.5;margin:0;">
                "Stern ships without an AI model. Pick one in Settings \
                and it downloads to this device only — then you can chat."
            </p>
            <button
                style=PRIMARY_BUTTON_STYLE
                on:click=move |_| set_in_config.set(true)
            >
                "Choose a model"
            </button>
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
        <div style="flex:1;display:flex;flex-direction:column;align-items:center;justify-content:center;gap:12px;padding:20px;text-align:center;">
            <div style="font-size:16px;color:#e6e6e6;">
                "Couldn't check whether a model is ready"
            </div>
            <p style="color:#a0a0b0;font-size:14px;max-width:420px;line-height:1.5;margin:0;">
                {error}
            </p>
            <button
                style=PRIMARY_BUTTON_STYLE
                on:click=move |_| {
                    set_status_error.set(None);
                    spawn_refresh(set_model_ready, set_status_error, cancelled.clone());
                }
            >
                "Retry"
            </button>
        </div>
    }
}

const HEADER_STYLE: &str = "display:flex;align-items:center;justify-content:space-between;padding:12px 20px;background:#16213e;border-bottom:1px solid #2a2a4a;flex-shrink:0;";
const GEAR_BUTTON_STYLE: &str = "padding:6px 12px;border:1px solid #2a2a4a;border-radius:8px;background:transparent;color:#e6e6e6;font-size:16px;cursor:pointer;line-height:1;";
const BACK_BUTTON_STYLE: &str = "padding:6px 14px;border:1px solid #2a2a4a;border-radius:8px;background:transparent;color:#a0a0b0;font-size:13px;cursor:pointer;";
const PRIMARY_BUTTON_STYLE: &str = "padding:10px 20px;border:none;border-radius:8px;background:#e94560;color:white;font-size:14px;cursor:pointer;";

#[component]
pub fn AppShell() -> impl IntoView {
    let (in_config, set_in_config) = signal(false);
    // None = still checking. Chat stays gated until this is Some(true).
    let (model_ready, set_model_ready) = signal(Option::<bool>::None);
    // A failed status check, kept apart from `model_ready` so the UI
    // can show the error with a retry instead of "Loading…" forever.
    let (status_error, set_status_error) = signal(Option::<String>::None);
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

    spawn_refresh(set_model_ready, set_status_error, cancelled.clone());
    // One clone per view closure below: each `move` closure takes
    // its own handle to the same flag.
    let cancelled_header = cancelled.clone();
    let cancelled_main = cancelled;

    view! {
        <div style="display:flex;flex-direction:column;height:100vh;">
            <header style=HEADER_STYLE>
                <h1 style="font-size:18px;font-weight:700;">"Stern"</h1>
                {move || -> AnyView {
                    if in_config.get() {
                        let cancelled_back = cancelled_header.clone();
                        view! {
                            <button
                                style=BACK_BUTTON_STYLE
                                on:click=move |_| {
                                    set_in_config.set(false);
                                    // The user may have just downloaded or
                                    // selected a model — re-check the gate.
                                    spawn_refresh(
                                        set_model_ready,
                                        set_status_error,
                                        cancelled_back.clone(),
                                    );
                                }
                            >
                                "← Chat"
                            </button>
                        }.into_any()
                    } else {
                        view! {
                            <button
                                style=GEAR_BUTTON_STYLE
                                aria-label="Settings"
                                title="Settings"
                                on:click=move |_| set_in_config.set(true)
                            >
                                "⚙"
                            </button>
                        }.into_any()
                    }
                }}
            </header>
            <main style="flex:1;display:flex;flex-direction:column;overflow:hidden;">
                {move || -> AnyView {
                    if in_config.get() {
                        view! { <ConfigView /> }.into_any()
                    } else if model_ready.get() == Some(true) {
                        view! { <ChatView /> }.into_any()
                    } else if model_ready.get() == Some(false) {
                        view! {
                            <ModelRequiredGate set_in_config=set_in_config />
                        }.into_any()
                    } else if let Some(error) = status_error.get() {
                        view! {
                            <StatusErrorGate
                                error=error
                                set_model_ready=set_model_ready
                                set_status_error=set_status_error
                                cancelled=cancelled_main.clone()
                            />
                        }.into_any()
                    } else {
                        view! {
                            <div style="flex:1;display:flex;align-items:center;justify-content:center;color:#a0a0b0;font-size:14px;">
                                "Loading…"
                            </div>
                        }.into_any()
                    }
                }}
            </main>
        </div>
    }
}
