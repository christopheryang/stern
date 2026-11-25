#![allow(clippy::unwrap_used)]

//! The search evaluation: a synthetic vault of 60 entries and 80-odd
//! questions (see `tests/eval/`), scored against what chat can find today.
//!
//! The score is a baseline, not a pass mark. It is printed with
//! `cargo test -p stern-core --test eval_tests -- --nocapture`, and the
//! floors asserted below only stop it from getting worse. Later phases raise
//! them as search improves (see the Vault Assistant design).

use stern_core::ai::intent::{classify_intent, Intent};
use stern_core::domain::vault::folder::{render_folder, resolve, FolderNode, Resolution, TreeItem};

const VAULT: &str = include_str!("eval/vault.txt");
const QUESTIONS: &str = include_str!("eval/questions.txt");

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
    rows(QUESTIONS)
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

/// What chat does today with a question: the intent classifier picks a
/// lookup, and the lookup matches entry names. A folder question shows the
/// folder. Everything else finds nothing. Returns entry ids, best first.
fn search_today(question: &str, items: &[Item], nodes: &[FolderNode]) -> Vec<String> {
    match classify_intent(question) {
        Intent::RetrieveSecret { name, .. } => {
            let needle = name.to_lowercase();
            items
                .iter()
                .filter(|i| i.name.to_lowercase().contains(&needle))
                .map(|i| i.id.clone())
                .collect()
        }
        Intent::ShowFolders {
            folder: Some(reference),
        } => match resolve(nodes, &reference) {
            Resolution::One(folder_id) => {
                let tree: Vec<TreeItem> = items
                    .iter()
                    .map(|i| TreeItem {
                        id: i.id.clone(),
                        name: i.name.clone(),
                        kind: i.kind,
                        folder: i
                            .folder
                            .as_ref()
                            .map(|f| format!("/{}", f.replace('/', "/"))),
                        older_file: false,
                    })
                    .collect();
                // Entries directly inside the folder or any folder below it.
                let shown = render_folder(nodes, &tree, &folder_id);
                items
                    .iter()
                    .filter(|i| shown.contains(&format!("• {} (", i.name)))
                    .map(|i| i.id.clone())
                    .collect()
            }
            _ => Vec::new(),
        },
        _ => Vec::new(),
    }
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
    let questions = questions();

    let mut totals: Vec<(String, usize, usize)> = Vec::new();
    let mut misses: Vec<String> = Vec::new();
    for q in &questions {
        let found = search_today(&q.text, &items, &nodes);
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

    println!("\nSearch today, top three, {} questions", questions.len());
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

/// Measured on 2026-11-25, before any search work: 10 of 16 name questions
/// and 23 of 85 overall. Raise these as search improves; never lower them.
const NAME_FLOOR: usize = 10;
const TOTAL_FLOOR: usize = 23;
