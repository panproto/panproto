#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Extend applies a migration's value transforms in every shape, as the
//! tree extend does: a row, a graph node and a tree node of a surviving
//! vertex leave with their fields transformed.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use panproto_gat::Name;
use panproto_inst::{
    CompiledMigration, FInstance, GInstance, Node, RowId, Value, WInstance, functor_extend,
    graph_extend, wtype_extend_engine,
};
use panproto_schema::{Protocol, ScalarKind, Schema, SchemaBuilder};

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

/// Table `t` with the int column `name`.
fn schema(column: &str) -> Schema {
    let p = protocol();
    SchemaBuilder::new(&p)
        .vertex("t", "record", None)
        .and_then(|b| b.vertex(&format!("t.{column}"), "int", None))
        .and_then(|b| b.edge("t", &format!("t.{column}"), "prop", Some(column)))
        .and_then(SchemaBuilder::build)
        .unwrap()
}

/// The fields of the one `t` element after extend, in every shape.
fn extended_fields(m: &CompiledMigration, tgt: &Schema) -> [BTreeMap<String, Value>; 3] {
    let p = protocol();
    let fields = HashMap::from([("x".to_owned(), Value::Int(7))]);
    let table = FInstance::new()
        .with_rows("t", [(RowId(0), fields.clone())])
        .unwrap();
    let table_out = functor_extend(&table, tgt, m, &p).unwrap();
    let mut node = Node::new(0, "t");
    for (k, v) in &fields {
        node = node.with_extra_field(k, v.clone());
    }
    let graph = GInstance::new().with_node(node.clone());
    let graph_out = graph_extend(&graph, tgt, m, &p).unwrap();
    let tree = WInstance::new(HashMap::from([(0, node)]), vec![], vec![], 0, "t".into());
    let tree_out = wtype_extend_engine(&tree, tgt, m).unwrap();
    let sorted = |map: &HashMap<String, Value>| -> BTreeMap<String, Value> {
        map.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
    };
    [
        sorted(table_out.row(RowId(0)).unwrap()),
        sorted(&graph_out.node(0).unwrap().extra_fields),
        sorted(&tree_out.nodes[&0].extra_fields),
    ]
}

#[test]
fn extend_renames_a_field_in_every_shape() {
    let tgt = schema("y");
    let mut m = CompiledMigration {
        surviving_verts: [Name::from("t"), Name::from("t.y")].into(),
        ..CompiledMigration::default()
    };
    m.add_field_rename("t", "x", "y");
    let expected = BTreeMap::from([("y".to_owned(), Value::Int(7))]);
    for (shape, fields) in ["table", "graph", "tree"]
        .iter()
        .zip(extended_fields(&m, &tgt))
    {
        assert_eq!(fields, expected, "{shape}");
    }
}

#[test]
fn extend_computes_a_field_once_in_every_shape() {
    // A computed field reads `x` and writes `x`: applied twice it would
    // give 9.
    let tgt = schema("x");
    let mut m = CompiledMigration {
        surviving_verts: [Name::from("t"), Name::from("t.x")].into(),
        ..CompiledMigration::default()
    };
    m.add_computed_field(
        "t",
        "x",
        panproto_expr::Expr::Builtin(
            panproto_expr::BuiltinOp::Add,
            vec![
                panproto_expr::Expr::Var("x".into()),
                panproto_expr::Expr::Lit(panproto_expr::Literal::Int(1)),
            ],
        ),
    );
    let expected = BTreeMap::from([("x".to_owned(), Value::Int(8))]);
    for (shape, fields) in ["table", "graph", "tree"]
        .iter()
        .zip(extended_fields(&m, &tgt))
    {
        assert_eq!(fields, expected, "{shape}");
    }
}
