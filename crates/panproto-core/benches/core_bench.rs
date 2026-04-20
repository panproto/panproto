//! End-to-end pipeline benchmarks using the full panproto-core facade
//! against real AT Proto Lexicons and Bluesky post records.

#![allow(clippy::expect_used)]

use panproto_core::protocols::web_document::atproto;
use panproto_core::schema::Schema;

fn main() {
    divan::main();
}

const FEED_POST_LEXICON: &str =
    include_str!("../../../fixtures/atproto/lexicons/app.bsky.feed.post.json");
const REAL_POST: &[u8] = include_bytes!("../../../fixtures/atproto/records/post-0.json");

fn real_post_schema() -> Schema {
    let v: serde_json::Value = serde_json::from_str(FEED_POST_LEXICON).expect("json");
    atproto::parse_lexicon(&v).expect("lex")
}

/// parse → identity lift → emit round-trip over a real Bluesky post.
#[divan::bench]
fn parse_lift_emit_real_post(bencher: divan::Bencher) {
    use panproto_core::gat::Name;
    use panproto_core::mig::{Migration, compile, lift_wtype};
    use panproto_core::schema::Edge;

    let schema = real_post_schema();
    let vertex_ids: Vec<Name> = schema.vertices.keys().cloned().collect();
    let edges: Vec<Edge> = schema.edges.keys().cloned().collect();
    let migration = Migration::identity(&vertex_ids, &edges);
    let compiled = compile(&schema, &schema, &migration).expect("compile");
    let registry = panproto_core::io::default_registry();

    bencher.bench(|| {
        let instance = registry
            .parse_wtype("atproto", &schema, REAL_POST)
            .expect("parse");
        let lifted = lift_wtype(&compiled, &schema, &schema, &instance).expect("lift");
        registry
            .emit_wtype("atproto", &schema, &lifted)
            .expect("emit")
    });
}
