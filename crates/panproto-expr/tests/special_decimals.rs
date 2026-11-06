//! A decimal carries the General Decimal Arithmetic's special values: a
//! negative zero at any exponent, the two infinities, and NaNs with a sign,
//! a signalling flag and a payload.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::cmp::Ordering;

use panproto_expr::exact::DecimalValue;
use panproto_expr::{BuiltinOp, ExprError, Literal, apply_builtin};

fn dec(text: &str) -> DecimalValue {
    DecimalValue::parse(text).unwrap_or_else(|e| panic!("{text}: {e}"))
}

/// The special forms read as distinct values and display as written (or in
/// their one canonical spelling); `NaN` and `NaN0` are one value; the
/// numeric preorder places the special values where compare-total does,
/// every zero `Equal`; canonical bytes tell every value apart; and `to_f64`
/// maps infinities and NaNs to the floats of their signs.
#[test]
fn a_special_decimal_is_held_and_ordered() {
    let distinct = [
        ("NaN", "NaN"),
        ("-NaN", "-NaN"),
        ("sNaN", "sNaN"),
        ("NaN12", "NaN12"),
        ("Inf", "Infinity"),
        ("-Infinity", "-Infinity"),
        ("-0", "-0"),
        ("-0E+3", "-0e3"),
        ("-0.00", "-0.00"),
        ("0", "0"),
    ];
    let values: Vec<DecimalValue> = distinct.iter().map(|(text, _)| dec(text)).collect();
    for (i, a) in values.iter().enumerate() {
        assert_eq!(a.to_string(), distinct[i].1, "{}", distinct[i].0);
        assert_eq!(&dec(&a.to_string()), a, "{a} reads back");
        for (j, b) in values.iter().enumerate() {
            assert_eq!(i == j, a == b, "{a} against {b}");
            assert_eq!(
                i == j,
                a.canonical_bytes() == b.canonical_bytes(),
                "{a} and {b} bytes"
            );
        }
    }
    assert_eq!(dec("NaN"), dec("NaN0"));
    assert_eq!(dec("nan"), dec("NaN"));
    assert_eq!(dec("SNAN007"), dec("sNaN7"));
    for text in ["NaN.5", "Infinit", "sNaNa", "-", "+inf.0", ".nan"] {
        assert!(DecimalValue::parse(text).is_err(), "{text}");
    }

    // The numeric order of compare-total's example, zeros equal.
    let order = [
        "-NaN",
        "-sNaN",
        "-Infinity",
        "-1",
        "-0",
        "0",
        "1",
        "Infinity",
        "sNaN",
        "NaN",
    ];
    for (i, a) in order.iter().enumerate() {
        for (j, b) in order.iter().enumerate() {
            let expected = if (*a, *b) == ("-0", "0") || (*a, *b) == ("0", "-0") {
                Ordering::Equal
            } else {
                i.cmp(&j)
            };
            assert_eq!(dec(a).cmp_numeric(&dec(b)), expected, "{a} against {b}");
        }
    }
    assert_eq!(dec("-0").cmp_numeric(&dec("0E+3")), Ordering::Equal);
    assert_eq!(dec("NaN1").cmp_numeric(&dec("NaN2")), Ordering::Equal);

    // Conversion to f64.
    assert_eq!(dec("Infinity").to_f64().to_bits(), f64::INFINITY.to_bits());
    assert_eq!(dec("-Inf").to_f64().to_bits(), f64::NEG_INFINITY.to_bits());
    assert_eq!(dec("sNaN5").to_f64().to_bits(), 0x7ff8_0000_0000_0000);
    assert_eq!(dec("-NaN").to_f64().to_bits(), 0xfff8_0000_0000_0000);
    assert_eq!(dec("-0.00").to_f64().to_bits(), (-0.0_f64).to_bits());
}

/// A special decimal is not ordered against a number: comparing it with an
/// integer or a decimal is a typed error, while equality, `to_string` and
/// `parse_decimal` keep it, and `decimal_to_float` converts it lossily.
#[test]
fn a_special_decimal_is_refused_by_numeric_comparison() {
    let nan = Literal::Decimal(dec("NaN"));
    let one = Literal::Int(1);
    for op in [BuiltinOp::Lt, BuiltinOp::Lte, BuiltinOp::Gt, BuiltinOp::Gte] {
        assert!(
            matches!(
                apply_builtin(op, &[nan.clone(), one.clone()]),
                Err(ExprError::TypeError { .. })
            ),
            "{op:?}"
        );
        assert!(
            matches!(
                apply_builtin(op, &[Literal::Decimal(dec("-Infinity")), one.clone()]),
                Err(ExprError::TypeError { .. })
            ),
            "{op:?}"
        );
    }
    assert!(matches!(
        apply_builtin(BuiltinOp::ParseDecimal, std::slice::from_ref(&nan)),
        Ok(Literal::Decimal(d)) if d == dec("NaN")
    ));
    assert!(matches!(
        apply_builtin(BuiltinOp::ToString, std::slice::from_ref(&nan)),
        Ok(Literal::Str(s)) if s == "NaN"
    ));
    assert!(matches!(
        apply_builtin(BuiltinOp::DecimalToFloat, &[Literal::Decimal(dec("-Infinity"))]),
        Ok(Literal::Float(x)) if x == f64::NEG_INFINITY
    ));
    assert!(matches!(
        apply_builtin(BuiltinOp::Lt, &[Literal::Decimal(dec("-0")), one]),
        Ok(Literal::Bool(true))
    ));
}
