use leptos::prelude::*;
use leptos::ev::SubmitEvent;
use wasm_bindgen_futures::spawn_local;
use wasm_bindgen::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ChatMsg {
    id: String,
    role: String,
    content: String,
}

async fn invoke_chat(content: String) -> Result<String, String> {
    let args = serde_json::json!({ "request": { "content": content } });
    let args_str = serde_json::to_string(&args).map_err(|e| e.to_string())?;

    let script = format!(
        "window.__TAURI__.core.invoke('send_chat_message', {})",
        args_str
    );

    let result = js_sys::eval(&script)
        .map_err(|e| format!("eval error: {:?}", e))?;

    let promise: js_sys::Promise = result
        .dyn_into()
        .map_err(|_| "result is not a promise")?;

    let resolved = wasm_bindgen_futures::JsFuture::from(promise)
        .await
        .map_err(|e| format!("promise error: {:?}", e))?;

    let response: serde_json::Value =
        serde_wasm_bindgen::from_value(resolved).map_err(|e| e.to_string())?;
    response["message"]
        .as_str()
        .map(String::from)
        .ok_or_else(|| "no message in response".to_string())
}

#[component]
pub fn ChatView() -> impl IntoView {
    let (messages, set_messages) = signal(Vec::<ChatMsg>::new());
    let (input, set_input) = signal(String::new());
    let (loading, set_loading) = signal(false);

    let send_message = move |ev: SubmitEvent| {
        ev.prevent_default();
        let text = input.get();
        if text.trim().is_empty() || loading.get() {
            return;
        }

        let user_msg = ChatMsg {
            id: uuid::Uuid::new_v4().to_string(),
            role: "user".to_string(),
            content: text.clone(),
        };
        set_messages.update(|msgs| msgs.push(user_msg));
        set_input.set(String::new());
        set_loading.set(true);

        spawn_local(async move {
            let response_text = invoke_chat(text).await.unwrap_or_else(|e| {
                format!("Error: {e}")
            });

            let assistant_msg = ChatMsg {
                id: uuid::Uuid::new_v4().to_string(),
                role: "assistant".to_string(),
                content: response_text,
            };
            set_messages.update(|msgs| msgs.push(assistant_msg));
            set_loading.set(false);
        });
    };

    let empty_view = move || -> AnyView {
        view! {
            <div style="display:flex;align-items:center;justify-content:center;height:100%;color:#a0a0b0;font-size:16px;">
                "Ask Stern to store, find, or manage your secrets."
            </div>
        }.into_any()
    };

    let messages_view = move || -> AnyView {
        let msgs = messages.get();
        msgs.into_iter().map(|msg| {
            let is_user = msg.role == "user";
            let bg = if is_user { "#e94560" } else { "#16213e" };
            let color = if is_user { "white" } else { "#e6e6e6" };
            let border = if is_user { "none" } else { "1px solid #2a2a4a" };
            let align = if is_user { "flex-end" } else { "flex-start" };
            let label = if is_user { "You" } else { "Stern" };
            let container_style = format!(
                "display:flex;flex-direction:column;margin-bottom:16px;max-width:80%;align-items:{};",
                align
            );
            let bubble_style = format!(
                "padding:10px 14px;border-radius:12px;font-size:14px;line-height:1.5;white-space:pre-wrap;background:{};color:{};border:{};",
                bg, color, border
            );
            view! {
                <div style=container_style>
                    <div style="font-size:12px;color:#a0a0b0;margin-bottom:4px;">
                        {label}
                    </div>
                    <div style=bubble_style>
                        {msg.content}
                    </div>
                </div>
            }
        }).collect_view().into_any()
    };

    view! {
        <div style="display:flex;flex-direction:column;height:100%;">
            <div style="flex:1;overflow-y:auto;padding:20px;">
                {move || {
                    let msgs = messages.get();
                    if msgs.is_empty() {
                        empty_view()
                    } else {
                        messages_view()
                    }
                }}
            </div>
            <form
                style="display:flex;gap:8px;padding:16px 20px;border-top:1px solid #2a2a4a;background:#16213e;"
                on:submit=send_message
            >
                <input
                    style="flex:1;padding:10px 14px;border:1px solid #2a2a4a;border-radius:8px;background:#1a1a2e;color:#e6e6e6;font-size:14px;outline:none;"
                    prop:value=input
                    on:input=move |ev| set_input.set(event_target_value(&ev))
                    placeholder="Ask Stern anything..."
                    prop:disabled=loading
                />
                <button
                    type="submit"
                    style="padding:10px 20px;border:none;border-radius:8px;background:#e94560;color:white;font-size:14px;cursor:pointer;"
                    prop:disabled=move || loading.get() || input.get().trim().is_empty()
                >
                    "Send"
                </button>
            </form>
        </div>
    }
}
