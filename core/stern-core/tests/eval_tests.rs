#![allow(clippy::unwrap_used)]

//! The search evaluation: a synthetic vault of 60 entries and 85
//! questions (see `tests/eval/`), scored against the real query reader and
//! search.
//!
//! The score is a baseline, not a pass mark. It is printed with
//! `cargo test -p stern-core --test eval_tests -- --nocapture`, and the
//! floors asserted below only stop it from getting worse. Later phases raise
//! them as search improves (see the Vault Assistant design).

use chrono::{DateTime, Duration, TimeZone, Utc};
use stern_core::ai::intent::{classify_folder_intent, Intent};
use stern_core::domain::vault::folder::FolderNode;
use stern_core::search::{find, Kind, SearchDoc};

const VAULT: &str = include_str!("eval/vault.txt");
const QUESTIONS: &str = include_str!("eval/questions.txt");
const HOLDOUT: &str = include_str!("eval/holdout.txt");

struct Item {
    id: String,
    name: String,
    kind: &'static str,
    days_ago: i64,
    folder: Option<String>,
    text: String,
    hidden: String,
}

struct Question {
    category: String,
    text: String,
    expected: Expected,
}

enum Expected {
    Any(Vec<String>),
    DaysBetween(i64, i64),
    Nothing,
}

fn rows(source: &str) -> impl Iterator<Item = Vec<String>> + '_ {
    source
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .map(|line| {
            let mut cells: Vec<String> = line.split('|').map(|c| c.trim().to_string()).collect();
            // A trailing "|" with nothing after it is an empty last cell.
            if line.trim_end().ends_with('|') {
                cells.pop();
            }
            cells
        })
}

fn kind_of(word: &str) -> &'static str {
    match word {
        "login" => "login",
        "document" => "document",
        _ => "note",
    }
}

fn items() -> Vec<Item> {
    rows(VAULT)
        .map(|cols| {
            assert!(cols.len() >= 6, "bad vault row: {cols:?}");
            assert!(cols.len() <= 7, "bad vault row: {cols:?}");
            Item {
                id: cols[0].clone(),
                name: cols[1].clone(),
                kind: kind_of(&cols[2]),
                days_ago: cols[3].parse().unwrap(),
                folder: (!cols[4].is_empty()).then(|| cols[4].clone()),
                text: cols[5].clone(),
                hidden: cols.get(6).cloned().unwrap_or_default(),
            }
        })
        .collect()
}

fn questions() -> Vec<Question> {
    parse_questions(QUESTIONS)
}

fn holdout() -> Vec<Question> {
    parse_questions(HOLDOUT)
}

fn parse_questions(source: &str) -> Vec<Question> {
    rows(source)
        .map(|cols| {
            assert_eq!(cols.len(), 3, "bad question row: {cols:?}");
            let expected = if cols[2] == "-" {
                Expected::Nothing
            } else if let Some(range) = cols[2].strip_prefix("@days:") {
                let (from, to) = range.split_once('-').unwrap();
                Expected::DaysBetween(from.parse().unwrap(), to.parse().unwrap())
            } else {
                Expected::Any(cols[2].split(',').map(|s| s.trim().to_string()).collect())
            };
            Question {
                category: cols[0].clone(),
                text: cols[1].clone(),
                expected,
            }
        })
        .collect()
}

/// The folder tree the vault's folder paths describe.
fn folder_nodes(items: &[Item]) -> Vec<FolderNode> {
    let mut nodes: Vec<FolderNode> = Vec::new();
    for path in items.iter().filter_map(|i| i.folder.as_deref()) {
        let mut parent: Option<String> = None;
        let mut so_far = String::new();
        for part in path.split('/') {
            so_far = format!("{so_far}/{part}");
            if !nodes.iter().any(|n| n.id == so_far) {
                nodes.push(FolderNode {
                    id: so_far.clone(),
                    parent: parent.clone(),
                    name: part.to_string(),
                });
            }
            parent = Some(so_far.clone());
        }
    }
    nodes
}

/// The clock the questions are asked against: the vault's "days ago" are
/// counted back from this.
fn eval_now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 25, 12, 0, 0).unwrap()
}

fn docs(items: &[Item]) -> Vec<SearchDoc> {
    items
        .iter()
        .map(|i| {
            let created = eval_now() - Duration::days(i.days_ago);
            SearchDoc {
                id: i.id.clone(),
                name: i.name.clone(),
                kind: match i.kind {
                    "login" => Kind::Login,
                    "document" => Kind::Document,
                    _ => Kind::Note,
                },
                folder: i
                    .folder
                    .as_deref()
                    .map(|f| f.split('/').map(str::to_string).collect())
                    .unwrap_or_default(),
                summary: String::new(),
                keywords: Vec::new(),
                text: i.text.clone(),
                created_at: created,
                updated_at: created,
            }
        })
        .collect()
}

/// Search as the app does. Folder questions ("what's in the Taxes folder")
/// are answered by the folder view, which lists a folder's entries, as the
/// chat does. Returns entry ids, best first.
fn search_now(question: &str, docs: &[SearchDoc], nodes: &[FolderNode]) -> Vec<String> {
    let asked = match classify_folder_intent(question) {
        Some(Intent::ShowFolders {
            folder: Some(reference),
        }) => format!("everything in the {reference} folder"),
        _ => question.to_string(),
    };
    find(docs, nodes, &asked, eval_now())
        .outcome
        .hits
        .into_iter()
        .map(|h| h.id)
        .collect()
}

fn passes(expected: &Expected, found: &[String], items: &[Item]) -> bool {
    let top: Vec<&String> = found.iter().take(3).collect();
    match expected {
        Expected::Nothing => found.is_empty(),
        Expected::Any(ids) => top.iter().any(|id| ids.contains(id)),
        Expected::DaysBetween(from, to) => {
            !top.is_empty()
                && top.iter().all(|id| {
                    items
                        .iter()
                        .find(|i| &i.id == *id)
                        .is_some_and(|i| i.days_ago >= *from && i.days_ago <= *to)
                })
        }
    }
}

#[test]
fn the_evaluation_data_is_sound() {
    let items = items();
    assert_eq!(items.len(), 60);
    let mut ids: Vec<&str> = items.iter().map(|i| i.id.as_str()).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), items.len(), "duplicate entry ids");

    // Every question points at entries that exist.
    for q in questions() {
        if let Expected::Any(expected) = &q.expected {
            for id in expected {
                assert!(ids.contains(&id.as_str()), "{}: unknown id {id}", q.text);
            }
        }
    }
    // A hidden value (a card or account number, a password) never appears in
    // an entry's name or text, so no search can leak one by matching it.
    for item in &items {
        if item.hidden.is_empty() {
            continue;
        }
        assert!(!item.name.contains(&item.hidden), "{}", item.id);
        assert!(!item.text.contains(&item.hidden), "{}", item.id);
    }
    // The planted look-alikes the design calls for.
    let names: Vec<&str> = items.iter().map(|i| i.name.as_str()).collect();
    assert!(
        names
            .iter()
            .filter(|n| n.starts_with("Meeting notes"))
            .count()
            >= 3
    );
    assert!(items
        .iter()
        .any(|i| i.name == "Misc 3" && i.text.contains("Passport")));
}

#[test]
fn the_questions_cover_every_kind_of_request() {
    let questions = questions();
    assert!(questions.len() >= 80, "only {} questions", questions.len());
    for category in ["name", "topic", "time", "folder", "none"] {
        assert!(
            questions.iter().filter(|q| q.category == category).count() >= 5,
            "too few {category} questions"
        );
    }
    // The question that started all this is in the set.
    assert!(questions.iter().any(|q| q
        .text
        .contains("something about my credit card a couple of months ago")));
}

/// Score today's search and print how it does, by kind of question.
#[test]
fn baseline_search_score() {
    let items = items();
    let nodes = folder_nodes(&items);
    let docs = docs(&items);
    let questions = questions();

    let mut totals: Vec<(String, usize, usize)> = Vec::new();
    let mut misses: Vec<String> = Vec::new();
    for q in &questions {
        let found = search_now(&q.text, &docs, &nodes);
        let ok = passes(&q.expected, &found, &items);
        match totals.iter_mut().find(|(c, _, _)| *c == q.category) {
            Some(row) => {
                row.1 += 1;
                row.2 += usize::from(ok);
            }
            None => totals.push((q.category.clone(), 1, usize::from(ok))),
        }
        if !ok {
            misses.push(format!("[{}] {}", q.category, q.text));
        }
    }

    println!("\nSearch, top three, {} questions", questions.len());
    let (mut all, mut all_ok) = (0, 0);
    for (category, count, ok) in &totals {
        println!("  {category:<7} {ok:>2} / {count:<2}");
        all += count;
        all_ok += ok;
    }
    println!("  total   {all_ok:>2} / {all}\n");
    for miss in misses.iter().take(60) {
        println!("  miss {miss}");
    }

    // Floors: today's measured behaviour, so it cannot quietly get worse.
    let rate = |category: &str| {
        let (_, count, ok) = totals.iter().find(|(c, _, _)| c == category).unwrap();
        (*ok, *count)
    };
    let (none_ok, none_count) = rate("none");
    assert_eq!(
        none_ok, none_count,
        "a question about nothing must find nothing"
    );
    let (name_ok, _) = rate("name");
    assert!(name_ok >= NAME_FLOOR, "name search got worse: {name_ok}");
    assert!(all_ok >= TOTAL_FLOOR, "overall search got worse: {all_ok}");
}

/// Measured on 2026-11-25. Before any search work (name matching only): 10 of
/// 16 name questions and 23 of 85 overall. After phase 2: 16 of 16 and 85 of
/// 85, on the questions the search was tuned with. Raise these as search
/// improves; never lower them.
const NAME_FLOOR: usize = 16;
const TOTAL_FLOOR: usize = 85;

/// Questions written after the search was tuned. A search fitted to the main
/// set scores much worse here, so this is the honest number.
#[test]
fn holdout_search_score() {
    let items = items();
    let nodes = folder_nodes(&items);
    let docs = docs(&items);
    let questions = holdout();
    let mut ok = 0;
    let mut misses = Vec::new();
    for q in &questions {
        let found = search_now(&q.text, &docs, &nodes);
        if passes(&q.expected, &found, &items) {
            ok += 1;
        } else {
            misses.push(format!("[{}] {}", q.category, q.text));
        }
    }
    println!("\nHoldout, top three: {ok} / {}", questions.len());
    for miss in &misses {
        println!("  miss {miss}");
    }
    assert!(ok >= HOLDOUT_FLOOR, "holdout search got worse: {ok}");
}

/// Measured on 2026-11-25 after phase 2: 34 of 40. The six misses need
/// synonyms ("landlord" for a lease, "cost" for "spend"), which is what
/// meaning search adds in phase 3. Raise it then; never lower it.
const HOLDOUT_FLOOR: usize = 34;
