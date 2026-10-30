//! Finite scopes for the segmentation page.
//!
//! The graph/partition carrier below is exact on its stated alphabet. It is
//! deliberately not called the complete C10 carrier: the contract has not yet
//! fixed finite alphabets for schemas, edge multiplicities, values, node
//! shapes, positions, allocation ranges, or full table/tree carriers. Small
//! graph, table and tree witnesses exercise the common implementation.

use std::collections::{BTreeMap, BTreeSet};

use crate::encode::TreeEmbedding;
use crate::model::{Fan, GEdge, Graph, Node, NodeShape, SEdge, Table, Tree};
use crate::segment::{
    AnyInstance, AnySegment, Boundary, GraphSegment, IdRange, InstanceShapeKind, Partition,
    SegmentId, SegmentSpec, TreeSegment,
};

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
    let nodes: Vec<u32> = graph.nodes.keys().copied().collect();
    ownership_splits(&nodes, k, InstanceShapeKind::Graph)
}

/// Every ownership function from the ascending ids `nodes` to `k` labelled
/// segments of `shape`, represented by singleton ranges, empty segments
/// retained: exactly `k^n` members for `n` ids.
fn ownership_splits(nodes: &[u32], k: u32, shape: InstanceShapeKind) -> Vec<Partition> {
    assert!(
        matches!(k, 2 | 3),
        "the settled carrier uses two or three segments"
    );
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
        for node in nodes {
            let owner = code % radix;
            code /= radix;
            specs[owner].nodes.push(IdRange {
                lo: *node,
                hi: *node,
            });
        }
        out.push(Partition { shape, specs });
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

    let [(one_fan, one_fan_embedding), (two_fans, two_fans_embedding)] = fan_trees();
    let ranges = |ranges: &[(u32, u32)]| -> Vec<IdRange> {
        ranges
            .iter()
            .map(|(lo, hi)| IdRange { lo: *lo, hi: *hi })
            .collect()
    };
    let tree_partition = |left: &[(u32, u32)], right: &[(u32, u32)]| Partition {
        shape: InstanceShapeKind::Tree,
        specs: vec![
            SegmentSpec {
                id: SegmentId(0),
                nodes: ranges(left),
                alloc_nodes: None,
                alloc_edges: None,
            },
            SegmentSpec {
                id: SegmentId(1),
                nodes: ranges(right),
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
        // The fan node and its child share a segment; its parent edge
        // crosses into the root's segment.
        (
            AnyInstance::Tree(one_fan, one_fan_embedding),
            tree_partition(&[(0, 0)], &[(1, 2)]),
        ),
        // Fan node order is the reverse of fan order, and the two fans lie in
        // different segments, so assembly recovers fan order across segments.
        (
            AnyInstance::Tree(two_fans, two_fans_embedding),
            tree_partition(&[(0, 0), (3, 3)], &[(1, 2), (4, 4)]),
        ),
    ]
}

/// The fan-bearing trees of the carrier, with embeddings whose parent edge
/// ids ascend in fan order.
///
/// The first has one fan over an arc. In the second, fan node ids are the
/// reverse of fan order (as after a fan node relocation), so fan node order
/// cannot stand for fan order.
#[must_use]
pub fn fan_trees() -> [(Tree, TreeEmbedding); 2] {
    let label = SEdge::new("a", "b", "e", None);
    let one = Tree {
        nodes: BTreeMap::from([(0, Node::new(0, "a")), (1, Node::new(1, "b"))]),
        arcs: vec![(0, 1, label.clone())],
        fans: vec![Fan {
            parent: 0,
            hyper_edge: "h".to_owned(),
            children: BTreeMap::from([("x".to_owned(), 1)]),
        }],
        root: 0,
    };
    let one_embedding = TreeEmbedding {
        arcs: BTreeMap::from([(1, 0)]),
        fans: vec![(2, 1, BTreeMap::from([("x".to_owned(), 2)]))],
        next_node: 3,
        next_edge: 3,
    };
    let two = Tree {
        nodes: BTreeMap::from([
            (0, Node::new(0, "a")),
            (1, Node::new(1, "b")),
            (2, Node::new(2, "b")),
        ]),
        arcs: vec![(0, 1, label.clone()), (0, 2, label)],
        fans: vec![
            Fan {
                parent: 1,
                hyper_edge: "h".to_owned(),
                children: BTreeMap::from([("x".to_owned(), 2)]),
            },
            Fan {
                parent: 2,
                hyper_edge: "h".to_owned(),
                children: BTreeMap::from([("x".to_owned(), 1)]),
            },
        ],
        root: 0,
    };
    let two_embedding = TreeEmbedding {
        arcs: BTreeMap::from([(1, 0), (2, 1)]),
        fans: vec![
            (4, 2, BTreeMap::from([("x".to_owned(), 3)])),
            (3, 4, BTreeMap::from([("x".to_owned(), 5)])),
        ],
        next_node: 5,
        next_edge: 6,
    };
    [(one, one_embedding), (two, two_embedding)]
}

/// Every fan-bearing tree of [`fan_trees`] paired with every labelled
/// two-segment ownership split of its tree and fan node ids.
#[must_use]
pub fn fan_tree_partition_cases() -> Vec<(AnyInstance, Partition)> {
    fan_trees()
        .into_iter()
        .flat_map(|(tree, embedding)| {
            let mut nodes: Vec<u32> = tree.nodes.keys().copied().collect();
            nodes.extend(embedding.fans.iter().map(|(node, _, _)| *node));
            nodes.sort_unstable();
            let whole = AnyInstance::Tree(tree, embedding);
            ownership_splits(&nodes, 2, InstanceShapeKind::Tree)
                .into_iter()
                .map(move |partition| (whole.clone(), partition))
        })
        .collect()
}

/// Independent count of [`fan_tree_partition_cases`]: the trees hold
/// `2 + 1` and `3 + 2` tree and fan nodes, so `2^3 + 2^5 = 40` cases.
#[must_use]
pub const fn fan_tree_partition_case_count() -> u64 {
    2_u64.pow(3) + 2_u64.pow(5)
}

/// The assembly carrier: every case of [`graph_partition_cases`], then
/// every case of [`fan_tree_partition_cases`].
#[must_use]
pub fn partition_cases() -> Vec<(AnyInstance, Partition)> {
    graph_partition_cases()
        .into_iter()
        .map(|case| (AnyInstance::Graph(case.graph), case.partition))
        .chain(fan_tree_partition_cases())
        .collect()
}

/// Independent count of [`partition_cases`]: `290 + 40 = 330`.
#[must_use]
pub fn partition_case_count() -> u64 {
    graph_partition_case_count() + fan_tree_partition_case_count()
}

/// The edge allocation ranges [`allocated_graph_partition_cases`] gives a
/// partition: none, ranges that all start above every edge id of the
/// carrier, and ranges whose first starts at edge id 1.
pub const ALLOCATION_BASES: [Option<u32>; 3] = [None, Some(2), Some(1)];

/// Every case of [`graph_partition_cases`] under each of
/// [`ALLOCATION_BASES`]: with base `b`, segment `i` allocates edge ids
/// `[b + 10i, b + 10i + 9]`, so the ranges are disjoint.
#[must_use]
pub fn allocated_graph_partition_cases() -> Vec<GraphPartitionCase> {
    graph_partition_cases()
        .into_iter()
        .flat_map(|case| {
            ALLOCATION_BASES.into_iter().map(move |base| {
                let mut case = case.clone();
                for (spec, index) in case.partition.specs.iter_mut().zip(0_u32..) {
                    spec.alloc_edges = base.map(|base| IdRange {
                        lo: base + 10 * index,
                        hi: base + 10 * index + 9,
                    });
                }
                case
            })
        })
        .collect()
}

/// Independent count of [`allocated_graph_partition_cases`]: `3 * 290 = 870`.
#[must_use]
pub fn allocated_graph_partition_case_count() -> u64 {
    3 * graph_partition_case_count()
}

/// One refusal input and the operation it is given to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RefusalInput {
    /// `Partition::validate` of a partition.
    Validate(Partition),
    /// Partitioning a whole instance.
    Split(AnyInstance, Partition),
    /// One segment's own contract.
    Contract(AnySegment, Partition),
}

fn labelled_partition(shape: InstanceShapeKind, ranges: &[&[(u32, u32)]]) -> Partition {
    Partition {
        shape,
        specs: ranges
            .iter()
            .zip(0..)
            .map(|(nodes, id)| SegmentSpec {
                id: SegmentId(id),
                nodes: nodes
                    .iter()
                    .map(|(lo, hi)| IdRange { lo: *lo, hi: *hi })
                    .collect(),
                alloc_nodes: None,
                alloc_edges: None,
            })
            .collect(),
    }
}

/// The labelled refusal carrier: one input per refusal the segment core
/// distinguishes by stage, with the multi-violation segments that fix the
/// order and completeness of an `Invalid` list.
///
/// The cases are: overlapping ranges; an inverted allocation range on a
/// table; an unowned node; a node-map key that is not its node id; tree
/// counters without the root; an owned edge whose source is owned elsewhere
/// and whose target is unowned; a graph segment breaking four clauses at
/// once; tree segment fans out of fan node order; a tree whose fan
/// parent edges descend in fan order; a segment of the wrong shape under an
/// invalid partition; and a whole tree whose node key is not its node id.
#[must_use]
pub fn refusal_cases() -> Vec<RefusalInput> {
    let graph = InstanceShapeKind::Graph;
    let tree = InstanceShapeKind::Tree;
    let label = SEdge::new("a", "a", "e", None);
    let mut unowned = Graph::default();
    unowned.nodes.insert(4, Node::new(4, "a"));
    let mut inverted_table = labelled_partition(InstanceShapeKind::Table, &[&[(0, 9)]]);
    inverted_table.specs[0].alloc_edges = Some(IdRange { lo: 29, hi: 20 });
    let graph_segment = |nodes: BTreeMap<u32, Node>, edges: BTreeMap<u32, GEdge>| {
        AnySegment::Graph(GraphSegment {
            segment: SegmentId(0),
            nodes,
            edges,
            boundary: Boundary::default(),
        })
    };
    let edge = |src: u32, tgt: u32| GEdge {
        src,
        tgt,
        label: label.clone(),
        position: None,
    };
    let mut four_clauses = graph_segment(
        BTreeMap::from([(0, Node::new(2, "a")), (1, Node::new(1, "a"))]),
        BTreeMap::from([(0, edge(0, 7)), (1, edge(9, 0))]),
    );
    if let AnySegment::Graph(segment) = &mut four_clauses {
        segment.boundary.ghosts.insert(
            5,
            crate::segment::Ghost {
                node: 5,
                owner: SegmentId(1),
                anchor: "a".to_owned(),
            },
        );
    }
    let mut cases = vec![
        RefusalInput::Validate(labelled_partition(graph, &[&[(0, 2)], &[(2, 3)]])),
        RefusalInput::Validate(inverted_table),
        RefusalInput::Split(
            AnyInstance::Graph(unowned),
            labelled_partition(graph, &[&[(0, 3)]]),
        ),
        RefusalInput::Contract(
            graph_segment(BTreeMap::from([(0, Node::new(1, "a"))]), BTreeMap::new()),
            labelled_partition(graph, &[&[(0, 0)]]),
        ),
        RefusalInput::Contract(
            AnySegment::Tree(TreeSegment {
                segment: SegmentId(0),
                nodes: BTreeMap::new(),
                arcs: BTreeMap::new(),
                fans: Vec::new(),
                root: None,
                next_node: Some(1),
                next_edge: Some(0),
                boundary: Boundary::default(),
            }),
            labelled_partition(tree, &[&[(0, 4)]]),
        ),
        RefusalInput::Contract(
            graph_segment(BTreeMap::new(), BTreeMap::from([(0, edge(1, 5))])),
            labelled_partition(graph, &[&[(0, 0)], &[(1, 1)]]),
        ),
        RefusalInput::Contract(
            four_clauses,
            labelled_partition(graph, &[&[(0, 0)], &[(1, 1)]]),
        ),
    ];
    cases.extend(fan_order_refusal_cases());
    let mut inverted = labelled_partition(InstanceShapeKind::Table, &[&[(0, 0)]]);
    inverted.specs[0].nodes[0] = IdRange { lo: 3, hi: 1 };
    cases.push(RefusalInput::Contract(
        graph_segment(BTreeMap::new(), BTreeMap::new()),
        inverted,
    ));
    cases.push(RefusalInput::Split(
        AnyInstance::Tree(
            Tree {
                nodes: BTreeMap::from([(0, Node::new(1, "a"))]),
                arcs: Vec::new(),
                fans: Vec::new(),
                root: 0,
            },
            TreeEmbedding {
                arcs: BTreeMap::new(),
                fans: Vec::new(),
                next_node: 2,
                next_edge: 0,
            },
        ),
        labelled_partition(tree, &[&[(0, 1)]]),
    ));
    cases
}

/// The fan-order refusals of [`refusal_cases`]: a tree segment whose fans
/// are out of fan node order, and a tree whose fan parent edges descend in
/// fan order.
fn fan_order_refusal_cases() -> Vec<RefusalInput> {
    let [_, (two_fans, two_fans_embedding)] = fan_trees();
    let fan_partition = labelled_partition(InstanceShapeKind::Tree, &[&[(0, 4)]]);
    let mut misordered = crate::segment::partition(
        &AnyInstance::Tree(two_fans.clone(), two_fans_embedding.clone()),
        &fan_partition,
    )
    .ok()
    .and_then(|mut segments| segments.pop());
    if let Some(AnySegment::Tree(segment)) = &mut misordered {
        segment.fans.reverse();
    }
    let mut descending = two_fans_embedding;
    descending.fans = vec![
        (3, 5, BTreeMap::from([("x".to_owned(), 4)])),
        (4, 2, BTreeMap::from([("x".to_owned(), 3)])),
    ];
    let mut cases: Vec<RefusalInput> = misordered
        .map(|segment| RefusalInput::Contract(segment, fan_partition.clone()))
        .into_iter()
        .collect();
    cases.push(RefusalInput::Split(
        AnyInstance::Tree(two_fans, descending),
        fan_partition,
    ));
    cases
}

/// The number of [`refusal_cases`].
pub const REFUSAL_CASE_COUNT: usize = 11;

/// The allocation carrier: every inclusive range inside `[0, 4]`, with
/// source and used identifier sets drawn from every subset of `[0, 5]`, so
/// identifiers below, inside and above each range all occur.
#[must_use]
pub fn allocation_cases() -> Vec<(IdRange, BTreeSet<u32>, BTreeSet<u32>)> {
    let universe: Vec<u32> = (0..=5).collect();
    let subsets: Vec<BTreeSet<u32>> = (0..(1_u32 << universe.len()))
        .map(|mask| {
            universe
                .iter()
                .filter(|id| (mask >> **id) & 1 == 1)
                .copied()
                .collect()
        })
        .collect();
    let mut out = Vec::new();
    for lo in 0..=4 {
        for hi in lo..=4 {
            for source in &subsets {
                for used in &subsets {
                    out.push((IdRange { lo, hi }, source.clone(), used.clone()));
                }
            }
        }
    }
    out
}

/// Independent count of [`allocation_cases`]: 15 ranges inside `[0, 4]`
/// times `2^6` source sets times `2^6` used sets, `61_440`.
#[must_use]
pub const fn allocation_case_count() -> u64 {
    15 * 2_u64.pow(6) * 2_u64.pow(6)
}
