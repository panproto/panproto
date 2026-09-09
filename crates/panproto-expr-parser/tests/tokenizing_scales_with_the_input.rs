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
//!
//! Two things keep the measurement honest under a loaded machine, which is the
//! condition it actually runs in: a whole test suite in parallel. The inputs
//! are large enough that both timings sit well above the scheduler's noise
//! floor, and each is the *minimum* of several runs rather than the median.
//! Contention can only ever make a sample slower, never faster, so the minimum
//! is the closest estimate of the uncontended cost, while a median moves with
//! whatever else the machine was doing.

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

/// The fastest of nine timings.
///
/// Every source of noise here is one-sided: another process taking the core,
/// a page fault, the allocator going to the OS. Each makes a run slower and
/// none makes one faster, so the minimum is the best available estimate of
/// what the work costs, and it is the statistic that stays put when the rest
/// of the suite runs alongside it.
fn time_tokenizing(lines: usize) -> Duration {
    let input = source(lines);
    (0..9)
        .map(|_| {
            let start = Instant::now();
            let tokens = tokenize(&input).expect("the fixture tokenizes");
            assert!(tokens.len() > lines, "every line contributes tokens");
            start.elapsed()
        })
        .min()
        .expect("nine samples")
}

#[test]
fn four_times_the_input_does_not_cost_sixteen_times_the_time() {
    // Large enough that the smaller timing is milliseconds rather than the
    // hundreds of microseconds where a single scheduling slice dominates it.
    let small = time_tokenizing(2_000);
    let large = time_tokenizing(8_000);

    // Linear scanning puts this at about 4. Quadratic scanning puts it at
    // about 16. Eight leaves room for cache effects and allocator noise while
    // staying well clear of the quadratic curve.
    let ratio = large.as_secs_f64() / small.as_secs_f64().max(f64::EPSILON);
    assert!(
        ratio < 8.0,
        "2000 lines took {small:?}, 8000 lines took {large:?} — a ratio of \
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
