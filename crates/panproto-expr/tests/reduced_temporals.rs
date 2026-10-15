//! Reduced, truncated, offset-rich and infinite temporals keep their exact
//! representations while the historical complete forms stay unchanged.

#![allow(clippy::unwrap_used)]

use std::cmp::Ordering;
use std::collections::HashSet;

use panproto_expr::exact::{
    CivilDate, CivilTime, DateFields, DateTimeSeparator, ExtendedTemporal, Fraction, Offset,
    OffsetForm, Temporal, TemporalForm, TemporalInfinity, TemporalLexicalForm, TemporalParseError,
    TemporalYear, TimeFields, ZuluCase,
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

fn complete_date() -> DateFields {
    DateFields {
        year: Some(TemporalYear {
            value: 2000,
            negative_zero: false,
            width: 4,
        }),
        month: Some(1),
        day: Some(2),
    }
}

fn complete_time() -> TimeFields {
    TimeFields {
        hour: Some(3),
        minute: Some(4),
        second: Some(5),
        fraction: Fraction::default(),
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

    for (kind, finite_text, date_shape, time_shape, separator) in [
        (
            TemporalForm::Date,
            "2000-01-02",
            Some((true, true, true)),
            None,
            None,
        ),
        (
            TemporalForm::DateTime,
            "2000-01-02T03:04:05",
            Some((true, true, true)),
            Some((true, true, true)),
            Some(DateTimeSeparator::Upper),
        ),
    ] {
        for offset in [
            Offset::Local,
            Offset::FixedForm(5 * 60 + 30, OffsetForm::Unwritten),
        ] {
            let finite_form = TemporalLexicalForm {
                kind,
                date_shape,
                time_shape,
                separator,
                offset,
                infinity: None,
            };
            let finite = Temporal::parse_as(finite_form, finite_text).unwrap();
            let negative_form = TemporalLexicalForm {
                kind,
                date_shape: None,
                time_shape: None,
                separator: None,
                offset,
                infinity: Some(TemporalInfinity::Negative),
            };
            let positive_form = TemporalLexicalForm {
                infinity: Some(TemporalInfinity::Positive),
                ..negative_form
            };
            let negative = Temporal::parse_as(negative_form, "-infinity").unwrap();
            let positive = Temporal::parse_as(positive_form, "infinity").unwrap();
            assert_eq!(negative.cmp_chronological(&finite), Some(Ordering::Less));
            assert_eq!(finite.cmp_chronological(&positive), Some(Ordering::Less));
            assert_eq!(negative.cmp_chronological(&positive), Some(Ordering::Less));
            let bytes = [
                negative.canonical_bytes(),
                finite.canonical_bytes(),
                positive.canonical_bytes(),
            ];
            assert_eq!(bytes.into_iter().collect::<HashSet<_>>().len(), 3);
        }
    }
}

#[test]
fn zoned_infinity_and_truncated_date_require_matching_offset_minutes() {
    let infinity = scoped_infinity(
        TemporalForm::Date,
        Offset::Zulu(ZuluCase::Upper),
        TemporalInfinity::Positive,
    )
    .value;
    let truncated = Temporal::parse_date("---01+05:30:00").unwrap();

    assert_eq!(infinity.cmp_chronological(&truncated), None);
    assert_eq!(truncated.cmp_chronological(&infinity), None);
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

#[test]
fn complete_extended_aliases_are_not_admitted_by_public_or_serde_carriers() {
    let aliases = [
        Temporal::Extended(ExtendedTemporal {
            date: Some(complete_date()),
            time: None,
            separator: None,
            offset: Offset::Local,
            infinity: None,
        }),
        Temporal::Extended(ExtendedTemporal {
            date: None,
            time: Some(complete_time()),
            separator: None,
            offset: Offset::Zulu(ZuluCase::Upper),
            infinity: None,
        }),
        Temporal::Extended(ExtendedTemporal {
            date: Some(complete_date()),
            time: Some(complete_time()),
            separator: Some(DateTimeSeparator::Upper),
            offset: Offset::Fixed(0),
            infinity: None,
        }),
    ];
    for alias in &aliases {
        assert!(!alias.well_formed(), "{alias:?}");
        assert_eq!(alias.lexical_form(), None, "{alias:?}");
        let json = serde_json::to_string(alias).unwrap();
        assert!(serde_json::from_str::<Temporal>(&json).is_err(), "{json}");
    }

    let misplaced_rich_offset = Temporal::Time(
        CivilTime::new(3, 4, 5, Fraction::default()).unwrap(),
        Offset::Zulu(ZuluCase::Unwritten),
    );
    assert!(!misplaced_rich_offset.well_formed());
    assert_eq!(misplaced_rich_offset.lexical_form(), None);
    let json = serde_json::to_string(&misplaced_rich_offset).unwrap();
    assert!(serde_json::from_str::<Temporal>(&json).is_err(), "{json}");

    for admitted in [
        Temporal::Date(CivilDate::new(2000, 1, 2).unwrap()),
        Temporal::parse_date("2000-01").unwrap(),
        Temporal::parse_time("03:04:05+00").unwrap(),
    ] {
        assert!(admitted.well_formed(), "{admitted:?}");
        let json = serde_json::to_string(&admitted).unwrap();
        assert_eq!(serde_json::from_str::<Temporal>(&json).unwrap(), admitted);
    }
}

#[test]
fn historical_variant_admission_checks_manual_fields_and_offsets() {
    let valid_date = CivilDate::new(2000, 1, 2).unwrap();
    let valid_time = CivilTime::new(3, 4, 5, Fraction::default()).unwrap();
    let invalid = [
        Temporal::Date(CivilDate {
            year: 2000,
            month: 13,
            day: 1,
        }),
        Temporal::Time(
            CivilTime {
                hour: 24,
                minute: 0,
                second: 0,
                fraction: Fraction::default(),
            },
            Offset::Local,
        ),
        Temporal::DateTime(
            CivilDate {
                year: 2001,
                month: 2,
                day: 29,
            },
            valid_time.clone(),
            Offset::Zulu(ZuluCase::Upper),
            DateTimeSeparator::Upper,
        ),
        Temporal::Time(valid_time.clone(), Offset::Fixed(24 * 60)),
        Temporal::Time(valid_time.clone(), Offset::Zulu(ZuluCase::Unwritten)),
        Temporal::DateTime(
            valid_date,
            valid_time,
            Offset::FixedForm(0, OffsetForm::Basic),
            DateTimeSeparator::Upper,
        ),
    ];
    for value in invalid {
        assert!(!value.well_formed(), "{value:?}");
        assert_eq!(value.lexical_form(), None, "{value:?}");
        let json = serde_json::to_string(&value).unwrap();
        assert!(serde_json::from_str::<Temporal>(&json).is_err(), "{json}");
    }

    for value in [
        Temporal::Date(valid_date),
        Temporal::Time(
            CivilTime::new(3, 4, 5, Fraction::default()).unwrap(),
            Offset::Fixed(0),
        ),
    ] {
        assert!(value.well_formed(), "{value:?}");
        assert!(value.lexical_form().is_some(), "{value:?}");
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ComparabilityClass {
    kind: TemporalForm,
    date_shape: Option<(bool, bool, bool)>,
    time_shape: Option<(bool, bool, bool)>,
    zoned: bool,
    truncated_offset_minutes: Option<i16>,
}

struct ScopedTemporal {
    value: Temporal,
    class: ComparabilityClass,
}

struct ScopedInfinity {
    value: Temporal,
    kind: TemporalForm,
    zoned: bool,
    offset_minutes: i16,
    sign: TemporalInfinity,
}

fn scoped_value(
    text: String,
    kind: TemporalForm,
    date_shape: Option<(bool, bool, bool)>,
    time_shape: Option<(bool, bool, bool)>,
    separator: Option<DateTimeSeparator>,
    offset: Offset,
) -> ScopedTemporal {
    let form = TemporalLexicalForm {
        kind,
        date_shape,
        time_shape,
        separator,
        offset,
        infinity: None,
    };
    let value = Temporal::parse_as(form, &text)
        .unwrap_or_else(|| panic!("finite-scope lexeme must parse under its descriptor: {text}"));
    assert_eq!(value.to_string(), text);
    assert_eq!(value.lexical_form(), Some(form));
    let truncated = date_shape.is_some_and(|(year, _, _)| !year)
        || (date_shape.is_none() && time_shape.is_some_and(|(hour, _, _)| !hour));
    ScopedTemporal {
        value,
        class: ComparabilityClass {
            kind,
            date_shape,
            time_shape,
            zoned: offset.is_zoned(),
            truncated_offset_minutes: truncated.then(|| offset.minutes()),
        },
    }
}

fn scoped_infinity(kind: TemporalForm, offset: Offset, sign: TemporalInfinity) -> ScopedInfinity {
    let form = TemporalLexicalForm {
        kind,
        date_shape: None,
        time_shape: None,
        separator: None,
        offset,
        infinity: Some(sign),
    };
    let text = match sign {
        TemporalInfinity::Negative => "-infinity",
        TemporalInfinity::Positive => "infinity",
    };
    let value = Temporal::parse_as(form, text)
        .unwrap_or_else(|| panic!("infinity must parse under its descriptor: {form:?}"));
    assert_eq!(value.to_string(), text);
    assert_eq!(value.lexical_form(), Some(form));
    let json = serde_json::to_string(&value).unwrap();
    assert_eq!(serde_json::from_str::<Temporal>(&json).unwrap(), value);
    ScopedInfinity {
        value,
        kind,
        zoned: offset.is_zoned(),
        offset_minutes: offset.minutes(),
        sign,
    }
}

fn infinity_applies(class: ComparabilityClass, infinity: &ScopedInfinity) -> bool {
    class.kind != TemporalForm::Time
        && class.kind == infinity.kind
        && class.zoned == infinity.zoned
        && class
            .truncated_offset_minutes
            .is_none_or(|minutes| minutes == infinity.offset_minutes)
}

#[test]
fn sql_temporal_forms() {
    const DATE_BODIES: [(&str, (bool, bool, bool)); 12] = [
        ("0000", (true, false, false)),
        ("-0000", (true, false, false)),
        ("2000-01", (true, true, false)),
        ("2000-02", (true, true, false)),
        ("2000-01-01", (true, true, true)),
        ("2000-01-02", (true, true, true)),
        ("--01", (false, true, false)),
        ("--02", (false, true, false)),
        ("--01-01", (false, true, true)),
        ("--01-02", (false, true, true)),
        ("---01", (false, false, true)),
        ("---02", (false, false, true)),
    ];
    const TIME_BODIES: [(&str, (bool, bool, bool)); 14] = [
        ("00", (true, false, false)),
        ("01", (true, false, false)),
        ("00:00", (true, true, false)),
        ("00:01", (true, true, false)),
        ("00:00:00", (true, true, true)),
        ("00:00:00.0", (true, true, true)),
        ("23:59:60", (true, true, true)),
        ("23:59:60.5", (true, true, true)),
        ("-00", (false, true, false)),
        ("-01", (false, true, false)),
        ("-00:00", (false, true, true)),
        ("-00:01", (false, true, true)),
        ("--00", (false, false, true)),
        ("--01", (false, false, true)),
    ];
    const DATETIME_DATES: [(&str, (bool, bool, bool)); 4] = [
        ("2000-01-01", (true, true, true)),
        ("2000-01-02", (true, true, true)),
        ("---01", (false, false, true)),
        ("---02", (false, false, true)),
    ];
    const DATETIME_TIMES: [(&str, (bool, bool, bool)); 8] = [
        ("00", (true, false, false)),
        ("01", (true, false, false)),
        ("00:00", (true, true, false)),
        ("00:01", (true, true, false)),
        ("00:00:00", (true, true, true)),
        ("00:00:00.0", (true, true, true)),
        ("23:59:60", (true, true, true)),
        ("23:59:60.5", (true, true, true)),
    ];
    let offsets = [
        ("", Offset::Local),
        ("Z", Offset::Zulu(ZuluCase::Upper)),
        ("", Offset::Zulu(ZuluCase::Unwritten)),
        ("+00:00", Offset::Fixed(0)),
        ("+0000", Offset::FixedForm(0, OffsetForm::Basic)),
        ("+00", Offset::FixedForm(0, OffsetForm::HourOnly)),
        (
            "+05:30:00",
            Offset::FixedForm(330, OffsetForm::ExtendedSeconds),
        ),
        ("-04:56:02", Offset::fixed_seconds(-17_762).unwrap()),
        ("-0000", Offset::NegZeroForm(OffsetForm::Basic)),
        ("", Offset::FixedForm(0, OffsetForm::Unwritten)),
        ("", Offset::FixedForm(330, OffsetForm::Unwritten)),
    ];

    assert_eq!(DATE_BODIES.len(), 12);
    assert_eq!(TIME_BODIES.len(), 14);
    assert_eq!(DATETIME_DATES.len(), 4);
    assert_eq!(DATETIME_TIMES.len(), 8);
    assert_eq!(offsets.len(), 11);
    assert_eq!(DATE_BODIES.len() * offsets.len(), 132);
    assert_eq!(TIME_BODIES.len() * offsets.len(), 154);
    assert_eq!(
        DATETIME_DATES.len() * DATETIME_TIMES.len() * offsets.len(),
        352
    );

    let mut scope = Vec::new();
    for (body, shape) in DATE_BODIES {
        for (suffix, offset) in offsets.iter().copied() {
            scope.push(scoped_value(
                format!("{body}{suffix}"),
                TemporalForm::Date,
                Some(shape),
                None,
                None,
                offset,
            ));
        }
    }
    for (body, shape) in TIME_BODIES {
        for (suffix, offset) in offsets.iter().copied() {
            scope.push(scoped_value(
                format!("{body}{suffix}"),
                TemporalForm::Time,
                None,
                Some(shape),
                None,
                offset,
            ));
        }
    }
    for (date, date_shape) in DATETIME_DATES {
        for (time, time_shape) in DATETIME_TIMES {
            for (suffix, offset) in offsets.iter().copied() {
                scope.push(scoped_value(
                    format!("{date}T{time}{suffix}"),
                    TemporalForm::DateTime,
                    Some(date_shape),
                    Some(time_shape),
                    Some(DateTimeSeparator::Upper),
                    offset,
                ));
            }
        }
    }
    assert_eq!(scope.len(), 638);

    let mut classes: Vec<(ComparabilityClass, Vec<usize>)> = Vec::new();
    for (index, item) in scope.iter().enumerate() {
        if let Some((_, members)) = classes.iter_mut().find(|(class, _)| *class == item.class) {
            members.push(index);
        } else {
            classes.push((item.class, vec![index]));
        }
    }
    assert_eq!(classes.len(), 54);

    let infinity_offsets = [
        Offset::Local,
        Offset::Zulu(ZuluCase::Upper),
        Offset::Zulu(ZuluCase::Unwritten),
        Offset::FixedForm(330, OffsetForm::Unwritten),
        Offset::fixed_seconds(-17_762).unwrap(),
    ];
    let mut infinities = Vec::new();
    for kind in [TemporalForm::Date, TemporalForm::DateTime] {
        for offset in infinity_offsets {
            for sign in [TemporalInfinity::Negative, TemporalInfinity::Positive] {
                infinities.push(scoped_infinity(kind, offset, sign));
            }
        }
    }
    assert_eq!(infinity_offsets.len(), 5);
    assert_eq!(2 * infinity_offsets.len() * 2, 20);
    assert_eq!(infinities.len(), 20);
    assert_eq!(scope.len() + infinities.len(), 658);

    let mut all_pairs = 0_usize;
    let mut comparable_pairs = 0_usize;
    for left in &scope {
        for right in &scope {
            all_pairs += 1;
            let forward = left.value.cmp_chronological(&right.value);
            let backward = right.value.cmp_chronological(&left.value);
            assert_eq!(forward.is_some(), left.class == right.class);
            assert_eq!(forward.map(Ordering::reverse), backward);
            if forward.is_some() {
                comparable_pairs += 1;
            }
        }
    }
    assert_eq!(all_pairs, 407_044);
    assert_eq!(comparable_pairs, 19_932);
    assert_eq!(
        classes.iter().map(|(_, xs)| xs.len().pow(2)).sum::<usize>(),
        19_932
    );

    let class_sizes: Vec<usize> = classes
        .iter()
        .map(|(class, members)| {
            members.len()
                + infinities
                    .iter()
                    .filter(|infinity| infinity_applies(*class, infinity))
                    .count()
        })
        .collect();
    assert_eq!(class_sizes.len(), 54);
    assert_eq!(
        class_sizes.iter().map(|size| size.pow(2)).sum::<usize>(),
        25_700
    );
    assert_eq!(
        class_sizes.iter().map(|size| size.pow(3)).sum::<usize>(),
        1_369_448
    );

    let mut class_pairs = 0_usize;
    let mut class_triples = 0_usize;
    for (class, members) in &classes {
        let applicable_infinities: Vec<_> = infinities
            .iter()
            .filter(|infinity| infinity_applies(*class, infinity))
            .collect();
        if class.kind == TemporalForm::Time {
            assert!(applicable_infinities.is_empty());
        } else {
            assert!(
                applicable_infinities
                    .iter()
                    .any(|infinity| infinity.sign == TemporalInfinity::Negative)
            );
            assert!(
                applicable_infinities
                    .iter()
                    .any(|infinity| infinity.sign == TemporalInfinity::Positive)
            );
        }
        for infinity in &applicable_infinities {
            let expected = match infinity.sign {
                TemporalInfinity::Negative => Ordering::Less,
                TemporalInfinity::Positive => Ordering::Greater,
            };
            for &finite in members {
                assert_eq!(
                    infinity.value.cmp_chronological(&scope[finite].value),
                    Some(expected)
                );
                assert_eq!(
                    scope[finite].value.cmp_chronological(&infinity.value),
                    Some(expected.reverse())
                );
            }
        }
        for infinity in infinities
            .iter()
            .filter(|infinity| !infinity_applies(*class, infinity))
        {
            for &finite in members {
                assert_eq!(infinity.value.cmp_chronological(&scope[finite].value), None);
                assert_eq!(scope[finite].value.cmp_chronological(&infinity.value), None);
            }
        }

        let values: Vec<&Temporal> = members
            .iter()
            .map(|&index| &scope[index].value)
            .chain(applicable_infinities.iter().map(|infinity| &infinity.value))
            .collect();
        for left in &values {
            for right in &values {
                class_pairs += 1;
                let forward = left.cmp_chronological(right).unwrap();
                let backward = right.cmp_chronological(left).unwrap();
                assert_eq!(forward.reverse(), backward);
            }
        }
        for a in &values {
            for b in &values {
                for c in &values {
                    class_triples += 1;
                    let ab = a.cmp_chronological(b).unwrap();
                    let bc = b.cmp_chronological(c).unwrap();
                    let ac = a.cmp_chronological(c).unwrap();
                    if ab != Ordering::Greater && bc != Ordering::Greater {
                        assert_ne!(ac, Ordering::Greater, "{a:?} <= {b:?} <= {c:?}");
                    }
                }
            }
        }
    }
    assert_eq!(class_pairs, 25_700);
    assert_eq!(class_triples, 1_369_448);

    let all_values: Vec<&Temporal> = scope
        .iter()
        .map(|item| &item.value)
        .chain(infinities.iter().map(|infinity| &infinity.value))
        .collect();
    let canonical: Vec<Vec<u8>> = all_values
        .iter()
        .map(|value| value.canonical_bytes())
        .collect();
    assert_eq!(all_values.len(), 658);
    assert_eq!(all_values.len() * all_values.len(), 432_964);
    assert_eq!(canonical.iter().cloned().collect::<HashSet<_>>().len(), 658);
    for left in 0..all_values.len() {
        for right in 0..all_values.len() {
            assert_eq!(
                all_values[left] == all_values[right],
                canonical[left] == canonical[right]
            );
        }
    }

    println!(
        "SQL temporal forms: 658 values (638 finite: 132 dates, 154 times, 352 date-times; 20 infinities) over 11 finite offsets and 5 infinity offsets, 54 finite comparability classes, 407044 finite ordered pairs, 19932 comparable finite pairs, 25700 class-local pairs with infinities, 1369448 class-local triples, 432964 representation pairs"
    );
}
