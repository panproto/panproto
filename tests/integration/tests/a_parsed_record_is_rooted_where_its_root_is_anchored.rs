//! A record parsed at a vertex that resolves through a transparent
//! indirection (an ATProto lexicon's record vertex resolves to its body)
//! is anchored at the body, so the instance's schema root is the body too.
//!
//! `WInstance::schema_root` is the vertex the root node is anchored to. A
//! parse that kept the requested vertex instead disagreed with every
//! instance a lens writes back, so the lens laws failed on the record
//! itself.

#![allow(
    clippy::expect_used,
    reason = "a fixture that will not read is a defect in this file"
)]

use std::path::Path;

fn read(path: &str) -> serde_json::Value {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let text = std::fs::read_to_string(root.join(path)).expect("the fixture reads");
    serde_json::from_str(&text).expect("the fixture is JSON")
}

#[test]
fn the_schema_root_of_a_parsed_record_is_its_root_anchor() {
    let schema = panproto_protocols::atproto::parse_lexicon(&read(
        "fixtures/atproto/lexicons/app.bsky.feed.post.json",
    ))
    .expect("the lexicon parses");
    let record = read("fixtures/atproto/records/post-0.json");

    let instance = panproto_inst::parse_json(&schema, "app.bsky.feed.post", &record)
        .expect("the record parses");
    let anchor = &instance.nodes[&instance.root].anchor;
    assert_eq!(anchor.as_ref(), "app.bsky.feed.post:body");
    assert_eq!(&instance.schema_root, anchor);
}

#[test]
fn the_laws_hold_for_the_post_lexicons_lens_to_itself_on_a_record() {
    let schema = panproto_protocols::atproto::parse_lexicon(&read(
        "fixtures/atproto/lexicons/app.bsky.feed.post.json",
    ))
    .expect("the lexicon parses");
    let protocol = panproto_protocols::atproto::protocol();
    let record = read("fixtures/atproto/records/post-0.json");
    let instance = panproto_inst::parse_json(&schema, "app.bsky.feed.post", &record)
        .expect("the record parses");

    let config = panproto_lens::AutoLensConfig {
        stringency: panproto_lens::Stringency::Lenient,
        ..panproto_lens::AutoLensConfig::default()
    };
    let generated = panproto_lens::auto_generate(&schema, &schema, &protocol, &config)
        .expect("the lexicon aligns with itself");
    let lens = generated
        .chain
        .instantiate(&schema, &protocol)
        .expect("the chain instantiates");

    let get_put = panproto_lens::check_get_put(&lens, &instance);
    assert!(get_put.is_ok(), "GetPut: {get_put:?}");
    let put_get = panproto_lens::check_put_get(&lens, &instance);
    assert!(put_get.is_ok(), "PutGet: {put_get:?}");
}
