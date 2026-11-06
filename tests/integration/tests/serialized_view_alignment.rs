#![allow(clippy::unwrap_used, clippy::expect_used)]
//! A view put back through JSON keeps the residue its complement holds.
//!
//! JSON carries no node ids, and `parse_json` numbers a view's nodes afresh,
//! while a tree complement keys its residue by the ids `get` gave the view.
//! The serialized put path parses the view, aligns it to the positions the
//! complement records, folds computed fields and puts. Here the source
//! `{"text", "obj": {"k", "d"}}` is put through a lens that drops `obj.d`:
//! `parse_json` gives `obj` a different id from the one `get` gave it, so
//! without alignment `put` restored `d` under no node and reported success.

use panproto_inst::{CompiledMigration, parse_json, to_json};
use panproto_lens::{
    Lens, LensError, ViewAlignmentError, align_view_to_complement,
    fold_computed_children_into_fields,
};
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

fn protocol() -> Protocol {
    Protocol {
        name: "demo-nested".into(),
        schema_theory: "ThConstrainedGraph".into(),
        instance_theory: "ThWType".into(),
        edge_rules: ["record-schema", "prop"]
            .into_iter()
            .map(|kind| EdgeRule {
                edge_kind: kind.into(),
                src_kinds: vec![],
                tgt_kinds: vec![],
            })
            .collect(),
        obj_kinds: ["rec", "rec:body", "object", "string"]
            .into_iter()
            .map(Into::into)
            .collect(),
        ..Protocol::default()
    }
}

/// `rec:body { text, obj { k, d } }`, or without `obj.d`, with the edges
/// out of `rec:body` declared in the order given.
fn schema(protocol: &Protocol, with_d: bool, body_edges: [&str; 2]) -> Schema {
    let mut builder = SchemaBuilder::new(protocol)
        .vertex("rec", "rec", None)
        .unwrap()
        .vertex("rec:body", "rec:body", None)
        .unwrap()
        .vertex("rec:body.text", "string", None)
        .unwrap()
        .vertex("rec:body.obj", "object", None)
        .unwrap()
        .vertex("rec:body.obj.k", "string", None)
        .unwrap();
    if with_d {
        builder = builder.vertex("rec:body.obj.d", "string", None).unwrap();
    }
    builder = builder
        .edge("rec", "rec:body", "record-schema", None)
        .unwrap();
    for name in body_edges {
        builder = builder
            .edge("rec:body", &format!("rec:body.{name}"), "prop", Some(name))
            .unwrap();
    }
    builder = builder
        .edge("rec:body.obj", "rec:body.obj.k", "prop", Some("k"))
        .unwrap();
    if with_d {
        builder = builder
            .edge("rec:body.obj", "rec:body.obj.d", "prop", Some("d"))
            .unwrap();
    }
    builder.build().unwrap()
}

/// The lens that drops `obj.d` and keeps everything else under its name.
///
/// Its target schema declares `obj` before `text`, as an instantiated
/// protolens may order the edges it rebuilds, so `parse_json`, which
/// numbers the nodes of a record in its schema's edge order, numbers a view
/// differently from the source `get` read it from.
fn dropping_lens(protocol: &Protocol) -> Lens {
    let src_schema = schema(protocol, true, ["text", "obj"]);
    let tgt_schema = schema(protocol, false, ["obj", "text"]);
    Lens {
        compiled: CompiledMigration {
            surviving_verts: tgt_schema.vertices.keys().cloned().collect(),
            surviving_edges: tgt_schema.edges.keys().cloned().collect(),
            ..CompiledMigration::default()
        },
        src_schema,
        tgt_schema,
    }
}

/// The serialized put path: read `shown` under the target schema, align
/// it to `complement`, fold computed fields, and put.
fn put_json(
    protocol: &Protocol,
    lens: &Lens,
    shown: &serde_json::Value,
    complement: &panproto_lens::Complement,
) -> Result<serde_json::Value, LensError> {
    let parsed = parse_json(&lens.tgt_schema, protocol, "rec:body", shown).unwrap();
    let aligned = align_view_to_complement(lens, &parsed, complement)?;
    let folded = fold_computed_children_into_fields(lens, &aligned, complement);
    panproto_lens::put(lens, &folded, complement)
        .map(|restored| to_json(&lens.src_schema, &restored).unwrap())
}

/// Get `source`, check its view reads as `expected_view`, and put `shown`
/// back through JSON.
fn round_trip(
    source: &serde_json::Value,
    expected_view: &serde_json::Value,
    shown: &serde_json::Value,
) -> Result<serde_json::Value, LensError> {
    let protocol = protocol();
    let lens = dropping_lens(&protocol);
    let source = parse_json(&lens.src_schema, &protocol, "rec:body", source).unwrap();
    let (view, complement) = panproto_lens::get(&lens, &source).unwrap();
    assert_eq!(&to_json(&lens.tgt_schema, &view).unwrap(), expected_view);
    put_json(&protocol, &lens, shown, &complement)
}

#[test]
fn an_unedited_view_with_a_nested_dropped_field_puts_back_the_field() {
    let source = serde_json::json!({ "text": "hello", "obj": { "k": "1", "d": "2" } });
    let view = serde_json::json!({ "text": "hello", "obj": { "k": "1" } });
    assert_eq!(round_trip(&source, &view, &view).unwrap(), source);
}

#[test]
fn an_edited_kept_field_beside_a_nested_dropped_field_puts_back_both() {
    let source = serde_json::json!({ "text": "hello", "obj": { "k": "1", "d": "2" } });
    let view = serde_json::json!({ "text": "hello", "obj": { "k": "1" } });
    let edited = serde_json::json!({ "text": "bye", "obj": { "k": "9" } });
    assert_eq!(
        round_trip(&source, &view, &edited).unwrap(),
        serde_json::json!({ "text": "bye", "obj": { "k": "9", "d": "2" } })
    );
}

#[test]
fn an_inserted_sibling_beside_a_nested_dropped_field_puts_back_both() {
    let source = serde_json::json!({ "obj": { "k": "1", "d": "2" } });
    let view = serde_json::json!({ "obj": { "k": "1" } });
    let inserted = serde_json::json!({ "text": "new", "obj": { "k": "1" } });
    assert_eq!(
        round_trip(&source, &view, &inserted).unwrap(),
        serde_json::json!({ "text": "new", "obj": { "k": "1", "d": "2" } })
    );
}

/// The view `get` returned, renumbered as a reader that keeps no ids
/// numbers it, aligns back to exactly that view.
#[test]
fn a_renumbered_view_aligns_back_to_the_view_get_returned() {
    let protocol = protocol();
    let lens = dropping_lens(&protocol);
    let source = parse_json(
        &lens.src_schema,
        &protocol,
        "rec:body",
        &serde_json::json!({ "text": "hello", "obj": { "k": "1", "d": "2" } }),
    )
    .unwrap();
    let (view, complement) = panproto_lens::get(&lens, &source).unwrap();
    let shown = to_json(&lens.tgt_schema, &view).unwrap();
    let parsed = parse_json(&lens.tgt_schema, &protocol, "rec:body", &shown).unwrap();
    assert_ne!(
        parsed.nodes[&2].anchor, view.nodes[&2].anchor,
        "the reader numbers the view differently from get"
    );
    // Any other numbering that keeps the arcs' order aligns alike: reverse
    // the reader's ids.
    let top = parsed.nodes.keys().max().copied().unwrap();
    let flip = |id: u32| top - id;
    let reversed = panproto_inst::WInstance::new(
        parsed
            .nodes
            .values()
            .map(|node| {
                let mut node = node.clone();
                node.id = flip(node.id);
                (node.id, node)
            })
            .collect(),
        parsed
            .arcs
            .iter()
            .map(|(p, c, e)| (flip(*p), flip(*c), e.clone()))
            .collect(),
        Vec::new(),
        flip(parsed.root),
        parsed.schema_root.clone(),
    );
    for renumbered in [&parsed, &reversed] {
        assert_aligns_to(&lens, renumbered, &complement, &view);
    }
}

fn assert_aligns_to(
    lens: &Lens,
    renumbered: &panproto_inst::WInstance,
    complement: &panproto_lens::Complement,
    view: &panproto_inst::WInstance,
) {
    let aligned = align_view_to_complement(lens, renumbered, complement).unwrap();
    let mut aligned_arcs = aligned.arcs.clone();
    let mut view_arcs = view.arcs.clone();
    aligned_arcs.sort_by_key(|(p, c, _)| (*p, *c));
    view_arcs.sort_by_key(|(p, c, _)| (*p, *c));
    assert_eq!(aligned_arcs, view_arcs);
    assert_eq!(aligned.root, view.root);
    for (id, node) in &aligned.nodes {
        assert_eq!(node.id, *id);
        assert_eq!(node.anchor, view.nodes[id].anchor);
        assert_eq!(node.value, view.nodes[id].value);
    }
    assert_eq!(aligned.nodes.len(), view.nodes.len());
}

/// A complement that records no positions, as one written before they were
/// recorded, cannot align a parsed view, and says so.
#[test]
fn a_complement_without_positions_refuses_alignment() {
    let protocol = protocol();
    let lens = dropping_lens(&protocol);
    let source = parse_json(
        &lens.src_schema,
        &protocol,
        "rec:body",
        &serde_json::json!({ "text": "hello", "obj": { "k": "1", "d": "2" } }),
    )
    .unwrap();
    let (view, mut complement) = panproto_lens::get(&lens, &source).unwrap();
    complement.view_positions.clear();
    let shown = to_json(&lens.tgt_schema, &view).unwrap();
    let refused = put_json(&protocol, &lens, &shown, &complement).unwrap_err();
    assert!(
        matches!(
            refused,
            LensError::ViewAlignment(ViewAlignmentError::PositionsNotRecorded)
        ),
        "{refused:?}"
    );
}

/// A view whose root sits at another vertex than the root `get` produced
/// is refused rather than aligned.
#[test]
fn a_view_rooted_elsewhere_refuses_alignment() {
    let protocol = protocol();
    let lens = dropping_lens(&protocol);
    let source = parse_json(
        &lens.src_schema,
        &protocol,
        "rec:body",
        &serde_json::json!({ "text": "hello", "obj": { "k": "1", "d": "2" } }),
    )
    .unwrap();
    let (_, complement) = panproto_lens::get(&lens, &source).unwrap();
    let elsewhere = parse_json(
        &lens.tgt_schema,
        &protocol,
        "rec:body.obj",
        &serde_json::json!({ "k": "1" }),
    )
    .unwrap();
    let refused = align_view_to_complement(&lens, &elsewhere, &complement).unwrap_err();
    assert!(
        matches!(
            &refused,
            LensError::ViewAlignment(ViewAlignmentError::RootAnchorMismatch { recorded, found })
                if recorded.as_str() == "rec:body" && found.as_str() == "rec:body.obj"
        ),
        "{refused:?}"
    );
}
