//! Regression: Python string literals must not accumulate interior
//! whitespace across emit cycles.
//!
//! A Python `string` is `SEQ[string_start, REPEAT(string_content),
//! string_end]` with the delimiters and content delivered by external
//! scanner tokens. If the delimiters are emitted as untyped operators,
//! the layout pass inserts a space between each quote and the content
//! (`'hello'` -> `' hello '`); on re-parse those spaces fold into the
//! captured `string_content`, so the literal grows by two characters
//! every round trip. The delimiters must bracket the content tightly so
//! emit is a byte fixed point with stable length.

#![cfg(all(feature = "grammars", feature = "lang-python"))]
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

fn assert_string_fixed_point(src: &'static [u8], must_contain: &'static str) {
    with_big_stack(move || {
        let reg = registry();
        let s1 = reg
            .parse_with_protocol("python", src, "x.py")
            .expect("parse");
        let e1 = reg.emit_pretty_with_protocol("python", &s1).expect("emit1");
        let s2 = reg
            .parse_with_protocol("python", &e1, "x.py")
            .expect("reparse");
        let e2 = reg.emit_pretty_with_protocol("python", &s2).expect("emit2");
        let e1s = String::from_utf8_lossy(&e1).into_owned();
        let e2s = String::from_utf8_lossy(&e2).into_owned();
        assert_eq!(
            e1, e2,
            "Python emit must be a fixed point.\ne1: {e1s}\ne2: {e2s}"
        );
        // No unbounded interior-whitespace growth across the round trip.
        assert_eq!(
            e1.len(),
            e2.len(),
            "string literal grew across emit cycle.\ne1: {e1s}\ne2: {e2s}"
        );
        assert!(
            e1s.contains(must_contain),
            "expected {must_contain:?} in emit; got {e1s}"
        );
    });
}

#[test]
fn python_single_quoted_string_is_fixed_point() {
    assert_string_fixed_point(b"x = 'hello'\n", "'hello'");
}

#[test]
fn python_double_quoted_string_is_fixed_point() {
    assert_string_fixed_point(b"x = \"hello\"\n", "\"hello\"");
}

#[test]
fn python_fstring_is_fixed_point() {
    assert_string_fixed_point(b"x = f\"hello\"\n", "\"hello\"");
}
