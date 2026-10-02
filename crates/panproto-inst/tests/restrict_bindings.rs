#![allow(clippy::unwrap_used, clippy::expect_used)]
//! What an expression at an element reads, in every shape: children bound
//! by column name, a repeated name bound to a list and refused when an
//! expression reads it, structural children materialized only over
//! out-trees, values converted only when read, and the outputs a transform
//! writes to a column coerced to the column's kind.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use panproto_expr::{BuiltinOp, Expr, Literal};
use panproto_gat::Name;
use panproto_inst::{
    BoundaryProtocols, CompiledMigration, ElementOps, FInstance, FieldPresence, GInstance, Node,
    RestrictError, RestrictPlan, RowId, Value, WInstance, functor_restrict_with_plan,
    graph_restrict_with_plan, wtype_restrict_with_plan,
};
use panproto_schema::{Edge, Protocol, ScalarKind, Schema, SchemaBuilder};

fn schema_with(protocol: &Protocol, vertices: &[(&str, &str)], edges: &[&Edge]) -> Schema {
    let mut builder = SchemaBuilder::new(protocol);
    for (v, kind) in vertices {
        builder = builder.vertex(v, kind, None).unwrap();
    }
    for edge in edges {
        builder = builder
            .edge(&edge.src, &edge.tgt, &edge.kind, edge.name.as_deref())
            .unwrap();
    }
    builder.build().unwrap()
}

fn edge(src: &str, tgt: &str, kind: &str, name: Option<&str>) -> Edge {
    Edge {
        src: src.into(),
        tgt: tgt.into(),
        kind: kind.into(),
        name: name.map(Name::from),
    }
}

fn protocol() -> Protocol {
    Protocol {
        value_kinds: BTreeMap::from([
            ("string".to_owned(), ScalarKind::Str),
            (
                "int".to_owned(),
                ScalarKind::Int {
                    bits: 64,
                    signed: true,
                },
            ),
        ]),
        non_value_kinds: BTreeSet::from(["record".to_owned()]),
        ..Protocol::default()
    }
}

fn identity(schema: &Schema) -> CompiledMigration {
    CompiledMigration {
        surviving_verts: schema.vertices.keys().cloned().collect(),
        surviving_edges: schema.edges.keys().cloned().collect(),
        ..CompiledMigration::default()
    }
}

const fn present(value: Value) -> FieldPresence {
    FieldPresence::Present(value)
}

/// `r` with two children named `x` holding 1 and 2.
fn repeated_tree() -> (WInstance, Schema) {
    let x = edge("r", "v", "prop", Some("x"));
    let schema = schema_with(
        &Protocol::default(),
        &[("r", "record"), ("v", "record")],
        &[&x],
    );
    let nodes: HashMap<u32, Node> = [
        (0, Node::new(0, "r")),
        (1, Node::new(1, "v").with_value(present(Value::Int(1)))),
        (2, Node::new(2, "v").with_value(present(Value::Int(2)))),
    ]
    .into();
    let tree = WInstance::new(
        nodes,
        vec![(0, 1, x.clone()), (0, 2, x)],
        vec![],
        0,
        "r".into(),
    );
    (tree, schema)
}

/// Tables `r` and `v` with a relation `r -x-> v`; row 0 of `r` reaches rows
/// 1 and 2 of `v`.
fn repeated_table() -> (FInstance, Schema) {
    let x = edge("r", "v", "fk", Some("x"));
    let schema = schema_with(&protocol(), &[("r", "record"), ("v", "record")], &[&x]);
    let f = FInstance::new()
        .with_rows("r", [(RowId(0), HashMap::new())])
        .and_then(|f| {
            f.with_rows(
                "v",
                [(RowId(1), HashMap::new()), (RowId(2), HashMap::new())],
            )
        })
        .and_then(|f| f.with_foreign_key(x, [(RowId(0), RowId(1)), (RowId(0), RowId(2))]))
        .unwrap();
    (f, schema)
}

fn table_plan(schema: &Schema, migration: &CompiledMigration) -> RestrictPlan {
    let p = protocol();
    RestrictPlan::compile_for_tables(
        schema,
        schema,
        migration,
        BoundaryProtocols { src: &p, tgt: &p },
    )
    .unwrap()
}

#[test]
fn repeated_child_names_bind_a_list_in_every_shape() {
    let (tree, _) = repeated_tree();
    let list = Value::List(vec![Value::Int(1), Value::Int(2)]);
    assert_eq!(
        panproto_inst::collect_scalar_child_values(&tree, 0).get("x"),
        Some(&list)
    );
    assert_eq!(
        panproto_inst::collect_child_values(None, &tree, 0, None).get("x"),
        Some(&list)
    );
    let graph = GInstance::from_wtype(&tree).unwrap();
    assert_eq!(graph.attributes(0).get("x"), Some(&list));
    // A table row's children are its pairs; their rows hold no value, so the
    // repeated name binds the empty list, as in the row's graph form.
    let (table, _) = repeated_table();
    assert_eq!(table.attributes(0).get("x"), Some(&Value::List(vec![])));
    let (table_graph, _) = GInstance::from_finstance(&table).unwrap();
    assert_eq!(
        table_graph.attributes(0).get("x"),
        Some(&Value::List(vec![]))
    );
}

#[test]
fn child_collectors_skip_self_loops() {
    let me = edge("r", "r", "prop", Some("me"));
    let graph = GInstance::new()
        .with_node(Node::new(0, "r").with_value(present(Value::Int(0))))
        .with_edge(0, 0, me.clone());
    assert_eq!(graph.attributes(0).get("me"), None);
    let table = FInstance::new()
        .with_rows("r", [(RowId(0), HashMap::new())])
        .and_then(|f| f.with_foreign_key(me, [(RowId(0), RowId(0))]))
        .unwrap();
    assert_eq!(table.attributes(0).get("me"), None);
}

fn reads(name: &str) -> Expr {
    Expr::Builtin(BuiltinOp::Length, vec![Expr::Var(name.into())])
}

#[test]
fn a_transform_naming_a_repeated_child_is_ambiguous() {
    let ambiguous = |error: &RestrictError, node: u32| {
        matches!(
            error.unlocated(),
            RestrictError::AmbiguousChildField { node: n, field } if *n == node && field == "x"
        )
    };
    // Tree and its graph form.
    let (tree, schema) = repeated_tree();
    let mut m = identity(&schema);
    m.add_computed_field("r", "n", reads("x"));
    let plan = RestrictPlan::compile(&schema, &schema, &m).unwrap();
    let error = wtype_restrict_with_plan(&tree, &plan).unwrap_err();
    assert!(ambiguous(&error, 0), "{error:?}");
    let error =
        graph_restrict_with_plan(&GInstance::from_wtype(&tree).unwrap(), &plan).unwrap_err();
    assert!(ambiguous(&error, 0), "{error:?}");
    // Table and its graph form.
    let (table, schema) = repeated_table();
    let mut m = identity(&schema);
    m.add_computed_field("r", "n", reads("x"));
    let error = functor_restrict_with_plan(&table, &table_plan(&schema, &m)).unwrap_err();
    assert!(ambiguous(&error, 0), "{error:?}");
    let plan = RestrictPlan::compile(&schema, &schema, &m).unwrap();
    let error =
        graph_restrict_with_plan(&GInstance::from_finstance(&table).unwrap().0, &plan).unwrap_err();
    assert!(ambiguous(&error, 0), "{error:?}");
}

#[test]
fn a_repeated_name_the_fields_shadow_is_read_from_the_fields() {
    let (mut tree, schema) = repeated_tree();
    tree.nodes
        .get_mut(&0)
        .unwrap()
        .extra_fields
        .insert("x".into(), Value::List(vec![Value::Int(9)]));
    let mut m = identity(&schema);
    m.add_computed_field("r", "n", reads("x"));
    let plan = RestrictPlan::compile(&schema, &schema, &m).unwrap();
    let (view, _) = wtype_restrict_with_plan(&tree, &plan).unwrap();
    assert_eq!(view.nodes[&0].extra_fields.get("n"), Some(&Value::Int(1)));
    let (graph_view, _) =
        graph_restrict_with_plan(&GInstance::from_wtype(&tree).unwrap(), &plan).unwrap();
    assert_eq!(
        graph_view.node(0).unwrap().extra_fields.get("n"),
        Some(&Value::Int(1))
    );
}

#[test]
fn a_repeated_name_with_an_absent_child_and_an_unnamed_repeated_edge_are_repeated() {
    // One present and one absent child under x: still two children, still
    // repeated, binding the list of the one present value.
    let x = edge("r", "v", "prop", Some("x"));
    let unnamed = edge("r", "w", "prop", None);
    let schema = schema_with(
        &Protocol::default(),
        &[("r", "record"), ("v", "record"), ("w", "record")],
        &[&x, &unnamed],
    );
    let nodes: HashMap<u32, Node> = [
        (0, Node::new(0, "r")),
        (1, Node::new(1, "v").with_value(present(Value::Int(1)))),
        (2, Node::new(2, "v").with_value(FieldPresence::Absent)),
        (3, Node::new(3, "w").with_value(present(Value::Int(3)))),
        (4, Node::new(4, "w").with_value(present(Value::Int(4)))),
    ]
    .into();
    let tree = WInstance::new(
        nodes,
        vec![
            (0, 1, x.clone()),
            (0, 2, x),
            (0, 3, unnamed.clone()),
            (0, 4, unnamed),
        ],
        vec![],
        0,
        "r".into(),
    );
    let scalars = panproto_inst::collect_scalar_child_values(&tree, 0);
    assert_eq!(scalars.get("x"), Some(&Value::List(vec![Value::Int(1)])));
    assert_eq!(
        scalars.get("w"),
        Some(&Value::List(vec![Value::Int(3), Value::Int(4)]))
    );
    for name in ["x", "w"] {
        let mut m = identity(&schema);
        m.add_computed_field("r", "n", reads(name));
        let plan = RestrictPlan::compile(&schema, &schema, &m).unwrap();
        let error = wtype_restrict_with_plan(&tree, &plan).unwrap_err();
        assert!(
            matches!(&error, RestrictError::AmbiguousChildField { field, .. } if field == name),
            "{error:?}"
        );
    }
}

/// `r -items-> arr`, `arr -item-> rec` twice, records holding `x`.
fn structural_tree() -> (WInstance, Schema) {
    let items = edge("r", "arr", "prop", Some("items"));
    let item = edge("arr", "rec", "item", None);
    let schema = schema_with(
        &Protocol::default(),
        &[("r", "record"), ("arr", "record"), ("rec", "record")],
        &[&items, &item],
    );
    let nodes: HashMap<u32, Node> = [
        (0, Node::new(0, "r")),
        (1, Node::new(1, "arr")),
        (2, Node::new(2, "rec").with_extra_field("x", Value::Int(1))),
        (3, Node::new(3, "rec").with_extra_field("x", Value::Int(2))),
    ]
    .into();
    let tree = WInstance::new(
        nodes,
        vec![(0, 1, items), (1, 2, item.clone()), (1, 3, item)],
        vec![],
        0,
        "r".into(),
    );
    (tree, schema)
}

#[test]
fn a_structural_child_aggregate_agrees_in_every_shape() {
    let (tree, schema) = structural_tree();
    assert_eq!(
        panproto_inst::collect_child_values(None, &tree, 0, None).get("items"),
        Some(&Value::List(vec![
            Value::Unknown(HashMap::from([("x".to_owned(), Value::Int(1))])),
            Value::Unknown(HashMap::from([("x".to_owned(), Value::Int(2))])),
        ]))
    );
    let mut m = identity(&schema);
    m.add_computed_field("r", "count", reads("items"));
    let plan = RestrictPlan::compile(&schema, &schema, &m).unwrap();
    let (view, _) = wtype_restrict_with_plan(&tree, &plan).unwrap();
    assert_eq!(
        view.nodes[&0].extra_fields.get("count"),
        Some(&Value::Int(2))
    );
    let (graph_view, _) =
        graph_restrict_with_plan(&GInstance::from_wtype(&tree).unwrap(), &plan).unwrap();
    assert_eq!(
        graph_view.node(0).unwrap().extra_fields.get("count"),
        Some(&Value::Int(2))
    );
    // Tables: the same structure over rows and pairs.
    let items = edge("r", "arr", "fk", Some("items"));
    let item = edge("arr", "rec", "fk", Some("item"));
    let x = edge("rec", "rec.x", "prop", Some("x"));
    let table_schema = schema_with(
        &protocol(),
        &[
            ("r", "record"),
            ("arr", "record"),
            ("rec", "record"),
            ("rec.x", "int"),
        ],
        &[&items, &item, &x],
    );
    let table = FInstance::new()
        .with_rows("r", [(RowId(0), HashMap::new())])
        .and_then(|f| f.with_rows("arr", [(RowId(1), HashMap::new())]))
        .and_then(|f| {
            f.with_rows(
                "rec",
                [
                    (RowId(2), HashMap::from([("x".to_owned(), Value::Int(1))])),
                    (RowId(3), HashMap::from([("x".to_owned(), Value::Int(2))])),
                ],
            )
        })
        .and_then(|f| f.with_foreign_key(items, [(RowId(0), RowId(1))]))
        .and_then(|f| f.with_foreign_key(item, [(RowId(1), RowId(2)), (RowId(1), RowId(3))]))
        .unwrap();
    let mut tm = identity(&table_schema);
    tm.add_computed_field("r", "count", reads("items"));
    let (table_view, _) =
        functor_restrict_with_plan(&table, &table_plan(&table_schema, &tm)).unwrap();
    assert_eq!(
        table_view.row(RowId(0)).unwrap().get("count"),
        Some(&Value::Int(2))
    );
}

#[test]
fn a_shared_structural_child_is_refused() {
    // r -items-> arr, and arr's two children are one node reached twice:
    // the subgraph under arr is no tree.
    let (tree, schema) = structural_tree();
    let mut graph = GInstance::from_wtype(&tree).unwrap();
    graph
        .add_edge(1, 2, edge("arr", "rec", "item", None))
        .unwrap();
    let mut m = identity(&schema);
    m.add_computed_field("r", "count", reads("items"));
    let plan = RestrictPlan::compile(&schema, &schema, &m).unwrap();
    let error = graph_restrict_with_plan(&graph, &plan).unwrap_err();
    assert!(
        matches!(
            error.unlocated(),
            RestrictError::StructuralChildNotATree { node: 0, field } if field == "items"
        ),
        "{error:?}"
    );
}

#[test]
fn a_transform_evaluates_alike_in_every_shape() {
    // A transform reading a field and a scalar child, at a tree node, the
    // node in the tree's graph form, and a table row and its graph form.
    let v = edge("r", "v", "prop", Some("v"));
    let schema = schema_with(
        &Protocol::default(),
        &[("r", "record"), ("v", "record")],
        &[&v],
    );
    let nodes: HashMap<u32, Node> = [
        (0, Node::new(0, "r").with_extra_field("a", Value::Int(5))),
        (1, Node::new(1, "v").with_value(present(Value::Int(7)))),
    ]
    .into();
    let tree = WInstance::new(nodes, vec![(0, 1, v)], vec![], 0, "r".into());
    let sum = Expr::Builtin(
        BuiltinOp::Add,
        vec![Expr::Var("a".into()), Expr::Var("v".into())],
    );
    let mut m = identity(&schema);
    m.add_computed_field("r", "s", sum);
    let plan = RestrictPlan::compile(&schema, &schema, &m).unwrap();
    let (view, _) = wtype_restrict_with_plan(&tree, &plan).unwrap();
    let (graph_view, _) =
        graph_restrict_with_plan(&GInstance::from_wtype(&tree).unwrap(), &plan).unwrap();
    assert_eq!(view.nodes[&0].extra_fields.get("s"), Some(&Value::Int(12)));
    assert_eq!(
        graph_view.node(0).unwrap().extra_fields.get("s"),
        Some(&Value::Int(12))
    );
    // A table row reads its cells.
    let a = edge("t", "t.a", "prop", Some("a"));
    let table_schema = schema_with(&protocol(), &[("t", "record"), ("t.a", "int")], &[&a]);
    let table = FInstance::new()
        .with_rows(
            "t",
            [(RowId(4), HashMap::from([("a".to_owned(), Value::Int(5))]))],
        )
        .unwrap();
    let mut tm = identity(&table_schema);
    tm.add_computed_field(
        "t",
        "s",
        Expr::Builtin(
            BuiltinOp::Add,
            vec![Expr::Var("a".into()), Expr::Lit(Literal::Int(7))],
        ),
    );
    let (table_view, _) =
        functor_restrict_with_plan(&table, &table_plan(&table_schema, &tm)).unwrap();
    let plan = RestrictPlan::compile(&table_schema, &table_schema, &tm).unwrap();
    let (graph_view, _) =
        graph_restrict_with_plan(&GInstance::from_finstance(&table).unwrap().0, &plan).unwrap();
    assert_eq!(
        table_view.row(RowId(4)).unwrap().get("s"),
        Some(&Value::Int(12))
    );
    assert_eq!(
        graph_view.node(4).unwrap().extra_fields.get("s"),
        Some(&Value::Int(12))
    );
}

#[test]
fn an_unexpressible_referenced_value_is_a_typed_error() {
    let schema = schema_with(&Protocol::default(), &[("r", "record")], &[]);
    let node = Node::new(0, "r")
        .with_extra_field("t", Value::Token("red".into()))
        .with_extra_field("n", Value::Int(1));
    let graph = GInstance::new().with_node(node);
    // Reading the token is refused...
    let mut m = identity(&schema);
    m.add_computed_field("r", "copy", Expr::Var("t".into()));
    let plan = RestrictPlan::compile(&schema, &schema, &m).unwrap();
    let error = graph_restrict_with_plan(&graph, &plan).unwrap_err();
    assert!(
        matches!(
            error.unlocated(),
            RestrictError::ValueNotExpressible { node: 0, field, value_type }
                if field == "t" && value_type == "token"
        ),
        "{error:?}"
    );
    // ...and an expression that does not read it runs.
    let mut m = identity(&schema);
    m.add_computed_field("r", "copy", Expr::Var("n".into()));
    let plan = RestrictPlan::compile(&schema, &schema, &m).unwrap();
    let (view, _) = graph_restrict_with_plan(&graph, &plan).unwrap();
    assert_eq!(
        view.node(0).unwrap().extra_fields.get("copy"),
        Some(&Value::Int(1))
    );
}

#[test]
fn integral_floats_stay_floats() {
    let schema = schema_with(&Protocol::default(), &[("r", "record")], &[]);
    let graph = GInstance::new().with_node(Node::new(0, "r"));
    let mut m = identity(&schema);
    m.add_computed_field("r", "y", Expr::Lit(Literal::Float(2.0)));
    let plan = RestrictPlan::compile(&schema, &schema, &m).unwrap();
    let (view, _) = graph_restrict_with_plan(&graph, &plan).unwrap();
    assert_eq!(
        view.node(0).unwrap().extra_fields.get("y"),
        Some(&Value::Float(2.0))
    );
    assert_eq!(
        panproto_inst::expr_literal_to_value(&Literal::Float(2.0)),
        Value::Float(2.0)
    );
}

/// A table `person` with an `age` column of kind `int`.
fn people() -> (Schema, FInstance) {
    let age = edge("person", "person.age", "prop", Some("age"));
    let schema = schema_with(
        &protocol(),
        &[("person", "record"), ("person.age", "int")],
        &[&age],
    );
    let table = FInstance::new()
        .with_rows(
            "person",
            [(RowId(0), HashMap::from([("age".to_owned(), Value::Int(1))]))],
        )
        .unwrap();
    (schema, table)
}

#[test]
fn an_integral_float_output_becomes_an_int_cell() {
    let (schema, table) = people();
    let mut m = identity(&schema);
    m.add_computed_field("person", "age", Expr::Lit(Literal::Float(2.0)));
    let (view, _) = functor_restrict_with_plan(&table, &table_plan(&schema, &m)).unwrap();
    assert_eq!(view.row(RowId(0)).unwrap().get("age"), Some(&Value::Int(2)));
    let p = protocol();
    let plan = RestrictPlan::compile_with_protocols(
        &schema,
        &schema,
        &m,
        BoundaryProtocols { src: &p, tgt: &p },
    )
    .unwrap();
    let (graph_view, _) =
        graph_restrict_with_plan(&GInstance::from_finstance(&table).unwrap().0, &plan).unwrap();
    assert_eq!(
        graph_view.node(0).unwrap().extra_fields.get("age"),
        Some(&Value::Int(2))
    );
    // Without a cell layer nothing is coerced.
    let plan = RestrictPlan::compile(&schema, &schema, &m).unwrap();
    let (graph_view, _) =
        graph_restrict_with_plan(&GInstance::from_finstance(&table).unwrap().0, &plan).unwrap();
    assert_eq!(
        graph_view.node(0).unwrap().extra_fields.get("age"),
        Some(&Value::Float(2.0))
    );
}

#[test]
fn a_lossy_transform_output_is_refused() {
    let (schema, table) = people();
    let mut m = identity(&schema);
    m.add_computed_field("person", "age", Expr::Lit(Literal::Float(2.5)));
    let error = functor_restrict_with_plan(&table, &table_plan(&schema, &m)).unwrap_err();
    match &error {
        RestrictError::ValueFidelity { node: 0, error } => {
            assert_eq!(error.site.vertex, "person.age");
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn a_dropped_column_vertex_drops_the_cell() {
    // Columns a and b; the migration drops b's vertex. Tables and graphs
    // drop the cell; the tree, whose column is a child node, drops the
    // child.
    let a = edge("t", "t.a", "prop", Some("a"));
    let b = edge("t", "t.b", "field-of", Some("b"));
    let src = schema_with(
        &protocol(),
        &[("t", "record"), ("t.a", "int"), ("t.b", "int")],
        &[&a, &b],
    );
    let tgt = schema_with(&protocol(), &[("t", "record"), ("t.a", "int")], &[&a]);
    let m = CompiledMigration {
        surviving_verts: ["t", "t.a"].iter().map(|v| Name::from(*v)).collect(),
        surviving_edges: [a.clone()].into(),
        ..CompiledMigration::default()
    };
    let p = protocol();
    let protocols = BoundaryProtocols { src: &p, tgt: &p };
    let cells = HashMap::from([
        ("a".to_owned(), Value::Int(1)),
        ("b".to_owned(), Value::Int(2)),
    ]);
    let table = FInstance::new()
        .with_rows("t", [(RowId(0), cells)])
        .unwrap();
    let table_plan = RestrictPlan::compile_for_tables(&src, &tgt, &m, protocols).unwrap();
    let (table_view, complement) = functor_restrict_with_plan(&table, &table_plan).unwrap();
    assert_eq!(table_view.row(RowId(0)).unwrap().get("b"), None);
    assert_eq!(
        table_view.row(RowId(0)).unwrap().get("a"),
        Some(&Value::Int(1))
    );
    assert_eq!(
        complement.dropped_cells.get(&(RowId(0), "b".to_owned())),
        Some(&Value::Int(2))
    );
    let graph_plan = RestrictPlan::compile_with_protocols(&src, &tgt, &m, protocols).unwrap();
    let (graph_view, _) =
        graph_restrict_with_plan(&GInstance::from_finstance(&table).unwrap().0, &graph_plan)
            .unwrap();
    assert_eq!(graph_view.node(0).unwrap().extra_fields.get("b"), None);
    // The tree oracle: the column is a child node, which the restrict drops.
    let nodes: HashMap<u32, Node> = [
        (0, Node::new(0, "t")),
        (1, Node::new(1, "t.a").with_value(present(Value::Int(1)))),
        (2, Node::new(2, "t.b").with_value(present(Value::Int(2)))),
    ]
    .into();
    let tree = WInstance::new(nodes, vec![(0, 1, a), (0, 2, b)], vec![], 0, "t".into());
    let (tree_view, _) = wtype_restrict_with_plan(&tree, &graph_plan).unwrap();
    assert!(!tree_view.nodes.contains_key(&2));
    assert_eq!(
        panproto_inst::collect_scalar_child_values(&tree_view, 0).get("b"),
        None
    );
}
