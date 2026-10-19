//! An epoch count of a time unit converts to a date-time and back, exactly
//! as the reference converts it.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use panproto_expr::FidelityReason;
use panproto_expr::exact::{CivilDate, Temporal, TimeUnit};
use panproto_reference::temporal as rt;

const UNITS: [TimeUnit; 4] = [
    TimeUnit::Seconds,
    TimeUnit::Millis,
    TimeUnit::Micros,
    TimeUnit::Nanos,
];

const fn unit_to_ref(unit: TimeUnit) -> rt::TimeUnit {
    match unit {
        TimeUnit::Seconds => rt::TimeUnit::Seconds,
        TimeUnit::Millis => rt::TimeUnit::Millis,
        TimeUnit::Micros => rt::TimeUnit::Micros,
        TimeUnit::Nanos => rt::TimeUnit::Nanos,
    }
}

/// The reference's value bytes of a production temporal: temporal tag 15
/// followed by the production payload.
fn tagged_bytes(value: &Temporal) -> Vec<u8> {
    let mut out = vec![15];
    out.extend_from_slice(&value.canonical_bytes());
    out
}

/// The least and the greatest count of `unit` whose date-time has a year
/// within `i64`: the first moment of year `i64::MIN` and the last unit of
/// year `i64::MAX`.
fn domain(unit: TimeUnit) -> (i128, i128) {
    let ticks = 10_i128.pow(unit.fraction_digits());
    let first = CivilDate::new(i64::MIN, 1, 1).unwrap().days_since_epoch();
    let last = CivilDate::new(i64::MAX, 12, 31).unwrap().days_since_epoch();
    (first * 86_400 * ticks, (last + 1) * 86_400 * ticks - 1)
}

/// Both round trips hold at each unit, for an instant column and a
/// wall-clock one, on every count in [-10^4, 10^4] and within 10^4 of each
/// bound of the domain; a count beyond either bound is refused with
/// `Overflow`; and the inverse refuses every value outside the image. At
/// every count the reference conversion yields the same value (the same
/// display and the same canonical bytes), the same inverse count and the
/// same refusals.
#[test]
fn an_epoch_count_round_trips() {
    let mut checked = 0_u64;
    for unit in UNITS {
        let (lo, hi) = domain(unit);
        let counts = (-10_000..=10_000)
            .chain(lo..=lo + 10_000)
            .chain(hi - 10_000..=hi);
        for count in counts {
            for instant in [true, false] {
                let value = Temporal::from_epoch_count(count, unit, instant)
                    .unwrap_or_else(|e| panic!("{count} {unit:?}: {e}"));
                assert_eq!(value.to_epoch_count(unit, instant), Some(count), "{value}");
                let back = Temporal::from_epoch_count(count, unit, instant).unwrap();
                assert_eq!(back, value);
                assert_eq!(value.to_epoch_count(unit, !instant), None, "{value}");
                let reference = rt::from_epoch_count(count, unit_to_ref(unit), instant)
                    .unwrap_or_else(|| panic!("the reference refused {count} {unit:?}"));
                assert_eq!(reference.to_string(), value.to_string(), "{count} {unit:?}");
                assert_eq!(
                    rt::canonical_bytes(&reference),
                    Some(tagged_bytes(&value)),
                    "{count} {unit:?}"
                );
                assert_eq!(
                    rt::to_epoch_count(&reference, unit_to_ref(unit), instant),
                    Some(count),
                    "{value}"
                );
                assert_eq!(
                    rt::to_epoch_count(&reference, unit_to_ref(unit), !instant),
                    None,
                    "{value}"
                );
                checked += 1;
            }
        }
        for beyond in [lo - 1, hi + 1, i128::MIN, i128::MAX] {
            assert_eq!(
                Temporal::from_epoch_count(beyond, unit, true).map_err(|e| e.reason),
                Err(FidelityReason::Overflow),
                "{beyond} {unit:?}"
            );
            assert_eq!(
                rt::from_epoch_count(beyond, unit_to_ref(unit), true),
                None,
                "{beyond} {unit:?}"
            );
        }
        println!(
            "epoch counts: unit {}, domain [{lo}, {hi}], 20001 + 2 x 10001 counts, 2 columns, all agree with the reference",
            unit.as_str()
        );
    }
    assert_eq!(checked, 4 * (20_001 + 2 * 10_001) * 2);

    // Outside the image: another fraction length, offset, separator or
    // case of `Z`, second 60, a date or a time.
    let parse = |text: &str| Temporal::parse(text).unwrap();
    for (text, unit) in [
        ("2024-01-02T03:04:05.123Z", TimeUnit::Seconds),
        ("2024-01-02T03:04:05Z", TimeUnit::Millis),
        ("2024-01-02T03:04:05.1234Z", TimeUnit::Millis),
        ("2024-01-02T03:04:05+00:00", TimeUnit::Seconds),
        ("2024-01-02T03:04:05-00:00", TimeUnit::Seconds),
        ("2024-01-02t03:04:05Z", TimeUnit::Seconds),
        ("2024-01-02 03:04:05Z", TimeUnit::Seconds),
        ("2024-01-02T03:04:05z", TimeUnit::Seconds),
        ("2016-12-31T23:59:60Z", TimeUnit::Seconds),
        ("2024-01-02", TimeUnit::Seconds),
        ("03:04:05Z", TimeUnit::Seconds),
    ] {
        assert_eq!(parse(text).to_epoch_count(unit, true), None, "{text}");
        let kind = match parse(text) {
            Temporal::Date(_) => rt::TemporalKind::Date,
            Temporal::Time(..) => rt::TemporalKind::Time,
            Temporal::DateTime(..) => rt::TemporalKind::DateTime,
            Temporal::Extended(value) => panic!("{text} read as a reduced form: {value:?}"),
        };
        let reference = rt::parse(kind, text).unwrap();
        assert_eq!(
            rt::to_epoch_count(&reference, unit_to_ref(unit), true),
            None,
            "{text}"
        );
    }
    assert_eq!(
        parse("1970-01-01T00:00:00.000000001Z").to_epoch_count(TimeUnit::Nanos, true),
        Some(1)
    );
    assert_eq!(
        Temporal::from_epoch_count(86_399, TimeUnit::Seconds, false)
            .unwrap()
            .to_string(),
        "1970-01-01T23:59:59"
    );
}
