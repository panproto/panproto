#![allow(clippy::unwrap_used, clippy::expect_used)]
//! An identity lens over an instance with fans leaves trivial residue in
//! the tree and in the graph: every view fan edge maps back to its own
//! source label (the parent edge's through the parent label its
//! hyper-edge declares), so the graph records no fan edge, as the tree
//! records nothing.

use std::collections::{BTreeMap, HashMap};

use panproto_gat::Name;
use panproto_inst::{
    CompiledMigration, Fan, GInstance, Node, RestrictPlan, WInstance, graph_restrict_with_plan,
};
use panproto_lens::Lens;
use panproto_schema::{Edge, Protocol, Schema, SchemaBuilder};

/// `r -c-> c` with the hyper-edge `h`, whose parent label `p` is on `r`
/// and whose child labels `x` and `y` are on `c`.
fn fanned_schema() -> Schema {
    let protocol = Protocol::default();
    SchemaBuilder::new(&protocol)
        .vertex("r", "record", None)
        .unwrap()
        .vertex("c", "record", None)
        .unwrap()
        .edge("r", "c", "prop", Some("c"))
        .unwrap()
        .hyper_edge(
            "h",
            "rel",
            HashMap::from([
                ("p".to_owned(), "r".to_owned()),
                ("x".to_owned(), "c".to_owned()),
                ("y".to_owned(), "c".to_owned()),
            ]),
            "p",
        )
        .unwrap()
        .build()
        .unwrap()
}

/// `r(0)` with two `c` children, 1 and 2, and the fan of `h` from `r` to
/// them under `x` and `y`.
fn fanned_tree() -> WInstance {
    let arc = Edge {
        src: "r".into(),
        tgt: "c".into(),
        kind: "prop".into(),
        name: Some("c".into()),
    };
    WInstance::new(
        [
            (0, Node::new(0, "r")),
            (1, Node::new(1, "c")),
            (2, Node::new(2, "c")),
        ]
        .into(),
        vec![(0, 1, arc.clone()), (0, 2, arc)],
        vec![Fan {
            hyper_edge_id: "h".into(),
            parent: 0,
            children: HashMap::from([("x".to_owned(), 1), ("y".to_owned(), 2)]),
        }],
        0,
        "r".into(),
    )
}

#[test]
fn an_identity_lens_over_fans_records_no_fan_edge() {
    let s = fanned_schema();
    let lens = Lens {
        compiled: CompiledMigration {
            surviving_verts: s.vertices.keys().cloned().collect(),
            surviving_edges: s.edges.keys().cloned().collect(),
            ..CompiledMigration::default()
        },
        src_schema: s.clone(),
        tgt_schema: s.clone(),
    };
    let tree = fanned_tree();
    let (view, complement) = panproto_lens::get(&lens, &tree).unwrap();
    assert_eq!(view.fans, tree.fans);
    assert!(complement.residue_is_trivial(), "{complement:?}");

    // The graph form, valid against the schema: its parent edge is named
    // by the parent label the schema declares.
    let plan = RestrictPlan::compile(&s, &s, &lens.compiled).unwrap();
    let g = GInstance::from_wtype_with_schema(&tree, &s).unwrap();
    let fan_edges: Vec<_> = g
        .edges_sorted()
        .filter(|(_, edge)| edge.edge.kind.starts_with("hyper_"))
        .map(|(id, edge)| (id, edge.clone()))
        .collect();
    assert_eq!(fan_edges.len(), 3, "{fan_edges:?}");
    let (graph_view, graph_complement) = graph_restrict_with_plan(&g, &plan).unwrap();
    for (id, source) in &fan_edges {
        assert_eq!(graph_view.edge(*id), Some(source));
    }
    assert_eq!(graph_complement.original_edges, BTreeMap::new());
    assert_eq!(
        graph_complement.original_anchors,
        BTreeMap::<u32, Name>::new()
    );
    assert!(
        graph_complement.residue_is_trivial(),
        "{graph_complement:?}"
    );
}
