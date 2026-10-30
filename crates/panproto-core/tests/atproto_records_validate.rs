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

/// Every committed post, read by the schema parse rather than the codec,
/// validates, and writes back as the record it was read from. `post-3`
/// and `post-4` are replies, whose `root` and `parent` refs name
/// `com.atproto.repo.strongRef`, a def in another document.
#[test]
fn real_posts_validate_through_the_schema_parse() -> TestResult {
    let schema = feed_post()?;
    let protocol = atproto::protocol();
    for (name, bytes) in POSTS {
        let record: serde_json::Value = serde_json::from_slice(bytes)?;
        let instance =
            panproto_core::inst::parse_json(&schema, &protocol, "app.bsky.feed.post", &record)?;
        let report = validate_wtype(&instance, &schema, &protocol);
        assert_eq!(
            report.status,
            CheckStatus::Passed,
            "{name}: {:?} {:?}",
            report.diagnostics,
            report.incomplete
        );
        assert_eq!(
            panproto_core::inst::to_json(&schema, &instance),
            record,
            "{name}"
        );
    }
    Ok(())
}

/// A lexicon whose record has an `unknown` field and a ref to a def in
/// another document.
fn open_record_lexicon() -> Result<Schema, Box<dyn Error>> {
    Ok(atproto::parse_lexicon(&serde_json::json!({
        "lexicon": 1,
        "id": "com.example.note",
        "defs": {"main": {"type": "record", "key": "tid", "record": {
            "type": "object",
            "properties": {
                "meta": {"type": "unknown"},
                "subject": {"type": "ref", "ref": "com.atproto.repo.strongRef"}
            }
        }}}
    }))?)
}

/// The data at an `unknown` field, and at a ref to a def no parsed
/// document defines, is one value of any shape. The codec and the
/// schema parse read it to the same instance, which validates, and the
/// codec writes it back unchanged.
#[test]
fn opaque_data_is_one_value_in_both_readers() -> TestResult {
    let schema = open_record_lexicon()?;
    assert_eq!(
        schema
            .vertex("com.atproto.repo.strongRef")
            .map(|vertex| vertex.kind.to_string()),
        Some("unknown".to_owned())
    );
    let protocol = atproto::protocol();
    let registry = panproto_core::io::default_registry();
    for record in [
        serde_json::json!({"$type": "com.example.note", "meta": {"a": 1, "b": ["x", null]}}),
        serde_json::json!({"$type": "com.example.note", "meta": [1, {"z": true}]}),
        serde_json::json!({"$type": "com.example.note", "meta": "plain"}),
        serde_json::json!({
            "$type": "com.example.note",
            "subject": {
                "uri": "at://did:plc:z72i7hdynmk6r22z27h6tvur/app.bsky.feed.post/3mjnlu7smsk2w",
                "cid": "bafyreierzikywhmelbcjl2iagn24oonrw3vq4kkhxgwwvnuztiklpczk5y"
            }
        }),
    ] {
        let bytes = serde_json::to_vec(&record)?;
        let through_codec = registry.parse_wtype("atproto", &schema, &bytes)?;
        let through_schema =
            panproto_core::inst::parse_json(&schema, &protocol, "com.example.note", &record)?;
        assert_eq!(
            serde_json::to_value(&through_codec)?,
            serde_json::to_value(&through_schema)?,
            "{record}"
        );
        let report = validate_wtype(&through_codec, &schema, &protocol);
        assert_eq!(
            report.status,
            CheckStatus::Passed,
            "{record}: {:?} {:?}",
            report.diagnostics,
            report.incomplete
        );
        let emitted: serde_json::Value =
            serde_json::from_slice(&registry.emit_wtype("atproto", &schema, &through_codec)?)?;
        assert_eq!(emitted, record);
        assert_eq!(
            panproto_core::inst::to_json(&schema, &through_schema),
            record
        );
    }
    Ok(())
}
