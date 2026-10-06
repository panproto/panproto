#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Every NaN that expression evaluation or field-transform evaluation
//! returns is the canonical quiet NaN, bits `0x7ff8000000000000`, so two
//! computed NaN cells are equal value for value and digest for digest.

use std::sync::Arc;

use panproto_expr::{BuiltinOp, Env, EvalConfig, Expr, Literal, eval};
use panproto_inst::wtype::apply_field_transforms;
use panproto_inst::{FieldTransform, Node, TransformContext, Value};

const CANONICAL: u64 = 0x7ff8_0000_0000_0000;

/// A binary operation on floats, as IEEE 754 arithmetic computes it.
type FloatOp = fn(f64, f64) -> f64;

const fn float(x: f64) -> Expr {
    Expr::Lit(Literal::Float(x))
}

const fn call(op: BuiltinOp, args: Vec<Expr>) -> Expr {
    Expr::Builtin(op, args)
}

/// The bits of the float an expression evaluates to.
fn evaluated_bits(expr: &Expr, env: &Env) -> u64 {
    match eval(expr, env, &EvalConfig::default()).unwrap() {
        Literal::Float(x) => x.to_bits(),
        other => panic!("expected a float, got {other:?}"),
    }
}

/// A NaN with a sign and a payload no arithmetic needs to produce.
const fn odd_nan() -> f64 {
    f64::from_bits(0xfff8_0000_0000_1234)
}

#[test]
fn every_computed_nan_is_the_canonical_nan() {
    let env = Env::new().extend(Arc::from("x"), Literal::Float(odd_nan()));
    let x = Expr::Var(Arc::from("x"));
    let cases = [
        (
            "inf - inf",
            call(
                BuiltinOp::Sub,
                vec![float(f64::INFINITY), float(f64::INFINITY)],
            ),
        ),
        (
            "0 * inf",
            call(BuiltinOp::Mul, vec![float(0.0), float(f64::INFINITY)]),
        ),
        (
            "inf + -inf",
            call(
                BuiltinOp::Add,
                vec![float(f64::INFINITY), float(f64::NEG_INFINITY)],
            ),
        ),
        (
            "inf / inf",
            call(
                BuiltinOp::Div,
                vec![float(f64::INFINITY), float(f64::INFINITY)],
            ),
        ),
        ("neg nan", call(BuiltinOp::Neg, vec![float(odd_nan())])),
        ("neg x", call(BuiltinOp::Neg, vec![x.clone()])),
        ("abs x", call(BuiltinOp::Abs, vec![x.clone()])),
        ("x + 1", call(BuiltinOp::Add, vec![x.clone(), float(1.0)])),
        (
            "clamp x",
            call(BuiltinOp::Clamp, vec![x, float(0.0), float(1.0)]),
        ),
        (
            "str_to_float -nan",
            call(
                BuiltinOp::StrToFloat,
                vec![Expr::Lit(Literal::Str("-nan".into()))],
            ),
        ),
        (
            "str_to_float NaN",
            call(
                BuiltinOp::StrToFloat,
                vec![Expr::Lit(Literal::Str("NaN".into()))],
            ),
        ),
    ];
    for (name, expr) in &cases {
        assert_eq!(evaluated_bits(expr, &env), CANONICAL, "{name}");
    }
    // Non-NaN floats are untouched, a negative zero included.
    let negative_zero = call(BuiltinOp::Neg, vec![float(0.0)]);
    assert_eq!(evaluated_bits(&negative_zero, &env), (-0.0_f64).to_bits());
}

/// A node holding `x` in field `a`, after `transforms`.
fn transformed(x: f64, transforms: &[FieldTransform]) -> Node {
    let mut node = Node::new(0, "row");
    node.extra_fields.insert("a".into(), Value::Float(x));
    apply_field_transforms(&mut node, transforms, &TransformContext::detached()).unwrap();
    node
}

fn cell_bits(node: &Node, key: &str) -> u64 {
    match node.extra_fields.get(key) {
        Some(Value::Float(x)) => x.to_bits(),
        other => panic!("expected a float in {key}, got {other:?}"),
    }
}

#[test]
fn a_field_transform_writes_the_canonical_nan() {
    let produce = FieldTransform::ApplyExpr {
        key: "a".into(),
        expr: call(
            BuiltinOp::Mul,
            vec![Expr::Var(Arc::from("a")), float(f64::INFINITY)],
        ),
        inverse: None,
        coercion_class: panproto_gat::CoercionClass::Opaque,
    };
    let node = transformed(0.0, std::slice::from_ref(&produce));
    assert_eq!(cell_bits(&node, "a"), CANONICAL);
    // A transform that returns the NaN it read writes the canonical NaN.
    let identity = FieldTransform::ApplyExpr {
        key: "a".into(),
        expr: Expr::Var(Arc::from("a")),
        inverse: None,
        coercion_class: panproto_gat::CoercionClass::Iso,
    };
    let node = transformed(odd_nan(), std::slice::from_ref(&identity));
    assert_eq!(cell_bits(&node, "a"), CANONICAL);
    // A computed list's NaNs are canonical too.
    let listed = FieldTransform::ComputeField {
        target_key: "l".into(),
        expr: Expr::List(vec![Expr::Var(Arc::from("a"))]),
        inverse: None,
        coercion_class: panproto_gat::CoercionClass::Opaque,
    };
    let node = transformed(odd_nan(), std::slice::from_ref(&listed));
    let Some(Value::List(items)) = node.extra_fields.get("l") else {
        panic!("a list")
    };
    assert!(matches!(items.as_slice(), [Value::Float(x)] if x.to_bits() == CANONICAL));
    // A cell no transform evaluates keeps its bits.
    let node = transformed(odd_nan(), &[]);
    assert_eq!(cell_bits(&node, "a"), odd_nan().to_bits());
}

#[test]
fn two_computed_nan_cells_are_equal_values_with_equal_bytes() {
    let transforms = |x: Expr| {
        vec![FieldTransform::ComputeField {
            target_key: "n".into(),
            expr: x,
            inverse: None,
            coercion_class: panproto_gat::CoercionClass::Opaque,
        }]
    };
    let first = transformed(
        0.0,
        &transforms(call(
            BuiltinOp::Sub,
            vec![float(f64::INFINITY), float(f64::INFINITY)],
        )),
    );
    let second = transformed(
        0.0,
        &transforms(call(
            BuiltinOp::Mul,
            vec![float(0.0), float(f64::NEG_INFINITY)],
        )),
    );
    let (a, b) = (&first.extra_fields["n"], &second.extra_fields["n"]);
    assert_eq!(a.canonical_bytes(), b.canonical_bytes());
    assert!(a.canonical_cmp(b).is_eq());
}

/// Every float-valued arithmetic builtin over every pair of operands from a
/// bounded scope of special values (signed zeros, ones, infinities, the
/// extremes, and NaNs of both signs with several payloads), against IEEE
/// arithmetic computed here: a NaN result is the canonical NaN, and every
/// other result is the IEEE result bit for bit.
#[test]
fn float_builtins_agree_with_ieee_arithmetic_up_to_nan_canonicalization() {
    let nans = [
        0x7ff8_0000_0000_0000_u64,
        0xfff8_0000_0000_0000,
        0x7ff8_0000_0000_1234,
        0xfff0_0000_0000_0001,
        0x7ff0_0000_0000_0001,
        0x7fff_ffff_ffff_ffff,
    ]
    .map(f64::from_bits);
    let mut operands = vec![
        0.0,
        -0.0,
        1.0,
        -1.0,
        0.5,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::MAX,
        f64::MIN_POSITIVE,
        f64::from_bits(1),
    ];
    operands.extend(nans);
    let env = Env::new();
    let expect = |x: f64| if x.is_nan() { CANONICAL } else { x.to_bits() };
    let binary: [(BuiltinOp, FloatOp); 4] = [
        (BuiltinOp::Add, |a, b| a + b),
        (BuiltinOp::Sub, |a, b| a - b),
        (BuiltinOp::Mul, |a, b| a * b),
        (BuiltinOp::Div, |a, b| a / b),
    ];
    let mut checked = 0_u32;
    for &a in &operands {
        for (op, ieee) in binary {
            for &b in &operands {
                if op == BuiltinOp::Div && b == 0.0 {
                    continue;
                }
                let got = evaluated_bits(&call(op, vec![float(a), float(b)]), &env);
                assert_eq!(got, expect(ieee(a, b)), "{op:?} {a} {b}");
                checked += 1;
            }
        }
        assert_eq!(
            evaluated_bits(&call(BuiltinOp::Neg, vec![float(a)]), &env),
            expect(-a)
        );
        assert_eq!(
            evaluated_bits(&call(BuiltinOp::Abs, vec![float(a)]), &env),
            expect(a.abs())
        );
        let clamp = call(BuiltinOp::Clamp, vec![float(a), float(-1.0), float(1.0)]);
        assert_eq!(evaluated_bits(&clamp, &env), expect(a.clamp(-1.0, 1.0)));
        checked += 3;
    }
    // Sixteen operands: 16 * 16 * 4 pairs less the 16 * 2 zero divisors,
    // and three unary checks each.
    assert_eq!(checked, 16 * 16 * 4 - 32 + 16 * 3);
    // An integer operand is widened to a float first, on either side.
    let int = |n: i64| Expr::Lit(Literal::Int(n));
    for &nan in &nans {
        for (op, _) in binary {
            for expr in [
                call(op, vec![int(3), float(nan)]),
                call(op, vec![float(nan), int(3)]),
            ] {
                assert_eq!(evaluated_bits(&expr, &env), CANONICAL, "{op:?}");
            }
        }
    }
    // Every NaN spelling `StrToFloat` reads is canonical.
    for text in ["NaN", "nan", "-nan", "+nan", "-NaN", "+NaN"] {
        let parsed = call(
            BuiltinOp::StrToFloat,
            vec![Expr::Lit(Literal::Str(text.into()))],
        );
        assert_eq!(evaluated_bits(&parsed, &env), CANONICAL, "{text}");
    }
}

/// `Clamp` refuses bounds out of order, and bounds that are NaN, with a
/// typed error rather than computing anything.
#[test]
fn clamp_refuses_bounds_that_are_not_ordered() {
    let env = Env::new();
    for (lo, hi) in [(1.0, 0.0), (odd_nan(), 1.0), (0.0, odd_nan())] {
        let expr = call(BuiltinOp::Clamp, vec![float(0.5), float(lo), float(hi)]);
        let error = eval(&expr, &env, &EvalConfig::default()).unwrap_err();
        assert!(
            matches!(error, panproto_expr::ExprError::TypeError { .. }),
            "{lo} {hi}: {error:?}"
        );
    }
    let ordered = call(BuiltinOp::Clamp, vec![float(2.0), float(0.0), float(1.0)]);
    assert_eq!(evaluated_bits(&ordered, &env), 1.0_f64.to_bits());
}

/// The value `eval` returns has the canonical NaN wherever it holds a NaN,
/// even one evaluation only passed along: a bound variable, a literal, an
/// element `Head` takes from a list, a list literal's element, a record's
/// field.
#[test]
fn evaluation_returns_the_canonical_nan_at_every_depth() {
    let payload = Literal::Float(f64::from_bits(0x7ff8_0000_0000_0001));
    let negative = f64::from_bits(0xfff8_0000_0000_0000);
    let env = Env::new().extend(Arc::from("p"), payload);
    let p = Expr::Var(Arc::from("p"));
    for (name, expr) in [
        ("variable", p.clone()),
        ("literal", float(negative)),
        (
            "head",
            call(
                BuiltinOp::Head,
                vec![Expr::List(vec![p.clone(), float(1.0)])],
            ),
        ),
    ] {
        assert_eq!(evaluated_bits(&expr, &env), CANONICAL, "{name}");
    }
    let listed = eval(
        &Expr::List(vec![float(negative), p.clone(), float(2.5)]),
        &env,
        &EvalConfig::default(),
    )
    .unwrap();
    let Literal::List(items) = listed else {
        panic!("a list")
    };
    let bits: Vec<u64> = items
        .iter()
        .map(|item| match item {
            Literal::Float(x) => x.to_bits(),
            other => panic!("a float, got {other:?}"),
        })
        .collect();
    assert_eq!(bits, vec![CANONICAL, CANONICAL, 2.5_f64.to_bits()]);
    let record = eval(
        &Expr::Record(vec![(Arc::from("a"), Expr::List(vec![p]))]),
        &env,
        &EvalConfig::default(),
    )
    .unwrap();
    let Literal::Record(fields) = record else {
        panic!("a record")
    };
    assert!(matches!(
        fields.as_slice(),
        [(_, Literal::List(items))] if matches!(items.as_slice(), [Literal::Float(x)] if x.to_bits() == CANONICAL)
    ));
}

/// The canonical NaN is one constant, defined by its bits and re-exported
/// by the instance crate.
#[test]
fn the_canonical_nan_constant_has_the_canonical_bits() {
    assert_eq!(panproto_expr::CANONICAL_NAN.to_bits(), CANONICAL);
    assert_eq!(panproto_inst::value::CANONICAL_NAN.to_bits(), CANONICAL);
}
