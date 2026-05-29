//! Regression: Stan float literals must survive emit as a fixed point.
//!
//! The decimal point in a Stan `real_literal` is an `IMMEDIATE_TOKEN`:
//! `0.5` parses as `integer_literal "." integer_literal` with the `.`
//! lexically glued to its neighbours. A role classifier that ignores
//! immediacy renders it `0 . 5`; on re-parse the tree-sitter scanner
//! reads `0` as a complete integer and drops everything after the space,
//! so the literal collapses to `0`. The emitter must derive the `.`'s
//! tightness from the grammar so emit is a byte fixed point and no bytes
//! are dropped on the round trip.

#![cfg(feature = "grammars")]
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use panproto_parse::ParserRegistry;

fn registry() -> ParserRegistry {
    ParserRegistry::new()
}

fn with_big_stack<F: FnOnce() + Send + 'static>(inner: F) {
    std::thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(inner)
        .expect("spawn")
        .join()
        .expect("worker panicked");
}

/// Emit twice through a re-parse and assert the byte fixed point, plus
/// that every fragment in `must_keep` survives both emits (no dropped
/// bytes from a `0 . 5`-style split re-parsing as a truncated integer).
fn assert_float_fixed_point(src: &'static [u8], must_keep: &'static [&'static str]) {
    with_big_stack(move || {
        let reg = registry();
        let sch1 = reg
            .parse_with_protocol("stan", src, "x.stan")
            .expect("parse");
        let emit1 = reg.emit_pretty_with_protocol("stan", &sch1).expect("emit1");
        let sch2 = reg
            .parse_with_protocol("stan", &emit1, "x.stan")
            .expect("reparse");
        let emit2 = reg.emit_pretty_with_protocol("stan", &sch2).expect("emit2");
        let e1 = String::from_utf8_lossy(&emit1).into_owned();
        let e2 = String::from_utf8_lossy(&emit2).into_owned();
        assert_eq!(
            emit1, emit2,
            "Stan emit must be a fixed point.\ne1: {e1}\ne2: {e2}"
        );
        for frag in must_keep {
            assert!(
                e1.contains(frag),
                "float fragment {frag:?} dropped on first emit; got: {e1}"
            );
        }
    });
}

#[test]
fn stan_distribution_arg_float_is_fixed_point() {
    assert_float_fixed_point(b"model{\n  y ~ normal(0, 0.5);\n}\n", &["0.5"]);
}

#[test]
fn stan_arithmetic_floats_are_fixed_point() {
    assert_float_fixed_point(
        b"transformed data{\n  real x = 0.5 * 2.0;\n}\n",
        &["0.5", "2.0"],
    );
}

#[test]
fn stan_array_literal_floats_are_fixed_point() {
    assert_float_fixed_point(
        b"transformed data{\n  array[3] real a = {1.0, 2.0, 3.0};\n}\n",
        &["1.0", "2.0", "3.0"],
    );
}
