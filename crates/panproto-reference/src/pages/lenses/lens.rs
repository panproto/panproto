//! The graph lens: get is restrict with its complement; put restores a
//! source from a view and a complement on the put domain.

use std::collections::{BTreeMap, BTreeSet};

/// One equality class of concrete complement payloads.
///
/// Complement composition inspects payloads only for equality. The reference
/// algebra therefore replaces each concrete node, edge, fan, field map,
/// value, anchor, or synthesis key by a caller-chosen atom identifying its
/// equality class. This keeps the operation independent of every production
/// crate while retaining every coordinate on which composition is defined.
pub type ComplementAtom = u32;

/// A normalized tree-complement in the reference partial monoid.
///
/// The two vector coordinates of the production representation are sets
/// here, because their presentation order is not part of complement
/// equality. `arc_order` remains a sequence: it is one global resource, so
/// two nonempty operands must state exactly the same sequence.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ComplementModel {
    /// Dropped node payload by node id.
    pub dropped_nodes: BTreeMap<u32, ComplementAtom>,
    /// Equality classes of dropped arcs.
    pub dropped_arcs: BTreeSet<ComplementAtom>,
    /// Equality classes of dropped fans.
    pub dropped_fans: BTreeSet<ComplementAtom>,
    /// Contraction choice payload by endpoint pair.
    pub contraction_choices: BTreeMap<(u32, u32), ComplementAtom>,
    /// Original parent by node id.
    pub original_parent: BTreeMap<u32, u32>,
    /// Original extra-field payload by node id.
    pub original_extra_fields: BTreeMap<u32, ComplementAtom>,
    /// Source-edge payload by arc endpoints.
    pub arc_edges: BTreeMap<(u32, u32), ComplementAtom>,
    /// The one global source-arc sequence.
    pub arc_order: Vec<(u32, u32)>,
    /// Original value payload by node id.
    pub original_values: BTreeMap<u32, ComplementAtom>,
    /// Synthesized view-node ids.
    pub synthesized_nodes: BTreeSet<u32>,
    /// Surviving ancestor by contracted node id.
    pub contracted_into: BTreeMap<u32, u32>,
    /// Original source-anchor payload by node id.
    pub original_anchors: BTreeMap<u32, ComplementAtom>,
    /// Fresh-node allocation floor.
    pub id_floor: u32,
    /// Structural synthesis-key payload by synthesized node id.
    pub synthesized_keys: BTreeMap<u32, ComplementAtom>,
    /// Legacy source-schema fingerprint; zero means absent.
    pub source_fingerprint: u64,
    /// Equality class of the complete identity envelope; absent means legacy
    /// when the fingerprint is nonzero and unbound otherwise.
    pub identity: Option<ComplementAtom>,
}

impl ComplementModel {
    /// Whether the complement records no non-derived residue or reassembly
    /// state, matching the production tree complement's unit predicate.
    ///
    /// Provenance, the floor, contraction bookkeeping, and synthesis keys do
    /// not make an otherwise empty complement data-bearing.
    #[must_use]
    pub fn records_nothing(&self) -> bool {
        self.dropped_nodes.is_empty()
            && self.dropped_arcs.is_empty()
            && self.dropped_fans.is_empty()
            && self.original_parent.is_empty()
            && self.original_extra_fields.is_empty()
            && self.arc_edges.is_empty()
            && self.arc_order.is_empty()
            && self.original_values.is_empty()
            && self.synthesized_nodes.is_empty()
            && self.original_anchors.is_empty()
    }

    /// Check that the model belongs to the partial monoid's carrier.
    ///
    /// A structural synthesis key names at most one synthesized id. The
    /// production representation is a map in the other direction, so this
    /// injectivity condition is not supplied by its type. It is nevertheless
    /// required by the complement algebra: without it, putting an internally
    /// duplicate map on the left or on the right can change whether a merge is
    /// defined. The reference operation therefore checks both operands before
    /// applying the binary compatibility rules.
    ///
    /// # Errors
    ///
    /// Returns [`ComplementComposeRefusal::SynthesizedIdByKey`] when the
    /// first synthesis key in id order names two ids.
    pub fn check_internal_coherence(&self) -> Result<(), ComplementComposeRefusal> {
        let mut id_by_key = BTreeMap::new();
        for (&id, &key) in &self.synthesized_keys {
            if id_by_key.insert(key, id).is_some() {
                return Err(ComplementComposeRefusal::SynthesizedIdByKey);
            }
        }
        Ok(())
    }
}

/// The named coordinate that makes reference complement composition
/// undefined.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComplementComposeRefusal {
    /// Distinct nodes occupy one dropped-node id.
    DroppedNodes,
    /// Distinct choices occupy one contraction pair.
    ContractionChoices,
    /// Distinct parents are recorded for one node.
    OriginalParent,
    /// Distinct field snapshots are recorded for one node.
    OriginalExtraFields,
    /// Distinct source edges are recorded for one arc.
    ArcEdges,
    /// Distinct value snapshots are recorded for one node.
    OriginalValues,
    /// Distinct surviving ancestors are recorded for one node.
    ContractedInto,
    /// Distinct source anchors are recorded for one node.
    OriginalAnchors,
    /// One synthesized id is assigned two structural keys.
    SynthesizedKeyById,
    /// One structural key is assigned to two synthesized ids.
    SynthesizedIdByKey,
    /// Two nonempty global arc sequences disagree.
    ArcOrder,
    /// A data-bearing complement carries neither identity nor fingerprint.
    UnboundData,
    /// Two legacy fingerprints disagree.
    LegacyFingerprint,
    /// A verified identity is paired with a legacy identity.
    MixedProvenance,
    /// Two verified identity envelopes disagree.
    EnvelopeMismatch,
}

fn merge_keyed<K: Clone + Ord, V: Clone + Eq>(
    left: &BTreeMap<K, V>,
    right: &BTreeMap<K, V>,
    refusal: ComplementComposeRefusal,
) -> Result<BTreeMap<K, V>, ComplementComposeRefusal> {
    let mut merged = left.clone();
    for (key, value) in right {
        match merged.get(key) {
            None => {
                merged.insert(key.clone(), value.clone());
            }
            Some(existing) if existing == value => {}
            Some(_) => return Err(refusal),
        }
    }
    Ok(merged)
}

const fn merge_fingerprints(left: u64, right: u64) -> Result<u64, ComplementComposeRefusal> {
    match (left, right) {
        (0, value) | (value, 0) => Ok(value),
        (left, right) if left == right => Ok(left),
        _ => Err(ComplementComposeRefusal::LegacyFingerprint),
    }
}

fn compose_complement_provenance(
    left: &ComplementModel,
    right: &ComplementModel,
) -> Result<(Option<ComplementAtom>, u64), ComplementComposeRefusal> {
    let unbound =
        |value: &ComplementModel| value.identity.is_none() && value.source_fingerprint == 0;
    if (unbound(left) && !left.records_nothing()) || (unbound(right) && !right.records_nothing()) {
        return Err(ComplementComposeRefusal::UnboundData);
    }
    if unbound(left) {
        return Ok((right.identity, right.source_fingerprint));
    }
    if unbound(right) {
        return Ok((left.identity, left.source_fingerprint));
    }
    match (left.identity, right.identity) {
        (Some(left_identity), Some(right_identity)) if left_identity == right_identity => Ok((
            Some(left_identity),
            merge_fingerprints(left.source_fingerprint, right.source_fingerprint)?,
        )),
        (Some(_), Some(_)) => Err(ComplementComposeRefusal::EnvelopeMismatch),
        (Some(_), None) | (None, Some(_)) => Err(ComplementComposeRefusal::MixedProvenance),
        (None, None) => Ok((
            None,
            merge_fingerprints(left.source_fingerprint, right.source_fingerprint)?,
        )),
    }
}

fn merge_synthesized_keys(
    left: &BTreeMap<u32, ComplementAtom>,
    right: &BTreeMap<u32, ComplementAtom>,
) -> Result<BTreeMap<u32, ComplementAtom>, ComplementComposeRefusal> {
    let mut merged = left.clone();
    let mut ids_by_key: BTreeMap<ComplementAtom, u32> =
        left.iter().map(|(&id, &key)| (key, id)).collect();
    for (&id, &key) in right {
        if merged.get(&id).is_some_and(|existing| *existing != key) {
            return Err(ComplementComposeRefusal::SynthesizedKeyById);
        }
        if ids_by_key.get(&key).is_some_and(|existing| *existing != id) {
            return Err(ComplementComposeRefusal::SynthesizedIdByKey);
        }
        merged.insert(id, key);
        ids_by_key.insert(key, id);
    }
    Ok(merged)
}

/// Compose two normalized tree complements.
///
/// This is the dependency-free reference definition of L.6. Composition is
/// set union on the two unordered coordinates, agreement union on every
/// keyed coordinate, maximum on the allocation floor, and agreement with an
/// empty unit on the global arc sequence. Synthesis keys must be injective in
/// both directions. Provenance follows the identity table: an unbound empty
/// complement is the unit; legacy fingerprints and verified envelopes must
/// agree; and legacy and verified operands do not mix.
///
/// # Errors
///
/// The first incompatible coordinate in the operation's declaration order.
pub fn compose_complement(
    left: &ComplementModel,
    right: &ComplementModel,
) -> Result<ComplementModel, ComplementComposeRefusal> {
    // Internal coherence is the carrier precondition, checked symmetrically
    // so malformed inputs cannot make definedness depend on operand order.
    left.check_internal_coherence()?;
    right.check_internal_coherence()?;
    let (identity, source_fingerprint) = compose_complement_provenance(left, right)?;
    let dropped_nodes = merge_keyed(
        &left.dropped_nodes,
        &right.dropped_nodes,
        ComplementComposeRefusal::DroppedNodes,
    )?;
    let mut dropped_arcs = left.dropped_arcs.clone();
    dropped_arcs.extend(right.dropped_arcs.iter().copied());
    let mut dropped_fans = left.dropped_fans.clone();
    dropped_fans.extend(right.dropped_fans.iter().copied());
    let contraction_choices = merge_keyed(
        &left.contraction_choices,
        &right.contraction_choices,
        ComplementComposeRefusal::ContractionChoices,
    )?;
    let original_parent = merge_keyed(
        &left.original_parent,
        &right.original_parent,
        ComplementComposeRefusal::OriginalParent,
    )?;
    let original_extra_fields = merge_keyed(
        &left.original_extra_fields,
        &right.original_extra_fields,
        ComplementComposeRefusal::OriginalExtraFields,
    )?;
    let arc_edges = merge_keyed(
        &left.arc_edges,
        &right.arc_edges,
        ComplementComposeRefusal::ArcEdges,
    )?;
    let original_values = merge_keyed(
        &left.original_values,
        &right.original_values,
        ComplementComposeRefusal::OriginalValues,
    )?;
    let mut synthesized_nodes = left.synthesized_nodes.clone();
    synthesized_nodes.extend(right.synthesized_nodes.iter().copied());
    let contracted_into = merge_keyed(
        &left.contracted_into,
        &right.contracted_into,
        ComplementComposeRefusal::ContractedInto,
    )?;
    let original_anchors = merge_keyed(
        &left.original_anchors,
        &right.original_anchors,
        ComplementComposeRefusal::OriginalAnchors,
    )?;
    let arc_order = match (left.arc_order.is_empty(), right.arc_order.is_empty()) {
        (true, _) => right.arc_order.clone(),
        (_, true) => left.arc_order.clone(),
        (false, false) if left.arc_order == right.arc_order => left.arc_order.clone(),
        (false, false) => return Err(ComplementComposeRefusal::ArcOrder),
    };
    let synthesized_keys = merge_synthesized_keys(&left.synthesized_keys, &right.synthesized_keys)?;
    Ok(ComplementModel {
        dropped_nodes,
        dropped_arcs,
        dropped_fans,
        contraction_choices,
        original_parent,
        original_extra_fields,
        arc_edges,
        arc_order,
        original_values,
        synthesized_nodes,
        contracted_into,
        original_anchors,
        id_floor: left.id_floor.max(right.id_floor),
        synthesized_keys,
        source_fingerprint,
        identity,
    })
}

pub use super::domain::{PutDomainError, ViewRef};
pub use super::graph::{
    GraphLens, IdentityError, IdentityStatus, LensError, cascade_universal_property,
    graph_lens_identity, is_subinstance, largest_exact_preimage, on_delta_fragment, present,
    subinstances,
};
pub use super::laws::{
    BasisCase, CongruenceCase, Divergence, DivergenceKind, DomainExpectation, EqualityClass,
    FusionCheck, FusionObservation, GateReport, GraphShape, LawFailure, LawGate, LawGateStatus,
    LawObstruction, LawOperation, LawReport, LawShape, LensShape, LocalityReport, Mutation,
    OutcomeClass, PutGetMode, ReferenceLawError, SynthIndex, TableLawError, TableShape,
    TreeLawComplement, TreeShape, check_graph_laws, check_laws_report, check_locality_report,
    check_table_laws, check_tree_laws, compare_fusion, put,
};
pub use super::protolens::{
    InstantiatedLens, Protolens, ProtolensApplication, ProtolensError, SchemaPrecondition, get,
    protolens_instance, protolens_schema,
};
pub use super::squares::{
    GraphSquareObservation, RelationalTableMigration, SquareError, SquareResult,
    SquareRouteObservation, TableSquareObservation, graph_put_square, graph_square,
    relational_table_get, relational_table_put, table_put_square, table_square,
};
pub use super::tree::{TreeOperationalError, tree_check_put_domain, tree_get, tree_put};
