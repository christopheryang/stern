// Copyright (c) 2026 Christopher Yang
// SPDX-License-Identifier: MIT

//! The note editor: a title field and a plain text area, shown in the
//! chat when a note is created or opened for editing. Chat starts and
//! finds notes; this is where their text is written and changed.
//!
//! The DTOs are local mirrors of `desktop/stern-tauri/src/notes.rs`,
//! because this crate does not depend on `stern-ipc`.

use super::friendly::friendly_error;
use leptos::prelude::*;
use serde::Deserialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use wasm_bindgen::JsCast;

#[derive(Clone, Debug, Deserialize)]
struct NoteDto {
    id: String,
    title: String,
    text: String,
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
async fn load_note(id: &str) -> Result<NoteDto, String> {
    let value = invoke("load_note", serde_json::json!({ "id": id })).await?;
    serde_json::from_value(value).map_err(|e| e.to_string())
}

#[allow(clippy::future_not_send)]
async fn save_note(id: Option<String>, title: String, text: String) -> Result<NoteDto, String> {
    let args = serde_json::json!({ "id": id, "title": title, "text": text });
    let value = invoke("save_note", args).await?;
    serde_json::from_value(value).map_err(|e| e.to_string())
}

/// The editor for one note. `note_id` is `None` for a new note.
/// `on_done` gets the saved note's title, or `None` when the user
/// cancels.
#[component]
pub fn NoteEditor(note_id: Option<String>, on_done: Callback<Option<String>>) -> impl IntoView {
    let current_id = RwSignal::new(note_id.clone());
    let title = RwSignal::new(String::new());
    let text = RwSignal::new(String::new());
    let status = RwSignal::new(Option::<String>::None);
    let busy = RwSignal::new(note_id.is_some());
    // Set when the editor goes away (the vault locks, the chat resets), so a
    // load or save that finishes late never writes to signals that are gone.
    let gone = Arc::new(AtomicBool::new(false));
    let gone_on_cleanup = Arc::clone(&gone);
    on_cleanup(move || gone_on_cleanup.store(true, Ordering::Relaxed));

    if let Some(id) = note_id {
        let gone = Arc::clone(&gone);
        leptos::task::spawn_local(async move {
            let loaded = load_note(&id).await;
            if gone.load(Ordering::Relaxed) {
                return;
            }
            match loaded {
                Ok(note) => {
                    current_id.set(Some(note.id));
                    title.set(note.title);
                    text.set(note.text);
                }
                Err(error) => status.set(Some(friendly_error(&error).message)),
            }
            busy.set(false);
        });
    }

    let save = move |_| {
        if busy.get_untracked() {
            return;
        }
        busy.set(true);
        status.set(None);
        let id = current_id.get_untracked();
        let title_value = title.get_untracked();
        let text_value = text.get_untracked();
        let gone = Arc::clone(&gone);
        leptos::task::spawn_local(async move {
            let saved = save_note(id, title_value, text_value).await;
            if gone.load(Ordering::Relaxed) {
                return;
            }
            match saved {
                Ok(note) => on_done.run(Some(note.title)),
                Err(error) => {
                    status.set(Some(friendly_error(&error).message));
                    busy.set(false);
                }
            }
        });
    };

    view! {
        <section class="note-editor" aria-label="Note editor">
            <label class="field-label" for="note-title">"Title"</label>
            <input
                id="note-title"
                class="input"
                type="text"
                autocomplete="off"
                prop:value=move || title.get()
                on:input=move |ev| title.set(event_target_value(&ev))
            />
            <label class="field-label" for="note-text">"Note"</label>
            <textarea
                id="note-text"
                class="input notes-text"
                rows="8"
                prop:value=move || text.get()
                on:input=move |ev| text.set(event_target_value(&ev))
            ></textarea>
            <div class="notes-actions">
                <button class="btn btn-primary" prop:disabled=move || busy.get() on:click=save>
                    "Save note"
                </button>
                <button class="btn" on:click=move |_| on_done.run(None)>"Cancel"</button>
                <span class="muted" role="status">{move || status.get().unwrap_or_default()}</span>
            </div>
        </section>
    }
}
