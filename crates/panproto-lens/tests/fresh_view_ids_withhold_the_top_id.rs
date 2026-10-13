//! A fresh view id never takes `u32::MAX`.
//!
//! At every recorded floor in the last four ids, the fresh node, edge and
//! row id of an empty view is the floor itself below `u32::MAX`, and the
//! typed exhaustion error of its kind at `u32::MAX`; a view id at or above
//! `u32::MAX - 1` leaves no fresh id.

use panproto_inst::{
    EdgeId, FInstance, GInstance, GraphComplement, IdFloor, InstError, Node, RowId, TableComplement,
};
use panproto_lens::graph_lens::{fresh_view_edge_id, fresh_view_node_id};
use panproto_lens::table_lens::fresh_view_row_id;
use panproto_schema::Edge;

const TOP: u32 = u32::MAX;

const fn graph_complement(floor: u32) -> GraphComplement {
    let mut complement = GraphComplement::empty();
    complement.id_floor = IdFloor {
        node: floor,
        edge: floor,
    };
    complement
}

fn table_complement(floor: u32) -> TableComplement {
    TableComplement {
        id_floor: floor,
        ..TableComplement::default()
    }
}

#[test]
fn fresh_view_ids_are_exhaustive_at_the_top() {
    for floor in (TOP - 3)..=TOP {
        let below = (floor < TOP).then_some(floor);
        let graph = graph_complement(floor);
        let view = GInstance::new();
        let node = fresh_view_node_id(&view, &graph);
        assert_eq!(node.as_ref().ok().copied(), below, "node at floor {floor}");
        if below.is_none() {
            assert!(matches!(node, Err(InstError::NodeIdsExhausted)));
        }
        let edge = fresh_view_edge_id(&view, &graph);
        assert_eq!(edge.as_ref().ok().copied(), below.map(EdgeId));
        if below.is_none() {
            assert!(matches!(edge, Err(InstError::EdgeIdsExhausted)));
        }
        let row = fresh_view_row_id(&FInstance::new(), &table_complement(floor));
        assert_eq!(row.as_ref().ok().copied(), below.map(RowId));
        if below.is_none() {
            assert!(matches!(row, Err(InstError::NodeIdsExhausted)));
        }
    }
}

#[test]
fn a_view_id_next_to_the_top_leaves_no_fresh_id() {
    let label = Edge {
        src: "a".into(),
        tgt: "a".into(),
        kind: "x".into(),
        name: None,
    };
    for taken in [TOP - 1, TOP] {
        let view = GInstance::new()
            .with_node(Node::new(taken, "a"))
            .with_edge_id(EdgeId(taken), taken, taken, label.clone());
        let graph = graph_complement(0);
        assert!(matches!(
            fresh_view_node_id(&view, &graph),
            Err(InstError::NodeIdsExhausted)
        ));
        assert!(matches!(
            fresh_view_edge_id(&view, &graph),
            Err(InstError::EdgeIdsExhausted)
        ));
    }
    let view = GInstance::new().with_node(Node::new(TOP - 2, "a"));
    assert_eq!(
        fresh_view_node_id(&view, &graph_complement(0)).ok(),
        Some(TOP - 1)
    );
}
