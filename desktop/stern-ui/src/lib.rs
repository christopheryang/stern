#![allow(clippy::must_use_candidate)]
// The TypedBuilder derive behind Leptos's #[component] macro generates an
// empty enum; clippy's empty_enums lint (as of 1.100) flags it at the
// component definition site, so allow it crate-wide.
#![allow(clippy::empty_enums)]

pub mod app_shell;
pub mod components;
