//! A deeply nested document is refused, not fatal.
//!
//! `UnifiedCodec` parses bytes into a CST and then walks that CST recursively
//! to build the instance. Both halves descend once per level of nesting, and
//! a recursive descent past the thread's stack aborts the process on a signal
//! no caller can handle. A codec reads bytes it did not author, so this has
//! to be an error value.

#![cfg(feature = "tree-sitter")]
#![allow(clippy::expect_used, clippy::unwrap_used)]

use panproto_io::cst_extract::{CstExtractError, MAX_EXTRACT_DEPTH, extract_json_cst};
use panproto_io::unified_codec::UnifiedCodec;
use panproto_schema::{Protocol, Schema, SchemaBuilder};

fn open_schema() -> Schema {
    let proto = Protocol {
        name: "test".into(),
        schema_theory: "ThtestSchema".into(),
        instance_theory: "ThtestInstance".into(),
        ..Protocol::default()
    };
    SchemaBuilder::new(&proto)
        .vertex("root", "object", None)
        .expect("root vertex")
        .build()
        .expect("build schema")
}

/// `[[[[…]]]]`, nested `levels` deep.
fn nested_arrays(levels: usize) -> Vec<u8> {
    let mut src = String::with_capacity(levels * 2);
    for _ in 0..levels {
        src.push('[');
    }
    for _ in 0..levels {
        src.push(']');
    }
    src.into_bytes()
}

#[test]
fn a_deeply_nested_document_is_an_error_and_not_a_crash() {
    let codec = UnifiedCodec::json("test").expect("json codec");
    let schema = open_schema();

    let err = codec
        .parse_wtype_preserving(&schema, &nested_arrays(100_000))
        .expect_err("a document nested a hundred thousand deep must not parse");

    // The message names the limit that stopped it; which of the two bounded
    // descents got there first is an implementation detail.
    let rendered = err.to_string();
    assert!(
        rendered.contains("nesting"),
        "expected a nesting-depth error, got {rendered}"
    );
}

/// A CST of `levels` nested JSON arrays under a document root.
fn nested_array_cst(levels: usize) -> Schema {
    let proto = Protocol {
        name: "test".into(),
        schema_theory: "ThtestSchema".into(),
        instance_theory: "ThtestInstance".into(),
        ..Protocol::default()
    };

    let mut builder = SchemaBuilder::new(&proto)
        .vertex("doc", "document", None)
        .expect("document vertex");
    let mut parent = "doc".to_owned();
    for level in 0..levels {
        let child = format!("a{level}");
        builder = builder
            .vertex(&child, "array", None)
            .expect("array vertex")
            .edge(&parent, &child, "child_of", None)
            .expect("child edge");
        parent = child;
    }
    builder.build().expect("build CST")
}

/// The extractors carry their own bound, independent of the parser's.
///
/// `extract_json_cst` is public and takes a CST as a [`Schema`], which a
/// caller can build without going through a parser at all. Handing it more
/// nesting than it can descend must be an error rather than an abort.
#[test]
fn extraction_bounds_its_own_descent() {
    let cst = nested_array_cst(MAX_EXTRACT_DEPTH * 4);

    let err = extract_json_cst(&cst, &open_schema(), "root")
        .expect_err("a CST nested past the limit must not extract");

    assert!(
        matches!(err, CstExtractError::NestingTooDeep { .. }),
        "expected a nesting-depth error, got {err}"
    );
}

/// Extraction right at the limit still succeeds.
///
/// The other half of the bound's contract: the limit has to be a depth the
/// extractors can actually reach. Running it at exactly the limit in an
/// unoptimised build turns a change that grows an extractor's stack frame
/// into a failing test rather than a field report.
#[test]
fn extraction_at_the_limit_succeeds() {
    let cst = nested_array_cst(MAX_EXTRACT_DEPTH);

    extract_json_cst(&cst, &open_schema(), "root")
        .expect("a CST nested exactly to the limit must extract");
}

/// Nesting the codec *can* take still parses, and still round-trips.
#[test]
fn moderate_nesting_still_round_trips() {
    let codec = UnifiedCodec::json("test").expect("json codec");
    let schema = open_schema();
    let src = nested_arrays(64);

    let (instance, complement) = codec
        .parse_wtype_preserving(&schema, &src)
        .expect("moderate nesting must parse");

    let emitted = codec
        .emit_wtype_preserving(&schema, &instance, &complement)
        .expect("emit");
    assert_eq!(emitted, src, "a moderately nested document did not replay");
}
