//! The tree complement records the source's node id floor and the keys of
//! the nodes `get` synthesized, and counts as residue only what the view
//! lacks.
//!
//! The floor is the floor of the tree's graph form: one above every tree
//! node id and every fan node id its embedding allocates. A fresh view id is
//! drawn at or above it, so a node the view adds never takes the id of a
//! source node the view deleted.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::{BTreeMap, HashMap};

use panproto_inst::complement::{fresh_view_node_id, tree_id_floor};
use panproto_inst::{Complement, EdgeId, Fan, GInstance, Node, SynthKey, WInstance};
use panproto_schema::Edge;
use proptest::prelude::*;

fn prop(src: &str, tgt: &str) -> Edge {
    Edge {
        src: src.into(),
        tgt: tgt.into(),
        kind: "prop".into(),
        name: Some(tgt.into()),
    }
}

/// A tree over up to eight nodes with spread-out ids and up to three fans.
fn arb_tree() -> impl Strategy<Value = WInstance> {
    (1usize..8)
        .prop_flat_map(|n| {
            let ids = Just((0..60u32).collect::<Vec<_>>())
                .prop_shuffle()
                .prop_map(move |ids| {
                    let mut ids = ids[..n].to_vec();
                    ids.sort_unstable();
                    ids
                });
            let parents: Vec<_> = (1..n).map(|i| 0..i).collect();
            let fans = prop::collection::vec(
                (0..n, prop::collection::hash_map("[a-c]", 0..n, 0..3)),
                0..4,
            );
            (ids, parents, fans)
        })
        .prop_map(|(ids, parents, fans)| {
            let nodes: HashMap<u32, Node> =
                ids.iter().map(|&id| (id, Node::new(id, "v"))).collect();
            let arcs = parents
                .iter()
                .zip(1usize..)
                .map(|(&p, c)| (ids[p], ids[c], prop("v", "v")))
                .collect();
            let fans = fans
                .into_iter()
                .map(|(parent, children)| Fan {
                    hyper_edge_id: "rel".into(),
                    parent: ids[parent],
                    children: children.into_iter().map(|(l, c)| (l, ids[c])).collect(),
                })
                .collect();
            WInstance::new(nodes, arcs, fans, ids[0], "v".into())
        })
}

proptest! {
    /// The tree floor is the embedding's `next_node` and the graph's own
    /// floor, one above its largest node id.
    #[test]
    fn the_tree_floor_equals_the_embedded_graph_floor(tree in arb_tree()) {
        let floor = tree_id_floor(&tree).unwrap();
        let (graph, embedding) = GInstance::from_wtype_embedded(&tree, None).unwrap();
        prop_assert_eq!(floor, embedding.next_node);
        prop_assert_eq!(floor, GInstance::from_wtype(&tree).unwrap().max_node_id().map_or(0, |m| m + 1));
        prop_assert_eq!(floor, graph.max_node_id().map_or(0, |m| m + 1));
    }
}

#[test]
fn an_empty_tree_has_floor_zero() {
    let tree = WInstance::new(HashMap::new(), vec![], vec![], 0, "v".into());
    assert_eq!(tree_id_floor(&tree).unwrap(), 0);
}

/// `id_floor` then `synthesized_keys` follow `original_anchors`, and
/// `identity` is last and omitted when absent.
#[test]
fn tree_complement_fields_serialize_in_contract_order() {
    let mut complement = Complement::empty();
    complement.original_anchors.insert(1, "a".into());
    complement.id_floor = 9;
    complement.synthesized_keys.insert(
        7,
        SynthKey::Expand {
            source_edge: EdgeId(2),
            hop: 0,
        },
    );
    let json = serde_json::to_string(&complement).unwrap();
    let at = |key: &str| json.find(&format!("\"{key}\":")).unwrap();
    assert!(at("original_anchors") < at("id_floor"));
    assert!(at("id_floor") < at("synthesized_keys"));
    assert!(
        json.ends_with(r#""synthesized_keys":{"7":{"expand":{"source_edge":2,"hop":0}}}}"#),
        "{json}"
    );
    assert!(!json.contains("\"identity\""));

    // Positional MessagePack: the two fields sit in their slots, and the
    // complement reads back.
    let bytes = rmp_serde::to_vec(&complement).unwrap();
    let back: Complement = rmp_serde::from_slice(&bytes).unwrap();
    assert_eq!(back.id_floor, 9);
    assert_eq!(back.synthesized_keys, complement.synthesized_keys);
    let positional: Vec<serde::de::IgnoredAny> = rmp_serde::from_slice(&bytes).unwrap();
    assert_eq!(positional.len(), 15);
}

/// A complement written before the two fields existed reads with floor 0
/// and no keys.
#[test]
fn a_complement_without_the_new_fields_reads_back() {
    let json = serde_json::json!({
        "dropped_nodes": {}, "dropped_arcs": [], "dropped_fans": [],
        "contraction_choices": [], "original_parent": {},
    });
    let back: Complement = serde_json::from_value(json).unwrap();
    assert_eq!(back.id_floor, 0);
    assert!(back.synthesized_keys.is_empty());
}

/// The residue counts neither identity nor provenance nor the derived
/// fields.
#[test]
fn tree_residue_ignores_bookkeeping_fields() {
    let mut complement = Complement::empty();
    complement.source_fingerprint = 42;
    complement.id_floor = 10;
    complement.synthesized_keys.insert(
        3,
        SynthKey::ContractExpand {
            src: 0,
            tgt: 1,
            hop: 0,
        },
    );
    complement
        .contraction_choices
        .insert((0, 1), prop("r", "a"));
    complement.contracted_into.insert(2, 0);
    complement.original_parent.insert(1, 0);
    complement.arc_order.push((0, 1));
    assert!(complement.residue_is_trivial());
    complement.arc_edges.insert((0, 1), prop("r", "a"));
    assert!(!complement.residue_is_trivial());
}

/// With no recorded floor, a fresh id avoids every id the complement and
/// the view hold.
#[test]
fn a_legacy_tree_complement_derives_its_floor() {
    let view = WInstance::new(
        HashMap::from([(0, Node::new(0, "r")), (1, Node::new(1, "a"))]),
        vec![(0, 1, prop("r", "a"))],
        vec![],
        0,
        "r".into(),
    );
    let mut complement = Complement::empty();
    complement.original_parent.insert(1, 0);
    complement.original_parent.insert(5, 0);
    assert_eq!(complement.id_floor, 0);
    assert_eq!(fresh_view_node_id(&view, &complement).unwrap(), 6);
    complement.dropped_nodes.insert(8, Node::new(8, "gone"));
    assert_eq!(fresh_view_node_id(&view, &complement).unwrap(), 9);
    complement.id_floor = 20;
    assert_eq!(fresh_view_node_id(&view, &complement).unwrap(), 20);
}

/// The synthesized index names each synthesized node by its key and each
/// synthesized arc by the graph edge of the view's embedding.
#[test]
fn tree_complement_synth_index_keys_arcs_through_the_embedding() {
    // Source arc 0 is (0, 1); `get` expanded it as 0 -> 5 -> 6 -> 1, and
    // contracted the pair (0, 2) through a dropped node.
    let view = WInstance::new(
        HashMap::from([
            (0, Node::new(0, "r")),
            (1, Node::new(1, "a")),
            (2, Node::new(2, "b")),
            (5, Node::new(5, "i")),
            (6, Node::new(6, "j")),
        ]),
        vec![
            (0, 5, prop("r", "i")),
            (5, 6, prop("i", "j")),
            (6, 1, prop("j", "a")),
            (0, 2, prop("r", "b")),
        ],
        vec![],
        0,
        "r".into(),
    );
    let (_, embedding) = GInstance::from_wtype_embedded(&view, None).unwrap();
    let mut complement = Complement::empty();
    complement.arc_order = vec![(0, 1), (0, 3), (3, 2)];
    let expand = |hop| SynthKey::Expand {
        source_edge: EdgeId(0),
        hop,
    };
    complement.synthesized_keys = BTreeMap::from([(5, expand(0)), (6, expand(1))]);
    complement
        .contraction_choices
        .insert((0, 2), prop("r", "b"));
    let index = complement.synth_index(&embedding);
    assert_eq!(index.nodes, complement.synthesized_keys);
    let contract = SynthKey::Contract {
        src: 0,
        tgt: 2,
        edge: prop("r", "b"),
    };
    assert_eq!(
        index.edges,
        BTreeMap::from([
            (EdgeId(0), expand(0)),
            (EdgeId(1), expand(1)),
            (EdgeId(2), expand(2)),
            (EdgeId(3), contract),
        ])
    );
}
