//! Tree put of the fields that children of a record hold, where they move,
//! where the view holds them, and what the complement records of them.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::{HashMap, HashSet};

use panproto_expr::{BuiltinOp, Expr, Literal};
use panproto_gat::CoercionClass;
use panproto_inst::value::FieldPresence;
use panproto_inst::{CompiledMigration, FieldTransform, GInstance, Node, Value, WInstance};
use panproto_lens::optic::{OpticKind, check_optic_laws};
use panproto_lens::{
    ComplementCompose, Lens, LensError, check_put_get, get, graph_lens, put,
    tree_complement_to_graph,
};
use panproto_schema::{Edge, Protocol, Schema, SchemaBuilder};

/// `s` holds records `r` on an item edge; a record holds `n` and `m`.
fn schema() -> Schema {
    let protocol = Protocol::default();
    SchemaBuilder::new(&protocol)
        .vertex("s", "object", None)
        .and_then(|b| b.vertex("r", "object", None))
        .and_then(|b| b.vertex("r.n", "integer", None))
        .and_then(|b| b.vertex("r.m", "integer", None))
        .and_then(|b| b.edge("s", "r", "item", None))
        .and_then(|b| b.edge("r", "r.n", "prop", Some("n")))
        .and_then(|b| b.edge("r", "r.m", "prop", Some("m")))
        .and_then(SchemaBuilder::build)
        .unwrap()
}

fn lens(transforms: Vec<FieldTransform>) -> Lens {
    let schema = schema();
    Lens {
        compiled: CompiledMigration {
            surviving_verts: schema.vertices.keys().cloned().collect::<HashSet<_>>(),
            surviving_edges: schema.edges.keys().cloned().collect(),
            field_transforms: [("r".into(), transforms)].into(),
            ..CompiledMigration::default()
        },
        src_schema: schema.clone(),
        tgt_schema: schema,
    }
}

fn var(name: &str) -> Expr {
    Expr::Var(name.into())
}

fn op(op: BuiltinOp, arguments: Vec<Expr>) -> Expr {
    Expr::Builtin(op, arguments)
}

fn rewrite(key: &str, expr: Expr, inverse: Option<Expr>) -> FieldTransform {
    FieldTransform::ApplyExpr {
        key: key.to_owned(),
        coercion_class: if inverse.is_some() {
            CoercionClass::Iso
        } else {
            CoercionClass::Opaque
        },
        expr,
        inverse,
    }
}

fn successor(key: &str) -> FieldTransform {
    rewrite(
        key,
        op(BuiltinOp::Add, vec![var(key), Expr::Lit(Literal::Int(1))]),
        Some(op(
            BuiltinOp::Sub,
            vec![var(key), Expr::Lit(Literal::Int(1))],
        )),
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

fn int(value: i64) -> Option<FieldPresence> {
    Some(FieldPresence::Present(Value::Int(value)))
}

/// Two records under `s`: record 1 with `n` (node 3) and `m` (node 4),
/// record 2 with `m` (node 5) alone.
fn source(n: Option<FieldPresence>, m: i64) -> WInstance {
    let mut nodes = HashMap::from([
        (0, Node::new(0, "s")),
        (1, Node::new(1, "r")),
        (2, Node::new(2, "r")),
    ]);
    let mut leaf = |id: u32, anchor: &str, value| {
        let mut node = Node::new(id, anchor);
        node.value = value;
        nodes.insert(id, node);
    };
    leaf(3, "r.n", n);
    leaf(4, "r.m", int(m));
    leaf(5, "r.m", int(1));
    let arcs = vec![
        (0, 1, edge("s", "r", "item", None)),
        (0, 2, edge("s", "r", "item", None)),
        (1, 3, edge("r", "r.n", "prop", Some("n"))),
        (1, 4, edge("r", "r.m", "prop", Some("m"))),
        (2, 5, edge("r", "r.m", "prop", Some("m"))),
    ];
    WInstance::new(nodes, arcs, Vec::new(), 0, "s".into())
}

/// `view` with the arc into `child` replaced by one from `parent` under
/// `label`, the child anchored where the label leads.
fn moved(view: &WInstance, child: u32, parent: u32, label: &str) -> WInstance {
    let mut nodes = view.nodes.clone();
    let anchor = format!("r.{label}");
    nodes.get_mut(&child).unwrap().anchor = anchor.as_str().into();
    let arcs = view
        .arcs
        .iter()
        .map(|arc| {
            if arc.1 == child {
                (parent, child, edge("r", &anchor, "prop", Some(label)))
            } else {
                arc.clone()
            }
        })
        .collect();
    WInstance::new(nodes, arcs, Vec::new(), view.root, view.schema_root.clone())
}

#[test]
fn a_child_moved_to_another_record_is_put_back_through_its_new_parents_rewrites() {
    let lens = lens(vec![successor("n")]);
    let (view, complement) = get(&lens, &source(int(3), 7)).unwrap();
    assert_eq!(view.nodes[&3].value, int(4));
    assert_eq!(complement.original_values[&3], int(3));

    // The child moves to the record that held no `n`: its recorded value is
    // not restored there, and the view's value is carried back.
    let shown = moved(&view, 3, 2, "n");
    let restored = put(&lens, &shown, &complement).unwrap();
    assert_eq!(restored.nodes[&3].value, int(3));
    assert_eq!(get(&lens, &restored).unwrap().0.nodes[&3].value, int(4));

    let mut edited = shown;
    edited.nodes.get_mut(&3).unwrap().value = int(9);
    let restored = put(&lens, &edited, &complement).unwrap();
    assert_eq!(restored.nodes[&3].value, int(8));
}

#[test]
fn children_exchanged_between_two_fields_are_put_back_as_the_view_shows_them() {
    let lens = lens(vec![successor("n")]);
    let (view, complement) = get(&lens, &source(int(3), 7)).unwrap();
    // `n`'s child now holds `m` and `m`'s child `n`: each shows what it held.
    let shown = moved(&moved(&view, 3, 1, "m"), 4, 1, "n");
    let restored = put(&lens, &shown, &complement).unwrap();
    assert_eq!(restored.nodes[&3].value, int(4), "now m, shown 4");
    assert_eq!(restored.nodes[&4].value, int(6), "now n, shown 7");
    let (again, _) = get(&lens, &restored).unwrap();
    assert_eq!(again.nodes[&3].value, int(4));
    assert_eq!(again.nodes[&4].value, int(7));
}

/// `null` shown as `"none"` on an `n` that holds text.
fn null_as_none() -> Lens {
    let mut lens = lens(vec![rewrite(
        "n",
        Expr::Match {
            scrutinee: Box::new(op(BuiltinOp::IsNull, vec![var("n")])),
            arms: vec![
                (
                    panproto_expr::Pattern::Lit(Literal::Bool(true)),
                    Expr::Lit(Literal::Str("none".into())),
                ),
                (panproto_expr::Pattern::Wildcard, var("n")),
            ],
        },
        Some(Expr::Match {
            scrutinee: Box::new(op(
                BuiltinOp::Eq,
                vec![var("n"), Expr::Lit(Literal::Str("none".into()))],
            )),
            arms: vec![
                (
                    panproto_expr::Pattern::Lit(Literal::Bool(true)),
                    Expr::Lit(Literal::Null),
                ),
                (panproto_expr::Pattern::Wildcard, var("n")),
            ],
        }),
    )]);
    for schema in [&mut lens.src_schema, &mut lens.tgt_schema] {
        if let Some(vertex) = schema.vertices.get_mut("r.n") {
            vertex.kind = "string".into();
        }
    }
    lens
}

#[test]
fn a_field_that_get_leaves_on_the_record_is_one_with_the_child_of_its_name() {
    let lens = null_as_none();
    let (view, complement) = get(&lens, &source(Some(FieldPresence::Null), 7)).unwrap();
    assert_eq!(
        view.nodes[&1].extra_fields.get("n"),
        Some(&Value::Str("none".into()))
    );
    assert_eq!(view.nodes[&3].value, Some(FieldPresence::Null));
    // The source's null is recorded, since a serialization loses it.
    assert_eq!(complement.original_values[&3], Some(FieldPresence::Null));

    // The view as a serialization holds it: the child shows the field.
    let mut serialized = view.clone();
    serialized
        .nodes
        .get_mut(&1)
        .unwrap()
        .extra_fields
        .remove("n");
    serialized.nodes.get_mut(&3).unwrap().value =
        Some(FieldPresence::Present(Value::Str("none".into())));
    let restored = put(&lens, &serialized, &complement).unwrap();
    assert_eq!(restored.nodes[&3].value, Some(FieldPresence::Null));
    // Get leaves it on the record again: the same view, held where get
    // holds it.
    assert_eq!(
        get(&lens, &restored).unwrap().0.nodes[&1].extra_fields,
        view.nodes[&1].extra_fields
    );

    // An edit there is carried back, and the next get writes it onto the
    // child.
    serialized.nodes.get_mut(&3).unwrap().value =
        Some(FieldPresence::Present(Value::Str("abc".into())));
    let restored = put(&lens, &serialized, &complement).unwrap();
    assert_eq!(
        restored.nodes[&3].value,
        Some(FieldPresence::Present(Value::Str("abc".into())))
    );

    // A view that holds the field on the record keeps the child as a field of
    // its own: an edit of the record's field is refused, since get would show
    // it on the child, and so is an edit of the child it shadows.
    let mut on_record = view.clone();
    on_record
        .nodes
        .get_mut(&1)
        .unwrap()
        .extra_fields
        .insert("n".into(), Value::Str("abc".into()));
    assert!(matches!(
        put(&lens, &on_record, &complement),
        Err(LensError::PutDomain(_))
    ));
    let mut shadowed = view.clone();
    shadowed.nodes.get_mut(&3).unwrap().value =
        Some(FieldPresence::Present(Value::Str("abc".into())));
    assert!(matches!(
        put(&lens, &shadowed, &complement),
        Err(LensError::PutDomain(_))
    ));
    check_put_get(&lens, &source(Some(FieldPresence::Null), 7)).unwrap();
}

#[test]
fn an_iso_rewrite_has_the_optic_laws_where_it_leaves_a_value_unchanged() {
    let lens = lens(vec![successor("n"), rewrite("m", var("m"), Some(var("m")))]);
    // `m` is read and left unchanged: nothing is recorded of it, as before.
    let unchanged = {
        let source = source(int(3), 7);
        let mut nodes = source.nodes.clone();
        nodes.remove(&3);
        let arcs = source
            .arcs
            .iter()
            .filter(|(_, child, _)| *child != 3)
            .cloned()
            .collect();
        WInstance::new(nodes, arcs, Vec::new(), 0, "s".into())
    };
    let (_, complement) = get(&lens, &unchanged).unwrap();
    assert!(complement.original_values.is_empty());
    check_optic_laws(OpticKind::Iso, &lens, &unchanged).unwrap();
    // `n` changes, and the complement records it, as before.
    assert!(check_optic_laws(OpticKind::Iso, &lens, &source(int(3), 7)).is_err());
}

#[test]
fn complements_with_and_without_the_new_records_put_alike_through_the_graph() {
    let lens = null_as_none();
    let tree = source(Some(FieldPresence::Null), 7);
    let (view, complement) = get(&lens, &tree).unwrap();
    let mut earlier = complement.clone();
    earlier.original_values.remove(&3);
    let (graph, source_embedding) = GInstance::from_wtype_embedded(&tree, None).unwrap();
    let (graph_view, view_embedding) = GInstance::from_wtype_embedded(&view, None).unwrap();
    let convert = |complement| {
        tree_complement_to_graph(
            &lens,
            &graph,
            &graph_view,
            complement,
            &source_embedding,
            &view_embedding,
        )
        .unwrap()
    };
    let (with, without) = (convert(&complement), convert(&earlier));
    assert_eq!(
        graph_lens::put(&lens, &graph_view, &with).unwrap(),
        graph_lens::put(&lens, &graph_view, &without).unwrap()
    );
    // Merging a complement with itself keeps its records.
    let merged = complement.compose(&complement).unwrap();
    assert_eq!(merged.original_values, complement.original_values);
}
