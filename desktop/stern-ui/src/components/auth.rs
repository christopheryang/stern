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

use super::friendly::friendly_error;
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
            "At least 8 characters",
            "at least 8 characters",
            password.chars().count() >= 8,
        ),
        (
            "A small letter (a–z)",
            "a small letter",
            password.chars().any(char::is_lowercase),
        ),
        (
            "A capital letter (A–Z)",
            "a capital letter",
            password.chars().any(char::is_uppercase),
        ),
        (
            "A number (0–9)",
            "a number",
            password.chars().any(|c| c.is_ascii_digit()),
        ),
        (
            "A symbol, like ! or ?",
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

/// The small "Disclaimer" link under the auth cards, so it can be
/// read before agreeing to it and on every later launch.
#[component]
fn DisclaimerLink(on_disclaimer: Callback<()>) -> impl IntoView {
    view! {
        <p class="fine-print">
            <button type="button" class="link-btn" on:click=move |_| on_disclaimer.run(())>
                "Disclaimer"
            </button>
        </p>
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
        <h2 class="title">"Don't lose your password"</h2>
        <p class="subtitle">
            "If you forget it, no one can get your information back. \
            Write it down and keep it somewhere safe."
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
                {move || if busy.get() { "Setting up…" } else { "I wrote it down" }}
            </button>
        </div>
    }
}

/// The first step of setup: choose a password, confirm it, and agree
/// to the disclaimer. `on_valid` fires once all of it checks out.
#[component]
fn SetupForm(
    password: RwSignal<String>,
    confirm: RwSignal<String>,
    on_valid: Callback<()>,
    on_disclaimer: Callback<()>,
) -> impl IntoView {
    let (show, set_show) = signal(false);
    let (error, set_error) = signal(Option::<String>::None);
    let (agreed, set_agreed) = signal(false);

    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let pw = password.get();
        if let Some(gap) = password_gap(&pw) {
            set_error.set(Some(format!("Your password still needs: {gap}.")));
        } else if !agreed.get() {
            set_error.set(Some("Please tick the box to continue.".to_string()));
        } else if pw != confirm.get() {
            set_error.set(Some(
                "The two passwords don't match. Please try again.".to_string(),
            ));
        } else {
            set_error.set(None);
            on_valid.run(());
        }
    };

    view! {
        <BrandMark />
        <h2 class="title">"Welcome to Stern"</h2>
        <p class="subtitle">"Create a password to protect everything you save."</p>
        <form class="form" on:submit=submit>
            <PasswordField
                value=password
                show=show
                label="Your password"
                placeholder="Create a password"
                autofocus=true
            />
            <Requirements password=password />
            <PasswordField
                value=confirm
                show=show
                label="Type it again"
                placeholder="Same password"
            />
            <ShowToggle show=show set_show=set_show />
            <label class="check agree">
                <input
                    type="checkbox"
                    prop:checked=agreed
                    on:change=move |ev| set_agreed.set(event_target_checked(&ev))
                />
                <span>
                    "I understand that Stern is used at my own risk. "
                    <button
                        type="button"
                        class="link-btn"
                        on:click=move |_| on_disclaimer.run(())
                    >
                        "Read the disclaimer"
                    </button>
                </span>
            </label>
            {move || error.get().map(|e| view! { <ErrorAlert message=e /> })}
            <button type="submit" class="btn btn-primary btn-block">
                "Continue"
            </button>
        </form>
    }
}

/// The first-run screen: choose the master password. No account,
/// no username — this password plus the device's Secret Key is
/// the whole credential. The no-recovery warning appears as its own
/// step after a valid password is entered.
#[component]
pub fn SetupView(
    set_auth: WriteSignal<Option<AuthPhase>>,
    on_disclaimer: Callback<()>,
) -> impl IntoView {
    let password = RwSignal::new(String::new());
    let confirm = RwSignal::new(String::new());
    let (error, set_error) = signal(Option::<String>::None);
    let (busy, set_busy) = signal(false);
    let (warn, set_warn) = signal(false);

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
                    set_error.set(Some(friendly_error(&e).message));
                    set_busy.set(false);
                }
            }
        });
    });
    let back = Callback::new(move |()| {
        set_warn.set(false);
        set_error.set(None);
    });
    let next = Callback::new(move |()| set_warn.set(true));

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
                            <SetupForm
                                password=password
                                confirm=confirm
                                on_valid=next
                                on_disclaimer=on_disclaimer
                            />
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
pub fn LoginView(
    set_auth: WriteSignal<Option<AuthPhase>>,
    on_disclaimer: Callback<()>,
) -> impl IntoView {
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
                    set_error.set(Some(friendly_error(&e).message));
                    password.set(String::new());
                    set_busy.set(false);
                }
            }
        });
    };

    view! {
        <div class="stage">
            <div class="card">
                <h2 class="title">"Welcome back"</h2>
                <p class="subtitle">"Enter your password to open Stern."</p>
                <form class="form" on:submit=submit>
                    <PasswordField
                        value=password
                        show=show
                        label="Your password"
                        placeholder="Your password"
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
                        {move || if busy.get() { "Opening…" } else { "Open" }}
                    </button>
                </form>
                <DisclaimerLink on_disclaimer=on_disclaimer />
            </div>
        </div>
    }
}
