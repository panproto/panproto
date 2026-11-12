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
