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
    list_keyed(items, false)
}

/// [`list`], with `m` a unique key of the records where `keyed` says so.
fn list_keyed(items: u32, keyed: bool) -> (Schema, WInstance) {
    let protocol = Protocol::default();
    let key = panproto_schema::Key {
        id: "m".into(),
        parts: vec![panproto_schema::KeyPart::Path(vec![edge(
            "r",
            "r.m",
            "prop",
            Some("m"),
        )])],
        role: panproto_schema::KeyRole::Unique,
        scope: panproto_schema::KeyScope::Document,
        equality: Vec::new(),
    };
    let schema = SchemaBuilder::new(&protocol)
        .vertex("s", "object", None)
        .and_then(|b| b.vertex("r", "object", None))
        .and_then(|b| b.vertex("r.n", "integer", None))
        .and_then(|b| b.vertex("r.m", "integer", None))
        .and_then(|b| b.edge("s", "r", "item", None))
        .and_then(|b| b.edge("r", "r.n", "prop", Some("n")))
        .and_then(|b| b.edge("r", "r.m", "prop", Some("m")))
        .map(|b| if keyed { b.key("r", key) } else { b })
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
    let runs = search.runs.get();
    println!("{runs} node runs");
    (restored, runs)
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
    assert!(runs <= 100 * items as usize, "{runs} node runs");
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
    assert!(runs <= 400 * fields as usize, "{runs} node runs");
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

/// A guard that reads only the structure of the instance, through a graph
/// builtin, sees no field's value: the items are still searched apart, and
/// an unreachable edit is refused in runs linear in the list.
#[test]
fn a_guard_on_the_structure_alone_leaves_the_items_apart() {
    let items = 1000;
    let (schema, source) = list(items);
    let structural = FieldTransform::Case {
        branches: vec![panproto_inst::CaseBranch {
            predicate: op(
                BuiltinOp::Eq,
                vec![
                    op(
                        BuiltinOp::Anchor,
                        vec![Expr::Lit(Literal::Str("self".into()))],
                    ),
                    Expr::Lit(Literal::Str("nothing".into())),
                ],
            ),
            transforms: Vec::new(),
        }],
    };
    let lens = lens(schema, "r", vec![doubled("n"), doubled("n"), structural]);
    let (restored, runs) = put_with(&lens, &source, 2, 7);
    assert!(restored.is_none(), "no source shows 7 at the first item");
    assert!(runs <= 100 * items as usize, "{runs} node runs");
}

/// A unique key compares the records, so every coordinate is in one part
/// and an unreachable edit is refused by the search of every coordinate
/// together, which ends at the first source it would try again: the runs
/// grow with the list as a single search of it does.
#[test]
fn an_unreachable_edit_under_a_unique_key_is_refused_by_one_search() {
    for items in [32, 64, 128] {
        let (schema, source) = list_keyed(items, true);
        let lens = lens(schema, "r", vec![doubled("n"), doubled("n")]);
        let (restored, runs) = put_with(&lens, &source, 2, 7);
        assert!(restored.is_none(), "no source shows 7 at the first item");
        assert!(
            runs <= 100 * items as usize,
            "{items} items: {runs} node runs"
        );
    }
}

/// Put of `source`'s view with each child of `edits` set to its value: the
/// restored source, or none for a refusal, and the node runs it took.
fn put_edits(lens: &Lens, source: &WInstance, edits: &[(u32, i64)]) -> (Option<WInstance>, usize) {
    let (mut view, complement) = crate::get(lens, source).unwrap();
    for &(child, value) in edits {
        view.nodes.get_mut(&child).unwrap().value = Some(FieldPresence::Present(Value::Int(value)));
    }
    let plan = RestrictPlan::compile(&lens.src_schema, &lens.tgt_schema, &lens.compiled).unwrap();
    let search = Search::new(lens, &view, &complement, &plan).unwrap();
    let restored = search.source().ok();
    let runs = search.runs.get();
    println!("{runs} node runs");
    (restored, runs)
}

/// The invertible rewrites `key + 1`, `key * 2`, `key + 3`, `key * 3` and
/// `key + 5`, the first `count` of them.
fn invertible(key: &str, count: usize) -> Vec<FieldTransform> {
    let step = |forward: BuiltinOp, backward: BuiltinOp, by: i64| {
        rewrite(
            key,
            op(forward, vec![var(key), lit(by)]),
            op(backward, vec![var(key), lit(by)]),
        )
    };
    [
        step(BuiltinOp::Add, BuiltinOp::Sub, 1),
        step(BuiltinOp::Mul, BuiltinOp::Div, 2),
        step(BuiltinOp::Add, BuiltinOp::Sub, 3),
        step(BuiltinOp::Mul, BuiltinOp::Div, 3),
        step(BuiltinOp::Add, BuiltinOp::Sub, 5),
    ]
    .into_iter()
    .take(count)
    .collect()
}

/// `key % 1000`, with no inverse.
fn thousandths(key: &str) -> FieldTransform {
    FieldTransform::ApplyExpr {
        key: key.to_owned(),
        expr: op(BuiltinOp::Mod, vec![var(key), lit(1000)]),
        inverse: None,
        coercion_class: CoercionClass::Opaque,
    }
}

/// A record of 128 fields, field `i` rewritten by `(i % 5) + 1` invertible
/// rewrites and then reduced modulo 1000, with every field edited to a value
/// no source gives: each field is a part whose candidates cycle with a
/// period of its own, and the first part that accepts nothing refuses.
#[test]
fn every_field_edited_out_of_reach_is_refused_in_runs_linear_in_the_fields() {
    let fields = 128;
    let (schema, source) = record(fields);
    let transforms = (0..fields)
        .flat_map(|field| {
            let key = format!("k{field}");
            let mut program = invertible(&key, (field % 5) as usize + 1);
            program.push(thousandths(&key));
            program
        })
        .collect();
    let lens = lens(schema, "rec", transforms);
    let edits: Vec<(u32, i64)> = (0..fields)
        .map(|field| (field + 1, 5000 + i64::from(field)))
        .collect();
    let (restored, runs) = put_edits(&lens, &source, &edits);
    assert!(restored.is_none());
    assert!(runs <= RECORD_RUNS_AT_010D408D * 4, "{runs} node runs");
}

/// A list of 1600 records, each rewriting `n` by two, three or four
/// invertible rewrites as `m % 3` says and then reducing it modulo 1000,
/// with every `n` edited to a value no source gives.
#[test]
fn every_item_edited_out_of_reach_is_refused_in_runs_linear_in_the_list() {
    let items = 1600;
    let (schema, source) = list(items);
    let branch = |test: Expr, count: usize| panproto_inst::CaseBranch {
        predicate: test,
        transforms: invertible("n", count),
    };
    let modulo = |residue: i64| {
        op(
            BuiltinOp::Eq,
            vec![op(BuiltinOp::Mod, vec![var("m"), lit(3)]), lit(residue)],
        )
    };
    let guarded = FieldTransform::Case {
        branches: vec![
            branch(modulo(0), 2),
            branch(modulo(1), 3),
            branch(Expr::Lit(Literal::Bool(true)), 4),
        ],
    };
    let lens = lens(schema, "r", vec![guarded, thousandths("n")]);
    let edits: Vec<(u32, i64)> = (0..items)
        .map(|item| (3 * item + 2, 5000 + i64::from(item)))
        .collect();
    let (restored, runs) = put_edits(&lens, &source, &edits);
    assert!(restored.is_none());
    assert!(runs <= LIST_RUNS_AT_010D408D * 4, "{runs} node runs");
}

/// The node runs of the two refusals above at 010d408d.
const RECORD_RUNS_AT_010D408D: usize = 780;
const LIST_RUNS_AT_010D408D: usize = 12_958;

/// Two contents equal but for the order their nested records were built in
/// digest alike, so a search finds a source it has tried again.
#[test]
fn equal_contents_digest_alike_whatever_order_their_records_were_built_in() {
    let nested = |keys: Vec<u32>| {
        let mut inner = HashMap::new();
        for key in keys {
            inner.insert(format!("f{key}"), Value::Int(i64::from(key)));
        }
        let mut fields = HashMap::new();
        fields.insert("r".to_owned(), Value::Unknown(inner));
        super::Content {
            value: None,
            fields,
        }
    };
    let forward = nested((0..64).collect());
    let backward = nested((0..64).rev().collect());
    assert!(forward.same(&backward));
    assert_eq!(super::digest(&[forward]), super::digest(&[backward]));
}
