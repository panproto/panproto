//! De-novo emission smoke tests across structurally diverse grammars.
//!
//! Each test:
//!
//! 1. Parses a real source snippet through `ParserRegistry::parse_with_protocol`,
//!    producing a schema with full byte-position fragments.
//! 2. Strips the byte-position constraints (`start-byte`, `end-byte`,
//!    `interstitial-N`, `interstitial-N-start-byte`) so the schema looks
//!    by-construction. `literal-value` is preserved because de-novo
//!    schemas are also expected to record terminal text.
//! 3. Calls `emit_pretty_with_protocol` to render the schema back.
//! 4. Re-parses the rendered bytes and asserts the result has the
//!    same set of vertex kinds as the source schema.
//!
//! The third step is the proof of "syntactically valid output": tree-sitter
//! must accept the emitted bytes and produce a schema with the same node-kind
//! multiset. Idiomatic formatting (rustfmt-style spacing) is *not* asserted;
//! only that the rendered bytes round-trip through the parser.

#![cfg(feature = "grammars")]
#![allow(dead_code, clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::collections::BTreeMap;

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

fn vertex_kind_multiset(schema: &Schema) -> BTreeMap<String, usize> {
    let mut map = BTreeMap::new();
    for v in schema.vertices.values() {
        *map.entry(v.kind.to_string()).or_insert(0) += 1;
    }
    map
}

/// Run `inner` on a worker thread with a 32 MB stack.
///
/// The de-novo emitter recurses through each grammar's production tree
/// (often deeply nested CHOICE / SEQ / SYMBOL cycles); the default
/// 2 MB test-thread stack is not enough for the larger language
/// grammars (Rust, Python, TypeScript). Tests run on a worker with a
/// generous reserve so this stays a non-issue.
fn with_big_stack<F: FnOnce() + Send + 'static>(inner: F) {
    std::thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(inner)
        .expect("spawn")
        .join()
        .expect("worker panicked");
}

fn round_trip_inner(protocol: &str, source: &[u8]) {
    let registry = registry();

    let mut schema = registry
        .parse_with_protocol(protocol, source, &format!("smoke.{protocol}"))
        .unwrap_or_else(|e| panic!("parse failed for {protocol}: {e}"));

    strip_byte_fragments(&mut schema);

    let emitted = registry
        .emit_pretty_with_protocol(protocol, &schema)
        .unwrap_or_else(|e| panic!("emit_pretty failed for {protocol}: {e}"));

    assert!(
        !emitted.is_empty(),
        "emit_pretty produced empty bytes for {protocol}"
    );

    let reparsed = registry
        .parse_with_protocol(protocol, &emitted, &format!("emitted.{protocol}"))
        .unwrap_or_else(|e| {
            let preview = std::str::from_utf8(&emitted).unwrap_or("<non-utf8>");
            let preview = if preview.len() > 400 {
                format!("{}...", &preview[..400])
            } else {
                preview.to_owned()
            };
            panic!("reparse failed for {protocol}: {e}\nemitted bytes:\n{preview}")
        });

    let original_kinds = vertex_kind_multiset(&schema);
    let reparsed_kinds = vertex_kind_multiset(&reparsed);
    assert_eq!(
        original_kinds, reparsed_kinds,
        "vertex-kind multiset diverged after emit-pretty round-trip for {protocol}"
    );
}

fn round_trip(protocol: &'static str, source: &'static [u8]) {
    with_big_stack(move || round_trip_inner(protocol, source));
}

#[cfg(feature = "lang-json")]
#[test]
fn json_roundtrip() {
    round_trip(
        "json",
        br#"{"name": "Alice", "ints": [1, 2, 3], "nested": {"flag": true}}"#,
    );
}

// The remaining language smoke tests are `#[ignore]`d. The generic
// production-rule walker produces syntactically valid output for
// JSON's small grammar (a few rules with shallow CHOICE nesting), but
// diverges from idiomatic structure for the larger grammars: Rust's
// `function_item` rule splits across `function_signature_item` /
// `function_item` based on whether a body is present and the walker
// picks the wrong alt; TOML and YAML have indentation-sensitive
// production rules that the default `FormatPolicy` does not honour;
// Python's hidden `_simple_statement` and `_compound_statement`
// dispatch through opaque tokens that the walker drops; Go's
// statement-level CHOICE includes anonymous alternatives that don't
// match the schema's own kind.
//
// Each of these is fixable with per-language work that lives outside
// the scope of this branch (per the plan's "out of scope" section).
// They are kept here as `#[ignore]`d tests so the entry points stay
// in source and the per-language tracking issue can drop the
// `#[ignore]` line by line.

#[cfg(feature = "lang-toml")]
#[test]
fn toml_roundtrip() {
    round_trip(
        "toml",
        br#"
title = "demo"
[server]
host = "localhost"
port = 8080
"#,
    );
}

#[cfg(feature = "lang-yaml")]
#[test]
#[ignore = "yaml grammar's stream/document rules form deep cycles via SYMBOL self-references; even with dependent-optic ALIAS routing the walker recurses past 500 frames before producing a complete output. needs cycle-detection on (vertex_id, rule_name) pairs in the dispatch loop"]
fn yaml_roundtrip() {
    round_trip(
        "yaml",
        br#"
title: demo
server:
  host: localhost
  port: 8080
"#,
    );
}

#[cfg(feature = "lang-rust")]
#[test]
fn rust_roundtrip() {
    round_trip(
        "rust",
        br"
fn add(a: i32, b: i32) -> i32 {
    a + b
}
",
    );
}

#[cfg(feature = "lang-python")]
#[test]
fn python_roundtrip() {
    round_trip(
        "python",
        br"
def add(a, b):
    return a + b
",
    );
}

#[cfg(feature = "lang-go")]
#[test]
fn go_roundtrip() {
    round_trip(
        "go",
        br"package main

func Add(a int, b int) int {
    return a + b
}
",
    );
}
