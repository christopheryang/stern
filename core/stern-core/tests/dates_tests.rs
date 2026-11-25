#![allow(clippy::unwrap_used)]

use chrono::{DateTime, TimeZone, Utc};
use stern_core::ai::dates::take_time;
use stern_core::domain::vault::query::{describe, DateField, TimeSpec};

/// Thursday 5 November 2026, mid-afternoon.
fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 5, 15, 30, 0).unwrap()
}

fn day(y: i32, m: u32, d: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(y, m, d, 0, 0, 0).unwrap()
}

fn parse(text: &str) -> (Vec<String>, Option<TimeSpec>) {
    let words: Vec<&str> = text.split_whitespace().collect();
    let t = take_time(&words);
    (t.rest, t.time)
}

fn range(spec: TimeSpec, at: DateTime<Utc>) -> (DateTime<Utc>, DateTime<Utc>) {
    let r = spec.resolve(at).unwrap();
    (r.start, r.end)
}

#[test]
fn a_couple_of_months_ago_is_forty_five_to_ninety_days() {
    let (rest, time) = parse("my credit card a couple of months ago");
    assert_eq!(time, Some(TimeSpec::DaysAgo { from: 45, to: 90 }));
    assert_eq!(rest, ["my", "credit", "card"]);
    // 90 days before 5 Nov is 7 Aug; 45 days before is 21 Sep. Both days are
    // inside the range, so it ends at the start of 22 Sep.
    let (start, end) = range(TimeSpec::DaysAgo { from: 45, to: 90 }, now());
    assert_eq!(start, day(2026, 8, 7));
    assert_eq!(end, day(2026, 9, 22));
}

#[test]
fn a_few_and_counted_units() {
    assert_eq!(
        parse("notes from a few weeks ago").1,
        Some(TimeSpec::DaysAgo { from: 14, to: 31 })
    );
    assert_eq!(
        parse("3 weeks ago").1,
        Some(TimeSpec::DaysAgo { from: 16, to: 26 })
    );
    assert_eq!(
        parse("two days ago").1,
        Some(TimeSpec::DaysAgo { from: 1, to: 3 })
    );
    assert_eq!(
        parse("a month ago").1,
        Some(TimeSpec::DaysAgo { from: 23, to: 37 })
    );
}

#[test]
fn recent_spans_run_up_to_now() {
    assert_eq!(parse("in the last week").1, Some(TimeSpec::LastDays(7)));
    assert_eq!(parse("in the past month").1, Some(TimeSpec::LastDays(30)));
    assert_eq!(
        parse("over the last 2 weeks").1,
        Some(TimeSpec::LastDays(14))
    );
    assert_eq!(
        parse("in the last couple weeks").1,
        Some(TimeSpec::LastDays(14))
    );
    assert_eq!(parse("the last few days").1, Some(TimeSpec::LastDays(3)));
    let (start, end) = range(TimeSpec::LastDays(7), now());
    assert_eq!(start, day(2026, 10, 29));
    assert_eq!(end, day(2026, 11, 6));
}

#[test]
fn last_week_is_the_calendar_week_before_this_one() {
    assert_eq!(parse("anything from last week").1, Some(TimeSpec::LastWeek));
    // Thursday 5 Nov: this week began Monday 2 Nov, so last week is 26 Oct to 1 Nov.
    let (start, end) = range(TimeSpec::LastWeek, now());
    assert_eq!(start, day(2026, 10, 26));
    assert_eq!(end, day(2026, 11, 2));
    // On a Monday the week before is still the full previous Monday to Sunday.
    let monday = Utc.with_ymd_and_hms(2026, 11, 2, 9, 0, 0).unwrap();
    let (start, end) = range(TimeSpec::LastWeek, monday);
    assert_eq!(start, day(2026, 10, 26));
    assert_eq!(end, day(2026, 11, 2));
}

#[test]
fn last_month_and_last_year_cross_year_boundaries() {
    let (start, end) = range(TimeSpec::LastMonth, now());
    assert_eq!(start, day(2026, 10, 1));
    assert_eq!(end, day(2026, 11, 1));
    let january = Utc.with_ymd_and_hms(2027, 1, 12, 8, 0, 0).unwrap();
    let (start, end) = range(TimeSpec::LastMonth, january);
    assert_eq!(start, day(2026, 12, 1));
    assert_eq!(end, day(2027, 1, 1));
    let (start, end) = range(TimeSpec::LastYear, now());
    assert_eq!(start, day(2025, 1, 1));
    assert_eq!(end, day(2026, 1, 1));
    assert_eq!(parse("stuff from last year").1, Some(TimeSpec::LastYear));
}

#[test]
fn yesterday_and_today() {
    let (start, end) = range(TimeSpec::Yesterday, now());
    assert_eq!((start, end), (day(2026, 11, 4), day(2026, 11, 5)));
    let (start, end) = range(TimeSpec::Today, now());
    assert_eq!((start, end), (day(2026, 11, 5), day(2026, 11, 6)));
    assert_eq!(parse("what I saved yesterday").1, Some(TimeSpec::Yesterday));
}

#[test]
fn named_months_mean_the_latest_one_not_in_the_future() {
    assert_eq!(
        parse("the receipt in March").1,
        Some(TimeSpec::Month {
            month: 3,
            year: None
        })
    );
    // March has passed this year, so it means March 2026; December has not.
    let (start, end) = range(
        TimeSpec::Month {
            month: 3,
            year: None,
        },
        now(),
    );
    assert_eq!((start, end), (day(2026, 3, 1), day(2026, 4, 1)));
    let (start, end) = range(
        TimeSpec::Month {
            month: 12,
            year: None,
        },
        now(),
    );
    assert_eq!((start, end), (day(2025, 12, 1), day(2026, 1, 1)));
    // The current month counts.
    let (start, _) = range(
        TimeSpec::Month {
            month: 11,
            year: None,
        },
        now(),
    );
    assert_eq!(start, day(2026, 11, 1));
    // December rolls to January of the next year.
    let (start, end) = range(
        TimeSpec::Month {
            month: 12,
            year: Some(2025),
        },
        now(),
    );
    assert_eq!((start, end), (day(2025, 12, 1), day(2026, 1, 1)));
}

#[test]
fn a_month_with_a_year_and_a_bare_year() {
    assert_eq!(
        parse("notes in march 2025").1,
        Some(TimeSpec::Month {
            month: 3,
            year: Some(2025)
        })
    );
    assert_eq!(parse("things from 2024").1, Some(TimeSpec::Year(2024)));
    let (start, end) = range(TimeSpec::Year(2024), now());
    assert_eq!((start, end), (day(2024, 1, 1), day(2025, 1, 1)));
}

#[test]
fn since_and_before() {
    assert_eq!(
        parse("anything since june").1,
        Some(TimeSpec::SinceMonth {
            month: 6,
            year: None
        })
    );
    let (start, end) = range(
        TimeSpec::SinceMonth {
            month: 6,
            year: None,
        },
        now(),
    );
    assert_eq!(start, day(2026, 6, 1));
    assert_eq!(end, day(2026, 11, 6));
    assert_eq!(parse("before 2026").1, Some(TimeSpec::BeforeYear(2026)));
    let (_, end) = range(TimeSpec::BeforeYear(2026), now());
    assert_eq!(end, day(2026, 1, 1));
}

#[test]
fn the_word_may_is_not_a_month_unless_it_is_used_as_one() {
    assert_eq!(parse("I may need the lease").1, None);
    assert_eq!(parse("what may I have").1, None);
    assert_eq!(
        parse("the receipt in may").1,
        Some(TimeSpec::Month {
            month: 5,
            year: None
        })
    );
}

#[test]
fn numbers_that_are_not_times_are_left_alone() {
    assert_eq!(parse("my card ending 4417").1, None);
    assert_eq!(parse("meeting notes 3").1, None);
    assert_eq!(parse("find test note 1").1, None);
    let (rest, _) = parse("find test note 1");
    assert_eq!(rest, ["find", "test", "note", "1"]);
}

#[test]
fn changed_means_the_modified_date() {
    let words: Vec<&str> = "the note I changed last week".split_whitespace().collect();
    let t = take_time(&words);
    assert_eq!(t.date_field, Some(DateField::Modified));
    assert_eq!(t.time, Some(TimeSpec::LastWeek));
    let words: Vec<&str> = "the note I saved last week".split_whitespace().collect();
    assert_eq!(take_time(&words).date_field, None);
}

#[test]
fn only_the_time_phrase_is_taken_out() {
    let (rest, time) = parse("what did I save a couple of months ago");
    assert!(time.is_some());
    assert_eq!(rest, ["what", "did", "i", "save"]);
}

#[test]
fn a_range_says_what_it_means() {
    let r = TimeSpec::DaysAgo { from: 45, to: 90 }
        .resolve(now())
        .unwrap();
    assert_eq!(describe(&r), "between 7 Aug and 21 Sep 2026");
    let r = TimeSpec::Yesterday.resolve(now()).unwrap();
    assert_eq!(describe(&r), "on 4 Nov 2026");
    let r = TimeSpec::LastYear.resolve(now()).unwrap();
    assert_eq!(describe(&r), "between 1 Jan and 31 Dec 2025");
    let r = TimeSpec::SinceMonth {
        month: 6,
        year: Some(2025),
    }
    .resolve(now())
    .unwrap();
    assert!(describe(&r).starts_with("between 1 Jun 2025 and "));
}

#[test]
fn a_range_can_be_widened_and_contains_its_start_but_not_its_end() {
    let r = TimeSpec::LastWeek.resolve(now()).unwrap();
    assert!(r.contains(r.start));
    assert!(!r.contains(r.end));
    let wide = r.widened();
    assert!(wide.start < r.start && wide.end > r.end);
    assert!(wide.contains(r.start) && wide.contains(r.end));
}

#[test]
fn an_impossible_date_resolves_to_nothing() {
    assert!(TimeSpec::Month {
        month: 13,
        year: Some(2026)
    }
    .resolve(now())
    .is_none());
}
