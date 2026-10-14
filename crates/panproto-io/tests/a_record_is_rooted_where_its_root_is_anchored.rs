//! The JSON codec anchors a record's root through a transparent
//! indirection (an ATProto lexicon's record vertex resolves to its body),
//! and the instance's schema root is the vertex the root node is anchored
//! to, as it is for `panproto_inst::parse_json` of the same record.

#![allow(
    clippy::expect_used,
    reason = "a fixture that will not read is a defect in this file"
)]

use std::path::{Path, PathBuf};

fn repository(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
}

#[test]
fn the_codec_roots_a_record_where_its_root_node_is_anchored() {
    let lexicon: serde_json::Value = serde_json::from_slice(
        &std::fs::read(repository(
            "fixtures/atproto/lexicons/app.bsky.feed.post.json",
        ))
        .expect("the lexicon reads"),
    )
    .expect("the lexicon is JSON");
    let schema = panproto_protocols::atproto::parse_lexicon(&lexicon).expect("the lexicon parses");
    let bytes = std::fs::read(repository("fixtures/atproto/records/post-0.json"))
        .expect("the record reads");

    let instance = panproto_io::default_registry()
        .parse_wtype("atproto", &schema, &bytes)
        .expect("the codec parses the record");
    let anchor = &instance.nodes[&instance.root].anchor;
    assert_eq!(anchor.as_ref(), "app.bsky.feed.post:body");
    assert_eq!(&instance.schema_root, anchor);
}
