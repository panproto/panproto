//! Regression test for JavaScript automatic semicolon insertion.
//!
//! Verifies that `emit_pretty` inserts semicolons between sibling
//! statements in a `statement_block`, producing output that JavaScript's
//! parser can re-parse without ERROR nodes.

#![cfg(feature = "grammars")]
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use panproto_parse::ParserRegistry;
use panproto_schema::Schema;

fn registry() -> ParserRegistry {
    ParserRegistry::new()
}

fn strip_byte_fragments(schema: &mut Schema) {
    for constraints in schema.constraints.values_mut() {
        constraints.retain(|c| {
            let s = c.sort.as_ref();
            !(s == "start-byte" || s == "end-byte" || s.starts_with("interstitial-"))
        });
    }
}

fn with_big_stack<F: FnOnce() + Send + 'static>(inner: F) {
    std::thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(inner)
        .expect("spawn")
        .join()
        .expect("worker panicked");
}

#[test]
fn js_statement_block_has_semicolons() {
    with_big_stack(|| {
        let reg = registry();
        let src = b"function model(y){\n  var theta = sample(Beta(2, 2));\n  observe(Bernoulli(theta), y);\n}\n";
        let mut schema = reg
            .parse_with_protocol("javascript", src, "m.js")
            .expect("parse");
        strip_byte_fragments(&mut schema);
        let emitted = reg
            .emit_pretty_with_protocol("javascript", &schema)
            .expect("emit_pretty");
        let text = std::str::from_utf8(&emitted).unwrap();
        assert!(
            text.contains(';'),
            "emitted JavaScript must contain semicolons, got: {text}"
        );
    });
}

#[test]
fn js_round_trip_no_error_nodes() {
    with_big_stack(|| {
        let reg = registry();
        let src = b"function f(){\n  var x = 1;\n  var y = 2;\n  return x + y;\n}\n";
        let mut schema = reg
            .parse_with_protocol("javascript", src, "f.js")
            .expect("parse");
        strip_byte_fragments(&mut schema);
        let emitted = reg
            .emit_pretty_with_protocol("javascript", &schema)
            .expect("emit_pretty");
        let reparsed = reg
            .parse_with_protocol("javascript", &emitted, "rt.js")
            .expect("reparse");
        let error_count = reparsed
            .vertices
            .values()
            .filter(|v| v.kind.as_ref() == "ERROR")
            .count();
        assert_eq!(
            error_count,
            0,
            "re-parsed JavaScript should have 0 ERROR nodes, got {error_count}\nemitted:\n{}",
            std::str::from_utf8(&emitted).unwrap_or("<non-utf8>")
        );
    });
}
