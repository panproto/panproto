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
            cardinality: Cardinality::One,
            nullable: false,
            on_unresolved: policy,
            mode: ResolveMode::AddEdge,
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
    let (view, c) = resolution::get(&s, &t, false).unwrap().unwrap();
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
    let (_, c) = resolution::get(&s, &t, true).unwrap().unwrap();
    assert_eq!(c.unresolved.len(), 1);
}
