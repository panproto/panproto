#![allow(clippy::unwrap_used, clippy::expect_used)]
//! The three zero offsets `Z`, `+00:00` and `-00:00` are three distinct
//! values that denote one instant, named by their written forms; a `Z` keeps
//! its written case and a date-time its written separator.

use std::cmp::Ordering;

use panproto_expr::exact::{DateTimeSeparator, Offset, Temporal, ZuluCase};

#[test]
fn lexical_marker_bytes_are_bijections_and_enter_canonical_payloads() {
    for (separator, byte) in [
        (DateTimeSeparator::Upper, b'T'),
        (DateTimeSeparator::Lower, b't'),
        (DateTimeSeparator::Space, b' '),
    ] {
        assert_eq!(separator.byte(), byte);
        assert_eq!(DateTimeSeparator::from_byte(byte), Some(separator));
    }
    for byte in [b'Z', b'z', b'_', 0, u8::MAX] {
        assert_eq!(
            ZuluCase::from_byte(byte),
            match byte {
                b'Z' => Some(ZuluCase::Upper),
                b'z' => Some(ZuluCase::Lower),
                _ => None,
            }
        );
    }
    assert_eq!(ZuluCase::Upper.byte(), b'Z');
    assert_eq!(ZuluCase::Lower.byte(), b'z');
    for byte in [b'Z', b'z', b'_', 0, u8::MAX] {
        assert_eq!(DateTimeSeparator::from_byte(byte), None, "{byte}");
    }

    let time = Temporal::parse("03:04:05z").unwrap();
    let mut expected_time = vec![1, 3, 4, 5];
    expected_time.extend_from_slice(&0_u64.to_le_bytes());
    expected_time.extend_from_slice(&[1, b'z']);
    assert_eq!(time.canonical_bytes(), expected_time);

    let datetime = Temporal::parse("2024-01-02t03:04:05Z").unwrap();
    let mut expected_datetime = vec![2];
    expected_datetime.extend_from_slice(&2024_i64.to_le_bytes());
    expected_datetime.extend_from_slice(&[1, 2, b't', 3, 4, 5]);
    expected_datetime.extend_from_slice(&0_u64.to_le_bytes());
    expected_datetime.extend_from_slice(&[1, b'Z']);
    assert_eq!(datetime.canonical_bytes(), expected_datetime);
}

#[test]
fn the_three_zero_offsets_are_distinct_values_of_one_instant() {
    for (stem, form) in [
        ("2026-09-28T12:34:56.789", "date-time"),
        ("12:34:56", "time"),
    ] {
        let texts = [
            format!("{stem}Z"),
            format!("{stem}+00:00"),
            format!("{stem}-00:00"),
        ];
        let values = texts.clone().map(|t| Temporal::parse(&t).unwrap());
        assert_eq!(
            values.clone().map(|v| v.offset()),
            [
                Some(Offset::Zulu(ZuluCase::Upper)),
                Some(Offset::Fixed(0)),
                Some(Offset::NegZero)
            ],
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
    // A lower-case `z` reads as the form `Z` in its own case.
    let lower = Temporal::parse("2026-09-28T12:34:56z").unwrap();
    assert_eq!(lower.offset(), Some(Offset::Zulu(ZuluCase::Lower)));
    assert_eq!(lower.to_string(), "2026-09-28T12:34:56z");
    // Every zero offset has zero minutes east of UTC.
    for offset in [
        Offset::Zulu(ZuluCase::Upper),
        Offset::Zulu(ZuluCase::Lower),
        Offset::Fixed(0),
        Offset::NegZero,
        Offset::Local,
    ] {
        assert_eq!(offset.minutes(), 0);
    }
}

#[test]
fn offset_serde_names_are_lexical_and_the_old_names_are_refused() {
    let accepted = [
        (Offset::Local, r#""local""#),
        (Offset::Zulu(ZuluCase::Upper), r#"{"zulu":"upper"}"#),
        (Offset::Zulu(ZuluCase::Lower), r#"{"zulu":"lower"}"#),
        (Offset::Fixed(-90), r#"{"fixed":-90}"#),
        (Offset::Fixed(0), r#"{"fixed":0}"#),
        (Offset::NegZero, r#""neg_zero""#),
    ];
    for (offset, json) in accepted {
        assert_eq!(serde_json::to_string(&offset).unwrap(), json);
        assert_eq!(serde_json::from_str::<Offset>(json).unwrap(), offset);
    }
    for old in ["utc", "unknown", "Zulu", "NegZero", "negzero", "Z", "zulu"] {
        let error = serde_json::from_str::<Offset>(&format!("\"{old}\""))
            .unwrap_err()
            .to_string();
        assert!(
            error.starts_with(&format!("unknown variant `{old}`"))
                || (old == "zulu" && error.starts_with("invalid type: unit variant")),
            "{old}: {error}"
        );
    }
    for bad in [r#"{"zulu":"Upper"}"#, r#"{"zulu":"z"}"#, r#"{"zulu":null}"#] {
        assert!(serde_json::from_str::<Offset>(bad).is_err(), "{bad}");
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
        assert_eq!(value.offset().unwrap().minutes(), minutes, "{written}");
        assert_eq!(value.to_string(), format!("00:00:00{written}"));
        let json = serde_json::to_string(&Offset::Fixed(minutes)).unwrap();
        assert_eq!(
            serde_json::from_str::<Offset>(&json).unwrap(),
            Offset::Fixed(minutes)
        );
        seen += 1;
    }
    assert_eq!(seen, 2879);
    for (written, offset) in [
        ("", Offset::Local),
        ("Z", Offset::Zulu(ZuluCase::Upper)),
        ("z", Offset::Zulu(ZuluCase::Lower)),
        ("-00:00", Offset::NegZero),
    ] {
        let value = Temporal::parse(&format!("00:00:00{written}")).unwrap();
        assert_eq!(value.offset(), Some(offset));
        assert_eq!(value.to_string(), format!("00:00:00{written}"));
    }
}

#[test]
fn chronological_comparison_subtracts_the_offset_in_minutes() {
    for (local, utc) in [
        ("1970-01-01T00:00:00+01:00", "1969-12-31T23:00:00Z"),
        ("1969-12-31T23:00:00-01:00", "1970-01-01T00:00:00Z"),
        ("2024-02-29T12:34:56+05:30", "2024-02-29T07:04:56Z"),
    ] {
        let local = Temporal::parse(local).unwrap();
        let utc = Temporal::parse(utc).unwrap();
        assert_eq!(local.cmp_chronological(&utc), Some(Ordering::Equal));
        assert_eq!(utc.cmp_chronological(&local), Some(Ordering::Equal));
    }
}

/// A temporal keeps the written case of `Z` and of its date-time separator:
/// `Display` writes back each spelling, `==`, `Hash` and the canonical
/// bytes tell them apart, and `cmp_chronological` compares them `Equal`.
#[test]
fn a_temporal_keeps_its_written_case() {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let hash = |value: &Temporal| {
        let mut hasher = DefaultHasher::new();
        value.hash(&mut hasher);
        hasher.finish()
    };
    for text in [
        "2024-01-02T03:04:05z",
        "2024-01-02t03:04:05Z",
        "2024-01-02t03:04:05z",
        "2024-01-02 03:04:05Z",
        "2024-01-02 03:04:05z",
        "03:04:05z",
        "2024-01-02T03:04:05Z",
    ] {
        let value = Temporal::parse(text).unwrap();
        assert_eq!(value.to_string(), text);
        assert_eq!(Temporal::parse(&value.to_string()).unwrap(), value);
        let json = serde_json::to_string(&value).unwrap();
        assert_eq!(
            serde_json::from_str::<Temporal>(&json).unwrap(),
            value,
            "{json}"
        );
    }
    let a = Temporal::parse("2024-01-02t03:04:05z").unwrap();
    let b = Temporal::parse("2024-01-02 03:04:05Z").unwrap();
    let c = Temporal::parse("2024-01-02T03:04:05Z").unwrap();
    for (x, y) in [(&a, &b), (&a, &c), (&b, &c)] {
        assert_ne!(x, y);
        assert_ne!(hash(x), hash(y), "{x} and {y}");
        assert_ne!(x.canonical_bytes(), y.canonical_bytes());
        assert_eq!(x.cmp_chronological(y), Some(Ordering::Equal), "{x} and {y}");
    }
    let Temporal::DateTime(_, _, offset, separator) = &a else {
        panic!("a date-time")
    };
    assert_eq!(
        (*offset, *separator),
        (Offset::Zulu(ZuluCase::Lower), DateTimeSeparator::Lower)
    );
    let Temporal::DateTime(_, _, _, separator) = &b else {
        panic!("a date-time")
    };
    assert_eq!(*separator, DateTimeSeparator::Space);
    // The separator's serde names.
    for (separator, json) in [
        (DateTimeSeparator::Upper, r#""upper""#),
        (DateTimeSeparator::Lower, r#""lower""#),
        (DateTimeSeparator::Space, r#""space""#),
    ] {
        assert_eq!(serde_json::to_string(&separator).unwrap(), json);
        assert_eq!(
            serde_json::from_str::<DateTimeSeparator>(json).unwrap(),
            separator
        );
    }
    // Any other separator is no date-time.
    for bad in [
        "2024-01-02_03:04:05Z",
        "2024-01-02x03:04:05Z",
        "2024-01-02T03:04:05Zz",
    ] {
        assert!(Temporal::parse(bad).is_err(), "{bad}");
    }
}
