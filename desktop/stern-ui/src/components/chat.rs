use super::friendly::friendly_error;
use super::notes::NoteEditor;
use super::ui::{BrandMark, Icon, IconKind};
use leptos::ev::SubmitEvent;
use leptos::prelude::*;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ChatMsg {
    id: String,
    role: String,
    content: String,
    /// Tappable choices offered with an assistant message (the
    /// guided conversation's kinds, a disambiguation list, yes/no).
    #[serde(default)]
    options: Vec<String>,
    /// The reply to this message is a secret: the input switches
    /// to peek-mask mode while this is the newest message.
    #[serde(default)]
    input_sensitive: bool,
}

/// Which note the editor opens on.
#[derive(Clone, Debug, PartialEq, Eq)]
enum EditorRequest {
    /// A new, empty note.
    New,
    /// An existing note, by id.
    Existing(String),
}

/// What a chat reply carries besides its text.
struct Reply {
    message: String,
    options: Vec<String>,
    input_sensitive: bool,
    /// Set when the reply asks for the note editor.
    open_editor: Option<EditorRequest>,
}

impl Reply {
    const fn text(message: String) -> Self {
        Self {
            message,
            options: Vec::new(),
            input_sensitive: false,
            open_editor: None,
        }
    }
}

/// The note the reply's action asks the editor to open, if it does.
fn editor_request(action: Option<&str>) -> Option<EditorRequest> {
    let action = action?;
    if action == "open_note_editor" {
        return Some(EditorRequest::New);
    }
    action
        .strip_prefix("open_note_editor:")
        .map(|id| EditorRequest::Existing(id.to_string()))
}

/// Send one message and return the reply.
#[allow(clippy::future_not_send)]
async fn invoke_chat(content: String) -> Result<Reply, String> {
    let args = serde_json::json!({ "request": { "content": content } });
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
        .call2(&core, &JsValue::from_str("send_chat_message"), &args_js)
        .map_err(|e| format!("invoke error: {e:?}"))?;

    let result = wasm_bindgen_futures::JsFuture::from(js_sys::Promise::resolve(&result))
        .await
        .map_err(|e| format!("promise error: {e:?}"))?;

    let response: serde_json::Value =
        serde_wasm_bindgen::from_value(result).map_err(|e| e.to_string())?;
    let message = response
        .get("message")
        .and_then(|v| v.as_str())
        .map(String::from)
        .ok_or_else(|| "no message in response".to_string())?;
    // map_or_else, because the obvious forms are each rejected
    // somewhere: map(..).unwrap_or_default() by CI's clippy 1.100
    // (map_unwrap_or), map_or_default() as unstable on the local
    // 1.97 toolchain, and a match by option_if_let_else.
    let options = response
        .get("options")
        .and_then(|v| v.as_array())
        .map_or_else(Vec::new, |items| {
            items
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        });
    let input_sensitive = response
        .get("input_sensitive")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let open_editor = editor_request(response.get("action").and_then(|v| v.as_str()));
    Ok(Reply {
        message,
        options,
        input_sensitive,
        open_editor,
    })
}

/// How long a typed secret character stays visible, in
/// milliseconds, before it turns into a mask character.
const PEEK_MILLIS: f64 = 2_000.0;

/// The masked display for a secret: a character shows while its
/// reveal deadline is in the future, `*` afterwards. The mask is
/// character-for-character, so display positions are the real
/// value's positions.
fn mask_display(secret: &str, reveals: &[f64]) -> String {
    let now = js_sys::Date::now();
    secret
        .chars()
        .enumerate()
        .map(|(index, ch)| {
            if reveals.get(index).is_some_and(|until| now < *until) {
                ch
            } else {
                '*'
            }
        })
        .collect()
}

/// The splice an edit applies, in char indices: where it
/// starts, how many chars it removes, and the chars it inserts.
type Splice = (usize, usize, Vec<char>);

/// Locate an edit from the caret: after an edit the caret sits
/// right after the inserted text (or right where the deleted
/// text was), so the changed region is the one ending at the
/// caret. Unlike a prefix/suffix diff this stays unambiguous
/// when the typed character is a literal `*` next to masked
/// characters — the diff cannot tell those apart, the caret
/// can. Returns `None` when the caret does not verify (a
/// select-and-retype, say), leaving the diff as the fallback.
/// `caret_utf16` is the input's `selectionStart`, which counts
/// UTF-16 code units.
#[allow(clippy::arithmetic_side_effects)]
fn caret_splice(old: &[char], new: &[char], caret_utf16: Option<u32>) -> Option<Splice> {
    let caret16 = usize::try_from(caret_utf16?).ok()?;
    let mut units = 0_usize;
    let mut caret = None;
    for (index, ch) in new.iter().enumerate() {
        if units == caret16 {
            caret = Some(index);
            break;
        }
        units += ch.len_utf16();
    }
    if caret.is_none() && units == caret16 {
        caret = Some(new.len());
    }
    let caret = caret?;
    let delta = isize::try_from(new.len()).ok()? - isize::try_from(old.len()).ok()?;
    if delta >= 0 {
        let inserted_len = usize::try_from(delta).ok()?;
        let start = caret.checked_sub(inserted_len)?;
        // Untouched before the insertion, and the tail after the
        // caret is the old tail from the insertion point on.
        let before_untouched = new.get(..start) == old.get(..start);
        let tail_untouched = new.get(caret..) == old.get(start..);
        if before_untouched && tail_untouched {
            return Some((start, 0, new.get(start..caret)?.to_vec()));
        }
    } else {
        let removed_len = usize::try_from(-delta).ok()?;
        let before_untouched = new.get(..caret) == old.get(..caret);
        let tail_untouched = new.get(caret..) == old.get(caret + removed_len..);
        if before_untouched && tail_untouched {
            return Some((caret, removed_len, Vec::new()));
        }
    }
    None
}

/// Locate an edit by the common prefix/suffix of the two
/// display strings — the fallback when no caret verifies.
#[allow(clippy::arithmetic_side_effects)]
fn diff_splice(old: &[char], new: &[char]) -> Splice {
    let shared = old.len().min(new.len());
    let mut prefix = 0;
    while prefix < shared && old.get(prefix) == new.get(prefix) {
        prefix += 1;
    }
    let mut suffix = 0;
    while suffix < shared.saturating_sub(prefix)
        && old.get(old.len() - 1 - suffix) == new.get(new.len() - 1 - suffix)
    {
        suffix += 1;
    }
    let inserted: Vec<char> = new
        .get(prefix..new.len().saturating_sub(suffix))
        .unwrap_or(&[])
        .to_vec();
    let removed = old.len().saturating_sub(prefix).saturating_sub(suffix);
    (prefix, removed, inserted)
}

/// Fold one edit of the masked display back into the real
/// secret. The browser edits the display string; the caret (or,
/// failing that, the common prefix/suffix against the last
/// rendered display) locates the change, and inserted
/// characters are the real typed text. The index math is
/// bounded by the strings' own lengths, and the float math is
/// millisecond timestamps.
#[allow(clippy::arithmetic_side_effects, clippy::float_arithmetic)]
fn fold_secret_edit(
    shadow: &str,
    secret: &str,
    reveals: &[f64],
    new_display: &str,
    caret_utf16: Option<u32>,
) -> (String, Vec<f64>) {
    let old_display: Vec<char> = shadow.chars().collect();
    let new_chars: Vec<char> = new_display.chars().collect();
    let real: Vec<char> = secret.chars().collect();
    let (start, removed, inserted) = caret_splice(&old_display, &new_chars, caret_utf16)
        .unwrap_or_else(|| diff_splice(&old_display, &new_chars));
    let mut next_real: Vec<char> = real.get(..start).unwrap_or(&[]).to_vec();
    next_real.extend(inserted.iter());
    next_real.extend(real.get(start + removed..).unwrap_or(&[]));
    let now = js_sys::Date::now();
    let mut next_rev: Vec<f64> = reveals.get(..start).unwrap_or(&[]).to_vec();
    next_rev.extend(inserted.iter().map(|_| now + PEEK_MILLIS));
    next_rev.extend(reveals.get(start + removed..).unwrap_or(&[]));
    (next_real.iter().collect(), next_rev)
}

/// A counter that bumps five times a second, so the peek-mask
/// display re-renders as typed characters expire. The interval
/// is cleared when the owning component is disposed, and the
/// callback is held in a stored value the arena drops with it —
/// without that, every mount leaked a timer that kept writing
/// a disposed signal.
fn use_tick() -> ReadSignal<u32> {
    let (tick, set_tick) = signal(0_u32);
    let tick_cb = Closure::wrap(Box::new(move || {
        set_tick.update(|t| *t = t.wrapping_add(1));
    }) as Box<dyn FnMut()>);
    let stored_cb = StoredValue::new_local(tick_cb);
    if let Some(window) = web_sys::window() {
        let interval_id = stored_cb.with_value(|cb| {
            window.set_interval_with_callback_and_timeout_and_arguments_0(
                cb.as_ref().unchecked_ref(),
                200,
            )
        });
        if let Ok(interval_id) = interval_id {
            on_cleanup(move || {
                if let Some(window) = web_sys::window() {
                    window.clear_interval_with_handle(interval_id);
                }
            });
        }
    }
    tick
}

/// The one send path for typed messages and tapped choices
/// alike: a choice sends its label as the next message, which
/// the guided conversation and the disambiguation flow both
/// accept. The optional display text replaces the sent text in
/// the transcript — a secret is shown as a mask, never in clear.
fn make_sender(
    set_messages: WriteSignal<Vec<ChatMsg>>,
    loading: ReadSignal<bool>,
    set_loading: WriteSignal<bool>,
    editor: ReadSignal<Option<EditorRequest>>,
    set_editor: WriteSignal<Option<EditorRequest>>,
) -> Callback<(String, Option<String>)> {
    Callback::new(move |(text, display): (String, Option<String>)| {
        if text.trim().is_empty() || loading.get() {
            return;
        }
        let user_msg = ChatMsg {
            id: uuid::Uuid::new_v4().to_string(),
            role: "user".to_string(),
            content: display.unwrap_or_else(|| text.clone()),
            options: Vec::new(),
            input_sensitive: false,
        };
        set_messages.update(|msgs| msgs.push(user_msg));
        set_loading.set(true);

        spawn_local(async move {
            let mut reply = invoke_chat(text)
                .await
                .unwrap_or_else(|e| Reply::text(friendly_error(&e).message));

            // A note is already open: never swap it for another, which
            // would throw away what was typed into it. Ask to finish it.
            if reply.open_editor.is_some() && editor.get_untracked().is_some() {
                reply = Reply::text(
                    "A note is already open. Save it or cancel it first, then ask again."
                        .to_string(),
                );
            }
            let assistant_msg = ChatMsg {
                id: uuid::Uuid::new_v4().to_string(),
                role: "assistant".to_string(),
                content: reply.message,
                options: reply.options,
                input_sensitive: reply.input_sensitive,
            };
            set_messages.update(|msgs| msgs.push(assistant_msg));
            if let Some(request) = reply.open_editor {
                set_editor.set(Some(request));
            }
            set_loading.set(false);
        });
    })
}

/// The note editor for a request.
fn editor_view(request: EditorRequest, on_done: Callback<Option<String>>) -> AnyView {
    let note_id = match request {
        EditorRequest::New => None,
        EditorRequest::Existing(id) => Some(id),
    };
    view! { <NoteEditor note_id=note_id on_done=on_done /> }.into_any()
}

/// What happens when the note editor closes: it goes away and the
/// transcript says whether the note was saved or the user cancelled.
fn editor_done_callback(
    set_messages: WriteSignal<Vec<ChatMsg>>,
    set_editor: WriteSignal<Option<EditorRequest>>,
) -> Callback<Option<String>> {
    Callback::new(move |saved: Option<String>| {
        set_editor.set(None);
        let content = saved.map_or_else(
            || "Cancelled — nothing was saved.".to_string(),
            |title| format!("Saved your note \"{title}\"."),
        );
        set_messages.update(|msgs| {
            msgs.push(ChatMsg {
                id: uuid::Uuid::new_v4().to_string(),
                role: "assistant".to_string(),
                content,
                options: Vec::new(),
                input_sensitive: false,
            });
        });
    })
}

/// Starter prompts shown on the empty chat. Each is sent as an
/// ordinary message, so it takes the same path as typed text.
const SUGGESTIONS: [(IconKind, &str, &str); 3] = [
    (IconKind::Note, "Save a note", "store a note"),
    (IconKind::Note, "Save a document", "store a document"),
    (IconKind::Key, "Save a password", "store a password"),
];

/// The empty state: brand mark, a greeting, and starter prompts.
fn empty_view(loading: ReadSignal<bool>, send_text: Callback<(String, Option<String>)>) -> AnyView {
    let chips = SUGGESTIONS
        .iter()
        .map(|(kind, label, prompt)| {
            let kind = *kind;
            let prompt = *prompt;
            view! {
                <button
                    class="suggestion"
                    prop:disabled=loading
                    on:click=move |_| send_text.run((prompt.to_string(), None))
                >
                    <Icon kind=kind class="icon-sm" />
                    {*label}
                </button>
            }
        })
        .collect_view();
    view! {
        <div class="empty">
            <BrandMark />
            <h2 class="title">"How can I help?"</h2>
            <p class="subtitle">"Save a note or document, or find one you saved."</p>
            <div class="suggestions">{chips}</div>
            <p class="hint">
                "Tip: for notes you want to keep forever, a plain text file (.txt) is best. "
                "Any computer can open it, with no special app."
            </p>
        </div>
    }
    .into_any()
}

/// The animated dots shown while a reply is being produced.
fn typing_view() -> AnyView {
    view! {
        <div class="msg msg--assistant" aria-label="Stern is typing">
            <div class="avatar"><img src="stern-mark.svg" alt="" /></div>
            <div class="msg-body">
                <div class="bubble bubble--assistant typing"><span></span><span></span><span></span></div>
            </div>
        </div>
    }
    .into_any()
}

/// The scrolling transcript: the empty-state, or every message
/// with the newest one's choices live.
fn transcript_view(
    messages: ReadSignal<Vec<ChatMsg>>,
    loading: ReadSignal<bool>,
    send_text: Callback<(String, Option<String>)>,
) -> AnyView {
    let msgs = messages.get();
    if msgs.is_empty() {
        return empty_view(loading, send_text);
    }
    let last_index = msgs.len().saturating_sub(1);
    let rows = msgs
        .into_iter()
        .enumerate()
        .map(|(index, msg)| message_view(msg, index == last_index, loading, send_text))
        .collect_view();
    view! {
        {rows}
        {move || loading.get().then(typing_view)}
    }
    .into_any()
}

/// Render one transcript message. Choices are live only on the
/// newest message — an older prompt's buttons would answer a
/// question the conversation has already moved past.
fn message_view(
    msg: ChatMsg,
    is_latest: bool,
    loading: ReadSignal<bool>,
    send_text: Callback<(String, Option<String>)>,
) -> AnyView {
    let is_user = msg.role == "user";
    let row_class = if is_user {
        "msg msg--user"
    } else {
        "msg msg--assistant"
    };
    let bubble_class = if is_user {
        "bubble bubble--user"
    } else {
        "bubble bubble--assistant"
    };
    let label = if is_user { "You" } else { "Stern" };
    let avatar = if is_user {
        view! {
            <div class="avatar avatar--user"><Icon kind=IconKind::User /></div>
        }
        .into_any()
    } else {
        view! { <div class="avatar"><img src="stern-mark.svg" alt="" /></div> }.into_any()
    };
    let options_view = if !is_user && is_latest && !msg.options.is_empty() {
        let buttons = msg
            .options
            .iter()
            .enumerate()
            .map(|(option_index, option)| {
                let letter = "abcdefghijklmnopqrstuvwxyz"
                    .chars()
                    .nth(option_index)
                    .unwrap_or('?');
                let display = format!("{letter}. {option}");
                let value = option.clone();
                view! {
                    <button
                        class="option"
                        prop:disabled=loading
                        on:click=move |_| send_text.run((value.clone(), None))
                    >
                        {display}
                    </button>
                }
            })
            .collect_view();
        view! { <div class="options">{buttons}</div> }.into_any()
    } else {
        ().into_any()
    };
    view! {
        <div class=row_class>
            {avatar}
            <div class="msg-body">
                <div class="msg-name">{label}</div>
                <div class=bubble_class>{msg.content}</div>
                {options_view}
            </div>
        </div>
    }
    .into_any()
}

/// Keep the input element showing `display`: apply the string
/// imperatively from an effect — never from inside the view,
/// where writing state fights the render — and only when it
/// actually changed. Assigning `.value` drops the caret to the
/// end, so the selection is saved and restored around the
/// assignment: a character expiring mid-edit must not yank the
/// cursor.
fn bind_input_value(input_ref: NodeRef<leptos::html::Input>, display: Memo<String>) {
    Effect::new(move |_| {
        let wanted = display.get();
        if let Some(el) = input_ref.get() {
            if el.value() != wanted {
                let start = el.selection_start().ok().flatten();
                let end = el.selection_end().ok().flatten();
                el.set_value(&wanted);
                if let (Some(start), Some(end)) = (start, end) {
                    el.set_selection_range(start, end).unwrap_or_default();
                }
            }
        }
    });
}

/// The input row: the pill-shaped composer with its secret-mode
/// tag and send button. Presentation only — the peek-mask logic
/// stays in [`ChatView`], which hands it the signals and callbacks.
#[component]
fn Composer(
    input_ref: NodeRef<leptos::html::Input>,
    loading: ReadSignal<bool>,
    is_sensitive: Signal<bool>,
    can_send: Signal<bool>,
    placeholder: Signal<&'static str>,
    on_submit: Callback<SubmitEvent>,
    on_edit: Callback<leptos::ev::Event>,
) -> impl IntoView {
    view! {
        <div class="composer-wrap">
            <form
                class="composer"
                class:composer--secure=is_sensitive
                on:submit=move |ev| on_submit.run(ev)
            >
                <span class="secure-tag">
                    <Icon kind=IconKind::Lock class="icon-sm" />
                    "Private"
                </span>
                <input
                    class="composer-input"
                    node_ref=input_ref
                    autocomplete="off"
                    on:input=move |ev| on_edit.run(ev)
                    placeholder=placeholder
                    prop:disabled=loading
                />
                <button
                    type="submit"
                    class="send"
                    aria-label="Send"
                    title="Send"
                    prop:disabled=move || !can_send.get()
                >
                    <Icon kind=IconKind::Send />
                </button>
            </form>
            <p class="composer-note">
                "Your information stays on this computer."
            </p>
        </div>
    }
}

/// Keep the transcript scrolled to the newest message (and the
/// typing dots) as the conversation grows.
fn follow_transcript(
    transcript_ref: NodeRef<leptos::html::Div>,
    messages: ReadSignal<Vec<ChatMsg>>,
    loading: ReadSignal<bool>,
) {
    Effect::new(move |_| {
        let _ = messages.get();
        let _ = loading.get();
        if let Some(el) = transcript_ref.get() {
            el.set_scroll_top(el.scroll_height());
        }
    });
}

#[component]
pub fn ChatView() -> impl IntoView {
    let (messages, set_messages) = signal(Vec::<ChatMsg>::new());
    let (input, set_input) = signal(String::new());
    let (loading, set_loading) = signal(false);
    // Peek-mask state for secret replies: the real value, the
    // per-character reveal deadlines, and a timer tick that
    // re-renders as characters expire.
    let (secret, set_secret) = signal(String::new());
    let (reveals, set_reveals) = signal(Vec::<f64>::new());
    let tick = use_tick();

    // Secret mode lasts while the newest message asks for one.
    let is_sensitive = move || {
        messages
            .get()
            .last()
            .is_some_and(|msg| msg.role == "assistant" && msg.input_sensitive)
    };

    // The string the input should show right now. A Memo, so it
    // only notifies when the string actually changes — the
    // 200 ms tick on its own no longer re-assigns the input's
    // value. Its current value doubles as the diff base for the
    // next edit (the display as last rendered).
    let display = Memo::new(move |_| {
        let _ = tick.get();
        if is_sensitive() {
            mask_display(&secret.get(), &reveals.get())
        } else {
            input.get()
        }
    });

    // The note editor shown under the transcript, when a reply asked for it.
    let (editor, set_editor) = signal(Option::<EditorRequest>::None);
    let send_text = make_sender(set_messages, loading, set_loading, editor, set_editor);
    let editor_done = editor_done_callback(set_messages, set_editor);

    let reconcile_secret = move |new_display: String, caret_utf16: Option<u32>| {
        let (next_secret, next_reveals) = fold_secret_edit(
            &display.get_untracked(),
            &secret.get_untracked(),
            &reveals.get_untracked(),
            &new_display,
            caret_utf16,
        );
        set_secret.set(next_secret);
        set_reveals.set(next_reveals);
    };

    // The display reaches the input element through an effect,
    // never from inside the view (see bind_input_value).
    let input_ref = NodeRef::<leptos::html::Input>::new();
    bind_input_value(input_ref, display);

    let send_message = move |ev: SubmitEvent| {
        ev.prevent_default();
        if is_sensitive() {
            let text = secret.get_untracked();
            set_secret.set(String::new());
            set_reveals.set(Vec::new());
            send_text.run((text, Some("••••••••".to_string())));
        } else {
            let text = input.get();
            set_input.set(String::new());
            send_text.run((text, None));
        }
    };

    let on_edit = move |ev: leptos::ev::Event| {
        let value = event_target_value(&ev);
        if is_sensitive() {
            let caret = event_target::<web_sys::HtmlInputElement>(&ev)
                .selection_start()
                .ok()
                .flatten();
            reconcile_secret(value, caret);
        } else {
            set_input.set(value);
        }
    };

    let can_send = move || {
        !loading.get()
            && if is_sensitive() {
                !secret.get().is_empty()
            } else {
                !input.get().trim().is_empty()
            }
    };
    let placeholder = move || {
        if is_sensitive() {
            "Type it here. Letters hide after a moment."
        } else {
            "Type a message…"
        }
    };

    let transcript_ref = NodeRef::<leptos::html::Div>::new();
    follow_transcript(transcript_ref, messages, loading);

    view! {
        <div class="chat">
            <div class="transcript" node_ref=transcript_ref>
                <div class="transcript-inner">
                    {move || transcript_view(messages, loading, send_text)}
                    {move || editor.get().map(|request| editor_view(request, editor_done))}
                </div>
            </div>
            <Composer
                input_ref=input_ref
                loading=loading
                is_sensitive=Signal::derive(is_sensitive)
                can_send=Signal::derive(can_send)
                placeholder=Signal::derive(placeholder)
                on_submit=Callback::new(send_message)
                on_edit=Callback::new(on_edit)
            />
        </div>
    }
}
