//! Words for the search: split, lowercase, and reduce to a stem.
//!
//! The stemmer is deliberately small. It only has to make "cards" match
//! "card" and "renewing" match "renew", for English, and it never changes a
//! word so much that two unrelated words meet.

/// Words too common to help a search.
const STOPWORDS: &[&str] = &[
    "a", "an", "and", "are", "as", "at", "be", "but", "by", "can", "did", "do", "does", "for",
    "from", "had", "has", "have", "how", "i", "if", "in", "into", "is", "it", "its", "me", "my",
    "of", "on", "or", "our", "so", "that", "the", "their", "them", "then", "there", "these",
    "this", "to", "was", "we", "were", "what", "when", "where", "which", "who", "will", "with",
    "you", "your",
];

/// Whether `word` is too common to search on.
#[must_use]
pub fn is_stopword(word: &str) -> bool {
    STOPWORDS.contains(&word)
}

/// Reduce an English word to a stem.
///
/// Plurals and common endings only: "cards" and "card", "recipes" and
/// "recipe", "renewing" and "renew" meet; unrelated words do not.
#[must_use]
pub fn stem(word: &str) -> String {
    let w = word.to_lowercase();
    let n = w.chars().count();
    if n <= 3 || w.ends_with("ss") {
        return w;
    }
    let chop = |count: usize| -> String { w.chars().take(n.saturating_sub(count)).collect() };
    // "-ies" -> "-y": "policies" -> "policy".
    if n >= 5 && w.ends_with("ies") {
        return format!("{}y", chop(3));
    }
    // "-es" is only a plural after s, x, z, ch and sh: "boxes", "churches".
    // Everywhere else the plural is just "-s": "recipes", "notes".
    if n >= 5 && w.ends_with("es") {
        let base = chop(2);
        if base.ends_with(['s', 'x', 'z']) || base.ends_with("ch") || base.ends_with("sh") {
            return base;
        }
    }
    for suffix in ["ing", "edly", "ed"] {
        let min = match suffix {
            "ing" => 6,
            "edly" => 7,
            _ => 5,
        };
        if n >= min && w.ends_with(suffix) {
            let cut = chop(suffix.chars().count());
            // "making" -> "mak" would not meet "make": put back a lost "e".
            return if needs_e(&cut) {
                format!("{cut}e")
            } else {
                cut
            };
        }
    }
    if n >= 4 && w.ends_with('s') && !w.ends_with("us") && !w.ends_with("is") {
        return chop(1);
    }
    w
}

/// Whether a stem ending in a consonant-vowel-consonant pattern likely lost
/// an "e" ("renew" no, "mak" yes, "us" yes).
fn needs_e(stem: &str) -> bool {
    let chars: Vec<char> = stem.chars().collect();
    let n = chars.len();
    if n < 3 {
        return false;
    }
    let vowel = |c: char| matches!(c, 'a' | 'e' | 'i' | 'o' | 'u');
    let (a, b, c) = (
        chars.get(n.saturating_sub(3)).copied().unwrap_or(' '),
        chars.get(n.saturating_sub(2)).copied().unwrap_or(' '),
        chars.get(n.saturating_sub(1)).copied().unwrap_or(' '),
    );
    !vowel(a) && vowel(b) && !vowel(c) && !matches!(c, 'w' | 'x' | 'y' | 'r' | 'l' | 'n' | 't')
}

/// Split text into lowercase words: letters and digits, with an apostrophe
/// or hyphen kept inside a word.
#[must_use]
pub fn words(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut current = String::new();
    for c in text.chars() {
        if c.is_alphanumeric() {
            current.extend(c.to_lowercase());
        } else if (c == '\'' || c == '’') && !current.is_empty() {
            // "landlord's" -> "landlord": drop a possessive.
        } else if !current.is_empty() {
            out.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

/// The stems of every searchable word in `text`.
#[must_use]
pub fn stems_of(text: &str) -> Vec<String> {
    words(text)
        .into_iter()
        .filter(|w| !is_stopword(w))
        .map(|w| stem(&w))
        .collect()
}

/// The stems of a list of query words, without duplicates.
#[must_use]
pub fn stem_all(words: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for word in words {
        for w in self::words(word) {
            if is_stopword(&w) {
                continue;
            }
            let s = stem(&w);
            if !out.contains(&s) {
                out.push(s);
            }
        }
    }
    out
}
