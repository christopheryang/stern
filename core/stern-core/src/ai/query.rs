//! From a sentence to a [`Query`]: what to look for, where, and when.
//!
//! The sentence is read in three steps. A time phrase is taken out first
//! (see [`crate::ai::dates`]). A folder clause ("in the Taxes folder", "under
//! Work/Meetings") is taken out next. What is left is split into search
//! terms and filler; the filler is the polite and question words that say
//! how the request was asked but nothing about what to find.

use crate::ai::dates::take_time;
use crate::domain::vault::query::Query;
use crate::search::tokens::is_stopword;

/// Words that only frame a request ("could you find", "I stored", "tell me").
const FILLER: &[&str] = &[
    "find",
    "get",
    "show",
    "open",
    "search",
    "look",
    "looking",
    "locate",
    "fetch",
    "retrieve",
    "give",
    "tell",
    "list",
    "display",
    "see",
    "view",
    "read",
    "bring",
    "pull",
    "check",
    "need",
    "want",
    "wanna",
    "like",
    "please",
    "kindly",
    "could",
    "would",
    "can",
    "should",
    "may",
    "might",
    "let",
    "lets",
    "know",
    "remember",
    "recall",
    "forgot",
    "forget",
    "lost",
    "lose",
    "stored",
    "store",
    "saved",
    "save",
    "put",
    "wrote",
    "write",
    "written",
    "kept",
    "keep",
    "added",
    "made",
    "created",
    "something",
    "anything",
    "everything",
    "stuff",
    "thing",
    "things",
    "some",
    "any",
    "about",
    "regarding",
    "concerning",
    "info",
    "information",
    "details",
    "detail",
    "data",
    "up",
    "out",
    "back",
    "ago",
    "also",
    "just",
    "really",
    "pls",
    "plz",
    "thanks",
    "thank",
    "mine",
    "own",
    "again",
    "still",
    "ever",
    "here",
    "yes",
    "ok",
    "okay",
    "hey",
    "hi",
    "hello",
    "tell",
    "told",
    "ask",
    "asked",
    "show",
    "shown",
    "does",
    "did",
    "doing",
    "done",
    "been",
    "being",
    "am",
    "im",
    "ive",
    "id",
    "ill",
    "dont",
    "didnt",
    "cant",
    "wont",
    "couldnt",
    "wouldnt",
    "whats",
    "thats",
    "theres",
    "heres",
    "whos",
    "named",
    "called",
    "titled",
    "entry",
    "entries",
    "vault",
    "secret",
    "secrets",
    "one",
    "ones",
    "first",
    "latest",
    "newest",
    "recent",
    "recently",
    "earlier",
    "before",
    "after",
    "during",
    "within",
    "over",
    "under",
    "inside",
    "contains",
    "containing",
    "contain",
    "says",
    "say",
    "said",
    "mentions",
    "mention",
    "mentioning",
    "mentioned",
    "talks",
    "talk",
    "talking",
    "regard",
    "re",
    "how",
    "why",
    "whom",
    "whose",
    "whether",
    "much",
    "many",
    "most",
    "more",
    "less",
    "go",
    "going",
    "come",
    "coming",
    "get",
    "got",
    "getting",
    "take",
    "took",
    "taken",
    "use",
    "used",
    "using",
    "make",
    "makes",
    "making",
    "mean",
    "means",
    "meant",
    "put",
    "set",
    "lot",
    "lots",
    "bit",
    "kind",
    "sort",
    "type",
    "such",
];

/// Words that name a kind of entry. They narrow the search ("my passport
/// note") when they stand alone, but only help rank when part of a name.
const KIND_WORDS: &[&str] = &[
    "note",
    "notes",
    "document",
    "documents",
    "file",
    "files",
    "login",
    "logins",
];

/// Words that ask for a value held in an entry rather than naming it.
const VALUE_WORDS: &[&str] = &[
    "password",
    "passwords",
    "passcode",
    "pin",
    "number",
    "numbers",
    "amount",
    "date",
    "dates",
    "code",
    "codes",
];

/// Words that begin a folder clause.
const FOLDER_LEADS: &[&str] = &["in", "inside", "under", "within", "from"];

fn is_filler(word: &str) -> bool {
    FILLER.contains(&word) || is_stopword(word)
}

/// Split off a folder clause: "... in the Taxes folder", "... under
/// Work/Meetings". Returns the words with the clause removed, and the folder
/// reference. A clause needs the word "folder", or a path with "/" or ">",
/// so "in the lease" is not mistaken for a folder.
fn take_folder(words: Vec<String>, original: &[&str]) -> (Vec<String>, Option<String>) {
    let lower: Vec<String> = original
        .iter()
        .map(|w| {
            w.trim_matches(|c: char| !c.is_alphanumeric() && c != '/' && c != '>')
                .to_string()
        })
        .collect();
    for (i, word) in lower.iter().enumerate() {
        if !FOLDER_LEADS.contains(&word.to_lowercase().as_str()) {
            continue;
        }
        let tail: Vec<&String> = lower.iter().skip(i.saturating_add(1)).collect();
        // "in the Taxes folder" / "in Taxes folder"
        if let Some(pos) = tail.iter().position(|w| w.eq_ignore_ascii_case("folder")) {
            let name: Vec<&str> = tail
                .iter()
                .take(pos)
                .map(|w| w.as_str())
                .filter(|w| !matches!(w.to_lowercase().as_str(), "the" | "my" | "a"))
                .collect();
            if !name.is_empty() {
                let drop = i.saturating_add(pos).saturating_add(2);
                return (
                    remove_range(&words, original, i, drop),
                    Some(name.join(" ")),
                );
            }
        }
        // "under Work/Meetings", "in Taxes > 2024"
        if let Some(next) = tail.first() {
            if next.contains('/') || next.contains('>') {
                let drop = i.saturating_add(2);
                return (
                    remove_range(&words, original, i, drop),
                    Some((*next).clone()),
                );
            }
        }
    }
    // "show the Insurance folder", "open the Taxes folder": the word before
    // "folder" is the folder.
    if let Some(i) = lower.iter().position(|w| w.eq_ignore_ascii_case("folder")) {
        let before = lower.get(..i).unwrap_or_default();
        if let Some(name) = before.last() {
            let lead = before.first().map(|w| w.to_lowercase()).unwrap_or_default();
            let asks = matches!(
                lead.as_str(),
                "show" | "open" | "view" | "list" | "see" | "display"
            );
            if asks && !FOLDER_LEADS.contains(&name.to_lowercase().as_str()) {
                return (
                    remove_range(&words, original, i.saturating_sub(1), i.saturating_add(1)),
                    Some(name.clone()),
                );
            }
        }
    }
    (words, None)
}

/// Remove words `[from, to)` by their position in the original sentence from
/// the already-cleaned `words` (which dropped the time phrase). Cleaned words
/// are matched by value, front to back.
fn remove_range(words: &[String], original: &[&str], from: usize, to: usize) -> Vec<String> {
    let mut gone: Vec<String> = original
        .iter()
        .enumerate()
        .filter(|(i, _)| *i >= from && *i < to)
        .map(|(_, w)| {
            w.trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase()
        })
        .collect();
    words
        .iter()
        .filter(|w| {
            gone.iter().position(|g| g == *w).is_none_or(|pos| {
                gone.remove(pos);
                false
            })
        })
        .cloned()
        .collect()
}

/// Read a sentence as a [`Query`].
#[must_use]
pub fn query_from_text(text: &str) -> Query {
    let original: Vec<&str> = text.split_whitespace().collect();
    // "what's" and "where's" are filler once their apostrophe is gone.
    let timed = take_time(&original);
    let (rest, folder) = take_folder(timed.rest, &original);

    let mut terms: Vec<String> = Vec::new();
    let mut soft: Vec<String> = Vec::new();
    let mut kind: Option<String> = None;
    for word in &rest {
        let w = word.trim();
        let plain: String = w
            .chars()
            .filter(|c| *c != '\'' && *c != '\u{2019}')
            .collect();
        if w.is_empty() || is_filler(w) || is_filler(&plain) {
            continue;
        }
        // "folder" in "show the Insurance folder" says where, not what.
        if w == "folder" || w == "folders" {
            continue;
        }
        if KIND_WORDS.contains(&w) {
            // A kind word narrows the search only when nothing else says
            // what to find; otherwise it just helps rank ("dentist note").
            soft.push(w.to_string());
            kind.get_or_insert_with(|| w.to_string());
            continue;
        }
        if !terms.contains(&w.to_string()) {
            terms.push(w.to_string());
        }
    }
    // With terms present, the kind word is a hint, not a filter: "the
    // passport note" must still find a note that is not typed "note".
    if !terms.is_empty() {
        kind = None;
    }
    // Words that ask for a value ("router password", "rent amount") say what
    // is wanted from the entry, not what it is called. They help rank but
    // are not required, because the value itself is never in the search
    // text. They stay required when nothing else was asked for.
    if terms.iter().any(|t| !VALUE_WORDS.contains(&t.as_str())) {
        let (value, plain): (Vec<String>, Vec<String>) = terms
            .into_iter()
            .partition(|t| VALUE_WORDS.contains(&t.as_str()));
        terms = plain;
        soft.extend(value);
    }
    Query {
        terms,
        soft_terms: soft,
        folder,
        kind,
        time: timed.time,
        date_field: timed.date_field,
    }
}
