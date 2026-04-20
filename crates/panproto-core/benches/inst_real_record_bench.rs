//! Instance benchmarks against real AT Protocol records, driven through
//! the panproto-core facade.

#![allow(missing_docs, clippy::expect_used)]

use std::collections::{HashMap, HashSet};

use divan::Bencher;
use panproto_core::gat::Name;
use panproto_core::inst::{CompiledMigration, wtype_restrict};
use panproto_core::protocols::web_document::atproto;
use panproto_core::schema::{Edge, Schema};

fn main() {
    divan::main();
}

const FEED_POST_LEXICON: &str =
    include_str!("../../../fixtures/atproto/lexicons/app.bsky.feed.post.json");
const REAL_POST_0: &[u8] = include_bytes!("../../../fixtures/atproto/records/post-0.json");
const REAL_POST_3: &[u8] = include_bytes!("../../../fixtures/atproto/records/post-3.json");

fn real_post_schema() -> Schema {
    let value: serde_json::Value = serde_json::from_str(FEED_POST_LEXICON).expect("lex json");
    atproto::parse_lexicon(&value).expect("lex schema")
}

#[divan::bench]
fn parse_real_bsky_post_to_winstance(bencher: Bencher) {
    let schema = real_post_schema();
    let registry = panproto_core::io::default_registry();
    bencher.bench(|| {
        registry
            .parse_wtype("atproto", &schema, REAL_POST_0)
            .expect("parse")
    });
}

#[divan::bench]
fn parse_real_bsky_post_with_reply_to_winstance(bencher: Bencher) {
    let schema = real_post_schema();
    let registry = panproto_core::io::default_registry();
    bencher.bench(|| {
        registry
            .parse_wtype("atproto", &schema, REAL_POST_3)
            .expect("parse")
    });
}

/// Restrict a real Bluesky post `WInstance` along the identity migration of
/// its schema. Identity is chosen because building a non-trivial migration
/// requires bespoke setup; the identity path still exercises the
/// reachability/contraction pipeline on a real record.
#[divan::bench]
fn wtype_restrict_identity_real_post(bencher: Bencher) {
    let schema = real_post_schema();
    let registry = panproto_core::io::default_registry();
    let instance = registry
        .parse_wtype("atproto", &schema, REAL_POST_0)
        .expect("parse");

    let surviving_verts: HashSet<Name> = schema.vertices.keys().cloned().collect();
    let surviving_edges: HashSet<Edge> = schema.edges.keys().cloned().collect();
    let compiled = CompiledMigration {
        surviving_verts,
        surviving_edges,
        vertex_remap: HashMap::new(),
        edge_remap: HashMap::new(),
        resolver: HashMap::new(),
        hyper_resolver: HashMap::new(),
        field_transforms: HashMap::new(),
        conditional_survival: HashMap::new(),
        expansion_path: HashMap::new(),
    };

    bencher.bench(|| wtype_restrict(&instance, &schema, &schema, &compiled));
}
