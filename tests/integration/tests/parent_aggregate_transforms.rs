#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Parent-level aggregates over an array-of-objects child.
//!
//! A field transform used to see only `extra_fields` and the node's
//! *scalar* children, so a child that is an array of records bound to
//! nothing and an aggregate over it (the minimum and maximum of a field
//! across the array) could not be written at all. The array is now bound
//! as a list of records, and the node itself is reachable both as `self`
//! and as the `"self"` node reference the graph-traversal builtins take.

use std::collections::HashMap;
use std::sync::Arc;

use panproto_expr::{BuiltinOp, Expr, Literal, Pattern};
use panproto_gat::{CoercionClass, Name};
use panproto_inst::value::Value;
use panproto_inst::{CompiledMigration, FieldTransform, WInstance, parse_json, wtype_restrict};
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

/// `clip { keyframes: [ { timeMs } ] }`: the array is a list vertex
/// (one anonymous item edge), each item a record with a scalar field.
fn keyframe_schema() -> Schema {
    let e_keyframes = Edge {
        src: "clip".into(),
        tgt: "clip.keyframes".into(),
        kind: "prop".into(),
        name: Some("keyframes".into()),
    };
    let e_item = Edge {
        src: "clip.keyframes".into(),
        tgt: "clip.keyframe".into(),
        kind: "item".into(),
        name: None,
    };
    let e_time = Edge {
        src: "clip.keyframe".into(),
        tgt: "clip.keyframe.timeMs".into(),
        kind: "prop".into(),
        name: Some("timeMs".into()),
    };
    make_schema(
        &[
            ("clip", "object"),
            ("clip.keyframes", "array"),
            ("clip.keyframe", "object"),
            ("clip.keyframe.timeMs", "integer"),
        ],
        &[e_keyframes, e_item, e_time],
    )
}

fn keyframe_instance(schema: &Schema) -> WInstance {
    parse_json(
        schema,
        "clip",
        &serde_json::json!({
            "keyframes": [ {"timeMs": 400}, {"timeMs": 100}, {"timeMs": 900} ]
        }),
    )
    .expect("parse")
}

fn migration_with(
    anchor: &str,
    transforms: Vec<FieldTransform>,
    schema: &Schema,
) -> CompiledMigration {
    let mut field_transforms = HashMap::new();
    field_transforms.insert(Name::from(anchor), transforms);
    CompiledMigration {
        surviving_verts: schema.vertices.keys().cloned().collect(),
        surviving_edges: schema.edges.keys().cloned().collect(),
        vertex_remap: HashMap::new(),
        edge_remap: HashMap::new(),
        resolver: HashMap::new(),
        hyper_resolver: HashMap::new(),
        field_transforms,
        conditional_survival: HashMap::new(),
        op_term_assignments: HashMap::new(),
        expansion_path: HashMap::new(),
    }
}

fn lam(param: &str, body: Expr) -> Expr {
    Expr::Lam(Arc::from(param), Box::new(body))
}

/// `fold (\a b -> if a <op> b then a else b) seed (map (\k -> k.timeMs) keyframes)`
fn extremum(op: BuiltinOp, seed: i64, source: Expr) -> Expr {
    let pick = lam(
        "a",
        lam(
            "b",
            Expr::Match {
                scrutinee: Box::new(Expr::Builtin(
                    op,
                    vec![Expr::Var(Arc::from("a")), Expr::Var(Arc::from("b"))],
                )),
                arms: vec![
                    (Pattern::Lit(Literal::Bool(true)), Expr::Var(Arc::from("a"))),
                    (Pattern::Wildcard, Expr::Var(Arc::from("b"))),
                ],
            },
        ),
    );
    // `map(list, f)` and `fold(list, init, f)`: the list comes first.
    let times = Expr::Builtin(
        BuiltinOp::Map,
        vec![
            source,
            lam(
                "k",
                Expr::Field(Box::new(Expr::Var(Arc::from("k"))), "timeMs".into()),
            ),
        ],
    );
    Expr::Builtin(
        BuiltinOp::Fold,
        vec![times, Expr::Lit(Literal::Int(seed)), pick],
    )
}

fn temporal_span(view: &WInstance) -> Option<Value> {
    view.nodes
        .get(&view.root)
        .and_then(|n| n.extra_fields.get("temporalSpan"))
        .cloned()
}

/// The reported case: an aggregate at the parent vertex over an
/// array-of-objects child, which previously failed with
/// "unbound variable: keyframes".
#[test]
fn parent_aggregate_over_child_object_array() {
    let schema = keyframe_schema();
    let instance = keyframe_instance(&schema);

    let expr = Expr::Record(vec![
        (
            "start".into(),
            extremum(
                BuiltinOp::Lt,
                999_999_999,
                Expr::Var(Arc::from("keyframes")),
            ),
        ),
        (
            "ending".into(),
            extremum(BuiltinOp::Gt, 0, Expr::Var(Arc::from("keyframes"))),
        ),
    ]);

    let migration = migration_with(
        "clip",
        vec![FieldTransform::ComputeField {
            target_key: "temporalSpan".into(),
            expr,
            inverse: None,
            coercion_class: CoercionClass::Projection,
        }],
        &schema,
    );

    let restricted = wtype_restrict(&instance, &schema, &schema, &migration)
        .expect("the keyframes array must be bound at the parent vertex");

    let span = temporal_span(&restricted).expect("temporalSpan computed");
    let Value::Unknown(fields) = span else {
        panic!("temporalSpan should be a record, got {span:?}");
    };
    assert_eq!(
        fields.get("start"),
        Some(&Value::Int(100)),
        "start is the minimum timeMs across the array"
    );
    assert_eq!(
        fields.get("ending"),
        Some(&Value::Int(900)),
        "ending is the maximum timeMs across the array"
    );
}

/// The array is bound as a list of records, so ordinary list builtins
/// reach it directly.
#[test]
fn child_object_array_binds_as_a_list_of_records() {
    let schema = keyframe_schema();
    let instance = keyframe_instance(&schema);

    let migration = migration_with(
        "clip",
        vec![
            FieldTransform::ComputeField {
                target_key: "count".into(),
                expr: Expr::Builtin(BuiltinOp::Length, vec![Expr::Var(Arc::from("keyframes"))]),
                inverse: None,
                coercion_class: CoercionClass::Projection,
            },
            FieldTransform::ComputeField {
                target_key: "first".into(),
                expr: Expr::Field(
                    Box::new(Expr::Builtin(
                        BuiltinOp::Head,
                        vec![Expr::Var(Arc::from("keyframes"))],
                    )),
                    "timeMs".into(),
                ),
                inverse: None,
                coercion_class: CoercionClass::Projection,
            },
        ],
        &schema,
    );

    let restricted =
        wtype_restrict(&instance, &schema, &schema, &migration).expect("transforms evaluate");
    let root = &restricted.nodes[&restricted.root];
    assert_eq!(root.extra_fields.get("count"), Some(&Value::Int(3)));
    assert_eq!(
        root.extra_fields.get("first"),
        Some(&Value::Int(400)),
        "list order is arc order, so `head` is the first keyframe"
    );
}

/// The node itself is reachable as `self`, so a field can be read through
/// the handle as well as directly.
#[test]
fn self_binds_the_current_node_as_a_record() {
    let schema = keyframe_schema();
    let instance = keyframe_instance(&schema);

    let migration = migration_with(
        "clip",
        vec![FieldTransform::ComputeField {
            target_key: "viaSelf".into(),
            expr: Expr::Builtin(
                BuiltinOp::Length,
                vec![Expr::Field(
                    Box::new(Expr::Var(Arc::from("self"))),
                    "keyframes".into(),
                )],
            ),
            inverse: None,
            coercion_class: CoercionClass::Projection,
        }],
        &schema,
    );

    let restricted =
        wtype_restrict(&instance, &schema, &schema, &migration).expect("self resolves");
    assert_eq!(
        restricted.nodes[&restricted.root]
            .extra_fields
            .get("viaSelf"),
        Some(&Value::Int(3))
    );
}

/// The graph-traversal builtins need a current node to walk from. They
/// take the node reference `"self"`, which the transform context now
/// supplies, so `edge_count` and `anchor` resolve instead of yielding null.
#[test]
fn graph_builtins_resolve_against_the_current_node() {
    let schema = keyframe_schema();
    let instance = keyframe_instance(&schema);

    let migration = migration_with(
        "clip",
        vec![
            FieldTransform::ComputeField {
                target_key: "outDegree".into(),
                expr: Expr::Builtin(
                    BuiltinOp::EdgeCount,
                    vec![Expr::Lit(Literal::Str("self".into()))],
                ),
                inverse: None,
                coercion_class: CoercionClass::Projection,
            },
            FieldTransform::ComputeField {
                target_key: "at".into(),
                expr: Expr::Builtin(
                    BuiltinOp::Anchor,
                    vec![Expr::Lit(Literal::Str("self".into()))],
                ),
                inverse: None,
                coercion_class: CoercionClass::Projection,
            },
        ],
        &schema,
    );

    let restricted =
        wtype_restrict(&instance, &schema, &schema, &migration).expect("graph builtins resolve");
    let root = &restricted.nodes[&restricted.root];
    assert_eq!(
        root.extra_fields.get("outDegree"),
        Some(&Value::Int(1)),
        "the clip has one outgoing arc, to the keyframes array"
    );
    assert_eq!(
        root.extra_fields.get("at"),
        Some(&Value::Str("clip".into())),
        "anchor(\"self\") names the current node's vertex"
    );
}

/// A transform anchored at the item vertex still sees its own scalars.
/// This worked before and has to keep working: the parent-over-array
/// direction is what was missing, not the per-item one.
#[test]
fn per_item_transform_still_sees_its_own_scalars() {
    let schema = keyframe_schema();
    let instance = keyframe_instance(&schema);

    let migration = migration_with(
        "clip.keyframe",
        vec![FieldTransform::ComputeField {
            target_key: "seconds".into(),
            expr: Expr::Builtin(
                BuiltinOp::Div,
                vec![
                    Expr::Var(Arc::from("timeMs")),
                    Expr::Lit(Literal::Int(1000)),
                ],
            ),
            inverse: None,
            coercion_class: CoercionClass::Projection,
        }],
        &schema,
    );

    let restricted =
        wtype_restrict(&instance, &schema, &schema, &migration).expect("per-item transform runs");
    let computed = restricted
        .nodes
        .values()
        .filter(|n| n.anchor.as_ref() == "clip.keyframe")
        .filter(|n| n.extra_fields.contains_key("seconds"))
        .count();
    assert_eq!(computed, 3, "every keyframe got its own computation");
}
