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
    assert!(error.detail.is_none());
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

#[test]
fn light_is_the_default_theme_and_unknown_values_fall_back_to_it() {
    use stern_ui::components::theme::Theme;

    assert_eq!(Theme::default(), Theme::Light);
    assert_eq!(Theme::from_stored(None), Theme::Light);
    assert_eq!(Theme::from_stored(Some("light")), Theme::Light);
    assert_eq!(Theme::from_stored(Some("purple")), Theme::Light);
    assert_eq!(Theme::from_stored(Some("dark")), Theme::Dark);
    assert_eq!(Theme::from_stored(Some(Theme::Dark.as_str())), Theme::Dark);
}

#[test]
fn the_stylesheet_is_light_by_default_and_dark_only_on_request() {
    let css = include_str!("../styles/stern.css");
    assert!(css.contains(":root[data-theme=\"dark\"]"));
    assert!(!css.contains("prefers-color-scheme"));
    let light = css.find(":root {").unwrap();
    let dark = css.find("\n:root[data-theme=\"dark\"] {").unwrap();
    assert!(light < dark);
    assert!(css[light..dark].contains("--bg: #f4f5fd;"));
}

#[test]
fn help_explains_what_an_ai_model_is_and_how_to_choose() {
    let blocks = parse(HELP);
    let headings: Vec<String> = blocks
        .iter()
        .filter_map(|b| match b {
            Block::Heading(text) => Some(text.clone()),
            _ => None,
        })
        .collect();
    assert!(headings.iter().any(|h| h == "What is an AI model?"));
    assert!(headings
        .iter()
        .any(|h| h == "Which AI model should I choose?"));
    assert!(!HELP.contains("Medium"), "Medium no longer exists");
    assert!(!HELP.to_lowercase().contains("a size"));
}

#[test]
fn a_space_does_not_count_as_a_symbol() {
    use stern_ui::components::auth::password_gap;

    assert_eq!(
        password_gap("Correct Horse 9"),
        Some("a symbol".to_string())
    );
    assert_eq!(
        password_gap("Correct Horse 9\t"),
        Some("a symbol".to_string())
    );
    assert_eq!(password_gap("Correct Horse 9!"), None);
    assert_eq!(password_gap("Correct-Horse-9"), None);
}

#[test]
fn an_unrelated_error_that_mentions_downloads_is_not_a_download_failure() {
    let error = friendly_error("io error: /Users/sam/Downloads/export.json: permission denied");
    assert_eq!(error.message, "Something went wrong. Please try again.");
    assert!(error.detail.is_none());
}

#[test]
fn raw_backend_diagnostics_never_become_small_print() {
    for raw in [
        "database error: no such table: vault_meta",
        "keychain error: item not found",
        "serialization error: expected value at line 1 column 1",
        "model error: model smollm2 has no tokenizer",
    ] {
        let error = friendly_error(raw);
        assert!(error.detail.is_none(), "{raw} leaked its detail");
        assert!(!error.message.contains("table"), "{raw}");
    }
}

#[test]
fn locking_asks_first_unless_the_choice_was_switched_off() {
    use stern_ui::components::prefs::confirm_lock_from_stored;
    assert!(confirm_lock_from_stored(None));
    assert!(confirm_lock_from_stored(Some("on")));
    assert!(confirm_lock_from_stored(Some("garbage")));
    assert!(!confirm_lock_from_stored(Some("off")));
}

#[test]
fn help_covers_removing_a_model_and_stays_plain() {
    let blocks = parse(HELP);
    let headings: Vec<String> = blocks
        .iter()
        .filter_map(|b| match b {
            Block::Heading(text) => Some(text.clone()),
            _ => None,
        })
        .collect();
    for wanted in [
        "How do I remove an AI model?",
        "How do I change a note I saved?",
        "How do I save a document?",
    ] {
        assert!(headings.iter().any(|h| h == wanted), "Help lacks {wanted}");
    }
    // Every question has an answer that says something.
    for section in sections(&blocks) {
        assert!(
            !section.body.is_empty(),
            "{} has no answer",
            section.heading
        );
    }
}

#[test]
fn file_sizes_read_in_plain_units_with_one_decimal() {
    use stern_ui::components::notes::size_label;
    assert_eq!(size_label(0), "0 bytes");
    assert_eq!(size_label(1023), "1023 bytes");
    assert_eq!(size_label(1024), "1.0 KB");
    assert_eq!(size_label(1536), "1.5 KB");
    assert_eq!(size_label(3_355_443), "3.1 MB"); // truncated, never rounded up past the limit
    assert_eq!(size_label(3_460_300), "3.2 MB");
    // The limit reads as the ten megabytes the card promises.
    assert_eq!(size_label(10 * 1024 * 1024), "10.0 MB");
}

#[test]
fn file_errors_keep_their_own_plain_sentence() {
    let big = friendly_error("io error: That file is too big. The limit is 1 MB.");
    assert_eq!(big.message, "That file is too big. The limit is 1 MB.");
    for sentence in [
        "Stern keeps plain text files only, like .txt, .md, .html, .csv or .json.",
        "That file isn't plain text Stern can read. Save it as a UTF-8 text file and try again.",
        "That text file is in an older format (UTF-16). Save it as a UTF-8 text file and try again.",
    ] {
        let shown = friendly_error(&format!("io error: {sentence}"));
        assert_eq!(shown.message, sentence);
        assert!(shown.detail.is_none());
    }
    assert!(big.detail.is_none());
    // A path never reaches the page, even inside a file error.
    let opened = friendly_error("io error: Couldn't open that file. /Users/sam/secret.txt");
    assert_eq!(opened.message, "Couldn't open that file.");
    assert!(!opened.message.contains("sam"));
}
