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
    let option = move |value: Theme, label: &'static str, icon: IconKind| {
        view! {
            <button
                class="theme-option"
                role="radio"
                aria-checked=move || (current.get() == value).to_string()
                on:click=move |_| theme::choose(current, value)
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
                {option(Theme::Light, "Light", IconKind::Sun)}
                {option(Theme::Dark, "Dark", IconKind::Moon)}
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
