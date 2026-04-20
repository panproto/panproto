//! WASM boundary benchmarks against real AT Proto Lexicons and records.

#![allow(clippy::expect_used)]

use panproto_wasm::parse_atproto_lexicon;

fn main() {
    divan::main();
}

const FEED_POST_LEXICON: &[u8] =
    include_bytes!("../../../fixtures/atproto/lexicons/app.bsky.feed.post.json");
const ACTOR_PROFILE_LEXICON: &[u8] =
    include_bytes!("../../../fixtures/atproto/lexicons/app.bsky.actor.profile.json");

#[divan::bench]
fn wasm_parse_feed_post_lexicon(bencher: divan::Bencher) {
    bencher.bench(|| parse_atproto_lexicon(FEED_POST_LEXICON));
}

#[divan::bench]
fn wasm_parse_actor_profile_lexicon(bencher: divan::Bencher) {
    bencher.bench(|| parse_atproto_lexicon(ACTOR_PROFILE_LEXICON));
}
