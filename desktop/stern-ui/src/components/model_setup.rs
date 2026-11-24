// Copyright (c) 2026 Christopher Yang
// SPDX-License-Identifier: MIT

//! Post-install AI model picker — the heart of the Settings view.
//!
//! AI models are not bundled with Stern; they are downloaded after
//! install. Following the model-manager pattern popularized by Meetily,
//! this card lists every model in the catalog with its size and a plain
//! quality/speed note, and lets the user download, select, update, and
//! remove models. Exactly one installed model is active at a time;
//! until one is, the app gates the chat behind choosing a model (the
//! first download also fetches the ONNX Runtime — the app ships with
//! no ONNX code at all). While a download runs, the card polls
//! `model_status` for progress.

use super::ui::{ErrorAlert, Icon, IconKind};
use leptos::prelude::*;
use serde::{Deserialize, Serialize};
use std::cell::Cell;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;

/// Mirror of `stern_ipc::dto::ModelStatusDto` (stern-ui does not depend on
/// stern-ipc; DTOs are duplicated locally, as in `chat.rs`).
#[derive(Clone, Debug, Serialize, Deserialize)]
struct ModelStatus {
    model_id: String,
    display_name: String,
    description: String,
    /// One of `not_installed`, `installed`, `update_available`.
    state: String,
    installed_version: Option<String>,
    latest_version: String,
    size_bytes: u64,
    is_active: bool,
    is_downloading: bool,
    downloaded_bytes: u64,
    total_bytes: u64,
}

#[allow(clippy::future_not_send)]
async fn invoke_cmd(cmd: &str, args: serde_json::Value) -> Result<serde_json::Value, String> {
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
        .call2(&core, &JsValue::from_str(cmd), &args_js)
        .map_err(|e| format!("invoke error: {e:?}"))?;

    let result = wasm_bindgen_futures::JsFuture::from(js_sys::Promise::resolve(&result))
        .await
        .map_err(|e| format!("promise error: {e:?}"))?;

    serde_wasm_bindgen::from_value(result).map_err(|e| e.to_string())
}

#[allow(clippy::future_not_send)]
async fn fetch_status() -> Result<Vec<ModelStatus>, String> {
    let value = invoke_cmd("model_status", serde_json::json!({})).await?;
    serde_json::from_value(value).map_err(|e| e.to_string())
}

#[allow(clippy::future_not_send)]
async fn sleep_ms(ms: i32) {
    let promise = js_sys::Promise::new(&mut |resolve, _reject| {
        let Some(window) = web_sys::window() else {
            return;
        };
        let Ok(callback) = resolve.dyn_into::<js_sys::Function>() else {
            return;
        };
        let _timer = window.set_timeout_with_callback_and_timeout_and_arguments_0(&callback, ms);
    });
    let _result = wasm_bindgen_futures::JsFuture::from(promise).await;
}

/// Poll `model_status` while any download runs, mirroring progress into
/// the models signal. Stops when the download finishes, when it never
/// started (the download invoke reports that error itself), after
/// ~30 minutes, when the owning action's invoke returns (`stop` is set
/// by `run_action` the moment its `invoke_cmd` settles — so a fast
/// failure leaves no lingering poller behind to overlap a retry), or
/// when the component unmounts (`cancelled` is set by `on_cleanup`, so
/// no write ever lands on a disposed signal).
#[allow(clippy::future_not_send)]
async fn poll_download_progress(
    set_models: WriteSignal<Vec<ModelStatus>>,
    cancelled: ReadSignal<bool>,
    stop: Rc<Cell<bool>>,
) {
    let mut seen_downloading = false;
    let mut iterations: u32 = 0;
    loop {
        if cancelled.get_untracked() || stop.get() {
            break;
        }
        sleep_ms(400).await;
        if cancelled.get_untracked() || stop.get() {
            break;
        }
        iterations = iterations.saturating_add(1);
        match fetch_status().await {
            Ok(models) => {
                let downloading = models.iter().any(|m| m.is_downloading);
                if downloading {
                    seen_downloading = true;
                }
                let finished = seen_downloading && !downloading;
                if cancelled.get_untracked() {
                    break;
                }
                set_models.set(models);
                if finished || iterations > 4500 {
                    break;
                }
            }
            Err(_) => break,
        }
        if !seen_downloading && iterations > 25 {
            break;
        }
    }
}

/// First load on mount: fetch the status, and resume polling if a
/// download started before this mount (e.g. the user navigated away and
/// back while it was running on the backend).
#[allow(clippy::future_not_send)]
async fn load_initial_status(
    set_models: WriteSignal<Vec<ModelStatus>>,
    set_error: WriteSignal<Option<String>>,
    cancelled: ReadSignal<bool>,
) {
    match fetch_status().await {
        Ok(list) => {
            if cancelled.get_untracked() {
                return;
            }
            let resume = list.iter().any(|m| m.is_downloading);
            set_models.set(list);
            if resume {
                // No owning action on this mount: the stop flag is never
                // set, and the poller ends on finish/cancel as before.
                spawn_local(poll_download_progress(
                    set_models,
                    cancelled,
                    Rc::new(Cell::new(false)),
                ));
            }
        }
        Err(e) => {
            if !cancelled.get_untracked() {
                set_error.set(Some(e));
            }
        }
    }
}

fn percent(downloaded: u64, total: u64) -> u64 {
    if total == 0 {
        return 0;
    }
    downloaded
        .saturating_mul(100)
        .checked_div(total)
        .unwrap_or(0)
        .min(100)
}

fn megabytes(bytes: u64) -> u64 {
    bytes.checked_div(1_000_000).unwrap_or(0)
}

#[component]
fn ModelRow(
    status: ModelStatus,
    any_downloading: bool,
    on_download: Callback<String>,
    on_select: Callback<String>,
    on_remove: Callback<String>,
) -> impl IntoView {
    let id = status.model_id.clone();
    let meta_line = format!("~{} MB", megabytes(status.size_bytes));
    let update_note = (status.state == "update_available")
        .then(|| "An improved version is available.".to_string());

    let id_download = id.clone();
    let id_select = id.clone();
    let id_remove = id;
    let downloading = status.is_downloading;
    let pct = percent(status.downloaded_bytes, status.total_bytes);
    let bar_width = format!("width:{pct}%");
    let progress_label = format!(
        "Downloading — {} MB of {} MB ({}%)",
        megabytes(status.downloaded_bytes),
        megabytes(status.total_bytes),
        pct,
    );

    let row_class = if status.is_active {
        "model-row model-row--active"
    } else {
        "model-row"
    };

    view! {
        <div class=row_class>
            <div class="model-top">
                <div class="model-info">
                    <div class="model-name">
                        {status.display_name.clone()}
                        {status.is_active.then(|| view! {
                            <span class="tag">
                                <Icon kind=IconKind::Check class="icon-sm" />
                                "Active"
                            </span>
                        })}
                        <span class="model-meta">{meta_line}</span>
                    </div>
                    <div class="model-desc">{status.description.clone()}</div>
                    {update_note.map(|note| view! { <div class="model-update">{note}</div> })}
                </div>
                <div class="model-actions">
                    {(!downloading && !any_downloading).then(|| {
                        let state = status.state.clone();
                        let is_active = status.is_active;
                        view! {
                            <>
                                {(state == "not_installed").then(|| {
                                    let id = id_download.clone();
                                    view! {
                                        <button class="btn btn-sm btn-primary" on:click=move |_| on_download.run(id.clone())>
                                            <Icon kind=IconKind::Download class="icon-sm" />
                                            "Download"
                                        </button>
                                    }
                                })}
                                {(state == "update_available").then(|| {
                                    let id = id_download.clone();
                                    view! {
                                        <button class="btn btn-sm btn-primary" on:click=move |_| on_download.run(id.clone())>
                                            <Icon kind=IconKind::Refresh class="icon-sm" />
                                            "Update"
                                        </button>
                                    }
                                })}
                                {(state != "not_installed" && !is_active).then(|| {
                                    let id = id_select.clone();
                                    view! {
                                        <button class="btn btn-sm btn-primary" on:click=move |_| on_select.run(id.clone())>
                                            "Use this size"
                                        </button>
                                    }
                                })}
                                {(state != "not_installed").then(|| {
                                    let id = id_remove.clone();
                                    view! {
                                        <button class="btn btn-sm" on:click=move |_| on_remove.run(id.clone())>
                                            "Remove"
                                        </button>
                                    }
                                })}
                            </>
                        }
                    })}
                </div>
            </div>
            {downloading.then(|| {
                view! {
                    <div class="progress-label">{progress_label}</div>
                    <div class="progress" role="progressbar" aria-valuemin="0" aria-valuemax="100" aria-valuenow=pct>
                        <div class="progress-bar" style=bar_width></div>
                    </div>
                }
            })}
        </div>
    }
}

/// The picker's heading and explanation. Sizes are described in
/// plain language; the underlying models are never named (ADR-0010).
#[component]
fn SectionHead() -> impl IntoView {
    view! {
        <div class="section-head">
            <div class="section-title">
                <Icon kind=IconKind::Chip />
                "AI model size"
            </div>
            <p class="section-text">
                "Stern needs an AI model to chat. Choose a size and download it — \
                bigger understands more phrasings but is slower and takes more \
                space. The first download also fetches the ONNX Runtime (~21 MB), \
                and everything is stored only on this device. You can switch or \
                remove it at any time."
            </p>
        </div>
    }
}

#[component]
pub fn ModelSetup() -> impl IntoView {
    let (models, set_models) = signal(Vec::<ModelStatus>::new());
    let (error, set_error) = signal(Option::<String>::None);
    let (action_in_flight, set_action_in_flight) = signal(false);

    // Set by `on_cleanup` when the card unmounts (e.g. navigation), so
    // in-flight polling/fetches stop and never write to disposed signals.
    let (cancelled, set_cancelled) = signal(false);
    on_cleanup(move || set_cancelled.set(true));

    spawn_local(load_initial_status(set_models, set_error, cancelled));

    let run_action = move |cmd: &'static str, model_id: String, poll: bool| {
        // One in-flight guard for *every* action — not just downloads —
        // so repeated clicks cannot fire overlapping select/remove IPCs.
        if action_in_flight.get_untracked() {
            return;
        }
        set_action_in_flight.set(true);
        set_error.set(None);
        // Per-action stop flag for this action's poller: set the moment
        // the invoke settles, so even a fast failure leaves no poller
        // running to overlap a retry.
        let stop = Rc::new(Cell::new(false));
        spawn_local(async move {
            if poll {
                spawn_local(poll_download_progress(set_models, cancelled, stop.clone()));
            }
            let result = invoke_cmd(cmd, serde_json::json!({ "modelId": model_id })).await;
            stop.set(true);
            if !cancelled.get_untracked() {
                match result {
                    Ok(value) => {
                        if let Ok(list) = serde_json::from_value::<Vec<ModelStatus>>(value) {
                            set_models.set(list);
                        }
                    }
                    Err(e) => {
                        let label = if cmd == "download_model" {
                            format!("Download failed: {e}")
                        } else {
                            e
                        };
                        set_error.set(Some(label));
                        if let Ok(list) = fetch_status().await {
                            if !cancelled.get_untracked() {
                                set_models.set(list);
                            }
                        }
                    }
                }
            }
            set_action_in_flight.set(false);
        });
    };

    let on_download =
        Callback::new(move |model_id: String| run_action("download_model", model_id, true));
    // Select polls too: on a runtime-less device, selecting a model
    // first downloads the runtime (~21-24 MB), and that fetch now
    // reports through the same progress slot as a model download.
    let on_select =
        Callback::new(move |model_id: String| run_action("select_model", model_id, true));
    let on_remove =
        Callback::new(move |model_id: String| run_action("remove_model", model_id, false));

    view! {
        <div>
            {move || {
                let list = models.get();
                if list.is_empty() {
                    return view! { <div class="loading"><div class="spinner"></div></div> }.into_any();
                }
                // Buttons hide while a download *or any other action* is
                // in flight, matching the guard in `run_action`.
                let any_downloading =
                    list.iter().any(|m| m.is_downloading) || action_in_flight.get();
                let rows = list
                    .into_iter()
                    .map(|m| {
                        view! {
                            <ModelRow
                                status=m
                                any_downloading=any_downloading
                                on_download=on_download
                                on_select=on_select
                                on_remove=on_remove
                            />
                        }
                        .into_any()
                    })
                    .collect::<Vec<_>>();
                view! {
                    <section class="section">
                        <SectionHead />
                        {rows}
                    </section>
                }
                .into_any()
            }}
            {move || {
                error.get().map(|e| {
                    view! { <div class="settings-error"><ErrorAlert message=e /></div> }
                })
            }}
        </div>
    }
}
