//! Anchor-seed integration test for two atproto lexicons whose records
//! sit under disjoint NSIDs but share user-visible prop names.
//!
//! The lexicons `app.bsky.feed.post` and `site.standard.document` each
//! expose a `tags` prop (identical `array<string>`) and a `labels` prop
//! (byte-identical `union refs=['com.atproto.label.defs#selfLabels']`).
//! `parse_lexicon` names each prop vertex as `{record_id}:body.{prop}`,
//! so the full IDs share no tokens and carry disjoint NSID prefixes.
//! At `Stringency::Balanced`, exact, alias, and identifier-token
//! strategies therefore produce zero anchors on this pair.
//!
//! This test confirms that `suffix_anchors` recovers both shared-name
//! pairs at confidence 1.0 under `StrategyTag::ExactSuffix`, so that the
//! CSP has something non-trivial to anchor on.

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

    // Confirm the input shape before asserting on the strategy output:
    // `parse_lexicon` names prop vertices `{record_id}:body.{prop}`.
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

    // `tags ↔ tags`: identical `array<string>` on both sides, anchored
    // at confidence 1.0 by the suffix strategy.
    assert!(
        pairs.contains(&(
            "app.bsky.feed.post:body.tags".into(),
            "site.standard.document:body.tags".into(),
        )),
        "expected suffix-anchored `tags` pair in {pairs:#?}",
    );

    // `labels ↔ labels`: byte-identical `union` on both sides.
    assert!(
        pairs.contains(&(
            "app.bsky.feed.post:body.labels".into(),
            "site.standard.document:body.labels".into(),
        )),
        "expected suffix-anchored `labels` pair in {pairs:#?}",
    );

    // Both shared-named pairs must have been emitted as ExactSuffix
    // anchors (not by some other strategy inadvertently hitting them).
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
