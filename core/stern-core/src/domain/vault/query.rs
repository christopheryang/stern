//! A request to find entries, as data: what to look for, where, and when.
//!
//! The words of a request become a [`Query`]; the search only ever sees this
//! value, never the sentence. Time phrases are kept as a [`TimeSpec`] (what
//! was said) and turned into a [`DateRange`] against a clock that is passed
//! in, so a given sentence on a given day always means the same dates.

use chrono::{DateTime, Datelike, Duration, NaiveDate, TimeZone, Utc};

/// Which date a time phrase refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DateField {
    /// When the entry was saved (the default).
    Created,
    /// When it was last changed ("I changed", "edited").
    Modified,
}

/// A span of time as it was said, before it is pinned to calendar dates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeSpec {
    /// Between `from` and `to` days ago (`from` <= `to`).
    DaysAgo { from: u32, to: u32 },
    /// The last `n` days, up to now.
    LastDays(u32),
    /// Yesterday.
    Yesterday,
    /// Today.
    Today,
    /// The calendar week before this one (Monday to Sunday).
    LastWeek,
    /// The calendar month before this one.
    LastMonth,
    /// The calendar year before this one.
    LastYear,
    /// A named month: the most recent one that is not in the future, or the
    /// one in `year` when given.
    Month { month: u32, year: Option<i32> },
    /// A whole calendar year.
    Year(i32),
    /// On or after the start of a month.
    SinceMonth { month: u32, year: Option<i32> },
    /// Before the start of a year.
    BeforeYear(i32),
}

/// A half-open span `[start, end)` of UTC time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DateRange {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

impl DateRange {
    /// Whether `when` falls inside the range.
    #[must_use]
    pub fn contains(&self, when: DateTime<Utc>) -> bool {
        when >= self.start && when < self.end
    }

    /// The same span, grown by half its length on each side (at least a
    /// week). Used to widen a search that found nothing.
    #[must_use]
    pub fn widened(&self) -> Self {
        let span = self.end.signed_duration_since(self.start);
        let half = Duration::seconds(span.num_seconds().checked_div(2).unwrap_or(0));
        let pad = half.max(Duration::days(7));
        Self {
            start: self.start.checked_sub_signed(pad).unwrap_or(self.start),
            end: self.end.checked_add_signed(pad).unwrap_or(self.end),
        }
    }
}

/// What to find. Every part is optional; an empty query matches everything.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Query {
    /// The words to look for, lowercased, without filler. An entry must
    /// match most of these to be found.
    pub terms: Vec<String>,
    /// Words that only help rank ("note" in "my dentist note"): they raise a
    /// match but are never needed for one.
    pub soft_terms: Vec<String>,
    /// A folder reference ("Taxes" or "Taxes > 2024"), if one was named.
    pub folder: Option<String>,
    /// A kind word that narrows the search ("note", "document", "login").
    pub kind: Option<String>,
    /// When, as said.
    pub time: Option<TimeSpec>,
    /// Which date the time refers to.
    pub date_field: Option<DateField>,
}

impl Query {
    /// Whether the query asks for anything at all.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.terms.is_empty() && self.folder.is_none() && self.kind.is_none() && self.time.is_none()
    }
}

/// `when` moved by `days` (negative goes back); unchanged if that leaves the
/// representable range.
fn shift_days(when: DateTime<Utc>, days: i64) -> DateTime<Utc> {
    when.checked_add_signed(Duration::days(days))
        .unwrap_or(when)
}

fn start_of_day(date: NaiveDate) -> DateTime<Utc> {
    Utc.from_utc_datetime(&date.and_hms_opt(0, 0, 0).unwrap_or_default())
}

fn day_floor(now: DateTime<Utc>) -> DateTime<Utc> {
    start_of_day(now.date_naive())
}

fn first_of_month(year: i32, month: u32) -> Option<DateTime<Utc>> {
    NaiveDate::from_ymd_opt(year, month, 1).map(start_of_day)
}

const fn next_month(year: i32, month: u32) -> (i32, u32) {
    if month >= 12 {
        (year.saturating_add(1), 1)
    } else {
        (year, month.saturating_add(1))
    }
}

/// The year a bare month name means: this year, unless that month has not
/// started yet, in which case last year.
fn year_for_month(now: DateTime<Utc>, month: u32) -> i32 {
    if month > now.month() {
        now.year().saturating_sub(1)
    } else {
        now.year()
    }
}

impl TimeSpec {
    /// The dates this phrase means, as of `now`. `None` when the phrase names
    /// a date that does not exist.
    #[must_use]
    pub fn resolve(&self, now: DateTime<Utc>) -> Option<DateRange> {
        let today = day_floor(now);
        let tomorrow = shift_days(today, 1);
        let range = match *self {
            Self::DaysAgo { from, to } => DateRange {
                start: shift_days(today, i64::from(to).saturating_neg()),
                end: shift_days(today, i64::from(from).saturating_neg().saturating_add(1)),
            },
            Self::LastDays(n) => DateRange {
                start: shift_days(today, i64::from(n).saturating_neg()),
                end: tomorrow,
            },
            Self::Yesterday => DateRange {
                start: shift_days(today, -1),
                end: today,
            },
            Self::Today => DateRange {
                start: today,
                end: tomorrow,
            },
            Self::LastWeek => {
                let since_monday = i64::from(today.weekday().num_days_from_monday());
                let this_monday = shift_days(today, since_monday.saturating_neg());
                DateRange {
                    start: shift_days(this_monday, -7),
                    end: this_monday,
                }
            }
            Self::LastMonth => {
                let (year, month) = if now.month() == 1 {
                    (now.year().saturating_sub(1), 12)
                } else {
                    (now.year(), now.month().saturating_sub(1))
                };
                let (next_year, next) = next_month(year, month);
                DateRange {
                    start: first_of_month(year, month)?,
                    end: first_of_month(next_year, next)?,
                }
            }
            Self::LastYear => {
                let year = now.year().saturating_sub(1);
                DateRange {
                    start: first_of_month(year, 1)?,
                    end: first_of_month(year.saturating_add(1), 1)?,
                }
            }
            Self::Month { month, year } => {
                let year = year.unwrap_or_else(|| year_for_month(now, month));
                let (next_year, next) = next_month(year, month);
                DateRange {
                    start: first_of_month(year, month)?,
                    end: first_of_month(next_year, next)?,
                }
            }
            Self::Year(year) => DateRange {
                start: first_of_month(year, 1)?,
                end: first_of_month(year.saturating_add(1), 1)?,
            },
            Self::SinceMonth { month, year } => {
                let year = year.unwrap_or_else(|| year_for_month(now, month));
                DateRange {
                    start: first_of_month(year, month)?,
                    end: tomorrow,
                }
            }
            Self::BeforeYear(year) => DateRange {
                start: first_of_month(1970, 1)?,
                end: first_of_month(year, 1)?,
            },
        };
        Some(range)
    }
}

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// A range in words the user can check: "between 25 Sep and 24 Oct 2026".
/// The end shown is the last day inside the range.
#[must_use]
pub fn describe(range: &DateRange) -> String {
    let last = shift_days(range.end, -1);
    let day = |d: DateTime<Utc>| {
        let name = MONTHS
            .get(usize::try_from(d.month().saturating_sub(1)).unwrap_or(0))
            .copied()
            .unwrap_or("");
        let short: String = name.chars().take(3).collect();
        (d.day(), short, d.year())
    };
    let (d1, m1, y1) = day(range.start);
    let (d2, m2, y2) = day(last);
    if range.start.date_naive() >= last.date_naive() {
        return format!("on {d2} {m2} {y2}");
    }
    if y1 == y2 {
        format!("between {d1} {m1} and {d2} {m2} {y2}")
    } else {
        format!("between {d1} {m1} {y1} and {d2} {m2} {y2}")
    }
}
