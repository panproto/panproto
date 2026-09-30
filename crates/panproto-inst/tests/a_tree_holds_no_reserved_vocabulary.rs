//! Reserved fan vocabulary is invalid in a tree.
//!
//! A tree carries hyper-edges as `Fan` records; its graph form reifies each
//! as a node of shape `HyperEdge` with edges of the reserved kinds
//! `hyper_parent` and `hyper_label`. A tree that held such a node or arc
//! itself would be ambiguous in that form, so `WInstance::try_new` refuses
//! it, and every tree it accepts embeds in its graph form and reads back.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;

use panproto_gat::Name;
use panproto_inst::{
    Fan, GInstance, HYPER_LABEL_KIND, HYPER_PARENT_KIND, InstError, Node, NodeShape, ViewRef,
    WInstance, wtype_check,
};
use panproto_schema::{Edge, Schema};
use proptest::prelude::*;

fn edge(src: &str, tgt: &str, kind: &str) -> Edge {
    Edge {
        src: src.into(),
        tgt: tgt.into(),
        kind: kind.into(),
        name: Some("x".into()),
    }
}

fn two_nodes() -> HashMap<u32, Node> {
    HashMap::from([(0, Node::new(0, "doc")), (1, Node::new(1, "p"))])
}

#[test]
fn a_non_fan_hyperedge_node_is_rejected_in_a_tree() {
    let mut nodes = two_nodes();
    nodes.insert(1, Node::new(1, "p").with_shape(NodeShape::HyperEdge));
    let refused = WInstance::try_new(
        nodes,
        vec![(0, 1, edge("doc", "p", "prop"))],
        vec![],
        0,
        "doc".into(),
    );
    assert!(
        matches!(
            refused,
            Err(InstError::ReservedVocabulary {
                at: ViewRef::Node(1)
            })
        ),
        "{refused:?}"
    );
}

#[test]
fn a_hyper_parent_arc_is_rejected_in_a_tree() {
    let refused = WInstance::try_new(
        two_nodes(),
        vec![(0, 1, edge("doc", "p", HYPER_PARENT_KIND))],
        vec![],
        0,
        "doc".into(),
    );
    assert!(
        matches!(
            refused,
            Err(InstError::ReservedVocabulary {
                at: ViewRef::Arc {
                    parent: 0,
                    child: 1
                }
            })
        ),
        "{refused:?}"
    );
}

#[test]
fn a_hyper_label_arc_is_rejected_in_a_tree() {
    let refused = WInstance::try_new(
        two_nodes(),
        vec![(0, 1, edge("doc", "p", HYPER_LABEL_KIND))],
        vec![],
        0,
        "doc".into(),
    );
    assert!(
        matches!(
            refused,
            Err(InstError::ReservedVocabulary {
                at: ViewRef::Arc {
                    parent: 0,
                    child: 1
                }
            })
        ),
        "{refused:?}"
    );
}

#[test]
fn a_tree_with_fans_and_no_reserved_vocabulary_is_accepted() {
    let tree = WInstance::try_new(
        two_nodes(),
        vec![(0, 1, edge("doc", "p", "prop"))],
        vec![Fan::new("rel", 0).with_child("arg", 1)],
        0,
        "doc".into(),
    )
    .expect("a valid tree is accepted");
    assert!(wtype_check::reserved_vocabulary(&tree).is_empty());
}

#[test]
fn the_validating_constructor_rejects_a_malformed_tree() {
    assert!(matches!(
        WInstance::try_new(two_nodes(), vec![], vec![], 7, "doc".into()),
        Err(InstError::MissingRoot)
    ));
    assert!(matches!(
        WInstance::try_new(
            two_nodes(),
            vec![(0, 9, edge("doc", "p", "prop"))],
            vec![],
            0,
            "doc".into()
        ),
        Err(InstError::DanglingArc { src: 0, tgt: 9 })
    ));
    let mut misfiled = two_nodes();
    misfiled.insert(1, Node::new(2, "p"));
    assert!(matches!(
        WInstance::try_new(misfiled, vec![], vec![], 0, "doc".into()),
        Err(InstError::NodeIdMismatch { key: 1, node_id: 2 })
    ));
}

/// Several violations are all listed, nodes before arcs.
#[test]
fn every_reserved_element_is_listed_in_order() {
    let nodes = HashMap::from([
        (0, Node::new(0, "doc")),
        (1, Node::new(1, "p")),
        (2, Node::new(2, "p").with_shape(NodeShape::HyperEdge)),
    ]);
    let tree = WInstance::new(
        nodes,
        vec![
            (0, 1, edge("doc", "p", HYPER_LABEL_KIND)),
            (0, 2, edge("doc", "p", "prop")),
        ],
        vec![],
        0,
        "doc".into(),
    );
    assert_eq!(
        wtype_check::reserved_vocabulary(&tree),
        vec![
            ViewRef::Node(2),
            ViewRef::Arc {
                parent: 0,
                child: 1
            }
        ]
    );
}

fn entry_schema(root: &Name) -> Schema {
    Schema {
        entries: vec![root.clone()],
        ..Schema::default()
    }
}

/// A tree over up to eight nodes whose node shapes and arc kinds are drawn
/// from a pool that includes the reserved fan vocabulary, with fans over its
/// nodes.
fn arb_candidate() -> impl Strategy<Value = (Vec<Node>, Vec<(u32, u32, Edge)>, Vec<Fan>)> {
    (1usize..8)
        .prop_flat_map(|n| {
            let shapes = prop::collection::vec(
                prop::sample::select(vec![
                    NodeShape::Plain,
                    NodeShape::List,
                    NodeShape::HyperEdge,
                ]),
                n,
            );
            let parents: Vec<_> = (1..n).map(|i| 0..i).collect();
            let kinds = prop::collection::vec(
                prop::sample::select(vec!["prop", "item", HYPER_PARENT_KIND, HYPER_LABEL_KIND]),
                n,
            );
            let fans = prop::collection::vec(
                (0..n, prop::collection::hash_map("[a-c]", 0..n, 0..3)),
                0..3,
            );
            (Just(n), shapes, parents, kinds, fans)
        })
        .prop_map(|(n, shapes, parents, kinds, fans)| {
            let ids: Vec<u32> = (0..n).map(|i| u32::try_from(i * 3).unwrap()).collect();
            let nodes: Vec<Node> = ids
                .iter()
                .zip(shapes)
                .map(|(&id, shape)| Node::new(id, "v").with_shape(shape))
                .collect();
            let arcs = parents
                .iter()
                .zip(1usize..)
                .map(|(&p, c)| (ids[p], ids[c], edge("v", "v", kinds[c])))
                .collect();
            let fans = fans
                .into_iter()
                .map(|(parent, children)| Fan {
                    hyper_edge_id: "rel".into(),
                    parent: ids[parent],
                    children: children.into_iter().map(|(l, c)| (l, ids[c])).collect(),
                })
                .collect();
            (nodes, arcs, fans)
        })
}

proptest! {
    /// Every tree the validating constructor accepts embeds in its graph
    /// form and reads back unchanged; every tree whose graph form would hold
    /// reserved vocabulary outside a fan is refused by the constructor.
    #[test]
    fn every_tree_the_validating_constructor_accepts_embeds_and_round_trips(
        (nodes, arcs, fans) in arb_candidate()
    ) {
        let nodes: HashMap<u32, Node> = nodes.into_iter().map(|n| (n.id, n)).collect();
        let raw = WInstance::new(nodes.clone(), arcs.clone(), fans.clone(), 0, "v".into());
        match WInstance::try_new(nodes, arcs, fans, 0, "v".into()) {
            Ok(tree) => {
                let (graph, embedding) = GInstance::from_wtype_embedded(&tree, None)
                    .expect("a valid tree always embeds");
                let (back, read) = graph
                    .to_wtype_embedded(&entry_schema(&tree.schema_root))
                    .expect("the graph form is a tree");
                prop_assert_eq!(
                    rmp_serde::to_vec(&back).unwrap(),
                    rmp_serde::to_vec(&tree).unwrap()
                );
                prop_assert_eq!(read, embedding);
            }
            Err(InstError::ReservedVocabulary { at }) => {
                prop_assert!(wtype_check::reserved_vocabulary(&raw).contains(&at));
            }
            Err(other) => prop_assert!(false, "unexpected refusal {other:?}"),
        }
    }
}
