#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Exact observable results of the exact-value types at the boundaries
//! their comparisons and arithmetic decide: kind names, the special values
//! of a decimal, binary16 rounding at the overflow threshold and its
//! closest-shortest text, the Gregorian leap rule and day count, offset
//! construction and parsing, and the admission of reduced and infinite
//! temporal values.

use panproto_expr::ExactParseError;
use panproto_expr::exact::{
    CivilDate, CivilTime, DateFields, DateTimeSeparator, DecimalValue, ExactKind, ExtendedTemporal,
    FidelityReason, Fraction, Offset, OffsetForm, Temporal, TemporalForm, TemporalInfinity,
    TemporalLexicalForm, TemporalParseError, TemporalYear, TimeFields, TimeUnit, ZuluCase,
    binary16_nearest,
};
use panproto_reference::temporal as rt;

// ---------------------------------------------------------------------------
// Kind names
// ---------------------------------------------------------------------------

#[test]
fn exact_kinds_are_named_as_error_messages_write_them() {
    assert_eq!(ExactKind::Decimal.as_str(), "decimal");
    assert_eq!(ExactKind::BigInt.as_str(), "bigint");
    assert_eq!(ExactKind::Temporal.as_str(), "temporal");
    let error = ExactParseError::new(ExactKind::Temporal, FidelityReason::InvalidTemporal);
    assert_eq!(
        error.to_string(),
        "cannot read a temporal exactly: not a valid date or time"
    );
}

// ---------------------------------------------------------------------------
// Decimals
// ---------------------------------------------------------------------------

#[test]
fn decimal_special_values_narrow_to_binary32_with_their_sign() {
    assert_eq!(
        DecimalValue::infinity(true).to_f32().to_bits(),
        f32::NEG_INFINITY.to_bits()
    );
    assert_eq!(
        DecimalValue::infinity(false).to_f32().to_bits(),
        f32::INFINITY.to_bits()
    );
    assert_eq!(
        DecimalValue::nan(true, false, "0").to_f32().to_bits(),
        0xffc0_0000
    );
    assert_eq!(
        DecimalValue::nan(false, true, "12").to_f32().to_bits(),
        0x7fc0_0000
    );
}

#[test]
fn a_decimal_infinity_deserializes_to_itself() {
    for negative in [false, true] {
        let infinity = DecimalValue::infinity(negative);
        let json = serde_json::to_string(&infinity).unwrap();
        let back: DecimalValue = serde_json::from_str(&json).unwrap();
        assert_eq!(back, infinity, "{json}");
    }
}

// ---------------------------------------------------------------------------
// Binary16
// ---------------------------------------------------------------------------

#[test]
fn binary16_rounding_decides_the_overflow_threshold_on_the_exact_decimal() {
    let l16 = |text: &str| binary16_nearest(&DecimalValue::parse(text).unwrap()).to_bits();
    // Below 65520 by less than binary64 resolves: the binary64 nearest is
    // 65520 itself, but the decimal is below the tie and rounds down.
    assert_eq!(l16("65519.9999999999999999999999"), 65_504.0_f64.to_bits());
    assert_eq!(
        l16("-65519.9999999999999999999999"),
        (-65_504.0_f64).to_bits()
    );
    // The tie itself goes to the even neighbour 2^16, an overflow.
    assert_eq!(l16("65520"), f64::INFINITY.to_bits());
    assert_eq!(l16("70000"), f64::INFINITY.to_bits());
    assert_eq!(l16("65519"), 65_504.0_f64.to_bits());
}

#[test]
fn binary16_shortest_text_is_some_exactly_for_finite_binary16_values() {
    let shortest = |x: f64| {
        DecimalValue::from_f16_shortest(x).map(|d| (d.negative, d.digits, d.exponent.to_string()))
    };
    // The binary16 nearest 0.1 reads back from "0.1": the two-digit
    // candidate `10e-2` carries its trailing zero into the exponent.
    assert_eq!(
        shortest(0.099_975_585_937_5),
        Some((false, "1".to_owned(), "-1".to_owned()))
    );
    assert_eq!(
        shortest(-0.099_975_585_937_5),
        Some((true, "1".to_owned(), "-1".to_owned()))
    );
    assert_eq!(
        shortest(65_504.0),
        Some((false, "655".to_owned(), "2".to_owned()))
    );
    assert_eq!(shortest(1.0), Some((false, "1".to_owned(), "0".to_owned())));
    // Not a binary16 value, and not finite.
    assert_eq!(shortest(0.1), None);
    assert_eq!(shortest(f64::INFINITY), None);
    assert_eq!(shortest(f64::NEG_INFINITY), None);
    assert_eq!(shortest(f64::from_bits(0x7ff8_0000_0000_0000)), None);
}

// ---------------------------------------------------------------------------
// Calendar arithmetic
// ---------------------------------------------------------------------------

#[test]
fn february_29_exists_exactly_in_reference_leap_years() {
    for year in -1_000_i64..=3_000 {
        assert_eq!(
            CivilDate::new(year, 2, 29).is_ok(),
            rt::is_leap(year),
            "{year}"
        );
    }
    assert!(CivilDate::new(1900, 2, 29).is_err());
    assert!(CivilDate::new(2000, 2, 29).is_ok());
}

#[test]
fn days_since_epoch_are_inverted_on_the_reference_day_number() {
    let epoch = rt::day_number(1970, 1, 1);
    // Two 400-year eras either side of the epoch, every day.
    for days in -2 * 146_097_i128..=2 * 146_097 {
        let date = CivilDate::from_days_since_epoch(days).unwrap();
        assert!((1..=12).contains(&date.month), "{days}");
        assert!(
            (1..=rt::month_length(date.year, date.month)).contains(&date.day),
            "{days}"
        );
        assert_eq!(
            rt::day_number(date.year, date.month, date.day) - epoch,
            days,
            "{days}"
        );
    }
    let at = |days| {
        let date = CivilDate::from_days_since_epoch(days).unwrap();
        (date.year, date.month, date.day)
    };
    assert_eq!(at(0), (1970, 1, 1));
    assert_eq!(at(-1), (1969, 12, 31));
    // The last day of a 400-year era.
    assert_eq!(at(11_016), (2000, 2, 29));
    assert_eq!(at(11_017), (2000, 3, 1));
}

#[test]
fn seconds_of_day_count_whole_seconds_since_midnight() {
    let seconds = |hour, minute, second| {
        CivilTime::new(hour, minute, second, Fraction::new("5").unwrap())
            .unwrap()
            .seconds_of_day()
    };
    assert_eq!(seconds(12, 34, 56), 45_296);
    assert_eq!(seconds(0, 0, 0), 0);
    assert_eq!(seconds(23, 59, 60), 86_400);
    assert_eq!(seconds(1, 1, 1), 3_661);
}

// ---------------------------------------------------------------------------
// Offsets
// ---------------------------------------------------------------------------

#[test]
fn fixed_form_refuses_a_day_an_uneven_hour_and_the_legacy_spelling() {
    assert_eq!(
        Offset::fixed_form(1_439, OffsetForm::Basic),
        Ok(Offset::FixedForm(1_439, OffsetForm::Basic))
    );
    assert!(Offset::fixed_form(1_440, OffsetForm::Basic).is_err());
    assert!(Offset::fixed_form(-1_440, OffsetForm::Basic).is_err());
    assert_eq!(
        Offset::fixed_form(60, OffsetForm::HourOnly),
        Ok(Offset::FixedForm(60, OffsetForm::HourOnly))
    );
    assert!(Offset::fixed_form(30, OffsetForm::HourOnly).is_err());
    assert!(Offset::fixed_form(90, OffsetForm::HourOnly).is_err());
    assert!(Offset::fixed_form(60, OffsetForm::Extended).is_err());
    assert!(Offset::fixed_form(0, OffsetForm::Extended).is_err());
}

#[test]
fn fixed_seconds_refuses_a_day_and_a_zero_seconds_field() {
    assert_eq!(Offset::fixed_seconds(30), Ok(Offset::FixedSeconds(30)));
    assert_eq!(
        Offset::fixed_seconds(86_399),
        Ok(Offset::FixedSeconds(86_399))
    );
    assert!(Offset::fixed_seconds(86_401).is_err());
    assert!(Offset::fixed_seconds(-86_401).is_err());
    assert!(Offset::fixed_seconds(120).is_err());
    assert!(Offset::fixed_seconds(-60).is_err());
    assert!(Offset::fixed_seconds(0).is_err());
}

#[test]
fn only_a_local_offset_is_unzoned() {
    assert!(!Offset::Local.is_zoned());
    assert!(Offset::Zulu(ZuluCase::Upper).is_zoned());
    assert!(Offset::Fixed(0).is_zoned());
    assert!(Offset::FixedSeconds(30).is_zoned());
}

#[test]
fn a_seconds_offset_has_its_whole_minutes() {
    assert_eq!(Offset::FixedSeconds(3_661).minutes(), 61);
    assert_eq!(Offset::FixedSeconds(-3_661).minutes(), -61);
    assert_eq!(Offset::FixedSeconds(59).minutes(), 0);
}

#[test]
fn a_negative_zero_form_deserializes_only_when_written() {
    let written = Offset::NegZeroForm(OffsetForm::Basic);
    let json = serde_json::to_string(&written).unwrap();
    assert_eq!(serde_json::from_str::<Offset>(&json).unwrap(), written);
    let hour_only = Offset::NegZeroForm(OffsetForm::HourOnly);
    let json = serde_json::to_string(&hour_only).unwrap();
    assert_eq!(serde_json::from_str::<Offset>(&json).unwrap(), hour_only);
    assert!(serde_json::from_str::<Offset>(r#"{"neg_zero_form":"unwritten"}"#).is_err());
}

#[test]
fn a_seconds_offset_displays_its_sign_only_when_negative() {
    assert_eq!(Offset::FixedSeconds(19_859).to_string(), "+05:30:59");
    assert_eq!(Offset::FixedSeconds(-30).to_string(), "-00:00:30");
    // Not an admitted offset, but its text still has a plus sign.
    assert_eq!(Offset::FixedSeconds(0).to_string(), "+00:00:00");
}

/// The offset `Temporal::parse_time` reads from the time `12:00:00` followed
/// by `offset`.
fn time_offset(offset: &str) -> Option<Offset> {
    Temporal::parse_time(&format!("12:00:00{offset}"))
        .ok()
        .and_then(|value| value.offset())
}

#[test]
fn a_seconds_offset_admits_each_field_up_to_its_bound() {
    assert_eq!(time_offset("+23:00:30"), Some(Offset::FixedSeconds(82_830)));
    assert_eq!(time_offset("+05:59:30"), Some(Offset::FixedSeconds(21_570)));
    assert_eq!(time_offset("+05:30:59"), Some(Offset::FixedSeconds(19_859)));
    assert_eq!(
        time_offset("-23:59:59"),
        Some(Offset::FixedSeconds(-86_399))
    );
    assert_eq!(time_offset("+24:00:30"), None);
    assert_eq!(time_offset("+05:60:30"), None);
    assert_eq!(time_offset("+05:30:60"), None);
}

#[test]
fn a_zero_seconds_field_keeps_the_minute_offset_and_its_sign() {
    assert_eq!(
        time_offset("-00:00:00"),
        Some(Offset::NegZeroForm(OffsetForm::ExtendedSeconds))
    );
    assert_eq!(
        time_offset("+00:00:00"),
        Some(Offset::FixedForm(0, OffsetForm::ExtendedSeconds))
    );
    assert_eq!(
        time_offset("-05:30:00"),
        Some(Offset::FixedForm(-330, OffsetForm::ExtendedSeconds))
    );
    assert_eq!(
        time_offset("+05:30:00"),
        Some(Offset::FixedForm(330, OffsetForm::ExtendedSeconds))
    );
}

// ---------------------------------------------------------------------------
// Reduced and infinite temporal values
// ---------------------------------------------------------------------------

/// A year written with its least width.
fn year(value: i64) -> TemporalYear {
    TemporalYear {
        value,
        negative_zero: false,
        width: value.unsigned_abs().to_string().len().max(4),
    }
}

const fn date(year: Option<TemporalYear>, month: Option<u8>, day: Option<u8>) -> DateFields {
    DateFields { year, month, day }
}

fn time(hour: Option<u8>, minute: Option<u8>, second: Option<u8>, fraction: &str) -> TimeFields {
    TimeFields {
        hour,
        minute,
        second,
        fraction: Fraction::new(fraction).unwrap(),
    }
}

/// A finite value of `date` and `time`, with the separator `T` when both
/// are present.
fn finite(date: Option<DateFields>, time: Option<TimeFields>, offset: Offset) -> ExtendedTemporal {
    let separator = (date.is_some() && time.is_some()).then_some(DateTimeSeparator::Upper);
    ExtendedTemporal {
        date,
        time,
        separator,
        offset,
        infinity: None,
    }
}

/// The year-month `2026-09`, local.
fn year_month(offset: Offset) -> ExtendedTemporal {
    finite(Some(date(Some(year(2026)), Some(9), None)), None, offset)
}

#[test]
fn an_extended_value_has_a_lexical_form_exactly_when_well_formed() {
    let Temporal::Extended(value) = Temporal::parse("2026-09").unwrap() else {
        panic!("a year-month is an extended value");
    };
    assert_eq!(value, year_month(Offset::Local));
    assert_eq!(
        value.lexical_form(),
        Some(TemporalLexicalForm {
            kind: TemporalForm::Date,
            date_shape: Some((true, true, false)),
            time_shape: None,
            separator: None,
            offset: Offset::Local,
            infinity: None,
        })
    );
    let month_13 = finite(
        Some(date(Some(year(2026)), Some(13), None)),
        None,
        Offset::Local,
    );
    assert!(!month_13.well_formed());
    assert_eq!(month_13.lexical_form(), None);
}

#[test]
fn an_infinity_carries_no_fields_and_is_not_a_time() {
    let base = || {
        ExtendedTemporal::infinite(
            TemporalInfinity::Positive,
            TemporalForm::Date,
            Offset::Local,
        )
        .unwrap()
    };
    assert!(base().well_formed());
    let with_date = ExtendedTemporal {
        date: Some(date(Some(year(2026)), Some(9), Some(28))),
        ..base()
    };
    assert!(!with_date.well_formed());
    let with_time = ExtendedTemporal {
        time: Some(time(Some(12), Some(0), Some(0), "")),
        ..base()
    };
    assert!(!with_time.well_formed());
    let with_separator = ExtendedTemporal {
        separator: Some(DateTimeSeparator::Upper),
        ..base()
    };
    assert!(!with_separator.well_formed());
    let a_time = ExtendedTemporal {
        infinity: Some((TemporalInfinity::Positive, TemporalForm::Time)),
        ..base()
    };
    assert!(!a_time.well_formed());
}

#[test]
fn reduced_date_fields_are_in_range_and_written_at_their_width() {
    assert!(year_month(Offset::Local).well_formed());
    let month_13 = finite(
        Some(date(Some(year(2026)), Some(13), None)),
        None,
        Offset::Local,
    );
    assert!(!month_13.well_formed());
    let wide = TemporalYear {
        width: 5,
        ..year(2026)
    };
    assert!(!finite(Some(date(Some(wide), Some(9), None)), None, Offset::Local).well_formed());
    let signed_five = TemporalYear {
        negative_zero: true,
        ..year(5)
    };
    assert!(
        !finite(
            Some(date(Some(signed_five), Some(9), None)),
            None,
            Offset::Local
        )
        .well_formed()
    );
}

#[test]
fn reduced_time_fields_are_in_range_and_a_fraction_needs_a_second() {
    let reduced = |fields| finite(None, Some(fields), Offset::Local).well_formed();
    assert!(reduced(time(Some(12), Some(30), None, "")));
    assert!(reduced(time(Some(23), Some(59), None, "")));
    assert!(!reduced(time(Some(24), Some(0), None, "")));
    assert!(!reduced(time(Some(12), Some(60), None, "")));
    // A complete time keeps the legacy carrier under a legacy offset, so the
    // seconds bound is checked under a basic-form offset.
    let complete =
        |fields| finite(None, Some(fields), Offset::FixedForm(60, OffsetForm::Basic)).well_formed();
    assert!(complete(time(Some(12), Some(0), Some(60), "")));
    assert!(!complete(time(Some(12), Some(0), Some(61), "")));
    assert!(!reduced(time(Some(12), Some(30), None, "5")));
}

#[test]
fn a_reduced_date_time_has_a_full_or_day_only_date_and_an_hour() {
    let hour = || Some(time(Some(12), None, None, ""));
    let date_time = |fields, time| finite(Some(fields), time, Offset::Local).well_formed();
    assert!(date_time(date(None, None, Some(28)), hour()));
    assert!(date_time(date(Some(year(2026)), Some(9), Some(28)), hour()));
    assert!(!date_time(date(None, Some(9), Some(28)), hour()));
    assert!(!date_time(date(Some(year(2026)), None, None), hour()));
    assert!(!date_time(date(Some(year(2026)), Some(9), None), hour()));
    assert!(!date_time(
        date(Some(year(2026)), Some(9), Some(28)),
        Some(time(None, Some(30), Some(0), ""))
    ));
}

#[test]
fn an_offset_of_an_extended_value_is_admitted_within_its_form() {
    let admitted = |offset| year_month(offset).well_formed();
    assert!(admitted(Offset::FixedForm(1_439, OffsetForm::Basic)));
    assert!(!admitted(Offset::FixedForm(1_440, OffsetForm::Basic)));
    assert!(admitted(Offset::FixedForm(120, OffsetForm::HourOnly)));
    assert!(!admitted(Offset::FixedForm(90, OffsetForm::HourOnly)));
    assert!(!admitted(Offset::FixedForm(60, OffsetForm::Extended)));
    assert!(admitted(Offset::FixedSeconds(30)));
    assert!(admitted(Offset::FixedSeconds(-86_399)));
    assert!(!admitted(Offset::FixedSeconds(120)));
    assert!(!admitted(Offset::FixedSeconds(-60)));
    assert!(!admitted(Offset::FixedSeconds(86_401)));
}

#[test]
fn a_month_day_has_the_days_of_its_month_in_a_leap_year() {
    assert!(Temporal::parse_date("--02-29").is_ok());
    assert!(Temporal::parse_date("--02-30").is_err());
    assert!(Temporal::parse_date("--04-30").is_ok());
    assert!(Temporal::parse_date("--04-31").is_err());
    assert!(Temporal::parse_date("--01-31").is_ok());
}

#[test]
fn a_date_time_lexeme_needs_a_full_or_day_only_date_and_an_hour() {
    assert!(Temporal::parse_date_time("---28T12").is_ok());
    assert!(Temporal::parse_date_time("2026-09-28T12").is_ok());
    assert!(Temporal::parse_date_time("--09-28T12").is_err());
    assert!(Temporal::parse_date_time("2026T12").is_err());
    assert!(Temporal::parse_date_time("2026-09T12").is_err());
    assert!(Temporal::parse_date_time("2026-09-28T-30:00").is_err());
}

#[test]
fn parse_as_supplies_only_an_unwritten_offset() {
    let zulu = Temporal::parse("2026-09-28T12:34:56Z").unwrap();
    let form = zulu.lexical_form().unwrap();
    assert_eq!(Temporal::parse_as(form, "2026-09-28T12:34:56Z"), Some(zulu));
    assert_eq!(Temporal::parse_as(form, "2026-09-28T12:34:56"), None);
    assert_eq!(Temporal::parse_as(form, "2026-09-28T12:34:56+05:00"), None);
}

#[test]
fn parse_union_refuses_text_no_form_reads_as_invalid() {
    assert_eq!(
        Temporal::parse_union("not a time"),
        Err(TemporalParseError::InvalidTemporal)
    );
    assert_eq!(
        Temporal::parse_union(""),
        Err(TemporalParseError::InvalidTemporal)
    );
    assert_eq!(
        Temporal::parse_union("2026-09-28"),
        Ok(Temporal::Date(CivilDate::new(2026, 9, 28).unwrap()))
    );
}

#[test]
fn time_units_read_and_write_their_names() {
    for (name, unit) in [
        ("s", TimeUnit::Seconds),
        ("ms", TimeUnit::Millis),
        ("us", TimeUnit::Micros),
        ("ns", TimeUnit::Nanos),
    ] {
        assert_eq!(TimeUnit::parse(name), Some(unit));
        assert_eq!(unit.as_str(), name);
    }
    for name in ["", "m", "S", "sec", "xyzzy"] {
        assert_eq!(TimeUnit::parse(name), None, "{name}");
    }
}
