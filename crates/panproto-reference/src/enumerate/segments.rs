//! Finite scopes for the segmentation page.
//!
//! The graph/partition carrier below is exact on its stated alphabet. It is
//! deliberately not called the complete C10 carrier: the contract has not yet
//! fixed finite alphabets for schemas, edge multiplicities, values, node
//! shapes, positions, allocation ranges, or full table/tree carriers. Small
//! graph, table and tree witnesses exercise the common implementation.

use std::collections::BTreeMap;

use crate::encode::TreeEmbedding;
use crate::model::{Graph, Node, NodeShape, SEdge, Table, Tree};
use crate::segment::{AnyInstance, IdRange, InstanceShapeKind, Partition, SegmentId, SegmentSpec};

/// A graph paired with one labelled two- or three-segment ownership split.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GraphPartitionCase {
    /// The graph whose canonical node ids are split.
    pub graph: Graph,
    /// One partition representing the ownership function.
    pub partition: Partition,
}

/// Dimensions still required before the segment scope can claim full C10
/// coverage.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ScopeGap {
    /// The finite schema anchor/label alphabet has not been fixed for C10.
    SchemaAlphabet,
    /// The exact graph-node bound (the current witness uses two; C10 names
    /// four) has not been reconciled with the remaining alphabets.
    GraphNodeBound,
    /// The finite parallel-edge multiplicity bound has not been fixed for C10.
    EdgeMultiplicity,
    /// The finite node-value alphabet has not been fixed for C10.
    NodeValues,
    /// The finite node-shape and fan alphabet has not been fixed for C10.
    NodeShapesAndFans,
    /// Discriminators, extra fields, node positions, and annotations have no
    /// finite C10 alphabet yet.
    NodeMetadataAlphabet,
    /// The finite edge-position alphabet has not been fixed for C10.
    EdgePositions,
    /// Allocation-range bounds have not been fixed for C10.
    AllocationRanges,
    /// Canonical state ids and general multi-range partition shapes need an
    /// exact carrier or a proved renaming quotient.
    StateIdsAndRangeShapes,
    /// The exhaustive table row/cell/pair carrier size is not fixed.
    TableCarrierBounds,
    /// The exhaustive tree arc/fan/embedding carrier size is not fixed.
    TreeCarrierBounds,
    /// The exhaustive phase/store/failure-schedule carrier is not fixed.
    ExecutionCarrierBounds,
}

/// The unresolved dimensions of the full C10/SG.11 carrier.
pub const FULL_SCOPE_GAPS: [ScopeGap; 12] = [
    ScopeGap::SchemaAlphabet,
    ScopeGap::GraphNodeBound,
    ScopeGap::EdgeMultiplicity,
    ScopeGap::NodeValues,
    ScopeGap::NodeShapesAndFans,
    ScopeGap::NodeMetadataAlphabet,
    ScopeGap::EdgePositions,
    ScopeGap::AllocationRanges,
    ScopeGap::StateIdsAndRangeShapes,
    ScopeGap::TableCarrierBounds,
    ScopeGap::TreeCarrierBounds,
    ScopeGap::ExecutionCarrierBounds,
];

/// Labelled graphs with at most two nodes and two parallel edges.
#[must_use]
pub fn graphs() -> Vec<Graph> {
    let labels = [SEdge::new("a", "a", "e", None)];
    super::graphs(&super::GraphScope {
        anchors: &["a"],
        values: &[],
        shapes: &[NodeShape::Plain],
        labels: &labels,
        positions: &[None],
        max_nodes: 2,
        max_edges: 2,
    })
}

/// Independent count of [`graphs`].
///
/// With `n` nodes, one anchor, one shape, one label, one position, and at
/// most two sequentially labelled edges, the count is
/// `sum(n=0..2, sum(m=0..2, (n^2)^m)) = 25`.
#[must_use]
pub fn graph_count() -> u64 {
    (0_u64..=2)
        .map(|nodes| {
            (0_u32..=2)
                .map(|edges| (nodes * nodes).pow(edges))
                .sum::<u64>()
        })
        .sum()
}

/// Every ownership function from the canonical node ids of `graph` to `k`
/// labelled segments, represented by singleton ranges.
///
/// Empty segments are retained, so this has exactly `k^n` members for an
/// `n`-node graph.  Allocation ranges are absent because their finite C10
/// alphabet is one of [`FULL_SCOPE_GAPS`].
#[must_use]
fn partitions(graph: &Graph, k: u32) -> Vec<Partition> {
    assert!(
        matches!(k, 2 | 3),
        "the settled carrier uses two or three segments"
    );
    let nodes: Vec<u32> = graph.nodes.keys().copied().collect();
    let radix = usize::try_from(k).unwrap_or(3);
    let count = nodes.iter().fold(1_usize, |product, _| product * radix);
    let mut out = Vec::with_capacity(count);
    for mut code in 0..count {
        let mut specs: Vec<SegmentSpec> = (0..k)
            .map(|id| SegmentSpec {
                id: SegmentId(id),
                nodes: Vec::new(),
                alloc_nodes: None,
                alloc_edges: None,
            })
            .collect();
        for node in &nodes {
            let owner = code % radix;
            code /= radix;
            specs[owner].nodes.push(IdRange {
                lo: *node,
                hi: *node,
            });
        }
        out.push(Partition {
            shape: InstanceShapeKind::Graph,
            specs,
        });
    }
    out
}

/// Every graph in [`graphs`] paired with every labelled two- and
/// three-segment ownership split.
#[must_use]
pub fn graph_partition_cases() -> Vec<GraphPartitionCase> {
    graphs()
        .into_iter()
        .flat_map(|graph| {
            [2, 3].into_iter().flat_map(move |segments| {
                partitions(&graph, segments).into_iter().map({
                    let graph = graph.clone();
                    move |partition| GraphPartitionCase {
                        graph: graph.clone(),
                        partition,
                    }
                })
            })
        })
        .collect()
}

/// Independent count of [`graph_partition_cases`].
///
/// For `n` nodes there are `2^n + 3^n` labelled ownership functions.  The
/// graph count at `n` is `sum(m=0..2, (n^2)^m)`, giving
/// `2 + 3*5 + 21*13 = 290` cases.
#[must_use]
pub fn graph_partition_case_count() -> u64 {
    (0_u32..=2)
        .map(|nodes| {
            let node_pairs = u64::from(nodes) * u64::from(nodes);
            let graph_count = (0_u32..=2).map(|edges| node_pairs.pow(edges)).sum::<u64>();
            let split_count = 2_u64.pow(nodes) + 3_u64.pow(nodes);
            graph_count * split_count
        })
        .sum()
}

/// One explicit witness for each shape, each with a crossing edge/pair/arc.
///
/// This closes shape dispatch and boundary representation. It is not the
/// missing exhaustive table/tree carrier named by [`FULL_SCOPE_GAPS`].
#[must_use]
pub fn shape_witnesses() -> Vec<(AnyInstance, Partition)> {
    let label = SEdge::new("a", "b", "e", None);

    let mut graph = Graph::default();
    graph.nodes.insert(0, Node::new(0, "a"));
    graph.nodes.insert(1, Node::new(1, "b"));
    graph.edges.insert(
        0,
        crate::model::GEdge {
            src: 0,
            tgt: 1,
            label: label.clone(),
            position: None,
        },
    );

    let mut table = Table::default();
    table.rows.insert(0, ("a".to_owned(), BTreeMap::new()));
    table.rows.insert(1, ("b".to_owned(), BTreeMap::new()));
    table.pairs.insert((label.clone(), 0, 1));

    let tree = Tree {
        nodes: BTreeMap::from([(0, Node::new(0, "a")), (1, Node::new(1, "b"))]),
        arcs: vec![(0, 1, label)],
        fans: Vec::new(),
        root: 0,
    };
    let embedding = TreeEmbedding {
        arcs: BTreeMap::from([(1, 0)]),
        fans: Vec::new(),
        next_node: 2,
        next_edge: 1,
    };

    let partition = |shape| Partition {
        shape,
        specs: vec![
            SegmentSpec {
                id: SegmentId(0),
                nodes: vec![IdRange { lo: 0, hi: 0 }],
                alloc_nodes: None,
                alloc_edges: None,
            },
            SegmentSpec {
                id: SegmentId(1),
                nodes: vec![IdRange { lo: 1, hi: 1 }],
                alloc_nodes: None,
                alloc_edges: None,
            },
        ],
    };

    vec![
        (
            AnyInstance::Graph(graph),
            partition(InstanceShapeKind::Graph),
        ),
        (
            AnyInstance::Table(table),
            partition(InstanceShapeKind::Table),
        ),
        (
            AnyInstance::Tree(tree, embedding),
            partition(InstanceShapeKind::Tree),
        ),
    ]
}
