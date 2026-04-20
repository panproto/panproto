//! Lens benchmarks against real AT Protocol records, driven through panproto-core.

#![allow(missing_docs, clippy::expect_used)]

use std::collections::{HashMap, HashSet};

use divan::Bencher;
use panproto_core::gat::Name;
use panproto_core::inst::CompiledMigration;
use panproto_core::lens::{Lens, get, put};
use panproto_core::protocols::web_document::atproto;
use panproto_core::schema::{Edge, Schema};

fn main() {
    divan::main();
}

const FEED_POST_LEXICON: &str =
    include_str!("../../../fixtures/atproto/lexicons/app.bsky.feed.post.json");
const REAL_POST: &[u8] = include_bytes!("../../../fixtures/atproto/records/post-0.json");

fn real_post_schema() -> Schema {
    let value: serde_json::Value = serde_json::from_str(FEED_POST_LEXICON).expect("lex json");
    atproto::parse_lexicon(&value).expect("lex schema")
}

fn real_identity_lens(schema: &Schema) -> Lens {
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
    Lens {
        compiled,
        src_schema: schema.clone(),
        tgt_schema: schema.clone(),
    }
}

/// `get` round-trip on a real Bluesky post record through an identity
/// lens over its Lexicon schema.
#[divan::bench]
fn lens_get_real_post_identity(bencher: Bencher) {
    let schema = real_post_schema();
    let lens = real_identity_lens(&schema);
    let registry = panproto_core::io::default_registry();
    let instance = registry
        .parse_wtype("atproto", &schema, REAL_POST)
        .expect("parse");
    bencher.bench(|| get(&lens, &instance));
}

/// `get` + `put` round-trip on a real record.
#[divan::bench]
fn lens_get_put_roundtrip_real_post(bencher: Bencher) {
    let schema = real_post_schema();
    let lens = real_identity_lens(&schema);
    let registry = panproto_core::io::default_registry();
    let instance = registry
        .parse_wtype("atproto", &schema, REAL_POST)
        .expect("parse");
    bencher.bench(|| {
        let (view, complement) = get(&lens, &instance).expect("get");
        put(&lens, &view, &complement)
    });
}
