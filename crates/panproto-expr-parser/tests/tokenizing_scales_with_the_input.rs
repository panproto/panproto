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
//! A raw quotient of two timings is exposed to the machine's load, which is the
//! condition the test actually runs in: a whole suite in parallel. Other test
//! binaries preempt the tokenizer, move it between fast and slow cores and
//! contend for the caches, and a longer run over a larger input is slowed more
//! than a shorter one, so a linear tokenizer can show a superlinear quotient.
//! So the large run is measured against the small run itself: each round times
//! one large run and, right after it, a window of repeated small runs that lasts
//! about as long. Both windows tokenize about the same number of lines for
//! about the same time at the same moment, so the load slows both alike and
//! their quotient, the large run in small runs, keeps its value. The measure is
//! the median of that quotient over the rounds.

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

/// The time one tokenization of `input` takes.
fn time_tokenizing(input: &str, lines: usize) -> Duration {
    let start = Instant::now();
    let tokens = tokenize(input).expect("the fixture tokenizes");
    let elapsed = start.elapsed();
    assert!(tokens.len() > lines, "every line contributes tokens");
    elapsed
}

/// The median, over `rounds` rounds, of one tokenization of `large` lines
/// measured in tokenizations of `small` lines run right after it for about
/// as long.
fn large_run_in_small_runs(rounds: usize, small: usize, large: usize) -> f64 {
    let (small_input, large_input) = (source(small), source(large));
    // One warm-up run of each, then the repeat count that makes the window
    // of small runs about as long as one large run.
    let one_small = time_tokenizing(&small_input, small).max(Duration::from_micros(1));
    let one_large = time_tokenizing(&large_input, large);
    let (s, l) = (one_small.as_nanos(), one_large.as_nanos());
    let repeats = u32::try_from((l + s / 2) / s)
        .unwrap_or(u32::MAX)
        .clamp(1, 1_000_000);
    let mut quotients: Vec<f64> = (0..rounds)
        .map(|_| {
            let large_run = time_tokenizing(&large_input, large).as_secs_f64();
            let window: f64 = (0..repeats)
                .map(|_| time_tokenizing(&small_input, small).as_secs_f64())
                .sum();
            large_run * f64::from(repeats) / window
        })
        .collect();
    quotients.sort_by(f64::total_cmp);
    quotients[quotients.len() / 2]
}

#[test]
fn four_times_the_input_does_not_cost_sixteen_times_the_time() {
    // Large enough that one small run is milliseconds even when optimized,
    // well above the scheduler slice that dominates a run of microseconds.
    let ratio = large_run_in_small_runs(9, 8_000, 32_000);
    println!("32000 lines took {ratio:.2} times as long as 8000 lines");

    // Linear scanning puts this at about 4. Quadratic scanning puts it at
    // about 16. Eight leaves room for cache effects and allocator noise while
    // staying well clear of the quadratic curve.
    assert!(
        ratio < 8.0,
        "32000 lines took {ratio:.1} times as long as 8000 lines, which is the \
         quadratic curve, not the linear one"
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
