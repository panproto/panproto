//! `asymmetric::put` must preserve view edits when per-field values
//! live on child vertices (e.g. `user/user.name/user.legacyId/user.email`)
//! rather than on the root vertex's `extra_fields`, and when the
//! `FieldTransform::RenameField` on the root targets a key that is
//! actually an *edge label* on an outgoing child edge rather than an
//! `extra_fields` entry. The all-`extra_fields` case has its own
//! regression; this file covers the child-vertex-via-edge-label case.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::too_many_lines,
    clippy::redundant_closure_for_method_calls
)]

use panproto_gat::Name;
use panproto_inst::parse::parse_json;
use panproto_inst::to_json;
use panproto_inst::value::Value;
use panproto_inst::{CompiledMigration, FieldTransform};
use panproto_lens::protolens::combinators;
use panproto_lens::{Lens, asymmetric};
use panproto_schema::{Protocol, SchemaBuilder};
use std::collections::{HashMap, HashSet};

fn object_graph_protocol() -> Protocol {
    Protocol {
        name: "object-graph".into(),
        schema_theory: "ThGraph".into(),
        instance_theory: "ThWType".into(),
        edge_rules: vec![],
        obj_kinds: vec![
            "object".into(),
            "string".into(),
            "integer".into(),
            "boolean".into(),
            "array".into(),
        ],
        constraint_sorts: vec!["entries".into()],
        ..Protocol::default()
    }
}

fn build_user_schema(protocol: &Protocol) -> panproto_schema::Schema {
    SchemaBuilder::new(protocol)
        .vertex("user", "object", None::<&str>)
        .unwrap()
        .vertex("user.name", "string", None::<&str>)
        .unwrap()
        .vertex("user.legacyId", "integer", None::<&str>)
        .unwrap()
        .vertex("user.email", "string", None::<&str>)
        .unwrap()
        .edge("user", "user.name", "prop", Some("name"))
        .unwrap()
        .edge("user", "user.legacyId", "prop", Some("legacyId"))
        .unwrap()
        .edge("user", "user.email", "prop", Some("email"))
        .unwrap()
        .entry("user")
        .build()
        .unwrap()
}

fn find_view_node_with_edge_label(view: &panproto_inst::WInstance, label: &str) -> Option<u32> {
    for (parent, child, edge) in &view.arcs {
        if edge.name.as_deref() == Some(label) {
            let _ = parent;
            return Some(*child);
        }
    }
    None
}

/// Mirror of protolab's `final_lens`: a `ProtolensChain` with
/// rename+add+drop schema steps, plus a side-channel `FieldTransform`
/// vec installed on the user vertex after instantiation.
fn build_final_lens(src_schema: &panproto_schema::Schema, protocol: &Protocol) -> Lens {
    let chain = combinators::pipeline(vec![
        combinators::rename_field(
            Name::from("user"),
            Name::from("user.name"),
            Name::from("name"),
            Name::from("displayName"),
        ),
        combinators::add_field(
            Name::from("user"),
            Name::from("user.bio"),
            Name::from("string"),
            Value::Str(String::new()),
        ),
        combinators::remove_field(Name::from("user.legacyId")),
    ]);

    let mut lens = chain
        .instantiate(src_schema, protocol)
        .expect("instantiate");

    // Install the side-channel FieldTransforms on the user vertex, mirroring
    // protolab's `install_field_transforms` (wire_data.rs:272).
    let user = Name::from("user");
    let installed = vec![
        FieldTransform::RenameField {
            old_key: "name".to_string(),
            new_key: "displayName".to_string(),
        },
        FieldTransform::AddField {
            key: "bio".to_string(),
            value: Value::Str(String::new()),
        },
        FieldTransform::DropField {
            key: "legacyId".to_string(),
        },
    ];
    lens.compiled
        .field_transforms
        .entry(user)
        .or_default()
        .extend(installed);

    lens
}

#[test]
fn put_preserves_view_edit_with_per_field_child_vertices() {
    let protocol = object_graph_protocol();
    let src_schema = build_user_schema(&protocol);

    let source_json = serde_json::json!({
        "name": "Dave",
        "legacyId": 1,
        "email": "d@e.com",
        "joinedAt": "2025-01-01",
    });
    let instance = parse_json(&src_schema, "user", &source_json).expect("parse source");

    let lens = build_final_lens(&src_schema, &protocol);

    // Forward.
    let (mut view, complement) = asymmetric::get(&lens, &instance).expect("forward get");

    // The view should now serialize to something with `displayName`.
    // Find the child node that is the target of the edge labelled
    // `displayName` and edit its value from "Dave" to "EDITED".
    let display_node_id =
        find_view_node_with_edge_label(&view, "displayName").expect("displayName edge exists");
    {
        let node = view
            .nodes
            .get_mut(&display_node_id)
            .expect("display node exists");
        node.value = Some(panproto_inst::value::FieldPresence::Present(Value::Str(
            "EDITED".into(),
        )));
    }

    // Put back.
    let restored = asymmetric::put(&lens, &view, &complement).expect("put back");

    let restored_json = to_json(&src_schema, &restored);
    let obj = restored_json.as_object().expect("restored root is object");

    assert_eq!(
        obj.get("name").and_then(|v| v.as_str()),
        Some("EDITED"),
        "restored name should be the edited value; got {obj:?}"
    );
    assert_eq!(
        obj.get("email").and_then(|v| v.as_str()),
        Some("d@e.com"),
        "email preserved"
    );
    assert_eq!(
        obj.get("legacyId").and_then(|v| v.as_i64()),
        Some(1),
        "legacyId restored from complement"
    );
    assert_eq!(
        obj.get("joinedAt").and_then(|v| v.as_str()),
        Some("2025-01-01"),
        "joinedAt preserved"
    );
}

/// Ported from protolab `expr_ops::remap_view_ids_by_anchor`
/// (crates/protolab-eval/src/expr_ops.rs:326). Keys child matches by
/// *edge name*, not anchor.
fn remap_view_ids_by_anchor(
    reparsed: &panproto_inst::WInstance,
    original: &panproto_inst::WInstance,
) -> panproto_inst::WInstance {
    let mut remap: HashMap<u32, u32> = HashMap::new();
    remap.insert(reparsed.root, original.root);

    let mut stack: Vec<(u32, u32)> = vec![(original.root, reparsed.root)];
    let max_orig = original.nodes.keys().max().copied().unwrap_or(0);
    let mut next_fresh: u32 = max_orig.wrapping_add(1);

    while let Some((orig_id, rep_id)) = stack.pop() {
        let orig_children: Vec<(String, u32)> = original
            .arcs
            .iter()
            .filter_map(|(p, c, e)| {
                if *p != orig_id {
                    return None;
                }
                e.name.as_ref().map(|n| (n.to_string(), *c))
            })
            .collect();
        let rep_children: Vec<(String, u32)> = reparsed
            .arcs
            .iter()
            .filter_map(|(p, c, e)| {
                if *p != rep_id {
                    return None;
                }
                e.name.as_ref().map(|n| (n.to_string(), *c))
            })
            .collect();
        for (rep_name, rep_child) in &rep_children {
            if let Some((_, orig_child)) = orig_children.iter().find(|(on, _)| on == rep_name) {
                remap.entry(*rep_child).or_insert(*orig_child);
                stack.push((*orig_child, *rep_child));
            } else {
                remap.entry(*rep_child).or_insert_with(|| {
                    let id = next_fresh;
                    next_fresh = next_fresh.wrapping_add(1);
                    id
                });
            }
        }
    }
    for id in reparsed.nodes.keys() {
        remap.entry(*id).or_insert_with(|| {
            let fresh = next_fresh;
            next_fresh = next_fresh.wrapping_add(1);
            fresh
        });
    }

    let mut new_nodes = HashMap::new();
    for (old_id, node) in &reparsed.nodes {
        let new_id = *remap.get(old_id).unwrap();
        let mut new_node = node.clone();
        new_node.id = new_id;
        new_nodes.insert(new_id, new_node);
    }
    let new_arcs: Vec<_> = reparsed
        .arcs
        .iter()
        .map(|(p, c, e)| {
            (
                *remap.get(p).unwrap_or(p),
                *remap.get(c).unwrap_or(c),
                e.clone(),
            )
        })
        .collect();
    panproto_inst::WInstance::new(
        new_nodes,
        new_arcs,
        reparsed.fans.clone(),
        *remap.get(&reparsed.root).unwrap_or(&reparsed.root),
        reparsed.schema_root.clone(),
    )
}

/// Full-protolab-style path: forward get → `to_json` → edit JSON → re-parse
/// against the *target* schema → remap ids → put. This is what
/// `protolab-wasm::apply_modified_output_inner` actually does.
#[test]
fn put_preserves_view_edit_via_json_reparse_and_remap() {
    let protocol = object_graph_protocol();
    let src_schema = build_user_schema(&protocol);

    let source_json = serde_json::json!({
        "name": "Dave",
        "legacyId": 1,
        "email": "d@e.com",
        "joinedAt": "2025-01-01",
    });
    let instance = parse_json(&src_schema, "user", &source_json).expect("parse source");

    let lens = build_final_lens(&src_schema, &protocol);
    // Sanity check: the target schema must retain the `user` entry so
    // that `parse_json` can find the root. If this regresses, protolab
    // (which calls `find_root_vertex(tgt_schema)`) will silently land on
    // the wrong basepoint and scramble the instance.
    assert_eq!(
        panproto_schema::primary_entry(&lens.tgt_schema).map(|n| n.as_ref()),
        Some("user"),
        "tgt_schema must retain the `user` entry after lens instantiation",
    );
    let (view, complement) = asymmetric::get(&lens, &instance).expect("forward get");

    // Serialize the view, mutate its displayName, re-parse against the
    // target schema (which has `displayName` as the edge label).
    let mut view_json = to_json(&lens.tgt_schema, &view);
    view_json
        .as_object_mut()
        .unwrap()
        .insert("displayName".into(), serde_json::json!("EDITED"));

    let reparsed = parse_json(&lens.tgt_schema, "user", &view_json).expect("reparse view");
    let remapped = remap_view_ids_by_anchor(&reparsed, &view);

    let restored = asymmetric::put(&lens, &remapped, &complement).expect("put");
    let restored_json = to_json(&src_schema, &restored);
    let obj = restored_json.as_object().unwrap();

    assert_eq!(
        obj.get("name").and_then(|v| v.as_str()),
        Some("EDITED"),
        "restored name after json-reparse path"
    );
    assert_eq!(
        obj.get("email").and_then(|v| v.as_str()),
        Some("d@e.com"),
        "email preserved"
    );
    assert_eq!(
        obj.get("legacyId").and_then(|v| v.as_i64()),
        Some(1),
        "legacyId restored"
    );
    assert_eq!(
        obj.get("joinedAt").and_then(|v| v.as_str()),
        Some("2025-01-01"),
        "joinedAt preserved"
    );
}

#[test]
fn put_preserves_view_edit_with_handcrafted_compiled_migration() {
    // Second variant: skip protolens instantiation entirely and build
    // a CompiledMigration by hand that mirrors what the chain produces.
    // This isolates the FieldTransform-on-user + per-child-vertex-edit
    // from any protolens compilation quirks.
    let protocol = object_graph_protocol();
    let src_schema = build_user_schema(&protocol);

    // Target schema: user.name edge is renamed to "displayName",
    // user.legacyId is dropped, user.bio is added.
    let tgt_schema = SchemaBuilder::new(&protocol)
        .vertex("user", "object", None::<&str>)
        .unwrap()
        .vertex("user.name", "string", None::<&str>)
        .unwrap()
        .vertex("user.email", "string", None::<&str>)
        .unwrap()
        .vertex("user.bio", "string", None::<&str>)
        .unwrap()
        .edge("user", "user.name", "prop", Some("displayName"))
        .unwrap()
        .edge("user", "user.email", "prop", Some("email"))
        .unwrap()
        .edge("user", "user.bio", "prop", Some("bio"))
        .unwrap()
        .entry("user")
        .build()
        .unwrap();

    let source_json = serde_json::json!({
        "name": "Dave",
        "legacyId": 1,
        "email": "d@e.com",
        "joinedAt": "2025-01-01",
    });
    let instance = parse_json(&src_schema, "user", &source_json).expect("parse source");

    let mut surviving_verts = HashSet::new();
    surviving_verts.insert(Name::from("user"));
    surviving_verts.insert(Name::from("user.name"));
    surviving_verts.insert(Name::from("user.email"));

    let mut field_transforms: HashMap<Name, Vec<FieldTransform>> = HashMap::new();
    field_transforms.insert(
        Name::from("user"),
        vec![
            FieldTransform::RenameField {
                old_key: "name".into(),
                new_key: "displayName".into(),
            },
            FieldTransform::AddField {
                key: "bio".into(),
                value: Value::Str(String::new()),
            },
            FieldTransform::DropField {
                key: "legacyId".into(),
            },
        ],
    );

    let compiled = CompiledMigration {
        surviving_verts,
        surviving_edges: HashSet::new(),
        vertex_remap: HashMap::new(),
        edge_remap: HashMap::new(),
        resolver: HashMap::new(),
        hyper_resolver: HashMap::new(),
        field_transforms,
        conditional_survival: HashMap::new(),
        op_term_assignments: HashMap::new(),
        expansion_path: HashMap::new(),
    };

    let lens = Lens {
        compiled,
        src_schema: src_schema.clone(),
        tgt_schema,
    };

    let (mut view, complement) = asymmetric::get(&lens, &instance).expect("forward get");

    // Edit the user.name child value.
    // (It keeps the same vertex anchor; edge label changed is not our concern here.)
    // Find the node whose anchor is "user.name".
    let name_node_id = *view
        .nodes
        .iter()
        .find(|(_, n)| n.anchor.as_ref() == "user.name")
        .expect("user.name child node in view")
        .0;
    view.nodes.get_mut(&name_node_id).unwrap().value = Some(
        panproto_inst::value::FieldPresence::Present(Value::Str("EDITED".into())),
    );

    let restored = asymmetric::put(&lens, &view, &complement).expect("put back");
    let restored_json = to_json(&src_schema, &restored);
    let obj = restored_json.as_object().unwrap();

    assert_eq!(
        obj.get("name").and_then(|v| v.as_str()),
        Some("EDITED"),
        "handcrafted: restored name should equal view edit"
    );
    assert_eq!(
        obj.get("email").and_then(|v| v.as_str()),
        Some("d@e.com"),
        "email preserved"
    );
    assert_eq!(
        obj.get("legacyId").and_then(|v| v.as_i64()),
        Some(1),
        "legacyId restored from complement"
    );
    assert_eq!(
        obj.get("joinedAt").and_then(|v| v.as_str()),
        Some("2025-01-01"),
        "joinedAt preserved"
    );
}
