//! Breaking-change detection benchmarks on real AT Proto Lexicons.
//!
//! Constructs a synthetic "next version" of a real Lexicon by dropping a
//! vertex or renaming an edge, then measures the diff/classify pipeline.

#![allow(clippy::expect_used)]

use panproto_check::{classify, diff};
use panproto_gat::Name;
use panproto_protocols::web_document::atproto;
use panproto_schema::Schema;

fn main() {
    divan::main();
}

const FEED_POST: &str =
    include_str!("../../../fixtures/atproto/lexicons/app.bsky.feed.post.json");
const ACTOR_PROFILE: &str =
    include_str!("../../../fixtures/atproto/lexicons/app.bsky.actor.profile.json");

fn load(src: &str) -> Schema {
    let v: serde_json::Value = serde_json::from_str(src).expect("json");
    atproto::parse_lexicon(&v).expect("lex")
}

/// Drop the first vertex that is not referenced as an entry. Models a
/// breaking change: removing a def that clients may depend on.
fn drop_one_vertex(schema: &Schema) -> Schema {
    let mut s = schema.clone();
    let entries: std::collections::HashSet<Name> = s.entries.iter().cloned().collect();
    if let Some(victim) = s
        .vertices
        .keys()
        .find(|k| !entries.contains(*k))
        .cloned()
    {
        s.vertices.remove(&victim);
        s.edges.retain(|e, _| e.src != victim && e.tgt != victim);
    }
    s
}

#[divan::bench]
fn diff_identity_feed_post(bencher: divan::Bencher) {
    let schema = load(FEED_POST);
    bencher.bench(|| diff(&schema, &schema));
}

#[divan::bench]
fn diff_feed_post_drop_vertex(bencher: divan::Bencher) {
    let old = load(FEED_POST);
    let new = drop_one_vertex(&old);
    bencher.bench(|| diff(&old, &new));
}

#[divan::bench]
fn classify_feed_post_drop_vertex(bencher: divan::Bencher) {
    let old = load(FEED_POST);
    let new = drop_one_vertex(&old);
    let d = diff(&old, &new);
    let protocol = atproto::protocol();
    bencher.bench(|| classify(&d, &protocol));
}

#[divan::bench]
fn diff_two_lexicons(bencher: divan::Bencher) {
    let post = load(FEED_POST);
    let profile = load(ACTOR_PROFILE);
    bencher.bench(|| diff(&post, &profile));
}
