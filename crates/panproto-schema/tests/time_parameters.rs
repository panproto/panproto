//! The time unit and time zone of a temporal column are value parameters.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeMap;

use panproto_expr::exact::TimeUnit;
use panproto_gat::Name;
use panproto_schema::{
    ConstraintSemantics, ConstraintValueReason, Protocol, ScalarKind, SchemaBuilder,
    ValueParameter, ValueSpec,
};

/// `time_unit` reads `s`, `ms`, `us` and `ns` and refuses any other text;
/// `time_zone` keeps any non-empty text byte for byte and refuses the empty
/// one; both are value-parameter sorts; `for_vertex` sets both; and two
/// specs that differ only in their zone or unit are unequal.
#[test]
fn a_time_parameter_is_read_and_kept() {
    for (text, unit) in [
        ("s", TimeUnit::Seconds),
        ("ms", TimeUnit::Millis),
        ("us", TimeUnit::Micros),
        ("ns", TimeUnit::Nanos),
    ] {
        assert_eq!(
            ValueParameter::parse("time_unit", text),
            Ok(ValueParameter::TimeUnit(unit))
        );
    }
    for text in ["min", "", "S", "MS", " ms", "µs"] {
        let error = ValueParameter::parse("time_unit", text).unwrap_err();
        assert_eq!(
            error.reason,
            ConstraintValueReason::NotATimeUnit,
            "{text:?}"
        );
    }
    for zone in [
        "Asia/Kolkata",
        "+05:30",
        "UTC",
        "Z",
        "America/Argentina/Buenos_Aires",
    ] {
        assert_eq!(
            ValueParameter::parse("time_zone", zone),
            Ok(ValueParameter::TimeZone(zone.to_owned()))
        );
    }
    let empty = ValueParameter::parse("time_zone", "").unwrap_err();
    assert_eq!(empty.reason, ConstraintValueReason::Empty);
    assert!(ValueParameter::SORTS.contains(&"time_unit"));
    assert!(ValueParameter::SORTS.contains(&"time_zone"));

    let protocol = Protocol {
        name: "demo".into(),
        obj_kinds: vec!["timestamp".into()],
        constraint_sorts: vec!["time_unit".into(), "time_zone".into()],
        constraint_semantics: BTreeMap::from([
            ("time_unit".to_owned(), ConstraintSemantics::ValueParameter),
            ("time_zone".to_owned(), ConstraintSemantics::ValueParameter),
        ]),
        value_kinds: BTreeMap::from([("timestamp".to_owned(), ScalarKind::DateTime)]),
        ..Protocol::default()
    };
    let schema = SchemaBuilder::new(&protocol)
        .vertex("at", "timestamp", None)
        .unwrap()
        .vertex("other", "timestamp", None)
        .unwrap()
        .constraint("at", "time_unit", "ms")
        .constraint("at", "time_zone", "Asia/Kolkata")
        .constraint("other", "time_unit", "ms")
        .constraint("other", "time_zone", "+05:30")
        .build()
        .unwrap();
    let at = ValueSpec::for_vertex(&schema, &protocol, &Name::from("at"));
    let other = ValueSpec::for_vertex(&schema, &protocol, &Name::from("other"));
    assert_eq!(at.time_unit, Some(TimeUnit::Millis));
    assert_eq!(at.time_zone.as_deref(), Some("Asia/Kolkata"));
    assert_eq!(other.time_zone.as_deref(), Some("+05:30"));
    assert_ne!(at, other);
    let seconds = ValueSpec {
        time_unit: Some(TimeUnit::Seconds),
        ..at.clone()
    };
    assert_ne!(seconds, at);
    let text = serde_json::to_string(&at).unwrap();
    assert!(text.contains(r#""time_unit":"millis""#), "{text}");
    assert!(text.contains(r#""time_zone":"Asia/Kolkata""#), "{text}");
    assert_eq!(serde_json::from_str::<ValueSpec>(&text).unwrap(), at);
    // A spec without either parameter writes neither field.
    let plain = serde_json::to_string(&ValueSpec::new(ScalarKind::DateTime)).unwrap();
    assert!(!plain.contains("time_"), "{plain}");
}
