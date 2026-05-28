//! Regression tests for Julia macrocall `emit_pretty`.
//!
//! Verifies that both long-form (`@model function ... end`) and
//! short-form (`@trace(args)`) macrocall expressions round-trip
//! through `emit_pretty` without dropping the body or arguments.

#![cfg(all(feature = "grammars", feature = "lang-julia"))]
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
fn julia_macrocall_long_form_preserves_body() {
    with_big_stack(|| {
        let reg = registry();
        let src = b"@model function model(y)\n    theta ~ Beta(2, 2)\nend\n";
        let mut schema = reg
            .parse_with_protocol("julia", src, "m.jl")
            .expect("parse");
        strip_byte_fragments(&mut schema);
        let emitted = reg
            .emit_pretty_with_protocol("julia", &schema)
            .expect("emit_pretty");
        let text = std::str::from_utf8(&emitted).unwrap();
        assert!(
            text.contains("function"),
            "long-form macrocall body must contain 'function', got: {text}"
        );
        assert!(
            text.contains("Beta"),
            "long-form macrocall body must contain 'Beta', got: {text}"
        );
        assert!(
            !text.starts_with('.'),
            "macrocall must not start with '.', got: {text}"
        );
    });
}

#[test]
fn julia_macrocall_short_form_preserves_args() {
    with_big_stack(|| {
        let reg = registry();
        let src = b"@trace(beta(2, 2), :theta)\n";
        let mut schema = reg
            .parse_with_protocol("julia", src, "x.jl")
            .expect("parse");
        strip_byte_fragments(&mut schema);
        let emitted = reg
            .emit_pretty_with_protocol("julia", &schema)
            .expect("emit_pretty");
        let text = std::str::from_utf8(&emitted).unwrap();
        assert!(
            text.contains("beta"),
            "short-form macrocall must contain 'beta', got: {text}"
        );
        assert!(
            text.contains("theta"),
            "short-form macrocall must contain 'theta', got: {text}"
        );
    });
}

#[test]
fn julia_macro_identifier_no_space_after_at() {
    with_big_stack(|| {
        let reg = registry();
        let src = b"@model function m()\nend\n";
        let mut schema = reg
            .parse_with_protocol("julia", src, "m.jl")
            .expect("parse");
        strip_byte_fragments(&mut schema);
        let emitted = reg
            .emit_pretty_with_protocol("julia", &schema)
            .expect("emit_pretty");
        let text = std::str::from_utf8(&emitted).unwrap();
        assert!(
            text.contains("@model") || text.contains("@m"),
            "@ must be tight against identifier (no space), got: {text}"
        );
        assert!(
            !text.contains("@ "),
            "@ must not be followed by a space, got: {text}"
        );
    });
}
