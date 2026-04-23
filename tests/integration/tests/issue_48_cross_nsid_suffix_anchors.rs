//! Regression test for panproto/panproto#48.
//!
//! Reproduces the reported failure on real atproto lexicons that share
//! user-visible prop names but live under disjoint NSIDs. Before the
//! fix, `auto_generate_candidates` at `Stringency::Balanced` seeded
//! zero anchors between `app.bsky.feed.post` and `site.standard.document`
//! and fell back to the degenerate all-`DropOp` chain, because every
//! alignment strategy at Balanced keyed on vertex IDs (which are
//! fully-qualified as `{object}.{prop}` by `parse_lexicon`) and none
//! recovered the obvious `tags ↔ tags` and `labels ↔ labels` matches.
//! After the fix, the new suffix strategy emits those pairs with
//! confidence 1.0, the CSP validates them against the naturality
//! check, and the resulting morphism aligns the shared props.

#![allow(clippy::expect_used)]
#![allow(clippy::unwrap_used)]

use panproto_lens::auto_lens::{AutoLensConfig, Stringency, run_strategies_for_tests};
use panproto_protocols::web_document::atproto::parse_lexicon;

const POST_LEXICON: &str =
    include_str!("../../../fixtures/atproto/lexicons/app.bsky.feed.post.json");
const DOCUMENT_LEXICON: &str =
    include_str!("../../../fixtures/atproto/lexicons/site.standard.document.json");

#[test]
fn suffix_strategy_seeds_cross_nsid_tags_and_labels_at_balanced() {
    let post_json: serde_json::Value =
        serde_json::from_str(POST_LEXICON).expect("post lexicon parses as JSON");
    let doc_json: serde_json::Value =
        serde_json::from_str(DOCUMENT_LEXICON).expect("document lexicon parses as JSON");

    let src = parse_lexicon(&post_json).expect("app.bsky.feed.post parses");
    let tgt = parse_lexicon(&doc_json).expect("site.standard.document parses");

    // Vertex IDs as the issue cites them: `{record_id}.{prop_name}`.
    assert!(
        src.vertices
            .keys()
            .any(|k| k.as_str() == "app.bsky.feed.post:body.tags"),
        "expected source to carry the NSID-qualified `tags` vertex",
    );
    assert!(
        tgt.vertices
            .keys()
            .any(|k| k.as_str() == "site.standard.document:body.tags"),
        "expected target to carry the NSID-qualified `tags` vertex",
    );

    let cfg = AutoLensConfig {
        stringency: Stringency::Balanced,
        try_overlap: true,
        ..AutoLensConfig::default()
    };

    let (anchors, _) = run_strategies_for_tests(&src, &tgt, &cfg);
    let pairs: Vec<(String, String)> = anchors
        .iter()
        .map(|a| (a.src.as_str().to_owned(), a.tgt.as_str().to_owned()))
        .collect();

    // `tags ↔ tags`: identical `array<string>` on both sides, caught
    // at confidence 1.0 by the suffix strategy.
    assert!(
        pairs.contains(&(
            "app.bsky.feed.post:body.tags".into(),
            "site.standard.document:body.tags".into(),
        )),
        "expected suffix-anchored `tags` pair in {pairs:#?}",
    );

    // `labels ↔ labels`: byte-identical `union refs=['com.atproto.label.defs#selfLabels']`
    // on both sides.
    assert!(
        pairs.contains(&(
            "app.bsky.feed.post:body.labels".into(),
            "site.standard.document:body.labels".into(),
        )),
        "expected suffix-anchored `labels` pair in {pairs:#?}",
    );

    // Before the fix, zero anchors were seeded; verify at least the
    // two shared-named pairs now appear.
    let shared = ["tags", "labels"];
    let seeded_tails: Vec<&str> = anchors
        .iter()
        .filter(|a| a.strategy == panproto_mig::align::StrategyTag::ExactSuffix)
        .map(|a| {
            a.src
                .as_str()
                .rsplit_once('.')
                .map_or(a.src.as_str(), |(_, t)| t)
        })
        .collect();
    for tail in shared {
        assert!(
            seeded_tails.contains(&tail),
            "expected ExactSuffix anchor with tail `{tail}` in {seeded_tails:?}",
        );
    }
}
