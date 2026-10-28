//! Shape comparison and finite law checking for reference lenses.
//!
//! A [`LawShape`] supplies execution and a deterministic Single-Step Basis
//! (SSB), while this module owns the five gate definitions and compares the
//! resulting values independently. Fusion uses the same comparison
//! vocabulary but remains separate because fused and product complements
//! need not be identical.

use std::collections::{BTreeMap, BTreeSet};

use super::domain::{PutDomainError, ViewRef};
use super::graph::{GraphLens, IdentityError, LensError};
use super::table::TableComplement;
use super::tree::{TreeOperationalError, tree_check_put_domain_law, tree_get, tree_put_law};
use crate::encode::TreeEmbedding;
use crate::fan::FanViolation;
use crate::keys::{KeyEquality, SqlCollation};
use crate::model::{GEdge, Graph, Node, NodeShape, SEdge, Table, Tree};
use crate::restrict::{
    Complement, EdgeFate, PairResolution, RestrictError, SynthKey, floor_of, is_fan_kind,
    reverse_anchor,
};
use crate::value::{Presence, Value};

/// Stable synthesized-object names used by graph and tree law equivalence.
///
/// Concrete ids are presentation data. Two instances may therefore be law
/// equivalent when their synthesized ids differ, provided each id carries
/// the same structural [`SynthKey`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SynthIndex {
    /// Synthesized graph/tree nodes by concrete id.
    pub nodes: BTreeMap<u32, SynthKey>,
    /// Synthesized graph edges by concrete id.
    pub edges: BTreeMap<u32, SynthKey>,
}

/// The coordinate on which two observations first differ.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DivergenceKind {
    /// Instance carrier or payload.
    Instance,
    /// Complement residue read by put.
    Complement,
    /// Change outside the mutation footprint.
    Locality,
}

/// A stable, shape-independent divergence description.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Divergence {
    /// Kind of disagreement.
    pub kind: DivergenceKind,
    /// First differing structural coordinate.
    pub detail: &'static str,
}

/// Structural comparison needed by lens laws.
pub trait LensShape {
    /// Instance representation.
    type Instance: Clone;
    /// Complement representation.
    type Complement: Clone;

    /// Exact presentation equality.
    fn identical(left: &Self::Instance, right: &Self::Instance) -> bool;

    /// Exact complement presentation equality, including derived fields
    /// omitted by [`Self::complement_divergence`].
    fn complements_identical(left: &Self::Complement, right: &Self::Complement) -> bool;

    /// Synthesized-object identities for this presentation.
    fn synth_index(instance: &Self::Instance, complement: &Self::Complement) -> SynthIndex;

    /// Declared observational equivalence modulo synthesized presentation ids.
    fn equivalent(
        left: &Self::Instance,
        left_index: &SynthIndex,
        right: &Self::Instance,
        right_index: &SynthIndex,
    ) -> bool;

    /// First instance divergence under declared equivalence.
    fn divergence(
        left: &Self::Instance,
        left_index: &SynthIndex,
        right: &Self::Instance,
        right_index: &SynthIndex,
    ) -> Option<Divergence>;

    /// First observational complement field that differs.
    ///
    /// Graph comparison includes both components of [`Complement::floor`],
    /// table comparison includes [`TableComplement::id_floor`], and tree
    /// comparison inherits the graph-floor comparison. Derived identifiers
    /// and other bookkeeping omitted by the shape are non-gating.
    fn complement_divergence(
        left: &Self::Complement,
        right: &Self::Complement,
    ) -> Option<Divergence>;
}

/// Graph-shaped structural comparisons.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GraphShape;

/// Table-shaped structural comparisons.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TableShape;

/// Tree-shaped structural comparisons.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TreeShape;

/// A tree-law complement binds graph residue to the arc/fan carrier
/// presentation consumed by tree put.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TreeLawComplement {
    /// Graph-coordinate complement produced by tree get.
    pub complement: Complement,
    /// Exact graph ids of the view's arcs and fans.
    pub embedding: TreeEmbedding,
    /// Source graph-edge ids mapped to the tree arc carriers they encode.
    ///
    /// This derived index makes expansion keys stable when an unrelated
    /// earlier tree arc is inserted or removed and the graph encoding's
    /// positional edge ids consequently shift.
    pub source_arcs: BTreeMap<u32, (u32, u32)>,
}

fn tree_source_arcs(source: &Tree) -> BTreeMap<u32, (u32, u32)> {
    source
        .arcs
        .iter()
        .enumerate()
        .filter_map(|(edge, (parent, child, _))| {
            u32::try_from(edge)
                .ok()
                .map(|edge| (edge, (*parent, *child)))
        })
        .collect()
}

fn tree_synth_key(key: &SynthKey, source_arcs: &BTreeMap<u32, (u32, u32)>) -> SynthKey {
    match key {
        SynthKey::Expand { source_edge, hop } => SynthKey::Expand {
            source_edge: source_arcs
                .get(source_edge)
                .map_or(*source_edge, |(_, child)| *child),
            hop: *hop,
        },
        SynthKey::Contract { .. } | SynthKey::ContractExpand { .. } => key.clone(),
    }
}

fn tree_synth_index(complement: &TreeLawComplement) -> SynthIndex {
    SynthIndex {
        nodes: complement
            .complement
            .synthesized_nodes
            .iter()
            .map(|(id, key)| (*id, tree_synth_key(key, &complement.source_arcs)))
            .collect(),
        edges: complement
            .complement
            .synthesized_edges
            .iter()
            .map(|(id, key)| (*id, tree_synth_key(key, &complement.source_arcs)))
            .collect(),
    }
}

/// A typed refusal produced by the executable graph and tree law adapters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReferenceLawError {
    /// Forward restriction refused.
    Restrict(RestrictError),
    /// Complement identity did not match the receiving lens and shape.
    Identity(IdentityError),
    /// Every put-domain violation in report order.
    PutDomain(Vec<PutDomainError>),
    /// Tree carrier construction or readback failed operationally.
    TreeOperational(TreeOperationalError),
    /// The tree lens is not defined on this source representation.
    OutsideTreeDomain,
}

impl LensShape for GraphShape {
    type Instance = Graph;
    type Complement = Complement;

    fn identical(left: &Graph, right: &Graph) -> bool {
        left == right
    }

    fn complements_identical(left: &Complement, right: &Complement) -> bool {
        left == right
    }

    fn synth_index(instance: &Graph, complement: &Complement) -> SynthIndex {
        SynthIndex {
            nodes: complement
                .synthesized_nodes
                .iter()
                .filter(|(id, _)| instance.nodes.contains_key(id))
                .map(|(id, key)| (*id, key.clone()))
                .collect(),
            edges: complement
                .synthesized_edges
                .iter()
                .filter(|(id, _)| instance.edges.contains_key(id))
                .map(|(id, key)| (*id, key.clone()))
                .collect(),
        }
    }

    fn equivalent(
        left: &Graph,
        left_index: &SynthIndex,
        right: &Graph,
        right_index: &SynthIndex,
    ) -> bool {
        normalized_graph(left, left_index, right_index).is_some_and(|graph| graph == *right)
    }

    fn divergence(
        left: &Graph,
        left_index: &SynthIndex,
        right: &Graph,
        right_index: &SynthIndex,
    ) -> Option<Divergence> {
        let Some(left) = normalized_graph(left, left_index, right_index) else {
            return Some(instance_divergence("graph synthesized key sets differ"));
        };
        if left.nodes != right.nodes {
            Some(instance_divergence("graph nodes differ"))
        } else if left.edges != right.edges {
            Some(instance_divergence("graph edges differ"))
        } else {
            None
        }
    }

    fn complement_divergence(left: &Complement, right: &Complement) -> Option<Divergence> {
        graph_complement_divergence(left, right)
    }
}

impl LensShape for TableShape {
    type Instance = Table;
    type Complement = TableComplement;

    fn identical(left: &Table, right: &Table) -> bool {
        left == right
    }

    fn complements_identical(left: &TableComplement, right: &TableComplement) -> bool {
        left == right
    }

    fn synth_index(_: &Table, _: &TableComplement) -> SynthIndex {
        SynthIndex::default()
    }

    fn equivalent(left: &Table, _: &SynthIndex, right: &Table, _: &SynthIndex) -> bool {
        left == right
    }

    fn divergence(
        left: &Table,
        _: &SynthIndex,
        right: &Table,
        _: &SynthIndex,
    ) -> Option<Divergence> {
        if left.rows != right.rows {
            Some(instance_divergence("table rows differ"))
        } else if left.pairs != right.pairs {
            Some(instance_divergence("table foreign-key pairs differ"))
        } else {
            None
        }
    }

    fn complement_divergence(
        left: &TableComplement,
        right: &TableComplement,
    ) -> Option<Divergence> {
        if left.id_floor != right.id_floor {
            Some(complement_divergence("id floor differs"))
        } else if left.dropped_rows != right.dropped_rows {
            Some(complement_divergence("dropped rows differ"))
        } else if left.dropped_cells != right.dropped_cells {
            Some(complement_divergence("dropped cells differ"))
        } else if left.dropped_fk_pairs != right.dropped_fk_pairs {
            Some(complement_divergence("dropped foreign-key pairs differ"))
        } else if left.original_fk_edges != right.original_fk_edges {
            Some(complement_divergence("original foreign-key edges differ"))
        } else if left.original_tables != right.original_tables {
            Some(complement_divergence("original tables differ"))
        } else if left.original_cells != right.original_cells {
            Some(complement_divergence("original cells differ"))
        } else if left.synthesized_fk_pairs != right.synthesized_fk_pairs {
            Some(complement_divergence(
                "synthesized foreign-key pairs differ",
            ))
        } else {
            None
        }
    }
}

impl LensShape for TreeShape {
    type Instance = Tree;
    type Complement = TreeLawComplement;

    fn identical(left: &Tree, right: &Tree) -> bool {
        left == right
    }

    fn complements_identical(left: &TreeLawComplement, right: &TreeLawComplement) -> bool {
        left == right
    }

    fn synth_index(_: &Tree, complement: &TreeLawComplement) -> SynthIndex {
        tree_synth_index(complement)
    }

    fn equivalent(
        left: &Tree,
        left_index: &SynthIndex,
        right: &Tree,
        right_index: &SynthIndex,
    ) -> bool {
        normalized_tree(left, left_index, right_index).is_some_and(|tree| {
            tree.nodes == right.nodes
                && tree_ordered_arcs(&tree) == tree_ordered_arcs(right)
                && tree.fans == right.fans
                && tree.root == right.root
        })
    }

    fn divergence(
        left: &Tree,
        left_index: &SynthIndex,
        right: &Tree,
        right_index: &SynthIndex,
    ) -> Option<Divergence> {
        let Some(left) = normalized_tree(left, left_index, right_index) else {
            return Some(instance_divergence("tree synthesized key sets differ"));
        };
        if left.nodes != right.nodes {
            Some(instance_divergence("tree nodes differ"))
        } else if tree_ordered_arcs(&left) != tree_ordered_arcs(right) {
            Some(instance_divergence("tree per-parent arc sequences differ"))
        } else if left.fans != right.fans {
            Some(instance_divergence("tree fans differ"))
        } else if left.root != right.root {
            Some(instance_divergence("tree roots differ"))
        } else {
            None
        }
    }

    fn complement_divergence(
        left: &TreeLawComplement,
        right: &TreeLawComplement,
    ) -> Option<Divergence> {
        graph_complement_divergence(&left.complement, &right.complement).or_else(|| {
            let Some(nodes) = synthesized_alignment(
                &left.complement.synthesized_nodes,
                &right.complement.synthesized_nodes,
            ) else {
                return Some(complement_divergence("tree synthesized nodes differ"));
            };
            let Some(edges) = synthesized_alignment(
                &left.complement.synthesized_edges,
                &right.complement.synthesized_edges,
            ) else {
                return Some(complement_divergence("tree synthesized edges differ"));
            };
            (reindexed_embedding(&left.embedding, &nodes, &edges) != right.embedding)
                .then(|| complement_divergence("tree embeddings differ"))
        })
    }
}

/// Each parent's ordered child/label sequence.
///
/// The flat arc vector may interleave arcs from different parents without
/// changing the tree. Order within one parent's sequence remains observable.
fn tree_ordered_arcs(tree: &Tree) -> BTreeMap<u32, Vec<(u32, SEdge)>> {
    let mut by_parent: BTreeMap<u32, Vec<(u32, SEdge)>> = BTreeMap::new();
    for (parent, child, label) in &tree.arcs {
        by_parent
            .entry(*parent)
            .or_default()
            .push((*child, label.clone()));
    }
    by_parent
}

fn synthesized_alignment(
    left: &BTreeMap<u32, SynthKey>,
    right: &BTreeMap<u32, SynthKey>,
) -> Option<BTreeMap<u32, u32>> {
    let right_by_key: BTreeMap<_, _> = right.iter().map(|(id, key)| (key.clone(), *id)).collect();
    if right_by_key.len() != right.len() || left.len() != right.len() {
        return None;
    }
    let mut alignment = BTreeMap::new();
    for (id, key) in left {
        alignment.insert(*id, *right_by_key.get(key)?);
    }
    Some(alignment)
}

fn aligned_id(alignment: &BTreeMap<u32, u32>, id: u32) -> u32 {
    alignment.get(&id).copied().unwrap_or(id)
}

fn reindexed_embedding(
    embedding: &TreeEmbedding,
    nodes: &BTreeMap<u32, u32>,
    edges: &BTreeMap<u32, u32>,
) -> TreeEmbedding {
    let arcs: BTreeMap<_, _> = embedding
        .arcs
        .iter()
        .map(|(child, edge)| (aligned_id(nodes, *child), aligned_id(edges, *edge)))
        .collect();
    let fans: Vec<_> = embedding
        .fans
        .iter()
        .map(|(node, parent, children)| {
            (
                aligned_id(nodes, *node),
                aligned_id(edges, *parent),
                children
                    .iter()
                    .map(|(label, edge)| (label.clone(), aligned_id(edges, *edge)))
                    .collect::<BTreeMap<_, _>>(),
            )
        })
        .collect();
    let next_node = embedding.next_node.max(floor_of(
        arcs.keys()
            .copied()
            .chain(fans.iter().map(|(node, _, _)| *node)),
    ));
    let next_edge = embedding
        .next_edge
        .max(floor_of(arcs.values().copied().chain(
            fans.iter().flat_map(|(_, parent, children)| {
                std::iter::once(*parent).chain(children.values().copied())
            }),
        )));
    TreeEmbedding {
        arcs,
        fans,
        next_node,
        next_edge,
    }
}

fn normalized_graph(
    left: &Graph,
    left_index: &SynthIndex,
    right_index: &SynthIndex,
) -> Option<Graph> {
    let nodes = synthesized_alignment(&left_index.nodes, &right_index.nodes)?;
    let edges = synthesized_alignment(&left_index.edges, &right_index.edges)?;
    if !left_index
        .nodes
        .keys()
        .all(|id| left.nodes.contains_key(id))
        || !left_index
            .edges
            .keys()
            .all(|id| left.edges.contains_key(id))
    {
        return None;
    }
    let mut normalized = Graph::default();
    for (id, node) in &left.nodes {
        let id = aligned_id(&nodes, *id);
        let mut node = node.clone();
        node.id = id;
        if normalized.nodes.insert(id, node).is_some() {
            return None;
        }
    }
    for (id, edge) in &left.edges {
        let id = aligned_id(&edges, *id);
        let mut edge = edge.clone();
        edge.src = aligned_id(&nodes, edge.src);
        edge.tgt = aligned_id(&nodes, edge.tgt);
        if normalized.edges.insert(id, edge).is_some() {
            return None;
        }
    }
    Some(normalized)
}

fn normalized_tree(left: &Tree, left_index: &SynthIndex, right_index: &SynthIndex) -> Option<Tree> {
    let nodes = synthesized_alignment(&left_index.nodes, &right_index.nodes)?;
    synthesized_alignment(&left_index.edges, &right_index.edges)?;
    if !left_index
        .nodes
        .keys()
        .all(|id| left.nodes.contains_key(id))
    {
        return None;
    }
    let mut normalized_nodes = BTreeMap::new();
    for (id, node) in &left.nodes {
        let id = aligned_id(&nodes, *id);
        let mut node = node.clone();
        node.id = id;
        if normalized_nodes.insert(id, node).is_some() {
            return None;
        }
    }
    Some(Tree {
        nodes: normalized_nodes,
        arcs: left
            .arcs
            .iter()
            .map(|(parent, child, label)| {
                (
                    aligned_id(&nodes, *parent),
                    aligned_id(&nodes, *child),
                    label.clone(),
                )
            })
            .collect(),
        fans: left
            .fans
            .iter()
            .map(|fan| {
                let mut fan = fan.clone();
                fan.parent = aligned_id(&nodes, fan.parent);
                for child in fan.children.values_mut() {
                    *child = aligned_id(&nodes, *child);
                }
                fan
            })
            .collect(),
        root: aligned_id(&nodes, left.root),
    })
}

const fn instance_divergence(detail: &'static str) -> Divergence {
    Divergence {
        kind: DivergenceKind::Instance,
        detail,
    }
}

const fn complement_divergence(detail: &'static str) -> Divergence {
    Divergence {
        kind: DivergenceKind::Complement,
        detail,
    }
}

fn graph_complement_divergence(left: &Complement, right: &Complement) -> Option<Divergence> {
    if left.floor != right.floor {
        Some(complement_divergence("id floor differs"))
    } else if left.dropped_nodes != right.dropped_nodes {
        Some(complement_divergence("dropped nodes differ"))
    } else if left.dropped_edges != right.dropped_edges {
        Some(complement_divergence("dropped edges differ"))
    } else if left.original_edges != right.original_edges {
        Some(complement_divergence("original edges differ"))
    } else if left.original_anchors != right.original_anchors {
        Some(complement_divergence("original anchors differ"))
    } else if left.original_fields != right.original_fields {
        Some(complement_divergence("original fields differ"))
    } else if left.original_values != right.original_values {
        Some(complement_divergence("original values differ"))
    } else if left.synthesized_nodes.values().collect::<BTreeSet<_>>()
        != right.synthesized_nodes.values().collect::<BTreeSet<_>>()
    {
        Some(complement_divergence("synthesized nodes differ"))
    } else if left.synthesized_edges.values().collect::<BTreeSet<_>>()
        != right.synthesized_edges.values().collect::<BTreeSet<_>>()
    {
        Some(complement_divergence("synthesized edges differ"))
    } else {
        None
    }
}

/// Exact or observational relation between two successful results.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EqualityClass {
    /// Byte-for-byte model equality.
    Exact,
    /// Declared equivalence without exact presentation equality.
    Equivalent,
    /// Not observationally equivalent.
    Divergent(Divergence),
}

/// Comparison of two result-valued observations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OutcomeClass<R> {
    /// Both calls succeeded with this relation.
    Success(EqualityClass),
    /// Both calls refused with the same typed value.
    EqualRefusal(R),
    /// Only one call refused, or the typed refusals differ.
    RefusalMismatch {
        /// Sequential refusal, when any.
        sequential: Option<R>,
        /// Fused refusal, when any.
        fused: Option<R>,
    },
}

/// Independent comparison of one sequential and one fused execution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FusionObservation<R> {
    /// Forward values or typed refusals.
    pub get: OutcomeClass<R>,
    /// Complement comparison when both gets succeed. This is recorded but
    /// is not a fusion gate: product and compiled complements may differ.
    pub complement: Option<EqualityClass>,
    /// Backward values or typed refusals.
    pub put: OutcomeClass<R>,
}

/// A fusion comparison or a typed reason no fused execution exists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FusionCheck<R, O> {
    /// Both implementations were executed and compared.
    Compared(FusionObservation<R>),
    /// The predicate retained sequential semantics.
    Obstructed(O),
}

/// Compare results produced independently by sequential and fused engines.
#[must_use]
pub fn compare_fusion<S, R>(
    sequential_get: &Result<(S::Instance, S::Complement), R>,
    fused_get: &Result<(S::Instance, S::Complement), R>,
    sequential_put: &Result<S::Instance, R>,
    fused_put: &Result<S::Instance, R>,
) -> FusionObservation<R>
where
    S: LensShape,
    R: Clone + Eq,
{
    let (get, complement) = match (sequential_get, fused_get) {
        (Ok((left_view, left_complement)), Ok((right_view, right_complement))) => (
            OutcomeClass::Success(compare_instances::<S>(
                left_view,
                &S::synth_index(left_view, left_complement),
                right_view,
                &S::synth_index(right_view, right_complement),
            )),
            Some(
                if S::complements_identical(left_complement, right_complement) {
                    EqualityClass::Exact
                } else {
                    S::complement_divergence(left_complement, right_complement)
                        .map_or(EqualityClass::Equivalent, EqualityClass::Divergent)
                },
            ),
        ),
        (Err(left), Err(right)) if left == right => {
            (OutcomeClass::EqualRefusal(left.clone()), None)
        }
        (left, right) => (
            OutcomeClass::RefusalMismatch {
                sequential: left.as_ref().err().cloned(),
                fused: right.as_ref().err().cloned(),
            },
            None,
        ),
    };
    FusionObservation {
        get,
        complement,
        put: compare_instance_results::<S, R>(sequential_put, fused_put),
    }
}

fn compare_instances<S: LensShape>(
    left: &S::Instance,
    left_index: &SynthIndex,
    right: &S::Instance,
    right_index: &SynthIndex,
) -> EqualityClass {
    if S::identical(left, right) {
        EqualityClass::Exact
    } else if S::equivalent(left, left_index, right, right_index) {
        EqualityClass::Equivalent
    } else {
        EqualityClass::Divergent(
            S::divergence(left, left_index, right, right_index)
                .unwrap_or_else(|| instance_divergence("shape equivalence disagrees")),
        )
    }
}

fn compare_instance_results<S, R>(
    sequential: &Result<S::Instance, R>,
    fused: &Result<S::Instance, R>,
) -> OutcomeClass<R>
where
    S: LensShape,
    R: Clone + Eq,
{
    match (sequential, fused) {
        (Ok(left), Ok(right)) => OutcomeClass::Success(compare_instances::<S>(
            left,
            &SynthIndex::default(),
            right,
            &SynthIndex::default(),
        )),
        (Err(left), Err(right)) if left == right => OutcomeClass::EqualRefusal(left.clone()),
        (left, right) => OutcomeClass::RefusalMismatch {
            sequential: left.as_ref().err().cloned(),
            fused: right.as_ref().err().cloned(),
        },
    }
}

/// Strict or weak backward round-trip comparison.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PutGetMode {
    /// Re-read view must equal the edited view under shape equivalence.
    Strict,
    /// Putting the re-read view with the original complement must recover
    /// the first restored source.
    Weak,
}

/// Basis role of one SSB candidate.
///
/// Positive proposals are classified by [`LawShape::check_put_domain`] after
/// generation. Deliberately refused cases instead carry an independently
/// constructed, exact certificate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DomainExpectation<R> {
    /// A positive proposal whose actual domain membership is checked later.
    Admitted,
    /// The candidate must be rejected with this exact typed refusal.
    Refused(R),
}

/// One mutation family in the deterministic Single-Step Basis.
///
/// Declaration order is the contract order used by the basis generators.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Mutation {
    /// Change a node's scalar value.
    ValueEdit,
    /// Change one named field.
    FieldEdit,
    /// Insert a node carrier.
    AddNode,
    /// Remove a node carrier.
    RemoveNode,
    /// Change a node anchor.
    RelabelNode,
    /// Insert an edge carrier.
    AddEdge,
    /// Remove an edge carrier.
    RemoveEdge,
    /// Change an edge endpoint.
    Reconnect,
    /// Change an edge label.
    RelabelEdge,
    /// Change sibling or positioned-edge order.
    Reorder,
    /// Insert a parallel graph edge.
    AddParallelEdge,
    /// Insert a graph self-loop.
    AddSelfLoop,
    /// Remove a synthesized carrier.
    RemoveSynthesized,
    /// Edit a synthesized carrier.
    EditSynthesized,
    /// Insert an edge explained only by contraction.
    AddContractedEdge,
    /// Reuse a complement-reserved id.
    ReservedIdReuse,
    /// Make a graph fan malformed.
    MalformedFan,
}

/// One deterministic SSB candidate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BasisCase<I, R> {
    /// Stable ordinal in the shape's basis.
    pub ordinal: usize,
    /// Mutation family exercised by this candidate.
    pub mutation: Mutation,
    /// Edited view.
    pub view: I,
    /// Positive-proposal or exact-negative role in the basis.
    pub domain: DomainExpectation<R>,
}

/// One coherently reindexed presentation used by the Congruence gate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CongruenceCase<I, C> {
    /// Stable ordinal in the shape's congruence basis.
    pub ordinal: usize,
    /// Equivalent view presentation.
    pub view: I,
    /// Complement reindexed coherently with `view`.
    pub complement: C,
}

/// Execution and finite-basis operations required by the five-gate checker.
pub trait LawShape<L>: LensShape {
    /// Typed operation or domain refusal.
    type Refusal: Clone + Eq;
    /// Shape context.
    type Context;

    /// Forward operation.
    ///
    /// # Errors
    ///
    /// A typed refusal when the source lies outside the operation's domain.
    fn get(
        lens: &L,
        source: &Self::Instance,
        context: &Self::Context,
    ) -> Result<(Self::Instance, Self::Complement), Self::Refusal>;

    /// Backward operation.
    ///
    /// # Errors
    ///
    /// A typed refusal when the view/complement pair lies outside the put domain.
    fn put(
        lens: &L,
        view: &Self::Instance,
        complement: &Self::Complement,
        context: &Self::Context,
    ) -> Result<Self::Instance, Self::Refusal>;

    /// Exact put-domain predicate.
    ///
    /// # Errors
    ///
    /// The exact typed reason the view/complement pair is outside the put domain.
    fn check_put_domain(
        lens: &L,
        view: &Self::Instance,
        complement: &Self::Complement,
        context: &Self::Context,
    ) -> Result<(), Self::Refusal>;

    /// Normalize a complete checker refusal to the exact refusal returned by
    /// the raw, first-error put API.
    ///
    /// Most law adapters use the same refusal for both APIs. Shapes whose
    /// checker reports several violations override this hook.
    fn first_put_refusal(refusal: &Self::Refusal) -> Option<Self::Refusal> {
        Some(refusal.clone())
    }

    /// Deterministic Single-Step Basis in canonical order.
    ///
    /// `Admitted` cases are positive proposals. The checker intersects them
    /// with the exact put domain before running the positive law gates.
    /// `Refused` cases retain their independently designated certificate.
    fn single_step_basis(
        lens: &L,
        source: &Self::Instance,
        view: &Self::Instance,
        complement: &Self::Complement,
        context: &Self::Context,
    ) -> Vec<BasisCase<Self::Instance, Self::Refusal>>;

    /// Contract-selected strict or weak `PutGet` law.
    fn put_get_mode() -> PutGetMode;

    /// Coherently reindex a re-read complement for the edited view used by
    /// the second put of the gated `PutTwice` law.
    fn rekey_complement(
        complement: &Self::Complement,
        observed_view: &Self::Instance,
        edited_view: &Self::Instance,
        producing_complement: &Self::Complement,
    ) -> Self::Complement;

    /// Deterministic coherent-reindexing witnesses for Congruence.
    fn congruence_cases(
        lens: &L,
        view: &Self::Instance,
        complement: &Self::Complement,
        context: &Self::Context,
    ) -> Vec<CongruenceCase<Self::Instance, Self::Complement>>;

    /// First source change outside the exact edit footprint and cascade.
    fn locality_divergence(
        lens: &L,
        source: &Self::Instance,
        original_view: &Self::Instance,
        edited_view: &Self::Instance,
        restored: &Self::Instance,
        complement: &Self::Complement,
        context: &Self::Context,
    ) -> Option<Divergence>;
}

fn reference_lens_error(error: LensError) -> ReferenceLawError {
    match error {
        LensError::Identity(error) => ReferenceLawError::Identity(error),
        LensError::PutDomain(error) => ReferenceLawError::PutDomain(vec![error]),
        LensError::PutDomainReport(errors) => ReferenceLawError::PutDomain(errors),
        LensError::TreeOperational(error) => ReferenceLawError::TreeOperational(error),
    }
}

fn reference_first_put_error(refusal: &ReferenceLawError) -> Option<ReferenceLawError> {
    match refusal {
        ReferenceLawError::PutDomain(errors) => errors
            .first()
            .cloned()
            .map(|error| ReferenceLawError::PutDomain(vec![error])),
        other => Some(other.clone()),
    }
}

fn reindexed_complement(
    complement: &Complement,
    nodes: &BTreeMap<u32, u32>,
    edges: &BTreeMap<u32, u32>,
) -> Complement {
    let mut reindexed = complement.clone();
    reindexed.original_edges = complement
        .original_edges
        .iter()
        .map(|(id, edge)| {
            let mut edge = edge.clone();
            edge.src = aligned_id(nodes, edge.src);
            edge.tgt = aligned_id(nodes, edge.tgt);
            (aligned_id(edges, *id), edge)
        })
        .collect();
    reindexed.original_anchors = complement
        .original_anchors
        .iter()
        .map(|(id, anchor)| (aligned_id(nodes, *id), anchor.clone()))
        .collect();
    reindexed.original_fields = complement
        .original_fields
        .iter()
        .map(|(id, fields)| (aligned_id(nodes, *id), fields.clone()))
        .collect();
    reindexed.original_values = complement
        .original_values
        .iter()
        .map(|(id, value)| (aligned_id(nodes, *id), value.clone()))
        .collect();
    reindexed.synthesized_nodes = complement
        .synthesized_nodes
        .iter()
        .map(|(id, key)| (aligned_id(nodes, *id), key.clone()))
        .collect();
    reindexed.synthesized_edges = complement
        .synthesized_edges
        .iter()
        .map(|(id, key)| (aligned_id(edges, *id), key.clone()))
        .collect();
    reindexed
}

const fn append_rekey_id(next: &mut u32) -> Option<u32> {
    if *next == u32::MAX {
        return None;
    }
    let id = *next;
    *next = next.saturating_add(1);
    Some(id)
}

fn collision_free_tree_rekey(
    complement: &TreeLawComplement,
    nodes: &BTreeMap<u32, u32>,
    edges: &BTreeMap<u32, u32>,
) -> Option<TreeLawComplement> {
    let target_nodes: BTreeSet<_> = nodes.values().copied().collect();
    let target_edges: BTreeSet<_> = edges.values().copied().collect();
    let current_nodes: BTreeSet<_> = complement
        .complement
        .synthesized_nodes
        .keys()
        .copied()
        .collect();
    let current_edges: BTreeSet<_> = complement
        .complement
        .synthesized_edges
        .keys()
        .copied()
        .collect();

    let embedded_edges: BTreeSet<_> = complement
        .embedding
        .arcs
        .values()
        .copied()
        .chain(
            complement
                .embedding
                .fans
                .iter()
                .flat_map(|(_, parent, children)| {
                    std::iter::once(*parent).chain(children.values().copied())
                }),
        )
        .collect();
    let mut next_edge = complement
        .embedding
        .next_edge
        .max(complement.complement.floor.1)
        .max(floor_of(
            embedded_edges
                .iter()
                .copied()
                .chain(complement.complement.dropped_edges.keys().copied())
                .chain(current_edges.iter().copied())
                .chain(target_edges.iter().copied()),
        ));
    let mut edge_moves = BTreeMap::new();
    for id in embedded_edges
        .intersection(&target_edges)
        .filter(|id| !current_edges.contains(id))
    {
        edge_moves.insert(*id, append_rekey_id(&mut next_edge)?);
    }

    let embedded_fan_nodes: BTreeSet<_> = complement
        .embedding
        .fans
        .iter()
        .map(|(node, _, _)| *node)
        .collect();
    let mut next_node = complement
        .embedding
        .next_node
        .max(complement.complement.floor.0)
        .max(floor_of(
            embedded_fan_nodes
                .iter()
                .copied()
                .chain(complement.embedding.arcs.keys().copied())
                .chain(complement.complement.dropped_nodes.keys().copied())
                .chain(current_nodes.iter().copied())
                .chain(target_nodes.iter().copied()),
        ));
    let mut node_moves = BTreeMap::new();
    for id in embedded_fan_nodes
        .intersection(&target_nodes)
        .filter(|id| !current_nodes.contains(id))
    {
        node_moves.insert(*id, append_rekey_id(&mut next_node)?);
    }

    let mut node_alignment = node_moves;
    node_alignment.extend(nodes.iter().map(|(from, to)| (*from, *to)));
    let mut edge_alignment = edge_moves;
    edge_alignment.extend(edges.iter().map(|(from, to)| (*from, *to)));
    let mut embedding =
        reindexed_embedding(&complement.embedding, &node_alignment, &edge_alignment);
    embedding.next_node = embedding.next_node.max(next_node);
    embedding.next_edge = embedding.next_edge.max(next_edge);
    Some(TreeLawComplement {
        complement: reindexed_complement(&complement.complement, &node_alignment, &edge_alignment),
        embedding,
        source_arcs: complement.source_arcs.clone(),
    })
}

fn fresh_alignment(
    synthesized: &BTreeMap<u32, SynthKey>,
    occupied: impl Iterator<Item = u32>,
) -> Option<BTreeMap<u32, u32>> {
    let mut next = floor_of(occupied);
    let mut alignment = BTreeMap::new();
    for id in synthesized.keys() {
        if next == u32::MAX {
            return None;
        }
        alignment.insert(*id, next);
        next = next.saturating_add(1);
    }
    Some(alignment)
}

fn graph_congruence_case(
    view: &Graph,
    complement: &Complement,
) -> Option<CongruenceCase<Graph, Complement>> {
    if complement.synthesized_nodes.is_empty() && complement.synthesized_edges.is_empty() {
        return None;
    }
    let nodes = fresh_alignment(
        &complement.synthesized_nodes,
        view.nodes
            .keys()
            .copied()
            .chain(complement.dropped_nodes.keys().copied()),
    )?;
    let edges = fresh_alignment(
        &complement.synthesized_edges,
        view.edges
            .keys()
            .copied()
            .chain(complement.dropped_edges.keys().copied()),
    )?;
    Some(CongruenceCase {
        ordinal: 0,
        view: normalized_graph(
            view,
            &SynthIndex {
                nodes: complement.synthesized_nodes.clone(),
                edges: complement.synthesized_edges.clone(),
            },
            &SynthIndex {
                nodes: complement
                    .synthesized_nodes
                    .iter()
                    .map(|(id, key)| (aligned_id(&nodes, *id), key.clone()))
                    .collect(),
                edges: complement
                    .synthesized_edges
                    .iter()
                    .map(|(id, key)| (aligned_id(&edges, *id), key.clone()))
                    .collect(),
            },
        )?,
        complement: reindexed_complement(complement, &nodes, &edges),
    })
}

fn alternate_value(value: Option<&Value>) -> Value {
    match value {
        Some(Value::Bool(value)) => Value::Bool(!value),
        _ => Value::Bool(false),
    }
}

fn fresh_graph_node(view: &Graph, complement: &Complement) -> Option<u32> {
    let recorded_edge_endpoints = complement
        .dropped_edges
        .values()
        .chain(complement.original_edges.values())
        .flat_map(|edge| [edge.src, edge.tgt]);
    let occupied = view
        .nodes
        .keys()
        .chain(complement.dropped_nodes.keys())
        .chain(complement.original_anchors.keys())
        .chain(complement.original_fields.keys())
        .chain(complement.original_values.keys())
        .chain(complement.synthesized_nodes.keys())
        .copied()
        .chain(recorded_edge_endpoints);
    let id = complement.floor.0.max(floor_of(occupied));
    (id != u32::MAX).then_some(id)
}

fn fresh_graph_edge(view: &Graph, complement: &Complement) -> Option<u32> {
    let occupied = view
        .edges
        .keys()
        .chain(complement.dropped_edges.keys())
        .chain(complement.original_edges.keys())
        .chain(complement.synthesized_edges.keys())
        .copied();
    let id = complement.floor.1.max(floor_of(occupied));
    (id != u32::MAX).then_some(id)
}

fn push_graph_admitted(
    basis: &mut Vec<BasisCase<Graph, ReferenceLawError>>,
    mutation: Mutation,
    view: Graph,
) {
    basis.push(BasisCase {
        ordinal: basis.len(),
        mutation,
        view,
        domain: DomainExpectation::Admitted,
    });
}

fn push_graph_refused(
    basis: &mut Vec<BasisCase<Graph, ReferenceLawError>>,
    mutation: Mutation,
    view: Graph,
    error: PutDomainError,
) {
    push_graph_refused_report(basis, mutation, view, vec![error]);
}

fn push_graph_refused_report(
    basis: &mut Vec<BasisCase<Graph, ReferenceLawError>>,
    mutation: Mutation,
    view: Graph,
    errors: Vec<PutDomainError>,
) {
    basis.push(BasisCase {
        ordinal: basis.len(),
        mutation,
        view,
        domain: DomainExpectation::Refused(ReferenceLawError::PutDomain(errors)),
    });
}

fn independent_edge_refusal(
    lens: &GraphLens<'_>,
    source_pair: (&str, &str),
    view_label: &crate::model::SEdge,
    edge: u32,
) -> Option<PutDomainError> {
    let preimages = lens
        .plan
        .fates
        .iter()
        .filter(|(source, fate)| {
            source.src == source_pair.0
                && source.tgt == source_pair.1
                && matches!(fate, EdgeFate::Keep(image) if image == view_label)
        })
        .count();
    if preimages == 1 {
        return None;
    }
    if preimages == 0
        && lens
            .plan
            .pairs
            .get(&(source_pair.0.to_owned(), source_pair.1.to_owned()))
            .is_some_and(|pair| pair.resolution == PairResolution::Label(view_label.clone()))
    {
        Some(PutDomainError::ContractedEdgeInserted {
            elem: ViewRef::Edge(edge),
        })
    } else {
        Some(PutDomainError::UnrepresentableViewEdge {
            elem: ViewRef::Edge(edge),
        })
    }
}

fn graph_basis(
    lens: &GraphLens<'_>,
    view: &Graph,
    complement: &Complement,
) -> Vec<BasisCase<Graph, ReferenceLawError>> {
    let mut basis = Vec::new();
    graph_payload_cases(view, complement, &mut basis);
    graph_node_cases(lens, view, complement, &mut basis);
    graph_edge_topology_cases(lens, view, complement, &mut basis);
    graph_edge_presentation_cases(lens, view, complement, &mut basis);
    graph_edge_order_cases(lens, view, complement, &mut basis);
    graph_synthesized_negative_cases(lens, view, complement, &mut basis);
    graph_other_negative_cases(lens, view, complement, &mut basis);
    basis.sort_by_key(|case| (case.mutation, case.ordinal));
    for (ordinal, case) in basis.iter_mut().enumerate() {
        case.ordinal = ordinal;
    }
    basis
}

fn ordinary_graph_nodes<'a>(
    view: &'a Graph,
    complement: &'a Complement,
) -> impl Iterator<Item = (&'a u32, &'a Node)> {
    view.nodes.iter().filter(|(id, node)| {
        !complement.synthesized_nodes.contains_key(id) && node.shape != NodeShape::HyperEdge
    })
}

fn graph_node_touches_synthesized_edge(view: &Graph, complement: &Complement, node: u32) -> bool {
    view.edges.iter().any(|(edge_id, edge)| {
        complement.synthesized_edges.contains_key(edge_id) && (edge.src == node || edge.tgt == node)
    })
}

fn uniquely_representable_anchor(lens: &GraphLens<'_>, anchor: &str) -> bool {
    crate::restrict::reverse_anchor(lens.src, lens.m, anchor).len() == 1
}

fn uniquely_representable_edge(lens: &GraphLens<'_>, label: &crate::model::SEdge) -> bool {
    graph_direct_edge_preimages(lens, label) == 1
}

fn graph_direct_edge_preimages(lens: &GraphLens<'_>, label: &crate::model::SEdge) -> usize {
    lens.plan
        .fates
        .values()
        .filter(|fate| matches!(fate, EdgeFate::Keep(image) if image == label))
        .count()
}

fn graph_relabel_is_structural(view: &Graph, id: u32, anchor: &str) -> bool {
    view.edges.values().all(|edge| {
        (edge.src != id || edge.label.src == anchor) && (edge.tgt != id || edge.label.tgt == anchor)
    })
}

fn edge_label_is_fan_neutral(lens: &GraphLens<'_>, label: &SEdge) -> bool {
    !is_fan_kind(&label.kind)
        && lens.plan.fates.iter().all(|(source, fate)| {
            !matches!(fate, EdgeFate::Keep(image) if image == label) || !is_fan_kind(&source.kind)
        })
}

fn graph_edge_is_fan_neutral(
    lens: &GraphLens<'_>,
    view: &Graph,
    src: u32,
    tgt: u32,
    label: &SEdge,
) -> bool {
    edge_label_is_fan_neutral(lens, label)
        && view
            .nodes
            .get(&src)
            .is_some_and(|node| node.shape != NodeShape::HyperEdge)
        && view
            .nodes
            .get(&tgt)
            .is_some_and(|node| node.shape != NodeShape::HyperEdge)
}

fn graph_payload_cases(
    view: &Graph,
    complement: &Complement,
    basis: &mut Vec<BasisCase<Graph, ReferenceLawError>>,
) {
    for (id, node) in ordinary_graph_nodes(view, complement) {
        let mut edited = view.clone();
        if let Some(edited_node) = edited.nodes.get_mut(id) {
            edited_node.value = Some(Presence::Present(alternate_value(
                node.value.as_ref().and_then(|presence| match presence {
                    Presence::Present(value) => Some(value),
                    Presence::Null | Presence::Absent => None,
                }),
            )));
        }
        push_graph_admitted(basis, Mutation::ValueEdit, edited);

        for (field, value) in &node.fields {
            let mut edited = view.clone();
            if let Some(edited_node) = edited.nodes.get_mut(id) {
                edited_node
                    .fields
                    .insert(field.clone(), alternate_value(Some(value)));
            }
            push_graph_admitted(basis, Mutation::FieldEdit, edited);
        }
    }
}

fn graph_node_cases(
    lens: &GraphLens<'_>,
    view: &Graph,
    complement: &Complement,
    basis: &mut Vec<BasisCase<Graph, ReferenceLawError>>,
) {
    if let Some(id) = fresh_graph_node(view, complement) {
        for anchor in lens
            .tgt
            .vertices
            .keys()
            .filter(|anchor| uniquely_representable_anchor(lens, anchor))
        {
            let mut edited = view.clone();
            edited.nodes.insert(id, Node::new(id, anchor));
            push_graph_admitted(basis, Mutation::AddNode, edited);
        }
    }

    for (id, _) in ordinary_graph_nodes(view, complement)
        .filter(|(id, _)| !graph_node_touches_synthesized_edge(view, complement, **id))
    {
        let mut edited = view.clone();
        edited.nodes.remove(id);
        edited
            .edges
            .retain(|_, edge| edge.src != *id && edge.tgt != *id);
        push_graph_admitted(basis, Mutation::RemoveNode, edited);
    }

    for (id, node) in ordinary_graph_nodes(view, complement)
        .filter(|(id, _)| !graph_node_touches_synthesized_edge(view, complement, **id))
    {
        for anchor in lens.tgt.vertices.keys().filter(|anchor| {
            *anchor != &node.anchor
                && uniquely_representable_anchor(lens, anchor)
                && graph_relabel_is_structural(view, *id, anchor)
        }) {
            let mut edited = view.clone();
            if let Some(edited_node) = edited.nodes.get_mut(id) {
                edited_node.anchor.clone_from(anchor);
            }
            push_graph_admitted(basis, Mutation::RelabelNode, edited);
        }
    }
}

fn graph_edge_topology_cases(
    lens: &GraphLens<'_>,
    view: &Graph,
    complement: &Complement,
    basis: &mut Vec<BasisCase<Graph, ReferenceLawError>>,
) {
    if let Some(edge_id) = fresh_graph_edge(view, complement) {
        for label in lens.tgt.edges.iter().filter(|label| {
            edge_label_is_fan_neutral(lens, label) && uniquely_representable_edge(lens, label)
        }) {
            for (src, _) in view
                .nodes
                .iter()
                .filter(|(_, node)| node.anchor == label.src)
            {
                for (tgt, _) in view
                    .nodes
                    .iter()
                    .filter(|(_, node)| node.anchor == label.tgt)
                {
                    if !graph_edge_is_fan_neutral(lens, view, *src, *tgt, label) {
                        continue;
                    }
                    if view
                        .edges
                        .values()
                        .any(|edge| edge.src == *src && edge.tgt == *tgt && edge.label == *label)
                    {
                        continue;
                    }
                    let mut edited = view.clone();
                    edited.edges.insert(
                        edge_id,
                        GEdge {
                            src: *src,
                            tgt: *tgt,
                            label: label.clone(),
                            position: None,
                        },
                    );
                    push_graph_admitted(basis, Mutation::AddEdge, edited);
                }
            }
        }
    }

    for (id, _) in view.edges.iter().filter(|(id, edge)| {
        !complement.synthesized_edges.contains_key(id)
            && graph_edge_is_fan_neutral(lens, view, edge.src, edge.tgt, &edge.label)
    }) {
        let mut edited = view.clone();
        edited.edges.remove(id);
        push_graph_admitted(basis, Mutation::RemoveEdge, edited);
    }

    for (id, edge) in view.edges.iter().filter(|(id, edge)| {
        !complement.synthesized_edges.contains_key(id)
            && graph_edge_is_fan_neutral(lens, view, edge.src, edge.tgt, &edge.label)
    }) {
        for (source, node) in &view.nodes {
            if *source != edge.src
                && node.anchor == edge.label.src
                && graph_edge_is_fan_neutral(lens, view, *source, edge.tgt, &edge.label)
            {
                let mut edited = view.clone();
                if let Some(edited_edge) = edited.edges.get_mut(id) {
                    edited_edge.src = *source;
                }
                push_graph_admitted(basis, Mutation::Reconnect, edited);
            }
        }
        for (target, node) in &view.nodes {
            if *target != edge.tgt
                && node.anchor == edge.label.tgt
                && graph_edge_is_fan_neutral(lens, view, edge.src, *target, &edge.label)
            {
                let mut edited = view.clone();
                if let Some(edited_edge) = edited.edges.get_mut(id) {
                    edited_edge.tgt = *target;
                }
                push_graph_admitted(basis, Mutation::Reconnect, edited);
            }
        }
    }
}

fn graph_edge_presentation_cases(
    lens: &GraphLens<'_>,
    view: &Graph,
    complement: &Complement,
    basis: &mut Vec<BasisCase<Graph, ReferenceLawError>>,
) {
    for (id, edge) in view
        .edges
        .iter()
        .filter(|(id, _)| !complement.synthesized_edges.contains_key(id))
    {
        for label in lens.tgt.edges.iter().filter(|label| {
            **label != edge.label
                && label.src == edge.label.src
                && label.tgt == edge.label.tgt
                && graph_edge_is_fan_neutral(lens, view, edge.src, edge.tgt, label)
                && uniquely_representable_edge(lens, label)
        }) {
            {
                let mut edited = view.clone();
                if let Some(edited_edge) = edited.edges.get_mut(id) {
                    edited_edge.label = label.clone();
                }
                push_graph_admitted(basis, Mutation::RelabelEdge, edited);
            }
        }
    }
}

fn graph_edge_order_cases(
    lens: &GraphLens<'_>,
    view: &Graph,
    complement: &Complement,
    basis: &mut Vec<BasisCase<Graph, ReferenceLawError>>,
) {
    for (id, edge) in view
        .edges
        .iter()
        .filter(|(id, _)| !complement.synthesized_edges.contains_key(id))
    {
        let mut edited = view.clone();
        if let Some(edited_edge) = edited.edges.get_mut(id) {
            edited_edge.position = Some(edge.position.map_or(0, |p| p ^ 1));
        }
        push_graph_admitted(basis, Mutation::Reorder, edited);
    }

    if let Some(id) = fresh_graph_edge(view, complement) {
        for (_, edge) in view.edges.iter().filter(|(id, edge)| {
            !complement.synthesized_edges.contains_key(id)
                && graph_edge_is_fan_neutral(lens, view, edge.src, edge.tgt, &edge.label)
        }) {
            let mut edited = view.clone();
            edited.edges.insert(id, edge.clone());
            push_graph_admitted(basis, Mutation::AddParallelEdge, edited);
        }
    }

    if let Some(edge_id) = fresh_graph_edge(view, complement) {
        for label in lens.tgt.edges.iter().filter(|label| {
            label.src == label.tgt
                && edge_label_is_fan_neutral(lens, label)
                && uniquely_representable_edge(lens, label)
        }) {
            for (node, _) in view
                .nodes
                .iter()
                .filter(|(_, node)| node.anchor == label.src)
            {
                if !graph_edge_is_fan_neutral(lens, view, *node, *node, label) {
                    continue;
                }
                let mut edited = view.clone();
                edited.edges.insert(
                    edge_id,
                    GEdge {
                        src: *node,
                        tgt: *node,
                        label: label.clone(),
                        position: None,
                    },
                );
                push_graph_admitted(basis, Mutation::AddSelfLoop, edited);
            }
        }
    }
}

fn graph_synthesized_negative_cases(
    lens: &GraphLens<'_>,
    view: &Graph,
    complement: &Complement,
    basis: &mut Vec<BasisCase<Graph, ReferenceLawError>>,
) {
    for id in complement.synthesized_nodes.keys().copied() {
        let mut removed = view.clone();
        removed.nodes.remove(&id);
        let mut errors = vec![PutDomainError::SynthesizedEdited {
            elem: ViewRef::Node(id),
        }];
        errors.extend(removed.edges.iter().filter_map(|(edge, value)| {
            ((value.src == id || value.tgt == id)
                && complement.synthesized_edges.contains_key(edge))
            .then_some(PutDomainError::SynthesizedEdited {
                elem: ViewRef::Edge(*edge),
            })
        }));
        removed
            .edges
            .retain(|_, edge| edge.src != id && edge.tgt != id);
        push_graph_refused_report(basis, Mutation::RemoveSynthesized, removed, errors);
        if view.nodes.contains_key(&id) {
            let mut edited = view.clone();
            if let Some(edited_node) = edited.nodes.get_mut(&id) {
                edited_node.value = Some(Presence::Present(Value::Int(1)));
            }
            push_graph_refused(
                basis,
                Mutation::EditSynthesized,
                edited,
                PutDomainError::SynthesizedEdited {
                    elem: ViewRef::Node(id),
                },
            );
        }
    }
    for id in complement.synthesized_edges.keys().copied() {
        let mut removed = view.clone();
        removed.edges.remove(&id);
        push_graph_refused(
            basis,
            Mutation::RemoveSynthesized,
            removed,
            PutDomainError::SynthesizedEdited {
                elem: ViewRef::Edge(id),
            },
        );

        if let Some(edge) = view.edges.get(&id)
            && let Some(replacement) = lens.tgt.edges.iter().find(|replacement| {
                replacement.src == edge.label.src
                    && replacement.tgt == edge.label.tgt
                    && *replacement != &edge.label
                    && isolated_graph_synthesized_edge_replacement(
                        lens,
                        view,
                        complement,
                        edge.src,
                        edge.tgt,
                        replacement,
                    )
            })
        {
            let mut edited = view.clone();
            if let Some(edited_edge) = edited.edges.get_mut(&id) {
                edited_edge.label.clone_from(replacement);
            }
            push_graph_refused(
                basis,
                Mutation::EditSynthesized,
                edited,
                PutDomainError::SynthesizedEdited {
                    elem: ViewRef::Edge(id),
                },
            );
        }
    }
}

fn graph_source_anchor(
    lens: &GraphLens<'_>,
    view: &Graph,
    complement: &Complement,
    id: u32,
) -> Option<String> {
    let node = view.nodes.get(&id)?;
    if let Some(recorded) = complement.original_anchors.get(&id)
        && lens.m.vertex_image(recorded) == node.anchor
    {
        return Some(recorded.clone());
    }
    let preimages = reverse_anchor(lens.src, lens.m, &node.anchor);
    let [source] = preimages.as_slice() else {
        return None;
    };
    Some(source.clone())
}

fn isolated_graph_synthesized_edge_replacement(
    lens: &GraphLens<'_>,
    view: &Graph,
    complement: &Complement,
    source: u32,
    target: u32,
    replacement: &crate::model::SEdge,
) -> bool {
    if !graph_edge_is_fan_neutral(lens, view, source, target, replacement) {
        return false;
    }
    // Presentation nodes are discarded before source-edge representability
    // is checked, so their incident edge cannot add an independent refusal.
    if complement.synthesized_nodes.contains_key(&source)
        || complement.synthesized_nodes.contains_key(&target)
    {
        return true;
    }
    let (Some(source_anchor), Some(target_anchor)) = (
        graph_source_anchor(lens, view, complement, source),
        graph_source_anchor(lens, view, complement, target),
    ) else {
        return false;
    };
    lens.plan
        .fates
        .iter()
        .filter(|(edge, fate)| {
            edge.src == source_anchor
                && edge.tgt == target_anchor
                && matches!(fate, EdgeFate::Keep(image) if image == replacement)
        })
        .count()
        == 1
}

fn validation_neutral_graph_anchor(lens: &GraphLens<'_>, target: &str) -> bool {
    let preimages = reverse_anchor(lens.src, lens.m, target);
    let [source] = preimages.as_slice() else {
        return false;
    };
    lens.src.vertices.get(source) == lens.tgt.vertices.get(target)
        && lens.src.vertices.contains_key(source)
        && lens.m.conditional.is_empty()
        && lens.m.transformed.is_empty()
        && lens.src.required.values().all(Vec::is_empty)
        && lens.src.constraints.values().all(Vec::is_empty)
        && lens.src.keys.values().all(Vec::is_empty)
        && lens.src.references.is_empty()
}

fn graph_node_restore_is_identity(lens: &GraphLens<'_>, candidate: &Node) -> bool {
    lens.vl
        .restore(candidate, None, None)
        .is_ok_and(|(fields, value)| fields == candidate.fields && value == candidate.value)
}

fn graph_reserved_node_isolated(view: &Graph, complement: &Complement, id: u32) -> bool {
    let incident_edges: BTreeSet<_> = view
        .edges
        .iter()
        .chain(&complement.dropped_edges)
        .chain(&complement.original_edges)
        .filter(|(_, edge)| edge.src == id || edge.tgt == id)
        .map(|(edge, _)| *edge)
        .collect();
    // Original or synthesized state keyed by the id can contribute another
    // refusal. The exact witness predicate separately excludes dropped
    // incident edges, whose endpoint typing or graph-wide effects would need
    // an independent proof.
    !complement
        .original_edges
        .values()
        .any(|edge| edge.src == id || edge.tgt == id)
        && !complement.original_anchors.contains_key(&id)
        && !complement.original_fields.contains_key(&id)
        && !complement.original_values.contains_key(&id)
        && !complement.synthesized_nodes.iter().any(|(node, key)| {
            *node == id
                || match key {
                    SynthKey::Contract { src, tgt, .. }
                    | SynthKey::ContractExpand { src, tgt, .. } => *src == id || *tgt == id,
                    SynthKey::Expand { source_edge, .. } => incident_edges.contains(source_edge),
                }
        })
        && !complement.synthesized_edges.iter().any(|(edge, key)| {
            incident_edges.contains(edge)
                || match key {
                    SynthKey::Contract { src, tgt, .. }
                    | SynthKey::ContractExpand { src, tgt, .. } => *src == id || *tgt == id,
                    SynthKey::Expand { source_edge, .. } => incident_edges.contains(source_edge),
                }
        })
}

fn graph_reserved_node_witness_is_structurally_exact(
    lens: &GraphLens<'_>,
    view: &Graph,
    complement: &Complement,
    candidate: &Node,
) -> bool {
    let id = candidate.id;
    complement
        .dropped_nodes
        .get(&id)
        .is_some_and(|node| node.shape != NodeShape::HyperEdge)
        && !view.nodes.contains_key(&id)
        && view
            .edges
            .values()
            .all(|edge| edge.src != id && edge.tgt != id)
        && complement
            .dropped_edges
            .values()
            .all(|edge| edge.src != id && edge.tgt != id)
        && candidate == &tree_node_for_anchor(id, lens.tgt, &candidate.anchor)
        && validation_neutral_graph_anchor(lens, &candidate.anchor)
        && graph_node_restore_is_identity(lens, candidate)
        && graph_reserved_node_isolated(view, complement, id)
}

fn graph_reserved_edge_isolated(complement: &Complement, id: u32) -> bool {
    !complement.original_edges.contains_key(&id)
        && !complement.synthesized_edges.contains_key(&id)
        && !complement
            .synthesized_nodes
            .values()
            .chain(complement.synthesized_edges.values())
            .any(|key| matches!(key, SynthKey::Expand { source_edge, .. } if *source_edge == id))
}

fn graph_reserved_edge_witness(
    lens: &GraphLens<'_>,
    view: &Graph,
    complement: &Complement,
    id: u32,
    edge: &GEdge,
) -> Option<GEdge> {
    if view.edges.contains_key(&id) || !graph_reserved_edge_isolated(complement, id) {
        return None;
    }
    let (Some(source_node), Some(target_node)) =
        (view.nodes.get(&edge.src), view.nodes.get(&edge.tgt))
    else {
        return None;
    };
    if source_node.shape == NodeShape::HyperEdge
        || target_node.shape == NodeShape::HyperEdge
        || complement.synthesized_nodes.contains_key(&edge.src)
        || complement.synthesized_nodes.contains_key(&edge.tgt)
        || !validation_neutral_graph_anchor(lens, &source_node.anchor)
        || !validation_neutral_graph_anchor(lens, &target_node.anchor)
    {
        return None;
    }
    let (Some(source_anchor), Some(target_anchor)) = (
        graph_source_anchor(lens, view, complement, edge.src),
        graph_source_anchor(lens, view, complement, edge.tgt),
    ) else {
        return None;
    };
    let label = lens.tgt.edges.iter().find(|label| {
        label.src == source_node.anchor
            && label.tgt == target_node.anchor
            && graph_edge_is_fan_neutral(lens, view, edge.src, edge.tgt, label)
            && independent_edge_refusal(lens, (&source_anchor, &target_anchor), label, id).is_none()
    })?;
    Some(GEdge {
        src: edge.src,
        tgt: edge.tgt,
        label: label.clone(),
        position: None,
    })
}

fn graph_other_negative_cases(
    lens: &GraphLens<'_>,
    view: &Graph,
    complement: &Complement,
    basis: &mut Vec<BasisCase<Graph, ReferenceLawError>>,
) {
    graph_contracted_negative_cases(lens, view, complement, basis);
    graph_reserved_negative_cases(lens, view, complement, basis);
    graph_malformed_fan_negative_cases(view, complement, basis);
}

fn graph_contracted_negative_cases(
    lens: &GraphLens<'_>,
    view: &Graph,
    complement: &Complement,
    basis: &mut Vec<BasisCase<Graph, ReferenceLawError>>,
) {
    if let Some(id) = fresh_graph_edge(view, complement) {
        for pair in lens.plan.pairs.values() {
            let PairResolution::Label(edge) = &pair.resolution else {
                continue;
            };
            if graph_direct_edge_preimages(lens, edge) != 0
                || is_fan_kind(&edge.kind)
                || pair.region.iter().any(|source| is_fan_kind(&source.kind))
                || !validation_neutral_graph_anchor(lens, &edge.src)
                || !validation_neutral_graph_anchor(lens, &edge.tgt)
            {
                continue;
            }
            let sources = ordinary_graph_nodes(view, complement)
                .filter(|(_, node)| node.anchor == pair.target.0)
                .map(|(node, _)| *node);
            let targets: Vec<_> = ordinary_graph_nodes(view, complement)
                .filter(|(_, node)| node.anchor == pair.target.1)
                .map(|(node, _)| *node)
                .collect();
            for src in sources {
                for tgt in &targets {
                    let mut edited = view.clone();
                    edited.edges.insert(
                        id,
                        GEdge {
                            src,
                            tgt: *tgt,
                            label: edge.clone(),
                            position: None,
                        },
                    );
                    push_graph_refused(
                        basis,
                        Mutation::AddContractedEdge,
                        edited,
                        PutDomainError::ContractedEdgeInserted {
                            elem: ViewRef::Edge(id),
                        },
                    );
                }
            }
        }
    }
}

fn graph_reserved_negative_cases(
    lens: &GraphLens<'_>,
    view: &Graph,
    complement: &Complement,
    basis: &mut Vec<BasisCase<Graph, ReferenceLawError>>,
) {
    for id in complement.dropped_nodes.keys() {
        let Some(candidate) = lens
            .tgt
            .vertices
            .keys()
            .map(|anchor| tree_node_for_anchor(*id, lens.tgt, anchor))
            .find(|candidate| {
                graph_reserved_node_witness_is_structurally_exact(lens, view, complement, candidate)
            })
        else {
            continue;
        };
        let mut edited = view.clone();
        edited.nodes.insert(*id, candidate);
        push_graph_refused(
            basis,
            Mutation::ReservedIdReuse,
            edited,
            PutDomainError::IdCollision {
                elem: ViewRef::Node(*id),
            },
        );
    }
    for (id, edge) in &complement.dropped_edges {
        let Some(candidate) = graph_reserved_edge_witness(lens, view, complement, *id, edge) else {
            continue;
        };
        let mut edited = view.clone();
        edited.edges.insert(*id, candidate);
        push_graph_refused(
            basis,
            Mutation::ReservedIdReuse,
            edited,
            PutDomainError::IdCollision {
                elem: ViewRef::Edge(*id),
            },
        );
    }
}

fn graph_malformed_fan_negative_cases(
    view: &Graph,
    complement: &Complement,
    basis: &mut Vec<BasisCase<Graph, ReferenceLawError>>,
) {
    for (id, _) in view.nodes.iter().filter(|(id, node)| {
        node.shape == NodeShape::HyperEdge && !complement.synthesized_nodes.contains_key(id)
    }) {
        let Some((_, parent)) = view.edges.iter().find(|(_, value)| {
            value.src == *id && value.label.kind == crate::restrict::HYPER_PARENT
        }) else {
            continue;
        };
        let Some(edge) = fresh_graph_edge(view, complement) else {
            break;
        };
        let mut edited = view.clone();
        edited.edges.insert(edge, parent.clone());
        let mut parents: Vec<_> = edited
            .edges
            .iter()
            .filter(|(_, value)| {
                value.src == *id && value.label.kind == crate::restrict::HYPER_PARENT
            })
            .map(|(edge, _)| *edge)
            .collect();
        parents.sort_unstable();
        push_graph_refused(
            basis,
            Mutation::MalformedFan,
            edited,
            PutDomainError::MalformedFan {
                node: *id,
                reason: FanViolation::MultipleParents { edges: parents },
            },
        );
    }
}

fn graph_changed_carriers(left: &Graph, right: &Graph) -> (BTreeSet<u32>, BTreeSet<u32>) {
    let nodes = left
        .nodes
        .keys()
        .chain(right.nodes.keys())
        .copied()
        .filter(|id| left.nodes.get(id) != right.nodes.get(id))
        .collect();
    let edges = left
        .edges
        .keys()
        .chain(right.edges.keys())
        .copied()
        .filter(|id| left.edges.get(id) != right.edges.get(id))
        .collect();
    (nodes, edges)
}

fn graph_locality_divergence(
    source: &Graph,
    original_view: &Graph,
    edited_view: &Graph,
    restored: &Graph,
    complement: &Complement,
) -> Option<Divergence> {
    let (mut expected_nodes, mut expected_edges) =
        graph_changed_carriers(original_view, edited_view);
    expected_nodes.retain(|id| !complement.synthesized_nodes.contains_key(id));
    expected_edges.retain(|id| !complement.synthesized_edges.contains_key(id));
    let removed_nodes: BTreeSet<_> = original_view
        .nodes
        .keys()
        .filter(|id| {
            !edited_view.nodes.contains_key(id) && !complement.synthesized_nodes.contains_key(id)
        })
        .copied()
        .collect();
    expected_edges.extend(complement.dropped_edges.iter().filter_map(|(id, edge)| {
        (removed_nodes.contains(&edge.src) || removed_nodes.contains(&edge.tgt)).then_some(*id)
    }));
    let (actual_nodes, actual_edges) = graph_changed_carriers(source, restored);
    if expected_nodes != actual_nodes {
        Some(Divergence {
            kind: DivergenceKind::Locality,
            detail: "graph node diff differs from the exact edit footprint",
        })
    } else if expected_edges != actual_edges {
        Some(Divergence {
            kind: DivergenceKind::Locality,
            detail: "graph edge diff differs from the exact edit footprint and cascade",
        })
    } else {
        None
    }
}

impl<'a> LawShape<GraphLens<'a>> for GraphShape {
    type Refusal = ReferenceLawError;
    type Context = ();

    fn get(
        lens: &GraphLens<'a>,
        source: &Graph,
        (): &(),
    ) -> Result<(Graph, Complement), ReferenceLawError> {
        lens.get(source).map_err(ReferenceLawError::Restrict)
    }

    fn put(
        lens: &GraphLens<'a>,
        view: &Graph,
        complement: &Complement,
        (): &(),
    ) -> Result<Graph, ReferenceLawError> {
        lens.put(view, complement).map_err(reference_lens_error)
    }

    fn check_put_domain(
        lens: &GraphLens<'a>,
        view: &Graph,
        complement: &Complement,
        (): &(),
    ) -> Result<(), ReferenceLawError> {
        lens.check_put_domain(view, complement)
            .map_err(ReferenceLawError::PutDomain)
    }

    fn first_put_refusal(refusal: &ReferenceLawError) -> Option<ReferenceLawError> {
        reference_first_put_error(refusal)
    }

    fn single_step_basis(
        lens: &GraphLens<'a>,
        _: &Graph,
        view: &Graph,
        complement: &Complement,
        (): &(),
    ) -> Vec<BasisCase<Graph, ReferenceLawError>> {
        graph_basis(lens, view, complement)
    }

    fn put_get_mode() -> PutGetMode {
        PutGetMode::Strict
    }

    fn rekey_complement(
        complement: &Complement,
        observed_view: &Graph,
        edited_view: &Graph,
        producing_complement: &Complement,
    ) -> Complement {
        let observed_index = GraphShape::synth_index(observed_view, complement);
        let edited_index = GraphShape::synth_index(edited_view, producing_complement);
        let Some(nodes) = synthesized_alignment(&observed_index.nodes, &edited_index.nodes) else {
            return complement.clone();
        };
        let Some(edges) = synthesized_alignment(&observed_index.edges, &edited_index.edges) else {
            return complement.clone();
        };
        reindexed_complement(complement, &nodes, &edges)
    }

    fn congruence_cases(
        _: &GraphLens<'a>,
        view: &Graph,
        complement: &Complement,
        (): &(),
    ) -> Vec<CongruenceCase<Graph, Complement>> {
        graph_congruence_case(view, complement)
            .into_iter()
            .collect()
    }

    fn locality_divergence(
        _: &GraphLens<'a>,
        source: &Graph,
        original_view: &Graph,
        edited_view: &Graph,
        restored: &Graph,
        complement: &Complement,
        (): &(),
    ) -> Option<Divergence> {
        graph_locality_divergence(source, original_view, edited_view, restored, complement)
    }
}

/// A typed refusal of the executable table law adapter.
///
/// Book anchors: `L-lenses-8`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TableLawError {
    /// Forward restriction refused.
    Restrict(RestrictError),
    /// Checked put refused.
    Put(super::table::TablePutError),
    /// The complete put-domain check refused.
    Domain(super::table::TableCheckError),
}

fn table_first_put_error(
    error: &super::table::TableCheckError,
) -> Option<super::table::TablePutError> {
    match error {
        super::table::TableCheckError::Domain {
            violations,
            unresolved_collations,
        } => violations.first().cloned().map_or_else(
            || {
                (!unresolved_collations.is_empty()).then(|| {
                    super::table::TablePutError::NamedCollationUnresolved {
                        names: unresolved_collations.clone(),
                    }
                })
            },
            |error| Some(super::table::TablePutError::Domain(error)),
        ),
        super::table::TableCheckError::NamedCollationUnresolved { names } => {
            Some(super::table::TablePutError::NamedCollationUnresolved {
                names: names.clone(),
            })
        }
        super::table::TableCheckError::Operational(error) => {
            Some(super::table::TablePutError::Operational(error.clone()))
        }
    }
}

/// The table Single-Step Basis: for each view row in ascending id, each
/// target column of its table in ascending name set to the alternate of its
/// cell; one empty row at the least id neither in the view nor reserved, in
/// each target table; and each view row removed with its incident pairs.
fn table_basis(
    lens: &super::table::TableLens<'_>,
    view: &Table,
    complement: &TableComplement,
) -> Vec<BasisCase<Table, TableLawError>> {
    let target = lens.plan().target_boundary();
    let mut basis = Vec::new();
    let mut push = |mutation: Mutation, edited: Table| {
        basis.push(BasisCase {
            ordinal: basis.len(),
            mutation,
            view: edited,
            domain: DomainExpectation::Admitted,
        });
    };
    for (id, (table, cells)) in &view.rows {
        for column in target
            .columns
            .get(table)
            .into_iter()
            .flat_map(BTreeMap::keys)
        {
            let mut edited = view.clone();
            let mut row = cells.clone();
            row.insert(column.clone(), alternate_value(cells.get(column)));
            edited.rows.insert(*id, (table.clone(), row));
            push(Mutation::ValueEdit, edited);
        }
    }
    if let Ok(fresh) = super::table::fresh_view_row_id(view, complement) {
        for table in &target.tables {
            let mut edited = view.clone();
            edited.rows.insert(fresh, (table.clone(), BTreeMap::new()));
            push(Mutation::AddNode, edited);
        }
    }
    for id in view.rows.keys() {
        let mut edited = view.clone();
        edited.rows.remove(id);
        edited.pairs.retain(|(_, s, t)| s != id && t != id);
        push(Mutation::RemoveNode, edited);
    }

    // A named SQL collation is deliberately opaque to the finite reference
    // evaluator. Its exact unresolved set is candidate-local, so the
    // independent basis does not claim an empty set merely from the shape of
    // a structural edit. Named-collation lenses are covered by the mixed
    // refusal oracle below; their structural negative basis remains
    // conservatively empty.
    if !table_source_has_named_collation(lens) {
        table_synthesized_negative_cases(lens, view, complement, &mut basis);
        table_contracted_negative_cases(lens, view, &mut basis);
        table_reserved_negative_cases(lens, view, complement, &mut basis);
    }
    basis.sort_by_key(|case| (case.mutation, case.ordinal));
    for (ordinal, case) in basis.iter_mut().enumerate() {
        case.ordinal = ordinal;
    }
    basis
}

fn push_table_refused(
    basis: &mut Vec<BasisCase<Table, TableLawError>>,
    mutation: Mutation,
    view: Table,
    violations: Vec<PutDomainError>,
    unresolved_collations: BTreeSet<String>,
) {
    basis.push(BasisCase {
        ordinal: basis.len(),
        mutation,
        view,
        domain: DomainExpectation::Refused(TableLawError::Domain(
            super::table::TableCheckError::Domain {
                violations,
                unresolved_collations,
            },
        )),
    });
}

fn table_source_has_named_collation(lens: &super::table::TableLens<'_>) -> bool {
    lens.source_schema().keys.values().flatten().any(|key| {
        key.equality.iter().any(|equality| {
            matches!(
                equality,
                KeyEquality::Sql {
                    collation: SqlCollation::Named(_),
                    ..
                }
            )
        })
    })
}

fn table_direct_pair_preimages(
    lens: &super::table::TableLens<'_>,
    label: &crate::model::SEdge,
) -> usize {
    lens.plan()
        .restrict_plan()
        .fates
        .values()
        .filter(|fate| matches!(fate, EdgeFate::Keep(image) if image == label))
        .count()
}

fn validation_neutral_table_anchor(lens: &super::table::TableLens<'_>, target: &str) -> bool {
    let preimages: Vec<_> = lens
        .plan()
        .functor()
        .tables
        .iter()
        .filter(|(_, image)| image.as_str() == target)
        .map(|(source, _)| source)
        .collect();
    let [source] = preimages.as_slice() else {
        return false;
    };
    let source_schema = lens.source_schema();
    let target_schema = lens.target_schema();
    source_schema.vertices.get(*source) == target_schema.vertices.get(target)
        && source_schema.vertices.contains_key(*source)
        && lens.migration().conditional.is_empty()
        && lens.migration().transformed.is_empty()
        && source_schema.required.values().all(Vec::is_empty)
        && source_schema.constraints.values().all(Vec::is_empty)
        && source_schema.keys.values().all(Vec::is_empty)
        && source_schema.references.is_empty()
}

fn table_pair_endpoints_neutral(
    lens: &super::table::TableLens<'_>,
    label: &crate::model::SEdge,
) -> bool {
    validation_neutral_table_anchor(lens, &label.src)
        && validation_neutral_table_anchor(lens, &label.tgt)
}

fn table_synthesized_negative_cases(
    lens: &super::table::TableLens<'_>,
    view: &Table,
    complement: &TableComplement,
    basis: &mut Vec<BasisCase<Table, TableLawError>>,
) {
    for pair @ (label, source, target) in &complement.synthesized_fk_pairs {
        if !view.pairs.contains(pair) || !table_pair_endpoints_neutral(lens, label) {
            continue;
        }
        let refusal = PutDomainError::SynthesizedEdited {
            elem: ViewRef::Pair(Box::new(label.clone()), *source, *target),
        };
        let mut removed = view.clone();
        removed.pairs.remove(pair);
        push_table_refused(
            basis,
            Mutation::RemoveSynthesized,
            removed,
            vec![refusal.clone()],
            BTreeSet::new(),
        );

        if table_direct_pair_preimages(lens, label) != 1 {
            continue;
        }
        for (other_source, (table, _)) in &view.rows {
            if *other_source == *source || table != &label.src {
                continue;
            }
            let mut edited = view.clone();
            edited.pairs.remove(pair);
            if edited.pairs.insert((label.clone(), *other_source, *target)) {
                push_table_refused(
                    basis,
                    Mutation::EditSynthesized,
                    edited,
                    vec![refusal.clone()],
                    BTreeSet::new(),
                );
            }
        }
        for (other_target, (table, _)) in &view.rows {
            if *other_target == *target || table != &label.tgt {
                continue;
            }
            let mut edited = view.clone();
            edited.pairs.remove(pair);
            if edited.pairs.insert((label.clone(), *source, *other_target)) {
                push_table_refused(
                    basis,
                    Mutation::EditSynthesized,
                    edited,
                    vec![refusal.clone()],
                    BTreeSet::new(),
                );
            }
        }
    }
}

fn table_contracted_negative_cases(
    lens: &super::table::TableLens<'_>,
    view: &Table,
    basis: &mut Vec<BasisCase<Table, TableLawError>>,
) {
    for pair in lens.plan().restrict_plan().pairs.values() {
        let PairResolution::Label(label) = &pair.resolution else {
            continue;
        };
        if table_direct_pair_preimages(lens, label) != 0
            || !table_pair_endpoints_neutral(lens, label)
        {
            continue;
        }
        let sources = view
            .rows
            .iter()
            .filter(|(_, (table, _))| table == &pair.target.0)
            .map(|(id, _)| *id);
        let targets: Vec<_> = view
            .rows
            .iter()
            .filter(|(_, (table, _))| table == &pair.target.1)
            .map(|(id, _)| *id)
            .collect();
        for source in sources {
            for target in &targets {
                let elem = (label.clone(), source, *target);
                if view.pairs.contains(&elem) {
                    continue;
                }
                let mut edited = view.clone();
                edited.pairs.insert(elem);
                push_table_refused(
                    basis,
                    Mutation::AddContractedEdge,
                    edited,
                    vec![PutDomainError::ContractedEdgeInserted {
                        elem: ViewRef::Pair(Box::new(label.clone()), source, *target),
                    }],
                    BTreeSet::new(),
                );
            }
        }
    }
}

fn table_reserved_negative_cases(
    lens: &super::table::TableLens<'_>,
    view: &Table,
    complement: &TableComplement,
    basis: &mut Vec<BasisCase<Table, TableLawError>>,
) {
    let neutral_table = lens
        .plan()
        .target_boundary()
        .tables
        .iter()
        .find(|table| {
            validation_neutral_table_anchor(lens, table.as_str())
                && lens
                    .plan()
                    .target_boundary()
                    .columns
                    .get(*table)
                    .is_none_or(BTreeMap::is_empty)
        })
        .cloned();
    let Some(neutral_table) = neutral_table else {
        return;
    };
    for id in complement.dropped_rows.keys() {
        let residue_isolated = !complement.dropped_cells.keys().any(|(row, _)| row == id)
            && !complement
                .dropped_fk_pairs
                .values()
                .flatten()
                .any(|(source, target)| source == id || target == id)
            && !complement
                .original_fk_edges
                .keys()
                .any(|(_, source, target)| source == id || target == id)
            && !complement.original_tables.contains_key(id)
            && !complement.original_cells.keys().any(|(row, _)| row == id)
            && !complement
                .synthesized_fk_pairs
                .iter()
                .any(|(_, source, target)| source == id || target == id);
        if view.rows.contains_key(id) || !residue_isolated {
            continue;
        }
        let mut edited = view.clone();
        edited
            .rows
            .insert(*id, (neutral_table.clone(), BTreeMap::new()));
        push_table_refused(
            basis,
            Mutation::ReservedIdReuse,
            edited,
            vec![PutDomainError::IdCollision {
                elem: ViewRef::Row(*id),
            }],
            BTreeSet::new(),
        );
    }
}

impl<'a> LawShape<super::table::TableLens<'a>> for TableShape {
    type Refusal = TableLawError;
    type Context = ();

    fn get(
        lens: &super::table::TableLens<'a>,
        source: &Table,
        (): &(),
    ) -> Result<(Table, TableComplement), TableLawError> {
        lens.get(source).map_err(TableLawError::Restrict)
    }

    fn put(
        lens: &super::table::TableLens<'a>,
        view: &Table,
        complement: &TableComplement,
        (): &(),
    ) -> Result<Table, TableLawError> {
        super::table::put(lens, view, complement).map_err(TableLawError::Put)
    }

    fn check_put_domain(
        lens: &super::table::TableLens<'a>,
        view: &Table,
        complement: &TableComplement,
        (): &(),
    ) -> Result<(), TableLawError> {
        super::table::check_put_domain(lens, view, complement).map_err(TableLawError::Domain)
    }

    fn first_put_refusal(refusal: &TableLawError) -> Option<TableLawError> {
        match refusal {
            TableLawError::Domain(error) => table_first_put_error(error).map(TableLawError::Put),
            other => Some(other.clone()),
        }
    }

    fn single_step_basis(
        lens: &super::table::TableLens<'a>,
        _: &Table,
        view: &Table,
        complement: &TableComplement,
        (): &(),
    ) -> Vec<BasisCase<Table, TableLawError>> {
        table_basis(lens, view, complement)
    }

    fn put_get_mode() -> PutGetMode {
        PutGetMode::Strict
    }

    fn rekey_complement(
        complement: &TableComplement,
        _: &Table,
        _: &Table,
        _: &TableComplement,
    ) -> TableComplement {
        complement.clone()
    }

    fn congruence_cases(
        _: &super::table::TableLens<'a>,
        _: &Table,
        _: &TableComplement,
        (): &(),
    ) -> Vec<CongruenceCase<Table, TableComplement>> {
        Vec::new()
    }

    fn locality_divergence(
        _: &super::table::TableLens<'a>,
        _: &Table,
        _: &Table,
        _: &Table,
        _: &Table,
        _: &TableComplement,
        (): &(),
    ) -> Option<Divergence> {
        None
    }
}

/// Definition: the five law gates of the table lens, over the table
/// Single-Step Basis (a value edit per row and target column, one fresh row
/// per target table, each row removed).
///
/// Book anchors: `L-lenses-8`.
#[must_use]
pub fn check_table_laws(
    lens: &super::table::TableLens<'_>,
    source: &Table,
) -> LawReport<TableLawError> {
    check_laws_report::<TableShape, _>(lens, source, &())
}

fn tree_congruence_case(
    view: &Tree,
    complement: &TreeLawComplement,
) -> Option<CongruenceCase<Tree, TreeLawComplement>> {
    let graph = &complement.complement;
    if graph.synthesized_nodes.is_empty() && graph.synthesized_edges.is_empty() {
        return None;
    }
    let nodes = fresh_alignment(
        &graph.synthesized_nodes,
        view.nodes
            .keys()
            .copied()
            .chain(graph.dropped_nodes.keys().copied()),
    )?;
    let edges = fresh_alignment(
        &graph.synthesized_edges,
        complement
            .embedding
            .arcs
            .values()
            .copied()
            .chain(graph.dropped_edges.keys().copied()),
    )?;
    let left_index = tree_synth_index(complement);
    let right_index = SynthIndex {
        nodes: left_index
            .nodes
            .iter()
            .map(|(id, key)| (aligned_id(&nodes, *id), key.clone()))
            .collect(),
        edges: left_index
            .edges
            .iter()
            .map(|(id, key)| (aligned_id(&edges, *id), key.clone()))
            .collect(),
    };
    Some(CongruenceCase {
        ordinal: 0,
        view: normalized_tree(view, &left_index, &right_index)?,
        complement: TreeLawComplement {
            complement: reindexed_complement(graph, &nodes, &edges),
            embedding: reindexed_embedding(&complement.embedding, &nodes, &edges),
            source_arcs: complement.source_arcs.clone(),
        },
    })
}

fn push_tree_admitted(
    basis: &mut Vec<BasisCase<Tree, ReferenceLawError>>,
    mutation: Mutation,
    view: Tree,
) {
    basis.push(BasisCase {
        ordinal: basis.len(),
        mutation,
        view,
        domain: DomainExpectation::Admitted,
    });
}

fn push_tree_refused(
    basis: &mut Vec<BasisCase<Tree, ReferenceLawError>>,
    mutation: Mutation,
    view: Tree,
    error: PutDomainError,
) {
    basis.push(BasisCase {
        ordinal: basis.len(),
        mutation,
        view,
        domain: DomainExpectation::Refused(ReferenceLawError::PutDomain(vec![error])),
    });
}

fn tree_basis(
    lens: &GraphLens<'_>,
    view: &Tree,
    complement: &TreeLawComplement,
) -> Vec<BasisCase<Tree, ReferenceLawError>> {
    let mut basis = Vec::new();
    tree_payload_cases(view, complement, &mut basis);
    tree_node_cases(lens, view, complement, &mut basis);
    tree_edge_cases(lens, view, complement, &mut basis);
    tree_synthesized_negative_cases(lens, view, complement, &mut basis);
    tree_other_negative_cases(lens, view, complement, &mut basis);
    basis.sort_by_key(|case| (case.mutation, case.ordinal));
    for (ordinal, case) in basis.iter_mut().enumerate() {
        case.ordinal = ordinal;
    }
    basis
}

fn ordinary_tree_nodes<'a>(
    view: &'a Tree,
    complement: &'a TreeLawComplement,
) -> impl Iterator<Item = (&'a u32, &'a Node)> {
    let graph = &complement.complement;
    view.nodes.iter().filter(|(id, node)| {
        !graph.synthesized_nodes.contains_key(id) && node.shape != NodeShape::HyperEdge
    })
}

fn fresh_tree_node(view: &Tree, complement: &TreeLawComplement) -> Option<u32> {
    let graph = &complement.complement;
    let recorded_edge_endpoints = graph
        .dropped_edges
        .values()
        .chain(graph.original_edges.values())
        .flat_map(|edge| [edge.src, edge.tgt]);
    let occupied = view
        .nodes
        .keys()
        .chain(graph.dropped_nodes.keys())
        .chain(graph.original_anchors.keys())
        .chain(graph.original_fields.keys())
        .chain(graph.original_values.keys())
        .chain(graph.synthesized_nodes.keys())
        .copied()
        .chain(complement.embedding.fans.iter().map(|(node, _, _)| *node))
        .chain(recorded_edge_endpoints);
    let id = graph
        .floor
        .0
        .max(complement.embedding.next_node)
        .max(floor_of(occupied));
    (id != u32::MAX).then_some(id)
}

fn tree_node_for_anchor(id: u32, schema: &crate::schema::Schema, anchor: &str) -> Node {
    let node = Node::new(id, anchor);
    let value = schema
        .vertices
        .get(anchor)
        .and_then(|kind| match kind.as_str() {
            "string" => Some(Value::Str(String::new())),
            "integer" | "int" => Some(Value::Int(0)),
            "number" | "float" => Some(Value::Float(0.0_f64.to_bits())),
            "boolean" | "bool" => Some(Value::Bool(false)),
            _ => None,
        });
    match value {
        Some(value) => node.with_value(value),
        None => node,
    }
}

fn validation_neutral_tree_anchor(lens: &GraphLens<'_>, target: &str) -> bool {
    let preimages = reverse_anchor(lens.src, lens.m, target);
    let [source] = preimages.as_slice() else {
        return false;
    };
    lens.src.vertices.get(source) == lens.tgt.vertices.get(target)
        && lens.src.vertices.contains_key(source)
        && lens.m.conditional.is_empty()
        && lens.m.transformed.is_empty()
        && lens.src.required.values().all(Vec::is_empty)
        && lens.src.constraints.values().all(Vec::is_empty)
        && lens.src.keys.values().all(Vec::is_empty)
        && lens.src.references.is_empty()
}

fn tree_arc_is_fan_neutral(
    lens: &GraphLens<'_>,
    view: &Tree,
    parent: u32,
    child: u32,
    label: &SEdge,
) -> bool {
    edge_label_is_fan_neutral(lens, label)
        && view
            .nodes
            .get(&parent)
            .is_some_and(|node| node.shape != NodeShape::HyperEdge)
        && view
            .nodes
            .get(&child)
            .is_some_and(|node| node.shape != NodeShape::HyperEdge)
}

fn tree_source_anchor(
    lens: &GraphLens<'_>,
    view: &Tree,
    complement: &TreeLawComplement,
    id: u32,
) -> Option<String> {
    let node = view.nodes.get(&id)?;
    if let Some(recorded) = complement.complement.original_anchors.get(&id)
        && lens.m.vertex_image(recorded) == node.anchor
    {
        return Some(recorded.clone());
    }
    let preimages = reverse_anchor(lens.src, lens.m, &node.anchor);
    let [source] = preimages.as_slice() else {
        return None;
    };
    Some(source.clone())
}

fn isolated_tree_attachment(
    lens: &GraphLens<'_>,
    view: &Tree,
    complement: &TreeLawComplement,
    parent: u32,
    child: u32,
) -> Option<crate::model::SEdge> {
    let graph = &complement.complement;
    if graph.synthesized_nodes.contains_key(&parent)
        || view
            .nodes
            .get(&parent)
            .is_none_or(|node| node.shape == NodeShape::HyperEdge)
    {
        return None;
    }
    let node = view.nodes.get(&parent)?;
    let parent_anchor = tree_source_anchor(lens, view, complement, parent)?;
    for prefer_unique in [true, false] {
        for label in &lens.tgt.edges {
            if uniquely_representable_edge(lens, label) != prefer_unique
                || !edge_label_is_fan_neutral(lens, label)
                || !validation_neutral_tree_anchor(lens, &label.src)
                || !validation_neutral_tree_anchor(lens, &label.tgt)
                || !graph_node_restore_is_identity(
                    lens,
                    &tree_node_for_anchor(child, lens.tgt, &label.tgt),
                )
            {
                continue;
            }
            let child_anchors = reverse_anchor(lens.src, lens.m, &label.tgt);
            let [child_anchor] = child_anchors.as_slice() else {
                continue;
            };
            if node.anchor == label.src {
                let preimages = lens
                    .plan
                    .fates()
                    .iter()
                    .filter(|(edge, fate)| {
                        edge.src == parent_anchor
                            && edge.tgt == *child_anchor
                            && matches!(fate, EdgeFate::Keep(image) if image == label)
                    })
                    .count();
                if preimages == 1 {
                    return Some(label.clone());
                }
            }
        }
    }
    None
}

fn isolated_reserved_tree_parent(complement: &TreeLawComplement, id: u32) -> Option<u32> {
    let graph = &complement.complement;
    let mut incident = graph
        .dropped_edges
        .values()
        .filter(|edge| edge.src == id || edge.tgt == id);
    let incoming = incident.next()?;
    if incoming.tgt != id
        || incident.next().is_some()
        || graph
            .original_edges
            .values()
            .any(|edge| edge.src == id || edge.tgt == id)
        || graph.original_anchors.contains_key(&id)
        || graph.original_fields.contains_key(&id)
        || graph.original_values.contains_key(&id)
    {
        return None;
    }
    Some(incoming.src)
}

fn isolated_tree_synthesized_edge_replacement(
    lens: &GraphLens<'_>,
    view: &Tree,
    complement: &TreeLawComplement,
    parent: u32,
    child: u32,
    replacement: &crate::model::SEdge,
) -> bool {
    if !tree_arc_is_fan_neutral(lens, view, parent, child, replacement) {
        return false;
    }
    let graph = &complement.complement;
    // Presentation nodes are discarded before source-edge representability
    // is checked, so an incident arc cannot add an independent refusal.
    if graph.synthesized_nodes.contains_key(&parent) || graph.synthesized_nodes.contains_key(&child)
    {
        return true;
    }
    let (Some(source), Some(target)) = (
        tree_source_anchor(lens, view, complement, parent),
        tree_source_anchor(lens, view, complement, child),
    ) else {
        return false;
    };
    lens.plan
        .fates()
        .iter()
        .filter(|(edge, fate)| {
            edge.src == source
                && edge.tgt == target
                && matches!(fate, EdgeFate::Keep(image) if image == replacement)
        })
        .count()
        == 1
}

fn fresh_tree_edge(complement: &TreeLawComplement) -> Option<u32> {
    let graph = &complement.complement;
    let fan_edges = complement
        .embedding
        .fans
        .iter()
        .flat_map(|(_, parent, children)| {
            std::iter::once(*parent).chain(children.values().copied())
        });
    let occupied = complement
        .embedding
        .arcs
        .values()
        .copied()
        .chain(graph.dropped_edges.keys().copied())
        .chain(graph.synthesized_edges.keys().copied())
        .chain(fan_edges);
    let id = graph
        .floor
        .1
        .max(complement.embedding.next_edge)
        .max(floor_of(occupied));
    (id != u32::MAX).then_some(id)
}

fn first_synthesized_tree_arc(view: &Tree, complement: &TreeLawComplement) -> usize {
    view.arcs
        .iter()
        .position(|(_, child, _)| {
            complement
                .embedding
                .arcs
                .get(child)
                .is_some_and(|edge| complement.complement.synthesized_edges.contains_key(edge))
        })
        .unwrap_or(view.arcs.len())
}

fn tree_node_touches_synthesized_arc(
    view: &Tree,
    complement: &TreeLawComplement,
    node: u32,
) -> bool {
    view.arcs.iter().any(|(parent, child, _)| {
        (*parent == node || *child == node)
            && complement
                .embedding
                .arcs
                .get(child)
                .is_some_and(|edge| complement.complement.synthesized_edges.contains_key(edge))
    })
}

fn tree_payload_cases(
    view: &Tree,
    complement: &TreeLawComplement,
    basis: &mut Vec<BasisCase<Tree, ReferenceLawError>>,
) {
    for (id, node) in ordinary_tree_nodes(view, complement) {
        let mut edited = view.clone();
        if let Some(edited_node) = edited.nodes.get_mut(id) {
            edited_node.value = Some(Presence::Present(alternate_value(
                node.value.as_ref().and_then(|presence| match presence {
                    Presence::Present(value) => Some(value),
                    Presence::Null | Presence::Absent => None,
                }),
            )));
        }
        push_tree_admitted(basis, Mutation::ValueEdit, edited);

        for (field, value) in &node.fields {
            let mut edited = view.clone();
            if let Some(edited_node) = edited.nodes.get_mut(id) {
                edited_node
                    .fields
                    .insert(field.clone(), alternate_value(Some(value)));
            }
            push_tree_admitted(basis, Mutation::FieldEdit, edited);
        }
    }
}

fn tree_relabel_is_structural(view: &Tree, id: u32, anchor: &str) -> bool {
    view.arcs.iter().all(|(parent, child, label)| {
        (*parent != id || label.src == anchor) && (*child != id || label.tgt == anchor)
    })
}

fn tree_is_descendant(view: &Tree, ancestor: u32, candidate: u32) -> bool {
    let mut frontier = vec![ancestor];
    while let Some(parent) = frontier.pop() {
        for (_, child, _) in view.arcs.iter().filter(|(from, _, _)| *from == parent) {
            if *child == candidate {
                return true;
            }
            frontier.push(*child);
        }
    }
    false
}

fn tree_node_cases(
    lens: &GraphLens<'_>,
    view: &Tree,
    complement: &TreeLawComplement,
    basis: &mut Vec<BasisCase<Tree, ReferenceLawError>>,
) {
    let fresh_node = fresh_tree_node(view, complement);
    if let Some(id) = fresh_node {
        for label in lens.tgt.edges.iter().filter(|label| {
            edge_label_is_fan_neutral(lens, label) && uniquely_representable_edge(lens, label)
        }) {
            for (parent, _) in view
                .nodes
                .iter()
                .filter(|(_, node)| node.anchor == label.src)
            {
                let mut edited = view.clone();
                edited
                    .nodes
                    .insert(id, tree_node_for_anchor(id, lens.tgt, &label.tgt));
                if !tree_arc_is_fan_neutral(lens, &edited, *parent, id, label) {
                    continue;
                }
                let insertion = edited
                    .arcs
                    .iter()
                    .position(|(_, child, _)| {
                        complement.embedding.arcs.get(child).is_some_and(|edge| {
                            complement.complement.synthesized_edges.contains_key(edge)
                        })
                    })
                    .unwrap_or(edited.arcs.len());
                edited.arcs.insert(insertion, (*parent, id, label.clone()));
                push_tree_admitted(basis, Mutation::AddNode, edited.clone());
                push_tree_admitted(basis, Mutation::AddEdge, edited);
            }
        }
    }

    for (id, _) in ordinary_tree_nodes(view, complement).filter(|(id, _)| {
        **id != view.root
            && !view.arcs.iter().any(|(parent, _, _)| parent == *id)
            && !tree_node_touches_synthesized_arc(view, complement, **id)
            && !view
                .fans
                .iter()
                .any(|fan| fan.parent == **id || fan.children.values().any(|child| child == *id))
    }) {
        let mut edited = view.clone();
        edited.nodes.remove(id);
        edited.arcs.retain(|(_, child, _)| child != id);
        push_tree_admitted(basis, Mutation::RemoveNode, edited.clone());
        push_tree_admitted(basis, Mutation::RemoveEdge, edited);
    }

    for (id, node) in ordinary_tree_nodes(view, complement)
        .filter(|(id, _)| !tree_node_touches_synthesized_arc(view, complement, **id))
    {
        for anchor in lens.tgt.vertices.keys().filter(|anchor| {
            *anchor != &node.anchor
                && uniquely_representable_anchor(lens, anchor)
                && tree_relabel_is_structural(view, *id, anchor)
        }) {
            let mut edited = view.clone();
            if let Some(edited_node) = edited.nodes.get_mut(id) {
                edited_node.anchor.clone_from(anchor);
            }
            push_tree_admitted(basis, Mutation::RelabelNode, edited);
        }
    }
}

fn tree_edge_cases(
    lens: &GraphLens<'_>,
    view: &Tree,
    complement: &TreeLawComplement,
    basis: &mut Vec<BasisCase<Tree, ReferenceLawError>>,
) {
    let graph = &complement.complement;
    for (index, (parent, child, label)) in view.arcs.iter().enumerate() {
        let edge = complement.embedding.arcs.get(child);
        if edge.is_some_and(|edge| graph.synthesized_edges.contains_key(edge))
            || !tree_arc_is_fan_neutral(lens, view, *parent, *child, label)
        {
            continue;
        }
        for (next_parent, node) in &view.nodes {
            let distinct_carriers = next_parent != parent && next_parent != child;
            let compatible_anchor = node.anchor == label.src;
            let preserves_tree = !tree_is_descendant(view, *child, *next_parent);
            if distinct_carriers && compatible_anchor && preserves_tree {
                let mut edited = view.clone();
                edited.arcs[index].0 = *next_parent;
                push_tree_admitted(basis, Mutation::Reconnect, edited);
            }
        }
    }

    for (index, (_, child, label)) in view.arcs.iter().enumerate() {
        let edge = complement.embedding.arcs.get(child);
        if edge.is_some_and(|edge| graph.synthesized_edges.contains_key(edge)) {
            continue;
        }
        for replacement in lens.tgt.edges.iter().filter(|replacement| {
            *replacement != label
                && replacement.src == label.src
                && replacement.tgt == label.tgt
                && tree_arc_is_fan_neutral(lens, view, view.arcs[index].0, *child, replacement)
                && uniquely_representable_edge(lens, replacement)
        }) {
            {
                let mut edited = view.clone();
                edited.arcs[index].2 = replacement.clone();
                push_tree_admitted(basis, Mutation::RelabelEdge, edited);
            }
        }
    }

    let movable: BTreeSet<_> = view
        .arcs
        .iter()
        .enumerate()
        .filter(|(_, (_, child, _))| {
            complement
                .embedding
                .arcs
                .get(child)
                .is_none_or(|edge| !graph.synthesized_edges.contains_key(edge))
        })
        .map(|(index, _)| index)
        .collect();
    let mut by_parent: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
    for (index, (parent, _, _)) in view.arcs.iter().enumerate() {
        by_parent.entry(*parent).or_default().push(index);
    }
    for siblings in by_parent.values() {
        for pair in siblings.windows(2) {
            let [left, right] = pair else {
                continue;
            };
            if !(movable.contains(left) && movable.contains(right)) {
                continue;
            }
            let mut edited = view.clone();
            edited.arcs.swap(*left, *right);
            push_tree_admitted(basis, Mutation::Reorder, edited);
        }
    }
}

fn tree_synthesized_negative_cases(
    lens: &GraphLens<'_>,
    view: &Tree,
    complement: &TreeLawComplement,
    basis: &mut Vec<BasisCase<Tree, ReferenceLawError>>,
) {
    let graph = &complement.complement;
    for id in graph.synthesized_edges.keys().copied() {
        let Some(child) = complement
            .embedding
            .arcs
            .iter()
            .find_map(|(child, edge)| (*edge == id).then_some(*child))
        else {
            continue;
        };
        let Some(index) = view
            .arcs
            .iter()
            .position(|(_, candidate, _)| *candidate == child)
        else {
            continue;
        };
        let parent = view.arcs[index].0;
        let mut removed = view.clone();
        removed.arcs.retain(|(_, candidate, _)| *candidate != child);
        push_tree_refused(
            basis,
            Mutation::RemoveSynthesized,
            removed,
            PutDomainError::SynthesizedEdited {
                elem: ViewRef::Arc { parent, child },
            },
        );
        {
            let original = &view.arcs[index].2;
            // A different target edge with a unique source preimage is the
            // only structural edit known to isolate this synthesized edge.
            // Without one, there is no sound one-coordinate witness to emit.
            if let Some(replacement) = lens.tgt.edges.iter().find(|replacement| {
                replacement.src == original.src
                    && replacement.tgt == original.tgt
                    && *replacement != original
                    && isolated_tree_synthesized_edge_replacement(
                        lens,
                        view,
                        complement,
                        parent,
                        child,
                        replacement,
                    )
            }) {
                let mut edited = view.clone();
                edited.arcs[index].2.clone_from(replacement);
                push_tree_refused(
                    basis,
                    Mutation::EditSynthesized,
                    edited,
                    PutDomainError::SynthesizedEdited {
                        elem: ViewRef::Arc { parent, child },
                    },
                );
            }
        }
    }
    for id in graph
        .synthesized_nodes
        .keys()
        .copied()
        .filter(|id| view.nodes.contains_key(id))
    {
        let mut removed = view.clone();
        removed.nodes.remove(&id);
        push_tree_refused(
            basis,
            Mutation::RemoveSynthesized,
            removed,
            PutDomainError::SynthesizedEdited {
                elem: ViewRef::Node(id),
            },
        );
        let mut edited = view.clone();
        if let Some(node) = edited.nodes.get_mut(&id) {
            node.value = Some(Presence::Present(Value::Int(1)));
        }
        push_tree_refused(
            basis,
            Mutation::EditSynthesized,
            edited,
            PutDomainError::SynthesizedEdited {
                elem: ViewRef::Node(id),
            },
        );
    }
}

fn tree_other_negative_cases(
    lens: &GraphLens<'_>,
    view: &Tree,
    complement: &TreeLawComplement,
    basis: &mut Vec<BasisCase<Tree, ReferenceLawError>>,
) {
    let graph = &complement.complement;
    let next_edge = fresh_tree_edge(complement);
    if let (Some(id), Some(_)) = (fresh_tree_node(view, complement), next_edge) {
        for (_, key) in graph
            .synthesized_edges
            .iter()
            .filter(|(_, key)| matches!(key, SynthKey::Contract { .. }))
        {
            let SynthKey::Contract { src, edge, .. } = key else {
                unreachable!();
            };
            if lens
                .plan
                .fates()
                .values()
                .any(|fate| matches!(fate, EdgeFate::Keep(image) if image == edge))
                || is_fan_kind(&edge.kind)
                || lens
                    .src
                    .edges
                    .iter()
                    .any(|source| is_fan_kind(&source.kind))
                || view
                    .nodes
                    .get(src)
                    .is_none_or(|node| node.shape == NodeShape::HyperEdge)
                || !validation_neutral_tree_anchor(lens, &edge.src)
                || !validation_neutral_tree_anchor(lens, &edge.tgt)
                || !graph_node_restore_is_identity(
                    lens,
                    &tree_node_for_anchor(id, lens.tgt, &edge.tgt),
                )
            {
                continue;
            }
            let mut edited = view.clone();
            edited
                .nodes
                .insert(id, tree_node_for_anchor(id, lens.tgt, &edge.tgt));
            let insertion = first_synthesized_tree_arc(&edited, complement);
            edited.arcs.insert(insertion, (*src, id, edge.clone()));
            push_tree_refused(
                basis,
                Mutation::AddContractedEdge,
                edited,
                PutDomainError::ContractedEdgeInserted {
                    elem: ViewRef::Arc {
                        parent: *src,
                        child: id,
                    },
                },
            );
        }
    }

    for (id, _) in graph.dropped_nodes.iter().filter(|(id, node)| {
        !view.nodes.contains_key(id)
            && node.shape != NodeShape::HyperEdge
            && !complement
                .embedding
                .fans
                .iter()
                .any(|(fan, _, _)| fan == *id)
    }) {
        if next_edge.is_none() {
            break;
        }
        let Some(parent) = isolated_reserved_tree_parent(complement, *id) else {
            continue;
        };
        let Some(label) = isolated_tree_attachment(lens, view, complement, parent, *id) else {
            continue;
        };
        let mut edited = view.clone();
        edited
            .nodes
            .insert(*id, tree_node_for_anchor(*id, lens.tgt, &label.tgt));
        let insertion = first_synthesized_tree_arc(&edited, complement);
        edited.arcs.insert(insertion, (parent, *id, label));
        push_tree_refused(
            basis,
            Mutation::ReservedIdReuse,
            edited,
            PutDomainError::IdCollision {
                elem: ViewRef::Node(*id),
            },
        );
    }
}

type TreeArcCarrier = (u32, u32);

struct TreeCarrierDiff {
    nodes: BTreeSet<u32>,
    arcs: BTreeSet<TreeArcCarrier>,
    order: BTreeMap<u32, (Vec<u32>, Vec<u32>)>,
    fans: bool,
}

fn tree_order(tree: &Tree, children: impl Fn(u32) -> bool) -> BTreeMap<u32, Vec<u32>> {
    let mut order: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for (parent, child, _) in &tree.arcs {
        if children(*child) {
            order.entry(*parent).or_default().push(*child);
        }
    }
    order
}

fn tree_order_diff(
    left: &Tree,
    right: &Tree,
    children: impl Fn(u32) -> bool + Copy,
) -> BTreeMap<u32, (Vec<u32>, Vec<u32>)> {
    let left = tree_order(left, children);
    let right = tree_order(right, children);
    left.keys()
        .chain(right.keys())
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .filter_map(|parent| {
            let before = left.get(&parent).cloned().unwrap_or_default();
            let after = right.get(&parent).cloned().unwrap_or_default();
            (before != after).then_some((parent, (before, after)))
        })
        .collect()
}

fn tree_changed_carriers(left: &Tree, right: &Tree) -> TreeCarrierDiff {
    let nodes = left
        .nodes
        .keys()
        .chain(right.nodes.keys())
        .copied()
        .filter(|id| left.nodes.get(id) != right.nodes.get(id))
        .collect();
    let left_arcs: BTreeMap<_, _> = left
        .arcs
        .iter()
        .map(|(parent, child, label)| ((*parent, *child), label))
        .collect();
    let right_arcs: BTreeMap<_, _> = right
        .arcs
        .iter()
        .map(|(parent, child, label)| ((*parent, *child), label))
        .collect();
    let arcs = left_arcs
        .keys()
        .chain(right_arcs.keys())
        .copied()
        .filter(|carrier| left_arcs.get(carrier) != right_arcs.get(carrier))
        .collect();
    TreeCarrierDiff {
        nodes,
        arcs,
        order: tree_order_diff(left, right, |_| true),
        fans: left.fans != right.fans,
    }
}

fn tree_locality_divergence(
    source: &Tree,
    original_view: &Tree,
    edited_view: &Tree,
    restored: &Tree,
    complement: &TreeLawComplement,
) -> Option<Divergence> {
    let graph = &complement.complement;
    let mut expected = tree_changed_carriers(original_view, edited_view);
    expected
        .nodes
        .retain(|id| !graph.synthesized_nodes.contains_key(id));
    expected.arcs.retain(|(_, child)| {
        complement
            .embedding
            .arcs
            .get(child)
            .is_none_or(|edge| !graph.synthesized_edges.contains_key(edge))
    });
    let removed_nodes: BTreeSet<_> = original_view
        .nodes
        .keys()
        .filter(|id| {
            !edited_view.nodes.contains_key(id) && !graph.synthesized_nodes.contains_key(id)
        })
        .copied()
        .collect();
    expected
        .arcs
        .extend(graph.dropped_edges.values().filter_map(|edge| {
            (removed_nodes.contains(&edge.src) || removed_nodes.contains(&edge.tgt))
                .then_some((edge.src, edge.tgt))
        }));
    let visible_children: BTreeSet<_> = original_view
        .arcs
        .iter()
        .chain(&edited_view.arcs)
        .filter_map(|(_, child, _)| {
            complement
                .embedding
                .arcs
                .get(child)
                .is_none_or(|edge| !graph.synthesized_edges.contains_key(edge))
                .then_some(*child)
        })
        .collect();
    expected.order = tree_order_diff(original_view, edited_view, |child| {
        visible_children.contains(&child)
    });
    let mut actual = tree_changed_carriers(source, restored);
    actual.order = tree_order_diff(source, restored, |child| visible_children.contains(&child));
    if expected.nodes != actual.nodes {
        Some(Divergence {
            kind: DivergenceKind::Locality,
            detail: "tree node diff differs from the exact edit footprint",
        })
    } else if expected.arcs != actual.arcs {
        Some(Divergence {
            kind: DivergenceKind::Locality,
            detail: "tree arc diff differs from the exact edit footprint and cascade",
        })
    } else if expected.order != actual.order {
        Some(Divergence {
            kind: DivergenceKind::Locality,
            detail: "tree child-order diff differs from the edited presentation",
        })
    } else if expected.fans != actual.fans {
        Some(Divergence {
            kind: DivergenceKind::Locality,
            detail: "tree fan diff differs from the exact edit footprint",
        })
    } else {
        None
    }
}

impl<'a> LawShape<GraphLens<'a>> for TreeShape {
    type Refusal = ReferenceLawError;
    type Context = ();

    fn get(
        lens: &GraphLens<'a>,
        source: &Tree,
        (): &(),
    ) -> Result<(Tree, TreeLawComplement), ReferenceLawError> {
        let result = tree_get(lens, source).ok_or(ReferenceLawError::OutsideTreeDomain)?;
        result
            .map(|(view, complement, embedding)| {
                (
                    view,
                    TreeLawComplement {
                        complement,
                        embedding,
                        source_arcs: tree_source_arcs(source),
                    },
                )
            })
            .map_err(ReferenceLawError::Restrict)
    }

    fn put(
        lens: &GraphLens<'a>,
        view: &Tree,
        complement: &TreeLawComplement,
        (): &(),
    ) -> Result<Tree, ReferenceLawError> {
        tree_put_law(lens, view, &complement.embedding, &complement.complement)
            .map_err(reference_lens_error)
    }

    fn check_put_domain(
        lens: &GraphLens<'a>,
        view: &Tree,
        complement: &TreeLawComplement,
        (): &(),
    ) -> Result<(), ReferenceLawError> {
        tree_check_put_domain_law(lens, view, &complement.embedding, &complement.complement)
            .map_err(reference_lens_error)
    }

    fn first_put_refusal(refusal: &ReferenceLawError) -> Option<ReferenceLawError> {
        reference_first_put_error(refusal)
    }

    fn single_step_basis(
        lens: &GraphLens<'a>,
        _: &Tree,
        view: &Tree,
        complement: &TreeLawComplement,
        (): &(),
    ) -> Vec<BasisCase<Tree, ReferenceLawError>> {
        tree_basis(lens, view, complement)
    }

    fn put_get_mode() -> PutGetMode {
        PutGetMode::Strict
    }

    fn rekey_complement(
        complement: &TreeLawComplement,
        _: &Tree,
        _: &Tree,
        producing_complement: &TreeLawComplement,
    ) -> TreeLawComplement {
        let next_index = tree_synth_index(complement);
        let producing_index = tree_synth_index(producing_complement);
        let Some(nodes) = synthesized_alignment(&next_index.nodes, &producing_index.nodes) else {
            return complement.clone();
        };
        let Some(edges) = synthesized_alignment(&next_index.edges, &producing_index.edges) else {
            return complement.clone();
        };
        collision_free_tree_rekey(complement, &nodes, &edges).unwrap_or_else(|| complement.clone())
    }

    fn congruence_cases(
        _: &GraphLens<'a>,
        view: &Tree,
        complement: &TreeLawComplement,
        (): &(),
    ) -> Vec<CongruenceCase<Tree, TreeLawComplement>> {
        tree_congruence_case(view, complement).into_iter().collect()
    }

    fn locality_divergence(
        _: &GraphLens<'a>,
        source: &Tree,
        original_view: &Tree,
        edited_view: &Tree,
        restored: &Tree,
        complement: &TreeLawComplement,
        (): &(),
    ) -> Option<Divergence> {
        tree_locality_divergence(source, original_view, edited_view, restored, complement)
    }
}

/// The five gates, in execution and report order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LawGate {
    /// Source round trip.
    GetPut,
    /// Strict or weak view round trip.
    PutGet,
    /// Put-domain agreement and totality on admitted views.
    Totality,
    /// Complement-stable repeated put.
    PutTwice,
    /// Put respects coherent synthesized-object reindexing.
    Congruence,
}

/// Operation named by a typed law failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LawOperation {
    /// Initial get.
    Get,
    /// Put.
    Put,
    /// Re-read get after put.
    Reget,
}

/// One concrete failure, retaining the refusal type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LawFailure<R> {
    /// Successful calls returned observably different states.
    Divergence {
        /// SSB ordinal, or `None` for `GetPut`.
        case: Option<usize>,
        /// First difference.
        divergence: Divergence,
    },
    /// An operation refused where the gate required success.
    Refusal {
        /// SSB ordinal, or `None` for `GetPut`.
        case: Option<usize>,
        /// Refusing operation.
        operation: LawOperation,
        /// Exact typed refusal.
        refusal: R,
    },
    /// Domain predicate and declared basis classification disagree.
    DomainMismatch {
        /// SSB ordinal.
        case: usize,
        /// Declared refusal (`None` means admitted).
        expected: Option<R>,
        /// Observed refusal (`None` means admitted).
        observed: Option<R>,
    },
    /// A declared coherent-reindexing witness failed Congruence.
    CongruenceMismatch {
        /// Congruence-basis ordinal.
        case: usize,
        /// Stable failing proposition.
        detail: &'static str,
    },
}

/// Why a gate could not run after an earlier prerequisite failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LawObstruction {
    /// Initial get produced no view or complement.
    InitialGetRefused,
}

/// Outcome of one law gate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LawGateStatus<R> {
    /// Every counted case passed.
    Passed {
        /// Number of cases checked.
        cases: usize,
    },
    /// First failure in canonical basis order.
    Failed(LawFailure<R>),
    /// A prerequisite made the gate inapplicable.
    Obstructed(LawObstruction),
}

/// One gate report.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GateReport<R> {
    /// Gate name.
    pub gate: LawGate,
    /// Exact result.
    pub status: LawGateStatus<R>,
}

/// Complete five-gate report over one source and its SSB.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LawReport<R> {
    /// Reports in [`LawGate`] contract order.
    pub gates: Vec<GateReport<R>>,
    /// Size of the deterministic Single-Step Basis.
    pub basis_cases: usize,
    /// Number of generated candidates in each represented mutation family.
    pub basis_families: BTreeMap<Mutation, usize>,
    /// Every generated mutation in canonical family/carrier order.
    pub basis_sequence: Vec<Mutation>,
    /// Positive candidates the put-domain predicate admits: the in-domain
    /// mutations `PutGet`, `Totality` and `PutTwice` range over.
    pub admitted: usize,
}

/// Separate locality-oracle result over the same finite SSB.
///
/// Locality is evidence about edit footprints, not one of the five lens-law
/// gates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalityReport<R> {
    /// Exact result over admitted basis cases.
    pub status: LawGateStatus<R>,
    /// Size of the deterministic Single-Step Basis.
    pub basis_cases: usize,
}

/// Run `GetPut`, `PutGet`/`WeakPutGet`, Totality, `PutTwice`, and Congruence.
#[must_use]
pub fn check_laws_report<S, L>(
    lens: &L,
    source: &S::Instance,
    context: &S::Context,
) -> LawReport<S::Refusal>
where
    S: LawShape<L>,
{
    let (original_view, complement) = match S::get(lens, source, context) {
        Ok(initial) => initial,
        Err(refusal) => {
            return LawReport {
                gates: vec![
                    GateReport {
                        gate: LawGate::GetPut,
                        status: LawGateStatus::Failed(LawFailure::Refusal {
                            case: None,
                            operation: LawOperation::Get,
                            refusal,
                        }),
                    },
                    obstructed(LawGate::PutGet),
                    obstructed(LawGate::Totality),
                    obstructed(LawGate::PutTwice),
                    obstructed(LawGate::Congruence),
                ],
                basis_cases: 0,
                basis_families: BTreeMap::new(),
                basis_sequence: Vec::new(),
                admitted: 0,
            };
        }
    };

    let generated = S::single_step_basis(lens, source, &original_view, &complement, context);
    // A positive candidate is in the domain exactly when the put-domain
    // predicate admits it; one a clause refuses is out of the domain and
    // leaves PutGet, Totality and PutTwice. A negative candidate keeps its
    // designated refusal, which Totality checks.
    let basis: Vec<_> = generated
        .iter()
        .filter(|candidate| match candidate.domain {
            DomainExpectation::Admitted => {
                S::check_put_domain(lens, &candidate.view, &complement, context).is_ok()
            }
            DomainExpectation::Refused(_) => true,
        })
        .cloned()
        .collect();
    let admitted = basis
        .iter()
        .filter(|candidate| matches!(candidate.domain, DomainExpectation::Admitted))
        .count();
    let mut basis_families = BTreeMap::new();
    for candidate in &generated {
        *basis_families.entry(candidate.mutation).or_insert(0) += 1;
    }
    let basis_sequence = generated
        .iter()
        .map(|candidate| candidate.mutation)
        .collect();
    LawReport {
        gates: vec![
            check_get_put::<S, L>(lens, source, &original_view, &complement, context),
            check_put_get::<S, L>(lens, &basis, &complement, context),
            check_domain::<S, L>(lens, &basis, &complement, context),
            check_put_twice::<S, L>(lens, &basis, &complement, context),
            check_congruence::<S, L>(lens, &original_view, &complement, context),
        ],
        basis_cases: generated.len(),
        basis_families,
        basis_sequence,
        admitted,
    }
}

/// Run the five law gates against the concrete graph reference lens.
#[must_use]
pub fn check_graph_laws(lens: &GraphLens<'_>, source: &Graph) -> LawReport<ReferenceLawError> {
    check_laws_report::<GraphShape, _>(lens, source, &())
}

/// Run the five law gates against the concrete tree reference lens.
#[must_use]
pub fn check_tree_laws(lens: &GraphLens<'_>, source: &Tree) -> LawReport<ReferenceLawError> {
    check_laws_report::<TreeShape, _>(lens, source, &())
}

fn check_get_put<S, L>(
    lens: &L,
    source: &S::Instance,
    view: &S::Instance,
    complement: &S::Complement,
    context: &S::Context,
) -> GateReport<S::Refusal>
where
    S: LawShape<L>,
{
    let status = match S::put(lens, view, complement, context) {
        Err(refusal) => LawGateStatus::Failed(LawFailure::Refusal {
            case: None,
            operation: LawOperation::Put,
            refusal,
        }),
        Ok(restored) if !S::identical(source, &restored) => {
            LawGateStatus::Failed(LawFailure::Divergence {
                case: None,
                divergence: S::divergence(
                    source,
                    &SynthIndex::default(),
                    &restored,
                    &SynthIndex::default(),
                )
                .unwrap_or_else(|| instance_divergence("GetPut sources differ")),
            })
        }
        Ok(_) => LawGateStatus::Passed { cases: 1 },
    };
    GateReport {
        gate: LawGate::GetPut,
        status,
    }
}

fn check_put_get<S, L>(
    lens: &L,
    basis: &[BasisCase<S::Instance, S::Refusal>],
    complement: &S::Complement,
    context: &S::Context,
) -> GateReport<S::Refusal>
where
    S: LawShape<L>,
{
    let mut cases = 0;
    for candidate in basis {
        if !matches!(candidate.domain, DomainExpectation::Admitted) {
            continue;
        }
        // A put refused on an admitted candidate is a Totality failure, which
        // the later dedicated gate reports; PutGet compares only views that
        // put accepts.
        let Ok(restored) = S::put(lens, &candidate.view, complement, context) else {
            continue;
        };
        cases += 1;
        let (observed, observed_complement) = match S::get(lens, &restored, context) {
            Ok(observed) => observed,
            Err(refusal) => {
                return failed_refusal(
                    LawGate::PutGet,
                    candidate.ordinal,
                    LawOperation::Reget,
                    refusal,
                );
            }
        };
        let divergence = match S::put_get_mode() {
            PutGetMode::Strict => {
                let expected_index = S::synth_index(&candidate.view, complement);
                let observed_index = S::synth_index(&observed, &observed_complement);
                (!S::equivalent(&candidate.view, &expected_index, &observed, &observed_index)).then(
                    || {
                        S::divergence(&candidate.view, &expected_index, &observed, &observed_index)
                            .unwrap_or_else(|| instance_divergence("PutGet views differ"))
                    },
                )
            }
            PutGetMode::Weak => match S::put(lens, &observed, complement, context) {
                Err(refusal) => {
                    return failed_refusal(
                        LawGate::PutGet,
                        candidate.ordinal,
                        LawOperation::Put,
                        refusal,
                    );
                }
                Ok(repeated) => (!S::identical(&restored, &repeated)).then(|| {
                    S::divergence(
                        &restored,
                        &SynthIndex::default(),
                        &repeated,
                        &SynthIndex::default(),
                    )
                    .unwrap_or_else(|| instance_divergence("WeakPutGet sources differ"))
                }),
            },
        };
        if let Some(divergence) = divergence {
            return failed_divergence(LawGate::PutGet, candidate.ordinal, divergence);
        }
    }
    passed(LawGate::PutGet, cases)
}

fn check_put_twice<S, L>(
    lens: &L,
    basis: &[BasisCase<S::Instance, S::Refusal>],
    complement: &S::Complement,
    context: &S::Context,
) -> GateReport<S::Refusal>
where
    S: LawShape<L>,
{
    let mut cases = 0;
    for candidate in basis {
        if !matches!(candidate.domain, DomainExpectation::Admitted) {
            continue;
        }
        cases += 1;
        let restored = match S::put(lens, &candidate.view, complement, context) {
            Ok(restored) => restored,
            Err(refusal) => {
                return failed_refusal(
                    LawGate::PutTwice,
                    candidate.ordinal,
                    LawOperation::Put,
                    refusal,
                );
            }
        };
        let (observed_view, next_complement) = match S::get(lens, &restored, context) {
            Ok(next) => next,
            Err(refusal) => {
                return failed_refusal(
                    LawGate::PutTwice,
                    candidate.ordinal,
                    LawOperation::Reget,
                    refusal,
                );
            }
        };
        let next_complement = S::rekey_complement(
            &next_complement,
            &observed_view,
            &candidate.view,
            complement,
        );
        let repeated = match S::put(lens, &candidate.view, &next_complement, context) {
            Ok(repeated) => repeated,
            Err(refusal) => {
                return failed_refusal(
                    LawGate::PutTwice,
                    candidate.ordinal,
                    LawOperation::Put,
                    refusal,
                );
            }
        };
        if !S::identical(&restored, &repeated) {
            return failed_divergence(
                LawGate::PutTwice,
                candidate.ordinal,
                S::divergence(
                    &restored,
                    &SynthIndex::default(),
                    &repeated,
                    &SynthIndex::default(),
                )
                .unwrap_or_else(|| instance_divergence("PutTwice sources differ")),
            );
        }
    }
    passed(LawGate::PutTwice, cases)
}

fn check_domain<S, L>(
    lens: &L,
    basis: &[BasisCase<S::Instance, S::Refusal>],
    complement: &S::Complement,
    context: &S::Context,
) -> GateReport<S::Refusal>
where
    S: LawShape<L>,
{
    for candidate in basis {
        let observed = S::check_put_domain(lens, &candidate.view, complement, context).err();
        let expected = match &candidate.domain {
            DomainExpectation::Admitted => None,
            DomainExpectation::Refused(refusal) => Some(refusal.clone()),
        };
        if observed != expected {
            return GateReport {
                gate: LawGate::Totality,
                status: LawGateStatus::Failed(LawFailure::DomainMismatch {
                    case: candidate.ordinal,
                    expected,
                    observed,
                }),
            };
        }
        match (
            &candidate.domain,
            S::put(lens, &candidate.view, complement, context),
        ) {
            (DomainExpectation::Admitted, Ok(_)) => {}
            (DomainExpectation::Admitted, Err(refusal)) => {
                return failed_refusal(
                    LawGate::Totality,
                    candidate.ordinal,
                    LawOperation::Put,
                    refusal,
                );
            }
            (DomainExpectation::Refused(expected), Err(observed))
                if S::first_put_refusal(expected).as_ref() == Some(&observed) => {}
            (DomainExpectation::Refused(expected), Err(observed)) => {
                return GateReport {
                    gate: LawGate::Totality,
                    status: LawGateStatus::Failed(LawFailure::DomainMismatch {
                        case: candidate.ordinal,
                        expected: S::first_put_refusal(expected).or_else(|| Some(expected.clone())),
                        observed: Some(observed),
                    }),
                };
            }
            (DomainExpectation::Refused(expected), Ok(_)) => {
                return GateReport {
                    gate: LawGate::Totality,
                    status: LawGateStatus::Failed(LawFailure::DomainMismatch {
                        case: candidate.ordinal,
                        expected: S::first_put_refusal(expected).or_else(|| Some(expected.clone())),
                        observed: None,
                    }),
                };
            }
        }
    }
    passed(LawGate::Totality, basis.len())
}

fn check_congruence<S, L>(
    lens: &L,
    view: &S::Instance,
    complement: &S::Complement,
    context: &S::Context,
) -> GateReport<S::Refusal>
where
    S: LawShape<L>,
{
    let mut cases = 0;
    let index = S::synth_index(view, complement);
    for candidate in S::congruence_cases(lens, view, complement, context) {
        cases += 1;
        let candidate_index = S::synth_index(&candidate.view, &candidate.complement);
        if !S::equivalent(view, &index, &candidate.view, &candidate_index) {
            return GateReport {
                gate: LawGate::Congruence,
                status: LawGateStatus::Failed(LawFailure::CongruenceMismatch {
                    case: candidate.ordinal,
                    detail: "declared presentations are not SynthIndex-equivalent",
                }),
            };
        }
        if S::complement_divergence(complement, &candidate.complement).is_some() {
            return GateReport {
                gate: LawGate::Congruence,
                status: LawGateStatus::Failed(LawFailure::CongruenceMismatch {
                    case: candidate.ordinal,
                    detail: "coherently reindexed complements diverge",
                }),
            };
        }
        let left_domain = S::check_put_domain(lens, view, complement, context).err();
        let right_domain =
            S::check_put_domain(lens, &candidate.view, &candidate.complement, context).err();
        if left_domain != right_domain {
            return GateReport {
                gate: LawGate::Congruence,
                status: LawGateStatus::Failed(LawFailure::CongruenceMismatch {
                    case: candidate.ordinal,
                    detail: "equivalent presentations have different domain verdicts",
                }),
            };
        }
        if left_domain.is_none() {
            let left = match S::put(lens, view, complement, context) {
                Ok(value) => value,
                Err(refusal) => {
                    return failed_refusal(
                        LawGate::Congruence,
                        candidate.ordinal,
                        LawOperation::Put,
                        refusal,
                    );
                }
            };
            let right = match S::put(lens, &candidate.view, &candidate.complement, context) {
                Ok(value) => value,
                Err(refusal) => {
                    return failed_refusal(
                        LawGate::Congruence,
                        candidate.ordinal,
                        LawOperation::Put,
                        refusal,
                    );
                }
            };
            if !S::equivalent(
                &left,
                &SynthIndex::default(),
                &right,
                &SynthIndex::default(),
            ) {
                return failed_divergence(
                    LawGate::Congruence,
                    candidate.ordinal,
                    S::divergence(
                        &left,
                        &SynthIndex::default(),
                        &right,
                        &SynthIndex::default(),
                    )
                    .unwrap_or_else(|| instance_divergence("Congruence put results differ")),
                );
            }
        }
    }
    passed(LawGate::Congruence, cases)
}

/// Check edit locality independently of the five lens-law gates.
#[must_use]
pub fn check_locality_report<S, L>(
    lens: &L,
    source: &S::Instance,
    context: &S::Context,
) -> LocalityReport<S::Refusal>
where
    S: LawShape<L>,
{
    let (original_view, complement) = match S::get(lens, source, context) {
        Ok(initial) => initial,
        Err(refusal) => {
            return LocalityReport {
                status: LawGateStatus::Failed(LawFailure::Refusal {
                    case: None,
                    operation: LawOperation::Get,
                    refusal,
                }),
                basis_cases: 0,
            };
        }
    };
    let basis = S::single_step_basis(lens, source, &original_view, &complement, context);
    let mut cases = 0;
    for candidate in &basis {
        if !matches!(candidate.domain, DomainExpectation::Admitted) {
            continue;
        }
        if S::check_put_domain(lens, &candidate.view, &complement, context).is_err() {
            continue;
        }
        cases += 1;
        let restored = match S::put(lens, &candidate.view, &complement, context) {
            Ok(restored) => restored,
            Err(refusal) => {
                return LocalityReport {
                    status: LawGateStatus::Failed(LawFailure::Refusal {
                        case: Some(candidate.ordinal),
                        operation: LawOperation::Put,
                        refusal,
                    }),
                    basis_cases: basis.len(),
                };
            }
        };
        if let Some(mut divergence) = S::locality_divergence(
            lens,
            source,
            &original_view,
            &candidate.view,
            &restored,
            &complement,
            context,
        ) {
            divergence.kind = DivergenceKind::Locality;
            return LocalityReport {
                status: LawGateStatus::Failed(LawFailure::Divergence {
                    case: Some(candidate.ordinal),
                    divergence,
                }),
                basis_cases: basis.len(),
            };
        }
    }
    LocalityReport {
        status: LawGateStatus::Passed { cases },
        basis_cases: basis.len(),
    }
}

const fn passed<R>(gate: LawGate, cases: usize) -> GateReport<R> {
    GateReport {
        gate,
        status: LawGateStatus::Passed { cases },
    }
}

const fn obstructed<R>(gate: LawGate) -> GateReport<R> {
    GateReport {
        gate,
        status: LawGateStatus::Obstructed(LawObstruction::InitialGetRefused),
    }
}

const fn failed_refusal<R>(
    gate: LawGate,
    case: usize,
    operation: LawOperation,
    refusal: R,
) -> GateReport<R> {
    GateReport {
        gate,
        status: LawGateStatus::Failed(LawFailure::Refusal {
            case: Some(case),
            operation,
            refusal,
        }),
    }
}

const fn failed_divergence<R>(gate: LawGate, case: usize, divergence: Divergence) -> GateReport<R> {
    GateReport {
        gate,
        status: LawGateStatus::Failed(LawFailure::Divergence {
            case: Some(case),
            divergence,
        }),
    }
}

/// Definition: The graph-lens put operation.
///
/// This free-function form is the stable semantic path used by the
/// correctness manifest; it delegates to [`GraphLens::put`].
///
/// # Errors
///
/// The first put-domain violation in report order.
///
/// Book anchors: `L-lenses-1`, `L-lenses-2`, `L-lenses-3`, `L-lenses-4`, `L-lenses-5`, `L-lenses-6`, `L-lenses-7`, `L-lenses-8`, `L-lenses-9`, `L-lenses-10`.
pub fn put(
    lens: &GraphLens<'_>,
    view: &Graph,
    complement: &Complement,
) -> Result<Graph, LensError> {
    lens.put(view, complement)
}

#[cfg(test)]
mod refusal_normalization_tests {
    use std::collections::BTreeSet;

    use super::{
        PutDomainError, ReferenceLawError, ViewRef, reference_first_put_error,
        table_first_put_error,
    };
    use crate::encode::TableEncodeError;
    use crate::model::SEdge;
    use crate::pages::lenses::table::{TableCheckError, TableOperationalError, TablePutError};

    fn collision(row: u32) -> PutDomainError {
        PutDomainError::IdCollision {
            elem: ViewRef::Row(row),
        }
    }

    #[test]
    fn graph_and_tree_complete_reports_normalize_to_the_first_violation() {
        let complete = ReferenceLawError::PutDomain(vec![collision(2), collision(3)]);
        assert_eq!(
            reference_first_put_error(&complete),
            Some(ReferenceLawError::PutDomain(vec![collision(2)]))
        );
        assert_eq!(
            reference_first_put_error(&ReferenceLawError::PutDomain(Vec::new())),
            None
        );
    }

    #[test]
    fn table_complete_report_prefers_the_first_known_violation() {
        let unresolved_collations = BTreeSet::from(["named_ci".to_owned()]);
        let complete = TableCheckError::Domain {
            violations: vec![collision(2), collision(3)],
            unresolved_collations,
        };
        assert_eq!(
            table_first_put_error(&complete),
            Some(TablePutError::Domain(collision(2)))
        );
    }

    #[test]
    fn table_unresolved_only_operational_and_empty_reports_normalize_exactly() {
        let names = BTreeSet::from(["named_ci".to_owned()]);
        assert_eq!(
            table_first_put_error(&TableCheckError::Domain {
                violations: Vec::new(),
                unresolved_collations: names.clone(),
            }),
            Some(TablePutError::NamedCollationUnresolved {
                names: names.clone(),
            })
        );
        assert_eq!(
            table_first_put_error(&TableCheckError::NamedCollationUnresolved {
                names: names.clone(),
            }),
            Some(TablePutError::NamedCollationUnresolved { names })
        );

        let pair = (SEdge::new("a", "b", "fk", Some("fk")), 1, 2);
        let operational =
            TableOperationalError::BaselineEncoding(TableEncodeError::MalformedTable {
                pair: Box::new(pair),
            });
        assert_eq!(
            table_first_put_error(&TableCheckError::Operational(operational.clone())),
            Some(TablePutError::Operational(operational))
        );
        assert_eq!(
            table_first_put_error(&TableCheckError::Domain {
                violations: Vec::new(),
                unresolved_collations: BTreeSet::new(),
            }),
            None
        );
    }
}

#[cfg(test)]
mod graph_basis_tests {
    use std::collections::BTreeMap;
    use std::fmt::Debug;

    use super::{BasisCase, DomainExpectation, Mutation, ReferenceLawError, ViewRef, graph_basis};
    use crate::migration::Migration;
    use crate::model::{GEdge, Graph, Node, NodeShape, SEdge};
    use crate::pages::lenses::domain::PutDomainError;
    use crate::pages::lenses::graph::{GraphLens, LensError};
    use crate::restrict::{
        Complement, LayerRefusal, NoValueLayer, PredicateRefusal, PutFieldRefusal, SynthKey,
        ValueLayer, ValueLayerSemantics, compile,
    };
    use crate::schema::{
        Cardinality, HyperEdge, Key, KeyPart, KeyRole, KeyScope, NullMatch, OnUnresolved,
        RefSource, Reference, ResolveMode, Schema,
    };
    use crate::validation::{Semantics, Violation};
    use crate::value::{Presence, Value};

    struct SelectiveRestoreLayer;

    impl ValueLayer for SelectiveRestoreLayer {
        fn canonical_semantics(&self) -> ValueLayerSemantics {
            ValueLayerSemantics {
                domain: "panproto-reference/selective-restore-test",
                version: 1,
                canonical_payload: Vec::new(),
            }
        }

        fn survives(&self, _: &Graph, _: &Node) -> Result<bool, PredicateRefusal> {
            Ok(true)
        }

        fn transform(
            &self,
            _: &Graph,
            node: &Node,
        ) -> Result<(BTreeMap<String, Value>, Option<Presence>), LayerRefusal> {
            Ok((node.fields.clone(), node.value.clone()))
        }

        fn restore(
            &self,
            view: &Node,
            _: Option<&BTreeMap<String, Value>>,
            _: Option<&Option<Presence>>,
        ) -> Result<(BTreeMap<String, Value>, Option<Presence>), PutFieldRefusal> {
            match view.anchor.as_str() {
                "a" => Err(PutFieldRefusal::TransformNotInvertible {
                    field: "refused".to_owned(),
                }),
                "b" => {
                    let mut fields = view.fields.clone();
                    fields.insert("changed".to_owned(), Value::Bool(true));
                    Ok((fields, view.value.clone()))
                }
                _ => Ok((view.fields.clone(), view.value.clone())),
            }
        }
    }

    struct GraphSensitivePredicateLayer;

    impl ValueLayer for GraphSensitivePredicateLayer {
        fn canonical_semantics(&self) -> ValueLayerSemantics {
            ValueLayerSemantics {
                domain: "panproto-reference/graph-sensitive-predicate-test",
                version: 1,
                canonical_payload: Vec::new(),
            }
        }

        fn survives(&self, source: &Graph, _: &Node) -> Result<bool, PredicateRefusal> {
            Ok(!source.nodes.values().any(|node| node.anchor == "candidate"))
        }

        fn transform(
            &self,
            _: &Graph,
            node: &Node,
        ) -> Result<(BTreeMap<String, Value>, Option<Presence>), LayerRefusal> {
            Ok((node.fields.clone(), node.value.clone()))
        }

        fn restore(
            &self,
            view: &Node,
            _: Option<&BTreeMap<String, Value>>,
            _: Option<&Option<Presence>>,
        ) -> Result<(BTreeMap<String, Value>, Option<Presence>), PutFieldRefusal> {
            Ok((view.fields.clone(), view.value.clone()))
        }
    }

    fn insert_nullable_reference(schema: &mut Schema) {
        schema.keys.entry("b".to_owned()).or_default().push(Key {
            id: "reference-target".to_owned(),
            parts: vec![KeyPart::Path(Vec::new())],
            role: KeyRole::Unique,
            scope: KeyScope::Global,
            equality: Vec::new(),
        });
        let reference = Reference {
            id: "nullable-ref".to_owned(),
            holder: "a".to_owned(),
            source: RefSource::KeyValue {
                parts: vec![Vec::new()],
            },
            target_vertex: "b".to_owned(),
            target_key: Some("reference-target".to_owned()),
            edge: None,
            cardinality: Cardinality::One,
            nullable: true,
            on_unresolved: OnUnresolved::Error,
            mode: ResolveMode::AddEdge,
            guard: None,
            null_match: NullMatch::Simple,
            null_vertex: None,
            scope_via: None,
        };
        schema.references.insert(reference.id.clone(), reference);
    }

    fn isolated_reserved_graph_fixture() -> (Schema, Schema, Migration, Graph) {
        let kept = SEdge::new("a", "b", "source", Some("kept"));
        let hidden = SEdge::new("a", "b", "source", Some("hidden"));
        let target = SEdge::new("a", "b", "target", Some("ab"));
        let source_schema = Schema::default()
            .vertex("a", "record")
            .vertex("b", "record")
            .vertex("dropped", "record")
            .edge("a", "b", "source", Some("kept"))
            .edge("a", "b", "source", Some("hidden"));
        let target_schema = Schema::default()
            .vertex("a", "record")
            .vertex("b", "record")
            .edge("a", "b", "target", Some("ab"));
        let migration = Migration {
            surviving_verts: ["a".to_owned(), "b".to_owned()].into(),
            edge_remap: [(kept.clone(), target)].into(),
            ..Migration::default()
        };
        let source = Graph {
            nodes: [
                (1, Node::new(1, "a")),
                (2, Node::new(2, "b")),
                (3, Node::new(3, "dropped")),
            ]
            .into(),
            edges: [
                (
                    4,
                    GEdge {
                        src: 1,
                        tgt: 2,
                        label: kept,
                        position: None,
                    },
                ),
                (
                    5,
                    GEdge {
                        src: 1,
                        tgt: 2,
                        label: hidden,
                        position: None,
                    },
                ),
            ]
            .into(),
        };
        (source_schema, target_schema, migration, source)
    }

    fn indirect_key_reserved_edge_fixture() -> (Schema, Schema, Migration, Graph, SEdge) {
        let kept = SEdge::new("a", "b", "source", Some("kept"));
        let hidden = SEdge::new("a", "b", "source", Some("hidden"));
        let target = SEdge::new("a", "b", "target", Some("ab"));
        let mut source_schema = Schema::default()
            .vertex("a", "record")
            .vertex("b", "string")
            .edge("a", "b", "source", Some("kept"))
            .edge("a", "b", "source", Some("hidden"));
        source_schema.keys.insert(
            "a".to_owned(),
            vec![Key {
                id: "indirect".to_owned(),
                parts: vec![KeyPart::Path(vec![kept.clone()])],
                role: KeyRole::Unique,
                scope: KeyScope::Global,
                equality: Vec::new(),
            }],
        );
        let target_schema = Schema::default()
            .vertex("a", "record")
            .vertex("b", "string")
            .edge("a", "b", "target", Some("ab"));
        let migration = Migration {
            surviving_verts: ["a".to_owned(), "b".to_owned()].into(),
            edge_remap: [(kept.clone(), target.clone())].into(),
            ..Migration::default()
        };
        let source = Graph {
            nodes: [
                (1, Node::new(1, "a")),
                (
                    2,
                    Node::new(2, "b").with_value(Value::Str("same".to_owned())),
                ),
                (3, Node::new(3, "a")),
                (
                    4,
                    Node::new(4, "b").with_value(Value::Str("same".to_owned())),
                ),
            ]
            .into(),
            edges: [
                (
                    5,
                    GEdge {
                        src: 3,
                        tgt: 4,
                        label: hidden,
                        position: None,
                    },
                ),
                (
                    6,
                    GEdge {
                        src: 1,
                        tgt: 2,
                        label: kept,
                        position: None,
                    },
                ),
            ]
            .into(),
        };
        (source_schema, target_schema, migration, source, target)
    }

    fn fan_kind_reserved_edge_fixture() -> (Schema, Schema, Migration, Graph, SEdge) {
        let source_kept = SEdge::new("a", "b", crate::restrict::HYPER_PARENT, Some("source-kept"));
        let target_kept = SEdge::new("a", "b", "ordinary", Some("target-kept"));
        let hidden = SEdge::new("a", "b", "ordinary", Some("hidden"));
        let source_schema = Schema::default()
            .vertex("a", "record")
            .vertex("b", "record")
            .edge("a", "b", crate::restrict::HYPER_PARENT, Some("source-kept"))
            .edge("a", "b", "ordinary", Some("hidden"));
        let target_schema = Schema::default()
            .vertex("a", "record")
            .vertex("b", "record")
            .edge("a", "b", "ordinary", Some("target-kept"));
        let migration = Migration {
            surviving_verts: ["a".to_owned(), "b".to_owned()].into(),
            edge_remap: [(source_kept, target_kept.clone())].into(),
            ..Migration::default()
        };
        let source = Graph {
            nodes: [(1, Node::new(1, "a")), (2, Node::new(2, "b"))].into(),
            edges: [(
                5,
                GEdge {
                    src: 1,
                    tgt: 2,
                    label: hidden,
                    position: None,
                },
            )]
            .into(),
        };
        (source_schema, target_schema, migration, source, target_kept)
    }

    fn assert_reserved_node_residue_is_rejected(
        lens: &GraphLens<'_>,
        view: &Graph,
        complement: &Complement,
    ) {
        let candidate = super::tree_node_for_anchor(3, lens.tgt, "a");
        assert!(super::graph_reserved_node_witness_is_structurally_exact(
            lens, view, complement, &candidate,
        ));
        let mut collateral = complement.clone();
        collateral.original_fields.insert(3, BTreeMap::new());
        assert!(
            !super::graph_reserved_node_witness_is_structurally_exact(
                lens,
                view,
                &collateral,
                &candidate,
            ),
            "node-keyed residue prevents an independently isolated collision witness"
        );
        let mut incident = complement.clone();
        incident.dropped_edges.insert(
            6,
            GEdge {
                src: 3,
                tgt: 2,
                label: SEdge::new("a", "b", "source", Some("hidden")),
                position: None,
            },
        );
        assert!(
            !super::graph_reserved_node_witness_is_structurally_exact(
                lens, view, &incident, &candidate,
            ),
            "even endpoint-compatible incident residue is outside the local proof"
        );
    }

    fn dropped_only_graph() -> Graph {
        Graph {
            nodes: [(7, Node::new(7, "dropped"))].into(),
            ..Graph::default()
        }
    }

    fn without_reserved_graph_node_residue(mut complement: Complement, id: u32) -> Complement {
        assert!(
            complement.dropped_nodes.remove(&id).is_some(),
            "the counterfactual removes exactly the reserved dropped node"
        );
        complement
    }

    fn without_reserved_graph_edge_residue(mut complement: Complement, id: u32) -> Complement {
        complement.dropped_edges.remove(&id);
        complement.original_edges.remove(&id);
        complement.synthesized_edges.remove(&id);
        complement.synthesized_nodes.retain(
            |_, key| !matches!(key, SynthKey::Expand { source_edge, .. } if *source_edge == id),
        );
        complement.synthesized_edges.retain(
            |_, key| !matches!(key, SynthKey::Expand { source_edge, .. } if *source_edge == id),
        );
        complement
    }

    fn assert_reserved_graph_counterfactual(
        lens: &GraphLens<'_>,
        case: &BasisCase<Graph, ReferenceLawError>,
        complement: &Complement,
        stripped: &Complement,
    ) {
        assert_exact_graph_negative(lens, complement, case);
        assert_eq!(lens.check_put_domain(&case.view, stripped), Ok(()));
        let restored = must(lens.put(&case.view, stripped));
        assert_eq!(
            crate::pages::validation::implementation::validate_graph(
                lens.src,
                lens.constraint_semantics(),
                &restored,
            )
            .status,
            crate::pages::validation::implementation::CheckStatus::Passed
        );
        let (regotten, regotten_complement) = must(lens.get(&restored));
        assert!(<super::GraphShape as super::LensShape>::equivalent(
            &case.view,
            &<super::GraphShape as super::LensShape>::synth_index(&case.view, stripped),
            &regotten,
            &<super::GraphShape as super::LensShape>::synth_index(&regotten, &regotten_complement,),
        ));
    }

    fn assert_reserved_graph_counterfactuals(
        lens: &GraphLens<'_>,
        cases: &[BasisCase<Graph, ReferenceLawError>],
        complement: &Complement,
    ) {
        for (reference, stripped) in [
            (
                ViewRef::Node(3),
                without_reserved_graph_node_residue(complement.clone(), 3),
            ),
            (
                ViewRef::Edge(5),
                without_reserved_graph_edge_residue(complement.clone(), 5),
            ),
        ] {
            let expected = DomainExpectation::Refused(ReferenceLawError::PutDomain(vec![
                PutDomainError::IdCollision { elem: reference },
            ]));
            let case = cases
                .iter()
                .find(|case| case.domain == expected)
                .unwrap_or_else(|| panic!("missing reserved-carrier witness"));
            assert_reserved_graph_counterfactual(lens, case, complement, &stripped);
        }
    }

    fn must<T, E: Debug>(result: Result<T, E>) -> T {
        result.unwrap_or_else(|error| panic!("unexpected error: {error:?}"))
    }

    fn assert_exact_graph_negative(
        lens: &GraphLens<'_>,
        complement: &Complement,
        case: &BasisCase<Graph, ReferenceLawError>,
    ) -> Vec<PutDomainError> {
        let DomainExpectation::Refused(ReferenceLawError::PutDomain(expected)) = &case.domain
        else {
            panic!("negative case lacks an independent complete refusal")
        };
        let Some(first) = expected.first() else {
            panic!("a refused graph declaration must contain a violation")
        };
        assert_eq!(
            lens.check_put_domain(&case.view, complement),
            Err(expected.clone())
        );
        assert_eq!(
            lens.put(&case.view, complement),
            Err(LensError::PutDomain(first.clone()))
        );
        expected.clone()
    }

    #[test]
    fn graph_synthesized_negatives_are_exact_node_first_and_raw_first() {
        let source_edge = SEdge::new("a", "b", "source", Some("ab"));
        let ax = SEdge::new("a", "x", "hop", Some("ax"));
        let ax_alternate = SEdge::new("a", "x", "hop", Some("ax-alternate"));
        let xb = SEdge::new("x", "b", "hop", Some("xb"));
        let source_schema = Schema::default()
            .vertex("a", "record")
            .vertex("b", "record")
            .edge("a", "b", "source", Some("ab"));
        let target_schema = Schema::default()
            .vertex("a", "record")
            .vertex("x", "record")
            .vertex("b", "record")
            .edge("a", "x", "hop", Some("ax"))
            .edge("a", "x", "hop", Some("ax-alternate"))
            .edge("x", "b", "hop", Some("xb"));
        let migration = Migration {
            surviving_verts: ["a".to_owned(), "b".to_owned()].into(),
            resolver: [
                (("a".to_owned(), "x".to_owned()), ax),
                (("x".to_owned(), "b".to_owned()), xb),
            ]
            .into(),
            expansion_path: [(("a".to_owned(), "b".to_owned()), vec!["x".to_owned()])].into(),
            ..Migration::default()
        };
        let plan = must(compile(&source_schema, &target_schema, &migration));
        let lens = must(GraphLens::new(
            &source_schema,
            &target_schema,
            &migration,
            &plan,
            &BTreeMap::new(),
            &NoValueLayer,
        ));
        let source = Graph {
            nodes: [(1, Node::new(1, "a")), (2, Node::new(2, "b"))].into(),
            edges: [(
                3,
                GEdge {
                    src: 1,
                    tgt: 2,
                    label: source_edge,
                    position: None,
                },
            )]
            .into(),
        };
        let (view, complement) = must(lens.get(&source));
        assert_eq!(complement.synthesized_nodes.len(), 1);
        assert_eq!(complement.synthesized_edges.len(), 2);

        let cases = graph_basis(&lens, &view, &complement);
        let removed: Vec<_> = cases
            .iter()
            .filter(|case| case.mutation == Mutation::RemoveSynthesized)
            .collect();
        assert_eq!(removed.len(), 3);
        let complete = assert_exact_graph_negative(&lens, &complement, removed[0]);
        assert_eq!(complete.len(), 3);
        assert!(matches!(
            complete.as_slice(),
            [
                PutDomainError::SynthesizedEdited {
                    elem: ViewRef::Node(_)
                },
                PutDomainError::SynthesizedEdited {
                    elem: ViewRef::Edge(_)
                },
                PutDomainError::SynthesizedEdited {
                    elem: ViewRef::Edge(_)
                }
            ]
        ));
        for case in &removed[1..] {
            assert_exact_graph_negative(&lens, &complement, case);
        }

        let edited: Vec<_> = cases
            .iter()
            .filter(|case| case.mutation == Mutation::EditSynthesized)
            .collect();
        assert_eq!(edited.len(), 2, "node and alternate-label edge witnesses");
        for case in edited {
            assert_exact_graph_negative(&lens, &complement, case);
        }
        assert!(cases.iter().all(|case| {
            case.mutation != Mutation::ReservedIdReuse
                || !matches!(
                    &case.domain,
                    DomainExpectation::Refused(ReferenceLawError::PutDomain(errors))
                        if errors.iter().any(|error| matches!(error,
                            PutDomainError::IdCollision { elem: ViewRef::Edge(3) }))
                )
        }));
        assert!(target_schema.edges.contains(&ax_alternate));
    }

    #[test]
    fn graph_contracted_edge_negative_is_exact_and_nonvacuous() {
        let source_schema = Schema::default()
            .vertex("a", "record")
            .vertex("m", "record")
            .vertex("b", "record")
            .edge("a", "m", "fk", Some("am"))
            .edge("m", "b", "fk", Some("mb"));
        let target_label = SEdge::new("a", "b", "fk", Some("ab"));
        let target_schema = Schema::default()
            .vertex("a", "record")
            .vertex("b", "record")
            .edge("a", "b", "fk", Some("ab"));
        let migration = Migration {
            surviving_verts: ["a".to_owned(), "b".to_owned()].into(),
            resolver: [(("a".to_owned(), "b".to_owned()), target_label)].into(),
            ..Migration::default()
        };
        let plan = must(compile(&source_schema, &target_schema, &migration));
        let lens = must(GraphLens::new(
            &source_schema,
            &target_schema,
            &migration,
            &plan,
            &BTreeMap::new(),
            &NoValueLayer,
        ));
        let source = Graph {
            nodes: [(1, Node::new(1, "a")), (2, Node::new(2, "b"))].into(),
            ..Graph::default()
        };
        let (view, complement) = must(lens.get(&source));
        let cases: Vec<_> = graph_basis(&lens, &view, &complement)
            .into_iter()
            .filter(|case| case.mutation == Mutation::AddContractedEdge)
            .collect();
        assert_eq!(cases.len(), 1);
        assert_exact_graph_negative(&lens, &complement, &cases[0]);
    }

    #[test]
    fn graph_malformed_fan_negative_is_exact_and_nonvacuous() {
        let mut schema = Schema::default()
            .vertex("a", "record")
            .vertex("b", "record");
        schema.hyper_edges.insert(
            "h".to_owned(),
            HyperEdge {
                kind: "fan".to_owned(),
                signature: [("l".to_owned(), "b".to_owned())].into(),
                parent_label: "p".to_owned(),
            },
        );
        let migration = Migration {
            surviving_verts: ["a".to_owned(), "b".to_owned()].into(),
            ..Migration::default()
        };
        let plan = must(compile(&schema, &schema, &migration));
        let lens = must(GraphLens::new(
            &schema,
            &schema,
            &migration,
            &plan,
            &BTreeMap::new(),
            &NoValueLayer,
        ));
        let mut fan = Node::new(2, "h");
        fan.shape = NodeShape::HyperEdge;
        let source = Graph {
            nodes: [(0, Node::new(0, "a")), (1, Node::new(1, "b")), (2, fan)].into(),
            edges: [
                (
                    3,
                    GEdge {
                        src: 2,
                        tgt: 0,
                        label: SEdge::new("h", "a", crate::restrict::HYPER_PARENT, Some("p")),
                        position: None,
                    },
                ),
                (
                    4,
                    GEdge {
                        src: 2,
                        tgt: 1,
                        label: SEdge::new("h", "b", crate::restrict::HYPER_LABEL, Some("l")),
                        position: None,
                    },
                ),
            ]
            .into(),
        };
        let (view, complement) = must(lens.get(&source));
        let cases: Vec<_> = graph_basis(&lens, &view, &complement)
            .into_iter()
            .filter(|case| case.mutation == Mutation::MalformedFan)
            .collect();
        assert_eq!(cases.len(), 1);
        let complete = assert_exact_graph_negative(&lens, &complement, &cases[0]);
        assert!(matches!(
            complete.as_slice(),
            [PutDomainError::MalformedFan {
                node: 2,
                reason: crate::fan::FanViolation::MultipleParents { edges }
            }] if edges.len() == 2 && edges[0] == 3
        ));
    }

    #[test]
    fn graph_reserved_node_and_edge_negatives_are_exact_and_isolated() {
        let (source_schema, target_schema, migration, source) = isolated_reserved_graph_fixture();
        let plan = must(compile(&source_schema, &target_schema, &migration));
        let lens = must(GraphLens::new(
            &source_schema,
            &target_schema,
            &migration,
            &plan,
            &BTreeMap::new(),
            &NoValueLayer,
        ));
        let (view, complement) = must(lens.get(&source));
        assert_eq!(
            complement.dropped_nodes.keys().copied().collect::<Vec<_>>(),
            vec![3]
        );
        assert_eq!(
            complement.dropped_edges.keys().copied().collect::<Vec<_>>(),
            vec![5]
        );
        assert_reserved_node_residue_is_rejected(&lens, &view, &complement);

        let cases: Vec<_> = graph_basis(&lens, &view, &complement)
            .into_iter()
            .filter(|case| case.mutation == Mutation::ReservedIdReuse)
            .collect();
        assert_eq!(cases.len(), 2);
        let reports: Vec<_> = cases
            .iter()
            .map(|case| assert_exact_graph_negative(&lens, &complement, case))
            .collect();
        assert!(reports.contains(&vec![PutDomainError::IdCollision {
            elem: ViewRef::Node(3),
        }]));
        assert!(reports.contains(&vec![PutDomainError::IdCollision {
            elem: ViewRef::Edge(5),
        }]));

        assert_reserved_graph_counterfactuals(&lens, &cases, &complement);
    }

    #[test]
    fn graph_reserved_edge_omits_an_indirect_key_collision() {
        let (source_schema, target_schema, migration, source, target) =
            indirect_key_reserved_edge_fixture();
        let plan = must(compile(&source_schema, &target_schema, &migration));
        let lens = must(GraphLens::new(
            &source_schema,
            &target_schema,
            &migration,
            &plan,
            &BTreeMap::new(),
            &NoValueLayer,
        ));
        let (view, complement) = must(lens.get(&source));
        assert_eq!(
            graph_basis(&lens, &view, &complement)
                .iter()
                .filter(|case| case.mutation == Mutation::ReservedIdReuse)
                .count(),
            0
        );

        let stripped = without_reserved_graph_edge_residue(complement, 5);
        let mut counterfactual = view;
        counterfactual.edges.insert(
            5,
            GEdge {
                src: 3,
                tgt: 4,
                label: target,
                position: None,
            },
        );
        assert_eq!(
            lens.check_put_domain(&counterfactual, &stripped),
            Err(vec![PutDomainError::DuplicateKey {
                table: "a".to_owned(),
                key: vec![("kept".to_owned(), Value::Str("same".to_owned()))],
                elems: vec![ViewRef::Node(1), ViewRef::Node(3)],
            }])
        );
    }

    #[test]
    fn graph_reserved_edge_omits_a_fan_reserved_label() {
        let (source_schema, target_schema, migration, source, kept) =
            fan_kind_reserved_edge_fixture();
        let plan = must(compile(&source_schema, &target_schema, &migration));
        let lens = must(GraphLens::new(
            &source_schema,
            &target_schema,
            &migration,
            &plan,
            &BTreeMap::new(),
            &NoValueLayer,
        ));
        let (view, complement) = must(lens.get(&source));
        assert_eq!(
            graph_basis(&lens, &view, &complement)
                .iter()
                .filter(|case| case.mutation == Mutation::ReservedIdReuse)
                .count(),
            0
        );

        let mut counterfactual = view;
        counterfactual.edges.insert(
            5,
            GEdge {
                src: 1,
                tgt: 2,
                label: kept,
                position: None,
            },
        );
        assert_eq!(
            lens.check_put_domain(&counterfactual, &complement),
            Err(vec![
                PutDomainError::IdCollision {
                    elem: ViewRef::Edge(5),
                },
                PutDomainError::MalformedFan {
                    node: 1,
                    reason: crate::fan::FanViolation::ReservedKindOutsideFan { edge: 5 },
                },
            ])
        );
        let stripped = without_reserved_graph_edge_residue(complement, 5);
        assert_eq!(
            lens.check_put_domain(&counterfactual, &stripped),
            Err(vec![PutDomainError::MalformedFan {
                node: 1,
                reason: crate::fan::FanViolation::ReservedKindOutsideFan { edge: 5 },
            }])
        );
    }

    #[test]
    fn graph_reserved_node_searches_for_an_identity_restoring_anchor() {
        let source_schema = Schema::default()
            .vertex("a", "record")
            .vertex("b", "record")
            .vertex("c", "record")
            .vertex("dropped", "record");
        let target_schema = Schema::default()
            .vertex("a", "record")
            .vertex("b", "record")
            .vertex("c", "record");
        let migration = Migration {
            surviving_verts: ["a", "b", "c"].into_iter().map(str::to_owned).collect(),
            ..Migration::default()
        };
        let plan = must(compile(&source_schema, &target_schema, &migration));
        let layer = SelectiveRestoreLayer;
        let lens = must(GraphLens::new(
            &source_schema,
            &target_schema,
            &migration,
            &plan,
            &BTreeMap::new(),
            &layer,
        ));
        let source = Graph {
            nodes: [(7, Node::new(7, "dropped"))].into(),
            ..Graph::default()
        };
        let (view, complement) = must(lens.get(&source));
        let cases: Vec<_> = graph_basis(&lens, &view, &complement)
            .into_iter()
            .filter(|case| case.mutation == Mutation::ReservedIdReuse)
            .collect();
        assert_eq!(cases.len(), 1);
        assert_eq!(cases[0].view.nodes[&7].anchor, "c");
        assert_exact_graph_negative(&lens, &complement, &cases[0]);

        let source_schema = Schema::default()
            .vertex("a", "record")
            .vertex("b", "record")
            .vertex("dropped", "record");
        let target_schema = Schema::default()
            .vertex("a", "record")
            .vertex("b", "record");
        let migration = Migration {
            surviving_verts: ["a", "b"].into_iter().map(str::to_owned).collect(),
            ..Migration::default()
        };
        let plan = must(compile(&source_schema, &target_schema, &migration));
        let lens = must(GraphLens::new(
            &source_schema,
            &target_schema,
            &migration,
            &plan,
            &BTreeMap::new(),
            &layer,
        ));
        let (view, complement) = must(lens.get(&source));
        assert_eq!(
            graph_basis(&lens, &view, &complement)
                .iter()
                .filter(|case| case.mutation == Mutation::ReservedIdReuse)
                .count(),
            0,
            "a refusing restore and b changing restore leave no exact witness"
        );
    }

    #[test]
    fn graph_reserved_node_omits_an_unrelated_graph_sensitive_predicate() {
        let source_schema = Schema::default()
            .vertex("watcher", "record")
            .vertex("candidate", "record")
            .vertex("dropped", "record");
        let target_schema = Schema::default()
            .vertex("watcher", "record")
            .vertex("candidate", "record");
        let migration = Migration {
            surviving_verts: ["watcher", "candidate"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
            conditional: ["watcher".to_owned()].into(),
            ..Migration::default()
        };
        let plan = must(compile(&source_schema, &target_schema, &migration));
        let layer = GraphSensitivePredicateLayer;
        let lens = must(GraphLens::new(
            &source_schema,
            &target_schema,
            &migration,
            &plan,
            &BTreeMap::new(),
            &layer,
        ));
        let source = Graph {
            nodes: [(1, Node::new(1, "watcher")), (7, Node::new(7, "dropped"))].into(),
            ..Graph::default()
        };
        let (view, complement) = must(lens.get(&source));
        assert_eq!(
            graph_basis(&lens, &view, &complement)
                .iter()
                .filter(|case| case.mutation == Mutation::ReservedIdReuse)
                .count(),
            0
        );

        let stripped = without_reserved_graph_node_residue(complement, 7);
        let mut counterfactual = view;
        counterfactual.nodes.insert(7, Node::new(7, "candidate"));
        assert_eq!(
            lens.check_put_domain(&counterfactual, &stripped),
            Err(vec![PutDomainError::SurvivalPredicateFails {
                elem: ViewRef::Node(1),
            }])
        );
    }

    #[test]
    fn graph_reserved_node_omits_an_indirect_required_value_constraint() {
        let required_field = SEdge::new("a", "b", "prop", Some("f"));
        let mut constrained_source = Schema::default()
            .vertex("a", "record")
            .vertex("b", "string")
            .vertex("dropped", "record")
            .edge("a", "b", "prop", Some("f"));
        constrained_source.constraints.insert(
            "b".to_owned(),
            vec![("required".to_owned(), "true".to_owned())],
        );
        let target_schema = Schema::default()
            .vertex("a", "record")
            .vertex("b", "string")
            .edge("a", "b", "prop", Some("f"));
        let migration = Migration {
            surviving_verts: ["a", "b"].into_iter().map(str::to_owned).collect(),
            surviving_edges: [required_field].into(),
            ..Migration::default()
        };
        let semantics = BTreeMap::from([("required".to_owned(), Semantics::RequiredValue)]);
        let plan = must(compile(&constrained_source, &target_schema, &migration));
        let lens = must(GraphLens::new(
            &constrained_source,
            &target_schema,
            &migration,
            &plan,
            &semantics,
            &NoValueLayer,
        ));
        let source = dropped_only_graph();
        let (view, complement) = must(lens.get(&source));
        assert!(
            !super::validation_neutral_graph_anchor(&lens, "a"),
            "a constraint on b is still global oracle state"
        );
        assert_eq!(
            graph_basis(&lens, &view, &complement)
                .iter()
                .filter(|case| case.mutation == Mutation::ReservedIdReuse)
                .count(),
            0
        );
        let stripped = without_reserved_graph_node_residue(complement, 7);
        let mut counterfactual = view;
        counterfactual
            .nodes
            .insert(7, super::tree_node_for_anchor(7, lens.tgt, "a"));
        assert_eq!(
            lens.check_put_domain(&counterfactual, &stripped),
            Err(vec![PutDomainError::MissingRequiredCell {
                elem: ViewRef::Node(7),
                field: "f".to_owned(),
            }])
        );
    }

    #[test]
    fn graph_reserved_node_omits_a_nullable_reference() {
        let mut referenced_source = Schema::default()
            .vertex("a", "string")
            .vertex("b", "string")
            .vertex("dropped", "record");
        insert_nullable_reference(&mut referenced_source);
        let referenced_target = Schema::default()
            .vertex("a", "string")
            .vertex("b", "string");
        let referenced_migration = Migration {
            surviving_verts: ["a", "b"].into_iter().map(str::to_owned).collect(),
            ..Migration::default()
        };
        let plan = must(compile(
            &referenced_source,
            &referenced_target,
            &referenced_migration,
        ));
        let lens = must(GraphLens::new(
            &referenced_source,
            &referenced_target,
            &referenced_migration,
            &plan,
            &BTreeMap::new(),
            &NoValueLayer,
        ));
        let source = dropped_only_graph();
        let (view, complement) = must(lens.get(&source));
        assert!(
            !super::validation_neutral_graph_anchor(&lens, "a"),
            "nullable references can still observe a present scalar key"
        );
        assert_eq!(
            graph_basis(&lens, &view, &complement)
                .iter()
                .filter(|case| case.mutation == Mutation::ReservedIdReuse)
                .count(),
            0
        );

        let stripped = without_reserved_graph_node_residue(complement, 7);
        let mut counterfactual = view;
        counterfactual
            .nodes
            .insert(7, super::tree_node_for_anchor(7, lens.tgt, "a"));
        let restored = must(lens.put(&counterfactual, &stripped));
        let validation = crate::pages::validation::implementation::validate_graph(
            lens.src,
            lens.constraint_semantics(),
            &restored,
        );
        assert_eq!(
            validation.status,
            crate::pages::validation::implementation::CheckStatus::Failed
        );
        assert!(validation.diagnostics.iter().any(|diagnostic| matches!(
            &diagnostic.violation,
            Violation::DanglingReference(reference, _) if reference == "nullable-ref"
        )));
    }

    #[test]
    fn graph_reserved_node_basis_rejects_a_fully_dropped_fan_replacement() {
        let mut source_schema = Schema::default()
            .vertex("a", "record")
            .vertex("b", "record");
        source_schema.hyper_edges.insert(
            "h".to_owned(),
            HyperEdge {
                kind: "fan".to_owned(),
                signature: [("l".to_owned(), "b".to_owned())].into(),
                parent_label: "p".to_owned(),
            },
        );
        let target_schema = Schema::default()
            .vertex("a", "record")
            .vertex("b", "record");
        let migration = Migration {
            surviving_verts: ["a".to_owned(), "b".to_owned()].into(),
            ..Migration::default()
        };
        let plan = must(compile(&source_schema, &target_schema, &migration));
        let lens = must(GraphLens::new(
            &source_schema,
            &target_schema,
            &migration,
            &plan,
            &BTreeMap::new(),
            &NoValueLayer,
        ));
        let mut fan = Node::new(2, "h");
        fan.shape = NodeShape::HyperEdge;
        let source = Graph {
            nodes: [(0, Node::new(0, "a")), (1, Node::new(1, "b")), (2, fan)].into(),
            edges: [(
                3,
                GEdge {
                    src: 2,
                    tgt: 0,
                    label: SEdge::new("h", "a", crate::restrict::HYPER_PARENT, Some("p")),
                    position: None,
                },
            )]
            .into(),
        };
        let (view, complement) = must(lens.get(&source));
        assert!(complement.dropped_nodes.contains_key(&2));
        assert_eq!(complement.dropped_edges.len(), 1);

        let mut replacement = view.clone();
        let candidate = Node::new(2, "a");
        replacement.nodes.insert(2, candidate.clone());
        assert!(
            !super::graph_reserved_node_witness_is_structurally_exact(
                &lens,
                &view,
                &complement,
                &candidate,
            ),
            "fan-shaped residue prevents an independently isolated collision witness"
        );
        assert_eq!(
            lens.check_put_domain(&replacement, &complement),
            Err(vec![
                PutDomainError::IdCollision {
                    elem: ViewRef::Node(2),
                },
                PutDomainError::MalformedFan {
                    node: 2,
                    reason: crate::fan::FanViolation::ReservedKindOutsideFan { edge: 3 },
                },
            ])
        );
        let mut stripped = complement.clone();
        assert!(stripped.dropped_nodes.remove(&2).is_some());
        assert_eq!(
            lens.check_put_domain(&replacement, &stripped),
            Err(vec![PutDomainError::MalformedFan {
                node: 2,
                reason: crate::fan::FanViolation::ReservedKindOutsideFan { edge: 3 },
            }])
        );
        assert!(
            graph_basis(&lens, &view, &complement)
                .iter()
                .filter(|case| case.mutation == Mutation::ReservedIdReuse)
                .all(|case| {
                    case.domain
                        != DomainExpectation::Refused(ReferenceLawError::PutDomain(vec![
                            PutDomainError::IdCollision {
                                elem: ViewRef::Node(2),
                            },
                        ]))
                }),
            "a mixed collision/fan failure is not a singleton reserved-id basis case"
        );
    }
}

#[cfg(test)]
mod table_basis_tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::fmt::Debug;

    use super::{
        BasisCase, DomainExpectation, Mutation, TableLawError, push_table_refused, table_basis,
        table_first_put_error,
    };
    use crate::keys::{KeyEquality, SqlCollation, SqlPad};
    use crate::migration::Migration;
    use crate::model::{SEdge, Table};
    use crate::pages::lenses::domain::{PutDomainError, ViewRef};
    use crate::pages::lenses::table::{
        self, TableBoundaries, TableCheckError, TableComplement, TableLens, TablePutError,
    };
    use crate::rel::{self, RelSchema};
    use crate::restrict::NoValueLayer;
    use crate::schema::{Key, KeyPart, KeyRole, KeyScope, Schema};
    use crate::value::Value;

    fn without_reserved_table_row_residue(
        mut complement: TableComplement,
        id: u32,
    ) -> TableComplement {
        complement.dropped_rows.remove(&id);
        complement.dropped_cells.retain(|(row, _), _| *row != id);
        for pairs in complement.dropped_fk_pairs.values_mut() {
            pairs.retain(|(source, target)| *source != id && *target != id);
        }
        complement
            .dropped_fk_pairs
            .retain(|_, pairs| !pairs.is_empty());
        complement
            .original_fk_edges
            .retain(|(_, source, target), _| *source != id && *target != id);
        complement.original_tables.remove(&id);
        complement.original_cells.retain(|(row, _), _| *row != id);
        complement
            .synthesized_fk_pairs
            .retain(|(_, source, target)| *source != id && *target != id);
        complement
    }

    fn assert_reserved_table_counterfactual(
        lens: &TableLens<'_>,
        case: &BasisCase<Table, TableLawError>,
        complement: &TableComplement,
        stripped: &TableComplement,
    ) {
        let DomainExpectation::Refused(TableLawError::Domain(expected)) = &case.domain else {
            panic!("reserved table witness must declare a complete refusal")
        };
        assert_eq!(
            table::check_put_domain(lens, &case.view, complement),
            Err(expected.clone())
        );
        assert_eq!(table::check_put_domain(lens, &case.view, stripped), Ok(()));
        let restored = must(table::put(lens, &case.view, stripped));
        let validation = must(crate::pages::validation::implementation::validate_table(
            lens.source_schema(),
            &BTreeMap::new(),
            &restored,
        ));
        assert_eq!(
            validation.status,
            crate::pages::validation::implementation::CheckStatus::Passed
        );
        let (regotten, regotten_complement) = must(lens.get(&restored));
        assert!(<super::TableShape as super::LensShape>::equivalent(
            &case.view,
            &<super::TableShape as super::LensShape>::synth_index(&case.view, stripped),
            &regotten,
            &<super::TableShape as super::LensShape>::synth_index(&regotten, &regotten_complement,),
        ));
    }

    fn named_collision_schemas() -> (Schema, Schema, Migration) {
        let key_column = SEdge::new("kept", "scalar", "field", Some("key"));
        let mut source = Schema::default()
            .vertex("kept", "record")
            .vertex("dropped", "record")
            .vertex("scalar", "string")
            .edge("kept", "scalar", "field", Some("key"));
        source.keys.insert(
            "kept".to_owned(),
            vec![Key {
                id: "kept-key".to_owned(),
                parts: vec![KeyPart::Path(vec![key_column.clone()])],
                role: KeyRole::Unique,
                scope: KeyScope::Global,
                equality: vec![KeyEquality::Sql {
                    collation: SqlCollation::Named("named_ci".to_owned()),
                    pad: SqlPad::NoPad,
                }],
            }],
        );
        let target = Schema::default()
            .vertex("kept", "record")
            .vertex("scalar", "string")
            .edge("kept", "scalar", "field", Some("key"));
        let migration = Migration {
            surviving_verts: ["kept".to_owned(), "scalar".to_owned()].into(),
            surviving_edges: [key_column].into(),
            ..Migration::default()
        };
        (source, target, migration)
    }

    fn named_collision_source() -> Table {
        Table {
            rows: [
                (
                    1,
                    (
                        "kept".to_owned(),
                        [("key".to_owned(), Value::Str("alpha".to_owned()))].into(),
                    ),
                ),
                (2, ("dropped".to_owned(), BTreeMap::new())),
            ]
            .into(),
            ..Table::default()
        }
    }

    fn contracted_table_pair_fixture() -> (Schema, Schema, Migration, Table) {
        let source_schema = Schema::default()
            .vertex("a", "record")
            .vertex("m", "record")
            .vertex("b", "record")
            .edge("a", "m", "fk", Some("am"))
            .edge("m", "b", "fk", Some("mb"));
        let target_schema = Schema::default()
            .vertex("a", "record")
            .vertex("b", "record")
            .edge("a", "b", "fk", Some("ab"));
        let label = SEdge::new("a", "b", "fk", Some("ab"));
        let migration = Migration {
            surviving_verts: ["a".to_owned(), "b".to_owned()].into(),
            resolver: [(("a".to_owned(), "b".to_owned()), label)].into(),
            ..Migration::default()
        };
        let source = Table {
            rows: [
                (1, ("a".to_owned(), BTreeMap::new())),
                (3, ("b".to_owned(), BTreeMap::new())),
            ]
            .into(),
            ..Table::default()
        };
        (source_schema, target_schema, migration, source)
    }

    fn must<T, E: Debug>(result: Result<T, E>) -> T {
        result.unwrap_or_else(|error| panic!("unexpected error: {error:?}"))
    }

    fn record_boundary(schema: &Schema) -> RelSchema {
        must(rel::classify(
            schema,
            &BTreeSet::new(),
            &BTreeSet::from(["record".to_owned()]),
        ))
    }

    fn record_string_boundary(schema: &Schema) -> RelSchema {
        must(rel::classify(
            schema,
            &BTreeSet::from(["string".to_owned()]),
            &BTreeSet::from(["record".to_owned()]),
        ))
    }

    fn assert_exact_table_negatives(
        lens: &TableLens<'_>,
        complement: &TableComplement,
        cases: &[BasisCase<Table, TableLawError>],
        mutation: Mutation,
    ) {
        let selected: Vec<_> = cases
            .iter()
            .filter(|case| case.mutation == mutation)
            .collect();
        assert!(
            !selected.is_empty(),
            "{mutation:?} fixture must be nonvacuous"
        );
        for case in selected {
            let DomainExpectation::Refused(TableLawError::Domain(expected)) = &case.domain else {
                panic!("negative case lacks an independent complete refusal")
            };
            assert_eq!(
                table::check_put_domain(lens, &case.view, complement),
                Err(expected.clone())
            );
            assert_eq!(
                table::put(lens, &case.view, complement).err(),
                table_first_put_error(expected)
            );
        }
    }

    #[test]
    fn table_mixed_domain_report_keeps_unrelated_named_collation_evidence() {
        let (source_schema, target_schema, migration) = named_collision_schemas();
        let source_boundary = record_string_boundary(&source_schema);
        let target_boundary = record_string_boundary(&target_schema);
        let plan = must(table::compile_for_tables(
            &source_schema,
            &target_schema,
            &source_boundary,
            &target_boundary,
            &migration,
        ));
        let lens = must(TableLens::new_at_boundary(
            &source_schema,
            &target_schema,
            TableBoundaries::new(&source_boundary, &target_boundary),
            &migration,
            &plan,
            &BTreeMap::new(),
            &NoValueLayer,
        ));
        let source = named_collision_source();
        let (view, complement) = must(lens.get(&source));
        let mut edited = view.clone();
        edited.rows.insert(
            2,
            (
                "kept".to_owned(),
                [("key".to_owned(), Value::Str("beta".to_owned()))].into(),
            ),
        );
        let violation = PutDomainError::IdCollision {
            elem: ViewRef::Row(2),
        };
        let unresolved = BTreeSet::from(["named_ci".to_owned()]);
        let expected = TableCheckError::Domain {
            violations: vec![violation.clone()],
            unresolved_collations: unresolved.clone(),
        };
        let mut cases = Vec::new();
        push_table_refused(
            &mut cases,
            Mutation::ReservedIdReuse,
            edited.clone(),
            vec![violation.clone()],
            unresolved,
        );

        assert_eq!(
            cases[0].domain,
            DomainExpectation::Refused(TableLawError::Domain(expected.clone()))
        );
        assert_eq!(
            table::check_put_domain(&lens, &edited, &complement),
            Err(expected)
        );
        assert_eq!(
            table::put(&lens, &edited, &complement),
            Err(TablePutError::Domain(violation))
        );
        assert_eq!(
            table_basis(&lens, &view, &complement)
                .iter()
                .filter(|case| matches!(
                    case.mutation,
                    Mutation::RemoveSynthesized
                        | Mutation::EditSynthesized
                        | Mutation::AddContractedEdge
                        | Mutation::ReservedIdReuse
                ))
                .count(),
            0,
            "the independent basis must not guess candidate-local collation evidence"
        );
    }

    #[test]
    fn table_synthesized_pair_negatives_are_exact_and_nonvacuous() {
        let am = SEdge::new("a", "m", "fk", Some("am"));
        let mb = SEdge::new("m", "b", "fk", Some("mb"));
        let direct = SEdge::new("a", "b", "fk", Some("ab"));
        let source_schema = Schema::default()
            .vertex("a", "record")
            .vertex("m", "record")
            .vertex("b", "record")
            .edge("a", "m", "fk", Some("am"))
            .edge("m", "b", "fk", Some("mb"))
            .edge("a", "b", "fk", Some("ab"));
        let target_schema = Schema::default()
            .vertex("a", "record")
            .vertex("b", "record")
            .edge("a", "b", "fk", Some("ab"));
        let migration = Migration {
            surviving_verts: ["a".to_owned(), "b".to_owned()].into(),
            surviving_edges: [direct.clone()].into(),
            resolver: [(("a".to_owned(), "b".to_owned()), direct.clone())].into(),
            ..Migration::default()
        };
        let source_boundary = record_boundary(&source_schema);
        let target_boundary = record_boundary(&target_schema);
        let plan = must(table::compile_for_tables(
            &source_schema,
            &target_schema,
            &source_boundary,
            &target_boundary,
            &migration,
        ));
        let lens = must(TableLens::new_at_boundary(
            &source_schema,
            &target_schema,
            TableBoundaries::new(&source_boundary, &target_boundary),
            &migration,
            &plan,
            &BTreeMap::new(),
            &NoValueLayer,
        ));
        let source = Table {
            rows: [
                (1, ("a".to_owned(), BTreeMap::new())),
                (2, ("m".to_owned(), BTreeMap::new())),
                (3, ("b".to_owned(), BTreeMap::new())),
                (4, ("a".to_owned(), BTreeMap::new())),
                (5, ("b".to_owned(), BTreeMap::new())),
            ]
            .into(),
            pairs: [(am, 1, 2), (mb, 2, 3), (direct, 1, 3)].into(),
        };
        let (view, complement) = must(lens.get(&source));
        let cases = table_basis(&lens, &view, &complement);
        assert_exact_table_negatives(&lens, &complement, &cases, Mutation::RemoveSynthesized);
        assert_exact_table_negatives(&lens, &complement, &cases, Mutation::EditSynthesized);
    }

    #[test]
    fn table_contracted_pair_negative_is_exact_and_nonvacuous() {
        let (source_schema, target_schema, migration, source) = contracted_table_pair_fixture();
        let source_boundary = record_boundary(&source_schema);
        let target_boundary = record_boundary(&target_schema);
        let plan = must(table::compile_for_tables(
            &source_schema,
            &target_schema,
            &source_boundary,
            &target_boundary,
            &migration,
        ));
        let lens = must(TableLens::new_at_boundary(
            &source_schema,
            &target_schema,
            TableBoundaries::new(&source_boundary, &target_boundary),
            &migration,
            &plan,
            &BTreeMap::new(),
            &NoValueLayer,
        ));
        let (view, complement) = must(lens.get(&source));
        let cases = table_basis(&lens, &view, &complement);
        assert_exact_table_negatives(&lens, &complement, &cases, Mutation::AddContractedEdge);
    }

    #[test]
    fn table_contracted_pair_omits_unrelated_key_state() {
        let (mut source_schema, target_schema, migration, source) = contracted_table_pair_fixture();
        source_schema.keys.insert(
            "m".to_owned(),
            vec![Key {
                id: "unrelated".to_owned(),
                parts: vec![KeyPart::Path(Vec::new())],
                role: KeyRole::Unique,
                scope: KeyScope::Global,
                equality: Vec::new(),
            }],
        );
        let source_boundary = record_boundary(&source_schema);
        let target_boundary = record_boundary(&target_schema);
        let plan = must(table::compile_for_tables(
            &source_schema,
            &target_schema,
            &source_boundary,
            &target_boundary,
            &migration,
        ));
        let lens = must(TableLens::new_at_boundary(
            &source_schema,
            &target_schema,
            TableBoundaries::new(&source_boundary, &target_boundary),
            &migration,
            &plan,
            &BTreeMap::new(),
            &NoValueLayer,
        ));
        let (view, complement) = must(lens.get(&source));
        assert_eq!(
            table_basis(&lens, &view, &complement)
                .iter()
                .filter(|case| case.mutation == Mutation::AddContractedEdge)
                .count(),
            0
        );
    }

    #[test]
    fn table_reserved_row_negative_is_exact_and_nonvacuous() {
        let source_schema = Schema::default()
            .vertex("kept", "record")
            .vertex("dropped", "record");
        let target_schema = Schema::default().vertex("kept", "record");
        let migration = Migration {
            surviving_verts: ["kept".to_owned()].into(),
            ..Migration::default()
        };
        let source_boundary = record_boundary(&source_schema);
        let target_boundary = record_boundary(&target_schema);
        let plan = must(table::compile_for_tables(
            &source_schema,
            &target_schema,
            &source_boundary,
            &target_boundary,
            &migration,
        ));
        let lens = must(TableLens::new_at_boundary(
            &source_schema,
            &target_schema,
            TableBoundaries::new(&source_boundary, &target_boundary),
            &migration,
            &plan,
            &BTreeMap::new(),
            &NoValueLayer,
        ));
        let source = Table {
            rows: [
                (1, ("kept".to_owned(), BTreeMap::new())),
                (2, ("dropped".to_owned(), BTreeMap::new())),
            ]
            .into(),
            ..Table::default()
        };
        let (view, complement) = must(lens.get(&source));
        let cases = table_basis(&lens, &view, &complement);
        assert_exact_table_negatives(&lens, &complement, &cases, Mutation::ReservedIdReuse);
        let reserved: Vec<_> = cases
            .iter()
            .filter(|case| case.mutation == Mutation::ReservedIdReuse)
            .collect();
        assert_eq!(reserved.len(), 1);
        let stripped = without_reserved_table_row_residue(complement.clone(), 2);
        assert_reserved_table_counterfactual(&lens, reserved[0], &complement, &stripped);
    }

    #[test]
    fn table_reserved_row_omits_an_empty_witness_for_a_nonempty_table() {
        let column = SEdge::new("kept", "scalar", "field", Some("value"));
        let source_schema = Schema::default()
            .vertex("kept", "record")
            .vertex("dropped", "record")
            .vertex("scalar", "string")
            .edge("kept", "scalar", "field", Some("value"));
        let target_schema = Schema::default()
            .vertex("kept", "record")
            .vertex("scalar", "string")
            .edge("kept", "scalar", "field", Some("value"));
        let migration = Migration {
            surviving_verts: ["kept".to_owned(), "scalar".to_owned()].into(),
            surviving_edges: [column].into(),
            ..Migration::default()
        };
        let source_boundary = record_string_boundary(&source_schema);
        let target_boundary = record_string_boundary(&target_schema);
        let plan = must(table::compile_for_tables(
            &source_schema,
            &target_schema,
            &source_boundary,
            &target_boundary,
            &migration,
        ));
        let lens = must(TableLens::new_at_boundary(
            &source_schema,
            &target_schema,
            TableBoundaries::new(&source_boundary, &target_boundary),
            &migration,
            &plan,
            &BTreeMap::new(),
            &NoValueLayer,
        ));
        let source = Table {
            rows: [
                (
                    1,
                    (
                        "kept".to_owned(),
                        [("value".to_owned(), Value::Str("present".to_owned()))].into(),
                    ),
                ),
                (2, ("dropped".to_owned(), BTreeMap::new())),
            ]
            .into(),
            ..Table::default()
        };
        let (view, complement) = must(lens.get(&source));
        assert_eq!(
            complement.dropped_rows.keys().copied().collect::<Vec<_>>(),
            vec![2]
        );
        assert_eq!(
            table_basis(&lens, &view, &complement)
                .iter()
                .filter(|case| case.mutation == Mutation::ReservedIdReuse)
                .count(),
            0
        );
    }
}

#[cfg(test)]
mod locality_tests {
    use std::collections::{BTreeMap, BTreeSet};

    use super::{
        DomainExpectation, EdgeFate, LawGateStatus, Mutation, PutDomainError, ReferenceLawError,
        TreeLawComplement, ViewRef, check_tree_laws, collision_free_tree_rekey, fresh_tree_edge,
        fresh_tree_node, reference_lens_error, tree_basis, tree_locality_divergence,
        tree_source_arcs,
    };
    use crate::encode::TreeEmbedding;
    use crate::enumerate::lenses::{C11Layer, C11Stage, c11_edge, c11_stages};
    use crate::migration::Migration;
    use crate::model::{GEdge, Node, SEdge, Tree};
    use crate::pages::lenses::graph::{GraphLens, LensError};
    use crate::pages::lenses::tree::{
        TreeOperationalError, tree_check_put_domain, tree_check_put_domain_law, tree_put,
    };
    use crate::restrict::{Complement, NoValueLayer, SynthKey, compile};
    use crate::schema::{
        Cardinality, Key, KeyPart, KeyRole, KeyScope, NullMatch, OnUnresolved, RefSource,
        Reference, ResolveMode, Schema,
    };
    use crate::value::{Presence, Value};

    fn label() -> SEdge {
        SEdge::new("r", "leaf", "child", Some("child"))
    }

    fn insert_reference(schema: &mut Schema, nullable: bool) {
        schema.keys.entry("b".to_owned()).or_default().push(Key {
            id: "reference-target".to_owned(),
            parts: vec![KeyPart::Path(Vec::new())],
            role: KeyRole::Unique,
            scope: KeyScope::Global,
            equality: Vec::new(),
        });
        let reference = Reference {
            id: "required-ref".to_owned(),
            holder: "a".to_owned(),
            source: RefSource::KeyValue {
                parts: vec![Vec::new()],
            },
            target_vertex: "b".to_owned(),
            target_key: Some("reference-target".to_owned()),
            edge: None,
            cardinality: Cardinality::One,
            nullable,
            on_unresolved: OnUnresolved::Error,
            mode: ResolveMode::AddEdge,
            guard: None,
            null_match: NullMatch::Simple,
            null_vertex: None,
            scope_via: None,
        };
        schema.references.insert(reference.id.clone(), reference);
    }

    fn insert_nonnullable_reference(schema: &mut Schema) {
        insert_reference(schema, false);
    }

    fn contraction_source() -> Tree {
        Tree {
            nodes: [
                (0, Node::new(0, "a")),
                (1, Node::new(1, "a")),
                (2, Node::new(2, "b")),
            ]
            .into(),
            arcs: vec![(0, 2, c11_edge("a", "b")), (2, 1, c11_edge("b", "a"))],
            fans: Vec::new(),
            root: 0,
        }
    }

    fn reserved_source() -> Tree {
        Tree {
            nodes: [(0, Node::new(0, "a")), (2, Node::new(2, "b"))].into(),
            arcs: vec![(0, 2, c11_edge("a", "b"))],
            fans: Vec::new(),
            root: 0,
        }
    }

    #[derive(Clone, Copy)]
    enum ReservedAttachment {
        FirstEligible,
        LaterEligible,
        None,
    }

    fn synthesized_parent_reserved_fixture(
        attachment: ReservedAttachment,
    ) -> (Schema, Schema, Migration, Tree) {
        let ab = SEdge::new("a", "b", "rel", Some("path"));
        let za = SEdge::new("z", "a", "rel", Some("to-a"));
        let zm = SEdge::new("z", "m", "rel", Some("to-m"));
        let zy = SEdge::new("z", "y", "rel", Some("to-y"));
        let mx = SEdge::new("m", "x", "rel", Some("attach"));
        let md = SEdge::new("m", "d", "rel", Some("drop"));
        let zd = SEdge::new("z", "d", "rel", Some("drop"));
        let yd = SEdge::new("y", "d", "rel", Some("drop"));
        let (drop_parent, dropped) = match attachment {
            ReservedAttachment::FirstEligible => (10, md),
            ReservedAttachment::LaterEligible => (0, zd),
            ReservedAttachment::None => (11, yd),
        };
        let source_schema = Schema::default()
            .vertex("z", "record")
            .vertex("a", "record")
            .vertex("m", "record")
            .vertex("b", "record")
            .vertex("x", "record")
            .vertex("y", "record")
            .vertex("d", "record")
            .edge("z", "a", "rel", Some("to-a"))
            .edge("z", "m", "rel", Some("to-m"))
            .edge("z", "y", "rel", Some("to-y"))
            .edge("a", "b", "rel", Some("path"))
            .edge("m", "x", "rel", Some("attach"))
            .edge(&dropped.src, &dropped.tgt, "rel", Some("drop"));
        let target_schema = Schema::default()
            .vertex("z", "record")
            .vertex("a", "record")
            .vertex("m", "record")
            .vertex("b", "record")
            .vertex("x", "record")
            .vertex("y", "record")
            .edge("a", "m", "rel", Some("path-in"))
            .edge("m", "b", "rel", Some("path-out"))
            .edge("m", "x", "rel", Some("attach"))
            .edge("z", "a", "rel", Some("to-a"))
            .edge("z", "m", "rel", Some("to-m"))
            .edge("z", "y", "rel", Some("to-y"));
        let migration = Migration {
            surviving_verts: ["z", "a", "m", "b", "x", "y"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
            surviving_edges: [za.clone(), zm.clone(), zy.clone(), mx]
                .into_iter()
                .collect(),
            expansion_path: [(("a".to_owned(), "b".to_owned()), vec!["m".to_owned()])].into(),
            ..Migration::default()
        };
        let source = Tree {
            nodes: [
                (0, Node::new(0, "z")),
                (1, Node::new(1, "b")),
                (2, Node::new(2, "d")),
                (4, Node::new(4, "a")),
                (10, Node::new(10, "m")),
                (11, Node::new(11, "y")),
            ]
            .into(),
            arcs: vec![
                (0, 4, za),
                (4, 1, ab),
                (0, 10, zm),
                (0, 11, zy),
                (drop_parent, 2, dropped),
            ],
            fans: Vec::new(),
            root: 0,
        };
        (source_schema, target_schema, migration, source)
    }

    fn without_reserved_tree_residue(
        mut complement: TreeLawComplement,
        id: u32,
    ) -> TreeLawComplement {
        let incident_source_edges: BTreeSet<_> = complement
            .source_arcs
            .iter()
            .filter(|(_, (parent, child))| *parent == id || *child == id)
            .map(|(edge, _)| *edge)
            .collect();
        complement
            .source_arcs
            .retain(|_, (parent, child)| *parent != id && *child != id);
        complement.embedding.arcs.remove(&id);
        complement.complement.dropped_nodes.remove(&id);
        complement
            .complement
            .dropped_edges
            .retain(|_, edge| edge.src != id && edge.tgt != id);
        complement
            .complement
            .original_edges
            .retain(|_, edge| edge.src != id && edge.tgt != id);
        complement.complement.original_anchors.remove(&id);
        complement.complement.original_fields.remove(&id);
        complement.complement.original_values.remove(&id);
        complement.complement.synthesized_nodes.retain(|node, key| {
            *node != id
                && match key {
                    SynthKey::Contract { src, tgt, .. }
                    | SynthKey::ContractExpand { src, tgt, .. } => *src != id && *tgt != id,
                    SynthKey::Expand { source_edge, .. } => {
                        !incident_source_edges.contains(source_edge)
                    }
                }
        });
        complement
            .complement
            .synthesized_edges
            .retain(|_, key| match key {
                SynthKey::Contract { src, tgt, .. } | SynthKey::ContractExpand { src, tgt, .. } => {
                    *src != id && *tgt != id
                }
                SynthKey::Expand { source_edge, .. } => {
                    !incident_source_edges.contains(source_edge)
                }
            });
        complement
    }

    fn assert_reserved_tree_counterfactual(
        lens: &GraphLens<'_>,
        candidate: &super::BasisCase<Tree, ReferenceLawError>,
        complement: &TreeLawComplement,
        id: u32,
    ) {
        let expected = PutDomainError::IdCollision {
            elem: ViewRef::Node(id),
        };
        assert_eq!(
            candidate.domain,
            DomainExpectation::Refused(ReferenceLawError::PutDomain(vec![expected.clone()]))
        );
        let Err(LensError::PutDomainReport(observed)) = tree_check_put_domain(
            lens,
            &candidate.view,
            &complement.embedding,
            &complement.complement,
        ) else {
            panic!("the reused id must be the complete domain report");
        };
        assert_eq!(observed, vec![expected.clone()]);
        assert_eq!(
            tree_put(
                lens,
                &candidate.view,
                &complement.embedding,
                &complement.complement,
            ),
            Err(LensError::PutDomain(expected))
        );

        let stripped = without_reserved_tree_residue(complement.clone(), id);
        assert_eq!(
            tree_check_put_domain(
                lens,
                &candidate.view,
                &stripped.embedding,
                &stripped.complement,
            ),
            Ok(())
        );
        let Ok(restored) = tree_put(
            lens,
            &candidate.view,
            &stripped.embedding,
            &stripped.complement,
        ) else {
            panic!("removing the colliding residue admits the witness");
        };
        let Ok(validation) = crate::pages::validation::implementation::validate_tree(
            lens.src,
            &BTreeMap::new(),
            &restored,
        ) else {
            panic!("the counterfactual source has a tree presentation");
        };
        assert_eq!(
            validation.status,
            crate::pages::validation::implementation::CheckStatus::Passed
        );
        let Some(Ok((roundtrip, graph, embedding))) = super::tree_get(lens, &restored) else {
            panic!("the counterfactual source has a tree view");
        };
        let roundtrip_complement = TreeLawComplement {
            complement: graph,
            embedding,
            source_arcs: tree_source_arcs(&restored),
        };
        assert!(<super::TreeShape as super::LensShape>::equivalent(
            &candidate.view,
            &super::tree_synth_index(&stripped),
            &roundtrip,
            &super::tree_synth_index(&roundtrip_complement),
        ));
    }

    fn stage_tree_basis(
        stage: &C11Stage,
        source: &Tree,
    ) -> Vec<super::BasisCase<Tree, ReferenceLawError>> {
        let Ok(plan) = compile(&stage.source, &stage.target, &stage.migration) else {
            panic!("the stage compiles");
        };
        let semantics = BTreeMap::new();
        let Ok(lens) = GraphLens::new(
            &stage.source,
            &stage.target,
            &stage.migration,
            &plan,
            &semantics,
            &NoValueLayer,
        ) else {
            panic!("the stage lens constructs");
        };
        let Some(Ok((view, graph, embedding))) = super::tree_get(&lens, source) else {
            panic!("the stage has a tree view");
        };
        let complement = TreeLawComplement {
            complement: graph,
            embedding,
            source_arcs: tree_source_arcs(source),
        };
        tree_basis(&lens, &view, &complement)
    }

    fn adversarial_contraction_edit_stage(include_actual_preimage: bool) -> (C11Stage, SEdge) {
        let stages = c11_stages();
        let Some(base) = stages.iter().find(|stage| stage.name == "C") else {
            panic!("the contraction stage exists");
        };
        let mut stage = base.clone();
        let replacement = SEdge::new("a", "a", "e", Some("replacement"));
        let source_cc = SEdge::new("c", "c", "e", Some("replacement"));
        let kind = stage.source.vertices["a"].clone();
        stage.source.vertices.insert("c".to_owned(), kind);
        stage.source.edges.insert(source_cc.clone());
        stage.target.edges.insert(replacement.clone());
        stage
            .migration
            .vertex_remap
            .insert("c".to_owned(), "a".to_owned());
        stage
            .migration
            .edge_remap
            .insert(source_cc, replacement.clone());
        if include_actual_preimage {
            stage.source.edges.insert(replacement.clone());
            stage
                .migration
                .edge_remap
                .insert(replacement.clone(), replacement.clone());
        }
        (stage, replacement)
    }

    #[test]
    fn reference_lens_error_preserves_both_tree_operational_variants() {
        let shape = TreeOperationalError::RestoredNotATree {
            reason: crate::encode::NotATree::SecondParent { node: 42 },
        };
        let exhausted = TreeOperationalError::IdsExhausted { nodes: true };

        assert_eq!(
            reference_lens_error(LensError::TreeOperational(shape.clone())),
            ReferenceLawError::TreeOperational(shape)
        );
        assert_eq!(
            reference_lens_error(LensError::TreeOperational(exhausted.clone())),
            ReferenceLawError::TreeOperational(exhausted)
        );
    }

    #[test]
    fn tree_law_reports_and_raw_put_use_native_arc_order() {
        let represented = SEdge::new("r", "r", "child", Some("represented"));
        let unrepresentable = SEdge::new("r", "r", "child", Some("unrepresentable"));
        let schema =
            Schema::default()
                .vertex("r", "record")
                .edge("r", "r", "child", Some("represented"));
        let migration = Migration {
            surviving_verts: ["r".to_owned()].into(),
            surviving_edges: [represented.clone()].into(),
            ..Migration::default()
        };
        let plan = compile(&schema, &schema, &migration)
            .unwrap_or_else(|error| panic!("the identity fixture compiles: {error:?}"));
        let lens = GraphLens::new(
            &schema,
            &schema,
            &migration,
            &plan,
            &BTreeMap::new(),
            &NoValueLayer,
        )
        .unwrap_or_else(|error| panic!("the identity lens constructs: {error:?}"));
        let source = Tree {
            nodes: [
                (0, Node::new(0, "r")),
                (1, Node::new(1, "r")),
                (2, Node::new(2, "r")),
            ]
            .into(),
            // Carrier edge 0 names Arc(0,2), while carrier edge 1 names
            // Arc(0,1): graph-id and native tree orders are opposite.
            arcs: vec![(0, 2, represented.clone()), (0, 1, represented)],
            fans: Vec::new(),
            root: 0,
        };
        let Some(Ok((mut view, graph, embedding))) = super::tree_get(&lens, &source) else {
            panic!("the fixture has a tree view");
        };
        let complement = TreeLawComplement {
            complement: graph,
            embedding,
            source_arcs: tree_source_arcs(&source),
        };
        for (_, _, label) in &mut view.arcs {
            label.clone_from(&unrepresentable);
        }

        let graph_order = vec![
            PutDomainError::UnrepresentableViewEdge {
                elem: ViewRef::Edge(0),
            },
            PutDomainError::UnrepresentableViewEdge {
                elem: ViewRef::Edge(1),
            },
        ];
        assert_eq!(
            tree_check_put_domain(&lens, &view, &complement.embedding, &complement.complement,),
            Err(LensError::PutDomainReport(graph_order))
        );

        let native_order = vec![
            PutDomainError::UnrepresentableViewEdge {
                elem: ViewRef::Arc {
                    parent: 0,
                    child: 1,
                },
            },
            PutDomainError::UnrepresentableViewEdge {
                elem: ViewRef::Arc {
                    parent: 0,
                    child: 2,
                },
            },
        ];
        assert_eq!(
            <super::TreeShape as super::LawShape<GraphLens<'_>>>::check_put_domain(
                &lens,
                &view,
                &complement,
                &(),
            ),
            Err(ReferenceLawError::PutDomain(native_order.clone()))
        );
        let raw = <super::TreeShape as super::LawShape<GraphLens<'_>>>::put(
            &lens,
            &view,
            &complement,
            &(),
        );
        assert_eq!(
            raw,
            Err(ReferenceLawError::PutDomain(vec![native_order[0].clone()]))
        );
        assert_eq!(
            <super::TreeShape as super::LawShape<GraphLens<'_>>>::first_put_refusal(
                &ReferenceLawError::PutDomain(native_order)
            ),
            raw.err()
        );
    }

    #[test]
    fn tree_law_structural_reparent_names_the_presented_arc() {
        let child = SEdge::new("r", "leaf", "child", Some("child"));
        let branch = SEdge::new("r", "x", "child", Some("branch"));
        let schema = Schema::default()
            .vertex("r", "record")
            .vertex("x", "record")
            .vertex("leaf", "record")
            .edge("r", "leaf", "child", Some("child"))
            .edge("r", "x", "child", Some("branch"));
        let migration = Migration {
            surviving_verts: schema.vertices.keys().cloned().collect(),
            surviving_edges: schema.edges.clone(),
            ..Migration::default()
        };
        let plan = compile(&schema, &schema, &migration)
            .unwrap_or_else(|error| panic!("the identity fixture compiles: {error:?}"));
        let lens = GraphLens::new(
            &schema,
            &schema,
            &migration,
            &plan,
            &BTreeMap::new(),
            &NoValueLayer,
        )
        .unwrap_or_else(|error| panic!("the identity lens constructs: {error:?}"));
        let source = Tree {
            nodes: [
                (0, Node::new(0, "r")),
                (1, Node::new(1, "leaf")),
                (2, Node::new(2, "x")),
            ]
            .into(),
            arcs: vec![(0, 1, child), (0, 2, branch)],
            fans: Vec::new(),
            root: 0,
        };
        let Some(Ok((mut view, graph, embedding))) = super::tree_get(&lens, &source) else {
            panic!("the fixture has a tree view");
        };
        let complement = TreeLawComplement {
            complement: graph,
            embedding,
            source_arcs: tree_source_arcs(&source),
        };
        let Some(arc) = view.arcs.iter_mut().find(|(_, child, _)| *child == 1) else {
            panic!("the child arc is present");
        };
        arc.0 = 2;

        let edge = complement.embedding.arcs[&1];
        assert_eq!(
            tree_check_put_domain(&lens, &view, &complement.embedding, &complement.complement,),
            Err(LensError::PutDomainReport(vec![
                PutDomainError::UnrepresentableViewEdge {
                    elem: ViewRef::Edge(edge),
                },
            ]))
        );
        let native = PutDomainError::UnrepresentableViewEdge {
            elem: ViewRef::Arc {
                parent: 2,
                child: 1,
            },
        };
        assert_eq!(
            tree_check_put_domain_law(&lens, &view, &complement.embedding, &complement.complement,),
            Err(LensError::PutDomainReport(vec![native.clone()]))
        );
        assert_eq!(
            super::tree_put_law(&lens, &view, &complement.embedding, &complement.complement,),
            Err(LensError::PutDomain(native))
        );
    }

    #[test]
    fn tree_law_synthesized_reparent_retains_original_and_current_arcs() {
        let source_edge = SEdge::new("a", "b", "rel", Some("path"));
        let branch = SEdge::new("a", "a", "rel", Some("branch"));
        let source_schema = Schema::default()
            .vertex("a", "record")
            .vertex("b", "record")
            .edge("a", "b", "rel", Some("path"))
            .edge("a", "a", "rel", Some("branch"));
        let target_schema = Schema::default()
            .vertex("a", "record")
            .vertex("m", "record")
            .vertex("b", "record")
            .edge("a", "m", "rel", Some("path-in"))
            .edge("m", "b", "rel", Some("path-out"))
            .edge("a", "a", "rel", Some("branch"));
        let migration = Migration {
            surviving_verts: ["a", "m", "b"].into_iter().map(str::to_owned).collect(),
            surviving_edges: [branch.clone()].into(),
            expansion_path: [(("a".to_owned(), "b".to_owned()), vec!["m".to_owned()])].into(),
            ..Migration::default()
        };
        let plan = compile(&source_schema, &target_schema, &migration)
            .unwrap_or_else(|error| panic!("the expansion fixture compiles: {error:?}"));
        let lens = GraphLens::new(
            &source_schema,
            &target_schema,
            &migration,
            &plan,
            &BTreeMap::new(),
            &NoValueLayer,
        )
        .unwrap_or_else(|error| panic!("the expansion lens constructs: {error:?}"));
        let source = Tree {
            nodes: [
                (0, Node::new(0, "a")),
                (1, Node::new(1, "b")),
                (2, Node::new(2, "a")),
            ]
            .into(),
            arcs: vec![(0, 1, source_edge), (0, 2, branch)],
            fans: Vec::new(),
            root: 0,
        };
        let Some(Ok((mut view, graph, embedding))) = super::tree_get(&lens, &source) else {
            panic!("the fixture has an expanded tree view");
        };
        let complement = TreeLawComplement {
            complement: graph,
            embedding,
            source_arcs: tree_source_arcs(&source),
        };
        let Some((&synthesized, _)) = complement.complement.synthesized_nodes.iter().next() else {
            panic!("the expanded path has an intermediate node");
        };
        let Some(arc) = view
            .arcs
            .iter_mut()
            .find(|(parent, child, _)| *parent == 0 && *child == synthesized)
        else {
            panic!("the expanded path has its first synthesized arc");
        };
        arc.0 = 2;
        let edge = complement.embedding.arcs[&synthesized];

        assert_eq!(
            tree_check_put_domain(&lens, &view, &complement.embedding, &complement.complement,),
            Err(LensError::PutDomainReport(vec![
                PutDomainError::SynthesizedEdited {
                    elem: ViewRef::Edge(edge),
                },
            ]))
        );
        let native = vec![
            PutDomainError::SynthesizedEdited {
                elem: ViewRef::Arc {
                    parent: 0,
                    child: synthesized,
                },
            },
            PutDomainError::SynthesizedEdited {
                elem: ViewRef::Arc {
                    parent: 2,
                    child: synthesized,
                },
            },
        ];
        assert_eq!(
            tree_check_put_domain_law(&lens, &view, &complement.embedding, &complement.complement,),
            Err(LensError::PutDomainReport(native.clone()))
        );
        assert_eq!(
            super::tree_put_law(&lens, &view, &complement.embedding, &complement.complement,),
            Err(LensError::PutDomain(native[0].clone()))
        );
    }

    #[test]
    fn tree_law_sibling_order_uses_the_minimum_synth_key_and_arc() {
        let path = SEdge::new("a", "b", "rel", Some("path"));
        let source_schema = Schema::default()
            .vertex("a", "record")
            .vertex("b", "record")
            .edge("a", "b", "rel", Some("path"));
        let target_schema = Schema::default()
            .vertex("a", "record")
            .vertex("m", "record")
            .vertex("b", "record")
            .edge("a", "m", "rel", Some("path-in"))
            .edge("m", "b", "rel", Some("path-out"));
        let migration = Migration {
            surviving_verts: ["a", "m", "b"].into_iter().map(str::to_owned).collect(),
            expansion_path: [(("a".to_owned(), "b".to_owned()), vec!["m".to_owned()])].into(),
            ..Migration::default()
        };
        let plan = compile(&source_schema, &target_schema, &migration)
            .unwrap_or_else(|error| panic!("the expansion fixture compiles: {error:?}"));
        let lens = GraphLens::new(
            &source_schema,
            &target_schema,
            &migration,
            &plan,
            &BTreeMap::new(),
            &NoValueLayer,
        )
        .unwrap_or_else(|error| panic!("the expansion lens constructs: {error:?}"));
        let source = Tree {
            nodes: [
                (0, Node::new(0, "a")),
                (1, Node::new(1, "b")),
                (2, Node::new(2, "b")),
            ]
            .into(),
            arcs: vec![(0, 1, path.clone()), (0, 2, path)],
            fans: Vec::new(),
            root: 0,
        };
        let Some(Ok((mut view, graph, embedding))) = super::tree_get(&lens, &source) else {
            panic!("the fixture has an expanded tree view");
        };
        let complement = TreeLawComplement {
            complement: graph,
            embedding,
            source_arcs: tree_source_arcs(&source),
        };
        let mut siblings: Vec<_> = view
            .arcs
            .iter()
            .enumerate()
            .filter(|(_, (parent, child, _))| {
                *parent == 0 && complement.complement.synthesized_nodes.contains_key(child)
            })
            .map(|(index, (_, child, _))| (index, *child))
            .collect();
        siblings.sort_by_key(|(_, child)| {
            let edge = complement.embedding.arcs[child];
            complement.complement.synthesized_edges[&edge].clone()
        });
        let [(left_index, minimum_child), (right_index, _)] = siblings.as_slice() else {
            panic!("the two expansions have two synthesized siblings");
        };
        view.arcs.swap(*left_index, *right_index);
        let edge = complement.embedding.arcs[minimum_child];

        assert_eq!(
            tree_check_put_domain(&lens, &view, &complement.embedding, &complement.complement,),
            Err(LensError::PutDomainReport(vec![
                PutDomainError::SynthesizedEdited {
                    elem: ViewRef::Edge(edge),
                },
            ]))
        );
        let native = PutDomainError::SynthesizedEdited {
            elem: ViewRef::Arc {
                parent: 0,
                child: *minimum_child,
            },
        };
        assert_eq!(
            tree_check_put_domain_law(&lens, &view, &complement.embedding, &complement.complement,),
            Err(LensError::PutDomainReport(vec![native.clone()]))
        );
        assert_eq!(
            super::tree_put_law(&lens, &view, &complement.embedding, &complement.complement,),
            Err(LensError::PutDomain(native))
        );
    }

    fn ordered(children: &[u32]) -> Tree {
        Tree {
            nodes: std::iter::once((0, Node::new(0, "r")))
                .chain(
                    children
                        .iter()
                        .copied()
                        .map(|child| (child, Node::new(child, "leaf"))),
                )
                .collect(),
            arcs: children
                .iter()
                .copied()
                .map(|child| (0, child, label()))
                .collect(),
            fans: Vec::new(),
            root: 0,
        }
    }

    #[test]
    fn tree_equivalence_ignores_only_cross_parent_arc_interleaving() {
        let branch = SEdge::new("r", "r", "child", Some("branch"));
        let child = label();
        let nodes: BTreeMap<u32, Node> = [
            (0, Node::new(0, "r")),
            (1, Node::new(1, "r")),
            (2, Node::new(2, "r")),
            (3, Node::new(3, "leaf")),
            (4, Node::new(4, "leaf")),
        ]
        .into();
        let left = Tree {
            nodes: nodes.clone(),
            arcs: vec![
                (0, 1, branch.clone()),
                (1, 3, child.clone()),
                (0, 2, branch.clone()),
                (2, 4, child.clone()),
            ],
            fans: Vec::new(),
            root: 0,
        };
        let interleaved = Tree {
            nodes: nodes.clone(),
            arcs: vec![
                (0, 1, branch.clone()),
                (0, 2, branch.clone()),
                (1, 3, child.clone()),
                (2, 4, child.clone()),
            ],
            fans: Vec::new(),
            root: 0,
        };
        let reordered_siblings = Tree {
            nodes,
            arcs: vec![
                (0, 2, branch.clone()),
                (1, 3, child.clone()),
                (0, 1, branch),
                (2, 4, child),
            ],
            fans: Vec::new(),
            root: 0,
        };
        let index = super::SynthIndex::default();

        assert!(<super::TreeShape as super::LensShape>::equivalent(
            &left,
            &index,
            &interleaved,
            &index,
        ));
        assert!(<super::TreeShape as super::LensShape>::equivalent(
            &interleaved,
            &index,
            &left,
            &index,
        ));
        assert!(!<super::TreeShape as super::LensShape>::equivalent(
            &left,
            &index,
            &reordered_siblings,
            &index,
        ));
    }

    #[test]
    fn exact_three_sibling_permutation_is_part_of_tree_locality() {
        let source = ordered(&[1, 2, 3]);
        let edited = ordered(&[2, 3, 1]);
        let wrong = ordered(&[3, 1, 2]);
        let complement = TreeLawComplement {
            complement: Complement::default(),
            embedding: TreeEmbedding {
                arcs: [(1, 0), (2, 1), (3, 2)].into(),
                ..TreeEmbedding::default()
            },
            source_arcs: [(0, (0, 1)), (1, (0, 2)), (2, (0, 3))].into(),
        };
        assert!(
            tree_locality_divergence(&source, &source, &edited, &edited, &complement).is_none()
        );
        assert!(tree_locality_divergence(&source, &source, &edited, &wrong, &complement).is_some());
    }

    #[test]
    fn tree_reorder_basis_is_exactly_adjacent_same_parent_swaps() {
        let edge = label();
        let schema = Schema::default()
            .vertex("r", "obj")
            .vertex("leaf", "obj")
            .edge("r", "leaf", "child", Some("child"));
        let migration = Migration {
            surviving_verts: schema.vertices.keys().cloned().collect(),
            surviving_edges: schema.edges.clone(),
            ..Migration::default()
        };
        let Ok(plan) = compile(&schema, &schema, &migration) else {
            panic!("identity migration did not compile");
        };
        let semantics = BTreeMap::new();
        let Ok(lens) = GraphLens::new(
            &schema,
            &schema,
            &migration,
            &plan,
            &semantics,
            &NoValueLayer,
        ) else {
            panic!("identity lens did not construct");
        };
        let source = ordered(&[1, 2, 3]);
        let Some(Ok((view, complement, embedding))) = super::tree_get(&lens, &source) else {
            panic!("identity tree get did not succeed");
        };
        let complement = TreeLawComplement {
            complement,
            embedding,
            source_arcs: tree_source_arcs(&source),
        };
        let reorder: Vec<_> = tree_basis(&lens, &view, &complement)
            .into_iter()
            .filter(|candidate| candidate.mutation == Mutation::Reorder)
            .collect();
        assert_eq!(reorder.len(), 2);
        assert!(
            reorder
                .iter()
                .all(|candidate| matches!(&candidate.domain, DomainExpectation::Admitted))
        );
        assert_eq!(
            reorder
                .iter()
                .map(|candidate| {
                    candidate
                        .view
                        .arcs
                        .iter()
                        .filter_map(|(parent, child, _)| (*parent == 0).then_some(*child))
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>(),
            vec![vec![2, 1, 3], vec![1, 3, 2]]
        );
        assert!(reorder.iter().all(|candidate| {
            candidate
                .view
                .arcs
                .iter()
                .all(|(parent, _, label)| *parent == 0 && label == &edge)
        }));
    }

    #[test]
    fn tree_reorder_basis_uses_per_parent_adjacency_when_arcs_interleave() {
        let branch = SEdge::new("r", "r", "child", Some("branch"));
        let child = label();
        let schema = Schema::default()
            .vertex("r", "obj")
            .vertex("leaf", "obj")
            .edge("r", "r", "child", Some("branch"))
            .edge("r", "leaf", "child", Some("child"));
        let migration = Migration {
            surviving_verts: schema.vertices.keys().cloned().collect(),
            surviving_edges: schema.edges.clone(),
            ..Migration::default()
        };
        let Ok(plan) = compile(&schema, &schema, &migration) else {
            panic!("identity migration did not compile");
        };
        let Ok(lens) = GraphLens::new(
            &schema,
            &schema,
            &migration,
            &plan,
            &BTreeMap::new(),
            &NoValueLayer,
        ) else {
            panic!("identity lens did not construct");
        };
        let source = Tree {
            nodes: [
                (0, Node::new(0, "r")),
                (1, Node::new(1, "r")),
                (2, Node::new(2, "leaf")),
                (3, Node::new(3, "leaf")),
                (4, Node::new(4, "leaf")),
            ]
            .into(),
            arcs: vec![
                (0, 1, branch.clone()),
                (1, 3, child.clone()),
                (0, 2, child.clone()),
                (1, 4, child.clone()),
            ],
            fans: Vec::new(),
            root: 0,
        };
        let Some(Ok((view, complement, embedding))) = super::tree_get(&lens, &source) else {
            panic!("identity tree get did not succeed");
        };
        let complement = TreeLawComplement {
            complement,
            embedding,
            source_arcs: tree_source_arcs(&source),
        };
        let reorder: Vec<_> = tree_basis(&lens, &view, &complement)
            .into_iter()
            .filter(|candidate| candidate.mutation == Mutation::Reorder)
            .map(|candidate| candidate.view.arcs)
            .collect();
        assert_eq!(reorder.len(), 2);
        assert_eq!(
            reorder,
            vec![
                vec![
                    (0, 2, child.clone()),
                    (1, 3, child.clone()),
                    (0, 1, branch),
                    (1, 4, child.clone()),
                ],
                vec![
                    (0, 1, SEdge::new("r", "r", "child", Some("branch"))),
                    (1, 4, child.clone()),
                    (0, 2, child.clone()),
                    (1, 3, child),
                ],
            ]
        );
    }

    #[test]
    fn fresh_tree_nodes_honor_every_reserved_carrier_and_exhaustion_floor() {
        let view = Tree {
            nodes: [(0, Node::new(0, "r"))].into(),
            arcs: Vec::new(),
            fans: Vec::new(),
            root: 0,
        };
        let mut complement = TreeLawComplement {
            complement: Complement {
                dropped_nodes: [(2, Node::new(2, "leaf"))].into(),
                floor: (4, 0),
                ..Complement::default()
            },
            embedding: TreeEmbedding {
                next_node: 1,
                ..TreeEmbedding::default()
            },
            source_arcs: BTreeMap::new(),
        };
        assert_eq!(fresh_tree_node(&view, &complement), Some(4));

        complement.embedding.next_node = 6;
        assert_eq!(fresh_tree_node(&view, &complement), Some(6));

        complement.embedding.fans.push((8, 0, BTreeMap::new()));
        assert_eq!(fresh_tree_node(&view, &complement), Some(9));

        complement.complement.synthesized_nodes.insert(
            10,
            SynthKey::Contract {
                src: 0,
                tgt: 1,
                edge: label(),
            },
        );
        assert_eq!(fresh_tree_node(&view, &complement), Some(11));

        complement
            .complement
            .original_anchors
            .insert(12, "leaf".into());
        assert_eq!(fresh_tree_node(&view, &complement), Some(13));

        complement.complement.floor.0 = u32::MAX;
        assert_eq!(fresh_tree_node(&view, &complement), None);
    }

    #[test]
    fn fresh_tree_edges_honor_every_reserved_carrier_and_exhaustion_floor() {
        let mut complement = TreeLawComplement {
            complement: Complement {
                synthesized_edges: [(
                    5,
                    SynthKey::Contract {
                        src: 0,
                        tgt: 1,
                        edge: label(),
                    },
                )]
                .into(),
                floor: (0, 4),
                ..Complement::default()
            },
            embedding: TreeEmbedding {
                arcs: [(1, 2)].into(),
                fans: vec![(8, 6, BTreeMap::new())],
                next_edge: 1,
                ..TreeEmbedding::default()
            },
            source_arcs: BTreeMap::new(),
        };
        assert_eq!(fresh_tree_edge(&complement), Some(7));

        complement.embedding.next_edge = 9;
        assert_eq!(fresh_tree_edge(&complement), Some(9));

        complement.complement.floor.1 = 10;
        assert_eq!(fresh_tree_edge(&complement), Some(10));

        complement.complement.floor.1 = u32::MAX;
        assert_eq!(fresh_tree_edge(&complement), None);
    }

    #[test]
    fn tree_default_nodes_match_the_operational_payload_convention() {
        let schema = Schema::default().vertex("flag", "boolean");
        assert_eq!(
            super::tree_node_for_anchor(3, &schema, "flag").value,
            Some(Presence::Present(Value::Bool(false)))
        );
    }

    #[test]
    fn contracted_tree_basis_skips_direct_ambiguous_and_required_witnesses() {
        let stages = c11_stages();
        let Some(base) = stages.iter().find(|stage| stage.name == "C") else {
            panic!("the contraction stage exists");
        };
        let source = contraction_source();
        assert!(
            stage_tree_basis(base, &source)
                .iter()
                .any(|candidate| candidate.mutation == Mutation::AddContractedEdge)
        );

        let mut direct = base.clone();
        let direct_edge = c11_edge("a", "a");
        direct.source.edges.insert(direct_edge.clone());
        direct.migration.surviving_edges.insert(direct_edge);
        assert_eq!(
            stage_tree_basis(&direct, &source)
                .iter()
                .filter(|candidate| candidate.mutation == Mutation::AddContractedEdge)
                .count(),
            0
        );

        let mut ambiguous = base.clone();
        let kind = ambiguous.source.vertices["a"].clone();
        ambiguous.source.vertices.insert("c".to_owned(), kind);
        ambiguous
            .migration
            .vertex_remap
            .insert("c".to_owned(), "a".to_owned());
        assert_eq!(
            stage_tree_basis(&ambiguous, &source)
                .iter()
                .filter(|candidate| candidate.mutation == Mutation::AddContractedEdge)
                .count(),
            0
        );

        let mut required = base.clone();
        required
            .source
            .required
            .insert("a".to_owned(), vec![c11_edge("a", "b")]);
        assert_eq!(
            stage_tree_basis(&required, &source)
                .iter()
                .filter(|candidate| candidate.mutation == Mutation::AddContractedEdge)
                .count(),
            0
        );

        let mut nonnullable = base.clone();
        insert_nonnullable_reference(&mut nonnullable.source);
        assert_eq!(
            stage_tree_basis(&nonnullable, &source)
                .iter()
                .filter(|candidate| candidate.mutation == Mutation::AddContractedEdge)
                .count(),
            0
        );
    }

    #[test]
    fn contracted_tree_a_to_b_requires_a_neutral_source_endpoint() {
        let am = SEdge::new("a", "m", "rel", Some("am"));
        let mb = SEdge::new("m", "b", "rel", Some("mb"));
        let ab = SEdge::new("a", "b", "rel", Some("ab"));
        let base = C11Stage {
            name: "a-to-b-contraction".to_owned(),
            source: Schema::default()
                .vertex("a", "record")
                .vertex("m", "record")
                .vertex("b", "record")
                .edge("a", "m", "rel", Some("am"))
                .edge("m", "b", "rel", Some("mb")),
            target: Schema::default()
                .vertex("a", "record")
                .vertex("b", "record")
                .edge("a", "b", "rel", Some("ab")),
            migration: Migration {
                surviving_verts: ["a".to_owned(), "b".to_owned()].into(),
                resolver: [(("a".to_owned(), "b".to_owned()), ab)].into(),
                ..Migration::default()
            },
            layer: C11Layer::None,
        };
        let source = Tree {
            nodes: [
                (0, Node::new(0, "a")),
                (1, Node::new(1, "m")),
                (2, Node::new(2, "b")),
            ]
            .into(),
            arcs: vec![(0, 1, am), (1, 2, mb)],
            fans: Vec::new(),
            root: 0,
        };
        assert_eq!(
            stage_tree_basis(&base, &source)
                .iter()
                .filter(|candidate| candidate.mutation == Mutation::AddContractedEdge)
                .count(),
            1
        );

        let mut mismatched = base;
        mismatched
            .source
            .vertices
            .insert("a".to_owned(), "other-record".to_owned());
        assert_eq!(
            stage_tree_basis(&mismatched, &source)
                .iter()
                .filter(|candidate| candidate.mutation == Mutation::AddContractedEdge)
                .count(),
            0
        );
    }

    fn assert_endpoint_mismatched_contraction_has_no_synth_edit(source: &Tree) {
        let (mismatched, replacement) = adversarial_contraction_edit_stage(false);
        let Ok(plan) = compile(
            &mismatched.source,
            &mismatched.target,
            &mismatched.migration,
        ) else {
            panic!("the endpoint-mismatched contraction compiles");
        };
        assert_eq!(
            plan.fates()
                .values()
                .filter(|fate| matches!(fate, EdgeFate::Keep(image) if image == &replacement))
                .count(),
            1
        );
        let semantics = BTreeMap::new();
        let Ok(lens) = GraphLens::new(
            &mismatched.source,
            &mismatched.target,
            &mismatched.migration,
            &plan,
            &semantics,
            &NoValueLayer,
        ) else {
            panic!("the endpoint-mismatched lens constructs");
        };
        let Some(Ok((view, graph, embedding))) = super::tree_get(&lens, source) else {
            panic!("the endpoint-mismatched contraction has a tree view");
        };
        let complement = TreeLawComplement {
            complement: graph,
            embedding,
            source_arcs: tree_source_arcs(source),
        };
        assert_eq!(
            tree_basis(&lens, &view, &complement)
                .iter()
                .filter(|candidate| candidate.mutation == Mutation::EditSynthesized)
                .count(),
            0
        );
    }

    #[test]
    fn synthesized_tree_edge_edits_use_endpoint_specific_preimages() {
        let (stage, replacement) = adversarial_contraction_edit_stage(true);

        let Ok(plan) = compile(&stage.source, &stage.target, &stage.migration) else {
            panic!("the adversarial contraction compiles");
        };
        assert_eq!(
            plan.fates()
                .values()
                .filter(|fate| matches!(fate, EdgeFate::Keep(image) if image == &replacement))
                .count(),
            2
        );
        let semantics = BTreeMap::new();
        let Ok(lens) = GraphLens::new(
            &stage.source,
            &stage.target,
            &stage.migration,
            &plan,
            &semantics,
            &NoValueLayer,
        ) else {
            panic!("the adversarial lens constructs");
        };
        let source = contraction_source();
        let Some(Ok((view, graph, embedding))) = super::tree_get(&lens, &source) else {
            panic!("the adversarial contraction has a tree view");
        };
        let complement = TreeLawComplement {
            complement: graph,
            embedding,
            source_arcs: tree_source_arcs(&source),
        };
        let edited: Vec<_> = tree_basis(&lens, &view, &complement)
            .into_iter()
            .filter(|candidate| candidate.mutation == Mutation::EditSynthesized)
            .collect();
        assert_eq!(edited.len(), 1);
        let DomainExpectation::Refused(ReferenceLawError::PutDomain(expected)) = &edited[0].domain
        else {
            panic!("the synthesized edit must retain its designated refusal");
        };
        assert!(matches!(
            expected.as_slice(),
            [PutDomainError::SynthesizedEdited { .. }]
        ));
        let Err(LensError::PutDomainReport(graph_observed)) = tree_check_put_domain(
            &lens,
            &edited[0].view,
            &complement.embedding,
            &complement.complement,
        ) else {
            panic!("the synthesized edit must be refused");
        };
        assert!(matches!(
            graph_observed.as_slice(),
            [PutDomainError::SynthesizedEdited {
                elem: ViewRef::Edge(_)
            }]
        ));
        let Err(LensError::PutDomainReport(observed)) = tree_check_put_domain_law(
            &lens,
            &edited[0].view,
            &complement.embedding,
            &complement.complement,
        ) else {
            panic!("the law route must report the synthesized edit");
        };
        assert_eq!(&observed, expected);
        assert_endpoint_mismatched_contraction_has_no_synth_edit(&source);
    }

    #[test]
    fn reserved_tree_basis_skips_validation_observable_anchors() {
        let stages = c11_stages();
        let Some(project) = stages.iter().find(|stage| stage.name == "P") else {
            panic!("the projection stage exists");
        };
        let mut base = project.clone();
        let attachable = c11_edge("a", "a");
        base.source.edges.insert(attachable.clone());
        base.target.edges.insert(attachable.clone());
        base.migration.surviving_edges.insert(attachable);
        let source = reserved_source();
        assert!(
            stage_tree_basis(&base, &source)
                .iter()
                .any(|candidate| candidate.mutation == Mutation::ReservedIdReuse)
        );
        let mut variants = Vec::new();

        let mut required = base.clone();
        required
            .source
            .required
            .insert("a".to_owned(), vec![c11_edge("a", "b")]);
        variants.push(required);

        let mut keyed = base.clone();
        keyed.source.keys.insert(
            "a".to_owned(),
            vec![Key {
                id: "a-key".to_owned(),
                parts: vec![KeyPart::Path(vec![c11_edge("a", "b")])],
                role: KeyRole::Unique,
                scope: KeyScope::Global,
                equality: Vec::new(),
            }],
        );
        variants.push(keyed);

        let mut predicate = base.clone();
        predicate.migration.conditional.insert("a".to_owned());
        variants.push(predicate);

        let mut indirect_constraint = base.clone();
        indirect_constraint.source.constraints.insert(
            "b".to_owned(),
            vec![("required".to_owned(), "true".to_owned())],
        );
        variants.push(indirect_constraint);

        let mut nonnullable = base.clone();
        insert_nonnullable_reference(&mut nonnullable.source);
        variants.push(nonnullable);

        let mut nullable = base.clone();
        insert_reference(&mut nullable.source, true);
        variants.push(nullable);

        for variant in variants {
            assert_eq!(
                stage_tree_basis(&variant, &source)
                    .iter()
                    .filter(|candidate| candidate.mutation == Mutation::ReservedIdReuse)
                    .count(),
                0
            );
        }
    }

    #[test]
    fn reserved_tree_a_to_b_attachment_skips_parent_side_state() {
        let attachment = SEdge::new("a", "b", "rel", Some("attach"));
        let dropped = SEdge::new("a", "d", "rel", Some("drop"));
        let base = C11Stage {
            name: "parent-state".to_owned(),
            source: Schema::default()
                .vertex("a", "record")
                .vertex("b", "record")
                .vertex("d", "record")
                .edge("a", "b", "rel", Some("attach"))
                .edge("a", "d", "rel", Some("drop")),
            target: Schema::default()
                .vertex("a", "record")
                .vertex("b", "record")
                .edge("a", "b", "rel", Some("attach")),
            migration: Migration {
                surviving_verts: ["a".to_owned(), "b".to_owned()].into(),
                surviving_edges: [attachment].into(),
                ..Migration::default()
            },
            layer: C11Layer::None,
        };
        let source = Tree {
            nodes: [(0, Node::new(0, "a")), (2, Node::new(2, "d"))].into(),
            arcs: vec![(0, 2, dropped)],
            fans: Vec::new(),
            root: 0,
        };
        assert_eq!(
            stage_tree_basis(&base, &source)
                .iter()
                .filter(|candidate| candidate.mutation == Mutation::ReservedIdReuse)
                .count(),
            1
        );

        let mut keyed = base.clone();
        keyed.source.keys.insert(
            "a".to_owned(),
            vec![Key {
                id: "parent-key".to_owned(),
                parts: vec![KeyPart::Path(Vec::new())],
                role: KeyRole::Unique,
                scope: KeyScope::Global,
                equality: Vec::new(),
            }],
        );
        let mut conditional = base;
        conditional.migration.conditional.insert("a".to_owned());
        for stage in [keyed, conditional] {
            assert_eq!(
                stage_tree_basis(&stage, &source)
                    .iter()
                    .filter(|candidate| candidate.mutation == Mutation::ReservedIdReuse)
                    .count(),
                0
            );
        }
    }

    #[test]
    fn reserved_tree_attachment_omits_a_fan_reserved_label() {
        let source_kept = SEdge::new("a", "b", crate::restrict::HYPER_LABEL, Some("source-kept"));
        let target_kept = SEdge::new("a", "b", "ordinary", Some("target-kept"));
        let hidden = SEdge::new("a", "d", "ordinary", Some("hidden"));
        let source_schema = Schema::default()
            .vertex("a", "record")
            .vertex("b", "record")
            .vertex("d", "record")
            .edge("a", "b", crate::restrict::HYPER_LABEL, Some("source-kept"))
            .edge("a", "d", "ordinary", Some("hidden"));
        let target_schema = Schema::default()
            .vertex("a", "record")
            .vertex("b", "record")
            .edge("a", "b", "ordinary", Some("target-kept"));
        let migration = Migration {
            surviving_verts: ["a".to_owned(), "b".to_owned()].into(),
            edge_remap: [(source_kept, target_kept.clone())].into(),
            ..Migration::default()
        };
        let plan = compile(&source_schema, &target_schema, &migration)
            .unwrap_or_else(|error| panic!("the fan-kind fixture compiles: {error:?}"));
        let lens = GraphLens::new(
            &source_schema,
            &target_schema,
            &migration,
            &plan,
            &BTreeMap::new(),
            &NoValueLayer,
        )
        .unwrap_or_else(|error| panic!("the fan-kind lens constructs: {error:?}"));
        let source = Tree {
            nodes: [(0, Node::new(0, "a")), (2, Node::new(2, "d"))].into(),
            arcs: vec![(0, 2, hidden)],
            fans: Vec::new(),
            root: 0,
        };
        let Some(Ok((view, graph, embedding))) = super::tree_get(&lens, &source) else {
            panic!("the fan-kind source has a tree view");
        };
        let complement = TreeLawComplement {
            complement: graph,
            embedding,
            source_arcs: tree_source_arcs(&source),
        };
        assert_eq!(
            tree_basis(&lens, &view, &complement)
                .iter()
                .filter(|candidate| candidate.mutation == Mutation::ReservedIdReuse)
                .count(),
            0
        );

        let mut counterfactual = view;
        counterfactual.nodes.insert(2, Node::new(2, "b"));
        counterfactual.arcs.push((0, 2, target_kept));
        let Err(LensError::PutDomainReport(observed)) = tree_check_put_domain(
            &lens,
            &counterfactual,
            &complement.embedding,
            &complement.complement,
        ) else {
            panic!("the bypassed fan-kind attachment must be refused");
        };
        assert!(matches!(
            observed.as_slice(),
            [
                PutDomainError::IdCollision {
                    elem: ViewRef::Node(2)
                },
                PutDomainError::MalformedFan {
                    node: 0,
                    reason: crate::fan::FanViolation::ReservedKindOutsideFan { .. }
                }
            ]
        ));

        let stripped = without_reserved_tree_residue(complement, 2);
        let Err(LensError::PutDomainReport(observed)) = tree_check_put_domain(
            &lens,
            &counterfactual,
            &stripped.embedding,
            &stripped.complement,
        ) else {
            panic!("removing the collision residue leaves the fan refusal");
        };
        assert!(matches!(
            observed.as_slice(),
            [PutDomainError::MalformedFan {
                node: 0,
                reason: crate::fan::FanViolation::ReservedKindOutsideFan { .. }
            }]
        ));
    }

    #[test]
    fn reserved_tree_basis_skips_a_synthesized_first_parent() {
        let (source_schema, target_schema, migration, source) =
            synthesized_parent_reserved_fixture(ReservedAttachment::FirstEligible);
        let Ok(plan) = compile(&source_schema, &target_schema, &migration) else {
            panic!("the synthesized-parent fixture compiles");
        };
        let semantics = BTreeMap::new();
        let Ok(lens) = GraphLens::new(
            &source_schema,
            &target_schema,
            &migration,
            &plan,
            &semantics,
            &NoValueLayer,
        ) else {
            panic!("the synthesized-parent lens constructs");
        };
        let Some(Ok((view, graph, embedding))) = super::tree_get(&lens, &source) else {
            panic!("the synthesized-parent source has a tree view");
        };
        assert_eq!(
            graph.synthesized_nodes.len(),
            1,
            "the fixture must have exactly one synthesized node"
        );
        let Some(old_id) = graph.synthesized_nodes.keys().copied().next() else {
            panic!("the synthesized node must have an id");
        };
        assert!(old_id > 10);
        let moves = BTreeMap::from([(old_id, 3)]);
        let mut nodes = view.nodes.clone();
        let Some(mut synthesized) = nodes.remove(&old_id) else {
            panic!("the synthesized node is present");
        };
        synthesized.id = 3;
        nodes.insert(3, synthesized);
        let view = Tree {
            nodes,
            arcs: view
                .arcs
                .iter()
                .map(|(parent, child, label)| {
                    (
                        if *parent == old_id { 3 } else { *parent },
                        if *child == old_id { 3 } else { *child },
                        label.clone(),
                    )
                })
                .collect(),
            fans: view.fans,
            root: view.root,
        };
        let complement = TreeLawComplement {
            complement: super::reindexed_complement(&graph, &moves, &BTreeMap::new()),
            embedding: super::reindexed_embedding(&embedding, &moves, &BTreeMap::new()),
            source_arcs: tree_source_arcs(&source),
        };
        assert_eq!(view.nodes[&3].anchor, "m");
        assert_eq!(view.nodes[&10].anchor, "m");
        assert_eq!(
            tree_check_put_domain(&lens, &view, &complement.embedding, &complement.complement,),
            Ok(())
        );
        assert_eq!(
            tree_put(&lens, &view, &complement.embedding, &complement.complement,),
            Ok(source)
        );

        let reserved: Vec<_> = tree_basis(&lens, &view, &complement)
            .into_iter()
            .filter(|candidate| candidate.mutation == Mutation::ReservedIdReuse)
            .collect();
        assert_eq!(reserved.len(), 1);
        let candidate = &reserved[0];
        assert!(
            candidate
                .view
                .arcs
                .iter()
                .any(|(parent, child, _)| (*parent, *child) == (10, 2))
        );
        assert!(
            !candidate
                .view
                .arcs
                .iter()
                .any(|(parent, child, _)| (*parent, *child) == (3, 2))
        );
        assert_reserved_tree_counterfactual(&lens, candidate, &complement, 2);
    }

    #[test]
    fn reserved_tree_basis_uses_a_later_attachment_at_the_original_parent() {
        let (source_schema, target_schema, migration, source) =
            synthesized_parent_reserved_fixture(ReservedAttachment::LaterEligible);
        let Ok(plan) = compile(&source_schema, &target_schema, &migration) else {
            panic!("the mismatched-parent fixture compiles");
        };
        let semantics = BTreeMap::new();
        let Ok(lens) = GraphLens::new(
            &source_schema,
            &target_schema,
            &migration,
            &plan,
            &semantics,
            &NoValueLayer,
        ) else {
            panic!("the mismatched-parent lens constructs");
        };
        let Some(Ok((view, graph, embedding))) = super::tree_get(&lens, &source) else {
            panic!("the mismatched-parent source has a tree view");
        };
        let Some(old_id) = graph.synthesized_nodes.keys().copied().next() else {
            panic!("the fixture has a synthesized node");
        };
        assert_eq!(graph.synthesized_nodes.len(), 1);
        let moves = BTreeMap::from([(old_id, 3)]);
        let mut nodes = view.nodes.clone();
        let Some(mut synthesized) = nodes.remove(&old_id) else {
            panic!("the synthesized node is present");
        };
        synthesized.id = 3;
        nodes.insert(3, synthesized);
        let view = Tree {
            nodes,
            arcs: view
                .arcs
                .iter()
                .map(|(parent, child, label)| {
                    (
                        if *parent == old_id { 3 } else { *parent },
                        if *child == old_id { 3 } else { *child },
                        label.clone(),
                    )
                })
                .collect(),
            fans: view.fans,
            root: view.root,
        };
        let complement = TreeLawComplement {
            complement: super::reindexed_complement(&graph, &moves, &BTreeMap::new()),
            embedding: super::reindexed_embedding(&embedding, &moves, &BTreeMap::new()),
            source_arcs: tree_source_arcs(&source),
        };
        assert_eq!(
            tree_check_put_domain(&lens, &view, &complement.embedding, &complement.complement,),
            Ok(())
        );
        assert_eq!(
            tree_put(&lens, &view, &complement.embedding, &complement.complement,),
            Ok(source)
        );
        let reserved: Vec<_> = tree_basis(&lens, &view, &complement)
            .into_iter()
            .filter(|candidate| candidate.mutation == Mutation::ReservedIdReuse)
            .collect();
        assert_eq!(reserved.len(), 1);
        let candidate = &reserved[0];
        assert!(candidate.view.arcs.iter().any(|(parent, child, label)| {
            (*parent, *child, label.src.as_str()) == (0, 2, "z")
        }));
        assert!(
            !candidate
                .view
                .arcs
                .iter()
                .any(|(parent, child, _)| (*parent, *child) == (10, 2))
        );
        assert_reserved_tree_counterfactual(&lens, candidate, &complement, 2);
    }

    #[test]
    fn reserved_tree_basis_skips_an_original_parent_without_an_attachment() {
        let (source_schema, target_schema, migration, source) =
            synthesized_parent_reserved_fixture(ReservedAttachment::None);
        let Ok(plan) = compile(&source_schema, &target_schema, &migration) else {
            panic!("the no-attachment fixture compiles");
        };
        let semantics = BTreeMap::new();
        let Ok(lens) = GraphLens::new(
            &source_schema,
            &target_schema,
            &migration,
            &plan,
            &semantics,
            &NoValueLayer,
        ) else {
            panic!("the no-attachment lens constructs");
        };
        let Some(Ok((view, graph, embedding))) = super::tree_get(&lens, &source) else {
            panic!("the no-attachment source has a tree view");
        };
        let complement = TreeLawComplement {
            complement: graph,
            embedding,
            source_arcs: tree_source_arcs(&source),
        };
        assert_eq!(
            tree_basis(&lens, &view, &complement)
                .into_iter()
                .filter(|candidate| candidate.mutation == Mutation::ReservedIdReuse)
                .count(),
            0
        );
    }

    #[test]
    fn reserved_tree_isolation_rejects_outgoing_and_fan_residue() {
        let (source_schema, target_schema, migration, source) =
            synthesized_parent_reserved_fixture(ReservedAttachment::FirstEligible);
        let Ok(plan) = compile(&source_schema, &target_schema, &migration) else {
            panic!("the residue fixture compiles");
        };
        let semantics = BTreeMap::new();
        let Ok(lens) = GraphLens::new(
            &source_schema,
            &target_schema,
            &migration,
            &plan,
            &semantics,
            &NoValueLayer,
        ) else {
            panic!("the residue lens constructs");
        };
        let Some(Ok((_, graph, embedding))) = super::tree_get(&lens, &source) else {
            panic!("the residue source has a tree view");
        };
        let complement = TreeLawComplement {
            complement: graph,
            embedding,
            source_arcs: tree_source_arcs(&source),
        };
        assert_eq!(
            super::isolated_reserved_tree_parent(&complement, 2),
            Some(10)
        );

        let mut outgoing = complement.clone();
        outgoing.complement.dropped_edges.insert(
            u32::MAX,
            GEdge {
                src: 2,
                tgt: 0,
                label: SEdge::new("d", "z", "rel", Some("hidden-child")),
                position: None,
            },
        );
        assert_eq!(super::isolated_reserved_tree_parent(&outgoing, 2), None);

        let mut fan = complement;
        let mut fan_node = Node::new(u32::MAX - 1, "fan");
        fan_node.shape = crate::model::NodeShape::HyperEdge;
        fan.complement.dropped_nodes.insert(fan_node.id, fan_node);
        fan.complement.dropped_edges.insert(
            u32::MAX,
            GEdge {
                src: u32::MAX - 1,
                tgt: 2,
                label: SEdge::new("fan", "d", "__label__", Some("member")),
                position: None,
            },
        );
        assert_eq!(super::isolated_reserved_tree_parent(&fan, 2), None);
    }

    #[test]
    fn contracted_tree_basis_retains_its_designated_refusal() {
        let stages = c11_stages();
        let Some(stage) = stages.iter().find(|stage| stage.name == "C") else {
            panic!("the contraction stage exists");
        };
        let Ok(plan) = compile(&stage.source, &stage.target, &stage.migration) else {
            panic!("the contraction stage compiles");
        };
        let semantics = BTreeMap::new();
        let Ok(lens) = GraphLens::new(
            &stage.source,
            &stage.target,
            &stage.migration,
            &plan,
            &semantics,
            &NoValueLayer,
        ) else {
            panic!("the contraction lens constructs");
        };
        let source = Tree {
            nodes: [
                (0, Node::new(0, "a")),
                (1, Node::new(1, "a")),
                (2, Node::new(2, "b")),
            ]
            .into(),
            arcs: vec![(0, 2, c11_edge("a", "b")), (2, 1, c11_edge("b", "a"))],
            fans: Vec::new(),
            root: 0,
        };
        let Some(Ok((view, graph, embedding))) = super::tree_get(&lens, &source) else {
            panic!("the contraction has a tree view");
        };
        let complement = TreeLawComplement {
            complement: graph,
            embedding,
            source_arcs: tree_source_arcs(&source),
        };
        let contracted: Vec<_> = tree_basis(&lens, &view, &complement)
            .into_iter()
            .filter(|candidate| candidate.mutation == Mutation::AddContractedEdge)
            .collect();
        assert_eq!(contracted.len(), 1);
        let candidate = &contracted[0];
        assert!(candidate.view.nodes.contains_key(&3));
        assert_eq!(
            candidate
                .view
                .arcs
                .iter()
                .map(|(parent, child, _)| (*parent, *child))
                .collect::<Vec<_>>(),
            vec![(0, 3), (0, 1)]
        );
        let expected = PutDomainError::ContractedEdgeInserted {
            elem: ViewRef::Arc {
                parent: 0,
                child: 3,
            },
        };
        let graph_expected = PutDomainError::ContractedEdgeInserted {
            elem: ViewRef::Edge(3),
        };
        assert_eq!(
            candidate.domain,
            DomainExpectation::Refused(ReferenceLawError::PutDomain(vec![expected]))
        );
        let Err(LensError::PutDomainReport(errors)) = tree_check_put_domain(
            &lens,
            &candidate.view,
            &complement.embedding,
            &complement.complement,
        ) else {
            panic!("the inserted contracted edge must be the complete domain report");
        };
        assert_eq!(errors, vec![graph_expected.clone()]);
        assert_eq!(
            tree_put(
                &lens,
                &candidate.view,
                &complement.embedding,
                &complement.complement,
            ),
            Err(LensError::PutDomain(graph_expected))
        );

        let report = check_tree_laws(&lens, &source);
        assert!(
            report
                .gates
                .iter()
                .all(|gate| { matches!(&gate.status, LawGateStatus::Passed { .. }) })
        );
    }

    #[test]
    fn tree_rekey_relocates_an_ordinary_edge_before_restoring_synth_ids() {
        let edge = label();
        let first = SynthKey::Contract {
            src: 0,
            tgt: 2,
            edge: edge.clone(),
        };
        let second = SynthKey::Contract {
            src: 0,
            tgt: 3,
            edge: edge.clone(),
        };
        let complement = TreeLawComplement {
            complement: Complement {
                original_edges: [(
                    5,
                    GEdge {
                        src: 0,
                        tgt: 1,
                        label: edge,
                        position: None,
                    },
                )]
                .into(),
                synthesized_edges: [(6, first.clone()), (7, second.clone())].into(),
                floor: (4, 8),
                ..Complement::default()
            },
            embedding: TreeEmbedding {
                arcs: [(1, 5), (2, 6), (3, 7)].into(),
                next_node: 4,
                next_edge: 8,
                ..TreeEmbedding::default()
            },
            source_arcs: BTreeMap::new(),
        };
        let Some(rekeyed) =
            collision_free_tree_rekey(&complement, &BTreeMap::new(), &[(6, 5), (7, 6)].into())
        else {
            panic!("finite coherent tree rekey unexpectedly exhausted ids");
        };
        assert_eq!(rekeyed.embedding.arcs, [(1, 8), (2, 5), (3, 6)].into());
        assert_eq!(rekeyed.embedding.next_edge, 9);
        assert_eq!(
            rekeyed.complement.synthesized_edges,
            [(5, first), (6, second)].into()
        );
        assert_eq!(
            rekeyed
                .complement
                .original_edges
                .keys()
                .copied()
                .collect::<Vec<_>>(),
            vec![8]
        );
    }

    #[test]
    fn exact_dropped_arc_cascade_is_part_of_tree_locality() {
        let edge = label();
        let source = Tree {
            nodes: [
                (0, Node::new(0, "r")),
                (1, Node::new(1, "leaf")),
                (2, Node::new(2, "leaf")),
                (9, Node::new(9, "leaf")),
            ]
            .into(),
            arcs: vec![
                (0, 1, edge.clone()),
                (1, 9, edge.clone()),
                (0, 2, edge.clone()),
            ],
            fans: Vec::new(),
            root: 0,
        };
        let original_view = Tree {
            nodes: [(0, Node::new(0, "r")), (1, Node::new(1, "leaf"))].into(),
            arcs: vec![(0, 1, edge.clone())],
            fans: Vec::new(),
            root: 0,
        };
        let edited = Tree {
            nodes: [(0, Node::new(0, "r"))].into(),
            arcs: Vec::new(),
            fans: Vec::new(),
            root: 0,
        };
        let exact = Tree {
            nodes: [
                (0, Node::new(0, "r")),
                (2, Node::new(2, "leaf")),
                (9, Node::new(9, "leaf")),
            ]
            .into(),
            arcs: vec![(0, 2, edge.clone())],
            fans: Vec::new(),
            root: 0,
        };
        let omitted_cascade = Tree {
            arcs: vec![(1, 9, edge.clone()), (0, 2, edge.clone())],
            ..exact.clone()
        };
        let unrelated_extra = Tree {
            arcs: Vec::new(),
            ..exact.clone()
        };
        let complement = TreeLawComplement {
            complement: Complement {
                dropped_edges: [(
                    7,
                    GEdge {
                        src: 1,
                        tgt: 9,
                        label: edge,
                        position: None,
                    },
                )]
                .into(),
                ..Complement::default()
            },
            embedding: TreeEmbedding {
                arcs: [(1, 0)].into(),
                ..TreeEmbedding::default()
            },
            source_arcs: [(0, (0, 1))].into(),
        };
        assert!(
            tree_locality_divergence(&source, &original_view, &edited, &exact, &complement,)
                .is_none()
        );
        assert!(
            tree_locality_divergence(
                &source,
                &original_view,
                &edited,
                &omitted_cascade,
                &complement,
            )
            .is_some()
        );
        assert!(
            tree_locality_divergence(
                &source,
                &original_view,
                &edited,
                &unrelated_extra,
                &complement,
            )
            .is_some()
        );
    }
}
