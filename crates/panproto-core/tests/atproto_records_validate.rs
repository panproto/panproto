//! Real Bluesky post records validate under their Lexicon and lift.
//!
//! `app.bsky.feed.post` declares `format: datetime` on `createdAt` and
//! `format: language` on the items of `langs`, so a post validates as
//! `Passed` only when every format it declares is evaluated. The records
//! are the committed fixtures `fixtures/atproto/records/post-*.json`.

use std::error::Error;

use panproto_core::gat::{CheckStatus, Name};
use panproto_core::inst::BoundaryProtocols;
use panproto_core::inst::validation::validate_wtype;
use panproto_core::mig::{Migration, compile, lift_wtype};
use panproto_core::protocols::web_document::atproto;
use panproto_core::schema::{Edge, Schema};

type TestResult = Result<(), Box<dyn Error>>;

const FEED_POST_LEXICON: &str =
    include_str!("../../../fixtures/atproto/lexicons/app.bsky.feed.post.json");

const POSTS: [(&str, &[u8]); 5] = [
    (
        "post-0",
        include_bytes!("../../../fixtures/atproto/records/post-0.json"),
    ),
    (
        "post-1",
        include_bytes!("../../../fixtures/atproto/records/post-1.json"),
    ),
    (
        "post-2",
        include_bytes!("../../../fixtures/atproto/records/post-2.json"),
    ),
    (
        "post-3",
        include_bytes!("../../../fixtures/atproto/records/post-3.json"),
    ),
    (
        "post-4",
        include_bytes!("../../../fixtures/atproto/records/post-4.json"),
    ),
];

fn feed_post() -> Result<Schema, Box<dyn Error>> {
    let lexicon: serde_json::Value = serde_json::from_str(FEED_POST_LEXICON)?;
    Ok(atproto::parse_lexicon(&lexicon)?)
}

#[test]
fn every_committed_post_carries_langs() -> TestResult {
    for (name, bytes) in POSTS {
        let record: serde_json::Value = serde_json::from_slice(bytes)?;
        assert!(
            record["langs"].as_array().is_some_and(|l| !l.is_empty()),
            "{name} has no langs"
        );
    }
    Ok(())
}

#[test]
fn real_posts_with_langs_validate_and_lift() -> TestResult {
    let schema = feed_post()?;
    let protocol = atproto::protocol();
    let registry = panproto_core::io::default_registry();
    let vertices: Vec<Name> = schema.vertices.keys().cloned().collect();
    let edges: Vec<Edge> = schema.edges.keys().cloned().collect();
    let compiled = compile(&schema, &schema, &Migration::identity(&vertices, &edges))?;
    for (name, bytes) in POSTS {
        let instance = registry.parse_wtype("atproto", &schema, bytes)?;
        let report = validate_wtype(&instance, &schema, &protocol);
        assert_eq!(
            report.status,
            CheckStatus::Passed,
            "{name}: {:?} {:?}",
            report.diagnostics,
            report.incomplete
        );
        let lifted = lift_wtype(
            &compiled,
            &schema,
            &schema,
            &instance,
            BoundaryProtocols {
                src: &protocol,
                tgt: &protocol,
            },
        )
        .map_err(|e| format!("{name}: {e}"))?;
        let emitted = registry.emit_wtype("atproto", &schema, &lifted)?;
        let reread: serde_json::Value = serde_json::from_slice(&emitted)?;
        let original: serde_json::Value = serde_json::from_slice(bytes)?;
        assert_eq!(reread["langs"], original["langs"], "{name}");
    }
    Ok(())
}

/// A post whose language tag is not well-formed fails validation on that
/// value, and lift refuses it at the input boundary.
#[test]
fn a_post_with_a_malformed_language_tag_fails_and_is_refused() -> TestResult {
    let schema = feed_post()?;
    let protocol = atproto::protocol();
    let registry = panproto_core::io::default_registry();
    let mut record: serde_json::Value = serde_json::from_slice(POSTS[0].1)?;
    record["langs"] = serde_json::json!(["en_US"]);
    let bytes = serde_json::to_vec(&record)?;
    let instance = registry.parse_wtype("atproto", &schema, &bytes)?;
    let report = validate_wtype(&instance, &schema, &protocol);
    assert_eq!(report.status, CheckStatus::Failed, "{report:?}");
    assert!(report.incomplete.is_empty(), "{:?}", report.incomplete);
    assert_eq!(report.diagnostics.len(), 1, "{:?}", report.diagnostics);
    let vertices: Vec<Name> = schema.vertices.keys().cloned().collect();
    let edges: Vec<Edge> = schema.edges.keys().cloned().collect();
    let compiled = compile(&schema, &schema, &Migration::identity(&vertices, &edges))?;
    assert!(
        lift_wtype(
            &compiled,
            &schema,
            &schema,
            &instance,
            BoundaryProtocols {
                src: &protocol,
                tgt: &protocol,
            },
        )
        .is_err()
    );
    Ok(())
}
