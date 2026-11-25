//! What an entry says about itself: a one-line summary and a few keywords.
//!
//! Both live in the entry's encrypted payload. These are the rules for
//! cleaning what a person types, and for the keyword suggestion that works
//! with no model. The suggestion keeps only plain words and skips anything
//! that could be a secret, because suggested keywords are shown on screen.

/// The longest summary, in characters.
pub const MAX_SUMMARY_CHARS: usize = 160;

/// The most keywords an entry may carry.
pub const MAX_KEYWORDS: usize = 8;

/// The longest single keyword, in characters.
pub const MAX_KEYWORD_CHARS: usize = 40;

/// A summary ready to store: whitespace collapsed to single spaces, then cut
/// to [`MAX_SUMMARY_CHARS`] on a character boundary.
#[must_use]
pub fn clean_summary(summary: &str) -> String {
    let collapsed = summary.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed.chars().take(MAX_SUMMARY_CHARS).collect()
}

/// Keywords ready to store.
///
/// Trimmed, lowercased, no empties, no duplicates, each cut to
/// [`MAX_KEYWORD_CHARS`], at most [`MAX_KEYWORDS`] in all. An item may hold
/// several keywords separated by commas or semicolons.
#[must_use]
pub fn clean_keywords<S: AsRef<str>>(keywords: &[S]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for item in keywords {
        for part in item.as_ref().split([',', ';', '\n']) {
            let word: String = part
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .to_lowercase()
                .chars()
                .take(MAX_KEYWORD_CHARS)
                .collect();
            if !word.is_empty() && !out.contains(&word) {
                out.push(word);
            }
            if out.len() >= MAX_KEYWORDS {
                return out;
            }
        }
    }
    out
}

/// Words that say nothing about a topic.
const STOPWORDS: &[&str] = &[
    "about", "above", "after", "again", "also", "because", "been", "before", "being", "below",
    "between", "both", "could", "does", "doing", "down", "during", "each", "from", "further",
    "have", "having", "here", "himself", "herself", "into", "just", "like", "more", "most", "much",
    "must", "only", "other", "ours", "over", "same", "should", "some", "such", "than", "that",
    "their", "theirs", "them", "then", "there", "these", "they", "this", "those", "through",
    "under", "until", "very", "want", "were", "what", "when", "where", "which", "while", "will",
    "with", "would", "your", "yours", "note", "notes", "text", "file",
];

/// A word that announces a secret value right after it.
const SECRET_CUES: &[&str] = &[
    "password",
    "passwords",
    "passcode",
    "passphrase",
    "pin",
    "pwd",
    "secret",
    "token",
    "key",
    "code",
    "cvv",
    "cvc",
    "ssn",
    "seed",
    "login",
];

/// Up to [`MAX_KEYWORDS`] plain words from `text`, most frequent first.
///
/// Only purely alphabetic words of four to twenty letters are considered,
/// so numbers, tokens and most secrets are never suggested, and a word that
/// directly follows a secret cue ("password hunter") is skipped as well.
/// Ties keep the order the words first appear in.
#[must_use]
pub fn suggest_keywords(text: &str) -> Vec<String> {
    let mut counts: Vec<(String, usize)> = Vec::new();
    let mut previous_was_cue = false;
    for raw in text.split(|c: char| !c.is_alphanumeric()) {
        if raw.is_empty() {
            continue;
        }
        let word = raw.to_lowercase();
        let is_cue = SECRET_CUES.contains(&word.as_str());
        let skip_for_cue = previous_was_cue;
        previous_was_cue = is_cue;
        let plain = word.chars().all(char::is_alphabetic)
            && (4..=20).contains(&word.chars().count())
            && !STOPWORDS.contains(&word.as_str())
            && !is_cue;
        if !plain || skip_for_cue {
            continue;
        }
        if let Some(entry) = counts.iter_mut().find(|(w, _)| *w == word) {
            entry.1 = entry.1.saturating_add(1);
        } else {
            counts.push((word, 1));
        }
    }
    // A stable sort keeps first-seen order among equal counts.
    counts.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    counts
        .into_iter()
        .take(MAX_KEYWORDS)
        .map(|(word, _)| word)
        .collect()
}
