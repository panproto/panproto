#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Tree Delta, Sigma and Pi through the graph form: worked examples on a
//! two-node tree and the typed refusals of tree Delta.

use std::collections::{BTreeMap, BTreeSet};

use panproto_reference::canonical;
use panproto_reference::encode::{self, TreeKanError};
use panproto_reference::migration::Migration;
use panproto_reference::model::{Node, SEdge, Tree};
use panproto_reference::rel::{self, RelFunctor};
use panproto_reference::schema::Schema;
use panproto_reference::value::{Presence, Value};

fn rel_of(s: &Schema) -> rel::RelSchema {
    rel::classify(
        s,
        &BTreeSet::from(["int".to_owned()]),
        &BTreeSet::from(["record".to_owned()]),
    )
    .unwrap()
}

fn functor(src: &Schema, tgt: &Schema, m: &Migration) -> RelFunctor {
    rel::entity_functor(&rel_of(src), &rel_of(tgt), m)
        .unwrap()
        .into_kan()
        .unwrap()
}

/// a(x), b(y) and an FK a -f-> b.
fn ab() -> Schema {
    Schema::default()
        .vertex("a", "record")
        .vertex("b", "record")
        .vertex("a.x", "int")
        .vertex("b.y", "int")
        .edge("a", "a.x", "prop", Some("x"))
        .edge("b", "b.y", "prop", Some("y"))
        .edge("a", "b", "fk", Some("f"))
}

fn identity(s: &Schema) -> Migration {
    Migration {
        surviving_verts: s.vertices.keys().cloned().collect(),
        surviving_edges: s.edges.clone(),
        ..Migration::default()
    }
}

fn leaf(id: u32, anchor: &str, v: i64) -> (u32, Node) {
    let mut n = Node::new(id, anchor);
    n.value = Some(Presence::Present(Value::Int(v)));
    (id, n)
}

/// Root 0 at `a` with its cell x = 1 (node 1), its child 4 at `b` by the FK,
/// with its cell y = 2 (node 5).
fn two_node_tree() -> Tree {
    Tree {
        nodes: [
            (0, Node::new(0, "a")),
            leaf(1, "a.x", 1),
            (4, Node::new(4, "b")),
            leaf(5, "b.y", 2),
        ]
        .into(),
        arcs: vec![
            (0, 1, SEdge::new("a", "a.x", "prop", Some("x"))),
            (0, 4, SEdge::new("a", "b", "fk", Some("f"))),
            (4, 5, SEdge::new("b", "b.y", "prop", Some("y"))),
        ],
        fans: Vec::new(),
        root: 0,
    }
}

/// A tree up to the ids of its value nodes (the graph decoding allocates
/// them afresh): one entry per arc, the parent's id and anchor, the arc's
/// label, and the child's anchor and leaf value (by its canonical bytes), with table-node children
/// named by id through the anchor of their own entry.
type Shape = BTreeSet<(u32, String, SEdge, String, Option<Vec<u8>>)>;

fn shape(t: &Tree) -> Shape {
    t.arcs
        .iter()
        .map(|(p, c, e)| {
            let child = &t.nodes[c];
            let leaf = match &child.value {
                Some(Presence::Present(v)) => Some(canonical::value_bytes(v).expect("encodable")),
                _ => None,
            };
            (
                *p,
                t.nodes[p].anchor.clone(),
                e.clone(),
                child.anchor.clone(),
                leaf,
            )
        })
        .collect()
}

#[test]
fn tree_extensions_along_the_identity_return_the_input() {
    // Delta, Sigma and Pi each keep every table node's id (value nodes are
    // decoded afresh, so trees are compared by their shape).
    let s = ab();
    let f = functor(&s, &s, &identity(&s));
    let t = two_node_tree();
    for out in [
        encode::t_delta(&f, &t),
        encode::t_sigma(&f, &t),
        encode::t_pi(&f, &t, usize::MAX),
    ] {
        let out = out.expect("a tree");
        assert_eq!(out.root, 0);
        assert_eq!(out.nodes.len(), t.nodes.len());
        assert_eq!(shape(&out), shape(&t));
    }
}

#[test]
fn tree_delta_refuses_a_merging_vertex_map() {
    // a(x), b(y) -> m(x, y): two kept tables share the image m.
    let source = ab();
    let target = Schema::default()
        .vertex("m", "record")
        .vertex("m.x", "int")
        .vertex("m.y", "int")
        .edge("m", "m.x", "prop", Some("x"))
        .edge("m", "m.y", "prop", Some("y"));
    let mig = Migration {
        vertex_remap: [("a", "m"), ("b", "m"), ("a.x", "m.x"), ("b.y", "m.y")]
            .iter()
            .map(|(a, b)| ((*a).to_owned(), (*b).to_owned()))
            .collect(),
        edge_remap: [
            (
                SEdge::new("a", "a.x", "prop", Some("x")),
                SEdge::new("m", "m.x", "prop", Some("x")),
            ),
            (
                SEdge::new("b", "b.y", "prop", Some("y")),
                SEdge::new("m", "m.y", "prop", Some("y")),
            ),
        ]
        .into(),
        ..Migration::default()
    };
    let f = functor(&source, &target, &mig);
    let t = Tree {
        nodes: [(0, Node::new(0, "m")), leaf(1, "m.x", 1), leaf(2, "m.y", 2)].into(),
        arcs: vec![
            (0, 1, SEdge::new("m", "m.x", "prop", Some("x"))),
            (0, 2, SEdge::new("m", "m.y", "prop", Some("y"))),
        ],
        fans: Vec::new(),
        root: 0,
    };
    assert_eq!(
        encode::t_delta(&f, &t),
        Err(TreeKanError::NonInjectiveVertexMap {
            first: "a".to_owned(),
            second: "b".to_owned(),
            target: "m".to_owned(),
        })
    );
}

#[test]
fn tree_delta_refuses_two_columns_merged_into_one() {
    // a(x, z) -> a(w): both columns map to w, a merge of value vertices
    // (checked first) and of attribute arrows.
    let source = Schema::default()
        .vertex("a", "record")
        .vertex("a.x", "int")
        .vertex("a.z", "int")
        .edge("a", "a.x", "prop", Some("x"))
        .edge("a", "a.z", "prop", Some("z"));
    let target = Schema::default()
        .vertex("a", "record")
        .vertex("a.w", "int")
        .edge("a", "a.w", "prop", Some("w"));
    let w = SEdge::new("a", "a.w", "prop", Some("w"));
    let mig = Migration {
        surviving_verts: ["a", "a.w"].iter().map(|v| (*v).to_owned()).collect(),
        surviving_edges: BTreeSet::from([w.clone()]),
        vertex_remap: BTreeMap::from([
            ("a.x".to_owned(), "a.w".to_owned()),
            ("a.z".to_owned(), "a.w".to_owned()),
        ]),
        edge_remap: BTreeMap::from([
            (SEdge::new("a", "a.x", "prop", Some("x")), w.clone()),
            (SEdge::new("a", "a.z", "prop", Some("z")), w.clone()),
        ]),
        ..Migration::default()
    };
    let f = functor(&source, &target, &mig);
    assert_eq!(f.columns.len(), 2);
    let t = Tree {
        nodes: [(0, Node::new(0, "a")), leaf(1, "a.w", 1)].into(),
        arcs: vec![(0, 1, w)],
        fans: Vec::new(),
        root: 0,
    };
    assert_eq!(
        encode::t_delta(&f, &t),
        Err(TreeKanError::NonInjectiveVertexMap {
            first: "a.x".to_owned(),
            second: "a.z".to_owned(),
            target: "a.w".to_owned(),
        })
    );
}

#[test]
fn tree_delta_along_an_injective_renaming_relabels_the_tree() {
    // a(x) -> a(w) by renaming the column: injective, so Delta pulls the
    // target tree back with the column renamed to x.
    let source = Schema::default()
        .vertex("a", "record")
        .vertex("a.x", "int")
        .edge("a", "a.x", "prop", Some("x"));
    let target = Schema::default()
        .vertex("a", "record")
        .vertex("a.w", "int")
        .edge("a", "a.w", "prop", Some("w"));
    let w = SEdge::new("a", "a.w", "prop", Some("w"));
    let x = SEdge::new("a", "a.x", "prop", Some("x"));
    let mig = Migration {
        surviving_verts: ["a", "a.w"].iter().map(|v| (*v).to_owned()).collect(),
        surviving_edges: BTreeSet::from([w.clone()]),
        vertex_remap: BTreeMap::from([("a.x".to_owned(), "a.w".to_owned())]),
        edge_remap: BTreeMap::from([(x.clone(), w.clone())]),
        ..Migration::default()
    };
    let f = functor(&source, &target, &mig);
    let t = Tree {
        nodes: [(0, Node::new(0, "a")), leaf(1, "a.w", 7)].into(),
        arcs: vec![(0, 1, w)],
        fans: Vec::new(),
        root: 0,
    };
    let pulled = encode::t_delta(&f, &t).unwrap();
    assert_eq!(
        shape(&pulled),
        BTreeSet::from([(
            0,
            "a".to_owned(),
            x,
            "a.x".to_owned(),
            canonical::value_bytes(&Value::Int(7))
        )])
    );
}

#[test]
fn tree_pi_along_the_identity_keeps_the_table_node_ids() {
    // Root 9 at `a`, child 2 at `b`: each fibre of the identity is one
    // table, so Pi keeps every table node's id and the root stays 9.
    let s = ab();
    let f = functor(&s, &s, &identity(&s));
    let t = Tree {
        nodes: [
            (9, Node::new(9, "a")),
            leaf(10, "a.x", 1),
            (2, Node::new(2, "b")),
            leaf(3, "b.y", 2),
        ]
        .into(),
        arcs: vec![
            (9, 10, SEdge::new("a", "a.x", "prop", Some("x"))),
            (9, 2, SEdge::new("a", "b", "fk", Some("f"))),
            (2, 3, SEdge::new("b", "b.y", "prop", Some("y"))),
        ],
        fans: Vec::new(),
        root: 9,
    };
    let pi = encode::t_pi(&f, &t, usize::MAX).expect("a tree");
    assert_eq!(pi.root, 9);
    assert_eq!(shape(&pi), shape(&t));
}

#[test]
fn tree_pi_refuses_a_noninjective_vertex_map_before_forming_products() {
    let source = Schema::default()
        .vertex("a", "record")
        .vertex("b", "record");
    let target = Schema::default().vertex("p", "record");
    let migration = Migration {
        vertex_remap: BTreeMap::from([
            ("a".to_owned(), "p".to_owned()),
            ("b".to_owned(), "p".to_owned()),
        ]),
        ..Migration::default()
    };
    let functor = functor(&source, &target, &migration);
    let tree = Tree {
        nodes: BTreeMap::from([(0, Node::new(0, "a"))]),
        arcs: Vec::new(),
        fans: Vec::new(),
        root: 0,
    };
    assert_eq!(
        encode::t_pi(&functor, &tree, 4),
        Err(TreeKanError::NonInjectiveVertexMap {
            first: "a".to_owned(),
            second: "b".to_owned(),
            target: "p".to_owned(),
        })
    );
}

#[test]
fn tree_pi_checks_vertices_below_the_column_root_for_injectivity() {
    // The table and the immediate column root map injectively.  The two value
    // vertices below that root do not: x,y -> z.  Tree Pi's domain is the
    // whole restricted value closure, so this collision is refused before a
    // graph product is formed.
    let source_root = SEdge::new("a", "r", "prop", Some("root"));
    let source_x = SEdge::new("r", "x", "inner", Some("left"));
    let source_y = SEdge::new("r", "y", "inner", Some("right"));
    let target_root = SEdge::new("a", "s", "prop", Some("root"));
    let target_x = SEdge::new("s", "z", "inner", Some("left"));
    let target_y = SEdge::new("s", "z", "inner", Some("right"));
    let source = Schema::default()
        .vertex("a", "record")
        .vertex("r", "int")
        .vertex("x", "int")
        .vertex("y", "int")
        .edge("a", "r", "prop", Some("root"))
        .edge("r", "x", "inner", Some("left"))
        .edge("r", "y", "inner", Some("right"));
    let target = Schema::default()
        .vertex("a", "record")
        .vertex("s", "int")
        .vertex("z", "int")
        .edge("a", "s", "prop", Some("root"))
        .edge("s", "z", "inner", Some("left"))
        .edge("s", "z", "inner", Some("right"));
    let migration = Migration {
        surviving_verts: BTreeSet::from(["a".to_owned()]),
        vertex_remap: BTreeMap::from([
            ("r".to_owned(), "s".to_owned()),
            ("x".to_owned(), "z".to_owned()),
            ("y".to_owned(), "z".to_owned()),
        ]),
        edge_remap: BTreeMap::from([
            (source_root, target_root),
            (source_x, target_x),
            (source_y, target_y),
        ]),
        ..Migration::default()
    };
    let functor = functor(&source, &target, &migration);
    let tree = Tree {
        nodes: BTreeMap::from([(0, Node::new(0, "a"))]),
        arcs: Vec::new(),
        fans: Vec::new(),
        root: 0,
    };
    assert_eq!(
        encode::t_pi(&functor, &tree, 4),
        Err(TreeKanError::NonInjectiveVertexMap {
            first: "x".to_owned(),
            second: "y".to_owned(),
            target: "z".to_owned(),
        })
    );
}

#[test]
fn tree_pi_forwards_the_product_bound_before_tree_readback() {
    let carried = SEdge::new("a", "a", "fk", Some("r"));
    let uncarried = SEdge::new("a", "a", "fk", Some("k"));
    let source = Schema::default().vertex("a", "record").edge(
        &carried.src,
        &carried.tgt,
        &carried.kind,
        carried.name.as_deref(),
    );
    let target = Schema::default()
        .vertex("a", "record")
        .edge(
            &carried.src,
            &carried.tgt,
            &carried.kind,
            carried.name.as_deref(),
        )
        .edge(
            &uncarried.src,
            &uncarried.tgt,
            &uncarried.kind,
            uncarried.name.as_deref(),
        );
    let migration = Migration {
        surviving_verts: BTreeSet::from(["a".to_owned()]),
        surviving_edges: BTreeSet::from([carried.clone()]),
        ..Migration::default()
    };
    let functor = functor(&source, &target, &migration);
    let tree = Tree {
        nodes: [
            (0, Node::new(0, "a")),
            (1, Node::new(1, "a")),
            (2, Node::new(2, "a")),
        ]
        .into(),
        arcs: vec![(0, 1, carried.clone()), (1, 2, carried)],
        fans: Vec::new(),
        root: 0,
    };
    assert_eq!(
        encode::t_pi(&functor, &tree, 4),
        Err(TreeKanError::Graph(encode::GraphKanError::Adjunction(
            Box::new(rel::AdjunctionError::ProductSizeExceeded {
                vertex: "a".to_owned(),
                actual: 9,
                limit: 4,
            }),
        )))
    );
}

#[test]
fn tree_delta_refuses_two_relations_merged_into_one() {
    // a -f-> b and a -g-> b both map to a -h-> b: the tables map injectively
    // but the edge map does not.
    let f_edge = SEdge::new("a", "b", "fk", Some("f"));
    let g_edge = SEdge::new("a", "b", "fk", Some("g"));
    let h_edge = SEdge::new("a", "b", "fk", Some("h"));
    let source = Schema::default()
        .vertex("a", "record")
        .vertex("b", "record")
        .edge("a", "b", "fk", Some("f"))
        .edge("a", "b", "fk", Some("g"));
    let target = Schema::default()
        .vertex("a", "record")
        .vertex("b", "record")
        .edge("a", "b", "fk", Some("h"));
    let mig = Migration {
        surviving_verts: ["a", "b"].iter().map(|v| (*v).to_owned()).collect(),
        surviving_edges: BTreeSet::from([h_edge.clone()]),
        edge_remap: BTreeMap::from([(f_edge, h_edge.clone()), (g_edge, h_edge.clone())]),
        ..Migration::default()
    };
    let f = functor(&source, &target, &mig);
    assert_eq!(f.relations.len(), 2);
    let t = Tree {
        nodes: [(0, Node::new(0, "a")), (1, Node::new(1, "b"))].into(),
        arcs: vec![(0, 1, h_edge.clone())],
        fans: Vec::new(),
        root: 0,
    };
    assert_eq!(
        encode::t_delta(&f, &t),
        Err(TreeKanError::NonInjectiveEdgeMap {
            target: Box::new(h_edge),
        })
    );
}
