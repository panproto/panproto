//! Reduced, truncated, offset-rich and infinite temporals keep their exact
//! representations while the historical complete forms stay unchanged.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::cmp::Ordering;
use std::collections::HashSet;

use panproto_expr::exact::{
    CivilDate, CivilTime, DateFields, DateTimeSeparator, ExtendedTemporal, Fraction, Offset,
    OffsetForm, Temporal, TemporalForm, TemporalInfinity, TemporalLexicalForm, TemporalParseError,
    TemporalYear, TimeFields, TimeUnit, ZuluCase,
};
use panproto_reference::temporal as rt;

const fn descriptor(
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

const fn complete_date() -> DateFields {
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

const fn is_written_zero_offset(offset: rt::Offset) -> bool {
    match offset {
        rt::Offset::Zulu(rt::ZuluCase::Upper | rt::ZuluCase::Lower) => true,
        rt::Offset::Fixed(0, form) | rt::Offset::NegZero(form) => matches!(
            form,
            rt::OffsetForm::Extended
                | rt::OffsetForm::Basic
                | rt::OffsetForm::HourOnly
                | rt::OffsetForm::ExtendedSeconds
        ),
        _ => false,
    }
}

const fn is_unwritten_zulu_replacement(offset: rt::Offset) -> bool {
    is_written_zero_offset(offset)
        || matches!(
            offset,
            rt::Offset::Local | rt::Offset::Fixed(0, rt::OffsetForm::Unwritten)
        )
}

fn assert_no_standard_unwritten_zulu(
    production_kind: TemporalForm,
    reference_kind: rt::TemporalForm,
    text: &str,
) {
    for value in [
        production_read(production_kind, text),
        Temporal::parse(text).ok(),
        Temporal::parse_union(text).ok(),
    ]
    .into_iter()
    .flatten()
    {
        assert_ne!(
            value.lexical_form().map(|form| form.offset),
            Some(Offset::Zulu(ZuluCase::Unwritten)),
            "standard production reading of {text}"
        );
    }
    for value in [
        rt::parse(reference_kind, text),
        rt::parse_union(text).ok(),
        rt::parse_any(text),
    ]
    .into_iter()
    .flatten()
    {
        assert!(
            !rt::unwritten_zulu(&value),
            "standard reference reading of {text}"
        );
    }
}

fn check_unwritten_zulu_order(
    value: &Temporal,
    reference: &rt::Temporal,
    replacement: &Temporal,
    replacement_reference: &rt::Temporal,
    replacement_offset: rt::Offset,
    competitors: &[(&Temporal, &rt::Temporal)],
) -> usize {
    if replacement_offset == rt::Offset::Local {
        assert_eq!(value.cmp_chronological(replacement), None);
        assert_eq!(replacement.cmp_chronological(value), None);
        assert_eq!(
            rt::cmp_chronological(reference, replacement_reference),
            None
        );
        assert_eq!(
            rt::cmp_chronological(replacement_reference, reference),
            None
        );
        return 0;
    }

    assert_eq!(replacement_offset.seconds(), Some(0));
    assert_eq!(value.cmp_chronological(replacement), Some(Ordering::Equal));
    assert_eq!(replacement.cmp_chronological(value), Some(Ordering::Equal));
    assert_eq!(
        rt::cmp_chronological(reference, replacement_reference),
        Some(Ordering::Equal)
    );
    for (other, other_reference) in competitors {
        assert_eq!(
            value.cmp_chronological(other),
            replacement.cmp_chronological(other)
        );
        assert_eq!(
            other.cmp_chronological(value),
            other.cmp_chronological(replacement)
        );
        assert_eq!(
            rt::cmp_chronological(reference, other_reference),
            rt::cmp_chronological(replacement_reference, other_reference)
        );
        assert_eq!(
            rt::cmp_chronological(other_reference, reference),
            rt::cmp_chronological(other_reference, replacement_reference)
        );
    }
    competitors.len()
}

fn check_unwritten_zulu_representation(
    text: &str,
    suffix: &str,
    forms: (TemporalLexicalForm, rt::TemporalLexicalForm),
    baseline: (&Temporal, &rt::Temporal),
    replacement_pair: (&Temporal, &rt::Temporal),
) {
    let (form, reference_form) = forms;
    let (value, reference) = baseline;
    let (replacement, replacement_reference) = replacement_pair;
    assert!(rt::unwritten_zulu(reference));
    assert_eq!(form.offset, Offset::Zulu(ZuluCase::Unwritten));
    assert!(!rt::unwritten_zulu(replacement_reference));
    assert_ne!(
        replacement.lexical_form().map(|item| item.offset),
        Some(form.offset)
    );

    assert_ne!(value, replacement);
    assert_ne!(value.lexical_form(), replacement.lexical_form());
    let value_bytes = value.canonical_bytes();
    let replacement_bytes = replacement.canonical_bytes();
    assert_ne!(value_bytes, replacement_bytes);
    assert_ne!(reference, replacement_reference);
    let reference_bytes = rt::canonical_bytes(reference).unwrap();
    let replacement_reference_bytes = rt::canonical_bytes(replacement_reference).unwrap();
    assert_ne!(reference_bytes, replacement_reference_bytes);

    let expected_layout_tag = match (form.kind, form.infinity) {
        (TemporalForm::Date, None) => 3,
        (TemporalForm::Time, None) => 4,
        (TemporalForm::DateTime, None) => 5,
        (TemporalForm::Date, Some(_)) => 6,
        (TemporalForm::DateTime, Some(_)) => 7,
        (TemporalForm::Time, Some(_)) => panic!("a time infinity is outside the carrier"),
    };
    assert_eq!(value_bytes.first(), Some(&expected_layout_tag));
    assert_eq!(reference_bytes.first(), Some(&15));
    assert_eq!(reference_bytes.get(1), Some(&expected_layout_tag));
    assert!(value_bytes.ends_with(&[1, 2]));
    assert!(reference_bytes.ends_with(&[1, 2]));

    let replacement_text = if form.infinity.is_some() {
        text.to_owned()
    } else {
        format!("{text}{suffix}")
    };
    assert_eq!(value.to_string(), text);
    assert_eq!(reference.to_string(), text);
    assert_eq!(replacement.to_string(), replacement_text);
    assert_eq!(replacement_reference.to_string(), replacement_text);

    assert_eq!(Temporal::parse_as(form, text).as_ref(), Some(value));
    assert_eq!(rt::parse_as(reference_form, text).as_ref(), Some(reference));
    assert_eq!(
        Temporal::parse_as(replacement.lexical_form().unwrap(), &replacement_text).as_ref(),
        Some(replacement)
    );
    assert_eq!(
        rt::parse_as(
            replacement_reference.lexical_form().unwrap(),
            &replacement_text
        )
        .as_ref(),
        Some(replacement_reference)
    );
    assert_no_standard_unwritten_zulu(form.kind, reference_form.kind, &replacement_text);
}

fn check_unwritten_zulu_baseline(
    text: &str,
    reference_form: rt::TemporalLexicalForm,
    replacements: &[(&str, rt::Offset)],
    competitors: &[(&Temporal, &rt::Temporal)],
) -> (usize, usize) {
    let form = ref_to_descriptor(reference_form);
    let value = Temporal::parse_as(form, text).unwrap();
    let reference = rt::parse_as(reference_form, text).unwrap();
    assert!(value.well_formed() && reference.well_formed());

    let local_reference_form = rt::TemporalLexicalForm {
        offset: rt::Offset::Local,
        ..reference_form
    };
    let local_form = ref_to_descriptor(local_reference_form);
    let local = Temporal::parse_as(local_form, text).unwrap();
    let local_reference = rt::parse_as(local_reference_form, text).unwrap();
    assert_eq!(local.to_string(), value.to_string());
    assert_eq!(local_reference.to_string(), reference.to_string());

    let mut zero_comparisons = 0_usize;
    for (suffix, replacement_offset) in replacements {
        let replacement_reference_form = rt::TemporalLexicalForm {
            offset: *replacement_offset,
            ..reference_form
        };
        let replacement_form = ref_to_descriptor(replacement_reference_form);
        let replacement_text = if reference_form.infinity.is_some() {
            text.to_owned()
        } else {
            format!("{text}{suffix}")
        };
        let replacement = Temporal::parse_as(replacement_form, &replacement_text).unwrap();
        let replacement_reference =
            rt::parse_as(replacement_reference_form, &replacement_text).unwrap();
        assert!(replacement.well_formed() && replacement_reference.well_formed());
        zero_comparisons += check_unwritten_zulu_order(
            &value,
            &reference,
            &replacement,
            &replacement_reference,
            *replacement_offset,
            competitors,
        );
        check_unwritten_zulu_representation(
            text,
            suffix,
            (form, reference_form),
            (&value, &reference),
            (&replacement, &replacement_reference),
        );
        assert_eq!(
            Temporal::parse_as(local_form, text).as_ref(),
            Some(&local),
            "local descriptor for {text}"
        );
        assert_eq!(
            rt::parse_as(local_reference_form, text).as_ref(),
            Some(&local_reference),
            "reference local descriptor for {text}"
        );
    }
    (replacements.len(), zero_comparisons)
}

type ReferenceTemporalForm = (String, rt::TemporalLexicalForm);
type ScopedOffset = (&'static str, rt::Offset);

struct UnwrittenZuluPartition<'a> {
    replacements: Vec<ScopedOffset>,
    baselines: Vec<&'a ReferenceTemporalForm>,
}

fn partition_unwritten_zulu_scope<'a>(
    forms: &'a [ReferenceTemporalForm],
    offset_scope: &[ScopedOffset; 16],
) -> UnwrittenZuluPartition<'a> {
    let replacements: Vec<_> = offset_scope
        .iter()
        .copied()
        .filter(|(_, offset)| is_unwritten_zulu_replacement(*offset))
        .collect();
    let unrelated: Vec<_> = offset_scope
        .iter()
        .copied()
        .filter(|(_, offset)| {
            *offset != rt::Offset::Zulu(rt::ZuluCase::Unwritten)
                && !is_unwritten_zulu_replacement(*offset)
        })
        .collect();
    assert_eq!(offset_scope.len(), 16);
    assert_eq!(replacements.len(), 12);
    assert_eq!(
        replacements
            .iter()
            .filter(|(_, offset)| is_written_zero_offset(*offset))
            .count(),
        10
    );
    assert_eq!(
        unrelated,
        vec![
            (
                "+05:30:00",
                rt::Offset::Fixed(330, rt::OffsetForm::ExtendedSeconds),
            ),
            ("-04:56:02", rt::Offset::FixedSeconds(-17_762)),
            ("", rt::Offset::Fixed(330, rt::OffsetForm::Unwritten)),
        ]
    );
    for (_, offset) in &unrelated {
        assert!(
            forms
                .iter()
                .any(|(_, form)| form.infinity.is_none() && form.offset == *offset)
        );
    }
    for (text, form) in forms {
        assert_no_standard_unwritten_zulu(ref_to_descriptor(*form).kind, form.kind, text);
    }
    let baselines: Vec<_> = forms
        .iter()
        .filter(|(_, form)| form.offset == rt::Offset::Zulu(rt::ZuluCase::Unwritten))
        .collect();
    assert_eq!(baselines.len(), 62);
    assert_eq!(
        baselines
            .iter()
            .map(|(text, form)| {
                rt::canonical_bytes(&rt::parse_as(*form, text).unwrap()).unwrap()
            })
            .collect::<HashSet<_>>()
            .len(),
        62,
        "every unwritten-Zulu baseline is representation-distinct"
    );
    UnwrittenZuluPartition {
        replacements,
        baselines,
    }
}

#[derive(Default)]
struct UnwrittenZuluCounts {
    cases: usize,
    zero_comparisons: usize,
    finite_dates: usize,
    finite_times: usize,
    finite_date_times: usize,
    date_shapes: HashSet<Option<(bool, bool, bool)>>,
    negative_date_infinities: usize,
    positive_date_infinities: usize,
    time_infinities: usize,
    negative_date_time_infinities: usize,
    positive_date_time_infinities: usize,
}

fn check_all_unwritten_zulu_baselines(
    baselines: Vec<&ReferenceTemporalForm>,
    replacements: &[ScopedOffset],
    competitors: &[(&Temporal, &rt::Temporal)],
) -> UnwrittenZuluCounts {
    let mut counts = UnwrittenZuluCounts::default();
    for (text, reference_form) in baselines {
        match (reference_form.kind, reference_form.infinity) {
            (rt::TemporalForm::Date, None) => {
                counts.finite_dates += 1;
                counts.date_shapes.insert(reference_form.date_shape);
            }
            (rt::TemporalForm::Time, None) => counts.finite_times += 1,
            (rt::TemporalForm::DateTime, None) => counts.finite_date_times += 1,
            (rt::TemporalForm::Date, Some(rt::TemporalInfinity::Negative)) => {
                counts.negative_date_infinities += 1;
            }
            (rt::TemporalForm::Date, Some(rt::TemporalInfinity::Positive)) => {
                counts.positive_date_infinities += 1;
            }
            (rt::TemporalForm::Time, Some(_)) => counts.time_infinities += 1,
            (rt::TemporalForm::DateTime, Some(rt::TemporalInfinity::Negative)) => {
                counts.negative_date_time_infinities += 1;
            }
            (rt::TemporalForm::DateTime, Some(rt::TemporalInfinity::Positive)) => {
                counts.positive_date_time_infinities += 1;
            }
        }
        let (baseline_cases, baseline_comparisons) =
            check_unwritten_zulu_baseline(text, *reference_form, replacements, competitors);
        counts.cases += baseline_cases;
        counts.zero_comparisons += baseline_comparisons;
    }
    counts
}

#[test]
fn unwritten_offsets_and_infinities_remain_distinct() {
    let scope = panproto_reference::enumerate::values::unwritten_zulu_temporal_scope();
    let partition = partition_unwritten_zulu_scope(&scope.forms, &scope.offsets);

    let (scope, infinities) = reference_scope();
    let competitors: Vec<_> = scope
        .iter()
        .map(|item| (&item.value, &item.reference))
        .chain(infinities.iter().map(|item| (&item.value, &item.reference)))
        .collect();
    assert_eq!(competitors.len(), 658);
    let counts = check_all_unwritten_zulu_baselines(
        partition.baselines,
        &partition.replacements,
        &competitors,
    );
    assert_eq!(counts.cases, 62 * 12);
    assert_eq!(counts.cases, 744);
    assert_eq!(counts.zero_comparisons, 62 * 11 * 658);
    assert_eq!(counts.zero_comparisons, 448_756);
    assert_eq!(counts.finite_dates, 12);
    assert_eq!(counts.finite_times, 14);
    assert_eq!(counts.finite_date_times, 4 * 8);
    assert_eq!(
        counts.date_shapes,
        [
            Some((true, false, false)),
            Some((true, true, false)),
            Some((true, true, true)),
            Some((false, true, false)),
            Some((false, true, true)),
            Some((false, false, true)),
        ]
        .into_iter()
        .collect()
    );
    assert_eq!(counts.negative_date_infinities, 1);
    assert_eq!(counts.positive_date_infinities, 1);
    assert_eq!(counts.time_infinities, 0);
    assert_eq!(counts.negative_date_time_infinities, 1);
    assert_eq!(counts.positive_date_time_infinities, 1);
    println!(
        "unwritten Zulu replacements: 16 scoped offsets (12 replacements, 10 written zero forms, 3 unrelated controls); {} replacement cases over 58 finite bodies and 4 infinities; {} zero-offset/competitor pairs; 658 standard-reading checks; {} finite dates over {} shapes",
        counts.cases,
        counts.zero_comparisons,
        counts.finite_dates,
        counts.date_shapes.len()
    );
}

#[test]
fn a_zoned_infinity_orders_every_truncated_date_of_its_zoning() {
    let infinity = scoped_infinity(
        TemporalForm::Date,
        Offset::Zulu(ZuluCase::Upper),
        TemporalInfinity::Positive,
    )
    .value;
    let truncated = Temporal::parse_date("---01+05:30:00").unwrap();

    assert_eq!(
        infinity.cmp_chronological(&truncated),
        Some(Ordering::Greater)
    );
    assert_eq!(truncated.cmp_chronological(&infinity), Some(Ordering::Less));
}

#[test]
fn an_infinity_spelling_reads_as_a_date_without_a_declared_form() {
    for (text, sign) in [
        ("infinity", TemporalInfinity::Positive),
        ("-infinity", TemporalInfinity::Negative),
    ] {
        let value = Temporal::parse(text).unwrap();
        assert_eq!(
            value.lexical_form().map(|form| form.kind),
            Some(TemporalForm::Date)
        );
        assert_eq!(
            value.lexical_form().and_then(|form| form.infinity),
            Some(sign)
        );
        assert_eq!(value.to_string(), text);
    }
}

#[test]
fn an_offset_with_a_non_ascii_character_is_refused() {
    for text in ["2018+aé1", "12:00+aé1", "2018-01-02T12:00+aé1", "+aé1"] {
        assert!(Temporal::parse(text).is_err(), "{text}");
        assert!(Temporal::parse_time(text).is_err(), "{text}");
        assert!(Temporal::parse_date(text).is_err(), "{text}");
    }
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

const fn offset_form_to_ref(form: OffsetForm) -> rt::OffsetForm {
    match form {
        OffsetForm::Extended => rt::OffsetForm::Extended,
        OffsetForm::Basic => rt::OffsetForm::Basic,
        OffsetForm::HourOnly => rt::OffsetForm::HourOnly,
        OffsetForm::ExtendedSeconds => rt::OffsetForm::ExtendedSeconds,
        OffsetForm::Unwritten => rt::OffsetForm::Unwritten,
    }
}

const fn offset_to_ref(offset: Offset) -> rt::Offset {
    match offset {
        Offset::Local => rt::Offset::Local,
        Offset::Zulu(ZuluCase::Upper) => rt::Offset::Zulu(rt::ZuluCase::Upper),
        Offset::Zulu(ZuluCase::Lower) => rt::Offset::Zulu(rt::ZuluCase::Lower),
        Offset::Zulu(ZuluCase::Unwritten) => rt::Offset::Zulu(rt::ZuluCase::Unwritten),
        Offset::Fixed(minutes) => rt::Offset::Fixed(minutes, rt::OffsetForm::Extended),
        Offset::FixedForm(minutes, form) => rt::Offset::Fixed(minutes, offset_form_to_ref(form)),
        Offset::FixedSeconds(seconds) => rt::Offset::FixedSeconds(seconds),
        Offset::NegZero => rt::Offset::NegZero(rt::OffsetForm::Extended),
        Offset::NegZeroForm(form) => rt::Offset::NegZero(offset_form_to_ref(form)),
    }
}

const fn form_to_ref(form: TemporalForm) -> rt::TemporalForm {
    match form {
        TemporalForm::Date => rt::TemporalForm::Date,
        TemporalForm::Time => rt::TemporalForm::Time,
        TemporalForm::DateTime => rt::TemporalForm::DateTime,
    }
}

/// The reference descriptor of a production descriptor, field by field.
const fn descriptor_to_ref(form: TemporalLexicalForm) -> rt::TemporalLexicalForm {
    rt::TemporalLexicalForm {
        kind: form_to_ref(form.kind),
        date_shape: form.date_shape,
        time_shape: form.time_shape,
        separator: match form.separator {
            None => None,
            Some(DateTimeSeparator::Upper) => Some(rt::Separator::Upper),
            Some(DateTimeSeparator::Lower) => Some(rt::Separator::Lower),
            Some(DateTimeSeparator::Space) => Some(rt::Separator::Space),
        },
        offset: offset_to_ref(form.offset),
        infinity: match form.infinity {
            None => None,
            Some(TemporalInfinity::Negative) => Some(rt::TemporalInfinity::Negative),
            Some(TemporalInfinity::Positive) => Some(rt::TemporalInfinity::Positive),
        },
    }
}

const fn offset_form_from_ref(form: rt::OffsetForm) -> OffsetForm {
    match form {
        rt::OffsetForm::Extended => OffsetForm::Extended,
        rt::OffsetForm::Basic => OffsetForm::Basic,
        rt::OffsetForm::HourOnly => OffsetForm::HourOnly,
        rt::OffsetForm::ExtendedSeconds => OffsetForm::ExtendedSeconds,
        rt::OffsetForm::Unwritten => OffsetForm::Unwritten,
    }
}

/// The production offset of a reference offset: the extended minute and
/// negative-zero forms are the historical `Fixed` and `NegZero` variants,
/// every other written form is kept beside its magnitude.
const fn offset_from_ref(offset: rt::Offset) -> Offset {
    match offset {
        rt::Offset::Local => Offset::Local,
        rt::Offset::Zulu(rt::ZuluCase::Upper) => Offset::Zulu(ZuluCase::Upper),
        rt::Offset::Zulu(rt::ZuluCase::Lower) => Offset::Zulu(ZuluCase::Lower),
        rt::Offset::Zulu(rt::ZuluCase::Unwritten) => Offset::Zulu(ZuluCase::Unwritten),
        rt::Offset::Fixed(minutes, rt::OffsetForm::Extended) => Offset::Fixed(minutes),
        rt::Offset::Fixed(minutes, form) => Offset::FixedForm(minutes, offset_form_from_ref(form)),
        rt::Offset::FixedSeconds(seconds) => Offset::FixedSeconds(seconds),
        rt::Offset::NegZero(rt::OffsetForm::Extended) => Offset::NegZero,
        rt::Offset::NegZero(form) => Offset::NegZeroForm(offset_form_from_ref(form)),
    }
}

const fn form_from_ref(form: rt::TemporalForm) -> TemporalForm {
    match form {
        rt::TemporalForm::Date => TemporalForm::Date,
        rt::TemporalForm::Time => TemporalForm::Time,
        rt::TemporalForm::DateTime => TemporalForm::DateTime,
    }
}

/// The production descriptor of a reference descriptor, field by field:
/// the inverse of [`descriptor_to_ref`].
const fn ref_to_descriptor(form: rt::TemporalLexicalForm) -> TemporalLexicalForm {
    TemporalLexicalForm {
        kind: form_from_ref(form.kind),
        date_shape: form.date_shape,
        time_shape: form.time_shape,
        separator: match form.separator {
            None => None,
            Some(rt::Separator::Upper) => Some(DateTimeSeparator::Upper),
            Some(rt::Separator::Lower) => Some(DateTimeSeparator::Lower),
            Some(rt::Separator::Space) => Some(DateTimeSeparator::Space),
        },
        offset: offset_from_ref(form.offset),
        infinity: match form.infinity {
            None => None,
            Some(rt::TemporalInfinity::Negative) => Some(TemporalInfinity::Negative),
            Some(rt::TemporalInfinity::Positive) => Some(TemporalInfinity::Positive),
        },
    }
}

/// The reference reading of one scoped lexeme under the mapped descriptor,
/// with the per-value laws checked against production: both readers accept
/// the lexeme, both displays restore it, both values report the descriptor,
/// the canonical bytes are the reference's value bytes (temporal tag 15
/// followed by the production payload), and both mark an unwritten UTC zone
/// alike.
fn reference_reading(value: &Temporal, form: TemporalLexicalForm, text: &str) -> rt::Temporal {
    let reference_form = descriptor_to_ref(form);
    let reference = rt::parse_as(reference_form, text)
        .unwrap_or_else(|| panic!("the reference must read {text} under {reference_form:?}"));
    assert!(rt::parse_display(reference_form, text), "{text}");
    assert!(rt::display_parse(&reference), "{text}");
    assert_eq!(reference.to_string(), value.to_string(), "{text}");
    assert_eq!(reference.lexical_form(), Some(reference_form), "{text}");
    let mut tagged = vec![15];
    tagged.extend_from_slice(&value.canonical_bytes());
    assert_eq!(rt::canonical_bytes(&reference), Some(tagged), "{text}");
    assert_eq!(
        rt::unwritten_zulu(&reference),
        form.offset == Offset::Zulu(ZuluCase::Unwritten),
        "{text}"
    );
    reference
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
    reference: rt::Temporal,
    class: ComparabilityClass,
}

struct ScopedInfinity {
    value: Temporal,
    reference: rt::Temporal,
    kind: TemporalForm,
    zoned: bool,
    sign: TemporalInfinity,
}

fn scoped_value(
    text: &str,
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
    let value = Temporal::parse_as(form, text)
        .unwrap_or_else(|| panic!("finite-scope lexeme must parse under its descriptor: {text}"));
    assert_eq!(value.to_string(), text);
    assert_eq!(value.lexical_form(), Some(form));
    let reference = reference_reading(&value, form, text);
    let truncated = date_shape.is_some_and(|(year, _, _)| !year)
        || (date_shape.is_none() && time_shape.is_some_and(|(hour, _, _)| !hour));
    ScopedTemporal {
        value,
        reference,
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
    let reference = reference_reading(&value, form, text);
    ScopedInfinity {
        value,
        reference,
        kind,
        zoned: offset.is_zoned(),
        sign,
    }
}

fn infinity_applies(class: ComparabilityClass, infinity: &ScopedInfinity) -> bool {
    // An infinity lies beyond every value of its form and zoning, whatever
    // the class's truncated offset.
    class.kind != TemporalForm::Time && class.kind == infinity.kind && class.zoned == infinity.zoned
}

/// The written temporal forms of the reference enumerator, read by
/// production under the adapted descriptor: the finite forms in enumerator
/// order and the infinities in enumerator order. Every reference descriptor
/// survives the round trip through the production descriptor.
fn reference_scope() -> (Vec<ScopedTemporal>, Vec<ScopedInfinity>) {
    let forms = panproto_reference::enumerate::values::temporal_forms();
    assert_eq!(forms.len(), 12 * 11 + 14 * 11 + 4 * 8 * 11 + 2 * 5 * 2);
    assert_eq!(forms.len(), 658);

    let mut scope = Vec::new();
    let mut infinities = Vec::new();
    for (text, reference_form) in &forms {
        let form = ref_to_descriptor(*reference_form);
        assert_eq!(descriptor_to_ref(form), *reference_form, "{text}");
        match form.infinity {
            None => scope.push(scoped_value(
                text,
                form.kind,
                form.date_shape,
                form.time_shape,
                form.separator,
                form.offset,
            )),
            Some(sign) => {
                assert!(form.date_shape.is_none() && form.time_shape.is_none());
                assert!(form.separator.is_none());
                let infinity = scoped_infinity(form.kind, form.offset, sign);
                assert_eq!(infinity.value.to_string(), *text);
                infinities.push(infinity);
            }
        }
    }
    (scope, infinities)
}

/// The distinct offsets of some descriptors, in first-seen order.
fn distinct_offsets(forms: impl Iterator<Item = Offset>) -> Vec<Offset> {
    let mut out: Vec<Offset> = Vec::new();
    for offset in forms {
        if !out.contains(&offset) {
            out.push(offset);
        }
    }
    out
}

fn comparability_classes(scope: &[ScopedTemporal]) -> Vec<(ComparabilityClass, Vec<usize>)> {
    let mut classes: Vec<(ComparabilityClass, Vec<usize>)> = Vec::new();
    for (index, item) in scope.iter().enumerate() {
        if let Some((_, members)) = classes.iter_mut().find(|(class, _)| *class == item.class) {
            members.push(index);
        } else {
            classes.push((item.class, vec![index]));
        }
    }

    classes
}

fn check_finite_pairs(scope: &[ScopedTemporal]) -> (usize, usize) {
    let mut all_pairs = 0_usize;
    let mut comparable_pairs = 0_usize;
    for left in scope {
        for right in scope {
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
    (all_pairs, comparable_pairs)
}

fn check_class_orders(
    scope: &[ScopedTemporal],
    classes: &[(ComparabilityClass, Vec<usize>)],
    infinities: &[ScopedInfinity],
) -> (usize, usize) {
    let mut class_pairs = 0_usize;
    let mut class_triples = 0_usize;
    for (class, members) in classes {
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
    (class_pairs, class_triples)
}

/// The production chronological order agrees with the reference order on
/// every ordered pair of the whole representation scope, in both operand
/// orders and including incomparable pairs.
fn check_reference_order(scope: &[ScopedTemporal], infinities: &[ScopedInfinity]) -> usize {
    let values: Vec<(&Temporal, &rt::Temporal)> = scope
        .iter()
        .map(|item| (&item.value, &item.reference))
        .chain(
            infinities
                .iter()
                .map(|infinity| (&infinity.value, &infinity.reference)),
        )
        .collect();
    let mut pairs = 0_usize;
    for (left, left_reference) in &values {
        for (right, right_reference) in &values {
            assert_eq!(
                left.cmp_chronological(right),
                rt::cmp_chronological(left_reference, right_reference),
                "{left} against {right}"
            );
            pairs += 1;
        }
    }
    pairs
}

fn check_canonical_injectivity(scope: &[ScopedTemporal], infinities: &[ScopedInfinity]) {
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
}

#[test]
fn sql_temporal_forms() {
    let (scope, infinities) = reference_scope();
    let count_kind = |kind| {
        scope
            .iter()
            .filter(|item| item.value.lexical_form().map(|form| form.kind) == Some(kind))
            .count()
    };
    let (dates, times, date_times) = (
        count_kind(TemporalForm::Date),
        count_kind(TemporalForm::Time),
        count_kind(TemporalForm::DateTime),
    );
    assert_eq!(dates, 12 * 11);
    assert_eq!(times, 14 * 11);
    assert_eq!(date_times, 4 * 8 * 11);
    assert_eq!(dates, 132);
    assert_eq!(times, 154);
    assert_eq!(date_times, 352);
    let finite_offsets = distinct_offsets(
        scope
            .iter()
            .filter_map(|item| item.value.lexical_form().map(|form| form.offset)),
    );
    assert_eq!(finite_offsets.len(), 11);
    let infinity_offsets = distinct_offsets(
        infinities
            .iter()
            .filter_map(|item| item.value.lexical_form().map(|form| form.offset)),
    );
    assert_eq!(infinity_offsets.len(), 5);
    assert_eq!(scope.len(), 638);
    assert_eq!(infinities.len(), 2 * infinity_offsets.len() * 2);
    assert_eq!(infinities.len(), 20);
    let classes = comparability_classes(&scope);
    assert_eq!(classes.len(), 54);
    assert_eq!(scope.len() + infinities.len(), 658);

    let (all_pairs, comparable_pairs) = check_finite_pairs(&scope);
    assert_eq!(all_pairs, 638 * 638);
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
        28_732
    );
    assert_eq!(
        class_sizes.iter().map(|size| size.pow(3)).sum::<usize>(),
        1_485_464
    );

    let (class_pairs, class_triples) = check_class_orders(&scope, &classes, &infinities);
    assert_eq!(class_pairs, 28_732);
    assert_eq!(class_triples, 1_485_464);

    check_canonical_injectivity(&scope, &infinities);
    let reference_pairs = check_reference_order(&scope, &infinities);
    assert_eq!(reference_pairs, 658 * 658);
    assert_eq!(reference_pairs, 432_964);

    println!("bound: forms=658");
    println!(
        "SQL temporal forms: {} values ({} finite: {dates} dates, {times} times, {date_times} date-times; {} infinities) over {} finite offsets and {} infinity offsets, {} finite comparability classes, {all_pairs} finite ordered pairs, {comparable_pairs} comparable finite pairs, {class_pairs} class-local pairs with infinities, {class_triples} class-local triples, {reference_pairs} representation pairs; the reference reads, displays, encodes and orders every value and ordered pair alike",
        scope.len() + infinities.len(),
        scope.len(),
        infinities.len(),
        finite_offsets.len(),
        infinity_offsets.len(),
        classes.len(),
    );
}

const KINDS: [TemporalForm; 3] = [
    TemporalForm::Date,
    TemporalForm::Time,
    TemporalForm::DateTime,
];

const UNITS: [TimeUnit; 4] = [
    TimeUnit::Seconds,
    TimeUnit::Millis,
    TimeUnit::Micros,
    TimeUnit::Nanos,
];

const WORKERS: usize = 4;

const fn unit_to_ref(unit: TimeUnit) -> rt::TimeUnit {
    match unit {
        TimeUnit::Seconds => rt::TimeUnit::Seconds,
        TimeUnit::Millis => rt::TimeUnit::Millis,
        TimeUnit::Micros => rt::TimeUnit::Micros,
        TimeUnit::Nanos => rt::TimeUnit::Nanos,
    }
}

/// The production reader of one carrier kind.
fn production_read(kind: TemporalForm, text: &str) -> Option<Temporal> {
    match kind {
        TemporalForm::Date => Temporal::parse_date(text).ok(),
        TemporalForm::Time => Temporal::parse_time(text).ok(),
        TemporalForm::DateTime => Temporal::parse_date_time(text).ok(),
    }
}

/// One lexeme accepted under one carrier kind by both readers.
struct AcceptedLexeme {
    text: String,
    kind: TemporalForm,
    value: Temporal,
    reference: rt::Temporal,
}

/// Read every lexeme under every kind with both readers, checking that they
/// agree on acceptance and, for an accepted lexeme, on the display, the
/// exact descriptor, the canonical bytes, both reference representation laws
/// and the production round trip under the descriptor.
fn read_lexemes(lexemes: &[String]) -> (Vec<AcceptedLexeme>, usize) {
    let mut accepted = Vec::new();
    let mut readings = 0_usize;
    for text in lexemes {
        for kind in KINDS {
            readings += 1;
            let production = production_read(kind, text);
            let reference = rt::parse(form_to_ref(kind), text);
            assert_eq!(
                production.is_some(),
                reference.is_some(),
                "{text} as {kind:?}: production {production:?}, reference {reference:?}"
            );
            let (Some(value), Some(reference)) = (production, reference) else {
                continue;
            };
            let display = value.to_string();
            assert_eq!(reference.to_string(), display, "{text} as {kind:?}");
            assert_eq!(display, *text, "{text} as {kind:?}");
            let form = value
                .lexical_form()
                .unwrap_or_else(|| panic!("{text} as {kind:?} has no production descriptor"));
            let reference_form = reference
                .lexical_form()
                .unwrap_or_else(|| panic!("{text} as {kind:?} has no reference descriptor"));
            assert_eq!(form.kind, kind, "{text}");
            assert_eq!(
                descriptor_to_ref(form),
                reference_form,
                "{text} as {kind:?}"
            );
            assert_eq!(
                ref_to_descriptor(reference_form),
                form,
                "{text} as {kind:?}"
            );
            let mut tagged = vec![15];
            tagged.extend_from_slice(&value.canonical_bytes());
            assert_eq!(
                rt::canonical_bytes(&reference),
                Some(tagged),
                "{text} as {kind:?}"
            );
            assert!(
                rt::parse_display(reference_form, text),
                "{text} as {kind:?}"
            );
            assert!(rt::display_parse(&reference), "{text} as {kind:?}");
            assert_eq!(
                Temporal::parse_as(form, &display).as_ref(),
                Some(&value),
                "{text} as {kind:?}"
            );
            accepted.push(AcceptedLexeme {
                text: text.clone(),
                kind,
                value,
                reference,
            });
        }
    }
    (accepted, readings)
}

/// Production and reference chronological orders agree on every ordered
/// pair of accepted values; the left operands are split across the workers.
/// Returns the pairs visited and the comparable pairs among them.
fn check_lexeme_order(accepted: &[AcceptedLexeme]) -> (usize, usize) {
    std::thread::scope(|scope| {
        let workers: [_; WORKERS] = std::array::from_fn(|worker| {
            scope.spawn(move || {
                let mut pairs = 0_usize;
                let mut comparable = 0_usize;
                for left in accepted.iter().skip(worker).step_by(WORKERS) {
                    for right in accepted {
                        let production = left.value.cmp_chronological(&right.value);
                        assert_eq!(
                            production,
                            rt::cmp_chronological(&left.reference, &right.reference),
                            "{} ({:?}) against {} ({:?})",
                            left.text,
                            left.kind,
                            right.text,
                            right.kind
                        );
                        pairs += 1;
                        comparable += usize::from(production.is_some());
                    }
                }
                (pairs, comparable)
            })
        });
        workers
            .into_iter()
            .map(|worker| worker.join().expect("an order worker panicked"))
            .fold((0, 0), |(pairs, comparable), (p, c)| {
                (pairs + p, comparable + c)
            })
    })
}

/// The comparability class key of a finite reference value: its shape, its
/// zoning and, for a truncated shape, its offset's minutes.
type ClassKey = (rt::Shape, bool, Option<i64>);

fn class_key(value: &rt::Temporal) -> ClassKey {
    let minutes = if value.is_truncated() {
        value.offset.minutes()
    } else {
        None
    };
    (value.shape(), value.offset.is_zoned(), minutes)
}

/// On every comparability class of the accepted values (the finite values
/// of one class key, joined by the infinities of their form and zoning) the
/// production order is a total preorder. Each class is sorted by the
/// production order, every value is ranked by the number of strict steps
/// before it, and every ordered pair of the class must compare as its ranks
/// do. A relation on a finite set is a total preorder exactly when it is the
/// order of integers pulled back along some ranking, so this decides
/// reflexivity, totality and transitivity over every triple of the class.
/// Returns the classes and the class-local ordered pairs checked.
fn check_lexeme_class_orders(accepted: &[AcceptedLexeme]) -> (usize, usize) {
    let mut classes: std::collections::BTreeMap<ClassKey, Vec<&AcceptedLexeme>> =
        std::collections::BTreeMap::new();
    let mut infinities: std::collections::BTreeMap<(rt::TemporalForm, bool), Vec<&AcceptedLexeme>> =
        std::collections::BTreeMap::new();
    for item in accepted {
        if let Some((_, form)) = item.reference.infinity {
            infinities
                .entry((form, item.reference.offset.is_zoned()))
                .or_default()
                .push(item);
        } else {
            classes
                .entry(class_key(&item.reference))
                .or_default()
                .push(item);
        }
    }
    let mut pairs = 0_usize;
    for (key, members) in &classes {
        let form = members[0].reference.form().unwrap();
        let mut class: Vec<&AcceptedLexeme> = members.clone();
        class.extend(infinities.get(&(form, key.1)).into_iter().flatten());
        class.sort_by(|a, b| {
            a.value
                .cmp_chronological(&b.value)
                .unwrap_or_else(|| panic!("{} and {} share a class", a.text, b.text))
        });
        let mut ranks = vec![0_usize; class.len()];
        for index in 1..class.len() {
            let step = class[index - 1]
                .value
                .cmp_chronological(&class[index].value)
                .unwrap();
            assert_ne!(step, Ordering::Greater, "{key:?}: sorted order");
            ranks[index] = ranks[index - 1] + usize::from(step == Ordering::Less);
        }
        let (class, ranks) = (&class, &ranks);
        pairs += std::thread::scope(|scope| {
            let workers: [_; WORKERS] = std::array::from_fn(|worker| {
                scope.spawn(move || {
                    let mut checked = 0_usize;
                    for left in (worker..class.len()).step_by(WORKERS) {
                        for right in 0..class.len() {
                            assert_eq!(
                                class[left].value.cmp_chronological(&class[right].value),
                                Some(ranks[left].cmp(&ranks[right])),
                                "{} against {} in class {key:?}",
                                class[left].text,
                                class[right].text
                            );
                            checked += 1;
                        }
                    }
                    checked
                })
            });
            workers
                .into_iter()
                .map(|worker| worker.join().expect("a class worker panicked"))
                .sum::<usize>()
        });
    }
    (classes.len(), pairs)
}

/// Production epoch counts agree with the reference on every accepted
/// date-time, unit and column kind. Returns the conversions checked and,
/// per unit in the order of [`UNITS`], those that yield a count.
fn check_lexeme_epoch_counts(accepted: &[AcceptedLexeme]) -> (usize, [usize; 4]) {
    let mut conversions = 0_usize;
    let mut counted = [0_usize; 4];
    for item in accepted
        .iter()
        .filter(|item| item.kind == TemporalForm::DateTime)
    {
        for (index, unit) in UNITS.into_iter().enumerate() {
            for instant in [true, false] {
                let production = item.value.to_epoch_count(unit, instant);
                assert_eq!(
                    production,
                    rt::to_epoch_count(&item.reference, unit_to_ref(unit), instant),
                    "{} {unit:?} instant={instant}",
                    item.text
                );
                conversions += 1;
                // A value read independently of the conversion that the
                // inverse accepts converts back to itself.
                if let Some(count) = production {
                    assert_eq!(
                        Temporal::from_epoch_count(count, unit, instant).as_ref(),
                        Ok(&item.value),
                        "{} {unit:?} instant={instant}",
                        item.text
                    );
                    assert_eq!(
                        rt::from_epoch_count(count, unit_to_ref(unit), instant).as_ref(),
                        Some(&item.reference),
                        "{} {unit:?} instant={instant}",
                        item.text
                    );
                    counted[index] += 1;
                }
            }
        }
    }
    (conversions, counted)
}

/// Read every form of the descriptor-read part of the scope under its
/// descriptor with both readers.
fn read_forms(forms: &[(String, rt::TemporalLexicalForm)]) -> Vec<AcceptedLexeme> {
    forms
        .iter()
        .map(|(text, reference_form)| {
            let form = ref_to_descriptor(*reference_form);
            let value = Temporal::parse_as(form, text)
                .unwrap_or_else(|| panic!("{text} under {form:?} must parse"));
            let reference = rt::parse_as(*reference_form, text)
                .unwrap_or_else(|| panic!("{text} under {reference_form:?} must parse"));
            assert!(rt::parse_display(*reference_form, text), "{text}");
            assert!(rt::display_parse(&reference), "{text}");
            AcceptedLexeme {
                text: text.clone(),
                kind: form.kind,
                value,
                reference,
            }
        })
        .collect()
}

#[test]
fn temporal_lexemes_agree_with_the_reference() {
    let scope = panproto_reference::enumerate::values::chronological_scope();
    let (lexemes, unwritten, forms) = (&scope.lexemes, &scope.unwritten, &scope.forms);
    assert_eq!(forms.len(), 12 * 11 + 14 * 11 + 4 * 8 * 11 + 2 * 5 * 2);
    // Per offset: dates 4 years x (1 + 3 months x (1 + 4 days)), times
    // 2 hours x (1 + 2 minutes x (1 + 3 seconds x 3 fractions)), and every
    // complete date (4 x 3 x 4) joined to every time; the nine offsets of the
    // order law and the two written offsets with seconds, `-04:56:02` and
    // `+05:30:00`.
    let dates_per_offset = 4 * (1 + 3 * (1 + 4));
    let times_per_offset = 2 * (1 + 2 * (1 + 3 * 3));
    let date_times_per_offset = 4 * 3 * 4 * times_per_offset;
    let per_offset = dates_per_offset + times_per_offset + date_times_per_offset;
    let bound = 11 * per_offset;
    assert_eq!(lexemes.len(), bound);
    assert_eq!(bound, 19_098 + 2 * 2122);
    assert_eq!(
        lexemes.iter().collect::<HashSet<_>>().len(),
        bound,
        "the lexemes are distinct"
    );
    // The valid fields of one offset: every year and year-month (4 + 12),
    // the full dates (months 1 and 12 take all four days; February takes 1
    // and 28, and 29 in the leap years 0 and 2000: 4 x 8 + 2 x 3 + 2 x 2 =
    // 42), every time (42), and every full date joined to every time.
    let valid_dates = 4 + 12 + 42;
    let valid_times = 42;
    let valid_date_times = 42 * 42;

    let (mut accepted, readings) = read_lexemes(lexemes);
    assert_eq!(readings, KINDS.len() * bound);
    let read_by_lexeme = accepted.len();
    let accepted_of = |accepted: &[AcceptedLexeme], kind: TemporalForm| {
        accepted.iter().filter(|item| item.kind == kind).count()
    };
    // Every offset of the product is admissible on every shape, so each
    // offset adds exactly the valid fields, read under their own kind only.
    assert_eq!(
        [
            accepted_of(&accepted, TemporalForm::Date),
            accepted_of(&accepted, TemporalForm::Time),
            accepted_of(&accepted, TemporalForm::DateTime),
        ],
        [11 * valid_dates, 11 * valid_times, 11 * valid_date_times]
    );
    assert_eq!(
        read_by_lexeme,
        11 * (valid_dates + valid_times + valid_date_times)
    );
    assert_eq!(read_by_lexeme, 20_504);
    // The unwritten offsets 0 and +05:30 on every local value of the product.
    let read_unwritten = read_forms(unwritten);
    assert_eq!(
        [
            accepted_of(&read_unwritten, TemporalForm::Date),
            accepted_of(&read_unwritten, TemporalForm::Time),
            accepted_of(&read_unwritten, TemporalForm::DateTime),
        ],
        [2 * valid_dates, 2 * valid_times, 2 * valid_date_times]
    );
    for item in &read_unwritten {
        assert!(
            matches!(
                item.reference.offset,
                rt::Offset::Fixed(0 | 330, rt::OffsetForm::Unwritten)
            ),
            "{} carries an unwritten offset",
            item.text
        );
        assert_eq!(item.value.to_string(), item.text, "{}", item.text);
    }
    accepted.extend(read_unwritten);
    accepted.extend(read_forms(forms));
    assert_eq!(
        accepted.len(),
        read_by_lexeme + 2 * (valid_dates + valid_times + valid_date_times) + forms.len()
    );
    let (dates, times, date_times) = (
        accepted_of(&accepted, TemporalForm::Date),
        accepted_of(&accepted, TemporalForm::Time),
        accepted_of(&accepted, TemporalForm::DateTime),
    );
    // The forms add 132 dates and 10 date infinities, 154 times, and 352
    // date-times and 10 date-time infinities.
    assert_eq!(
        [dates, times, date_times],
        [
            13 * valid_dates + 132 + 10,
            13 * valid_times + 154,
            13 * valid_date_times + 352 + 10
        ]
    );
    assert_eq!(accepted.len(), 24_890);

    let (pairs, comparable) = check_lexeme_order(&accepted);
    assert_eq!(pairs, accepted.len() * accepted.len());

    let mut by_bytes: std::collections::HashMap<Vec<u8>, &Temporal> =
        std::collections::HashMap::new();
    for item in &accepted {
        let bytes = item.value.canonical_bytes();
        if let Some(previous) = by_bytes.insert(bytes, &item.value) {
            assert_eq!(
                previous, &item.value,
                "{} shares canonical bytes",
                item.text
            );
        }
    }
    let distinct_values = accepted
        .iter()
        .map(|item| &item.value)
        .collect::<HashSet<_>>()
        .len();
    assert_eq!(by_bytes.len(), distinct_values);

    let (classes, class_pairs) = check_lexeme_class_orders(&accepted);
    // The 54 classes of the form scope: the lexeme product adds no shape the
    // forms lack, and its offsets give a truncated shape no new minutes.
    assert_eq!(classes, 54);
    assert_eq!(class_pairs, 340_699_116);

    let (conversions, counted) = check_lexeme_epoch_counts(&accepted);
    assert_eq!(conversions, date_times * UNITS.len() * 2);
    // Only a date-time with the `T` separator, a full date with a canonical
    // year, seconds 0 to 59, a local offset (wall clock) or `Z` (instant)
    // and exactly as many fraction digits as the unit has is counted. The
    // scope's fractions have at most one digit, so only `Seconds` counts:
    // the 42 full dates by the 2 x 2 x 2 times with seconds 0 or 59 and no
    // fraction, local and `Z`, and the two full dates of the forms at
    // `00:00:00`, local and `Z`. The other units are covered on the count
    // scope by `epoch_counts::an_epoch_count_round_trips`.
    assert_eq!(counted, [2 * 42 * 8 + 2 * 2, 0, 0, 0]);

    println!(
        "bound: lexemes={bound}, unwritten={}, forms={}",
        unwritten.len(),
        forms.len()
    );
    println!(
        "temporal lexemes: {bound} lexemes ({dates_per_offset} dates, {times_per_offset} times, {date_times_per_offset} date-times per offset over 11 offsets), {readings} kind readings, {read_by_lexeme} accepted, {} rejected; {} unwritten-offset values and {} forms read under their descriptors; {} values ({dates} dates, {times} times, {date_times} date-times); {pairs} ordered pairs, {comparable} comparable; {classes} classes, {class_pairs} class-local ordered pairs, each class a total preorder; {} distinct canonical byte strings for {distinct_values} distinct values; {conversions} epoch conversions, {counted:?} per unit with a count converted back; production agrees with the reference on every one",
        readings - read_by_lexeme,
        unwritten.len(),
        forms.len(),
        accepted.len(),
        by_bytes.len(),
    );
}
