#![allow(clippy::unwrap_used, clippy::expect_used)]
//! A temporal is complete exactly when it is finite, writes at least one of
//! a date and a time, and writes every component of each part it writes.

use panproto_expr::exact::Temporal;

type Shape = Option<(bool, bool, bool)>;

/// Parse `text`, check that it has the given date and time shapes, and
/// return whether it is complete.
fn completeness(text: &str, date: Shape, time: Shape) -> bool {
    let value = Temporal::parse(text).unwrap_or_else(|err| panic!("{text:?}: {err:?}"));
    let form = value
        .lexical_form()
        .unwrap_or_else(|| panic!("{text:?} has no lexical form"));
    assert_eq!(form.date_shape, date, "{text:?} date shape");
    assert_eq!(form.time_shape, time, "{text:?} time shape");
    assert!(form.infinity.is_none(), "{text:?} is finite");
    value.is_complete()
}

const FULL: Shape = Some((true, true, true));

#[test]
fn full_dates_times_and_date_times_are_complete() {
    assert!(completeness("2024-01-15", FULL, None));
    assert!(completeness("12:30:45", None, FULL));
    assert!(completeness("2024-01-15T12:30:45Z", FULL, FULL));
}

#[test]
fn a_reduced_or_truncated_part_is_not_complete() {
    // Reduced precision: year and month, no day.
    assert!(!completeness("2024-01", Some((true, true, false)), None));
    // Front-truncated: month and day, no year.
    assert!(!completeness("--01-15", Some((false, true, true)), None));
    // Reduced precision: hour and minute, no second.
    assert!(!completeness("12:30", None, Some((true, true, false))));
    // Front-truncated: minute and second, no hour.
    assert!(!completeness("-30:15", None, Some((false, true, true))));
    // A full date with a reduced time.
    assert!(!completeness(
        "2024-01-15T12:30",
        FULL,
        Some((true, true, false))
    ));
}

#[test]
fn an_infinity_is_not_complete() {
    for text in ["infinity", "-infinity"] {
        let value = Temporal::parse(text).unwrap();
        let form = value.lexical_form().unwrap();
        assert!(form.infinity.is_some(), "{text}");
        assert!(!value.is_complete(), "{text}");
    }
}
