// Copyright (c) 2026 Christopher Yang
// SPDX-License-Identifier: MIT

//! A tiny Markdown reader for the in-app Help and Disclaimer pages.
//!
//! The pages are `docs/Help.md` and `docs/Disclaimer.md`, embedded at
//! compile time, so the docs and the app can never disagree. Only the
//! subset those files use is understood: a `# ` title, `## ` headings,
//! `- ` bullet lists, and paragraphs. There is no inline markup —
//! keep those two files free of `**`, backticks, and links.

/// One block of a parsed page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    Title(String),
    Heading(String),
    Paragraph(String),
    Bullets(Vec<String>),
}

/// A heading and the blocks under it, up to the next heading.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    pub heading: String,
    pub body: Vec<Block>,
}

fn flush(blocks: &mut Vec<Block>, paragraph: &mut Vec<String>, bullets: &mut Vec<String>) {
    if !paragraph.is_empty() {
        blocks.push(Block::Paragraph(paragraph.join(" ")));
        paragraph.clear();
    }
    if !bullets.is_empty() {
        blocks.push(Block::Bullets(std::mem::take(bullets)));
    }
}

/// Parse a page into blocks.
#[must_use]
pub fn parse(source: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut paragraph: Vec<String> = Vec::new();
    let mut bullets: Vec<String> = Vec::new();
    for line in source.lines().map(str::trim) {
        if let Some(text) = line.strip_prefix("## ") {
            flush(&mut blocks, &mut paragraph, &mut bullets);
            blocks.push(Block::Heading(text.trim().to_string()));
        } else if let Some(text) = line.strip_prefix("# ") {
            flush(&mut blocks, &mut paragraph, &mut bullets);
            blocks.push(Block::Title(text.trim().to_string()));
        } else if let Some(item) = line.strip_prefix("- ") {
            if !paragraph.is_empty() {
                flush(&mut blocks, &mut paragraph, &mut bullets);
            }
            bullets.push(item.trim().to_string());
        } else if line.is_empty() {
            flush(&mut blocks, &mut paragraph, &mut bullets);
        } else {
            if !bullets.is_empty() {
                flush(&mut blocks, &mut paragraph, &mut bullets);
            }
            paragraph.push(line.to_string());
        }
    }
    flush(&mut blocks, &mut paragraph, &mut bullets);
    blocks
}

/// The page title, if it has one.
#[must_use]
pub fn title(blocks: &[Block]) -> Option<&str> {
    blocks.iter().find_map(|block| match block {
        Block::Title(text) => Some(text.as_str()),
        _ => None,
    })
}

/// Group the blocks after each heading into sections (anything before
/// the first heading, such as the title, is left out).
#[must_use]
pub fn sections(blocks: &[Block]) -> Vec<Section> {
    let mut out: Vec<Section> = Vec::new();
    for block in blocks {
        match block {
            Block::Heading(text) => out.push(Section {
                heading: text.clone(),
                body: Vec::new(),
            }),
            Block::Title(_) => {}
            other => {
                if let Some(section) = out.last_mut() {
                    section.body.push(other.clone());
                }
            }
        }
    }
    out
}
