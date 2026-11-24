// Copyright (c) 2026 Christopher Yang
// SPDX-License-Identifier: MIT

#![allow(clippy::unwrap_used)]

use stern_ui::components::friendly::friendly_error;
use stern_ui::components::markdown::{parse, sections, title, Block};

const HELP: &str = include_str!("../../../docs/Help.md");
const DISCLAIMER: &str = include_str!("../../../docs/Disclaimer.md");

#[test]
fn password_and_lockout_errors_are_plain_and_carry_no_detail() {
    let wrong = friendly_error("invalid master password");
    assert_eq!(
        wrong.message,
        "That password isn't right. Please try again."
    );
    assert!(wrong.detail.is_none());

    let locked = friendly_error("Too many failed attempts. Try again in 240 seconds.");
    assert!(locked.message.contains("wait a few minutes"));
    assert!(!locked.message.contains("240"));
}

#[test]
fn download_failures_hide_jargon_but_keep_the_detail() {
    let raw = r#"promise error: JsValue("model error: download failed: io: invalid peer certificate: UnknownIssuer")"#;
    let error = friendly_error(raw);
    assert_eq!(
        error.message,
        "The download didn't work. Check your internet connection and try again."
    );
    let detail = error.detail.unwrap();
    assert!(detail.starts_with("model error: download failed"));
    assert!(!detail.contains("JsValue"));
}

#[test]
fn unknown_errors_fall_back_to_a_generic_sentence() {
    let error = friendly_error("database error: disk I/O error");
    assert_eq!(error.message, "Something went wrong. Please try again.");
    assert_eq!(
        error.detail.as_deref(),
        Some("database error: disk I/O error")
    );
}

#[test]
fn markdown_reader_handles_the_supported_subset() {
    let blocks =
        parse("# Title\n\n## First\nOne line\ncontinues here.\n\n- a\n- b\n\n## Second\nText.\n");
    assert_eq!(title(&blocks), Some("Title"));
    let parts = sections(&blocks);
    assert_eq!(parts.len(), 2);
    assert_eq!(parts[0].heading, "First");
    assert_eq!(
        parts[0].body,
        [
            Block::Paragraph("One line continues here.".to_string()),
            Block::Bullets(vec!["a".to_string(), "b".to_string()]),
        ]
    );
    assert_eq!(parts[1].body, [Block::Paragraph("Text.".to_string())]);
}

#[test]
fn shipped_pages_parse_and_use_no_inline_markup() {
    for (name, source) in [("Help", HELP), ("Disclaimer", DISCLAIMER)] {
        // The reader has no inline support; markup would show as literal text.
        for marker in ["**", "`", "](", "__"] {
            assert!(!source.contains(marker), "{name} contains {marker:?}");
        }
        let blocks = parse(source);
        assert!(title(&blocks).is_some(), "{name} has no title");
        assert!(!sections(&blocks).is_empty(), "{name} has no sections");
    }
    // Help must stay free of jargon the page exists to avoid.
    let help = HELP.to_lowercase();
    for jargon in ["onnx", "sha-256", "tokenizer", "argon", "smollm", "qwen"] {
        assert!(!help.contains(jargon), "Help mentions {jargon}");
    }
}
