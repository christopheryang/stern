// Copyright (c) 2026 Christopher Yang
// SPDX-License-Identifier: MIT

//! The note editor: a list of notes and a plain text area to write
//! and change them. Chat creates and finds notes; this page edits them.
//!
//! The DTOs are local mirrors of `desktop/stern-tauri/src/notes.rs`,
//! because this crate does not depend on `stern-ipc`.

use leptos::prelude::*;
use serde::{Deserialize, Serialize};
use wasm_bindgen::JsCast;

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct NoteSummary {
    id: String,
    title: String,
}

#[derive(Clone, Debug, Deserialize)]
struct NoteDto {
    id: String,
    title: String,
    text: String,
    version: u64,
}

#[derive(Serialize)]
struct SaveRequest<'a> {
    id: Option<&'a str>,
    title: &'a str,
    text: &'a str,
}

#[allow(clippy::future_not_send)]
async fn invoke(name: &str, args: serde_json::Value) -> Result<serde_json::Value, String> {
    let args_js = serde_wasm_bindgen::to_value(&args).map_err(|e| e.to_string())?;
    let window = web_sys::window().ok_or("no window")?;
    let tauri = js_sys::Reflect::get(&window, &wasm_bindgen::JsValue::from_str("__TAURI__"))
        .map_err(|e| format!("no __TAURI__: {e:?}"))?;
    let core = js_sys::Reflect::get(&tauri, &wasm_bindgen::JsValue::from_str("core"))
        .map_err(|e| format!("no core: {e:?}"))?;
    let invoke_fn: js_sys::Function =
        js_sys::Reflect::get(&core, &wasm_bindgen::JsValue::from_str("invoke"))
            .map_err(|e| format!("no invoke: {e:?}"))?
            .dyn_into()
            .map_err(|_| "invoke is not a function")?;
    let result = invoke_fn
        .call2(&core, &wasm_bindgen::JsValue::from_str(name), &args_js)
        .map_err(|e| format!("invoke error: {e:?}"))?;
    let result = wasm_bindgen_futures::JsFuture::from(js_sys::Promise::resolve(&result)).await;
    match result {
        Ok(value) => serde_wasm_bindgen::from_value(value).map_err(|e| e.to_string()),
        Err(rejection) => Err(rejection
            .as_string()
            .unwrap_or_else(|| "the command failed".to_string())),
    }
}

#[allow(clippy::future_not_send)]
async fn list_notes() -> Result<Vec<NoteSummary>, String> {
    let value = invoke("list_notes", serde_json::json!({})).await?;
    serde_json::from_value(value).map_err(|e| e.to_string())
}

#[allow(clippy::future_not_send)]
async fn load_note(id: &str) -> Result<NoteDto, String> {
    let value = invoke("load_note", serde_json::json!({ "id": id })).await?;
    serde_json::from_value(value).map_err(|e| e.to_string())
}

#[allow(clippy::future_not_send)]
async fn save_note(id: Option<String>, title: String, text: String) -> Result<NoteDto, String> {
    let request = SaveRequest {
        id: id.as_deref(),
        title: &title,
        text: &text,
    };
    let args = serde_json::json!({ "request": request });
    let value = invoke("save_note", args).await?;
    serde_json::from_value(value).map_err(|e| e.to_string())
}

/// The notes page: the list on the left, the editor beside it.
#[component]
#[allow(clippy::too_many_lines)]
pub fn NotesView() -> impl IntoView {
    let notes = RwSignal::new(Vec::<NoteSummary>::new());
    // `None` means a new note; `Some(id)` edits an existing note.
    let current_id = RwSignal::new(Option::<String>::None);
    let title = RwSignal::new(String::new());
    let text = RwSignal::new(String::new());
    let version = RwSignal::new(0u64);
    let status = RwSignal::new(Option::<String>::None);
    let saving = RwSignal::new(false);

    let refresh_list = move || {
        leptos::task::spawn_local(async move {
            match list_notes().await {
                Ok(list) => notes.set(list),
                Err(error) => status.set(Some(
                    crate::components::friendly::friendly_error(&error).message,
                )),
            }
        });
    };
    refresh_list();

    let open_note = move |id: String| {
        leptos::task::spawn_local(async move {
            match load_note(&id).await {
                Ok(note) => {
                    current_id.set(Some(note.id));
                    title.set(note.title);
                    text.set(note.text);
                    version.set(note.version);
                    status.set(None);
                }
                Err(error) => status.set(Some(
                    crate::components::friendly::friendly_error(&error).message,
                )),
            }
        });
    };

    let new_note = move |_| {
        current_id.set(None);
        title.set(String::new());
        text.set(String::new());
        version.set(0);
        status.set(None);
    };

    let save = move |_| {
        if saving.get_untracked() {
            return;
        }
        saving.set(true);
        status.set(None);
        let id = current_id.get_untracked();
        let title_value = title.get_untracked();
        let text_value = text.get_untracked();
        leptos::task::spawn_local(async move {
            match save_note(id, title_value, text_value).await {
                Ok(note) => {
                    current_id.set(Some(note.id));
                    version.set(note.version);
                    status.set(Some("Saved.".to_string()));
                    refresh_list();
                }
                Err(error) => status.set(Some(
                    crate::components::friendly::friendly_error(&error).message,
                )),
            }
            saving.set(false);
        });
    };

    view! {
        <div class="page">
            <div class="page-inner">
                <div class="page-head">
                    <h1 class="title">"Notes"</h1>
                </div>
        <section class="notes">
            <aside class="notes-list" aria-label="Your notes">
                <button class="btn btn-sm" on:click=new_note>"New note"</button>
                <ul>
                    <For
                        each=move || notes.get()
                        key=|note| note.id.clone()
                        children=move |note| {
                            let id = note.id.clone();
                            view! {
                                <li>
                                    <button class="btn-link" on:click=move |_| open_note(id.clone())>
                                        {note.title}
                                    </button>
                                </li>
                            }
                        }
                    />
                </ul>
            </aside>
            <div class="notes-editor">
                <label class="field-label" for="note-title">"Title"</label>
                <input
                    id="note-title"
                    class="input"
                    type="text"
                    prop:value=move || title.get()
                    on:input=move |ev| title.set(event_target_value(&ev))
                />
                <label class="field-label" for="note-text">"Note"</label>
                <textarea
                    id="note-text"
                    class="input notes-text"
                    rows="16"
                    prop:value=move || text.get()
                    on:input=move |ev| text.set(event_target_value(&ev))
                ></textarea>
                <div class="notes-actions">
                    <button class="btn btn-primary" prop:disabled=move || saving.get() on:click=save>
                        {move || if current_id.get().is_some() { "Save changes" } else { "Save note" }}
                    </button>
                    <span class="muted" role="status">
                        {move || status.get().unwrap_or_default()}
                    </span>
                </div>
            </div>
        </section>
            </div>
        </div>
    }
}
