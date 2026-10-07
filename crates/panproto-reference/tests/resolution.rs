#![allow(clippy::unwrap_used, clippy::expect_used)]
//! A key-value reference resolved to an edge, and its put.

use std::collections::BTreeMap;

use panproto_reference::model::{GEdge, Node, SEdge, Tree};
use panproto_reference::resolution::{self, ResolveError};
use panproto_reference::schema::{
    Cardinality, Key, KeyPart, KeyRole, KeyScope, OnUnresolved, RefSource, Reference, ResolveMode,
    Schema,
};
use panproto_reference::value::Value;

fn setup(policy: OnUnresolved) -> (Schema, Tree) {
    setup_with(policy, Cardinality::One, ResolveMode::AddEdge)
}

fn setup_with(policy: OnUnresolved, cardinality: Cardinality, mode: ResolveMode) -> (Schema, Tree) {
    let lead = SEdge::new("film", "s", "prop", Some("lead"));
    let name = SEdge::new("actor", "s", "prop", Some("name"));
    let link = SEdge::new("film", "actor", "ref", Some("lead_actor"));
    let mut s = Schema::default()
        .vertex("doc", "obj")
        .vertex("film", "obj")
        .vertex("actor", "obj")
        .vertex("s", "str")
        .edge("doc", "film", "prop", Some("film"))
        .edge("doc", "actor", "prop", Some("actor"));
    s.edges.extend([lead.clone(), name.clone(), link.clone()]);
    s.keys.insert(
        "actor".into(),
        vec![Key {
            id: "pk".into(),
            parts: vec![KeyPart::Path(vec![name])],
            role: KeyRole::Primary,
            scope: KeyScope::Global,
        }],
    );
    s.references.insert(
        "film_lead".into(),
        Reference {
            id: "film_lead".into(),
            holder: "film".into(),
            source: RefSource::KeyValue {
                parts: vec![vec![lead]],
            },
            target_vertex: "actor".into(),
            target_key: Some("pk".into()),
            edge: Some(link),
            cardinality,
            nullable: false,
            on_unresolved: policy,
            mode,
        },
    );
    let nodes = BTreeMap::from([
        (0, Node::new(0, "doc")),
        (
            1,
            Node::new(1, "film").with_field("lead", Value::Str("Ann".into())),
        ),
        (
            2,
            Node::new(2, "actor").with_field("name", Value::Str("Ann".into())),
        ),
        (
            3,
            Node::new(3, "actor").with_field("name", Value::Str("Bob".into())),
        ),
    ]);
    let arcs = vec![
        (0, 1, SEdge::new("doc", "film", "prop", Some("film"))),
        (0, 2, SEdge::new("doc", "actor", "prop", Some("actor"))),
        (0, 3, SEdge::new("doc", "actor", "prop", Some("actor"))),
    ];
    (
        s,
        Tree {
            nodes,
            arcs,
            fans: vec![],
            root: 0,
        },
    )
}

#[test]
fn a_reference_resolves_and_a_reconnected_edge_rewrites_the_key() {
    let (s, t) = setup(OnUnresolved::Error);
    let (view, c) = resolution::get(&s, &t, false).unwrap();
    assert_eq!(view.edges[&3].src, 1);
    assert_eq!(view.edges[&3].tgt, 2);
    assert_eq!(resolution::put(&s, &view, &c), Ok(t));
    let mut moved = view;
    moved.edges.insert(
        3,
        GEdge {
            tgt: 3,
            ..moved.edges[&3].clone()
        },
    );
    let back = resolution::put(&s, &moved, &c).unwrap();
    assert_eq!(back.nodes[&1].fields["lead"], Value::Str("Bob".into()));
}

#[test]
fn a_dangling_reference_is_strict_or_recorded() {
    let (s, mut t) = setup(OnUnresolved::Error);
    t.nodes.insert(
        1,
        Node::new(1, "film").with_field("lead", Value::Str("Cy".into())),
    );
    assert!(matches!(
        resolution::get(&s, &t, false),
        Err(ResolveError::DanglingReference { .. })
    ));
    let (_, c) = resolution::get(&s, &t, true).unwrap();
    assert_eq!(c.unresolved.len(), 1);
}

#[test]
fn a_reference_edge_from_a_missing_node_is_unrepresentable_not_a_panic() {
    let (s, t) = setup(OnUnresolved::Error);
    let (mut view, c) = resolution::get(&s, &t, false).unwrap();
    view.edges.insert(
        3,
        GEdge {
            src: 7,
            ..view.edges[&3].clone()
        },
    );
    assert_eq!(
        format!("{:?}", resolution::put(&s, &view, &c)),
        "Err(UnrepresentableReference { carrier: Edge(3), reference: None })"
    );
}

fn many_tree(t: &mut Tree, leads: &[&str]) {
    t.nodes.insert(
        1,
        Node::new(1, "film").with_field(
            "lead",
            Value::List(leads.iter().map(|l| Value::Str((*l).into())).collect()),
        ),
    );
}

#[test]
fn a_many_reference_resolves_each_position_and_writes_its_sequence_back() {
    let (s, mut t) = setup_with(OnUnresolved::Error, Cardinality::Many, ResolveMode::AddEdge);
    many_tree(&mut t, &["Bob", "Ann", "Bob"]);
    let (view, c) = resolution::get(&s, &t, false).unwrap();
    // One edge per position, duplicates kept, in occurrence order.
    let targets: Vec<(u32, u32)> = view
        .edges
        .iter()
        .filter(|(i, _)| c.resolved.contains_key(i))
        .map(|(_, e)| (e.src, e.tgt))
        .collect();
    assert_eq!(targets, vec![(1, 3), (1, 2), (1, 3)]);
    assert_eq!(resolution::put(&s, &view, &c), Ok(t.clone()));
    // Removing the middle occurrence's edge removes it from the sequence.
    let middle = *c.resolved.iter().find(|(_, o)| o.index == 1).unwrap().0;
    let mut removed = view.clone();
    removed.edges.remove(&middle);
    let back = resolution::put(&s, &removed, &c).unwrap();
    assert_eq!(
        back.nodes[&1].fields["lead"],
        Value::List(vec![Value::Str("Bob".into()), Value::Str("Bob".into())])
    );
    // Reconnecting the first occurrence rewrites its position.
    let first = *c.resolved.iter().find(|(_, o)| o.index == 0).unwrap().0;
    let mut moved = view;
    moved.edges.get_mut(&first).unwrap().tgt = 2;
    let back = resolution::put(&s, &moved, &c).unwrap();
    assert_eq!(
        back.nodes[&1].fields["lead"],
        Value::List(vec![
            Value::Str("Ann".into()),
            Value::Str("Ann".into()),
            Value::Str("Bob".into())
        ])
    );
    // A non-nullable Many with no occurrence is a cardinality mismatch.
    many_tree(&mut t, &[]);
    assert_eq!(
        resolution::get(&s, &t, false).map(|_| ()),
        Err(ResolveError::ReferenceCardinalityMismatch {
            reference: "film_lead".into(),
            at: panproto_reference::lens::ViewRef::Node(1),
            lengths: vec![0],
        })
    );
}

#[test]
fn a_redirect_removes_the_holder_and_points_its_arc_at_the_target() {
    let (s, t) = setup_with(OnUnresolved::Error, Cardinality::One, ResolveMode::Redirect);
    let (view, c) = resolution::get(&s, &t, false).unwrap();
    // The film node is gone, and the reference edge runs from its parent.
    assert!(!view.nodes.contains_key(&1));
    let (id, edge) = view
        .edges
        .iter()
        .find(|(i, _)| c.resolved.contains_key(i))
        .map(|(i, e)| (*i, e.clone()))
        .unwrap();
    assert_eq!((edge.src, edge.tgt), (0, 2));
    assert_eq!(c.redirected[&1].parent, 0);
    assert_eq!(resolution::put(&s, &view, &c), Ok(t.clone()));
    // Reconnecting the edge rewrites the restored holder's key.
    let mut moved = view.clone();
    moved.edges.get_mut(&id).unwrap().tgt = 3;
    let back = resolution::put(&s, &moved, &c).unwrap();
    assert_eq!(back.nodes[&1].fields["lead"], Value::Str("Bob".into()));
    // Removing the edge removes the occurrence and its subtree.
    let mut removed = view;
    removed.edges.remove(&id);
    let back = resolution::put(&s, &removed, &c).unwrap();
    assert!(!back.nodes.contains_key(&1));
    assert_eq!(back.arcs.len(), 2);
}
