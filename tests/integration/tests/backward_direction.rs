#![allow(clippy::unwrap_used, clippy::expect_used)]
//! The backward direction agrees with the round trip.
//!
//! Two things a `put` has to get right that the round-trip laws did not
//! previously see. Array order: the children of a collection node are its
//! elements in sequence, so a reconstruction that permutes them returns a
//! different record while every node and arc is still present. And an
//! invertible computation whose result crossed the JSON boundary: the
//! forward pass leaves it in `extra_fields` shadowing the child it read,
//! but serializing and re-parsing moves it onto the child, where the
//! backward pass used not to look.

use std::collections::HashMap;
use std::sync::Arc;

use panproto_expr::{BuiltinOp, Expr, Literal};
use panproto_gat::{CoercionClass, Name};
use panproto_inst::value::Value;
use panproto_inst::{CompiledMigration, FieldTransform, WInstance, parse_json, to_json};
use panproto_lens::Lens;
use panproto_lens::asymmetric::{get, put};
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

fn lens_with(schema: &Schema, anchor: &str, t: Vec<FieldTransform>) -> Lens {
    let mut ft = HashMap::new();
    ft.insert(Name::from(anchor), t);
    Lens {
        compiled: CompiledMigration {
            surviving_verts: schema.vertices.keys().cloned().collect(),
            surviving_edges: schema.edges.keys().cloned().collect(),
            vertex_remap: HashMap::new(),
            edge_remap: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            field_transforms: ft,
            conditional_survival: HashMap::new(),
            op_term_assignments: HashMap::new(),
            expansion_path: HashMap::new(),
        },
        src_schema: schema.clone(),
        tgt_schema: schema.clone(),
    }
}

/// An `compute_field` that overwrites the field it reads, with an
/// inverse: `confidence -> floor(confidence*1000 + 0.5)`, back via
/// `confidence / 1000`.
#[test]
fn an_overwriting_computation_is_undone() {
    let e = Edge {
        src: "root".into(),
        tgt: "root.confidence".into(),
        kind: "prop".into(),
        name: Some("confidence".into()),
    };
    let schema = make_schema(&[("root", "object"), ("root.confidence", "number")], &[e]);
    let instance: WInstance =
        parse_json(&schema, "root", &serde_json::json!({"confidence": 0.8})).expect("parse");

    let fwd = Expr::Builtin(
        BuiltinOp::Floor,
        vec![Expr::Builtin(
            BuiltinOp::Add,
            vec![
                Expr::Builtin(
                    BuiltinOp::Mul,
                    vec![
                        Expr::Var(Arc::from("confidence")),
                        Expr::Lit(Literal::Float(1000.0)),
                    ],
                ),
                Expr::Lit(Literal::Float(0.5)),
            ],
        )],
    );
    let inv = Expr::Builtin(
        BuiltinOp::Div,
        vec![
            Expr::Var(Arc::from("confidence")),
            Expr::Lit(Literal::Float(1000.0)),
        ],
    );

    let lens = lens_with(
        &schema,
        "root",
        vec![FieldTransform::ComputeField {
            target_key: "confidence".into(),
            expr: fwd,
            inverse: Some(inv),
            coercion_class: CoercionClass::Iso,
        }],
    );

    let (view, comp) = get(&lens, &instance).expect("get");
    assert_eq!(
        to_json(&schema, &view)["confidence"],
        800,
        "the forward pass scales and rounds"
    );

    let restored = put(&lens, &view, &comp).expect("put");
    assert_eq!(
        to_json(&schema, &restored)["confidence"],
        0.8,
        "the backward pass undoes it"
    );
    assert!(panproto_lens::laws::check_get_put(&lens, &instance).is_ok());
    assert!(panproto_lens::laws::check_put_get(&lens, &instance).is_ok());
}

/// Array element order through `put`.
#[test]
fn collection_order_survives_the_round_trip() {
    let e_items = Edge {
        src: "root".into(),
        tgt: "root.items".into(),
        kind: "prop".into(),
        name: Some("items".into()),
    };
    let e_item = Edge {
        src: "root.items".into(),
        tgt: "root.item".into(),
        kind: "item".into(),
        name: None,
    };
    let e_n = Edge {
        src: "root.item".into(),
        tgt: "root.item.n".into(),
        kind: "prop".into(),
        name: Some("n".into()),
    };
    let schema = make_schema(
        &[
            ("root", "object"),
            ("root.items", "array"),
            ("root.item", "object"),
            ("root.item.n", "integer"),
        ],
        &[e_items, e_item, e_n],
    );
    let instance: WInstance = parse_json(
        &schema,
        "root",
        &serde_json::json!({"items": [{"n": 1}, {"n": 2}, {"n": 3}]}),
    )
    .expect("parse");

    let lens = lens_with(
        &schema,
        "root.item",
        vec![FieldTransform::ComputeField {
            target_key: "n2".into(),
            expr: Expr::Builtin(
                BuiltinOp::Mul,
                vec![Expr::Var(Arc::from("n")), Expr::Lit(Literal::Int(2))],
            ),
            inverse: None,
            coercion_class: CoercionClass::Projection,
        }],
    );

    let (view, comp) = get(&lens, &instance).expect("get");
    let restored = put(&lens, &view, &comp).expect("put");

    assert_eq!(
        to_json(&schema, &restored),
        serde_json::json!({"items": [{"n": 1}, {"n": 2}, {"n": 3}]}),
        "elements come back in the order they went in"
    );
    // The arc sequence is what array order is read off, so pin it directly
    // as well as through the serialized form.
    let pairs = |i: &WInstance| i.arcs.iter().map(|(p, c, _)| (*p, *c)).collect::<Vec<_>>();
    assert_eq!(pairs(&instance), pairs(&restored), "arc order is preserved");

    // And the laws must be able to see a permutation, or they are not
    // evidence that the backward direction works.
    assert!(panproto_lens::laws::check_get_put(&lens, &instance).is_ok());
    assert!(panproto_lens::laws::check_put_get(&lens, &instance).is_ok());
    let _ = Value::Null;
}

/// A reconstruction that permutes a collection's children is not
/// equivalent to the original, even though every node and arc is present.
#[test]
fn instance_equivalence_sees_a_permuted_collection() {
    let e_items = Edge {
        src: "root".into(),
        tgt: "root.items".into(),
        kind: "prop".into(),
        name: Some("items".into()),
    };
    let e_item = Edge {
        src: "root.items".into(),
        tgt: "root.item".into(),
        kind: "item".into(),
        name: None,
    };
    let e_n = Edge {
        src: "root.item".into(),
        tgt: "root.item.n".into(),
        kind: "prop".into(),
        name: Some("n".into()),
    };
    let schema = make_schema(
        &[
            ("root", "object"),
            ("root.items", "array"),
            ("root.item", "object"),
            ("root.item.n", "integer"),
        ],
        &[e_items, e_item, e_n],
    );
    let instance: WInstance = parse_json(
        &schema,
        "root",
        &serde_json::json!({"items": [{"n": 1}, {"n": 2}, {"n": 3}]}),
    )
    .expect("parse");

    let mut permuted = instance.clone();
    permuted.arcs.reverse();

    assert!(
        !panproto_lens::laws::instances_equivalent(&instance, &permuted),
        "same arcs in a different order is a different record"
    );
}

/// What `put_json` does: serialize the view, re-parse it, then put.
#[test]
fn an_inverse_runs_when_the_value_crossed_the_json_boundary() {
    let e = Edge {
        src: "root".into(),
        tgt: "root.confidence".into(),
        kind: "prop".into(),
        name: Some("confidence".into()),
    };
    let schema = make_schema(&[("root", "object"), ("root.confidence", "number")], &[e]);
    let instance: WInstance =
        parse_json(&schema, "root", &serde_json::json!({"confidence": 0.8})).expect("parse");

    let fwd = Expr::Builtin(
        BuiltinOp::Floor,
        vec![Expr::Builtin(
            BuiltinOp::Add,
            vec![
                Expr::Builtin(
                    BuiltinOp::Mul,
                    vec![
                        Expr::Var(Arc::from("confidence")),
                        Expr::Lit(Literal::Float(1000.0)),
                    ],
                ),
                Expr::Lit(Literal::Float(0.5)),
            ],
        )],
    );
    let inv = Expr::Builtin(
        BuiltinOp::Div,
        vec![
            Expr::Var(Arc::from("confidence")),
            Expr::Lit(Literal::Float(1000.0)),
        ],
    );
    let lens = lens_with(
        &schema,
        "root",
        vec![FieldTransform::ComputeField {
            target_key: "confidence".into(),
            expr: fwd,
            inverse: Some(inv),
            coercion_class: CoercionClass::Iso,
        }],
    );

    let (view, comp) = get(&lens, &instance).expect("get");
    let view_json = to_json(&schema, &view);
    assert_eq!(view_json["confidence"], 800);

    // The WASM `put_json` re-parses the view rather than using the
    // in-memory one, which moves the computed value from the parent's
    // extra_fields onto the child node.
    let reparsed: WInstance = parse_json(&schema, "root", &view_json).expect("reparse");
    assert!(
        reparsed.nodes[&reparsed.root].extra_fields.is_empty(),
        "re-parsing puts the value on the child, not in extra_fields"
    );

    let restored = put(&lens, &reparsed, &comp).expect("put");
    assert_eq!(
        to_json(&schema, &restored)["confidence"],
        0.8,
        "the inverse still has to run when the value arrives on the child"
    );
}
