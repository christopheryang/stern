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
#[allow(clippy::future_not_send)]
async fn fetch_model_ready() -> Option<bool> {
    let value = invoke_model_status().await.ok()?;
    let rows = value.as_array()?;
    Some(rows.iter().any(|row| {
        row.get("is_active").and_then(serde_json::Value::as_bool) == Some(true)
            && row
                .get("runtime_installed")
                .and_then(serde_json::Value::as_bool)
                == Some(true)
    }))
}

fn spawn_refresh(set_model_ready: WriteSignal<Option<bool>>, cancelled: ReadSignal<bool>) {
    spawn_local(async move {
        if let Some(ready) = fetch_model_ready().await {
            if !cancelled.get_untracked() {
                set_model_ready.set(Some(ready));
            }
        }
    });
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
    // Set on unmount so a late status fetch never writes a dead signal.
    let (cancelled, set_cancelled) = signal(false);
    on_cleanup(move || set_cancelled.set(true));

    spawn_refresh(set_model_ready, cancelled);

    view! {
        <div style="display:flex;flex-direction:column;height:100vh;">
            <header style=HEADER_STYLE>
                <h1 style="font-size:18px;font-weight:700;">"Stern"</h1>
                {move || -> AnyView {
                    if in_config.get() {
                        view! {
                            <button
                                style=BACK_BUTTON_STYLE
                                on:click=move |_| {
                                    set_in_config.set(false);
                                    // The user may have just downloaded or
                                    // selected a model — re-check the gate.
                                    spawn_refresh(set_model_ready, cancelled);
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
