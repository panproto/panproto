//! Reduced, truncated, offset-rich and infinite temporals keep their exact
//! representations while the historical complete forms stay unchanged.

#![allow(clippy::unwrap_used)]

use std::cmp::Ordering;

use panproto_expr::exact::{
    Offset, OffsetForm, Temporal, TemporalForm, TemporalInfinity, TemporalLexicalForm,
    TemporalParseError, ZuluCase,
};

fn descriptor(
    kind: TemporalForm,
    date_shape: Option<(bool, bool, bool)>,
    time_shape: Option<(bool, bool, bool)>,
    offset: Offset,
) -> TemporalLexicalForm {
    TemporalLexicalForm {
        kind,
        date_shape,
        time_shape,
        separator: None,
        offset,
        infinity: None,
    }
}

#[test]
fn every_reduced_and_truncated_form_displays_back() {
    for text in [
        "2018",
        "-0044",
        "1973-06",
        "--04-12",
        "---12",
        "-0000",
        "2018Z",
        "2002-10-10+13:00",
    ] {
        assert_eq!(Temporal::parse_date(text).unwrap().to_string(), text);
    }
    for text in [
        "23",
        "23:20",
        "-20:50",
        "-20",
        "--50",
        "23:20+04",
        "03:04:05+01:02:03",
        "03:04:05+01:02:00",
    ] {
        assert_eq!(Temporal::parse_time(text).unwrap().to_string(), text);
    }
    for text in [
        "1985-04-12T23",
        "1979-05-27 07:32Z",
        "---15T09:45:00+01:00",
        "2024-01-02T03:04:05-0000",
    ] {
        assert_eq!(Temporal::parse_date_time(text).unwrap().to_string(), text);
    }
}

#[test]
fn exact_descriptors_recover_colliding_lexemes() {
    assert_eq!(
        Temporal::parse_union("--04"),
        Err(TemporalParseError::AmbiguousTemporal)
    );
    assert_eq!(
        Temporal::parse_union("1973-06"),
        Err(TemporalParseError::AmbiguousTemporal)
    );

    let year_month = Temporal::parse_date("1973-06").unwrap();
    assert_eq!(
        Temporal::parse_as(year_month.lexical_form().unwrap(), "1973-06"),
        Some(year_month)
    );

    let year_with_offset = descriptor(
        TemporalForm::Date,
        Some((true, false, false)),
        None,
        Offset::FixedForm(-6 * 60, OffsetForm::HourOnly),
    );
    let offset_value = Temporal::parse_as(year_with_offset, "1973-06").unwrap();
    assert_eq!(offset_value.to_string(), "1973-06");
    assert_eq!(offset_value.lexical_form(), Some(year_with_offset));
    assert_ne!(offset_value, Temporal::parse_date("1973-06").unwrap());
}

#[test]
fn unwritten_offsets_and_infinities_remain_distinct() {
    let local = Temporal::parse_date_time("2024-01-02T03:04:05").unwrap();
    let mut unwritten = local.lexical_form().unwrap();
    unwritten.offset = Offset::Zulu(ZuluCase::Unwritten);
    let utc = Temporal::parse_as(unwritten, "2024-01-02T03:04:05").unwrap();
    assert_eq!(utc.to_string(), local.to_string());
    assert_ne!(utc, local);
    assert_ne!(utc.canonical_bytes(), local.canonical_bytes());
    assert_eq!(utc.cmp_chronological(&local), None);

    let infinity = TemporalLexicalForm {
        kind: TemporalForm::DateTime,
        date_shape: None,
        time_shape: None,
        separator: None,
        offset: Offset::FixedForm(5 * 60, OffsetForm::Unwritten),
        infinity: Some(TemporalInfinity::Positive),
    };
    let positive = Temporal::parse_as(infinity, "infinity").unwrap();
    let mut negative_form = infinity;
    negative_form.infinity = Some(TemporalInfinity::Negative);
    let negative = Temporal::parse_as(negative_form, "-infinity").unwrap();
    assert_eq!(negative.cmp_chronological(&positive), Some(Ordering::Less));
    assert_ne!(negative.canonical_bytes(), positive.canonical_bytes());
}

#[test]
fn written_offset_forms_are_representation_distinct() {
    let texts = [
        "03:04:05+01:00",
        "03:04:05+0100",
        "03:04:05+01",
        "03:04:05+01:00:00",
    ];
    let values: Vec<_> = texts
        .into_iter()
        .map(|text| Temporal::parse_time(text).unwrap())
        .collect();
    for (value, text) in values.iter().zip(texts) {
        assert_eq!(value.to_string(), text);
    }
    for left in 0..values.len() {
        for right in 0..values.len() {
            assert_eq!(
                values[left].cmp_chronological(&values[right]),
                Some(Ordering::Equal)
            );
            assert_eq!(left == right, values[left] == values[right]);
            assert_eq!(
                left == right,
                values[left].canonical_bytes() == values[right].canonical_bytes()
            );
        }
    }
}
