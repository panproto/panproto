#![allow(clippy::unwrap_used, clippy::expect_used)]
//! The tree form of a graph: every malformed fan and arc is a typed
//! refusal, and the tree Kan extensions report why a tree has no graph
//! form or a result has no tree form.

use std::collections::{BTreeMap, BTreeSet};

use panproto_reference::encode::{self, NotATree, TreeEncodeError, TreeKanError};
use panproto_reference::fan::FanViolation;
use panproto_reference::migration::Migration;
use panproto_reference::model::{GEdge, Graph, Node, NodeShape, SEdge, Tree};
use panproto_reference::rel;
use panproto_reference::schema::{HyperEdge, Schema};
use panproto_reference::value::Value;

const fn edge(src: u32, tgt: u32, label: SEdge) -> GEdge {
    GEdge {
        src,
        tgt,
        label,
        position: None,
    }
}

/// Root 0 at `r`, its child 1 at `a` by arc 0, and fan node 2 of `h` with
/// parent 0 (edge 1) and child `x` = 1 (edge 2), plus `extra` edges.
fn fan_graph(extra: &[(u32, GEdge)]) -> Graph {
    let mut g = Graph::default();
    g.nodes.insert(0, Node::new(0, "r"));
    g.nodes.insert(1, Node::new(1, "a"));
    let mut fan = Node::new(2, "h");
    fan.shape = NodeShape::HyperEdge;
    g.nodes.insert(2, fan);
    g.edges
        .insert(0, edge(0, 1, SEdge::new("r", "a", "prop", None)));
    g.edges
        .insert(1, edge(2, 0, SEdge::new("h", "r", "hyper_parent", None)));
    g.edges.insert(
        2,
        edge(2, 1, SEdge::new("h", "a", "hyper_label", Some("x"))),
    );
    for (id, e) in extra {
        g.edges.insert(*id, e.clone());
    }
    g
}

const fn none(_: &str) -> encode::ParentNameRule {
    encode::ParentNameRule::Any
}

const fn unnamed(_: &str) -> encode::ParentNameRule {
    encode::ParentNameRule::Exact(None)
}

#[test]
fn a_well_formed_fan_graph_reads_as_a_tree_and_back() {
    let g = fan_graph(&[]);
    let t = encode::graph_to_tree(&g, 0, &none).unwrap();
    assert_eq!(t.arcs, vec![(0, 1, SEdge::new("r", "a", "prop", None))]);
    assert_eq!(t.fans.len(), 1);
    assert_eq!(t.fans[0].parent, 0);
    assert_eq!(t.fans[0].children, BTreeMap::from([("x".to_owned(), 1)]));
    let (back, _) = encode::tree_to_graph(&t, None).unwrap();
    assert_eq!(back, g);
}

#[test]
fn a_malformed_fan_has_no_tree_form() {
    let foreign = fan_graph(&[(3, edge(2, 1, SEdge::new("h", "a", "prop", None)))]);
    assert_eq!(
        encode::graph_to_tree(&foreign, 0, &none),
        Err(NotATree::Fan {
            node: 2,
            violation: FanViolation::ForeignIncidentEdge { edge: 3 }
        })
    );
    let nameless = fan_graph(&[(3, edge(2, 1, SEdge::new("h", "a", "hyper_label", None)))]);
    assert_eq!(
        encode::graph_to_tree(&nameless, 0, &none),
        Err(NotATree::Fan {
            node: 2,
            violation: FanViolation::ForeignIncidentEdge { edge: 3 }
        })
    );
    let duplicate = fan_graph(&[(
        3,
        edge(2, 0, SEdge::new("h", "r", "hyper_label", Some("x"))),
    )]);
    assert_eq!(
        encode::graph_to_tree(&duplicate, 0, &none),
        Err(NotATree::Fan {
            node: 2,
            violation: FanViolation::DuplicateLabel {
                label: "x".into(),
                edges: vec![2, 3]
            }
        })
    );
    let two_parents = fan_graph(&[(3, edge(2, 1, SEdge::new("h", "a", "hyper_parent", None)))]);
    assert_eq!(
        encode::graph_to_tree(&two_parents, 0, &none),
        Err(NotATree::Fan {
            node: 2,
            violation: FanViolation::MultipleParents { edges: vec![1, 3] }
        })
    );
}

#[test]
fn every_other_malformed_graph_has_its_own_reason() {
    let mut data = fan_graph(&[]);
    data.nodes.get_mut(&2).unwrap().value =
        Some(panproto_reference::value::Presence::Present(Value::Int(1)));
    assert_eq!(
        encode::graph_to_tree(&data, 0, &none),
        Err(NotATree::FanData { node: 2 })
    );
    let mut misnamed = fan_graph(&[]);
    misnamed.edges.get_mut(&1).unwrap().label.name = Some("p".into());
    // Without a schema, the parent-edge name is deliberately unconstrained.
    assert!(encode::graph_to_tree(&misnamed, 0, &none).is_ok());
    // An explicit exact-unnamed policy remains independently observable.
    assert_eq!(
        encode::graph_to_tree(&misnamed, 0, &unnamed),
        Err(NotATree::FanLabel { edge: 1 })
    );
    // Under a schema declaring `h` with parent label `p`, the named parent
    // edge is the graph form's and the unnamed one is not.
    let schema = Schema {
        hyper_edges: [(
            "h".to_owned(),
            HyperEdge {
                kind: "k".into(),
                signature: BTreeMap::new(),
                parent_label: "p".into(),
            },
        )]
        .into(),
        ..Schema::default()
    };
    let named = encode::schema_parent_name(Some(&schema));
    assert!(encode::graph_to_tree(&misnamed, 0, &named).is_ok());
    assert_eq!(
        encode::graph_to_tree(&fan_graph(&[]), 0, &named),
        Err(NotATree::FanLabel { edge: 1 })
    );
    let mut relabelled = fan_graph(&[]);
    relabelled.edges.get_mut(&2).unwrap().label.tgt = "r".into();
    assert_eq!(
        encode::graph_to_tree(&relabelled, 0, &none),
        Err(NotATree::FanLabel { edge: 2 })
    );
    let mut positioned = fan_graph(&[]);
    positioned.edges.get_mut(&0).unwrap().position = Some(0);
    assert_eq!(
        encode::graph_to_tree(&positioned, 0, &none),
        Err(NotATree::PositionedEdge { edge: 0 })
    );
    let reserved = fan_graph(&[(
        3,
        edge(0, 1, SEdge::new("r", "a", "hyper_label", Some("y"))),
    )]);
    assert_eq!(
        encode::graph_to_tree(&reserved, 0, &none),
        Err(NotATree::ReservedArc { edge: 3 })
    );
    let into_fan = fan_graph(&[(3, edge(1, 2, SEdge::new("a", "h", "prop", None)))]);
    assert_eq!(
        encode::graph_to_tree(&into_fan, 0, &none),
        Err(NotATree::ArcIntoFan { edge: 3 })
    );
    let into_root = fan_graph(&[(3, edge(1, 0, SEdge::new("a", "r", "prop", None)))]);
    assert_eq!(
        encode::graph_to_tree(&into_root, 0, &none),
        Err(NotATree::RootHasParent { edge: 3 })
    );
    let mut second = fan_graph(&[]);
    second.nodes.insert(3, Node::new(3, "r"));
    second
        .edges
        .insert(3, edge(3, 1, SEdge::new("r", "a", "prop", None)));
    assert_eq!(
        encode::graph_to_tree(&second, 0, &none),
        Err(NotATree::SecondParent { node: 1 })
    );
    let mut stray = fan_graph(&[]);
    stray.nodes.insert(5, Node::new(5, "a"));
    assert_eq!(
        encode::graph_to_tree(&stray, 0, &none),
        Err(NotATree::Unreached { node: 5 })
    );
    let dangling = fan_graph(&[(3, edge(1, 9, SEdge::new("a", "a", "prop", None)))]);
    assert_eq!(
        encode::graph_to_tree(&dangling, 0, &none),
        Err(NotATree::DanglingEdge { edge: 3 })
    );
    assert_eq!(
        encode::graph_to_tree(&fan_graph(&[]), 2, &none),
        Err(NotATree::MissingRoot)
    );
    assert_eq!(
        encode::graph_to_tree(&fan_graph(&[]), 7, &none),
        Err(NotATree::MissingRoot)
    );
}

fn one_table() -> (rel::RelFunctor, Schema) {
    let s = Schema::default().vertex("r", "record");
    let r = rel::classify(
        &s,
        &BTreeSet::from(["int".to_owned()]),
        &BTreeSet::from(["record".to_owned()]),
    )
    .unwrap();
    let m = Migration {
        surviving_verts: ["r".to_owned()].into(),
        ..Migration::default()
    };
    (
        rel::entity_functor(&r, &r, &m).unwrap().into_kan().unwrap(),
        s,
    )
}

#[test]
fn the_tree_kan_extensions_name_why_a_tree_is_refused() {
    let (f, _) = one_table();
    let reserved = Tree {
        nodes: BTreeMap::from([(0, Node::new(0, "r")), (1, Node::new(1, "r"))]),
        arcs: vec![(0, 1, SEdge::new("r", "r", "hyper_label", Some("x")))],
        fans: vec![],
        root: 0,
    };
    let refused = Err(TreeKanError::Encode(TreeEncodeError::ReservedVocabulary));
    assert_eq!(encode::t_sigma(&f, &reserved), refused);
    assert_eq!(encode::t_pi(&f, &reserved, usize::MAX), refused);
    assert_eq!(encode::t_delta(&f, &reserved), refused);
    // Two rows with no relation between them: graph Sigma keeps both, so
    // the result has two roots.
    let two = Tree {
        nodes: BTreeMap::from([(0, Node::new(0, "r")), (1, Node::new(1, "r"))]),
        arcs: vec![],
        fans: vec![],
        root: 0,
    };
    assert_eq!(
        encode::t_sigma(&f, &two),
        Err(TreeKanError::RootCount { count: 2 })
    );
    let one = Tree {
        nodes: BTreeMap::from([(4, Node::new(4, "r"))]),
        arcs: vec![],
        fans: vec![],
        root: 4,
    };
    assert_eq!(encode::t_sigma(&f, &one), Ok(one.clone()));
    assert_eq!(encode::t_pi(&f, &one, usize::MAX), Ok(one.clone()));
    assert_eq!(encode::t_delta(&f, &one), Ok(one));
}
