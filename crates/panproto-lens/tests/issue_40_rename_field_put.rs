//! Regression test for issue #40: `rename_field` lens scrambles field
//! assignments during `put` (backward evaluation).
//!
//! Scenario: a flat user record whose fields live on the record node
//! itself as `extra_fields` (no per-field schema vertices). The forward
//! lens renames `name -> displayName`. The user edits the view's
//! `displayName` to a new value and calls `put`. The restored source
//! should have the user's edit propagated back to `name`, with the
//! other three original fields intact. Under v0.33.0/v0.34.0, the
//! current put path clobbers the view's edit with the pre-get snapshot.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::too_many_lines,
    clippy::redundant_closure_for_method_calls
)]

use panproto_inst::parse::parse_json;
use panproto_inst::to_json;
use panproto_lens::{Lens, asymmetric};
use panproto_schema::{Protocol, SchemaBuilder};

fn generic_protocol() -> Protocol {
    Protocol {
        name: "generic".into(),
        schema_theory: "ThGraph".into(),
        instance_theory: "ThWType".into(),
        edge_rules: vec![],
        obj_kinds: vec![
            "record".into(),
            "string".into(),
            "integer".into(),
            "boolean".into(),
            "array".into(),
        ],
        constraint_sorts: vec![],
        ..Protocol::default()
    }
}

#[test]
fn issue_40_rename_field_put_preserves_edit_on_record_extra_fields() {
    use panproto_gat::Name;
    use panproto_inst::{CompiledMigration, FieldTransform};
    use std::collections::{HashMap, HashSet};

    let protocol = generic_protocol();

    // Source schema: a bare `user` record with no field vertices.
    // All record fields get parsed into `extra_fields` on the user node.
    let src_schema = SchemaBuilder::new(&protocol)
        .vertex("user", "record", None::<&str>)
        .unwrap()
        .build()
        .unwrap();
    // Target schema is structurally identical at the graph level; the
    // rename happens at the FieldTransform layer (not edge labels).
    let tgt_schema = src_schema.clone();

    // Parse source JSON.
    let source_json = serde_json::json!({
        "name": "Alice Chen",
        "legacyId": 7042,
        "email": "alice@example.com",
        "joinedAt": "2023-06-15",
    });
    let instance = parse_json(&src_schema, "user", &source_json).expect("parse source");

    // Build a compiled migration with a RenameField transform on the
    // `user` vertex. This is how `auto_lens::derive_field_transforms`
    // encodes schema-level edge renames on record vertices.
    let mut field_transforms: HashMap<Name, Vec<FieldTransform>> = HashMap::new();
    field_transforms.insert(
        Name::from("user"),
        vec![FieldTransform::RenameField {
            old_key: "name".to_string(),
            new_key: "displayName".to_string(),
        }],
    );

    let mut surviving_verts = HashSet::new();
    surviving_verts.insert(Name::from("user"));

    let compiled = CompiledMigration {
        surviving_verts,
        surviving_edges: HashSet::new(),
        vertex_remap: HashMap::new(),
        edge_remap: HashMap::new(),
        resolver: HashMap::new(),
        hyper_resolver: HashMap::new(),
        field_transforms,
        conditional_survival: HashMap::new(),
        expansion_path: HashMap::new(),
    };

    let lens = Lens {
        compiled,
        src_schema: src_schema.clone(),
        tgt_schema,
    };

    // Forward get.
    let (mut view, complement) =
        asymmetric::get(&lens, &instance).expect("forward get should succeed");

    // Mutate the view: edit displayName to "Bob".
    for node in view.nodes.values_mut() {
        if node.extra_fields.contains_key("displayName") {
            node.extra_fields.insert(
                "displayName".to_string(),
                panproto_inst::value::Value::Str("Bob".into()),
            );
        }
    }

    // Put back.
    let restored = asymmetric::put(&lens, &view, &complement).expect("put should succeed");

    let restored_json = to_json(&src_schema, &restored);
    let obj = restored_json
        .as_object()
        .expect("restored root should be an object");

    assert_eq!(
        obj.get("name").and_then(|v| v.as_str()),
        Some("Bob"),
        "restored name should be the edited value"
    );
    assert_eq!(
        obj.get("legacyId").and_then(|v| v.as_i64()),
        Some(7042),
        "restored legacyId should equal 7042"
    );
    assert_eq!(
        obj.get("email").and_then(|v| v.as_str()),
        Some("alice@example.com"),
        "restored email should be preserved"
    );
    assert_eq!(
        obj.get("joinedAt").and_then(|v| v.as_str()),
        Some("2023-06-15"),
        "restored joinedAt should be preserved"
    );
}
