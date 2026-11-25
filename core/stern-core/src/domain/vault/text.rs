//! The text of a stored file, as the search should see it.
//!
//! Plain text is used as it is. HTML is reduced to what a reader would see:
//! tags go, `<script>` and `<style>` content goes, comments go, and the
//! common entities become their characters. No HTML is parsed beyond that, and
//! nothing here runs or fetches anything.

/// The most text taken from one file for searching, in characters.
pub const MAX_INDEXED_CHARS: usize = 200_000;

/// Whether `name` or the start of `text` says this is HTML.
#[must_use]
pub fn looks_like_html(name: &str, text: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    let ext = std::path::Path::new(&lower)
        .extension()
        .and_then(|e| e.to_str());
    if ext.is_some_and(|e| e.eq_ignore_ascii_case("html") || e.eq_ignore_ascii_case("htm")) {
        return true;
    }
    let head: String = text.trim_start().chars().take(200).collect();
    let head = head.to_ascii_lowercase();
    head.starts_with("<!doctype html") || head.starts_with("<html")
}

/// Replace the common entities with their characters. `&amp;` goes last so
/// that "&amp;lt;" ends as "&lt;" and not "<".
fn decode_entities(text: &str) -> String {
    text.replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// The visible text of an HTML document.
#[must_use]
pub fn visible_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len().checked_div(2).unwrap_or(0));
    let mut rest = html;
    while let Some(open) = rest.find('<') {
        out.push_str(rest.get(..open).unwrap_or(""));
        let after = rest.get(open..).unwrap_or("");
        if after.starts_with("<!--") {
            // A comment runs to its closing marker, or to the end.
            rest = after
                .find("-->")
                .and_then(|end| after.get(end.saturating_add(3)..))
                .unwrap_or("");
            continue;
        }
        let Some(close) = after.find('>') else {
            // A "<" that never closes is just a character.
            out.push_str(after);
            rest = "";
            break;
        };
        let tag = after.get(1..close).unwrap_or("").trim();
        let name: String = tag
            .trim_start_matches('/')
            .chars()
            .take_while(char::is_ascii_alphanumeric)
            .collect::<String>()
            .to_ascii_lowercase();
        let after_tag = after.get(close.saturating_add(1)..).unwrap_or("");
        if (name == "script" || name == "style") && !tag.starts_with('/') {
            // Skip to the matching close tag, or to the end.
            let needle = format!("</{name}");
            let lower = after_tag.to_ascii_lowercase();
            rest = lower
                .find(&needle)
                .and_then(|at| after_tag.get(at..))
                .and_then(|from| from.find('>').and_then(|g| from.get(g.saturating_add(1)..)))
                .unwrap_or("");
            continue;
        }
        // A tag is a word break.
        out.push(' ');
        rest = after_tag;
    }
    out.push_str(rest);
    decode_entities(&out)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// The text to search for a file: HTML reduced to its visible text, anything
/// else as it is, cut to [`MAX_INDEXED_CHARS`].
#[must_use]
pub fn searchable_text(name: &str, text: &str) -> String {
    let base = if looks_like_html(name, text) {
        visible_text(text)
    } else {
        text.to_string()
    };
    base.chars().take(MAX_INDEXED_CHARS).collect()
}
