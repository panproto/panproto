//! `auto_generate` under the default configuration reports "no morphism
//! found" on corpus pairs that have no total morphism, and the report is
//! the truth about the pair rather than a gap in the search.
//!
//! The default configuration runs the balanced tier, which does not
//! permit spans, so what it looks for is a total schema morphism: every
//! source vertex sent to a target vertex of the same kind, and every
//! source edge to a target edge of the same kind between the images of
//! its ends. Each pair below fails that for a reason the test states and
//! checks on the schemas themselves, the exact total search
//! (`find_best_morphism`, which answers `None` only when no total
//! morphism exists) agrees, and `auto_generate` returns the typed
//! failure. The lenient tier, which permits spans, relates every pair by
//! a partial alignment, which is the answer these pairs have.

mod identity_corpus;

use std::collections::HashMap;

use identity_corpus::{Required, load_schemas, protocol_named, read_golden};
use panproto_lens::{AutoLensConfig, LensError, Stringency, auto_generate};
use panproto_schema::Schema;

const NO_MORPHISM: &str = "no morphism found between schemas";

fn schemas() -> HashMap<String, Schema> {
    load_schemas(&read_golden())
}

fn kinds(schema: &Schema) -> Vec<&str> {
    schema.vertices.values().map(|v| v.kind.as_ref()).collect()
}

/// The exact total search finds nothing, `auto_generate` reports the
/// typed failure under the default configuration, and a span tier finds
/// a partial alignment.
fn assert_no_total_morphism(protocol: &str, src: &Schema, tgt: &Schema) {
    let exact = panproto_mig::find_best_morphism(src, tgt, &panproto_mig::SearchOptions::default())
        .required("the total search completes");
    assert!(exact.is_none(), "the exact search finds a total morphism");

    let protocol = protocol_named(protocol);
    match auto_generate(src, tgt, &protocol, &AutoLensConfig::default()) {
        Err(LensError::ProtolensError(message)) => assert_eq!(message, NO_MORPHISM),
        Err(other) => panic!("expected the no-morphism failure, got {other}"),
        Ok(_) => panic!("auto_generate reported a lens where no total morphism exists"),
    }

    let lenient = AutoLensConfig {
        stringency: Stringency::Lenient,
        ..AutoLensConfig::default()
    };
    auto_generate(src, tgt, &protocol, &lenient).required("a span tier finds a partial alignment");
}

/// `users.active` is a boolean column and `users_posts` has no boolean
/// column, so the column has no image.
#[test]
fn users_to_users_posts_has_no_total_morphism() {
    let schemas = schemas();
    let src = &schemas["sql-tests/users"];
    let tgt = &schemas["sql-tests/users_posts"];
    assert_eq!(kinds(src).iter().filter(|k| **k == "boolean").count(), 1);
    assert!(!kinds(tgt).contains(&"boolean"));
    assert_no_total_morphism("sql", src, tgt);
}

/// The full lexicon declares `textSlice.start` and `textSlice.end` as
/// integers, and the test lexicon has no integer field.
#[test]
fn the_full_post_lexicon_to_the_test_one_has_no_total_morphism() {
    let schemas = schemas();
    let src = &schemas["atproto/app.bsky.feed.post"];
    let tgt = &schemas["atproto-tests/app.bsky.feed.post"];
    assert!(kinds(src).contains(&"integer"));
    assert!(!kinds(tgt).contains(&"integer"));
    assert_no_total_morphism("atproto", src, tgt);
}

/// The test lexicon declares `reply` inline, as an object property of the
/// record body. The full lexicon's body has one image (the only target of
/// its only `record-schema` edge), and none of that body's properties is
/// an object: its `reply` is a `ref` to `#replyRef`. So the source's
/// `body -prop-> reply` edge has no image.
#[test]
fn the_test_post_lexicon_to_the_full_one_has_no_total_morphism() {
    let schemas = schemas();
    let src = &schemas["atproto-tests/app.bsky.feed.post"];
    let tgt = &schemas["atproto/app.bsky.feed.post"];
    let body = "app.bsky.feed.post:body";

    let reply = src
        .vertex(&format!("{body}.reply"))
        .required("the test lexicon declares reply");
    assert_eq!(reply.kind, "object");
    let bodies: Vec<_> = tgt
        .edges
        .keys()
        .filter(|e| e.kind == "record-schema")
        .map(|e| e.tgt.as_ref())
        .collect();
    assert_eq!(bodies, [body]);
    assert!(
        tgt.outgoing_edges(body)
            .iter()
            .filter(|e| e.kind == "prop")
            .all(|e| tgt.vertices[&e.tgt].kind != "object"),
        "no property of the target body is an object"
    );
    assert_no_total_morphism("atproto", src, tgt);
}
