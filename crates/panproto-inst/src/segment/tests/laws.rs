//! The segment laws: partitioning then assembling is the identity, and the
//! streaming instance digest of the parts is the digest of the whole.

use panproto_schema::InstanceShapeKind;
use proptest::prelude::*;

use super::{arb_finstance, arb_ginstance, arb_partition, arb_winstance, next_free, same_tree};
use crate::ginstance::GInstance;
use crate::segment::{
    AnySegment, RecordSource, SegmentError, assemble_graph, assemble_table, assemble_tree,
    instance_digest, partition_graph, partition_table, partition_tree, winstance_digest,
};

fn fail(e: &SegmentError) -> TestCaseError {
    TestCaseError::fail(e.to_string())
}

/// The record sources of a list of segments.
fn sources(segments: &[AnySegment]) -> Vec<Box<dyn RecordSource>> {
    segments
        .iter()
        .map(|s| Box::new(s.records()) as Box<dyn RecordSource>)
        .collect()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(96))]

    #[test]
    fn partitioning_then_assembling_returns_the_identical_instance_for_every_partition(
        (graph, gp) in arb_ginstance().prop_flat_map(|g| {
            let free = next_free(g.max_node_id(), g.max_edge_id().map(|e| e.0));
            (Just(g), arb_partition(InstanceShapeKind::Graph, free))
        }),
        (tables, tp) in arb_finstance().prop_flat_map(|t| {
            let free = next_free(t.row_ids().last().map(|r| r.0), None);
            (Just(t), arb_partition(InstanceShapeKind::Table, free))
        }),
        ((tree, embedding), wp) in arb_winstance().prop_flat_map(|(t, e)| {
            let free = next_free(e.next_node.checked_sub(1), e.next_edge.checked_sub(1));
            (Just((t, e)), arb_partition(InstanceShapeKind::Tree, free))
        }),
    ) {
        let parts = partition_graph(&graph, &gp).map_err(|e| fail(&e))?;
        prop_assert_eq!(parts.len(), gp.specs.len());
        let back = assemble_graph(parts, &gp).map_err(|e| fail(&e))?;
        prop_assert!(GInstance::identical(&back, &graph));

        let parts = partition_table(&tables, &tp).map_err(|e| fail(&e))?;
        let back = assemble_table(parts, &tp).map_err(|e| fail(&e))?;
        prop_assert_eq!(back, tables);

        let parts = partition_tree(&tree, &embedding, &wp).map_err(|e| fail(&e))?;
        let (back, back_embedding) = assemble_tree(parts, &wp).map_err(|e| fail(&e))?;
        prop_assert!(same_tree(&back, &tree));
        prop_assert_eq!(back_embedding, embedding);
    }

    #[test]
    fn streaming_instance_digest_equals_the_digest_of_the_assembled_instance(
        (graph, gp) in arb_ginstance().prop_flat_map(|g| {
            let free = next_free(g.max_node_id(), g.max_edge_id().map(|e| e.0));
            (Just(g), arb_partition(InstanceShapeKind::Graph, free))
        }),
        (tables, tp) in arb_finstance().prop_flat_map(|t| {
            let free = next_free(t.row_ids().last().map(|r| r.0), None);
            (Just(t), arb_partition(InstanceShapeKind::Table, free))
        }),
        ((tree, embedding), wp) in arb_winstance().prop_flat_map(|(t, e)| {
            let free = next_free(e.next_node.checked_sub(1), e.next_edge.checked_sub(1));
            (Just((t, e)), arb_partition(InstanceShapeKind::Tree, free))
        }),
    ) {
        let parts: Vec<AnySegment> = partition_graph(&graph, &gp).map_err(|e| fail(&e))?.into_iter().map(AnySegment::Graph).collect();
        prop_assert_eq!(instance_digest(InstanceShapeKind::Graph, sources(&parts)).map_err(|e| fail(&e))?, graph.digest());

        let parts: Vec<AnySegment> = partition_table(&tables, &tp).map_err(|e| fail(&e))?.into_iter().map(AnySegment::Table).collect();
        prop_assert_eq!(instance_digest(InstanceShapeKind::Table, sources(&parts)).map_err(|e| fail(&e))?, tables.digest());

        let parts: Vec<AnySegment> = partition_tree(&tree, &embedding, &wp).map_err(|e| fail(&e))?.into_iter().map(AnySegment::Tree).collect();
        let streamed = instance_digest(InstanceShapeKind::Tree, sources(&parts)).map_err(|e| fail(&e))?;
        prop_assert_eq!(streamed, winstance_digest(&tree));
        // The header counts are the sums of the segments' stats.
        let arcs: u64 = parts.iter().map(|p| p.stats().edges).sum();
        let fans: u64 = parts.iter().map(|p| p.stats().fans).sum();
        prop_assert_eq!(arcs, tree.arcs.len() as u64);
        prop_assert_eq!(fans, tree.fans.len() as u64);
    }
}
