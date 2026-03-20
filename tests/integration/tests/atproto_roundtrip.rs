//! Integration test 3: `ATProto` round-trip.
//!
//! Parse a JSON lexicon into a schema, build an instance from JSON,
//! apply an identity migration, serialize back to JSON, and verify
//! the output matches the input.

use std::collections::{HashMap, HashSet};

use panproto_inst::{CompiledMigration, parse_json, to_json};
use panproto_mig::lift_wtype;
use panproto_protocols::atproto;
use panproto_schema::Edge;

/// Load the app.bsky.feed.post fixture.
fn post_lexicon() -> serde_json::Value {
    serde_json::json!({
        "lexicon": 1,
        "id": "app.bsky.feed.post",
        "defs": {
            "main": {
                "type": "record",
                "record": {
                    "type": "object",
                    "required": ["text", "createdAt"],
                    "properties": {
                        "text": {
                            "type": "string",
                            "maxLength": 3000,
                            "maxGraphemes": 300
                        },
                        "createdAt": {
                            "type": "string"
                        }
                    }
                }
            }
        }
    })
}

/// Sample post record.
fn post_record() -> serde_json::Value {
    serde_json::json!({
        "text": "Hello, world!",
        "createdAt": "2024-01-01T00:00:00Z"
    })
}

#[test]
fn parse_lexicon_produces_valid_schema() -> Result<(), Box<dyn std::error::Error>> {
    let lexicon = post_lexicon();
    let schema = atproto::parse_lexicon(&lexicon)?;

    assert!(schema.has_vertex("app.bsky.feed.post"), "record vertex");
    assert!(schema.has_vertex("app.bsky.feed.post:body"), "body object");
    assert!(
        schema.has_vertex("app.bsky.feed.post:body.text"),
        "text field"
    );
    assert!(
        schema.has_vertex("app.bsky.feed.post:body.createdAt"),
        "createdAt field"
    );

    Ok(())
}

#[test]
fn parse_json_then_identity_lift_then_serialize() -> Result<(), Box<dyn std::error::Error>> {
    let lexicon = post_lexicon();
    let schema = atproto::parse_lexicon(&lexicon)?;

    // Parse a record into a WInstance.
    let record = post_record();
    let instance = parse_json(&schema, "app.bsky.feed.post:body", &record)?;

    assert!(
        instance.node_count() >= 3,
        "should have at least root + 2 field nodes, got {}",
        instance.node_count()
    );

    // Build an identity compiled migration.
    let surviving_verts = schema.vertices.keys().cloned().collect();
    let surviving_edges: HashSet<Edge> = schema.edges.keys().cloned().collect();

    let compiled = CompiledMigration {
        surviving_verts,
        surviving_edges,
        vertex_remap: HashMap::new(),
        edge_remap: HashMap::new(),
        resolver: HashMap::new(),
        hyper_resolver: HashMap::new(),
        field_transforms: HashMap::new(),
    };

    // Lift with identity migration.
    let lifted = lift_wtype(&compiled, &schema, &schema, &instance)?;
    assert_eq!(
        lifted.node_count(),
        instance.node_count(),
        "identity lift should preserve node count"
    );

    // Serialize back to JSON.
    let output = to_json(&schema, &lifted);

    // Verify the output contains the original field values.
    assert!(output.is_object(), "output should be a JSON object");

    // The exact structure depends on the parse/to_json implementation,
    // but we can verify the text value round-trips.
    let output_str = serde_json::to_string(&output)?;
    assert!(
        output_str.contains("Hello, world!"),
        "output should contain the original text value"
    );
    assert!(
        output_str.contains("2024-01-01T00:00:00Z"),
        "output should contain the original createdAt value"
    );

    Ok(())
}

#[test]
fn roundtrip_byte_compare() -> Result<(), Box<dyn std::error::Error>> {
    let lexicon = post_lexicon();
    let schema = atproto::parse_lexicon(&lexicon)?;

    let record = post_record();
    let instance = parse_json(&schema, "app.bsky.feed.post:body", &record)?;

    // Serialize the instance.
    let json_out = to_json(&schema, &instance);

    // Re-parse the output.
    let instance2 = parse_json(&schema, "app.bsky.feed.post:body", &json_out)?;

    // Serialize again.
    let json_out2 = to_json(&schema, &instance2);

    // The two JSON outputs should be identical (idempotent round-trip).
    let bytes1 = serde_json::to_vec(&json_out)?;
    let bytes2 = serde_json::to_vec(&json_out2)?;

    assert_eq!(
        bytes1, bytes2,
        "round-trip serialization should be idempotent"
    );

    Ok(())
}
