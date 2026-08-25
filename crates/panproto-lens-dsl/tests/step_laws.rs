//! DSL front-end lens-law coverage across the `Step` constructors.
//!
//! Each test compiles a real [`LensDocument`](panproto_lens_dsl::LensDocument)
//! through the DSL compiler and law-checks the construct it produces with the
//! panproto-lens law runners
//! ([`check_get_put`](panproto_lens::laws::check_get_put) and friends).
//!
//! The DSL `Step` enum has 19 constructors. They split into two compile
//! targets:
//!
//! - Schema-level steps produce a [`ProtolensChain`]; the chain is
//!   instantiated against a fixture schema and law-checked directly.
//! - Value-level steps (`apply_expr`, `compute_field`) produce a
//!   [`FieldTransform`](panproto_inst::FieldTransform) and an empty chain.
//!   Their round-trip fidelity lives in the migration's `field_transforms`,
//!   so each is folded into an identity migration over the fixture and the
//!   same law runner is applied to the resulting lens.
//!
//! One constructor is lossy by design: `merge_sorts` collapses two carriers
//! into one, which is not invertible. Its test asserts the honest lossy
//! shape (the compiled step is not lossless and its complement retains the
//! pre-merge data) rather than a round-trip law it cannot satisfy.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::collections::{HashMap, HashSet};

use panproto_gat::Name;
use panproto_inst::value::{FieldPresence, Value};
use panproto_inst::{CompiledMigration, Node, WInstance};
use panproto_lens::laws::check_get_put;
use panproto_lens::{ComplementConstructor, Lens};
use panproto_schema::{Edge, Protocol, Schema, SchemaBuilder};

use panproto_lens_dsl::{CompiledLens, LensDocument, compile};

/// Parent vertex under which the DSL adds and removes fields.
const BODY: &str = "doc";

/// Open protocol carrying the object kinds the fixtures use.
fn step_protocol() -> Protocol {
    Protocol {
        name: "test".into(),
        schema_theory: "ThGraph".into(),
        instance_theory: "ThWType".into(),
        edge_rules: vec![],
        obj_kinds: vec![
            "object".into(),
            "string".into(),
            "boolean".into(),
            "integer".into(),
        ],
        constraint_sorts: vec![],
        ..Protocol::default()
    }
}

/// `compose` bodies are absent from these documents, so the resolver never
/// fires.
const fn null_resolver(_: &str) -> Option<CompiledLens> {
    None
}

fn edge(src: &str, tgt: &str, kind: &str, label: &str) -> Edge {
    Edge {
        src: Name::from(src),
        tgt: Name::from(tgt),
        kind: Name::from(kind),
        name: Some(Name::from(label)),
    }
}

/// Fixture schema: `doc` object with a `title` string child and a nested
/// `meta` object carrying an `author` string grandchild.
fn nested_schema() -> Schema {
    SchemaBuilder::new(&step_protocol())
        .entry("doc")
        .vertex("doc", "object", None)
        .unwrap()
        .vertex("doc.title", "string", None)
        .unwrap()
        .vertex("doc.meta", "object", None)
        .unwrap()
        .vertex("doc.meta.author", "string", None)
        .unwrap()
        .edge("doc", "doc.title", "prop", Some("title"))
        .unwrap()
        .edge("doc", "doc.meta", "prop", Some("meta"))
        .unwrap()
        .edge("doc.meta", "doc.meta.author", "prop", Some("author"))
        .unwrap()
        .build()
        .unwrap()
}

/// Instance matching [`nested_schema`].
fn nested_instance() -> WInstance {
    let mut nodes = HashMap::new();
    nodes.insert(0, Node::new(0, "doc"));
    nodes.insert(
        1,
        Node::new(1, "doc.title").with_value(FieldPresence::Present(Value::Str("T".into()))),
    );
    nodes.insert(2, Node::new(2, "doc.meta"));
    nodes.insert(
        3,
        Node::new(3, "doc.meta.author").with_value(FieldPresence::Present(Value::Str("A".into()))),
    );
    let arcs = vec![
        (0, 1, edge("doc", "doc.title", "prop", "title")),
        (0, 2, edge("doc", "doc.meta", "prop", "meta")),
        (2, 3, edge("doc.meta", "doc.meta.author", "prop", "author")),
    ];
    WInstance::new(nodes, arcs, vec![], 0, Name::from("doc"))
}

/// Fixture whose `ghost` vertex is reachable by an `attr`-kind edge with no
/// instance node. Dropping that edge or its op orphans no surviving node, so
/// the drop round-trips cleanly.
fn detachable_schema() -> Schema {
    SchemaBuilder::new(&step_protocol())
        .entry("doc")
        .vertex("doc", "object", None)
        .unwrap()
        .vertex("doc.title", "string", None)
        .unwrap()
        .vertex("doc.ghost", "string", None)
        .unwrap()
        .edge("doc", "doc.title", "prop", Some("title"))
        .unwrap()
        .edge("doc", "doc.ghost", "attr", Some("ghost"))
        .unwrap()
        .build()
        .unwrap()
}

/// Instance matching [`detachable_schema`], with no node for `doc.ghost`.
fn detachable_instance() -> WInstance {
    let mut nodes = HashMap::new();
    nodes.insert(0, Node::new(0, "doc"));
    nodes.insert(
        1,
        Node::new(1, "doc.title").with_value(FieldPresence::Present(Value::Str("T".into()))),
    );
    let arcs = vec![(0, 1, edge("doc", "doc.title", "prop", "title"))];
    WInstance::new(nodes, arcs, vec![], 0, Name::from("doc"))
}

/// Instance whose `doc` node carries `field` as an extra field, exercising
/// the value-level steps.
fn valued_instance(field: &str, value: Value) -> WInstance {
    let mut nodes = HashMap::new();
    nodes.insert(0, Node::new(0, "doc").with_extra_field(field, value));
    nodes.insert(
        1,
        Node::new(1, "doc.title").with_value(FieldPresence::Present(Value::Str("T".into()))),
    );
    let arcs = vec![(0, 1, edge("doc", "doc.title", "prop", "title"))];
    WInstance::new(nodes, arcs, vec![], 0, Name::from("doc"))
}

/// Compile a JSON lens document through the DSL.
fn compile_doc(json: &str) -> CompiledLens {
    let doc: LensDocument = panproto_lens_dsl::eval::eval_json(json).expect("document evaluates");
    compile(&doc, BODY, &null_resolver).expect("document compiles")
}

/// Instantiate a compiled document's chain against a fixture and assert
/// `GetPut` holds on the instance.
fn assert_chain_getput(json: &str, schema: &Schema, instance: &WInstance) {
    let compiled = compile_doc(json);
    let proto = step_protocol();
    let lens = compiled
        .instantiate(schema, &proto)
        .expect("compiled document instantiates on the fixture");
    let result = check_get_put(&lens, instance);
    assert!(result.is_ok(), "GetPut should hold: {result:?}");
}

/// Fold a compiled document's value-level field transforms into an identity
/// migration over `schema`, producing a runnable lens.
fn value_lens(compiled: &CompiledLens, schema: &Schema) -> Lens {
    let surviving_verts: HashSet<Name> = schema.vertices.keys().cloned().collect();
    let surviving_edges: HashSet<Edge> = schema.edges.keys().cloned().collect();
    let migration = CompiledMigration {
        surviving_verts,
        surviving_edges,
        vertex_remap: HashMap::new(),
        edge_remap: HashMap::new(),
        resolver: HashMap::new(),
        hyper_resolver: HashMap::new(),
        field_transforms: compiled.field_transforms.clone(),
        conditional_survival: HashMap::new(),
        op_term_assignments: HashMap::new(),
        expansion_path: HashMap::new(),
    };
    Lens {
        compiled: migration,
        src_schema: schema.clone(),
        tgt_schema: schema.clone(),
    }
}

#[test]
fn ordered_stages_compute_in_the_post_rename_field_frame() {
    let schema = SchemaBuilder::new(&step_protocol())
        .entry("doc")
        .vertex("doc", "object", None)
        .unwrap()
        .vertex("doc.count", "integer", None)
        .unwrap()
        .edge("doc", "doc.count", "prop", Some("count"))
        .unwrap()
        .build()
        .unwrap();
    let instance =
        panproto_inst::parse::parse_json(&schema, "doc", &serde_json::json!({"count": 2}))
            .expect("source parses");
    let compiled = compile_doc(
        r#"{ "id": "l", "source": "s", "target": "t", "steps": [
            { "rename_field": { "old": "count", "new": "amount" } },
            { "compute_field": { "target": "derived", "expr": "add amount 1" } }
        ] }"#,
    );

    assert_eq!(compiled.stages.len(), 2);
    let lens = compiled
        .instantiate(&schema, &step_protocol())
        .expect("ordered stages instantiate");
    let (view, _) = panproto_lens::get(&lens, &instance).expect("ordered get succeeds");
    let json = panproto_inst::to_json(&lens.tgt_schema, &view);

    assert_eq!(json.get("amount"), Some(&serde_json::json!(2)));
    assert_eq!(json.get("derived"), Some(&serde_json::json!(3)));
    assert!(json.get("count").is_none());
}

// --- high-level field combinators ---

#[test]
fn step_law_remove_field() {
    // `remove_field` drops the `title` child; the complement restores it.
    assert_chain_getput(
        r#"{ "id": "l", "source": "s", "target": "t",
             "steps": [{ "remove_field": "title" }] }"#,
        &nested_schema(),
        &nested_instance(),
    );
}

#[test]
fn step_law_rename_field() {
    // `rename_field` relabels the `title` edge; a bijective relabel is
    // lossless.
    assert_chain_getput(
        r#"{ "id": "l", "source": "s", "target": "t",
             "steps": [{ "rename_field": { "old": "title", "new": "heading" } }] }"#,
        &nested_schema(),
        &nested_instance(),
    );
}

#[test]
fn step_law_add_field() {
    // `add_field` introduces a `note` string with a default; the added field
    // has no source datum, so the round trip recovers the original.
    assert_chain_getput(
        r#"{ "id": "l", "source": "s", "target": "t",
             "steps": [{ "add_field": { "name": "note", "kind": "string", "fallback": "d" } }] }"#,
        &nested_schema(),
        &nested_instance(),
    );
}

#[test]
fn ordered_stage_keeps_default_synthesized_by_add_field() {
    let compiled = compile_doc(
        r#"{ "id": "l", "source": "s", "target": "t",
             "steps": [{ "add_field": {
                 "name": "note", "kind": "string", "fallback": "d"
             } }] }"#,
    );
    let schema = nested_schema();
    let lens = compiled
        .instantiate(&schema, &step_protocol())
        .expect("add-field document instantiates");
    assert!(
        lens.tgt_schema
            .vertices
            .contains_key(&Name::from("doc.note"))
    );
    assert!(lens.tgt_schema.edges.keys().any(|edge| {
        edge.src == "doc"
            && edge.tgt == "doc.note"
            && edge.kind == "prop"
            && edge.name.as_ref() == Some(&Name::from("note"))
    }));
    let (view, _) = panproto_lens::get(&lens, &nested_instance()).expect("get succeeds");
    let json = panproto_inst::to_json(&lens.tgt_schema, &view);

    assert_eq!(json.get("note"), Some(&serde_json::json!("d")));
    assert!(json.get("doc.note").is_none());
}

// --- value-level transforms ---
//
// `apply_expr` and `compute_field` compile to value-level field transforms
// with an empty protolens chain, so the schema-level lens is the identity.
// Each is folded into an identity migration's `field_transforms` and the
// law runner is applied to the resulting lens.

#[test]
fn step_law_apply_expr() {
    // `add count 1` forward with `sub count 1` inverse is an Iso on the
    // `count` field; the complement snapshot plus the inverse recover the
    // original value.
    let compiled = compile_doc(
        r#"{ "id": "l", "source": "s", "target": "t",
             "steps": [{ "apply_expr": {
                 "field": "count", "expr": "add count 1",
                 "inverse": "sub count 1", "coercion": "iso" } }] }"#,
    );
    let schema = nested_schema();
    let lens = value_lens(&compiled, &schema);
    let instance = valued_instance("count", Value::Int(5));
    let result = check_get_put(&lens, &instance);
    assert!(result.is_ok(), "GetPut should hold: {result:?}");
}

#[test]
fn step_law_compute_field() {
    // `compute_field` derives `derived` from `count` with no inverse
    // (Projection). The forward pass adds the derived field; the complement
    // snapshot restores the pre-compute fields exactly on the way back.
    let compiled = compile_doc(
        r#"{ "id": "l", "source": "s", "target": "t",
             "steps": [{ "compute_field": {
                 "target": "derived", "expr": "count", "coercion": "projection" } }] }"#,
    );
    let schema = nested_schema();
    let lens = value_lens(&compiled, &schema);
    let instance = valued_instance("count", Value::Int(7));
    let result = check_get_put(&lens, &instance);
    assert!(result.is_ok(), "GetPut should hold: {result:?}");
}

// --- structural combinators ---

#[test]
fn step_law_hoist_field() {
    // Hoist `doc.meta.author` from under `doc.meta` up to `doc`.
    assert_chain_getput(
        r#"{ "id": "l", "source": "s", "target": "t",
             "steps": [{ "hoist_field": {
                 "parent": "doc", "intermediate": "doc.meta", "child": "doc.meta.author" } }] }"#,
        &nested_schema(),
        &nested_instance(),
    );
}

#[test]
fn step_law_nest_field() {
    // Nest `doc.title` under a fresh `doc.wrapper` intermediate object.
    assert_chain_getput(
        r#"{ "id": "l", "source": "s", "target": "t",
             "steps": [{ "nest_field": {
                 "parent": "doc", "child": "doc.title", "intermediate": "doc.wrapper",
                 "intermediate_kind": "object", "edge_kind": "prop",
                 "old_edge_name": "title", "parent_to_intermediate": "wrapper",
                 "intermediate_to_child": "title" } }] }"#,
        &nested_schema(),
        &nested_instance(),
    );
}

#[test]
fn step_law_scoped() {
    // Scope a `rename_sort` within the `doc.meta` sub-schema.
    assert_chain_getput(
        r#"{ "id": "l", "source": "s", "target": "t",
             "steps": [{ "scoped": {
                 "focus": "doc.meta",
                 "inner": [{ "rename_sort": { "old": "string", "new": "text" } }] } }] }"#,
        &nested_schema(),
        &nested_instance(),
    );
}

#[test]
fn scoped_transform_only_pipeline_does_not_require_a_structural_chain() {
    let compiled = compile_doc(
        r#"{ "id": "l", "source": "s", "target": "t",
             "steps": [{ "scoped": {
                 "focus": "doc.meta",
                 "inner": [{ "compute_field": {
                     "target": "derived", "expr": "author ++ \"!\""
                 } }]
             } }] }"#,
    );

    assert_eq!(compiled.stages.len(), 1);
    assert!(compiled.stages[0].chain.steps.is_empty());
    let lens = compiled
        .instantiate(&nested_schema(), &step_protocol())
        .expect("transform-only scoped document instantiates");
    let (view, _) = panproto_lens::get(&lens, &nested_instance()).expect("get succeeds");
    let json = panproto_inst::to_json(&lens.tgt_schema, &view);

    assert_eq!(
        json.pointer("/meta/derived"),
        Some(&serde_json::json!("A!"))
    );
}

#[test]
fn scoped_pipeline_preserves_inner_rename_then_compute_order() {
    let compiled = compile_doc(
        r#"{ "id": "l", "source": "s", "target": "t",
             "steps": [{ "scoped": {
                 "focus": "doc.meta",
                 "inner": [
                     { "rename_field": { "old": "author", "new": "writer" } },
                     { "compute_field": {
                         "target": "derived", "expr": "writer ++ \"!\""
                     } }
                 ]
             } }] }"#,
    );

    assert_eq!(compiled.stages.len(), 2);
    let lens = compiled
        .instantiate(&nested_schema(), &step_protocol())
        .expect("ordered scoped document instantiates");
    let (view, _) = panproto_lens::get(&lens, &nested_instance()).expect("get succeeds");
    let json = panproto_inst::to_json(&lens.tgt_schema, &view);

    assert_eq!(json.pointer("/meta/writer"), Some(&serde_json::json!("A")));
    assert_eq!(
        json.pointer("/meta/derived"),
        Some(&serde_json::json!("A!"))
    );
    assert!(json.pointer("/meta/author").is_none());
}

#[test]
fn step_law_pullback() {
    // Pullback along a morphism with identity sort and op maps is a lossless
    // schema no-op.
    assert_chain_getput(
        r#"{ "id": "l", "source": "s", "target": "t",
             "steps": [{ "pullback": { "name": "id", "domain": "T", "codomain": "T" } }] }"#,
        &nested_schema(),
        &nested_instance(),
    );
}

// --- sort-level coercions and merges ---

#[test]
fn step_law_coerce_sort() {
    // An identity Iso coercion on the `string` sort is lossless. The
    // declared class is honest, so the step compiles and round-trips.
    assert_chain_getput(
        r#"{ "id": "l", "source": "s", "target": "t",
             "steps": [{ "coerce_sort": {
                 "sort": "string", "source_kind": "string", "target_kind": "string",
                 "expr": "v", "inverse": "v", "coercion": "iso" } }] }"#,
        &nested_schema(),
        &nested_instance(),
    );
}

#[test]
fn step_law_merge_sorts() {
    // `merge_sorts` is the lossy constructor: collapsing two distinct
    // carriers into one is not invertible, so no round-trip law holds for
    // the merged component. The honest assertion is on shape: the compiled
    // step is not lossless, and its complement retains the pre-merge data
    // for each source sort.
    let compiled = compile_doc(
        r#"{ "id": "l", "source": "s", "target": "t",
             "steps": [{ "merge_sorts": {
                 "sort_a": "string", "sort_b": "object", "merged": "any", "expr": "v" } }] }"#,
    );
    let step = &compiled.chain.steps[0];
    assert!(!step.is_lossless(), "a merge is lossy");
    assert!(
        matches!(&step.complement_constructor, ComplementConstructor::Composite(parts)
            if parts.iter().all(|c| matches!(c, ComplementConstructor::DroppedSortData { .. }))),
        "merge complement must retain dropped-sort data for each source sort, got {:?}",
        step.complement_constructor,
    );
}

// --- elementary theory operations ---

#[test]
fn step_law_add_sort() {
    // Add a fresh `extra` string sort; the added sort has no instance datum.
    assert_chain_getput(
        r#"{ "id": "l", "source": "s", "target": "t",
             "steps": [{ "add_sort": { "name": "extra", "kind": "string" } }] }"#,
        &nested_schema(),
        &nested_instance(),
    );
}

#[test]
fn step_law_drop_sort() {
    // Drop the `doc.title` leaf vertex by id; the complement restores it.
    assert_chain_getput(
        r#"{ "id": "l", "source": "s", "target": "t",
             "steps": [{ "drop_sort": "doc.title" }] }"#,
        &nested_schema(),
        &nested_instance(),
    );
}

#[test]
fn step_law_rename_sort() {
    // Rename the `string` sort to `text`; a bijective rename is lossless.
    assert_chain_getput(
        r#"{ "id": "l", "source": "s", "target": "t",
             "steps": [{ "rename_sort": { "old": "string", "new": "text" } }] }"#,
        &nested_schema(),
        &nested_instance(),
    );
}

#[test]
fn step_law_add_op() {
    // Add an `extra_op` prop edge between existing vertices.
    assert_chain_getput(
        r#"{ "id": "l", "source": "s", "target": "t",
             "steps": [{ "add_op": {
                 "name": "extra_op", "src": "doc", "tgt": "doc.title", "kind": "prop" } }] }"#,
        &nested_schema(),
        &nested_instance(),
    );
}

#[test]
fn step_law_drop_op() {
    // Drop the `attr` op, whose only edge targets a vertex with no instance
    // node, so no surviving node is orphaned.
    assert_chain_getput(
        r#"{ "id": "l", "source": "s", "target": "t",
             "steps": [{ "drop_op": "attr" }] }"#,
        &detachable_schema(),
        &detachable_instance(),
    );
}

#[test]
fn step_law_rename_op() {
    // Rename the `prop` op to `field`; a bijective op rename is lossless.
    assert_chain_getput(
        r#"{ "id": "l", "source": "s", "target": "t",
             "steps": [{ "rename_op": { "old": "prop", "new": "field" } }] }"#,
        &nested_schema(),
        &nested_instance(),
    );
}

#[test]
fn step_law_add_equation() {
    // Add a reflexivity equation; a theory-only equation addition has an
    // empty complement, so the lens is the identity.
    assert_chain_getput(
        r#"{ "id": "l", "source": "s", "target": "t",
             "steps": [{ "add_equation": { "name": "refl", "lhs": "x", "rhs": "x" } }] }"#,
        &nested_schema(),
        &nested_instance(),
    );
}

#[test]
fn step_law_drop_equation() {
    // Dropping an absent equation is a theory-level no-op with an empty
    // complement, so the lens is the identity.
    assert_chain_getput(
        r#"{ "id": "l", "source": "s", "target": "t",
             "steps": [{ "drop_equation": "nonexistent_eq" }] }"#,
        &nested_schema(),
        &nested_instance(),
    );
}
