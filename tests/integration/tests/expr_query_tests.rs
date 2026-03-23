#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::cast_possible_truncation
)]
//! Integration tests for the expression parser and query engine.
//!
//! Verifies parse/eval round-trips for arithmetic, lambda, let, string,
//! boolean, and comparison expressions; parse/pretty-print round-trips;
//! query execution with predicates on W-type instances; and fiber
//! operations (decomposition, restrict with complement, fiber at node).

use std::collections::{HashMap, HashSet};

use panproto_expr::{BuiltinOp, Env, EvalConfig, Expr, Literal, eval};
use panproto_gat::Name;
use panproto_inst::query::{InstanceQuery, execute};
use panproto_inst::value::Value;
use panproto_inst::wtype::{CompiledMigration, WInstance};
use panproto_inst::{
    Node, fiber_at_anchor, fiber_at_node, fiber_decomposition, restrict_with_complement,
};
use panproto_schema::{Edge, Schema, Vertex};
use smallvec::SmallVec;

// ═══════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════

/// Build a schema with the given vertices and edges.
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

/// Parse and evaluate a surface-syntax expression string.
fn parse_and_eval(source: &str) -> Literal {
    let tokens = panproto_expr_parser::tokenize(source).expect("tokenize failed");
    let expr = panproto_expr_parser::parse(&tokens).expect("parse failed");
    eval(&expr, &Env::new(), &EvalConfig::default()).expect("eval failed")
}

// ═══════════════════════════════════════════════════════════════════
// 1. Parse/eval round-trip tests
// ═══════════════════════════════════════════════════════════════════

#[test]
fn parse_eval_arithmetic() {
    let result = parse_and_eval("2 + 3 * 4");
    assert_eq!(result, Literal::Int(14));
}

#[test]
fn parse_eval_lambda() {
    let result = parse_and_eval("(\\x -> x + 1) 5");
    assert_eq!(result, Literal::Int(6));
}

#[test]
fn parse_eval_let_binding() {
    let result = parse_and_eval("let x = 10 in x * 2");
    assert_eq!(result, Literal::Int(20));
}

#[test]
fn parse_eval_string_concat() {
    let result = parse_and_eval("concat \"hello\" \" world\"");
    assert_eq!(result, Literal::Str("hello world".into()));
}

#[test]
fn parse_eval_boolean_and() {
    let result = parse_and_eval("True && False");
    assert_eq!(result, Literal::Bool(false));
}

#[test]
fn parse_eval_comparison() {
    let result = parse_and_eval("3 > 2");
    assert_eq!(result, Literal::Bool(true));
}

// ═══════════════════════════════════════════════════════════════════
// 2. Parse/pretty-print round-trip tests
// ═══════════════════════════════════════════════════════════════════

#[test]
fn parse_pretty_roundtrip() {
    let sources = [
        "\\x -> x + 1",
        "let x = 1 in x + 2",
        "if True then 1 else 2",
    ];
    let cfg = EvalConfig::default();
    let env = Env::new();

    for source in sources {
        let tokens = panproto_expr_parser::tokenize(source).expect("tokenize failed");
        let expr = panproto_expr_parser::parse(&tokens).expect("parse failed");
        let pretty = panproto_expr_parser::pretty_print(&expr);

        // Re-parse the pretty-printed output
        let tokens2 = panproto_expr_parser::tokenize(&pretty).expect("re-tokenize failed");
        let expr2 = panproto_expr_parser::parse(&tokens2).expect("re-parse failed");

        // Both should evaluate to the same result
        let r1 = eval(&expr, &env, &cfg).expect("eval1 failed");
        let r2 = eval(&expr2, &env, &cfg).expect("eval2 failed");
        assert_eq!(r1, r2, "roundtrip failed for: {source}");
    }
}

// ═══════════════════════════════════════════════════════════════════
// 3. Query on instance
// ═══════════════════════════════════════════════════════════════════

#[test]
fn query_on_instance() {
    let edge = Edge {
        src: Name::from("root"),
        tgt: Name::from("item"),
        kind: Name::from("child"),
        name: None,
    };

    let schema = make_schema(
        &[("root", "object"), ("item", "object")],
        std::slice::from_ref(&edge),
    );

    // Build instance with 3 item nodes
    let mut nodes = HashMap::new();
    nodes.insert(0, Node::new(0, "root"));

    let mut n1 = Node::new(1, "item");
    n1.extra_fields
        .insert("name".into(), Value::Str("apple".into()));
    n1.extra_fields
        .insert("category".into(), Value::Str("fruit".into()));
    nodes.insert(1, n1);

    let mut n2 = Node::new(2, "item");
    n2.extra_fields
        .insert("name".into(), Value::Str("carrot".into()));
    n2.extra_fields
        .insert("category".into(), Value::Str("vegetable".into()));
    nodes.insert(2, n2);

    let mut n3 = Node::new(3, "item");
    n3.extra_fields
        .insert("name".into(), Value::Str("banana".into()));
    n3.extra_fields
        .insert("category".into(), Value::Str("fruit".into()));
    nodes.insert(3, n3);

    let arcs = vec![(0, 1, edge.clone()), (0, 2, edge.clone()), (0, 3, edge)];
    let inst = WInstance::new(nodes, arcs, vec![], 0, Name::from("root"));

    // Query: all items with category == "fruit"
    let predicate = Expr::Builtin(
        BuiltinOp::Eq,
        vec![
            Expr::Var("category".into()),
            Expr::Lit(Literal::Str("fruit".into())),
        ],
    );

    let query = InstanceQuery {
        anchor: Name::from("item"),
        predicate: Some(predicate),
        group_by: None,
        project: None,
        limit: None,
        path: vec![],
    };

    let results = execute(&query, &inst, &schema);
    assert_eq!(results.len(), 2);
    // Both should be fruits
    for r in &results {
        assert_eq!(r.fields.get("category"), Some(&Value::Str("fruit".into())));
    }
}

#[test]
fn query_with_projection_and_limit() {
    let edge = Edge {
        src: Name::from("root"),
        tgt: Name::from("item"),
        kind: Name::from("child"),
        name: None,
    };

    let schema = make_schema(
        &[("root", "object"), ("item", "object")],
        std::slice::from_ref(&edge),
    );

    let mut nodes = HashMap::new();
    nodes.insert(0, Node::new(0, "root"));

    for i in 1..=5 {
        let mut n = Node::new(i, "item");
        n.extra_fields
            .insert("name".into(), Value::Str(format!("item_{i}")));
        n.extra_fields
            .insert("score".into(), Value::Int(i64::from(i) * 10));
        nodes.insert(i, n);
    }

    let arcs: Vec<_> = (1..=5).map(|i| (0, i, edge.clone())).collect();
    let inst = WInstance::new(nodes, arcs, vec![], 0, Name::from("root"));

    // Query with projection and limit
    let query = InstanceQuery {
        anchor: Name::from("item"),
        predicate: None,
        group_by: None,
        project: Some(vec!["name".into()]),
        limit: Some(3),
        path: vec![],
    };

    let results = execute(&query, &inst, &schema);
    assert_eq!(results.len(), 3);
    // Only "name" field should be present, not "score"
    for r in &results {
        assert!(r.fields.contains_key("name"));
        assert!(!r.fields.contains_key("score"));
    }
}

// ═══════════════════════════════════════════════════════════════════
// 4. Fiber operations integration
// ═══════════════════════════════════════════════════════════════════

#[test]
fn fiber_operations_integration() {
    // Source schema: doc -> annotation, doc -> text
    let doc_ann_edge = Edge {
        src: Name::from("doc"),
        tgt: Name::from("annotation"),
        kind: Name::from("child"),
        name: None,
    };
    let doc_text_edge = Edge {
        src: Name::from("doc"),
        tgt: Name::from("text"),
        kind: Name::from("child"),
        name: None,
    };

    let src_schema = make_schema(
        &[
            ("doc", "object"),
            ("annotation", "object"),
            ("text", "object"),
        ],
        &[doc_ann_edge.clone(), doc_text_edge.clone()],
    );
    let tgt_schema = make_schema(
        &[("doc", "object"), ("text", "object")],
        std::slice::from_ref(&doc_text_edge),
    );

    // Migration: doc -> doc, text -> text; annotation is dropped
    let mut vertex_remap = HashMap::new();
    vertex_remap.insert(Name::from("doc"), Name::from("doc"));
    vertex_remap.insert(Name::from("text"), Name::from("text"));
    let migration = CompiledMigration {
        surviving_verts: ["doc", "text"].iter().map(|s| Name::from(*s)).collect(),
        surviving_edges: HashSet::new(),
        vertex_remap,
        edge_remap: HashMap::new(),
        resolver: HashMap::new(),
        hyper_resolver: HashMap::new(),
        field_transforms: HashMap::new(),
        conditional_survival: HashMap::new(),
    };

    // Source instance: doc(0) -> annotation(1), doc(0) -> annotation(2), doc(0) -> text(3)
    let mut nodes = HashMap::new();
    nodes.insert(0, Node::new(0, "doc"));

    let mut ann1 = Node::new(1, "annotation");
    ann1.extra_fields
        .insert("label".into(), Value::Str("ingredient".into()));
    nodes.insert(1, ann1);

    let mut ann2 = Node::new(2, "annotation");
    ann2.extra_fields
        .insert("label".into(), Value::Str("step".into()));
    nodes.insert(2, ann2);

    nodes.insert(3, Node::new(3, "text"));

    let source = WInstance::new(
        nodes,
        vec![
            (0, 1, doc_ann_edge.clone()),
            (0, 2, doc_ann_edge),
            (0, 3, doc_text_edge),
        ],
        vec![],
        0,
        Name::from("doc"),
    );

    // Test fiber_at_anchor: annotations map to no surviving anchor,
    // doc maps to "doc", text maps to "text"
    let fiber_doc = fiber_at_anchor(&migration, &source, &Name::from("doc"));
    assert_eq!(fiber_doc, vec![0]);

    let fiber_text = fiber_at_anchor(&migration, &source, &Name::from("text"));
    assert_eq!(fiber_text, vec![3]);

    // Annotations have no target anchor in surviving_verts, so they
    // do appear in fiber_decomposition under their remapped name (which
    // maps to themselves since not in vertex_remap).
    let fibers = fiber_decomposition(&migration, &source);
    // doc and text are remapped; annotations are NOT in vertex_remap
    // so they don't appear in any fiber.
    let mut all_ids: Vec<u32> = fibers.values().flatten().copied().collect();
    all_ids.sort_unstable();
    assert_eq!(all_ids, vec![0, 3]);
    assert_eq!(fibers.len(), 2);

    // Test restrict_with_complement
    let (restricted, complement) =
        restrict_with_complement(&source, &src_schema, &tgt_schema, &migration)
            .expect("restrict failed");

    // Restricted should have 2 nodes: doc and text
    assert_eq!(restricted.nodes.len(), 2);
    assert!(restricted.nodes.contains_key(&0));
    assert!(restricted.nodes.contains_key(&3));

    // Complement should have 2 dropped nodes (both annotations)
    assert_eq!(complement.dropped_nodes.len(), 2);
    for dropped in &complement.dropped_nodes {
        assert_eq!(dropped.anchor, Name::from("annotation"));
        assert_eq!(dropped.contracted_into, Some(0));
    }

    // Test fiber_at_node: root should include direct match + contracted nodes
    let fiber_root = fiber_at_node(&source, &restricted, 0, &complement);
    assert!(fiber_root.contains(&0)); // direct preimage
    assert!(fiber_root.contains(&1)); // contracted annotation
    assert!(fiber_root.contains(&2)); // contracted annotation
    assert_eq!(fiber_root.len(), 3);

    // fiber_at_node for text should be just the text node
    let fiber_text_node = fiber_at_node(&source, &restricted, 3, &complement);
    assert!(fiber_text_node.contains(&3));
    assert_eq!(fiber_text_node.len(), 1);
}
