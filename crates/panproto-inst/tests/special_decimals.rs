//! A special decimal is read and kept under `Decimal`, refused where a
//! number is needed, and not ordered against a numeric bound.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeMap;

use panproto_gat::CheckStatus;
use panproto_inst::value::DecimalValue;
use panproto_inst::value::coerce::{LexClass, Lexeme, coerce, coerce_value};
use panproto_inst::{
    FidelityReason, FieldPresence, GInstance, IncompleteReason, Node, Value, ValueSite,
    validate_graph,
};
use panproto_schema::{Constraint, ConstraintSemantics, Protocol, ScalarKind, Schema, ValueSpec, Vertex};

fn decimal(text: &str) -> Value {
    Value::Decimal(DecimalValue::parse(text).unwrap_or_else(|e| panic!("{text}: {e}")))
}

fn read(text: &str, kind: ScalarKind) -> Result<Value, FidelityReason> {
    coerce(&Lexeme::new(text, LexClass::Number), &ValueSpec::new(kind)).map_err(|e| e.reason)
}

/// A numeric bound check of a special decimal reports that the value cannot
/// be compared, as it does for a non-finite float, while a negative zero is
/// compared as zero; and under `Decimal` the special spellings and a
/// negative zero are read and kept.
#[test]
fn a_special_decimal_is_not_compared_with_a_bound() {
    let protocol = Protocol {
        name: "test".into(),
        constraint_sorts: vec!["maximum".into()],
        constraint_semantics: BTreeMap::from([(
            "maximum".to_owned(),
            ConstraintSemantics::Maximum,
        )]),
        ..Protocol::default()
    };
    let mut schema = Schema::default();
    schema.vertices.insert(
        "a".into(),
        Vertex {
            id: "a".into(),
            kind: "number".into(),
            nsid: None,
        },
    );
    schema.constraints.insert(
        "a".into(),
        vec![Constraint {
            sort: "maximum".into(),
            value: "10".into(),
        }],
    );
    let check = |value: Value| {
        validate_graph(
            &GInstance::new().with_node(Node::new(0, "a").with_value(FieldPresence::Present(value))),
            &schema,
            &protocol,
        )
    };
    for special in ["NaN", "-sNaN3", "Infinity", "-Infinity"] {
        let report = check(decimal(special));
        assert_eq!(report.status, CheckStatus::Incomplete, "{special}: {report:?}");
        assert!(
            report.incomplete.iter().all(|i| matches!(
                &i.reason,
                IncompleteReason::IllFormedDeclaration { .. }
            )) && !report.incomplete.is_empty(),
            "{special}: {report:?}"
        );
    }
    assert!(check(decimal("-0")).passed());
    assert!(!check(decimal("1e2")).passed());

    // Under `Decimal` the special spellings and negative zeros are values.
    assert_eq!(read("-0", ScalarKind::Decimal), Ok(decimal("-0")));
    assert_ne!(read("-0", ScalarKind::Decimal), read("0", ScalarKind::Decimal));
    assert_eq!(read("-0.00", ScalarKind::Decimal), Ok(decimal("-0.00")));
    for special in ["NaN", "-NaN", "sNaN", "NaN12", "Inf", "-Infinity"] {
        assert_eq!(read(special, ScalarKind::Decimal), Ok(decimal(special)), "{special}");
    }
    // Where a number of another kind is needed, a special decimal is
    // refused, and `Float(-0.0)` under `Decimal` is a negative zero.
    let site = ValueSite::unknown();
    let refuse = |value: &Value, kind: ScalarKind| {
        coerce_value(value, &ValueSpec::new(kind), site.clone()).map_err(|e| e.reason)
    };
    for special in ["NaN", "Infinity"] {
        for kind in [ScalarKind::Float { bits: 64 }, ScalarKind::BigInt] {
            assert_eq!(
                refuse(&decimal(special), kind.clone()),
                Err(FidelityReason::NonFinite),
                "{special} under {kind:?}"
            );
        }
    }
    assert_eq!(
        refuse(&Value::Float(-0.0), ScalarKind::Decimal),
        Ok(decimal("-0"))
    );
    assert!(matches!(
        refuse(&decimal("-0"), ScalarKind::Float { bits: 64 }),
        Ok(Value::Float(x)) if x.to_bits() == (-0.0_f64).to_bits()
    ));
}
