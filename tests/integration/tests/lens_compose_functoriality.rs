#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Composition is functorial on value-level field transforms.
//!
//! `compose` transports the second migration's transforms into the first's
//! source frame. Both coordinates have to be conjugated for that to be
//! sound: the anchor through `vertex_remap`, and the field names an
//! expression reads through the renames the first migration performs. The
//! tests here pin `get(m2 ∘ m1) = get(m2) ∘ get(m1)` across the rename
//! forms that reach the value layer by different routes: a schema edge
//! rename, an `extra_fields` rename, and a transitive chain of both.

use std::collections::HashMap;
use std::sync::Arc;

use panproto_expr::{BuiltinOp, Expr};
use panproto_gat::{CoercionClass, Name};
use panproto_inst::value::Value;
use panproto_inst::{CompiledMigration, FieldTransform, WInstance, parse_json};
use panproto_lens::asymmetric::get;
use panproto_lens::{Lens, compose};
use panproto_schema::{Edge, Schema, Vertex};
use smallvec::SmallVec;

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
        entries: Vec::new(),
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

fn user_schema(field: &str) -> Schema {
    let edge = Edge {
        src: "user".into(),
        tgt: "user.name".into(),
        kind: "prop".into(),
        name: Some(field.into()),
    };
    make_schema(&[("user", "object"), ("user.name", "string")], &[edge])
}

fn identity_migration(schema: &Schema) -> CompiledMigration {
    CompiledMigration {
        surviving_verts: schema.vertices.keys().cloned().collect(),
        surviving_edges: schema.edges.keys().cloned().collect(),
        vertex_remap: HashMap::new(),
        edge_remap: HashMap::new(),
        resolver: HashMap::new(),
        hyper_resolver: HashMap::new(),
        field_transforms: HashMap::new(),
        conditional_survival: HashMap::new(),
        op_term_assignments: HashMap::new(),
        expansion_path: HashMap::new(),
    }
}

/// `m1`: rename the schema edge `name → displayName`.
fn rename_edge_lens(src: &Schema, tgt: &Schema) -> Lens {
    let src_edge = src.edges.keys().next().expect("one edge").clone();
    let tgt_edge = tgt.edges.keys().next().expect("one edge").clone();
    let mut compiled = identity_migration(src);
    compiled.edge_remap.insert(src_edge, tgt_edge);
    Lens {
        compiled,
        src_schema: src.clone(),
        tgt_schema: tgt.clone(),
    }
}

/// `m2`: identity chain carrying `ComputeField { slug, lower(<field>) }`.
fn compute_slug_lens(schema: &Schema, field: &str) -> Lens {
    let mut compiled = identity_migration(schema);
    compiled.field_transforms.insert(
        Name::from("user"),
        vec![FieldTransform::ComputeField {
            target_key: "slug".into(),
            expr: Expr::Builtin(BuiltinOp::Lower, vec![Expr::Var(Arc::from(field))]),
            inverse: None,
            coercion_class: CoercionClass::Projection,
        }],
    );
    Lens {
        compiled,
        src_schema: schema.clone(),
        tgt_schema: schema.clone(),
    }
}

fn slug_of(view: &WInstance) -> Option<Value> {
    view.nodes
        .get(&view.root)
        .and_then(|node| node.extra_fields.get("slug"))
        .cloned()
}

/// The exact scenario from the report: `m1` renames a schema edge that
/// `m2`'s expression reads. Sequential application succeeded while the
/// composite failed with `UnboundVariable`.
#[test]
fn composite_agrees_with_sequential_across_an_edge_rename() {
    let src = user_schema("name");
    let mid = user_schema("displayName");

    let instance: WInstance =
        parse_json(&src, "user", &serde_json::json!({"name": "Alice"})).expect("parse");

    let l1 = rename_edge_lens(&src, &mid);
    let l2 = compute_slug_lens(&mid, "displayName");

    // Sequential: get(m2)(get(m1)(x)).
    let (v1, _) = get(&l1, &instance).expect("get m1");
    let (v2, _) = get(&l2, &v1).expect("get m2 on m1's output");
    let sequential = slug_of(&v2);
    assert_eq!(
        sequential,
        Some(Value::Str("alice".into())),
        "sequential application computes the slug"
    );

    // Composite: get(m2 ∘ m1)(x).
    let composed = compose(&l1, &l2).expect("compose");
    let (vc, _) = get(&composed, &instance).expect(
        "the composite must evaluate: m2's `displayName` conjugates to m1's source name `name`",
    );

    assert_eq!(
        slug_of(&vc),
        sequential,
        "get(m2 ∘ m1) must agree with get(m2) ∘ get(m1)"
    );
}

/// The unsound direction: an expression naming a field that does *not*
/// exist in `m2`'s input schema was the one the composite used to accept,
/// because the un-conjugated name happened to match `m1`'s source frame.
/// It has to be rejected, and composition time is where the cause is still
/// legible.
#[test]
fn composite_rejects_an_expression_written_against_the_wrong_frame() {
    let src = user_schema("name");
    let mid = user_schema("displayName");

    let instance: WInstance =
        parse_json(&src, "user", &serde_json::json!({"name": "Alice"})).expect("parse");

    let l1 = rename_edge_lens(&src, &mid);
    // `name` is not a field of m2's input schema, which presents `displayName`.
    let l2 = compute_slug_lens(&mid, "name");

    assert!(
        get(&l2, &get(&l1, &instance).expect("get m1").0).is_err(),
        "reading `name` fails against m2's own input, as it must"
    );

    match compose(&l1, &l2) {
        Err(panproto_lens::LensError::ComposeUnboundField { anchor, field }) => {
            assert_eq!(anchor, "user");
            assert_eq!(
                field, "name",
                "the diagnostic names the field that the rename took away"
            );
        }
        Err(other) => panic!("expected ComposeUnboundField, got {other:?}"),
        Ok(composed) => assert!(
            get(&composed, &instance).is_err(),
            "a composite that accepts this rejects nothing sequential application rejects"
        ),
    }
}

/// An `extra_fields` rename reaches the value layer by a different route
/// than an edge rename: `RenameField` rewrites the key in place, and
/// `m1`'s entries run ahead of `m2`'s in the merged batch, so `m2`'s
/// expression already sees the new name. Composition was correct here
/// before the fix and must stay correct after it, since conjugating this route
/// as well would rewrite a sound reference into a dangling one.
#[test]
fn composite_agrees_with_sequential_across_an_extra_field_rename() {
    let schema = user_schema("name");
    let instance: WInstance = parse_json(
        &schema,
        "user",
        &serde_json::json!({"name": "Alice", "handle": "ALICE"}),
    )
    .expect("parse");

    let mut m1 = identity_migration(&schema);
    m1.field_transforms.insert(
        Name::from("user"),
        vec![FieldTransform::RenameField {
            old_key: "handle".into(),
            new_key: "userHandle".into(),
        }],
    );
    let l1 = Lens {
        compiled: m1,
        src_schema: schema.clone(),
        tgt_schema: schema.clone(),
    };
    let l2 = compute_slug_lens(&schema, "userHandle");

    let (v1, _) = get(&l1, &instance).expect("get m1");
    let (v2, _) = get(&l2, &v1).expect("get m2");
    let composed = compose(&l1, &l2).expect("compose");
    let (vc, _) = get(&composed, &instance).expect("get composite");

    assert_eq!(
        slug_of(&vc),
        slug_of(&v2),
        "an extra_fields rename composes the same way an edge rename does"
    );
    assert_eq!(slug_of(&vc), Some(Value::Str("alice".into())));
}

/// The rename map has to resolve transitively: `a → b` then `b → c` must
/// send `c` back to `a` in one conjugation step. Chaining happens when
/// two rename lenses are composed first, so the test composes
/// `(l_ab ∘ l_bc) ∘ l2` and reads the far end of the chain.
#[test]
fn rename_chain_resolves_transitively() {
    let s_a = user_schema("a");
    let s_b = user_schema("b");
    let s_c = user_schema("c");

    let instance: WInstance =
        parse_json(&s_a, "user", &serde_json::json!({"a": "Alice"})).expect("parse");

    let l_ab = rename_edge_lens(&s_a, &s_b);
    let l_bc = rename_edge_lens(&s_b, &s_c);
    let l2 = compute_slug_lens(&s_c, "c");

    let chained = compose(&l_ab, &l_bc).expect("compose the two renames");
    let composed = compose(&chained, &l2).expect("compose with the reader");

    let (view, _) = get(&composed, &instance).expect("composite evaluates");
    assert_eq!(
        slug_of(&view),
        Some(Value::Str("alice".into())),
        "`c` must conjugate back through `b` to the source name `a`"
    );
}

/// Two-edge schema whose edges carry the given field names.
fn pair_schema(first: &str, second: &str) -> Schema {
    let e1 = Edge {
        src: "user".into(),
        tgt: "user.x".into(),
        kind: "prop".into(),
        name: Some(first.into()),
    };
    let e2 = Edge {
        src: "user".into(),
        tgt: "user.y".into(),
        kind: "prop".into(),
        name: Some(second.into()),
    };
    make_schema(
        &[
            ("user", "object"),
            ("user.x", "string"),
            ("user.y", "string"),
        ],
        &[e1, e2],
    )
}

/// A swap `{a → b, b → a}` is the case sequential substitution gets
/// wrong: replacing `a` with `b` and then `b` with `a` sends everything
/// back to `a`, collapsing the two. Conjugation has to be simultaneous.
#[test]
fn rename_swap_conjugates_simultaneously() {
    let src = pair_schema("a", "b");
    // The same two edges, with their field names exchanged.
    let mid = pair_schema("b", "a");

    let instance: WInstance =
        parse_json(&src, "user", &serde_json::json!({"a": "AAA", "b": "BBB"})).expect("parse");

    let mut m1 = identity_migration(&src);
    for src_edge in src.edges.keys() {
        let mate = mid
            .edges
            .keys()
            .find(|e| e.tgt == src_edge.tgt)
            .expect("same target vertex on both sides");
        m1.edge_remap.insert(src_edge.clone(), mate.clone());
    }
    let l1 = Lens {
        compiled: m1,
        src_schema: src,
        tgt_schema: mid.clone(),
    };

    // m2 reads both swapped names, so a sequential rewrite would collapse them.
    let mut m2 = identity_migration(&mid);
    m2.field_transforms.insert(
        Name::from("user"),
        vec![FieldTransform::ComputeField {
            target_key: "joined".into(),
            expr: Expr::Builtin(
                BuiltinOp::Concat,
                vec![Expr::Var(Arc::from("a")), Expr::Var(Arc::from("b"))],
            ),
            inverse: None,
            coercion_class: CoercionClass::Projection,
        }],
    );
    let l2 = Lens {
        compiled: m2,
        src_schema: mid.clone(),
        tgt_schema: mid,
    };

    let joined = |view: &WInstance| {
        view.nodes
            .get(&view.root)
            .and_then(|n| n.extra_fields.get("joined"))
            .cloned()
    };

    let (v1, _) = get(&l1, &instance).expect("get m1");
    let (v2, _) = get(&l2, &v1).expect("get m2");
    let composed = compose(&l1, &l2).expect("compose");
    let (vc, _) = get(&composed, &instance).expect("get composite");

    assert_eq!(
        joined(&v2),
        Some(Value::Str("BBBAAA".into())),
        "after the swap, `a` names the edge that held BBB and `b` the one that held AAA"
    );
    assert_eq!(
        joined(&vc),
        joined(&v2),
        "a swap must conjugate simultaneously, not by sequential substitution"
    );
}

/// `ApplyExpr` reads and writes one key, so an edge rename splits its two
/// coordinates: it must read the source name and write the output name.
/// The composite has to land the transformed value on the renamed edge,
/// not emit the original under the new name beside the transformed value
/// under the old one.
#[test]
fn apply_expr_on_a_renamed_edge_writes_to_the_output_name() {
    let src = user_schema("name");
    let mid = user_schema("displayName");

    let instance: WInstance =
        parse_json(&src, "user", &serde_json::json!({"name": "Alice"})).expect("parse");

    let l1 = rename_edge_lens(&src, &mid);

    let mut m2 = identity_migration(&mid);
    m2.field_transforms.insert(
        Name::from("user"),
        vec![FieldTransform::ApplyExpr {
            key: "displayName".into(),
            expr: Expr::Builtin(BuiltinOp::Lower, vec![Expr::Var(Arc::from("displayName"))]),
            inverse: None,
            coercion_class: CoercionClass::Projection,
        }],
    );
    let l2 = Lens {
        compiled: m2,
        src_schema: mid.clone(),
        tgt_schema: mid,
    };

    let (v1, _) = get(&l1, &instance).expect("get m1");
    let (v2, _) = get(&l2, &v1).expect("get m2");
    let composed = compose(&l1, &l2).expect("compose");
    let (vc, _) = get(&composed, &instance).expect("get composite");

    let field = |view: &WInstance, key: &str| {
        view.nodes
            .get(&view.root)
            .and_then(|n| n.extra_fields.get(key))
            .cloned()
    };

    assert_eq!(
        field(&v2, "displayName"),
        Some(Value::Str("alice".into())),
        "sequentially, the transform writes the lowered value under the new edge name"
    );
    assert_eq!(
        field(&vc, "displayName"),
        field(&v2, "displayName"),
        "the composite must write to the output name, not the source name"
    );
    assert_eq!(
        field(&vc, "name"),
        None,
        "no value may be left behind under the pre-rename name"
    );
}

/// An `m2` expression reading a field `m1` drops is reported when the
/// lenses are composed, not as an `UnboundVariable` when the composite runs.
#[test]
fn reading_a_dropped_field_fails_at_composition_time() {
    let schema = user_schema("name");

    let mut m1 = identity_migration(&schema);
    m1.field_transforms.insert(
        Name::from("user"),
        vec![FieldTransform::DropField {
            key: "handle".into(),
        }],
    );
    let l1 = Lens {
        compiled: m1,
        src_schema: schema.clone(),
        tgt_schema: schema.clone(),
    };
    let l2 = compute_slug_lens(&schema, "handle");

    let result = compose(&l1, &l2);
    assert!(
        matches!(
            result,
            Err(panproto_lens::LensError::ComposeUnboundField { ref field, .. }) if field == "handle"
        ),
        "composition must name the dropped field it cannot bind: {result:?}"
    );
}

/// The drop diagnostic must not fire on a name that survives as a child
/// edge: `DropField` filters `extra_fields` only, so the child scalar is
/// still bound at evaluation time.
#[test]
fn dropping_an_extra_field_shadowing_a_child_edge_still_composes() {
    let schema = user_schema("name");

    let mut m1 = identity_migration(&schema);
    m1.field_transforms.insert(
        Name::from("user"),
        vec![FieldTransform::DropField { key: "name".into() }],
    );
    let l1 = Lens {
        compiled: m1,
        src_schema: schema.clone(),
        tgt_schema: schema.clone(),
    };
    let l2 = compute_slug_lens(&schema, "name");

    let composed = compose(&l1, &l2)
        .expect("`name` survives as a child edge, so reading it after DropField is sound");
    let instance: WInstance =
        parse_json(&schema, "user", &serde_json::json!({"name": "Alice"})).expect("parse");
    let (view, _) = get(&composed, &instance).expect("composite evaluates");
    assert_eq!(slug_of(&view), Some(Value::Str("alice".into())));
}
