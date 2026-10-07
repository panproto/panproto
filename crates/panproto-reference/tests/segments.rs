#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Partitioning then gluing returns the instance for every partition of a
//! small graph, and a tampered ghost is caught.

use panproto_reference::model::{GEdge, Graph, Node, SEdge};
use panproto_reference::segment::{self, IdRange, Partition, SegmentError, SegmentSpec};

fn spec(id: u32, lo: u32, hi: u32) -> SegmentSpec {
    SegmentSpec {
        id,
        nodes: vec![IdRange { lo, hi }],
        alloc_nodes: None,
        alloc_edges: None,
    }
}

#[test]
fn a_dangling_edge_endpoint_is_a_typed_refusal_of_the_partition() {
    // Edge 0 runs from node 0 to node 3, which the graph does not hold;
    // node 3 lies in segment 1's range, so the edge would cross segments.
    let mut g = Graph::default();
    g.nodes.insert(0, Node::new(0, "a"));
    g.edges.insert(
        0,
        GEdge {
            src: 0,
            tgt: 3,
            label: SEdge::new("a", "b", "e", None),
            position: None,
        },
    );
    let p = Partition {
        specs: vec![spec(0, 0, 1), spec(1, 2, u32::MAX)],
    };
    let refused = segment::partition(&g, &p);
    assert_eq!(format!("{refused:?}"), "Err(DanglingEdge { edge: 0 })");
}

#[test]
fn every_two_way_split_glues_back() {
    let mut g = Graph::default();
    for i in 0..4 {
        g.nodes
            .insert(i, Node::new(i, if i % 2 == 0 { "a" } else { "b" }));
    }
    let l = SEdge::new("a", "b", "e", None);
    for (id, (s, t)) in [(0, 1), (1, 0), (2, 3), (0, 3), (0, 3)].iter().enumerate() {
        g.edges.insert(
            u32::try_from(id).unwrap(),
            GEdge {
                src: *s,
                tgt: *t,
                label: l.clone(),
                position: None,
            },
        );
    }
    for cut in 0..4 {
        let p = Partition {
            specs: vec![spec(0, 0, cut), spec(1, cut + 1, u32::MAX)],
        };
        let segs = segment::partition(&g, &p).unwrap();
        assert_eq!(segment::glue(&segs, &p), Ok(g.clone()));
    }
    let p = Partition {
        specs: vec![spec(0, 0, 1), spec(1, 2, u32::MAX)],
    };
    let mut segs = segment::partition(&g, &p).unwrap();
    segs[0].ghosts.get_mut(&3).unwrap().anchor = "x".into();
    assert_eq!(
        segment::glue(&segs, &p),
        Err(SegmentError::GhostMismatch { node: 3 })
    );
    let bad = Partition {
        specs: vec![spec(0, 0, 2), spec(1, 2, 5)],
    };
    assert_eq!(
        bad.validate(),
        Err(SegmentError::OverlappingRanges { a: 0, b: 1 })
    );
    let r = IdRange { lo: 10, hi: 12 };
    assert_eq!(segment::fresh(r, &[11].into(), &[].into()), Some(12));
    assert_eq!(segment::fresh(r, &[12].into(), &[].into()), None);
    assert_eq!(
        segment::fresh(
            IdRange {
                lo: 0,
                hi: u32::MAX
            },
            &[u32::MAX - 1].into(),
            &[].into()
        ),
        None
    );
}
