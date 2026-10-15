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
use super::tree::{TreeOperationalError, tree_check_put_domain, tree_get, tree_put};
use crate::encode::TreeEmbedding;
use crate::fan::FanViolation;
use crate::model::{GEdge, Graph, Node, NodeShape, Table, Tree};
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

    /// First observational complement-residue field that differs. Derived
    /// identifiers and bookkeeping omitted by the shape are non-gating.
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

    fn synth_index(_: &Graph, complement: &Complement) -> SynthIndex {
        SynthIndex {
            nodes: complement.synthesized_nodes.clone(),
            edges: complement.synthesized_edges.clone(),
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
        if left.dropped_rows != right.dropped_rows {
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
        normalized_tree(left, left_index, right_index).is_some_and(|tree| tree == *right)
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
        } else if left.arcs != right.arcs {
            Some(instance_divergence("tree arcs or their order differ"))
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
    if left.dropped_nodes != right.dropped_nodes {
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

/// Declared domain status of one SSB candidate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DomainExpectation<R> {
    /// The candidate is in the put domain.
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
    /// Remove a synthesized carrier; this is a negative case.
    RemoveSynthesized,
    /// Edit a synthesized carrier; this is a negative case.
    EditSynthesized,
    /// Insert an edge explained only by contraction; this is a negative case.
    AddContractedEdge,
    /// Reuse a complement-reserved id; this is a negative case.
    ReservedIdReuse,
    /// Make a graph fan malformed; this is a negative case.
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
    /// Expected domain classification.
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

    /// Deterministic Single-Step Basis in canonical order.
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
    let id = floor_of(
        view.nodes
            .keys()
            .chain(complement.dropped_nodes.keys())
            .copied(),
    );
    (id != u32::MAX).then_some(id)
}

fn fresh_graph_edge(view: &Graph, complement: &Complement) -> Option<u32> {
    let id = floor_of(
        view.edges
            .keys()
            .chain(complement.dropped_edges.keys())
            .copied(),
    );
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
    basis.push(BasisCase {
        ordinal: basis.len(),
        mutation,
        view,
        domain: DomainExpectation::Refused(ReferenceLawError::PutDomain(vec![error])),
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
    graph_synthesized_negative_cases(view, complement, &mut basis);
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
    lens.plan
        .fates
        .values()
        .filter(|fate| matches!(fate, EdgeFate::Keep(image) if image == label))
        .count()
        == 1
}

fn graph_relabel_is_structural(view: &Graph, id: u32, anchor: &str) -> bool {
    view.edges.values().all(|edge| {
        (edge.src != id || edge.label.src == anchor) && (edge.tgt != id || edge.label.tgt == anchor)
    })
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
        for label in lens
            .tgt
            .edges
            .iter()
            .filter(|label| uniquely_representable_edge(lens, label))
        {
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

    for (id, _) in view
        .edges
        .iter()
        .filter(|(id, _)| !complement.synthesized_edges.contains_key(id))
    {
        let mut edited = view.clone();
        edited.edges.remove(id);
        push_graph_admitted(basis, Mutation::RemoveEdge, edited);
    }

    for (id, edge) in view
        .edges
        .iter()
        .filter(|(id, _)| !complement.synthesized_edges.contains_key(id))
    {
        for (source, node) in &view.nodes {
            if *source != edge.src && node.anchor == edge.label.src {
                let mut edited = view.clone();
                if let Some(edited_edge) = edited.edges.get_mut(id) {
                    edited_edge.src = *source;
                }
                push_graph_admitted(basis, Mutation::Reconnect, edited);
            }
        }
        for (target, node) in &view.nodes {
            if *target != edge.tgt && node.anchor == edge.label.tgt {
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
        for (_, edge) in view
            .edges
            .iter()
            .filter(|(id, _)| !complement.synthesized_edges.contains_key(id))
        {
            let mut edited = view.clone();
            edited.edges.insert(id, edge.clone());
            push_graph_admitted(basis, Mutation::AddParallelEdge, edited);
        }
    }

    if let Some(edge_id) = fresh_graph_edge(view, complement) {
        for label in lens
            .tgt
            .edges
            .iter()
            .filter(|label| label.src == label.tgt && uniquely_representable_edge(lens, label))
        {
            for (node, _) in view
                .nodes
                .iter()
                .filter(|(_, node)| node.anchor == label.src)
            {
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
    view: &Graph,
    complement: &Complement,
    basis: &mut Vec<BasisCase<Graph, ReferenceLawError>>,
) {
    for id in complement.synthesized_edges.keys().copied() {
        let mut edited = view.clone();
        edited.edges.remove(&id);
        push_graph_refused(
            basis,
            Mutation::RemoveSynthesized,
            edited,
            PutDomainError::SynthesizedEdited {
                elem: ViewRef::Edge(id),
            },
        );

        if let Some(edge) = view.edges.get(&id) {
            let mut edited = view.clone();
            if let Some(edited_edge) = edited.edges.get_mut(&id) {
                edited_edge.label.kind = format!("{}#", edge.label.kind);
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
    for id in complement.synthesized_nodes.keys().copied() {
        let mut removed = view.clone();
        removed.nodes.remove(&id);
        push_graph_refused(
            basis,
            Mutation::RemoveSynthesized,
            removed,
            PutDomainError::SynthesizedEdited {
                elem: ViewRef::Node(id),
            },
        );
        if view.nodes.contains_key(&id) {
            let mut edited = view.clone();
            if let Some(edited_node) = edited.nodes.get_mut(&id) {
                edited_node.anchor.push('#');
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
}

fn graph_other_negative_cases(
    lens: &GraphLens<'_>,
    view: &Graph,
    complement: &Complement,
    basis: &mut Vec<BasisCase<Graph, ReferenceLawError>>,
) {
    if let Some(id) = fresh_graph_edge(view, complement) {
        for (_, key) in complement
            .synthesized_edges
            .iter()
            .filter(|(_, key)| matches!(key, SynthKey::Contract { .. }))
        {
            let SynthKey::Contract { src, tgt, edge } = key else {
                unreachable!();
            };
            let mut edited = view.clone();
            edited.edges.insert(
                id,
                GEdge {
                    src: *src,
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

    if let Some((_, template)) = ordinary_graph_nodes(view, complement).next() {
        for id in complement.dropped_nodes.keys() {
            let mut edited = view.clone();
            let mut node = template.clone();
            node.id = *id;
            edited.nodes.insert(*id, node);
            push_graph_refused(
                basis,
                Mutation::ReservedIdReuse,
                edited,
                PutDomainError::IdCollision {
                    elem: ViewRef::Node(*id),
                },
            );
        }
    }
    for (id, edge) in &complement.dropped_edges {
        if independent_edge_refusal(lens, (&edge.label.src, &edge.label.tgt), &edge.label, *id)
            .is_some()
        {
            continue;
        }
        let mut edited = view.clone();
        edited.edges.insert(*id, edge.clone());
        push_graph_refused(
            basis,
            Mutation::ReservedIdReuse,
            edited,
            PutDomainError::IdCollision {
                elem: ViewRef::Edge(*id),
            },
        );
    }

    for (id, _) in view.nodes.iter().filter(|(id, node)| {
        node.shape == NodeShape::HyperEdge && !complement.synthesized_nodes.contains_key(id)
    }) {
        let Some(edge) = view.edges.iter().find_map(|(edge, value)| {
            (value.src == *id && is_fan_kind(&value.label.kind)).then_some(*edge)
        }) else {
            continue;
        };
        let mut edited = view.clone();
        if let Some(edited_node) = edited.nodes.get_mut(id) {
            edited_node.shape = NodeShape::Plain;
        }
        push_graph_refused(
            basis,
            Mutation::MalformedFan,
            edited,
            PutDomainError::MalformedFan {
                node: *id,
                reason: FanViolation::ReservedKindOutsideFan { edge },
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
        lens.verify_identity(
            complement,
            crate::pages::representation::canonical::ComplementShape::Graph,
        )
        .map_err(reference_lens_error)?;
        lens.check_put_domain(view, complement)
            .map_err(ReferenceLawError::PutDomain)?;
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
        _: &Graph,
        _: &Graph,
        producing_complement: &Complement,
    ) -> Complement {
        let Some(nodes) = synthesized_alignment(
            &complement.synthesized_nodes,
            &producing_complement.synthesized_nodes,
        ) else {
            return complement.clone();
        };
        let Some(edges) = synthesized_alignment(
            &complement.synthesized_edges,
            &producing_complement.synthesized_edges,
        ) else {
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
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TableLawError {
    /// Forward restriction refused.
    Restrict(RestrictError),
    /// Checked put refused.
    Put(super::table::TablePutError),
    /// The complete put-domain check refused.
    Domain(super::table::TableCheckError),
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
    let columns = &lens.plan().target_boundary().columns;
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
        for column in columns.get(table).into_iter().flat_map(BTreeMap::keys) {
            let mut edited = view.clone();
            let mut row = cells.clone();
            row.insert(column.clone(), alternate_value(cells.get(column)));
            edited.rows.insert(*id, (table.clone(), row));
            push(Mutation::ValueEdit, edited);
        }
    }
    let fresh = floor_of(
        view.rows
            .keys()
            .chain(complement.dropped_rows.keys())
            .copied(),
    );
    if fresh != u32::MAX {
        for table in columns.keys() {
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
    basis
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

/// Run the five law gates against the concrete table reference lens.
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
    tree_synthesized_negative_cases(view, complement, &mut basis);
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
    fresh_graph_node(
        &Graph {
            nodes: view.nodes.clone(),
            edges: BTreeMap::new(),
        },
        &complement.complement,
    )
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
        for label in lens
            .tgt
            .edges
            .iter()
            .filter(|label| uniquely_representable_edge(lens, label))
        {
            for (parent, _) in view
                .nodes
                .iter()
                .filter(|(_, node)| node.anchor == label.src)
            {
                let mut edited = view.clone();
                edited.nodes.insert(id, Node::new(id, &label.tgt));
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
        if edge.is_some_and(|edge| graph.synthesized_edges.contains_key(edge)) {
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
        let mut removed = view.clone();
        removed.arcs.retain(|(_, candidate, _)| *candidate != child);
        push_tree_refused(
            basis,
            Mutation::RemoveSynthesized,
            removed,
            PutDomainError::SynthesizedEdited {
                elem: ViewRef::Edge(id),
            },
        );
        if let Some(index) = view
            .arcs
            .iter()
            .position(|(_, candidate, _)| *candidate == child)
        {
            let mut edited = view.clone();
            edited.arcs[index].2.kind.push('#');
            push_tree_refused(
                basis,
                Mutation::EditSynthesized,
                edited,
                PutDomainError::SynthesizedEdited {
                    elem: ViewRef::Edge(id),
                },
            );
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
            node.anchor.push('#');
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
    let fan_edges = complement
        .embedding
        .fans
        .iter()
        .flat_map(|(_, parent, children)| {
            std::iter::once(*parent).chain(children.values().copied())
        });
    let next_edge = complement
        .embedding
        .arcs
        .values()
        .copied()
        .chain(graph.dropped_edges.keys().copied())
        .chain(graph.synthesized_edges.keys().copied())
        .chain(fan_edges)
        .max()
        .into_iter()
        .chain(graph.floor.1.checked_sub(1))
        .max()
        .map_or(Some(0), |edge| edge.checked_add(1))
        .filter(|edge| *edge != u32::MAX);
    if let (Some(id), Some(edge_id)) = (fresh_tree_node(view, complement), next_edge) {
        for (_, key) in graph
            .synthesized_edges
            .iter()
            .filter(|(_, key)| matches!(key, SynthKey::Contract { .. }))
        {
            let SynthKey::Contract { src, edge, .. } = key else {
                unreachable!();
            };
            let mut edited = view.clone();
            edited.nodes.insert(id, Node::new(id, &edge.tgt));
            edited.arcs.push((*src, id, edge.clone()));
            push_tree_refused(
                basis,
                Mutation::AddContractedEdge,
                edited,
                PutDomainError::ContractedEdgeInserted {
                    elem: ViewRef::Edge(edge_id),
                },
            );
        }
    }

    let attachment_for = |label: &crate::model::SEdge| {
        let (parent, _) = view
            .nodes
            .iter()
            .find(|(_, node)| node.anchor == label.src)?;
        let parent_anchor = graph.original_anchors.get(parent).cloned().or_else(|| {
            reverse_anchor(lens.src, lens.m, &label.src)
                .first()
                .cloned()
        })?;
        let child_anchors = reverse_anchor(lens.src, lens.m, &label.tgt);
        let [child_anchor] = child_anchors.as_slice() else {
            return None;
        };
        Some((*parent, label.clone(), parent_anchor, child_anchor.clone()))
    };
    let attachment = lens
        .tgt
        .edges
        .iter()
        .filter(|label| uniquely_representable_edge(lens, label))
        .find_map(attachment_for)
        .or_else(|| lens.tgt.edges.iter().find_map(attachment_for));
    for (id, dropped) in graph
        .dropped_nodes
        .iter()
        .filter(|(id, _)| !view.nodes.contains_key(id))
    {
        let Some((parent, label, parent_anchor, child_anchor)) = &attachment else {
            break;
        };
        let Some(edge_id) = next_edge else {
            break;
        };
        if independent_edge_refusal(lens, (parent_anchor, child_anchor), label, edge_id).is_some() {
            continue;
        }
        let mut edited = view.clone();
        let mut node = dropped.clone();
        node.anchor.clone_from(&label.tgt);
        edited.nodes.insert(*id, node);
        edited.arcs.push((*parent, *id, (*label).clone()));
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
        lens.verify_identity(
            &complement.complement,
            crate::pages::representation::canonical::ComplementShape::WType,
        )
        .map_err(reference_lens_error)?;
        tree_check_put_domain(lens, view, &complement.embedding, &complement.complement)
            .map_err(reference_lens_error)?;
        tree_put(lens, view, &complement.embedding, &complement.complement)
            .map_err(reference_lens_error)
    }

    fn check_put_domain(
        lens: &GraphLens<'a>,
        view: &Tree,
        complement: &TreeLawComplement,
        (): &(),
    ) -> Result<(), ReferenceLawError> {
        tree_check_put_domain(lens, view, &complement.embedding, &complement.complement)
            .map_err(reference_lens_error)
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
            };
        }
    };

    let basis = S::single_step_basis(lens, source, &original_view, &complement, context);
    let mut basis_families = BTreeMap::new();
    for candidate in &basis {
        *basis_families.entry(candidate.mutation).or_insert(0) += 1;
    }
    let basis_sequence = basis.iter().map(|candidate| candidate.mutation).collect();
    LawReport {
        gates: vec![
            check_get_put::<S, L>(lens, source, &original_view, &complement, context),
            check_put_get::<S, L>(lens, &basis, &complement, context),
            check_domain::<S, L>(lens, &basis, &complement, context),
            check_put_twice::<S, L>(lens, &basis, &complement, context),
            check_congruence::<S, L>(lens, &original_view, &complement, context),
        ],
        basis_cases: basis.len(),
        basis_families,
        basis_sequence,
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
            (DomainExpectation::Refused(expected), Err(observed)) if *expected == observed => {}
            (DomainExpectation::Refused(expected), Err(observed)) => {
                return GateReport {
                    gate: LawGate::Totality,
                    status: LawGateStatus::Failed(LawFailure::DomainMismatch {
                        case: candidate.ordinal,
                        expected: Some(expected.clone()),
                        observed: Some(observed),
                    }),
                };
            }
            (DomainExpectation::Refused(expected), Ok(_)) => {
                return GateReport {
                    gate: LawGate::Totality,
                    status: LawGateStatus::Failed(LawFailure::DomainMismatch {
                        case: candidate.ordinal,
                        expected: Some(expected.clone()),
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
mod locality_tests {
    use std::collections::BTreeMap;

    use super::{
        DomainExpectation, Mutation, ReferenceLawError, TreeLawComplement,
        collision_free_tree_rekey, reference_lens_error, tree_basis, tree_locality_divergence,
        tree_source_arcs,
    };
    use crate::encode::TreeEmbedding;
    use crate::migration::Migration;
    use crate::model::{GEdge, Node, SEdge, Tree};
    use crate::pages::lenses::graph::{GraphLens, LensError};
    use crate::pages::lenses::tree::TreeOperationalError;
    use crate::restrict::{Complement, NoValueLayer, SynthKey, compile};
    use crate::schema::Schema;

    fn label() -> SEdge {
        SEdge::new("r", "leaf", "child", Some("child"))
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
