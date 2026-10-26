#![allow(clippy::unwrap_used, clippy::expect_used)]
//! A hyper-edge label rename or removal reaches the cell the label names, in
//! every shape: `panproto_mig::compile` lowers the label onto the column's
//! attribute arrow, the table and graph engines rename or drop the cell, and
//! the tree, whose column is a child node, relabels or drops the child.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use panproto_gat::Name;
use panproto_inst::value::FieldPresence;
use panproto_inst::{
    BoundaryProtocols, FInstance, GInstance, Node, RestrictPlan, RowId, Value, WInstance,
    collect_scalar_child_values, functor_restrict_with_plan, graph_restrict_with_plan,
    wtype_restrict_with_plan,
};
use panproto_mig::{Migration, compile};
use panproto_schema::{Edge, Protocol, ScalarKind, Schema, SchemaBuilder};

fn protocol() -> Protocol {
    Protocol {
        value_kinds: BTreeMap::from([(
            "int".to_owned(),
            ScalarKind::Int {
                bits: 64,
                signed: true,
            },
        )]),
        non_value_kinds: BTreeSet::from(["record".to_owned()]),
        ..Protocol::default()
    }
}

fn prop(tgt: &str, name: &str) -> Edge {
    Edge {
        src: "T".into(),
        tgt: tgt.into(),
        kind: "prop".into(),
        name: Some(name.into()),
    }
}

/// Table `T` with int columns `T.a` and `T.b`, joined by the `prop` edges
/// `columns` (name, vertex), and the hyper-edge `hyper_edge` over `T` with
/// the labels `labels`.
fn table(hyper_edge: &str, columns: &[(&str, &str)], labels: &[(&str, &str)]) -> Schema {
    let protocol = protocol();
    let mut builder = SchemaBuilder::new(&protocol)
        .vertex("T", "record", None)
        .and_then(|b| b.vertex("T.a", "int", None))
        .and_then(|b| b.vertex("T.b", "int", None))
        .unwrap();
    for (name, vertex) in columns {
        builder = builder.edge("T", vertex, "prop", Some(name)).unwrap();
    }
    let mut signature: HashMap<String, String> = labels
        .iter()
        .map(|(l, v)| ((*l).to_owned(), (*v).to_owned()))
        .collect();
    signature.insert("parent".to_owned(), "T".to_owned());
    builder
        .hyper_edge(hyper_edge, "row", signature, "parent")
        .unwrap()
        .build()
        .unwrap()
}

fn migration(labels: &[(&str, &str)]) -> Migration {
    let mut m = Migration::empty();
    for v in ["T", "T.a", "T.b"] {
        m.vertex_map.insert(v.into(), v.into());
    }
    m.hyper_edge_map.insert("H".into(), "H2".into());
    for (s, t) in labels {
        m.label_map
            .insert((Name::from("H"), Name::from(*s)), Name::from(*t));
    }
    m
}

/// The cells of the one row of `T` after a restrict, as each shape gives
/// them: the table's row, the graph node's fields, and the tree's scalar
/// children.
fn cells_in_every_shape(src: &Schema, tgt: &Schema, m: &Migration) -> [BTreeMap<String, Value>; 3] {
    let compiled = compile(src, tgt, m).unwrap();
    let p = protocol();
    let protocols = BoundaryProtocols { src: &p, tgt: &p };
    let row = HashMap::from([
        ("a".to_owned(), Value::Int(1)),
        ("b".to_owned(), Value::Int(2)),
    ]);
    let table = FInstance::new().with_rows("T", [(RowId(0), row)]).unwrap();
    let table_plan = RestrictPlan::compile_for_tables(src, tgt, &compiled, protocols).unwrap();
    let (table_view, _) = functor_restrict_with_plan(&table, &table_plan).unwrap();
    let graph_plan = RestrictPlan::compile_with_protocols(src, tgt, &compiled, protocols).unwrap();
    let graph = GInstance::from_finstance(&table).unwrap().0;
    let (graph_view, _) = graph_restrict_with_plan(&graph, &graph_plan).unwrap();
    let nodes: HashMap<u32, Node> = [
        (0, Node::new(0, "T")),
        (
            1,
            Node::new(1, "T.a").with_value(FieldPresence::Present(Value::Int(1))),
        ),
        (
            2,
            Node::new(2, "T.b").with_value(FieldPresence::Present(Value::Int(2))),
        ),
    ]
    .into();
    let tree = WInstance::new(
        nodes,
        vec![(0, 1, prop("T.a", "a")), (0, 2, prop("T.b", "b"))],
        vec![],
        0,
        "T".into(),
    );
    let (tree_view, _) = wtype_restrict_with_plan(&tree, &graph_plan).unwrap();
    [
        table_view
            .row(RowId(0))
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
        graph_view
            .node(0)
            .unwrap()
            .extra_fields
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
        collect_scalar_child_values(&tree_view, 0)
            .into_iter()
            .collect(),
    ]
}

#[test]
fn a_hyperedge_label_rename_renames_the_cell() {
    let src = table(
        "H",
        &[("a", "T.a"), ("b", "T.b")],
        &[("a", "T.a"), ("b", "T.b")],
    );
    let tgt = table(
        "H2",
        &[("a2", "T.a"), ("b", "T.b")],
        &[("a2", "T.a"), ("b", "T.b")],
    );
    let m = migration(&[("a", "a2"), ("b", "b")]);
    let expected = BTreeMap::from([
        ("a2".to_owned(), Value::Int(1)),
        ("b".to_owned(), Value::Int(2)),
    ]);
    for (shape, cells) in ["table", "graph", "tree"]
        .iter()
        .zip(cells_in_every_shape(&src, &tgt, &m))
    {
        assert_eq!(cells, expected, "{shape}");
    }
}

#[test]
fn a_hyperedge_label_drop_drops_the_cell() {
    let src = table(
        "H",
        &[("a", "T.a"), ("b", "T.b")],
        &[("a", "T.a"), ("b", "T.b")],
    );
    let tgt = table("H2", &[("a", "T.a")], &[("a", "T.a")]);
    let m = migration(&[("a", "a")]);
    let expected = BTreeMap::from([("a".to_owned(), Value::Int(1))]);
    for (shape, cells) in ["table", "graph", "tree"]
        .iter()
        .zip(cells_in_every_shape(&src, &tgt, &m))
    {
        assert_eq!(cells, expected, "{shape}");
    }
}
