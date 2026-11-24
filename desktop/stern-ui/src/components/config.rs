// Copyright (c) 2026 Christopher Yang
// SPDX-License-Identifier: MIT

//! The configuration view, opened from the gear button in the header.
//!
//! It holds the look (light or dark) and the AI model picker. Stern
//! ships with no model and no ONNX Runtime, so choosing and
//! downloading a model here is the first thing a new user does — the
//! chat stays gated until one is active.

use super::model_setup::ModelSetup;
use super::theme::{self, Theme, ThemeSignal};
use super::ui::{Icon, IconKind};
use leptos::prelude::*;

/// Light / Dark choice. Takes effect and is saved as soon as it is tapped.
#[component]
fn Appearance() -> impl IntoView {
    let current = expect_context::<ThemeSignal>();
    let light_ref = NodeRef::<leptos::html::Button>::new();
    let dark_ref = NodeRef::<leptos::html::Button>::new();
    // One radio is in the tab order (the chosen one); an arrow key
    // moves to the other and picks it, as a native radio group does.
    let option = move |value: Theme,
                       label: &'static str,
                       icon: IconKind,
                       this: NodeRef<leptos::html::Button>,
                       other: NodeRef<leptos::html::Button>| {
        view! {
            <button
                type="button"
                node_ref=this
                class="theme-option"
                role="radio"
                aria-checked=move || if current.get() == value { "true" } else { "false" }
                tabindex=move || if current.get() == value { "0" } else { "-1" }
                on:click=move |_| theme::choose(current, value)
                on:keydown=move |ev| {
                    if matches!(
                        ev.key().as_str(),
                        "ArrowLeft" | "ArrowRight" | "ArrowUp" | "ArrowDown"
                    ) {
                        ev.prevent_default();
                        theme::choose(current, value.other());
                        if let Some(button) = other.get() {
                            drop(button.focus());
                        }
                    }
                }
            >
                <Icon kind=icon class="icon-sm" />
                {label}
            </button>
        }
    };
    view! {
        <div class="section">
            <div class="section-head">
                <div class="section-title">
                    <Icon kind=IconKind::Sun />
                    "Look"
                </div>
                <p class="section-text">"Pick light or dark."</p>
            </div>
            <div class="theme-options" role="radiogroup" aria-label="Look">
                {option(Theme::Light, "Light", IconKind::Sun, light_ref, dark_ref)}
                {option(Theme::Dark, "Dark", IconKind::Moon, dark_ref, light_ref)}
            </div>
        </div>
    }
}

#[component]
pub fn ConfigView() -> impl IntoView {
    view! {
        <div class="page">
            <div class="page-inner">
                <div class="page-head">
                    <h2 class="title">"Settings"</h2>
                </div>
                <Appearance />
                <div class="section-gap"></div>
                <ModelSetup />
            </div>
        </div>
    }
}
