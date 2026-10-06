#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Every NaN that expression evaluation or field-transform evaluation
//! returns is the canonical quiet NaN, bits `0x7ff8000000000000`, so two
//! computed NaN cells are equal value for value and digest for digest.

use std::sync::Arc;

use panproto_expr::{BuiltinOp, Env, EvalConfig, Expr, Literal, eval};
use panproto_inst::wtype::apply_field_transforms;
use panproto_inst::{FieldTransform, Node, TransformContext, Value};

const CANONICAL: u64 = 0x7ff8_0000_0000_0000;

fn float(x: f64) -> Expr {
    Expr::Lit(Literal::Float(x))
}

fn call(op: BuiltinOp, args: Vec<Expr>) -> Expr {
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
fn odd_nan() -> f64 {
    f64::from_bits(0xfff8_0000_0000_1234)
}

#[test]
fn every_computed_nan_is_the_canonical_nan() {
    let env = Env::new().extend(Arc::from("x"), Literal::Float(odd_nan()));
    let x = Expr::Var(Arc::from("x"));
    let cases = [
        ("inf - inf", call(BuiltinOp::Sub, vec![float(f64::INFINITY), float(f64::INFINITY)])),
        ("0 * inf", call(BuiltinOp::Mul, vec![float(0.0), float(f64::INFINITY)])),
        ("inf + -inf", call(BuiltinOp::Add, vec![float(f64::INFINITY), float(f64::NEG_INFINITY)])),
        ("inf / inf", call(BuiltinOp::Div, vec![float(f64::INFINITY), float(f64::INFINITY)])),
        ("neg nan", call(BuiltinOp::Neg, vec![float(f64::NAN)])),
        ("neg x", call(BuiltinOp::Neg, vec![x.clone()])),
        ("abs x", call(BuiltinOp::Abs, vec![x.clone()])),
        ("x + 1", call(BuiltinOp::Add, vec![x.clone(), float(1.0)])),
        ("clamp x", call(BuiltinOp::Clamp, vec![x, float(0.0), float(1.0)])),
        (
            "str_to_float -nan",
            call(BuiltinOp::StrToFloat, vec![Expr::Lit(Literal::Str("-nan".into()))]),
        ),
        (
            "str_to_float NaN",
            call(BuiltinOp::StrToFloat, vec![Expr::Lit(Literal::Str("NaN".into()))]),
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
        expr: call(BuiltinOp::Mul, vec![Expr::Var(Arc::from("a")), float(f64::INFINITY)]),
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
        &transforms(call(BuiltinOp::Sub, vec![float(f64::INFINITY), float(f64::INFINITY)])),
    );
    let second = transformed(
        0.0,
        &transforms(call(BuiltinOp::Mul, vec![float(0.0), float(f64::NEG_INFINITY)])),
    );
    let (a, b) = (&first.extra_fields["n"], &second.extra_fields["n"]);
    assert_eq!(a.canonical_bytes(), b.canonical_bytes());
    assert!(a.canonical_cmp(b).is_eq());
}
