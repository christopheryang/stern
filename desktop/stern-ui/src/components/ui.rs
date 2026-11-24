// Copyright (c) 2026 Christopher Yang
// SPDX-License-Identifier: MIT

//! Small shared UI pieces for the redesign: the icon set, the brand
//! mark and logo, and the centered loading indicator. Styling lives
//! in `styles/stern.css`; components only pick class names.

use leptos::prelude::*;

/// The icons the UI uses — 24×24 stroke paths in a Feather-like
/// style, drawn by the `.icon` class (currentColor, no fill).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum IconKind {
    Lock,
    Unlock,
    Settings,
    ArrowLeft,
    User,
    Send,
    Key,
    Search,
    List,
    Shield,
    Check,
    Alert,
    Chip,
    Download,
    Refresh,
    Help,
    Note,
    Sun,
    Moon,
    Minus,
    Trash,
}

impl IconKind {
    const fn paths(self) -> &'static str {
        match self {
            Self::Lock => {
                r#"<rect x="4" y="11" width="16" height="10" rx="2.5"/><path d="M8 11V7.5a4 4 0 0 1 8 0V11"/>"#
            }
            Self::Minus => r#"<path d="M6 12h12"/>"#,
            Self::Trash => {
                r#"<path d="M4 7h16M10 11v6M14 11v6"/><path d="M6 7l1 12a2 2 0 0 0 2 2h6a2 2 0 0 0 2-2l1-12"/><path d="M9 7V4h6v3"/>"#
            }
            Self::Sun => {
                r#"<circle cx="12" cy="12" r="4"/><path d="M12 2.5v2M12 19.5v2M4.6 4.6l1.4 1.4M18 18l1.4 1.4M2.5 12h2M19.5 12h2M4.6 19.4 6 18M18 6l1.4-1.4"/>"#
            }
            Self::Moon => r#"<path d="M20 14.5A8 8 0 0 1 9.5 4 8 8 0 1 0 20 14.5z"/>"#,
            Self::Note => {
                r#"<path d="M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z"/><path d="M14 3v5h5"/><path d="M9 13h6M9 17h4"/>"#
            }
            Self::Unlock => {
                r#"<rect x="4" y="11" width="16" height="10" rx="2.5"/><path d="M8 11V7.5a4 4 0 0 1 7.6-1.7"/>"#
            }
            Self::Settings => {
                r#"<circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.7 1.7 0 0 0 .3 1.9l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.9-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.9.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.9 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.9l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.9.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.9-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.9V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z"/>"#
            }
            Self::ArrowLeft => r#"<path d="M19 12H5"/><path d="m12 19-7-7 7-7"/>"#,
            Self::User => r#"<circle cx="12" cy="8" r="4"/><path d="M4 21a8 8 0 0 1 16 0"/>"#,
            Self::Send => r#"<path d="M12 19V5"/><path d="m5 12 7-7 7 7"/>"#,
            Self::Key => {
                r#"<circle cx="7.5" cy="15.5" r="4.5"/><path d="m10.7 12.3 9.8-9.8"/><path d="m16 7 3 3"/><path d="m14 9 2 2"/>"#
            }
            Self::Search => r#"<circle cx="11" cy="11" r="7"/><path d="m21 21-4.3-4.3"/>"#,
            Self::List => {
                r#"<path d="M8 6h13"/><path d="M8 12h13"/><path d="M8 18h13"/><path d="M3 6h.01"/><path d="M3 12h.01"/><path d="M3 18h.01"/>"#
            }
            Self::Shield => {
                r#"<path d="M12 3 4 6v6c0 4.5 3.2 8.2 8 9 4.8-.8 8-4.5 8-9V6z"/><path d="m9 12 2 2 4-4"/>"#
            }
            Self::Check => r#"<path d="m5 12.5 4.5 4.5L19 7.5"/>"#,
            Self::Alert => {
                r#"<path d="M10.3 3.9 1.8 18a2 2 0 0 0 1.7 3h17a2 2 0 0 0 1.7-3L13.7 3.9a2 2 0 0 0-3.4 0z"/><path d="M12 9v4"/><path d="M12 17h.01"/>"#
            }
            Self::Chip => {
                r#"<rect x="6" y="6" width="12" height="12" rx="2"/><rect x="9.5" y="9.5" width="5" height="5" rx="1"/><path d="M9 2v3M15 2v3M9 19v3M15 19v3M2 9h3M2 15h3M19 9h3M19 15h3"/>"#
            }
            Self::Download => {
                r#"<path d="M12 3v12"/><path d="m7 10 5 5 5-5"/><path d="M5 21h14"/>"#
            }
            Self::Refresh => r#"<path d="M21 12a9 9 0 1 1-2.6-6.4"/><path d="M21 4v5h-5"/>"#,
            Self::Help => {
                r#"<circle cx="12" cy="12" r="9"/><path d="M9.6 9.4a2.5 2.5 0 1 1 3.5 2.3c-.7.3-1.1.9-1.1 1.7"/><path d="M12 17h.01"/>"#
            }
        }
    }
}

/// One icon, sized by the `.icon` classes and colored by the
/// surrounding text color.
#[component]
pub fn Icon(kind: IconKind, #[prop(optional, into)] class: Option<&'static str>) -> impl IntoView {
    let class = class.map_or_else(|| "icon".to_string(), |extra| format!("icon {extra}"));
    view! {
        <svg
            class=class
            viewBox="0 0 24 24"
            aria-hidden="true"
            inner_html=kind.paths()
        ></svg>
    }
}

/// The Stern mark (a ring around a star) — a static asset Trunk
/// copies next to the app.
#[component]
pub fn BrandMark(#[prop(optional, into)] class: Option<&'static str>) -> impl IntoView {
    view! { <img class=class.unwrap_or("hero-mark") src="stern-mark.svg" alt="" /> }
}

/// The full Stern logo for the top bar.
#[component]
pub fn BrandLogo() -> impl IntoView {
    view! {
        <div class="brand">
            <img class="brand-logo brand-logo--light" src="stern-logo.svg" alt="Stern" />
            <img class="brand-logo brand-logo--dark" src="stern-logo-dark.svg" alt="" />
        </div>
    }
}

/// A centered spinner with a short label.
#[component]
pub fn Loading(#[prop(optional, into)] label: Option<&'static str>) -> impl IntoView {
    view! {
        <div class="loading" role="status">
            <div class="spinner"></div>
            <span>{label.unwrap_or("Loading…")}</span>
        </div>
    }
}

/// An error line with an alert icon: a short, plain-language
/// message, plus an optional smaller line with the technical detail.
#[component]
pub fn ErrorAlert(
    #[prop(into)] message: String,
    #[prop(optional, into)] detail: Option<String>,
) -> impl IntoView {
    view! {
        <div class="alert alert-error" role="alert">
            <Icon kind=IconKind::Alert class="icon-sm" />
            <div>
                <span>{message}</span>
                {detail.map(|text| view! { <small class="alert-detail">{text}</small> })}
            </div>
        </div>
    }
}

/// A "are you sure?" dialog. Cancel is the focused default, so a stray
/// Enter never confirms; Escape and a click outside cancel too. Tab stays
/// inside the two buttons. `danger` styles the confirm button as a
/// destructive action.
#[component]
pub fn ConfirmDialog(
    #[prop(into)] title: String,
    #[prop(into)] text: String,
    confirm_label: &'static str,
    confirm_icon: IconKind,
    #[prop(optional)] danger: bool,
    on_cancel: Callback<()>,
    on_confirm: Callback<()>,
) -> impl IntoView {
    let cancel_ref = NodeRef::<leptos::html::Button>::new();
    let confirm_ref = NodeRef::<leptos::html::Button>::new();
    Effect::new(move |_| {
        if let Some(button) = cancel_ref.get() {
            drop(button.focus());
        }
    });
    let on_keydown = move |ev: leptos::ev::KeyboardEvent| match ev.key().as_str() {
        "Escape" => on_cancel.run(()),
        "Tab" => {
            ev.prevent_default();
            let on_cancel_button = web_sys::window()
                .and_then(|w| w.document())
                .and_then(|d| d.active_element())
                .zip(cancel_ref.get())
                .is_some_and(|(active, cancel)| active == **cancel);
            // From Cancel, Tab goes to the confirm button and Shift+Tab
            // wraps to it; from the confirm button, the other way round.
            let next = if on_cancel_button {
                confirm_ref
            } else {
                cancel_ref
            };
            if let Some(button) = next.get() {
                drop(button.focus());
            }
        }
        _ => {}
    };
    let confirm_class = if danger {
        "btn btn-danger"
    } else {
        "btn btn-primary"
    };
    view! {
        <div class="modal-backdrop" on:click=move |_| on_cancel.run(())>
            <div
                class="modal"
                role="dialog"
                aria-modal="true"
                aria-labelledby="confirm-dialog-title"
                aria-describedby="confirm-dialog-text"
                on:click=move |ev| ev.stop_propagation()
                on:keydown=on_keydown
            >
                <h2 id="confirm-dialog-title" class="modal-title">{title}</h2>
                <p id="confirm-dialog-text" class="modal-text">{text}</p>
                <div class="modal-actions">
                    <button class="btn" node_ref=cancel_ref on:click=move |_| on_cancel.run(())>
                        "Cancel"
                    </button>
                    <button class=confirm_class node_ref=confirm_ref on:click=move |_| on_confirm.run(())>
                        <Icon kind=confirm_icon class="icon-sm" />
                        {confirm_label}
                    </button>
                </div>
            </div>
        </div>
    }
}
