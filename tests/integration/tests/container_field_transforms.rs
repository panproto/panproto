#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Integration tests for field transforms over list- and record-valued fields.
//!
//! The companion to `scalar_field_transforms`, covering the container
//! case: a transform whose expression reads or returns an array or a
//! nested object. Records of this shape are the normal case rather than
//! the exception, since a protocol like `ATProto` keeps arrays and nested
//! objects inline in `extra_fields` rather than as child vertices.
//!
//! Two properties are exercised together, because a transform needs both
//! to run at all:
//!
//! 1. The instance-value/expression-literal conversion is
//!    structure-preserving in both directions, so `map` / `fold` / field
//!    projection can reach into a container and an expression that
//!    returns one is written back as structured data.
//! 2. Surface-syntax expressions lower to the argument order the
//!    evaluator reads, so a text-authored `map f xs` evaluates.
//!
//! Expressions here are written as source text and parsed, which is how
//! a lens document carries them, rather than being built as ASTs.

use std::collections::HashMap;

use panproto_gat::{CoercionClass, Name};
use panproto_inst::value::Value;
use panproto_inst::{
    CompiledMigration, FieldTransform, WInstance, parse_json, to_json, wtype_restrict,
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

/// A single-vertex schema. The container fields ride in `extra_fields`,
/// which is where a lexicon-parsed array or nested object lands.
fn record_schema() -> Schema {
    make_schema(&[("rec", "object")], &[])
}

/// The record from the reported reproduction: a scalar array, an array
/// of objects, and a nested object.
fn record_instance(schema: &Schema) -> WInstance {
    let json = serde_json::json!({
        "nums": [1, 2, 3],
        "objs": [{ "a": 1, "b": 10 }, { "a": 2, "b": 20 }],
        "nested": { "a": 7, "b": 70 },
    });
    parse_json(schema, "rec", &json).expect("parse should succeed")
}

/// Parse an expression from source text, as a lens document carries it.
fn expr(src: &str) -> panproto_expr::Expr {
    let tokens = panproto_expr_parser::tokenize(src)
        .unwrap_or_else(|e| panic!("lex failed for `{src}`: {e}"));
    panproto_expr_parser::parse(&tokens)
        .unwrap_or_else(|e| panic!("parse failed for `{src}`: {e:?}"))
}

/// Run a single transform over the fixture record and return the output JSON.
fn run_transform(transform: FieldTransform) -> serde_json::Value {
    let schema = record_schema();
    let instance = record_instance(&schema);

    let mut field_transforms = HashMap::new();
    field_transforms.insert(Name::from("rec"), vec![transform]);

    let migration = CompiledMigration {
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
    };

    let restricted =
        wtype_restrict(&instance, &schema, &schema, &migration).expect("restrict should succeed");
    to_json(&schema, &restricted)
}

fn apply_expr(key: &str, src: &str) -> FieldTransform {
    FieldTransform::ApplyExpr {
        key: key.to_string(),
        expr: expr(src),
        inverse: None,
        coercion_class: CoercionClass::Projection,
    }
}

fn compute_field(target: &str, src: &str) -> FieldTransform {
    FieldTransform::ComputeField {
        target_key: target.to_string(),
        expr: expr(src),
        inverse: None,
        coercion_class: CoercionClass::Projection,
    }
}

#[test]
fn apply_expr_maps_over_an_integer_array() {
    let out = run_transform(apply_expr("nums", "map (\\x -> x + 1) nums"));
    assert_eq!(
        out["nums"],
        serde_json::json!([2, 3, 4]),
        "map over an integer array must rewrite it, not leave it untouched"
    );
}

#[test]
fn apply_expr_projects_a_field_from_an_object_array() {
    let out = run_transform(apply_expr("objs", "map (\\o -> o.a) objs"));
    assert_eq!(
        out["objs"],
        serde_json::json!([1, 2]),
        "field projection must reach into each object of an array"
    );
}

#[test]
fn compute_field_reads_through_a_nested_object() {
    let out = run_transform(compute_field("out", "nested.a"));
    assert_eq!(
        out["out"],
        serde_json::json!(7),
        "field access into a nested object must resolve"
    );
}

#[test]
fn compute_field_folds_over_an_array() {
    let out = run_transform(compute_field("out", "fold (\\x -> \\y -> x + y) 0 nums"));
    assert_eq!(
        out["out"],
        serde_json::json!(6),
        "fold over an integer array must reduce it"
    );
}

#[test]
fn compute_field_filters_an_array() {
    let out = run_transform(compute_field("big", "filter (\\x -> x > 1) nums"));
    assert_eq!(out["big"], serde_json::json!([2, 3]));
}

#[test]
fn compute_field_regroups_an_object_array_into_nested_objects() {
    // The flat-to-nested regroup shape this was blocked on: rebuild an
    // array of objects with some fields moved one level deeper. Requires
    // structure preservation in both directions within one transform.
    let out = run_transform(compute_field(
        "regrouped",
        "map (\\o -> { outer = o.a, inner = { deep = o.b } }) objs",
    ));
    assert_eq!(
        out["regrouped"],
        serde_json::json!([
            { "outer": 1, "inner": { "deep": 10 } },
            { "outer": 2, "inner": { "deep": 20 } },
        ]),
        "a computed array of nested objects must serialize as structured JSON"
    );
}

#[test]
fn transforms_compose_over_containers() {
    // A second transform must be able to read the first one's output,
    // which only holds if the value written back is structured rather
    // than flattened.
    let schema = record_schema();
    let instance = record_instance(&schema);

    let mut field_transforms = HashMap::new();
    field_transforms.insert(
        Name::from("rec"),
        vec![
            apply_expr("nums", "map (\\x -> x * 10) nums"),
            compute_field("total", "fold (\\x -> \\y -> x + y) 0 nums"),
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
        op_term_assignments: HashMap::new(),
        expansion_path: HashMap::new(),
    };

    let restricted =
        wtype_restrict(&instance, &schema, &schema, &migration).expect("restrict should succeed");
    let out = to_json(&schema, &restricted);

    assert_eq!(out["nums"], serde_json::json!([10, 20, 30]));
    assert_eq!(
        out["total"],
        serde_json::json!(60),
        "the fold must see the mapped list, not the original"
    );
}

#[test]
fn container_fields_survive_an_unrelated_transform() {
    // A transform touching one field must not disturb the containers it
    // does not name: they pass through the environment and back out.
    let out = run_transform(compute_field("copy", "nested.b"));
    assert_eq!(out["nums"], serde_json::json!([1, 2, 3]));
    assert_eq!(
        out["objs"],
        serde_json::json!([{ "a": 1, "b": 10 }, { "a": 2, "b": 20 }])
    );
    assert_eq!(out["nested"], serde_json::json!({ "a": 7, "b": 70 }));
    assert_eq!(out["copy"], serde_json::json!(70));
}

#[test]
fn a_failing_transform_reports_rather_than_no_opping() {
    // The diagnosability half: an expression that cannot evaluate must
    // surface, since a silent skip is indistinguishable from a transform
    // that ran and changed nothing.
    let schema = record_schema();
    let instance = record_instance(&schema);

    let mut field_transforms = HashMap::new();
    field_transforms.insert(
        Name::from("rec"),
        vec![compute_field("out", "missing_field.a")],
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
        op_term_assignments: HashMap::new(),
        expansion_path: HashMap::new(),
    };

    let err = wtype_restrict(&instance, &schema, &schema, &migration)
        .expect_err("an unevaluable transform must fail the restrict");
    let msg = err.to_string();
    assert!(
        msg.contains("out"),
        "the error should name the offending field, got: {msg}"
    );
}

#[test]
fn membership_predicate_reads_a_list_field() {
    // `contains` over a list field tests element membership. This is the
    // use the old joined-string conversion existed to serve.
    let schema = record_schema();
    let json = serde_json::json!({ "tags": ["alpha", "beta"] });
    let instance = parse_json(&schema, "rec", &json).expect("parse should succeed");

    let mut field_transforms = HashMap::new();
    field_transforms.insert(
        Name::from("rec"),
        vec![FieldTransform::Case {
            branches: vec![panproto_inst::CaseBranch {
                predicate: expr("contains tags \"beta\""),
                transforms: vec![FieldTransform::AddField {
                    key: "matched".into(),
                    value: Value::Bool(true),
                }],
            }],
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
        op_term_assignments: HashMap::new(),
        expansion_path: HashMap::new(),
    };

    let restricted =
        wtype_restrict(&instance, &schema, &schema, &migration).expect("restrict should succeed");
    let out = to_json(&schema, &restricted);
    assert_eq!(
        out["matched"],
        serde_json::json!(true),
        "contains must test membership on a list-valued field"
    );
}
