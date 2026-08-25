//! Deeply nested input is refused, not fatal.
//!
//! The walk that turns a tree-sitter parse tree into a schema is recursive,
//! and its stack frame is large. Descending without a bound turns a few
//! hundred bytes of nesting into a stack overflow, which is not a failure a
//! caller can handle: the process dies on a signal, taking down whatever was
//! hosting the parse. A parser at a library boundary reads bytes it did not
//! author, so this has to be an error value.
//!
//! The bound is [`DEFAULT_MAX_NESTING_DEPTH`], adjustable per walk through
//! [`WalkerConfig::max_depth`].

#![cfg(all(feature = "grammars", feature = "lang-json"))]
#![allow(clippy::expect_used, clippy::unwrap_used)]

use panproto_parse::{DEFAULT_MAX_NESTING_DEPTH, ParseError, ParserRegistry};

/// `[[[[…]]]]`, nested `levels` deep.
fn nested_arrays(levels: usize) -> Vec<u8> {
    let mut src = String::with_capacity(levels * 2 + 1);
    for _ in 0..levels {
        src.push('[');
    }
    for _ in 0..levels {
        src.push(']');
    }
    src.into_bytes()
}

#[test]
fn nesting_past_the_limit_is_an_error_and_not_a_crash() {
    let reg = ParserRegistry::new();
    // Comfortably past the bound, and small enough that the old unbounded
    // walk would still have been well inside tree-sitter's own capacity.
    let src = nested_arrays(DEFAULT_MAX_NESTING_DEPTH * 4);

    let err = reg
        .parse_with_protocol("json", &src, "deep.json")
        .expect_err("a tree nested past the limit must not parse");

    match err {
        ParseError::NestingTooDeep { limit, .. } => {
            assert_eq!(limit, DEFAULT_MAX_NESTING_DEPTH);
        }
        other => panic!("expected a nesting-depth error, got {other}"),
    }
}

/// Nesting right at the limit still parses, and still round-trips.
///
/// This is the other half of the bound's contract: the limit has to be a
/// depth the walk can actually reach. Running it at exactly the limit in an
/// unoptimised build turns a change that grows the walk's stack frame into a
/// failing test rather than a field report.
#[test]
fn nesting_at_the_limit_still_round_trips() {
    let reg = ParserRegistry::new();
    let src = nested_arrays(DEFAULT_MAX_NESTING_DEPTH);

    let parsed = reg
        .parse_with_protocol("json", &src, "deep.json")
        .expect("nesting inside the limit must parse");
    let emitted = reg.emit_with_protocol("json", &parsed).expect("emit");

    assert_eq!(emitted, src, "a deep but admissible tree did not replay");
}
