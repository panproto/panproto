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

/// Real posts with an embed, one per kind the committed lexicon's `embed`
/// union names, and one whose embed kind it does not name.
const EMBED_POSTS: [(&str, &[u8]); 6] = [
    (
        "post-embed-images",
        include_bytes!("../../../fixtures/atproto/records/post-embed-images.json"),
    ),
    (
        "post-embed-external",
        include_bytes!("../../../fixtures/atproto/records/post-embed-external.json"),
    ),
    (
        "post-embed-record",
        include_bytes!("../../../fixtures/atproto/records/post-embed-record.json"),
    ),
    (
        "post-embed-record-with-media",
        include_bytes!("../../../fixtures/atproto/records/post-embed-record-with-media.json"),
    ),
    (
        "post-embed-video",
        include_bytes!("../../../fixtures/atproto/records/post-embed-video.json"),
    ),
    (
        "post-embed-gallery",
        include_bytes!("../../../fixtures/atproto/records/post-embed-gallery.json"),
    ),
];

/// Read `record` through the codec and through the schema parse, check
/// the two readings agree and validate, and that both write the record
/// back.
fn reads_validates_and_writes_back(
    name: &str,
    schema: &Schema,
    root: &str,
    record: &serde_json::Value,
) -> TestResult {
    let protocol = atproto::protocol();
    let registry = panproto_core::io::default_registry();
    let bytes = serde_json::to_vec(record)?;
    let through_codec = registry.parse_wtype("atproto", schema, &bytes)?;
    let through_schema = panproto_core::inst::parse_json(schema, &protocol, root, record)?;
    assert_eq!(
        serde_json::to_value(&through_codec)?,
        serde_json::to_value(&through_schema)?,
        "{name}"
    );
    let report = validate_wtype(&through_codec, schema, &protocol);
    assert_eq!(
        report.status,
        CheckStatus::Passed,
        "{name}: {:?} {:?}",
        report.diagnostics,
        report.incomplete
    );
    let emitted: serde_json::Value =
        serde_json::from_slice(&registry.emit_wtype("atproto", schema, &through_codec)?)?;
    assert_eq!(&emitted, record, "{name}");
    assert_eq!(
        &panproto_core::inst::to_json(schema, &through_schema),
        record,
        "{name}"
    );
    Ok(())
}

/// A post's `embed` is an open union. Each embed is read through the arm
/// its `$type` names, an embed kind the lexicon does not name through the
/// open union's catch-all arm, and every post validates and writes back.
#[test]
fn real_posts_with_embeds_validate_and_write_back() -> TestResult {
    let schema = feed_post()?;
    for (name, bytes) in EMBED_POSTS {
        let record: serde_json::Value = serde_json::from_slice(bytes)?;
        reads_validates_and_writes_back(name, &schema, "app.bsky.feed.post", &record)?;
    }
    let gallery: serde_json::Value = serde_json::from_slice(EMBED_POSTS[5].1)?;
    let instance = panproto_core::inst::parse_json(
        &schema,
        &atproto::protocol(),
        "app.bsky.feed.post",
        &gallery,
    )?;
    assert!(
        instance
            .nodes
            .values()
            .any(|node| node.anchor.as_ref() == "app.bsky.feed.post:body.embed:open"),
        "an embed kind the union does not name is read by its open arm"
    );
    Ok(())
}

/// A lexicon with an open union of an in-document object and a closed
/// union of the same object.
fn union_lexicon() -> Result<Schema, Box<dyn Error>> {
    Ok(atproto::parse_lexicon(&serde_json::json!({
        "lexicon": 1,
        "id": "com.example.pick",
        "defs": {
            "main": {"type": "record", "key": "tid", "record": {
                "type": "object",
                "properties": {
                    "open": {"type": "union", "refs": ["#local"]},
                    "shut": {"type": "union", "refs": ["#local"], "closed": true}
                }
            }},
            "local": {
                "type": "object",
                "required": ["n"],
                "properties": {"n": {"type": "integer"}}
            }
        }
    }))?)
}

/// An in-document arm is read as the object it refers to, at either kind
/// of union.
#[test]
fn an_in_document_arm_reads_as_its_object() -> TestResult {
    let schema = union_lexicon()?;
    for field in ["open", "shut"] {
        let record = serde_json::json!({
            "$type": "com.example.pick",
            field: {"$type": "com.example.pick#local", "n": 3}
        });
        reads_validates_and_writes_back(field, &schema, "com.example.pick", &record)?;
        let instance = panproto_core::inst::parse_json(
            &schema,
            &atproto::protocol(),
            "com.example.pick",
            &record,
        )?;
        assert!(
            instance
                .nodes
                .values()
                .any(|node| node.anchor.as_ref() == "com.example.pick#local"),
            "{field}"
        );
    }
    Ok(())
}

/// A bundled union resolves both the bare NSID of a `main` object and a
/// named definition, including when the union is an array's item type.
#[test]
fn bundled_union_items_resolve_to_their_declared_objects() -> TestResult {
    let schema = atproto::parse_lexicon_bundle(&[
        serde_json::json!({
            "lexicon": 1,
            "id": "com.example.pick",
            "defs": {"main": {"type": "record", "key": "tid", "record": {
                "type": "object",
                "required": ["items"],
                "properties": {"items": {"type": "array", "items": {
                    "type": "union",
                    "refs": ["com.example.arm#main", "com.example.arm#other"],
                    "closed": true
                }}}
            }}}
        }),
        serde_json::json!({
            "lexicon": 1,
            "id": "com.example.arm",
            "defs": {
                "main": {"type": "object", "required": ["n"],
                    "properties": {"n": {"type": "integer", "minimum": 0}}},
                "other": {"type": "object", "required": ["text"],
                    "properties": {"text": {"type": "string"}}}
            }
        }),
    ])?;
    let record = serde_json::json!({
        "$type": "com.example.pick",
        "items": [
            {"$type": "com.example.arm", "n": 3},
            {"$type": "com.example.arm#other", "text": "named arm"}
        ]
    });
    reads_validates_and_writes_back("bundled items", &schema, "com.example.pick", &record)?;
    let registry = panproto_core::io::default_registry();
    let instance = registry.parse_wtype("atproto", &schema, &serde_json::to_vec(&record)?)?;
    for anchor in ["com.example.arm", "com.example.arm#other"] {
        assert!(
            instance
                .nodes
                .values()
                .any(|node| node.anchor.as_ref() == anchor),
            "the selected union arm must anchor at its resolved object {anchor}"
        );
    }
    let mut invalid = record;
    invalid["items"][0]["n"] = serde_json::json!(-1);
    let instance = registry.parse_wtype("atproto", &schema, &serde_json::to_vec(&invalid)?)?;
    assert_eq!(
        validate_wtype(&instance, &schema, &atproto::protocol()).status,
        CheckStatus::Failed,
        "the selected object's field constraints must be checked"
    );
    Ok(())
}

/// A closed union refuses a `$type` none of its refs names, and every
/// union refuses a value with no `$type`, as an object without one or a
/// value that is not an object. Both readers refuse alike.
#[test]
fn a_value_no_arm_reads_is_refused() -> TestResult {
    let schema = union_lexicon()?;
    let protocol = atproto::protocol();
    let registry = panproto_core::io::default_registry();
    for (field, value, found) in [
        (
            "shut",
            serde_json::json!({"$type": "com.example.other", "n": 3}),
            Some("com.example.other"),
        ),
        ("shut", serde_json::json!({"n": 3}), None),
        ("open", serde_json::json!({"n": 3}), None),
        ("open", serde_json::json!(3), None),
    ] {
        let record = serde_json::json!({"$type": "com.example.pick", field: value});
        let union = format!("com.example.pick:body.{field}");
        match registry.parse_wtype("atproto", &schema, &serde_json::to_vec(&record)?) {
            Err(panproto_core::io::ParseInstanceError::NoUnionVariant {
                union: refused,
                found: seen,
                ..
            }) => {
                assert_eq!(refused, union, "{record}");
                assert_eq!(seen.as_deref(), found, "{record}");
            }
            other => panic!("{record}: the codec read {other:?}"),
        }
        match panproto_core::inst::parse_json(&schema, &protocol, "com.example.pick", &record) {
            Err(panproto_core::inst::ParseError::NoUnionVariant {
                union: refused,
                found: seen,
                ..
            }) => {
                assert_eq!(refused, union, "{record}");
                assert_eq!(seen.as_deref(), found, "{record}");
            }
            other => panic!("{record}: the schema parse read {other:?}"),
        }
    }
    // The open union reads an unnamed `$type` whole through its open arm.
    let record = serde_json::json!({
        "$type": "com.example.pick",
        "open": {"$type": "com.example.other", "n": 3}
    });
    reads_validates_and_writes_back("open", &schema, "com.example.pick", &record)
}

/// The codec writes a union value only where its reader reads it back: a
/// value at the open arm whose `$type` names a tagged arm, or that carries
/// no `$type`, has no faithful form and is refused.
#[test]
fn a_value_its_union_would_read_elsewhere_is_not_written() -> TestResult {
    use panproto_core::inst::{FieldPresence, Value};
    let schema = feed_post()?;
    let registry = panproto_core::io::default_registry();
    let instance = registry.parse_wtype("atproto", &schema, EMBED_POSTS[5].1)?;
    let (open, fields) = instance
        .nodes
        .iter()
        .find_map(|(id, node)| match &node.value {
            Some(FieldPresence::Present(Value::Unknown(fields)))
                if node.anchor.as_ref() == "app.bsky.feed.post:body.embed:open" =>
            {
                Some((*id, fields.clone()))
            }
            _ => None,
        })
        .ok_or("the gallery embed is read at the open arm")?;
    for tag in [Some("app.bsky.embed.images"), None] {
        let mut moved = instance.clone();
        let mut value = fields.clone();
        match tag {
            Some(tag) => value.insert("$type".to_owned(), Value::Str(tag.to_owned())),
            None => value.remove("$type"),
        };
        if let Some(node) = moved.nodes.get_mut(&open) {
            node.value = Some(FieldPresence::Present(Value::Unknown(value)));
        }
        assert!(
            registry.emit_wtype("atproto", &schema, &moved).is_err(),
            "{tag:?}"
        );
    }
    Ok(())
}
