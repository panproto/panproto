//! Complete finite scopes for the restrict and contraction page.

use crate::migration::Migration;
use std::collections::BTreeSet;

use crate::model::{GEdge, Graph, Node, NodeShape, SEdge};
use crate::schema::Schema;

/// Every two-object schema with at most one labelled edge.
#[must_use]
pub fn schemas() -> Vec<Schema> {
    super::schemas(2, &["k"], &["e"], 1)
}

/// Every vertex-only migration between the first and last bounded schemas.
#[must_use]
pub fn migrations() -> Vec<Migration> {
    let schemas = schemas();
    super::migrations(
        &schemas[0],
        &schemas[schemas.len() - 1],
        super::MigrationScope::VERTICES,
    )
}

/// Labelled graphs with at most two nodes and one edge.
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
        max_edges: 1,
    })
}

/// Every simple directed schema on the three vertices `a`, `b`, and `c`
/// with at most four non-loop edges.
///
/// The six possible ordered pairs form the carrier. Each pair has one
/// distinct `prop` edge, so this scope enumerates all 57 subsets of size at
/// most four without quotienting distinct directed graphs.
#[must_use]
pub fn release_schemas() -> Vec<Schema> {
    let vertices = ["a", "b", "c"];
    let possible = [
        ("a", "b", "ab"),
        ("a", "c", "ac"),
        ("b", "a", "ba"),
        ("b", "c", "bc"),
        ("c", "a", "ca"),
        ("c", "b", "cb"),
    ];
    let mut out = Vec::new();
    for bits in 0u8..(1 << possible.len()) {
        if bits.count_ones() > 4 {
            continue;
        }
        let mut schema = Schema::default();
        for vertex in vertices {
            schema = schema.vertex(vertex, "record");
        }
        for (index, (src, tgt, name)) in possible.iter().enumerate() {
            if bits & (1 << index) != 0 {
                schema = schema.edge(src, tgt, "prop", Some(name));
            }
        }
        out.push(schema);
    }
    out
}

/// All 27 endpoint/interior classifications of `a`, `b`, and `c`.
///
/// A vertex is respectively dropped, surviving, or conditionally surviving.
/// The latter is both endpoint-capable and interior-capable, which is the
/// classification most easily lost by an optimized contraction planner.
#[must_use]
pub fn survival_classifications(schema: &Schema) -> Vec<Migration> {
    let vertices = ["a", "b", "c"];
    let mut out = Vec::new();
    for code in 0u8..27 {
        let mut state = code;
        let mut migration = Migration::default();
        for vertex in vertices {
            match state % 3 {
                0 => {}
                1 => {
                    migration.surviving_verts.insert(vertex.to_owned());
                }
                2 => {
                    migration.surviving_verts.insert(vertex.to_owned());
                    migration.conditional.insert(vertex.to_owned());
                }
                _ => unreachable!(),
            }
            state /= 3;
        }
        migration.surviving_edges = schema
            .edges
            .iter()
            .filter(|edge| {
                migration.surviving_verts.contains(&edge.src)
                    && migration.surviving_verts.contains(&edge.tgt)
            })
            .cloned()
            .collect::<BTreeSet<_>>();
        out.push(migration);
    }
    out
}

/// Every subgraph with at most three edges of the one-node-per-anchor graph
/// presented by `schema`.
///
/// Node ids 0, 1, and 2 carry anchors `a`, `b`, and `c`. Edge ids follow the
/// schema edge order after omitted edges are removed. This is the complete
/// finite carrier needed to census dropped-interior reachability for the
/// release schema scope.
#[must_use]
pub fn release_graphs(schema: &Schema) -> Vec<Graph> {
    let node_ids = [("a", 0), ("b", 1), ("c", 2)]
        .into_iter()
        .collect::<std::collections::BTreeMap<_, _>>();
    let edges: Vec<SEdge> = schema.edges.iter().cloned().collect();
    let mut out = Vec::new();
    for bits in 0u16..(1u16 << edges.len()) {
        if bits.count_ones() > 3 {
            continue;
        }
        let mut graph = Graph::default();
        for (id, anchor) in [(0, "a"), (1, "b"), (2, "c")] {
            graph.nodes.insert(id, Node::new(id, anchor));
        }
        let mut id = 0u32;
        for (index, label) in edges.iter().enumerate() {
            if bits & (1 << index) == 0 {
                continue;
            }
            let (Some(&src), Some(&tgt)) = (
                node_ids.get(label.src.as_str()),
                node_ids.get(label.tgt.as_str()),
            ) else {
                continue;
            };
            graph.edges.insert(
                id,
                GEdge {
                    src,
                    tgt,
                    label: label.clone(),
                    position: Some(id),
                },
            );
            id += 1;
        }
        out.push(graph);
    }
    out
}

/// The five bounded states of one ordered target-anchor pair.
///
/// The pair may have zero, one, or two schema edges. For a nonempty pair,
/// the migration may additionally name the first edge as its resolver.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PairState {
    /// No schema edge and no resolver.
    Missing,
    /// One schema edge and no resolver.
    Unique,
    /// One schema edge selected explicitly.
    UniqueResolved,
    /// Two schema edges and no resolver.
    Ambiguous,
    /// Two schema edges with the first selected explicitly.
    AmbiguousResolved,
}

impl PairState {
    /// Every state of the bounded pair carrier.
    pub const ALL: [Self; 5] = [
        Self::Missing,
        Self::Unique,
        Self::UniqueResolved,
        Self::Ambiguous,
        Self::AmbiguousResolved,
    ];

    /// Number of target-schema edges in this state.
    #[must_use]
    pub const fn edge_count(self) -> usize {
        match self {
            Self::Missing => 0,
            Self::Unique | Self::UniqueResolved => 1,
            Self::Ambiguous | Self::AmbiguousResolved => 2,
        }
    }

    /// Whether the migration supplies a resolver for this pair.
    #[must_use]
    pub const fn has_resolver(self) -> bool {
        matches!(self, Self::UniqueResolved | Self::AmbiguousResolved)
    }
}

/// The expansion-path state used by the bounded pair-resolution carrier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExpansionState {
    /// No expansion path is registered.
    Absent,
    /// The registered path has no intermediate vertex.
    Empty,
    /// The registered path has the single intermediate vertex `c`.
    ViaC,
}

/// One point in the complete bounded pair-resolution carrier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PairResolutionCase {
    /// State of the queried pair `a -> b`.
    pub direct: PairState,
    /// State of the first expansion hop `a -> c`.
    pub first: PairState,
    /// State of the second expansion hop `c -> b`.
    pub second: PairState,
    /// Whether and how the query expands.
    pub expansion: ExpansionState,
}

/// All `5^3 * 3 = 375` bounded pair-resolution cases.
#[must_use]
pub fn pair_resolution_cases() -> Vec<PairResolutionCase> {
    let mut out = Vec::with_capacity(375);
    for direct in PairState::ALL {
        for first in PairState::ALL {
            for second in PairState::ALL {
                for expansion in [
                    ExpansionState::Absent,
                    ExpansionState::Empty,
                    ExpansionState::ViaC,
                ] {
                    out.push(PairResolutionCase {
                        direct,
                        first,
                        second,
                        expansion,
                    });
                }
            }
        }
    }
    out
}

/// The three possible fates of one source table column.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColumnState {
    /// The source attribute is removed.
    Drop,
    /// The source attribute and column name are retained.
    Keep,
    /// The attribute and column name are both renamed.
    Rename,
}

/// All `3^2 = 9` fate assignments for two source columns.
#[must_use]
pub fn column_fate_cases() -> Vec<[ColumnState; 2]> {
    let states = [ColumnState::Drop, ColumnState::Keep, ColumnState::Rename];
    states
        .into_iter()
        .flat_map(|left| states.into_iter().map(move |right| [left, right]))
        .collect()
}

/// One ordered child-binding slot in the bounded TR8 carrier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChildBindingSlot {
    /// A self-loop labelled `x`.
    SelfX,
    /// A self-loop labelled `y`.
    SelfY,
    /// A non-self `x` child without a scalar value.
    MissingX,
    /// A non-self `x` child with scalar value zero.
    ZeroX,
    /// A non-self `x` child with scalar value one.
    OneX,
    /// A non-self `y` child without a scalar value.
    MissingY,
    /// A non-self `y` child with scalar value zero.
    ZeroY,
    /// A non-self `y` child with scalar value one.
    OneY,
}

/// Every ordered child-binding word of length at most three.
///
/// There are `1 + 8 + 8^2 + 8^3 = 585` words. The empty word, repeated
/// labels, duplicate values, missing values, cross-column order, and
/// self-loop exclusion are therefore represented without sampling.
#[must_use]
pub fn child_binding_cases() -> Vec<Vec<ChildBindingSlot>> {
    const SLOTS: [ChildBindingSlot; 8] = [
        ChildBindingSlot::SelfX,
        ChildBindingSlot::SelfY,
        ChildBindingSlot::MissingX,
        ChildBindingSlot::ZeroX,
        ChildBindingSlot::OneX,
        ChildBindingSlot::MissingY,
        ChildBindingSlot::ZeroY,
        ChildBindingSlot::OneY,
    ];
    let mut out = vec![Vec::new()];
    for length in 1usize..=3 {
        let count = (0..length).fold(1usize, |count, _| count * SLOTS.len());
        for mut code in 0..count {
            let mut word = Vec::with_capacity(length);
            for _ in 0..length {
                word.push(SLOTS[code % SLOTS.len()]);
                code /= SLOTS.len();
            }
            out.push(word);
        }
    }
    out
}

/// One arc route in the bounded tree-restrict carrier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TreeRoute {
    /// A source arc retained as one target arc.
    Keep,
    /// A source arc expanded through one synthesized node.
    Expand,
}

/// The five allocation floors exercised by the bounded tree carrier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TreeFloorCase {
    /// Consecutive small node ids.
    Compact,
    /// Small node ids with gaps.
    Gapped,
    /// The floor leaves exactly one allocatable id.
    LastFree,
    /// The floor is `u32::MAX` without an existing maximum id.
    Saturated,
    /// The source already contains `u32::MAX`.
    AlreadyMax,
}

/// One of the `14 * 5 = 70` bounded tree-restrict cases.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TreeRestrictCase {
    /// Ordered source routes, with lengths one through three.
    pub routes: Vec<TreeRoute>,
    /// Source node-id floor shape.
    pub floor: TreeFloorCase,
}

/// All nonempty keep/expand route words of length at most three at all five
/// allocation floors.
#[must_use]
pub fn tree_restrict_cases() -> Vec<TreeRestrictCase> {
    let floors = [
        TreeFloorCase::Compact,
        TreeFloorCase::Gapped,
        TreeFloorCase::LastFree,
        TreeFloorCase::Saturated,
        TreeFloorCase::AlreadyMax,
    ];
    let mut routes = Vec::with_capacity(14);
    for length in 1..=3 {
        for code in 0usize..(1usize << length) {
            routes.push(
                (0..length)
                    .map(|bit| {
                        if code & (1 << bit) == 0 {
                            TreeRoute::Keep
                        } else {
                            TreeRoute::Expand
                        }
                    })
                    .collect::<Vec<_>>(),
            );
        }
    }
    floors
        .into_iter()
        .flat_map(|floor| {
            routes
                .iter()
                .cloned()
                .map(move |routes| TreeRestrictCase { routes, floor })
        })
        .collect()
}
