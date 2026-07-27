#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Backward propagation through an invertible field transform.
//!
//! A transform carrying an inverse has two coordinates to keep straight: the
//! one its forward expression *reads* and the one it *writes*. `ComputeField`
//! separates them, and `put` has to send the inverse's result to the read
//! coordinate while leaving no trace of the written one, which the source
//! never carried. `ApplyExpr` keeps them together on one key, but only when
//! that key is an `extra_fields` entry; over a child scalar it reads the
//! child and writes a shadowing entry on the parent, so the two part company
//! again.
//!
//! The tests here pin both the round-trip laws and the propagation itself:
//! a law can hold because an edit was correctly propagated, or because it
//! was quietly dropped, and only checking the value distinguishes them.

use std::collections::HashMap;
use std::sync::Arc;

use panproto_expr::{BuiltinOp, Expr};
use panproto_gat::{CoercionClass, Name};
use panproto_inst::value::Value;
use panproto_inst::{CompiledMigration, FieldTransform, WInstance, parse_json};
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

/// `user { a: string }` with `a` as a schema-defined child vertex.
fn schema_with_child() -> Schema {
    let edge = Edge {
        src: "user".into(),
        tgt: "user.a".into(),
        kind: "prop".into(),
        name: Some("a".into()),
    };
    make_schema(&[("user", "object"), ("user.a", "string")], &[edge])
}

/// `user` with no declared properties, so `a` parses into `extra_fields`.
fn schema_without_child() -> Schema {
    make_schema(&[("user", "object")], &[])
}

fn lens_with(schema: &Schema, transforms: Vec<FieldTransform>) -> Lens {
    let mut field_transforms = HashMap::new();
    field_transforms.insert(Name::from("user"), transforms);
    Lens {
        compiled: CompiledMigration {
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
        },
        src_schema: schema.clone(),
        tgt_schema: schema.clone(),
    }
}

/// `up = upper(a)`, inverse `lower(up)`, declared `Iso`.
fn compute_upper() -> FieldTransform {
    FieldTransform::ComputeField {
        target_key: "up".into(),
        expr: Expr::Builtin(BuiltinOp::Upper, vec![Expr::Var(Arc::from("a"))]),
        inverse: Some(Expr::Builtin(
            BuiltinOp::Lower,
            vec![Expr::Var(Arc::from("up"))],
        )),
        coercion_class: CoercionClass::Iso,
    }
}

fn root_fields(instance: &WInstance) -> &HashMap<String, Value> {
    &instance.nodes[&instance.root].extra_fields
}

fn instance_of(schema: &Schema) -> WInstance {
    parse_json(schema, "user", &serde_json::json!({"a": "hello"})).expect("parse")
}

/// The computed key belongs to the view, not the source. `put` must not
/// leave it behind, whichever side the value it read came from.
#[test]
fn put_does_not_reinstate_the_computed_key() {
    for (label, schema) in [
        ("child scalar", schema_with_child()),
        ("extra field", schema_without_child()),
    ] {
        let instance = instance_of(&schema);
        let lens = lens_with(&schema, vec![compute_upper()]);

        let (view, complement) = get(&lens, &instance).expect("get");
        assert_eq!(
            view.nodes[&view.root].extra_fields.get("up"),
            Some(&Value::Str("HELLO".into())),
            "{label}: the forward pass computes `up`"
        );

        let restored = put(&lens, &view, &complement).expect("put");
        assert!(
            !root_fields(&restored).contains_key("up"),
            "{label}: `up` is a computed view field; the source never had it, so put must \
             not write it back: {:?}",
            root_fields(&restored)
        );
    }
}

/// Both laws hold for an invertible `ComputeField` whichever side its
/// source sits on.
#[test]
fn invertible_compute_field_satisfies_both_laws() {
    for (label, schema) in [
        ("child scalar", schema_with_child()),
        ("extra field", schema_without_child()),
    ] {
        let instance = instance_of(&schema);
        let lens = lens_with(&schema, vec![compute_upper()]);

        assert!(
            panproto_lens::laws::check_get_put(&lens, &instance).is_ok(),
            "{label}: GetPut must hold"
        );
        let put_get = panproto_lens::laws::check_put_get(&lens, &instance);
        assert!(put_get.is_ok(), "{label}: PutGet must hold: {put_get:?}");
    }
}

/// Computing `up` while `a` stays in the view leaves the two redundant, so
/// `up` is a derived coordinate however the transform is classified: `get`
/// recomputes it from `a` regardless of what the inverse would say.
#[test]
fn a_computed_field_beside_its_source_is_derived() {
    let schema = schema_without_child();
    let lens = lens_with(&schema, vec![compute_upper()]);

    let derived = panproto_lens::collect_derived_fields(&lens.compiled);
    let fiber = derived
        .get(&Name::from("user"))
        .expect("the anchor carries a derived coordinate");
    assert!(
        !fiber.is_empty(),
        "`up` duplicates `a`, so it cannot be an independent coordinate"
    );
}

/// Dropping the source in the same batch makes the transform a genuine
/// change of coordinates: `up` becomes independent, and an edit to it has
/// to reach `a` in the source.
#[test]
fn a_computed_field_replacing_its_source_round_trips_edits() {
    let schema = schema_without_child();
    let instance = instance_of(&schema);
    let lens = lens_with(
        &schema,
        vec![
            compute_upper(),
            FieldTransform::DropField { key: "a".into() },
        ],
    );

    assert!(
        panproto_lens::collect_derived_fields(&lens.compiled).is_empty(),
        "`up` replaces `a`, so it is an independent coordinate"
    );

    let (view, complement) = get(&lens, &instance).expect("get");
    assert!(
        !root_fields(&view).contains_key("a"),
        "the source coordinate is gone from the view"
    );

    // Edit the view's computed field; the edit must land on `a`.
    // Moved, not cloned: nothing reads the pristine view after this.
    let mut edited = view;
    edited
        .nodes
        .get_mut(&edited.root)
        .expect("root node")
        .extra_fields
        .insert("up".into(), Value::Str("WORLD".into()));

    let restored = put(&lens, &edited, &complement).expect("put");
    assert_eq!(
        root_fields(&restored).get("a"),
        Some(&Value::Str("world".into())),
        "the inverse writes to the field the forward expression read, not to `up`"
    );
    assert!(
        !root_fields(&restored).contains_key("up"),
        "and leaves no trace of the computed key"
    );

    let (re_get, _) = get(&lens, &restored).expect("re-get");
    assert_eq!(
        root_fields(&re_get).get("up"),
        Some(&Value::Str("WORLD".into())),
        "so the edit survives the round trip"
    );

    assert!(panproto_lens::laws::check_get_put(&lens, &instance).is_ok());
    assert!(panproto_lens::laws::check_put_get(&lens, &instance).is_ok());
}

/// An `ApplyExpr` over a child scalar reads the child but writes a
/// shadowing `extra_fields` entry, so the source has no entry to restore.
/// `put` must not invent one.
#[test]
fn apply_expr_over_a_child_scalar_leaves_no_shadow_field() {
    let schema = schema_with_child();
    let instance = instance_of(&schema);
    let lens = lens_with(
        &schema,
        vec![FieldTransform::ApplyExpr {
            key: "a".into(),
            expr: Expr::Builtin(BuiltinOp::Upper, vec![Expr::Var(Arc::from("a"))]),
            inverse: Some(Expr::Builtin(
                BuiltinOp::Lower,
                vec![Expr::Var(Arc::from("a"))],
            )),
            coercion_class: CoercionClass::Iso,
        }],
    );

    let (view, complement) = get(&lens, &instance).expect("get");
    assert_eq!(
        root_fields(&view).get("a"),
        Some(&Value::Str("HELLO".into())),
        "the forward pass shadows the child with its transformed value"
    );

    let restored = put(&lens, &view, &complement).expect("put");
    assert!(
        !root_fields(&restored).contains_key("a"),
        "the child node carries the source value; the parent had no `a` entry: {:?}",
        root_fields(&restored)
    );

    assert!(
        panproto_lens::laws::check_get_put(&lens, &instance).is_ok(),
        "GetPut must hold"
    );
    let put_get = panproto_lens::laws::check_put_get(&lens, &instance);
    assert!(put_get.is_ok(), "PutGet must hold: {put_get:?}");
}

/// An `ApplyExpr` over an `extra_fields` entry does read and write the same
/// slot, so it stays an independent coordinate and its inverse still runs.
#[test]
fn apply_expr_over_an_extra_field_still_inverts_in_place() {
    let schema = schema_without_child();
    let instance = instance_of(&schema);
    let lens = lens_with(
        &schema,
        vec![FieldTransform::ApplyExpr {
            key: "a".into(),
            expr: Expr::Builtin(BuiltinOp::Upper, vec![Expr::Var(Arc::from("a"))]),
            inverse: Some(Expr::Builtin(
                BuiltinOp::Lower,
                vec![Expr::Var(Arc::from("a"))],
            )),
            coercion_class: CoercionClass::Iso,
        }],
    );

    assert!(
        panproto_lens::collect_derived_fields(&lens.compiled).is_empty(),
        "an in-place invertible swap is an independent coordinate"
    );

    let (view, complement) = get(&lens, &instance).expect("get");
    // Moved, not cloned: nothing reads the pristine view after this.
    let mut edited = view;
    edited
        .nodes
        .get_mut(&edited.root)
        .expect("root node")
        .extra_fields
        .insert("a".into(), Value::Str("WORLD".into()));

    let restored = put(&lens, &edited, &complement).expect("put");
    assert_eq!(
        root_fields(&restored).get("a"),
        Some(&Value::Str("world".into())),
        "the inverse runs in place on the same key"
    );

    assert!(panproto_lens::laws::check_get_put(&lens, &instance).is_ok());
    assert!(panproto_lens::laws::check_put_get(&lens, &instance).is_ok());
}

/// Renaming the source is not removing it: the value is still in the view
/// under another key, so the computed field remains redundant with it.
#[test]
fn renaming_the_source_does_not_make_the_computation_independent() {
    let schema = schema_without_child();
    let lens = lens_with(
        &schema,
        vec![
            compute_upper(),
            FieldTransform::RenameField {
                old_key: "a".into(),
                new_key: "moved".into(),
            },
        ],
    );

    let derived = panproto_lens::collect_derived_fields(&lens.compiled);
    let fiber = derived
        .get(&Name::from("user"))
        .expect("the anchor carries a derived coordinate");
    assert!(
        !fiber.is_empty(),
        "`a` survives as `moved`, so `up` still duplicates information the view holds"
    );
}

/// An expression over several fields has no single coordinate to invert
/// to, so the target stays derived whatever `coercion_class` claims.
#[test]
fn a_multi_source_computation_is_not_invertible() {
    let schema = schema_without_child();
    let lens = lens_with(
        &schema,
        vec![
            FieldTransform::ComputeField {
                target_key: "joined".into(),
                expr: Expr::Builtin(
                    BuiltinOp::Concat,
                    vec![Expr::Var(Arc::from("a")), Expr::Var(Arc::from("b"))],
                ),
                inverse: Some(Expr::Var(Arc::from("joined"))),
                coercion_class: CoercionClass::Iso,
            },
            FieldTransform::DropField { key: "a".into() },
        ],
    );

    let derived = panproto_lens::collect_derived_fields(&lens.compiled);
    let fiber = derived
        .get(&Name::from("user"))
        .expect("the anchor carries a derived coordinate");
    assert!(
        !fiber.is_empty(),
        "one inverse expression cannot restore two source coordinates"
    );
}
