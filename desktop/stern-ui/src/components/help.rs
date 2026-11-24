// Copyright (c) 2026 Christopher Yang
// SPDX-License-Identifier: MIT

//! The Help and Disclaimer pages.
//!
//! Their text is `docs/Help.md` and `docs/Disclaimer.md`, embedded at
//! compile time and rendered with the small reader in
//! [`super::markdown`], so there is one copy of each to maintain.

use super::markdown::{parse, sections, title, Block};
use leptos::prelude::*;

const HELP_MD: &str = include_str!("../../../../docs/Help.md");
const DISCLAIMER_MD: &str = include_str!("../../../../docs/Disclaimer.md");

fn block_view(block: Block) -> AnyView {
    match block {
        Block::Title(text) => view! { <h2 class="title">{text}</h2> }.into_any(),
        Block::Heading(text) => view! { <h3 class="doc-heading">{text}</h3> }.into_any(),
        Block::Paragraph(text) => view! { <p class="doc-text">{text}</p> }.into_any(),
        Block::Bullets(items) => view! {
            <ul class="doc-list">
                {items.into_iter().map(|item| view! { <li>{item}</li> }).collect_view()}
            </ul>
        }
        .into_any(),
    }
}

/// Help: one collapsible question per `##` heading, the first open.
#[component]
pub fn HelpView(on_disclaimer: Callback<()>) -> impl IntoView {
    let blocks = parse(HELP_MD);
    let heading = title(&blocks).unwrap_or("Help").to_string();
    let questions = sections(&blocks)
        .into_iter()
        .enumerate()
        .map(|(index, section)| {
            view! {
                <details open=index == 0>
                    <summary>{section.heading}</summary>
                    <div class="faq-body">
                        {section.body.into_iter().map(block_view).collect_view()}
                    </div>
                </details>
            }
        })
        .collect_view();
    view! {
        <div class="page">
            <div class="page-inner">
                <div class="page-head">
                    <h2 class="title">{heading}</h2>
                </div>
                <div class="faq">{questions}</div>
                <div class="help-foot">
                    <button class="link-btn" on:click=move |_| on_disclaimer.run(())>
                        "Read the disclaimer"
                    </button>
                </div>
            </div>
        </div>
    }
}

/// The disclaimer, shown as a plain document.
#[component]
pub fn DisclaimerView() -> impl IntoView {
    let body = parse(DISCLAIMER_MD)
        .into_iter()
        .map(block_view)
        .collect_view();
    view! {
        <div class="page">
            <div class="page-inner">
                <div class="doc">{body}</div>
            </div>
        </div>
    }
}
