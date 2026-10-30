#![allow(clippy::unwrap_used, clippy::expect_used)]
//! The evaluator's depth and list-length accounting, pinned exactly.
//!
//! Every evaluation of a subexpression runs one level below the expression
//! that evaluates it, and an evaluation at a depth beyond `max_depth` is
//! refused. So the least `max_depth` that admits an expression is the
//! deepest level its evaluation reaches. Each test below builds an
//! expression whose deepest evaluation passes through one construct and
//! pins that level: a construct that charged no level, or charged it
//! differently, would move it.

use std::sync::Arc;

use panproto_expr::{
    BuiltinOp, BuiltinResolver, Env, EvalConfig, Expr, ExprError, Literal, Pattern, eval,
    eval_with_resolver,
};

fn name(text: &str) -> Arc<str> {
    Arc::from(text)
}

fn int(n: i64) -> Expr {
    Expr::Lit(Literal::Int(n))
}

fn var(text: &str) -> Expr {
    Expr::Var(name(text))
}

fn lam(param: &str, body: Expr) -> Expr {
    Expr::Lam(name(param), Box::new(body))
}

/// `let x = 0 in ... in body`, `k` bindings deep: `body` is evaluated `k`
/// levels below the outermost `let`, and nothing in the chain is deeper.
fn let_chain(k: usize, body: Expr) -> Expr {
    (0..k).fold(body, |body, _| Expr::Let {
        name: name("x"),
        value: Box::new(int(0)),
        body: Box::new(body),
    })
}

/// A payload whose evaluation reaches five levels below its own.
fn payload() -> Expr {
    let_chain(5, int(1))
}

fn config(max_depth: u32, max_list_len: usize) -> EvalConfig {
    EvalConfig {
        max_steps: 1_000_000,
        max_depth,
        max_list_len,
    }
}

/// A resolver that answers `Children` with its argument.
struct EchoChildren;

impl BuiltinResolver for EchoChildren {
    fn handles(&self, op: BuiltinOp) -> bool {
        op == BuiltinOp::Children
    }

    fn apply(&self, op: BuiltinOp, args: &[Literal]) -> Result<Literal, ExprError> {
        assert_eq!(op, BuiltinOp::Children);
        Ok(args[0].clone())
    }
}

fn run(expr: &Expr, config: &EvalConfig) -> Result<Literal, ExprError> {
    eval_with_resolver(expr, &Env::new(), config, &EchoChildren)
}

/// The expression evaluates to `value` with `max_depth = depth` and is
/// refused for depth with `max_depth = depth - 1`: its deepest evaluation is
/// exactly at `depth`.
fn assert_deepest(label: &str, expr: &Expr, depth: u32, value: &Literal) {
    match run(expr, &config(depth, 10_000)) {
        Ok(got) => assert_eq!(&got, value, "{label}: value"),
        Err(err) => panic!("{label}: refused at max_depth {depth}: {err}"),
    }
    match run(expr, &config(depth - 1, 10_000)) {
        Err(ExprError::DepthExceeded(limit)) => assert_eq!(limit, depth - 1, "{label}"),
        other => panic!("{label}: max_depth {} answered {other:?}", depth - 1),
    }
}

fn singleton(n: i64) -> Literal {
    Literal::List(vec![Literal::Int(n)])
}

#[test]
fn a_let_chain_reaches_one_level_per_binding() {
    assert_deepest("let body", &payload(), 5, &Literal::Int(1));
    assert_deepest(
        "let value",
        &Expr::Let {
            name: name("y"),
            value: Box::new(payload()),
            body: Box::new(var("y")),
        },
        6,
        &Literal::Int(1),
    );
}

#[test]
fn records_lists_and_fields_evaluate_their_parts_one_level_down() {
    let fields = Expr::Record(vec![(name("a"), payload())]);
    assert_deepest(
        "record field",
        &fields,
        6,
        &Literal::Record(vec![(name("a"), Literal::Int(1))]),
    );
    assert_deepest("list item", &Expr::List(vec![payload()]), 6, &singleton(1));
    assert_deepest(
        "field access",
        &Expr::Field(Box::new(fields), name("a")),
        7,
        &Literal::Int(1),
    );
}

#[test]
fn builtin_operands_are_evaluated_one_level_down() {
    assert_deepest(
        "first-order operand",
        &Expr::Builtin(
            BuiltinOp::Not,
            vec![let_chain(5, Expr::Lit(Literal::Bool(true)))],
        ),
        6,
        &Literal::Bool(false),
    );
    assert_deepest(
        "resolver operand",
        &Expr::Builtin(BuiltinOp::Children, vec![payload()]),
        6,
        &Literal::Int(1),
    );
    assert_deepest(
        "range bound",
        &Expr::Builtin(BuiltinOp::Range, vec![payload(), int(2)]),
        6,
        &Literal::List(vec![Literal::Int(1), Literal::Int(2)]),
    );
    // A partial application evaluates the operands it has and returns the
    // function of the rest.
    let partial = Expr::Builtin(BuiltinOp::Add, vec![payload()]);
    assert!(matches!(
        run(&partial, &config(6, 10_000)),
        Ok(Literal::Closure { .. })
    ));
    assert!(matches!(
        run(&partial, &config(5, 10_000)),
        Err(ExprError::DepthExceeded(5))
    ));
}

#[test]
fn a_resolver_answers_the_builtins_it_handles() {
    let expr = Expr::Builtin(BuiltinOp::Children, vec![int(7)]);
    assert_eq!(run(&expr, &EvalConfig::default()).unwrap(), Literal::Int(7));
    // Without a resolver the same builtin has no instance to read.
    assert!(matches!(
        eval(&expr, &Env::new(), &EvalConfig::default()),
        Err(ExprError::NoInstanceContext { .. })
    ));
}

#[test]
fn application_evaluates_function_argument_and_body_one_level_down() {
    assert_deepest(
        "applied function",
        &Expr::App(Box::new(let_chain(5, lam("v", var("v")))), Box::new(int(3))),
        6,
        &Literal::Int(3),
    );
    assert_deepest(
        "argument",
        &Expr::App(Box::new(lam("v", var("v"))), Box::new(payload())),
        6,
        &Literal::Int(1),
    );
    assert_deepest(
        "closure body",
        &Expr::App(Box::new(lam("v", payload())), Box::new(int(3))),
        6,
        &Literal::Int(1),
    );
}

#[test]
fn index_and_match_evaluate_their_parts_one_level_down() {
    assert_deepest(
        "indexed list",
        &Expr::Index(
            Box::new(let_chain(5, Expr::List(vec![int(4)]))),
            Box::new(int(0)),
        ),
        7,
        &Literal::Int(4),
    );
    assert_deepest(
        "index",
        &Expr::Index(
            Box::new(Expr::List(vec![int(4)])),
            Box::new(let_chain(5, int(0))),
        ),
        6,
        &Literal::Int(4),
    );
    assert_deepest(
        "scrutinee",
        &Expr::Match {
            scrutinee: Box::new(payload()),
            arms: vec![(Pattern::Wildcard, int(9))],
        },
        6,
        &Literal::Int(9),
    );
    assert_deepest(
        "arm body",
        &Expr::Match {
            scrutinee: Box::new(int(0)),
            arms: vec![(Pattern::Wildcard, payload())],
        },
        6,
        &Literal::Int(1),
    );
}

#[test]
fn higher_order_builtins_charge_their_list_and_their_function() {
    let deep_list = || let_chain(5, Expr::List(vec![int(1)]));
    let shallow_list = || Expr::List(vec![int(1)]);
    let identity = || lam("v", var("v"));
    let keep = || lam("v", Expr::Lit(Literal::Bool(true)));
    let wrap = || lam("v", Expr::List(vec![var("v")]));
    let first = || lam("acc", lam("item", var("acc")));

    for (label, expr, depth, value) in [
        (
            "map list",
            Expr::Builtin(BuiltinOp::Map, vec![deep_list(), identity()]),
            7,
            singleton(1),
        ),
        (
            "map function",
            Expr::Builtin(
                BuiltinOp::Map,
                vec![shallow_list(), let_chain(5, identity())],
            ),
            7,
            singleton(1),
        ),
        (
            "filter list",
            Expr::Builtin(BuiltinOp::Filter, vec![deep_list(), keep()]),
            7,
            singleton(1),
        ),
        (
            "filter predicate",
            Expr::Builtin(
                BuiltinOp::Filter,
                vec![shallow_list(), let_chain(5, keep())],
            ),
            7,
            singleton(1),
        ),
        (
            "fold list",
            Expr::Builtin(BuiltinOp::Fold, vec![deep_list(), int(0), first()]),
            7,
            Literal::Int(0),
        ),
        (
            "fold initial value",
            Expr::Builtin(BuiltinOp::Fold, vec![shallow_list(), payload(), first()]),
            6,
            Literal::Int(1),
        ),
        (
            "fold function",
            Expr::Builtin(
                BuiltinOp::Fold,
                vec![shallow_list(), int(0), let_chain(5, first())],
            ),
            7,
            Literal::Int(0),
        ),
        (
            "flat_map list",
            Expr::Builtin(BuiltinOp::FlatMap, vec![deep_list(), wrap()]),
            7,
            singleton(1),
        ),
        (
            "flat_map function",
            Expr::Builtin(
                BuiltinOp::FlatMap,
                vec![shallow_list(), let_chain(5, wrap())],
            ),
            7,
            singleton(1),
        ),
    ] {
        assert_deepest(label, &expr, depth, &value);
    }
}

fn assert_too_long(label: &str, expr: &Expr, max_list_len: usize, len: usize) {
    match run(expr, &config(256, max_list_len)) {
        Err(ExprError::ListLengthExceeded(got)) => assert_eq!(got, len, "{label}"),
        other => panic!("{label}: max_list_len {max_list_len} answered {other:?}"),
    }
}

fn assert_fits(label: &str, expr: &Expr, max_list_len: usize, value: &Literal) {
    assert_eq!(
        &run(expr, &config(256, max_list_len)).unwrap(),
        value,
        "{label}"
    );
}

#[test]
fn a_list_of_exactly_the_budget_is_admitted_and_one_longer_is_not() {
    let three = Literal::List(vec![Literal::Int(1), Literal::Int(2), Literal::Int(3)]);

    let literal_list = Expr::List(vec![int(1), int(2), int(3)]);
    assert_fits("list", &literal_list, 3, &three);
    assert_too_long("list", &literal_list, 2, 3);

    // The source list is a literal, so only the result is checked.
    let mapped = Expr::Builtin(
        BuiltinOp::Map,
        vec![Expr::Lit(three.clone()), lam("v", var("v"))],
    );
    assert_fits("map", &mapped, 3, &three);
    assert_too_long("map", &mapped, 2, 3);

    let flat = Expr::Builtin(
        BuiltinOp::FlatMap,
        vec![Expr::Lit(singleton(0)), lam("v", Expr::Lit(three.clone()))],
    );
    assert_fits("flat_map", &flat, 3, &three);
    assert_too_long("flat_map", &flat, 2, 3);

    // [1..3] has length stop - start + 1 = 3.
    let range = Expr::Builtin(BuiltinOp::Range, vec![int(1), int(3)]);
    assert_fits("range", &range, 3, &three);
    assert_too_long("range", &range, 2, 3);
    let from_zero = Expr::Builtin(BuiltinOp::Range, vec![int(0), int(2)]);
    assert_fits(
        "range from zero",
        &from_zero,
        3,
        &Literal::List(vec![Literal::Int(0), Literal::Int(1), Literal::Int(2)]),
    );
    assert_too_long("range from zero", &from_zero, 2, 3);
}
