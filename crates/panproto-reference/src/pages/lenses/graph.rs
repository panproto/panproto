//! Graph-lens restoration and its put-domain checks.

use std::collections::{BTreeMap, BTreeSet};

use super::domain::{
    PutDomainError, ViewRef, key_part_present, key_uniqueness_violations, part_column,
    required_value_violations, structural_required_cell_violations, uniqueness_role,
};
use super::tree::TreeOperationalError;
use crate::fan::{candidates, violations_at};
use crate::migration::Migration;
use crate::model::{GEdge, Graph, Name, Node, SEdge};
use crate::pages::representation::canonical::{ComplementShape, IdentityEnvelope};
use crate::restrict::{
    Complement, EdgeFate, PairResolution, Plan, PlanError, PutFieldRefusal, SynthKey, ValueLayer,
    is_fan_kind, map_back, reverse_anchor,
};
use crate::schema::{KeyRole, Schema};
use crate::value::{Presence, Value};

const SCHEMA_CONTEXT: &str = "panproto 2026-09 schema identity v2";
const LENS_CONTEXT: &str = "panproto 2026-09 compiled lens identity v1";
const GRAPH_COMPLEMENT_FORMAT_VERSION: u16 = 1;

/// The exact durable identity of a graph or tree complement made by a lens.
///
/// Boundary digests cover every canonical schema field. The lens digest
/// covers both boundaries followed by every field of the finite reference
/// migration, under the same contexts as production.
#[must_use]
pub fn graph_lens_identity(
    source: &Schema,
    target: &Schema,
    migration: &Migration,
    shape: ComplementShape,
) -> IdentityEnvelope {
    let source_schema =
        crate::blake3_ref::derive_key(SCHEMA_CONTEXT, &crate::schema::canonical_bytes(source));
    let target_schema =
        crate::blake3_ref::derive_key(SCHEMA_CONTEXT, &crate::schema::canonical_bytes(target));
    let mut lens = crate::blake3_ref::Hasher::new_derive_key(LENS_CONTEXT);
    lens.update(&source_schema);
    lens.update(&target_schema);
    lens.update(&crate::restrict::canonical_migration_bytes(migration));
    IdentityEnvelope {
        source_schema,
        target_schema,
        lens: lens.finalize(),
        shape,
        format_version: GRAPH_COMPLEMENT_FORMAT_VERSION,
    }
}

/// The identity state accepted by a checked graph or tree put.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdentityStatus {
    /// The complement carries the exact expected identity.
    Verified,
    /// The complement is empty and carries no identity.
    Unbound,
}

/// Why a complement cannot be used by this lens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IdentityError {
    /// A stamped complement names another boundary, lens, shape, or format.
    Mismatch {
        /// Identity expected by the receiving lens and shape.
        expected: Box<IdentityEnvelope>,
        /// Identity carried by the complement.
        found: Box<IdentityEnvelope>,
    },
    /// A complement records residue but has no identity.
    UnboundComplement,
}

/// A graph- or tree-lens refusal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LensError {
    /// Complement identity is checked before the put domain.
    Identity(IdentityError),
    /// The first graph put-domain violation in report order.
    PutDomain(PutDomainError),
    /// Every graph put-domain violation in report order.
    PutDomainReport(Vec<PutDomainError>),
    /// Tree carrier construction or readback failed operationally.
    TreeOperational(TreeOperationalError),
}

impl LensError {
    /// Whether this error report contains a put-domain violation.
    #[must_use]
    pub fn contains(&self, error: &PutDomainError) -> bool {
        match self {
            Self::PutDomain(found) => found == error,
            Self::PutDomainReport(found) => found.contains(error),
            Self::Identity(_) | Self::TreeOperational(_) => false,
        }
    }
}

/// The lens: two schemas, a migration, its compiled plan and its value
/// layer.
pub struct GraphLens<'a> {
    /// Source schema.
    pub(super) src: &'a Schema,
    /// Target schema.
    pub(super) tgt: &'a Schema,
    /// Migration.
    pub(super) m: &'a Migration,
    /// Compiled plan.
    pub(super) plan: &'a Plan,
    /// Source protocol meanings for declared constraint sorts.
    constraint_semantics: BTreeMap<Name, crate::validation::Semantics>,
    /// Value layer.
    pub(super) vl: &'a dyn ValueLayer,
    /// Boundary- and migration-bound graph identity.
    identity: IdentityEnvelope,
}

impl<'a> GraphLens<'a> {
    /// Build a graph lens only when `plan` was compiled for these exact
    /// source and target schemas and migration.
    ///
    /// # Errors
    ///
    /// [`PlanError::SourceSchemaMismatch`] or
    /// [`PlanError::TargetSchemaMismatch`] when a plan is reused across a
    /// different schema endpoint, or [`PlanError::MigrationMismatch`] when
    /// it is reused with a different migration.
    pub fn new(
        src: &'a Schema,
        tgt: &'a Schema,
        m: &'a Migration,
        plan: &'a Plan,
        constraint_semantics: &BTreeMap<Name, crate::validation::Semantics>,
        vl: &'a dyn ValueLayer,
    ) -> Result<Self, PlanError> {
        plan.validate_for(src, tgt, m)?;
        Ok(Self {
            src,
            tgt,
            m,
            plan,
            constraint_semantics: constraint_semantics.clone(),
            vl,
            identity: graph_lens_identity(src, tgt, m, ComplementShape::Graph),
        })
    }

    /// Exact graph-complement identity of this lens.
    #[must_use]
    pub const fn identity(&self) -> &IdentityEnvelope {
        &self.identity
    }

    /// Run graph get and stamp its complement with this lens's exact
    /// boundary and migration identity.
    ///
    /// # Errors
    ///
    /// The first restrict refusal.
    pub fn get(
        &self,
        source: &Graph,
    ) -> Result<(Graph, Complement), crate::restrict::RestrictError> {
        let (view, mut complement) =
            crate::restrict::get_with_plan(self.src, self.tgt, self.m, self.plan, self.vl, source)?;
        complement.identity = Some(self.identity);
        Ok((view, complement))
    }

    pub(super) fn identity_for_shape(&self, shape: ComplementShape) -> IdentityEnvelope {
        if shape == ComplementShape::Graph {
            self.identity
        } else {
            graph_lens_identity(self.src, self.tgt, self.m, shape)
        }
    }

    /// Source-protocol meanings needed by the independent tree domain
    /// checker. The checker shares these immutable declarations, not the
    /// graph restoration engine.
    pub(super) const fn constraint_semantics(
        &self,
    ) -> &BTreeMap<Name, crate::validation::Semantics> {
        &self.constraint_semantics
    }
}

/// The part of `view` that put can restore given the clause (3) and (4)
/// violations `structural`: without each node with no single source anchor,
/// its incident edges, and each edge with no single source label.
fn restorable_part(view: &Graph, structural: &[PutDomainError]) -> Graph {
    let mut restorable = view.clone();
    for violation in structural {
        match violation {
            PutDomainError::AmbiguousReverseAnchor {
                elem: ViewRef::Node(id),
                ..
            } => {
                restorable.nodes.remove(id);
                restorable
                    .edges
                    .retain(|_, edge| edge.src != *id && edge.tgt != *id);
            }
            PutDomainError::UnrepresentableViewEdge {
                elem: ViewRef::Edge(id),
            }
            | PutDomainError::ContractedEdgeInserted {
                elem: ViewRef::Edge(id),
            } => {
                restorable.edges.remove(id);
            }
            _ => {}
        }
    }
    restorable
}

/// Whether a violation is one of the clauses read on the restored source:
/// (5) to (8), and the field clauses of (6).
const fn reads_the_restored_source(violation: &PutDomainError) -> bool {
    matches!(
        violation,
        PutDomainError::SurvivalPredicateFails { .. }
            | PutDomainError::SurvivalPredicateRefused { .. }
            | PutDomainError::DerivedCellEdited { .. }
            | PutDomainError::TransformNotInvertible { .. }
            | PutDomainError::DuplicateKey { .. }
            | PutDomainError::MissingRequiredCell { .. }
            | PutDomainError::MalformedFan { .. }
    )
}

/// A view node or edge reusing the id of a dropped source node or edge.
fn id_collisions(view: &Graph, c: &Complement) -> Vec<PutDomainError> {
    let nodes = view
        .nodes
        .keys()
        .filter(|i| c.dropped_nodes.contains_key(i))
        .map(|id| ViewRef::Node(*id));
    let edges = view
        .edges
        .keys()
        .filter(|i| c.dropped_edges.contains_key(i))
        .map(|id| ViewRef::Edge(*id));
    nodes
        .chain(edges)
        .map(|elem| PutDomainError::IdCollision { elem })
        .collect()
}

/// The put-domain error of a value layer's field refusal at `elem`.
fn field_refusal(elem: ViewRef, refusal: PutFieldRefusal) -> PutDomainError {
    match refusal {
        PutFieldRefusal::DerivedCellEdited { field } => {
            PutDomainError::DerivedCellEdited { elem, field }
        }
        PutFieldRefusal::TransformNotInvertible { field } => {
            PutDomainError::TransformNotInvertible { elem, field }
        }
    }
}

/// The expected content of a synthesized object.
enum Expected {
    /// A node with this anchor.
    Node(Name),
    /// An edge with this label and endpoints.
    Edge(SEdge, SynthOrId, SynthOrId),
}

/// Builds the synthesized key of each hop of a chain.
type KeyOf = Box<dyn Fn(u16) -> SynthKey>;

/// The expected content of a synthesized object, read from its key, the
/// plan and the source anchors.
fn expected_synth(
    lens: &GraphLens<'_>,
    c: &Complement,
    key: &SynthKey,
    is_node: bool,
    anchor_of: &dyn Fn(u32) -> Option<Name>,
) -> Option<Expected> {
    let (s, t, inter, labels, hop, key_of): (u32, u32, Vec<Name>, Vec<SEdge>, u16, KeyOf) =
        match key {
            SynthKey::Contract { src, tgt, edge } => {
                return (!is_node).then(|| {
                    Expected::Edge(edge.clone(), SynthOrId::Id(*src), SynthOrId::Id(*tgt))
                });
            }
            SynthKey::Expand { source_edge, hop } => {
                let e = c.dropped_edges.get(source_edge)?;
                let EdgeFate::Expand {
                    intermediates,
                    labels,
                } = lens.plan.fates.get(&e.label)?
                else {
                    return None;
                };
                let se = *source_edge;
                (
                    e.src,
                    e.tgt,
                    intermediates.clone(),
                    labels.clone(),
                    *hop,
                    Box::new(move |h| SynthKey::Expand {
                        source_edge: se,
                        hop: h,
                    }),
                )
            }
            SynthKey::ContractExpand { src, tgt, hop } => {
                let pair = lens.plan.pairs.get(&(anchor_of(*src)?, anchor_of(*tgt)?))?;
                let PairResolution::Expand {
                    intermediates,
                    labels,
                } = &pair.resolution
                else {
                    return None;
                };
                let (s, t) = (*src, *tgt);
                (
                    s,
                    t,
                    intermediates.clone(),
                    labels.clone(),
                    *hop,
                    Box::new(move |h| SynthKey::ContractExpand {
                        src: s,
                        tgt: t,
                        hop: h,
                    }),
                )
            }
        };
    let h = usize::from(hop);
    if is_node {
        return inter.get(h).map(|a| Expected::Node(a.clone()));
    }
    let label = labels.get(h)?;
    let from = if h == 0 {
        SynthOrId::Id(s)
    } else {
        SynthOrId::Synth(Box::new(key_of(hop - 1)))
    };
    let to = if h == inter.len() {
        SynthOrId::Id(t)
    } else {
        SynthOrId::Synth(Box::new(key_of(hop)))
    };
    Some(Expected::Edge(label.clone(), from, to))
}

/// An endpoint named by id or by synthesized key.
#[derive(Clone, Debug, PartialEq, Eq)]
enum SynthOrId {
    Id(u32),
    Synth(Box<SynthKey>),
}

/// The source-origin endpoints of a synthesized object's chain.
fn chain_ends(c: &Complement, key: &SynthKey) -> Vec<u32> {
    match key {
        SynthKey::Contract { src, tgt, .. } | SynthKey::ContractExpand { src, tgt, .. } => {
            vec![*src, *tgt]
        }
        SynthKey::Expand { source_edge, .. } => c
            .dropped_edges
            .get(source_edge)
            .map(|e| vec![e.src, e.tgt])
            .unwrap_or_default(),
    }
}

impl GraphLens<'_> {
    /// The source anchor of a view node: its recorded anchor when the image
    /// of that anchor is its view anchor, else the unique preimage of its
    /// view anchor; `Err` with the candidates when there are several.
    fn source_anchor(&self, c: &Complement, n: &Node) -> Result<Name, Vec<Name>> {
        if let Some(a) = c.original_anchors.get(&n.id)
            && self.m.vertex_image(a) == n.anchor
        {
            return Ok(a.clone());
        }
        let pre = reverse_anchor(self.src, self.m, &n.anchor);
        match pre.as_slice() {
            [one] => Ok(one.clone()),
            _ => Err(pre),
        }
    }

    /// The source label of a non-synthesized view edge: the recorded one
    /// when the view label is its image and the endpoints are unchanged,
    /// else the put rule's unique preimage. `Err` with the domain error.
    fn edge_label(
        &self,
        c: &Complement,
        view: &Graph,
        id: u32,
        e: &GEdge,
        anchor: &dyn Fn(u32) -> Option<Name>,
    ) -> Result<SEdge, PutDomainError> {
        if let Some(r) = c.original_edges.get(&id)
            && r.src == e.src
            && r.tgt == e.tgt
            && self.image_of(c, view, r, e)
        {
            return Ok(r.label.clone());
        }
        match map_back(self.src, self.m, self.plan, view, e, anchor) {
            Ok(l) => Ok(l),
            Err(0) if !is_fan_kind(&e.label.kind) => {
                let planned = anchor(e.src)
                    .zip(anchor(e.tgt))
                    .and_then(|p| self.plan.pairs.get(&p));
                if planned.is_some_and(|p| p.resolution == PairResolution::Label(e.label.clone())) {
                    Err(PutDomainError::ContractedEdgeInserted {
                        elem: ViewRef::Edge(id),
                    })
                } else {
                    Err(PutDomainError::UnrepresentableViewEdge {
                        elem: ViewRef::Edge(id),
                    })
                }
            }
            Err(_) => Err(PutDomainError::UnrepresentableViewEdge {
                elem: ViewRef::Edge(id),
            }),
        }
    }

    /// Whether the view label of `e` is the image of the recorded edge
    /// `r`: for a non-fan edge, the label its fate keeps; for a fan edge,
    /// the complete label selected by the fan's governing surviving/full
    /// shape, target hyper-edge and endpoint anchors.
    fn image_of(&self, c: &Complement, view: &Graph, r: &GEdge, e: &GEdge) -> bool {
        if is_fan_kind(&r.label.kind) {
            return self.expected_view_fan_label(c, view, r).as_ref() == Some(&e.label);
        }
        matches!(self.plan.fates.get(&r.label), Some(EdgeFate::Keep(i)) if *i == e.label)
    }

    /// The exact forward label of a recorded source fan edge in `view`.
    fn expected_view_fan_label(
        &self,
        c: &Complement,
        view: &Graph,
        recorded: &GEdge,
    ) -> Option<SEdge> {
        let fan = c
            .original_anchors
            .get(&recorded.src)
            .or_else(|| view.nodes.get(&recorded.src).map(|node| &node.anchor))?;
        let target = view.nodes.get(&recorded.tgt)?;
        let (surviving, full) = source_fan_shapes(view, c, recorded.src);
        let entry = fan_resolver_entry(self.m, fan, &surviving, &full);
        let hyper = entry.map_or_else(|| fan.clone(), |(target, _)| target.clone());
        let name = if recorded.label.kind == crate::restrict::HYPER_PARENT {
            recorded.label.name.as_ref().map(|_| {
                self.tgt.hyper_edges.get(&hyper).map_or_else(
                    || recorded.label.name.clone().unwrap_or_default(),
                    |h| h.parent_label.clone(),
                )
            })
        } else {
            recorded.label.name.as_ref().map(|name| {
                entry
                    .and_then(|(_, labels)| labels.get(name))
                    .cloned()
                    .unwrap_or_else(|| name.clone())
            })
        };
        Some(SEdge {
            src: hyper,
            tgt: target.anchor.clone(),
            kind: recorded.label.kind.clone(),
            name,
        })
    }

    /// Definition: The graph put domain is empty exactly when restoration
    /// has no violation; otherwise return every violation in report order.
    ///
    /// # Errors
    ///
    /// The sorted violations.
    ///
    /// Book anchors: `L-lenses-1`, `L-lenses-2`, `L-lenses-3`,
    /// `L-lenses-4`, `L-lenses-5`, `L-lenses-6`, `L-lenses-7`,
    /// `L-lenses-8`, `L-lenses-9`, `L-lenses-10`.
    pub fn check_put_domain(
        &self,
        view: &Graph,
        c: &Complement,
    ) -> Result<(), Vec<PutDomainError>> {
        self.restore_unchecked(view, c)
            .map(|_| ())
            .map_err(|mut errors| {
                PutDomainError::sort_report(&mut errors);
                errors
            })
    }

    /// Definition: Graph put is the restored source, or the first domain violation.
    ///
    /// # Errors
    ///
    /// The first violation in report order.
    ///
    /// Book anchors: `L-lenses-1`, `L-lenses-2`, `L-lenses-3`,
    /// `L-lenses-4`, `L-lenses-5`, `L-lenses-6`, `L-lenses-7`,
    /// `L-lenses-8`, `L-lenses-9`, `L-lenses-10`.
    pub fn put(&self, view: &Graph, c: &Complement) -> Result<Graph, LensError> {
        self.put_checked_for_shape(view, c, ComplementShape::Graph)
            .map(|(source, _)| source)
    }

    /// Graph put with the accepted complement-identity state.
    ///
    /// # Errors
    ///
    /// Identity mismatch or the first put-domain violation, in that order.
    pub fn put_checked(
        &self,
        view: &Graph,
        c: &Complement,
    ) -> Result<(Graph, IdentityStatus), LensError> {
        self.put_checked_for_shape(view, c, ComplementShape::Graph)
    }

    pub(super) fn put_checked_for_shape(
        &self,
        view: &Graph,
        c: &Complement,
        shape: ComplementShape,
    ) -> Result<(Graph, IdentityStatus), LensError> {
        let status = self.verify_identity(c, shape)?;
        self.restore_unchecked(view, c)
            .map(|source| (source, status))
            .map_err(|mut errors| {
                PutDomainError::sort_report(&mut errors);
                LensError::PutDomain(errors.remove(0))
            })
    }

    pub(super) fn verify_identity(
        &self,
        complement: &Complement,
        shape: ComplementShape,
    ) -> Result<IdentityStatus, LensError> {
        let expected = self.identity_for_shape(shape);
        match complement.identity {
            Some(found) if found == expected => Ok(IdentityStatus::Verified),
            Some(found) => Err(LensError::Identity(IdentityError::Mismatch {
                expected: Box::new(expected),
                found: Box::new(found),
            })),
            None if complement_records_nothing(complement) => Ok(IdentityStatus::Unbound),
            None => Err(LensError::Identity(IdentityError::UnboundComplement)),
        }
    }
}

fn complement_records_nothing(complement: &Complement) -> bool {
    complement.dropped_nodes.is_empty()
        && complement.dropped_edges.is_empty()
        && complement.original_edges.is_empty()
        && complement.original_anchors.is_empty()
        && complement.original_fields.is_empty()
        && complement.original_values.is_empty()
        && complement.synthesized_nodes.is_empty()
        && complement.synthesized_edges.is_empty()
}

impl GraphLens<'_> {
    fn synth_violations(
        &self,
        view: &Graph,
        comp: &Complement,
        anchor_of: &dyn Fn(u32) -> Option<Name>,
    ) -> Vec<PutDomainError> {
        let mut out = Vec::new();
        let id_of_key = |key: &SynthKey| {
            comp.synthesized_nodes
                .iter()
                .find(|(_, other)| *other == key)
                .map(|(id_found, _)| *id_found)
        };
        let resolve = |end: &SynthOrId| match end {
            SynthOrId::Id(node) => Some(*node),
            SynthOrId::Synth(key) => id_of_key(key),
        };
        let explained = |key: &SynthKey| {
            chain_ends(comp, key)
                .iter()
                .any(|node| !view.nodes.contains_key(node))
        };
        for (id, key) in &comp.synthesized_nodes {
            let ok = view.nodes.get(id).map_or_else(
                || explained(key),
                |node| matches!(expected_synth(self, comp, key, true, anchor_of), Some(Expected::Node(anchor)) if anchor == node.anchor
                    && node.value.is_none()
                    && node.fields.is_empty()
                    && node.discriminator.is_none()
                    && node.position.is_none()
                    && node.shape == crate::model::NodeShape::Plain
                    && node.annotations.is_empty()),
            );
            if !ok {
                out.push(PutDomainError::SynthesizedEdited {
                    elem: ViewRef::Node(*id),
                });
            }
        }
        for (id, key) in &comp.synthesized_edges {
            let ok = view.edges.get(id).map_or_else(
                || explained(key),
                |edge| {
                    matches!(expected_synth(self, comp, key, false, anchor_of),
                        Some(Expected::Edge(label, from, to)) if label == edge.label && resolve(&from) == Some(edge.src) && resolve(&to) == Some(edge.tgt) && edge.position.is_none())
                },
            );
            if !ok {
                out.push(PutDomainError::SynthesizedEdited {
                    elem: ViewRef::Edge(*id),
                });
            }
        }
        out
    }

    /// Restoration and every put-domain violation.
    ///
    /// Clauses (1) `IdCollision` and (2) `SynthesizedEdited` read the view
    /// and complement alone and always run. Clauses (3) and (4) map every
    /// view edge and node back. The clauses read on the restored source,
    /// (5) to (8), run only when (3) and (4) report nothing, since only then
    /// is there a restored source; a view node whose id collides with a
    /// dropped node is restored in that node's place.
    pub(super) fn restore_unchecked(
        &self,
        view: &Graph,
        c: &Complement,
    ) -> Result<Graph, Vec<PutDomainError>> {
        let mut errs = id_collisions(view, c);
        let mut structural = Vec::new();
        let mut field_refusals = Vec::new();
        let mut anchors: BTreeMap<u32, Name> = BTreeMap::new();
        let mut out = Graph::default();
        for (id, n) in view
            .nodes
            .iter()
            .filter(|(i, _)| !c.synthesized_nodes.contains_key(i))
        {
            match self.source_anchor(c, n) {
                Ok(a) => {
                    anchors.insert(*id, a);
                }
                Err(candidates) => structural.push(PutDomainError::AmbiguousReverseAnchor {
                    elem: ViewRef::Node(*id),
                    candidates,
                }),
            }
        }
        let anchor_of = |n: u32| {
            anchors
                .get(&n)
                .cloned()
                .or_else(|| c.dropped_nodes.get(&n).map(|x| x.anchor.clone()))
        };
        errs.extend(self.synth_violations(view, c, &anchor_of));
        for (id, n) in view
            .nodes
            .iter()
            .filter(|(i, _)| !c.synthesized_nodes.contains_key(i))
        {
            match self
                .vl
                .restore(n, c.original_fields.get(id), c.original_values.get(id))
            {
                Ok((fields, value)) => {
                    let anchor = anchors.get(id).cloned().unwrap_or_else(|| n.anchor.clone());
                    out.nodes.insert(
                        *id,
                        Node {
                            anchor,
                            value,
                            fields,
                            ..n.clone()
                        },
                    );
                }
                Err(refusal) => field_refusals.push(field_refusal(ViewRef::Node(*id), refusal)),
            }
        }
        for (id, e) in view
            .edges
            .iter()
            .filter(|(i, _)| !c.synthesized_edges.contains_key(i))
        {
            match self.edge_label(c, view, *id, e, &anchor_of) {
                Ok(label) => {
                    out.edges.insert(*id, GEdge { label, ..e.clone() });
                }
                Err(err) => structural.push(err),
            }
        }
        let removed: BTreeSet<u32> = c
            .dropped_edges
            .values()
            .flat_map(|e| [e.src, e.tgt])
            .filter(|n| !c.dropped_nodes.contains_key(n) && !view.nodes.contains_key(n))
            .collect();
        for (id, n) in &c.dropped_nodes {
            out.nodes.entry(*id).or_insert_with(|| n.clone());
        }
        for (id, e) in &c.dropped_edges {
            if !removed.contains(&e.src) && !removed.contains(&e.tgt) {
                out.edges.entry(*id).or_insert_with(|| e.clone());
            }
        }
        if structural.is_empty() {
            errs.extend(field_refusals);
            errs.extend(self.source_violations(view, c, &out));
            errs.extend(self.hidden_by_the_next_get(view, c, &out));
        } else {
            // The clauses read on the restored source run on the restorable
            // part: the view without each node with no single source anchor
            // (and its incident edges) and without each edge that maps back
            // to no single source label.
            let restorable = restorable_part(view, &structural);
            errs.extend(structural);
            if let Err(again) = self.restore_unchecked(&restorable, c) {
                errs.extend(again.into_iter().filter(reads_the_restored_source));
            }
        }
        if errs.is_empty() { Ok(out) } else { Err(errs) }
    }

    /// Clause (5) by its reason: a restored, non-synthesized view node that
    /// the next get of the restored source would not keep is
    /// `SurvivalPredicateFails`, whether a survival predicate or the
    /// restriction itself (for instance a fan with no surviving child)
    /// drops it. A get that refuses reports nothing here; its predicate
    /// refusals are those of [`Self::source_violations`].
    fn hidden_by_the_next_get(
        &self,
        view: &Graph,
        c: &Complement,
        out: &Graph,
    ) -> Vec<PutDomainError> {
        let Ok((next, _)) = self.get(out) else {
            return Vec::new();
        };
        view.nodes
            .keys()
            .filter(|id| {
                !c.synthesized_nodes.contains_key(id)
                    && out.nodes.contains_key(id)
                    && !next.nodes.contains_key(id)
            })
            .map(|id| PutDomainError::SurvivalPredicateFails {
                elem: ViewRef::Node(*id),
            })
            .collect()
    }

    /// The violations read on the restored source: survival predicates of
    /// restored view nodes, declared keys and required cells, and the fan
    /// domain.
    fn source_violations(&self, view: &Graph, c: &Complement, out: &Graph) -> Vec<PutDomainError> {
        let mut errs = Vec::new();
        for (id, n) in out
            .nodes
            .iter()
            .filter(|(i, _)| view.nodes.contains_key(i) && !c.synthesized_nodes.contains_key(i))
        {
            if !self.m.conditional.contains(&n.anchor) {
                continue;
            }
            match self.vl.survives(out, n) {
                Ok(true) => {}
                Ok(false) => errs.push(PutDomainError::SurvivalPredicateFails {
                    elem: ViewRef::Node(*id),
                }),
                Err(refusal) => errs.push(PutDomainError::SurvivalPredicateRefused {
                    elem: ViewRef::Node(*id),
                    refusal,
                }),
            }
        }
        errs.extend(required_value_violations(
            self.src,
            &self.constraint_semantics,
            out,
            ViewRef::Node,
        ));
        errs.extend(structural_required_cell_violations(
            self.src,
            out,
            ViewRef::Node,
        ));
        for (vertex, ks) in &self.src.keys {
            for key in ks {
                if uniqueness_role(key.role) {
                    errs.extend(key_uniqueness_violations(out, vertex, key, ViewRef::Node));
                }
                if key.role != KeyRole::Primary {
                    continue;
                }
                for id in out
                    .nodes
                    .iter()
                    .filter(|(i, n)| n.anchor == *vertex && view.nodes.contains_key(i))
                    .map(|(i, _)| i)
                {
                    for p in &key.parts {
                        if !key_part_present(out, *id, p) {
                            errs.push(PutDomainError::MissingRequiredCell {
                                elem: ViewRef::Node(*id),
                                field: part_column(p),
                            });
                        }
                    }
                }
            }
        }
        errs.extend(candidates(out).into_iter().flat_map(|node| {
            violations_at(out, node, Some(self.src))
                .into_iter()
                .map(move |reason| PutDomainError::MalformedFan { node, reason })
        }));
        PutDomainError::sort_report(&mut errs);
        errs
    }
}

/// The source child-label shapes of a view fan after and before pruning.
fn source_fan_shapes(
    view: &Graph,
    complement: &Complement,
    fan: u32,
) -> (BTreeSet<Name>, BTreeSet<Name>) {
    let mut surviving = BTreeSet::new();
    let mut full = BTreeSet::new();
    for (id, edge) in &view.edges {
        if edge.src != fan || edge.label.kind != crate::restrict::HYPER_LABEL {
            continue;
        }
        let source = complement.original_edges.get(id).unwrap_or(edge);
        if let Some(name) = &source.label.name {
            surviving.insert(name.clone());
            full.insert(name.clone());
        }
    }
    full.extend(
        complement
            .dropped_edges
            .values()
            .filter(|edge| edge.src == fan && edge.label.kind == crate::restrict::HYPER_LABEL)
            .filter_map(|edge| edge.label.name.clone()),
    );
    (surviving, full)
}

/// Select the resolver for a fan: surviving shape, full shape, then the
/// source hyper-edge's sole unambiguous entry.
fn fan_resolver_entry<'a>(
    migration: &'a Migration,
    fan: &str,
    surviving: &BTreeSet<Name>,
    full: &BTreeSet<Name>,
) -> Option<&'a (Name, BTreeMap<Name, Name>)> {
    let surviving_key = (fan.to_owned(), surviving.clone());
    if let Some(entry) = migration.hyper_resolver.get(&surviving_key) {
        return Some(entry);
    }
    let full_key = (fan.to_owned(), full.clone());
    if full != surviving
        && let Some(entry) = migration.hyper_resolver.get(&full_key)
    {
        return Some(entry);
    }
    let mut entries = migration
        .hyper_resolver
        .iter()
        .filter(|((source, _), _)| source == fan)
        .map(|(_, entry)| entry);
    let only = entries.next()?;
    entries.next().is_none().then_some(only)
}

/// Read a value's presence as a plain value, for the tests.
#[must_use]
pub fn present(n: &Node) -> Option<&Value> {
    match n.value.as_ref()? {
        Presence::Present(v) => Some(v),
        _ => None,
    }
}

/// Whether a lens lies on the Delta fragment of L.4: a subschema
/// restriction with monic deletions and no contraction, expansion, fan
/// retargeting, survival predicate or transform.
///
/// Every surviving source vertex keeps its own name and every surviving
/// source edge is kept as itself, the target schema's vertices and edges are
/// among the source schema's, and the migration states no resolver,
/// expansion path, hyper-edge resolver, survival predicate or transform.
///
/// Book anchors: `L-lenses-4`.
#[must_use]
pub fn on_delta_fragment(lens: &GraphLens<'_>) -> bool {
    let m = lens.m;
    m.vertex_remap.is_empty()
        && m.edge_remap.is_empty()
        && m.resolver.is_empty()
        && m.expansion_path.is_empty()
        && m.hyper_resolver.is_empty()
        && m.conditional.is_empty()
        && m.transformed.is_empty()
        && lens
            .tgt
            .vertices
            .iter()
            .all(|(v, kind)| lens.src.vertices.get(v) == Some(kind))
        && lens.tgt.edges.iter().all(|e| lens.src.edges.contains(e))
}

/// Whether `part` is a subinstance of `whole`.
///
/// Every node of `part` is a
/// node of `whole` with the same id and content, and every edge of `part`
/// is an edge of `whole` with the same id, endpoints, label and position.
///
/// Book anchors: `L-lenses-4`.
#[must_use]
pub fn is_subinstance(part: &Graph, whole: &Graph) -> bool {
    part.nodes
        .iter()
        .all(|(id, node)| whole.nodes.get(id) == Some(node))
        && part
            .edges
            .iter()
            .all(|(id, edge)| whole.edges.get(id) == Some(edge))
}

/// Every endpoint-closed subinstance of `source`: each set of its nodes
/// together with each set of the edges whose endpoints are both kept, in
/// ascending node-mask and then edge-mask order.
///
/// Book anchors: `L-lenses-4`.
#[must_use]
pub fn subinstances(source: &Graph) -> Vec<Graph> {
    let nodes: Vec<(&u32, &Node)> = source.nodes.iter().collect();
    let edges: Vec<(&u32, &GEdge)> = source.edges.iter().collect();
    let mut out = Vec::new();
    for node_mask in 0..(1_u64 << nodes.len()) {
        let mut base = Graph::default();
        for (index, (id, node)) in nodes.iter().enumerate() {
            if node_mask & (1 << index) != 0 {
                base.nodes.insert(**id, (*node).clone());
            }
        }
        let eligible: Vec<&(&u32, &GEdge)> = edges
            .iter()
            .filter(|(_, edge)| {
                base.nodes.contains_key(&edge.src) && base.nodes.contains_key(&edge.tgt)
            })
            .collect();
        for edge_mask in 0..(1_u64 << eligible.len()) {
            let mut graph = base.clone();
            for (index, (id, edge)) in eligible.iter().enumerate() {
                if edge_mask & (1 << index) != 0 {
                    graph.edges.insert(**id, (*edge).clone());
                }
            }
            out.push(graph);
        }
    }
    out
}

/// Definition: on the Delta fragment, the largest source subinstance whose
/// restriction is exactly the edited view.
///
/// For a lens on [`on_delta_fragment`] and a source `s`, the candidates are
/// the endpoint-closed subinstances of `s` whose reference get view equals
/// `edited`. The result is the candidate that contains every other
/// candidate, which is the cartesian reading of the L.4 cascade for
/// deletion-only view edits.
///
/// `None` when the lens is off the fragment, when a candidate's get refuses,
/// or when no candidate contains every other one (in particular when there
/// is no candidate, as for a view that is not a deletion of `get(s)`).
///
/// Book anchors: `L-lenses-4`.
#[must_use]
pub fn largest_exact_preimage(
    lens: &GraphLens<'_>,
    source: &Graph,
    edited: &Graph,
) -> Option<Graph> {
    if !on_delta_fragment(lens) {
        return None;
    }
    let mut exact = Vec::new();
    for candidate in subinstances(source) {
        let (view, _) = lens.get(&candidate).ok()?;
        if view == *edited {
            exact.push(candidate);
        }
    }
    exact
        .iter()
        .find(|largest| exact.iter().all(|other| is_subinstance(other, largest)))
        .cloned()
}

/// Definition: the universal property of the L.4 cascade on the Delta
/// fragment.
///
/// `result` is a subinstance of `source`, and every endpoint-closed
/// subinstance of `source` whose reference get view is a subinstance of
/// `edited` is a subinstance of `result`. `false` off the fragment or when a
/// candidate's get refuses.
///
/// Book anchors: `L-lenses-4`.
#[must_use]
pub fn cascade_universal_property(
    lens: &GraphLens<'_>,
    source: &Graph,
    edited: &Graph,
    result: &Graph,
) -> bool {
    if !on_delta_fragment(lens) || !is_subinstance(result, source) {
        return false;
    }
    subinstances(source)
        .iter()
        .all(|candidate| match lens.get(candidate) {
            Ok((view, _)) => !is_subinstance(&view, edited) || is_subinstance(candidate, result),
            Err(_) => false,
        })
}
