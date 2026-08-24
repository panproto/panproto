//! Binding a name costs the same whatever else is in scope.
//!
//! Every `let`, every lambda, and every closure application extends the
//! environment. Copying the whole environment to do so makes each of those
//! cost as much as the scope is wide, so evaluating a fixed expression under a
//! wide environment cost time proportional to bindings the expression never
//! reads.
//!
//! Sharing the environment also settles a question of identity: a closure
//! carries the scope it captured, so two closures over the same bindings are
//! the same closure however those bindings were assembled.

#![allow(
    clippy::expect_used,
    reason = "a fixture that will not build is a defect in this file"
)]

use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;
use std::time::{Duration, Instant};

use panproto_expr::{BuiltinOp, Env, EvalConfig, Expr, Literal, eval};

/// An environment of `width` bindings the expression under test never reads.
fn wide_env(width: usize) -> Env {
    (0..width)
        .map(|i| {
            (
                Arc::from(format!("unused{i}").as_str()),
                Literal::Int(i64::try_from(i).unwrap_or(i64::MAX)),
            )
        })
        .collect()
}

/// `let b0 = 0 in let b1 = b0 + 1 in ... in b{depth-1}`: a chain of bindings,
/// each of which extends the environment.
fn binding_chain(depth: usize) -> Expr {
    let mut body = Expr::Var(Arc::from(format!("b{}", depth - 1).as_str()));
    for level in (0..depth).rev() {
        let value = if level == 0 {
            Expr::Lit(Literal::Int(0))
        } else {
            Expr::Builtin(
                BuiltinOp::Add,
                vec![
                    Expr::Var(Arc::from(format!("b{}", level - 1).as_str())),
                    Expr::Lit(Literal::Int(1)),
                ],
            )
        };
        body = Expr::Let {
            name: Arc::from(format!("b{level}").as_str()),
            value: Box::new(value),
            body: Box::new(body),
        };
    }
    body
}

/// Median of five timings, so one scheduling hiccup does not decide the test.
fn time_eval(expr: &Expr, env: &Env, expected: i64) -> Duration {
    let config = EvalConfig::default();
    let mut samples: Vec<Duration> = (0..5)
        .map(|_| {
            let start = Instant::now();
            let value = eval(expr, env, &config).expect("the fixture evaluates");
            let elapsed = start.elapsed();
            assert_eq!(value, Literal::Int(expected));
            elapsed
        })
        .collect();
    samples.sort_unstable();
    samples[2]
}

#[test]
fn a_wider_scope_does_not_make_the_same_expression_cost_more() {
    let depth = 200;
    let expr = binding_chain(depth);
    let expected = i64::try_from(depth).unwrap_or(i64::MAX) - 1;

    let narrow = time_eval(&expr, &wide_env(200), expected);
    let wide = time_eval(&expr, &wide_env(3_200), expected);

    // Sharing the environment puts this at about 1. Copying it puts it at
    // about 16, the ratio of the two widths. Four leaves ample room for noise.
    let ratio = wide.as_secs_f64() / narrow.as_secs_f64().max(f64::EPSILON);
    assert!(
        ratio < 4.0,
        "200 unused bindings took {narrow:?}, 3200 took {wide:?} — a ratio of \
         {ratio:.1}, so binding a name is still paying for the whole scope"
    );
}

/// Evaluate `\z -> z` under `env`, yielding the closure it captures.
fn closure_over(env: &Env) -> Literal {
    let lambda = Expr::Lam(Arc::from("z"), Box::new(Expr::Var(Arc::from("z"))));
    eval(&lambda, env, &EvalConfig::default()).expect("a lambda evaluates to a closure")
}

fn hash_of(value: &Literal) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

#[test]
fn a_closure_is_the_same_however_its_scope_was_assembled() {
    let forward: Env = [
        (Arc::from("a"), Literal::Int(1)),
        (Arc::from("b"), Literal::Int(2)),
        (Arc::from("c"), Literal::Int(3)),
    ]
    .into_iter()
    .collect();
    let backward: Env = [
        (Arc::from("c"), Literal::Int(3)),
        (Arc::from("b"), Literal::Int(2)),
        (Arc::from("a"), Literal::Int(1)),
    ]
    .into_iter()
    .collect();

    let one = closure_over(&forward);
    let other = closure_over(&backward);
    assert_eq!(one, other, "the same bindings make the same closure");
    assert_eq!(
        hash_of(&one),
        hash_of(&other),
        "equal closures must hash alike"
    );
}

#[test]
fn a_shadowed_binding_is_not_part_of_the_scope_a_closure_captures() {
    let shadowed = Env::new()
        .extend(Arc::from("a"), Literal::Int(1))
        .extend(Arc::from("a"), Literal::Int(2));
    let direct = Env::new().extend(Arc::from("a"), Literal::Int(2));
    assert_eq!(closure_over(&shadowed), closure_over(&direct));
}

#[test]
fn extending_leaves_the_environment_it_extended_alone() {
    let base = Env::new().extend(Arc::from("x"), Literal::Int(1));
    let extended = base.extend(Arc::from("x"), Literal::Int(2));
    assert_eq!(base.get("x"), Some(&Literal::Int(1)));
    assert_eq!(extended.get("x"), Some(&Literal::Int(2)));
    assert_eq!(base.len(), 1);
    assert_eq!(extended.len(), 1, "shadowing does not widen the scope");
}
