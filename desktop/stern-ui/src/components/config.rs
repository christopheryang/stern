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
        <div style="flex:1;overflow-y:auto;padding-bottom:20px;">
            <div style="padding:20px 20px 0;">
                <h2 style="font-size:18px;font-weight:600;">"Settings"</h2>
            </div>
            <ModelSetup />
        </div>
    }
}
