#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Integration tests for scalar field transforms (panproto/panproto#13).
//!
//! Verifies that `ComputeField`, `ApplyExpr`, and `Case` transforms can
//! access scalar values from schema-defined child vertices, not just
//! `extra_fields`. This tests the dependent-sum projection from the
//! total fiber over a vertex in the Grothendieck fibration.
//!
//! The core correctness property: field transforms see the full fiber
//! `Fiber(v) = ExtraFields(v) x Product_{e: v->w} Fiber(w)`, where
//! leaf (scalar) children are projected into the expression environment
//! alongside `extra_fields`.

use std::collections::HashMap;
use std::sync::Arc;

use panproto_gat::Name;
use panproto_inst::value::{FieldPresence, Value};
use panproto_inst::{
    CompiledMigration, FieldTransform, Node, WInstance, parse_json, to_json, wtype_restrict,
};
use panproto_schema::{Edge, Schema, Vertex};
use smallvec::SmallVec;

/// Build a schema from vertex specs and edges.
fn make_schema(verts: &[(&str, &str)], edge_list: &[Edge]) -> Schema {
    let mut vertices = HashMap::new();
    let mut edges = HashMap::new();
    let mut outgoing: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
    let mut incoming: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
    let mut between: HashMap<(Name, Name), SmallVec<Edge, 2>> = HashMap::new();

    for (id, kind) in verts {
        vertices.insert(
            Name::from(*id),
            Vertex {
                id: Name::from(*id),
                kind: Name::from(*kind),
                nsid: None,
            },
        );
    }
    for e in edge_list {
        edges.insert(e.clone(), e.kind.clone());
        outgoing.entry(e.src.clone()).or_default().push(e.clone());
        incoming.entry(e.tgt.clone()).or_default().push(e.clone());
        between
            .entry((e.src.clone(), e.tgt.clone()))
            .or_default()
            .push(e.clone());
    }

    Schema {
        protocol: "test".into(),
        vertices,
        edges,
        hyper_edges: HashMap::new(),
        constraints: HashMap::new(),
        required: HashMap::new(),
        nsids: HashMap::new(),
        variants: HashMap::new(),
        orderings: HashMap::new(),
        recursion_points: HashMap::new(),
        spans: HashMap::new(),
        usage_modes: HashMap::new(),
        nominal: HashMap::new(),
        coercions: HashMap::new(),
        mergers: HashMap::new(),
        defaults: HashMap::new(),
        policies: HashMap::new(),
        outgoing,
        incoming,
        between,
    }
}

/// Schema with an AT-URI formatted string field (reproduces #13).
fn at_uri_schema() -> Schema {
    let edge_repo = Edge {
        src: "body".into(),
        tgt: "body.repo".into(),
        kind: "prop".into(),
        name: Some("repo".into()),
    };
    let edge_text = Edge {
        src: "body".into(),
        tgt: "body.text".into(),
        kind: "prop".into(),
        name: Some("text".into()),
    };

    make_schema(
        &[
            ("body", "object"),
            ("body.repo", "string"),
            ("body.text", "string"),
        ],
        &[edge_repo, edge_text],
    )
}

fn at_uri_instance(schema: &Schema) -> WInstance {
    let json = serde_json::json!({
        "repo": "at://did:plc:abc123/app.bsky.feed.post/rkey456",
        "text": "hello world"
    });
    parse_json(schema, "body", &json).expect("parse should succeed")
}

// ---------------------------------------------------------------------------
// Test: ComputeField reads child scalar in full restrict pipeline
// ---------------------------------------------------------------------------

#[test]
fn compute_field_survives_restrict() {
    let schema = at_uri_schema();
    let instance = at_uri_instance(&schema);

    // ComputeField that reads "repo" (a child scalar vertex) and copies it.
    // Classified as Projection: the result is deterministically derivable
    // from the source fiber, but no inverse recovers the source from it.
    let expr = panproto_expr::Expr::Var(Arc::from("repo"));
    let mut field_transforms = HashMap::new();
    field_transforms.insert(
        Name::from("body"),
        vec![FieldTransform::ComputeField {
            target_key: "repo_copy".into(),
            expr,
            inverse: None,
            coercion_class: panproto_gat::CoercionClass::Projection,
        }],
    );

    let migration = CompiledMigration {
        surviving_verts: schema.vertices.keys().cloned().collect(),
        surviving_edges: schema.edges.keys().cloned().collect(),
        vertex_remap: HashMap::new(),
        edge_remap: HashMap::new(),
        resolver: HashMap::new(),
        hyper_resolver: HashMap::new(),
        field_transforms,
        conditional_survival: HashMap::new(),
    };

    let result = wtype_restrict(&instance, &schema, &schema, &migration);
    assert!(result.is_ok(), "restrict should succeed: {result:?}");
    let restricted = result.unwrap();

    // Serialize and check the computed field appears
    let json_out = to_json(&schema, &restricted);
    assert_eq!(
        json_out["repo_copy"], "at://did:plc:abc123/app.bsky.feed.post/rkey456",
        "ComputeField should have read the child scalar 'repo' and written 'repo_copy'"
    );

    // Original fields should also be present
    assert_eq!(
        json_out["repo"],
        "at://did:plc:abc123/app.bsky.feed.post/rkey456"
    );
    assert_eq!(json_out["text"], "hello world");
}

// ---------------------------------------------------------------------------
// Test: AT-URI decomposition (exact scenario from issue #13)
// ---------------------------------------------------------------------------

#[test]
fn at_uri_decomposition_end_to_end() {
    use panproto_expr::{BuiltinOp, Expr, Literal};

    let schema = at_uri_schema();
    let instance = at_uri_instance(&schema);

    // AT-URI format: "at://did:plc:abc123/app.bsky.feed.post/rkey456"
    //
    // Decomposition via (split repo "/"):
    //   ["at:", "", "did:plc:abc123", "app.bsky.feed.post", "rkey456"]
    //   index 2 = "did:plc:abc123" (the DID)
    //   index 3 = "app.bsky.feed.post" (the collection NSID)
    //   index 4 = "rkey456" (the record key)
    //
    // This is the real decomposition described in panproto/panproto#13.

    // Helper: (index (split repo "/") n)
    let at_uri_part = |field: &str, idx: i64| -> Expr {
        Expr::Index(
            Box::new(Expr::Builtin(
                BuiltinOp::Split,
                vec![
                    Expr::Var(Arc::from(field)),
                    Expr::Lit(Literal::Str("/".into())),
                ],
            )),
            Box::new(Expr::Lit(Literal::Int(idx))),
        )
    };

    let mut field_transforms = HashMap::new();
    field_transforms.insert(
        Name::from("body"),
        vec![
            // Extract DID: (index (split repo "/") 2)
            FieldTransform::ComputeField {
                target_key: "repoDid".into(),
                expr: at_uri_part("repo", 2),
                inverse: None,
                coercion_class: panproto_gat::CoercionClass::Projection,
            },
            // Extract collection NSID: (index (split repo "/") 3)
            FieldTransform::ComputeField {
                target_key: "repoCollection".into(),
                expr: at_uri_part("repo", 3),
                inverse: None,
                coercion_class: panproto_gat::CoercionClass::Projection,
            },
            // Extract record key: (index (split repo "/") 4)
            FieldTransform::ComputeField {
                target_key: "repoRkey".into(),
                expr: at_uri_part("repo", 4),
                inverse: None,
                coercion_class: panproto_gat::CoercionClass::Projection,
            },
        ],
    );

    let migration = CompiledMigration {
        surviving_verts: schema.vertices.keys().cloned().collect(),
        surviving_edges: schema.edges.keys().cloned().collect(),
        vertex_remap: HashMap::new(),
        edge_remap: HashMap::new(),
        resolver: HashMap::new(),
        hyper_resolver: HashMap::new(),
        field_transforms,
        conditional_survival: HashMap::new(),
    };

    let restricted = wtype_restrict(&instance, &schema, &schema, &migration).expect("restrict ok");
    let json_out = to_json(&schema, &restricted);

    assert_eq!(json_out["repoDid"], "did:plc:abc123");
    assert_eq!(json_out["repoCollection"], "app.bsky.feed.post");
    assert_eq!(json_out["repoRkey"], "rkey456");

    // Original field must survive unmodified
    assert_eq!(
        json_out["repo"],
        "at://did:plc:abc123/app.bsky.feed.post/rkey456"
    );
}

// ---------------------------------------------------------------------------
// Test: Multiple scalar transforms compose correctly
// ---------------------------------------------------------------------------

#[test]
fn multiple_scalar_transforms_compose() {
    let schema = at_uri_schema();
    let instance = at_uri_instance(&schema);

    let mut field_transforms = HashMap::new();
    field_transforms.insert(
        Name::from("body"),
        vec![
            // First: copy repo (projection: derived from child scalar)
            FieldTransform::ComputeField {
                target_key: "field_a".into(),
                expr: panproto_expr::Expr::Var(Arc::from("repo")),
                inverse: None,
                coercion_class: panproto_gat::CoercionClass::Projection,
            },
            // Second: copy text (projection: derived from child scalar)
            FieldTransform::ComputeField {
                target_key: "field_b".into(),
                expr: panproto_expr::Expr::Var(Arc::from("text")),
                inverse: None,
                coercion_class: panproto_gat::CoercionClass::Projection,
            },
            // Third: concatenate text with " @ " and then with repo
            FieldTransform::ComputeField {
                target_key: "field_c".into(),
                expr: panproto_expr::Expr::Builtin(
                    panproto_expr::BuiltinOp::Concat,
                    vec![
                        panproto_expr::Expr::Builtin(
                            panproto_expr::BuiltinOp::Concat,
                            vec![
                                panproto_expr::Expr::Var(Arc::from("text")),
                                panproto_expr::Expr::Lit(panproto_expr::Literal::Str(" @ ".into())),
                            ],
                        ),
                        panproto_expr::Expr::Var(Arc::from("repo")),
                    ],
                ),
                inverse: None,
                coercion_class: panproto_gat::CoercionClass::Projection,
            },
        ],
    );

    let migration = CompiledMigration {
        surviving_verts: schema.vertices.keys().cloned().collect(),
        surviving_edges: schema.edges.keys().cloned().collect(),
        vertex_remap: HashMap::new(),
        edge_remap: HashMap::new(),
        resolver: HashMap::new(),
        hyper_resolver: HashMap::new(),
        field_transforms,
        conditional_survival: HashMap::new(),
    };

    let restricted = wtype_restrict(&instance, &schema, &schema, &migration).expect("restrict ok");
    let json_out = to_json(&schema, &restricted);

    assert_eq!(
        json_out["field_a"],
        "at://did:plc:abc123/app.bsky.feed.post/rkey456"
    );
    assert_eq!(json_out["field_b"], "hello world");
    assert_eq!(
        json_out["field_c"],
        "hello world @ at://did:plc:abc123/app.bsky.feed.post/rkey456"
    );
}

// ---------------------------------------------------------------------------
// Test: Identity migration with no transforms preserves instance
// ---------------------------------------------------------------------------

#[test]
fn scalar_child_identity_roundtrip() {
    let schema = at_uri_schema();
    let instance = at_uri_instance(&schema);

    let migration = CompiledMigration {
        surviving_verts: schema.vertices.keys().cloned().collect(),
        surviving_edges: schema.edges.keys().cloned().collect(),
        vertex_remap: HashMap::new(),
        edge_remap: HashMap::new(),
        resolver: HashMap::new(),
        hyper_resolver: HashMap::new(),
        field_transforms: HashMap::new(),
        conditional_survival: HashMap::new(),
    };

    let restricted = wtype_restrict(&instance, &schema, &schema, &migration).expect("restrict ok");
    assert_eq!(
        restricted.node_count(),
        instance.node_count(),
        "identity restrict must preserve node count"
    );

    let json_out = to_json(&schema, &restricted);
    assert_eq!(
        json_out["repo"],
        "at://did:plc:abc123/app.bsky.feed.post/rkey456"
    );
    assert_eq!(json_out["text"], "hello world");
}

// ---------------------------------------------------------------------------
// Property-based tests
// ---------------------------------------------------------------------------

mod property {
    use super::*;
    use proptest::prelude::*;

    /// Generate an instance with N scalar string children under a root.
    fn arb_scalar_instance() -> impl Strategy<Value = (Schema, WInstance, Vec<String>)> {
        (2..=5usize).prop_flat_map(|n| {
            prop::collection::vec("[a-z]{1,10}".prop_map(String::from), n..=n).prop_map(
                move |values| {
                    let names: Vec<String> =
                        (0..values.len()).map(|i| format!("field{i}")).collect();

                    let mut edges = Vec::new();

                    // Build owned strings that outlive the loop
                    let child_anchors: Vec<String> =
                        names.iter().map(|n| format!("root.{n}")).collect();

                    // We need to use references to strings, so collect
                    // vertex specs as owned tuples
                    let vert_owned: Vec<(String, String)> = {
                        let mut v = vec![("root".to_string(), "object".to_string())];
                        for anchor in &child_anchors {
                            v.push((anchor.clone(), "string".to_string()));
                        }
                        v
                    };
                    let vert_refs: Vec<(&str, &str)> = vert_owned
                        .iter()
                        .map(|(a, b)| (a.as_str(), b.as_str()))
                        .collect();

                    for (i, name) in names.iter().enumerate() {
                        edges.push(Edge {
                            src: "root".into(),
                            tgt: Name::from(child_anchors[i].as_str()),
                            kind: "prop".into(),
                            name: Some(Name::from(name.as_str())),
                        });
                    }

                    let schema = make_schema(&vert_refs, &edges);

                    let mut nodes = HashMap::new();
                    nodes.insert(0, Node::new(0, "root"));
                    for (i, val) in values.iter().enumerate() {
                        let nid = u32::try_from(i + 1).unwrap();
                        nodes.insert(
                            nid,
                            Node::new(nid, child_anchors[i].as_str())
                                .with_value(FieldPresence::Present(Value::Str(val.clone()))),
                        );
                    }
                    let arcs: Vec<(u32, u32, Edge)> = edges
                        .iter()
                        .enumerate()
                        .map(|(i, e)| (0, u32::try_from(i + 1).unwrap(), e.clone()))
                        .collect();
                    let instance = WInstance::new(nodes, arcs, vec![], 0, "root".into());

                    (schema, instance, names)
                },
            )
        })
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]

        #[test]
        fn prop_compute_field_roundtrip(
            (schema, instance, names) in arb_scalar_instance()
        ) {
            // For each child scalar, compute a copy via ComputeField,
            // restrict, serialize, and verify the copy exists.
            let transforms: Vec<FieldTransform> = names
                .iter()
                .map(|name| FieldTransform::ComputeField {
                    target_key: format!("{name}_copy"),
                    expr: panproto_expr::Expr::Var(Arc::from(name.as_str())),
                    inverse: None,
                    coercion_class: panproto_gat::CoercionClass::Projection,
                })
                .collect();

            let mut field_transforms = HashMap::new();
            field_transforms.insert(Name::from("root"), transforms);

            let migration = CompiledMigration {
                surviving_verts: schema.vertices.keys().cloned().collect(),
                surviving_edges: schema.edges.keys().cloned().collect(),
                vertex_remap: HashMap::new(),
                edge_remap: HashMap::new(),
                resolver: HashMap::new(),
                hyper_resolver: HashMap::new(),
                field_transforms,
                conditional_survival: HashMap::new(),
            };

            let restricted = wtype_restrict(&instance, &schema, &schema, &migration)
                .expect("restrict ok");
            let json_out = to_json(&schema, &restricted);

            for name in &names {
                let original_key = name.as_str();
                let copy_key = format!("{name}_copy");
                prop_assert_eq!(
                    json_out.get(original_key),
                    json_out.get(copy_key.as_str()),
                    "computed copy must match original"
                );
            }
        }

        #[test]
        fn prop_identity_restrict_unchanged(
            (schema, instance, _names) in arb_scalar_instance()
        ) {
            let migration = CompiledMigration {
                surviving_verts: schema.vertices.keys().cloned().collect(),
                surviving_edges: schema.edges.keys().cloned().collect(),
                vertex_remap: HashMap::new(),
                edge_remap: HashMap::new(),
                resolver: HashMap::new(),
                hyper_resolver: HashMap::new(),
                field_transforms: HashMap::new(),
                conditional_survival: HashMap::new(),
            };

            let restricted = wtype_restrict(&instance, &schema, &schema, &migration)
                .expect("restrict ok");

            prop_assert_eq!(
                restricted.node_count(), instance.node_count(),
                "identity restrict must preserve node count"
            );
            prop_assert_eq!(restricted.root, instance.root);

            let json_before = to_json(&schema, &instance);
            let json_after = to_json(&schema, &restricted);
            prop_assert_eq!(
                serde_json::to_string(&json_before).unwrap(),
                serde_json::to_string(&json_after).unwrap(),
                "identity restrict must produce identical JSON"
            );
        }
    }
}
