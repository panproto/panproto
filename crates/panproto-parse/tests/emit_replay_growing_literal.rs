//! Replay coverage is decided by recorded source spans, not by the length
//! of the text a fragment currently carries.
//!
//! The layout complement records, for every fragment, the span of source it
//! came from. Replay writes the fragments in source order and skips any whose
//! span the output has already passed. Deciding that from the *rewritten*
//! text's length instead conflates two different quantities: a fragment edited
//! to be longer than the bytes it replaced pushes the notional cursor past the
//! recorded start of the fragments that follow, and every one of them is
//! dropped. The observable damage is that editing one value deletes the rest
//! of the file after it.
//!
//! These tests edit a `literal-value` in place — the same thing
//! `panproto-io`'s injection path does when an instance's value changes — and
//! assert that the fragments after the edit still come out.

#![cfg(all(feature = "grammars", feature = "lang-rust"))]
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use panproto_gat::is_interstitial_text_sort;
use panproto_parse::ParserRegistry;
use panproto_schema::Schema;

/// Rewrite the `literal-value` of the vertex whose literal is exactly
/// `old`, to `new`. Returns the number of vertices rewritten.
fn rewrite_literal(schema: &mut Schema, old: &str, new: &str) -> usize {
    let mut rewritten = 0;
    for constraints in schema.constraints.values_mut() {
        let is_target = constraints
            .iter()
            .any(|c| c.sort.as_ref() == "literal-value" && c.value == old);
        if !is_target {
            continue;
        }
        for c in constraints.iter_mut() {
            if (c.sort.as_ref() == "literal-value" || is_interstitial_text_sort(c.sort.as_ref()))
                && c.value == old
            {
                new.clone_into(&mut c.value);
            }
        }
        rewritten += 1;
    }
    rewritten
}

#[test]
fn growing_a_literal_keeps_every_following_fragment() {
    let reg = ParserRegistry::new();
    let src = b"fn main() {\n    let a = 42;\n    let b = 7;\n    let c = 9;\n}\n";
    let mut parsed = reg
        .parse_with_protocol("rust", src, "main.rs")
        .expect("parse");

    // Untouched replay is byte-identical: the baseline the edit is measured
    // against.
    let replayed = reg.emit_with_protocol("rust", &parsed).expect("emit");
    assert_eq!(
        replayed,
        src.to_vec(),
        "untouched replay is not byte-identical"
    );

    // Grow `42` (two bytes) into a nineteen-byte literal.
    assert_eq!(
        rewrite_literal(&mut parsed, "42", "1234567890123456789"),
        1,
        "expected exactly one `42` literal in the source"
    );

    let emitted = reg
        .emit_with_protocol("rust", &parsed)
        .expect("emit after edit");
    let emitted = String::from_utf8(emitted).expect("emitted bytes are UTF-8");

    assert_eq!(
        emitted,
        "fn main() {\n    let a = 1234567890123456789;\n    let b = 7;\n    let c = 9;\n}\n",
        "growing a literal displaced the fragments after it"
    );
}

#[test]
fn shrinking_a_literal_keeps_every_following_fragment() {
    let reg = ParserRegistry::new();
    let src = b"fn main() {\n    let a = 1234567890123456789;\n    let b = 7;\n}\n";
    let mut parsed = reg
        .parse_with_protocol("rust", src, "main.rs")
        .expect("parse");

    assert_eq!(
        rewrite_literal(&mut parsed, "1234567890123456789", "0"),
        1,
        "expected exactly one long literal in the source"
    );

    let emitted = reg
        .emit_with_protocol("rust", &parsed)
        .expect("emit after edit");
    let emitted = String::from_utf8(emitted).expect("emitted bytes are UTF-8");

    assert_eq!(
        emitted, "fn main() {\n    let a = 0;\n    let b = 7;\n}\n",
        "shrinking a literal replayed stale bytes"
    );
}

#[test]
fn growing_a_string_literal_keeps_every_following_fragment() {
    let reg = ParserRegistry::new();
    let src = b"fn main() {\n    let a = \"x\";\n    let b = \"y\";\n}\n";
    let mut parsed = reg
        .parse_with_protocol("rust", src, "main.rs")
        .expect("parse");

    assert_eq!(
        rewrite_literal(&mut parsed, "x", "a much longer string value"),
        1,
        "expected exactly one `x` string content in the source"
    );

    let emitted = reg
        .emit_with_protocol("rust", &parsed)
        .expect("emit after edit");
    let emitted = String::from_utf8(emitted).expect("emitted bytes are UTF-8");

    assert_eq!(
        emitted,
        "fn main() {\n    let a = \"a much longer string value\";\n    let b = \"y\";\n}\n",
        "growing a string literal displaced the fragments after it"
    );
}
