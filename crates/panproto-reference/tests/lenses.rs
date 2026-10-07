#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Worked examples of graph restrict and of the graph lens.

use panproto_reference::lens::{GraphLens, PutDomainError, ViewRef};
use panproto_reference::migration::Migration;
use panproto_reference::model::{GEdge, Graph, Node, SEdge};
use panproto_reference::restrict::{self, NoValueLayer, SynthKey};
use panproto_reference::schema::Schema;

fn wrapper() -> (Schema, Schema, Migration, Graph) {
    let source = Schema::default()
        .vertex("r", "obj")
        .vertex("m", "obj")
        .vertex("leaf", "str")
        .edge("r", "m", "prop", Some("m"))
        .edge("m", "m", "prop", Some("m"))
        .edge("m", "leaf", "prop", Some("leaf"));
    let target = Schema::default()
        .vertex("r", "obj")
        .vertex("leaf", "str")
        .edge("r", "leaf", "prop", Some("leaf"));
    let mig = Migration {
        surviving_verts: ["r".to_owned(), "leaf".to_owned()].into(),
        resolver: [(
            ("r".to_owned(), "leaf".to_owned()),
            SEdge::new("r", "leaf", "prop", Some("leaf")),
        )]
        .into(),
        ..Migration::default()
    };
    let mut graph = Graph::default();
    for (id, a) in [(0, "r"), (1, "m"), (2, "m"), (3, "m"), (4, "leaf")] {
        graph.nodes.insert(id, Node::new(id, a));
    }
    let labels = [
        ("r", "m", "m"),
        ("m", "m", "m"),
        ("m", "m", "m"),
        ("m", "leaf", "leaf"),
    ];
    for (idx, (a, b, n)) in labels.iter().enumerate() {
        let idx = u32::try_from(idx).unwrap();
        graph.edges.insert(
            idx,
            GEdge {
                src: idx,
                tgt: idx + 1,
                label: SEdge::new(a, b, "prop", Some(n)),
                position: None,
            },
        );
    }
    (source, target, mig, graph)
}

#[test]
fn a_recursive_dropped_wrapper_contracts_to_one_edge_and_put_restores_it() {
    let (source, target, mig, graph) = wrapper();
    let plan = restrict::compile(&source, &target, &mig).unwrap();
    assert_eq!(plan.mode(), restrict::ContractionMode::Closure);
    let (view, c) =
        restrict::get_with_plan(&source, &target, &mig, &plan, &NoValueLayer, &graph).unwrap();
    assert_eq!(view.nodes.keys().copied().collect::<Vec<_>>(), vec![0, 4]);
    assert_eq!(view.edges.len(), 1);
    let (id, e) = view.edges.iter().next().unwrap();
    assert_eq!((*id, e.src, e.tgt), (4, 0, 4));
    assert!(matches!(
        c.synthesized_edges[&4],
        SynthKey::Contract { src: 0, tgt: 4, .. }
    ));
    let lens = GraphLens {
        src: &source,
        tgt: &target,
        m: &mig,
        plan: &plan,
        vl: &NoValueLayer,
    };
    let back = lens.put(&view, &c).unwrap();
    assert_eq!(back, graph);
    // Removing the synthesized edge alone is an edit of a synthesized object.
    let mut edited = view.clone();
    edited.edges.clear();
    assert_eq!(
        lens.put(&edited, &c),
        Err(PutDomainError::SynthesizedEdited {
            elem: ViewRef::Edge(4)
        })
    );
    // Removing the leaf explains the missing edge; put cascades nothing
    // that the view did not remove.
    let mut no_leaf = view.clone();
    no_leaf.edges.clear();
    no_leaf.nodes.remove(&4);
    let restored = lens.put(&no_leaf, &c).unwrap();
    assert!(!restored.nodes.contains_key(&4));
    assert!(!restored.edges.contains_key(&3));
    // A view edge between r and leaf that is no synthesized edge is a new
    // contracted pair.
    let mut inserted = view.clone();
    inserted.edges.insert(
        9,
        GEdge {
            src: 0,
            tgt: 4,
            label: SEdge::new("r", "leaf", "prop", Some("leaf")),
            position: None,
        },
    );
    let errs = lens.check_put_domain(&inserted, &c).unwrap_err();
    assert!(errs.contains(&PutDomainError::ContractedEdgeInserted {
        elem: ViewRef::Edge(9)
    }));
}

#[test]
fn the_identity_lens_records_nothing_and_put_get_holds_on_edits() {
    let (s, _, _, g) = wrapper();
    let id = Migration {
        surviving_verts: s.vertices.keys().cloned().collect(),
        surviving_edges: s.edges.clone(),
        ..Migration::default()
    };
    let plan = restrict::compile(&s, &s, &id).unwrap();
    let (view, c) = restrict::get_with_plan(&s, &s, &id, &plan, &NoValueLayer, &g).unwrap();
    assert_eq!(view, g);
    assert!(
        c.dropped_nodes.is_empty() && c.original_edges.is_empty() && c.original_anchors.is_empty()
    );
    let lens = GraphLens {
        src: &s,
        tgt: &s,
        m: &id,
        plan: &plan,
        vl: &NoValueLayer,
    };
    let mut v2 = view;
    v2.nodes
        .get_mut(&4)
        .unwrap()
        .fields
        .insert("x".into(), panproto_reference::value::Value::Int(1));
    v2.edges.remove(&3);
    v2.nodes.remove(&4);
    let s2 = lens.put(&v2, &c).unwrap();
    let (again, _) = restrict::get_with_plan(&s, &s, &id, &plan, &NoValueLayer, &s2).unwrap();
    assert_eq!(again, v2);
}

#[test]
fn a_view_node_reusing_a_dropped_id_collides() {
    let (source, target, mig, graph) = wrapper();
    let plan = restrict::compile(&source, &target, &mig).unwrap();
    let (mut view, c) =
        restrict::get_with_plan(&source, &target, &mig, &plan, &NoValueLayer, &graph).unwrap();
    view.nodes.insert(2, Node::new(2, "leaf"));
    let lens = GraphLens {
        src: &source,
        tgt: &target,
        m: &mig,
        plan: &plan,
        vl: &NoValueLayer,
    };
    assert_eq!(
        lens.put(&view, &c),
        Err(PutDomainError::IdCollision {
            elem: ViewRef::Node(2)
        })
    );
}
