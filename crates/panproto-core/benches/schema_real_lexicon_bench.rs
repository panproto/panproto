//! Schema benchmarks against real AT Protocol Lexicons.

#![allow(clippy::expect_used)]

use divan::Bencher;
use panproto_core::protocols::web_document::atproto;
use panproto_core::schema::{Schema, normalize, validate};

fn main() {
    divan::main();
}

const FEED_POST: &str = include_str!("../../../fixtures/atproto/lexicons/app.bsky.feed.post.json");
const ACTOR_PROFILE: &str =
    include_str!("../../../fixtures/atproto/lexicons/app.bsky.actor.profile.json");
const FEED_LIKE: &str = include_str!("../../../fixtures/atproto/lexicons/app.bsky.feed.like.json");
const GRAPH_FOLLOW: &str =
    include_str!("../../../fixtures/atproto/lexicons/app.bsky.graph.follow.json");
const CREATE_RECORD: &str =
    include_str!("../../../fixtures/atproto/lexicons/com.atproto.repo.createRecord.json");

fn load(src: &str) -> Schema {
    let v: serde_json::Value = serde_json::from_str(src).expect("json");
    atproto::parse_lexicon(&v).expect("lexicon")
}

#[divan::bench(args = [
    ("feed.post", FEED_POST),
    ("actor.profile", ACTOR_PROFILE),
    ("feed.like", FEED_LIKE),
    ("graph.follow", GRAPH_FOLLOW),
    ("createRecord", CREATE_RECORD),
])]
fn parse_lexicon_to_schema(bencher: Bencher, (_name, src): (&str, &str)) {
    let v: serde_json::Value = serde_json::from_str(src).expect("json");
    bencher.bench(|| atproto::parse_lexicon(&v));
}

#[divan::bench]
fn normalize_feed_post(bencher: Bencher) {
    let schema = load(FEED_POST);
    bencher.bench(|| normalize(&schema));
}

#[divan::bench]
fn validate_feed_post(bencher: Bencher) {
    let schema = load(FEED_POST);
    let protocol = atproto::protocol();
    bencher.bench(|| validate(&schema, &protocol));
}

#[divan::bench]
fn clone_feed_post(bencher: Bencher) {
    let schema = load(FEED_POST);
    bencher.bench(|| schema.clone());
}
