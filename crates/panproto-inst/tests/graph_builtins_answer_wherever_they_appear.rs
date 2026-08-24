//! The graph builtins are ordinary builtins that happen to need an instance.
//!
//! Instance-aware evaluation supplies the instance at every point a builtin is
//! applied, so `edge_count self > 0` — the shape any real predicate takes —
//! reads the same graph the root of the expression would, and so does a graph
//! builtin under a binding, a lambda, or a comprehension. Short of its
//! arguments a graph builtin is a function of the rest, like any other
//! builtin; called directly with the wrong number it reports arity rather than
//! reading an argument it was not given.

#![allow(
    clippy::expect_used,
    reason = "a fixture that will not build is a defect in this file"
)]

use std::collections::HashMap;

use panproto_expr::{BuiltinOp, Env, EvalConfig, Expr, ExprError, Literal};
use panproto_gat::Name;
use panproto_inst::value::Value;
use panproto_inst::{ElementOps, Node, WInstance, eval_with_instance};
use panproto_schema::Edge;

/// A two-node instance: `document` with one `body` child.
fn instance() -> WInstance {
    let mut nodes = HashMap::new();
    let mut root = Node::new(0, "document");
    root.extra_fields
        .insert("title".into(), Value::Str("Test".into()));
    nodes.insert(0, root);

    let mut child = Node::new(1, "paragraph");
    child
        .extra_fields
        .insert("text".into(), Value::Str("Hello".into()));
    nodes.insert(1, child);

    let edge = Edge {
        src: Name::from("document"),
        tgt: Name::from("paragraph"),
        kind: Name::from("body"),
        name: None,
    };

    WInstance::new(nodes, vec![(0, 1, edge)], vec![], 0, Name::from("document"))
}

fn eval(expr: &Expr) -> Result<Literal, ExprError> {
    eval_with_instance(
        expr,
        &Env::new(),
        &EvalConfig::default(),
        &instance(),
        Some(0),
    )
}

fn edge_count(node: i64) -> Expr {
    Expr::Builtin(BuiltinOp::EdgeCount, vec![Expr::Lit(Literal::Int(node))])
}

#[test]
fn a_graph_builtin_under_a_comparison_still_sees_the_instance() {
    let expr = Expr::Builtin(
        BuiltinOp::Gt,
        vec![edge_count(0), Expr::Lit(Literal::Int(0))],
    );
    assert_eq!(
        eval(&expr).expect("the comparison evaluates"),
        Literal::Bool(true),
        "the root has one arc, so its edge count is above zero"
    );
}

#[test]
fn a_graph_builtin_under_a_let_still_sees_the_instance() {
    let expr = Expr::Let {
        name: "n".into(),
        value: Box::new(edge_count(0)),
        body: Box::new(Expr::Builtin(
            BuiltinOp::Add,
            vec![Expr::Var("n".into()), Expr::Lit(Literal::Int(10))],
        )),
    };
    assert_eq!(
        eval(&expr).expect("the binding evaluates"),
        Literal::Int(11)
    );
}

#[test]
fn a_graph_builtin_inside_a_lambda_still_sees_the_instance() {
    // (\x -> anchor x) 1
    let lambda = Expr::Lam(
        "x".into(),
        Box::new(Expr::Builtin(
            BuiltinOp::Anchor,
            vec![Expr::Var("x".into())],
        )),
    );
    let expr = Expr::App(Box::new(lambda), Box::new(Expr::Lit(Literal::Int(1))));
    assert_eq!(
        eval(&expr).expect("the application evaluates"),
        Literal::Str("paragraph".into())
    );
}

#[test]
fn a_graph_builtin_inside_a_map_still_sees_the_instance() {
    // map([0, 1], \x -> anchor x)
    let lambda = Expr::Lam(
        "x".into(),
        Box::new(Expr::Builtin(
            BuiltinOp::Anchor,
            vec![Expr::Var("x".into())],
        )),
    );
    let expr = Expr::Builtin(
        BuiltinOp::Map,
        vec![
            Expr::List(vec![Expr::Lit(Literal::Int(0)), Expr::Lit(Literal::Int(1))]),
            lambda,
        ],
    );
    assert_eq!(
        eval(&expr).expect("the map evaluates"),
        Literal::List(vec![
            Literal::Str("document".into()),
            Literal::Str("paragraph".into()),
        ])
    );
}

#[test]
fn a_graph_builtin_given_too_few_arguments_is_a_function_of_the_rest() {
    // `edge 1` names the node but not the edge kind, so it denotes a function
    // of the kind. Supplying it completes the call against the instance.
    let partial = Expr::Builtin(BuiltinOp::Edge, vec![Expr::Lit(Literal::Int(0))]);
    assert!(
        matches!(
            eval(&partial).expect("the partial application evaluates"),
            Literal::Closure { .. }
        ),
        "one argument short of `edge` is a function of the missing one"
    );

    let applied = Expr::App(
        Box::new(partial),
        Box::new(Expr::Lit(Literal::Str("body".into()))),
    );
    assert!(
        matches!(
            eval(&applied).expect("the completed call evaluates"),
            Literal::Record(_)
        ),
        "completing the call reaches the child across the body arc"
    );
}

#[test]
fn calling_a_graph_builtin_short_of_its_arguments_reports_arity() {
    // The handler is reachable directly, without the evaluator's currying in
    // front of it, so it checks how many arguments it was given before it
    // reads any of them.
    let err = instance()
        .eval_graph_builtin(BuiltinOp::Edge, &[Literal::Int(1)], Some(0))
        .expect_err("one argument is not enough for edge");
    assert!(
        matches!(
            err,
            ExprError::ArityMismatch {
                expected: 2,
                got: 1,
                ..
            }
        ),
        "expected an arity mismatch, got {err:?}"
    );
}

#[test]
fn a_graph_builtin_given_too_many_arguments_reports_arity() {
    let expr = Expr::Builtin(
        BuiltinOp::Anchor,
        vec![Expr::Lit(Literal::Int(1)), Expr::Lit(Literal::Int(1))],
    );
    let err = eval(&expr).expect_err("two arguments are too many for anchor");
    assert!(
        matches!(
            err,
            ExprError::ArityMismatch {
                expected: 1,
                got: 2,
                ..
            }
        ),
        "expected an arity mismatch, got {err:?}"
    );
}

#[test]
fn the_pure_evaluator_refuses_a_graph_builtin() {
    // Without an instance there is no graph to traverse, and answering `null`
    // makes a wrong result indistinguishable from a missing edge.
    let err = panproto_expr::eval(&edge_count(0), &Env::new(), &EvalConfig::default())
        .expect_err("the pure evaluator has no instance to consult");
    assert!(
        matches!(err, ExprError::NoInstanceContext { .. }),
        "expected a missing-instance report, got {err:?}"
    );
}
