#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Algebraic identities of the edit algebra on tiny deltas.

use panproto_reference::edit::{self, AttrKey, AttrVal, GElem, GPayload, GraphDelta, Mismatch};
use panproto_reference::model::{GEdge, Graph, Node, SEdge};

fn node(id: u32, a: &str) -> GPayload {
    GPayload::Node(Node::new(id, a))
}

fn graph() -> Graph {
    let mut g = Graph::default();
    g.nodes.insert(0, Node::new(0, "a"));
    g.nodes.insert(1, Node::new(1, "b"));
    g.edges.insert(
        0,
        GEdge {
            src: 0,
            tgt: 1,
            label: SEdge::new("a", "b", "e", None),
            position: None,
        },
    );
    g
}

#[test]
fn add_then_delete_cancels_and_delete_then_add_replaces() {
    let mut add = GraphDelta::identity();
    add.add.insert(GElem::Node(5), node(5, "a"));
    let del = add.invert();
    assert_eq!(add.compose(&del), Ok(GraphDelta::identity()));
    let repl = del.compose(&add).unwrap();
    assert_eq!(repl.del.len() + repl.add.len(), 2);
    assert_eq!(add.invert().invert(), add);
}

#[test]
fn composition_is_partial_and_both_bracketings_may_refuse() {
    // a = Add(q), b = Attr(k: q[k] -> n), c = Del(r) with r[k] = n and
    // r != q[k := n]: both bracketings refuse.
    let mut a = GraphDelta::identity();
    a.add.insert(GElem::Node(5), node(5, "a"));
    let mut b = GraphDelta::identity();
    b.attrs.insert(
        AttrKey::Anchor(5),
        (AttrVal::Anchor("a".into()), AttrVal::Anchor("z".into())),
    );
    let mut c = GraphDelta::identity();
    let mut r = Node::new(5, "z");
    r.discriminator = Some("d".into());
    c.del.insert(GElem::Node(5), GPayload::Node(r));
    assert!(a.compose(&b).is_ok() && b.compose(&c).is_ok());
    assert!(a.compose(&b).unwrap().compose(&c).is_err());
    assert!(a.compose(&b.compose(&c).unwrap()).is_err());
    // attr after add then delete is defined, attr before add is not.
    assert!(matches!(b.compose(&a), Err(Mismatch::Elem(_))));
}

#[test]
fn the_differ_applies_and_regularity_holds() {
    let g = graph();
    let mut h = g.clone();
    h.nodes.get_mut(&1).unwrap().anchor = "c".into();
    h.edges.get_mut(&0).unwrap().tgt = 0;
    h.nodes.insert(7, Node::new(7, "a"));
    let d = edit::diff(&g, &h);
    assert_eq!(edit::apply(&d, &g), Ok(h.clone()));
    assert_eq!(edit::diff(&g, &g), GraphDelta::identity());
    assert_eq!(edit::apply(&d.invert(), &h), Ok(g));
    let back = d.compose(&d.invert()).unwrap().compose(&d).unwrap();
    assert_eq!(back, d);
}

#[test]
fn independent_deltas_commute_and_merge() {
    let g = graph();
    let mut a = GraphDelta::identity();
    a.add.insert(GElem::Node(9), node(9, "a"));
    let mut b = GraphDelta::identity();
    b.attrs.insert(
        AttrKey::Field(1, "x".into()),
        (
            AttrVal::Field(None),
            AttrVal::Field(Some(panproto_reference::value::Value::Int(1))),
        ),
    );
    assert!(edit::independence(&a, &b).is_empty());
    let ab = edit::apply(&b, &edit::apply(&a, &g).unwrap()).unwrap();
    let ba = edit::apply(&a, &edit::apply(&b, &g).unwrap()).unwrap();
    assert_eq!(ab, ba);
    assert_eq!(edit::apply(&edit::merge(&a, &b).unwrap(), &g).unwrap(), ab);
    assert!(!edit::independence(&a, &a).is_empty());
    // Deleting a node another delta re-anchors is a conflict.
    let mut del = GraphDelta::identity();
    del.del.insert(GElem::Node(0), node(0, "a"));
    let mut anchor = GraphDelta::identity();
    anchor.attrs.insert(
        AttrKey::Anchor(0),
        (AttrVal::Anchor("a".into()), AttrVal::Anchor("q".into())),
    );
    assert!(!edit::independence(&del, &anchor).is_empty());
    assert!(matches!(
        edit::apply(&del, &g),
        Err(edit::ApplyError::DanglingEdges { node: 0, .. })
    ));
}
