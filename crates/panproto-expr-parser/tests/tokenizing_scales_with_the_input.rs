//! Tokenizing a long source costs time proportional to its length.
//!
//! The layout pass needs each token's line and column. It used to recompute
//! them by counting newlines from the start of the input for every token,
//! which is quadratic: a source four times as long costs sixteen times as
//! much, and a forty-thousand-line one costs minutes.
//!
//! Wall-clock budgets make poor assertions, so this measures the shape of the
//! curve instead: quadrupling the input must not quadruple the cost four times
//! over. The bound below is loose enough that machine noise cannot trip it and
//! tight enough that a per-token rescan cannot pass.

#![allow(
    clippy::expect_used,
    reason = "a fixture that will not build is a defect in this file"
)]

use std::time::{Duration, Instant};

use panproto_expr_parser::lexer::tokenize;
use panproto_expr_parser::token::Token;

/// A source of `lines` bindings, each on its own line, so the layout pass has
/// a line and column to compute for every token.
fn source(lines: usize) -> String {
    let mut out = String::with_capacity(lines * 24);
    for i in 0..lines {
        out.push_str("let x");
        out.push_str(&i.to_string());
        out.push_str(" = ");
        out.push_str(&i.to_string());
        out.push_str(" in x");
        out.push_str(&i.to_string());
        out.push('\n');
    }
    out
}

/// Median of five timings, so one scheduling hiccup does not decide the test.
fn time_tokenizing(lines: usize) -> Duration {
    let input = source(lines);
    let mut samples: Vec<Duration> = (0..5)
        .map(|_| {
            let start = Instant::now();
            let tokens = tokenize(&input).expect("the fixture tokenizes");
            assert!(tokens.len() > lines, "every line contributes tokens");
            start.elapsed()
        })
        .collect();
    samples.sort_unstable();
    samples[2]
}

#[test]
fn four_times_the_input_does_not_cost_sixteen_times_the_time() {
    let small = time_tokenizing(500);
    let large = time_tokenizing(2_000);

    // Linear scanning puts this at about 4. Quadratic scanning puts it at
    // about 16. Eight leaves room for cache effects and allocator noise while
    // staying well clear of the quadratic curve.
    let ratio = large.as_secs_f64() / small.as_secs_f64().max(f64::EPSILON);
    assert!(
        ratio < 8.0,
        "500 lines took {small:?}, 2000 lines took {large:?} — a ratio of \
         {ratio:.1}, which is the quadratic curve, not the linear one"
    );
}

#[test]
fn layout_is_unchanged_by_how_the_positions_are_computed() {
    let input = "let\n  x = 1\n  y = 2\nin x";
    let tokens = tokenize(input).expect("the fixture tokenizes");
    let kinds: Vec<&Token> = tokens.iter().map(|s| &s.token).collect();
    assert!(kinds.contains(&&Token::Indent));
    assert!(kinds.contains(&&Token::Newline));
    assert!(kinds.contains(&&Token::Dedent));
}

#[test]
fn a_carriage_return_line_ending_still_starts_a_line() {
    let input = "let\r\n  x = 1\r\n  y = 2\r\nin x";
    let tokens = tokenize(input).expect("the fixture tokenizes");
    let kinds: Vec<&Token> = tokens.iter().map(|s| &s.token).collect();
    assert!(kinds.contains(&&Token::Indent));
    assert!(kinds.contains(&&Token::Newline));
    assert!(kinds.contains(&&Token::Dedent));
}

/// A nested block still opens and closes where the columns say it does, which
/// is the property a running line-and-column cursor has to preserve.
#[test]
fn nested_blocks_open_and_close_in_the_same_places() {
    let input = "let\n  x = let\n        y = 1\n      in y\n  z = 2\nin x";
    let tokens = tokenize(input).expect("the fixture tokenizes");
    let indents = tokens.iter().filter(|s| s.token == Token::Indent).count();
    let dedents = tokens.iter().filter(|s| s.token == Token::Dedent).count();
    assert_eq!(indents, 2, "one block per `let`");
    assert_eq!(dedents, 2, "every block that opens closes");
}
