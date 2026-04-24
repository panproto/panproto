//! Integration test for the edge-label anchoring strategy.
//!
//! Uses the same two vendored lexicons as
//! `issue_48_cross_nsid_suffix_anchors.rs` to verify that edge-label
//! anchoring seeds shared labeled-edge targets at Balanced stringency.
//! Suffix and edge-label are complementary: suffix keys on vertex-id
//! tails, edge-label keys on (edge name, edge kind). Together they
//! cover the common case where two protocols share prop labels but
//! disagree on every enclosing identifier.

#![allow(clippy::expect_used)]
#![allow(clippy::unwrap_used)]

use panproto_lens::auto_lens::{AutoLensConfig, Stringency, run_strategies_for_tests};
use panproto_mig::align::StrategyTag;
use panproto_protocols::web_document::atproto::parse_lexicon;

const POST_LEXICON: &str =
    include_str!("../../../fixtures/atproto/lexicons/app.bsky.feed.post.json");
const DOCUMENT_LEXICON: &str =
    include_str!("../../../fixtures/atproto/lexicons/site.standard.document.json");

#[test]
fn edge_label_seeds_shared_props_at_balanced() {
    let post_json: serde_json::Value =
        serde_json::from_str(POST_LEXICON).expect("post lexicon parses as JSON");
    let doc_json: serde_json::Value =
        serde_json::from_str(DOCUMENT_LEXICON).expect("document lexicon parses as JSON");

    let src = parse_lexicon(&post_json).expect("source lexicon parses");
    let tgt = parse_lexicon(&doc_json).expect("target lexicon parses");

    let cfg = AutoLensConfig {
        stringency: Stringency::Balanced,
        try_overlap: true,
        ..AutoLensConfig::default()
    };

    let (anchors, _) = run_strategies_for_tests(&src, &tgt, &cfg);

    let edge_label_anchors: Vec<_> = anchors
        .iter()
        .filter(|a| a.strategy == StrategyTag::EdgeLabel)
        .collect();

    assert!(
        !edge_label_anchors.is_empty(),
        "edge-label strategy must seed at least one anchor between the two lexicons",
    );

    // The two shared-label props verified by the suffix regression test
    // must also be anchored by edge-label (they are reached through
    // labeled edges on each side).
    let edge_label_tails: Vec<&str> = edge_label_anchors
        .iter()
        .map(|a| {
            a.src
                .as_str()
                .rsplit_once('.')
                .map_or(a.src.as_str(), |(_, t)| t)
        })
        .collect();
    for label in ["tags", "labels"] {
        assert!(
            edge_label_tails.contains(&label),
            "expected EdgeLabel anchor reaching child `{label}` in {edge_label_tails:?}",
        );
    }
}
