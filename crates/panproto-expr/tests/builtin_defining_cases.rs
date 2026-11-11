#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Each first-order builtin answers its defining cases exactly when called
//! directly through `apply_builtin`, the entry point that bypasses the
//! evaluator's own short-circuiting and higher-order handling.

use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;

use panproto_expr::{BuiltinOp, ExprError, Literal, apply_builtin};

fn s(text: &str) -> Literal {
    Literal::Str(text.into())
}

fn ints(values: &[i64]) -> Literal {
    Literal::List(values.iter().copied().map(Literal::Int).collect())
}

fn record(fields: &[(&str, Literal)]) -> Literal {
    Literal::Record(
        fields
            .iter()
            .map(|(name, value)| (Arc::from(*name), value.clone()))
            .collect(),
    )
}

fn call(op: BuiltinOp, args: &[Literal]) -> Literal {
    apply_builtin(op, args).unwrap_or_else(|err| panic!("{op:?}{args:?} failed: {err}"))
}

/// `op(args)` computed on another thread, failing the test when it does not
/// answer within the deadline rather than hanging the run.
fn call_within_deadline(op: BuiltinOp, args: Vec<Literal>) -> Literal {
    let (send, receive) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = send.send(apply_builtin(op, &args).map_err(|err| err.to_string()));
    });
    receive
        .recv_timeout(Duration::from_secs(10))
        .expect("the builtin did not terminate within ten seconds")
        .unwrap()
}

#[test]
fn boolean_connectives_compute_their_truth_tables() {
    for a in [false, true] {
        for b in [false, true] {
            let args = [Literal::Bool(a), Literal::Bool(b)];
            assert_eq!(
                call(BuiltinOp::And, &args),
                Literal::Bool(a && b),
                "{a} and {b}"
            );
            assert_eq!(
                call(BuiltinOp::Or, &args),
                Literal::Bool(a || b),
                "{a} or {b}"
            );
        }
        assert_eq!(call(BuiltinOp::Not, &[Literal::Bool(a)]), Literal::Bool(!a));
    }
}

#[test]
fn string_builtins_compute_their_values() {
    assert_eq!(call(BuiltinOp::Len, &[s("héllo")]), Literal::Int(6));
    assert_eq!(
        call(
            BuiltinOp::Slice,
            &[s("héllo"), Literal::Int(1), Literal::Int(3)]
        ),
        s("él")
    );
    assert_eq!(call(BuiltinOp::Upper, &[s("aBc")]), s("ABC"));
    assert_eq!(call(BuiltinOp::Lower, &[s("aBc")]), s("abc"));
    assert_eq!(call(BuiltinOp::Trim, &[s("  a b \t")]), s("a b"));
    assert_eq!(
        call(BuiltinOp::Replace, &[s("aXbX"), s("X"), s("-")]),
        s("a-b-")
    );
    assert_eq!(
        call(BuiltinOp::Contains, &[s("abc"), s("bc")]),
        Literal::Bool(true)
    );
    assert_eq!(
        call(BuiltinOp::Contains, &[s("abc"), s("cb")]),
        Literal::Bool(false)
    );
    assert_eq!(
        call(BuiltinOp::Contains, &[ints(&[1, 2]), Literal::Int(2)]),
        Literal::Bool(true)
    );
    assert_eq!(
        call(BuiltinOp::Contains, &[ints(&[1, 2]), Literal::Int(3)]),
        Literal::Bool(false)
    );
}

#[test]
fn higher_order_builtins_called_directly_are_refused_as_evaluator_work() {
    for (op, args) in [
        (BuiltinOp::Map, vec![ints(&[1]), Literal::Null]),
        (BuiltinOp::Filter, vec![ints(&[1]), Literal::Null]),
        (
            BuiltinOp::Fold,
            vec![ints(&[1]), Literal::Int(0), Literal::Null],
        ),
        (BuiltinOp::FlatMap, vec![ints(&[1]), Literal::Null]),
        (BuiltinOp::Range, vec![Literal::Int(0), Literal::Int(2)]),
    ] {
        match apply_builtin(op, &args) {
            Err(ExprError::TypeError { expected, got }) => {
                assert_eq!(expected, "handled in evaluator", "{op:?}");
                assert_eq!(got, "direct builtin call", "{op:?}");
            }
            other => panic!("{op:?} answered {other:?}"),
        }
    }
}

#[test]
fn list_builtins_compute_their_values() {
    assert_eq!(
        call(BuiltinOp::Append, &[ints(&[1, 2]), Literal::Int(3)]),
        ints(&[1, 2, 3])
    );
    assert_eq!(
        call(BuiltinOp::Reverse, &[ints(&[1, 2, 3])]),
        ints(&[3, 2, 1])
    );
    assert_eq!(
        call(BuiltinOp::Length, &[ints(&[4, 5, 6])]),
        Literal::Int(3)
    );
    assert_eq!(call(BuiltinOp::Tail, &[ints(&[7])]), ints(&[]));
    assert!(matches!(
        apply_builtin(BuiltinOp::Tail, &[ints(&[])]),
        Err(ExprError::IndexOutOfBounds { index: 0, len: 0 })
    ));
}

#[test]
fn has_field_answers_by_the_field_name() {
    let fields = record(&[("x", Literal::Int(1))]);
    assert_eq!(
        call(BuiltinOp::HasField, &[fields.clone(), s("x")]),
        Literal::Bool(true)
    );
    assert_eq!(
        call(BuiltinOp::HasField, &[fields, s("y")]),
        Literal::Bool(false)
    );
}

#[test]
fn coercion_inspection_and_default_builtins_compute_their_values() {
    assert_eq!(
        call(BuiltinOp::FloatToStr, &[Literal::Float(1.5)]),
        s("1.5")
    );
    assert_eq!(
        call(BuiltinOp::IsNull, &[Literal::Null]),
        Literal::Bool(true)
    );
    assert_eq!(
        call(BuiltinOp::IsNull, &[Literal::Int(0)]),
        Literal::Bool(false)
    );
    assert_eq!(call(BuiltinOp::IsList, &[ints(&[])]), Literal::Bool(true));
    assert_eq!(call(BuiltinOp::IsList, &[s("[]")]), Literal::Bool(false));
    assert_eq!(
        call(BuiltinOp::DefaultVal, &[Literal::Null, Literal::Int(3)]),
        Literal::Int(3)
    );
    assert_eq!(
        call(BuiltinOp::DefaultVal, &[Literal::Int(1), Literal::Int(3)]),
        Literal::Int(1)
    );
}

#[test]
fn truncate_str_keeps_at_most_the_byte_budget_on_a_char_boundary() {
    // "héllo": `h` is byte 0, `é` bytes 1..3, so byte 2 is not a boundary.
    for (text, budget, expected) in [
        ("hello", 10, "hello"),
        ("hello", 5, "hello"),
        ("hello", 3, "hel"),
        ("hello", 0, ""),
        ("hello", -4, ""),
        ("héllo", 2, "h"),
        ("héllo", 3, "hé"),
        ("héllo", 1, "h"),
        ("éa", 1, ""),
    ] {
        assert_eq!(
            call_within_deadline(BuiltinOp::TruncateStr, vec![s(text), Literal::Int(budget)]),
            s(expected),
            "truncate_str({text:?}, {budget})"
        );
    }
}

#[test]
fn equality_of_lists_and_records_is_structural() {
    let eq = |a: Literal, b: Literal| call(BuiltinOp::Eq, &[a, b]);
    // A list compares item by item under the record-as-map equality, so field
    // order inside an item does not count.
    let ab = record(&[("a", Literal::Int(1)), ("b", Literal::Int(2))]);
    let ba = record(&[("b", Literal::Int(2)), ("a", Literal::Int(1))]);
    assert_eq!(
        eq(Literal::List(vec![ab.clone()]), Literal::List(vec![ba])),
        Literal::Bool(true)
    );
    assert_eq!(eq(ints(&[1]), ints(&[1, 2])), Literal::Bool(false));
    assert_eq!(eq(ints(&[1, 2]), ints(&[1, 2])), Literal::Bool(true));
    // Records of different sizes whose shared prefix agrees differ.
    assert_eq!(
        eq(record(&[("a", Literal::Int(1))]), ab),
        Literal::Bool(false)
    );
    // Same names, different values.
    assert_eq!(
        eq(
            record(&[("a", Literal::Int(1))]),
            record(&[("a", Literal::Int(2))])
        ),
        Literal::Bool(false)
    );
    // Same values, different names.
    assert_eq!(
        eq(
            record(&[("a", Literal::Int(1))]),
            record(&[("b", Literal::Int(1))])
        ),
        Literal::Bool(false)
    );
}
