//! The target schema an auto-generated lens instantiates equals the target
//! it was asked for, up to renaming.
//!
//! `auto_generate` aligns part of the source with part of the target, and
//! realizes the rest of the target as concrete additions. A source vertex or
//! edge the alignment leaves out has no place in the requested target, so it
//! must not survive into the instantiated one. Renaming is read through the
//! kind multiset and the edge-shape multiset, which a renaming of vertex ids
//! preserves.

#![allow(
    clippy::expect_used,
    reason = "a fixture that will not read is a defect in this file"
)]

use std::path::Path;

use panproto_lens::{AutoLensConfig, Stringency};
use panproto_schema::{Schema, edge_multiset, kind_multiset};

fn lexicon(name: &str) -> Schema {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let text = std::fs::read_to_string(root.join("fixtures/atproto/lexicons").join(name))
        .expect("the lexicon reads");
    let json: serde_json::Value = serde_json::from_str(&text).expect("the lexicon is JSON");
    panproto_protocols::atproto::parse_lexicon(&json).expect("the lexicon parses")
}

#[test]
fn the_post_to_profile_lens_targets_the_profile_schema_up_to_renaming() {
    let post = lexicon("app.bsky.feed.post.json");
    let profile = lexicon("app.bsky.actor.profile.json");
    let protocol = panproto_protocols::atproto::protocol();
    let config = AutoLensConfig {
        stringency: Stringency::Lenient,
        ..AutoLensConfig::default()
    };

    let generated = panproto_lens::auto_generate(&post, &profile, &protocol, &config)
        .expect("a lenient lens exists from post to profile");
    let target = generated
        .chain
        .instantiate(&post, &protocol)
        .expect("the chain instantiates at the post schema")
        .tgt_schema;

    assert_eq!(
        (target.vertices.len(), target.edges.len()),
        (profile.vertices.len(), profile.edges.len()),
        "vertex and edge counts of the instantiated target against the requested one"
    );
    assert_eq!(kind_multiset(&target), kind_multiset(&profile));
    assert_eq!(edge_multiset(&target), edge_multiset(&profile));
    assert_eq!(generated.lens.tgt_schema.vertices.len(), profile.vertices.len());
}
