//! Finite scopes for the segmentation page.
//!
//! The graph/partition carrier below is exact on its stated alphabet. It is
//! deliberately not called the complete segment carrier: its finite alphabets
//! are not yet fixed for schemas, edge multiplicities, values, node
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

/// Dimensions still required before the segment scope can claim full
/// coverage.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ScopeGap {
    /// The finite schema anchor/label alphabet has not been fixed for the segment scope.
    SchemaAlphabet,
    /// The exact graph-node bound (the current witness uses two; the segment scope names
    /// four) has not been reconciled with the remaining alphabets.
    GraphNodeBound,
    /// The finite parallel-edge multiplicity bound has not been fixed for the segment scope.
    EdgeMultiplicity,
    /// The finite node-value alphabet has not been fixed for the segment scope.
    NodeValues,
    /// The finite node-shape and fan alphabet has not been fixed for the segment scope.
    NodeShapesAndFans,
    /// Discriminators, extra fields, node positions, and annotations have no
    /// finite segment alphabet yet.
    NodeMetadataAlphabet,
    /// The finite edge-position alphabet has not been fixed for the segment scope.
    EdgePositions,
    /// Allocation-range bounds have not been fixed for the segment scope.
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

/// The unresolved dimensions of the full segment and segment-execution carrier.
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
/// `n`-node graph.  Allocation ranges are absent because their finite segment
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

/// The first id of the allocation ranges of the segmented restrict carrier.
///
/// Segment `i` allocates node and edge ids from the ten ids starting at
/// this base plus `10 i`, above every node and edge id of the release
/// graphs.
pub const SEGMENTED_RESTRICT_ALLOCATION_BASE: u32 = 100;

/// One case of the segmented restrict carrier: a release schema, a
/// migration of it without survival predicates, a release graph of it, and
/// an ownership split of the graph's nodes with allocation ranges.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SegmentedRestrictCase {
    /// The source and target schema.
    pub schema: crate::schema::Schema,
    /// The migration.
    pub migration: crate::migration::Migration,
    /// The graph.
    pub graph: Graph,
    /// The partition of the graph's nodes, with allocation ranges.
    pub partition: Partition,
}

/// The partition of `split` with allocation ranges.
///
/// Segment `i` owns and allocates node ids from, and allocates edge ids
/// from, the ten ids starting at [`SEGMENTED_RESTRICT_ALLOCATION_BASE`]
/// plus `10 i`.
fn with_allocation(mut split: Partition) -> Partition {
    for (spec, index) in split.specs.iter_mut().zip(0_u32..) {
        let lo = SEGMENTED_RESTRICT_ALLOCATION_BASE + 10 * index;
        let range = IdRange { lo, hi: lo + 9 };
        spec.nodes.push(range);
        spec.alloc_nodes = Some(range);
        spec.alloc_edges = Some(range);
    }
    split
}

/// Every case of the segmented restrict carrier.
///
/// The cases are each release schema
/// ([`release_schemas`](crate::enumerate::restrict_and_contraction::release_schemas)),
/// each of its survival classifications without a conditional vertex, each
/// release graph of the schema, and each labelled two- and three-segment
/// ownership split of the graph's three nodes, with the allocation ranges of
/// [`SEGMENTED_RESTRICT_ALLOCATION_BASE`].
pub fn segmented_restrict_cases() -> impl Iterator<Item = SegmentedRestrictCase> {
    use crate::enumerate::restrict_and_contraction::{
        release_graphs, release_schemas, survival_classifications,
    };
    release_schemas().into_iter().flat_map(|schema| {
        let graphs = release_graphs(&schema);
        let migrations = survival_classifications(&schema)
            .into_iter()
            .filter(|migration| migration.conditional.is_empty());
        migrations.flat_map(move |migration| {
            let schema = schema.clone();
            graphs.clone().into_iter().flat_map(move |graph| {
                let nodes: Vec<u32> = graph.nodes.keys().copied().collect();
                let splits = allocated_splits(&nodes);
                let (schema, migration) = (schema.clone(), migration.clone());
                splits
                    .into_iter()
                    .map(move |partition| SegmentedRestrictCase {
                        schema: schema.clone(),
                        migration: migration.clone(),
                        graph: graph.clone(),
                        partition,
                    })
            })
        })
    })
}

/// Independent count of [`segmented_restrict_cases`].
///
/// A release schema with `e` of the six possible edges (`e <= 4`, so
/// `C(6, e)` schemas) has `sum(j <= min(3, e), C(e, j))` release graphs and
/// `2^3 = 8` classifications without a conditional vertex; each graph has
/// three nodes, so `2^3 + 3^3 = 35` splits. The graphs number
/// `1 + 6 * 2 + 15 * 4 + 20 * 8 + 15 * 15 = 458`, giving
/// `458 * 8 * 35 = 128_240` cases.
#[must_use]
pub fn segmented_restrict_case_count() -> u64 {
    let choose = |n: u64, k: u64| -> u64 { (0..k).fold(1, |acc, i| acc * (n - i) / (i + 1)) };
    let graphs: u64 = (0..=4)
        .map(|edges| choose(6, edges) * (0..=edges.min(3)).map(|j| choose(edges, j)).sum::<u64>())
        .sum();
    graphs * 8 * (2_u64.pow(3) + 3_u64.pow(3))
}

/// Every labelled two- and three-segment ownership split of `nodes`.
///
/// Each split carries the allocation ranges of
/// [`SEGMENTED_RESTRICT_ALLOCATION_BASE`]; there are exactly `2^n + 3^n`
/// splits of `n` ids below the allocation base.
#[must_use]
pub fn allocated_splits(nodes: &[u32]) -> Vec<Partition> {
    [2, 3]
        .into_iter()
        .flat_map(|k| ownership_splits(nodes, k, InstanceShapeKind::Graph))
        .map(with_allocation)
        .collect()
}

/// One case of the segmented extension carrier.
///
/// A case is a release schema, a target schema and the migration whose
/// vertex map sends the source onto it, a release graph of the source, and
/// an ownership split of the graph's nodes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SegmentedExtendCase {
    /// The source schema.
    pub source: crate::schema::Schema,
    /// The target schema.
    pub target: crate::schema::Schema,
    /// The migration.
    pub migration: crate::migration::Migration,
    /// The graph.
    pub graph: Graph,
    /// The partition of the graph's nodes, with allocation ranges.
    pub partition: Partition,
}

/// The target vertices of the segmented extension carrier.
pub const SEGMENTED_EXTEND_IMAGES: [&str; 2] = ["p", "q"];

/// The target schema and migration of the vertex map `code` of `source`.
///
/// The code is read in base three, one digit per source vertex `a`, `b`,
/// `c` in that order: `0` leaves the vertex without an image, `1` and `2`
/// send it to the target vertex `p` and `q`. The target schema holds `p`
/// and `q`, as records, and the image of every source
/// edge whose endpoints are both mapped, the edge's endpoints replaced by
/// their images (two source edges with one image are one target edge). The
/// migration remaps each mapped vertex to its image and nothing else, so a
/// source edge has an image exactly when one target edge joins the images
/// of its endpoints.
#[must_use]
pub fn extension_map(
    source: &crate::schema::Schema,
    code: u8,
) -> (crate::schema::Schema, crate::migration::Migration) {
    let mut images = BTreeMap::new();
    let mut digits = code;
    for vertex in ["a", "b", "c"] {
        match digits % 3 {
            1 => {
                images.insert(vertex.to_owned(), SEGMENTED_EXTEND_IMAGES[0].to_owned());
            }
            2 => {
                images.insert(vertex.to_owned(), SEGMENTED_EXTEND_IMAGES[1].to_owned());
            }
            _ => {}
        }
        digits /= 3;
    }
    let mut target = crate::schema::Schema::default();
    for image in SEGMENTED_EXTEND_IMAGES {
        target = target.vertex(image, "record");
    }
    for edge in &source.edges {
        if let (Some(src), Some(tgt)) = (images.get(&edge.src), images.get(&edge.tgt)) {
            target = target.edge(src, tgt, &edge.kind, edge.name.as_deref());
        }
    }
    let migration = crate::migration::Migration {
        vertex_remap: images,
        ..crate::migration::Migration::default()
    };
    (target, migration)
}

/// Every case of the segmented extension carrier.
///
/// The cases are each release schema
/// ([`release_schemas`](crate::enumerate::restrict_and_contraction::release_schemas)),
/// each of the 27 vertex maps of [`extension_map`], each release graph of
/// the schema, and each labelled two- and three-segment ownership split of
/// the graph's three nodes ([`allocated_splits`]).
pub fn segmented_extend_cases() -> impl Iterator<Item = SegmentedExtendCase> {
    use crate::enumerate::restrict_and_contraction::{release_graphs, release_schemas};
    release_schemas().into_iter().flat_map(|source| {
        let graphs = release_graphs(&source);
        (0..27_u8).flat_map(move |code| {
            let (target, migration) = extension_map(&source, code);
            let source = source.clone();
            graphs.clone().into_iter().flat_map(move |graph| {
                let nodes: Vec<u32> = graph.nodes.keys().copied().collect();
                let (source, target, migration) =
                    (source.clone(), target.clone(), migration.clone());
                allocated_splits(&nodes)
                    .into_iter()
                    .map(move |partition| SegmentedExtendCase {
                        source: source.clone(),
                        target: target.clone(),
                        migration: migration.clone(),
                        graph: graph.clone(),
                        partition,
                    })
            })
        })
    })
}

/// Independent count of [`segmented_extend_cases`]: the 458 release graphs
/// of [`segmented_restrict_case_count`], 27 vertex maps and 35 splits each,
/// `458 * 27 * 35 = 432_810` cases.
#[must_use]
pub fn segmented_extend_case_count() -> u64 {
    segmented_restrict_case_count() / 8 * 27
}

/// One case of the segmented graph edit carrier: a graph, a partition of
/// its node ids and of the ids a delta may add, and a delta.
#[derive(Clone, Debug)]
pub struct SegmentedApplyCase {
    /// The graph the segments hold.
    pub graph: Graph,
    /// The partition.
    pub partition: Partition,
    /// The delta applied to the segments.
    pub delta: crate::edit::GraphDelta,
}

/// The graphs of the segmented graph edit carrier: nodes `0 .. n-1` for `n`
/// at most three, each anchored at `a` with no value, and at most two
/// edges `0` and `1`, each with any endpoints and the label `e`.
#[must_use]
pub fn segmented_apply_graphs() -> Vec<Graph> {
    let labels = [SEdge::new("a", "a", "e", None)];
    super::graphs(&super::GraphScope {
        anchors: &["a"],
        values: &[],
        shapes: &[NodeShape::Plain],
        labels: &labels,
        positions: &[None],
        max_nodes: 3,
        max_edges: 2,
    })
}

/// The partitions of the segmented graph edit carrier.
///
/// Each assigns the node ids `0` to `3` to at most three segments,
/// numbered in the order of their least id (a restricted growth string),
/// or assigns `0` to `2` that way and leaves `3` to no segment; each
/// segment owns its ids as singleton ranges. Each assignment comes with
/// three edge allocations: none; `[10 + 10 i, 19 + 10 i]` for every segment
/// `i`; and `[10, 19]` for segment 0 alone.
#[must_use]
pub fn segmented_apply_partitions() -> Vec<Partition> {
    let mut owners: Vec<Vec<Option<u32>>> = Vec::new();
    for ids in [4_usize, 3] {
        let mut strings: Vec<Vec<u32>> = vec![Vec::new()];
        for _ in 0..ids {
            strings = strings
                .into_iter()
                .flat_map(|string| {
                    let next = string.iter().max().map_or(0, |max| max + 1);
                    (0..=next.min(2)).map(move |block| {
                        let mut longer = string.clone();
                        longer.push(block);
                        longer
                    })
                })
                .collect();
        }
        owners.extend(strings.into_iter().map(|string| {
            let mut owned: Vec<Option<u32>> = string.into_iter().map(Some).collect();
            owned.resize(4, None);
            owned
        }));
    }
    let mut out = Vec::new();
    for owned in owners {
        let segments = owned.iter().flatten().max().map_or(0, |max| max + 1);
        for allocation in 0..3_u32 {
            let specs = (0..segments)
                .map(|id| SegmentSpec {
                    id: SegmentId(id),
                    nodes: (0_u32..)
                        .zip(&owned)
                        .filter(|(_, owner)| **owner == Some(id))
                        .map(|(node, _)| IdRange { lo: node, hi: node })
                        .collect(),
                    alloc_nodes: None,
                    alloc_edges: match allocation {
                        1 => Some(IdRange {
                            lo: 10 + 10 * id,
                            hi: 19 + 10 * id,
                        }),
                        2 if id == 0 => Some(IdRange { lo: 10, hi: 19 }),
                        _ => None,
                    },
                })
                .collect();
            out.push(Partition {
                shape: InstanceShapeKind::Graph,
                specs,
            });
        }
    }
    out
}

/// The deltas of the segmented graph edit carrier.
///
/// Every atomic delta and the identity ([`super::graph_deltas`]) over the
/// node ids `0`, `2` and `3`, the anchors `a` and `b`, no value or field,
/// the edge ids `1`, `2` and `10`, the labels `e` and `e` named `x`, and the
/// positions none and `0`.
#[must_use]
pub fn segmented_apply_deltas() -> Vec<crate::edit::GraphDelta> {
    let labels = [
        SEdge::new("a", "a", "e", None),
        SEdge::new("a", "a", "e", Some("x")),
    ];
    super::graph_deltas(&super::DeltaScope {
        nodes: &[0, 2, 3],
        anchors: &["a", "b"],
        values: &[],
        fields: &[],
        edges: &[1, 2, 10],
        labels: &labels,
        positions: &[None, Some(0)],
    })
}

/// Every graph of [`segmented_apply_graphs`] under every partition of
/// [`segmented_apply_partitions`] with every delta of
/// [`segmented_apply_deltas`], in that nesting order.
pub fn segmented_apply_cases() -> impl Iterator<Item = SegmentedApplyCase> {
    let partitions = segmented_apply_partitions();
    let deltas = segmented_apply_deltas();
    segmented_apply_graphs().into_iter().flat_map(move |graph| {
        let deltas = deltas.clone();
        partitions.clone().into_iter().flat_map(move |partition| {
            let graph = graph.clone();
            deltas
                .clone()
                .into_iter()
                .map(move |delta| SegmentedApplyCase {
                    graph: graph.clone(),
                    partition: partition.clone(),
                    delta,
                })
        })
    })
}

/// Independent counts of the segmented graph edit carrier.
///
/// Graphs: with `n` nodes there are `n^2` labelled edges, so
/// `sum_(n=0)^3 (1 + n^2 + n^4) = 1 + 3 + 21 + 91 = 116` graphs (the
/// graph with no node has no edge). Partitions: the restricted growth
/// strings of length four with at most three blocks number
/// `1 + 7 + 6 = 14` (the Stirling numbers `S(4, k)` for `k` up to three)
/// and those of length three `1 + 3 + 1 = 5`, each with three edge
/// allocations, `57`. Deltas: the identity; four node payloads per id to
/// add or delete over three ids (two anchors, add or delete), `12`; to add
/// or delete each of three edge ids with nine endpoint pairs and two
/// labels, `108`; an anchor and a shape change in two directions per node,
/// `12`; and a label and a position change in two directions per edge,
/// `12`; `145` in all. So `116 * 57 * 145 = 958_740` cases.
#[must_use]
pub const fn segmented_apply_case_counts() -> (u64, u64, u64, u64) {
    let graphs = 1 + 3 + 21 + 91;
    let partitions = (14 + 5) * 3;
    let deltas = 1 + 12 + 108 + 12 + 12;
    (graphs, partitions, deltas, graphs * partitions * deltas)
}

/// The families of the segmented edit composition carrier.
///
/// Every graph over at most two nodes and one edge (anchor `a`, no value,
/// label `e`) under each of four partitions of the node identifiers `0` to `3`, each segment
/// owning its identifiers as singleton ranges and allocating edge
/// identifiers from `[10 + 10 i, 19 + 10 i]`: all in one segment; `0` apart
/// from `1`, `2` and `3`; `0` and `3` apart from `1` and `2`; and `0`, `1`,
/// and `2` with `3` in three segments.
#[must_use]
pub fn segmented_compose_families() -> Vec<(Graph, Partition)> {
    let labels = [SEdge::new("a", "a", "e", None)];
    let graphs = super::graphs(&super::GraphScope {
        anchors: &["a"],
        values: &[],
        shapes: &[NodeShape::Plain],
        labels: &labels,
        positions: &[None],
        max_nodes: 2,
        max_edges: 1,
    });
    let assignments: [[u32; 4]; 4] = [[0, 0, 0, 0], [0, 1, 1, 1], [0, 1, 1, 0], [0, 1, 2, 2]];
    let partitions: Vec<Partition> = assignments
        .iter()
        .map(|owners| {
            let segments = owners.iter().max().map_or(0, |max| max + 1);
            Partition {
                shape: InstanceShapeKind::Graph,
                specs: (0..segments)
                    .map(|id| SegmentSpec {
                        id: SegmentId(id),
                        nodes: (0_u32..)
                            .zip(owners)
                            .filter(|(_, owner)| **owner == id)
                            .map(|(node, _)| IdRange { lo: node, hi: node })
                            .collect(),
                        alloc_nodes: None,
                        alloc_edges: Some(IdRange {
                            lo: 10 + 10 * id,
                            hi: 19 + 10 * id,
                        }),
                    })
                    .collect(),
            }
        })
        .collect();
    graphs
        .into_iter()
        .flat_map(|graph| {
            partitions
                .iter()
                .map(move |partition| (graph.clone(), partition.clone()))
        })
        .collect()
}

/// The deltas of the segmented edit composition carrier.
///
/// Every atomic delta and the identity ([`super::graph_deltas`]) over the
/// node identifiers `0` and `3`, the anchors `a` and `b`, no value or
/// field, the edge identifiers `1` and `10`, the label `e`, and the
/// positions none and `0`.
#[must_use]
pub fn segmented_compose_deltas() -> Vec<crate::edit::GraphDelta> {
    let labels = [SEdge::new("a", "a", "e", None)];
    super::graph_deltas(&super::DeltaScope {
        nodes: &[0, 3],
        anchors: &["a", "b"],
        values: &[],
        fields: &[],
        edges: &[1, 10],
        labels: &labels,
        positions: &[None, Some(0)],
    })
}

/// Independent counts of the segmented edit composition carrier.
///
/// Graphs: `sum_(n=0)^2 (1 + n^2) = 1 + 2 + 5 = 8`, under four partitions,
/// `32` families. Deltas: the identity; four node payloads per identifier to
/// add or delete over two identifiers, `8`; to add or delete each of two
/// edge identifiers with four endpoint pairs, `16`; an anchor and a shape
/// change in two directions per node, `8`; and a position change in two
/// directions per edge, `4`; `37` in all. So `32 * 37 * 37 = 43_808`
/// ordered pairs.
#[must_use]
pub const fn segmented_compose_counts() -> (u64, u64, u64) {
    let families = (1 + 2 + 5) * 4;
    let deltas = 1 + 8 + 16 + 8 + 4;
    (families, deltas, families * deltas * deltas)
}

/// One case of the segmented table edit carrier: a table, a partition of
/// its row ids, and a table delta.
#[derive(Clone, Debug)]
pub struct SegmentedTableApplyCase {
    /// The table the segments hold.
    pub table: Table,
    /// The partition.
    pub partition: Partition,
    /// The delta applied to the segments.
    pub delta: crate::edit::TableDelta,
}

/// The table partitions assigning the row ids `0` to `3` as `owners` does
/// (`None` for no segment), each segment owning its ids as singleton ranges
/// and, with `allocate`, allocating row ids from `[10 + 10 i, 19 + 10 i]`.
fn table_partition(owners: &[Option<u32>], allocate: bool) -> Partition {
    let segments = owners.iter().flatten().max().map_or(0, |max| max + 1);
    Partition {
        shape: InstanceShapeKind::Table,
        specs: (0..segments)
            .map(|id| SegmentSpec {
                id: SegmentId(id),
                nodes: (0_u32..)
                    .zip(owners)
                    .filter(|(_, owner)| **owner == Some(id))
                    .map(|(node, _)| IdRange { lo: node, hi: node })
                    .chain(allocate.then_some(IdRange {
                        lo: 10 + 10 * id,
                        hi: 19 + 10 * id,
                    }))
                    .collect(),
                alloc_nodes: allocate.then_some(IdRange {
                    lo: 10 + 10 * id,
                    hi: 19 + 10 * id,
                }),
                alloc_edges: None,
            })
            .collect(),
    }
}

/// The partitions of the segmented table edit carrier: every assignment of
/// [`segmented_apply_partitions`] as a table partition without allocation
/// ranges.
#[must_use]
pub fn segmented_table_apply_partitions() -> Vec<Partition> {
    segmented_apply_partitions()
        .into_iter()
        .filter(|partition| {
            partition
                .specs
                .iter()
                .all(|spec| spec.alloc_edges.is_none())
        })
        .map(|partition| {
            let owners: Vec<Option<u32>> = (0..4_u32)
                .map(|row| {
                    partition
                        .specs
                        .iter()
                        .find(|spec| spec.nodes.iter().any(|range| range.lo == row))
                        .map(|spec| spec.id.0)
                })
                .collect();
            table_partition(&owners, false)
        })
        .collect()
}

/// The atomic table deltas over the row ids `ids`.
///
/// They are the identity; deleting or adding each row in table `a` without
/// cells or with `f = 0`; deleting or adding the pair `a -e-> a` between
/// each two of them; adding or removing the cell `f = 0` of each; and
/// moving each from `a` to `b` and back.
#[must_use]
pub fn segmented_table_deltas(ids: &[u32]) -> Vec<crate::edit::TableDelta> {
    use crate::edit::{TElem, TKey, TPayload, TVal, TableDelta};
    use crate::value::Value;
    let mut out = vec![TableDelta::identity()];
    for id in ids {
        for cells in [
            BTreeMap::new(),
            BTreeMap::from([("f".to_owned(), Value::Int(0))]),
        ] {
            let payload = TPayload::Row("a".to_owned(), cells);
            let mut delete = TableDelta::identity();
            delete.del.insert(TElem::Row(*id), payload.clone());
            out.push(delete);
            let mut add = TableDelta::identity();
            add.add.insert(TElem::Row(*id), payload);
            out.push(add);
        }
    }
    let edge = SEdge::new("a", "a", "e", None);
    for src in ids {
        for tgt in ids {
            let pair = TElem::Pair(edge.clone(), *src, *tgt);
            let mut delete = TableDelta::identity();
            delete.del.insert(pair.clone(), TPayload::Pair);
            out.push(delete);
            let mut add = TableDelta::identity();
            add.add.insert(pair, TPayload::Pair);
            out.push(add);
        }
    }
    for id in ids {
        for (old, new) in [(None, Some(Value::Int(0))), (Some(Value::Int(0)), None)] {
            let mut cell = TableDelta::identity();
            cell.attrs.insert(
                TKey::Cell(*id, "f".to_owned()),
                (TVal::Cell(old), TVal::Cell(new)),
            );
            out.push(cell);
        }
    }
    for id in ids {
        for (old, new) in [("a", "b"), ("b", "a")] {
            let mut table = TableDelta::identity();
            table.attrs.insert(
                TKey::Table(*id),
                (TVal::Table(old.to_owned()), TVal::Table(new.to_owned())),
            );
            out.push(table);
        }
    }
    out
}

/// Every table of [`super::edits::tables`] under every partition of
/// [`segmented_table_apply_partitions`] with every delta of
/// [`segmented_table_deltas`] over the row ids `0`, `2` and `3`, in that
/// nesting order.
pub fn segmented_table_apply_cases() -> impl Iterator<Item = SegmentedTableApplyCase> {
    let partitions = segmented_table_apply_partitions();
    let deltas = segmented_table_deltas(&[0, 2, 3]);
    super::edits::tables().into_iter().flat_map(move |table| {
        let deltas = deltas.clone();
        partitions.clone().into_iter().flat_map(move |partition| {
            let table = table.clone();
            deltas
                .clone()
                .into_iter()
                .map(move |delta| SegmentedTableApplyCase {
                    table: table.clone(),
                    partition: partition.clone(),
                    delta,
                })
        })
    })
}

/// Independent counts of the segmented table edit carrier.
///
/// Tables: `105` ([`super::edits::tables_count_certificate`]). Partitions:
/// the `14 + 5 = 19` restricted growth strings of
/// [`segmented_apply_case_counts`]. Deltas over three row ids: the
/// identity; two payloads to delete or add per row, `12`; nine pairs to
/// delete or add, `18`; a cell in two directions per row, `6`; and a table
/// in two directions per row, `6`; `43` in all. So `105 * 19 * 43 =
/// 85_785` cases.
#[must_use]
pub const fn segmented_table_apply_case_counts() -> (u64, u64, u64, u64) {
    let tables = 1 + 4 + 20 + 80;
    let partitions = 14 + 5;
    let deltas = 1 + 12 + 18 + 6 + 6;
    (tables, partitions, deltas, tables * partitions * deltas)
}

/// The families of the segmented table edit composition carrier.
///
/// Every table of [`super::edits::tables`] over at most two rows under each
/// of four partitions of the row ids `0` to `3` (all in one segment; `0`
/// apart from `1`, `2` and `3`; `0` and `3` apart from `1` and `2`; and
/// `0`, `1`, and `2` with `3` in three segments), each segment owning its
/// ids as singleton ranges and allocating row ids from
/// `[10 + 10 i, 19 + 10 i]`.
#[must_use]
pub fn segmented_table_compose_families() -> Vec<(Table, Partition)> {
    let assignments: [[u32; 4]; 4] = [[0, 0, 0, 0], [0, 1, 1, 1], [0, 1, 1, 0], [0, 1, 2, 2]];
    let partitions: Vec<Partition> = assignments
        .iter()
        .map(|owners| {
            let owners: Vec<Option<u32>> = owners.iter().copied().map(Some).collect();
            table_partition(&owners, true)
        })
        .collect();
    super::edits::tables()
        .into_iter()
        .filter(|table| table.rows.len() <= 2)
        .flat_map(|table| {
            partitions
                .iter()
                .map(move |partition| (table.clone(), partition.clone()))
        })
        .collect()
}

/// Independent counts of the segmented table edit composition carrier.
///
/// Tables over at most two rows: `sum_(n=0)^2 2^n (1 + n^2) = 1 + 4 + 20 =
/// 25`, under four partitions, `100` families. Deltas of
/// [`segmented_table_deltas`] over the row ids `0` and `3`: the identity,
/// `8` row deletions and additions, `8` pair deletions and additions, `4`
/// cell changes and `4` table changes, `25` in all. So
/// `100 * 25 * 25 = 62_500` ordered pairs.
#[must_use]
pub const fn segmented_table_compose_counts() -> (u64, u64, u64) {
    let families = (1 + 4 + 20) * 4;
    let deltas = 1 + 8 + 8 + 4 + 4;
    (families, deltas, families * deltas * deltas)
}

/// One case of the segmented tree edit carrier: a tree with its embedding,
/// a partition of its node ids, and a tree edit.
#[derive(Clone, Debug)]
pub struct SegmentedTreeApplyCase {
    /// The tree the segments hold.
    pub tree: Tree,
    /// The tree's embedding: its graph encoding's
    /// ([`crate::encode::tree_to_graph`]).
    pub embedding: TreeEmbedding,
    /// The partition.
    pub partition: Partition,
    /// The edit applied to the segments.
    pub edit: crate::edit::TreeEdit,
}

/// The label of every arc of the segmented tree edit carrier.
fn tree_arc_label() -> SEdge {
    SEdge::new("a", "a", "e", None)
}

/// The trees of the segmented tree edit carrier, with their embeddings.
///
/// Four shapes over nodes `0 .. n-1` rooted at `0`, every node anchored at
/// `a` and every arc labelled `a -e-> a`: the root alone; `0 -> 1`;
/// `0 -> 1, 0 -> 2`; and `0 -> 1 -> 2`. Each comes without a fan and with
/// the fan of hyper-edge `h` from its last node to the root under the label
/// `x`. The embedding is the graph encoding's, so the fan node is `n` and
/// the node counter `n` or `n + 1`.
#[must_use]
pub fn segmented_tree_apply_trees() -> Vec<(Tree, TreeEmbedding)> {
    let shapes: [&[(u32, u32)]; 4] = [&[], &[(0, 1)], &[(0, 1), (0, 2)], &[(0, 1), (1, 2)]];
    let mut out = Vec::new();
    for arcs in shapes {
        let n = u32::try_from(arcs.len()).unwrap_or(u32::MAX) + 1;
        for fan in [false, true] {
            let tree = Tree {
                nodes: (0..n).map(|id| (id, Node::new(id, "a"))).collect(),
                arcs: arcs
                    .iter()
                    .map(|(parent, child)| (*parent, *child, tree_arc_label()))
                    .collect(),
                fans: if fan {
                    vec![Fan {
                        parent: n - 1,
                        hyper_edge: "h".to_owned(),
                        children: BTreeMap::from([("x".to_owned(), 0)]),
                    }]
                } else {
                    Vec::new()
                },
                root: 0,
            };
            if let Ok((_, embedding)) = crate::encode::tree_to_graph(&tree, None) {
                out.push((tree, embedding));
            }
        }
    }
    out
}

/// The edits of the segmented tree edit carrier at a tree with nodes
/// `0 .. n-1` and node counter `fresh`.
///
/// In order: an insertion of node `fresh` (anchored at `a`, under the label
/// `a -e-> a`) under each node; an insertion of node `n - 1`, which exists,
/// under the root; an insertion of node `fresh` under node `3`, which is no
/// tree node; the deletion of each of the ids `0` to `3`; the contraction of
/// each node; the change of each node's anchor to `b`; setting the field `f`
/// of each node to `0`; the move of each node under each other node; the
/// insertion of a fan of hyper-edge `g` from each node to the root under the
/// label `y`; the deletion of the fans of `h` and of `g`; and the join of
/// each node into each other node, the product being the primary anchored
/// at `a`. That is `5n + 8 + 2n(n - 1)` edits.
#[must_use]
pub fn segmented_tree_edits(n: u32, fresh: u32) -> Vec<crate::edit::TreeEdit> {
    use crate::edit::TreeEdit;
    let insert = |parent: u32, child_id: u32| TreeEdit::InsertNode {
        parent,
        child_id,
        node: Node::new(child_id, "a"),
        edge: tree_arc_label(),
    };
    let mut out: Vec<TreeEdit> = (0..n).map(|parent| insert(parent, fresh)).collect();
    out.push(insert(0, n - 1));
    out.push(insert(3, fresh));
    out.extend((0..=3).map(|id| TreeEdit::DeleteNode { id }));
    out.extend((0..n).map(|id| TreeEdit::ContractNode { id }));
    out.extend((0..n).map(|id| TreeEdit::RelabelNode {
        id,
        new_anchor: "b".to_owned(),
    }));
    out.extend((0..n).map(|node_id| TreeEdit::SetField {
        node_id,
        field: "f".to_owned(),
        value: crate::value::Value::Int(0),
    }));
    for node_id in 0..n {
        for new_parent in (0..n).filter(|parent| *parent != node_id) {
            out.push(TreeEdit::MoveSubtree {
                node_id,
                new_parent,
                edge: tree_arc_label(),
            });
        }
    }
    out.extend((0..n).map(|parent| TreeEdit::InsertFan {
        fan: Fan {
            parent,
            hyper_edge: "g".to_owned(),
            children: BTreeMap::from([("y".to_owned(), 0)]),
        },
    }));
    out.extend(["h", "g"].map(|hyper| TreeEdit::DeleteFan {
        hyper_edge: hyper.to_owned(),
    }));
    for primary in 0..n {
        for joined in (0..n).filter(|joined| *joined != primary) {
            out.push(TreeEdit::JoinFeatures {
                primary,
                joined: vec![joined],
                produce: Node::new(primary, "a"),
            });
        }
    }
    out
}

/// The tree partitions assigning the node ids `0` to `4` as `owners` does
/// (`None` for no segment), each segment owning its ids as singleton
/// ranges.
fn tree_owner_partition(owners: &[Option<u32>]) -> Partition {
    let segments = owners.iter().flatten().max().map_or(0, |max| max + 1);
    Partition {
        shape: InstanceShapeKind::Tree,
        specs: (0..segments)
            .map(|id| SegmentSpec {
                id: SegmentId(id),
                nodes: (0_u32..)
                    .zip(owners)
                    .filter(|(_, owner)| **owner == Some(id))
                    .map(|(node, _)| IdRange { lo: node, hi: node })
                    .collect(),
                alloc_nodes: None,
                alloc_edges: None,
            })
            .collect(),
    }
}

/// The restricted growth strings of length `len` with at most `blocks`
/// blocks: the assignments of `len` ids to at most `blocks` segments,
/// numbered in the order of their least id.
fn restricted_growth(len: usize, blocks: u32) -> Vec<Vec<u32>> {
    let mut strings: Vec<Vec<u32>> = vec![Vec::new()];
    for _ in 0..len {
        strings = strings
            .into_iter()
            .flat_map(|string| {
                let next = string.iter().max().map_or(0, |max| max + 1);
                (0..=next.min(blocks - 1)).map(move |block| {
                    let mut longer = string.clone();
                    longer.push(block);
                    longer
                })
            })
            .collect();
    }
    strings
}

/// The partitions of the segmented tree edit carrier: every assignment of
/// the node ids `0` to `4` to at most three segments, then every assignment
/// of `0` to `3` to at most three segments leaving `4` to no segment.
#[must_use]
pub fn segmented_tree_apply_partitions() -> Vec<Partition> {
    let mut out = Vec::new();
    for (len, unowned) in [(5_usize, 0_usize), (4, 1)] {
        for string in restricted_growth(len, 3) {
            let mut owners: Vec<Option<u32>> = string.into_iter().map(Some).collect();
            owners.extend(std::iter::repeat_n(None, unowned));
            out.push(tree_owner_partition(&owners));
        }
    }
    out
}

/// Every tree of [`segmented_tree_apply_trees`] under every partition of
/// [`segmented_tree_apply_partitions`] with every edit of
/// [`segmented_tree_edits`] at that tree, in that nesting order.
pub fn segmented_tree_apply_cases() -> impl Iterator<Item = SegmentedTreeApplyCase> {
    let partitions = segmented_tree_apply_partitions();
    segmented_tree_apply_trees()
        .into_iter()
        .flat_map(move |(tree, embedding)| {
            let n = u32::try_from(tree.nodes.len()).unwrap_or(u32::MAX);
            let edits = segmented_tree_edits(n, embedding.next_node);
            partitions.clone().into_iter().flat_map(move |partition| {
                let (tree, embedding) = (tree.clone(), embedding.clone());
                edits
                    .clone()
                    .into_iter()
                    .map(move |edit| SegmentedTreeApplyCase {
                        tree: tree.clone(),
                        embedding: embedding.clone(),
                        partition: partition.clone(),
                        edit,
                    })
            })
        })
}

/// Independent counts of the segmented tree edit carrier.
///
/// Trees: four shapes, each without and with a fan, `8`, two with one node,
/// two with two and four with three. Partitions: the restricted growth
/// strings of length five with at most three blocks number
/// `1 + 15 + 25 = 41` (the Stirling numbers `S(5, k)` for `k` up to three)
/// and those of length four `1 + 7 + 6 = 14`, `55`. Edits at a tree of `n`
/// nodes: `5n + 8 + 2n(n - 1)`, so `13`, `22` and `35`, and
/// `2 * 13 + 2 * 22 + 4 * 35 = 210` over the trees. So `55 * 210 = 11_550`
/// cases.
#[must_use]
pub const fn segmented_tree_apply_case_counts() -> (u64, u64, u64, u64) {
    let trees = 8;
    let partitions = 41 + 14;
    let edits = 2 * 13 + 2 * 22 + 4 * 35;
    (trees, partitions, edits, partitions * edits)
}

/// The foreign keys of the segmented table restriction carrier:
/// `a -ab-> b` and `b -ba-> a`, both of kind `relation`.
#[must_use]
pub fn segmented_table_restrict_keys() -> [SEdge; 2] {
    [
        SEdge::new("a", "b", "relation", Some("ab")),
        SEdge::new("b", "a", "relation", Some("ba")),
    ]
}

/// Every table of the segmented table restriction carrier.
///
/// Each of the row ids `0` to `3` is absent, a row of table `a` with the
/// cell `x = 0`, or a row of table `b` without cells; the pairs are every
/// set of at most two of the pairs of [`segmented_table_restrict_keys`]
/// whose rows lie in the key's tables, in ascending order of the sets'
/// sorted pair lists.
#[must_use]
pub fn segmented_table_restrict_tables() -> Vec<Table> {
    use crate::value::Value;
    let keys = segmented_table_restrict_keys();
    let mut out = Vec::new();
    for code in 0..81_u32 {
        let mut rows = BTreeMap::new();
        let mut rest = code;
        for id in 0..4_u32 {
            match rest % 3 {
                1 => {
                    rows.insert(
                        id,
                        (
                            "a".to_owned(),
                            BTreeMap::from([("x".to_owned(), Value::Int(0))]),
                        ),
                    );
                }
                2 => {
                    rows.insert(id, ("b".to_owned(), BTreeMap::new()));
                }
                _ => {}
            }
            rest /= 3;
        }
        let mut joinable = Vec::new();
        for key in &keys {
            for (src, (src_table, _)) in &rows {
                for (tgt, (tgt_table, _)) in &rows {
                    if *src_table == key.src && *tgt_table == key.tgt {
                        joinable.push((key.clone(), *src, *tgt));
                    }
                }
            }
        }
        joinable.sort();
        let table = |pairs: &[&(SEdge, u32, u32)]| Table {
            rows: rows.clone(),
            pairs: pairs.iter().map(|pair| (*pair).clone()).collect(),
        };
        out.push(table(&[]));
        for (i, first) in joinable.iter().enumerate() {
            out.push(table(&[first]));
            for second in &joinable[i + 1..] {
                out.push(table(&[first, second]));
            }
        }
    }
    out
}

/// Independent count of [`segmented_table_restrict_tables`].
///
/// With `k_a` rows in `a` and `k_b` in `b`, chosen in `4! / (k_a! k_b!
/// (4 - k_a - k_b)!)` ways, there are `P = 2 k_a k_b` joinable pairs and
/// `1 + P + P (P - 1) / 2` sets of at most two of them. Summed over every
/// `k_a + k_b <= 4`: `741`.
#[must_use]
pub const fn segmented_table_restrict_table_count() -> u64 {
    const FACTORIAL: [u64; 5] = [1, 1, 2, 6, 24];
    let mut total = 0;
    let mut ka = 0;
    while ka <= 4 {
        let mut kb = 0;
        while ka + kb <= 4 {
            let ways = FACTORIAL[4] / (FACTORIAL[ka] * FACTORIAL[kb] * FACTORIAL[4 - ka - kb]);
            let pairs = 2 * (ka as u64) * (kb as u64);
            let sets = 1 + pairs + pairs * pairs.saturating_sub(1) / 2;
            total += ways * sets;
            kb += 1;
        }
        ka += 1;
    }
    total
}

/// Every table of the segmented table put carrier: the tables of
/// [`segmented_table_restrict_tables`] whose rows all lie among the row ids
/// `0` to `2`, in the same order.
#[must_use]
pub fn segmented_table_put_tables() -> Vec<Table> {
    segmented_table_restrict_tables()
        .into_iter()
        .filter(|table| table.rows.keys().all(|id| *id < 3))
        .collect()
}

/// Independent count of [`segmented_table_put_tables`].
///
/// With `k_a` rows in `a` and `k_b` in `b` among three ids, chosen in
/// `3! / (k_a! k_b! (3 - k_a - k_b)!)` ways, there are `P = 2 k_a k_b`
/// joinable pairs and `1 + P + P (P - 1) / 2` sets of at most two of them.
/// Summed over every `k_a + k_b <= 3`: `105`.
#[must_use]
pub const fn segmented_table_put_table_count() -> u64 {
    const FACTORIAL: [u64; 4] = [1, 1, 2, 6];
    let mut total = 0;
    let mut ka = 0;
    while ka <= 3 {
        let mut kb = 0;
        while ka + kb <= 3 {
            let ways = FACTORIAL[3] / (FACTORIAL[ka] * FACTORIAL[kb] * FACTORIAL[3 - ka - kb]);
            let pairs = 2 * (ka as u64) * (kb as u64);
            let sets = 1 + pairs + pairs * pairs.saturating_sub(1) / 2;
            total += ways * sets;
            kb += 1;
        }
        ka += 1;
    }
    total
}

/// The partitions of the segmented table put carrier: every set partition
/// of the row ids `0` to `2`, by restricted growth string ordered by
/// `sum s_i 4^i`, each segment owning its ids as singleton ranges and
/// allocating nothing. There are `5`, the Bell number of three.
#[must_use]
pub fn segmented_table_put_partitions() -> Vec<Partition> {
    let mut out = Vec::new();
    for code in 0..64_u32 {
        let owners: Vec<u32> = (0..3).map(|i| (code >> (2 * i)) & 3).collect();
        let restricted = owners
            .iter()
            .enumerate()
            .all(|(i, owner)| *owner <= owners[..i].iter().max().map_or(0, |max| max + 1));
        if restricted {
            let owners: Vec<Option<u32>> = owners.into_iter().map(Some).collect();
            out.push(table_partition(&owners, false));
        }
    }
    out
}

/// The partitions of the segmented table restriction carrier: every set
/// partition of the row ids `0` to `3`.
///
/// A partition is given by its restricted growth string (row `i` in segment
/// `s_i`, with `s_0 = 0` and each `s_i` at most one above every earlier one)
/// and ordered by `sum s_i 4^i`; each segment owns its ids as singleton
/// ranges and allocates nothing. There are `15`, the Bell number of four.
#[must_use]
pub fn segmented_table_restrict_partitions() -> Vec<Partition> {
    let mut out = Vec::new();
    for code in 0..256_u32 {
        let owners: Vec<u32> = (0..4).map(|i| (code >> (2 * i)) & 3).collect();
        let restricted = owners
            .iter()
            .enumerate()
            .all(|(i, owner)| *owner <= owners[..i].iter().max().map_or(0, |max| max + 1));
        if restricted {
            let owners: Vec<Option<u32>> = owners.into_iter().map(Some).collect();
            out.push(table_partition(&owners, false));
        }
    }
    out
}
