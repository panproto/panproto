//! Regression test for Python `with X as Y:` `emit_pretty` (#157).
//!
//! Verifies that the `as_pattern_target` identifier survives the
//! `emit_pretty` round-trip.

#![cfg(all(feature = "grammars", feature = "lang-python"))]
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
fn python_with_as_preserves_alias_identifier() {
    with_big_stack(|| {
        let reg = registry();
        let src = b"with ctx() as handle:\n    pass\n";
        let mut schema = reg
            .parse_with_protocol("python", src, "x.py")
            .expect("parse");
        strip_byte_fragments(&mut schema);
        let emitted = reg
            .emit_pretty_with_protocol("python", &schema)
            .expect("emit_pretty");
        let text = std::str::from_utf8(&emitted).unwrap();
        assert!(
            text.contains("handle"),
            "with-as alias identifier must survive emit_pretty, got: {text}"
        );
        assert!(
            !text.contains("as:") && !text.contains("as :"),
            "alias must not be empty after 'as', got: {text}"
        );
    });
}

#[test]
fn python_with_as_round_trip_no_errors() {
    with_big_stack(|| {
        let reg = registry();
        let src = b"with ctx() as handle:\n    pass\n";
        let mut schema = reg
            .parse_with_protocol("python", src, "x.py")
            .expect("parse");
        strip_byte_fragments(&mut schema);
        let emitted = reg
            .emit_pretty_with_protocol("python", &schema)
            .expect("emit_pretty");
        let reparsed = reg
            .parse_with_protocol("python", &emitted, "rt.py")
            .expect("reparse");
        let error_count = reparsed
            .vertices
            .values()
            .filter(|v| v.kind.as_ref() == "ERROR")
            .count();
        assert_eq!(
            error_count,
            0,
            "re-parsed Python should have 0 ERROR nodes, got {error_count}\nemitted:\n{}",
            std::str::from_utf8(&emitted).unwrap_or("<non-utf8>")
        );
    });
}
