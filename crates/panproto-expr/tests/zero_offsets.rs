#![allow(clippy::unwrap_used, clippy::expect_used)]
//! The three zero offsets `Z`, `+00:00` and `-00:00` are three distinct
//! values that denote one instant, named by their written forms.

use std::cmp::Ordering;

use panproto_expr::exact::{Offset, Temporal};

#[test]
fn the_three_zero_offsets_are_distinct_values_of_one_instant() {
    for (stem, form) in [
        ("2026-09-28T12:34:56.789", "date-time"),
        ("12:34:56", "time"),
    ] {
        let texts = [format!("{stem}Z"), format!("{stem}+00:00"), format!("{stem}-00:00")];
        let values = texts.clone().map(|t| Temporal::parse(&t).unwrap());
        assert_eq!(
            values.clone().map(|v| v.offset()),
            [Some(Offset::Zulu), Some(Offset::Fixed(0)), Some(Offset::NegZero)],
            "{form}"
        );
        for (i, a) in values.iter().enumerate() {
            // Each displays byte-identically to its lexeme.
            assert_eq!(a.to_string(), texts[i]);
            for (j, b) in values.iter().enumerate() {
                assert_eq!(a == b, i == j, "{a} and {b}");
                assert_eq!(a.canonical_bytes() == b.canonical_bytes(), i == j);
                assert_eq!(a.cmp_chronological(b), Some(Ordering::Equal), "{a} and {b}");
            }
        }
        // A local value is not comparable with a zoned one.
        let local = Temporal::parse(stem).unwrap();
        for v in &values {
            assert_eq!(local.cmp_chronological(v), None);
        }
    }
    // A lower-case `z` reads as the form `Z` and is written `Z`.
    let lower = Temporal::parse("2026-09-28T12:34:56z").unwrap();
    assert_eq!(lower.offset(), Some(Offset::Zulu));
    assert_eq!(lower.to_string(), "2026-09-28T12:34:56Z");
    // Every zero offset has zero minutes east of UTC.
    for offset in [Offset::Zulu, Offset::Fixed(0), Offset::NegZero, Offset::Local] {
        assert_eq!(offset.minutes(), 0);
    }
}

#[test]
fn offset_serde_names_are_lexical_and_the_old_names_are_refused() {
    let accepted = [
        (Offset::Local, r#""local""#),
        (Offset::Zulu, r#""zulu""#),
        (Offset::Fixed(-90), r#"{"fixed":-90}"#),
        (Offset::Fixed(0), r#"{"fixed":0}"#),
        (Offset::NegZero, r#""neg_zero""#),
    ];
    for (offset, json) in accepted {
        assert_eq!(serde_json::to_string(&offset).unwrap(), json);
        assert_eq!(serde_json::from_str::<Offset>(json).unwrap(), offset);
    }
    for old in ["utc", "unknown", "Zulu", "NegZero", "negzero", "Z"] {
        let error = serde_json::from_str::<Offset>(&format!("\"{old}\""))
            .unwrap_err()
            .to_string();
        assert!(error.starts_with(&format!("unknown variant `{old}`")), "{error}");
    }
    // A fixed offset of a day or more is refused.
    assert!(serde_json::from_str::<Offset>(r#"{"fixed":1440}"#).is_err());
    assert!(serde_json::from_str::<Offset>(r#"{"fixed":-1440}"#).is_err());
    assert_eq!(
        serde_json::from_str::<Offset>(r#"{"fixed":1439}"#).unwrap(),
        Offset::Fixed(1439)
    );
    // A whole value carries the new names.
    let value = Temporal::parse("12:00:00-00:00").unwrap();
    let json = serde_json::to_string(&value).unwrap();
    assert!(json.contains(r#""neg_zero""#), "{json}");
    assert_eq!(serde_json::from_str::<Temporal>(&json).unwrap(), value);
    let old = json.replace("neg_zero", "unknown");
    assert!(serde_json::from_str::<Temporal>(&old).is_err());
}

/// Every offset a lexeme can write, parsed, displayed and read back:
/// `Local`, `Z`, `-00:00` and every `Fixed` offset of `-23:59` to `+23:59`,
/// against an oracle that computes the written form from the minutes.
#[test]
fn every_offset_round_trips_through_its_written_form() {
    let mut seen = 0;
    for minutes in -1439_i16..=1439 {
        let sign = if minutes < 0 { '-' } else { '+' };
        let m = minutes.unsigned_abs();
        let written = format!("{sign}{:02}:{:02}", m / 60, m % 60);
        let value = Temporal::parse(&format!("00:00:00{written}")).unwrap();
        assert_eq!(value.offset(), Some(Offset::Fixed(minutes)), "{written}");
        assert_eq!(value.to_string(), format!("00:00:00{written}"));
        let json = serde_json::to_string(&Offset::Fixed(minutes)).unwrap();
        assert_eq!(serde_json::from_str::<Offset>(&json).unwrap(), Offset::Fixed(minutes));
        seen += 1;
    }
    assert_eq!(seen, 2879);
    for (written, offset) in [("", Offset::Local), ("Z", Offset::Zulu), ("-00:00", Offset::NegZero)] {
        let value = Temporal::parse(&format!("00:00:00{written}")).unwrap();
        assert_eq!(value.offset(), Some(offset));
        assert_eq!(value.to_string(), format!("00:00:00{written}"));
    }
}
