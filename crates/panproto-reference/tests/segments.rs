#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Partitioning then gluing returns the instance for every partition of a
//! small graph, and a tampered ghost is caught.

use panproto_reference::model::{GEdge, Graph, Node, SEdge};
use panproto_reference::segment::{
    self, IdRange, Partition, PartitionDefect, SegmentError, SegmentSpec,
};

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
fn malformed_ranges_and_node_allocation_are_typed_refusals() {
    let partition = |nodes, alloc_nodes| Partition {
        specs: vec![SegmentSpec {
            id: 0,
            nodes,
            alloc_nodes,
            alloc_edges: None,
        }],
    };
    assert_eq!(
        partition(
            vec![IdRange { lo: 5, hi: 9 }, IdRange { lo: 0, hi: 4 }],
            None,
        )
        .validate(),
        Err(SegmentError::MalformedPartition {
            segment: 0,
            defect: PartitionDefect::UnsortedRanges,
        })
    );
    assert_eq!(
        partition(
            vec![IdRange { lo: 0, hi: 5 }, IdRange { lo: 5, hi: 9 }],
            None,
        )
        .validate(),
        Err(SegmentError::OverlappingRanges { a: 0, b: 0 })
    );
    for allocation in [IdRange { lo: 10, hi: 12 }, IdRange { lo: 3, hi: 6 }] {
        assert_eq!(
            partition(
                vec![IdRange { lo: 0, hi: 4 }, IdRange { lo: 6, hi: 9 }],
                Some(allocation),
            )
            .validate(),
            Err(SegmentError::MalformedPartition {
                segment: 0,
                defect: PartitionDefect::AllocationOutsideNodes,
            })
        );
    }
    assert_eq!(
        partition(
            vec![IdRange { lo: 0, hi: 4 }, IdRange { lo: 5, hi: 9 }],
            Some(IdRange { lo: 3, hi: 6 }),
        )
        .validate(),
        Ok(())
    );
}

#[test]
fn glue_requires_every_known_segment_exactly_once() {
    let p = Partition {
        specs: vec![spec(0, 0, 4), spec(1, 5, 9)],
    };
    let mut g = Graph::default();
    g.nodes.insert(0, Node::new(0, "a"));
    let segments = segment::partition(&g, &p).unwrap();
    assert_eq!(
        segment::glue(&segments[..1], &p),
        Err(SegmentError::MalformedPartition {
            segment: 1,
            defect: PartitionDefect::SegmentMissing,
        })
    );
    let mut repeated = segments.clone();
    repeated.push(segments[0].clone());
    assert_eq!(
        segment::glue(&repeated, &p),
        Err(SegmentError::MalformedPartition {
            segment: 0,
            defect: PartitionDefect::SegmentRepeated,
        })
    );
    let mut unknown = segments;
    unknown[1].segment = 7;
    assert_eq!(
        segment::glue(&unknown, &p),
        Err(SegmentError::MalformedPartition {
            segment: 7,
            defect: PartitionDefect::UnknownSegment,
        })
    );
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
