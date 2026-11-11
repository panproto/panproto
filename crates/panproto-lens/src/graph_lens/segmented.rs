//! Graph lens put over a view held as the segments of a partition.
//!
//! [`put_segmented`] restores a view laid out as segments (the output of a
//! segmented get, possibly edited) against the complement segments of that
//! get, one segment at a time, never assembling the view or the source. Each
//! segment restores the nodes it owns and the edges it holds against its own
//! complement segment, by the same restoration whole-state [`put`] runs,
//! with the nodes other segments own held as placeholders. What a segment
//! reads of another segment's nodes and edges arrives in three steps:
//!
//! - a census of edge ids, a merge of each segment's sorted runs of view,
//!   dropped, synthesized and recorded edge ids, which finds every view edge
//!   whose id a complement elsewhere also uses;
//! - one exchange of node facts: each segment asks the owner of every
//!   remote node its edges, dropped edges and synthesized chains name for
//!   that node's view anchor and its restored form;
//! - after restoration, the boundary of the restored family: every restored
//!   edge entering another segment becomes an in-stub there.
//!
//! Forward stability restricts the restored family segment by segment
//! ([`graph_restrict_segmented_nodes`]); uniqueness keys merge each
//! segment's sorted key runs. Every segment's violations are collected and
//! the least in report order is returned, so a refused put changes no
//! segment. The restored segments are laid out under the view's partition.
//!
//! [`put`]: super::put

use std::collections::{BTreeMap, BTreeSet};

use panproto_inst::identity::{ComplementShape, IdentityCarrier};
use panproto_inst::locality::{GlobalReason, Locality, SegmentOp, SummaryKind};
use panproto_inst::segment::{
    AnySegment, Boundary, Ghost, GraphComplementSegment, GraphSegment, InStub, OwnerIndex,
    Partition, PartitionDefect, Residual, SegmentError, SegmentId, SegmentedRestrictError,
    graph_restrict_locality, graph_restrict_segmented_nodes, verify_partition,
};
use panproto_inst::{
    EdgeId, GEdge, GInstance, GraphComplement, Node, NodeShape, PutDomainError, RestrictPlan,
    SynthKey, ViewRef,
};
use panproto_schema::{KeyPart, KeyRole, KeyScope, Schema};

use super::{
    PutScope, Restored, fan_violations, key_values, refusal_from_restrict, required_violations,
    restorable_part, restore_nodes, restore_parts, stability_against, synthesized_edits,
    uniqueness_role,
};
use crate::{Lens, LensError};

/// Why a segmented graph put returned no source.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SegmentedPutError {
    /// The view segments are not a valid family under the partition, or a
    /// complement segment is missing, repeated, or holds an entry about an
    /// element another segment owns.
    #[error(transparent)]
    Segment(SegmentError),
    /// Whole-state put of the assembled view refuses with this error: the
    /// least put-domain violation of every segment, in report order.
    #[error(transparent)]
    Lens(LensError),
    /// The lens's put is not supported segment by segment.
    #[error("{0}")]
    Residual(Residual),
}

/// The locality class of graph put along `lens`.
///
/// Put restricts the restored source again to check forward stability, so
/// a restrict that [`graph_restrict_locality`] classifies as global is
/// global here too. A uniqueness or primary key of the source schema that
/// reads a position among siblings, a scope found among ancestors, or a
/// path of more than one edge is [`GlobalReason::KeyDemand`] at the least
/// such vertex and key. Otherwise put exchanges node facts across the
/// boundary and merges sorted runs of edge ids and key values, so its class
/// is [`SummaryKind::SortedMerge`].
///
/// # Errors
///
/// The plan compilation error.
pub fn graph_put_locality(lens: &Lens) -> Result<Locality, LensError> {
    let plan = RestrictPlan::compile(&lens.src_schema, &lens.tgt_schema, &lens.compiled)?;
    Ok(put_locality(lens, &plan))
}

fn put_locality(lens: &Lens, plan: &RestrictPlan) -> Locality {
    let restrict = graph_restrict_locality(plan);
    if matches!(restrict, Locality::Global(_)) {
        return restrict;
    }
    if let Some(reason) = key_demand(&lens.src_schema) {
        return Locality::Global(reason);
    }
    Locality::of_clauses([
        restrict,
        Locality::Exchange,
        Locality::Summary(SummaryKind::SortedMerge),
    ])
}

/// The least key put reads beyond an element and its out-edges' targets.
fn key_demand(schema: &Schema) -> Option<GlobalReason> {
    schema
        .declarations
        .keys
        .iter()
        .flat_map(|(vertex, keys)| keys.iter().map(move |key| (vertex, key)))
        .filter(|(_, key)| {
            (uniqueness_role(&key.role) || key.role == KeyRole::Primary)
                && (matches!(key.scope, KeyScope::Within(_))
                    || key.parts.iter().any(|part| match part {
                        KeyPart::Position { .. } => true,
                        KeyPart::Path(path) => path.len() > 1,
                    }))
        })
        .map(|(vertex, key)| (vertex.clone(), key.id.clone()))
        .min()
        .map(|(vertex, key)| GlobalReason::KeyDemand { vertex, key })
}

/// One segment's part of the put: its view with placeholders, its
/// complement, and what it knows of other segments.
struct Part<'a> {
    view: &'a GraphSegment,
    /// The view segment's owned nodes and held edges, with a placeholder
    /// for each remote view node the segment reads.
    local: GInstance,
    /// The complement segment as a whole-state complement, stamped with the
    /// lens's graph identity.
    complement: GraphComplement,
    scope: PutScope,
}

/// Restore the graph a lens's view held as `views` under `partition`
/// against the complement segments `complements`, segment by segment.
///
/// Write `glue` for gluing segments under `partition` and `join` for joining
/// the complement segments into one complement stamped with the graph
/// identity of `lens`, as [`get`](super::get) stamps it. Then the output,
/// glued, is [`put`](super::put) of `glue(views)` against `join(complements)`,
/// and each output segment holds the restored nodes its segment owns and the
/// restored edges whose source it owns, with the boundary those edges
/// cross. When whole-state put refuses, the segmented put returns the same
/// refusal, which is the least violation found in any segment, and returns
/// no segment.
///
/// Every view segment restores against its own complement segment; the
/// segments exchange only a census of edge ids, the view anchors and
/// restored forms of the remote nodes their elements name, the restored
/// edges that cross the boundary, the forward-stability run of
/// [`graph_restrict_segmented_nodes`] and sorted runs of key values.
///
/// # Errors
///
/// [`SegmentedPutError::Residual`] with [`Residual::Global`] when
/// [`graph_put_locality`] classifies the lens as global, before any segment
/// is read; [`SegmentedPutError::Segment`] when the views are not a family
/// under `partition` ([`verify_partition`]), when the complements are not one
/// per segment in the views' order (a missing or repeated segment), when a
/// complement entry is about an element its segment does not own
/// ([`SegmentError::WrongOwner`] or [`SegmentError::Unowned`]), or when two
/// complement segments record one edge id
/// ([`SegmentError::DuplicateEdgeId`]); [`SegmentedPutError::Lens`] with the
/// refusal of whole-state put: [`LensError::PutDomain`] with
/// [`PutDomainError::PlanCompilation`] when the plan does not compile, and
/// otherwise the least put-domain violation in report order.
pub fn put_segmented(
    lens: &Lens,
    views: &[GraphSegment],
    complements: &[GraphComplementSegment],
    partition: &Partition,
) -> Result<Vec<GraphSegment>, SegmentedPutError> {
    let plan = RestrictPlan::compile(&lens.src_schema, &lens.tgt_schema, &lens.compiled).map_err(
        |error| {
            SegmentedPutError::Lens(LensError::PutDomain(Box::new(
                PutDomainError::PlanCompilation {
                    detail: error.to_string(),
                },
            )))
        },
    )?;
    if let Locality::Global(reason) = put_locality(lens, &plan) {
        return Err(SegmentedPutError::Residual(Residual::Global {
            op: SegmentOp::Put,
            reason,
        }));
    }
    let index = check_inputs(views, complements, partition).map_err(SegmentedPutError::Segment)?;
    let census = Census::of(views, complements).map_err(SegmentedPutError::Segment)?;

    // Id collisions are the least violations in report order, so when there
    // is one the least of them is the refusal.
    let mut collisions = census.collisions.clone();
    for (view, complement) in views.iter().zip(complements) {
        collisions.extend(
            view.nodes
                .keys()
                .filter(|id| complement.dropped_nodes.contains_key(id))
                .map(|&id| PutDomainError::IdCollision {
                    elem: ViewRef::Node(id),
                }),
        );
    }
    if let Some(least) = least(collisions) {
        return Err(refused(least));
    }

    let identity = lens.identity(ComplementShape::Graph);
    let mut parts: Vec<Part<'_>> = Vec::with_capacity(views.len());
    for (view, complement) in views.iter().zip(complements) {
        let mut whole = whole_complement(complement, &census, view);
        whole.stamp(identity.clone(), &lens.src_schema);
        let (local, foreign) = local_view(view).map_err(SegmentedPutError::Segment)?;
        parts.push(Part {
            view,
            local,
            complement: whole,
            scope: PutScope {
                foreign,
                synthesized_edges: census.synthesized_elsewhere(view),
                ..PutScope::default()
            },
        });
    }

    exchange_node_facts(lens, &mut parts, views, &index);

    // Restoration, segment by segment.
    let mut found: Vec<PutDomainError> = Vec::new();
    let mut restored: Vec<Restored> = Vec::with_capacity(parts.len());
    for part in &parts {
        found.extend(synthesized_edits(&part.local, &part.complement, &plan));
        let outcome = restore_parts(lens, &part.local, &part.complement, &plan, &part.scope);
        found.extend(outcome.errors.iter().cloned());
        found.extend(outcome.trace_errors.iter().cloned());
        restored.push(outcome);
    }

    // The clauses read on the restored source run on what restores, unless
    // a node's transforms cannot be traced, which leaves the restored source
    // undetermined.
    let mut family = Vec::new();
    if restored
        .iter()
        .all(|outcome| outcome.trace_errors.is_empty())
    {
        family = restored_family(views, &restored, &index)?;
        found.extend(source_clauses(
            lens, &plan, &parts, &restored, &family, partition,
        )?);
    }
    least(found).map_or(Ok(family), |error| Err(refused(error)))
}

/// The exchange of node facts: each owner offers the restored form of each
/// node it owns, restored from its own view and complement segments, and
/// each segment takes, for every remote node it reads, a placeholder with
/// the node's view anchor when the view holds it and the node's restored
/// anchor and value when it restores.
fn exchange_node_facts(
    lens: &Lens,
    parts: &mut [Part<'_>],
    views: &[GraphSegment],
    index: &OwnerIndex,
) {
    let offered: Vec<GInstance> = parts
        .iter()
        .map(|part| restore_nodes(lens, &part.local, &part.complement, &part.scope).0)
        .collect();
    let position: BTreeMap<SegmentId, usize> = views
        .iter()
        .enumerate()
        .map(|(at, view)| (view.segment, at))
        .collect();
    for part in parts.iter_mut() {
        for node in requests(part, index) {
            let Some(owner) = index
                .owner_of_node(node)
                .and_then(|owner| position.get(&owner))
                .and_then(|at| views.get(*at).zip(offered.get(*at)))
            else {
                continue;
            };
            if let Some(shown) = owner.0.nodes.get(&node)
                && !part.scope.foreign.contains(&node)
            {
                part.local = std::mem::take(&mut part.local)
                    .with_node(Node::new(node, shown.anchor.clone()));
                part.scope.foreign.insert(node);
            }
            if let Some(restored) = owner.1.node(node) {
                let mut placeholder = Node::new(node, restored.anchor.clone());
                placeholder.value.clone_from(&restored.value);
                part.scope.restored.insert(node, placeholder);
            }
        }
    }
}

fn refused(error: PutDomainError) -> SegmentedPutError {
    SegmentedPutError::Lens(LensError::PutDomain(Box::new(error)))
}

/// The least violation in report order.
fn least(mut found: Vec<PutDomainError>) -> Option<PutDomainError> {
    PutDomainError::sort_report(&mut found);
    found.into_iter().next()
}

/// Check the views as a family under `partition`, and that the complements
/// are one per view segment in the same order, each holding entries only
/// about the elements its segment owns.
fn check_inputs(
    views: &[GraphSegment],
    complements: &[GraphComplementSegment],
    partition: &Partition,
) -> Result<OwnerIndex, SegmentError> {
    let any: Vec<AnySegment> = views.iter().cloned().map(AnySegment::Graph).collect();
    verify_partition(&any, partition)?;
    let index = partition.owner_index()?;
    for (at, view) in views.iter().enumerate() {
        let Some(complement) = complements.get(at) else {
            return Err(SegmentError::MalformedPartition {
                segment: view.segment,
                defect: PartitionDefect::SegmentMissing,
            });
        };
        if complement.segment != view.segment {
            return Err(SegmentError::MalformedPartition {
                segment: complement.segment,
                defect: PartitionDefect::UnknownSegment,
            });
        }
    }
    if let Some(extra) = complements.get(views.len()) {
        return Err(SegmentError::MalformedPartition {
            segment: extra.segment,
            defect: PartitionDefect::SegmentRepeated,
        });
    }
    for complement in complements {
        let holder = complement.segment;
        let place = |elem: ViewRef, node: u32| match index.owner_of_node(node) {
            None => Err(SegmentError::Unowned { elem }),
            Some(owner) if owner != holder => Err(SegmentError::WrongOwner {
                elem,
                expected: owner,
                found: holder,
            }),
            Some(_) => Ok(()),
        };
        let nodes = complement
            .dropped_nodes
            .keys()
            .chain(complement.original_anchors.keys())
            .chain(complement.original_extra_fields.keys())
            .chain(complement.original_values.keys())
            .chain(complement.synthesized_nodes.keys());
        for id in nodes {
            place(ViewRef::Node(*id), *id)?;
        }
        for (id, edge) in complement
            .dropped_edges
            .iter()
            .chain(&complement.original_edges)
        {
            place(ViewRef::Edge(*id), edge.src)?;
        }
        for (id, key) in &complement.synthesized_edges {
            let chain = match key {
                SynthKey::Contract { src, .. } | SynthKey::ContractExpand { src, .. } => Some(*src),
                SynthKey::Expand { source_edge, .. } => complement
                    .dropped_edges
                    .get(source_edge)
                    .map(|edge| edge.src),
                SynthKey::ResolutionReference { .. } | SynthKey::ResolutionSpan { .. } => None,
            };
            match chain {
                Some(src) => place(ViewRef::Edge(*id), src)?,
                None => {
                    return Err(SegmentError::Unowned {
                        elem: ViewRef::Edge(*id),
                    });
                }
            }
        }
    }
    Ok(index)
}

/// The census of edge ids: the merge of every segment's ascending runs of
/// held view edge ids and of the dropped, synthesized and recorded edge ids
/// of its complement.
struct Census {
    /// A collision for each view edge whose id a complement records as
    /// dropped.
    collisions: Vec<PutDomainError>,
    /// The ids each complement records as synthesized, with the segment.
    synthesized: BTreeMap<EdgeId, SegmentId>,
    /// The recorded source form of each kept edge, by id, with the segment
    /// recording it.
    original: BTreeMap<EdgeId, (SegmentId, GEdge)>,
}

impl Census {
    fn of(
        views: &[GraphSegment],
        complements: &[GraphComplementSegment],
    ) -> Result<Self, SegmentError> {
        let mut dropped: BTreeMap<EdgeId, Vec<SegmentId>> = BTreeMap::new();
        let mut synthesized: BTreeMap<EdgeId, Vec<SegmentId>> = BTreeMap::new();
        let mut original: BTreeMap<EdgeId, Vec<(SegmentId, GEdge)>> = BTreeMap::new();
        for complement in complements {
            for id in complement.dropped_edges.keys() {
                dropped.entry(*id).or_default().push(complement.segment);
            }
            for id in complement.synthesized_edges.keys() {
                synthesized.entry(*id).or_default().push(complement.segment);
            }
            for (id, edge) in &complement.original_edges {
                original
                    .entry(*id)
                    .or_default()
                    .push((complement.segment, edge.clone()));
            }
        }
        let repeated = |edge: EdgeId, segments: Vec<SegmentId>| {
            if segments.len() > 1 {
                Err(SegmentError::DuplicateEdgeId { edge, segments })
            } else {
                Ok(())
            }
        };
        for (edge, segments) in &dropped {
            repeated(*edge, segments.clone())?;
        }
        for (edge, segments) in &synthesized {
            repeated(*edge, segments.clone())?;
        }
        for (edge, records) in &original {
            repeated(*edge, records.iter().map(|(segment, _)| *segment).collect())?;
        }
        let mut held: Vec<EdgeId> = views
            .iter()
            .flat_map(|view| view.edges.keys().copied())
            .collect();
        held.sort_unstable();
        let collisions = held
            .into_iter()
            .filter(|id| dropped.contains_key(id))
            .map(|id| PutDomainError::IdCollision {
                elem: ViewRef::Edge(id),
            })
            .collect();
        Ok(Self {
            collisions,
            synthesized: synthesized
                .into_iter()
                .filter_map(|(id, segments)| segments.first().map(|segment| (id, *segment)))
                .collect(),
            original: original
                .into_iter()
                .filter_map(|(id, mut records)| records.pop().map(|record| (id, record)))
                .collect(),
        })
    }

    /// The view edges `view` holds whose ids another segment's complement
    /// records as synthesized.
    fn synthesized_elsewhere(&self, view: &GraphSegment) -> BTreeSet<EdgeId> {
        view.edges
            .keys()
            .filter(|id| {
                self.synthesized
                    .get(id)
                    .is_some_and(|segment| *segment != view.segment)
            })
            .copied()
            .collect()
    }
}

/// A complement segment as a whole-state complement, with the recorded
/// source form of every view edge `view` holds whose record another segment
/// keeps.
fn whole_complement(
    complement: &GraphComplementSegment,
    census: &Census,
    view: &GraphSegment,
) -> GraphComplement {
    let mut whole = GraphComplement::empty();
    whole.dropped_nodes.clone_from(&complement.dropped_nodes);
    whole.dropped_edges.clone_from(&complement.dropped_edges);
    whole.original_edges.clone_from(&complement.original_edges);
    for id in view.edges.keys() {
        if let Some((segment, edge)) = census.original.get(id)
            && *segment != view.segment
        {
            whole.original_edges.insert(*id, edge.clone());
        }
    }
    whole
        .original_anchors
        .clone_from(&complement.original_anchors);
    whole
        .original_extra_fields
        .clone_from(&complement.original_extra_fields);
    whole
        .original_values
        .clone_from(&complement.original_values);
    whole
        .synthesized_nodes
        .clone_from(&complement.synthesized_nodes);
    whole
        .synthesized_edges
        .clone_from(&complement.synthesized_edges);
    whole.id_floor = complement.floors;
    whole
}

/// The view segment's owned nodes and held edges, with a placeholder
/// carrying the view anchor for each ghost, and the ids of the
/// placeholders.
fn local_view(view: &GraphSegment) -> Result<(GInstance, BTreeSet<u32>), SegmentError> {
    let mut local = GInstance::new();
    for node in view.nodes.values() {
        local = local.with_node(node.clone());
    }
    let mut foreign = BTreeSet::new();
    for ghost in view.boundary.ghosts.values() {
        if local.node(ghost.node).is_none() {
            local = local.with_node(Node::new(ghost.node, ghost.anchor.clone()));
            foreign.insert(ghost.node);
        }
    }
    for (id, edge) in &view.edges {
        local
            .insert_edge(*id, edge.clone())
            .map_err(|_| SegmentError::DanglingEndpoint {
                elem: ViewRef::Edge(*id),
                node: edge.tgt,
            })?;
    }
    Ok((local, foreign))
}

/// The remote nodes a segment's restoration reads: the targets of its view
/// edges and of its dropped edges, and the endpoints of its synthesized
/// chains, that another segment owns.
fn requests(part: &Part<'_>, index: &OwnerIndex) -> BTreeSet<u32> {
    let mut nodes: BTreeSet<u32> = part.view.edges.values().map(|edge| edge.tgt).collect();
    nodes.extend(
        part.complement
            .dropped_edges
            .values()
            .flat_map(|edge| [edge.src, edge.tgt]),
    );
    for key in part
        .complement
        .synthesized_nodes
        .values()
        .chain(part.complement.synthesized_edges.values())
    {
        if let SynthKey::Contract { src, tgt, .. } | SynthKey::ContractExpand { src, tgt, .. } = key
        {
            nodes.extend([*src, *tgt]);
        }
    }
    nodes.retain(|node| index.owner_of_node(*node) != Some(part.view.segment));
    nodes
}

/// The restored family: each segment's restored owned nodes and held
/// edges, with the boundary its crossing edges give it.
fn restored_family(
    views: &[GraphSegment],
    restored: &[Restored],
    index: &OwnerIndex,
) -> Result<Vec<GraphSegment>, SegmentedPutError> {
    let mut family: Vec<GraphSegment> = Vec::with_capacity(views.len());
    let mut crossings: Vec<(SegmentId, EdgeId, InStub, SegmentId, panproto_gat::Name)> = Vec::new();
    for (view, outcome) in views.iter().zip(restored) {
        let mut segment = GraphSegment {
            segment: view.segment,
            nodes: BTreeMap::new(),
            edges: BTreeMap::new(),
            boundary: Boundary::default(),
        };
        for (id, node) in outcome.source.nodes() {
            if index.owner_of_node(*id) == Some(view.segment) {
                segment.nodes.insert(*id, node.clone());
            }
        }
        for (id, edge) in outcome.source.edges() {
            segment.edges.insert(*id, edge.clone());
            if segment.nodes.contains_key(&edge.tgt) {
                continue;
            }
            let owner = index
                .owner_of_node(edge.tgt)
                .ok_or(SegmentedPutError::Segment(SegmentError::Unowned {
                    elem: ViewRef::Edge(*id),
                }))?;
            let anchor = |node: u32| {
                outcome
                    .source
                    .node(node)
                    .map(|node| node.anchor.clone())
                    .unwrap_or_default()
            };
            crossings.push((
                view.segment,
                *id,
                InStub {
                    src: edge.src,
                    src_owner: view.segment,
                    src_anchor: anchor(edge.src),
                    tgt: edge.tgt,
                    label: edge.edge.clone(),
                },
                owner,
                anchor(edge.tgt),
            ));
        }
        family.push(segment);
    }
    for (holder, id, stub, owner, anchor) in crossings {
        if let Some(segment) = family.iter_mut().find(|segment| segment.segment == holder) {
            segment.boundary.ghosts.entry(stub.tgt).or_insert(Ghost {
                node: stub.tgt,
                owner,
                anchor,
            });
        }
        if let Some(segment) = family.iter_mut().find(|segment| segment.segment == owner) {
            segment.boundary.in_stubs.insert(ViewRef::Edge(id), stub);
        }
    }
    Ok(family)
}

/// The clauses read on the restored source: forward stability, the source
/// schema's required cells and keys, and the fan domain.
fn source_clauses(
    lens: &Lens,
    plan: &RestrictPlan,
    parts: &[Part<'_>],
    restored: &[Restored],
    family: &[GraphSegment],
    partition: &Partition,
) -> Result<Vec<PutDomainError>, SegmentedPutError> {
    let mut found = Vec::new();
    let in_views: Vec<GInstance> = parts
        .iter()
        .zip(restored)
        .map(|(part, outcome)| restorable_part(&part.local, &outcome.errors))
        .collect();
    match graph_restrict_segmented_nodes(plan, family, partition) {
        Ok(regenerated) => {
            for (((part, outcome), in_view), nodes) in
                parts.iter().zip(restored).zip(&in_views).zip(&regenerated)
            {
                found.extend(stability_against(
                    lens,
                    in_view,
                    &part.complement,
                    &outcome.source,
                    plan,
                    &part.scope,
                    &|id| nodes.get(&id),
                ));
            }
        }
        Err(SegmentedRestrictError::Restrict { error, .. }) => {
            found.extend(refusal_from_restrict(error));
        }
        Err(SegmentedRestrictError::Segment(error)) => {
            return Err(SegmentedPutError::Segment(error));
        }
        Err(SegmentedRestrictError::Residual(residual)) => {
            return Err(SegmentedPutError::Residual(residual));
        }
        Err(other) => {
            return Err(SegmentedPutError::Lens(LensError::ComplementMismatch {
                detail: other.to_string(),
            }));
        }
    }
    let schema = &lens.src_schema;
    let mut keys = KeyRuns::default();
    for (((part, outcome), in_view), segment) in
        parts.iter().zip(restored).zip(&in_views).zip(family)
    {
        let mine = |id: u32| segment.nodes.contains_key(&id);
        found.extend(
            required_violations(&outcome.source, schema)
                .into_iter()
                .filter(|error| matches!(error, PutDomainError::MissingRequiredCell { elem: ViewRef::Node(id), .. } if mine(*id))),
        );
        keys.add(&outcome.source, schema, &mine, &|id| {
            in_view.nodes().contains_key(&id) && !part.scope.foreign.contains(&id)
        });
        found.extend(
            fan_violations(&fan_graph(&outcome.source, segment)?, schema)
                .into_iter()
                .filter(|error| matches!(error, PutDomainError::MalformedFan { node, .. } if mine(*node))),
        );
    }
    found.extend(keys.finish());
    Ok(found)
}

/// A segment's restored graph with every restored edge entering it from
/// another segment, its source held as a placeholder, so each owned node
/// sees all of its in-edges.
fn fan_graph(source: &GInstance, segment: &GraphSegment) -> Result<GInstance, SegmentedPutError> {
    let mut graph = source.clone();
    for (elem, stub) in &segment.boundary.in_stubs {
        let ViewRef::Edge(id) = elem else {
            continue;
        };
        if graph.node(stub.src).is_none() {
            graph = graph.with_node(Node::new(stub.src, stub.src_anchor.clone()));
        }
        graph
            .insert_edge(*id, GEdge::new(stub.src, stub.tgt, stub.label.clone()))
            .map_err(|_| {
                SegmentedPutError::Segment(SegmentError::InStubMismatch { elem: elem.clone() })
            })?;
    }
    Ok(graph)
}

/// The sorted runs of key values of every segment, merged per key.
#[derive(Default)]
struct KeyRuns {
    /// Per key (vertex and key id, with its parts): the canonical bytes, id
    /// and values of each element with a complete tuple, and the least
    /// element whose tuple the keying kernel refuses, with the refusal.
    runs: BTreeMap<(panproto_gat::Name, panproto_gat::Name), KeyRun>,
    /// The primary-key parts missing at restored view elements.
    missing: Vec<PutDomainError>,
}

struct KeyRun {
    parts: Vec<KeyPart>,
    entries: Vec<(Vec<u8>, u32, Vec<panproto_inst::Value>)>,
    refused: Option<(u32, PutDomainError)>,
}

impl KeyRuns {
    /// Add one segment's run: the elements it owns (`mine`) of every keyed
    /// vertex of `source`, and the primary-key parts missing at those that
    /// `in_view` admits.
    fn add(
        &mut self,
        source: &GInstance,
        schema: &Schema,
        mine: &dyn Fn(u32) -> bool,
        in_view: &dyn Fn(u32) -> bool,
    ) {
        for (vertex, keys) in &schema.declarations.keys {
            let mut ids: Vec<u32> = source
                .nodes()
                .iter()
                .filter_map(|(&id, node)| (&node.anchor == vertex && mine(id)).then_some(id))
                .collect();
            ids.sort_unstable();
            for key in keys {
                if uniqueness_role(&key.role) {
                    let run = self
                        .runs
                        .entry((vertex.clone(), key.id.clone()))
                        .or_insert_with(|| KeyRun {
                            parts: key.parts.clone(),
                            entries: Vec::new(),
                            refused: None,
                        });
                    for &id in &ids {
                        if source
                            .node(id)
                            .is_some_and(|node| node.shape == NodeShape::HyperEdge)
                        {
                            continue;
                        }
                        let Some(values) = key_values(source, id, &key.parts) else {
                            continue;
                        };
                        match panproto_inst::key::canonical_key_bytes(key, &values) {
                            Ok(bytes) => run.entries.push((bytes, id, values)),
                            Err(error) => {
                                if run.refused.as_ref().is_none_or(|(least, _)| id < *least) {
                                    run.refused = Some((
                                        id,
                                        PutDomainError::KeyRefused {
                                            table: vertex.clone(),
                                            key: key.id.clone(),
                                            reason: error.into(),
                                        },
                                    ));
                                }
                                break;
                            }
                        }
                    }
                }
                if key.role == KeyRole::Primary {
                    for &id in ids.iter().filter(|&&id| in_view(id)) {
                        for part in &key.parts {
                            if super::part_value(source, id, part).is_none() {
                                self.missing.push(PutDomainError::MissingRequiredCell {
                                    elem: ViewRef::Node(id),
                                    field: super::key_part_name(part),
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    /// Merge the runs: a key whose kernel refuses an element reports the
    /// refusal at the least such element and nothing else; otherwise every
    /// group of elements with equal key bytes is a duplicate, with the
    /// values of its least element.
    fn finish(self) -> Vec<PutDomainError> {
        let mut found = self.missing;
        for ((vertex, _), run) in self.runs {
            if let Some((_, refusal)) = run.refused {
                found.push(refusal);
                continue;
            }
            let mut entries = run.entries;
            entries.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
            let mut at = 0;
            while at < entries.len() {
                let end = entries[at..]
                    .iter()
                    .position(|entry| entry.0 != entries[at].0)
                    .map_or(entries.len(), |offset| at + offset);
                if end - at > 1 {
                    found.push(PutDomainError::DuplicateKey {
                        table: vertex.clone(),
                        key: run
                            .parts
                            .iter()
                            .map(super::key_part_name)
                            .zip(entries[at].2.clone())
                            .collect(),
                        elems: entries[at..end]
                            .iter()
                            .map(|entry| ViewRef::Node(entry.1))
                            .collect(),
                    });
                }
                at = end;
            }
        }
        found
    }
}
