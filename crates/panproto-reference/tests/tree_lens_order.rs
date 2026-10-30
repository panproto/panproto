#![allow(clippy::expect_used, clippy::unwrap_used)]
//! Exact child-order laws for the reference tree lens.

use std::collections::BTreeMap;

use panproto_reference::lens::{GraphLens, tree_get, tree_put};
use panproto_reference::migration::Migration;
use panproto_reference::model::{Fan, Node, SEdge, Tree};
use panproto_reference::restrict::{NoValueLayer, Plan, compile};
use panproto_reference::schema::{HyperEdge, Schema};
use panproto_reference::validation::Semantics;

fn ordinary_schema() -> Schema {
    Schema::default()
        .vertex("root", "obj")
        .vertex("item", "obj")
        .edge("root", "item", "prop", Some("item"))
        .edge("root", "item", "prop", Some("first"))
        .edge("root", "item", "prop", Some("second"))
        .edge("root", "item", "prop", Some("third"))
}

fn identity_migration(schema: &Schema) -> Migration {
    Migration {
        surviving_verts: schema.vertices.keys().cloned().collect(),
        surviving_edges: schema.edges.clone(),
        ..Migration::default()
    }
}

fn lens<'a>(
    schema: &'a Schema,
    migration: &'a Migration,
    plan: &'a Plan,
    semantics: &'a BTreeMap<String, Semantics>,
) -> GraphLens<'a> {
    GraphLens::new(schema, schema, migration, plan, semantics, &NoValueLayer).unwrap()
}

fn two_child_tree(first_label: &str, second_label: &str) -> Tree {
    Tree {
        nodes: [
            (0, Node::new(0, "root")),
            (1, Node::new(1, "item")),
            (2, Node::new(2, "item")),
        ]
        .into(),
        arcs: vec![
            (0, 1, SEdge::new("root", "item", "prop", Some(first_label))),
            (0, 2, SEdge::new("root", "item", "prop", Some(second_label))),
        ],
        fans: vec![],
        root: 0,
    }
}

#[test]
fn tree_put_preserves_a_swap_of_two_existing_children() {
    let schema = ordinary_schema();
    let migration = identity_migration(&schema);
    let plan = compile(&schema, &schema, &migration).unwrap();
    let semantics = BTreeMap::new();
    let lens = lens(&schema, &migration, &plan, &semantics);
    let source = two_child_tree("first", "second");
    let (mut view, complement, embedding) = tree_get(&lens, &source).unwrap().unwrap();

    view.arcs.swap(0, 1);
    let restored = tree_put(&lens, &view, &embedding, &complement).unwrap();
    let (round_trip, _, _) = tree_get(&lens, &restored).unwrap().unwrap();

    assert_eq!(restored.arcs, view.arcs);
    assert_eq!(round_trip, view);
}

#[test]
fn tree_put_preserves_a_new_child_inserted_in_the_middle() {
    let schema = ordinary_schema();
    let migration = identity_migration(&schema);
    let plan = compile(&schema, &schema, &migration).unwrap();
    let semantics = BTreeMap::new();
    let lens = lens(&schema, &migration, &plan, &semantics);
    let source = two_child_tree("first", "third");
    let (mut view, complement, embedding) = tree_get(&lens, &source).unwrap().unwrap();

    view.nodes.insert(3, Node::new(3, "item"));
    view.arcs.insert(
        1,
        (0, 3, SEdge::new("root", "item", "prop", Some("second"))),
    );
    let restored = tree_put(&lens, &view, &embedding, &complement).unwrap();
    let (round_trip, _, _) = tree_get(&lens, &restored).unwrap().unwrap();

    assert_eq!(restored.arcs, view.arcs);
    assert_eq!(round_trip, view);
}

#[test]
fn tree_put_orders_repeated_equal_labels_by_child_identity() {
    let schema = ordinary_schema();
    let migration = identity_migration(&schema);
    let plan = compile(&schema, &schema, &migration).unwrap();
    let semantics = BTreeMap::new();
    let lens = lens(&schema, &migration, &plan, &semantics);
    let source = two_child_tree("item", "item");
    let (mut view, complement, embedding) = tree_get(&lens, &source).unwrap().unwrap();

    view.arcs.swap(0, 1);
    let restored = tree_put(&lens, &view, &embedding, &complement).unwrap();
    let (round_trip, _, _) = tree_get(&lens, &restored).unwrap().unwrap();

    assert_eq!(restored.arcs, view.arcs);
    assert_eq!(
        restored.arcs.iter().map(|arc| arc.1).collect::<Vec<_>>(),
        vec![2, 1]
    );
    assert_eq!(round_trip, view);
}

#[test]
fn tree_put_keeps_hidden_arc_slots_while_reordering_visible_children() {
    let kept = SEdge::new("root", "item", "prop", Some("item"));
    let hidden = SEdge::new("root", "hidden", "prop", Some("hidden"));
    let source_schema = Schema::default()
        .vertex("root", "obj")
        .vertex("item", "obj")
        .vertex("hidden", "obj")
        .edge("root", "item", "prop", Some("item"))
        .edge("root", "hidden", "prop", Some("hidden"));
    let target_schema = Schema::default()
        .vertex("root", "obj")
        .vertex("item", "obj")
        .edge("root", "item", "prop", Some("item"));
    let migration = Migration {
        surviving_verts: ["root".to_owned(), "item".to_owned()].into(),
        surviving_edges: [kept.clone()].into(),
        ..Migration::default()
    };
    let plan = compile(&source_schema, &target_schema, &migration).unwrap();
    let semantics = BTreeMap::new();
    let lens = GraphLens::new(
        &source_schema,
        &target_schema,
        &migration,
        &plan,
        &semantics,
        &NoValueLayer,
    )
    .unwrap();
    let source = Tree {
        nodes: [
            (0, Node::new(0, "root")),
            (1, Node::new(1, "item")),
            (2, Node::new(2, "hidden")),
            (3, Node::new(3, "item")),
        ]
        .into(),
        arcs: vec![
            (0, 1, kept.clone()),
            (0, 2, hidden.clone()),
            (0, 3, kept.clone()),
        ],
        fans: vec![],
        root: 0,
    };
    let (mut view, complement, embedding) = tree_get(&lens, &source).unwrap().unwrap();

    let unedited = tree_put(&lens, &view, &embedding, &complement).unwrap();
    assert_eq!(unedited, source);

    view.arcs.swap(0, 1);
    let restored = tree_put(&lens, &view, &embedding, &complement).unwrap();
    assert_eq!(
        restored.arcs.iter().map(|arc| arc.1).collect::<Vec<_>>(),
        vec![3, 2, 1]
    );
    assert_eq!(restored.arcs[1].2, hidden);
    let (round_trip, _, _) = tree_get(&lens, &restored).unwrap().unwrap();
    assert_eq!(round_trip, view);

    let (mut inserted, complement, embedding) = tree_get(&lens, &source).unwrap().unwrap();
    inserted.nodes.insert(4, Node::new(4, "item"));
    inserted.arcs.insert(1, (0, 4, kept));
    let restored = tree_put(&lens, &inserted, &embedding, &complement).unwrap();
    assert_eq!(
        restored.arcs.iter().map(|arc| arc.1).collect::<Vec<_>>(),
        vec![1, 2, 4, 3]
    );
    assert_eq!(restored.arcs[1].2, hidden);
    let (round_trip, _, _) = tree_get(&lens, &restored).unwrap().unwrap();
    assert_eq!(round_trip, inserted);
}

#[test]
fn tree_put_orders_restored_source_tuples_after_edge_remap_and_reparenting() {
    let branch = SEdge::new("root", "root", "prop", Some("branch"));
    let source_item = SEdge::new("root", "item", "prop", Some("source_item"));
    let target_item = SEdge::new("root", "item", "prop", Some("target_item"));
    let source_schema = Schema::default()
        .vertex("root", "obj")
        .vertex("item", "obj")
        .edge("root", "root", "prop", Some("branch"))
        .edge("root", "item", "prop", Some("source_item"));
    let target_schema = Schema::default()
        .vertex("root", "obj")
        .vertex("item", "obj")
        .edge("root", "root", "prop", Some("branch"))
        .edge("root", "item", "prop", Some("target_item"));
    let migration = Migration {
        surviving_verts: ["root".to_owned(), "item".to_owned()].into(),
        surviving_edges: [branch.clone()].into(),
        edge_remap: [(source_item.clone(), target_item.clone())].into(),
        ..Migration::default()
    };
    let plan = compile(&source_schema, &target_schema, &migration).unwrap();
    let semantics = BTreeMap::new();
    let lens = GraphLens::new(
        &source_schema,
        &target_schema,
        &migration,
        &plan,
        &semantics,
        &NoValueLayer,
    )
    .unwrap();
    let source = Tree {
        nodes: [
            (0, Node::new(0, "root")),
            (1, Node::new(1, "item")),
            (2, Node::new(2, "item")),
            (3, Node::new(3, "root")),
        ]
        .into(),
        arcs: vec![
            (0, 3, branch.clone()),
            (0, 1, source_item.clone()),
            (0, 2, source_item.clone()),
        ],
        fans: vec![],
        root: 0,
    };
    let (mut view, complement, embedding) = tree_get(&lens, &source).unwrap().unwrap();

    let moved = view.arcs.remove(2);
    assert_eq!(moved, (0, 2, target_item.clone()));
    view.arcs.insert(1, (3, 2, target_item));
    let restored = tree_put(&lens, &view, &embedding, &complement).unwrap();

    assert_eq!(
        restored.arcs,
        vec![
            (0, 3, branch),
            (3, 2, source_item.clone()),
            (0, 1, source_item),
        ]
    );
    let (round_trip, _, _) = tree_get(&lens, &restored).unwrap().unwrap();
    assert_eq!(round_trip, view);
}

#[test]
fn tree_put_preserves_fan_occurrence_order_and_label_to_child_maps() {
    let mut schema = ordinary_schema();
    schema.hyper_edges.insert(
        "pair".to_owned(),
        HyperEdge {
            kind: "pair".to_owned(),
            signature: [
                ("left".to_owned(), "item".to_owned()),
                ("right".to_owned(), "item".to_owned()),
            ]
            .into(),
            parent_label: "parent".to_owned(),
        },
    );
    let migration = identity_migration(&schema);
    let plan = compile(&schema, &schema, &migration).unwrap();
    let semantics = BTreeMap::new();
    let lens = lens(&schema, &migration, &plan, &semantics);
    let mut source = two_child_tree("item", "item");
    source.fans = vec![
        Fan {
            parent: 0,
            hyper_edge: "pair".to_owned(),
            children: [("left".to_owned(), 1), ("right".to_owned(), 2)].into(),
        },
        Fan {
            parent: 0,
            hyper_edge: "pair".to_owned(),
            children: [("left".to_owned(), 2), ("right".to_owned(), 1)].into(),
        },
    ];
    let (mut view, complement, embedding) = tree_get(&lens, &source).unwrap().unwrap();

    view.fans.swap(0, 1);
    view.fans[1].children.insert("left".to_owned(), 2);
    view.fans[1].children.insert("right".to_owned(), 2);
    let restored = tree_put(&lens, &view, &embedding, &complement).unwrap();
    let (round_trip, _, _) = tree_get(&lens, &restored).unwrap().unwrap();

    assert_eq!(restored.fans, view.fans);
    assert_eq!(round_trip.fans, view.fans);
}

#[test]
fn tree_put_get_put_and_deletion_keep_exact_order() {
    let schema = ordinary_schema();
    let migration = identity_migration(&schema);
    let plan = compile(&schema, &schema, &migration).unwrap();
    let semantics = BTreeMap::new();
    let lens = lens(&schema, &migration, &plan, &semantics);
    let source = two_child_tree("first", "second");
    let (mut view, complement, embedding) = tree_get(&lens, &source).unwrap().unwrap();

    assert_eq!(tree_put(&lens, &view, &embedding, &complement), Ok(source));

    view.arcs.remove(0);
    view.nodes.remove(&1);
    let restored = tree_put(&lens, &view, &embedding, &complement).unwrap();
    let (round_trip, _, _) = tree_get(&lens, &restored).unwrap().unwrap();
    assert_eq!(restored.arcs, view.arcs);
    assert_eq!(round_trip, view);
}
