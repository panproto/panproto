//! A deeply nested term is refused, not fatal.
//!
//! `parse_term` is public and recursive descent, and everything downstream of
//! it — substitution, free-variable collection, alpha-equivalence,
//! normalisation — recurses over the structure it builds. A term nested past
//! what the stack can hold does not fail: the process aborts on a signal no
//! caller can handle. Bounding the depth where the term is built means every
//! consumer inherits the bound.

#![allow(clippy::expect_used, clippy::unwrap_used, missing_docs)]

use std::sync::Arc;

use panproto_gat::Term;
use panproto_theory_dsl::compile_theory::{MAX_TERM_NESTING_DEPTH, parse_term};

/// `f(f(…f(x)…))`, nested `levels` deep.
fn nested_application(levels: usize) -> String {
    let mut src = String::with_capacity(levels * 3 + 1);
    for _ in 0..levels {
        src.push_str("f(");
    }
    src.push('x');
    for _ in 0..levels {
        src.push(')');
    }
    src
}

#[test]
fn a_term_nested_past_the_limit_is_an_error_and_not_a_crash() {
    let err = parse_term(&nested_application(MAX_TERM_NESTING_DEPTH * 8))
        .expect_err("a term nested past the limit must not parse");

    assert!(
        err.contains("nests deeper"),
        "expected a nesting-depth error, got {err}"
    );
}

/// A term right at the limit still parses, and the traversals that consume it
/// still run.
///
/// This is the other half of the bound's contract: the limit has to be a
/// depth the parser and its consumers can actually reach. Running it at
/// exactly the limit in an unoptimised build turns a change that grows any of
/// those frames into a failing test rather than a field report.
#[test]
fn a_term_at_the_limit_parses_and_survives_its_consumers() {
    let term = parse_term(&nested_application(MAX_TERM_NESTING_DEPTH))
        .expect("a term at the limit parses");

    assert_eq!(term.free_vars().len(), 1, "the term has one free variable");

    let mut subst = rustc_hash::FxHashMap::default();
    subst.insert(Arc::from("x"), Term::var("y"));
    let substituted = term.substitute(&subst);
    assert_ne!(substituted, term, "substitution reached the leaf");

    assert!(
        panproto_gat::alpha_equivalent(&term, &term),
        "a term is alpha-equivalent to itself"
    );
}

/// A `case` scrutinee and a `let` body descend through the same bound.
#[test]
fn binder_forms_are_bounded_too() {
    let deep = nested_application(MAX_TERM_NESTING_DEPTH * 8);

    let case = format!("case {deep} of C(a) => a end");
    assert!(
        parse_term(&case).is_err(),
        "a case scrutinee past the limit must not parse"
    );

    let binding = format!("let v = {deep} in v");
    assert!(
        parse_term(&binding).is_err(),
        "a let-bound term past the limit must not parse"
    );
}
