// Copyright (c) 2026 Christopher Yang
// SPDX-License-Identifier: MIT

//! The configuration view, opened from the gear button in the header.
//!
//! Today it holds the AI model picker; Stern ships with no model and
//! no ONNX Runtime, so choosing and downloading a model here is the
//! first thing a new user does — the chat stays gated until one is
//! active.

use super::model_setup::ModelSetup;
use leptos::prelude::*;

#[component]
pub fn ConfigView() -> impl IntoView {
    view! {
        <div class="page">
            <div class="page-inner">
                <div class="page-head">
                    <h2 class="title">"Settings"</h2>
                    <p class="subtitle">"Everything here stays on this device."</p>
                </div>
                <ModelSetup />
            </div>
        </div>
    }
}
