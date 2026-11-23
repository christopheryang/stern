// Copyright (c) 2026 Christopher Yang
// SPDX-License-Identifier: MIT

//! The vault's front door: a setup screen on first run and a
//! login screen on every later launch.
//!
//! Stern is a personal, local vault — there is no account and no
//! username, the pattern local products (`KeePass`, `Enpass`) use.
//! The only credential is the master password, combined on this
//! device with the stored Secret Key (2SKD). Until the vault is
//! unlocked, nothing else in the app is reachable: no chat, and
//! not even the model picker — setup comes first.

use leptos::prelude::*;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;

/// Where the vault stands, as the shell's outermost gate sees it.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AuthPhase {
    /// No vault exists yet: show the setup screen.
    Setup,
    /// A vault exists but is locked: show the login screen.
    Login,
    /// The vault is unlocked: the app is usable.
    Unlocked,
}

/// Call a Tauri command and return its JSON result. A command
/// error arrives as the backend's message string.
#[allow(clippy::future_not_send)]
async fn invoke(name: &str, args: serde_json::Value) -> Result<serde_json::Value, String> {
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
        .call2(&core, &JsValue::from_str(name), &args_js)
        .map_err(|e| format!("invoke error: {e:?}"))?;

    let result = wasm_bindgen_futures::JsFuture::from(js_sys::Promise::resolve(&result)).await;
    match result {
        Ok(value) => serde_wasm_bindgen::from_value(value).map_err(|e| e.to_string()),
        Err(rejection) => Err(rejection
            .as_string()
            .unwrap_or_else(|| "the command failed".to_string())),
    }
}

/// The phase a `VaultStatus` payload describes.
fn phase_of(status: &serde_json::Value) -> Result<AuthPhase, String> {
    let initialized = status
        .get("is_initialized")
        .and_then(serde_json::Value::as_bool)
        .ok_or("vault status had no is_initialized")?;
    let unlocked = status
        .get("is_unlocked")
        .and_then(serde_json::Value::as_bool)
        .ok_or("vault status had no is_unlocked")?;
    Ok(match (initialized, unlocked) {
        (false, _) => AuthPhase::Setup,
        (true, false) => AuthPhase::Login,
        (true, true) => AuthPhase::Unlocked,
    })
}

/// Ask the backend where the vault stands.
#[allow(clippy::future_not_send)]
pub async fn fetch_vault_phase() -> Result<AuthPhase, String> {
    let status = invoke("vault_status", serde_json::json!({})).await?;
    phase_of(&status)
}

/// Run a password-carrying vault command (`create_vault`,
/// `unlock_vault`) and return the phase it leaves the vault in.
#[allow(clippy::future_not_send)]
async fn invoke_password_command(name: &str, password: &str) -> Result<AuthPhase, String> {
    let args = serde_json::json!({ "request": { "password": password } });
    let status = invoke(name, args).await?;
    phase_of(&status)
}

/// Lock the vault again.
#[allow(clippy::future_not_send)]
pub async fn lock_vault() -> Result<(), String> {
    invoke("lock_vault", serde_json::json!({})).await?;
    Ok(())
}

const PANEL_STYLE: &str = "flex:1;display:flex;flex-direction:column;align-items:center;justify-content:center;gap:12px;padding:20px;text-align:center;";
const TITLE_STYLE: &str = "font-size:18px;color:#e6e6e6;";
const BODY_STYLE: &str = "color:#a0a0b0;font-size:14px;max-width:420px;line-height:1.5;margin:0;";
const FORM_STYLE: &str =
    "display:flex;flex-direction:column;gap:10px;width:100%;max-width:340px;margin-top:6px;";
const INPUT_STYLE: &str = "padding:10px 14px;border:1px solid #2a2a4a;border-radius:8px;background:#1a1a2e;color:#e6e6e6;font-size:14px;outline:none;";
const PRIMARY_BUTTON_STYLE: &str = "padding:10px 20px;border:none;border-radius:8px;background:#e94560;color:white;font-size:14px;cursor:pointer;";
const ERROR_STYLE: &str = "color:#e94560;font-size:13px;max-width:420px;line-height:1.5;margin:0;";
const TOGGLE_STYLE: &str =
    "color:#a0a0b0;font-size:13px;display:flex;align-items:center;gap:6px;cursor:pointer;";

/// One password field whose visibility follows the screen's
/// shared show toggle.
#[component]
fn PasswordField(
    value: RwSignal<String>,
    show: ReadSignal<bool>,
    placeholder: &'static str,
) -> impl IntoView {
    view! {
        <input
            style=INPUT_STYLE
            prop:value=value
            on:input=move |ev| value.set(event_target_value(&ev))
            placeholder=placeholder
            type=move || if show.get() { "text" } else { "password" }
        />
    }
}

/// What a candidate master password is still missing under the
/// policy (at least 8 characters, a lowercase letter, an
/// uppercase letter, a number, and a symbol), as one brief
/// comma-joined phrase — or nothing when it passes. Mirrors the
/// chat gate's check in stern-core.
fn password_gap(password: &str) -> Option<String> {
    let mut missing: Vec<&str> = Vec::new();
    if password.chars().count() < 8 {
        missing.push("8+ characters");
    }
    if !password.chars().any(char::is_lowercase) {
        missing.push("a lowercase letter");
    }
    if !password.chars().any(char::is_uppercase) {
        missing.push("an uppercase letter");
    }
    if !password.chars().any(|c| c.is_ascii_digit()) {
        missing.push("a number");
    }
    if !password.chars().any(|c| !c.is_alphanumeric()) {
        missing.push("a symbol");
    }
    if missing.is_empty() {
        None
    } else {
        Some(missing.join(", "))
    }
}

const HINT_STYLE: &str = "color:#a0a0b0;font-size:12px;margin:0;text-align:left;";
const GHOST_BUTTON_STYLE: &str = "padding:10px 20px;border:1px solid #2a2a4a;border-radius:8px;background:transparent;color:#a0a0b0;font-size:14px;cursor:pointer;";

/// The first-run screen: choose the master password. No account,
/// no username — this password plus the device's Secret Key is
/// the whole credential. Kept brief, the way other vaults do it:
/// labeled fields and one requirements line; the no-recovery
/// warning appears as its own step after the password is entered.
#[component]
pub fn SetupView(set_auth: WriteSignal<Option<AuthPhase>>) -> impl IntoView {
    let password = RwSignal::new(String::new());
    let confirm = RwSignal::new(String::new());
    let (show, set_show) = signal(false);
    let (error, set_error) = signal(Option::<String>::None);
    let (busy, set_busy) = signal(false);
    // Step two: the no-recovery warning, shown once a valid
    // password has been entered, before anything is created.
    let (warn, set_warn) = signal(false);

    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let pw = password.get();
        if let Some(gap) = password_gap(&pw) {
            set_error.set(Some(format!("Password still needs: {gap}.")));
            return;
        }
        if pw != confirm.get() {
            set_error.set(Some("The two passwords don't match.".to_string()));
            return;
        }
        set_error.set(None);
        set_warn.set(true);
    };

    let create = move |_| {
        if busy.get() {
            return;
        }
        set_busy.set(true);
        let pw = password.get();
        spawn_local(async move {
            match invoke_password_command("create_vault", &pw).await {
                Ok(phase) => set_auth.set(Some(phase)),
                Err(e) => {
                    set_error.set(Some(e));
                    set_busy.set(false);
                }
            }
        });
    };

    view! {
        <div style=PANEL_STYLE>
            {move || -> AnyView {
                if warn.get() {
                    view! {
                        <div style=TITLE_STYLE>"⚠ No recovery"</div>
                        <p style=BODY_STYLE>
                            "If you forget this password, your vault can never be \
                            opened again — there is no way to recover it. Write it \
                            down somewhere safe."
                        </p>
                        {move || error.get().map(|e| view! { <p style=ERROR_STYLE>{e}</p> })}
                        <div style="display:flex;gap:10px;">
                            <button
                                style=GHOST_BUTTON_STYLE
                                on:click=move |_| set_warn.set(false)
                            >
                                "Back"
                            </button>
                            <button
                                style=PRIMARY_BUTTON_STYLE
                                prop:disabled=busy
                                on:click=create
                            >
                                "Create vault"
                            </button>
                        </div>
                    }.into_any()
                } else {
                    view! {
                        <div style=TITLE_STYLE>"Set up your vault"</div>
                        <form style=FORM_STYLE on:submit=submit>
                            <PasswordField value=password show=show placeholder="Master password" />
                            <p style=HINT_STYLE>
                                "8+ characters, with a lowercase letter, an uppercase \
                                letter, a number, and a symbol."
                            </p>
                            <PasswordField value=confirm show=show placeholder="Type it again to confirm" />
                            <label style=TOGGLE_STYLE>
                                <input
                                    type="checkbox"
                                    on:change=move |ev| set_show.set(event_target_checked(&ev))
                                />
                                "Show password"
                            </label>
                            {move || error.get().map(|e| view! { <p style=ERROR_STYLE>{e}</p> })}
                            <button type="submit" style=PRIMARY_BUTTON_STYLE>
                                "Continue"
                            </button>
                        </form>
                    }.into_any()
                }
            }}
        </div>
    }
}

/// The every-launch screen: the vault exists and is locked, and
/// the app stays here until the master password opens it.
#[component]
pub fn LoginView(set_auth: WriteSignal<Option<AuthPhase>>) -> impl IntoView {
    let password = RwSignal::new(String::new());
    let (show, set_show) = signal(false);
    let (error, set_error) = signal(Option::<String>::None);
    let (busy, set_busy) = signal(false);

    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        if busy.get() {
            return;
        }
        let pw = password.get();
        if pw.is_empty() {
            return;
        }
        set_error.set(None);
        set_busy.set(true);
        spawn_local(async move {
            match invoke_password_command("unlock_vault", &pw).await {
                Ok(phase) => set_auth.set(Some(phase)),
                Err(e) => {
                    set_error.set(Some(e));
                    password.set(String::new());
                    set_busy.set(false);
                }
            }
        });
    };

    view! {
        <div style=PANEL_STYLE>
            <div style=TITLE_STYLE>"Unlock your vault"</div>
            <p style=BODY_STYLE>
                "Enter your master password to open the vault and start chatting."
            </p>
            <form style=FORM_STYLE on:submit=submit>
                <PasswordField value=password show=show placeholder="Master password" />
                <label style=TOGGLE_STYLE>
                    <input
                        type="checkbox"
                        on:change=move |ev| set_show.set(event_target_checked(&ev))
                    />
                    "Show password"
                </label>
                {move || error.get().map(|e| view! { <p style=ERROR_STYLE>{e}</p> })}
                <button
                    type="submit"
                    style=PRIMARY_BUTTON_STYLE
                    prop:disabled=busy
                >
                    "Unlock"
                </button>
            </form>
        </div>
    }
}
