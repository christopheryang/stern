//! Reading time phrases out of a request: "a couple of months ago",
//! "last week", "in March", "since June", "before 2026".
//!
//! Only phrases that name a time are taken out. What is left is returned
//! with them removed, so it can be searched for. Every phrase has one fixed
//! meaning, documented on [`crate::domain::vault::query::TimeSpec`].

use crate::domain::vault::query::{DateField, TimeSpec};

/// What was found in a sentence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Timed {
    /// The sentence with the time phrase taken out.
    pub rest: Vec<String>,
    pub time: Option<TimeSpec>,
    pub date_field: Option<DateField>,
}

const MONTH_NAMES: [(&str, u32); 24] = [
    ("january", 1),
    ("jan", 1),
    ("february", 2),
    ("feb", 2),
    ("march", 3),
    ("mar", 3),
    ("april", 4),
    ("apr", 4),
    ("may", 5),
    ("june", 6),
    ("jun", 6),
    ("july", 7),
    ("jul", 7),
    ("august", 8),
    ("aug", 8),
    ("september", 9),
    ("sept", 9),
    ("sep", 9),
    ("october", 10),
    ("oct", 10),
    ("november", 11),
    ("nov", 11),
    ("december", 12),
    ("dec", 12),
];

fn month_number(word: &str) -> Option<u32> {
    MONTH_NAMES
        .iter()
        .find(|(name, _)| *name == word)
        .map(|(_, n)| *n)
}

fn year_number(word: &str) -> Option<i32> {
    let year: i32 = word.parse().ok()?;
    (1990..=2200).contains(&year).then_some(year)
}

fn count_word(word: &str) -> Option<u32> {
    match word {
        "a" | "an" | "one" => Some(1),
        "two" | "couple" => Some(2),
        "three" => Some(3),
        "four" => Some(4),
        "five" => Some(5),
        "six" => Some(6),
        "seven" => Some(7),
        "eight" => Some(8),
        "nine" => Some(9),
        "ten" => Some(10),
        "twelve" => Some(12),
        other => other.parse().ok().filter(|n| (1..=1000).contains(n)),
    }
}

/// A unit and what it is worth in days.
fn unit_days(word: &str) -> Option<u32> {
    match word.trim_end_matches('s') {
        "day" => Some(1),
        "week" => Some(7),
        "month" => Some(30),
        "year" => Some(365),
        _ => None,
    }
}

/// The words, lowercased and stripped of end punctuation, keeping the
/// original index of each.
fn clean(words: &[&str]) -> Vec<String> {
    words
        .iter()
        .map(|w| {
            w.trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase()
        })
        .collect()
}

/// A phrase found at `at`, with how many words it used.
struct Hit {
    len: usize,
    time: TimeSpec,
    field: Option<DateField>,
}

fn half(n: u32) -> u32 {
    n.checked_div(2).unwrap_or(0)
}

fn at(lw: &[String], i: usize) -> &str {
    lw.get(i).map_or("", String::as_str)
}

/// "a couple of months ago", "3 weeks ago", "a few days ago".
fn ago_phrase(lw: &[String], i: usize) -> Option<Hit> {
    let first = at(lw, i);
    let mut j = i;
    let (low, high): (u32, u32);
    if first == "a" && at(lw, i.saturating_add(1)) == "couple" {
        // "a couple of months ago"
        j = j.saturating_add(2);
        if at(lw, j) == "of" {
            j = j.saturating_add(1);
        }
        low = 2;
        high = 2;
    } else if first == "a" && at(lw, i.saturating_add(1)) == "few" {
        j = j.saturating_add(2);
        low = 2;
        high = 4;
    } else if first == "several" || first == "some" {
        j = j.saturating_add(1);
        low = 2;
        high = 5;
    } else {
        let n = count_word(first)?;
        j = j.saturating_add(1);
        low = n;
        high = n;
    }
    let unit = unit_days(at(lw, j))?;
    j = j.saturating_add(1);
    if at(lw, j) != "ago" {
        return None;
    }
    j = j.saturating_add(1);
    // "a couple of months" is about two months, give or take half a month on
    // either side; "a few" runs from two to four; "N units" is N units, give or
    // take a quarter, so "3 weeks ago" finds what was saved around then.
    let (from, to) = if first == "a" && at(lw, i.saturating_add(1)) == "couple" {
        (
            low.saturating_mul(unit).saturating_sub(half(unit)),
            low.saturating_mul(unit).saturating_add(unit),
        )
    } else if high > low {
        (
            low.saturating_mul(unit),
            high.saturating_mul(unit).saturating_add(half(unit)),
        )
    } else {
        let mid = low.saturating_mul(unit);
        let slack = (mid.checked_div(4).unwrap_or(0)).max(1);
        (mid.saturating_sub(slack), mid.saturating_add(slack))
    };
    Some(Hit {
        len: j.saturating_sub(i),
        time: TimeSpec::DaysAgo { from, to },
        field: None,
    })
}

/// "in the last 2 weeks", "in the past month", "over the last few days".
fn recent_phrase(lw: &[String], i: usize) -> Option<Hit> {
    let mut j = i;
    let led_in = matches!(at(lw, j), "in" | "over" | "within" | "during");
    if led_in {
        j = j.saturating_add(1);
    }
    if at(lw, j) == "the" {
        j = j.saturating_add(1);
    }
    let lead = at(lw, j);
    if !matches!(lead, "last" | "past" | "previous") {
        return None;
    }
    j = j.saturating_add(1);
    // "last week" and "last month" alone are calendar spans, handled by
    // `relative_phrase`. They only mean "the last N days" with a count, a
    // few/couple, "past", or a lead-in such as "in the last week".
    let bare_unit = unit_days(at(lw, j)).is_some();
    if bare_unit && lead != "past" && !led_in {
        return None;
    }
    let n = if let Some(n) = count_word(at(lw, j)) {
        j = j.saturating_add(1);
        n
    } else if at(lw, j) == "few" || at(lw, j) == "couple" {
        j = j.saturating_add(1);
        if at(lw, j) == "of" {
            j = j.saturating_add(1);
        }
        if lw.get(j.saturating_sub(1)).is_some_and(|w| w == "few") {
            3
        } else {
            2
        }
    } else {
        1
    };
    let unit = unit_days(at(lw, j))?;
    j = j.saturating_add(1);
    Some(Hit {
        len: j.saturating_sub(i),
        time: TimeSpec::LastDays(n.saturating_mul(unit)),
        field: None,
    })
}

/// "last week", "last month", "last year", "yesterday", "today", "this
/// week" and the like.
fn relative_phrase(lw: &[String], i: usize) -> Option<Hit> {
    let w = at(lw, i);
    let next = at(lw, i.saturating_add(1));
    let hit = |len: usize, time: TimeSpec| Hit {
        len,
        time,
        field: None,
    };
    match (w, next) {
        ("yesterday", _) => Some(hit(1, TimeSpec::Yesterday)),
        ("today", _) => Some(hit(1, TimeSpec::Today)),
        ("last" | "previous", "week") => Some(hit(2, TimeSpec::LastWeek)),
        ("last" | "previous", "month") => Some(hit(2, TimeSpec::LastMonth)),
        ("last" | "previous", "year") => Some(hit(2, TimeSpec::LastYear)),
        ("this", "week") => Some(hit(2, TimeSpec::LastDays(7))),
        ("this", "month") => Some(hit(2, TimeSpec::LastDays(30))),
        ("this", "year") => Some(hit(2, TimeSpec::LastDays(365))),
        _ => None,
    }
}

/// "in March", "in march 2025", "in 2024", "since June", "before 2026",
/// "from last March" is not supported.
fn calendar_phrase(lw: &[String], i: usize) -> Option<Hit> {
    let w = at(lw, i);
    match w {
        "since" | "after" => {
            if let Some(month) = month_number(at(lw, i.saturating_add(1))) {
                let year = year_number(at(lw, i.saturating_add(2)));
                let len = if year.is_some() { 3 } else { 2 };
                return Some(Hit {
                    len,
                    time: TimeSpec::SinceMonth { month, year },
                    field: None,
                });
            }
            None
        }
        "before" => year_number(at(lw, i.saturating_add(1))).map(|year| Hit {
            len: 2,
            time: TimeSpec::BeforeYear(year),
            field: None,
        }),
        "in" | "during" | "from" | "of" => {
            if let Some(month) = month_number(at(lw, i.saturating_add(1))) {
                // "may" is also a word: only take it with "in" or a year after.
                if month == 5 && w != "in" && year_number(at(lw, i.saturating_add(2))).is_none() {
                    return None;
                }
                let year = year_number(at(lw, i.saturating_add(2)));
                let len = if year.is_some() { 3 } else { 2 };
                return Some(Hit {
                    len,
                    time: TimeSpec::Month { month, year },
                    field: None,
                });
            }
            year_number(at(lw, i.saturating_add(1))).map(|year| Hit {
                len: 2,
                time: TimeSpec::Year(year),
                field: None,
            })
        }
        _ => None,
    }
}

/// Which date a sentence is about: modified when it says the entry was
/// changed, otherwise created.
fn date_field(lw: &[String]) -> Option<DateField> {
    lw.iter()
        .any(|w| {
            matches!(
                w.as_str(),
                "changed" | "edited" | "updated" | "modified" | "revised"
            )
        })
        .then_some(DateField::Modified)
}

/// Take the time phrase out of `words`, if there is one. Only the first
/// phrase is used. Words like "saved", "stored" and "made", which only say
/// when, stay in `rest` for the caller to drop as filler.
#[must_use]
pub fn take_time(words: &[&str]) -> Timed {
    let lw = clean(words);
    let mut found: Option<(usize, Hit)> = None;
    for i in 0..lw.len() {
        let hit = ago_phrase(&lw, i)
            .or_else(|| recent_phrase(&lw, i))
            .or_else(|| relative_phrase(&lw, i))
            .or_else(|| calendar_phrase(&lw, i));
        if let Some(hit) = hit {
            found = Some((i, hit));
            break;
        }
    }
    let field = date_field(&lw);
    let Some((start, hit)) = found else {
        return Timed {
            rest: lw,
            time: None,
            date_field: field,
        };
    };
    let rest: Vec<String> = lw
        .iter()
        .enumerate()
        .filter(|(i, _)| *i < start || *i >= start.saturating_add(hit.len))
        .map(|(_, w)| w.clone())
        .collect();
    Timed {
        rest,
        time: Some(hit.time),
        date_field: hit.field.or(field),
    }
}
