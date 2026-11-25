//! Finding entries by name, by what they are about, and by when.
//!
//! The search works on [`SearchDoc`]s the caller builds from decrypted text
//! while the vault is unlocked: names, summaries, keywords, note text and
//! non-hidden fields. Hidden fields are never put in a doc. Nothing here
//! touches the database or a key, and all scoring is whole numbers, so a
//! search gives the same answer every time.
//!
//! Ranking is by field weight (a name counts most, then keywords and the
//! summary, then body text), by how rare a word is across the vault, and by
//! how many of the query's words an entry matches. An entry must match most of
//! the query's words to be found at all, so one common word cannot flood the
//! list.

pub mod tokens;

use crate::domain::vault::query::{DateField, DateRange, Query};
use chrono::{DateTime, Utc};
use tokens::{stem_all, stems_of};

/// The most candidates a search returns.
pub const MAX_RESULTS: usize = 5;

/// What the search knows about one entry.
#[derive(Debug, Clone)]
pub struct SearchDoc {
    pub id: String,
    pub name: String,
    pub kind: Kind,
    /// The folder path as names from the top, e.g. `["Money", "Taxes"]`.
    pub folder: Vec<String>,
    pub summary: String,
    pub keywords: Vec<String>,
    /// Body text and non-hidden fields, never hidden fields.
    pub text: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// The kind of an entry, as the search filters by it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Note,
    Document,
    Login,
}

impl Kind {
    /// The word for it in a listing.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Note => "note",
            Self::Document => "document",
            Self::Login => "login",
        }
    }

    fn from_word(word: &str) -> Option<Self> {
        match word {
            "note" | "notes" => Some(Self::Note),
            "document" | "documents" | "file" | "files" => Some(Self::Document),
            "login" | "logins" | "password" | "passwords" | "account" | "accounts" => {
                Some(Self::Login)
            }
            _ => None,
        }
    }
}

/// One candidate, with why it was chosen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub id: String,
    pub score: u32,
    /// How many of the query's words it matched, out of how many.
    pub matched: usize,
    pub wanted: usize,
}

/// What a search found, and what it did to find it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub hits: Vec<Hit>,
    /// The dates actually used, if the query had a time.
    pub range: Option<DateRange>,
    /// True when nothing matched inside the asked-for time and the result
    /// comes from a wider one.
    pub widened: bool,
    /// True when the time was dropped to find anything.
    pub ignored_time: bool,
}

const NAME_EXACT: u32 = 1000;
const NAME_WORD: u32 = 120;
const KEYWORD_WORD: u32 = 80;
const SUMMARY_WORD: u32 = 60;
const FOLDER_WORD: u32 = 30;
const BODY_WORD: u32 = 25;
const SOFT_WORD: u32 = 15;

/// How many of `wanted` words an entry must match: all of up to two, then
/// two thirds rounded up.
fn needed(wanted: usize) -> usize {
    match wanted {
        0 => 0,
        1 | 2 => wanted,
        n => n
            .saturating_mul(2)
            .saturating_add(2)
            .checked_div(3)
            .unwrap_or(n),
    }
}

/// An integer stand-in for rarity: words found in fewer entries count more.
/// Between 10 (in every entry) and 30 (in one).
fn rarity(total: usize, containing: usize) -> u32 {
    if containing == 0 || total == 0 {
        return 30;
    }
    let ratio = containing
        .saturating_mul(100)
        .checked_div(total)
        .unwrap_or(0);
    match ratio {
        0..=5 => 30,
        6..=15 => 24,
        16..=40 => 18,
        _ => 10,
    }
}

struct Stems {
    name: Vec<String>,
    keywords: Vec<String>,
    summary: Vec<String>,
    folder: Vec<String>,
    body: Vec<String>,
}

impl Stems {
    fn of(doc: &SearchDoc) -> Self {
        Self {
            name: stems_of(&doc.name),
            keywords: doc.keywords.iter().flat_map(|k| stems_of(k)).collect(),
            summary: stems_of(&doc.summary),
            folder: doc.folder.iter().flat_map(|f| stems_of(f)).collect(),
            body: stems_of(&doc.text),
        }
    }

    fn any_contains(&self, stem: &str) -> bool {
        [
            &self.name,
            &self.keywords,
            &self.summary,
            &self.folder,
            &self.body,
        ]
        .iter()
        .any(|field| field.iter().any(|s| s == stem))
    }
}

fn count(field: &[String], stem: &str) -> u32 {
    let n = field.iter().filter(|s| *s == stem).count();
    u32::try_from(n.min(3)).unwrap_or(3)
}

/// The score of one doc for the query's stems, and how many it matched.
fn score_doc(
    doc: &SearchDoc,
    stems: &Stems,
    wanted: &[String],
    soft: &[String],
    weight: &[u32],
    exact_name: bool,
) -> (u32, usize) {
    let mut score = 0u32;
    let mut matched = 0usize;
    for (stem, rare) in wanted.iter().zip(weight) {
        let name_hits = count(&stems.name, stem);
        let kw_hits = count(&stems.keywords, stem);
        let sum_hits = count(&stems.summary, stem);
        let folder_hits = count(&stems.folder, stem);
        // Body hits add little past the first few, so a long file does not
        // beat a note that is plainly about the thing.
        let body_hits = count(&stems.body, stem);
        let here = name_hits
            .saturating_mul(NAME_WORD)
            .saturating_add(kw_hits.min(1).saturating_mul(KEYWORD_WORD))
            .saturating_add(sum_hits.min(1).saturating_mul(SUMMARY_WORD))
            .saturating_add(folder_hits.min(1).saturating_mul(FOLDER_WORD))
            .saturating_add(body_hits.saturating_mul(BODY_WORD));
        if here > 0 {
            matched = matched.saturating_add(1);
            score = score.saturating_add(here.saturating_mul(*rare).checked_div(20).unwrap_or(0));
        }
    }
    for stem in soft {
        if stems.any_contains(stem) {
            score = score.saturating_add(SOFT_WORD);
        }
    }
    if exact_name {
        score = score.saturating_add(NAME_EXACT);
    }
    // A fuller match always outranks a partial one.
    score = score.saturating_add(u32::try_from(matched).unwrap_or(0).saturating_mul(200));
    let _ = doc;
    (score, matched)
}

fn in_scope(doc: &SearchDoc, query: &Query, range: Option<&DateRange>) -> bool {
    if let Some(kind) = query.kind.as_deref().and_then(Kind::from_word) {
        if doc.kind != kind {
            return false;
        }
    }
    if let Some(range) = range {
        let when = match query.date_field {
            Some(DateField::Modified) => doc.updated_at,
            _ => doc.created_at,
        };
        if !range.contains(when) {
            return false;
        }
    }
    true
}

/// Run `query` over `docs` as of `now`. `folder_ok` says whether a doc is
/// inside the folder the query named (the caller knows the folder tree).
#[must_use]
pub fn run(
    docs: &[SearchDoc],
    query: &Query,
    now: DateTime<Utc>,
    folder_ok: &dyn Fn(&SearchDoc) -> bool,
) -> Outcome {
    let asked = query.time.and_then(|t| t.resolve(now));
    let mut outcome = attempt(docs, query, asked.as_ref(), folder_ok);
    let mut range = asked;
    let mut widened = false;
    let mut ignored_time = false;
    // Only widen when there are words to look for. "What I saved yesterday"
    // with nothing saved yesterday has a clear answer: nothing.
    if outcome.is_empty() && !query.terms.is_empty() {
        if let Some(wide) = asked.map(|r| r.widened()) {
            let second = attempt(docs, query, Some(&wide), folder_ok);
            if second.is_empty() {
                let third = attempt(docs, query, None, folder_ok);
                if !third.is_empty() {
                    outcome = third;
                    range = None;
                    ignored_time = true;
                }
            } else {
                outcome = second;
                range = Some(wide);
                widened = true;
            }
        }
    }
    Outcome {
        hits: outcome,
        range,
        widened,
        ignored_time,
    }
}

fn attempt(
    docs: &[SearchDoc],
    query: &Query,
    range: Option<&DateRange>,
    folder_ok: &dyn Fn(&SearchDoc) -> bool,
) -> Vec<Hit> {
    let wanted = stem_all(&query.terms);
    let soft = stem_all(&query.soft_terms);
    let need = needed(wanted.len());
    let candidates: Vec<&SearchDoc> = docs
        .iter()
        .filter(|d| in_scope(d, query, range) && folder_ok(d))
        .collect();
    let all_stems: Vec<Stems> = docs.iter().map(Stems::of).collect();
    let weight: Vec<u32> = wanted
        .iter()
        .map(|stem| {
            let containing = all_stems.iter().filter(|s| s.any_contains(stem)).count();
            rarity(docs.len(), containing)
        })
        .collect();
    let joined_query = wanted.join(" ");
    let mut hits: Vec<(Hit, DateTime<Utc>)> = Vec::new();
    for doc in candidates {
        let stems = Stems::of(doc);
        if wanted.is_empty() {
            // Only filters were given: everything in scope is a candidate,
            // newest first.
            hits.push((
                Hit {
                    id: doc.id.clone(),
                    score: 1,
                    matched: 0,
                    wanted: 0,
                },
                doc.created_at,
            ));
            continue;
        }
        let exact_name = stems.name.join(" ") == joined_query;
        let (score, matched) = score_doc(doc, &stems, &wanted, &soft, &weight, exact_name);
        if matched >= need && matched > 0 {
            hits.push((
                Hit {
                    id: doc.id.clone(),
                    score,
                    matched,
                    wanted: wanted.len(),
                },
                doc.created_at,
            ));
        }
    }
    // Best score first; ties go to the newest, then to the id, so the order
    // never depends on the order the docs were given in.
    hits.sort_by(|(a, a_at), (b, b_at)| {
        b.score
            .cmp(&a.score)
            .then(b_at.cmp(a_at))
            .then(a.id.cmp(&b.id))
    });
    hits.into_iter().map(|(hit, _)| hit).collect()
}

/// A folder the query named that could not be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FolderProblem {
    /// No folder has that name.
    Unknown(String),
    /// Several folders share the name; these are their full paths.
    Ambiguous(Vec<String>),
}

/// The result of reading a sentence and searching with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub query: Query,
    pub outcome: Outcome,
    /// Set when the request named a folder that cannot be used.
    pub folder_problem: Option<FolderProblem>,
    /// Set when the words of the request name a folder and nothing else
    /// matched: the entries directly or indirectly in it.
    pub folder_view: Option<String>,
}

const fn empty_outcome() -> Outcome {
    Outcome {
        hits: Vec::new(),
        range: None,
        widened: false,
        ignored_time: false,
    }
}

/// Read `text` and search `docs` with it, as of `now`.
///
/// A folder clause narrows the search to that folder and everything below
/// it. When nothing matches and the words themselves name a folder
/// ("what's in Cards"), the folder's entries are returned instead.
#[must_use]
pub fn find(
    docs: &[SearchDoc],
    folders: &[crate::domain::vault::folder::FolderNode],
    text: &str,
    now: DateTime<Utc>,
) -> Found {
    use crate::domain::vault::folder::{is_within_path, resolve, Resolution};
    let query = crate::ai::query::query_from_text(text);
    let mut folder_id: Option<String> = None;
    if let Some(reference) = query.folder.as_deref() {
        match resolve(folders, reference) {
            Resolution::One(id) => folder_id = Some(id),
            Resolution::None => {
                return Found {
                    folder_problem: Some(FolderProblem::Unknown(reference.to_string())),
                    outcome: empty_outcome(),
                    folder_view: None,
                    query,
                };
            }
            Resolution::Many(paths) => {
                return Found {
                    folder_problem: Some(FolderProblem::Ambiguous(paths)),
                    outcome: empty_outcome(),
                    folder_view: None,
                    query,
                };
            }
        }
    }
    let in_folder = |doc: &SearchDoc| {
        folder_id
            .as_deref()
            .is_none_or(|id| is_within_path(folders, &doc.folder, id))
    };
    let outcome = run(docs, &query, now, &in_folder);
    if outcome.hits.is_empty() && folder_id.is_none() && !query.terms.is_empty() {
        let reference = query.terms.join(" ");
        if let Resolution::One(id) = resolve(folders, &reference) {
            let inside = |doc: &SearchDoc| is_within_path(folders, &doc.folder, &id);
            let no_terms = Query::default();
            let listed = run(docs, &no_terms, now, &inside);
            return Found {
                query,
                outcome: listed,
                folder_problem: None,
                folder_view: Some(id),
            };
        }
    }
    Found {
        query,
        outcome,
        folder_problem: None,
        folder_view: None,
    }
}
