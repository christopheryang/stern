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

/// Send one message and return the reply text, any choices the
/// reply offers, and whether the next reply is a secret.
#[allow(clippy::future_not_send)]
async fn invoke_chat(content: String) -> Result<(String, Vec<String>, bool), String> {
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
    Ok((message, options, input_sensitive))
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
            let (response_text, options, input_sensitive) = invoke_chat(text)
                .await
                .unwrap_or_else(|e| (format!("Error: {e}"), Vec::new(), false));

            let assistant_msg = ChatMsg {
                id: uuid::Uuid::new_v4().to_string(),
                role: "assistant".to_string(),
                content: response_text,
                options,
                input_sensitive,
            };
            set_messages.update(|msgs| msgs.push(assistant_msg));
            set_loading.set(false);
        });
    })
}

/// The scrolling transcript: the empty-state hint, or every
/// message with the newest one's choices live.
fn transcript_view(
    messages: ReadSignal<Vec<ChatMsg>>,
    loading: ReadSignal<bool>,
    send_text: Callback<(String, Option<String>)>,
) -> AnyView {
    let msgs = messages.get();
    if msgs.is_empty() {
        return view! {
            <div style="display:flex;align-items:center;justify-content:center;height:100%;color:#a0a0b0;font-size:16px;">
                "Ask Stern to store, find, or manage your secrets."
            </div>
        }
        .into_any();
    }
    let last_index = msgs.len().saturating_sub(1);
    msgs.into_iter()
        .enumerate()
        .map(|(index, msg)| message_view(msg, index == last_index, loading, send_text))
        .collect_view()
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
    let bg = if is_user { "#e94560" } else { "#16213e" };
    let color = if is_user { "white" } else { "#e6e6e6" };
    let border = if is_user { "none" } else { "1px solid #2a2a4a" };
    let align = if is_user { "flex-end" } else { "flex-start" };
    let label = if is_user { "You" } else { "Stern" };
    let container_style = format!(
        "display:flex;flex-direction:column;margin-bottom:16px;max-width:80%;align-items:{align}"
    );
    let bubble_style = format!(
        "padding:10px 14px;border-radius:12px;font-size:14px;line-height:1.5;white-space:pre-wrap;background:{bg};color:{color};border:{border}"
    );
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
                        style="padding:8px 14px;border:1px solid #e94560;border-radius:8px;background:transparent;color:#e6e6e6;font-size:14px;cursor:pointer;"
                        prop:disabled=loading
                        on:click=move |_| send_text.run((value.clone(), None))
                    >
                        {display}
                    </button>
                }
            })
            .collect_view();
        view! {
            <div style="display:flex;flex-wrap:wrap;gap:8px;margin-top:10px;">
                {buttons}
            </div>
        }
        .into_any()
    } else {
        ().into_any()
    };
    view! {
        <div style=container_style>
            <div style="font-size:12px;color:#a0a0b0;margin-bottom:4px;">
                {label}
            </div>
            <div style=bubble_style>
                {msg.content}
            </div>
            {options_view}
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

    let send_text = make_sender(set_messages, loading, set_loading);

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

    view! {
        <div style="display:flex;flex-direction:column;height:100%;">
            <div style="flex:1;overflow-y:auto;padding:20px;">
                {move || transcript_view(messages, loading, send_text)}
            </div>
            <form
                style="display:flex;gap:8px;padding:16px 20px;border-top:1px solid #2a2a4a;background:#16213e;"
                on:submit=send_message
            >
                <input
                    style="flex:1;padding:10px 14px;border:1px solid #2a2a4a;border-radius:8px;background:#1a1a2e;color:#e6e6e6;font-size:14px;outline:none;"
                    node_ref=input_ref
                    on:input=move |ev| {
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
                    }
                    placeholder=move || {
                        if is_sensitive() {
                            "Type here — each character hides after 2 seconds"
                        } else {
                            "Ask Stern anything..."
                        }
                    }
                    prop:disabled=loading
                />
                <button
                    type="submit"
                    style="padding:10px 20px;border:none;border-radius:8px;background:#e94560;color:white;font-size:14px;cursor:pointer;"
                    prop:disabled=move || {
                        loading.get()
                            || if is_sensitive() {
                                secret.get().is_empty()
                            } else {
                                input.get().trim().is_empty()
                            }
                    }
                >
                    "Send"
                </button>
            </form>
        </div>
    }
}
