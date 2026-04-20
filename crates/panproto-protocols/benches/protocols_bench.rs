//! Protocol-level benchmarks on real AT Proto and Avro schemas.

#![allow(clippy::expect_used)]

use panproto_protocols::web_document::atproto;

fn main() {
    divan::main();
}

const FEED_POST: &str = include_str!("../../../fixtures/atproto/lexicons/app.bsky.feed.post.json");
const ACTOR_PROFILE: &str =
    include_str!("../../../fixtures/atproto/lexicons/app.bsky.actor.profile.json");
const FEED_LIKE: &str = include_str!("../../../fixtures/atproto/lexicons/app.bsky.feed.like.json");
const FEED_REPOST: &str =
    include_str!("../../../fixtures/atproto/lexicons/app.bsky.feed.repost.json");
const GRAPH_FOLLOW: &str =
    include_str!("../../../fixtures/atproto/lexicons/app.bsky.graph.follow.json");
const CREATE_RECORD: &str =
    include_str!("../../../fixtures/atproto/lexicons/com.atproto.repo.createRecord.json");

#[divan::bench]
fn build_atproto_protocol(bencher: divan::Bencher) {
    bencher.bench(atproto::protocol);
}

#[divan::bench(args = [
    ("feed.post", FEED_POST),
    ("actor.profile", ACTOR_PROFILE),
    ("feed.like", FEED_LIKE),
    ("feed.repost", FEED_REPOST),
    ("graph.follow", GRAPH_FOLLOW),
    ("createRecord", CREATE_RECORD),
])]
fn parse_lexicon(bencher: divan::Bencher, (_name, src): (&str, &str)) {
    let v: serde_json::Value = serde_json::from_str(src).expect("json");
    bencher.bench(|| atproto::parse_lexicon(&v));
}

#[divan::bench]
fn register_atproto_theories(bencher: divan::Bencher) {
    bencher.bench(|| {
        let mut registry = std::collections::HashMap::new();
        atproto::register_theories(&mut registry);
        registry.len()
    });
}
