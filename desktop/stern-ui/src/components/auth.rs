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

use super::ui::{BrandMark, ErrorAlert, Icon, IconKind};
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

/// One password field whose visibility follows the screen's
/// shared show toggle.
#[component]
fn PasswordField(
    value: RwSignal<String>,
    show: ReadSignal<bool>,
    placeholder: &'static str,
    label: &'static str,
    #[prop(optional)] autofocus: bool,
) -> impl IntoView {
    view! {
        <label class="field">
            <span class="field-label">{label}</span>
            <input
                class="input"
                prop:value=value
                on:input=move |ev| value.set(event_target_value(&ev))
                placeholder=placeholder
                autofocus=autofocus
                autocomplete="off"
                type=move || if show.get() { "text" } else { "password" }
            />
        </label>
    }
}

/// The show-password checkbox both screens share.
#[component]
fn ShowToggle(show: ReadSignal<bool>, set_show: WriteSignal<bool>) -> impl IntoView {
    view! {
        <label class="check">
            <input
                type="checkbox"
                prop:checked=show
                on:change=move |ev| set_show.set(event_target_checked(&ev))
            />
            "Show password"
        </label>
    }
}

/// The password policy, one entry per rule: its label and whether a
/// candidate passes it. Drives both the live checklist and
/// [`password_gap`], so the two cannot drift apart.
fn password_rules(password: &str) -> [(&'static str, &'static str, bool); 5] {
    [
        (
            "8+ characters",
            "8+ characters",
            password.chars().count() >= 8,
        ),
        (
            "A lowercase letter",
            "a lowercase letter",
            password.chars().any(char::is_lowercase),
        ),
        (
            "An uppercase letter",
            "an uppercase letter",
            password.chars().any(char::is_uppercase),
        ),
        (
            "A number",
            "a number",
            password.chars().any(|c| c.is_ascii_digit()),
        ),
        (
            "A symbol",
            "a symbol",
            password.chars().any(|c| !c.is_alphanumeric()),
        ),
    ]
}

/// What a candidate master password is still missing under the
/// policy (at least 8 characters, a lowercase letter, an
/// uppercase letter, a number, and a symbol), as one brief
/// comma-joined phrase — or nothing when it passes. Mirrors the
/// chat gate's check in stern-core.
fn password_gap(password: &str) -> Option<String> {
    let missing: Vec<&str> = password_rules(password)
        .into_iter()
        .filter(|(_, _, met)| !met)
        .map(|(_, phrase, _)| phrase)
        .collect();
    if missing.is_empty() {
        None
    } else {
        Some(missing.join(", "))
    }
}

/// The live requirements checklist under the new-password field.
#[component]
fn Requirements(password: RwSignal<String>) -> impl IntoView {
    view! {
        <ul class="requirements" aria-label="Password requirements">
            {move || {
                password_rules(&password.get())
                    .into_iter()
                    .map(|(label, _, met)| {
                        view! {
                            <li class:met=met>
                                <span class="dot"><Icon kind=IconKind::Check /></span>
                                {label}
                            </li>
                        }
                    })
                    .collect_view()
            }}
        </ul>
    }
}

/// Step two of setup: the no-recovery warning, shown once a valid
/// password has been entered, before anything is created.
#[component]
fn RecoveryWarning(
    busy: ReadSignal<bool>,
    error: ReadSignal<Option<String>>,
    on_back: Callback<()>,
    on_create: Callback<()>,
) -> impl IntoView {
    view! {
        <div class="badge-icon badge-icon--warn">
            <Icon kind=IconKind::Alert />
        </div>
        <h2 class="title">"There is no recovery"</h2>
        <p class="subtitle">
            "If you forget this password, your vault can never be \
            opened again — there is no way to recover it. Write it \
            down somewhere safe."
        </p>
        {move || error.get().map(|e| view! {
            <div class="gap-top"><ErrorAlert message=e /></div>
        })}
        <div class="btn-row">
            <button class="btn" prop:disabled=busy on:click=move |_| on_back.run(())>
                "Back"
            </button>
            <button
                class="btn btn-primary"
                prop:disabled=busy
                on:click=move |_| on_create.run(())
            >
                {move || if busy.get() { "Creating…" } else { "Create vault" }}
            </button>
        </div>
    }
}

/// The first-run screen: choose the master password. No account,
/// no username — this password plus the device's Secret Key is
/// the whole credential. Kept brief, the way other vaults do it:
/// labeled fields and a live requirements checklist; the no-recovery
/// warning appears as its own step after the password is entered.
#[component]
pub fn SetupView(set_auth: WriteSignal<Option<AuthPhase>>) -> impl IntoView {
    let password = RwSignal::new(String::new());
    let confirm = RwSignal::new(String::new());
    let (show, set_show) = signal(false);
    let (error, set_error) = signal(Option::<String>::None);
    let (busy, set_busy) = signal(false);
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

    let create = Callback::new(move |()| {
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
    });
    let back = Callback::new(move |()| {
        set_warn.set(false);
        set_error.set(None);
    });

    view! {
        <div class="stage">
            <div class="card">
                {move || -> AnyView {
                    if warn.get() {
                        view! {
                            <RecoveryWarning busy=busy error=error on_back=back on_create=create />
                        }.into_any()
                    } else {
                        view! {
                            <BrandMark />
                            <h2 class="title">"Set up your vault"</h2>
                            <p class="subtitle">
                                "Choose a master password. It never leaves this device."
                            </p>
                            <form class="form" on:submit=submit>
                                <PasswordField
                                    value=password
                                    show=show
                                    label="Master password"
                                    placeholder="Create a strong password"
                                    autofocus=true
                                />
                                <Requirements password=password />
                                <PasswordField
                                    value=confirm
                                    show=show
                                    label="Confirm password"
                                    placeholder="Type it again"
                                />
                                <ShowToggle show=show set_show=set_show />
                                {move || error.get().map(|e| view! { <ErrorAlert message=e /> })}
                                <button type="submit" class="btn btn-primary btn-block">
                                    "Continue"
                                </button>
                            </form>
                        }.into_any()
                    }
                }}
            </div>
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
        <div class="stage">
            <div class="card">
                <BrandMark />
                <h2 class="title">"Welcome back"</h2>
                <p class="subtitle">
                    "Enter your master password to unlock your vault."
                </p>
                <form class="form" on:submit=submit>
                    <PasswordField
                        value=password
                        show=show
                        label="Master password"
                        placeholder="Master password"
                        autofocus=true
                    />
                    <ShowToggle show=show set_show=set_show />
                    {move || error.get().map(|e| view! { <ErrorAlert message=e /> })}
                    <button
                        type="submit"
                        class="btn btn-primary btn-block"
                        prop:disabled=busy
                    >
                        <Icon kind=IconKind::Unlock class="icon-sm" />
                        {move || if busy.get() { "Unlocking…" } else { "Unlock" }}
                    </button>
                </form>
            </div>
        </div>
    }
}
