//! Round-trip law verification for lenses.
//!
//! The strict law report gates on five properties:
//! - **`GetPut`**: `put(s, get(s)) = s`: round-tripping with an unmodified
//!   view recovers the original source.
//! - **`PutGet`**: `get(put(s, v)) = v`: what you put is what you get back.
//! - **Totality** against every declared put-domain outcome in the finite
//!   single-step basis: admitted views put successfully, while declared
//!   refusals match both the complete ordered domain report and raw put's
//!   first violation.
//! - **`PutTwice`** complement stability.
//! - **Congruence** under coherent synthesized-object reindexing.
//!
//! A third, **`PutPut`**, is what a *very* well-behaved lens adds: a put of
//! `v1` followed by a put of `v2` equals the put of `v2` alone. It is
//! checked separately by [`check_put_put`]. `PutGet` in the public law gate
//! is strict, including every indexed synthesized coordinate.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::Lens;
#[cfg(test)]
use crate::asymmetric::check_put_domain;
use crate::asymmetric::{Complement, get_with_identity, put_with_identity};
#[cfg(test)]
use crate::derived::DerivedMap;
use crate::error::LawViolation;
use crate::lens_shape::{GraphShape, LensShape, TreeShape};
use crate::table_shape::{TableContext, TableShape};

use panproto_inst::identity::{ComplementShape, IdentityEnvelope, IdentityPolicy};
use panproto_inst::{
    CellFate, CellPlan, ContractionMode, EdgeId, FInstance, FanViolation, FieldPresence,
    FieldTransform, GEdge, GInstance, GraphComplement, Node, PutDomainError, RowId, SynthIndex,
    SynthKey, TableComplement, Value, ViewRef, WInstance,
};
use panproto_schema::{Edge, Schema};

/// One family of single-step view mutations exercised by the law checker.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mutation {
    /// Change a node value or table cell.
    ValueEdit,
    /// Change an extra field or a second table cell.
    FieldEdit,
    /// Add a node or row.
    AddNode,
    /// Remove a node or row.
    RemoveNode,
    /// Move a node or row to another schema vertex.
    RelabelNode,
    /// Add an edge or foreign-key pair.
    AddEdge,
    /// Remove an edge or foreign-key pair.
    RemoveEdge,
    /// Change an edge's endpoints.
    Reconnect,
    /// Change an edge's schema label.
    RelabelEdge,
    /// Change the order coordinate of a source-origin edge.
    Reorder,
    /// Add an edge parallel to an existing edge.
    AddParallelEdge,
    /// Add a self-loop.
    AddSelfLoop,
    /// Remove a presentation object synthesized by get.
    RemoveSynthesized,
    /// Edit a presentation object synthesized by get.
    EditSynthesized,
    /// Add an edge where the lens contracts a source path.
    AddContractedEdge,
    /// Reuse an id reserved by the complement.
    ReservedIdReuse,
    /// Construct a malformed hyper-edge fan.
    MalformedFan,
}

impl Mutation {
    /// All mutation families in their report order.
    pub const ALL: [Self; 17] = [
        Self::ValueEdit,
        Self::FieldEdit,
        Self::AddNode,
        Self::RemoveNode,
        Self::RelabelNode,
        Self::AddEdge,
        Self::RemoveEdge,
        Self::Reconnect,
        Self::RelabelEdge,
        Self::Reorder,
        Self::AddParallelEdge,
        Self::AddSelfLoop,
        Self::RemoveSynthesized,
        Self::EditSynthesized,
        Self::AddContractedEdge,
        Self::ReservedIdReuse,
        Self::MalformedFan,
    ];

    /// Stable snake-case name used by diagnostics and bindings.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ValueEdit => "value_edit",
            Self::FieldEdit => "field_edit",
            Self::AddNode => "add_node",
            Self::RemoveNode => "remove_node",
            Self::RelabelNode => "relabel_node",
            Self::AddEdge => "add_edge",
            Self::RemoveEdge => "remove_edge",
            Self::Reconnect => "reconnect",
            Self::RelabelEdge => "relabel_edge",
            Self::Reorder => "reorder",
            Self::AddParallelEdge => "add_parallel_edge",
            Self::AddSelfLoop => "add_self_loop",
            Self::RemoveSynthesized => "remove_synthesized",
            Self::EditSynthesized => "edit_synthesized",
            Self::AddContractedEdge => "add_contracted_edge",
            Self::ReservedIdReuse => "reserved_id_reuse",
            Self::MalformedFan => "malformed_fan",
        }
    }
}

/// Why a mutation family was not run for one shape and source.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkipReason {
    /// The shape has no representation of the requested mutation.
    ShapeCannotExpress,
    /// The shape supports the family, but this view has no candidate.
    NoCandidate,
}

/// Counts observed for one mutation family.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FamilyReport {
    /// The family exercised.
    pub mutation: Mutation,
    /// Candidates accepted by the declared put domain.
    pub in_domain: u32,
    /// Accepted candidates whose put and gated checks succeeded.
    pub in_domain_ok: u32,
    /// Candidates declared outside the put domain.
    pub negative: u32,
    /// Declared refusals whose complete ordered domain report and raw put's
    /// first violation matched the certificate.
    pub negative_ok: u32,
}

/// Observed non-gating classifications of one lens.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LensClassification {
    /// The contraction mode of the compiled plan.
    pub contraction: ContractionMode,
    /// Classical state-based `PutPut` over the generated basis.
    pub put_put: Option<bool>,
    /// `PutPut` along insertion-only chains.
    pub insert_put_put: Option<bool>,
    /// `PutPut` along deletion-only chains.
    pub delete_put_put: Option<bool>,
    /// The mixed insertion/deletion law over generated chains.
    pub mixed_put_put: Option<bool>,
    /// Delete-then-reinsert restoration over generated chains.
    pub undoable: Option<bool>,
}

/// A complete run over the finite single-step basis for one source.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LawReport {
    /// Families for which at least one candidate was run.
    pub ran: Vec<FamilyReport>,
    /// Families the shape cannot express or the view cannot instantiate.
    pub skipped: Vec<(Mutation, SkipReason)>,
    /// Observed, non-gating classifications.
    pub classification: LensClassification,
}

/// The finite, labelled structural universe used by exhaustive checkers.
///
/// Instances use canonical consecutive ids, the fixed compatible payload
/// alphabet, and default metadata. The scope is therefore exhaustive only for
/// this explicitly finite universe. No isomorphism classes are identified and
/// no partial symmetry breaking is performed: independent counts for this
/// enumerator must consequently be labelled counts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SmallScope {
    /// Maximum number of nodes or rows.
    pub max_nodes: u8,
    /// Maximum number of edges, arcs or foreign-key pairs.
    pub max_edges: u8,
}

/// Candidate views for one mutation family.
#[derive(Clone, Debug)]
pub enum MutationCases<I> {
    /// Positive candidates, in deterministic order.
    Cases(Vec<I>),
    /// Why this family cannot be exercised.
    Skip(SkipReason),
}

/// One deliberately refused view and its complete ordered domain report.
#[derive(Clone, Debug)]
pub struct NegativeMutationCase<I> {
    /// The view presented to the put-domain predicate and to raw `put`.
    pub view: I,
    /// Every expected refusal, in canonical report order.
    pub expected: Vec<PutDomainError>,
}

/// Shape-specific operations needed beyond lens execution and equivalence.
pub trait LawShape: LensShape {
    /// Generate the finite single-step basis for one family.
    fn mutation_cases(
        lens: &Lens,
        source: &Self::Instance,
        view: &Self::Instance,
        complement: &Self::Complement,
        ctx: &Self::Context,
        mutation: Mutation,
    ) -> MutationCases<Self::Instance>;

    /// Deliberately refused candidates for one mutation family.
    ///
    /// Each candidate carries its complete, ordered put-domain report. The
    /// default keeps existing third-party shape implementations source
    /// compatible: shapes opt into the exact-negative basis only when they
    /// can construct a sound witness.
    fn negative_mutation_cases(
        _lens: &Lens,
        _source: &Self::Instance,
        _view: &Self::Instance,
        _complement: &Self::Complement,
        _ctx: &Self::Context,
        _mutation: Mutation,
    ) -> Vec<NegativeMutationCase<Self::Instance>> {
        Vec::new()
    }

    /// Full one-coordinate value-edit basis used by exhaustive checkers.
    ///
    /// The ordinary single-step report keeps one deterministic alternate per
    /// coordinate. This method instead returns every other value in the fixed
    /// six-value alphabet that the coordinate's declared kind admits. Shapes
    /// without a wider finite value basis may use the ordinary candidates.
    fn exhaustive_value_cases(
        lens: &Lens,
        source: &Self::Instance,
        view: &Self::Instance,
        complement: &Self::Complement,
        ctx: &Self::Context,
    ) -> Vec<Self::Instance> {
        match Self::mutation_cases(lens, source, view, complement, ctx, Mutation::ValueEdit) {
            MutationCases::Cases(cases) => cases,
            MutationCases::Skip(_) => Vec::new(),
        }
    }

    /// Exact equality of source presentations, with NaN-reflexive values.
    fn identical(a: &Self::Instance, b: &Self::Instance) -> bool;

    /// Equivalent presentation states with view and complement reindexed
    /// coherently.
    fn congruent_states(
        view: &Self::Instance,
        complement: &Self::Complement,
    ) -> Vec<(Self::Instance, Self::Complement)>;

    /// Reindex `complement` from one synthesized presentation to another.
    ///
    /// `from` indexes the view paired with `complement`; `to` indexes the
    /// exact view that the next put will receive. Stable synthesized keys
    /// determine the unique presentation-id renaming.
    fn rekey_complement(
        complement: &Self::Complement,
        from: &SynthIndex,
        to: &SynthIndex,
    ) -> Self::Complement;

    /// The first source change outside the edit footprint and exact cascade.
    fn locality_divergence(
        lens: &Lens,
        source: &Self::Instance,
        original_view: &Self::Instance,
        edited_view: &Self::Instance,
        restored: &Self::Instance,
        complement: &Self::Complement,
        ctx: &Self::Context,
    ) -> Option<String>;

    /// Intersect two views by their shape-specific stable carriers.
    fn carrier_intersection(a: &Self::Instance, b: &Self::Instance) -> Self::Instance;

    /// Reinsert content removed by `edited`, if the shape can express the
    /// inverse as one view.
    fn undo_view(original: &Self::Instance, edited: &Self::Instance) -> Option<Self::Instance>;

    /// Enumerate the finite structural universe declared by `scope`.
    fn enumerate_small(
        schema: &Schema,
        scope: SmallScope,
        ctx: &Self::Context,
    ) -> Vec<Self::Instance>;
}

/// Law-shape adapter whose value-edit family is the complete finite alphabet.
///
/// Keeping this as a shape adapter makes every consumer of `mutation_cases`
/// see the same exhaustive family: all five gates, report counts and the
/// non-gating classifications cannot silently fall back to one alternate.
struct ExhaustiveValueShape<S>(std::marker::PhantomData<S>);

impl<S: LawShape> LensShape for ExhaustiveValueShape<S> {
    type Instance = S::Instance;
    type Complement = S::Complement;
    type Context = S::Context;

    fn get(
        lens: &Lens,
        source: &Self::Instance,
        ctx: &Self::Context,
    ) -> Result<(Self::Instance, Self::Complement), crate::LensError> {
        S::get(lens, source, ctx)
    }

    fn put(
        lens: &Lens,
        view: &Self::Instance,
        complement: &Self::Complement,
        ctx: &Self::Context,
    ) -> Result<Self::Instance, crate::LensError> {
        S::put(lens, view, complement, ctx)
    }

    fn check_put_domain(
        lens: &Lens,
        view: &Self::Instance,
        complement: &Self::Complement,
        ctx: &Self::Context,
    ) -> Result<(), Vec<PutDomainError>> {
        S::check_put_domain(lens, view, complement, ctx)
    }

    fn synth_index(instance: &Self::Instance, complement: &Self::Complement) -> SynthIndex {
        S::synth_index(instance, complement)
    }

    fn equivalent(
        a: &Self::Instance,
        ia: &SynthIndex,
        b: &Self::Instance,
        ib: &SynthIndex,
    ) -> bool {
        S::equivalent(a, ia, b, ib)
    }

    fn divergence(
        a: &Self::Instance,
        ia: &SynthIndex,
        b: &Self::Instance,
        ib: &SynthIndex,
    ) -> Option<String> {
        S::divergence(a, ia, b, ib)
    }

    fn divergence_modulo_derived(
        a: &Self::Instance,
        ia: &SynthIndex,
        b: &Self::Instance,
        ib: &SynthIndex,
        derived: &crate::derived::DerivedMap,
    ) -> Option<String> {
        S::divergence_modulo_derived(a, ia, b, ib, derived)
    }

    fn complement_divergence(a: &Self::Complement, b: &Self::Complement) -> Option<String> {
        S::complement_divergence(a, b)
    }

    fn congruence_witnesses(
        instance: &Self::Instance,
        index: &SynthIndex,
    ) -> Vec<(Self::Instance, SynthIndex)> {
        S::congruence_witnesses(instance, index)
    }
}

impl<S: LawShape> LawShape for ExhaustiveValueShape<S> {
    fn mutation_cases(
        lens: &Lens,
        source: &Self::Instance,
        view: &Self::Instance,
        complement: &Self::Complement,
        ctx: &Self::Context,
        mutation: Mutation,
    ) -> MutationCases<Self::Instance> {
        if mutation == Mutation::ValueEdit {
            match S::mutation_cases(lens, source, view, complement, ctx, mutation) {
                MutationCases::Cases(_) => MutationCases::Cases(S::exhaustive_value_cases(
                    lens, source, view, complement, ctx,
                )),
                MutationCases::Skip(reason) => MutationCases::Skip(reason),
            }
        } else {
            S::mutation_cases(lens, source, view, complement, ctx, mutation)
        }
    }

    fn negative_mutation_cases(
        lens: &Lens,
        source: &Self::Instance,
        view: &Self::Instance,
        complement: &Self::Complement,
        ctx: &Self::Context,
        mutation: Mutation,
    ) -> Vec<NegativeMutationCase<Self::Instance>> {
        S::negative_mutation_cases(lens, source, view, complement, ctx, mutation)
    }

    fn identical(a: &Self::Instance, b: &Self::Instance) -> bool {
        S::identical(a, b)
    }

    fn congruent_states(
        view: &Self::Instance,
        complement: &Self::Complement,
    ) -> Vec<(Self::Instance, Self::Complement)> {
        S::congruent_states(view, complement)
    }

    fn rekey_complement(
        complement: &Self::Complement,
        from: &SynthIndex,
        to: &SynthIndex,
    ) -> Self::Complement {
        S::rekey_complement(complement, from, to)
    }

    fn locality_divergence(
        lens: &Lens,
        source: &Self::Instance,
        original_view: &Self::Instance,
        edited_view: &Self::Instance,
        restored: &Self::Instance,
        complement: &Self::Complement,
        ctx: &Self::Context,
    ) -> Option<String> {
        S::locality_divergence(
            lens,
            source,
            original_view,
            edited_view,
            restored,
            complement,
            ctx,
        )
    }

    fn carrier_intersection(a: &Self::Instance, b: &Self::Instance) -> Self::Instance {
        S::carrier_intersection(a, b)
    }

    fn undo_view(original: &Self::Instance, edited: &Self::Instance) -> Option<Self::Instance> {
        S::undo_view(original, edited)
    }

    fn enumerate_small(
        schema: &Schema,
        scope: SmallScope,
        ctx: &Self::Context,
    ) -> Vec<Self::Instance> {
        S::enumerate_small(schema, scope, ctx)
    }
}

/// `get` and `put` through one lens, with its envelope computed once for
/// every call a checker makes.
struct Bound<'a> {
    lens: &'a Lens,
    envelope: IdentityEnvelope,
}

impl<'a> Bound<'a> {
    fn new(lens: &'a Lens) -> Self {
        Self {
            lens,
            envelope: lens.identity(ComplementShape::WType),
        }
    }

    fn get(&self, instance: &WInstance) -> Result<(WInstance, Complement), LawViolation> {
        get_with_identity(self.lens, instance, &self.envelope).map_err(LawViolation::Error)
    }

    fn put(&self, view: &WInstance, complement: &Complement) -> Result<WInstance, LawViolation> {
        put_with_identity(
            self.lens,
            view,
            complement,
            &self.envelope,
            IdentityPolicy::AllowLegacy,
        )
        .map(|(source, _)| source)
        .map_err(LawViolation::Error)
    }
}

/// Run the five strict tree gates on one source and its finite single-step
/// mutation basis.
///
/// Success means that there was no counterexample found in that finite basis; it
/// is not a proof over arbitrary sources or views.
///
/// For an accepted view `v`, write `s' = put(v, c)` and
/// `get(s') = (v', c')`. `GetPut` at `s'` and exact `PutGet` (`v' = v`) imply
/// `put(v, c') = s'`, so raw `PutTwice` is a corollary in exact shapes. This
/// covers [`TableShape`] and graph or tree views with no synthesized carrier.
/// It does not follow from `WeakPutGet`. When a graph or tree view does contain
/// a synthesized carrier, this gate uses `c'` rekeyed from `v'`'s index to
/// `v`'s index. For the universal graph and tree laws, indexed `PutGet` and
/// Congruence transport `GetPut` at `s'` across that rekeying. The finite checker
/// nevertheless retains this gate because its `GetPut` pass covers the supplied
/// source, not every once-put `s'`.
///
/// # Errors
///
/// Returns the first violation in gate order: `GetPut`, `PutGet`, Totality,
/// `PutTwice`, Congruence; or [`LawViolation::Error`] for an operational
/// failure.
pub fn check_laws(lens: &Lens, instance: &WInstance) -> Result<(), LawViolation> {
    check_laws_report::<TreeShape>(lens, instance, &()).map(|_| ())
}

/// Check if two instances are structurally equivalent.
///
/// Since `WInstance` does not derive `PartialEq`, this compares structural
/// properties: root, schema root, node and arc counts, and per-node anchors,
/// values, and extra fields (values compared with NaN-tolerant equivalence).
/// This is the legacy structural comparator retained for older optic,
/// fibration, edit, and VCS checks. The strict law gates use the indexed
/// [`TreeShape`] relation and strict source identity instead.
#[must_use]
pub fn instances_equivalent(a: &WInstance, b: &WInstance) -> bool {
    if a.root != b.root || a.schema_root != b.schema_root {
        return false;
    }

    if a.node_count() != b.node_count() || a.arc_count() != b.arc_count() {
        return false;
    }

    // Check that all node IDs match and anchors are the same.
    // Value comparison delegates to `asymmetric::value_equiv` so NaN
    // payloads compare equal to themselves (the derived `PartialEq` on
    // `Value` would say NaN ≠ NaN and falsely report drift).
    for (&id, node_a) in &a.nodes {
        match b.nodes.get(&id) {
            Some(node_b) => {
                if node_a.anchor != node_b.anchor {
                    return false;
                }
                if !crate::asymmetric::presence_equiv(node_a.value.as_ref(), node_b.value.as_ref())
                {
                    return false;
                }
                if !crate::asymmetric::extra_fields_equiv(
                    &node_a.extra_fields,
                    &node_b.extra_fields,
                ) {
                    return false;
                }
            }
            None => return false,
        }
    }

    // Compare parent maps for structural consistency.
    if a.parent_map != b.parent_map {
        return false;
    }

    // Compare the arc *set*: sort by (parent, child, edge) then compare.
    let mut arcs_a: Vec<_> = a.arcs.clone();
    let mut arcs_b: Vec<_> = b.arcs.clone();
    arcs_a.sort();
    arcs_b.sort();
    if arcs_a != arcs_b {
        return false;
    }

    // And compare the order children appear in under each parent. The
    // children of a collection node are its elements in sequence, so two
    // instances with the same arc set but a different order serialize to
    // different arrays. Comparing only the sorted set reports such a pair
    // as equivalent, which is how a backward pass that permuted every
    // array could satisfy `GetPut` while handing back reordered records.
    if child_sequences(a) != child_sequences(b) {
        return false;
    }

    // Compare fans (order-independent).
    if a.fans.len() != b.fans.len() {
        return false;
    }
    let mut fans_a: Vec<_> = a.fans.clone();
    let mut fans_b: Vec<_> = b.fans.clone();
    fans_a.sort_by(|x, y| (&x.hyper_edge_id, x.parent).cmp(&(&y.hyper_edge_id, y.parent)));
    fans_b.sort_by(|x, y| (&x.hyper_edge_id, x.parent).cmp(&(&y.hyper_edge_id, y.parent)));
    if fans_a != fans_b {
        return false;
    }

    true
}

/// The ordered child list of every parent, keyed by parent id.
///
/// This is the coordinate that array serialization reads: a collection
/// node's children *are* its elements, in arc order.
fn child_sequences(instance: &WInstance) -> std::collections::HashMap<u32, Vec<u32>> {
    let mut seqs: std::collections::HashMap<u32, Vec<u32>> = std::collections::HashMap::new();
    for (parent, child, _) in &instance.arcs {
        seqs.entry(*parent).or_default().push(*child);
    }
    seqs
}

/// Structural equivalence of two [`Complement`]s.
///
/// Complements do not derive `PartialEq` (their node/value payloads need
/// the NaN-reflexive comparison used elsewhere in this crate). This is
/// `true` exactly when [`complement_divergence`] finds no diverging
/// field — dropped nodes (by id, anchor, value, and `extra_fields`),
/// dropped arcs and fans (order-independent), contraction choices,
/// parent maps, arc-edge disambiguators, snapshot `extra_fields`/values,
/// synthesized-node sets, contracted-into fibres, arc order, original
/// anchors, the identity envelope and the source fingerprint. The
/// edit-lens complement-coherence checker
/// ([`crate::edit_laws`]) and the delta-lens functoriality proptests both
/// rely on it.
pub(crate) fn complements_equivalent(a: &Complement, b: &Complement) -> bool {
    complement_divergence(a, b).is_none()
}

/// Compare two `u32`-keyed maps for structural equivalence under `eq`,
/// returning a divergence detail naming `field` (and the node id) on the
/// first mismatch, or `None` when equivalent.
fn map_divergence<V>(
    field: &str,
    a: &std::collections::HashMap<u32, V>,
    b: &std::collections::HashMap<u32, V>,
    eq: impl Fn(&V, &V) -> bool,
) -> Option<String> {
    if a.len() != b.len() {
        return Some(format!("{field} count: {} vs {}", a.len(), b.len()));
    }
    for (id, va) in a {
        match b.get(id) {
            Some(vb) if eq(va, vb) => {}
            Some(_) => return Some(format!("{field}: node {id} differs")),
            None => return Some(format!("{field}: node {id} present on one side only")),
        }
    }
    None
}

/// The first structural field on which two [`Complement`]s diverge, or
/// `None` when they are structurally equivalent.
///
/// On divergence the returned string names the field (and, where a
/// per-node map diverges, the node id) so callers can report *which*
/// component drifted rather than merely that the complements differ.
/// The field ordering matches [`complements_equivalent`].
pub(crate) fn complement_divergence(a: &Complement, b: &Complement) -> Option<String> {
    if a.source_fingerprint != b.source_fingerprint {
        return Some(format!(
            "source_fingerprint: {} vs {}",
            a.source_fingerprint, b.source_fingerprint
        ));
    }
    if a.identity != b.identity {
        return Some(format!("identity: {:?} vs {:?}", a.identity, b.identity));
    }

    // Dropped nodes: same ids, structurally equal payloads.
    if let Some(detail) = map_divergence(
        "dropped_nodes",
        &a.dropped_nodes,
        &b.dropped_nodes,
        dropped_node_equiv,
    ) {
        return Some(detail);
    }

    // Dropped arcs (order-independent).
    let mut arcs_a: Vec<_> = a.dropped_arcs.clone();
    let mut arcs_b: Vec<_> = b.dropped_arcs.clone();
    arcs_a.sort();
    arcs_b.sort();
    if arcs_a != arcs_b {
        return Some("dropped_arcs differ".to_owned());
    }

    // Dropped fans (order-independent).
    if a.dropped_fans.len() != b.dropped_fans.len() {
        return Some(format!(
            "dropped_fans count: {} vs {}",
            a.dropped_fans.len(),
            b.dropped_fans.len()
        ));
    }
    let mut fans_a: Vec<_> = a.dropped_fans.clone();
    let mut fans_b: Vec<_> = b.dropped_fans.clone();
    fans_a.sort_by(|x, y| (&x.hyper_edge_id, x.parent).cmp(&(&y.hyper_edge_id, y.parent)));
    fans_b.sort_by(|x, y| (&x.hyper_edge_id, x.parent).cmp(&(&y.hyper_edge_id, y.parent)));
    if fans_a != fans_b {
        return Some("dropped_fans differ".to_owned());
    }

    // Maps with directly comparable (Edge / u32) values.
    if a.contraction_choices != b.contraction_choices {
        return Some("contraction_choices differ".to_owned());
    }
    if a.original_parent != b.original_parent {
        return Some("original_parent differ".to_owned());
    }
    if a.arc_edges != b.arc_edges {
        return Some("arc_edges differ".to_owned());
    }
    if a.synthesized_nodes != b.synthesized_nodes {
        return Some("synthesized_nodes differ".to_owned());
    }
    if a.contracted_into != b.contracted_into {
        return Some("contracted_into differ".to_owned());
    }
    if a.original_anchors != b.original_anchors {
        return Some("original_anchors differ".to_owned());
    }
    // The source's arc sequence: the order a collection's elements come
    // back in, so two complements that record it differently restore
    // differently ordered sources.
    if a.arc_order != b.arc_order {
        return Some("arc_order differs".to_owned());
    }

    // Snapshot extra_fields and node values: NaN-reflexive comparison.
    if let Some(detail) = map_divergence(
        "original_extra_fields",
        &a.original_extra_fields,
        &b.original_extra_fields,
        crate::asymmetric::extra_fields_equiv,
    ) {
        return Some(detail);
    }
    if let Some(detail) = map_divergence(
        "original_values",
        &a.original_values,
        &b.original_values,
        |x, y| crate::asymmetric::presence_equiv(x.as_ref(), y.as_ref()),
    ) {
        return Some(detail);
    }

    None
}

/// Structural equivalence of two dropped-complement nodes.
fn dropped_node_equiv(a: &panproto_inst::Node, b: &panproto_inst::Node) -> bool {
    a.anchor == b.anchor
        && crate::asymmetric::presence_equiv(a.value.as_ref(), b.value.as_ref())
        && crate::asymmetric::extra_fields_equiv(&a.extra_fields, &b.extra_fields)
}

/// Verify only the `GetPut` law.
///
/// Success means that there was no counterexample found for this source.
///
/// # Errors
///
/// Returns [`LawViolation::GetPut`] or [`LawViolation::Error`].
pub fn check_get_put(lens: &Lens, instance: &WInstance) -> Result<(), LawViolation> {
    check_get_put_for_shape::<TreeShape>(lens, instance, &())
}

/// Verify strict `PutGet`, `get(put(s, v, c)) = v`, on the original view and
/// every admitted positive view in the deterministic single-step basis.
///
/// Success means that there was no counterexample found in that finite basis; it
/// is not a proof over arbitrary views.
///
/// # Errors
///
/// Returns [`LawViolation::PutGet`] or [`LawViolation::Error`].
pub fn check_put_get(lens: &Lens, instance: &WInstance) -> Result<(), LawViolation> {
    check_put_get_for_shape::<TreeShape>(lens, instance, &())
}

/// Check the `PutGet` law for a specific view: `get(put(s, v, c)) = v`.
///
/// The comparison is strict and uses the paired synthesized-object index:
/// every carrier, payload, derived coordinate, and indexed synthesized
/// coordinate must agree exactly.
///
/// Exposed to the crate so the `property` proptests can drive it with
/// generated view mutations (see [`property::arb_view_mutation`]).
///
/// # Errors
///
/// Returns [`LawViolation::PutGet`] or [`LawViolation::Error`].
#[cfg(test)]
pub(crate) fn check_put_get_with_view(
    lens: &Lens,
    view: &WInstance,
    complement: &Complement,
) -> Result<(), LawViolation> {
    put_get_with_view(&Bound::new(lens), view, complement)
}

/// [`check_put_get_with_view`] through an already-bound lens.
#[cfg(test)]
fn put_get_with_view(
    bound: &Bound<'_>,
    view: &WInstance,
    complement: &Complement,
) -> Result<(), LawViolation> {
    let restored = bound.put(view, complement)?;
    let (view2, complement2) = bound.get(&restored)?;
    let expected_index = TreeShape::synth_index(view, complement);
    let actual_index = TreeShape::synth_index(&view2, &complement2);
    if let Some(detail) = TreeShape::divergence(view, &expected_index, &view2, &actual_index) {
        return Err(LawViolation::PutGet { detail });
    }
    Ok(())
}

/// Verify the `PutPut` law: a put of `v1` followed by a put of `v2`
/// equals the put of `v2` alone.
///
/// Success means that there was no counterexample found for the supplied source
/// and two views.
///
/// With `(v, c) = get(s)`, the check computes `s1 = put(v1, c)` and its
/// complement `c1 = get(s1).1`, then asserts `put(v2, c1) ≡ put(v2, c)`.
/// `v1` must differ from `v`: putting the unmodified view reproduces `s`
/// (by `GetPut`), so `c1` would equal `c` and the two sides would be the
/// same computation, which checks nothing.
///
/// `PutPut` is what separates a *very well-behaved* lens from a
/// well-behaved one: well-behavedness is `GetPut` and `PutGet` alone, and
/// a lens whose `put` depends on what the source held before (as any lens
/// with a complement can) satisfies those two while failing this one,
/// which is why it is checked separately rather than as part of
/// [`check_laws`].
///
/// # Errors
///
/// Returns [`LawViolation::PutPut`] when the two sides differ, and
/// [`LawViolation::Error`] when an operation fails or `v1` equals the view
/// of `s`.
pub fn check_put_put(
    lens: &Lens,
    source: &WInstance,
    first_view: &WInstance,
    second_view: &WInstance,
) -> Result<(), LawViolation> {
    let bound = Bound::new(lens);
    let (view, complement) = bound.get(source)?;
    let base_index = TreeShape::synth_index(&view, &complement);
    let first_index = TreeShape::synth_index(first_view, &complement);
    if TreeShape::equivalent(first_view, &first_index, &view, &base_index) {
        return Err(LawViolation::Error(crate::LensError::ComplementMismatch {
            detail: "PutPut needs a first view that differs from the source's own view; putting \
                     the unmodified view reproduces the source, so the check would compare a \
                     computation with itself"
                .to_owned(),
        }));
    }

    let after_first = bound.put(first_view, &complement)?;
    let (view_after_first, complement_after_first) = bound.get(&after_first)?;
    let after_first_index = TreeShape::synth_index(&view_after_first, &complement_after_first);
    let second_index = TreeShape::synth_index(second_view, &complement);
    let chained_complement =
        rekey_tree_complement(&complement_after_first, &after_first_index, &second_index);
    let chained = bound.put(second_view, &chained_complement)?;
    let direct = bound.put(second_view, &complement)?;

    if !tree_identical(&chained, &direct) {
        return Err(LawViolation::PutPut {
            detail: format!(
                "putting the first view and then the second gives {} nodes and {} arcs; putting \
                 the second alone gives {} nodes and {} arcs",
                chained.node_count(),
                chained.arc_count(),
                direct.node_count(),
                direct.arc_count(),
            ),
        });
    }
    Ok(())
}

/// Create a copy of `view` with its scalar leaves perturbed, keeping only
/// the perturbations that leave the view inside the domain of [`put`].
///
/// `PutGet` quantifies over the views `put` accepts, and the strict tree
/// `put` refuses a view whose leaf a computed field reads (the computed
/// field would no longer match), whose leaf a transform without a usable
/// inverse writes, or whose leaf a survival predicate reads so that the
/// node would be hidden again. Perturbing such a leaf on its own tests
/// nothing about the law; it only reports that the view left the domain.
///
/// The mutation first perturbs every leaf at once and keeps the result if
/// [`check_put_domain`] accepts it. Otherwise it visits the leaves in
/// ascending node id and keeps each perturbation only if the view built so
/// far stays in the domain, so the result is deterministic and always a
/// view `put` accepts (the original view is, since `get` produced it).
///
/// Each scalar kind is moved off its current value so that the canned
/// mutation is non-vacuous whatever the leaf type is. Mutating only strings
/// would leave an integer-, float-, or boolean-leaved instance equal to its
/// own mutation, and the callers skip the mutated view when it matches the
/// original, so those leaf types would never exercise the mutated branch.
///
/// [`put`]: crate::asymmetric::put
#[cfg(test)]
fn modify_leaf_values_in_domain(
    lens: &Lens,
    view: &WInstance,
    complement: &Complement,
) -> WInstance {
    let in_domain = |candidate: &WInstance| check_put_domain(lens, candidate, complement).is_ok();

    let mut all = view.clone();
    for node in all.nodes.values_mut() {
        perturb_leaf(node);
    }
    if in_domain(&all) {
        return all;
    }

    let mut ids: Vec<u32> = view.nodes.keys().copied().collect();
    ids.sort_unstable();
    let mut modified = view.clone();
    for id in ids {
        let mut candidate = modified.clone();
        let changed = candidate.nodes.get_mut(&id).is_some_and(perturb_leaf);
        if changed && in_domain(&candidate) {
            modified = candidate;
        }
    }
    modified
}

/// Move a node's present scalar value off its current value, returning
/// whether anything changed.
#[cfg(test)]
fn perturb_leaf(node: &mut panproto_inst::Node) -> bool {
    use panproto_inst::value::{FieldPresence, Value};

    let Some(FieldPresence::Present(ref mut value)) = node.value else {
        return false;
    };
    match value {
        Value::Str(s) => s.push_str("_modified"),
        Value::Int(i) => *i = i.wrapping_add(1),
        Value::Float(f) => *f = alternate_float(*f),
        Value::Bool(b) => *b = !*b,
        _ => return false,
    }
    true
}

fn put_get_candidate<S: LawShape>(
    lens: &Lens,
    view: &S::Instance,
    complement: &S::Complement,
    ctx: &S::Context,
) -> Result<(), LawViolation> {
    let restored = S::put(lens, view, complement, ctx).map_err(LawViolation::Error)?;
    let (actual, actual_complement) = S::get(lens, &restored, ctx).map_err(LawViolation::Error)?;
    let expected_index = S::synth_index(view, complement);
    let actual_index = S::synth_index(&actual, &actual_complement);
    if let Some(detail) = S::divergence(view, &expected_index, &actual, &actual_index) {
        return Err(LawViolation::PutGet { detail });
    }
    Ok(())
}

fn exact_negative_oracle<S: LawShape>(
    lens: &Lens,
    candidate: &NegativeMutationCase<S::Instance>,
    complement: &S::Complement,
    ctx: &S::Context,
) -> Result<(), String> {
    let Some(first) = candidate.expected.first() else {
        return Err("negative case declared an empty refusal report".to_owned());
    };
    match S::check_put_domain(lens, &candidate.view, complement, ctx) {
        Err(found) if found == candidate.expected => {}
        Err(found) => {
            return Err(format!(
                "negative case report differs: expected {:?}, found {found:?}",
                candidate.expected
            ));
        }
        Ok(()) => return Err("negative case was admitted by the put domain".to_owned()),
    }
    match S::put(lens, &candidate.view, complement, ctx) {
        Err(crate::LensError::PutDomain(found)) if found.as_ref() == first => Ok(()),
        Err(crate::LensError::PutDomain(found)) => Err(format!(
            "negative case raw put returned the wrong first refusal: expected {first:?}, found {found:?}"
        )),
        Err(error) => Err(format!(
            "negative case raw put returned a non-domain error: {error}"
        )),
        Ok(_) => Err("negative case raw put succeeded".to_owned()),
    }
}

/// Check `GetPut` for an arbitrary law shape.
///
/// A successful finite check means that there was no counterexample found for the
/// supplied source; it is not a proof over all sources.
///
/// # Errors
///
/// Returns a typed operational error or [`LawViolation::GetPut`].
pub fn check_get_put_for_shape<S: LawShape>(
    lens: &Lens,
    source: &S::Instance,
    ctx: &S::Context,
) -> Result<(), LawViolation> {
    let (view, complement) = S::get(lens, source, ctx).map_err(LawViolation::Error)?;
    let restored = S::put(lens, &view, &complement, ctx).map_err(LawViolation::Error)?;
    if !S::identical(source, &restored) {
        return Err(LawViolation::GetPut {
            detail: "restored source differs from the original presentation".to_owned(),
        });
    }
    Ok(())
}

/// Check strict `PutGet` on the original view and every accepted positive
/// candidate in the finite single-step basis.
///
/// A pass means that there was no counterexample found in that basis.
///
/// # Errors
///
/// Returns a typed operational error or [`LawViolation::PutGet`].
pub fn check_put_get_for_shape<S: LawShape>(
    lens: &Lens,
    source: &S::Instance,
    ctx: &S::Context,
) -> Result<(), LawViolation> {
    let (view, complement) = S::get(lens, source, ctx).map_err(LawViolation::Error)?;
    put_get_candidate::<S>(lens, &view, &complement, ctx)?;
    for mutation in Mutation::ALL {
        let MutationCases::Cases(candidates) =
            S::mutation_cases(lens, source, &view, &complement, ctx, mutation)
        else {
            continue;
        };
        for candidate in candidates {
            if S::check_put_domain(lens, &candidate, &complement, ctx).is_ok() {
                let Ok(restored) = S::put(lens, &candidate, &complement, ctx) else {
                    // Totality is the later, dedicated gate for an admitted
                    // view on which put is partial.
                    continue;
                };
                let (actual, actual_complement) =
                    S::get(lens, &restored, ctx).map_err(LawViolation::Error)?;
                let expected_index = S::synth_index(&candidate, &complement);
                let actual_index = S::synth_index(&actual, &actual_complement);
                if let Some(detail) =
                    S::divergence(&candidate, &expected_index, &actual, &actual_index)
                {
                    return Err(LawViolation::PutGet {
                        detail: format!("{}: {detail}", mutation.as_str()),
                    });
                }
            }
        }
    }
    Ok(())
}

/// Check every generated mutation against its declared put-domain outcome.
///
/// A pass means that every admitted candidate could be put, and every declared
/// refusal matched both the complete ordered domain report and raw put's first
/// violation, throughout the finite single-step basis.
///
/// # Errors
///
/// Returns [`LawViolation::Totality`] for the first admitted view whose put
/// fails or the first declared refusal whose exact report or raw first
/// violation differs.
pub fn check_totality_for_shape<S: LawShape>(
    lens: &Lens,
    source: &S::Instance,
    ctx: &S::Context,
) -> Result<(), LawViolation> {
    let (view, complement) = S::get(lens, source, ctx).map_err(LawViolation::Error)?;
    for mutation in Mutation::ALL {
        if let MutationCases::Cases(candidates) =
            S::mutation_cases(lens, source, &view, &complement, ctx, mutation)
        {
            for candidate in candidates {
                if S::check_put_domain(lens, &candidate, &complement, ctx).is_ok()
                    && let Err(error) = S::put(lens, &candidate, &complement, ctx)
                {
                    return Err(LawViolation::Totality {
                        mutation,
                        error: error.to_string(),
                    });
                }
            }
        }
        for candidate in S::negative_mutation_cases(lens, source, &view, &complement, ctx, mutation)
        {
            if let Err(error) = exact_negative_oracle::<S>(lens, &candidate, &complement, ctx) {
                return Err(LawViolation::Totality { mutation, error });
            }
        }
    }
    Ok(())
}

/// Check complement stability under a repeated put of the same view.
///
/// A pass means that there was no counterexample found in the finite single-step
/// basis.
///
/// # Errors
///
/// Returns [`LawViolation::PutTwice`] on the first drift.
pub fn check_put_twice_for_shape<S: LawShape>(
    lens: &Lens,
    source: &S::Instance,
    ctx: &S::Context,
) -> Result<(), LawViolation> {
    let (view, complement) = S::get(lens, source, ctx).map_err(LawViolation::Error)?;
    let mut views = vec![view.clone()];
    for mutation in Mutation::ALL {
        if let MutationCases::Cases(candidates) =
            S::mutation_cases(lens, source, &view, &complement, ctx, mutation)
        {
            views.extend(candidates.into_iter().filter(|candidate| {
                S::check_put_domain(lens, candidate, &complement, ctx).is_ok()
            }));
        }
    }
    for candidate in views {
        let once = S::put(lens, &candidate, &complement, ctx).map_err(LawViolation::Error)?;
        let (coherent_candidate, next_complement) =
            S::get(lens, &once, ctx).map_err(LawViolation::Error)?;
        let candidate_index = S::synth_index(&candidate, &complement);
        let coherent_index = S::synth_index(&coherent_candidate, &next_complement);
        let rekeyed_complement =
            S::rekey_complement(&next_complement, &coherent_index, &candidate_index);
        let twice =
            S::put(lens, &candidate, &rekeyed_complement, ctx).map_err(LawViolation::Error)?;
        if !S::identical(&once, &twice) {
            return Err(LawViolation::PutTwice {
                detail: "the second put changed the source presentation".to_owned(),
            });
        }
    }
    Ok(())
}

/// Check equivalence congruence for coherently reindexed presentation
/// states.
///
/// A pass means that there was no counterexample found among the witnesses the
/// shape produced.
///
/// # Errors
///
/// Returns [`LawViolation::Congruence`] if equivalent states have different
/// domain verdicts or inequivalent put results.
pub fn check_congruence_for_shape<S: LawShape>(
    lens: &Lens,
    source: &S::Instance,
    ctx: &S::Context,
) -> Result<(), LawViolation> {
    let (view, complement) = S::get(lens, source, ctx).map_err(LawViolation::Error)?;
    let index = S::synth_index(&view, &complement);
    for (witness, witness_complement) in S::congruent_states(&view, &complement) {
        let witness_index = S::synth_index(&witness, &witness_complement);
        if !S::equivalent(&view, &index, &witness, &witness_index) {
            return Err(LawViolation::Congruence {
                detail: "a declared witness is not equivalent".to_owned(),
            });
        }
        if S::complement_divergence(&complement, &witness_complement).is_some() {
            return Err(LawViolation::Congruence {
                detail: "coherently reindexed complements diverge".to_owned(),
            });
        }
        let left_domain = S::check_put_domain(lens, &view, &complement, ctx).is_ok();
        let right_domain = S::check_put_domain(lens, &witness, &witness_complement, ctx).is_ok();
        if left_domain != right_domain {
            return Err(LawViolation::Congruence {
                detail: "equivalent states have different put-domain verdicts".to_owned(),
            });
        }
        if left_domain {
            let left = S::put(lens, &view, &complement, ctx).map_err(LawViolation::Error)?;
            let right =
                S::put(lens, &witness, &witness_complement, ctx).map_err(LawViolation::Error)?;
            let root_index = SynthIndex::default();
            if !S::equivalent(&left, &root_index, &right, &root_index) {
                return Err(LawViolation::Congruence {
                    detail: "equivalent presentation states put to different sources".to_owned(),
                });
            }
        }
    }
    Ok(())
}

/// Check `WeakPutGet` on every accepted view in the finite basis.
///
/// A pass means that there was no counterexample found; this checker is not part
/// of [`check_laws_report`].
///
/// # Errors
///
/// Returns [`LawViolation::WeakPutGet`] on the first counterexample.
pub fn check_weak_put_get_for_shape<S: LawShape>(
    lens: &Lens,
    source: &S::Instance,
    ctx: &S::Context,
) -> Result<(), LawViolation> {
    let (base, complement) = S::get(lens, source, ctx).map_err(LawViolation::Error)?;
    let mut candidates = vec![base.clone()];
    for mutation in Mutation::ALL {
        if let MutationCases::Cases(found) =
            S::mutation_cases(lens, source, &base, &complement, ctx, mutation)
        {
            candidates.extend(found);
        }
    }
    for view in candidates {
        if S::check_put_domain(lens, &view, &complement, ctx).is_err() {
            continue;
        }
        let once = S::put(lens, &view, &complement, ctx).map_err(LawViolation::Error)?;
        let (regotten, next_complement) = S::get(lens, &once, ctx).map_err(LawViolation::Error)?;
        let view_index = S::synth_index(&view, &complement);
        let regotten_index = S::synth_index(&regotten, &next_complement);
        let original_complement = S::rekey_complement(&complement, &view_index, &regotten_index);
        let twice =
            S::put(lens, &regotten, &original_complement, ctx).map_err(LawViolation::Error)?;
        if !S::identical(&once, &twice) {
            return Err(LawViolation::WeakPutGet {
                detail: "putting the re-read view against the original complement drifted"
                    .to_owned(),
            });
        }
    }
    Ok(())
}

fn accepted_views<S: LawShape>(
    lens: &Lens,
    source: &S::Instance,
    base: &S::Instance,
    complement: &S::Complement,
    ctx: &S::Context,
) -> Vec<(Mutation, S::Instance)> {
    let mut accepted = Vec::new();
    for mutation in Mutation::ALL {
        if let MutationCases::Cases(candidates) =
            S::mutation_cases(lens, source, base, complement, ctx, mutation)
        {
            accepted.extend(candidates.into_iter().filter_map(|candidate| {
                S::check_put_domain(lens, &candidate, complement, ctx)
                    .is_ok()
                    .then_some((mutation, candidate))
            }));
        }
    }
    accepted
}

/// Check injectivity of put on generated, inequivalent in-domain views.
///
/// A pass means that there was no counterexample found in the finite basis.
///
/// # Errors
///
/// Returns [`LawViolation::PutInjectivity`] on the first collision.
pub fn check_put_injectivity_for_shape<S: LawShape>(
    lens: &Lens,
    source: &S::Instance,
    ctx: &S::Context,
) -> Result<(), LawViolation> {
    let (base, complement) = S::get(lens, source, ctx).map_err(LawViolation::Error)?;
    let mut views = vec![(None, base.clone())];
    views.extend(
        accepted_views::<S>(lens, source, &base, &complement, ctx)
            .into_iter()
            .map(|(mutation, view)| (Some(mutation), view)),
    );
    for i in 0..views.len() {
        for j in (i + 1)..views.len() {
            let left_index = S::synth_index(&views[i].1, &complement);
            let right_index = S::synth_index(&views[j].1, &complement);
            if S::equivalent(&views[i].1, &left_index, &views[j].1, &right_index) {
                continue;
            }
            let left = S::put(lens, &views[i].1, &complement, ctx).map_err(LawViolation::Error)?;
            let right = S::put(lens, &views[j].1, &complement, ctx).map_err(LawViolation::Error)?;
            if S::identical(&left, &right) {
                return Err(LawViolation::PutInjectivity {
                    detail: format!(
                        "distinct {} and {} views put to one source",
                        views[i].0.map_or("base", Mutation::as_str),
                        views[j].0.map_or("base", Mutation::as_str)
                    ),
                });
            }
        }
    }
    Ok(())
}

/// Check that put changes exactly the direct edit footprint and its declared
/// cascade.
///
/// A pass means that there was no counterexample found in the finite basis.
///
/// # Errors
///
/// Returns [`LawViolation::Locality`] on the first extra or missing change.
pub fn check_locality_for_shape<S: LawShape>(
    lens: &Lens,
    source: &S::Instance,
    ctx: &S::Context,
) -> Result<(), LawViolation> {
    let (base, complement) = S::get(lens, source, ctx).map_err(LawViolation::Error)?;
    for (_, view) in accepted_views::<S>(lens, source, &base, &complement, ctx) {
        let restored = S::put(lens, &view, &complement, ctx).map_err(LawViolation::Error)?;
        if let Some(detail) =
            S::locality_divergence(lens, source, &base, &view, &restored, &complement, ctx)
        {
            return Err(LawViolation::Locality { detail });
        }
    }
    Ok(())
}

/// Check put totality for every admitted tree view in the finite basis.
///
/// Success means that there was no counterexample found in that finite basis.
///
/// # Errors
///
/// Returns [`LawViolation::Totality`] or [`LawViolation::Error`].
pub fn check_totality(lens: &Lens, source: &WInstance) -> Result<(), LawViolation> {
    check_totality_for_shape::<TreeShape>(lens, source, &())
}

/// Check tree complement stability under a repeated put of the same view.
///
/// Success means that there was no counterexample found in the finite basis.
///
/// # Errors
///
/// Returns [`LawViolation::PutTwice`] or [`LawViolation::Error`].
pub fn check_put_twice(lens: &Lens, source: &WInstance) -> Result<(), LawViolation> {
    check_put_twice_for_shape::<TreeShape>(lens, source, &())
}

/// Check tree equivalence congruence under coherent presentation reindexing.
///
/// Success means that there was no counterexample found among the generated
/// witnesses.
///
/// # Errors
///
/// Returns [`LawViolation::Congruence`] or [`LawViolation::Error`].
pub fn check_congruence(lens: &Lens, source: &WInstance) -> Result<(), LawViolation> {
    check_congruence_for_shape::<TreeShape>(lens, source, &())
}

/// Check tree `WeakPutGet` over the finite single-step basis.
///
/// Success means that there was no counterexample found in that finite basis.
///
/// # Errors
///
/// Returns [`LawViolation::WeakPutGet`] or [`LawViolation::Error`].
pub fn check_weak_put_get(lens: &Lens, source: &WInstance) -> Result<(), LawViolation> {
    check_weak_put_get_for_shape::<TreeShape>(lens, source, &())
}

/// Check tree put injectivity over the finite single-step basis.
///
/// Success means that there was no counterexample found in that finite basis.
///
/// # Errors
///
/// Returns [`LawViolation::PutInjectivity`] or [`LawViolation::Error`].
pub fn check_put_injectivity(lens: &Lens, source: &WInstance) -> Result<(), LawViolation> {
    check_put_injectivity_for_shape::<TreeShape>(lens, source, &())
}

/// Check the exact tree put footprint and declared deletion cascade.
///
/// Success means that there was no counterexample found in the finite basis.
///
/// # Errors
///
/// Returns [`LawViolation::Locality`] or [`LawViolation::Error`].
pub fn check_locality(lens: &Lens, source: &WInstance) -> Result<(), LawViolation> {
    check_locality_for_shape::<TreeShape>(lens, source, &())
}

fn report_families<S: LawShape>(
    lens: &Lens,
    source: &S::Instance,
    view: &S::Instance,
    complement: &S::Complement,
    ctx: &S::Context,
) -> (Vec<FamilyReport>, Vec<(Mutation, SkipReason)>) {
    let mut ran = Vec::new();
    let mut skipped = Vec::new();
    for mutation in Mutation::ALL {
        let (positive, skip_reason) =
            match S::mutation_cases(lens, source, view, complement, ctx, mutation) {
                MutationCases::Cases(candidates) => (candidates, None),
                MutationCases::Skip(reason) => (Vec::new(), Some(reason)),
            };
        let negative = S::negative_mutation_cases(lens, source, view, complement, ctx, mutation);
        if positive.is_empty() && negative.is_empty() {
            skipped.push((mutation, skip_reason.unwrap_or(SkipReason::NoCandidate)));
            continue;
        }
        let mut family = FamilyReport {
            mutation,
            in_domain: 0,
            in_domain_ok: 0,
            negative: 0,
            negative_ok: 0,
        };
        for candidate in positive {
            if S::check_put_domain(lens, &candidate, complement, ctx).is_ok() {
                family.in_domain += 1;
                if S::put(lens, &candidate, complement, ctx).is_ok() {
                    family.in_domain_ok += 1;
                }
            }
        }
        for candidate in negative {
            family.negative += 1;
            if exact_negative_oracle::<S>(lens, &candidate, complement, ctx).is_ok() {
                family.negative_ok += 1;
            }
        }
        ran.push(family);
    }
    (ran, skipped)
}

fn classify_put_put<S: LawShape>(
    lens: &Lens,
    source: &S::Instance,
    base: &S::Instance,
    complement: &S::Complement,
    ctx: &S::Context,
    first_allowed: impl Fn(Mutation) -> bool,
    second_allowed: impl Fn(Mutation) -> bool,
) -> Option<bool> {
    let firsts: Vec<_> = accepted_views::<S>(lens, source, base, complement, ctx)
        .into_iter()
        .filter(|(mutation, _)| first_allowed(*mutation))
        .collect();
    if firsts.is_empty() {
        return None;
    }
    let mut saw_chain = false;
    for (_, first) in &firsts {
        let Ok(after_first) = S::put(lens, first, complement, ctx) else {
            return Some(false);
        };
        let Ok((next_base, next_complement)) = S::get(lens, &after_first, ctx) else {
            return Some(false);
        };
        for mutation in Mutation::ALL
            .into_iter()
            .filter(|mutation| second_allowed(*mutation))
        {
            let MutationCases::Cases(seconds) = S::mutation_cases(
                lens,
                &after_first,
                &next_base,
                &next_complement,
                ctx,
                mutation,
            ) else {
                continue;
            };
            for second in seconds {
                if S::check_put_domain(lens, &second, &next_complement, ctx).is_err() {
                    continue;
                }
                saw_chain = true;
                let chained = S::put(lens, &second, &next_complement, ctx);
                let base_index = S::synth_index(base, complement);
                let second_index = S::synth_index(&second, &next_complement);
                let direct_complement = S::rekey_complement(complement, &base_index, &second_index);
                let direct = S::put(lens, &second, &direct_complement, ctx);
                if !matches!((chained, direct), (Ok(a), Ok(b)) if S::identical(&a, &b)) {
                    return Some(false);
                }
            }
        }
    }
    saw_chain.then_some(true)
}

fn classify_mixed_jr<S: LawShape>(
    lens: &Lens,
    source: &S::Instance,
    base: &S::Instance,
    complement: &S::Complement,
    ctx: &S::Context,
) -> Option<bool> {
    let is_insert = |mutation| {
        matches!(
            mutation,
            Mutation::AddNode
                | Mutation::AddEdge
                | Mutation::AddParallelEdge
                | Mutation::AddSelfLoop
        )
    };
    let insertions: Vec<_> = accepted_views::<S>(lens, source, base, complement, ctx)
        .into_iter()
        .filter(|(mutation, _)| is_insert(*mutation))
        .collect();
    if insertions.is_empty() {
        return None;
    }
    let mut saw_witness = false;
    for (_, inserted) in insertions {
        let Ok(after_insert) = S::put(lens, &inserted, complement, ctx) else {
            return Some(false);
        };
        let Ok((inserted_view, insert_complement)) = S::get(lens, &after_insert, ctx) else {
            return Some(false);
        };
        for deletion in [Mutation::RemoveNode, Mutation::RemoveEdge] {
            let MutationCases::Cases(deleted_views) = S::mutation_cases(
                lens,
                &after_insert,
                &inserted_view,
                &insert_complement,
                ctx,
                deletion,
            ) else {
                continue;
            };
            for deleted in deleted_views {
                if S::check_put_domain(lens, &deleted, &insert_complement, ctx).is_err() {
                    continue;
                }
                saw_witness = true;
                let Ok(left) = S::put(lens, &deleted, &insert_complement, ctx) else {
                    return Some(false);
                };
                let intersection = S::carrier_intersection(base, &deleted);
                let Ok(after_intersection) = S::put(lens, &intersection, complement, ctx) else {
                    return Some(false);
                };
                let Ok((delete_view, delete_complement)) = S::get(lens, &after_intersection, ctx)
                else {
                    return Some(false);
                };
                let intersection_index = S::synth_index(&delete_view, &delete_complement);
                let final_index = S::synth_index(&deleted, &insert_complement);
                let right_complement =
                    S::rekey_complement(&delete_complement, &intersection_index, &final_index);
                let Ok(right) = S::put(lens, &deleted, &right_complement, ctx) else {
                    return Some(false);
                };
                if !S::identical(&left, &right) {
                    return Some(false);
                }
            }
        }
    }
    saw_witness.then_some(true)
}

fn classify<S: LawShape>(
    lens: &Lens,
    source: &S::Instance,
    base: &S::Instance,
    complement: &S::Complement,
    ctx: &S::Context,
) -> LensClassification {
    let contraction =
        panproto_inst::RestrictPlan::compile(&lens.src_schema, &lens.tgt_schema, &lens.compiled)
            .map_or(ContractionMode::None, |plan| plan.contraction.mode());
    let is_insert = |m| {
        matches!(
            m,
            Mutation::AddNode
                | Mutation::AddEdge
                | Mutation::AddParallelEdge
                | Mutation::AddSelfLoop
        )
    };
    let is_delete = |m| matches!(m, Mutation::RemoveNode | Mutation::RemoveEdge);
    let put_put = classify_put_put::<S>(lens, source, base, complement, ctx, |_| true, |_| true);
    let insert_put_put =
        classify_put_put::<S>(lens, source, base, complement, ctx, is_insert, is_insert);
    let delete_put_put =
        classify_put_put::<S>(lens, source, base, complement, ctx, is_delete, is_delete);

    let deletes: Vec<_> = accepted_views::<S>(lens, source, base, complement, ctx)
        .into_iter()
        .filter(|(m, _)| matches!(m, Mutation::RemoveNode | Mutation::RemoveEdge))
        .collect();
    let mixed_put_put = classify_mixed_jr::<S>(lens, source, base, complement, ctx);

    let mut saw_undo = false;
    let mut all_undoable = true;
    for (_, deleted) in deletes {
        if let Some(restored_view) = S::undo_view(base, &deleted) {
            saw_undo = true;
            let restored = S::put(lens, &deleted, complement, ctx)
                .ok()
                .and_then(|after_delete| S::get(lens, &after_delete, ctx).ok())
                .and_then(|(delete_view, delete_complement)| {
                    let delete_index = S::synth_index(&delete_view, &delete_complement);
                    let base_index = S::synth_index(base, complement);
                    let restored_complement =
                        S::rekey_complement(&delete_complement, &delete_index, &base_index);
                    S::put(lens, &restored_view, &restored_complement, ctx).ok()
                });
            all_undoable &= restored
                .as_ref()
                .is_some_and(|result| S::identical(source, result));
        }
    }

    LensClassification {
        contraction,
        put_put,
        insert_put_put,
        delete_put_put,
        mixed_put_put,
        undoable: saw_undo.then_some(all_undoable),
    }
}

/// Run all gating laws and return counts and non-gating classifications.
///
/// A pass means that there was no counterexample found for this source in the
/// finite Single-Step Basis. The ordinary report uses one deterministic value
/// alternate per coordinate; it is not t-way coverage for any t of two or
/// more, and it is not a proof over all instances. The basis covers only the
/// single-edit neighbourhood of the supplied source. Multi-step behaviour is
/// exercised separately by `PutTwice`, the `PutPut` classifications and E.6.
///
/// Raw `PutTwice` follows from `GetPut` at the once-put source and exact `PutGet`:
/// if `s' = put(v, c)` and `get(s') = (v, c')`, then `GetPut` at `s'` gives
/// `put(v, c') = s'`. Thus `PutTwice` is a corollary for [`TableShape`] and for
/// graph and tree views without synthesized carriers, where divergence is
/// exact. `WeakPutGet` is insufficient. With synthesized carriers, the checker
/// rekeys `c'` to the candidate's index. For the universal graph and tree laws,
/// indexed `PutGet` makes `(v, rekey(c'))` congruent to `(v', c')`; Congruence
/// transports their puts, and `GetPut` at `s'` identifies the latter with `s'`.
/// The finite checker still retains the gate because its `GetPut` pass checks the
/// supplied source, not every once-put `s'`.
///
/// # Errors
///
/// Returns the first gate violation in the order `GetPut`, `PutGet`,
/// `Totality`, `PutTwice`, `Congruence`.
pub fn check_laws_report<S: LawShape>(
    lens: &Lens,
    source: &S::Instance,
    ctx: &S::Context,
) -> Result<LawReport, LawViolation> {
    check_get_put_for_shape::<S>(lens, source, ctx)?;
    check_put_get_for_shape::<S>(lens, source, ctx)?;
    check_totality_for_shape::<S>(lens, source, ctx)?;
    check_put_twice_for_shape::<S>(lens, source, ctx)?;
    check_congruence_for_shape::<S>(lens, source, ctx)?;
    let (view, complement) = S::get(lens, source, ctx).map_err(LawViolation::Error)?;
    let (ran, skipped) = report_families::<S>(lens, source, &view, &complement, ctx);
    let classification = classify::<S>(lens, source, &view, &complement, ctx);
    Ok(LawReport {
        ran,
        skipped,
        classification,
    })
}

pub(crate) fn check_laws_report_exhaustive<S: LawShape>(
    lens: &Lens,
    source: &S::Instance,
    ctx: &S::Context,
) -> Result<LawReport, LawViolation> {
    check_laws_report::<ExhaustiveValueShape<S>>(lens, source, ctx)
}

/// Check every canonical labelled tree instance in a finite structural scope.
///
/// Instances and reports are visited in the deterministic order produced by
/// [`LawShape::enumerate_small`]. For each value-bearing coordinate, every
/// other admitted member of the fixed six-value alphabet is checked. A
/// successful run reports "no counterexample found" in this scope; it is not
/// a proof over all tree instances or payloads. The enumeration does not prune
/// labelled isomorphs. At each enumerated source the Single-Step Basis covers
/// only its single-edit neighbourhood; `PutTwice`, the `PutPut` classifications and
/// E.6 exercise multi-step behaviour separately.
///
/// # Errors
///
/// Returns the first law violation in enumeration order.
pub fn check_laws_exhaustive(
    lens: &Lens,
    schema: &Schema,
    scope: SmallScope,
) -> Result<Vec<LawReport>, LawViolation> {
    TreeShape::enumerate_small(schema, scope, &())
        .iter()
        .map(|source| check_laws_report_exhaustive::<TreeShape>(lens, source, &()))
        .collect()
}

fn rebuild_tree(
    instance: &WInstance,
    nodes: HashMap<u32, Node>,
    arcs: Vec<(u32, u32, Edge)>,
) -> WInstance {
    WInstance::new(
        nodes,
        arcs,
        instance.fans.clone(),
        instance.root,
        instance.schema_root.clone(),
    )
}

fn alternate_value(value: &FieldPresence, kind: &str) -> Option<FieldPresence> {
    match value {
        FieldPresence::Present(Value::Bool(v)) => Some(FieldPresence::Present(Value::Bool(!v))),
        FieldPresence::Present(Value::Int(v)) => {
            Some(FieldPresence::Present(Value::Int(v.wrapping_add(1))))
        }
        FieldPresence::Present(Value::Float(v)) => {
            Some(FieldPresence::Present(Value::Float(alternate_float(*v))))
        }
        FieldPresence::Present(Value::Str(v)) => {
            Some(FieldPresence::Present(Value::Str(format!("{v}_law"))))
        }
        FieldPresence::Null => null_alternate_for_kind(kind).map(FieldPresence::Present),
        _ => Some(FieldPresence::Null),
    }
}

fn exhaustive_presence_values(kind: &str, current: &FieldPresence) -> Vec<FieldPresence> {
    finite_values_for_kind(kind)
        .into_iter()
        .map(|value| match value {
            Value::Null => FieldPresence::Null,
            value => FieldPresence::Present(value),
        })
        .filter(|candidate| candidate != current)
        .collect()
}

fn null_alternate_for_kind(kind: &str) -> Option<Value> {
    match kind {
        "string" | "str" | "text" | "date" | "date32" | "date64" | "time" | "time32" | "time64"
        | "datetime" | "dateTime" | "date-time" | "timestamp" | "duration" | "timedelta"
        | "timezone" | "uuid" | "uri" | "url" => Some(Value::Str(String::new())),
        "float" | "number" | "double" | "decimal" | "real" | "numeric" | "float16" | "float32"
        | "float64" | "f16" | "f32" | "f64" => Some(Value::Float(0.0)),
        "bool" | "boolean" => Some(Value::Bool(false)),
        "bytes" | "byte" | "binary" | "blob" | "octet-string" | "large-binary" => {
            Some(Value::Bytes(Vec::new()))
        }
        "token" | "enum" | "enumerated" | "enum-symbol" | "enum-value" => {
            Some(Value::Token(String::new()))
        }
        "cid" | "cid-link" => Some(Value::CidLink(String::new())),
        "array" | "list" | "sequence" | "set" | "tuple" | "vector" | "string-list"
        | "integer-list" => Some(Value::List(Vec::new())),
        "object" | "record" | "map" | "dict" | "struct" | "schema-object" => {
            Some(Value::Unknown(HashMap::new()))
        }
        "null" | "nil" => None,
        _ => Some(Value::Int(0)),
    }
}

fn null_alternate_for_scalar(kind: &panproto_schema::ScalarKind) -> Option<Value> {
    use panproto_schema::ScalarKind;

    match kind {
        ScalarKind::Bool | ScalarKind::Any => Some(Value::Bool(false)),
        ScalarKind::Int { .. } | ScalarKind::BigInt => Some(Value::Int(0)),
        ScalarKind::Float { .. } => Some(Value::Float(0.0)),
        ScalarKind::Decimal => panproto_inst::value::DecimalValue::parse("0")
            .ok()
            .map(Value::Decimal),
        ScalarKind::Str => Some(Value::Str(String::new())),
        ScalarKind::Bytes(_) => Some(Value::Bytes(Vec::new())),
        ScalarKind::Date => panproto_inst::value::Temporal::parse_date("1970-01-01")
            .ok()
            .map(Value::Temporal),
        ScalarKind::Time => panproto_inst::value::Temporal::parse_time("00:00:00")
            .ok()
            .map(Value::Temporal),
        ScalarKind::DateTime => {
            panproto_inst::value::Temporal::parse_date_time("1970-01-01T00:00:00")
                .ok()
                .map(Value::Temporal)
        }
        ScalarKind::Token => Some(Value::Token(String::new())),
    }
}

fn alternate_flat_value(
    value: &Value,
    scalar_kind: Option<&panproto_schema::ScalarKind>,
) -> Option<Value> {
    match value {
        Value::Bool(v) => Some(Value::Bool(!v)),
        Value::Int(v) => Some(Value::Int(v.wrapping_add(1))),
        Value::Float(v) => Some(Value::Float(alternate_float(*v))),
        Value::Str(v) => Some(Value::Str(format!("{v}_law"))),
        Value::Null => {
            scalar_kind.map_or_else(|| Some(Value::Bool(false)), null_alternate_for_scalar)
        }
        _ => Some(Value::Null),
    }
}

fn alternate_float(value: f64) -> f64 {
    if value == 0.0 { 1.0 } else { 0.0 }
}

fn alternate_position(position: Option<u32>) -> u32 {
    position.map_or(0, |value| value.checked_add(1).unwrap_or(0))
}

const fn alternate_shape(shape: &panproto_inst::NodeShape) -> panproto_inst::NodeShape {
    if matches!(shape, panproto_inst::NodeShape::Plain) {
        panproto_inst::NodeShape::List
    } else {
        panproto_inst::NodeShape::Plain
    }
}

fn fresh_u32(ids: impl Iterator<Item = u32>) -> Option<u32> {
    ids.max().map_or(Some(0), |id| id.checked_add(1))
}

fn node_for_anchor(id: u32, schema: &Schema, anchor: &panproto_gat::Name) -> Node {
    let node = Node::new(id, anchor.clone());
    let value = schema
        .vertices
        .get(anchor)
        .and_then(|vertex| match vertex.kind.as_ref() {
            "string" => Some(Value::Str(String::new())),
            "integer" | "int" => Some(Value::Int(0)),
            "number" | "float" => Some(Value::Float(0.0)),
            "boolean" | "bool" => Some(Value::Bool(false)),
            _ => None,
        });
    match value {
        Some(value) => node.with_value(FieldPresence::Present(value)),
        None => node,
    }
}

fn has_source_anchor_preimage(lens: &Lens, target: &panproto_gat::Name) -> bool {
    source_anchor_preimage_count(lens, target) > 0
}

fn source_anchor_preimage_count(lens: &Lens, target: &panproto_gat::Name) -> usize {
    if !lens.compiled.surviving_verts.contains(target) {
        return 0;
    }
    lens.src_schema
        .vertices
        .keys()
        .filter(|source| lens.compiled.vertex_remap.get(*source).unwrap_or(*source) == target)
        .count()
}

fn source_anchor_preimages<'a>(
    lens: &'a Lens,
    target: &panproto_gat::Name,
) -> Vec<&'a panproto_gat::Name> {
    if !lens.compiled.surviving_verts.contains(target) {
        return Vec::new();
    }
    lens.src_schema
        .vertices
        .keys()
        .filter(|source| lens.compiled.vertex_remap.get(*source).unwrap_or(*source) == target)
        .collect()
}

fn validation_neutral_target_anchor(lens: &Lens, target: &panproto_gat::Name) -> bool {
    let preimages = source_anchor_preimages(lens, target);
    let [source] = preimages.as_slice() else {
        return false;
    };
    let (Some(source_vertex), Some(target_vertex)) = (
        lens.src_schema.vertices.get(*source),
        lens.tgt_schema.vertices.get(target),
    ) else {
        return false;
    };
    source_vertex.kind == target_vertex.kind
        && lens.compiled.conditional_survival.is_empty()
        && lens.compiled.field_transforms.values().all(Vec::is_empty)
        && lens
            .compiled
            .op_term_assignments
            .values()
            .all(Vec::is_empty)
        && lens.src_schema.required.values().all(Vec::is_empty)
        && lens.src_schema.constraints.values().all(Vec::is_empty)
        && lens
            .src_schema
            .declarations
            .keys
            .values()
            .all(Vec::is_empty)
        && lens.src_schema.declarations.references.is_empty()
}

fn reserved_fan_kind(edge: &Edge) -> bool {
    edge.kind.as_ref() == panproto_inst::HYPER_PARENT_KIND
        || edge.kind.as_ref() == panproto_inst::HYPER_LABEL_KIND
}

fn edge_label_is_fan_neutral(plan: &panproto_inst::RestrictPlan, edge: &Edge) -> bool {
    !reserved_fan_kind(edge)
        && plan
            .edge_image
            .preimages(edge)
            .into_iter()
            .all(|source| !reserved_fan_kind(source))
}

fn tree_arc_is_fan_neutral(
    plan: &panproto_inst::RestrictPlan,
    view: &WInstance,
    parent: u32,
    child: Option<u32>,
    edge: &Edge,
) -> bool {
    edge_label_is_fan_neutral(plan, edge)
        && std::iter::once(parent).chain(child).all(|id| {
            view.nodes
                .get(&id)
                .is_some_and(|node| node.shape != panproto_inst::NodeShape::HyperEdge)
        })
}

fn graph_edge_is_fan_neutral(
    plan: &panproto_inst::RestrictPlan,
    view: &GInstance,
    src: u32,
    tgt: u32,
    edge: &Edge,
) -> bool {
    edge_label_is_fan_neutral(plan, edge)
        && [src, tgt].into_iter().all(|id| {
            view.node(id)
                .is_some_and(|node| node.shape != panproto_inst::NodeShape::HyperEdge)
        })
}

fn tree_source_anchor(
    lens: &Lens,
    view: &WInstance,
    complement: &Complement,
    id: u32,
) -> Option<panproto_gat::Name> {
    let node = view.nodes.get(&id)?;
    if let Some(recorded) = complement.original_anchors.get(&id)
        && lens.compiled.vertex_remap.get(recorded).unwrap_or(recorded) == &node.anchor
    {
        return Some(recorded.clone());
    }
    let preimages = source_anchor_preimages(lens, &node.anchor);
    let [source] = preimages.as_slice() else {
        return None;
    };
    Some((**source).clone())
}

fn isolated_synthesized_edge_replacement(
    lens: &Lens,
    plan: &panproto_inst::RestrictPlan,
    view: &WInstance,
    complement: &Complement,
    parent: u32,
    child: u32,
    replacement: &Edge,
) -> bool {
    // Presentation nodes never survive reassembly, so their incident arcs
    // cannot independently trigger the source-edge preimage check.
    if complement.synthesized_keys.contains_key(&parent)
        || complement.synthesized_keys.contains_key(&child)
    {
        return true;
    }
    let (Some(source), Some(target)) = (
        tree_source_anchor(lens, view, complement, parent),
        tree_source_anchor(lens, view, complement, child),
    ) else {
        return false;
    };
    plan.edge_image
        .preimages(replacement)
        .into_iter()
        .filter(|edge| edge.src == source && edge.tgt == target)
        .count()
        == 1
}

fn isolated_tree_attachment(
    lens: &Lens,
    plan: &panproto_inst::RestrictPlan,
    view: &WInstance,
    complement: &Complement,
    parent: u32,
) -> Option<Edge> {
    if complement.synthesized_keys.contains_key(&parent) {
        return None;
    }
    let node = view.nodes.get(&parent)?;
    let parent_source = tree_source_anchor(lens, view, complement, parent)?;
    let mut labels: Vec<_> = lens.tgt_schema.edges.keys().cloned().collect();
    labels.sort_unstable();
    for prefer_unique in [true, false] {
        for label in &labels {
            let preimages = plan.edge_image.preimages(label);
            if (preimages.len() == 1) != prefer_unique
                || !tree_arc_is_fan_neutral(plan, view, parent, None, label)
                || !validation_neutral_target_anchor(lens, &label.src)
                || !validation_neutral_target_anchor(lens, &label.tgt)
            {
                continue;
            }
            let child_sources = source_anchor_preimages(lens, &label.tgt);
            let [child_source] = child_sources.as_slice() else {
                continue;
            };
            if node.anchor == label.src
                && preimages
                    .iter()
                    .filter(|edge| edge.src == parent_source && edge.tgt == **child_source)
                    .count()
                    == 1
            {
                return Some(label.clone());
            }
        }
    }
    None
}

fn canonical_tree_arcs(view: &WInstance) -> Vec<(usize, u32, u32, &Edge)> {
    let mut arcs: Vec<_> = view
        .arcs
        .iter()
        .enumerate()
        .map(|(index, (parent, child, edge))| (index, *parent, *child, edge))
        .collect();
    arcs.sort_by(|a, b| (a.1, a.2, a.3, a.0).cmp(&(b.1, b.2, b.3, b.0)));
    arcs
}

fn collected_cases<I>(candidates: Vec<I>) -> MutationCases<I> {
    if candidates.is_empty() {
        MutationCases::Skip(SkipReason::NoCandidate)
    } else {
        MutationCases::Cases(candidates)
    }
}

fn sorted_tree_ids(view: &WInstance) -> Vec<u32> {
    let mut ids: Vec<_> = view.nodes.keys().copied().collect();
    ids.sort_unstable();
    ids
}

fn tree_value_cases(lens: &Lens, view: &WInstance, complement: &Complement) -> Vec<WInstance> {
    let mut candidates = Vec::new();
    for id in sorted_tree_ids(view) {
        if complement.synthesized_keys.contains_key(&id) {
            continue;
        }
        let replacement = view.nodes.get(&id).and_then(|node| {
            let kind = lens.tgt_schema.vertices.get(&node.anchor)?.kind.as_ref();
            alternate_value(node.value.as_ref()?, kind)
        });
        if let Some(replacement) = replacement {
            let mut next = view.clone();
            if let Some(node) = next.nodes.get_mut(&id) {
                node.value = Some(replacement);
            }
            candidates.push(next);
        }
    }
    candidates
}

fn tree_exhaustive_value_cases(
    lens: &Lens,
    view: &WInstance,
    complement: &Complement,
) -> Vec<WInstance> {
    let mut candidates = Vec::new();
    for id in sorted_tree_ids(view) {
        if complement.synthesized_keys.contains_key(&id) {
            continue;
        }
        let Some(node) = view.nodes.get(&id) else {
            continue;
        };
        let Some(current) = node.value.as_ref() else {
            continue;
        };
        let Some(kind) = lens.tgt_schema.vertices.get(&node.anchor) else {
            continue;
        };
        for replacement in exhaustive_presence_values(kind.kind.as_ref(), current) {
            let mut next = view.clone();
            if let Some(node) = next.nodes.get_mut(&id) {
                node.value = Some(replacement);
                candidates.push(next);
            }
        }
    }
    candidates
}

fn tree_field_cases(view: &WInstance, complement: &Complement) -> Vec<WInstance> {
    let mut candidates = Vec::new();
    for id in sorted_tree_ids(view) {
        if complement.synthesized_keys.contains_key(&id) {
            continue;
        }
        let mut fields: Vec<_> = view.nodes[&id].extra_fields.keys().cloned().collect();
        fields.sort_unstable();
        for field in fields {
            let mut next = view.clone();
            if let Some(node) = next.nodes.get_mut(&id) {
                let replacement = node.extra_fields.get(&field).map_or_else(
                    || Value::Bool(true),
                    |value| alternate_flat_value(value, None).unwrap_or(Value::Bool(true)),
                );
                node.extra_fields.insert(field, replacement);
                candidates.push(next);
            }
        }
        let mut discriminator = view.clone();
        if let Some(node) = discriminator.nodes.get_mut(&id) {
            node.discriminator = node
                .discriminator
                .as_ref()
                .map_or_else(|| Some("__law__".into()), |_| None);
            candidates.push(discriminator);
        }
        let mut shape = view.clone();
        if let Some(node) = shape.nodes.get_mut(&id) {
            node.shape = alternate_shape(&node.shape);
            candidates.push(shape);
        }
        let mut annotations: Vec<_> = view.nodes[&id].annotations.keys().cloned().collect();
        annotations.sort_unstable();
        for annotation in annotations {
            let mut next = view.clone();
            if let Some(node) = next.nodes.get_mut(&id) {
                let replacement = node.annotations.get(&annotation).map_or_else(
                    || Value::Bool(true),
                    |value| alternate_flat_value(value, None).unwrap_or(Value::Bool(true)),
                );
                node.annotations.insert(annotation, replacement);
                candidates.push(next);
            }
        }
    }
    candidates
}

fn tree_insert_cases(lens: &Lens, view: &WInstance, complement: &Complement) -> Vec<WInstance> {
    let Ok(id) = crate::asymmetric::fresh_view_node_id(view, complement) else {
        return Vec::new();
    };
    let synthesized = TreeShape::synth_index(view, complement);
    let insertion = (0..view.arcs.len())
        .find(|index| {
            u32::try_from(*index).is_ok_and(|id| synthesized.edges.contains_key(&EdgeId(id)))
        })
        .unwrap_or(view.arcs.len());
    let plan =
        panproto_inst::RestrictPlan::compile(&lens.src_schema, &lens.tgt_schema, &lens.compiled)
            .ok();
    let mut candidates = Vec::new();
    for parent in sorted_tree_ids(view) {
        if complement.synthesized_keys.contains_key(&parent) {
            continue;
        }
        let mut outgoing = lens
            .tgt_schema
            .outgoing_edges(&view.nodes[&parent].anchor)
            .to_vec();
        outgoing.sort_unstable();
        for edge in outgoing {
            if plan
                .as_ref()
                .is_none_or(|plan| plan.edge_image.preimages(&edge).is_empty())
            {
                continue;
            }
            let mut nodes = view.nodes.clone();
            nodes.insert(id, node_for_anchor(id, &lens.tgt_schema, &edge.tgt));
            let mut arcs = view.arcs.clone();
            arcs.insert(insertion, (parent, id, edge));
            candidates.push(rebuild_tree(view, nodes, arcs));
        }
    }
    candidates
}

fn tree_remove_node_cases(view: &WInstance, complement: &Complement) -> Vec<WInstance> {
    let mut candidates = Vec::new();
    for id in sorted_tree_ids(view).into_iter().filter(|id| {
        *id != view.root
            && !complement.synthesized_keys.contains_key(id)
            && view.children(*id).is_empty()
    }) {
        let mut nodes = view.nodes.clone();
        let mut removed = BTreeSet::from([id]);
        let mut frontier = vec![id];
        while let Some(current) = frontier.pop() {
            for (parent, child, _) in &view.arcs {
                let other = if *parent == current {
                    Some(*child)
                } else if *child == current {
                    Some(*parent)
                } else {
                    None
                };
                if let Some(other) = other
                    && complement.synthesized_keys.contains_key(&other)
                    && removed.insert(other)
                {
                    frontier.push(other);
                }
            }
        }
        for removed_id in &removed {
            nodes.remove(removed_id);
        }
        let arcs = view
            .arcs
            .iter()
            .filter(|(parent, child, _)| !removed.contains(parent) && !removed.contains(child))
            .cloned()
            .collect();
        candidates.push(rebuild_tree(view, nodes, arcs));
    }
    candidates
}

fn tree_relabel_node_cases(
    lens: &Lens,
    view: &WInstance,
    complement: &Complement,
) -> Vec<WInstance> {
    let mut anchors: Vec<_> = lens.tgt_schema.vertices.keys().cloned().collect();
    anchors.sort_unstable();
    let mut candidates = Vec::new();
    for id in sorted_tree_ids(view) {
        if complement.synthesized_keys.contains_key(&id) {
            continue;
        }
        for anchor in &anchors {
            let old = &view.nodes[&id].anchor;
            let arcs_fit = view.arcs.iter().all(|(parent, child, edge)| {
                (*parent != id || edge.src == *anchor) && (*child != id || edge.tgt == *anchor)
            });
            if old != anchor && arcs_fit {
                let mut next = view.clone();
                if let Some(node) = next.nodes.get_mut(&id) {
                    node.anchor = anchor.clone();
                    // The schema root is the root node's anchor.
                    if id == view.root {
                        next.schema_root = anchor.clone();
                    }
                    candidates.push(next);
                }
            }
        }
    }
    candidates
}

fn tree_remove_edge_cases(view: &WInstance, complement: &Complement) -> Vec<WInstance> {
    let synthesized = TreeShape::synth_index(view, complement);
    let mut candidates = Vec::new();
    for (index, _, child, _) in canonical_tree_arcs(view) {
        let is_synthesized =
            u32::try_from(index).is_ok_and(|id| synthesized.edges.contains_key(&EdgeId(id)));
        if !is_synthesized && view.children(child).is_empty() {
            let mut nodes = view.nodes.clone();
            nodes.remove(&child);
            let arcs = view
                .arcs
                .iter()
                .filter(|(_, candidate, _)| *candidate != child)
                .cloned()
                .collect();
            candidates.push(rebuild_tree(view, nodes, arcs));
        }
    }
    candidates
}

fn tree_reconnect_cases(view: &WInstance, complement: &Complement) -> Vec<WInstance> {
    let synthesized = TreeShape::synth_index(view, complement);
    let mut candidates = Vec::new();
    for (index, old_parent, child, edge) in canonical_tree_arcs(view) {
        if u32::try_from(index).is_ok_and(|id| synthesized.edges.contains_key(&EdgeId(id))) {
            continue;
        }
        let descendants = tree_descendants(view, child);
        for parent in sorted_tree_ids(view) {
            if parent != old_parent
                && parent != child
                && !descendants.contains(&parent)
                && view.nodes[&parent].anchor == edge.src
            {
                let mut arcs = view.arcs.clone();
                arcs[index].0 = parent;
                candidates.push(rebuild_tree(view, view.nodes.clone(), arcs));
            }
        }
    }
    candidates
}

fn tree_relabel_edge_cases(
    lens: &Lens,
    view: &WInstance,
    complement: &Complement,
) -> Vec<WInstance> {
    let synthesized = TreeShape::synth_index(view, complement);
    let mut candidates = Vec::new();
    for (index, parent, child, old) in canonical_tree_arcs(view) {
        if u32::try_from(index).is_ok_and(|id| synthesized.edges.contains_key(&EdgeId(id))) {
            continue;
        }
        let mut replacements = lens
            .tgt_schema
            .edges_between(&view.nodes[&parent].anchor, &view.nodes[&child].anchor)
            .to_vec();
        replacements.sort_unstable();
        for edge in replacements {
            if edge != *old {
                let mut arcs = view.arcs.clone();
                arcs[index].2 = edge;
                candidates.push(rebuild_tree(view, view.nodes.clone(), arcs));
            }
        }
    }
    candidates
}

fn tree_reorder_cases(view: &WInstance, complement: &Complement) -> Vec<WInstance> {
    let synthesized = TreeShape::synth_index(view, complement);
    let mut candidates = Vec::new();
    for id in sorted_tree_ids(view) {
        if !complement.synthesized_keys.contains_key(&id) {
            let mut next = view.clone();
            if let Some(node) = next.nodes.get_mut(&id) {
                node.position = Some(alternate_position(node.position));
                candidates.push(next);
            }
        }
    }
    let arcs = canonical_tree_arcs(view);
    for (left, (i, left_parent, _, _)) in arcs.iter().enumerate() {
        for (j, right_parent, _, _) in arcs.iter().skip(left + 1) {
            let i_synthesized =
                u32::try_from(*i).is_ok_and(|id| synthesized.edges.contains_key(&EdgeId(id)));
            let j_synthesized =
                u32::try_from(*j).is_ok_and(|id| synthesized.edges.contains_key(&EdgeId(id)));
            if left_parent == right_parent && !i_synthesized && !j_synthesized {
                let mut reordered = view.arcs.clone();
                reordered.swap(*i, *j);
                candidates.push(rebuild_tree(view, view.nodes.clone(), reordered));
            }
        }
    }
    candidates
}

fn tree_synthesized_cases(
    lens: &Lens,
    view: &WInstance,
    complement: &Complement,
    remove: bool,
) -> Vec<NegativeMutationCase<WInstance>> {
    let plan = (!remove)
        .then(|| {
            panproto_inst::RestrictPlan::compile(&lens.src_schema, &lens.tgt_schema, &lens.compiled)
        })
        .transpose()
        .ok()
        .flatten();
    let mut candidates = Vec::new();
    let synthesized = TreeShape::synth_index(view, complement);
    for id in synthesized.edges.keys() {
        let Ok(index) = usize::try_from(id.0) else {
            continue;
        };
        let Some((parent, child, original)) = view.arcs.get(index) else {
            continue;
        };
        let mut next = view.clone();
        if remove {
            next.arcs.remove(index);
        } else if let Some((_, _, edge)) = next.arcs.get_mut(index) {
            let Some(plan) = &plan else {
                continue;
            };
            let mut replacements = lens
                .tgt_schema
                .edges_between(&original.src, &original.tgt)
                .to_vec();
            replacements.sort_unstable();
            // A different target edge with one source preimage is the only
            // structural edit known to isolate the synthesized coordinate.
            // Without one, this mutation has no sound one-coordinate witness.
            let Some(replacement) = replacements.into_iter().find(|replacement| {
                replacement != original
                    && tree_arc_is_fan_neutral(plan, view, *parent, Some(*child), replacement)
                    && isolated_synthesized_edge_replacement(
                        lens,
                        plan,
                        view,
                        complement,
                        *parent,
                        *child,
                        replacement,
                    )
            }) else {
                continue;
            };
            *edge = replacement;
        }
        candidates.push(NegativeMutationCase {
            view: rebuild_tree(view, next.nodes, next.arcs),
            expected: vec![PutDomainError::SynthesizedEdited {
                elem: ViewRef::Arc {
                    parent: *parent,
                    child: *child,
                },
            }],
        });
    }
    let mut nodes: Vec<_> = complement.synthesized_keys.keys().copied().collect();
    nodes.sort_unstable();
    for id in nodes {
        if !view.nodes.contains_key(&id) {
            continue;
        }
        let mut next = view.clone();
        if remove {
            next.nodes.remove(&id);
        } else if let Some(node) = next.nodes.get_mut(&id) {
            node.value = Some(FieldPresence::Present(Value::Int(1)));
        }
        candidates.push(NegativeMutationCase {
            view: rebuild_tree(view, next.nodes, next.arcs),
            expected: vec![PutDomainError::SynthesizedEdited {
                elem: ViewRef::Node(id),
            }],
        });
    }
    candidates
}

fn tree_contracted_cases(
    lens: &Lens,
    view: &WInstance,
    complement: &Complement,
) -> Vec<NegativeMutationCase<WInstance>> {
    let Ok(plan) =
        panproto_inst::RestrictPlan::compile(&lens.src_schema, &lens.tgt_schema, &lens.compiled)
    else {
        return Vec::new();
    };
    let Ok(child) = crate::asymmetric::fresh_view_node_id(view, complement) else {
        return Vec::new();
    };
    let synthesized = TreeShape::synth_index(view, complement);
    let insertion = (0..view.arcs.len())
        .find(|index| {
            u32::try_from(*index).is_ok_and(|id| synthesized.edges.contains_key(&EdgeId(id)))
        })
        .unwrap_or(view.arcs.len());
    let mut candidates = Vec::new();
    for key in synthesized.edges.values() {
        let SynthKey::Contract { src, edge, .. } = key else {
            continue;
        };
        if !plan.edge_image.preimages(edge).is_empty()
            || !tree_arc_is_fan_neutral(&plan, view, *src, None, edge)
            || !validation_neutral_target_anchor(lens, &edge.tgt)
        {
            continue;
        }
        let mut nodes = view.nodes.clone();
        nodes.insert(child, node_for_anchor(child, &lens.tgt_schema, &edge.tgt));
        let mut arcs = view.arcs.clone();
        arcs.insert(insertion, (*src, child, edge.clone()));
        candidates.push(NegativeMutationCase {
            view: rebuild_tree(view, nodes, arcs),
            expected: vec![PutDomainError::ContractedEdgeInserted {
                elem: ViewRef::Arc {
                    parent: *src,
                    child,
                },
            }],
        });
    }
    candidates
}

fn isolated_reserved_tree_parent(complement: &Complement, id: u32) -> Option<u32> {
    let mut incoming = complement
        .dropped_arcs
        .iter()
        .filter(|(_, child, _)| *child == id);
    let (original_parent, _, _) = incoming.next()?;
    let parent = *original_parent;
    if incoming.next().is_some()
        || complement
            .dropped_arcs
            .iter()
            .any(|(source, _, _)| *source == id)
        || complement
            .dropped_fans
            .iter()
            .any(|fan| fan.parent == id || fan.children.values().any(|child| *child == id))
        || complement.arc_order.iter().any(|&(source, child)| {
            let incident = source == id || child == id;
            let designated = source == parent && child == id;
            incident && !designated
        })
        || complement.original_parent.contains_key(&id)
        || complement
            .original_parent
            .values()
            .any(|source| *source == id)
        || complement
            .arc_edges
            .keys()
            .any(|(source, child)| *source == id || *child == id)
        || complement
            .contraction_choices
            .keys()
            .any(|(source, child)| *source == id || *child == id)
        || complement.contracted_into.contains_key(&id)
        || complement
            .contracted_into
            .values()
            .any(|source| *source == id)
        || complement.original_extra_fields.contains_key(&id)
        || complement.original_values.contains_key(&id)
        || complement.original_anchors.contains_key(&id)
    {
        return None;
    }
    Some(parent)
}

fn tree_reserved_cases(
    lens: &Lens,
    view: &WInstance,
    complement: &Complement,
) -> Vec<NegativeMutationCase<WInstance>> {
    let Ok(plan) =
        panproto_inst::RestrictPlan::compile(&lens.src_schema, &lens.tgt_schema, &lens.compiled)
    else {
        return Vec::new();
    };
    let synthesized = TreeShape::synth_index(view, complement);
    let insertion = (0..view.arcs.len())
        .find(|index| {
            u32::try_from(*index).is_ok_and(|id| synthesized.edges.contains_key(&EdgeId(id)))
        })
        .unwrap_or(view.arcs.len());
    let mut dropped: Vec<_> = complement.dropped_nodes.iter().collect();
    dropped.sort_unstable_by_key(|(id, _)| **id);
    let mut candidates = Vec::new();
    for (&id, dropped) in dropped {
        if view.nodes.contains_key(&id)
            || matches!(dropped.shape, panproto_inst::NodeShape::HyperEdge)
        {
            continue;
        }
        let Some(parent) = isolated_reserved_tree_parent(complement, id) else {
            continue;
        };
        let Some(label) = isolated_tree_attachment(lens, &plan, view, complement, parent) else {
            continue;
        };
        let mut nodes = view.nodes.clone();
        nodes.insert(id, node_for_anchor(id, &lens.tgt_schema, &label.tgt));
        let mut arcs = view.arcs.clone();
        arcs.insert(insertion, (parent, id, label));
        candidates.push(NegativeMutationCase {
            view: rebuild_tree(view, nodes, arcs),
            expected: vec![PutDomainError::IdCollision {
                elem: ViewRef::Node(id),
            }],
        });
    }
    candidates
}

fn tree_cases(
    lens: &Lens,
    view: &WInstance,
    complement: &Complement,
    mutation: Mutation,
) -> MutationCases<WInstance> {
    match mutation {
        Mutation::ValueEdit => collected_cases(tree_value_cases(lens, view, complement)),
        Mutation::FieldEdit => collected_cases(tree_field_cases(view, complement)),
        Mutation::AddNode | Mutation::AddEdge => {
            collected_cases(tree_insert_cases(lens, view, complement))
        }
        Mutation::RemoveNode => collected_cases(tree_remove_node_cases(view, complement)),
        Mutation::RelabelNode => collected_cases(tree_relabel_node_cases(lens, view, complement)),
        Mutation::RemoveEdge => collected_cases(tree_remove_edge_cases(view, complement)),
        Mutation::Reconnect => collected_cases(tree_reconnect_cases(view, complement)),
        Mutation::RelabelEdge => collected_cases(tree_relabel_edge_cases(lens, view, complement)),
        Mutation::Reorder => collected_cases(tree_reorder_cases(view, complement)),
        Mutation::AddParallelEdge | Mutation::AddSelfLoop | Mutation::MalformedFan => {
            MutationCases::Skip(SkipReason::ShapeCannotExpress)
        }
        Mutation::RemoveSynthesized
        | Mutation::EditSynthesized
        | Mutation::AddContractedEdge
        | Mutation::ReservedIdReuse => MutationCases::Skip(SkipReason::NoCandidate),
    }
}

fn tree_negative_cases(
    lens: &Lens,
    view: &WInstance,
    complement: &Complement,
    mutation: Mutation,
) -> Vec<NegativeMutationCase<WInstance>> {
    match mutation {
        Mutation::RemoveSynthesized => tree_synthesized_cases(lens, view, complement, true),
        Mutation::EditSynthesized => tree_synthesized_cases(lens, view, complement, false),
        Mutation::AddContractedEdge => tree_contracted_cases(lens, view, complement),
        Mutation::ReservedIdReuse => tree_reserved_cases(lens, view, complement),
        _ => Vec::new(),
    }
}

fn tree_descendants(instance: &WInstance, root: u32) -> HashSet<u32> {
    let mut found = HashSet::new();
    let mut work = vec![root];
    while let Some(node) = work.pop() {
        for &child in instance.children(node) {
            if found.insert(child) {
                work.push(child);
            }
        }
    }
    found
}

fn sorted_graph_nodes(view: &GInstance) -> Vec<u32> {
    let mut ids: Vec<_> = view.nodes().keys().copied().collect();
    ids.sort_unstable();
    ids
}

fn sorted_graph_edges(view: &GInstance) -> Vec<EdgeId> {
    let mut ids: Vec<_> = view.edges().keys().copied().collect();
    ids.sort_unstable();
    ids
}

fn graph_value_cases(
    lens: &Lens,
    view: &GInstance,
    complement: &GraphComplement,
) -> Vec<GInstance> {
    let mut candidates = Vec::new();
    for id in sorted_graph_nodes(view) {
        if complement.synthesized_nodes.contains_key(&id) {
            continue;
        }
        let replacement = view.node(id).and_then(|node| {
            let kind = lens.tgt_schema.vertices.get(&node.anchor)?.kind.as_ref();
            alternate_value(node.value.as_ref()?, kind)
        });
        if let Some(replacement) = replacement {
            let mut next = view.clone();
            if let Some(node) = next.node_mut(id) {
                node.value = Some(replacement);
            }
            candidates.push(next);
        }
    }
    candidates
}

fn graph_exhaustive_value_cases(
    lens: &Lens,
    view: &GInstance,
    complement: &GraphComplement,
) -> Vec<GInstance> {
    let mut candidates = Vec::new();
    for id in sorted_graph_nodes(view) {
        if complement.synthesized_nodes.contains_key(&id) {
            continue;
        }
        let Some(node) = view.node(id) else {
            continue;
        };
        let Some(current) = node.value.as_ref() else {
            continue;
        };
        let Some(kind) = lens.tgt_schema.vertices.get(&node.anchor) else {
            continue;
        };
        for replacement in exhaustive_presence_values(kind.kind.as_ref(), current) {
            let mut next = view.clone();
            if let Some(node) = next.node_mut(id) {
                node.value = Some(replacement);
                candidates.push(next);
            }
        }
    }
    candidates
}

fn graph_field_cases(view: &GInstance, complement: &GraphComplement) -> Vec<GInstance> {
    let mut candidates = Vec::new();
    for id in sorted_graph_nodes(view) {
        if complement.synthesized_nodes.contains_key(&id) {
            continue;
        }
        let mut fields: Vec<_> = view
            .node(id)
            .map(|node| node.extra_fields.keys().cloned().collect())
            .unwrap_or_default();
        fields.sort_unstable();
        for field in fields {
            let mut next = view.clone();
            if let Some(node) = next.node_mut(id) {
                let replacement = node.extra_fields.get(&field).map_or_else(
                    || Value::Bool(true),
                    |value| alternate_flat_value(value, None).unwrap_or(Value::Bool(true)),
                );
                node.extra_fields.insert(field, replacement);
                candidates.push(next);
            }
        }
        let mut discriminator = view.clone();
        if let Some(node) = discriminator.node_mut(id) {
            node.discriminator = node
                .discriminator
                .as_ref()
                .map_or_else(|| Some("__law__".into()), |_| None);
            candidates.push(discriminator);
        }
        let mut shape = view.clone();
        if let Some(node) = shape.node_mut(id) {
            node.shape = alternate_shape(&node.shape);
            candidates.push(shape);
        }
        let mut annotations: Vec<_> = view
            .node(id)
            .into_iter()
            .flat_map(|node| node.annotations.keys().cloned())
            .collect();
        annotations.sort_unstable();
        for annotation in annotations {
            let mut next = view.clone();
            if let Some(node) = next.node_mut(id) {
                let replacement = node.annotations.get(&annotation).map_or_else(
                    || Value::Bool(true),
                    |value| alternate_flat_value(value, None).unwrap_or(Value::Bool(true)),
                );
                node.annotations.insert(annotation, replacement);
                candidates.push(next);
            }
        }
    }
    candidates
}

fn graph_add_node_cases(
    lens: &Lens,
    view: &GInstance,
    complement: &GraphComplement,
) -> Vec<GInstance> {
    let Ok(id) = crate::graph_lens::fresh_view_node_id(view, complement) else {
        return Vec::new();
    };
    let mut anchors: Vec<_> = lens.tgt_schema.vertices.keys().cloned().collect();
    anchors.sort_unstable();
    let mut candidates = Vec::new();
    for anchor in anchors {
        if !has_source_anchor_preimage(lens, &anchor) {
            continue;
        }
        let mut next = view.clone();
        if next
            .insert_node(node_for_anchor(id, &lens.tgt_schema, &anchor))
            .is_ok()
        {
            candidates.push(next);
        }
    }
    candidates
}

fn graph_remove_node_cases(view: &GInstance, complement: &GraphComplement) -> Vec<GInstance> {
    let mut candidates = Vec::new();
    for id in sorted_graph_nodes(view) {
        if complement.synthesized_nodes.contains_key(&id) {
            continue;
        }
        let mut removed = BTreeSet::from([id]);
        let mut frontier = vec![id];
        while let Some(current) = frontier.pop() {
            for edge_id in view.out_edges(current).iter().chain(view.in_edges(current)) {
                let Some(edge) = view.edge(*edge_id) else {
                    continue;
                };
                let other = if edge.src == current {
                    edge.tgt
                } else {
                    edge.src
                };
                if complement.synthesized_nodes.contains_key(&other) && removed.insert(other) {
                    frontier.push(other);
                }
            }
        }
        let mut next = view.clone();
        let incident: BTreeSet<_> = removed
            .iter()
            .flat_map(|removed_id| {
                next.out_edges(*removed_id)
                    .iter()
                    .chain(next.in_edges(*removed_id))
                    .copied()
            })
            .collect();
        for edge in incident {
            let _ = next.remove_edge(edge);
        }
        let mut valid = true;
        for removed_id in removed.iter().rev() {
            valid &= next.remove_node(*removed_id).is_ok();
        }
        if valid {
            candidates.push(next);
        }
    }
    candidates
}

fn graph_relabel_node_cases(
    lens: &Lens,
    view: &GInstance,
    complement: &GraphComplement,
) -> Vec<GInstance> {
    let mut anchors: Vec<_> = lens.tgt_schema.vertices.keys().cloned().collect();
    anchors.sort_unstable();
    let mut candidates = Vec::new();
    for id in sorted_graph_nodes(view) {
        if complement.synthesized_nodes.contains_key(&id) {
            continue;
        }
        for anchor in &anchors {
            let Some(old) = view.node(id).map(|node| &node.anchor) else {
                continue;
            };
            let outgoing_fit = view.out_edges(id).iter().all(|edge| {
                view.edge(*edge)
                    .is_some_and(|edge| edge.edge.src == *anchor)
            });
            let incoming_fit = view.in_edges(id).iter().all(|edge| {
                view.edge(*edge)
                    .is_some_and(|edge| edge.edge.tgt == *anchor)
            });
            if old != anchor && outgoing_fit && incoming_fit {
                let mut next = view.clone();
                if let Some(node) = next.node_mut(id) {
                    node.anchor = anchor.clone();
                    candidates.push(next);
                }
            }
        }
    }
    candidates
}

fn graph_remove_edge_cases(view: &GInstance, complement: &GraphComplement) -> Vec<GInstance> {
    let mut candidates = Vec::new();
    for id in sorted_graph_edges(view) {
        if !complement.synthesized_edges.contains_key(&id) {
            let mut next = view.clone();
            if next.remove_edge(id).is_ok() {
                candidates.push(next);
            }
        }
    }
    candidates
}

fn graph_reconnect_cases(view: &GInstance, complement: &GraphComplement) -> Vec<GInstance> {
    let nodes = sorted_graph_nodes(view);
    let mut candidates = Vec::new();
    for id in sorted_graph_edges(view) {
        if complement.synthesized_edges.contains_key(&id) {
            continue;
        }
        for src in &nodes {
            for tgt in &nodes {
                let Some(old) = view.edge(id) else {
                    continue;
                };
                let endpoints_fit = view
                    .node(*src)
                    .is_some_and(|node| node.anchor == old.edge.src)
                    && view
                        .node(*tgt)
                        .is_some_and(|node| node.anchor == old.edge.tgt);
                if endpoints_fit && (*src, *tgt) != (old.src, old.tgt) {
                    let mut next = view.clone();
                    if next.set_endpoints(id, *src, *tgt).is_ok() {
                        candidates.push(next);
                    }
                }
            }
        }
    }
    candidates
}

fn graph_relabel_edge_cases(
    lens: &Lens,
    view: &GInstance,
    complement: &GraphComplement,
) -> Vec<GInstance> {
    let mut labels: Vec<_> = lens.tgt_schema.edges.keys().cloned().collect();
    labels.sort_unstable();
    let mut candidates = Vec::new();
    for id in sorted_graph_edges(view) {
        if complement.synthesized_edges.contains_key(&id) {
            continue;
        }
        for label in &labels {
            let fits = view.edge(id).is_some_and(|edge| {
                &edge.edge != label
                    && view
                        .node(edge.src)
                        .is_some_and(|node| node.anchor == label.src)
                    && view
                        .node(edge.tgt)
                        .is_some_and(|node| node.anchor == label.tgt)
            });
            if fits {
                let mut next = view.clone();
                if next.relabel_edge(id, label.clone()).is_ok() {
                    candidates.push(next);
                }
            }
        }
    }
    candidates
}

fn graph_reorder_cases(view: &GInstance, complement: &GraphComplement) -> Vec<GInstance> {
    let mut candidates = Vec::new();
    for id in sorted_graph_nodes(view) {
        if !complement.synthesized_nodes.contains_key(&id) {
            let mut next = view.clone();
            if let Some(node) = next.node_mut(id) {
                node.position = Some(alternate_position(node.position));
                candidates.push(next);
            }
        }
    }
    for id in sorted_graph_edges(view) {
        if !complement.synthesized_edges.contains_key(&id) {
            let mut next = view.clone();
            let position = Some(alternate_position(
                next.edge(id).and_then(|edge| edge.position),
            ));
            if next.set_edge_position(id, position).is_ok() {
                candidates.push(next);
            }
        }
    }
    candidates
}

fn graph_synthesized_cases(
    view: &GInstance,
    complement: &GraphComplement,
    remove: bool,
) -> Vec<NegativeMutationCase<GInstance>> {
    let mut candidates = Vec::new();
    for &id in complement.synthesized_nodes.keys() {
        let mut next = view.clone();
        if remove {
            let incident: BTreeSet<_> = next
                .out_edges(id)
                .iter()
                .chain(next.in_edges(id))
                .copied()
                .collect();
            for edge in &incident {
                let _ = next.remove_edge(*edge);
            }
            if next.remove_node(id).is_ok() {
                let mut expected = vec![PutDomainError::SynthesizedEdited {
                    elem: ViewRef::Node(id),
                }];
                expected.extend(incident.into_iter().filter_map(|edge| {
                    complement.synthesized_edges.contains_key(&edge).then_some(
                        PutDomainError::SynthesizedEdited {
                            elem: ViewRef::Edge(edge),
                        },
                    )
                }));
                candidates.push(NegativeMutationCase {
                    view: next,
                    expected,
                });
            }
        } else if let Some(node) = next.node_mut(id) {
            node.value = Some(FieldPresence::Present(Value::Int(1)));
            candidates.push(NegativeMutationCase {
                view: next,
                expected: vec![PutDomainError::SynthesizedEdited {
                    elem: ViewRef::Node(id),
                }],
            });
        }
    }
    for &id in complement.synthesized_edges.keys() {
        let mut next = view.clone();
        if remove && next.remove_edge(id).is_ok() {
            candidates.push(NegativeMutationCase {
                view: next,
                expected: vec![PutDomainError::SynthesizedEdited {
                    elem: ViewRef::Edge(id),
                }],
            });
        } else if !remove && next.edge(id).is_some() {
            let _ = next.set_edge_position(id, Some(1));
            candidates.push(NegativeMutationCase {
                view: next,
                expected: vec![PutDomainError::SynthesizedEdited {
                    elem: ViewRef::Edge(id),
                }],
            });
        }
    }
    candidates
}

fn graph_contracted_cases(
    lens: &Lens,
    view: &GInstance,
    complement: &GraphComplement,
) -> Vec<NegativeMutationCase<GInstance>> {
    let Ok(plan) =
        panproto_inst::RestrictPlan::compile(&lens.src_schema, &lens.tgt_schema, &lens.compiled)
    else {
        return Vec::new();
    };
    let mut candidates = Vec::new();
    for pair in plan.contraction.pairs.values() {
        let panproto_inst::PairResolution::Label(label) = &pair.resolution else {
            continue;
        };
        if !plan.edge_image.preimages(label).is_empty()
            || !edge_label_is_fan_neutral(&plan, label)
            || !validation_neutral_target_anchor(lens, &pair.target.0)
            || !validation_neutral_target_anchor(lens, &pair.target.1)
        {
            continue;
        }
        let mut srcs: Vec<_> = view
            .nodes()
            .iter()
            .filter(|(id, node)| {
                node.anchor == pair.target.0 && !complement.synthesized_nodes.contains_key(id)
            })
            .map(|(&id, _)| id)
            .collect();
        let mut tgts: Vec<_> = view
            .nodes()
            .iter()
            .filter(|(id, node)| {
                node.anchor == pair.target.1 && !complement.synthesized_nodes.contains_key(id)
            })
            .map(|(&id, _)| id)
            .collect();
        srcs.sort_unstable();
        tgts.sort_unstable();
        let Ok(id) = crate::graph_lens::fresh_view_edge_id(view, complement) else {
            continue;
        };
        for src in &srcs {
            for tgt in &tgts {
                if !graph_edge_is_fan_neutral(&plan, view, *src, *tgt, label) {
                    continue;
                }
                let mut next = view.clone();
                if next
                    .insert_edge(id, GEdge::new(*src, *tgt, label.clone()))
                    .is_ok()
                {
                    candidates.push(NegativeMutationCase {
                        view: next,
                        expected: vec![PutDomainError::ContractedEdgeInserted {
                            elem: ViewRef::Edge(id),
                        }],
                    });
                }
            }
        }
    }
    candidates
}

const fn graph_ref_mentions_node(elem: &ViewRef, id: u32) -> bool {
    match elem {
        ViewRef::Node(node) => *node == id,
        ViewRef::Arc { parent, child } => *parent == id || *child == id,
        ViewRef::Edge(_) | ViewRef::Row(_) | ViewRef::Pair { .. } => false,
    }
}

const fn graph_ref_mentions_edge(elem: &ViewRef, id: EdgeId) -> bool {
    matches!(elem, ViewRef::Edge(edge) if edge.0 == id.0)
}

const fn graph_synth_key_mentions_node(key: &SynthKey, id: u32) -> bool {
    match key {
        SynthKey::Contract { src, tgt, .. } | SynthKey::ContractExpand { src, tgt, .. } => {
            *src == id || *tgt == id
        }
        SynthKey::Expand { .. } => false,
        SynthKey::ResolutionReference { holder, .. } => graph_ref_mentions_node(holder, id),
        SynthKey::ResolutionSpan { text, .. } => graph_ref_mentions_node(text.site().holder(), id),
    }
}

const fn graph_synth_key_mentions_edge(key: &SynthKey, id: EdgeId) -> bool {
    match key {
        SynthKey::Expand { source_edge, .. } => source_edge.0 == id.0,
        SynthKey::ResolutionReference { holder, .. } => graph_ref_mentions_edge(holder, id),
        SynthKey::ResolutionSpan { text, .. } => graph_ref_mentions_edge(text.site().holder(), id),
        SynthKey::Contract { .. } | SynthKey::ContractExpand { .. } => false,
    }
}

fn graph_synth_keys(complement: &GraphComplement) -> impl Iterator<Item = &SynthKey> {
    complement
        .synthesized_nodes
        .values()
        .chain(complement.synthesized_edges.values())
}

fn reserved_graph_node_witness_is_structurally_exact(
    lens: &Lens,
    view: &GInstance,
    complement: &GraphComplement,
    candidate: &Node,
) -> bool {
    let id = candidate.id;
    complement
        .dropped_nodes
        .get(&id)
        .is_some_and(|node| node.shape != panproto_inst::NodeShape::HyperEdge)
        && view.node(id).is_none()
        && view
            .edges()
            .values()
            .all(|edge| edge.src != id && edge.tgt != id)
        // Even an endpoint-compatible dropped edge is outside this local
        // proof: restoring it can engage edge typing, fan, and stability
        // clauses that are independent of the id collision.
        && complement
            .dropped_edges
            .values()
            .all(|edge| edge.src != id && edge.tgt != id)
        && !complement
            .original_edges
            .values()
            .any(|edge| edge.src == id || edge.tgt == id)
        && !complement.original_anchors.contains_key(&id)
        && !complement.original_extra_fields.contains_key(&id)
        && !complement.original_values.contains_key(&id)
        && !complement.synthesized_nodes.iter().any(|(node, key)| {
            *node == id || graph_synth_key_mentions_node(key, id)
        })
        && !complement
            .synthesized_edges
            .values()
            .any(|key| graph_synth_key_mentions_node(key, id))
        && crate::asymmetric::node_equiv(
            candidate,
            &node_for_anchor(id, &lens.tgt_schema, &candidate.anchor),
        )
        && validation_neutral_target_anchor(lens, &candidate.anchor)
}

fn isolated_reserved_graph_edge(complement: &GraphComplement, id: EdgeId) -> bool {
    !complement.original_edges.contains_key(&id)
        && !complement.synthesized_edges.contains_key(&id)
        && !graph_synth_keys(complement).any(|key| graph_synth_key_mentions_edge(key, id))
}

fn graph_reserved_cases(
    lens: &Lens,
    view: &GInstance,
    complement: &GraphComplement,
) -> Vec<NegativeMutationCase<GInstance>> {
    let Ok(plan) =
        panproto_inst::RestrictPlan::compile(&lens.src_schema, &lens.tgt_schema, &lens.compiled)
    else {
        return Vec::new();
    };
    let mut candidates = Vec::new();
    let mut anchors: Vec<_> = lens.tgt_schema.vertices.keys().collect();
    anchors.sort_unstable();
    for &id in complement.dropped_nodes.keys() {
        let candidate = anchors
            .iter()
            .map(|anchor| node_for_anchor(id, &lens.tgt_schema, anchor))
            .find(|candidate| {
                reserved_graph_node_witness_is_structurally_exact(lens, view, complement, candidate)
            });
        let Some(candidate) = candidate else {
            continue;
        };
        let mut next = view.clone();
        if next.insert_node(candidate).is_ok() {
            candidates.push(NegativeMutationCase {
                view: next,
                expected: vec![PutDomainError::IdCollision {
                    elem: ViewRef::Node(id),
                }],
            });
        }
    }
    let mut labels: Vec<_> = lens.tgt_schema.edges.keys().cloned().collect();
    labels.sort_unstable();
    for &id in complement.dropped_edges.keys() {
        if view.edge(id).is_some() || !isolated_reserved_graph_edge(complement, id) {
            continue;
        }
        for label in &labels {
            if !edge_label_is_fan_neutral(&plan, label)
                || !validation_neutral_target_anchor(lens, &label.src)
                || !validation_neutral_target_anchor(lens, &label.tgt)
                || plan
                    .edge_image
                    .preimages(label)
                    .into_iter()
                    .filter(|source| {
                        lens.compiled
                            .vertex_remap
                            .get(&source.src)
                            .unwrap_or(&source.src)
                            == &label.src
                            && lens
                                .compiled
                                .vertex_remap
                                .get(&source.tgt)
                                .unwrap_or(&source.tgt)
                                == &label.tgt
                    })
                    .count()
                    != 1
            {
                continue;
            }
            let sources = sorted_graph_nodes(view)
                .into_iter()
                .filter(|node| {
                    view.node(*node)
                        .is_some_and(|value| value.anchor == label.src)
                        && !complement.synthesized_nodes.contains_key(node)
                })
                .collect::<Vec<_>>();
            let targets = sorted_graph_nodes(view)
                .into_iter()
                .filter(|node| {
                    view.node(*node)
                        .is_some_and(|value| value.anchor == label.tgt)
                        && !complement.synthesized_nodes.contains_key(node)
                })
                .collect::<Vec<_>>();
            for src in &sources {
                for tgt in &targets {
                    if !graph_edge_is_fan_neutral(&plan, view, *src, *tgt, label) {
                        continue;
                    }
                    let mut next = view.clone();
                    if next
                        .insert_edge(id, GEdge::new(*src, *tgt, label.clone()))
                        .is_ok()
                    {
                        candidates.push(NegativeMutationCase {
                            view: next,
                            expected: vec![PutDomainError::IdCollision {
                                elem: ViewRef::Edge(id),
                            }],
                        });
                    }
                }
            }
        }
    }
    candidates
}

fn graph_malformed_fan_cases(
    view: &GInstance,
    complement: &GraphComplement,
) -> Vec<NegativeMutationCase<GInstance>> {
    let Ok(fresh_edge) = crate::graph_lens::fresh_view_edge_id(view, complement) else {
        return Vec::new();
    };
    let mut candidates = Vec::new();
    for id in sorted_graph_nodes(view) {
        let Some(node) = view.node(id) else {
            continue;
        };
        if matches!(node.shape, panproto_inst::NodeShape::HyperEdge) {
            let mut parents: Vec<_> = view
                .out_edges(id)
                .iter()
                .copied()
                .filter(|edge| {
                    view.edge(*edge).is_some_and(|edge| {
                        edge.edge.kind.as_ref() == panproto_inst::HYPER_PARENT_KIND
                    })
                })
                .collect();
            parents.sort_unstable();
            for edge in parents {
                let mut next = view.clone();
                let Some(duplicate) = view.edge(edge).cloned() else {
                    continue;
                };
                if next.insert_edge(fresh_edge, duplicate).is_ok() {
                    let mut edges = vec![edge, fresh_edge];
                    edges.sort_unstable();
                    candidates.push(NegativeMutationCase {
                        view: next,
                        expected: vec![PutDomainError::MalformedFan {
                            node: id,
                            reason: FanViolation::MultipleParents { edges },
                        }],
                    });
                }
            }
        }
    }
    candidates
}

fn graph_cases(
    lens: &Lens,
    view: &GInstance,
    complement: &GraphComplement,
    mutation: Mutation,
) -> MutationCases<GInstance> {
    match mutation {
        Mutation::ValueEdit => collected_cases(graph_value_cases(lens, view, complement)),
        Mutation::FieldEdit => collected_cases(graph_field_cases(view, complement)),
        Mutation::AddNode => collected_cases(graph_add_node_cases(lens, view, complement)),
        Mutation::RemoveNode => collected_cases(graph_remove_node_cases(view, complement)),
        Mutation::RelabelNode => collected_cases(graph_relabel_node_cases(lens, view, complement)),
        Mutation::AddEdge => {
            let mut candidates = Vec::new();
            add_graph_edges(lens, view, complement, false, false, &mut candidates);
            collected_cases(candidates)
        }
        Mutation::RemoveEdge => collected_cases(graph_remove_edge_cases(view, complement)),
        Mutation::Reconnect => collected_cases(graph_reconnect_cases(view, complement)),
        Mutation::RelabelEdge => collected_cases(graph_relabel_edge_cases(lens, view, complement)),
        Mutation::Reorder => collected_cases(graph_reorder_cases(view, complement)),
        Mutation::AddParallelEdge => {
            let mut candidates = Vec::new();
            add_graph_edges(lens, view, complement, true, false, &mut candidates);
            collected_cases(candidates)
        }
        Mutation::AddSelfLoop => {
            let mut candidates = Vec::new();
            add_graph_edges(lens, view, complement, false, true, &mut candidates);
            collected_cases(candidates)
        }
        Mutation::RemoveSynthesized
        | Mutation::EditSynthesized
        | Mutation::AddContractedEdge
        | Mutation::ReservedIdReuse
        | Mutation::MalformedFan => MutationCases::Skip(SkipReason::NoCandidate),
    }
}

fn graph_negative_cases(
    lens: &Lens,
    view: &GInstance,
    complement: &GraphComplement,
    mutation: Mutation,
) -> Vec<NegativeMutationCase<GInstance>> {
    match mutation {
        Mutation::RemoveSynthesized => graph_synthesized_cases(view, complement, true),
        Mutation::EditSynthesized => graph_synthesized_cases(view, complement, false),
        Mutation::AddContractedEdge => graph_contracted_cases(lens, view, complement),
        Mutation::ReservedIdReuse => graph_reserved_cases(lens, view, complement),
        Mutation::MalformedFan => graph_malformed_fan_cases(view, complement),
        _ => Vec::new(),
    }
}

fn add_graph_edges(
    lens: &Lens,
    view: &GInstance,
    complement: &GraphComplement,
    parallel_only: bool,
    self_loop_only: bool,
    candidates: &mut Vec<GInstance>,
) {
    let Ok(id) = crate::graph_lens::fresh_view_edge_id(view, complement) else {
        return;
    };
    let mut labels: Vec<_> = lens.tgt_schema.edges.keys().cloned().collect();
    labels.sort_unstable();
    let plan =
        panproto_inst::RestrictPlan::compile(&lens.src_schema, &lens.tgt_schema, &lens.compiled)
            .ok();
    for label in labels {
        if plan
            .as_ref()
            .is_none_or(|plan| plan.edge_image.preimages(&label).is_empty())
        {
            continue;
        }
        let mut srcs: Vec<_> = view
            .nodes()
            .iter()
            .filter(|(id, node)| {
                node.anchor == label.src && !complement.synthesized_nodes.contains_key(id)
            })
            .map(|(&id, _)| id)
            .collect();
        let mut tgts: Vec<_> = view
            .nodes()
            .iter()
            .filter(|(id, node)| {
                node.anchor == label.tgt && !complement.synthesized_nodes.contains_key(id)
            })
            .map(|(&id, _)| id)
            .collect();
        srcs.sort_unstable();
        tgts.sort_unstable();
        for src in &srcs {
            for tgt in &tgts {
                if self_loop_only && src != tgt {
                    continue;
                }
                if parallel_only
                    && !view
                        .edges_between(*src, *tgt)
                        .into_iter()
                        .any(|edge_id| view.edge(edge_id).is_some_and(|edge| edge.edge == label))
                {
                    continue;
                }
                let mut next = view.clone();
                if next
                    .insert_edge(id, GEdge::new(*src, *tgt, label.clone()))
                    .is_ok()
                {
                    candidates.push(next);
                }
            }
        }
    }
}

fn table_cell_scalar_kind(
    lens: &Lens,
    ctx: &TableContext,
    table: &str,
    cell: &str,
) -> Option<panproto_schema::ScalarKind> {
    let mut columns: Vec<_> = lens
        .tgt_schema
        .edges
        .keys()
        .filter(|edge| {
            edge.src.as_ref() == table
                && edge.name.as_ref().is_some_and(|name| name.as_ref() == cell)
        })
        .collect();
    columns.sort_unstable();
    columns.into_iter().next().map(|edge| {
        panproto_schema::ValueSpec::for_vertex(&lens.tgt_schema, &ctx.tgt, &edge.tgt).kind
    })
}

fn table_cell_coordinates(
    lens: &Lens,
    view: &FInstance,
    ctx: &TableContext,
    id: RowId,
) -> BTreeSet<String> {
    let mut cells: BTreeSet<_> = view
        .row(id)
        .into_iter()
        .flat_map(HashMap::keys)
        .cloned()
        .collect();
    let Some(table) = view.table_of(id) else {
        return cells;
    };
    cells.extend(lens.tgt_schema.edges.keys().filter_map(|edge| {
        let target = lens.tgt_schema.vertices.get(&edge.tgt)?;
        (edge.src == *table && ctx.tgt.value_kinds.contains_key(target.kind.as_ref()))
            .then(|| edge.name.as_ref().map(ToString::to_string))
            .flatten()
    }));
    cells
}

fn table_payload_cases(
    lens: &Lens,
    view: &FInstance,
    ctx: &TableContext,
    value_edit: bool,
) -> Vec<FInstance> {
    let mut candidates = Vec::new();
    for id in view.row_ids() {
        for (index, cell) in table_cell_coordinates(lens, view, ctx, id)
            .into_iter()
            .enumerate()
        {
            if value_edit != (index == 0) {
                continue;
            }
            let scalar_kind = view
                .table_of(id)
                .and_then(|table| table_cell_scalar_kind(lens, ctx, table, &cell));
            let current = view.row(id).and_then(|row| row.get(&cell));
            let replacement = current.map_or_else(
                || {
                    scalar_kind
                        .as_ref()
                        .map_or_else(|| Some(Value::Bool(false)), null_alternate_for_scalar)
                },
                |value| alternate_flat_value(value, scalar_kind.as_ref()),
            );
            let Some(replacement) = replacement else {
                continue;
            };
            let mut next = view.clone();
            if let Some(row) = next.row_mut(id) {
                row.insert(cell, replacement);
                candidates.push(next);
            }
        }
    }
    candidates
}

fn table_exhaustive_value_cases(
    lens: &Lens,
    view: &FInstance,
    ctx: &TableContext,
) -> Vec<FInstance> {
    let mut candidates = Vec::new();
    for id in view.row_ids() {
        let Some(table) = view.table_of(id) else {
            continue;
        };
        for cell in table_cell_coordinates(lens, view, ctx, id) {
            let Some(kind) = table_cell_scalar_kind(lens, ctx, table, &cell) else {
                continue;
            };
            let Some(current) = view.row(id).and_then(|row| row.get(&cell)) else {
                continue;
            };
            for replacement in finite_values_for_scalar(&kind)
                .into_iter()
                .filter(|candidate| candidate != current)
            {
                let mut next = view.clone();
                if let Some(row) = next.row_mut(id) {
                    row.insert(cell.clone(), replacement);
                    candidates.push(next);
                }
            }
        }
    }
    candidates
}

fn table_add_row_cases(
    lens: &Lens,
    view: &FInstance,
    complement: &TableComplement,
) -> Vec<FInstance> {
    let Ok(id) = crate::table_lens::fresh_view_row_id(view, complement) else {
        return Vec::new();
    };
    let mut tables: Vec<_> = lens.tgt_schema.vertices.keys().cloned().collect();
    tables.sort_unstable();
    let mut candidates = Vec::new();
    for table in tables {
        if !has_source_anchor_preimage(lens, &table) {
            continue;
        }
        let mut next = view.clone();
        if next.insert_row(table, id, HashMap::new()).is_ok() {
            candidates.push(next);
        }
    }
    candidates
}

fn table_remove_row_cases(view: &FInstance) -> Vec<FInstance> {
    let mut candidates = Vec::new();
    for id in view.row_ids() {
        let mut next = view.clone();
        if next.remove_row(id).is_some() {
            candidates.push(next);
        }
    }
    candidates
}

fn table_relabel_row_cases(lens: &Lens, view: &FInstance) -> Vec<FInstance> {
    let mut tables: Vec<_> = lens.tgt_schema.vertices.keys().cloned().collect();
    tables.sort_unstable();
    let mut candidates = Vec::new();
    for id in view.row_ids() {
        for table in &tables {
            if view.table_of(id).is_some_and(|old| old != table) {
                let mut next = view.clone();
                if next.move_row(id, table.clone()).is_ok() {
                    candidates.push(next);
                }
            }
        }
    }
    candidates
}

fn table_remove_pair_cases(view: &FInstance, complement: &TableComplement) -> Vec<FInstance> {
    let mut candidates = Vec::new();
    for (edge, pairs) in view.foreign_keys() {
        for &(src, tgt) in pairs {
            if !complement
                .synthesized_fk_pairs
                .contains(&(edge.clone(), src, tgt))
            {
                let mut next = view.clone();
                if next.remove_foreign_key(edge, src, tgt) {
                    candidates.push(next);
                }
            }
        }
    }
    candidates
}

fn table_reconnect_cases(view: &FInstance, complement: &TableComplement) -> Vec<FInstance> {
    let ids = view.row_ids();
    let mut candidates = Vec::new();
    for (edge, pairs) in view.foreign_keys() {
        for &(src, tgt) in pairs {
            if complement
                .synthesized_fk_pairs
                .contains(&(edge.clone(), src, tgt))
            {
                continue;
            }
            for other in &ids {
                if *other != tgt && view.table_of(*other) == Some(&edge.tgt) {
                    let mut next = view.clone();
                    next.remove_foreign_key(edge, src, tgt);
                    if next.insert_foreign_key(edge.clone(), src, *other).is_ok() {
                        candidates.push(next);
                    }
                }
                if *other != src && view.table_of(*other) == Some(&edge.src) {
                    let mut next = view.clone();
                    next.remove_foreign_key(edge, src, tgt);
                    if next.insert_foreign_key(edge.clone(), *other, tgt).is_ok() {
                        candidates.push(next);
                    }
                }
            }
        }
    }
    candidates
}

fn table_relabel_pair_cases(
    lens: &Lens,
    view: &FInstance,
    complement: &TableComplement,
) -> Vec<FInstance> {
    let mut labels: Vec<_> = lens.tgt_schema.edges.keys().cloned().collect();
    labels.sort_unstable();
    let mut candidates = Vec::new();
    for (edge, pairs) in view.foreign_keys() {
        for &(src, tgt) in pairs {
            if complement
                .synthesized_fk_pairs
                .contains(&(edge.clone(), src, tgt))
            {
                continue;
            }
            for label in &labels {
                if label != edge
                    && view.table_of(src) == Some(&label.src)
                    && view.table_of(tgt) == Some(&label.tgt)
                {
                    let mut next = view.clone();
                    next.remove_foreign_key(edge, src, tgt);
                    if next.insert_foreign_key(label.clone(), src, tgt).is_ok() {
                        candidates.push(next);
                    }
                }
            }
        }
    }
    candidates
}

fn table_synthesized_cases(
    lens: &Lens,
    view: &FInstance,
    complement: &TableComplement,
    remove: bool,
) -> Vec<NegativeMutationCase<FInstance>> {
    let ids = view.row_ids();
    let plan =
        panproto_inst::RestrictPlan::compile(&lens.src_schema, &lens.tgt_schema, &lens.compiled)
            .ok();
    let mut candidates = Vec::new();
    for (edge, src, tgt) in &complement.synthesized_fk_pairs {
        if remove {
            let mut next = view.clone();
            if next.remove_foreign_key(edge, *src, *tgt) {
                candidates.push(NegativeMutationCase {
                    view: next,
                    expected: vec![PutDomainError::SynthesizedEdited {
                        elem: ViewRef::Pair {
                            edge: edge.clone(),
                            src: *src,
                            tgt: *tgt,
                        },
                    }],
                });
            }
            continue;
        }
        let Some(plan) = &plan else {
            continue;
        };
        let direct_writers = plan
            .edge_image
            .preimages(edge)
            .into_iter()
            .filter(|source| {
                lens.compiled
                    .vertex_remap
                    .get(&source.src)
                    .unwrap_or(&source.src)
                    == &edge.src
                    && lens
                        .compiled
                        .vertex_remap
                        .get(&source.tgt)
                        .unwrap_or(&source.tgt)
                        == &edge.tgt
            })
            .count();
        if direct_writers != 1
            || !validation_neutral_target_anchor(lens, &edge.src)
            || !validation_neutral_target_anchor(lens, &edge.tgt)
        {
            continue;
        }
        for (other, replace_source) in ids.iter().flat_map(|other| [(other, false), (other, true)])
        {
            let (new_src, new_tgt) = if replace_source {
                (*other, *tgt)
            } else {
                (*src, *other)
            };
            let compatible = if replace_source {
                *other != *src && view.table_of(*other) == Some(&edge.src)
            } else {
                *other != *tgt && view.table_of(*other) == Some(&edge.tgt)
            };
            if compatible
                && !view.contains_pair(edge, new_src, new_tgt)
                && !complement
                    .synthesized_fk_pairs
                    .contains(&(edge.clone(), new_src, new_tgt))
            {
                let mut next = view.clone();
                next.remove_foreign_key(edge, *src, *tgt);
                if next
                    .insert_foreign_key(edge.clone(), new_src, new_tgt)
                    .is_ok()
                {
                    candidates.push(NegativeMutationCase {
                        view: next,
                        expected: vec![PutDomainError::SynthesizedEdited {
                            elem: ViewRef::Pair {
                                edge: edge.clone(),
                                src: *src,
                                tgt: *tgt,
                            },
                        }],
                    });
                }
            }
        }
    }
    candidates
}

fn table_contracted_cases(lens: &Lens, view: &FInstance) -> Vec<NegativeMutationCase<FInstance>> {
    let Ok(plan) =
        panproto_inst::RestrictPlan::compile(&lens.src_schema, &lens.tgt_schema, &lens.compiled)
    else {
        return Vec::new();
    };
    let ids = view.row_ids();
    let mut candidates = Vec::new();
    for pair in plan.contraction.pairs.values() {
        let panproto_inst::PairResolution::Label(edge) = &pair.resolution else {
            continue;
        };
        if !plan.edge_image.preimages(edge).is_empty()
            || !validation_neutral_target_anchor(lens, &pair.target.0)
            || !validation_neutral_target_anchor(lens, &pair.target.1)
        {
            continue;
        }
        let sources: Vec<_> = ids
            .iter()
            .copied()
            .filter(|id| view.table_of(*id) == Some(&pair.target.0))
            .collect();
        let targets: Vec<_> = ids
            .iter()
            .copied()
            .filter(|id| view.table_of(*id) == Some(&pair.target.1))
            .collect();
        for src in &sources {
            for tgt in &targets {
                if !view.contains_pair(edge, *src, *tgt) {
                    let mut next = view.clone();
                    if next.insert_foreign_key(edge.clone(), *src, *tgt).is_ok() {
                        candidates.push(NegativeMutationCase {
                            view: next,
                            expected: vec![PutDomainError::ContractedEdgeInserted {
                                elem: ViewRef::Pair {
                                    edge: edge.clone(),
                                    src: *src,
                                    tgt: *tgt,
                                },
                            }],
                        });
                    }
                }
            }
        }
    }
    candidates
}

fn table_reserved_cases(
    lens: &Lens,
    view: &FInstance,
    complement: &TableComplement,
    ctx: &TableContext,
) -> Vec<NegativeMutationCase<FInstance>> {
    let neutral_table = lens
        .tgt_schema
        .vertices
        .keys()
        .filter(|table| {
            validation_neutral_target_anchor(lens, table)
                && !lens.tgt_schema.edges.keys().any(|edge| {
                    edge.src == **table
                        && lens
                            .tgt_schema
                            .vertices
                            .get(&edge.tgt)
                            .is_some_and(|target| {
                                ctx.tgt.value_kinds.contains_key(target.kind.as_ref())
                            })
                })
        })
        .min();
    let Some(neutral_table) = neutral_table else {
        return Vec::new();
    };
    let mut candidates = Vec::new();
    for &id in complement.dropped_rows.keys() {
        let isolated = !complement.dropped_cells.keys().any(|(row, _)| *row == id)
            && !complement
                .dropped_fk_pairs
                .values()
                .flatten()
                .any(|(src, tgt)| *src == id || *tgt == id)
            && !complement
                .original_fk_edges
                .keys()
                .any(|(_, src, tgt)| *src == id || *tgt == id)
            && !complement.original_tables.contains_key(&id)
            && !complement.original_cells.keys().any(|(row, _)| *row == id)
            && !complement
                .synthesized_fk_pairs
                .iter()
                .any(|(_, src, tgt)| *src == id || *tgt == id);
        if view.row(id).is_none() && isolated {
            let mut next = view.clone();
            if next
                .insert_row((*neutral_table).clone(), id, HashMap::new())
                .is_ok()
            {
                candidates.push(NegativeMutationCase {
                    view: next,
                    expected: vec![PutDomainError::IdCollision {
                        elem: ViewRef::Row(id),
                    }],
                });
            }
        }
    }
    candidates
}

fn table_cases(
    lens: &Lens,
    view: &FInstance,
    complement: &TableComplement,
    ctx: &TableContext,
    mutation: Mutation,
) -> MutationCases<FInstance> {
    match mutation {
        Mutation::ValueEdit => collected_cases(table_payload_cases(lens, view, ctx, true)),
        Mutation::FieldEdit => collected_cases(table_payload_cases(lens, view, ctx, false)),
        Mutation::AddNode => collected_cases(table_add_row_cases(lens, view, complement)),
        Mutation::RemoveNode => collected_cases(table_remove_row_cases(view)),
        Mutation::RelabelNode => collected_cases(table_relabel_row_cases(lens, view)),
        Mutation::AddEdge | Mutation::AddSelfLoop => {
            let mut candidates = Vec::new();
            add_table_pairs(
                lens,
                view,
                mutation == Mutation::AddSelfLoop,
                &mut candidates,
            );
            collected_cases(candidates)
        }
        Mutation::RemoveEdge => collected_cases(table_remove_pair_cases(view, complement)),
        Mutation::Reconnect => collected_cases(table_reconnect_cases(view, complement)),
        Mutation::RelabelEdge => collected_cases(table_relabel_pair_cases(lens, view, complement)),
        Mutation::Reorder | Mutation::AddParallelEdge | Mutation::MalformedFan => {
            MutationCases::Skip(SkipReason::ShapeCannotExpress)
        }
        Mutation::RemoveSynthesized
        | Mutation::EditSynthesized
        | Mutation::AddContractedEdge
        | Mutation::ReservedIdReuse => MutationCases::Skip(SkipReason::NoCandidate),
    }
}

fn table_negative_cases(
    lens: &Lens,
    view: &FInstance,
    complement: &TableComplement,
    ctx: &TableContext,
    mutation: Mutation,
) -> Vec<NegativeMutationCase<FInstance>> {
    match mutation {
        Mutation::RemoveSynthesized => table_synthesized_cases(lens, view, complement, true),
        Mutation::EditSynthesized => table_synthesized_cases(lens, view, complement, false),
        Mutation::AddContractedEdge => table_contracted_cases(lens, view),
        Mutation::ReservedIdReuse => table_reserved_cases(lens, view, complement, ctx),
        _ => Vec::new(),
    }
}

fn add_table_pairs(
    lens: &Lens,
    view: &FInstance,
    loops_only: bool,
    candidates: &mut Vec<FInstance>,
) {
    let mut labels: Vec<_> = lens.tgt_schema.edges.keys().cloned().collect();
    labels.sort_unstable();
    for edge in labels {
        for src in view.row_ids() {
            for tgt in view.row_ids() {
                if view.table_of(src) != Some(&edge.src)
                    || view.table_of(tgt) != Some(&edge.tgt)
                    || (loops_only && src != tgt)
                    || view.contains_pair(&edge, src, tgt)
                {
                    continue;
                }
                let mut next = view.clone();
                if next.insert_foreign_key(edge.clone(), src, tgt).is_ok() {
                    candidates.push(next);
                }
            }
        }
    }
}

fn tree_identical(a: &WInstance, b: &WInstance) -> bool {
    a.root == b.root
        && a.schema_root == b.schema_root
        && a.nodes.len() == b.nodes.len()
        && a.nodes.iter().all(|(id, node)| {
            b.nodes
                .get(id)
                .is_some_and(|other| crate::asymmetric::node_equiv(node, other))
        })
        && a.arcs == b.arcs
        && a.fans == b.fans
}

fn graph_state_witnesses(
    view: &GInstance,
    complement: &GraphComplement,
) -> Vec<(GInstance, GraphComplement)> {
    let index = GraphShape::synth_index(view, complement);
    GraphShape::congruence_witnesses(view, &index)
        .into_iter()
        .map(|(witness, witness_index)| {
            let mut next = complement.clone();
            next.synthesized_nodes = witness_index.nodes.clone();
            next.synthesized_edges = witness_index.edges;
            (witness, next)
        })
        .collect()
}

fn target_node_ids(from: &SynthIndex, to: &SynthIndex) -> BTreeMap<u32, u32> {
    let by_key: BTreeMap<_, _> = to.nodes.iter().map(|(&id, key)| (key, id)).collect();
    from.nodes
        .iter()
        .filter_map(|(&id, key)| by_key.get(key).copied().map(|target| (id, target)))
        .collect()
}

fn target_edge_ids(from: &SynthIndex, to: &SynthIndex) -> BTreeMap<EdgeId, EdgeId> {
    let by_key: BTreeMap<_, _> = to.edges.iter().map(|(&id, key)| (key, id)).collect();
    from.edges
        .iter()
        .filter_map(|(&id, key)| by_key.get(key).copied().map(|target| (id, target)))
        .collect()
}

fn rekey_graph_complement(
    complement: &GraphComplement,
    from: &SynthIndex,
    to: &SynthIndex,
) -> GraphComplement {
    let node_ids = target_node_ids(from, to);
    let edge_ids = target_edge_ids(from, to);
    let mut next = complement.clone();
    next.synthesized_nodes = next
        .synthesized_nodes
        .into_iter()
        .map(|(id, key)| (node_ids.get(&id).copied().unwrap_or(id), key))
        .collect();
    next.synthesized_edges = next
        .synthesized_edges
        .into_iter()
        .map(|(id, key)| (edge_ids.get(&id).copied().unwrap_or(id), key))
        .collect();
    next
}

fn rekey_tree_complement(
    complement: &Complement,
    from: &SynthIndex,
    to: &SynthIndex,
) -> Complement {
    let node_ids = target_node_ids(from, to);
    let mut next = complement.clone();
    next.synthesized_keys = next
        .synthesized_keys
        .into_iter()
        .map(|(id, key)| (node_ids.get(&id).copied().unwrap_or(id), key))
        .collect();
    next.synthesized_nodes = next
        .synthesized_nodes
        .into_iter()
        .map(|id| node_ids.get(&id).copied().unwrap_or(id))
        .collect();
    next.contraction_choices = next
        .contraction_choices
        .into_iter()
        .map(|((parent, child), edge)| {
            (
                (
                    node_ids.get(&parent).copied().unwrap_or(parent),
                    node_ids.get(&child).copied().unwrap_or(child),
                ),
                edge,
            )
        })
        .collect();
    next
}

fn tree_state_witnesses(view: &WInstance, complement: &Complement) -> Vec<(WInstance, Complement)> {
    let index = TreeShape::synth_index(view, complement);
    TreeShape::congruence_witnesses(view, &index)
        .into_iter()
        .map(|(witness, witness_index)| {
            let mut next = complement.clone();
            let old_ids: HashSet<u32> = next.synthesized_keys.keys().copied().collect();
            let renamed = index.nodes.iter().find_map(|(&old, key)| {
                witness_index
                    .nodes
                    .iter()
                    .find(|(new, candidate)| **new != old && *candidate == key)
                    .map(|(&new, _)| (old, new))
            });
            next.synthesized_keys = witness_index.nodes.clone();
            for old in old_ids {
                next.synthesized_nodes.remove(&old);
            }
            next.synthesized_nodes
                .extend(witness_index.nodes.keys().copied());
            if let Some((old, new)) = renamed {
                next.contraction_choices = next
                    .contraction_choices
                    .into_iter()
                    .map(|((parent, child), edge)| {
                        (
                            (
                                if parent == old { new } else { parent },
                                if child == old { new } else { child },
                            ),
                            edge,
                        )
                    })
                    .collect();
            }
            (witness, next)
        })
        .collect()
}

fn graph_locality(
    lens: &Lens,
    source: &GInstance,
    original_view: &GInstance,
    edited_view: &GInstance,
    restored: &GInstance,
    complement: &GraphComplement,
) -> Option<String> {
    let Ok(plan) =
        panproto_inst::RestrictPlan::compile(&lens.src_schema, &lens.tgt_schema, &lens.compiled)
    else {
        return Some("the graph locality plan could not be compiled".to_owned());
    };
    let (expected_nodes, expected_edges) =
        graph_expected_footprint(source, original_view, edited_view, complement, &plan);
    let actual_nodes: BTreeSet<u32> = source
        .nodes()
        .keys()
        .chain(restored.nodes().keys())
        .copied()
        .filter(|id| match (source.node(*id), restored.node(*id)) {
            (Some(a), Some(b)) => !crate::asymmetric::node_equiv(a, b),
            _ => true,
        })
        .collect();
    let actual_edges: BTreeSet<EdgeId> = source
        .edges()
        .keys()
        .chain(restored.edges().keys())
        .copied()
        .filter(|id| source.edge(*id) != restored.edge(*id))
        .collect();
    if actual_nodes != expected_nodes {
        return Some(format!(
            "node footprint differs: expected {expected_nodes:?}, found {actual_nodes:?}"
        ));
    }
    (actual_edges != expected_edges).then(|| {
        format!("edge footprint differs: expected {expected_edges:?}, found {actual_edges:?}")
    })
}

fn graph_expected_footprint(
    source: &GInstance,
    original_view: &GInstance,
    edited_view: &GInstance,
    complement: &GraphComplement,
    plan: &panproto_inst::RestrictPlan,
) -> (BTreeSet<u32>, BTreeSet<EdgeId>) {
    let mut expected_nodes = BTreeSet::new();
    let node_ids: BTreeSet<u32> = original_view
        .nodes()
        .keys()
        .chain(edited_view.nodes().keys())
        .copied()
        .collect();
    for id in node_ids {
        if complement.synthesized_nodes.contains_key(&id) {
            continue;
        }
        match (original_view.node(id), edited_view.node(id)) {
            (Some(a), Some(b)) if crate::asymmetric::node_equiv(a, b) => {}
            _ => {
                expected_nodes.insert(id);
            }
        }
    }
    let removed: BTreeSet<u32> = original_view
        .nodes()
        .keys()
        .filter(|id| edited_view.node(**id).is_none())
        .copied()
        .collect();
    let mut expected_edges = BTreeSet::new();
    let edge_ids: BTreeSet<EdgeId> = original_view
        .edges()
        .keys()
        .chain(edited_view.edges().keys())
        .copied()
        .collect();
    for id in edge_ids {
        if complement.synthesized_edges.contains_key(&id) && source.edge(id).is_none() {
            continue;
        }
        if original_view.edge(id) != edited_view.edge(id) {
            let target_edges = [original_view.edge(id), edited_view.edge(id)];
            if target_edges
                .into_iter()
                .flatten()
                .all(|edge| plan.edge_image.preimages(&edge.edge).is_empty())
                && source.edge(id).is_none()
            {
                continue;
            }
            expected_edges.insert(id);
        }
    }
    expected_edges.extend(
        complement
            .dropped_edges
            .iter()
            .filter(|(_, edge)| removed.contains(&edge.src) || removed.contains(&edge.tgt))
            .map(|(&id, _)| id),
    );
    expected_edges.extend(source.edges().iter().filter_map(|(&id, edge)| {
        (removed.contains(&edge.src) || removed.contains(&edge.tgt)).then_some(id)
    }));
    // A reified fan edge can be marked synthesized in the view index while
    // still naming a real source carrier. Such an edge remains in the
    // footprint above. Only deleting the fan or its declared parent closes
    // over the whole fan.
    let mut removed_fans: BTreeSet<u32> = removed
        .iter()
        .copied()
        .filter(|id| {
            source
                .node(*id)
                .or_else(|| original_view.node(*id))
                .is_some_and(|node| matches!(node.shape, panproto_inst::NodeShape::HyperEdge))
        })
        .collect();
    removed_fans.extend(source.edges().values().filter_map(|edge| {
        (edge.edge.kind.as_ref() == panproto_inst::HYPER_PARENT_KIND && removed.contains(&edge.tgt))
            .then_some(edge.src)
    }));
    expected_edges.extend(
        source
            .edges()
            .iter()
            .filter(|(_, edge)| {
                removed_fans.contains(&edge.src) || removed_fans.contains(&edge.tgt)
            })
            .map(|(&id, _)| id),
    );

    (expected_nodes, expected_edges)
}

fn tree_locality(
    lens: &Lens,
    source: &WInstance,
    original_view: &WInstance,
    edited_view: &WInstance,
    restored: &WInstance,
    complement: &Complement,
) -> Option<String> {
    let Ok(plan) =
        panproto_inst::RestrictPlan::compile(&lens.src_schema, &lens.tgt_schema, &lens.compiled)
    else {
        return Some("the tree locality plan could not be compiled".to_owned());
    };
    let (expected_nodes, expected_arcs) =
        tree_expected_footprint(original_view, edited_view, complement, &plan);
    let (actual_nodes, actual_arcs) = tree_actual_footprint(source, restored);
    if actual_nodes != expected_nodes {
        return Some(format!(
            "node footprint differs: expected {expected_nodes:?}, found {actual_nodes:?}"
        ));
    }
    (actual_arcs != expected_arcs).then(|| {
        format!("arc footprint differs: expected {expected_arcs:?}, found {actual_arcs:?}")
    })
}

fn tree_expected_footprint(
    original_view: &WInstance,
    edited_view: &WInstance,
    complement: &Complement,
    plan: &panproto_inst::RestrictPlan,
) -> (BTreeSet<u32>, BTreeSet<(u32, u32)>) {
    let synth: BTreeSet<u32> = complement.synthesized_keys.keys().copied().collect();
    let mut expected_nodes = BTreeSet::new();
    let view_ids: BTreeSet<u32> = original_view
        .nodes
        .keys()
        .chain(edited_view.nodes.keys())
        .copied()
        .collect();
    for id in view_ids {
        if synth.contains(&id) {
            continue;
        }
        match (original_view.nodes.get(&id), edited_view.nodes.get(&id)) {
            (Some(a), Some(b)) if crate::asymmetric::node_equiv(a, b) => {}
            _ => {
                expected_nodes.insert(id);
            }
        }
    }
    let removed: BTreeSet<u32> = original_view
        .nodes
        .keys()
        .filter(|id| !edited_view.nodes.contains_key(id))
        .copied()
        .collect();
    let view_arcs: BTreeMap<(u32, u32), (usize, &Edge)> = original_view
        .arcs
        .iter()
        .enumerate()
        .map(|(rank, (p, c, edge))| ((*p, *c), (rank, edge)))
        .collect();
    let edited_arcs: BTreeMap<(u32, u32), (usize, &Edge)> = edited_view
        .arcs
        .iter()
        .enumerate()
        .map(|(rank, (p, c, edge))| ((*p, *c), (rank, edge)))
        .collect();
    let mut expected_arcs: BTreeSet<(u32, u32)> = view_arcs
        .keys()
        .chain(edited_arcs.keys())
        .copied()
        .filter(|key| {
            let represented = view_arcs
                .get(key)
                .into_iter()
                .chain(edited_arcs.get(key))
                .any(|(_, edge)| !plan.edge_image.preimages(edge).is_empty());
            !synth.contains(&key.0)
                && !synth.contains(&key.1)
                && represented
                && view_arcs.get(key) != edited_arcs.get(key)
        })
        .collect();
    expected_arcs.extend(
        complement
            .dropped_arcs
            .iter()
            .filter(|(p, c, _)| removed.contains(p) || removed.contains(c))
            .map(|(p, c, _)| (*p, *c)),
    );
    (expected_nodes, expected_arcs)
}

fn tree_actual_footprint(
    source: &WInstance,
    restored: &WInstance,
) -> (BTreeSet<u32>, BTreeSet<(u32, u32)>) {
    let actual_nodes: BTreeSet<u32> = source
        .nodes
        .keys()
        .chain(restored.nodes.keys())
        .copied()
        .filter(|id| match (source.nodes.get(id), restored.nodes.get(id)) {
            (Some(a), Some(b)) => !crate::asymmetric::node_equiv(a, b),
            _ => true,
        })
        .collect();
    let source_arcs: BTreeMap<(u32, u32), (usize, &Edge)> = source
        .arcs
        .iter()
        .enumerate()
        .map(|(rank, (p, c, edge))| ((*p, *c), (rank, edge)))
        .collect();
    let restored_arcs: BTreeMap<(u32, u32), (usize, &Edge)> = restored
        .arcs
        .iter()
        .enumerate()
        .map(|(rank, (p, c, edge))| ((*p, *c), (rank, edge)))
        .collect();
    let actual_arcs: BTreeSet<(u32, u32)> = source_arcs
        .keys()
        .chain(restored_arcs.keys())
        .copied()
        .filter(|key| source_arcs.get(key) != restored_arcs.get(key))
        .collect();
    (actual_nodes, actual_arcs)
}

fn row_equiv(a: &HashMap<String, Value>, b: &HashMap<String, Value>) -> bool {
    a.len() == b.len()
        && a.iter().all(|(key, value)| {
            b.get(key)
                .is_some_and(|other| value.canonical_cmp(other).is_eq())
        })
}

fn table_identical(a: &FInstance, b: &FInstance) -> bool {
    let a_ids = a.row_ids();
    let b_ids = b.row_ids();
    a_ids == b_ids
        && a_ids.iter().all(|id| {
            a.table_of(*id) == b.table_of(*id)
                && a.row(*id)
                    .zip(b.row(*id))
                    .is_some_and(|(left, right)| row_equiv(left, right))
        })
        && a.foreign_keys() == b.foreign_keys()
}

fn table_locality(
    lens: &Lens,
    source: &FInstance,
    original_view: &FInstance,
    edited_view: &FInstance,
    restored: &FInstance,
    complement: &TableComplement,
    ctx: &crate::table_shape::TableContext,
) -> Option<String> {
    let protocols = panproto_inst::BoundaryProtocols {
        src: &ctx.src,
        tgt: &ctx.tgt,
    };
    let Ok(plan) = panproto_inst::RestrictPlan::compile_for_tables(
        &lens.src_schema,
        &lens.tgt_schema,
        &lens.compiled,
        protocols,
    ) else {
        return Some("the table locality plan could not be compiled".to_owned());
    };
    let Some(boundary) = plan.boundary.as_ref() else {
        return Some("the table locality plan has no boundary layer".to_owned());
    };
    let (Some(functor), Some(cells)) = (boundary.table.as_ref(), boundary.cells.as_ref()) else {
        return Some("the table locality plan has no entity functor or cell plan".to_owned());
    };
    let expected_rows = table_structural_rows(original_view, edited_view);
    let expected_cells = table_expected_cells(lens, cells, source, original_view, edited_view);
    let expected_pairs = table_expected_pairs(functor, original_view, edited_view, complement);
    let actual_rows = table_structural_rows(source, restored);
    let actual_cells = table_cell_changes(source, restored);
    let source_pairs = table_pairs(source);
    let actual_pairs: BTreeSet<_> = source_pairs
        .symmetric_difference(&table_pairs(restored))
        .cloned()
        .collect();
    if actual_rows != expected_rows {
        return Some(format!(
            "row footprint differs: expected {expected_rows:?}, found {actual_rows:?}"
        ));
    }
    if actual_cells != expected_cells {
        return Some(format!(
            "cell footprint differs: expected {expected_cells:?}, found {actual_cells:?}"
        ));
    }
    (actual_pairs != expected_pairs).then(|| {
        format!("pair footprint differs: expected {expected_pairs:?}, found {actual_pairs:?}")
    })
}

fn table_structural_rows(before: &FInstance, after: &FInstance) -> BTreeSet<RowId> {
    before
        .row_ids()
        .into_iter()
        .chain(after.row_ids())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .filter(|id| match (before.row(*id), after.row(*id)) {
            (Some(_), Some(_)) => before.table_of(*id) != after.table_of(*id),
            _ => true,
        })
        .collect()
}

fn table_pairs(instance: &FInstance) -> BTreeSet<(Edge, RowId, RowId)> {
    instance
        .foreign_keys()
        .into_iter()
        .flat_map(|(edge, pairs)| {
            pairs
                .iter()
                .map(move |&(src, tgt)| (edge.clone(), src, tgt))
        })
        .collect()
}

fn table_expected_cells(
    lens: &Lens,
    cells: &CellPlan,
    source: &FInstance,
    original_view: &FInstance,
    edited_view: &FInstance,
) -> BTreeSet<(RowId, String)> {
    let mut expected = BTreeSet::new();
    for (row, target_column) in table_cell_changes(original_view, edited_view) {
        let Some(source_table) = source.table_of(row) else {
            continue;
        };
        let fates = cells.fates.get(source_table).map_or(&[][..], Vec::as_slice);
        if let Some(source_column) = reverse_table_cell_writer(
            &lens.compiled.value_transforms(source_table),
            fates,
            &target_column,
        ) {
            expected.insert((row, source_column));
        }
    }
    expected
}

fn reverse_table_cell_writer(
    transforms: &[FieldTransform],
    fates: &[CellFate],
    target: &str,
) -> Option<String> {
    let mut key = reverse_cell_fates(fates, target)?;
    for transform in transforms.iter().rev() {
        key = reverse_transform_writer(transform, key)?;
    }
    Some(key)
}

fn reverse_cell_fates(fates: &[CellFate], target: &str) -> Option<String> {
    if let Some(source) = fates.iter().rev().find_map(|fate| match fate {
        CellFate::Rename(source, image) if image == target => Some(source.clone()),
        CellFate::Drop(_) | CellFate::Rename(_, _) => None,
    }) {
        return Some(source);
    }
    let displaced = fates.iter().any(|fate| match fate {
        CellFate::Drop(source) | CellFate::Rename(source, _) => source == target,
    });
    (!displaced).then(|| target.to_owned())
}

fn reverse_transform_writer(transform: &FieldTransform, key: String) -> Option<String> {
    match transform {
        FieldTransform::RenameField { old_key, new_key } if key == *new_key => {
            Some(old_key.clone())
        }
        FieldTransform::RenameField { old_key, new_key }
            if key == *old_key && old_key != new_key =>
        {
            None
        }
        FieldTransform::DropField { key: dropped } if key == *dropped => None,
        FieldTransform::AddField { key: added, .. } if key == *added => None,
        FieldTransform::KeepFields { keys } if !keys.contains(&key) => None,
        FieldTransform::ApplyExpr {
            key: output,
            inverse,
            ..
        } if key == *output => inverse.as_ref().map(|_| key),
        FieldTransform::ComputeField {
            target_key,
            expr,
            inverse,
            ..
        } if key == *target_key => inverse
            .as_ref()
            .and_then(|_| crate::derived::sole_source_var(expr)),
        FieldTransform::PathTransform { path, inner } if path.is_empty() => {
            reverse_transform_writer(inner, key)
        }
        _ => Some(key),
    }
}

fn table_expected_pairs(
    functor: &panproto_inst::EntityFunctor,
    original_view: &FInstance,
    edited_view: &FInstance,
    complement: &TableComplement,
) -> BTreeSet<(Edge, RowId, RowId)> {
    let view_pairs = table_pairs(original_view);
    let edited_pairs = table_pairs(edited_view);
    let mut expected = BTreeSet::new();
    for (edge, src, tgt) in view_pairs.symmetric_difference(&edited_pairs) {
        if let Some(originals) = complement
            .original_fk_edges
            .get(&(edge.clone(), *src, *tgt))
        {
            expected.extend(
                originals
                    .iter()
                    .cloned()
                    .map(|source_edge| (source_edge, *src, *tgt)),
            );
        } else {
            expected.extend(
                functor
                    .relation_map()
                    .iter()
                    .filter(|(_, image)| *image == edge)
                    .map(|(source_edge, _)| (source_edge.clone(), *src, *tgt)),
            );
        }
    }
    let removed: BTreeSet<RowId> = original_view
        .row_ids()
        .into_iter()
        .filter(|id| edited_view.row(*id).is_none())
        .collect();
    for (edge, pairs) in &complement.dropped_fk_pairs {
        expected.extend(
            pairs
                .iter()
                .filter(|(src, tgt)| removed.contains(src) || removed.contains(tgt))
                .map(|&(src, tgt)| (edge.clone(), src, tgt)),
        );
    }
    expected
}

fn table_cell_changes(before: &FInstance, after: &FInstance) -> BTreeSet<(RowId, String)> {
    before
        .row_ids()
        .into_iter()
        .chain(after.row_ids())
        .collect::<BTreeSet<_>>()
        .into_iter()
        // Inserted/deleted rows are structural carriers. A row present on
        // both sides retains its cell coordinates even when its table changes.
        .filter(|row| before.row(*row).is_some() && after.row(*row).is_some())
        .flat_map(|row| {
            let before_row = before.row(row);
            let after_row = after.row(row);
            before_row
                .into_iter()
                .flat_map(HashMap::keys)
                .chain(after_row.into_iter().flat_map(HashMap::keys))
                .cloned()
                .collect::<BTreeSet<_>>()
                .into_iter()
                .filter(move |column| {
                    match (
                        before_row.and_then(|r| r.get(column)),
                        after_row.and_then(|r| r.get(column)),
                    ) {
                        (Some(a), Some(b)) => !a.canonical_cmp(b).is_eq(),
                        (None, None) => false,
                        _ => true,
                    }
                })
                .map(move |column| (row, column))
        })
        .collect()
}

fn graph_intersection(a: &GInstance, b: &GInstance) -> GInstance {
    let mut result = a.clone();
    let edge_ids: Vec<_> = result
        .edges()
        .keys()
        .copied()
        .filter(|id| b.edge(*id).is_none())
        .collect();
    for id in edge_ids {
        let _ = result.remove_edge(id);
    }
    let node_ids: Vec<_> = result
        .nodes()
        .keys()
        .copied()
        .filter(|id| b.node(*id).is_none())
        .collect();
    for id in node_ids {
        let incident: Vec<_> = result
            .out_edges(id)
            .iter()
            .chain(result.in_edges(id))
            .copied()
            .collect();
        for edge in incident {
            let _ = result.remove_edge(edge);
        }
        let _ = result.remove_node(id);
    }
    result
}

fn tree_intersection(a: &WInstance, b: &WInstance) -> WInstance {
    let nodes: HashMap<u32, Node> = a
        .nodes
        .iter()
        .filter(|(id, _)| b.nodes.contains_key(id))
        .map(|(&id, node)| (id, node.clone()))
        .collect();
    if !nodes.contains_key(&a.root) {
        return a.clone();
    }
    let arcs = a
        .arcs
        .iter()
        .filter(|(p, c, _)| {
            nodes.contains_key(p)
                && nodes.contains_key(c)
                && b.arcs.iter().any(|(bp, bc, _)| bp == p && bc == c)
        })
        .cloned()
        .collect();
    rebuild_tree(a, nodes, arcs)
}

fn table_intersection(a: &FInstance, b: &FInstance) -> FInstance {
    let mut result = a.clone();
    for id in result.row_ids() {
        if b.row(id).is_none() {
            let _ = result.remove_row(id);
        }
    }
    let pairs: Vec<_> = result
        .foreign_keys()
        .into_iter()
        .flat_map(|(edge, pairs)| {
            pairs
                .iter()
                .map(move |&(src, tgt)| (edge.clone(), src, tgt))
        })
        .collect();
    for (edge, src, tgt) in pairs {
        if !b.contains_pair(&edge, src, tgt) {
            result.remove_foreign_key(&edge, src, tgt);
        }
    }
    result
}

fn assignments<T: Clone>(items: &[T], length: usize) -> Vec<Vec<T>> {
    if length == 0 {
        return vec![Vec::new()];
    }
    let tails = assignments(items, length - 1);
    items
        .iter()
        .flat_map(|item| {
            tails.iter().map(move |tail| {
                let mut result = Vec::with_capacity(length);
                result.push(item.clone());
                result.extend_from_slice(tail);
                result
            })
        })
        .collect()
}

fn finite_values_for_kind(kind: &str) -> Vec<Value> {
    let filter_kind: panproto_gat::Name = if kind == "date-time" {
        "datetime"
    } else {
        kind
    }
    .into();
    let alphabet = [
        Value::Null,
        Value::Bool(false),
        Value::Bool(true),
        Value::Int(0),
        Value::Int(1),
        Value::Str(String::new()),
    ];
    alphabet
        .into_iter()
        .filter(|value| panproto_inst::attributes::kind_accepts(&filter_kind, value))
        .collect()
}

fn finite_values_for_scalar(kind: &panproto_schema::ScalarKind) -> Vec<Value> {
    use panproto_schema::ScalarKind;

    match kind {
        ScalarKind::Bool => vec![Value::Null, Value::Bool(false), Value::Bool(true)],
        ScalarKind::Int { .. } | ScalarKind::BigInt => {
            vec![Value::Null, Value::Int(0), Value::Int(1)]
        }
        ScalarKind::Str => vec![Value::Null, Value::Str(String::new())],
        ScalarKind::Any => vec![
            Value::Null,
            Value::Bool(false),
            Value::Bool(true),
            Value::Int(0),
            Value::Int(1),
            Value::Str(String::new()),
        ],
        ScalarKind::Float { .. }
        | ScalarKind::Decimal
        | ScalarKind::Bytes(_)
        | ScalarKind::Date
        | ScalarKind::Time
        | ScalarKind::DateTime
        | ScalarKind::Token => vec![Value::Null],
    }
}

fn node_payload_variants(schema: &Schema, id: u32, anchor: &panproto_gat::Name) -> Vec<Node> {
    let base = Node::new(id, anchor.clone());
    let Some(vertex) = schema.vertices.get(anchor) else {
        return vec![base];
    };
    let values = finite_values_for_kind(vertex.kind.as_ref());
    values
        .into_iter()
        .map(|value| match value {
            Value::Null => base.clone().with_value(FieldPresence::Null),
            value => base.clone().with_value(FieldPresence::Present(value)),
        })
        .collect()
}

fn node_products(options: &[Vec<Node>]) -> Vec<Vec<Node>> {
    if options.is_empty() {
        return vec![Vec::new()];
    }
    let tails = node_products(&options[1..]);
    options[0]
        .iter()
        .flat_map(|node| {
            tails.iter().map(move |tail| {
                let mut product = Vec::with_capacity(options.len());
                product.push(node.clone());
                product.extend_from_slice(tail);
                product
            })
        })
        .collect()
}

fn carrier_products(options: &[Vec<u32>]) -> Vec<Vec<u32>> {
    if options.is_empty() {
        return vec![Vec::new()];
    }
    let tails = carrier_products(&options[1..]);
    options[0]
        .iter()
        .flat_map(|id| {
            tails.iter().map(move |tail| {
                let mut product = Vec::with_capacity(options.len());
                product.push(*id);
                product.extend_from_slice(tail);
                product
            })
        })
        .collect()
}

fn append_small_fan(
    schema: &Schema,
    scope: SmallScope,
    graph: &GInstance,
    hyper: &panproto_schema::HyperEdge,
    signature: &[(&panproto_gat::Name, &panproto_gat::Name)],
    out: &mut Vec<GInstance>,
) {
    let choices: Vec<Vec<u32>> = signature
        .iter()
        .map(|(_, anchor)| {
            let mut ids: Vec<_> = graph
                .nodes()
                .iter()
                .filter(|(_, node)| {
                    node.anchor == **anchor
                        && !matches!(node.shape, panproto_inst::NodeShape::HyperEdge)
                })
                .map(|(&id, _)| id)
                .collect();
            ids.sort_unstable();
            ids
        })
        .collect();
    if choices.iter().any(Vec::is_empty) {
        return;
    }
    for targets in carrier_products(&choices) {
        let Some(fan_id) = fresh_u32(graph.nodes().keys().copied()) else {
            continue;
        };
        let mut next = graph.clone();
        if next
            .insert_node(
                Node::new(fan_id, hyper.id.clone()).with_shape(panproto_inst::NodeShape::HyperEdge),
            )
            .is_err()
        {
            continue;
        }
        let mut edge_id = u32::try_from(next.edge_count()).unwrap_or(u32::MAX);
        let mut valid = true;
        for (index, ((label, _), target)) in signature.iter().zip(&targets).enumerate() {
            let target_anchor = next
                .node(*target)
                .map(|node| node.anchor.clone())
                .unwrap_or_default();
            let edge = Edge {
                src: hyper.id.clone(),
                tgt: target_anchor,
                kind: if index == 0 {
                    panproto_inst::HYPER_PARENT_KIND.into()
                } else {
                    panproto_inst::HYPER_LABEL_KIND.into()
                },
                name: Some((*label).clone()),
            };
            valid &= next
                .insert_edge(EdgeId(edge_id), GEdge::new(fan_id, *target, edge))
                .is_ok();
            edge_id = edge_id.saturating_add(1);
        }
        if valid {
            out.push(next.clone());
            add_small_fans(schema, scope, &next, out);
        }
    }
}

fn add_small_fans(schema: &Schema, scope: SmallScope, graph: &GInstance, out: &mut Vec<GInstance>) {
    if graph.node_count() >= usize::from(scope.max_nodes) {
        return;
    }
    let mut hyper_edges: Vec<_> = schema.hyper_edges.iter().collect();
    hyper_edges.sort_by_key(|(name, _)| (*name).clone());
    for (_, hyper) in hyper_edges {
        let Some(parent) = hyper.signature.get_key_value(&hyper.parent_label) else {
            continue;
        };
        let mut children: Vec<_> = hyper
            .signature
            .iter()
            .filter(|(label, _)| *label != &hyper.parent_label)
            .collect();
        children.sort_by_key(|(label, _)| (*label).clone());
        for count in 0..=children.len() {
            for subset in combinations(&children, count) {
                let mut signature = vec![parent];
                signature.extend(subset);
                if graph.edge_count() + signature.len() <= usize::from(scope.max_edges) {
                    append_small_fan(schema, scope, graph, hyper, &signature, out);
                }
            }
        }
    }
}

fn graph_small(schema: &Schema, scope: SmallScope) -> Vec<GInstance> {
    let mut anchors: Vec<_> = schema.vertices.keys().cloned().collect();
    anchors.sort_unstable();
    let mut labels: Vec<_> = schema.edges.keys().cloned().collect();
    labels.sort_unstable();
    let mut result = vec![GInstance::new()];
    for count in 1..=usize::from(scope.max_nodes) {
        for node_anchors in assignments(&anchors, count) {
            let payload_options: Vec<_> = node_anchors
                .iter()
                .enumerate()
                .map(|(id, anchor)| {
                    node_payload_variants(schema, u32::try_from(id).unwrap_or(u32::MAX), anchor)
                })
                .collect();
            let mut choices: Vec<(u32, u32, Edge)> = Vec::new();
            for edge in &labels {
                for (src, src_anchor) in node_anchors.iter().enumerate() {
                    for (tgt, tgt_anchor) in node_anchors.iter().enumerate() {
                        if edge.src == **src_anchor && edge.tgt == **tgt_anchor {
                            choices.push((
                                u32::try_from(src).unwrap_or(u32::MAX),
                                u32::try_from(tgt).unwrap_or(u32::MAX),
                                edge.clone(),
                            ));
                        }
                    }
                }
            }
            for nodes in node_products(&payload_options) {
                let base = nodes
                    .into_iter()
                    .fold(GInstance::new(), GInstance::with_node);
                result.push(base.clone());
                for edge_count in 1..=usize::from(scope.max_edges) {
                    for edges in assignments(&choices, edge_count) {
                        let mut graph = base.clone();
                        let mut valid = true;
                        for (id, (src, tgt, edge)) in edges.into_iter().enumerate() {
                            valid &= graph
                                .insert_edge(
                                    EdgeId(u32::try_from(id).unwrap_or(u32::MAX)),
                                    GEdge::new(src, tgt, edge),
                                )
                                .is_ok();
                        }
                        if valid {
                            result.push(graph);
                        }
                    }
                }
            }
        }
    }
    let ordinary = result.clone();
    for graph in ordinary {
        add_small_fans(schema, scope, &graph, &mut result);
    }
    result
}

fn tree_small(schema: &Schema, scope: SmallScope) -> Vec<WInstance> {
    graph_small(schema, scope)
        .into_iter()
        .filter_map(|graph| {
            if graph
                .nodes()
                .values()
                .any(|node| matches!(node.shape, panproto_inst::NodeShape::HyperEdge))
            {
                return None;
            }
            let n = graph.node_count();
            if n == 0 || graph.edge_count() != n.saturating_sub(1) || graph.node(0).is_none() {
                return None;
            }
            let mut parents = HashMap::new();
            let mut arcs = Vec::new();
            for (_, edge) in graph.edges_sorted() {
                if edge.tgt == 0 || parents.insert(edge.tgt, edge.src).is_some() {
                    return None;
                }
                arcs.push((edge.src, edge.tgt, edge.edge.clone()));
            }
            if (1..u32::try_from(n).ok()?).any(|id| !parents.contains_key(&id)) {
                return None;
            }
            let mut reached = BTreeSet::from([0]);
            let mut frontier = vec![0];
            while let Some(parent) = frontier.pop() {
                for edge_id in graph.out_edges(parent) {
                    if let Some(edge) = graph.edge(*edge_id)
                        && reached.insert(edge.tgt)
                    {
                        frontier.push(edge.tgt);
                    }
                }
            }
            if reached.len() != n {
                return None;
            }
            let nodes = graph.nodes().clone();
            let schema_root = nodes.get(&0)?.anchor.clone();
            Some(WInstance::new(nodes, arcs, Vec::new(), 0, schema_root))
        })
        .collect()
}

fn combinations<T: Clone>(items: &[T], count: usize) -> Vec<Vec<T>> {
    fn visit<T: Clone>(
        items: &[T],
        count: usize,
        at: usize,
        current: &mut Vec<T>,
        out: &mut Vec<Vec<T>>,
    ) {
        if current.len() == count {
            out.push(current.clone());
            return;
        }
        for index in at..items.len() {
            current.push(items[index].clone());
            visit(items, count, index + 1, current, out);
            current.pop();
        }
    }
    let mut out = Vec::new();
    visit(items, count, 0, &mut Vec::new(), &mut out);
    out
}

fn table_row_variants(
    schema: &Schema,
    protocol: &panproto_schema::Protocol,
    table: &panproto_gat::Name,
) -> Vec<HashMap<String, Value>> {
    let mut columns = BTreeMap::<String, Vec<Value>>::new();
    let mut edges: Vec<_> = schema
        .edges
        .keys()
        .filter(|edge| edge.src == *table)
        .collect();
    edges.sort_unstable();
    for edge in edges {
        let Some(column) = edge.name.as_ref() else {
            continue;
        };
        let Some(vertex) = schema.vertices.get(&edge.tgt) else {
            continue;
        };
        let Some(kind) = protocol.value_kinds.get(vertex.kind.as_ref()) else {
            continue;
        };
        let values = finite_values_for_scalar(kind);
        columns.entry(column.to_string()).or_insert(values);
    }
    columns
        .into_iter()
        .fold(vec![HashMap::new()], |rows, (column, values)| {
            rows.into_iter()
                .flat_map(|row| {
                    std::iter::once(row.clone()).chain(values.iter().map({
                        let column = column.clone();
                        move |value| {
                            let mut next = row.clone();
                            next.insert(column.clone(), value.clone());
                            next
                        }
                    }))
                })
                .collect()
        })
}

fn row_products(options: &[Vec<HashMap<String, Value>>]) -> Vec<Vec<HashMap<String, Value>>> {
    if options.is_empty() {
        return vec![Vec::new()];
    }
    let tails = row_products(&options[1..]);
    options[0]
        .iter()
        .flat_map(|row| {
            tails.iter().map(move |tail| {
                let mut product = Vec::with_capacity(options.len());
                product.push(row.clone());
                product.extend_from_slice(tail);
                product
            })
        })
        .collect()
}

fn table_small(
    schema: &Schema,
    protocol: &panproto_schema::Protocol,
    scope: SmallScope,
) -> Vec<FInstance> {
    let mut tables: Vec<_> = schema
        .vertices
        .iter()
        .filter(|(_, vertex)| protocol.non_value_kinds.contains(vertex.kind.as_ref()))
        .map(|(name, _)| name.clone())
        .collect();
    tables.sort_unstable();
    let mut labels: Vec<_> = schema.edges.keys().cloned().collect();
    labels.sort_unstable();
    let mut result = vec![FInstance::new()];
    for count in 1..=usize::from(scope.max_nodes) {
        for row_tables in assignments(&tables, count) {
            let row_options: Vec<_> = row_tables
                .iter()
                .map(|table| table_row_variants(schema, protocol, table))
                .collect();
            let mut possible = Vec::new();
            for edge in &labels {
                for (src, src_table) in row_tables.iter().enumerate() {
                    for (tgt, tgt_table) in row_tables.iter().enumerate() {
                        if edge.src == **src_table && edge.tgt == **tgt_table {
                            possible.push((
                                edge.clone(),
                                RowId(u32::try_from(src).unwrap_or(u32::MAX)),
                                RowId(u32::try_from(tgt).unwrap_or(u32::MAX)),
                            ));
                        }
                    }
                }
            }
            for rows in row_products(&row_options) {
                let mut base = FInstance::new();
                for (id, (table, row)) in row_tables.iter().zip(rows).enumerate() {
                    let _ = base.insert_row(
                        table.clone(),
                        RowId(u32::try_from(id).unwrap_or(u32::MAX)),
                        row,
                    );
                }
                result.push(base.clone());
                for pair_count in 1..=usize::from(scope.max_edges).min(possible.len()) {
                    for pairs in combinations(&possible, pair_count) {
                        let mut instance = base.clone();
                        let mut valid = true;
                        for (edge, src, tgt) in pairs {
                            valid &= instance.insert_foreign_key(edge, src, tgt).is_ok();
                        }
                        if valid {
                            result.push(instance);
                        }
                    }
                }
            }
        }
    }
    result
}

impl LawShape for TreeShape {
    fn mutation_cases(
        lens: &Lens,
        _source: &Self::Instance,
        view: &Self::Instance,
        complement: &Self::Complement,
        _ctx: &Self::Context,
        mutation: Mutation,
    ) -> MutationCases<Self::Instance> {
        tree_cases(lens, view, complement, mutation)
    }

    fn negative_mutation_cases(
        lens: &Lens,
        _source: &Self::Instance,
        view: &Self::Instance,
        complement: &Self::Complement,
        _ctx: &Self::Context,
        mutation: Mutation,
    ) -> Vec<NegativeMutationCase<Self::Instance>> {
        tree_negative_cases(lens, view, complement, mutation)
    }

    fn exhaustive_value_cases(
        lens: &Lens,
        _source: &Self::Instance,
        view: &Self::Instance,
        complement: &Self::Complement,
        _ctx: &Self::Context,
    ) -> Vec<Self::Instance> {
        tree_exhaustive_value_cases(lens, view, complement)
    }

    fn identical(a: &Self::Instance, b: &Self::Instance) -> bool {
        tree_identical(a, b)
    }

    fn congruent_states(
        view: &Self::Instance,
        complement: &Self::Complement,
    ) -> Vec<(Self::Instance, Self::Complement)> {
        tree_state_witnesses(view, complement)
    }

    fn rekey_complement(
        complement: &Self::Complement,
        from: &SynthIndex,
        to: &SynthIndex,
    ) -> Self::Complement {
        rekey_tree_complement(complement, from, to)
    }

    fn locality_divergence(
        lens: &Lens,
        source: &Self::Instance,
        original_view: &Self::Instance,
        edited_view: &Self::Instance,
        restored: &Self::Instance,
        complement: &Self::Complement,
        _ctx: &Self::Context,
    ) -> Option<String> {
        tree_locality(
            lens,
            source,
            original_view,
            edited_view,
            restored,
            complement,
        )
    }

    fn carrier_intersection(a: &Self::Instance, b: &Self::Instance) -> Self::Instance {
        tree_intersection(a, b)
    }

    fn undo_view(original: &Self::Instance, edited: &Self::Instance) -> Option<Self::Instance> {
        (!tree_identical(original, edited)).then(|| original.clone())
    }

    fn enumerate_small(
        schema: &Schema,
        scope: SmallScope,
        _ctx: &Self::Context,
    ) -> Vec<Self::Instance> {
        tree_small(schema, scope)
    }
}

impl LawShape for GraphShape {
    fn mutation_cases(
        lens: &Lens,
        _source: &Self::Instance,
        view: &Self::Instance,
        complement: &Self::Complement,
        _ctx: &Self::Context,
        mutation: Mutation,
    ) -> MutationCases<Self::Instance> {
        graph_cases(lens, view, complement, mutation)
    }

    fn negative_mutation_cases(
        lens: &Lens,
        _source: &Self::Instance,
        view: &Self::Instance,
        complement: &Self::Complement,
        _ctx: &Self::Context,
        mutation: Mutation,
    ) -> Vec<NegativeMutationCase<Self::Instance>> {
        graph_negative_cases(lens, view, complement, mutation)
    }

    fn exhaustive_value_cases(
        lens: &Lens,
        _source: &Self::Instance,
        view: &Self::Instance,
        complement: &Self::Complement,
        _ctx: &Self::Context,
    ) -> Vec<Self::Instance> {
        graph_exhaustive_value_cases(lens, view, complement)
    }

    fn identical(a: &Self::Instance, b: &Self::Instance) -> bool {
        GInstance::identical(a, b)
    }

    fn congruent_states(
        view: &Self::Instance,
        complement: &Self::Complement,
    ) -> Vec<(Self::Instance, Self::Complement)> {
        graph_state_witnesses(view, complement)
    }

    fn rekey_complement(
        complement: &Self::Complement,
        from: &SynthIndex,
        to: &SynthIndex,
    ) -> Self::Complement {
        rekey_graph_complement(complement, from, to)
    }

    fn locality_divergence(
        lens: &Lens,
        source: &Self::Instance,
        original_view: &Self::Instance,
        edited_view: &Self::Instance,
        restored: &Self::Instance,
        complement: &Self::Complement,
        _ctx: &Self::Context,
    ) -> Option<String> {
        graph_locality(
            lens,
            source,
            original_view,
            edited_view,
            restored,
            complement,
        )
    }

    fn carrier_intersection(a: &Self::Instance, b: &Self::Instance) -> Self::Instance {
        graph_intersection(a, b)
    }

    fn undo_view(original: &Self::Instance, edited: &Self::Instance) -> Option<Self::Instance> {
        (!GInstance::identical(original, edited)).then(|| original.clone())
    }

    fn enumerate_small(
        schema: &Schema,
        scope: SmallScope,
        _ctx: &Self::Context,
    ) -> Vec<Self::Instance> {
        graph_small(schema, scope)
    }
}

impl LawShape for TableShape {
    fn mutation_cases(
        lens: &Lens,
        _source: &Self::Instance,
        view: &Self::Instance,
        complement: &Self::Complement,
        ctx: &Self::Context,
        mutation: Mutation,
    ) -> MutationCases<Self::Instance> {
        table_cases(lens, view, complement, ctx, mutation)
    }

    fn negative_mutation_cases(
        lens: &Lens,
        _source: &Self::Instance,
        view: &Self::Instance,
        complement: &Self::Complement,
        ctx: &Self::Context,
        mutation: Mutation,
    ) -> Vec<NegativeMutationCase<Self::Instance>> {
        table_negative_cases(lens, view, complement, ctx, mutation)
    }

    fn exhaustive_value_cases(
        lens: &Lens,
        _source: &Self::Instance,
        view: &Self::Instance,
        _complement: &Self::Complement,
        ctx: &Self::Context,
    ) -> Vec<Self::Instance> {
        table_exhaustive_value_cases(lens, view, ctx)
    }

    fn identical(a: &Self::Instance, b: &Self::Instance) -> bool {
        table_identical(a, b)
    }

    fn congruent_states(
        _view: &Self::Instance,
        _complement: &Self::Complement,
    ) -> Vec<(Self::Instance, Self::Complement)> {
        Vec::new()
    }

    fn rekey_complement(
        complement: &Self::Complement,
        _from: &SynthIndex,
        _to: &SynthIndex,
    ) -> Self::Complement {
        complement.clone()
    }

    fn locality_divergence(
        lens: &Lens,
        source: &Self::Instance,
        original_view: &Self::Instance,
        edited_view: &Self::Instance,
        restored: &Self::Instance,
        complement: &Self::Complement,
        ctx: &Self::Context,
    ) -> Option<String> {
        table_locality(
            lens,
            source,
            original_view,
            edited_view,
            restored,
            complement,
            ctx,
        )
    }

    fn carrier_intersection(a: &Self::Instance, b: &Self::Instance) -> Self::Instance {
        table_intersection(a, b)
    }

    fn undo_view(original: &Self::Instance, edited: &Self::Instance) -> Option<Self::Instance> {
        (!table_identical(original, edited)).then(|| original.clone())
    }

    fn enumerate_small(
        schema: &Schema,
        scope: SmallScope,
        ctx: &Self::Context,
    ) -> Vec<Self::Instance> {
        table_small(schema, &ctx.src, scope)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asymmetric::get;
    use crate::tests::{identity_lens, three_node_instance, three_node_schema};
    use panproto_inst::CompiledMigration;
    use panproto_schema::{
        BytesText, HyperEdge, Key, KeyPart, KeyRole, KeyScope, Protocol, ScalarKind, SchemaBuilder,
        Vertex,
    };

    #[derive(Clone, Copy)]
    enum MockMode {
        Partial,
        Drift,
        Collapse,
        IdSensitive,
        MaskedDrift,
        Combined,
        CongruenceOnly,
        MixedNegative,
        AdmittedNegative,
        ExactNegative,
        ExactTwoNegative,
        MissingNegative,
        ExtraNegative,
        ReorderedNegative,
        DuplicateNegative,
        WrongCoordinateNegative,
        WrongRawNegative,
        RawSuccessNegative,
        RawNonDomainNegative,
        PolarityIndependent,
    }

    struct MockShape;

    struct JrShape;

    /// The Johnson--Rosebrugh labelled-arrow lens: `get` forgets edge labels
    /// and `put` restores a remembered label or assigns the default `e` to a
    /// newly inserted carrier.
    struct LabelledGraphJrShape;

    type LabelComplement = BTreeMap<EdgeId, String>;

    fn map_graph_labels(
        graph: &GInstance,
        mut label: impl FnMut(EdgeId, &GEdge) -> Option<panproto_gat::Name>,
    ) -> Result<GInstance, crate::LensError> {
        let mut result = GInstance::new();
        let mut node_ids: Vec<_> = graph.nodes().keys().copied().collect();
        node_ids.sort_unstable();
        for id in node_ids {
            if let Some(node) = graph.node(id) {
                result = result.with_node(node.clone());
            }
        }
        let mut edge_ids: Vec<_> = graph.edges().keys().copied().collect();
        edge_ids.sort_unstable();
        for id in edge_ids {
            if let Some(edge) = graph.edge(id) {
                let mut mapped = edge.clone();
                mapped.edge.name = label(id, edge);
                result
                    .insert_edge(id, mapped)
                    .map_err(|error| crate::LensError::ProtolensError(error.to_string()))?;
            }
        }
        Ok(result)
    }

    fn jr_arrow(id: EdgeId, src: u32, tgt: u32) -> GInstance {
        let source_anchor = match src {
            0 => "A",
            1 => "B",
            _ => "C",
        };
        let target_anchor = match tgt {
            0 => "A",
            1 => "B",
            _ => "C",
        };
        GInstance::new()
            .with_node(Node::new(src, source_anchor))
            .with_node(Node::new(tgt, target_anchor))
            .with_edge_id(
                id,
                src,
                tgt,
                Edge {
                    src: source_anchor.into(),
                    tgt: target_anchor.into(),
                    kind: "arrow".into(),
                    name: None,
                },
            )
    }

    fn jr_a() -> GInstance {
        GInstance::new().with_node(Node::new(0, "A"))
    }

    fn jr_ab() -> GInstance {
        jr_arrow(EdgeId(0), 0, 1)
    }

    fn jr_ac() -> GInstance {
        jr_arrow(EdgeId(1), 0, 2)
    }

    fn jr_ab_ac() -> GInstance {
        let mut graph = jr_ab().with_node(Node::new(2, "C"));
        assert!(
            graph
                .insert_edge(
                    EdgeId(1),
                    GEdge::new(
                        0,
                        2,
                        Edge {
                            src: "A".into(),
                            tgt: "C".into(),
                            kind: "arrow".into(),
                            name: None,
                        },
                    ),
                )
                .is_ok()
        );
        graph
    }

    impl LensShape for MockShape {
        type Instance = u8;
        type Complement = u8;
        type Context = MockMode;

        fn get(
            _lens: &Lens,
            source: &Self::Instance,
            ctx: &Self::Context,
        ) -> Result<(Self::Instance, Self::Complement), crate::LensError> {
            if matches!(ctx, MockMode::MaskedDrift) && *source == 10 {
                Ok((2, 1))
            } else {
                Ok((
                    *source,
                    matches!(ctx, MockMode::Drift)
                        .then_some(*source)
                        .unwrap_or(0),
                ))
            }
        }

        fn put(
            _lens: &Lens,
            view: &Self::Instance,
            complement: &Self::Complement,
            ctx: &Self::Context,
        ) -> Result<Self::Instance, crate::LensError> {
            match ctx {
                MockMode::Partial if *view == 1 => {
                    Err(crate::LensError::ProtolensError("partial put".to_owned()))
                }
                MockMode::Drift => Ok(view.wrapping_add(*complement)),
                MockMode::Collapse => Ok(0),
                MockMode::IdSensitive => Ok(view / 2),
                MockMode::MaskedDrift => match (*view, *complement) {
                    (1, 0) | (2, 1) => Ok(10),
                    (1, 1) => Ok(11),
                    _ => Ok(*view),
                },
                MockMode::Combined => match *view {
                    1 => Ok(2),
                    2 => Err(crate::LensError::ProtolensError(
                        "later totality failure".to_owned(),
                    )),
                    _ => Ok(*view),
                },
                MockMode::CongruenceOnly if *view == 2 => Ok(1),
                MockMode::ExactNegative if *view == 1 => Err(crate::LensError::PutDomain(
                    Box::new(PutDomainError::SynthesizedEdited {
                        elem: panproto_inst::ViewRef::Node(1),
                    }),
                )),
                MockMode::ExactTwoNegative
                | MockMode::MissingNegative
                | MockMode::ExtraNegative
                | MockMode::ReorderedNegative
                | MockMode::DuplicateNegative
                | MockMode::WrongCoordinateNegative
                | MockMode::PolarityIndependent
                    if *view == 1 =>
                {
                    Err(crate::LensError::PutDomain(Box::new(
                        PutDomainError::IdCollision {
                            elem: ViewRef::Node(1),
                        },
                    )))
                }
                MockMode::WrongRawNegative if *view == 1 => Err(crate::LensError::PutDomain(
                    Box::new(PutDomainError::SynthesizedEdited {
                        elem: ViewRef::Node(2),
                    }),
                )),
                MockMode::RawNonDomainNegative if *view == 1 => Err(
                    crate::LensError::ProtolensError("not a domain error".to_owned()),
                ),
                MockMode::RawSuccessNegative if *view == 1 => Ok(*view),
                MockMode::CongruenceOnly
                | MockMode::Partial
                | MockMode::MixedNegative
                | MockMode::AdmittedNegative
                | MockMode::ExactNegative
                | MockMode::ExactTwoNegative
                | MockMode::MissingNegative
                | MockMode::ExtraNegative
                | MockMode::ReorderedNegative
                | MockMode::DuplicateNegative
                | MockMode::WrongCoordinateNegative
                | MockMode::WrongRawNegative
                | MockMode::RawSuccessNegative
                | MockMode::RawNonDomainNegative
                | MockMode::PolarityIndependent => Ok(*view),
            }
        }

        fn check_put_domain(
            _lens: &Lens,
            view: &Self::Instance,
            _complement: &Self::Complement,
            ctx: &Self::Context,
        ) -> Result<(), Vec<PutDomainError>> {
            if matches!(ctx, MockMode::MixedNegative) && *view == 1 {
                return Err(vec![
                    PutDomainError::SynthesizedEdited {
                        elem: panproto_inst::ViewRef::Node(1),
                    },
                    PutDomainError::IdCollision {
                        elem: panproto_inst::ViewRef::Node(1),
                    },
                ]);
            }
            if matches!(ctx, MockMode::ExactNegative) && *view == 1 {
                return Err(vec![PutDomainError::SynthesizedEdited {
                    elem: panproto_inst::ViewRef::Node(1),
                }]);
            }
            if *view == 1 {
                let id = PutDomainError::IdCollision {
                    elem: ViewRef::Node(1),
                };
                let synthesized = PutDomainError::SynthesizedEdited {
                    elem: ViewRef::Node(2),
                };
                let report = match ctx {
                    MockMode::ExactTwoNegative
                    | MockMode::WrongRawNegative
                    | MockMode::RawSuccessNegative
                    | MockMode::RawNonDomainNegative
                    | MockMode::PolarityIndependent => Some(vec![id, synthesized]),
                    MockMode::MissingNegative => Some(vec![id]),
                    MockMode::ExtraNegative => Some(vec![
                        id,
                        synthesized,
                        PutDomainError::MalformedFan {
                            node: 3,
                            reason: FanViolation::MissingParent,
                        },
                    ]),
                    MockMode::ReorderedNegative => Some(vec![synthesized, id]),
                    MockMode::DuplicateNegative => Some(vec![id.clone(), id, synthesized]),
                    MockMode::WrongCoordinateNegative => Some(vec![
                        PutDomainError::IdCollision {
                            elem: ViewRef::Node(9),
                        },
                        synthesized,
                    ]),
                    _ => None,
                };
                if let Some(report) = report {
                    return Err(report);
                }
            }
            Ok(())
        }

        fn synth_index(_instance: &Self::Instance, _complement: &Self::Complement) -> SynthIndex {
            SynthIndex::default()
        }

        fn equivalent(
            a: &Self::Instance,
            _ia: &SynthIndex,
            b: &Self::Instance,
            _ib: &SynthIndex,
        ) -> bool {
            a % 2 == b % 2
        }

        fn divergence(
            a: &Self::Instance,
            ia: &SynthIndex,
            b: &Self::Instance,
            ib: &SynthIndex,
        ) -> Option<String> {
            (!Self::equivalent(a, ia, b, ib)).then(|| format!("{a} != {b} modulo 2"))
        }

        fn divergence_modulo_derived(
            a: &Self::Instance,
            ia: &SynthIndex,
            b: &Self::Instance,
            ib: &SynthIndex,
            _derived: &DerivedMap,
        ) -> Option<String> {
            Self::divergence(a, ia, b, ib)
        }

        fn complement_divergence(a: &Self::Complement, b: &Self::Complement) -> Option<String> {
            (a != b).then(|| format!("{a} != {b}"))
        }

        fn congruence_witnesses(
            instance: &Self::Instance,
            index: &SynthIndex,
        ) -> Vec<(Self::Instance, SynthIndex)> {
            vec![(instance.wrapping_add(2), index.clone())]
        }
    }

    impl LawShape for MockShape {
        fn mutation_cases(
            _lens: &Lens,
            _source: &Self::Instance,
            view: &Self::Instance,
            _complement: &Self::Complement,
            ctx: &Self::Context,
            mutation: Mutation,
        ) -> MutationCases<Self::Instance> {
            if matches!(
                ctx,
                MockMode::MixedNegative | MockMode::AdmittedNegative | MockMode::ExactNegative
            ) {
                return MutationCases::Skip(SkipReason::NoCandidate);
            }
            if matches!(ctx, MockMode::PolarityIndependent) {
                return MutationCases::Skip(SkipReason::NoCandidate);
            }
            match mutation {
                Mutation::ValueEdit => MutationCases::Cases(vec![view.wrapping_add(1)]),
                Mutation::FieldEdit if matches!(ctx, MockMode::CongruenceOnly) => {
                    MutationCases::Skip(SkipReason::NoCandidate)
                }
                Mutation::FieldEdit => MutationCases::Cases(vec![view.wrapping_add(2)]),
                _ => MutationCases::Skip(SkipReason::NoCandidate),
            }
        }

        fn negative_mutation_cases(
            _lens: &Lens,
            _source: &Self::Instance,
            _view: &Self::Instance,
            _complement: &Self::Complement,
            ctx: &Self::Context,
            mutation: Mutation,
        ) -> Vec<NegativeMutationCase<Self::Instance>> {
            if matches!(
                ctx,
                MockMode::MixedNegative | MockMode::AdmittedNegative | MockMode::ExactNegative
            ) && mutation == Mutation::RemoveSynthesized
            {
                return vec![NegativeMutationCase {
                    view: 1,
                    expected: vec![PutDomainError::SynthesizedEdited {
                        elem: ViewRef::Node(1),
                    }],
                }];
            }
            if matches!(ctx, MockMode::PolarityIndependent) && mutation == Mutation::ValueEdit {
                return vec![NegativeMutationCase {
                    view: 1,
                    expected: vec![
                        PutDomainError::IdCollision {
                            elem: ViewRef::Node(1),
                        },
                        PutDomainError::SynthesizedEdited {
                            elem: ViewRef::Node(2),
                        },
                    ],
                }];
            }
            Vec::new()
        }

        fn identical(a: &Self::Instance, b: &Self::Instance) -> bool {
            a == b
        }

        fn congruent_states(
            view: &Self::Instance,
            complement: &Self::Complement,
        ) -> Vec<(Self::Instance, Self::Complement)> {
            vec![(view.wrapping_add(2), *complement)]
        }

        fn rekey_complement(
            complement: &Self::Complement,
            _from: &SynthIndex,
            _to: &SynthIndex,
        ) -> Self::Complement {
            *complement
        }

        fn locality_divergence(
            _lens: &Lens,
            _source: &Self::Instance,
            _original_view: &Self::Instance,
            edited_view: &Self::Instance,
            restored: &Self::Instance,
            _complement: &Self::Complement,
            _ctx: &Self::Context,
        ) -> Option<String> {
            (edited_view != restored).then(|| "restored outside direct footprint".to_owned())
        }

        fn carrier_intersection(a: &Self::Instance, b: &Self::Instance) -> Self::Instance {
            (*a).min(*b)
        }

        fn undo_view(original: &Self::Instance, edited: &Self::Instance) -> Option<Self::Instance> {
            (original != edited).then_some(*original)
        }

        fn enumerate_small(
            _schema: &Schema,
            _scope: SmallScope,
            _ctx: &Self::Context,
        ) -> Vec<Self::Instance> {
            Vec::new()
        }
    }

    impl LensShape for LabelledGraphJrShape {
        type Instance = GInstance;
        type Complement = LabelComplement;
        type Context = ();

        fn get(
            _lens: &Lens,
            source: &Self::Instance,
            _ctx: &Self::Context,
        ) -> Result<(Self::Instance, Self::Complement), crate::LensError> {
            let complement = source
                .edges()
                .iter()
                .filter_map(|(&id, edge)| {
                    edge.edge.name.as_ref().map(|name| (id, name.to_string()))
                })
                .collect();
            let view = map_graph_labels(source, |_, _| None)?;
            Ok((view, complement))
        }

        fn put(
            _lens: &Lens,
            view: &Self::Instance,
            complement: &Self::Complement,
            _ctx: &Self::Context,
        ) -> Result<Self::Instance, crate::LensError> {
            map_graph_labels(view, |id, _| {
                Some(complement.get(&id).map_or("e", String::as_str).into())
            })
        }

        fn check_put_domain(
            _lens: &Lens,
            _view: &Self::Instance,
            _complement: &Self::Complement,
            _ctx: &Self::Context,
        ) -> Result<(), Vec<PutDomainError>> {
            Ok(())
        }

        fn synth_index(_instance: &Self::Instance, _complement: &Self::Complement) -> SynthIndex {
            SynthIndex::default()
        }

        fn equivalent(
            a: &Self::Instance,
            ia: &SynthIndex,
            b: &Self::Instance,
            ib: &SynthIndex,
        ) -> bool {
            GraphShape::equivalent(a, ia, b, ib)
        }

        fn divergence(
            a: &Self::Instance,
            ia: &SynthIndex,
            b: &Self::Instance,
            ib: &SynthIndex,
        ) -> Option<String> {
            GraphShape::divergence(a, ia, b, ib)
        }

        fn divergence_modulo_derived(
            a: &Self::Instance,
            ia: &SynthIndex,
            b: &Self::Instance,
            ib: &SynthIndex,
            derived: &DerivedMap,
        ) -> Option<String> {
            GraphShape::divergence_modulo_derived(a, ia, b, ib, derived)
        }

        fn complement_divergence(a: &Self::Complement, b: &Self::Complement) -> Option<String> {
            (a != b).then(|| "remembered edge labels differ".to_owned())
        }

        fn congruence_witnesses(
            _instance: &Self::Instance,
            _index: &SynthIndex,
        ) -> Vec<(Self::Instance, SynthIndex)> {
            Vec::new()
        }
    }

    impl LawShape for LabelledGraphJrShape {
        fn mutation_cases(
            _lens: &Lens,
            _source: &Self::Instance,
            view: &Self::Instance,
            _complement: &Self::Complement,
            _ctx: &Self::Context,
            mutation: Mutation,
        ) -> MutationCases<Self::Instance> {
            let is = |other: &GInstance| {
                GraphShape::equivalent(view, &SynthIndex::default(), other, &SynthIndex::default())
            };
            match mutation {
                Mutation::AddNode if is(&jr_ab()) => MutationCases::Cases(vec![jr_ab_ac()]),
                Mutation::AddNode if is(&jr_a()) => MutationCases::Cases(vec![jr_ab()]),
                Mutation::RemoveNode if is(&jr_ab()) => MutationCases::Cases(vec![jr_a()]),
                Mutation::RemoveNode if is(&jr_ab_ac()) => MutationCases::Cases(vec![jr_ac()]),
                _ => MutationCases::Skip(SkipReason::NoCandidate),
            }
        }

        fn identical(a: &Self::Instance, b: &Self::Instance) -> bool {
            GraphShape::equivalent(a, &SynthIndex::default(), b, &SynthIndex::default())
        }

        fn congruent_states(
            _view: &Self::Instance,
            _complement: &Self::Complement,
        ) -> Vec<(Self::Instance, Self::Complement)> {
            Vec::new()
        }

        fn rekey_complement(
            complement: &Self::Complement,
            _from: &SynthIndex,
            _to: &SynthIndex,
        ) -> Self::Complement {
            complement.clone()
        }

        fn locality_divergence(
            _lens: &Lens,
            _source: &Self::Instance,
            _original_view: &Self::Instance,
            _edited_view: &Self::Instance,
            _restored: &Self::Instance,
            _complement: &Self::Complement,
            _ctx: &Self::Context,
        ) -> Option<String> {
            None
        }

        fn carrier_intersection(a: &Self::Instance, b: &Self::Instance) -> Self::Instance {
            graph_intersection(a, b)
        }

        fn undo_view(original: &Self::Instance, edited: &Self::Instance) -> Option<Self::Instance> {
            (!Self::identical(original, edited)).then(|| original.clone())
        }

        fn enumerate_small(
            _schema: &Schema,
            _scope: SmallScope,
            _ctx: &Self::Context,
        ) -> Vec<Self::Instance> {
            Vec::new()
        }
    }

    impl LensShape for JrShape {
        type Instance = BTreeSet<char>;
        type Complement = u8;
        type Context = ();

        fn get(
            _lens: &Lens,
            source: &Self::Instance,
            _ctx: &Self::Context,
        ) -> Result<(Self::Instance, Self::Complement), crate::LensError> {
            let complement = if source == &BTreeSet::from(['a', 'x']) {
                1
            } else if source.is_empty() {
                2
            } else {
                0
            };
            Ok((source.clone(), complement))
        }

        fn put(
            _lens: &Lens,
            view: &Self::Instance,
            complement: &Self::Complement,
            _ctx: &Self::Context,
        ) -> Result<Self::Instance, crate::LensError> {
            if view == &BTreeSet::from(['x']) {
                return Ok(BTreeSet::from([if *complement == 0 { 'd' } else { 'l' }]));
            }
            Ok(view.clone())
        }

        fn check_put_domain(
            _lens: &Lens,
            _view: &Self::Instance,
            _complement: &Self::Complement,
            _ctx: &Self::Context,
        ) -> Result<(), Vec<PutDomainError>> {
            Ok(())
        }

        fn synth_index(_instance: &Self::Instance, _complement: &Self::Complement) -> SynthIndex {
            SynthIndex::default()
        }

        fn equivalent(
            a: &Self::Instance,
            _ia: &SynthIndex,
            b: &Self::Instance,
            _ib: &SynthIndex,
        ) -> bool {
            a == b
        }

        fn divergence(
            a: &Self::Instance,
            _ia: &SynthIndex,
            b: &Self::Instance,
            _ib: &SynthIndex,
        ) -> Option<String> {
            (a != b).then(|| "carrier sets differ".to_owned())
        }

        fn divergence_modulo_derived(
            a: &Self::Instance,
            ia: &SynthIndex,
            b: &Self::Instance,
            ib: &SynthIndex,
            _derived: &DerivedMap,
        ) -> Option<String> {
            Self::divergence(a, ia, b, ib)
        }

        fn complement_divergence(a: &Self::Complement, b: &Self::Complement) -> Option<String> {
            (a != b).then(|| "complements differ".to_owned())
        }

        fn congruence_witnesses(
            _instance: &Self::Instance,
            _index: &SynthIndex,
        ) -> Vec<(Self::Instance, SynthIndex)> {
            Vec::new()
        }
    }

    impl LawShape for JrShape {
        fn mutation_cases(
            _lens: &Lens,
            _source: &Self::Instance,
            view: &Self::Instance,
            _complement: &Self::Complement,
            _ctx: &Self::Context,
            mutation: Mutation,
        ) -> MutationCases<Self::Instance> {
            match (mutation, view) {
                (Mutation::AddNode, view) if view == &BTreeSet::from(['a']) => {
                    MutationCases::Cases(vec![BTreeSet::from(['a', 'x'])])
                }
                (Mutation::RemoveNode, view) if view == &BTreeSet::from(['a', 'x']) => {
                    MutationCases::Cases(vec![BTreeSet::from(['x'])])
                }
                _ => MutationCases::Skip(SkipReason::NoCandidate),
            }
        }

        fn identical(a: &Self::Instance, b: &Self::Instance) -> bool {
            a == b
        }

        fn congruent_states(
            _view: &Self::Instance,
            _complement: &Self::Complement,
        ) -> Vec<(Self::Instance, Self::Complement)> {
            Vec::new()
        }

        fn rekey_complement(
            complement: &Self::Complement,
            _from: &SynthIndex,
            _to: &SynthIndex,
        ) -> Self::Complement {
            *complement
        }

        fn locality_divergence(
            _lens: &Lens,
            _source: &Self::Instance,
            _original_view: &Self::Instance,
            _edited_view: &Self::Instance,
            _restored: &Self::Instance,
            _complement: &Self::Complement,
            _ctx: &Self::Context,
        ) -> Option<String> {
            None
        }

        fn carrier_intersection(a: &Self::Instance, b: &Self::Instance) -> Self::Instance {
            a.intersection(b).copied().collect()
        }

        fn undo_view(original: &Self::Instance, edited: &Self::Instance) -> Option<Self::Instance> {
            (original != edited).then(|| original.clone())
        }

        fn enumerate_small(
            _schema: &Schema,
            _scope: SmallScope,
            _ctx: &Self::Context,
        ) -> Vec<Self::Instance> {
            Vec::new()
        }
    }

    fn one_vertex_schema(kind: &str, with_loop: bool) -> Schema {
        let mut schema = Schema::default();
        schema.vertices.insert(
            "n".into(),
            Vertex {
                id: "n".into(),
                kind: kind.into(),
                nsid: None,
            },
        );
        if with_loop {
            let loop_edge = Edge {
                src: "n".into(),
                tgt: "n".into(),
                kind: "edge".into(),
                name: None,
            };
            schema.edges.insert(loop_edge, "edge".into());
        }
        schema
    }

    fn two_label_hyper_schema() -> Schema {
        let root = Vertex {
            id: "r".into(),
            kind: "record".into(),
            nsid: None,
        };
        let child = Vertex {
            id: "c".into(),
            kind: "record".into(),
            nsid: None,
        };
        let hyper = HyperEdge {
            id: "h".into(),
            kind: "rel".into(),
            signature: HashMap::from([
                ("p".into(), "r".into()),
                ("x".into(), "c".into()),
                ("y".into(), "c".into()),
            ]),
            parent_label: "p".into(),
        };
        Schema {
            vertices: HashMap::from([(root.id.clone(), root), (child.id.clone(), child)]),
            hyper_edges: HashMap::from([(hyper.id.clone(), hyper)]),
            ..Schema::default()
        }
    }

    fn assert_table_float_alternate(lens: &Lens) {
        let mut table = FInstance::new();
        assert!(
            table
                .insert_row(
                    "row",
                    RowId(0),
                    HashMap::from([("value".to_owned(), Value::Float(f64::INFINITY))]),
                )
                .is_ok()
        );
        let MutationCases::Cases(cases) = table_cases(
            lens,
            &table,
            &TableComplement::empty(),
            &TableContext::default(),
            Mutation::ValueEdit,
        ) else {
            panic!("table float cell must have an alternate");
        };
        assert!(cases.iter().any(|candidate| {
            matches!(
                candidate.row(RowId(0)).and_then(|row| row.get("value")),
                Some(Value::Float(0.0))
            )
        }));
    }

    fn table_protocol() -> Protocol {
        Protocol {
            value_kinds: BTreeMap::from([(
                "int".to_owned(),
                ScalarKind::Int {
                    bits: 64,
                    signed: true,
                },
            )]),
            non_value_kinds: BTreeSet::from(["record".to_owned()]),
            ..Protocol::default()
        }
    }

    fn column(table: &str, name: &str) -> Edge {
        Edge {
            src: table.into(),
            tgt: format!("{table}.{name}").into(),
            kind: "prop".into(),
            name: Some(name.into()),
        }
    }

    fn relation(src: &str, tgt: &str, name: &str) -> Edge {
        Edge {
            src: src.into(),
            tgt: tgt.into(),
            kind: "fk".into(),
            name: Some(name.into()),
        }
    }

    fn graph_schema(
        vertices: &[&str],
        edges: &[Edge],
    ) -> Result<Schema, Box<dyn std::error::Error>> {
        let protocol = Protocol::default();
        let mut builder = SchemaBuilder::new(&protocol);
        for vertex in vertices {
            builder = builder.vertex(vertex, "record", None)?;
        }
        for edge in edges {
            builder = builder.edge(
                edge.src.as_ref(),
                edge.tgt.as_ref(),
                edge.kind.as_ref(),
                edge.name.as_ref().map(AsRef::as_ref),
            )?;
        }
        Ok(builder.build()?)
    }

    fn cloned_lens(lens: &Lens) -> Lens {
        Lens {
            compiled: lens.compiled.clone(),
            src_schema: lens.src_schema.clone(),
            tgt_schema: lens.tgt_schema.clone(),
        }
    }

    fn insert_nonnullable_reference(schema: &mut Schema) {
        schema
            .declarations
            .keys
            .entry("b".into())
            .or_default()
            .push(Key {
                id: "reference-target".into(),
                parts: vec![KeyPart::Path(Vec::new())],
                role: KeyRole::Unique,
                scope: KeyScope::Global,
            });
        let reference = panproto_schema::Reference {
            id: "required-ref".into(),
            holder: "a".into(),
            source: panproto_schema::RefSource::KeyValue {
                parts: vec![Vec::new()],
            },
            target_vertex: "b".into(),
            target_key: Some("reference-target".into()),
            edge: None,
            cardinality: panproto_schema::Cardinality::One,
            nullable: false,
            on_unresolved: panproto_schema::OnUnresolved::Error,
            mode: panproto_schema::ResolveMode::AddEdge,
        };
        schema
            .declarations
            .references
            .insert(reference.id.clone(), reference);
    }

    fn contraction_stage_fixture() -> Result<(Lens, WInstance), Box<dyn std::error::Error>> {
        let ab = Edge {
            src: "a".into(),
            tgt: "b".into(),
            kind: "prop".into(),
            name: Some("e".into()),
        };
        let ba = Edge {
            src: "b".into(),
            tgt: "a".into(),
            kind: "prop".into(),
            name: Some("e".into()),
        };
        let aa = Edge {
            src: "a".into(),
            tgt: "a".into(),
            kind: "prop".into(),
            name: Some("e".into()),
        };
        let lens = Lens {
            compiled: CompiledMigration {
                surviving_verts: std::iter::once("a".into()).collect(),
                resolver: std::iter::once((("a".into(), "a".into()), aa.clone())).collect(),
                ..CompiledMigration::default()
            },
            src_schema: graph_schema(&["a", "b"], &[ab.clone(), ba.clone()])?,
            tgt_schema: graph_schema(&["a"], std::slice::from_ref(&aa))?,
        };
        let source = WInstance::new(
            HashMap::from([
                (0, Node::new(0, "a")),
                (1, Node::new(1, "a")),
                (2, Node::new(2, "b")),
            ]),
            vec![(0, 2, ab), (2, 1, ba)],
            Vec::new(),
            0,
            "a".into(),
        );
        Ok((lens, source))
    }

    fn reserved_insert_fixture() -> Result<(Lens, WInstance), Box<dyn std::error::Error>> {
        let aa = Edge {
            src: "a".into(),
            tgt: "a".into(),
            kind: "prop".into(),
            name: Some("e".into()),
        };
        let ab = Edge {
            src: "a".into(),
            tgt: "b".into(),
            kind: "prop".into(),
            name: Some("e".into()),
        };
        let lens = Lens {
            compiled: CompiledMigration {
                surviving_verts: std::iter::once("a".into()).collect(),
                surviving_edges: std::iter::once(aa.clone()).collect(),
                ..CompiledMigration::default()
            },
            src_schema: graph_schema(&["a", "b"], &[aa.clone(), ab.clone()])?,
            tgt_schema: graph_schema(&["a"], std::slice::from_ref(&aa))?,
        };
        let source = WInstance::new(
            HashMap::from([(0, Node::new(0, "a")), (2, Node::new(2, "b"))]),
            vec![(0, 2, ab)],
            Vec::new(),
            0,
            "a".into(),
        );
        Ok((lens, source))
    }

    fn reserved_fan_kind_collision_fixture(
        kind: &str,
    ) -> Result<(Lens, WInstance, Edge), Box<dyn std::error::Error>> {
        let kept = Edge {
            src: "a".into(),
            tgt: "a".into(),
            kind: kind.into(),
            name: (kind == panproto_inst::HYPER_LABEL_KIND).then(|| "member".into()),
        };
        let dropped = relation("a", "b", "dropped");
        let lens = Lens {
            compiled: CompiledMigration {
                surviving_verts: std::iter::once("a".into()).collect(),
                surviving_edges: std::iter::once(kept.clone()).collect(),
                ..CompiledMigration::default()
            },
            src_schema: graph_schema(&["a", "b"], &[kept.clone(), dropped.clone()])?,
            tgt_schema: graph_schema(&["a"], std::slice::from_ref(&kept))?,
        };
        let source = WInstance::new(
            HashMap::from([(0, Node::new(0, "a")), (2, Node::new(2, "b"))]),
            vec![(0, 2, dropped)],
            Vec::new(),
            0,
            "a".into(),
        );
        Ok((lens, source, kept))
    }

    fn source_reserved_fan_kind_collision_fixture(
        kind: &str,
    ) -> Result<(Lens, WInstance, Edge), Box<dyn std::error::Error>> {
        let source_kept = Edge {
            src: "a".into(),
            tgt: "a".into(),
            kind: kind.into(),
            name: (kind == panproto_inst::HYPER_LABEL_KIND).then(|| "member".into()),
        };
        let target_kept = relation("a", "a", "ordinary");
        let dropped = relation("a", "b", "dropped");
        let lens = Lens {
            compiled: CompiledMigration {
                surviving_verts: std::iter::once("a".into()).collect(),
                surviving_edges: std::iter::once(target_kept.clone()).collect(),
                edge_remap: HashMap::from([(source_kept.clone(), target_kept.clone())]),
                ..CompiledMigration::default()
            },
            src_schema: graph_schema(&["a", "b"], &[source_kept, dropped.clone()])?,
            tgt_schema: graph_schema(&["a"], std::slice::from_ref(&target_kept))?,
        };
        let source = WInstance::new(
            HashMap::from([(0, Node::new(0, "a")), (2, Node::new(2, "b"))]),
            vec![(0, 2, dropped)],
            Vec::new(),
            0,
            "a".into(),
        );
        Ok((lens, source, target_kept))
    }

    #[derive(Clone, Copy)]
    enum ReservedAttachment {
        FirstEligible,
        LaterEligible,
        None,
    }

    fn synthesized_parent_reserved_fixture(
        attachment: ReservedAttachment,
    ) -> Result<(Lens, WInstance), Box<dyn std::error::Error>> {
        let rel = |src: &str, tgt: &str, name: &str| Edge {
            src: src.into(),
            tgt: tgt.into(),
            kind: "rel".into(),
            name: Some(name.into()),
        };
        let ab = rel("a", "b", "path");
        let am = rel("a", "m", "path-in");
        let mb = rel("m", "b", "path-out");
        let mx = rel("m", "x", "attach");
        let za = rel("z", "a", "to-a");
        let zm = rel("z", "m", "to-m");
        let zy = rel("z", "y", "to-y");
        let md = rel("m", "d", "drop");
        let zd = rel("z", "d", "drop");
        let yd = rel("y", "d", "drop");
        let (drop_parent, dropped) = match attachment {
            ReservedAttachment::FirstEligible => (10, md),
            ReservedAttachment::LaterEligible => (0, zd),
            ReservedAttachment::None => (11, yd),
        };
        let lens = Lens {
            compiled: CompiledMigration {
                surviving_verts: [
                    "z".into(),
                    "a".into(),
                    "m".into(),
                    "b".into(),
                    "x".into(),
                    "y".into(),
                ]
                .into_iter()
                .collect(),
                surviving_edges: [za.clone(), zm.clone(), zy.clone(), mx.clone()]
                    .into_iter()
                    .collect(),
                expansion_path: std::iter::once((("a".into(), "b".into()), vec!["m".into()]))
                    .collect(),
                ..CompiledMigration::default()
            },
            src_schema: graph_schema(
                &["z", "a", "m", "b", "x", "y", "d"],
                &[
                    za.clone(),
                    zm.clone(),
                    zy.clone(),
                    ab.clone(),
                    mx.clone(),
                    dropped.clone(),
                ],
            )?,
            tgt_schema: graph_schema(
                &["z", "a", "m", "b", "x", "y"],
                &[am, mb, mx, za.clone(), zm.clone(), zy.clone()],
            )?,
        };
        let source = WInstance::new(
            HashMap::from([
                (0, Node::new(0, "z")),
                (1, Node::new(1, "b")),
                (2, Node::new(2, "d")),
                (4, Node::new(4, "a")),
                (10, Node::new(10, "m")),
                (11, Node::new(11, "y")),
            ]),
            vec![
                (0, 4, za),
                (4, 1, ab),
                (0, 10, zm),
                (0, 11, zy),
                (drop_parent, 2, dropped),
            ],
            Vec::new(),
            0,
            "z".into(),
        );
        Ok((lens, source))
    }

    fn move_only_synthesized_tree_node(
        view: &WInstance,
        complement: &Complement,
        next_id: u32,
    ) -> (WInstance, Complement) {
        assert_eq!(
            complement.synthesized_keys.len(),
            1,
            "the fixture must have exactly one synthesized node"
        );
        let Some(old_id) = complement.synthesized_keys.keys().copied().next() else {
            panic!("the synthesized node must have an id");
        };
        let from = TreeShape::synth_index(view, complement);
        let mut to = from.clone();
        let Some(key) = to.nodes.remove(&old_id) else {
            panic!("the synthesized node must have a key");
        };
        to.nodes.insert(next_id, key);
        let next_complement = rekey_tree_complement(complement, &from, &to);
        let mut nodes = view.nodes.clone();
        let Some(mut node) = nodes.remove(&old_id) else {
            panic!("the synthesized node must be present");
        };
        node.id = next_id;
        nodes.insert(next_id, node);
        let arcs = view
            .arcs
            .iter()
            .map(|(parent, child, edge)| {
                (
                    if *parent == old_id { next_id } else { *parent },
                    if *child == old_id { next_id } else { *child },
                    edge.clone(),
                )
            })
            .collect();
        (rebuild_tree(view, nodes, arcs), next_complement)
    }

    fn without_reserved_tree_residue(mut complement: Complement, id: u32) -> Complement {
        let mut incident_source_edges = BTreeSet::new();
        let mut source_edge_reindex = BTreeMap::new();
        let mut retained_index = 0_usize;
        for (old_index, (parent, child)) in complement.arc_order.iter().enumerate() {
            let Ok(old_index) = u32::try_from(old_index) else {
                retained_index += usize::from(*parent != id && *child != id);
                continue;
            };
            if *parent == id || *child == id {
                incident_source_edges.insert(EdgeId(old_index));
            } else {
                if let Ok(new_index) = u32::try_from(retained_index) {
                    source_edge_reindex.insert(EdgeId(old_index), EdgeId(new_index));
                }
                retained_index += 1;
            }
        }
        let mentions_id = |elem: &panproto_inst::ViewRef| match elem {
            panproto_inst::ViewRef::Node(node) => *node == id,
            panproto_inst::ViewRef::Arc { parent, child } => *parent == id || *child == id,
            panproto_inst::ViewRef::Edge(_)
            | panproto_inst::ViewRef::Row(_)
            | panproto_inst::ViewRef::Pair { .. } => false,
        };
        complement.dropped_nodes.remove(&id);
        complement
            .dropped_arcs
            .retain(|(parent, child, _)| *parent != id && *child != id);
        complement
            .dropped_fans
            .retain(|fan| fan.parent != id && fan.children.values().all(|child| *child != id));
        complement
            .contraction_choices
            .retain(|(parent, child), _| *parent != id && *child != id);
        complement
            .original_parent
            .retain(|child, parent| *child != id && *parent != id);
        complement
            .arc_edges
            .retain(|(parent, child), _| *parent != id && *child != id);
        complement
            .arc_order
            .retain(|(parent, child)| *parent != id && *child != id);
        complement.original_extra_fields.remove(&id);
        complement.original_values.remove(&id);
        complement.synthesized_nodes.remove(&id);
        complement.contracted_into.remove(&id);
        complement.contracted_into.retain(|_, target| *target != id);
        complement.original_anchors.remove(&id);
        complement.synthesized_keys.retain(|node, key| {
            *node != id
                && match key {
                    SynthKey::Contract { src, tgt, .. }
                    | SynthKey::ContractExpand { src, tgt, .. } => *src != id && *tgt != id,
                    SynthKey::Expand { source_edge, .. } => {
                        if incident_source_edges.contains(source_edge) {
                            false
                        } else if let Some(reindexed) = source_edge_reindex.get(source_edge) {
                            *source_edge = *reindexed;
                            true
                        } else {
                            false
                        }
                    }
                    SynthKey::ResolutionReference { holder, .. } => !mentions_id(holder),
                    SynthKey::ResolutionSpan { text, .. } => !mentions_id(text.site().holder()),
                }
        });
        complement
    }

    fn assert_reserved_tree_counterfactual(
        lens: &Lens,
        candidate: &WInstance,
        complement: &Complement,
        id: u32,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let expected = PutDomainError::IdCollision {
            elem: panproto_inst::ViewRef::Node(id),
        };
        assert_eq!(
            check_put_domain(lens, candidate, complement),
            Err(vec![expected.clone()])
        );
        assert!(matches!(
            crate::asymmetric::put(lens, candidate, complement),
            Err(crate::LensError::PutDomain(error)) if error.as_ref() == &expected
        ));

        let stripped = without_reserved_tree_residue(complement.clone(), id);
        assert_eq!(check_put_domain(lens, candidate, &stripped), Ok(()));
        let restored = crate::asymmetric::put(lens, candidate, &stripped)?;
        assert!(
            panproto_inst::validate_wtype(
                &restored,
                &lens.src_schema,
                &panproto_schema::Protocol::default(),
            )
            .passed()
        );
        let (roundtrip, roundtrip_complement) = get(lens, &restored)?;
        let expected_index = TreeShape::synth_index(candidate, &stripped);
        let actual_index = TreeShape::synth_index(&roundtrip, &roundtrip_complement);
        assert!(TreeShape::equivalent(
            candidate,
            &expected_index,
            &roundtrip,
            &actual_index,
        ));
        Ok(())
    }

    fn adversarial_contraction_edit_fixture(
        include_actual_preimage: bool,
    ) -> Result<(Lens, WInstance, Edge), Box<dyn std::error::Error>> {
        let ab = Edge {
            src: "a".into(),
            tgt: "b".into(),
            kind: "prop".into(),
            name: Some("path-out".into()),
        };
        let ba = Edge {
            src: "b".into(),
            tgt: "a".into(),
            kind: "prop".into(),
            name: Some("path-in".into()),
        };
        let contracted = Edge {
            src: "a".into(),
            tgt: "a".into(),
            kind: "prop".into(),
            name: Some("contracted".into()),
        };
        let replacement = Edge {
            src: "a".into(),
            tgt: "a".into(),
            kind: "prop".into(),
            name: Some("replacement".into()),
        };
        let source_aa = replacement.clone();
        let source_cc = Edge {
            src: "c".into(),
            tgt: "c".into(),
            ..replacement.clone()
        };
        let mut edge_remap = HashMap::from([(source_cc.clone(), replacement.clone())]);
        let mut source_edges = vec![ab.clone(), ba.clone(), source_cc];
        if include_actual_preimage {
            edge_remap.insert(source_aa.clone(), replacement.clone());
            source_edges.push(source_aa);
        }
        let lens = Lens {
            compiled: CompiledMigration {
                surviving_verts: std::iter::once("a".into()).collect(),
                vertex_remap: std::iter::once(("c".into(), "a".into())).collect(),
                edge_remap,
                resolver: std::iter::once((("a".into(), "a".into()), contracted.clone())).collect(),
                ..CompiledMigration::default()
            },
            src_schema: graph_schema(&["a", "b", "c"], &source_edges)?,
            tgt_schema: graph_schema(&["a"], &[contracted, replacement.clone()])?,
        };
        let source = WInstance::new(
            HashMap::from([
                (0, Node::new(0, "a")),
                (1, Node::new(1, "a")),
                (2, Node::new(2, "b")),
            ]),
            vec![(0, 2, ab), (2, 1, ba)],
            Vec::new(),
            0,
            "a".into(),
        );
        Ok((lens, source, replacement))
    }

    fn cascading_projection_fixture() -> Result<(Lens, GInstance), Box<dyn std::error::Error>> {
        let hidden_edge = Edge {
            src: "kept".into(),
            tgt: "hidden".into(),
            kind: "rel".into(),
            name: Some("secret".into()),
        };
        let lens = Lens {
            compiled: CompiledMigration {
                surviving_verts: std::iter::once("kept".into()).collect(),
                ..CompiledMigration::default()
            },
            src_schema: graph_schema(&["kept", "hidden"], std::slice::from_ref(&hidden_edge))?,
            tgt_schema: graph_schema(&["kept"], &[])?,
        };
        let source = GInstance::new()
            .with_node(Node::new(0, "kept"))
            .with_node(Node::new(9, "hidden"))
            .with_edge_id(EdgeId(7), 0, 9, hidden_edge);
        Ok((lens, source))
    }

    fn tree_expansion_fixture() -> Result<(Lens, WInstance, Edge), Box<dyn std::error::Error>> {
        let ab = Edge {
            src: "a".into(),
            tgt: "b".into(),
            kind: "rel".into(),
            name: Some("b".into()),
        };
        let am = Edge {
            src: "a".into(),
            tgt: "m".into(),
            kind: "rel".into(),
            name: Some("m".into()),
        };
        let mb = Edge {
            src: "m".into(),
            tgt: "b".into(),
            kind: "rel".into(),
            name: Some("b".into()),
        };
        let ax = Edge {
            src: "a".into(),
            tgt: "x".into(),
            kind: "rel".into(),
            name: Some("x".into()),
        };
        let lens = Lens {
            compiled: CompiledMigration {
                surviving_verts: ["a".into(), "b".into(), "x".into()].into_iter().collect(),
                expansion_path: std::iter::once((("a".into(), "b".into()), vec!["m".into()]))
                    .collect(),
                ..CompiledMigration::default()
            },
            src_schema: graph_schema(&["a", "b", "x"], &[ab.clone(), ax.clone()])?,
            tgt_schema: graph_schema(&["a", "m", "b", "x"], &[am, mb, ax.clone()])?,
        };
        let source = WInstance::new(
            HashMap::from([(7, Node::new(7, "a")), (19, Node::new(19, "b"))]),
            vec![(7, 19, ab)],
            Vec::new(),
            7,
            "a".into(),
        );
        Ok((lens, source, ax))
    }

    fn graph_null_value_report(kind: &str) -> Result<FamilyReport, Box<dyn std::error::Error>> {
        let schema = one_vertex_schema(kind, false);
        let lens = identity_lens(&schema);
        let source = GInstance::new().with_node(Node::new(0, "n").with_value(FieldPresence::Null));
        let (view, complement) = GraphShape::get(&lens, &source, &())?;
        let (ran, _) = report_families::<GraphShape>(&lens, &source, &view, &complement, &());
        ran.into_iter()
            .find(|family| family.mutation == Mutation::ValueEdit)
            .ok_or_else(|| format!("{kind}-null graph did not exercise ValueEdit").into())
    }

    fn tree_null_value_report(kind: &str) -> Result<FamilyReport, Box<dyn std::error::Error>> {
        let schema = one_vertex_schema(kind, false);
        let lens = identity_lens(&schema);
        let source = WInstance::new(
            HashMap::from([(0, Node::new(0, "n").with_value(FieldPresence::Null))]),
            Vec::new(),
            Vec::new(),
            0,
            "n".into(),
        );
        let (view, complement) = TreeShape::get(&lens, &source, &())?;
        let (ran, _) = report_families::<TreeShape>(&lens, &source, &view, &complement, &());
        ran.into_iter()
            .find(|family| family.mutation == Mutation::ValueEdit)
            .ok_or_else(|| format!("{kind}-null tree did not exercise ValueEdit").into())
    }

    fn table_value_report(
        scalar: Option<ScalarKind>,
        initial: Value,
    ) -> Result<FamilyReport, Box<dyn std::error::Error>> {
        let protocol = Protocol {
            value_kinds: BTreeMap::from([("scalar".to_owned(), scalar.unwrap_or(ScalarKind::Any))]),
            non_value_kinds: BTreeSet::from(["record".to_owned()]),
            ..Protocol::default()
        };
        let schema = SchemaBuilder::new(&protocol)
            .vertex("row", "record", None)?
            .vertex("row.value", "scalar", None)?
            .edge("row", "row.value", "column", Some("value"))?
            .build()?;
        let lens = identity_lens(&schema);
        let source = FInstance::new().with_rows(
            "row",
            [(RowId(0), HashMap::from([("value".to_owned(), initial)]))],
        )?;
        let ctx = TableContext {
            src: protocol.clone(),
            tgt: protocol,
        };
        let (view, complement) = TableShape::get(&lens, &source, &ctx)?;
        let (ran, _) = report_families::<TableShape>(&lens, &source, &view, &complement, &ctx);
        ran.into_iter()
            .find(|family| family.mutation == Mutation::ValueEdit)
            .ok_or_else(|| "table null did not exercise ValueEdit".into())
    }

    fn optional_bool_table_scenario()
    -> Result<(Lens, FInstance, TableContext), Box<dyn std::error::Error>> {
        let protocol = Protocol {
            value_kinds: BTreeMap::from([("bool".to_owned(), ScalarKind::Bool)]),
            non_value_kinds: BTreeSet::from(["record".to_owned()]),
            ..Protocol::default()
        };
        let schema = SchemaBuilder::new(&protocol)
            .vertex("t", "record", None)?
            .vertex("t.a", "bool", None)?
            .edge("t", "t.a", "column", Some("a"))?
            .build()?;
        let lens = identity_lens(&schema);
        let source = FInstance::new().with_rows("t", [(RowId(0), HashMap::new())])?;
        let ctx = TableContext {
            src: protocol.clone(),
            tgt: protocol,
        };
        Ok((lens, source, ctx))
    }

    #[test]
    fn mutation_variants_are_the_contract_list() -> Result<(), serde_json::Error> {
        assert_eq!(
            serde_json::to_value(Mutation::ALL)?,
            serde_json::json!([
                "value_edit",
                "field_edit",
                "add_node",
                "remove_node",
                "relabel_node",
                "add_edge",
                "remove_edge",
                "reconnect",
                "relabel_edge",
                "reorder",
                "add_parallel_edge",
                "add_self_loop",
                "remove_synthesized",
                "edit_synthesized",
                "add_contracted_edge",
                "reserved_id_reuse",
                "malformed_fan"
            ])
        );
        Ok(())
    }

    #[test]
    fn single_step_payload_alternates_cover_metadata_and_numeric_boundaries() {
        for value in [f64::INFINITY, f64::NEG_INFINITY, f64::NAN] {
            assert_ne!(alternate_float(value).to_bits(), value.to_bits());
        }
        assert_eq!(alternate_position(Some(u32::MAX)), 0);

        let schema = one_vertex_schema("object", false);
        let lens = identity_lens(&schema);
        let mut node = Node::new(0, "n").with_shape(panproto_inst::NodeShape::List);
        node.value = Some(FieldPresence::Present(Value::Float(f64::INFINITY)));
        node.discriminator = Some("tag".into());
        node.position = Some(u32::MAX);
        node.annotations
            .insert("weight".to_owned(), Value::Float(f64::NEG_INFINITY));

        let graph = GInstance::new().with_node(node.clone());
        let graph_complement = GraphComplement::empty();
        let MutationCases::Cases(value_cases) =
            graph_cases(&lens, &graph, &graph_complement, Mutation::ValueEdit)
        else {
            panic!("a present graph value must have an alternate");
        };
        assert!(value_cases.iter().any(|candidate| {
            candidate
                .node(0)
                .and_then(|node| node.value.as_ref())
                .is_some_and(|value| matches!(value, FieldPresence::Present(Value::Float(0.0))))
        }));
        let MutationCases::Cases(field_cases) =
            graph_cases(&lens, &graph, &graph_complement, Mutation::FieldEdit)
        else {
            panic!("graph metadata must have field-edit alternates");
        };
        assert!(field_cases.iter().any(|candidate| {
            candidate
                .node(0)
                .is_some_and(|node| node.discriminator.is_none())
        }));
        assert!(field_cases.iter().any(|candidate| {
            candidate
                .node(0)
                .is_some_and(|node| matches!(node.shape, panproto_inst::NodeShape::Plain))
        }));
        assert!(field_cases.iter().any(|candidate| {
            candidate.node(0).is_some_and(|node| {
                matches!(node.annotations.get("weight"), Some(Value::Float(0.0)))
            })
        }));
        let MutationCases::Cases(reorder_cases) =
            graph_cases(&lens, &graph, &graph_complement, Mutation::Reorder)
        else {
            panic!("graph node position must have a reorder alternate");
        };
        assert!(reorder_cases.iter().any(|candidate| {
            candidate
                .node(0)
                .is_some_and(|node| node.position == Some(0))
        }));

        let tree = WInstance::new(
            HashMap::from([(0, node)]),
            Vec::new(),
            Vec::new(),
            0,
            "n".into(),
        );
        let tree_complement = Complement::empty();
        let MutationCases::Cases(tree_fields) =
            tree_cases(&lens, &tree, &tree_complement, Mutation::FieldEdit)
        else {
            panic!("tree metadata must have field-edit alternates");
        };
        assert!(tree_fields.iter().any(|candidate| {
            candidate.nodes[&0].discriminator.is_none()
                && matches!(candidate.nodes[&0].shape, panproto_inst::NodeShape::List)
        }));
        let MutationCases::Cases(tree_reorders) =
            tree_cases(&lens, &tree, &tree_complement, Mutation::Reorder)
        else {
            panic!("tree node position must have a reorder alternate");
        };
        assert!(
            tree_reorders
                .iter()
                .any(|candidate| candidate.nodes[&0].position == Some(0))
        );

        assert_table_float_alternate(&lens);
    }

    #[test]
    fn null_value_edits_follow_each_coordinates_declared_scalar_kind()
    -> Result<(), Box<dyn std::error::Error>> {
        for kind in ["integer", "float", "bytes"] {
            let report = graph_null_value_report(kind)?;
            assert!(report.in_domain > 0, "{kind} graph has no admitted edit");
            assert_eq!(report.in_domain_ok, report.in_domain);
        }
        for kind in ["string", "token", "date"] {
            let report = tree_null_value_report(kind)?;
            assert!(report.in_domain > 0, "{kind} tree has no admitted edit");
            assert_eq!(report.in_domain_ok, report.in_domain);
        }
        let scalars = [
            ScalarKind::Bool,
            ScalarKind::Int {
                bits: 64,
                signed: true,
            },
            ScalarKind::BigInt,
            ScalarKind::Float { bits: 64 },
            ScalarKind::Decimal,
            ScalarKind::Str,
            ScalarKind::Bytes(BytesText::Raw),
            ScalarKind::Date,
            ScalarKind::Time,
            ScalarKind::DateTime,
            ScalarKind::Token,
            ScalarKind::Any,
        ];
        for scalar in scalars.into_iter().map(Some).chain(std::iter::once(None)) {
            let report = table_value_report(scalar, Value::Null)?;
            assert!(report.in_domain > 0, "table scalar has no admitted edit");
            assert_eq!(report.in_domain_ok, report.in_domain);
        }
        let unclassified = table_value_report(None, Value::Bytes(vec![1]))?;
        assert!(unclassified.in_domain > 0);
        assert_eq!(unclassified.in_domain_ok, unclassified.in_domain);
        assert!(matches!(
            alternate_flat_value(&Value::Token("x".to_owned()), None),
            Some(Value::Null)
        ));
        Ok(())
    }

    #[test]
    fn absent_optional_table_cell_has_an_admitted_strict_value_edit()
    -> Result<(), Box<dyn std::error::Error>> {
        let (lens, source, ctx) = optional_bool_table_scenario()?;
        let (view, complement) = TableShape::get(&lens, &source, &ctx)?;
        let MutationCases::Cases(candidates) =
            table_cases(&lens, &view, &complement, &ctx, Mutation::ValueEdit)
        else {
            panic!("the absent optional cell must have a ValueEdit candidate");
        };
        assert!(candidates.iter().any(|candidate| {
            matches!(
                candidate.row(RowId(0)).and_then(|row| row.get("a")),
                Some(Value::Bool(false))
            )
        }));
        let Some(report) = report_families::<TableShape>(&lens, &source, &view, &complement, &ctx)
            .0
            .into_iter()
            .find(|family| family.mutation == Mutation::ValueEdit)
        else {
            panic!("ValueEdit was not reported");
        };
        assert!(report.in_domain > 0);
        assert_eq!(report.in_domain_ok, report.in_domain);
        check_put_get_for_shape::<TableShape>(&lens, &source, &ctx)?;
        Ok(())
    }

    #[test]
    fn put_put_uses_strict_metadata_for_non_vacuity_and_source_equality() {
        let schema = three_node_schema();
        let lens = identity_lens(&schema);
        let source = three_node_instance();
        let (view, _) = get(&lens, &source).unwrap_or_else(|error| panic!("get failed: {error}"));
        let mut first = view.clone();
        if let Some(node) = first.nodes.get_mut(&1) {
            node.shape = panproto_inst::NodeShape::List;
        }
        assert!(instances_equivalent(&first, &view));
        assert!(!tree_identical(&first, &view));
        assert!(check_put_put(&lens, &source, &first, &view).is_ok());
    }

    #[test]
    fn put_put_rekeys_an_expansion_complement_to_the_second_view()
    -> Result<(), Box<dyn std::error::Error>> {
        let (lens, source, ax) = tree_expansion_fixture()?;
        let (base, complement) = get(&lens, &source)?;
        assert!(complement.synthesized_nodes.contains(&20));

        let mut first = base;
        first.nodes.insert(21, Node::new(21, "x"));
        first.arcs.insert(0, (7, 21, ax));
        let after_first = crate::asymmetric::put(&lens, &first, &complement)?;
        let (_, complement_after_first) = get(&lens, &after_first)?;
        assert!(complement_after_first.synthesized_nodes.contains(&22));
        assert!(!complement_after_first.synthesized_nodes.contains(&20));

        check_put_put(&lens, &source, &first, &first)?;
        Ok(())
    }

    #[test]
    fn law_report_serializes_with_contract_fields() -> Result<(), serde_json::Error> {
        let report = LawReport {
            ran: vec![FamilyReport {
                mutation: Mutation::ValueEdit,
                in_domain: 1,
                in_domain_ok: 1,
                negative: 0,
                negative_ok: 0,
            }],
            skipped: vec![(Mutation::MalformedFan, SkipReason::ShapeCannotExpress)],
            classification: LensClassification {
                contraction: ContractionMode::None,
                put_put: Some(true),
                insert_put_put: None,
                delete_put_put: Some(false),
                mixed_put_put: None,
                undoable: Some(true),
            },
        };
        assert_eq!(
            serde_json::to_value(&report)?,
            serde_json::json!({
                "ran": [{
                    "mutation": "value_edit",
                    "in_domain": 1,
                    "in_domain_ok": 1,
                    "negative": 0,
                    "negative_ok": 0
                }],
                "skipped": [["malformed_fan", "shape_cannot_express"]],
                "classification": {
                    "contraction": "none",
                    "put_put": true,
                    "insert_put_put": null,
                    "delete_put_put": false,
                    "mixed_put_put": null,
                    "undoable": true
                }
            })
        );
        Ok(())
    }

    #[test]
    fn report_partitions_all_seventeen_families_in_contract_order() {
        let lens = identity_lens(&three_node_schema());
        let (ran, skipped) = report_families::<MockShape>(&lens, &0, &0, &0, &MockMode::Collapse);
        assert_eq!(
            ran.iter().map(|family| family.mutation).collect::<Vec<_>>(),
            vec![Mutation::ValueEdit, Mutation::FieldEdit]
        );
        assert_eq!(
            skipped
                .iter()
                .map(|(mutation, _)| *mutation)
                .collect::<Vec<_>>(),
            Mutation::ALL[2..]
        );
        assert_eq!(ran.len() + skipped.len(), Mutation::ALL.len());
    }

    #[test]
    fn law_shape_default_exhaustive_values_reuses_the_value_edit_basis() {
        let lens = identity_lens(&three_node_schema());
        assert_eq!(
            MockShape::exhaustive_value_cases(&lens, &7, &11, &0, &MockMode::Collapse,),
            vec![12]
        );
    }

    #[test]
    fn exhaustive_value_adapter_preserves_the_underlying_shape_contract() {
        type Adapter = ExhaustiveValueShape<MockShape>;

        let lens = identity_lens(&three_node_schema());
        assert!(matches!(
            Adapter::get(&lens, &7, &MockMode::Collapse),
            Ok((7, 0))
        ));
        assert!(matches!(
            Adapter::put(&lens, &3, &0, &MockMode::CongruenceOnly),
            Ok(3)
        ));
        let Err(domain_errors) = Adapter::check_put_domain(&lens, &1, &0, &MockMode::MixedNegative)
        else {
            panic!("the adapter must preserve the underlying domain refusal");
        };
        assert_eq!(domain_errors.len(), 2);

        let index = SynthIndex::default();
        assert!(Adapter::equivalent(&1, &index, &3, &index));
        assert!(!Adapter::equivalent(&1, &index, &2, &index));
        assert!(Adapter::divergence(&1, &index, &3, &index).is_none());
        assert!(Adapter::divergence(&1, &index, &2, &index).is_some());
        let derived = DerivedMap::new();
        assert!(Adapter::divergence_modulo_derived(&1, &index, &3, &index, &derived).is_none());
        assert!(Adapter::divergence_modulo_derived(&1, &index, &2, &index, &derived).is_some());
        assert!(Adapter::complement_divergence(&4, &4).is_none());
        assert!(Adapter::complement_divergence(&4, &5).is_some());
        assert_eq!(
            Adapter::congruence_witnesses(&5, &index),
            vec![(7, index.clone())]
        );

        let MutationCases::Cases(value_cases) =
            Adapter::mutation_cases(&lens, &7, &11, &0, &MockMode::Collapse, Mutation::ValueEdit)
        else {
            panic!("the exhaustive adapter must preserve value candidates");
        };
        assert_eq!(value_cases, vec![12]);
        let MutationCases::Cases(field_cases) =
            Adapter::mutation_cases(&lens, &7, &11, &0, &MockMode::Collapse, Mutation::FieldEdit)
        else {
            panic!("the exhaustive adapter must preserve non-value candidates");
        };
        assert_eq!(field_cases, vec![13]);
        assert!(Adapter::identical(&6, &6));
        assert!(!Adapter::identical(&6, &7));
        assert_eq!(Adapter::congruent_states(&5, &9), vec![(7, 9)]);
        assert_eq!(Adapter::rekey_complement(&9, &index, &index), 9);
        assert!(
            Adapter::locality_divergence(&lens, &0, &0, &3, &3, &0, &MockMode::Collapse,).is_none()
        );
        assert!(
            Adapter::locality_divergence(&lens, &0, &0, &3, &4, &0, &MockMode::Collapse,).is_some()
        );
        assert_eq!(Adapter::carrier_intersection(&4, &7), 4);
        assert_eq!(Adapter::undo_view(&2, &3), Some(2));
        assert_eq!(Adapter::undo_view(&2, &2), None);

        let schema = one_vertex_schema("boolean", false);
        let scope = SmallScope {
            max_nodes: 1,
            max_edges: 0,
        };
        let instances = ExhaustiveValueShape::<TreeShape>::enumerate_small(&schema, scope, &());
        assert_eq!(instances.len(), 3);
        let value_lens = identity_lens(&schema);
        let source = &instances[0];
        let Ok((view, complement)) = TreeShape::get(&value_lens, source, &()) else {
            panic!("the identity lens must get the finite value fixture");
        };
        let MutationCases::Cases(exhaustive_cases) =
            ExhaustiveValueShape::<TreeShape>::mutation_cases(
                &value_lens,
                source,
                &view,
                &complement,
                &(),
                Mutation::ValueEdit,
            )
        else {
            panic!("the finite value fixture must admit value edits");
        };
        assert_eq!(exhaustive_cases.len(), 2);

        let mut tree_complement = Complement::default();
        tree_complement.synthesized_nodes.insert(1);
        tree_complement.synthesized_keys.insert(
            1,
            panproto_inst::SynthKey::Expand {
                source_edge: EdgeId(0),
                hop: 0,
            },
        );
        let tree = three_node_instance();
        let tree_index = ExhaustiveValueShape::<TreeShape>::synth_index(&tree, &tree_complement);
        assert!(!tree_index.is_empty());
        assert_eq!(tree_index, TreeShape::synth_index(&tree, &tree_complement));
    }

    #[test]
    fn mock_oracles_detect_each_distinct_failure_class() {
        let lens = identity_lens(&three_node_schema());
        assert!(matches!(
            check_totality_for_shape::<MockShape>(&lens, &0, &MockMode::Partial),
            Err(LawViolation::Totality {
                mutation: Mutation::ValueEdit,
                ..
            })
        ));
        assert!(matches!(
            check_put_twice_for_shape::<MockShape>(&lens, &0, &MockMode::Drift),
            Err(LawViolation::PutTwice { .. })
        ));
        assert!(matches!(
            check_put_twice_for_shape::<MockShape>(&lens, &0, &MockMode::MaskedDrift),
            Err(LawViolation::PutTwice { .. })
        ));
        assert!(matches!(
            check_put_injectivity_for_shape::<MockShape>(&lens, &0, &MockMode::Collapse),
            Err(LawViolation::PutInjectivity { .. })
        ));
        assert!(matches!(
            check_locality_for_shape::<MockShape>(&lens, &0, &MockMode::Collapse),
            Err(LawViolation::Locality { .. })
        ));
        assert!(matches!(
            check_congruence_for_shape::<MockShape>(&lens, &0, &MockMode::IdSensitive),
            Err(LawViolation::Congruence { .. })
        ));
    }

    #[test]
    fn gating_returns_the_first_failure_in_the_declared_order() {
        let lens = identity_lens(&three_node_schema());
        let cases = [
            (1, MockMode::Collapse, "get_put"),
            (0, MockMode::Combined, "put_get"),
            (0, MockMode::Partial, "totality"),
            (0, MockMode::Drift, "put_twice"),
            (0, MockMode::CongruenceOnly, "congruence"),
        ];
        for (source, mode, expected) in cases {
            let actual = match check_laws_report::<MockShape>(&lens, &source, &mode) {
                Err(LawViolation::GetPut { .. }) => "get_put",
                Err(LawViolation::PutGet { .. }) => "put_get",
                Err(LawViolation::Totality { .. }) => "totality",
                Err(LawViolation::PutTwice { .. }) => "put_twice",
                Err(LawViolation::Congruence { .. }) => "congruence",
                Err(other) => panic!("unexpected gate error: {other}"),
                Ok(_) => panic!("{expected} fixture did not fail"),
            };
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn put_get_is_strict_under_every_contraction_mode() -> Result<(), Box<dyn std::error::Error>> {
        let direct = Edge {
            src: "r".into(),
            tgt: "leaf".into(),
            kind: "rel".into(),
            name: Some("leaf".into()),
        };
        let identity_schema = graph_schema(&["r", "leaf"], std::slice::from_ref(&direct))?;
        let identity = identity_lens(&identity_schema);
        let identity_source = GInstance::new()
            .with_node(Node::new(0, "r"))
            .with_node(Node::new(2, "leaf"))
            .with_edge_id(EdgeId(0), 0, 2, direct);

        let rm = Edge {
            src: "r".into(),
            tgt: "m".into(),
            kind: "rel".into(),
            name: Some("m".into()),
        };
        let ml = Edge {
            src: "m".into(),
            tgt: "leaf".into(),
            kind: "rel".into(),
            name: Some("leaf".into()),
        };
        let declared = Lens {
            compiled: CompiledMigration {
                surviving_verts: ["r".into(), "leaf".into()].into_iter().collect(),
                ..CompiledMigration::default()
            },
            src_schema: graph_schema(&["r", "m", "leaf"], &[rm.clone(), ml.clone()])?,
            tgt_schema: identity_schema.clone(),
        };
        let declared_source = GInstance::new()
            .with_node(Node::new(0, "r"))
            .with_node(Node::new(1, "m"))
            .with_node(Node::new(2, "leaf"))
            .with_edge_id(EdgeId(0), 0, 1, rm.clone())
            .with_edge_id(EdgeId(1), 1, 2, ml.clone());

        let mm = Edge {
            src: "m".into(),
            tgt: "m".into(),
            kind: "rel".into(),
            name: Some("self".into()),
        };
        let closure = Lens {
            compiled: declared.compiled.clone(),
            src_schema: graph_schema(&["r", "m", "leaf"], &[rm.clone(), mm.clone(), ml.clone()])?,
            tgt_schema: identity_schema,
        };
        let closure_source = GInstance::new()
            .with_node(Node::new(0, "r"))
            .with_node(Node::new(1, "m"))
            .with_node(Node::new(2, "leaf"))
            .with_edge_id(EdgeId(0), 0, 1, rm)
            .with_edge_id(EdgeId(1), 1, 1, mm)
            .with_edge_id(EdgeId(2), 1, 2, ml);

        for (lens, source, expected) in [
            (&identity, &identity_source, ContractionMode::None),
            (&declared, &declared_source, ContractionMode::Declared),
            (&closure, &closure_source, ContractionMode::Closure),
        ] {
            let plan = panproto_inst::RestrictPlan::compile(
                &lens.src_schema,
                &lens.tgt_schema,
                &lens.compiled,
            )?;
            assert_eq!(plan.contraction.mode(), expected);
            assert!(
                check_put_get_for_shape::<GraphShape>(lens, source, &()).is_ok(),
                "strict PutGet failed in {expected:?} mode"
            );
        }
        Ok(())
    }

    #[test]
    fn a_cascading_lens_is_not_undoable() -> Result<(), Box<dyn std::error::Error>> {
        let (lens, source) = cascading_projection_fixture()?;
        let (base, complement) = GraphShape::get(&lens, &source, &())?;
        let classification = classify::<GraphShape>(&lens, &source, &base, &complement, &());
        assert_eq!(classification.undoable, Some(false));
        Ok(())
    }

    #[test]
    fn the_labelled_graph_example_fails_put_put_and_satisfies_mixed_put_put()
    -> Result<(), Box<dyn std::error::Error>> {
        let lens = identity_lens(&three_node_schema());
        let source = map_graph_labels(&jr_ab(), |_, _| Some("f".into()))?;
        let (base, complement) = LabelledGraphJrShape::get(&lens, &source, &())?;
        let classification =
            classify::<LabelledGraphJrShape>(&lens, &source, &base, &complement, &());
        assert_eq!(classification.put_put, Some(false));
        assert_eq!(classification.mixed_put_put, Some(true));

        let after_delete = LabelledGraphJrShape::put(&lens, &jr_a(), &complement, &())?;
        let (_, delete_complement) = LabelledGraphJrShape::get(&lens, &after_delete, &())?;
        let sequential = LabelledGraphJrShape::put(&lens, &base, &delete_complement, &())?;
        let direct = LabelledGraphJrShape::put(&lens, &base, &complement, &())?;
        assert_eq!(
            sequential
                .edge(EdgeId(0))
                .and_then(|edge| edge.edge.name.as_ref())
                .map(AsRef::as_ref),
            Some("e")
        );
        assert_eq!(
            direct
                .edge(EdgeId(0))
                .and_then(|edge| edge.edge.name.as_ref())
                .map(AsRef::as_ref),
            Some("f")
        );
        Ok(())
    }

    #[test]
    fn mixed_classification_uses_the_johnson_rosebrugh_intersection_square() {
        let lens = identity_lens(&three_node_schema());
        let base = BTreeSet::from(['a']);
        assert_eq!(
            classify_mixed_jr::<JrShape>(&lens, &base, &base, &0, &()),
            Some(true)
        );
        let direct = JrShape::put(&lens, &BTreeSet::from(['x']), &0, &());
        let threaded = JrShape::put(&lens, &BTreeSet::from(['x']), &1, &());
        assert!(matches!(direct, Ok(value) if value == BTreeSet::from(['d'])));
        assert!(matches!(threaded, Ok(value) if value == BTreeSet::from(['l'])));
    }

    #[test]
    fn exhaustive_graph_enumeration_has_loops_parallels_cycles_and_fixed_values() {
        let schema = one_vertex_schema("object", true);
        let instances = GraphShape::enumerate_small(
            &schema,
            SmallScope {
                max_nodes: 2,
                max_edges: 2,
            },
            &(),
        );
        assert!(instances.iter().any(|graph| {
            graph.edge_count() == 1
                && graph
                    .edges()
                    .values()
                    .any(|edge| edge.src == 0 && edge.tgt == 0)
        }));
        assert!(instances.iter().any(|graph| {
            graph.edge_count() == 2
                && graph
                    .edges()
                    .values()
                    .all(|edge| edge.src == 0 && edge.tgt == 0)
        }));
        assert!(instances.iter().any(|graph| {
            graph.edge_count() == 2
                && graph
                    .edges()
                    .values()
                    .any(|edge| edge.src == 0 && edge.tgt == 1)
                && graph
                    .edges()
                    .values()
                    .any(|edge| edge.src == 1 && edge.tgt == 0)
        }));
        let one_node = GraphShape::enumerate_small(
            &schema,
            SmallScope {
                max_nodes: 1,
                max_edges: 2,
            },
            &(),
        );
        assert_eq!(
            one_node.len(),
            4,
            "empty, edgeless, one-loop and two-parallel-loop instances"
        );

        let boolean = GraphShape::enumerate_small(
            &one_vertex_schema("boolean", false),
            SmallScope {
                max_nodes: 1,
                max_edges: 0,
            },
            &(),
        );
        assert_eq!(
            boolean.len(),
            4,
            "empty, null, false and true are exhaustive"
        );
        assert!(boolean.iter().any(|graph| {
            graph.node(0).is_some_and(|node| {
                node.value
                    .as_ref()
                    .is_some_and(|value| matches!(value, FieldPresence::Null))
            })
        }));
    }

    #[test]
    fn exhaustive_graph_enumeration_reifies_canonical_fans() {
        let schema = two_label_hyper_schema();
        let instances = GraphShape::enumerate_small(
            &schema,
            SmallScope {
                max_nodes: 4,
                max_edges: 3,
            },
            &(),
        );
        // There are 1 + 2 + 4 + 8 + 16 = 31 ordinary ordered-anchor
        // graphs.  For one fan, summing r(1+c)^2 over anchor words with
        // n=1,2,3 ordinary nodes gives 1+10+54=65.  Two fans fit beside at
        // most two ordinary nodes and within three edges in 1+14=15 ways;
        // three parent-only fans fit beside the sole `r` node in one way.
        let ordinary_count = 31;
        let expected_by_fan_count = [65, 15, 1];
        assert_eq!(
            instances.len(),
            ordinary_count + expected_by_fan_count.iter().sum::<usize>()
        );
        let fans: Vec<_> = instances
            .iter()
            .filter(|graph| {
                graph
                    .nodes()
                    .values()
                    .any(|node| matches!(node.shape, panproto_inst::NodeShape::HyperEdge))
            })
            .collect();
        let actual_by_fan_count = [1, 2, 3].map(|count| {
            fans.iter()
                .filter(|graph| {
                    graph
                        .nodes()
                        .values()
                        .filter(|node| matches!(node.shape, panproto_inst::NodeShape::HyperEdge))
                        .count()
                        == count
                })
                .count()
        });
        assert_eq!(actual_by_fan_count, expected_by_fan_count);
        assert!(fans.iter().any(|graph| {
            graph.edge_count() == 1
                && graph
                    .nodes()
                    .values()
                    .filter(|node| matches!(node.shape, panproto_inst::NodeShape::HyperEdge))
                    .count()
                    == 1
        }));
        assert!(fans.iter().any(|graph| {
            graph.edge_count() == 2
                && graph
                    .nodes()
                    .values()
                    .filter(|node| matches!(node.shape, panproto_inst::NodeShape::HyperEdge))
                    .count()
                    == 1
        }));
        assert!(fans.iter().any(|graph| {
            graph.edge_count() == 3
                && graph
                    .nodes()
                    .values()
                    .filter(|node| matches!(node.shape, panproto_inst::NodeShape::HyperEdge))
                    .count()
                    == 1
        }));
        for fan in fans {
            assert!(fan.edges().values().any(|edge| {
                edge.edge.kind.as_ref() == panproto_inst::HYPER_PARENT_KIND
                    && edge
                        .edge
                        .name
                        .as_ref()
                        .is_some_and(|name| name.as_ref() == "p")
            }));
        }
        let trees = TreeShape::enumerate_small(
            &schema,
            SmallScope {
                max_nodes: 4,
                max_edges: 3,
            },
            &(),
        );
        assert!(trees.iter().all(|tree| {
            tree.nodes
                .values()
                .all(|node| !matches!(node.shape, panproto_inst::NodeShape::HyperEdge))
        }));
    }

    #[test]
    fn exhaustive_tree_and_graph_enumerate_compatible_null_and_scalar_values() {
        let tree = TreeShape::enumerate_small(
            &one_vertex_schema("boolean", false),
            SmallScope {
                max_nodes: 1,
                max_edges: 0,
            },
            &(),
        );
        assert_eq!(tree.len(), 3, "null, false and true rooted trees");

        let number_schema = one_vertex_schema("number", false);
        let number_graphs = GraphShape::enumerate_small(
            &number_schema,
            SmallScope {
                max_nodes: 1,
                max_edges: 0,
            },
            &(),
        );
        assert_eq!(
            number_graphs.len(),
            4,
            "empty plus null, zero and one number nodes"
        );
        assert!(matches!(
            number_graphs[1]
                .node(0)
                .and_then(|node| node.value.as_ref()),
            Some(FieldPresence::Null)
        ));
        let number_trees = TreeShape::enumerate_small(
            &number_schema,
            SmallScope {
                max_nodes: 1,
                max_edges: 0,
            },
            &(),
        );
        assert_eq!(number_trees.len(), 3, "null, zero and one number trees");
        assert!(matches!(
            number_trees[0]
                .nodes
                .get(&0)
                .and_then(|node| node.value.as_ref()),
            Some(FieldPresence::Null)
        ));
    }

    #[test]
    fn exhaustive_table_enumerates_absent_null_and_scalar_cells() {
        let table = Vertex {
            id: "row".into(),
            kind: "entity".into(),
            nsid: None,
        };
        let value = Vertex {
            id: "row.flag".into(),
            kind: "flag_scalar".into(),
            nsid: None,
        };
        let column = Edge {
            src: table.id.clone(),
            tgt: value.id.clone(),
            kind: "column".into(),
            name: Some("flag".into()),
        };
        let schema = Schema {
            vertices: HashMap::from([(table.id.clone(), table), (value.id.clone(), value)]),
            edges: HashMap::from([(column, "column".into())]),
            ..Schema::default()
        };
        let protocol = Protocol {
            value_kinds: BTreeMap::from([("flag_scalar".to_owned(), ScalarKind::Bool)]),
            non_value_kinds: BTreeSet::from(["entity".to_owned()]),
            ..Protocol::default()
        };
        let context = crate::TableContext {
            src: protocol.clone(),
            tgt: protocol,
        };
        let instances = TableShape::enumerate_small(
            &schema,
            SmallScope {
                max_nodes: 1,
                max_edges: 0,
            },
            &context,
        );
        assert_eq!(
            instances.len(),
            5,
            "zero rows plus absent, null, false and true one-row payloads"
        );
        assert!(
            instances
                .iter()
                .any(|instance| { instance.row(RowId(0)).is_some_and(HashMap::is_empty) })
        );
        let cells: Vec<_> = instances
            .iter()
            .filter_map(|instance| instance.row(RowId(0)))
            .filter_map(|row| row.get("flag"))
            .cloned()
            .collect();
        assert_eq!(
            cells,
            vec![Value::Null, Value::Bool(false), Value::Bool(true)]
        );
    }

    #[test]
    fn exhaustive_graph_aliases_filter_the_fixed_alphabet_by_kind() {
        for (kind, expected_count) in [("int64", 4), ("token", 3), ("date", 5)] {
            let instances = GraphShape::enumerate_small(
                &one_vertex_schema(kind, false),
                SmallScope {
                    max_nodes: 1,
                    max_edges: 0,
                },
                &(),
            );
            assert_eq!(instances.len(), expected_count, "wrong {kind} universe");
            assert!(instances.iter().skip(1).all(|graph| {
                graph
                    .node(0)
                    .and_then(|node| node.value.as_ref())
                    .is_some_and(|presence| match presence {
                        FieldPresence::Null => true,
                        FieldPresence::Present(value) => {
                            panproto_inst::attributes::kind_accepts(&kind.into(), value)
                        }
                        FieldPresence::Absent => false,
                    })
            }));
        }
    }

    #[test]
    fn exhaustive_checker_returns_one_report_per_enumerated_source() {
        let schema = one_vertex_schema("boolean", false);
        let scope = SmallScope {
            max_nodes: 1,
            max_edges: 0,
        };
        let sources = TreeShape::enumerate_small(&schema, scope, &());
        assert_eq!(sources.len(), 3);
        let Ok(reports) = check_laws_exhaustive(&identity_lens(&schema), &schema, scope) else {
            panic!("identity lens must pass on every enumerated source");
        };
        assert_eq!(reports.len(), sources.len());
    }

    #[test]
    fn table_exhaustive_wrapper_invokes_the_generic_checker() {
        let protocol = table_protocol();
        let schema = SchemaBuilder::new(&protocol)
            .vertex("row", "record", None)
            .and_then(SchemaBuilder::build);
        let Ok(schema) = schema else {
            panic!("the one-table schema must compile");
        };
        let scope = SmallScope {
            max_nodes: 0,
            max_edges: 0,
        };
        let Ok(reports) = crate::table_lens::check_laws_exhaustive(
            &identity_lens(&schema),
            &schema,
            panproto_inst::BoundaryProtocols {
                src: &protocol,
                tgt: &protocol,
            },
            scope,
        ) else {
            panic!("the empty table instance must satisfy every gate");
        };
        assert_eq!(reports.len(), 1);
    }

    #[test]
    fn reorder_candidates_never_move_synthesized_edges() {
        use panproto_inst::SynthKey;

        let tree = three_node_instance();
        let mut tree_complement = Complement::empty();
        let synthesized_label = tree.arcs[0].2.clone();
        tree_complement
            .contraction_choices
            .insert((0, 1), synthesized_label);
        let MutationCases::Cases(tree_cases) = tree_cases(
            &identity_lens(&three_node_schema()),
            &tree,
            &tree_complement,
            Mutation::Reorder,
        ) else {
            panic!("tree node positions provide reorder candidates");
        };
        assert!(
            tree_cases
                .iter()
                .all(|candidate| candidate.arcs == tree.arcs),
            "an arc pair containing a synthesized carrier must not be reordered"
        );

        let label = relation("a", "b", "ab");
        let Ok(schema) = graph_schema(&["a", "b"], std::slice::from_ref(&label)) else {
            panic!("the graph schema must compile");
        };
        let graph = GInstance::new()
            .with_node(Node::new(0, "a"))
            .with_node(Node::new(1, "b"))
            .with_edge_id(EdgeId(7), 0, 1, label.clone());
        let mut graph = graph;
        assert!(graph.set_edge_position(EdgeId(7), Some(3)).is_ok());
        let mut graph_complement = GraphComplement::empty();
        graph_complement.synthesized_edges.insert(
            EdgeId(7),
            SynthKey::Contract {
                src: 0,
                tgt: 1,
                edge: label,
            },
        );
        let MutationCases::Cases(graph_cases) = graph_cases(
            &identity_lens(&schema),
            &graph,
            &graph_complement,
            Mutation::Reorder,
        ) else {
            panic!("graph node positions provide reorder candidates");
        };
        assert!(graph_cases.iter().all(|candidate| {
            candidate
                .edge(EdgeId(7))
                .is_some_and(|edge| edge.position == Some(3))
        }));
    }

    #[test]
    fn table_locality_pulls_back_table_and_column_renames() -> Result<(), Box<dyn std::error::Error>>
    {
        let protocol = table_protocol();
        let source_column = column("old", "a");
        let target_column = column("new", "b");
        let source_schema = SchemaBuilder::new(&protocol)
            .vertex("old", "record", None)?
            .vertex("old.a", "int", None)?
            .edge("old", "old.a", "prop", Some("a"))?
            .build()?;
        let target_schema = SchemaBuilder::new(&protocol)
            .vertex("new", "record", None)?
            .vertex("new.b", "int", None)?
            .edge("new", "new.b", "prop", Some("b"))?
            .build()?;
        let lens = Lens {
            compiled: CompiledMigration {
                surviving_verts: ["new", "new.b"].into_iter().map(Into::into).collect(),
                surviving_edges: [target_column.clone()].into(),
                vertex_remap: HashMap::from([
                    ("old".into(), "new".into()),
                    ("old.a".into(), "new.b".into()),
                ]),
                edge_remap: HashMap::from([(source_column, target_column)]),
                ..CompiledMigration::default()
            },
            src_schema: source_schema,
            tgt_schema: target_schema,
        };
        let source = FInstance::new().with_rows(
            "old",
            [(RowId(0), HashMap::from([("a".to_owned(), Value::Int(1))]))],
        )?;
        let ctx = crate::TableContext {
            src: protocol.clone(),
            tgt: protocol,
        };
        let (base, complement) = TableShape::get(&lens, &source, &ctx)?;
        let mut edited = base.clone();
        let Some(row) = edited.row_mut(RowId(0)) else {
            panic!("get keeps row 0");
        };
        row.insert("b".to_owned(), Value::Int(2));
        let restored = TableShape::put(&lens, &edited, &complement, &ctx)?;
        assert!(
            table_locality(&lens, &source, &base, &edited, &restored, &complement, &ctx,).is_none()
        );
        Ok(())
    }

    #[test]
    fn table_locality_pulls_back_a_transform_only_column_rename()
    -> Result<(), Box<dyn std::error::Error>> {
        let protocol = table_protocol();
        let source_schema = SchemaBuilder::new(&protocol)
            .vertex("t", "record", None)?
            .vertex("t.old", "int", None)?
            .edge("t", "t.old", "prop", Some("old"))?
            .build()?;
        let target_schema = SchemaBuilder::new(&protocol)
            .vertex("t", "record", None)?
            .vertex("t.new", "int", None)?
            .edge("t", "t.new", "prop", Some("new"))?
            .build()?;
        let lens = Lens {
            compiled: CompiledMigration {
                surviving_verts: std::iter::once("t".into()).collect(),
                field_transforms: HashMap::from([(
                    "t".into(),
                    vec![FieldTransform::RenameField {
                        old_key: "old".to_owned(),
                        new_key: "new".to_owned(),
                    }],
                )]),
                ..CompiledMigration::default()
            },
            src_schema: source_schema,
            tgt_schema: target_schema,
        };
        let source = FInstance::new().with_rows(
            "t",
            [(RowId(0), HashMap::from([("old".to_owned(), Value::Int(1))]))],
        )?;
        let ctx = TableContext {
            src: protocol.clone(),
            tgt: protocol,
        };
        let (base, complement) = TableShape::get(&lens, &source, &ctx)?;
        let mut edited = base.clone();
        let Some(row) = edited.row_mut(RowId(0)) else {
            panic!("transform rename keeps row 0");
        };
        row.insert("new".to_owned(), Value::Int(2));
        let restored = TableShape::put(&lens, &edited, &complement, &ctx)?;
        assert_eq!(
            restored.row(RowId(0)).and_then(|row| row.get("old")),
            Some(&Value::Int(2))
        );
        assert!(
            table_locality(&lens, &source, &base, &edited, &restored, &complement, &ctx).is_none()
        );
        Ok(())
    }

    #[test]
    fn table_locality_symbolically_reverses_only_operational_transform_writers() {
        use std::sync::Arc;

        use panproto_expr::Expr;
        use panproto_gat::CoercionClass;

        let variable = || Expr::Var(Arc::from("value"));
        let apply = FieldTransform::ApplyExpr {
            key: "value".to_owned(),
            expr: variable(),
            inverse: Some(variable()),
            coercion_class: CoercionClass::Iso,
        };
        let compute = FieldTransform::ComputeField {
            target_key: "value".to_owned(),
            expr: variable(),
            inverse: Some(variable()),
            coercion_class: CoercionClass::Iso,
        };
        assert_eq!(
            reverse_table_cell_writer(&[apply], &[], "value"),
            Some("value".to_owned())
        );
        assert_eq!(
            reverse_table_cell_writer(&[compute], &[], "value"),
            Some("value".to_owned())
        );
        let derived = FieldTransform::ApplyExpr {
            key: "value".to_owned(),
            expr: variable(),
            inverse: None,
            coercion_class: CoercionClass::Projection,
        };
        let added = FieldTransform::AddField {
            key: "value".to_owned(),
            value: Value::Int(0),
        };
        assert_eq!(reverse_table_cell_writer(&[derived], &[], "value"), None);
        assert_eq!(reverse_table_cell_writer(&[added], &[], "value"), None);
    }

    #[test]
    fn table_locality_pulls_back_a_foreign_key_rename() -> Result<(), Box<dyn std::error::Error>> {
        let protocol = table_protocol();
        let source_edge = relation("author", "book", "wrote");
        let target_edge = relation("author", "book", "authored");
        let build = |edge: &Edge| {
            SchemaBuilder::new(&protocol)
                .vertex("author", "record", None)?
                .vertex("book", "record", None)?
                .edge(&edge.src, &edge.tgt, &edge.kind, edge.name.as_deref())?
                .build()
        };
        let lens = Lens {
            compiled: CompiledMigration {
                surviving_verts: ["author", "book"].into_iter().map(Into::into).collect(),
                surviving_edges: [target_edge.clone()].into(),
                edge_remap: HashMap::from([(source_edge.clone(), target_edge.clone())]),
                ..CompiledMigration::default()
            },
            src_schema: build(&source_edge)?,
            tgt_schema: build(&target_edge)?,
        };
        let source = FInstance::new()
            .with_rows("author", [(RowId(0), HashMap::new())])?
            .with_rows("book", [(RowId(1), HashMap::new())])?
            .with_foreign_key(source_edge, [(RowId(0), RowId(1))])?;
        let ctx = crate::TableContext {
            src: protocol.clone(),
            tgt: protocol,
        };
        let (base, complement) = TableShape::get(&lens, &source, &ctx)?;
        let mut edited = base.clone();
        edited.remove_foreign_key(&target_edge, RowId(0), RowId(1));
        let restored = TableShape::put(&lens, &edited, &complement, &ctx)?;
        assert!(
            table_locality(&lens, &source, &base, &edited, &restored, &complement, &ctx,).is_none()
        );
        Ok(())
    }

    #[test]
    fn table_locality_detects_an_unedited_same_row_cell_change()
    -> Result<(), Box<dyn std::error::Error>> {
        let protocol = table_protocol();
        let schema = SchemaBuilder::new(&protocol)
            .vertex("t", "record", None)?
            .vertex("t.a", "int", None)?
            .vertex("t.b", "int", None)?
            .edge("t", "t.a", "prop", Some("a"))?
            .edge("t", "t.b", "prop", Some("b"))?
            .build()?;
        let lens = Lens {
            compiled: CompiledMigration {
                surviving_verts: schema.vertices.keys().cloned().collect(),
                surviving_edges: schema.edges.keys().cloned().collect(),
                ..CompiledMigration::default()
            },
            src_schema: schema.clone(),
            tgt_schema: schema,
        };
        let source = FInstance::new().with_rows(
            "t",
            [(
                RowId(0),
                HashMap::from([
                    ("a".to_owned(), Value::Int(1)),
                    ("b".to_owned(), Value::Int(2)),
                ]),
            )],
        )?;
        let mut edited = source.clone();
        let Some(row) = edited.row_mut(RowId(0)) else {
            panic!("fixture has row 0");
        };
        row.insert("a".to_owned(), Value::Int(3));
        let mut collateral = edited.clone();
        let Some(row) = collateral.row_mut(RowId(0)) else {
            panic!("fixture has row 0");
        };
        row.insert("b".to_owned(), Value::Int(4));
        let ctx = crate::TableContext {
            src: protocol.clone(),
            tgt: protocol,
        };
        assert!(
            table_locality(
                &lens,
                &source,
                &source,
                &edited,
                &collateral,
                &TableComplement::empty(),
                &ctx,
            )
            .is_some()
        );
        Ok(())
    }

    #[test]
    fn table_locality_does_not_hide_cell_drift_behind_a_row_relabel()
    -> Result<(), Box<dyn std::error::Error>> {
        let protocol = table_protocol();
        let schema = SchemaBuilder::new(&protocol)
            .vertex("t1", "record", None)?
            .vertex("t1.a", "int", None)?
            .vertex("t2", "record", None)?
            .vertex("t2.a", "int", None)?
            .edge("t1", "t1.a", "prop", Some("a"))?
            .edge("t2", "t2.a", "prop", Some("a"))?
            .build()?;
        let lens = identity_lens(&schema);
        let row = |value| HashMap::from([("a".to_owned(), Value::Int(value))]);
        let source = FInstance::new().with_rows("t1", [(RowId(0), row(1))])?;
        let edited = FInstance::new().with_rows("t2", [(RowId(0), row(1))])?;
        let collateral = FInstance::new().with_rows("t2", [(RowId(0), row(2))])?;
        let ctx = crate::TableContext {
            src: protocol.clone(),
            tgt: protocol,
        };
        let divergence = table_locality(
            &lens,
            &source,
            &source,
            &edited,
            &collateral,
            &TableComplement::empty(),
            &ctx,
        );
        assert!(divergence.is_some_and(|message| message.contains("cell footprint differs")));
        Ok(())
    }

    #[test]
    fn tree_locality_observes_arc_order_as_carrier_content()
    -> Result<(), Box<dyn std::error::Error>> {
        let schema = three_node_schema();
        let lens = identity_lens(&schema);
        let source = three_node_instance();
        let (view, complement) = get(&lens, &source)?;
        let mut reordered = view.clone();
        reordered.arcs.swap(0, 1);

        let missed = tree_locality(&lens, &source, &view, &reordered, &source, &complement);
        assert!(
            missed.is_some_and(|detail| detail.contains("arc footprint differs")),
            "an unchanged source must not satisfy a reorder footprint"
        );
        assert_eq!(
            tree_locality(&lens, &source, &view, &reordered, &reordered, &complement,),
            None,
            "the exact reordered carriers satisfy the footprint"
        );
        Ok(())
    }

    #[test]
    fn negative_mutations_require_their_exact_typed_refusal() {
        let lens = identity_lens(&three_node_schema());
        let (ran, _) = report_families::<MockShape>(&lens, &0, &0, &0, &MockMode::MixedNegative);
        let Some(mixed) = ran
            .iter()
            .find(|family| family.mutation == Mutation::RemoveSynthesized)
        else {
            panic!("the mixed-refusal fixture must run");
        };
        assert_eq!(mixed.negative, 1);
        assert_eq!(mixed.negative_ok, 0);

        assert!(matches!(
            check_totality_for_shape::<MockShape>(&lens, &0, &MockMode::MixedNegative),
            Err(LawViolation::Totality {
                mutation: Mutation::RemoveSynthesized,
                ..
            })
        ));
        assert!(matches!(
            check_totality_for_shape::<MockShape>(&lens, &0, &MockMode::AdmittedNegative),
            Err(LawViolation::Totality {
                mutation: Mutation::RemoveSynthesized,
                ..
            })
        ));
        assert!(check_totality_for_shape::<MockShape>(&lens, &0, &MockMode::ExactNegative).is_ok());
    }

    #[test]
    fn exact_negative_oracle_rejects_every_report_and_raw_put_mismatch() {
        let lens = identity_lens(&three_node_schema());
        let expected = vec![
            PutDomainError::IdCollision {
                elem: ViewRef::Node(1),
            },
            PutDomainError::SynthesizedEdited {
                elem: ViewRef::Node(2),
            },
        ];
        let case = NegativeMutationCase { view: 1, expected };
        assert_eq!(
            exact_negative_oracle::<MockShape>(&lens, &case, &0, &MockMode::ExactTwoNegative,),
            Ok(())
        );

        let empty = NegativeMutationCase {
            view: 1,
            expected: Vec::new(),
        };
        assert!(
            exact_negative_oracle::<MockShape>(&lens, &empty, &0, &MockMode::ExactTwoNegative,)
                .is_err()
        );
        for mode in [
            MockMode::MissingNegative,
            MockMode::ExtraNegative,
            MockMode::ReorderedNegative,
            MockMode::DuplicateNegative,
            MockMode::WrongCoordinateNegative,
            MockMode::MixedNegative,
            MockMode::AdmittedNegative,
            MockMode::WrongRawNegative,
            MockMode::RawSuccessNegative,
            MockMode::RawNonDomainNegative,
        ] {
            assert!(
                exact_negative_oracle::<MockShape>(&lens, &case, &0, &mode).is_err(),
                "faulty exact-negative mode was accepted"
            );
        }
    }

    #[test]
    fn explicit_case_polarity_does_not_depend_on_the_mutation_name() {
        let lens = identity_lens(&three_node_schema());
        let (ran, _) =
            report_families::<MockShape>(&lens, &0, &0, &0, &MockMode::PolarityIndependent);
        let value_edit = ran
            .iter()
            .find(|family| family.mutation == Mutation::ValueEdit)
            .unwrap_or_else(|| panic!("the explicitly negative value-edit case must run"));
        assert_eq!(value_edit.in_domain, 0);
        assert_eq!(value_edit.negative, 1);
        assert_eq!(value_edit.negative_ok, 1);
        assert!(
            check_totality_for_shape::<MockShape>(&lens, &0, &MockMode::PolarityIndependent)
                .is_ok()
        );

        let (exhaustive, _) = report_families::<ExhaustiveValueShape<MockShape>>(
            &lens,
            &0,
            &0,
            &0,
            &MockMode::PolarityIndependent,
        );
        let exhaustive_value_edit = exhaustive
            .iter()
            .find(|family| family.mutation == Mutation::ValueEdit)
            .unwrap_or_else(|| panic!("the exhaustive adapter must preserve negative polarity"));
        assert_eq!(exhaustive_value_edit.in_domain, 0);
        assert_eq!(exhaustive_value_edit.negative, 1);
        assert_eq!(exhaustive_value_edit.negative_ok, 1);
        assert!(
            check_totality_for_shape::<ExhaustiveValueShape<MockShape>>(
                &lens,
                &0,
                &MockMode::PolarityIndependent,
            )
            .is_ok()
        );
    }

    #[test]
    fn contracted_tree_basis_uses_an_isolated_fresh_child() -> Result<(), Box<dyn std::error::Error>>
    {
        use panproto_inst::ViewRef;

        let (lens, source) = contraction_stage_fixture()?;
        let (view, complement) = get(&lens, &source)?;
        let candidates = TreeShape::negative_mutation_cases(
            &lens,
            &source,
            &view,
            &complement,
            &(),
            Mutation::AddContractedEdge,
        );
        assert_eq!(candidates.len(), 1, "one contracted arc gives one witness");
        let candidate = &candidates[0];
        assert!(candidate.view.nodes.contains_key(&3));
        assert!(
            candidate
                .view
                .arcs
                .iter()
                .all(|(parent, child, _)| parent != child)
        );
        assert_eq!((candidate.view.arcs[0].0, candidate.view.arcs[0].1), (0, 3));

        let removed_synthesized = TreeShape::negative_mutation_cases(
            &lens,
            &source,
            &view,
            &complement,
            &(),
            Mutation::RemoveSynthesized,
        );
        assert_eq!(removed_synthesized.len(), 1);
        assert_eq!(
            check_put_domain(&lens, &removed_synthesized[0].view, &complement),
            Err(vec![PutDomainError::SynthesizedEdited {
                elem: ViewRef::Arc {
                    parent: 0,
                    child: 1,
                },
            }])
        );
        assert!(
            TreeShape::negative_mutation_cases(
                &lens,
                &source,
                &view,
                &complement,
                &(),
                Mutation::EditSynthesized,
            )
            .is_empty()
        );

        let Err(errors) = check_put_domain(&lens, &candidate.view, &complement) else {
            panic!("the inserted contracted edge must be refused");
        };
        assert_eq!(
            errors,
            vec![PutDomainError::ContractedEdgeInserted {
                elem: ViewRef::Arc {
                    parent: 0,
                    child: 3,
                },
            }]
        );
        let put = crate::asymmetric::put(&lens, &candidate.view, &complement);
        assert!(matches!(
            put,
            Err(crate::LensError::PutDomain(error))
                if matches!(
                    error.as_ref(),
                    PutDomainError::ContractedEdgeInserted {
                        elem: ViewRef::Arc {
                            parent: 0,
                            child: 3
                        }
                    }
                )
        ));

        let report = check_laws_report::<TreeShape>(&lens, &source, &())?;
        assert_eq!(
            report
                .ran
                .iter()
                .find(|family| family.mutation == Mutation::AddContractedEdge)
                .map(|family| (family.negative, family.negative_ok)),
            Some((1, 1))
        );
        Ok(())
    }

    #[test]
    fn contracted_graph_and_table_bases_emit_exact_singleton_refusals()
    -> Result<(), Box<dyn std::error::Error>> {
        let (lens, tree_source) = contraction_stage_fixture()?;
        let (graph_source, _) = GInstance::from_wtype_embedded(&tree_source, None)?;
        let (graph_view, graph_complement) = GraphShape::get(&lens, &graph_source, &())?;
        let graph_cases = GraphShape::negative_mutation_cases(
            &lens,
            &graph_source,
            &graph_view,
            &graph_complement,
            &(),
            Mutation::AddContractedEdge,
        );
        assert!(!graph_cases.is_empty());
        for case in &graph_cases {
            assert!(matches!(
                case.expected.as_slice(),
                [PutDomainError::ContractedEdgeInserted {
                    elem: ViewRef::Edge(_)
                }]
            ));
            assert_eq!(
                exact_negative_oracle::<GraphShape>(&lens, case, &graph_complement, &(),),
                Ok(())
            );
        }
        let graph_synthesized = GraphShape::negative_mutation_cases(
            &lens,
            &graph_source,
            &graph_view,
            &graph_complement,
            &(),
            Mutation::RemoveSynthesized,
        );
        assert!(!graph_synthesized.is_empty());
        for case in &graph_synthesized {
            assert_eq!(
                exact_negative_oracle::<GraphShape>(&lens, case, &graph_complement, &(),),
                Ok(())
            );
        }

        let ab = tree_source.arcs[0].2.clone();
        let ba = tree_source.arcs[1].2.clone();
        let mut table_source = FInstance::new();
        table_source.insert_row("a", RowId(0), HashMap::new())?;
        table_source.insert_row("a", RowId(1), HashMap::new())?;
        table_source.insert_row("b", RowId(2), HashMap::new())?;
        table_source.insert_foreign_key(ab, RowId(0), RowId(2))?;
        table_source.insert_foreign_key(ba, RowId(2), RowId(1))?;
        let protocol = table_protocol();
        let ctx = TableContext {
            src: protocol.clone(),
            tgt: protocol,
        };
        let (table_view, table_complement) = TableShape::get(&lens, &table_source, &ctx)?;
        let table_cases = TableShape::negative_mutation_cases(
            &lens,
            &table_source,
            &table_view,
            &table_complement,
            &ctx,
            Mutation::AddContractedEdge,
        );
        assert!(!table_cases.is_empty());
        for case in &table_cases {
            assert!(matches!(
                case.expected.as_slice(),
                [PutDomainError::ContractedEdgeInserted {
                    elem: ViewRef::Pair { .. }
                }]
            ));
            assert_eq!(
                exact_negative_oracle::<TableShape>(&lens, case, &table_complement, &ctx,),
                Ok(())
            );
        }
        let table_synthesized = TableShape::negative_mutation_cases(
            &lens,
            &table_source,
            &table_view,
            &table_complement,
            &ctx,
            Mutation::RemoveSynthesized,
        );
        assert!(!table_synthesized.is_empty());
        for case in &table_synthesized {
            assert_eq!(
                exact_negative_oracle::<TableShape>(&lens, case, &table_complement, &ctx,),
                Ok(())
            );
        }
        Ok(())
    }

    #[test]
    fn synthesized_table_pair_edits_cover_both_endpoints_with_exact_refusals()
    -> Result<(), Box<dyn std::error::Error>> {
        let (mut lens, tree_source) = contraction_stage_fixture()?;
        let direct = lens
            .tgt_schema
            .edges
            .keys()
            .next()
            .cloned()
            .ok_or("direct target edge")?;
        lens.src_schema.edges.insert(direct.clone(), "edge".into());
        lens.compiled.surviving_edges.insert(direct);

        let ab = tree_source.arcs[0].2.clone();
        let ba = tree_source.arcs[1].2.clone();
        let mut source = FInstance::new();
        for id in [RowId(0), RowId(1), RowId(3)] {
            source.insert_row("a", id, HashMap::new())?;
        }
        source.insert_row("b", RowId(2), HashMap::new())?;
        source.insert_foreign_key(ab, RowId(0), RowId(2))?;
        source.insert_foreign_key(ba, RowId(2), RowId(1))?;
        let protocol = table_protocol();
        let ctx = TableContext {
            src: protocol.clone(),
            tgt: protocol,
        };
        let (view, complement) = TableShape::get(&lens, &source, &ctx)?;
        let cases = TableShape::negative_mutation_cases(
            &lens,
            &source,
            &view,
            &complement,
            &ctx,
            Mutation::EditSynthesized,
        );
        assert!(cases.len() >= 2);
        let (synth_edge, synth_src, synth_tgt) = complement
            .synthesized_fk_pairs
            .iter()
            .next()
            .cloned()
            .ok_or("synthesized table pair")?;
        let expected = ViewRef::Pair {
            edge: synth_edge.clone(),
            src: synth_src,
            tgt: synth_tgt,
        };
        assert!(cases.iter().any(|case| {
            case.view
                .foreign_keys()
                .into_iter()
                .find_map(|(edge, pairs)| (edge == &synth_edge).then_some(pairs))
                .is_some_and(|pairs| {
                    pairs
                        .iter()
                        .any(|(src, tgt)| *src != synth_src && *tgt == synth_tgt)
                })
        }));
        assert!(cases.iter().any(|case| {
            case.view
                .foreign_keys()
                .into_iter()
                .find_map(|(edge, pairs)| (edge == &synth_edge).then_some(pairs))
                .is_some_and(|pairs| {
                    pairs
                        .iter()
                        .any(|(src, tgt)| *src == synth_src && *tgt != synth_tgt)
                })
        }));
        for case in &cases {
            assert_eq!(
                case.expected,
                vec![PutDomainError::SynthesizedEdited {
                    elem: expected.clone(),
                }]
            );
            assert_eq!(
                exact_negative_oracle::<TableShape>(&lens, case, &complement, &ctx),
                Ok(())
            );
        }
        Ok(())
    }

    #[test]
    fn reserved_graph_node_negative_basis_has_a_nonvacuous_exact_witness()
    -> Result<(), Box<dyn std::error::Error>> {
        let schema = graph_schema(&["a", "b"], &[])?;
        let target = graph_schema(&["a"], &[])?;
        let lens = Lens {
            compiled: CompiledMigration {
                surviving_verts: std::iter::once("a".into()).collect(),
                ..CompiledMigration::default()
            },
            src_schema: schema,
            tgt_schema: target,
        };
        let graph_source = GInstance::new()
            .with_node(Node::new(0, "a"))
            .with_node(Node::new(2, "b"));
        let (graph_view, graph_complement) = GraphShape::get(&lens, &graph_source, &())?;
        let graph_reserved = GraphShape::negative_mutation_cases(
            &lens,
            &graph_source,
            &graph_view,
            &graph_complement,
            &(),
            Mutation::ReservedIdReuse,
        );
        assert_eq!(graph_reserved.len(), 1);
        assert_eq!(
            exact_negative_oracle::<GraphShape>(&lens, &graph_reserved[0], &graph_complement, &(),),
            Ok(())
        );
        let mut graph_without_residue = graph_complement;
        graph_without_residue.dropped_nodes.remove(&2);
        assert_eq!(
            GraphShape::check_put_domain(
                &lens,
                &graph_reserved[0].view,
                &graph_without_residue,
                &(),
            ),
            Ok(())
        );
        let graph_restored =
            GraphShape::put(&lens, &graph_reserved[0].view, &graph_without_residue, &())?;
        let (graph_regotten, graph_regotten_complement) =
            GraphShape::get(&lens, &graph_restored, &())?;
        assert!(GraphShape::equivalent(
            &graph_reserved[0].view,
            &GraphShape::synth_index(&graph_reserved[0].view, &graph_without_residue),
            &graph_regotten,
            &GraphShape::synth_index(&graph_regotten, &graph_regotten_complement),
        ));
        Ok(())
    }

    #[test]
    fn reserved_graph_node_basis_omits_indirect_required_value_collateral()
    -> Result<(), Box<dyn std::error::Error>> {
        let field = Edge {
            src: "a".into(),
            tgt: "a.f".into(),
            kind: "prop".into(),
            name: Some("f".into()),
        };
        let mut source_schema =
            graph_schema(&["a", "a.f", "dropped"], std::slice::from_ref(&field))?;
        source_schema.constraints.insert(
            "a.f".into(),
            vec![panproto_schema::Constraint {
                sort: "required".into(),
                value: "true".to_owned(),
            }],
        );
        let target_schema = graph_schema(&["a", "a.f"], std::slice::from_ref(&field))?;
        let lens = Lens {
            compiled: CompiledMigration {
                surviving_verts: ["a".into(), "a.f".into()].into(),
                surviving_edges: std::iter::once(field.clone()).collect(),
                ..CompiledMigration::default()
            },
            src_schema: source_schema,
            tgt_schema: target_schema,
        };
        let source = GInstance::new()
            .with_node(Node::new(0, "a"))
            .with_node(
                Node::new(1, "a.f")
                    .with_value(FieldPresence::Present(Value::Str("present".to_owned()))),
            )
            .with_node(Node::new(2, "dropped"))
            .with_edge_id(EdgeId(3), 0, 1, field);
        let (view, complement) = GraphShape::get(&lens, &source, &())?;
        let candidate = node_for_anchor(2, &lens.tgt_schema, &"a".into());
        assert!(!lens.src_schema.constraints.contains_key("a"));
        assert!(!validation_neutral_target_anchor(&lens, &"a".into()));
        assert!(
            !reserved_graph_node_witness_is_structurally_exact(
                &lens,
                &view,
                &complement,
                &candidate,
            ),
            "a constraint reached through a value path is collateral to the collision"
        );
        let cases = GraphShape::negative_mutation_cases(
            &lens,
            &source,
            &view,
            &complement,
            &(),
            Mutation::ReservedIdReuse,
        );
        assert!(cases.iter().all(|case| {
            case.expected
                != vec![PutDomainError::IdCollision {
                    elem: ViewRef::Node(2),
                }]
        }));

        let mut replacement = view;
        replacement.insert_node(candidate)?;
        assert_eq!(
            GraphShape::check_put_domain(&lens, &replacement, &complement, &()),
            Err(vec![PutDomainError::IdCollision {
                elem: ViewRef::Node(2),
            }])
        );
        let mut stripped = complement;
        assert!(stripped.dropped_nodes.remove(&2).is_some());
        assert_eq!(
            GraphShape::check_put_domain(&lens, &replacement, &stripped, &()),
            Ok(())
        );
        let restored = GraphShape::put(&lens, &replacement, &stripped, &())?;
        let mut protocol = Protocol::default();
        protocol.constraint_sorts.push("required".to_owned());
        protocol.constraint_semantics.insert(
            "required".to_owned(),
            panproto_schema::ConstraintSemantics::RequiredValue,
        );
        let report = panproto_inst::validate_graph(&restored, &lens.src_schema, &protocol);
        assert_eq!(
            report.status,
            panproto_gat::CheckStatus::Failed,
            "{report:#?}"
        );
        assert!(report.diagnostics.iter().any(|diagnostic| {
            diagnostic.check.kind == panproto_inst::CheckKind::RequiredValue
                && diagnostic.subject.as_ref() == Some(&ViewRef::Node(2))
                && matches!(
                    &diagnostic.violation,
                    panproto_inst::Violation::MissingValue { field }
                        if field.as_deref() == Some("f")
                )
        }));
        Ok(())
    }

    #[test]
    fn reserved_graph_node_basis_omits_nullable_reference_collateral()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut source_schema = graph_schema(&["a", "b", "dropped"], &[])?;
        source_schema
            .vertices
            .get_mut("a")
            .ok_or("source holder vertex")?
            .kind = "string".into();
        insert_nonnullable_reference(&mut source_schema);
        source_schema
            .declarations
            .references
            .get_mut("required-ref")
            .ok_or("source reference")?
            .nullable = true;
        let mut target_schema = graph_schema(&["a", "b"], &[])?;
        target_schema
            .vertices
            .get_mut("a")
            .ok_or("target holder vertex")?
            .kind = "string".into();
        let lens = Lens {
            compiled: CompiledMigration {
                surviving_verts: ["a".into(), "b".into()].into(),
                ..CompiledMigration::default()
            },
            src_schema: source_schema,
            tgt_schema: target_schema,
        };
        let source = GInstance::new()
            .with_node(Node::new(0, "a"))
            .with_node(Node::new(2, "dropped"));
        let (view, complement) = GraphShape::get(&lens, &source, &())?;
        let candidate = node_for_anchor(2, &lens.tgt_schema, &"a".into());
        assert!(matches!(
            &candidate.value,
            Some(FieldPresence::Present(Value::Str(value))) if value.is_empty()
        ));
        assert!(
            !reserved_graph_node_witness_is_structurally_exact(
                &lens,
                &view,
                &complement,
                &candidate,
            ),
            "a nullable reference becomes non-null on the scalar default"
        );
        let cases = GraphShape::negative_mutation_cases(
            &lens,
            &source,
            &view,
            &complement,
            &(),
            Mutation::ReservedIdReuse,
        );
        assert!(cases.iter().all(|case| {
            case.expected
                != vec![PutDomainError::IdCollision {
                    elem: ViewRef::Node(2),
                }]
        }));

        let mut replacement = view;
        replacement.insert_node(candidate)?;
        assert_eq!(
            GraphShape::check_put_domain(&lens, &replacement, &complement, &()),
            Err(vec![PutDomainError::IdCollision {
                elem: ViewRef::Node(2),
            }])
        );
        let mut stripped = complement;
        assert!(stripped.dropped_nodes.remove(&2).is_some());
        assert_eq!(
            GraphShape::check_put_domain(&lens, &replacement, &stripped, &()),
            Ok(())
        );
        let restored = GraphShape::put(&lens, &replacement, &stripped, &())?;
        let protocol = Protocol {
            syntactic_shape: panproto_schema::InstanceShapeKind::Graph,
            semantic_shape: panproto_schema::InstanceShapeKind::Graph,
            ..Protocol::default()
        };
        let report = panproto_inst::validate_graph(&restored, &lens.src_schema, &protocol);
        assert_eq!(
            report.status,
            panproto_gat::CheckStatus::Failed,
            "{report:#?}"
        );
        assert!(report.diagnostics.iter().any(|diagnostic| {
            diagnostic.check.kind == panproto_inst::CheckKind::ReferenceResolves
                && diagnostic.subject.as_ref() == Some(&ViewRef::Node(2))
                && matches!(
                    &diagnostic.violation,
                    panproto_inst::Violation::DanglingReference { reference, key }
                        if reference.as_ref() == "required-ref"
                            && matches!(key.as_slice(), [Value::Str(value)] if value.is_empty())
                )
        }));
        Ok(())
    }

    #[test]
    fn reserved_graph_node_basis_omits_endpoint_compatible_incident_residue()
    -> Result<(), Box<dyn std::error::Error>> {
        let (lens, tree_source) = contraction_stage_fixture()?;
        let (source, _) = GInstance::from_wtype_embedded(&tree_source, None)?;
        let (view, complement) = GraphShape::get(&lens, &source, &())?;
        assert!(
            complement
                .dropped_edges
                .values()
                .any(|edge| edge.src == 2 || edge.tgt == 2),
            "the reserved node must retain incident contraction residue"
        );
        let anchor = lens
            .tgt_schema
            .vertices
            .keys()
            .find(|anchor| validation_neutral_target_anchor(&lens, anchor))
            .ok_or("validation-neutral target anchor")?;
        let candidate = node_for_anchor(2, &lens.tgt_schema, anchor);
        assert!(
            !reserved_graph_node_witness_is_structurally_exact(
                &lens,
                &view,
                &complement,
                &candidate,
            ),
            "incident dropped-edge residue is outside the structural proof"
        );
        let cases = GraphShape::negative_mutation_cases(
            &lens,
            &source,
            &view,
            &complement,
            &(),
            Mutation::ReservedIdReuse,
        );
        assert!(cases.iter().all(|case| {
            case.expected
                != vec![PutDomainError::IdCollision {
                    elem: ViewRef::Node(2),
                }]
        }));
        let mut replacement = view;
        replacement.insert_node(candidate)?;
        assert_eq!(
            GraphShape::check_put_domain(&lens, &replacement, &complement, &()),
            Err(vec![PutDomainError::IdCollision {
                elem: ViewRef::Node(2),
            }])
        );
        let mut stripped = complement;
        assert!(stripped.dropped_nodes.remove(&2).is_some());
        assert_eq!(
            GraphShape::check_put_domain(&lens, &replacement, &stripped, &()),
            Ok(())
        );
        let restored = GraphShape::put(&lens, &replacement, &stripped, &())?;
        assert_eq!(
            panproto_inst::validate_graph(&restored, &lens.src_schema, &Protocol::default()).status,
            panproto_gat::CheckStatus::Failed,
            "the checker-admitted counterfactual violates the source schema"
        );
        Ok(())
    }

    #[test]
    fn reserved_graph_node_basis_rejects_a_fully_dropped_fan_replacement()
    -> Result<(), Box<dyn std::error::Error>> {
        let source_schema = two_label_hyper_schema();
        let target_schema = graph_schema(&["r", "c"], &[])?;
        let lens = Lens {
            compiled: CompiledMigration {
                surviving_verts: ["r".into(), "c".into()].into(),
                ..CompiledMigration::default()
            },
            src_schema: source_schema,
            tgt_schema: target_schema,
        };
        let mut fan = Node::new(2, "h");
        fan.shape = panproto_inst::NodeShape::HyperEdge;
        let source = GInstance::new()
            .with_node(Node::new(0, "r"))
            .with_node(Node::new(1, "c"))
            .with_node(fan)
            .with_edge_id(
                EdgeId(3),
                2,
                0,
                Edge {
                    src: "h".into(),
                    tgt: "r".into(),
                    kind: panproto_inst::HYPER_PARENT_KIND.into(),
                    name: Some("p".into()),
                },
            );
        let (view, complement) = GraphShape::get(&lens, &source, &())?;
        assert!(complement.dropped_nodes.contains_key(&2));
        assert_eq!(complement.dropped_edges.len(), 1);

        let mut replacement = view.clone();
        replacement.insert_node(Node::new(2, "r"))?;
        assert_eq!(
            GraphShape::check_put_domain(&lens, &replacement, &complement, &()),
            Err(vec![
                PutDomainError::IdCollision {
                    elem: ViewRef::Node(2),
                },
                PutDomainError::MalformedFan {
                    node: 2,
                    reason: FanViolation::ReservedKindOutsideFan { edge: EdgeId(3) },
                },
            ])
        );
        let mut stripped = complement.clone();
        assert!(stripped.dropped_nodes.remove(&2).is_some());
        assert_eq!(
            GraphShape::check_put_domain(&lens, &replacement, &stripped, &()),
            Err(vec![PutDomainError::MalformedFan {
                node: 2,
                reason: FanViolation::ReservedKindOutsideFan { edge: EdgeId(3) },
            }])
        );
        assert!(
            GraphShape::negative_mutation_cases(
                &lens,
                &source,
                &view,
                &complement,
                &(),
                Mutation::ReservedIdReuse,
            )
            .iter()
            .all(|case| {
                case.expected
                    != vec![PutDomainError::IdCollision {
                        elem: ViewRef::Node(2),
                    }]
            }),
            "a mixed collision/fan failure is not a singleton reserved-id basis case"
        );
        Ok(())
    }

    #[test]
    fn reserved_graph_edge_basis_omits_reserved_fan_kinds() -> Result<(), Box<dyn std::error::Error>>
    {
        for kind in [
            panproto_inst::HYPER_PARENT_KIND,
            panproto_inst::HYPER_LABEL_KIND,
        ] {
            let (lens, tree_source, kept) = reserved_fan_kind_collision_fixture(kind)?;
            let (source, _) = GInstance::from_wtype_embedded(&tree_source, None)?;
            let (view, complement) = GraphShape::get(&lens, &source, &())?;
            let Some((&reserved, _)) = complement.dropped_edges.iter().next() else {
                panic!("the fixture must reserve its dropped edge id");
            };

            let mut unguarded = view.clone();
            unguarded.insert_edge(reserved, GEdge::new(0, 0, kept))?;
            assert_eq!(
                GraphShape::check_put_domain(&lens, &unguarded, &complement, &()),
                Err(vec![
                    PutDomainError::IdCollision {
                        elem: ViewRef::Edge(reserved),
                    },
                    PutDomainError::UnrepresentableViewEdge {
                        elem: ViewRef::Edge(reserved),
                    },
                ]),
                "an unguarded {kind} collision has reserved-vocabulary collateral",
            );
            assert!(
                GraphShape::negative_mutation_cases(
                    &lens,
                    &source,
                    &view,
                    &complement,
                    &(),
                    Mutation::ReservedIdReuse,
                )
                .iter()
                .all(|case| {
                    case.expected
                        != vec![PutDomainError::IdCollision {
                            elem: ViewRef::Edge(reserved),
                        }]
                }),
                "the singleton basis must omit {kind} edge collisions",
            );
        }
        Ok(())
    }

    #[test]
    fn reserved_edge_basis_omits_ordinary_labels_with_reserved_source_preimages()
    -> Result<(), Box<dyn std::error::Error>> {
        for kind in [
            panproto_inst::HYPER_PARENT_KIND,
            panproto_inst::HYPER_LABEL_KIND,
        ] {
            let (lens, tree_source, target_label) =
                source_reserved_fan_kind_collision_fixture(kind)?;

            let (graph_source, _) = GInstance::from_wtype_embedded(&tree_source, None)?;
            let (graph_view, graph_complement) = GraphShape::get(&lens, &graph_source, &())?;
            let Some((&reserved, _)) = graph_complement.dropped_edges.iter().next() else {
                panic!("the fixture must reserve its dropped edge id");
            };
            let mut unguarded_graph = graph_view.clone();
            unguarded_graph.insert_edge(reserved, GEdge::new(0, 0, target_label.clone()))?;
            assert_eq!(
                GraphShape::check_put_domain(&lens, &unguarded_graph, &graph_complement, &(),),
                Err(vec![
                    PutDomainError::IdCollision {
                        elem: ViewRef::Edge(reserved),
                    },
                    PutDomainError::MalformedFan {
                        node: 0,
                        reason: FanViolation::ReservedKindOutsideFan { edge: reserved },
                    },
                ]),
                "an unguarded ordinary graph edge restores a {kind} source edge",
            );
            assert!(
                GraphShape::negative_mutation_cases(
                    &lens,
                    &graph_source,
                    &graph_view,
                    &graph_complement,
                    &(),
                    Mutation::ReservedIdReuse,
                )
                .iter()
                .all(|case| {
                    case.expected
                        != vec![PutDomainError::IdCollision {
                            elem: ViewRef::Edge(reserved),
                        }]
                })
            );

            let (tree_view, tree_complement) = get(&lens, &tree_source)?;
            let mut nodes = tree_view.nodes.clone();
            nodes.insert(2, node_for_anchor(2, &lens.tgt_schema, &"a".into()));
            let unguarded_tree =
                rebuild_tree(&tree_view, nodes, vec![(0, 2, target_label.clone())]);
            assert_eq!(
                TreeShape::check_put_domain(&lens, &unguarded_tree, &tree_complement, &()),
                Err(vec![
                    PutDomainError::IdCollision {
                        elem: ViewRef::Node(2),
                    },
                    PutDomainError::MalformedFan {
                        node: 0,
                        reason: FanViolation::ReservedKindOutsideFan { edge: EdgeId(0) },
                    },
                ]),
                "the complete report retains the restored {kind} source-arc refusal",
            );
            let stripped = without_reserved_tree_residue(tree_complement.clone(), 2);
            assert_eq!(
                TreeShape::check_put_domain(&lens, &unguarded_tree, &stripped, &()),
                Err(vec![PutDomainError::MalformedFan {
                    node: 0,
                    reason: FanViolation::ReservedKindOutsideFan { edge: EdgeId(0) },
                }]),
                "without the collision, the restored {kind} source arc is malformed",
            );
            assert!(
                TreeShape::negative_mutation_cases(
                    &lens,
                    &tree_source,
                    &tree_view,
                    &tree_complement,
                    &(),
                    Mutation::ReservedIdReuse,
                )
                .iter()
                .all(|case| {
                    case.expected
                        != vec![PutDomainError::IdCollision {
                            elem: ViewRef::Node(2),
                        }]
                })
            );
        }
        Ok(())
    }

    #[test]
    fn reserved_graph_edge_basis_rejects_fan_shaped_endpoints()
    -> Result<(), Box<dyn std::error::Error>> {
        let ordinary = relation("a", "r", "ordinary");
        let dropped = relation("r", "b", "dropped");
        let hyper = HyperEdge {
            id: "a".into(),
            kind: "rel".into(),
            signature: HashMap::from([("p".into(), "r".into())]),
            parent_label: "p".into(),
        };
        let mut source_schema =
            graph_schema(&["a", "r", "b"], &[ordinary.clone(), dropped.clone()])?;
        source_schema
            .hyper_edges
            .insert(hyper.id.clone(), hyper.clone());
        let mut target_schema = graph_schema(&["a", "r"], std::slice::from_ref(&ordinary))?;
        target_schema.hyper_edges.insert(hyper.id.clone(), hyper);
        let lens = Lens {
            compiled: CompiledMigration {
                surviving_verts: ["a".into(), "r".into()].into(),
                surviving_edges: std::iter::once(ordinary.clone()).collect(),
                ..CompiledMigration::default()
            },
            src_schema: source_schema,
            tgt_schema: target_schema,
        };
        let parent = Edge {
            src: "a".into(),
            tgt: "r".into(),
            kind: panproto_inst::HYPER_PARENT_KIND.into(),
            name: Some("p".into()),
        };
        let view = GInstance::new()
            .with_node(Node::new(0, "a").with_shape(panproto_inst::NodeShape::HyperEdge))
            .with_node(Node::new(1, "r"))
            .with_edge_id(EdgeId(5), 0, 1, parent);
        let reserved = EdgeId(7);
        let mut complement = GraphComplement::empty();
        complement.dropped_nodes.insert(2, Node::new(2, "b"));
        complement
            .dropped_edges
            .insert(reserved, GEdge::new(1, 2, dropped));

        let plan = panproto_inst::RestrictPlan::compile(
            &lens.src_schema,
            &lens.tgt_schema,
            &lens.compiled,
        )?;
        assert!(!graph_edge_is_fan_neutral(&plan, &view, 0, 1, &ordinary,));
        let mut unguarded = view.clone();
        unguarded.insert_edge(reserved, GEdge::new(0, 1, ordinary))?;
        let Err(errors) = GraphShape::check_put_domain(&lens, &unguarded, &complement, &()) else {
            panic!("the fan-incident collision must be refused");
        };
        assert!(matches!(
            errors.first(),
            Some(PutDomainError::IdCollision {
                elem: ViewRef::Edge(id),
            }) if *id == reserved
        ));
        assert!(errors.iter().any(|error| matches!(
            error,
            PutDomainError::MalformedFan {
                node: 0,
                reason: FanViolation::ForeignIncidentEdge { edge },
            } if *edge == reserved
        )));
        assert!(
            GraphShape::negative_mutation_cases(
                &lens,
                &GInstance::new(),
                &view,
                &complement,
                &(),
                Mutation::ReservedIdReuse,
            )
            .iter()
            .all(|case| {
                case.expected
                    != vec![PutDomainError::IdCollision {
                        elem: ViewRef::Edge(reserved),
                    }]
            })
        );
        Ok(())
    }

    #[test]
    fn reserved_graph_edge_basis_omits_indirect_key_collateral()
    -> Result<(), Box<dyn std::error::Error>> {
        let pa = relation("p", "a", "pa");
        let ab = relation("a", "b", "ab");
        let hidden = relation("a", "b", "hidden");
        let mut source_schema =
            graph_schema(&["p", "a", "b"], &[pa.clone(), ab.clone(), hidden.clone()])?;
        source_schema
            .vertices
            .get_mut("b")
            .ok_or("source key value vertex")?
            .kind = "string".into();
        source_schema.declarations.keys.insert(
            "p".into(),
            vec![Key {
                id: "indirect".into(),
                parts: vec![KeyPart::Path(vec![pa.clone(), ab.clone()])],
                role: KeyRole::Unique,
                scope: KeyScope::Global,
            }],
        );
        let mut target_schema = graph_schema(&["p", "a", "b"], &[pa.clone(), ab.clone()])?;
        target_schema
            .vertices
            .get_mut("b")
            .ok_or("target key value vertex")?
            .kind = "string".into();
        let lens = Lens {
            compiled: CompiledMigration {
                surviving_verts: ["p".into(), "a".into(), "b".into()].into(),
                surviving_edges: [pa.clone(), ab.clone()].into(),
                ..CompiledMigration::default()
            },
            src_schema: source_schema,
            tgt_schema: target_schema,
        };
        let source = GInstance::new()
            .with_node(Node::new(10, "p"))
            .with_node(Node::new(11, "p"))
            .with_node(Node::new(20, "a"))
            .with_node(Node::new(21, "a"))
            .with_node(
                Node::new(30, "b")
                    .with_value(FieldPresence::Present(Value::Str("duplicate".to_owned()))),
            )
            .with_node(
                Node::new(31, "b")
                    .with_value(FieldPresence::Present(Value::Str("duplicate".to_owned()))),
            )
            .with_edge_id(EdgeId(1), 10, 20, pa.clone())
            .with_edge_id(EdgeId(2), 11, 21, pa)
            .with_edge_id(EdgeId(3), 21, 31, ab.clone())
            .with_edge_id(EdgeId(70), 20, 30, hidden);
        let (view, complement) = GraphShape::get(&lens, &source, &())?;
        assert!(complement.dropped_edges.contains_key(&EdgeId(70)));
        let mut unkeyed = cloned_lens(&lens);
        unkeyed.src_schema.declarations.keys.clear();
        assert!(validation_neutral_target_anchor(&unkeyed, &"a".into()));
        assert!(validation_neutral_target_anchor(&unkeyed, &"b".into()));
        assert!(!validation_neutral_target_anchor(&lens, &"a".into()));
        assert!(
            GraphShape::negative_mutation_cases(
                &lens,
                &source,
                &view,
                &complement,
                &(),
                Mutation::ReservedIdReuse,
            )
            .is_empty(),
            "a key on a parent vertex is collateral to the reserved edge collision"
        );

        let mut replacement = view;
        replacement.insert_edge(EdgeId(70), GEdge::new(20, 30, ab))?;
        let Err(errors) = GraphShape::check_put_domain(&lens, &replacement, &complement, &())
        else {
            panic!("the collision and its indirect duplicate key must both be reported");
        };
        assert!(matches!(
            errors.first(),
            Some(PutDomainError::IdCollision {
                elem: ViewRef::Edge(EdgeId(70)),
            })
        ));
        assert!(errors.iter().any(|error| matches!(
            error,
            PutDomainError::DuplicateKey { table, elems, .. }
                if table.as_ref() == "p"
                    && elems.as_slice() == [ViewRef::Node(10), ViewRef::Node(11)]
        )));
        Ok(())
    }

    #[test]
    fn reserved_graph_edge_negative_basis_has_a_nonvacuous_exact_witness()
    -> Result<(), Box<dyn std::error::Error>> {
        let (edge_lens, edge_tree_source) = reserved_insert_fixture()?;
        let (edge_graph_source, _) = GInstance::from_wtype_embedded(&edge_tree_source, None)?;
        let (edge_view, edge_complement) = GraphShape::get(&edge_lens, &edge_graph_source, &())?;
        let edge_reserved = GraphShape::negative_mutation_cases(
            &edge_lens,
            &edge_graph_source,
            &edge_view,
            &edge_complement,
            &(),
            Mutation::ReservedIdReuse,
        );
        let edge_case = edge_reserved
            .iter()
            .find(|case| {
                matches!(
                    case.expected.as_slice(),
                    [PutDomainError::IdCollision {
                        elem: ViewRef::Edge(_)
                    }]
                )
            })
            .ok_or("reserved graph edge witness")?;
        assert_eq!(
            exact_negative_oracle::<GraphShape>(&edge_lens, edge_case, &edge_complement, &(),),
            Ok(())
        );
        let ViewRef::Edge(reserved_edge) = (match &edge_case.expected[0] {
            PutDomainError::IdCollision { elem } => elem,
            _ => unreachable!(),
        }) else {
            unreachable!()
        };
        let mut edge_without_residue = edge_complement;
        edge_without_residue.dropped_edges.remove(reserved_edge);
        assert_eq!(
            GraphShape::check_put_domain(&edge_lens, &edge_case.view, &edge_without_residue, &(),),
            Ok(())
        );
        let edge_restored =
            GraphShape::put(&edge_lens, &edge_case.view, &edge_without_residue, &())?;
        assert_eq!(
            panproto_inst::validate_graph(
                &edge_restored,
                &edge_lens.src_schema,
                &Protocol::default(),
            )
            .status,
            panproto_gat::CheckStatus::Passed,
        );
        let (edge_regotten, edge_regotten_complement) =
            GraphShape::get(&edge_lens, &edge_restored, &())?;
        assert!(GraphShape::equivalent(
            &edge_case.view,
            &GraphShape::synth_index(&edge_case.view, &edge_without_residue),
            &edge_regotten,
            &GraphShape::synth_index(&edge_regotten, &edge_regotten_complement),
        ));
        Ok(())
    }

    fn reserved_graph_node_isolation_fixture()
    -> Result<(Lens, Node, GraphComplement), Box<dyn std::error::Error>> {
        let reserved_node = 7;
        let lens = Lens {
            compiled: CompiledMigration {
                surviving_verts: std::iter::once("a".into()).collect(),
                ..CompiledMigration::default()
            },
            src_schema: graph_schema(&["a", "b"], &[])?,
            tgt_schema: graph_schema(&["a"], &[])?,
        };
        let candidate = node_for_anchor(reserved_node, &lens.tgt_schema, &"a".into());
        let mut isolated = GraphComplement::empty();
        isolated
            .dropped_nodes
            .insert(reserved_node, Node::new(reserved_node, "b"));
        Ok((lens, candidate, isolated))
    }

    #[test]
    fn reserved_graph_node_isolation_closes_over_synthesized_key_carriers()
    -> Result<(), Box<dyn std::error::Error>> {
        use panproto_inst::{ResolutionTextId, ResolutionValueSite};

        let reserved_node = 7;
        let (lens, candidate, isolated) = reserved_graph_node_isolation_fixture()?;
        assert!(reserved_graph_node_witness_is_structurally_exact(
            &lens,
            &GInstance::new(),
            &isolated,
            &candidate,
        ));
        let node_keys = [
            SynthKey::Contract {
                src: reserved_node,
                tgt: 9,
                edge: relation("a", "b", "ab"),
            },
            SynthKey::ContractExpand {
                src: 9,
                tgt: reserved_node,
                hop: 0,
            },
            SynthKey::ResolutionReference {
                holder: ViewRef::Node(reserved_node),
                reference: "reference".into(),
                index: 0,
            },
            SynthKey::ResolutionSpan {
                text: ResolutionTextId::Local(ResolutionValueSite::Own(ViewRef::Node(
                    reserved_node,
                ))),
                start: 0,
                end: 1,
            },
        ];
        for key in node_keys {
            let mut as_node_value = isolated.clone();
            as_node_value.synthesized_nodes.insert(100, key.clone());
            assert!(!reserved_graph_node_witness_is_structurally_exact(
                &lens,
                &GInstance::new(),
                &as_node_value,
                &candidate,
            ));

            let mut as_edge_value = isolated.clone();
            as_edge_value.synthesized_edges.insert(EdgeId(100), key);
            assert!(!reserved_graph_node_witness_is_structurally_exact(
                &lens,
                &GInstance::new(),
                &as_edge_value,
                &candidate,
            ));
        }
        Ok(())
    }

    #[test]
    fn reserved_graph_edge_isolation_closes_over_synthesized_key_carriers() {
        use panproto_inst::{ResolutionTextId, ResolutionValueSite};

        let reserved_edge = EdgeId(11);
        let edge_keys = [
            SynthKey::Expand {
                source_edge: reserved_edge,
                hop: 0,
            },
            SynthKey::ResolutionReference {
                holder: ViewRef::Edge(reserved_edge),
                reference: "reference".into(),
                index: 0,
            },
            SynthKey::ResolutionSpan {
                text: ResolutionTextId::ExternalKey(ResolutionValueSite::Field {
                    holder: ViewRef::Edge(reserved_edge),
                    field: "text".to_owned(),
                }),
                start: 0,
                end: 1,
            },
        ];

        for key in edge_keys {
            let mut as_node_value = GraphComplement::empty();
            as_node_value.synthesized_nodes.insert(100, key.clone());
            assert!(!isolated_reserved_graph_edge(&as_node_value, reserved_edge));

            let mut as_edge_value = GraphComplement::empty();
            as_edge_value.synthesized_edges.insert(EdgeId(100), key);
            assert!(!isolated_reserved_graph_edge(&as_edge_value, reserved_edge));
        }
    }

    #[test]
    fn reserved_graph_node_isolation_rejects_an_incident_expand_carrier()
    -> Result<(), Box<dyn std::error::Error>> {
        let reserved_node = 7;
        let reserved_edge = EdgeId(11);
        let (lens, candidate, mut incident_expand) = reserved_graph_node_isolation_fixture()?;

        incident_expand.dropped_edges.insert(
            reserved_edge,
            GEdge::new(reserved_node, 9, relation("a", "b", "ab")),
        );
        incident_expand.synthesized_nodes.insert(
            100,
            SynthKey::Expand {
                source_edge: reserved_edge,
                hop: 0,
            },
        );
        assert!(
            !reserved_graph_node_witness_is_structurally_exact(
                &lens,
                &GInstance::new(),
                &incident_expand,
                &candidate,
            ),
            "a synthesized key carried by an incident dropped edge is node residue"
        );
        Ok(())
    }

    #[test]
    fn reserved_table_negative_basis_has_a_nonvacuous_exact_witness()
    -> Result<(), Box<dyn std::error::Error>> {
        let source_schema = graph_schema(&["kept", "dropped"], &[])?;
        let target_schema = graph_schema(&["kept"], &[])?;
        let table_lens = Lens {
            compiled: CompiledMigration {
                surviving_verts: std::iter::once("kept".into()).collect(),
                ..CompiledMigration::default()
            },
            src_schema: source_schema,
            tgt_schema: target_schema,
        };
        let protocol = table_protocol();
        let ctx = TableContext {
            src: protocol.clone(),
            tgt: protocol.clone(),
        };
        let table_source = FInstance::new()
            .with_rows("kept", [(RowId(1), HashMap::new())])?
            .with_rows("dropped", [(RowId(2), HashMap::new())])?;
        let (table_view, table_complement) = TableShape::get(&table_lens, &table_source, &ctx)?;
        assert_eq!(
            table_complement
                .dropped_rows
                .get(&RowId(2))
                .map(|row| row.table.as_ref()),
            Some("dropped")
        );
        let table_reserved = TableShape::negative_mutation_cases(
            &table_lens,
            &table_source,
            &table_view,
            &table_complement,
            &ctx,
            Mutation::ReservedIdReuse,
        );
        assert_eq!(table_reserved.len(), 1);
        assert_eq!(
            exact_negative_oracle::<TableShape>(
                &table_lens,
                &table_reserved[0],
                &table_complement,
                &ctx,
            ),
            Ok(())
        );
        let mut table_without_residue = table_complement;
        table_without_residue.dropped_rows.remove(&RowId(2));
        assert_eq!(
            TableShape::check_put_domain(
                &table_lens,
                &table_reserved[0].view,
                &table_without_residue,
                &ctx,
            ),
            Ok(())
        );
        let table_restored = TableShape::put(
            &table_lens,
            &table_reserved[0].view,
            &table_without_residue,
            &ctx,
        )?;
        assert_eq!(
            panproto_inst::validate_table(&table_restored, &table_lens.src_schema, &protocol)
                .status,
            panproto_gat::CheckStatus::Passed,
        );
        let (table_regotten, table_regotten_complement) =
            TableShape::get(&table_lens, &table_restored, &ctx)?;
        assert!(TableShape::equivalent(
            &table_reserved[0].view,
            &TableShape::synth_index(&table_reserved[0].view, &table_without_residue),
            &table_regotten,
            &TableShape::synth_index(&table_regotten, &table_regotten_complement),
        ));
        Ok(())
    }

    #[test]
    fn malformed_fan_basis_has_a_nonvacuous_exact_witness() -> Result<(), Box<dyn std::error::Error>>
    {
        let hyper_schema = two_label_hyper_schema();
        let hyper_lens = identity_lens(&hyper_schema);
        let fan_source = GraphShape::enumerate_small(
            &hyper_schema,
            SmallScope {
                max_nodes: 2,
                max_edges: 1,
            },
            &(),
        )
        .into_iter()
        .find(|graph| {
            graph
                .nodes()
                .values()
                .any(|node| matches!(node.shape, panproto_inst::NodeShape::HyperEdge))
                && graph.edge_count() == 1
        })
        .ok_or("canonical fan witness")?;
        let fan_view = fan_source.clone();
        let mut fan_complement = GraphComplement::empty();
        fan_complement.identity = Some(hyper_lens.identity(ComplementShape::Graph));
        let malformed = GraphShape::negative_mutation_cases(
            &hyper_lens,
            &fan_source,
            &fan_view,
            &fan_complement,
            &(),
            Mutation::MalformedFan,
        );
        assert_eq!(malformed.len(), 1);
        assert_eq!(
            exact_negative_oracle::<GraphShape>(&hyper_lens, &malformed[0], &fan_complement, &(),),
            Ok(())
        );
        Ok(())
    }

    #[test]
    fn mutation_cases_retains_the_exhaustive_two_arm_match_contract() {
        fn old_style_match(cases: MutationCases<u8>) -> usize {
            match cases {
                MutationCases::Cases(candidates) => candidates.len(),
                MutationCases::Skip(_) => 0,
            }
        }

        assert_eq!(old_style_match(MutationCases::Cases(vec![1, 2])), 2);
        assert_eq!(
            old_style_match(MutationCases::Skip(SkipReason::NoCandidate)),
            0
        );
    }

    #[test]
    fn removing_a_synthesized_tree_node_preserves_its_arc_witness()
    -> Result<(), Box<dyn std::error::Error>> {
        use panproto_inst::ViewRef;

        let (lens, source, _) = tree_expansion_fixture()?;
        let (view, complement) = get(&lens, &source)?;
        let candidates = TreeShape::negative_mutation_cases(
            &lens,
            &source,
            &view,
            &complement,
            &(),
            Mutation::RemoveSynthesized,
        );
        let Some(candidate) = candidates
            .iter()
            .find(|candidate| !candidate.view.nodes.contains_key(&20))
        else {
            panic!("the synthesized node must have its own removal witness");
        };
        assert_eq!(candidate.view.arcs, view.arcs);
        assert_eq!(
            check_put_domain(&lens, &candidate.view, &complement),
            Err(vec![PutDomainError::SynthesizedEdited {
                elem: ViewRef::Node(20),
            }])
        );
        Ok(())
    }

    #[test]
    fn removing_a_synthesized_graph_node_declares_every_removed_coordinate_in_order()
    -> Result<(), Box<dyn std::error::Error>> {
        let (lens, tree_source, _) = tree_expansion_fixture()?;
        let (graph_source, _) = GInstance::from_wtype_embedded(&tree_source, None)?;
        let (view, complement) = GraphShape::get(&lens, &graph_source, &())?;
        let cases = GraphShape::negative_mutation_cases(
            &lens,
            &graph_source,
            &view,
            &complement,
            &(),
            Mutation::RemoveSynthesized,
        );
        let case = cases
            .iter()
            .find(|case| {
                matches!(
                    case.expected.first(),
                    Some(PutDomainError::SynthesizedEdited {
                        elem: ViewRef::Node(_)
                    })
                )
            })
            .ok_or("synthesized node removal")?;
        assert!(case.expected.len() > 1);
        assert!(case.expected[1..].iter().all(|error| matches!(
            error,
            PutDomainError::SynthesizedEdited {
                elem: ViewRef::Edge(_)
            }
        )));
        assert_eq!(
            exact_negative_oracle::<GraphShape>(&lens, case, &complement, &()),
            Ok(())
        );
        Ok(())
    }

    #[test]
    fn tree_insert_basis_does_not_reuse_a_complement_reserved_id()
    -> Result<(), Box<dyn std::error::Error>> {
        let (lens, source) = reserved_insert_fixture()?;
        let (view, complement) = get(&lens, &source)?;
        assert!(complement.dropped_nodes.contains_key(&2));
        let candidates = tree_insert_cases(&lens, &view, &complement);
        assert!(!candidates.is_empty());
        assert!(candidates.iter().all(|candidate| {
            candidate.nodes.contains_key(&3) && !candidate.nodes.contains_key(&2)
        }));
        Ok(())
    }

    #[test]
    fn contracted_tree_basis_skips_nonisolated_fresh_children()
    -> Result<(), Box<dyn std::error::Error>> {
        let (lens, source) = contraction_stage_fixture()?;
        let (view, complement) = get(&lens, &source)?;
        assert!(
            !TreeShape::negative_mutation_cases(
                &lens,
                &source,
                &view,
                &complement,
                &(),
                Mutation::AddContractedEdge,
            )
            .is_empty()
        );
        let direct = lens.tgt_schema.edges.keys().next().cloned().ok_or("edge")?;
        let mut direct_lens = cloned_lens(&lens);
        direct_lens.src_schema = graph_schema(
            &["a", "b"],
            &[
                source.arcs[0].2.clone(),
                source.arcs[1].2.clone(),
                direct.clone(),
            ],
        )?;
        direct_lens.compiled.surviving_edges.insert(direct);
        let (view, complement) = get(&direct_lens, &source)?;
        assert!(
            TreeShape::negative_mutation_cases(
                &direct_lens,
                &source,
                &view,
                &complement,
                &(),
                Mutation::AddContractedEdge,
            )
            .is_empty()
        );

        let mut ambiguous_lens = cloned_lens(&lens);
        ambiguous_lens.src_schema = graph_schema(
            &["a", "b", "c"],
            &[source.arcs[0].2.clone(), source.arcs[1].2.clone()],
        )?;
        ambiguous_lens
            .compiled
            .vertex_remap
            .insert("c".into(), "a".into());
        let (view, complement) = get(&ambiguous_lens, &source)?;
        assert!(
            TreeShape::negative_mutation_cases(
                &ambiguous_lens,
                &source,
                &view,
                &complement,
                &(),
                Mutation::AddContractedEdge,
            )
            .is_empty()
        );

        let mut required_lens = cloned_lens(&lens);
        required_lens
            .src_schema
            .required
            .insert("a".into(), vec![source.arcs[0].2.clone()]);
        let (view, complement) = get(&required_lens, &source)?;
        assert!(
            TreeShape::negative_mutation_cases(
                &required_lens,
                &source,
                &view,
                &complement,
                &(),
                Mutation::AddContractedEdge,
            )
            .is_empty()
        );

        let mut nonnullable_lens = lens;
        insert_nonnullable_reference(&mut nonnullable_lens.src_schema);
        let (view, complement) = get(&nonnullable_lens, &source)?;
        assert!(
            TreeShape::negative_mutation_cases(
                &nonnullable_lens,
                &source,
                &view,
                &complement,
                &(),
                Mutation::AddContractedEdge,
            )
            .is_empty()
        );
        Ok(())
    }

    #[test]
    fn synthesized_tree_edge_edits_use_endpoint_specific_preimages()
    -> Result<(), Box<dyn std::error::Error>> {
        use panproto_inst::ViewRef;

        let (lens, source, replacement) = adversarial_contraction_edit_fixture(true)?;
        let plan = panproto_inst::RestrictPlan::compile(
            &lens.src_schema,
            &lens.tgt_schema,
            &lens.compiled,
        )?;
        assert_eq!(plan.edge_image.preimages(&replacement).len(), 2);
        let (view, complement) = get(&lens, &source)?;
        let candidates = TreeShape::negative_mutation_cases(
            &lens,
            &source,
            &view,
            &complement,
            &(),
            Mutation::EditSynthesized,
        );
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].view.arcs[0].2, replacement);
        assert_eq!(
            check_put_domain(&lens, &candidates[0].view, &complement),
            Err(vec![PutDomainError::SynthesizedEdited {
                elem: ViewRef::Arc {
                    parent: 0,
                    child: 1,
                },
            }])
        );

        let (lens, source, replacement) = adversarial_contraction_edit_fixture(false)?;
        let plan = panproto_inst::RestrictPlan::compile(
            &lens.src_schema,
            &lens.tgt_schema,
            &lens.compiled,
        )?;
        assert_eq!(plan.edge_image.preimages(&replacement).len(), 1);
        let (view, complement) = get(&lens, &source)?;
        assert!(
            TreeShape::negative_mutation_cases(
                &lens,
                &source,
                &view,
                &complement,
                &(),
                Mutation::EditSynthesized,
            )
            .is_empty()
        );
        Ok(())
    }

    #[test]
    fn reserved_tree_basis_skips_required_key_and_predicate_anchors()
    -> Result<(), Box<dyn std::error::Error>> {
        let (lens, source) = reserved_insert_fixture()?;
        let (view, complement) = get(&lens, &source)?;
        let candidates = TreeShape::negative_mutation_cases(
            &lens,
            &source,
            &view,
            &complement,
            &(),
            Mutation::ReservedIdReuse,
        );
        assert_eq!(candidates.len(), 1);
        let candidate = &candidates[0];
        assert_eq!(candidate.view.parent(2), Some(0));
        assert!(
            candidate
                .view
                .nodes
                .keys()
                .all(|id| { *id == candidate.view.root || candidate.view.parent(*id).is_some() })
        );
        assert_reserved_tree_counterfactual(&lens, &candidate.view, &complement, 2)?;
        let target_edge = lens
            .src_schema
            .edges
            .keys()
            .find(|edge| edge.src.as_ref() == "a" && edge.tgt.as_ref() == "a")
            .cloned()
            .ok_or("self edge")?;

        let mut variants = Vec::new();
        let mut required = cloned_lens(&lens);
        required
            .src_schema
            .required
            .insert("a".into(), vec![target_edge.clone()]);
        variants.push(required);

        let mut keyed = cloned_lens(&lens);
        keyed.src_schema.declarations.keys.insert(
            "a".into(),
            vec![Key {
                id: "a-key".into(),
                parts: vec![KeyPart::Path(vec![target_edge])],
                role: KeyRole::Unique,
                scope: KeyScope::Global,
            }],
        );
        variants.push(keyed);

        let mut predicate = cloned_lens(&lens);
        predicate.compiled.conditional_survival.insert(
            "a".into(),
            panproto_expr::Expr::Lit(panproto_expr::Literal::Bool(true)),
        );
        variants.push(predicate);

        let mut nonnullable = lens;
        insert_nonnullable_reference(&mut nonnullable.src_schema);
        variants.push(nonnullable);

        for variant in variants {
            let (view, complement) = get(&variant, &source)?;
            assert!(
                TreeShape::negative_mutation_cases(
                    &variant,
                    &source,
                    &view,
                    &complement,
                    &(),
                    Mutation::ReservedIdReuse,
                )
                .is_empty()
            );
        }
        Ok(())
    }

    #[test]
    fn reserved_tree_basis_omits_reserved_fan_kind_attachments()
    -> Result<(), Box<dyn std::error::Error>> {
        for kind in [
            panproto_inst::HYPER_PARENT_KIND,
            panproto_inst::HYPER_LABEL_KIND,
        ] {
            let (lens, source, kept) = reserved_fan_kind_collision_fixture(kind)?;
            let (view, complement) = get(&lens, &source)?;
            let mut nodes = view.nodes.clone();
            nodes.insert(2, node_for_anchor(2, &lens.tgt_schema, &"a".into()));
            let unguarded = rebuild_tree(&view, nodes, vec![(0, 2, kept)]);
            assert_eq!(
                TreeShape::check_put_domain(&lens, &unguarded, &complement, &()),
                Err(vec![
                    PutDomainError::IdCollision {
                        elem: ViewRef::Node(2),
                    },
                    PutDomainError::MalformedFan {
                        node: 0,
                        reason: FanViolation::ReservedKindOutsideFan { edge: EdgeId(0) },
                    },
                ]),
                "the complete report retains the {kind} attachment refusal",
            );
            let stripped = without_reserved_tree_residue(complement.clone(), 2);
            assert_eq!(
                TreeShape::check_put_domain(&lens, &unguarded, &stripped, &()),
                Err(vec![PutDomainError::MalformedFan {
                    node: 0,
                    reason: FanViolation::ReservedKindOutsideFan { edge: EdgeId(0) },
                }]),
                "without the collision, the {kind} attachment is malformed",
            );
            assert!(
                TreeShape::negative_mutation_cases(
                    &lens,
                    &source,
                    &view,
                    &complement,
                    &(),
                    Mutation::ReservedIdReuse,
                )
                .iter()
                .all(|case| {
                    case.expected
                        != vec![PutDomainError::IdCollision {
                            elem: ViewRef::Node(2),
                        }]
                }),
                "the singleton basis must omit {kind} tree collisions",
            );
        }
        Ok(())
    }

    #[test]
    fn reserved_tree_basis_skips_a_nonunique_parent_preimage()
    -> Result<(), Box<dyn std::error::Error>> {
        let (lens, source) =
            synthesized_parent_reserved_fixture(ReservedAttachment::LaterEligible)?;
        let (view, complement) = get(&lens, &source)?;
        assert!(
            !TreeShape::negative_mutation_cases(
                &lens,
                &source,
                &view,
                &complement,
                &(),
                Mutation::ReservedIdReuse,
            )
            .is_empty()
        );

        let mut ambiguous_parent = cloned_lens(&lens);
        let mut alias = ambiguous_parent.src_schema.vertices["z"].clone();
        alias.id = "q".into();
        ambiguous_parent
            .src_schema
            .vertices
            .insert("q".into(), alias);
        ambiguous_parent
            .compiled
            .vertex_remap
            .insert("q".into(), "z".into());
        let (view, complement) = get(&ambiguous_parent, &source)?;
        assert!(validation_neutral_target_anchor(
            &ambiguous_parent,
            &"m".into()
        ));
        assert!(!validation_neutral_target_anchor(
            &ambiguous_parent,
            &"z".into()
        ));
        assert!(
            TreeShape::negative_mutation_cases(
                &ambiguous_parent,
                &source,
                &view,
                &complement,
                &(),
                Mutation::ReservedIdReuse,
            )
            .is_empty()
        );
        Ok(())
    }

    #[test]
    fn reserved_tree_basis_skips_a_synthesized_first_parent()
    -> Result<(), Box<dyn std::error::Error>> {
        let (lens, source) =
            synthesized_parent_reserved_fixture(ReservedAttachment::FirstEligible)?;
        let (view, complement) = get(&lens, &source)?;
        let (view, complement) = move_only_synthesized_tree_node(&view, &complement, 3);
        assert_eq!(view.nodes[&3].anchor.as_ref(), "m");
        assert_eq!(view.nodes[&10].anchor.as_ref(), "m");
        assert_eq!(check_put_domain(&lens, &view, &complement), Ok(()));
        let restored = Bound::new(&lens).put(&view, &complement)?;
        assert!(tree_identical(&restored, &source));

        let candidates = TreeShape::negative_mutation_cases(
            &lens,
            &source,
            &view,
            &complement,
            &(),
            Mutation::ReservedIdReuse,
        );
        assert_eq!(candidates.len(), 1);
        let candidate = &candidates[0];
        assert_eq!(candidate.view.parent(2), Some(10));
        assert!(
            !candidate
                .view
                .arcs
                .iter()
                .any(|(parent, child, _)| (*parent, *child) == (3, 2))
        );
        assert_reserved_tree_counterfactual(&lens, &candidate.view, &complement, 2)?;
        Ok(())
    }

    #[test]
    fn reserved_tree_basis_uses_a_later_attachment_at_the_original_parent()
    -> Result<(), Box<dyn std::error::Error>> {
        let (lens, source) =
            synthesized_parent_reserved_fixture(ReservedAttachment::LaterEligible)?;
        let (view, complement) = get(&lens, &source)?;
        let (view, complement) = move_only_synthesized_tree_node(&view, &complement, 3);
        assert_eq!(check_put_domain(&lens, &view, &complement), Ok(()));
        let restored = Bound::new(&lens).put(&view, &complement)?;
        assert!(tree_identical(&restored, &source));

        let candidates = TreeShape::negative_mutation_cases(
            &lens,
            &source,
            &view,
            &complement,
            &(),
            Mutation::ReservedIdReuse,
        );
        assert_eq!(candidates.len(), 1);
        let candidate = &candidates[0];
        assert_eq!(candidate.view.parent(2), Some(0));
        assert!(candidate.view.arcs.iter().any(|(parent, child, label)| {
            (*parent, *child, label.src.as_ref()) == (0, 2, "z")
        }));
        assert!(
            !candidate
                .view
                .arcs
                .iter()
                .any(|(parent, child, _)| (*parent, *child) == (10, 2))
        );
        assert_reserved_tree_counterfactual(&lens, &candidate.view, &complement, 2)?;
        Ok(())
    }

    #[test]
    fn reserved_tree_basis_skips_an_original_parent_without_an_attachment()
    -> Result<(), Box<dyn std::error::Error>> {
        let (lens, source) = synthesized_parent_reserved_fixture(ReservedAttachment::None)?;
        let (view, complement) = get(&lens, &source)?;
        let (view, complement) = move_only_synthesized_tree_node(&view, &complement, 3);
        assert_eq!(check_put_domain(&lens, &view, &complement), Ok(()));
        assert!(
            TreeShape::negative_mutation_cases(
                &lens,
                &source,
                &view,
                &complement,
                &(),
                Mutation::ReservedIdReuse,
            )
            .is_empty()
        );
        Ok(())
    }

    #[test]
    fn reserved_tree_isolation_rejects_outgoing_and_fan_residue()
    -> Result<(), Box<dyn std::error::Error>> {
        let (lens, source) = reserved_insert_fixture()?;
        let (_, complement) = get(&lens, &source)?;
        assert_eq!(isolated_reserved_tree_parent(&complement, 2), Some(0));

        let mut outgoing = complement.clone();
        let label = outgoing.dropped_arcs[0].2.clone();
        outgoing.dropped_arcs.push((2, 0, label));
        assert_eq!(isolated_reserved_tree_parent(&outgoing, 2), None);

        let mut fan = complement;
        fan.dropped_fans
            .push(panproto_inst::Fan::new("fan", 0).with_child("member", 2));
        assert_eq!(isolated_reserved_tree_parent(&fan, 2), None);
        Ok(())
    }

    #[test]
    fn reserved_tree_counterfactual_reindexes_a_later_expansion_key()
    -> Result<(), Box<dyn std::error::Error>> {
        let (lens, source) =
            synthesized_parent_reserved_fixture(ReservedAttachment::FirstEligible)?;
        let mut arcs = source.arcs.clone();
        let Some(dropped) = arcs.pop() else {
            panic!("the fixture has a dropped incident arc");
        };
        arcs.insert(0, dropped);
        let source = rebuild_tree(&source, source.nodes.clone(), arcs);
        let (view, complement) = get(&lens, &source)?;
        let candidates = TreeShape::negative_mutation_cases(
            &lens,
            &source,
            &view,
            &complement,
            &(),
            Mutation::ReservedIdReuse,
        );
        assert_eq!(candidates.len(), 1);
        assert_reserved_tree_counterfactual(&lens, &candidates[0].view, &complement, 2)?;

        let Some((&synthesized, SynthKey::Expand { source_edge, hop })) = complement
            .synthesized_keys
            .iter()
            .find(|(_, key)| matches!(key, SynthKey::Expand { .. }))
        else {
            panic!("the later source arc has an expansion key");
        };
        let old_edge = *source_edge;
        let old_hop = *hop;
        let old_coordinate = complement.arc_order[usize::try_from(old_edge.0)?];
        assert_eq!(old_coordinate, (4, 1));

        let stripped = without_reserved_tree_residue(complement, 2);
        let Some(SynthKey::Expand { source_edge, hop }) =
            stripped.synthesized_keys.get(&synthesized)
        else {
            panic!("the unrelated expansion key is retained");
        };
        assert_eq!(*hop, old_hop);
        assert_eq!(source_edge.0.checked_add(1), Some(old_edge.0));
        assert_eq!(
            stripped.arc_order[usize::try_from(source_edge.0)?],
            old_coordinate
        );
        Ok(())
    }

    #[test]
    fn tree_reconnect_reports_no_candidate_when_every_other_parent_is_invalid()
    -> Result<(), crate::LensError> {
        let schema = three_node_schema();
        let lens = identity_lens(&schema);
        let source = three_node_instance();
        let (view, complement) = get(&lens, &source)?;
        assert!(matches!(
            TreeShape::mutation_cases(&lens, &source, &view, &complement, &(), Mutation::Reconnect),
            MutationCases::Skip(SkipReason::NoCandidate)
        ));
        Ok(())
    }

    #[test]
    fn synthesized_tree_renaming_requires_and_accepts_a_coherent_complement() {
        use panproto_inst::{SynthKey, ViewRef};

        let schema = three_node_schema();
        let lens = identity_lens(&schema);
        let fixture = three_node_instance();
        // Node 1 plays a synthesized first hop, so its arc follows its kept
        // sibling, as tree get emits it.
        let mut arcs = fixture.arcs.clone();
        arcs.rotate_left(1);
        let mut view = WInstance::new(
            fixture.nodes.clone(),
            arcs,
            fixture.fans.clone(),
            fixture.root,
            fixture.schema_root,
        );
        let Some(synthesized) = view.nodes.get_mut(&1) else {
            panic!("fixture has node 1");
        };
        synthesized.value = None;
        let Some((_, _, edge)) = view.arcs.iter().find(|(_, child, _)| *child == 1) else {
            panic!("fixture has an arc to node 1");
        };
        let edge = edge.clone();
        let key = SynthKey::Expand {
            source_edge: EdgeId(0),
            hop: 0,
        };
        let mut complement = Complement::default();
        complement.synthesized_nodes.insert(1);
        complement.synthesized_keys.insert(1, key);
        complement.contraction_choices.insert((0, 1), edge);

        let index = TreeShape::synth_index(&view, &complement);
        let Some((renamed_view, _)) = TreeShape::congruence_witnesses(&view, &index).pop() else {
            panic!("a synthesized node has a renaming witness");
        };
        let Err(raw_errors) = check_put_domain(&lens, &renamed_view, &complement) else {
            panic!("renaming only the view must be refused");
        };
        assert!(raw_errors.iter().any(|error| {
            matches!(
                error,
                PutDomainError::SynthesizedEdited {
                    elem: ViewRef::Node(_)
                }
            )
        }));

        let Some((coherent_view, coherent_complement)) =
            tree_state_witnesses(&view, &complement).pop()
        else {
            panic!("a coherent state witness exists");
        };
        let coherent_errors = match check_put_domain(&lens, &coherent_view, &coherent_complement) {
            Ok(()) => Vec::new(),
            Err(errors) => errors,
        };
        assert!(
            !coherent_errors
                .iter()
                .any(|error| { matches!(error, PutDomainError::SynthesizedEdited { .. }) })
        );
        assert!(TreeShape::equivalent(
            &view,
            &TreeShape::synth_index(&view, &complement),
            &coherent_view,
            &TreeShape::synth_index(&coherent_view, &coherent_complement),
        ));
        assert!(
            TreeShape::complement_divergence(&complement, &coherent_complement).is_none(),
            "coherent reindexing preserves every complement observation"
        );
    }

    #[test]
    fn shape_specific_skips_are_exact() -> Result<(), crate::LensError> {
        let schema = three_node_schema();
        let lens = identity_lens(&schema);
        let tree = three_node_instance();
        let (_, tree_complement) = get(&lens, &tree)?;
        for mutation in [
            Mutation::AddParallelEdge,
            Mutation::AddSelfLoop,
            Mutation::MalformedFan,
        ] {
            assert!(matches!(
                TreeShape::mutation_cases(&lens, &tree, &tree, &tree_complement, &(), mutation),
                MutationCases::Skip(SkipReason::ShapeCannotExpress)
            ));
        }

        let table = FInstance::new();
        let table_complement = TableComplement::empty();
        let context = crate::table_shape::TableContext::default();
        for mutation in [
            Mutation::Reorder,
            Mutation::AddParallelEdge,
            Mutation::MalformedFan,
        ] {
            assert!(matches!(
                TableShape::mutation_cases(
                    &lens,
                    &table,
                    &table,
                    &table_complement,
                    &context,
                    mutation
                ),
                MutationCases::Skip(SkipReason::ShapeCannotExpress)
            ));
        }
        Ok(())
    }

    /// Re-anchoring the root re-anchors the tree: the candidate's schema
    /// root follows the root node's anchor, so the identity lens passes.
    #[test]
    fn a_relabelled_root_carries_its_schema_root() {
        let protocol = panproto_schema::Protocol::default();
        let Ok(builder) =
            panproto_schema::SchemaBuilder::new(&protocol).vertex("a", "object", None)
        else {
            panic!("vertex a is valid");
        };
        let Ok(builder) = builder.vertex("b", "object", None) else {
            panic!("vertex b is valid");
        };
        let Ok(schema) = builder.build() else {
            panic!("the two-vertex schema is valid");
        };
        let lens = identity_lens(&schema);
        let lone = WInstance::new(
            HashMap::from([(0, Node::new(0, "a"))]),
            Vec::new(),
            Vec::new(),
            0,
            "a".into(),
        );
        let Ok((view, complement)) = crate::asymmetric::get(&lens, &lone) else {
            panic!("the identity tree get succeeds");
        };
        let candidates = tree_relabel_node_cases(&lens, &view, &complement);
        assert_eq!(candidates.len(), 1);
        assert_eq!(
            candidates[0].nodes[&0].anchor,
            panproto_gat::Name::from("b")
        );
        assert_eq!(candidates[0].schema_root, panproto_gat::Name::from("b"));
        let report = check_laws_report::<TreeShape>(&lens, &lone, &());
        assert!(report.is_ok(), "{report:?}");
    }

    #[test]
    fn identity_lens_satisfies_laws() {
        let schema = three_node_schema();
        let lens = identity_lens(&schema);
        let instance = three_node_instance();

        let result = check_laws(&lens, &instance);
        assert!(
            result.is_ok(),
            "identity lens should satisfy all laws: {result:?}"
        );
    }

    #[test]
    fn tree_law_wrappers_preserve_an_operational_failure() {
        let schema = three_node_schema();
        let lens = identity_lens(&schema);
        let mut instance = three_node_instance();
        instance.root = u32::MAX;

        assert!(
            check_laws(&lens, &instance).is_err(),
            "the public wrapper must not erase the shape checker's failure"
        );
        assert!(
            check_put_twice(&lens, &instance).is_err(),
            "the PutTwice wrapper must not erase the shape checker's failure"
        );
    }

    #[test]
    fn identity_lens_satisfies_get_put() {
        let schema = three_node_schema();
        let lens = identity_lens(&schema);
        let instance = three_node_instance();

        let result = check_get_put(&lens, &instance);
        assert!(result.is_ok(), "identity lens should satisfy GetPut");
    }

    #[test]
    fn identity_lens_satisfies_put_get() {
        let schema = three_node_schema();
        let lens = identity_lens(&schema);
        let instance = three_node_instance();

        let result = check_put_get(&lens, &instance);
        assert!(result.is_ok(), "identity lens should satisfy PutGet");
    }

    /// Complements that restore differently are not equivalent: the arc
    /// sequence decides the order a collection comes back in, the original
    /// anchors decide which vertex a merged node returns to, and the
    /// envelope decides which lens may use the complement at all.
    #[test]
    fn complements_that_restore_differently_diverge() {
        let schema = three_node_schema();
        let lens = identity_lens(&schema);
        let Ok((_, base)) = get(&lens, &three_node_instance()) else {
            panic!("get failed on the fixture");
        };
        assert!(complement_divergence(&base, &base.clone()).is_none());

        let mut reordered = base.clone();
        reordered.arc_order.reverse();
        assert_ne!(
            reordered.arc_order, base.arc_order,
            "the fixture has two arcs"
        );
        assert_eq!(
            complement_divergence(&base, &reordered).as_deref(),
            Some("arc_order differs")
        );

        let mut reanchored = base.clone();
        reanchored.original_anchors.insert(1, "elsewhere".into());
        assert_eq!(
            complement_divergence(&base, &reanchored).as_deref(),
            Some("original_anchors differ")
        );

        let mut unstamped = base.clone();
        unstamped.identity = None;
        assert!(
            complement_divergence(&base, &unstamped)
                .is_some_and(|detail| detail.starts_with("identity"))
        );
    }

    #[test]
    fn different_arcs_are_not_equivalent() {
        use panproto_schema::Edge;

        let a = three_node_instance();
        let mut b = a.clone();

        // Swap an arc's edge kind in b so arcs differ
        if let Some(arc) = b.arcs.first_mut() {
            arc.2 = Edge {
                src: arc.2.src.clone(),
                tgt: arc.2.tgt.clone(),
                kind: "different_kind".into(),
                name: arc.2.name.clone(),
            };
        }

        assert!(
            !instances_equivalent(&a, &b),
            "instances with different arcs should not be equivalent"
        );
    }

    /// The canned `PutGet` mutation has to stay inside the domain of the
    /// strict tree `put`: a leaf a computed field reads, or a leaf written
    /// by a transform `put` cannot invert, is not an independent coordinate
    /// of the view, so perturbing it on its own gives a view `put` refuses.
    mod put_domain_mutation {
        use std::sync::Arc;

        use super::*;
        use panproto_expr::{BuiltinOp, Expr};
        use panproto_gat::CoercionClass;
        use panproto_inst::FieldTransform;

        fn var(name: &str) -> Expr {
            Expr::Var(Arc::from(name))
        }

        fn lens_with(transforms: Vec<FieldTransform>) -> Lens {
            let mut lens = identity_lens(&three_node_schema());
            lens.compiled
                .field_transforms
                .insert("post:body".into(), transforms);
            lens
        }

        /// `shout = upper(text)`, inverse `lower(shout)`, over the child
        /// scalar `text`.
        fn compute_over_a_child_scalar() -> Lens {
            lens_with(vec![FieldTransform::ComputeField {
                target_key: "shout".into(),
                expr: Expr::Builtin(BuiltinOp::Upper, vec![var("text")]),
                inverse: Some(Expr::Builtin(BuiltinOp::Lower, vec![var("shout")])),
                coercion_class: CoercionClass::Iso,
            }])
        }

        /// `text := upper(text)` over the child scalar `text`, which shadows
        /// the child with a parent entry `put` cannot write back.
        fn apply_over_a_child_scalar() -> Lens {
            lens_with(vec![FieldTransform::ApplyExpr {
                key: "text".into(),
                expr: Expr::Builtin(BuiltinOp::Upper, vec![var("text")]),
                inverse: Some(Expr::Builtin(BuiltinOp::Lower, vec![var("text")])),
                coercion_class: CoercionClass::Iso,
            }])
        }

        #[test]
        fn put_get_holds_for_a_computed_field_over_a_child_scalar() {
            let lens = compute_over_a_child_scalar();
            let result = check_put_get(&lens, &three_node_instance());
            assert!(result.is_ok(), "PutGet must hold: {result:?}");
            assert!(check_laws(&lens, &three_node_instance()).is_ok());
        }

        /// The mutation skips the leaf the computed field reads but still
        /// perturbs the independent one, so the check is not vacuous.
        #[test]
        fn the_mutation_keeps_every_perturbation_put_accepts() {
            use panproto_inst::value::{FieldPresence, Value};

            let lens = compute_over_a_child_scalar();
            let Ok((view, complement)) = get(&lens, &three_node_instance()) else {
                panic!("get failed on the fixture");
            };
            let modified = modify_leaf_values_in_domain(&lens, &view, &complement);
            assert_eq!(check_put_domain(&lens, &modified, &complement), Ok(()));
            assert_eq!(
                modified.nodes[&1].value,
                Some(FieldPresence::Present(Value::Str("hello".into()))),
                "`text` feeds `shout`, so perturbing it alone leaves the domain"
            );
            assert_eq!(
                modified.nodes[&2].value,
                Some(FieldPresence::Present(Value::Str(
                    "2024-01-01_modified".into()
                ))),
                "`createdAt` is independent, so it is perturbed"
            );
        }

        /// With no transform every leaf is perturbed at once.
        #[test]
        fn the_mutation_perturbs_every_leaf_of_an_unconstrained_view() {
            use panproto_inst::value::{FieldPresence, Value};

            let lens = identity_lens(&three_node_schema());
            let Ok((view, complement)) = get(&lens, &three_node_instance()) else {
                panic!("get failed on the fixture");
            };
            let modified = modify_leaf_values_in_domain(&lens, &view, &complement);
            assert_eq!(
                modified.nodes[&1].value,
                Some(FieldPresence::Present(Value::Str("hello_modified".into())))
            );
            assert_eq!(
                modified.nodes[&2].value,
                Some(FieldPresence::Present(Value::Str(
                    "2024-01-01_modified".into()
                )))
            );
        }

        #[test]
        fn put_get_holds_for_a_transform_over_a_child_scalar() {
            let lens = apply_over_a_child_scalar();
            let result = check_put_get(&lens, &three_node_instance());
            assert!(result.is_ok(), "PutGet must hold: {result:?}");
            assert!(check_laws(&lens, &three_node_instance()).is_ok());
        }
    }

    // --- proptest strategies and property tests ---

    #[allow(clippy::unwrap_used, clippy::expect_used)]
    mod property {
        use super::*;
        use panproto_gat::Name;
        use panproto_inst::value::{FieldPresence, Value};
        use panproto_inst::{CompiledMigration, Node, WInstance};
        use panproto_schema::{Edge, Schema, Vertex};
        use proptest::prelude::*;
        use smallvec::SmallVec;
        use std::collections::{HashMap, HashSet};

        const LEAF_KINDS: &[&str] = &["string", "integer", "boolean"];

        fn make_schema(verts: &[(&str, &str)], edge_list: &[Edge]) -> Schema {
            let mut vertices = HashMap::new();
            let mut edges = HashMap::new();
            let mut outgoing: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
            let mut incoming: HashMap<Name, SmallVec<Edge, 4>> = HashMap::new();
            let mut between: HashMap<(Name, Name), SmallVec<Edge, 2>> = HashMap::new();

            for (id, kind) in verts {
                vertices.insert(
                    Name::from(*id),
                    Vertex {
                        id: Name::from(*id),
                        kind: Name::from(*kind),
                        nsid: None,
                    },
                );
            }
            for e in edge_list {
                edges.insert(e.clone(), e.kind.clone());
                outgoing.entry(e.src.clone()).or_default().push(e.clone());
                incoming.entry(e.tgt.clone()).or_default().push(e.clone());
                between
                    .entry((e.src.clone(), e.tgt.clone()))
                    .or_default()
                    .push(e.clone());
            }

            Schema {
                protocol: "test".into(),
                vertices,
                edges,
                hyper_edges: HashMap::new(),
                constraints: HashMap::new(),
                required: HashMap::new(),
                nsids: HashMap::new(),
                entries: Vec::new(),
                variants: HashMap::new(),
                orderings: HashMap::new(),
                recursion_points: HashMap::new(),
                spans: HashMap::new(),
                usage_modes: HashMap::new(),
                nominal: HashMap::new(),
                coercions: HashMap::new(),
                mergers: HashMap::new(),
                defaults: HashMap::new(),
                policies: HashMap::new(),
                outgoing,
                incoming,
                between,
                ..Default::default()
            }
        }

        /// Generate a random schema + instance + identity lens.
        fn arb_identity_lens_scenario() -> impl Strategy<Value = (Lens, WInstance)> {
            // 1-4 leaf children under a root object.
            (1..=4usize).prop_flat_map(|n_children| {
                prop::collection::vec(
                    prop::sample::select(LEAF_KINDS).prop_map(ToOwned::to_owned),
                    n_children..=n_children,
                )
                .prop_flat_map(move |kinds| {
                    // Generate seeds, then materialize values at the sampled kinds.
                    prop::collection::vec(
                        "[a-z]{1,8}".prop_map(String::from),
                        n_children..=n_children,
                    )
                    .prop_map(move |values| {
                        let kinds = kinds.clone();
                        let root_name = "root";
                        let child_names: Vec<String> =
                            (0..kinds.len()).map(|i| format!("child{i}")).collect();

                        // Build schema.
                        let mut vert_specs: Vec<(String, String)> =
                            vec![(root_name.to_owned(), "object".to_owned())];
                        let mut edges = Vec::new();
                        for (i, kind) in kinds.iter().enumerate() {
                            vert_specs.push((child_names[i].clone(), kind.clone()));
                            edges.push(Edge {
                                src: root_name.into(),
                                tgt: Name::from(child_names[i].as_str()),
                                kind: "prop".into(),
                                name: Some(Name::from(child_names[i].as_str())),
                            });
                        }
                        let vert_refs: Vec<(&str, &str)> = vert_specs
                            .iter()
                            .map(|(a, b)| (a.as_str(), b.as_str()))
                            .collect();
                        let schema = make_schema(&vert_refs, &edges);

                        // Build instance.
                        let mut nodes = HashMap::new();
                        nodes.insert(0, Node::new(0, root_name));
                        for (i, val) in values.iter().enumerate() {
                            let node_id = u32::try_from(i + 1).unwrap();
                            nodes.insert(
                                node_id,
                                Node::new(node_id, child_names[i].as_str())
                                    .with_value(FieldPresence::Present(leaf_value(&kinds[i], val))),
                            );
                        }
                        let arcs: Vec<(u32, u32, Edge)> = edges
                            .iter()
                            .enumerate()
                            .map(|(i, e)| (0, u32::try_from(i + 1).unwrap(), e.clone()))
                            .collect();
                        let instance = WInstance::new(nodes, arcs, vec![], 0, root_name.into());

                        // Build identity lens.
                        let surviving_verts: HashSet<Name> =
                            schema.vertices.keys().cloned().collect();
                        let surviving_edges: HashSet<Edge> = schema.edges.keys().cloned().collect();
                        let lens = Lens {
                            compiled: CompiledMigration {
                                surviving_verts,
                                surviving_edges,
                                vertex_remap: HashMap::new(),
                                edge_remap: HashMap::new(),
                                resolver: HashMap::new(),
                                hyper_resolver: HashMap::new(),
                                field_transforms: HashMap::new(),
                                conditional_survival: HashMap::new(),
                                op_term_assignments: HashMap::new(),
                                expansion_path: HashMap::new(),
                            },
                            src_schema: schema.clone(),
                            tgt_schema: schema,
                        };

                        (lens, instance)
                    })
                })
            })
        }

        /// Generate a projection lens scenario: schema with root + N children,
        /// lens drops one child.
        fn arb_projection_lens_scenario() -> impl Strategy<Value = (Lens, WInstance)> {
            // 2-4 leaf children; we'll drop the last one.
            (2..=4usize).prop_flat_map(|n_children| {
                prop::collection::vec("[a-z]{1,8}".prop_map(String::from), n_children..=n_children)
                    .prop_map(move |values| {
                        let root_name = "root";
                        let child_names: Vec<String> =
                            (0..n_children).map(|i| format!("child{i}")).collect();

                        // Build full schema.
                        let mut vert_specs: Vec<(String, String)> =
                            vec![(root_name.to_owned(), "object".to_owned())];
                        let mut all_edges = Vec::new();
                        for name in &child_names {
                            vert_specs.push((name.clone(), "string".to_owned()));
                            all_edges.push(Edge {
                                src: root_name.into(),
                                tgt: Name::from(name.as_str()),
                                kind: "prop".into(),
                                name: Some(Name::from(name.as_str())),
                            });
                        }
                        let vert_refs: Vec<(&str, &str)> = vert_specs
                            .iter()
                            .map(|(a, b)| (a.as_str(), b.as_str()))
                            .collect();
                        let src_schema = make_schema(&vert_refs, &all_edges);

                        // Target schema: drop last child.
                        let tgt_vert_refs: Vec<(&str, &str)> =
                            vert_refs[..vert_refs.len() - 1].to_vec();
                        let tgt_edges: Vec<Edge> = all_edges[..all_edges.len() - 1].to_vec();
                        let tgt_schema = make_schema(&tgt_vert_refs, &tgt_edges);

                        // Build instance.
                        let mut nodes = HashMap::new();
                        nodes.insert(0, Node::new(0, root_name));
                        for (i, val) in values.iter().enumerate() {
                            let node_id = u32::try_from(i + 1).unwrap();
                            nodes.insert(
                                node_id,
                                Node::new(node_id, child_names[i].as_str())
                                    .with_value(FieldPresence::Present(Value::Str(val.clone()))),
                            );
                        }
                        let arcs: Vec<(u32, u32, Edge)> = all_edges
                            .iter()
                            .enumerate()
                            .map(|(i, e)| (0, u32::try_from(i + 1).unwrap(), e.clone()))
                            .collect();
                        let instance = WInstance::new(nodes, arcs, vec![], 0, root_name.into());

                        // Build projection lens.
                        let surviving_verts: HashSet<Name> =
                            tgt_schema.vertices.keys().cloned().collect();
                        let surviving_edges: HashSet<Edge> =
                            tgt_schema.edges.keys().cloned().collect();
                        let lens = Lens {
                            compiled: CompiledMigration {
                                surviving_verts,
                                surviving_edges,
                                vertex_remap: HashMap::new(),
                                edge_remap: HashMap::new(),
                                resolver: HashMap::new(),
                                hyper_resolver: HashMap::new(),
                                field_transforms: HashMap::new(),
                                conditional_survival: HashMap::new(),
                                op_term_assignments: HashMap::new(),
                                expansion_path: HashMap::new(),
                            },
                            src_schema,
                            tgt_schema,
                        };

                        (lens, instance)
                    })
            })
        }

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(64))]

            #[test]
            fn identity_lens_satisfies_laws_proptest(
                (lens, instance) in arb_identity_lens_scenario()
            ) {
                prop_assert!(
                    check_laws(&lens, &instance).is_ok(),
                    "identity lens should satisfy all laws",
                );
            }

            #[test]
            fn projection_lens_satisfies_get_put_proptest(
                (lens, instance) in arb_projection_lens_scenario()
            ) {
                prop_assert!(
                    check_get_put(&lens, &instance).is_ok(),
                    "projection lens should satisfy GetPut",
                );
            }

            #[test]
            fn projection_lens_satisfies_put_get_proptest(
                (lens, instance) in arb_projection_lens_scenario()
            ) {
                prop_assert!(
                    check_put_get(&lens, &instance).is_ok(),
                    "projection lens should satisfy PutGet",
                );
            }

            #[test]
            fn projection_lens_satisfies_full_laws_proptest(
                (lens, instance) in arb_projection_lens_scenario()
            ) {
                prop_assert!(
                    check_laws(&lens, &instance).is_ok(),
                    "projection lens should satisfy all laws",
                );
            }

            /// Verify GetPut holds when ComputeField transforms access
            /// child scalar values. This tests the formal property that
            /// the complement mechanism correctly handles computed
            /// extra_fields derived from the dependent-sum projection.
            ///
            /// The correctness argument: ComputeField writes derived
            /// data to extra_fields. The complement captures
            /// `original_extra_fields` (pre-transform), which does NOT
            /// contain the computed field. `put` restores
            /// `original_extra_fields`, discarding the computed field.
            /// Child scalar nodes survive via tree structure. So GetPut
            /// holds: the restored instance equals the original.
            #[test]
            fn identity_lens_with_compute_field_satisfies_getput(
                (lens, instance) in arb_identity_lens_with_compute_field()
            ) {
                prop_assert!(
                    check_get_put(&lens, &instance).is_ok(),
                    "identity lens with ComputeField should satisfy GetPut",
                );
            }

            /// PutPut: a second put subsumes the first. With `(v, c) = get(s)`,
            /// putting a perturbed view and then `v` again gives what putting
            /// `v` alone gives. Exercised over identity lenses and over
            /// projections, where the complement carries data.
            #[test]
            fn identity_lens_satisfies_put_put_proptest(
                (lens, instance) in arb_identity_lens_scenario()
            ) {
                let (view, _) = get(&lens, &instance).unwrap();
                let perturbed = perturb_view_leaves(&view);
                prop_assume!(!instances_equivalent(&perturbed, &view));
                prop_assert!(
                    check_put_put(&lens, &instance, &perturbed, &view).is_ok(),
                    "identity lens should satisfy PutPut",
                );
            }

            #[test]
            fn projection_lens_satisfies_put_put_proptest(
                (lens, instance) in arb_projection_lens_scenario()
            ) {
                let (view, _) = get(&lens, &instance).unwrap();
                let perturbed = perturb_view_leaves(&view);
                prop_assume!(!instances_equivalent(&perturbed, &view));
                prop_assert!(
                    check_put_put(&lens, &instance, &perturbed, &view).is_ok(),
                    "projection lens should satisfy PutPut",
                );
            }
        }

        /// Perturb every leaf-string value in `view` so that it differs
        /// from the original. Used by `PutPut` tests to obtain a second
        /// view structurally compatible with the schema but distinct
        /// from the round-tripped first view.
        fn perturb_view_leaves(view: &WInstance) -> WInstance {
            let mut perturbed = view.clone();
            for node in perturbed.nodes.values_mut() {
                if let Some(FieldPresence::Present(Value::Str(ref mut s))) = node.value {
                    s.push_str("_v2");
                } else if let Some(FieldPresence::Present(Value::Int(ref mut i))) = node.value {
                    *i = i.wrapping_add(1);
                }
            }
            perturbed
        }

        /// Generate an identity lens scenario WITH a `ComputeField` that
        /// copies a child scalar to a new key. This tests that the
        /// complement mechanism handles the new computed `extra_fields`
        /// correctly.
        fn arb_identity_lens_with_compute_field() -> impl Strategy<Value = (Lens, WInstance)> {
            (2..=4usize).prop_flat_map(|n_children| {
                prop::collection::vec("[a-z]{1,8}".prop_map(String::from), n_children..=n_children)
                    .prop_map(move |values| {
                        let root_name = "root";
                        let child_names: Vec<String> =
                            (0..n_children).map(|i| format!("child{i}")).collect();

                        let mut vert_specs: Vec<(String, String)> =
                            vec![(root_name.to_owned(), "object".to_owned())];
                        let mut edges = Vec::new();
                        for name in &child_names {
                            vert_specs.push((name.clone(), "string".to_owned()));
                            edges.push(Edge {
                                src: root_name.into(),
                                tgt: Name::from(name.as_str()),
                                kind: "prop".into(),
                                name: Some(Name::from(name.as_str())),
                            });
                        }
                        let vert_refs: Vec<(&str, &str)> = vert_specs
                            .iter()
                            .map(|(a, b)| (a.as_str(), b.as_str()))
                            .collect();
                        let schema = make_schema(&vert_refs, &edges);

                        let mut nodes = HashMap::new();
                        nodes.insert(0, Node::new(0, root_name));
                        for (i, val) in values.iter().enumerate() {
                            let node_id = u32::try_from(i + 1).unwrap();
                            nodes.insert(
                                node_id,
                                Node::new(node_id, child_names[i].as_str())
                                    .with_value(FieldPresence::Present(Value::Str(val.clone()))),
                            );
                        }
                        let arcs: Vec<(u32, u32, Edge)> = edges
                            .iter()
                            .enumerate()
                            .map(|(i, e)| (0, u32::try_from(i + 1).unwrap(), e.clone()))
                            .collect();
                        let instance = WInstance::new(nodes, arcs, vec![], 0, root_name.into());

                        // Add a ComputeField that copies child0 to a new key.
                        // This exercises the child scalar access path.
                        let compute = panproto_inst::FieldTransform::ComputeField {
                            target_key: "derived_from_child0".into(),
                            expr: panproto_expr::Expr::Var(std::sync::Arc::from("child0")),
                            inverse: None,
                            coercion_class: panproto_gat::CoercionClass::Projection,
                        };

                        let mut field_transforms = HashMap::new();
                        field_transforms.insert(Name::from(root_name), vec![compute]);

                        let surviving_verts: HashSet<Name> =
                            schema.vertices.keys().cloned().collect();
                        let surviving_edges: HashSet<Edge> = schema.edges.keys().cloned().collect();
                        let lens = Lens {
                            compiled: CompiledMigration {
                                surviving_verts,
                                surviving_edges,
                                vertex_remap: HashMap::new(),
                                edge_remap: HashMap::new(),
                                resolver: HashMap::new(),
                                hyper_resolver: HashMap::new(),
                                field_transforms,
                                conditional_survival: HashMap::new(),
                                op_term_assignments: HashMap::new(),
                                expansion_path: HashMap::new(),
                            },
                            src_schema: schema.clone(),
                            tgt_schema: schema,
                        };

                        (lens, instance)
                    })
            })
        }

        // -------------------------------------------------------------------
        // Widened generators — nested trees, vertex/edge remaps,
        // and interior field transforms.
        // -------------------------------------------------------------------

        /// Generate one leaf child spec: `(kind, value-seed)`.
        fn arb_leaf() -> impl Strategy<Value = (String, String)> {
            (
                prop::sample::select(LEAF_KINDS).prop_map(ToOwned::to_owned),
                "[a-z]{1,6}".prop_map(String::from),
            )
        }

        /// Materialize a leaf value of the given kind from a string seed.
        ///
        /// Keeps the value type aligned with the schema's vertex kind so
        /// that mutation and round-tripping stay schema-compatible.
        fn leaf_value(kind: &str, seed: &str) -> Value {
            match kind {
                "integer" => Value::Int(i64::try_from(seed.len()).unwrap_or(0)),
                "boolean" => Value::Bool(seed.len() % 2 == 0),
                // "string" and anything else fall back to a string carrier.
                _ => Value::Str(seed.to_owned()),
            }
        }

        #[test]
        fn identity_generator_materializes_each_sampled_leaf_kind() {
            assert!(matches!(leaf_value("string", "abc"), Value::Str(_)));
            assert!(matches!(leaf_value("integer", "abc"), Value::Int(3)));
            assert!(matches!(leaf_value("boolean", "abc"), Value::Bool(false)));
        }

        /// Generate a depth-3 nested tree (root → object mids → leaves) with
        /// an identity lens. Leaves are grandchildren of the root (their
        /// parent is a mid-level object, not the root), and the root and each
        /// mid carry `extra_fields` so the view-mutation strategy has
        /// non-scalar payloads to perturb.
        fn arb_nested_tree_scenario() -> impl Strategy<Value = (Lens, WInstance)> {
            // 1-3 mid-level objects, each with 1-2 leaf children.
            prop::collection::vec(prop::collection::vec(arb_leaf(), 1..=2), 1..=3)
                .prop_map(|mids| build_nested_scenario(&mids))
        }

        /// Build the nested-tree scenario from a per-mid list of leaf specs.
        fn build_nested_scenario(mids: &[Vec<(String, String)>]) -> (Lens, WInstance) {
            let root_name = "root";
            let mut vert_specs: Vec<(String, String)> =
                vec![(root_name.to_owned(), "object".to_owned())];
            let mut edges: Vec<Edge> = Vec::new();
            let mut nodes: HashMap<u32, Node> = HashMap::new();
            let mut arcs: Vec<(u32, u32, Edge)> = Vec::new();

            // Root carries an extra field.
            let mut root_node = Node::new(0, root_name);
            root_node
                .extra_fields
                .insert("kind_tag".to_owned(), Value::Str("object".to_owned()));
            nodes.insert(0, root_node);

            let mut next_id: u32 = 1;
            for (i, leaves) in mids.iter().enumerate() {
                let mid_name = format!("mid{i}");
                vert_specs.push((mid_name.clone(), "object".to_owned()));
                let mid_edge = Edge {
                    src: root_name.into(),
                    tgt: Name::from(mid_name.as_str()),
                    kind: "prop".into(),
                    name: Some(Name::from(mid_name.as_str())),
                };
                edges.push(mid_edge.clone());
                let mid_id = next_id;
                next_id += 1;
                let mut mid_node = Node::new(mid_id, mid_name.as_str());
                mid_node
                    .extra_fields
                    .insert("depth".to_owned(), Value::Int(1));
                nodes.insert(mid_id, mid_node);
                arcs.push((0, mid_id, mid_edge));

                for (j, (kind, seed)) in leaves.iter().enumerate() {
                    let leaf_name = format!("leaf{i}_{j}");
                    vert_specs.push((leaf_name.clone(), kind.clone()));
                    let leaf_edge = Edge {
                        src: Name::from(mid_name.as_str()),
                        tgt: Name::from(leaf_name.as_str()),
                        kind: "prop".into(),
                        name: Some(Name::from(leaf_name.as_str())),
                    };
                    edges.push(leaf_edge.clone());
                    let leaf_id = next_id;
                    next_id += 1;
                    let leaf_node = Node::new(leaf_id, leaf_name.as_str())
                        .with_value(FieldPresence::Present(leaf_value(kind, seed)));
                    nodes.insert(leaf_id, leaf_node);
                    // Grandchild arc: parent is the mid node, not the root.
                    arcs.push((mid_id, leaf_id, leaf_edge));
                }
            }

            let vert_refs: Vec<(&str, &str)> = vert_specs
                .iter()
                .map(|(a, b)| (a.as_str(), b.as_str()))
                .collect();
            let schema = make_schema(&vert_refs, &edges);
            let instance = WInstance::new(nodes, arcs, vec![], 0, root_name.into());

            let surviving_verts: HashSet<Name> = schema.vertices.keys().cloned().collect();
            let surviving_edges: HashSet<Edge> = schema.edges.keys().cloned().collect();
            let lens = Lens {
                compiled: CompiledMigration {
                    surviving_verts,
                    surviving_edges,
                    vertex_remap: HashMap::new(),
                    edge_remap: HashMap::new(),
                    resolver: HashMap::new(),
                    hyper_resolver: HashMap::new(),
                    field_transforms: HashMap::new(),
                    conditional_survival: HashMap::new(),
                    op_term_assignments: HashMap::new(),
                    expansion_path: HashMap::new(),
                },
                src_schema: schema.clone(),
                tgt_schema: schema,
            };
            (lens, instance)
        }

        /// Generate a lossless *rename remap* lens between two distinct
        /// schemas: every child vertex `src_childᵢ` is renamed to
        /// `tgt_childᵢ`, with `vertex_remap` and `edge_remap` populated
        /// accordingly. The remap is a schema isomorphism, so the lens is
        /// well-behaved (all laws hold).
        fn arb_remap_lens_scenario() -> impl Strategy<Value = (Lens, WInstance)> {
            (1..=4usize).prop_flat_map(|n| {
                prop::collection::vec("[a-z]{1,6}".prop_map(String::from), n..=n)
                    .prop_map(|values| build_remap_scenario(&values))
            })
        }

        /// Build the rename-remap scenario from a list of leaf string values.
        fn build_remap_scenario(values: &[String]) -> (Lens, WInstance) {
            let root_name = "root";
            let n = values.len();

            let mut src_vert_specs: Vec<(String, String)> =
                vec![(root_name.to_owned(), "object".to_owned())];
            let mut tgt_vert_specs: Vec<(String, String)> =
                vec![(root_name.to_owned(), "object".to_owned())];
            let mut src_edges: Vec<Edge> = Vec::new();
            let mut tgt_edges: Vec<Edge> = Vec::new();

            let mut vertex_remap: HashMap<Name, Name> = HashMap::new();
            let mut edge_remap: HashMap<Edge, Edge> = HashMap::new();

            for i in 0..n {
                let src_child = format!("src_child{i}");
                let tgt_child = format!("tgt_child{i}");
                let key = format!("key{i}");
                src_vert_specs.push((src_child.clone(), "string".to_owned()));
                tgt_vert_specs.push((tgt_child.clone(), "string".to_owned()));

                let src_edge = Edge {
                    src: root_name.into(),
                    tgt: Name::from(src_child.as_str()),
                    kind: "prop".into(),
                    name: Some(Name::from(key.as_str())),
                };
                let tgt_edge = Edge {
                    src: root_name.into(),
                    tgt: Name::from(tgt_child.as_str()),
                    kind: "prop".into(),
                    name: Some(Name::from(key.as_str())),
                };
                src_edges.push(src_edge.clone());
                tgt_edges.push(tgt_edge.clone());

                // Non-empty vertex_remap and edge_remap: the migration
                // renames every child vertex and its incoming edge.
                vertex_remap.insert(
                    Name::from(src_child.as_str()),
                    Name::from(tgt_child.as_str()),
                );
                edge_remap.insert(src_edge, tgt_edge);
            }

            let src_vert_refs: Vec<(&str, &str)> = src_vert_specs
                .iter()
                .map(|(a, b)| (a.as_str(), b.as_str()))
                .collect();
            let tgt_vert_refs: Vec<(&str, &str)> = tgt_vert_specs
                .iter()
                .map(|(a, b)| (a.as_str(), b.as_str()))
                .collect();
            let src_schema = make_schema(&src_vert_refs, &src_edges);
            let tgt_schema = make_schema(&tgt_vert_refs, &tgt_edges);

            // Build the source instance over the source (un-renamed) schema.
            let mut nodes = HashMap::new();
            nodes.insert(0, Node::new(0, root_name));
            let mut arcs: Vec<(u32, u32, Edge)> = Vec::new();
            for (i, val) in values.iter().enumerate() {
                let node_id = u32::try_from(i + 1).unwrap();
                let src_child = format!("src_child{i}");
                nodes.insert(
                    node_id,
                    Node::new(node_id, src_child.as_str())
                        .with_value(FieldPresence::Present(Value::Str(val.clone()))),
                );
                arcs.push((0, node_id, src_edges[i].clone()));
            }
            let instance = WInstance::new(nodes, arcs, vec![], 0, root_name.into());

            // Surviving vertices/edges are keyed by *target* anchors, since
            // `wtype_restrict` checks survival against the remapped anchor.
            let surviving_verts: HashSet<Name> = tgt_schema.vertices.keys().cloned().collect();
            let surviving_edges: HashSet<Edge> = tgt_schema.edges.keys().cloned().collect();
            let lens = Lens {
                compiled: CompiledMigration {
                    surviving_verts,
                    surviving_edges,
                    vertex_remap,
                    edge_remap,
                    resolver: HashMap::new(),
                    hyper_resolver: HashMap::new(),
                    field_transforms: HashMap::new(),
                    conditional_survival: HashMap::new(),
                    op_term_assignments: HashMap::new(),
                    expansion_path: HashMap::new(),
                },
                src_schema,
                tgt_schema,
            };
            (lens, instance)
        }

        /// Generate an identity-structure lens carrying interior
        /// `FieldTransform`s (`RenameField`, `DropField`, `AddField`) on the
        /// root object's `extra_fields`. Each transform snapshots the
        /// pre-transform fields into the complement, so `GetPut` holds.
        fn arb_field_transform_scenario() -> impl Strategy<Value = (Lens, WInstance)> {
            (
                "[a-z]{1,6}".prop_map(String::from),
                "[a-z]{1,6}".prop_map(String::from),
                "[a-z]{1,6}".prop_map(String::from),
                prop::collection::vec("[a-z]{1,6}".prop_map(String::from), 1..=3),
            )
                .prop_map(|(a_val, b_val, add_val, leaf_vals)| {
                    build_field_transform_scenario(&a_val, &b_val, &add_val, &leaf_vals)
                })
        }

        /// Build the interior-field-transform scenario.
        fn build_field_transform_scenario(
            a_val: &str,
            b_val: &str,
            add_val: &str,
            leaf_vals: &[String],
        ) -> (Lens, WInstance) {
            use panproto_inst::FieldTransform;

            let root_name = "root";
            let mut vert_specs: Vec<(String, String)> =
                vec![(root_name.to_owned(), "object".to_owned())];
            let mut edges = Vec::new();
            let child_names: Vec<String> =
                (0..leaf_vals.len()).map(|i| format!("child{i}")).collect();
            for name in &child_names {
                vert_specs.push((name.clone(), "string".to_owned()));
                edges.push(Edge {
                    src: root_name.into(),
                    tgt: Name::from(name.as_str()),
                    kind: "prop".into(),
                    name: Some(Name::from(name.as_str())),
                });
            }
            let vert_refs: Vec<(&str, &str)> = vert_specs
                .iter()
                .map(|(a, b)| (a.as_str(), b.as_str()))
                .collect();
            let schema = make_schema(&vert_refs, &edges);

            let mut root = Node::new(0, root_name);
            root.extra_fields
                .insert("field_a".to_owned(), Value::Str(a_val.to_owned()));
            root.extra_fields
                .insert("field_b".to_owned(), Value::Str(b_val.to_owned()));
            root.extra_fields
                .insert("keep".to_owned(), Value::Str("kept".to_owned()));
            let mut nodes = HashMap::new();
            nodes.insert(0, root);
            let mut arcs = Vec::new();
            for (i, val) in leaf_vals.iter().enumerate() {
                let node_id = u32::try_from(i + 1).unwrap();
                nodes.insert(
                    node_id,
                    Node::new(node_id, child_names[i].as_str())
                        .with_value(FieldPresence::Present(Value::Str(val.clone()))),
                );
                arcs.push((0, node_id, edges[i].clone()));
            }
            let instance = WInstance::new(nodes, arcs, vec![], 0, root_name.into());

            // Interior transforms on the root object (an interior anchor:
            // it has children). Rename one field, drop another, add a third.
            let transforms = vec![
                FieldTransform::RenameField {
                    old_key: "field_a".to_owned(),
                    new_key: "field_a_renamed".to_owned(),
                },
                FieldTransform::DropField {
                    key: "field_b".to_owned(),
                },
                FieldTransform::AddField {
                    key: "field_c".to_owned(),
                    value: Value::Str(add_val.to_owned()),
                },
            ];
            let mut field_transforms: HashMap<Name, Vec<FieldTransform>> = HashMap::new();
            field_transforms.insert(Name::from(root_name), transforms);

            let surviving_verts: HashSet<Name> = schema.vertices.keys().cloned().collect();
            let surviving_edges: HashSet<Edge> = schema.edges.keys().cloned().collect();
            let lens = Lens {
                compiled: CompiledMigration {
                    surviving_verts,
                    surviving_edges,
                    vertex_remap: HashMap::new(),
                    edge_remap: HashMap::new(),
                    resolver: HashMap::new(),
                    hyper_resolver: HashMap::new(),
                    field_transforms,
                    conditional_survival: HashMap::new(),
                    op_term_assignments: HashMap::new(),
                    expansion_path: HashMap::new(),
                },
                src_schema: schema.clone(),
                tgt_schema: schema,
            };
            (lens, instance)
        }

        /// Generate an identity-structure lens carrying an interior
        /// `FieldTransform::ApplyExpr` on the root object's `extra_fields`
        /// (the DSL `ApplyExpr` step's compile target). The forward expr
        /// uppercases a field and the inverse lowercases it; the complement
        /// snapshots the pre-transform fields, so `GetPut` holds regardless
        /// of the expression's own round-trip fidelity.
        fn arb_apply_expr_scenario() -> impl Strategy<Value = (Lens, WInstance)> {
            (
                "[a-z]{1,6}".prop_map(String::from),
                prop::collection::vec("[a-z]{1,6}".prop_map(String::from), 1..=3),
            )
                .prop_map(|(field_val, leaf_vals)| {
                    build_apply_expr_scenario(&field_val, &leaf_vals)
                })
        }

        /// Build the `ApplyExpr` scenario.
        fn build_apply_expr_scenario(field_val: &str, leaf_vals: &[String]) -> (Lens, WInstance) {
            use panproto_expr::{BuiltinOp, Expr};
            use panproto_inst::FieldTransform;
            use std::sync::Arc;

            let root_name = "root";
            let mut vert_specs: Vec<(String, String)> =
                vec![(root_name.to_owned(), "object".to_owned())];
            let mut edges = Vec::new();
            let child_names: Vec<String> =
                (0..leaf_vals.len()).map(|i| format!("child{i}")).collect();
            for name in &child_names {
                vert_specs.push((name.clone(), "string".to_owned()));
                edges.push(Edge {
                    src: root_name.into(),
                    tgt: Name::from(name.as_str()),
                    kind: "prop".into(),
                    name: Some(Name::from(name.as_str())),
                });
            }
            let vert_refs: Vec<(&str, &str)> = vert_specs
                .iter()
                .map(|(a, b)| (a.as_str(), b.as_str()))
                .collect();
            let schema = make_schema(&vert_refs, &edges);

            let mut root = Node::new(0, root_name);
            root.extra_fields
                .insert("label".to_owned(), Value::Str(field_val.to_owned()));
            let mut nodes = HashMap::new();
            nodes.insert(0, root);
            let mut arcs = Vec::new();
            for (i, val) in leaf_vals.iter().enumerate() {
                let node_id = u32::try_from(i + 1).unwrap();
                nodes.insert(
                    node_id,
                    Node::new(node_id, child_names[i].as_str())
                        .with_value(FieldPresence::Present(Value::Str(val.clone()))),
                );
                arcs.push((0, node_id, edges[i].clone()));
            }
            let instance = WInstance::new(nodes, arcs, vec![], 0, root_name.into());

            let transform = FieldTransform::ApplyExpr {
                key: "label".to_owned(),
                expr: Expr::Builtin(BuiltinOp::Upper, vec![Expr::Var(Arc::from("label"))]),
                inverse: Some(Expr::Builtin(
                    BuiltinOp::Lower,
                    vec![Expr::Var(Arc::from("label"))],
                )),
                coercion_class: panproto_gat::CoercionClass::Retraction,
            };
            let mut field_transforms: HashMap<Name, Vec<FieldTransform>> = HashMap::new();
            field_transforms.insert(Name::from(root_name), vec![transform]);

            let surviving_verts: HashSet<Name> = schema.vertices.keys().cloned().collect();
            let surviving_edges: HashSet<Edge> = schema.edges.keys().cloned().collect();
            let lens = Lens {
                compiled: CompiledMigration {
                    surviving_verts,
                    surviving_edges,
                    vertex_remap: HashMap::new(),
                    edge_remap: HashMap::new(),
                    resolver: HashMap::new(),
                    hyper_resolver: HashMap::new(),
                    field_transforms,
                    conditional_survival: HashMap::new(),
                    op_term_assignments: HashMap::new(),
                    expansion_path: HashMap::new(),
                },
                src_schema: schema.clone(),
                tgt_schema: schema,
            };
            (lens, instance)
        }

        // -------------------------------------------------------------------
        // Generated view mutations.
        // -------------------------------------------------------------------

        /// A generated plan for mutating a view. Applying it produces a
        /// schema-compatible mutant that differs from the original view in
        /// a random subset of its strict payload and metadata coordinates.
        #[derive(Debug, Clone)]
        pub(super) struct ViewMutation {
            /// Bitmask selecting which view nodes (in sorted-id order) to
            /// mutate. Mutating a subset rather than every node is the point:
            /// the canned mutator touches every scalar leaf uniformly.
            node_mask: u64,
            /// Suffix appended to selected string carriers (guaranteed
            /// non-empty, so the mutant genuinely differs).
            string_suffix: String,
            /// Delta added (wrapping) to selected integer carriers.
            int_delta: i64,
            /// Whether to flip selected boolean carriers.
            flip_bool: bool,
            /// Whether to also mutate `extra_fields` values (not only the
            /// node's scalar `value`).
            mutate_extra_fields: bool,
            /// Metadata-coordinate bits: discriminator, position, shape,
            /// and annotations, from least to most significant.
            metadata_mask: u8,
        }

        /// Strategy producing arbitrary [`ViewMutation`] plans.
        pub(super) fn arb_view_mutation() -> impl Strategy<Value = ViewMutation> {
            (
                any::<u64>(),
                "[a-z]{1,4}".prop_map(String::from),
                -5i64..=5i64,
                any::<bool>(),
                any::<bool>(),
                any::<bool>(),
                any::<bool>(),
                any::<bool>(),
                any::<bool>(),
            )
                .prop_map(
                    |(
                        node_mask,
                        string_suffix,
                        int_delta,
                        flip_bool,
                        mutate_extra_fields,
                        mutate_discriminator,
                        mutate_position,
                        mutate_shape,
                        mutate_annotations,
                    )| {
                        ViewMutation {
                            node_mask,
                            string_suffix,
                            int_delta,
                            flip_bool,
                            mutate_extra_fields,
                            metadata_mask: u8::from(mutate_discriminator)
                                | (u8::from(mutate_position) << 1)
                                | (u8::from(mutate_shape) << 2)
                                | (u8::from(mutate_annotations) << 3),
                        }
                    },
                )
        }

        /// Apply a [`ViewMutation`] plan to `view`, returning a new instance.
        ///
        /// The mutation is type-preserving: strings stay strings, integers
        /// stay integers, booleans stay booleans. Only the selected nodes
        /// (per `node_mask`) are touched.
        pub(super) fn apply_view_mutation(view: &WInstance, m: &ViewMutation) -> WInstance {
            let mut mutant = view.clone();
            let mut ids: Vec<u32> = mutant.nodes.keys().copied().collect();
            ids.sort_unstable();
            for (idx, id) in ids.iter().enumerate() {
                let selected = idx < 64 && (m.node_mask >> idx) & 1 == 1;
                if !selected {
                    continue;
                }
                let Some(node) = mutant.nodes.get_mut(id) else {
                    continue;
                };
                match node.value {
                    Some(FieldPresence::Present(Value::Str(ref mut s))) => {
                        s.push_str(&m.string_suffix);
                    }
                    Some(FieldPresence::Present(Value::Int(ref mut i))) => {
                        *i = i.wrapping_add(m.int_delta);
                    }
                    Some(FieldPresence::Present(Value::Bool(ref mut b))) if m.flip_bool => {
                        *b = !*b;
                    }
                    _ => {}
                }
                if m.mutate_extra_fields {
                    for v in node.extra_fields.values_mut() {
                        match v {
                            Value::Str(s) => s.push_str(&m.string_suffix),
                            Value::Int(i) => *i = i.wrapping_add(m.int_delta),
                            Value::Bool(b) if m.flip_bool => *b = !*b,
                            _ => {}
                        }
                    }
                }
                if m.metadata_mask & 1 != 0 {
                    node.discriminator = node
                        .discriminator
                        .as_ref()
                        .map_or_else(|| Some("__property__".into()), |_| None);
                }
                if m.metadata_mask & 2 != 0 {
                    node.position = Some(alternate_position(node.position));
                }
                if m.metadata_mask & 4 != 0 {
                    node.shape = alternate_shape(&node.shape);
                }
                if m.metadata_mask & 8 != 0 {
                    node.annotations.insert(
                        "__property__".to_owned(),
                        Value::Str(m.string_suffix.clone()),
                    );
                }
            }
            mutant
        }

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(128))]

            /// A depth-3 nested tree with grandchild nodes round-trips under
            /// the identity lens.
            #[test]
            fn nested_tree_satisfies_laws(
                (lens, instance) in arb_nested_tree_scenario()
            ) {
                prop_assert!(
                    check_laws(&lens, &instance).is_ok(),
                    "nested identity lens should satisfy all laws: {:?}",
                    check_laws(&lens, &instance),
                );
            }

            /// A lossless rename remap (non-empty vertex_remap + edge_remap)
            /// is well-behaved.
            #[test]
            fn remap_lens_satisfies_laws(
                (lens, instance) in arb_remap_lens_scenario()
            ) {
                prop_assert!(
                    check_laws(&lens, &instance).is_ok(),
                    "rename remap lens should satisfy all laws: {:?}",
                    check_laws(&lens, &instance),
                );
                prop_assert!(check_get_put(&lens, &instance).is_ok());
            }

            /// Interior field transforms (rename/drop/add) preserve GetPut,
            /// since the complement snapshots the pre-transform fields.
            ///
            /// Together with [`arb_apply_expr_scenario`] below and the
            /// existing `identity_lens_with_compute_field` proptest, this
            /// covers the compile targets of the DSL field-level steps
            /// (RemoveField/DropField, RenameField, AddField, ApplyExpr,
            /// ComputeField) over generated instances.
            #[test]
            fn field_transform_satisfies_get_put(
                (lens, instance) in arb_field_transform_scenario()
            ) {
                prop_assert!(
                    check_get_put(&lens, &instance).is_ok(),
                    "interior field transforms should satisfy GetPut: {:?}",
                    check_get_put(&lens, &instance),
                );
            }

            /// The `ApplyExpr` field step preserves GetPut over generated
            /// instances.
            #[test]
            fn field_apply_expr_satisfies_get_put(
                (lens, instance) in arb_apply_expr_scenario()
            ) {
                prop_assert!(
                    check_get_put(&lens, &instance).is_ok(),
                    "ApplyExpr field transform should satisfy GetPut: {:?}",
                    check_get_put(&lens, &instance),
                );
            }

            /// PutGet over generated view mutations (identity lens).
            #[test]
            fn identity_put_get_generated_mutation(
                (lens, instance) in arb_identity_lens_scenario(),
                mutation in arb_view_mutation(),
            ) {
                let (view, complement) = get(&lens, &instance).unwrap();
                let mutant = apply_view_mutation(&view, &mutation);
                prop_assert!(
                    check_put_get_with_view(&lens, &mutant, &complement).is_ok(),
                    "identity lens PutGet should hold for generated mutant",
                );
            }

            /// PutGet over generated view mutations (projection lens).
            #[test]
            fn projection_put_get_generated_mutation(
                (lens, instance) in arb_projection_lens_scenario(),
                mutation in arb_view_mutation(),
            ) {
                let (view, complement) = get(&lens, &instance).unwrap();
                let mutant = apply_view_mutation(&view, &mutation);
                prop_assert!(
                    check_put_get_with_view(&lens, &mutant, &complement).is_ok(),
                    "projection lens PutGet should hold for generated mutant",
                );
            }

            /// PutGet over generated view mutations on a nested tree, where
            /// the mutation also perturbs interior `extra_fields`.
            #[test]
            fn nested_put_get_generated_mutation(
                (lens, instance) in arb_nested_tree_scenario(),
                mutation in arb_view_mutation(),
            ) {
                let (view, complement) = get(&lens, &instance).unwrap();
                let mutant = apply_view_mutation(&view, &mutation);
                prop_assert!(
                    check_put_get_with_view(&lens, &mutant, &complement).is_ok(),
                    "nested identity lens PutGet should hold for generated mutant",
                );
            }

            /// PutPut driven by a generated second view (identity lens).
            #[test]
            fn identity_put_put_generated_mutation(
                (lens, instance) in arb_identity_lens_scenario(),
                mutation in arb_view_mutation(),
            ) {
                let (view, _) = get(&lens, &instance).unwrap();
                let first = apply_view_mutation(&view, &mutation);
                prop_assume!(!instances_equivalent(&first, &view));
                let second = perturb_view_leaves(&view);
                prop_assert!(
                    check_put_put(&lens, &instance, &first, &second).is_ok(),
                    "identity lens PutPut should hold for generated second view",
                );
            }

            /// PutPut driven by a generated second view (projection lens).
            #[test]
            fn projection_put_put_generated_mutation(
                (lens, instance) in arb_projection_lens_scenario(),
                mutation in arb_view_mutation(),
            ) {
                let (view, _) = get(&lens, &instance).unwrap();
                let first = apply_view_mutation(&view, &mutation);
                prop_assume!(!instances_equivalent(&first, &view));
                let second = perturb_view_leaves(&view);
                prop_assert!(
                    check_put_put(&lens, &instance, &first, &second).is_ok(),
                    "projection lens PutPut should hold for generated second view",
                );
            }
        }
    }
}
