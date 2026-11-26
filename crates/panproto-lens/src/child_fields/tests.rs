//! The cost of the search for the fields children hold, counted in node
//! runs, and the coordinates it searches together.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::{HashMap, HashSet};

use panproto_expr::{BuiltinOp, Expr, Literal};
use panproto_gat::CoercionClass;
use panproto_inst::value::FieldPresence;
use panproto_inst::{CompiledMigration, FieldTransform, Node, RestrictPlan, Value, WInstance};
use panproto_schema::{Edge, Protocol, Schema, SchemaBuilder};

use super::Search;
use crate::Lens;

fn var(name: &str) -> Expr {
    Expr::Var(name.into())
}

const fn lit(value: i64) -> Expr {
    Expr::Lit(Literal::Int(value))
}

const fn op(op: BuiltinOp, arguments: Vec<Expr>) -> Expr {
    Expr::Builtin(op, arguments)
}

fn rewrite(key: &str, expr: Expr, inverse: Expr) -> FieldTransform {
    FieldTransform::ApplyExpr {
        key: key.to_owned(),
        expr,
        inverse: Some(inverse),
        coercion_class: CoercionClass::Iso,
    }
}

/// `key * 2`, declared inverted by `key / 2`, which no odd value is the
/// image of.
fn doubled(key: &str) -> FieldTransform {
    rewrite(
        key,
        op(BuiltinOp::Mul, vec![var(key), lit(2)]),
        op(BuiltinOp::Div, vec![var(key), lit(2)]),
    )
}

fn edge(src: &str, tgt: &str, kind: &str, name: Option<&str>) -> Edge {
    Edge {
        src: src.into(),
        tgt: tgt.into(),
        kind: kind.into(),
        name: name.map(Into::into),
    }
}

fn lens(schema: Schema, anchor: &str, transforms: Vec<FieldTransform>) -> Lens {
    Lens {
        compiled: CompiledMigration {
            surviving_verts: schema.vertices.keys().cloned().collect::<HashSet<_>>(),
            surviving_edges: schema.edges.keys().cloned().collect(),
            field_transforms: [(anchor.into(), transforms)].into(),
            ..CompiledMigration::default()
        },
        src_schema: schema.clone(),
        tgt_schema: schema,
    }
}

fn leaf(nodes: &mut HashMap<u32, Node>, id: u32, anchor: &str, value: i64) {
    let mut node = Node::new(id, anchor);
    node.value = Some(FieldPresence::Present(Value::Int(value)));
    nodes.insert(id, node);
}

/// A list `s` of `items` records, record `i` holding `n` and `m` equal to
/// `i`; the record of item `i` is node `3 i + 1`, its `n` node `3 i + 2`.
fn list(items: u32) -> (Schema, WInstance) {
    let protocol = Protocol::default();
    let schema = SchemaBuilder::new(&protocol)
        .vertex("s", "object", None)
        .and_then(|b| b.vertex("r", "object", None))
        .and_then(|b| b.vertex("r.n", "integer", None))
        .and_then(|b| b.vertex("r.m", "integer", None))
        .and_then(|b| b.edge("s", "r", "item", None))
        .and_then(|b| b.edge("r", "r.n", "prop", Some("n")))
        .and_then(|b| b.edge("r", "r.m", "prop", Some("m")))
        .and_then(SchemaBuilder::build)
        .unwrap();
    let mut nodes = HashMap::from([(0, Node::new(0, "s"))]);
    let mut arcs = Vec::new();
    for item in 0..items {
        let record = 3 * item + 1;
        nodes.insert(record, Node::new(record, "r"));
        leaf(&mut nodes, record + 1, "r.n", i64::from(item));
        leaf(&mut nodes, record + 2, "r.m", i64::from(item));
        arcs.push((0, record, edge("s", "r", "item", None)));
        arcs.push((record, record + 1, edge("r", "r.n", "prop", Some("n"))));
        arcs.push((record, record + 2, edge("r", "r.m", "prop", Some("m"))));
    }
    (
        schema,
        WInstance::new(nodes, arcs, Vec::new(), 0, "s".into()),
    )
}

/// A record holding the fields `k0` to `k{fields - 1}`, each 1; field `i`
/// is node `i + 1`.
fn record(fields: u32) -> (Schema, WInstance) {
    let protocol = Protocol::default();
    let mut builder = SchemaBuilder::new(&protocol).vertex("rec", "object", None);
    for field in 0..fields {
        let (key, vertex) = (format!("k{field}"), format!("rec.k{field}"));
        builder = builder
            .and_then(|b| b.vertex(&vertex, "integer", None))
            .and_then(|b| b.edge("rec", &vertex, "prop", Some(&key)));
    }
    let schema = builder.and_then(SchemaBuilder::build).unwrap();
    let mut nodes = HashMap::from([(0, Node::new(0, "rec"))]);
    let mut arcs = Vec::new();
    for field in 0..fields {
        let (key, vertex) = (format!("k{field}"), format!("rec.k{field}"));
        leaf(&mut nodes, field + 1, &vertex, 1);
        arcs.push((0, field + 1, edge("rec", &vertex, "prop", Some(&key))));
    }
    (
        schema,
        WInstance::new(nodes, arcs, Vec::new(), 0, "rec".into()),
    )
}

/// Put of `source`'s view with the child `child` set to `value`: the
/// restored source, or none for a refusal, and the node runs it took.
fn put_with(lens: &Lens, source: &WInstance, child: u32, value: i64) -> (Option<WInstance>, usize) {
    let (mut view, complement) = crate::get(lens, source).unwrap();
    view.nodes.get_mut(&child).unwrap().value = Some(FieldPresence::Present(Value::Int(value)));
    let plan = RestrictPlan::compile(&lens.src_schema, &lens.tgt_schema, &lens.compiled).unwrap();
    let search = Search::new(lens, &view, &complement, &plan).unwrap();
    let restored = search.source().ok();
    (restored, search.runs.get())
}

fn value_of(tree: &WInstance, id: u32) -> Option<&Value> {
    match &tree.nodes[&id].value {
        Some(FieldPresence::Present(value)) => Some(value),
        _ => None,
    }
}

#[test]
fn an_unreachable_edit_of_one_item_of_a_long_list_is_refused_in_runs_linear_in_the_list() {
    let items = 1000;
    let (schema, source) = list(items);
    let lens = lens(schema, "r", vec![doubled("n"), doubled("n")]);
    let (restored, runs) = put_with(&lens, &source, 2, 7);
    assert!(restored.is_none(), "no source shows 7 at the first item");
    assert!(runs <= 20 * items as usize, "{runs} node runs");
}

#[test]
fn an_unreachable_edit_of_one_of_many_independent_fields_is_refused_in_few_runs() {
    let fields = 64;
    let (schema, source) = record(fields);
    let transforms = (0..fields)
        .flat_map(|field| {
            let key = format!("k{field}");
            [doubled(&key), doubled(&key), doubled(&key)]
        })
        .collect();
    let lens = lens(schema, "rec", transforms);
    let (restored, runs) = put_with(&lens, &source, 1, 9);
    assert!(restored.is_none(), "no source shows 9 at k0");
    assert!(runs <= 200 * fields as usize, "{runs} node runs");
}

#[test]
fn a_reachable_edit_of_one_item_of_a_long_list_is_put_back_alone() {
    let items = 1000;
    let (schema, source) = list(items);
    let lens = lens(schema, "r", vec![doubled("n"), doubled("n")]);
    let (restored, runs) = put_with(&lens, &source, 3 * 7 + 2, 8);
    let restored = restored.expect("2 shows 8");
    assert_eq!(value_of(&restored, 3 * 7 + 2), Some(&Value::Int(2)));
    for item in (0..items).filter(|item| *item != 7) {
        assert_eq!(
            value_of(&restored, 3 * item + 2),
            Some(&Value::Int(i64::from(item)))
        );
    }
    assert!(runs <= 20 * items as usize, "{runs} node runs");
}

/// `k0` is rewritten only where `k1`, rewritten first, shows 10, so the
/// candidates of `k0` depend on those of `k1` and the two are searched
/// together.
#[test]
fn fields_one_rewrite_reads_together_are_searched_together() {
    let (schema, source) = record(2);
    let tenfold = rewrite(
        "k1",
        op(BuiltinOp::Mul, vec![var("k1"), lit(10)]),
        op(BuiltinOp::Div, vec![var("k1"), lit(10)]),
    );
    let guarded = FieldTransform::Case {
        branches: vec![panproto_inst::CaseBranch {
            predicate: op(BuiltinOp::Eq, vec![var("k1"), lit(10)]),
            transforms: vec![rewrite(
                "k0",
                op(BuiltinOp::Add, vec![var("k0"), lit(1)]),
                op(BuiltinOp::Sub, vec![var("k0"), lit(1)]),
            )],
        }],
    };
    let lens = lens(schema, "rec", vec![tenfold, guarded]);
    let plan = RestrictPlan::compile(&lens.src_schema, &lens.tgt_schema, &lens.compiled).unwrap();
    let (mut view, complement) = crate::get(&lens, &source).unwrap();
    assert_eq!(value_of(&view, 1), Some(&Value::Int(2)));
    assert_eq!(value_of(&view, 2), Some(&Value::Int(10)));
    view.nodes.get_mut(&1).unwrap().value = Some(FieldPresence::Present(Value::Int(9)));
    view.nodes.get_mut(&2).unwrap().value = Some(FieldPresence::Present(Value::Int(20)));
    let search = Search::new(&lens, &view, &complement, &plan).unwrap();
    assert_eq!(search.parts(), vec![vec![0, 1]]);
    let restored = search.source().unwrap();
    assert_eq!(value_of(&restored, 1), Some(&Value::Int(9)));
    assert_eq!(value_of(&restored, 2), Some(&Value::Int(2)));
}

#[test]
fn fields_no_operation_reads_together_are_searched_apart() {
    let (schema, source) = record(3);
    let lens = lens(
        schema,
        "rec",
        vec![doubled("k0"), doubled("k1"), doubled("k1"), doubled("k2")],
    );
    let plan = RestrictPlan::compile(&lens.src_schema, &lens.tgt_schema, &lens.compiled).unwrap();
    let (view, complement) = crate::get(&lens, &source).unwrap();
    let search = Search::new(&lens, &view, &complement, &plan).unwrap();
    assert_eq!(search.parts(), vec![vec![0], vec![1], vec![2]]);
}
