//! Partitions: dense ids, well-formed specs, disjoint ranges, ownership.

use panproto_schema::InstanceShapeKind;
use proptest::prelude::*;

use super::{
    TestResult, arb_ginstance, arb_partition, arb_winstance, interleaved, label, next_free,
};
use crate::ginstance::{EdgeId, GEdge, GInstance};
use crate::metadata::Node;
use crate::segment::{
    IdKind, IdRange, Partition, PartitionDefect, SegmentError, SegmentId, SegmentSpec,
    partition_graph, partition_tree,
};
use crate::view_ref::ViewRef;

fn spec(id: u32, nodes: &[(u32, u32)]) -> SegmentSpec {
    SegmentSpec {
        id: SegmentId(id),
        nodes: nodes.iter().map(|&(lo, hi)| IdRange { lo, hi }).collect(),
        alloc_nodes: None,
        alloc_edges: None,
    }
}

fn graph_partition(specs: Vec<SegmentSpec>) -> Partition {
    Partition {
        shape: InstanceShapeKind::Graph,
        specs,
    }
}

#[test]
fn partition_validate_rejects_non_dense_ids() {
    let skipped = graph_partition(vec![spec(0, &[(0, 4)]), spec(2, &[(5, 9)])]);
    assert_eq!(
        skipped.validate(),
        Err(SegmentError::MalformedPartition {
            segment: SegmentId(1),
            defect: PartitionDefect::NotDense {
                found: SegmentId(2)
            },
        })
    );
    let swapped = graph_partition(vec![spec(1, &[(0, 4)]), spec(0, &[(5, 9)])]);
    assert_eq!(
        swapped.validate(),
        Err(SegmentError::MalformedPartition {
            segment: SegmentId(0),
            defect: PartitionDefect::NotDense {
                found: SegmentId(1)
            },
        })
    );
    assert_eq!(
        graph_partition(vec![spec(0, &[(0, 4)]), spec(1, &[(5, 9)])]).validate(),
        Ok(())
    );
}

#[test]
fn partition_validate_rejects_each_malformed_spec() {
    let s0 = SegmentId(0);
    let malformed = |defect| {
        Err(SegmentError::MalformedPartition {
            segment: s0,
            defect,
        })
    };
    // Node ranges out of order.
    assert_eq!(
        graph_partition(vec![spec(0, &[(5, 9), (0, 4)])]).validate(),
        malformed(PartitionDefect::UnsortedRanges)
    );
    // Node ranges of one spec that overlap.
    assert_eq!(
        graph_partition(vec![spec(0, &[(0, 5), (5, 9)])]).validate(),
        Err(SegmentError::OverlappingRanges { a: s0, b: s0 })
    );
    // An inverted range.
    assert_eq!(
        graph_partition(vec![spec(0, &[(4, 2)])]).validate(),
        Err(SegmentError::InvertedRange {
            segment: s0,
            range: IdRange { lo: 4, hi: 2 },
        })
    );
    // A node allocation range not inside the node ranges, even when it
    // straddles a gap between them.
    for alloc in [IdRange { lo: 10, hi: 12 }, IdRange { lo: 3, hi: 6 }] {
        let mut s = spec(0, &[(0, 4), (6, 9)]);
        s.alloc_nodes = Some(alloc);
        assert_eq!(
            graph_partition(vec![s]).validate(),
            malformed(PartitionDefect::AllocationOutsideNodes)
        );
    }
    let mut inside = spec(0, &[(0, 4), (5, 9)]);
    inside.alloc_nodes = Some(IdRange { lo: 3, hi: 6 });
    assert_eq!(graph_partition(vec![inside]).validate(), Ok(()));
    // An edge allocation range on a table.
    let mut s = spec(0, &[(0, 9)]);
    s.alloc_edges = Some(IdRange { lo: 20, hi: 29 });
    assert_eq!(
        Partition {
            shape: InstanceShapeKind::Table,
            specs: vec![s.clone()],
        }
        .validate(),
        malformed(PartitionDefect::EdgeAllocationOnTable)
    );
    assert_eq!(graph_partition(vec![s]).validate(), Ok(()));
}

#[test]
fn overlapping_node_or_edge_allocation_ranges_are_rejected() {
    let nodes = graph_partition(vec![
        spec(0, &[(0, 4), (20, 30)]),
        spec(1, &[(5, 9)]),
        spec(2, &[(25, 26)]),
    ]);
    assert_eq!(
        nodes.validate(),
        Err(SegmentError::OverlappingRanges {
            a: SegmentId(0),
            b: SegmentId(2),
        })
    );
    let mut a = spec(0, &[(0, 4)]);
    let mut b = spec(1, &[(5, 9)]);
    a.alloc_edges = Some(IdRange { lo: 100, hi: 110 });
    b.alloc_edges = Some(IdRange { lo: 110, hi: 120 });
    assert_eq!(
        graph_partition(vec![a.clone(), b.clone()]).validate(),
        Err(SegmentError::OverlappingRanges {
            a: SegmentId(0),
            b: SegmentId(1),
        })
    );
    b.alloc_edges = Some(IdRange { lo: 111, hi: 120 });
    assert_eq!(graph_partition(vec![a, b]).validate(), Ok(()));
}

#[test]
fn an_element_with_an_unowned_id_is_rejected() -> TestResult {
    let partition = graph_partition(vec![spec(0, &[(0, 4)]), spec(1, &[(10, 14)])]);
    assert_eq!(partition.owner_of_node(7), None);
    assert_eq!(partition.owner_index()?.owner_of_node(7), None);
    let ab = label("a", "a", "prop", None);
    let lone = GInstance::new().with_node(Node::new(7, "a"));
    assert_eq!(
        partition_graph(&lone, &partition).err(),
        Some(SegmentError::Unowned {
            elem: ViewRef::Node(7)
        })
    );
    let mut dangling = GInstance::new().with_node(Node::new(1, "a"));
    dangling.insert_node(Node::new(12, "a"))?;
    dangling.insert_edge(EdgeId(3), GEdge::new(1, 12, ab))?;
    let mut narrowed = partition.clone();
    narrowed.specs[1].nodes = vec![IdRange { lo: 11, hi: 11 }];
    // Node 12 now has no owner; it is reported before any edge.
    assert_eq!(
        partition_graph(&dangling, &narrowed).err(),
        Some(SegmentError::Unowned {
            elem: ViewRef::Node(12)
        })
    );
    Ok(())
}

#[test]
fn nodes_are_owned_by_their_range_owner() -> TestResult {
    let partition = interleaved(InstanceShapeKind::Graph, 12, 3, 2);
    let index = partition.owner_index()?;
    for id in 0..18 {
        let by_scan = partition.owner_of_node(id);
        assert_eq!(index.owner_of_node(id), by_scan);
        let expected = if id < 12 {
            Some(SegmentId(id % 3))
        } else {
            Some(SegmentId((id - 12) / 2))
        };
        assert_eq!(by_scan, expected, "node {id}");
    }
    assert_eq!(index.owner_of_node(18), None);
    let mut graph = GInstance::new();
    for id in 0..12 {
        graph.insert_node(Node::new(id, "a"))?;
    }
    for segment in partition_graph(&graph, &partition)? {
        for id in segment.nodes.keys() {
            assert_eq!(index.owner_of_node(*id), Some(segment.segment));
        }
    }
    Ok(())
}

proptest! {
    #[test]
    fn an_edge_is_owned_by_its_source_owner(
        graph in arb_ginstance(),
        k in 1u32..5,
    ) {
        let free = next_free(graph.max_node_id(), graph.max_edge_id().map(|e| e.0));
        let partition = interleaved(InstanceShapeKind::Graph, free.max(1), k, 0);
        let index = partition.owner_index().map_err(|e| TestCaseError::fail(e.to_string()))?;
        let segments = partition_graph(&graph, &partition).map_err(|e| TestCaseError::fail(e.to_string()))?;
        let mut held = 0;
        for segment in &segments {
            for edge in segment.edges.values() {
                prop_assert_eq!(index.owner_of_node(edge.src), Some(segment.segment));
                // A node and all its out-edges are co-located.
                prop_assert!(segment.nodes.contains_key(&edge.src));
            }
            held += segment.edges.len();
        }
        prop_assert_eq!(held, graph.edge_count());
    }

    #[test]
    fn a_fan_and_its_whole_incidence_are_owned_by_the_fan_node_owner(
        (tree, embedding) in arb_winstance(),
        k in 1u32..4,
    ) {
        let free = next_free(embedding.next_node.checked_sub(1), embedding.next_edge.checked_sub(1));
        let partition = interleaved(InstanceShapeKind::Tree, free.max(1), k, 0);
        let index = partition.owner_index().map_err(|e| TestCaseError::fail(e.to_string()))?;
        let segments = partition_tree(&tree, &embedding, &partition).map_err(|e| TestCaseError::fail(e.to_string()))?;
        for segment in &segments {
            for fan in &segment.fans {
                prop_assert_eq!(index.owner_of_node(fan.node), Some(segment.segment));
                for id in std::iter::once(fan.parent_edge).chain(fan.child_edges.values().copied()) {
                    let arc = segment.arcs.get(&id);
                    prop_assert!(arc.is_some_and(|(src, _, _)| *src == fan.node));
                }
            }
            for (parent, _, _) in segment.arcs.values() {
                prop_assert_eq!(index.owner_of_node(*parent), Some(segment.segment));
            }
        }
    }

    #[test]
    fn every_generated_partition_is_valid(
        (free, partition) in (0u32..50, prop::sample::select(vec![InstanceShapeKind::Graph, InstanceShapeKind::Table, InstanceShapeKind::Tree]))
            .prop_flat_map(|(free, shape)| (Just(free), arb_partition(shape, free))),
    ) {
        prop_assert_eq!(partition.validate(), Ok(()));
        for id in 0..free {
            prop_assert!(partition.owner_of_node(id).is_some());
        }
    }
}

#[test]
fn by_node_ranges_carves_consecutive_allocation_ranges() -> TestResult {
    let p = Partition::by_node_ranges(InstanceShapeKind::Graph, &[10, 15], 5, 20)?;
    let r = |lo, hi| IdRange { lo, hi };
    assert_eq!(p.specs.len(), 3);
    assert_eq!(p.specs[0].nodes, vec![r(0, 9), r(20, 24)]);
    assert_eq!(p.specs[1].nodes, vec![r(10, 14), r(25, 29)]);
    assert_eq!(p.specs[2].nodes, vec![r(15, 19), r(30, 34)]);
    for (i, s) in p.specs.iter().enumerate() {
        let lo = 20 + 5 * u32::try_from(i)?;
        assert_eq!(s.alloc_nodes, Some(r(lo, lo + 4)));
        assert_eq!(s.alloc_edges, Some(r(lo, lo + 4)));
    }
    let t = Partition::by_node_ranges(InstanceShapeKind::Table, &[10], 5, 20)?;
    assert!(t.specs.iter().all(|s| s.alloc_edges.is_none()));
    let none = Partition::by_node_ranges(InstanceShapeKind::Graph, &[10], 0, 20)?;
    assert_eq!(none.specs[1].nodes, vec![r(10, 19)]);
    assert_eq!(none.specs[1].alloc_nodes, None);
    assert_eq!(
        Partition::by_node_ranges(InstanceShapeKind::Graph, &[10, 10], 0, 20),
        Err(SegmentError::MalformedPartition {
            segment: SegmentId(1),
            defect: PartitionDefect::UnsortedRanges,
        })
    );
    assert_eq!(
        Partition::by_node_ranges(InstanceShapeKind::Graph, &[10], u32::MAX / 2, u32::MAX - 10),
        Err(SegmentError::AllocationExhausted {
            segment: SegmentId(0),
            kind: IdKind::Node,
        })
    );
    let single = Partition::single(InstanceShapeKind::Graph, Some(41));
    assert_eq!(single.specs.len(), 1);
    assert_eq!(single.specs[0].nodes, vec![IdRange::FULL]);
    assert_eq!(single.specs[0].alloc_nodes, Some(r(42, u32::MAX)));
    assert_eq!(single.specs[0].alloc_edges, Some(r(42, u32::MAX)));
    assert_eq!(
        Partition::single(InstanceShapeKind::Graph, Some(u32::MAX)).specs[0].alloc_nodes,
        None
    );
    assert_eq!(
        Partition::single(InstanceShapeKind::Graph, None).specs[0].alloc_nodes,
        Some(IdRange::FULL)
    );
    Ok(())
}
