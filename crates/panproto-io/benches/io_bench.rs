//! Throughput benchmarks for panproto-io against real fixtures.
//!
//! Parses real Bluesky post records (AT Proto JSON), real JSON Schema
//! Store documents, and a real GitHub Actions workflow schema through
//! the `panproto-io` codec registry.

#![allow(clippy::expect_used)]

use panproto_protocols::web_document::atproto;
use panproto_schema::Schema;

fn main() {
    divan::main();
}

const FEED_POST_LEXICON: &str =
    include_str!("../../../fixtures/atproto/lexicons/app.bsky.feed.post.json");
const ACTOR_PROFILE_LEXICON: &str =
    include_str!("../../../fixtures/atproto/lexicons/app.bsky.actor.profile.json");
const POST_RECORD_BYTES: &[u8] =
    include_bytes!("../../../fixtures/atproto/records/post-0.json");
const POST_WITH_REPLY_BYTES: &[u8] =
    include_bytes!("../../../fixtures/atproto/records/post-3.json");
const PROFILE_RECORD_BYTES: &[u8] =
    include_bytes!("../../../fixtures/atproto/records/profile-record.json");

fn load_lexicon(src: &str) -> Schema {
    let value: serde_json::Value = serde_json::from_str(src).expect("lexicon parses");
    atproto::parse_lexicon(&value).expect("lexicon builds schema")
}

#[divan::bench]
fn parse_real_bsky_post(bencher: divan::Bencher<'_, '_>) {
    let schema = load_lexicon(FEED_POST_LEXICON);
    let registry = panproto_io::default_registry();
    bencher.bench_local(|| {
        registry
            .parse_wtype("atproto", &schema, POST_RECORD_BYTES)
            .expect("parse")
    });
}

#[divan::bench]
fn parse_real_bsky_post_with_reply(bencher: divan::Bencher<'_, '_>) {
    let schema = load_lexicon(FEED_POST_LEXICON);
    let registry = panproto_io::default_registry();
    bencher.bench_local(|| {
        registry
            .parse_wtype("atproto", &schema, POST_WITH_REPLY_BYTES)
            .expect("parse")
    });
}

#[divan::bench]
fn parse_real_bsky_profile(bencher: divan::Bencher<'_, '_>) {
    let schema = load_lexicon(ACTOR_PROFILE_LEXICON);
    let registry = panproto_io::default_registry();
    bencher.bench_local(|| {
        registry
            .parse_wtype("atproto", &schema, PROFILE_RECORD_BYTES)
            .expect("parse")
    });
}

#[divan::bench]
fn parse_emit_roundtrip_real_post(bencher: divan::Bencher<'_, '_>) {
    let schema = load_lexicon(FEED_POST_LEXICON);
    let registry = panproto_io::default_registry();
    bencher.bench_local(|| {
        let instance = registry
            .parse_wtype("atproto", &schema, POST_RECORD_BYTES)
            .expect("parse");
        registry
            .emit_wtype("atproto", &schema, &instance)
            .expect("emit")
    });
}
