//! Validation of one segment against a compiled validation plan.
//!
//! The local checks of a tree run on its native local forest. Tree fan
//! well-formedness is a sorted-merge summary, so a segment reports those
//! instances as outside its local scope; graph fan well-formedness remains a
//! local check over the stored reified graph. In every shape, ghosts and
//! in-stubs are context only. A report retains check instances whose subjects
//! are owned by the segment.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};

use super::body::{AnySegment, is_fan_node};
use super::check::check_segment;
use super::error::SegmentError;
use super::ids::SegmentId;
use super::partition::Partition;
use crate::fan_violation::FanViolation;
use crate::functor::FInstance;
use crate::ginstance::{EdgeId, GEdge, GInstance, HYPER_LABEL_KIND, HYPER_PARENT_KIND};
use crate::locality::Locality;
use crate::metadata::{Node, NodeShape};
use crate::validation::{
    CheckClass, CheckCoverage, CheckInstance, CheckKey, CheckKind, CheckSpec, Diagnostic,
    ExternalVerdicts, IncompleteCheck, IncompleteReason, ReportScope, SchemaRef, SubsetBudget,
    ValidationBudget, ValidationPlan, ValidationReport, Violation,
};
use crate::view_ref::{RowId, ViewRef};
use crate::wtype::WInstance;
use panproto_gat::Name;
use panproto_schema::Schema;

/// Validate one segment against `plan`.
///
/// The segment contract is checked first. Only check instances local to one
/// segment are evaluated; each other instance is reported as
/// [`IncompleteReason::OutsideSegment`]. Remote endpoints are materialized
/// only as anchor-bearing context, and never become subjects in the report.
/// Application and external-constraint checks read `external`, the same
/// verdict projection used by whole-instance validation.
///
/// A finite `budget` evaluates exactly the ordered prefix of check instances
/// whose complete footprints fit. It never performs an unlimited validation
/// and filters the report afterward. The segment contract, local indices and
/// candidate subjects are prepared before that semantic-evaluation budget;
/// the budget meters each admitted check instance's complete footprint, so no
/// unmetered semantic evaluation can escape the deterministic prefix. This
/// explicit prefix discipline avoids the resource-accounting failures
/// illustrated by Z3 issues #56 and #10939; it is a bound on this
/// implementation, not a novelty claim about budgeted validation.
///
/// # Errors
///
/// As [`super::check_segment`]. In particular, violations of the segment's
/// own contract are returned together, in deterministic element order, as
/// [`SegmentError::Invalid`]. A graph or tree segment whose owned view edges
/// and incoming stubs exceed the temporary [`EdgeId`] carrier returns
/// [`SegmentError::ValidationViewUnrepresentable`] before view construction.
pub fn validate_segment(
    segment: &AnySegment,
    partition: &Partition,
    plan: &ValidationPlan,
    budget: &ValidationBudget,
    external: &ExternalVerdicts,
) -> Result<ValidationReport, SegmentError> {
    validate_segment_with_edge_capacity(
        segment,
        partition,
        plan,
        budget,
        external,
        VALIDATION_VIEW_EDGE_CAPACITY,
    )
}

/// The implementation entry point with an injectable edge-carrier capacity.
///
/// The injected bound makes the `C`/`C + 1` preflight observable with a
/// finite carrier while the public entry point remains pinned to [`EdgeId`].
pub(super) fn validate_segment_with_edge_capacity(
    segment: &AnySegment,
    partition: &Partition,
    plan: &ValidationPlan,
    budget: &ValidationBudget,
    external: &ExternalVerdicts,
    edge_capacity: u64,
) -> Result<ValidationReport, SegmentError> {
    check_segment(segment, partition)?;
    preflight_validation_view_capacity(segment, edge_capacity)?;

    let shape = segment.shape();
    let index = LocalIndex::new(segment);
    let mut selection = LocalSelection::new(plan);
    for (key, spec) in plan.checks() {
        if selection.statically_incomplete(key) {
            continue;
        }
        let Some(locality) = spec.locality.get(&shape) else {
            continue;
        };
        let subjects = check_subjects(&index, plan, key, spec);
        if locality != &Locality::Local {
            selection.outside_segment(key, spec, locality, subjects);
            continue;
        }
        selection.local(key, spec, subjects);
    }

    let (mut incomplete, admitted) = selection.finish();
    let mut diagnostics = Vec::new();
    let (local_diagnostics, local_incomplete) = match segment {
        AnySegment::Graph(_) => run_graph_candidates(segment, plan, budget, external, &admitted),
        AnySegment::Tree(_) => run_tree_candidates(segment, plan, budget, external, &admitted),
        AnySegment::Table(_) => {
            run_table_candidates(segment, &index, plan, budget, external, &admitted)
        }
    };
    diagnostics.extend(local_diagnostics);
    incomplete.extend(local_incomplete);
    let coverage = coverage(&index, plan, &diagnostics, &incomplete);
    Ok(ValidationReport::new(
        shape,
        ReportScope::Segment {
            segment: segment.segment(),
        },
        *plan.digest(),
        diagnostics,
        incomplete,
        coverage,
    ))
}

/// The cardinality of the temporary [`EdgeId`] carrier used by a local
/// graph view.
const VALIDATION_VIEW_EDGE_CAPACITY: u64 = 1_u64 << u32::BITS;

/// Check an already counted local validation-view carrier.
///
/// Taking the two counts directly keeps the exact `C`/`C + 1` boundary
/// test independent of an allocation of either carrier.
///
/// # Errors
///
/// [`SegmentError::ValidationViewUnrepresentable`] when the combined count
/// is greater than `capacity`. The public validator supplies
/// [`VALIDATION_VIEW_EDGE_CAPACITY`].
pub(super) const fn check_validation_view_capacity(
    segment: SegmentId,
    owned_edges: u64,
    incoming_stubs: u64,
    capacity: u64,
) -> Result<(), SegmentError> {
    let Some(edges) = owned_edges.checked_add(incoming_stubs) else {
        panic!("two simultaneously resident collections cannot exceed u64::MAX elements");
    };
    if edges > capacity {
        return Err(SegmentError::ValidationViewUnrepresentable { segment, edges });
    }
    Ok(())
}

/// Count graph/tree owned view edges and incoming stubs before constructing
/// the local graph view. Tables are validated without that view.
fn preflight_validation_view_capacity(
    segment: &AnySegment,
    capacity: u64,
) -> Result<(), SegmentError> {
    let owned_edges = match segment {
        AnySegment::Graph(graph) => graph.edges.len(),
        AnySegment::Tree(tree) => tree.arcs.len(),
        AnySegment::Table(_) => return Ok(()),
    };
    let Ok(owned_edges) = u64::try_from(owned_edges) else {
        unreachable!("an in-memory collection length is representable as u64");
    };
    let Ok(incoming_stubs) = u64::try_from(segment.boundary().in_stubs.len()) else {
        unreachable!("an in-memory collection length is representable as u64");
    };
    check_validation_view_capacity(segment.segment(), owned_edges, incoming_stubs, capacity)
}

#[derive(Clone)]
struct GraphEdgeRef {
    at: ViewRef,
    original: Option<EdgeId>,
    owned: bool,
}

type GraphEdgeRefs = BTreeMap<EdgeId, GraphEdgeRef>;

/// One near-linear pass over a segment's logical elements and adjacency.
/// Subject enumeration and direct table footprints share these indexes
/// instead of rescanning every edge for every node and compiled check.
struct LocalIndex<'a> {
    segment: &'a AnySegment,
    logical_nodes: Vec<(ViewRef, u32, &'a Name, bool)>,
    ordinary_edges: Vec<(ViewRef, u32, &'a panproto_schema::Edge)>,
    ordinary_labels: BTreeMap<u32, Vec<&'a panproto_schema::Edge>>,
    endpoints: BTreeMap<ViewRef, (u32, u32)>,
    outgoing: BTreeMap<u32, BTreeSet<ViewRef>>,
    incoming: BTreeMap<u32, BTreeSet<ViewRef>>,
    boundary_incoming: BTreeMap<u32, BTreeSet<ViewRef>>,
}

impl<'a> LocalIndex<'a> {
    fn new(segment: &'a AnySegment) -> Self {
        let logical_nodes = collect_logical_nodes(segment);
        let ordinary_edges = collect_ordinary_edges(segment);
        let context_edges = collect_context_edges(segment);
        let visible_edges: BTreeSet<ViewRef> = context_edges
            .iter()
            .filter(|(at, src, tgt)| edge_visible_in_validation_view(segment, at, *src, *tgt))
            .map(|(at, _, _)| at.clone())
            .collect();
        let mut ordinary_labels: BTreeMap<u32, Vec<&panproto_schema::Edge>> = BTreeMap::new();
        for (at, src, label) in &ordinary_edges {
            if visible_edges.contains(at) {
                ordinary_labels.entry(*src).or_default().push(*label);
            }
        }

        let mut endpoints = BTreeMap::new();
        let mut outgoing: BTreeMap<u32, BTreeSet<ViewRef>> = BTreeMap::new();
        let mut incoming: BTreeMap<u32, BTreeSet<ViewRef>> = BTreeMap::new();
        for (at, src, tgt) in context_edges {
            endpoints.insert(at.clone(), (src, tgt));
            // Raw adjacency is also the budget carrier for direct table
            // checks, so it retains malformed pairs omitted from FInstance.
            outgoing.entry(src).or_default().insert(at.clone());
            incoming.entry(tgt).or_default().insert(at);
        }
        let mut boundary_incoming: BTreeMap<u32, BTreeSet<ViewRef>> = BTreeMap::new();
        for (at, stub) in &segment.boundary().in_stubs {
            boundary_incoming
                .entry(stub.tgt)
                .or_default()
                .insert(at.clone());
        }

        Self {
            segment,
            logical_nodes,
            ordinary_edges,
            ordinary_labels,
            endpoints,
            outgoing,
            incoming,
            boundary_incoming,
        }
    }
}

struct LocalSelection<'a> {
    plan: &'a ValidationPlan,
    incomplete: Vec<IncompleteCheck>,
    admitted: BTreeSet<CheckInstance>,
}

impl<'a> LocalSelection<'a> {
    const fn new(plan: &'a ValidationPlan) -> Self {
        Self {
            plan,
            incomplete: Vec::new(),
            admitted: BTreeSet::new(),
        }
    }

    fn outside_segment(
        &mut self,
        key: &CheckKey,
        spec: &CheckSpec,
        locality: &Locality,
        subjects: Vec<Option<ViewRef>>,
    ) {
        self.incomplete
            .extend(subjects.into_iter().map(|subject| IncompleteCheck {
                check: key.clone(),
                class: spec.class,
                requirement: spec.requirement,
                subject,
                reason: IncompleteReason::OutsideSegment {
                    locality: locality.clone(),
                },
            }));
    }

    /// Retain the plan's static explanation before locality can replace it
    /// with a weaker segment-boundary unknown.
    fn statically_incomplete(&mut self, key: &CheckKey) -> bool {
        let incomplete = self.plan.is_statically_incomplete(key);
        if incomplete {
            self.incomplete.extend(
                self.plan
                    .static_findings()
                    .iter()
                    .filter(|entry| entry.check == *key)
                    .cloned(),
            );
        }
        incomplete
    }

    fn local(&mut self, key: &CheckKey, spec: &CheckSpec, subjects: Vec<Option<ViewRef>>) {
        // Static checks were settled while compiling the plan. A successful
        // static check therefore needs no carrier view and consumes no read.
        if spec.class == CheckClass::Static {
            return;
        }
        self.admitted
            .extend(subjects.into_iter().map(|subject| CheckInstance {
                check: key.clone(),
                subject,
            }));
    }

    fn finish(self) -> (Vec<IncompleteCheck>, BTreeSet<CheckInstance>) {
        (self.incomplete, self.admitted)
    }
}

const fn segment_scope(segment: &AnySegment) -> ReportScope {
    ReportScope::Segment {
        segment: segment.segment(),
    }
}

/// Retain only findings produced by the selected carrier instances. Every
/// subset report repeats the plan's static findings; the segment selector
/// has already inserted the applicable ones exactly once.
fn append_subset_report(
    report: ValidationReport,
    plan: &ValidationPlan,
    diagnostics: &mut Vec<Diagnostic>,
    incomplete: &mut Vec<IncompleteCheck>,
) {
    diagnostics.extend(report.diagnostics);
    incomplete.extend(
        report
            .incomplete
            .into_iter()
            .filter(|entry| !plan.is_statically_incomplete(&entry.check)),
    );
}

/// Record the selected suffix after an earlier slice exhausted the shared
/// cursor, without constructing another carrier context or discovering any
/// skipped footprint.
fn record_exhausted_suffix(
    cursor: &mut SubsetBudget,
    instances: impl IntoIterator<Item = CheckInstance>,
) {
    debug_assert!(cursor.is_exhausted());
    for instance in instances {
        let charged = cursor.charge(&instance, 0);
        debug_assert!(!charged);
    }
}

fn run_graph_candidates(
    segment: &AnySegment,
    plan: &ValidationPlan,
    budget: &ValidationBudget,
    external: &ExternalVerdicts,
    admitted: &BTreeSet<CheckInstance>,
) -> (Vec<Diagnostic>, Vec<IncompleteCheck>) {
    if admitted.is_empty() {
        return (Vec::new(), Vec::new());
    }
    let (graph, edge_refs) = graph_view(segment);
    let admitted = graph_instances(admitted, &edge_refs);
    let mut cursor = SubsetBudget::new(budget);
    let report = plan.validate_graph_subset_cursor(
        &graph,
        &admitted,
        &mut cursor,
        external,
        segment_scope(segment),
    );
    let report = remap_graph_report(report, &edge_refs);
    let mut diagnostics = Vec::new();
    let mut incomplete = Vec::new();
    append_subset_report(report, plan, &mut diagnostics, &mut incomplete);
    let mut cuts = cursor.incomplete(plan);
    remap_incomplete(&mut cuts, &edge_refs);
    incomplete.extend(cuts);
    (diagnostics, incomplete)
}

fn run_tree_candidates(
    segment: &AnySegment,
    plan: &ValidationPlan,
    budget: &ValidationBudget,
    external: &ExternalVerdicts,
    admitted: &BTreeSet<CheckInstance>,
) -> (Vec<Diagnostic>, Vec<IncompleteCheck>) {
    if admitted.is_empty() {
        return (Vec::new(), Vec::new());
    }
    debug_assert!(
        admitted
            .iter()
            .all(|instance| instance.check.kind != CheckKind::FanWellFormed),
        "tree fan checks are sorted-merge summaries, not local candidates"
    );

    let tree = tree_view(segment);
    let mut cursor = SubsetBudget::new(budget);
    let mut diagnostics = Vec::new();
    let mut incomplete = Vec::new();
    let report = plan.validate_tree_subset_cursor(
        &tree,
        admitted,
        &mut cursor,
        external,
        segment_scope(segment),
    );
    append_subset_report(report, plan, &mut diagnostics, &mut incomplete);
    incomplete.extend(cursor.incomplete(plan));
    (diagnostics, incomplete)
}

struct TableCandidateSlices {
    prefix: BTreeSet<CheckInstance>,
    structural: Vec<CheckInstance>,
    middle: BTreeSet<CheckInstance>,
    external: Vec<CheckInstance>,
}

impl TableCandidateSlices {
    fn new(admitted: &BTreeSet<CheckInstance>) -> Self {
        Self {
            prefix: admitted
                .iter()
                .filter(|instance| instance.check.kind < CheckKind::EdgeDeclared)
                .cloned()
                .collect(),
            structural: admitted
                .iter()
                .filter(|instance| {
                    matches!(
                        instance.check.kind,
                        CheckKind::EdgeDeclared | CheckKind::EndpointCoherence
                    )
                })
                .cloned()
                .collect(),
            middle: admitted
                .iter()
                .filter(|instance| {
                    instance.check.kind > CheckKind::EndpointCoherence
                        && instance.check.kind < CheckKind::External
                })
                .cloned()
                .collect(),
            external: admitted
                .iter()
                .filter(|instance| instance.check.kind == CheckKind::External)
                .cloned()
                .collect(),
        }
    }
}

fn run_table_candidates(
    segment: &AnySegment,
    index: &LocalIndex<'_>,
    plan: &ValidationPlan,
    budget: &ValidationBudget,
    external: &ExternalVerdicts,
    admitted: &BTreeSet<CheckInstance>,
) -> (Vec<Diagnostic>, Vec<IncompleteCheck>) {
    if admitted.is_empty() {
        return (Vec::new(), Vec::new());
    }
    let slices = TableCandidateSlices::new(admitted);

    let table = table_view(segment);
    let mut cursor = SubsetBudget::new(budget);
    let mut diagnostics = Vec::new();
    let mut incomplete = Vec::new();
    if !slices.prefix.is_empty() {
        let report = plan.validate_table_subset_cursor(
            &table,
            &slices.prefix,
            &mut cursor,
            external,
            segment_scope(segment),
        );
        append_subset_report(report, plan, &mut diagnostics, &mut incomplete);
    }

    // A malformed pair cannot inhabit `FInstance`, so its structural checks
    // must be charged directly. Charging every structural pair here keeps
    // those synthetic instances interleaved with representable pairs in the
    // canonical `CheckInstance` order.
    let mut manual = BTreeSet::new();
    for instance in slices.structural {
        let reads = if cursor.is_exhausted() {
            0
        } else {
            direct_reads(
                index,
                &instance,
                cursor.limit.map(|limit| limit.saturating_sub(cursor.spent)),
            )
        };
        if cursor.charge(&instance, reads) {
            manual.insert(instance);
        }
    }
    if !slices.middle.is_empty() {
        if cursor.is_exhausted() {
            record_exhausted_suffix(&mut cursor, slices.middle);
        } else {
            let report = plan.validate_table_subset_cursor(
                &table,
                &slices.middle,
                &mut cursor,
                external,
                segment_scope(segment),
            );
            append_subset_report(report, plan, &mut diagnostics, &mut incomplete);
        }
    }
    // Table in-stubs are absent from the owned `FInstance` view, but they
    // remain incident context for External. Charge the exact union of owned
    // pairs and boundary stubs before evaluating the admitted verdicts.
    for instance in slices.external {
        let reads = if cursor.is_exhausted() {
            0
        } else {
            direct_reads(
                index,
                &instance,
                cursor.limit.map(|limit| limit.saturating_sub(cursor.spent)),
            )
        };
        if cursor.charge(&instance, reads) {
            manual.insert(instance);
        }
    }

    if !manual.is_empty() {
        // Structural pair checks use the complete direct oracle below.
        // Only External row subjects need the ordinary table evaluator;
        // submitting pair subjects absent from `table_view` would be an
        // observationally redundant no-op.
        let concrete: BTreeSet<CheckInstance> = manual
            .iter()
            .filter(|instance| {
                matches!(
                    (instance.check.kind, instance.subject.as_ref()),
                    (CheckKind::External, Some(ViewRef::Row(_)))
                )
            })
            .cloned()
            .collect();
        if !concrete.is_empty() {
            let mut evaluation_cursor = SubsetBudget::new(&ValidationBudget::default());
            let report = plan.validate_table_subset_cursor(
                &table,
                &concrete,
                &mut evaluation_cursor,
                external,
                segment_scope(segment),
            );
            append_subset_report(report, plan, &mut diagnostics, &mut incomplete);
        }
        if manual.iter().any(|instance| {
            matches!(
                instance.check.kind,
                CheckKind::EdgeDeclared | CheckKind::EndpointCoherence
            )
        }) {
            diagnostics.extend(table_direct_diagnostics(segment, plan, &manual));
        }
    }
    incomplete.extend(cursor.incomplete(plan));
    (diagnostics, incomplete)
}

fn graph_instances(
    admitted: &BTreeSet<CheckInstance>,
    edge_refs: &GraphEdgeRefs,
) -> BTreeSet<CheckInstance> {
    let owned_edges: BTreeMap<ViewRef, ViewRef> = edge_refs
        .iter()
        .filter(|(_, edge)| edge.owned)
        .map(|(id, edge)| (edge.at.clone(), ViewRef::Edge(*id)))
        .collect();
    admitted
        .iter()
        .map(|instance| CheckInstance {
            check: instance.check.clone(),
            subject: instance.subject.as_ref().map(|subject| {
                owned_edges
                    .get(subject)
                    .cloned()
                    .unwrap_or_else(|| subject.clone())
            }),
        })
        .collect()
}

/// A graph view of a graph segment or of a tree segment's stored reified
/// graph. Every in-stub is context for incident-edge footprints, but never a
/// segment-owned check subject. Edge ids are made dense so a malformed
/// cross-segment allocation cannot collide with an in-stub; `edge_refs`
/// restores global references in the report.
fn graph_view(segment: &AnySegment) -> (GInstance, GraphEdgeRefs) {
    let mut nodes: BTreeMap<u32, Node> = match segment {
        AnySegment::Graph(graph) => graph.nodes.clone(),
        AnySegment::Tree(tree) => tree.nodes.clone(),
        AnySegment::Table(_) => BTreeMap::new(),
    };
    for ghost in segment.boundary().ghosts.values() {
        nodes
            .entry(ghost.node)
            .or_insert_with(|| Node::new(ghost.node, ghost.anchor.clone()));
    }

    let mut edges: Vec<(ViewRef, Option<EdgeId>, bool, GEdge)> = match segment {
        AnySegment::Graph(graph) => graph
            .edges
            .iter()
            .map(|(id, edge)| (ViewRef::Edge(*id), Some(*id), true, edge.clone()))
            .collect(),
        AnySegment::Tree(tree) => {
            let fan_edges = tree.fan_edge_ids();
            tree.arcs
                .iter()
                .map(|(id, (src, tgt, edge))| {
                    let at = if fan_edges.contains(id) {
                        ViewRef::Edge(*id)
                    } else {
                        ViewRef::Arc {
                            parent: *src,
                            child: *tgt,
                        }
                    };
                    (at, Some(*id), true, GEdge::new(*src, *tgt, edge.clone()))
                })
                .collect()
        }
        AnySegment::Table(_) => Vec::new(),
    };
    for (at, stub) in &segment.boundary().in_stubs {
        nodes
            .entry(stub.src)
            .or_insert_with(|| Node::new(stub.src, stub.src_anchor.clone()));
        edges.push((
            at.clone(),
            match at {
                ViewRef::Edge(id) => Some(*id),
                _ => None,
            },
            false,
            GEdge::new(stub.src, stub.tgt, stub.label.clone()),
        ));
    }
    edges.sort_by(|(at_a, id_a, owned_a, a), (at_b, id_b, owned_b, b)| {
        (at_a, id_a, owned_a, a.src, a.tgt, &a.edge, a.position)
            .cmp(&(at_b, id_b, owned_b, b.src, b.tgt, &b.edge, b.position))
    });

    let mut graph = GInstance::new();
    for node in nodes.into_values() {
        graph = graph.with_node(node);
    }
    let mut edge_refs = BTreeMap::new();
    for (index, (at, original, owned, edge)) in edges.into_iter().enumerate() {
        let Ok(index) = u32::try_from(index) else {
            unreachable!("the validation-view edge carrier was preflighted");
        };
        let id = EdgeId(index);
        if graph.insert_edge(id, edge).is_err() {
            unreachable!("a checked segment makes a well-formed local graph view");
        }
        edge_refs.insert(
            id,
            GraphEdgeRef {
                at,
                original,
                owned,
            },
        );
    }
    (graph, edge_refs)
}

/// A native forest view of a tree segment. Reified fan nodes and edges are
/// absent; ordinary in-stubs are included so `TreeParentMap` counts parents
/// across the boundary.
fn tree_view(segment: &AnySegment) -> WInstance {
    let AnySegment::Tree(tree) = segment else {
        unreachable!("tree_view is called only for a tree segment");
    };
    let fan_nodes = tree.fan_node_ids();
    let fan_edges = tree.fan_edge_ids();
    let mut nodes: HashMap<u32, Node> = tree
        .nodes
        .iter()
        .filter(|(id, _)| !fan_nodes.contains(id))
        .map(|(id, node)| (*id, node.clone()))
        .collect();
    let mut arcs: Vec<(u32, u32, panproto_schema::Edge)> = tree
        .arcs
        .iter()
        .filter(|(id, _)| !fan_edges.contains(id))
        .map(|(_, arc)| arc.clone())
        .collect();
    for ghost in tree.boundary.ghosts.values() {
        nodes
            .entry(ghost.node)
            .or_insert_with(|| Node::new(ghost.node, ghost.anchor.clone()));
    }
    for (at, stub) in &tree.boundary.in_stubs {
        if !matches!(at, ViewRef::Arc { .. }) || fan_nodes.contains(&stub.tgt) {
            continue;
        }
        nodes
            .entry(stub.src)
            .or_insert_with(|| Node::new(stub.src, stub.src_anchor.clone()));
        arcs.push((stub.src, stub.tgt, stub.label.clone()));
    }
    let root = tree
        .root
        .or_else(|| nodes.keys().copied().min())
        .unwrap_or_default();
    let schema_root = nodes
        .get(&root)
        .map_or_else(|| "".into(), |node| node.anchor.clone());
    WInstance::new(nodes, arcs, Vec::new(), root, schema_root)
}

/// A table view with one empty row for each remote target, under the ghost's
/// table anchor.
fn table_view(segment: &AnySegment) -> FInstance {
    let AnySegment::Table(table) = segment else {
        unreachable!("table_view is called only for a table segment");
    };
    let mut tables: BTreeMap<_, Vec<_>> = BTreeMap::new();
    for (row, (anchor, cells)) in &table.rows {
        tables
            .entry(anchor.clone())
            .or_default()
            .push((*row, cells.clone()));
    }
    for ghost in table.boundary.ghosts.values() {
        tables
            .entry(ghost.anchor.clone())
            .or_default()
            .push((RowId(ghost.node), HashMap::new()));
    }
    let pairs = table
        .pairs
        .iter()
        .filter(|(edge, src, tgt)| table_pair_representable(table, edge, *src, *tgt))
        .map(|(edge, src, tgt)| (edge.clone(), vec![(*src, *tgt)]));
    FInstance::from_parts(tables, pairs)
        .unwrap_or_else(|_| unreachable!("a checked segment makes a well-formed local table view"))
}

/// The table anchor of an owned row or remote target ghost.
fn table_anchor(table: &super::body::TableSegment, row: RowId) -> Option<&Name> {
    table
        .rows
        .get(&row)
        .map(|(anchor, _)| anchor)
        .or_else(|| table.boundary.ghosts.get(&row.0).map(|ghost| &ghost.anchor))
}

fn table_pair_representable(
    table: &super::body::TableSegment,
    edge: &panproto_schema::Edge,
    src: RowId,
    tgt: RowId,
) -> bool {
    table_anchor(table, src).is_some_and(|anchor| anchor == &edge.src)
        && table_anchor(table, tgt).is_some_and(|anchor| anchor == &edge.tgt)
}

/// Structural diagnostics for directly charged table pairs.
///
/// The direct oracle handles both representable and malformed pairs. The
/// latter cannot inhabit [`FInstance`], but validation must retain them long
/// enough to diagnose the declaration and each endpoint.
fn table_direct_diagnostics(
    segment: &AnySegment,
    plan: &ValidationPlan,
    admitted: &BTreeSet<CheckInstance>,
) -> Vec<Diagnostic> {
    let AnySegment::Table(table) = segment else {
        return Vec::new();
    };
    let checks = DirectTableChecks::new(table, plan, admitted);
    let pairs: BTreeSet<_> = admitted
        .iter()
        .filter_map(
            |instance| match (instance.check.kind, instance.subject.as_ref()) {
                (
                    CheckKind::EdgeDeclared | CheckKind::EndpointCoherence,
                    Some(ViewRef::Pair { edge, src, tgt }),
                ) => Some((edge.clone(), *src, *tgt)),
                _ => None,
            },
        )
        .collect();
    pairs
        .iter()
        .flat_map(|(edge, src, tgt)| checks.diagnostics(edge, *src, *tgt))
        .collect()
}

struct DirectTableChecks<'a> {
    table: &'a super::body::TableSegment,
    plan: &'a ValidationPlan,
    admitted: &'a BTreeSet<CheckInstance>,
    endpoint: Option<(&'a CheckKey, &'a CheckSpec)>,
    declared: Option<(&'a CheckKey, &'a CheckSpec)>,
    families: AnchorFamilies<'a>,
}

impl<'a> DirectTableChecks<'a> {
    fn new(
        table: &'a super::body::TableSegment,
        plan: &'a ValidationPlan,
        admitted: &'a BTreeSet<CheckInstance>,
    ) -> Self {
        Self {
            table,
            plan,
            admitted,
            endpoint: plan
                .checks()
                .iter()
                .find(|(key, _)| key.kind == CheckKind::EndpointCoherence),
            declared: plan
                .checks()
                .iter()
                .find(|(key, _)| key.kind == CheckKind::EdgeDeclared),
            families: AnchorFamilies::new(plan.schema()),
        }
    }

    fn diagnostics(&self, edge: &panproto_schema::Edge, src: RowId, tgt: RowId) -> Vec<Diagnostic> {
        let at = ViewRef::Pair {
            edge: edge.clone(),
            src,
            tgt,
        };
        let declared = self.declared.filter(|(key, _)| {
            self.admitted.contains(&CheckInstance {
                check: (*key).clone(),
                subject: Some(at.clone()),
            })
        });
        let endpoint = self.endpoint.filter(|(key, _)| {
            self.admitted.contains(&CheckInstance {
                check: (*key).clone(),
                subject: Some(at.clone()),
            })
        });
        let mut diagnostics = Vec::new();
        if !self.plan.schema().edges.contains_key(edge)
            && let Some((key, spec)) = declared
        {
            diagnostics.push(Diagnostic {
                check: key.clone(),
                class: spec.class,
                requirement: spec.requirement,
                subject: Some(at.clone()),
                elements: vec![at.clone()],
                violation: Violation::UndeclaredEdge {
                    label: edge.clone(),
                },
                source: self.plan.source(&key.decl),
            });
        }
        let Some((key, spec)) = endpoint else {
            return diagnostics;
        };
        for (side, row, expected) in [
            (crate::validation::EndpointSide::Src, src, &edge.src),
            (crate::validation::EndpointSide::Tgt, tgt, &edge.tgt),
        ] {
            let Some(found) = table_anchor(self.table, row) else {
                continue;
            };
            if self.families.accepts(expected, found) {
                continue;
            }
            let mut elements = vec![at.clone(), ViewRef::Row(row)];
            elements.sort();
            elements.dedup();
            diagnostics.push(Diagnostic {
                check: key.clone(),
                class: spec.class,
                requirement: spec.requirement,
                subject: Some(at.clone()),
                elements,
                violation: Violation::IncoherentEndpoint {
                    side,
                    expected: expected.clone(),
                    found: found.clone(),
                },
                source: self.plan.source(&key.decl),
            });
        }
        diagnostics
    }
}

/// The endpoint-family graph shared by direct table endpoint checks.
struct AnchorFamilies<'a> {
    stands_for: HashMap<&'a Name, Vec<&'a Name>>,
}

impl<'a> AnchorFamilies<'a> {
    fn new(schema: &'a Schema) -> Self {
        let mut stands_for: HashMap<&Name, Vec<&Name>> = HashMap::new();
        let mut link = |placeholder: &'a Name, member: &'a Name| {
            stands_for.entry(placeholder).or_default().push(member);
        };
        for (union_vertex, variants) in &schema.variants {
            for variant in variants {
                link(union_vertex, &variant.id);
                if &variant.parent_vertex != union_vertex {
                    link(&variant.parent_vertex, &variant.id);
                }
            }
        }
        for (point, unfolding) in &schema.recursion_points {
            link(point, &unfolding.target_vertex);
        }
        for (id, vertex) in &schema.vertices {
            if vertex.kind == "ref"
                && let Some(edges) = schema.outgoing.get(id)
            {
                for edge in edges.iter().filter(|edge| edge.kind == "ref") {
                    link(id, &edge.tgt);
                }
            }
        }
        Self { stands_for }
    }

    fn accepts(&self, expected: &Name, anchor: &Name) -> bool {
        expected == anchor || self.reaches(expected, anchor) || self.reaches(anchor, expected)
    }

    fn reaches(&self, from: &Name, to: &Name) -> bool {
        let mut seen: HashSet<&Name> = HashSet::from([from]);
        let mut queue: VecDeque<&Name> = VecDeque::from([from]);
        while let Some(current) = queue.pop_front() {
            let Some(members) = self.stands_for.get(current) else {
                continue;
            };
            for member in members {
                if *member == to {
                    return true;
                }
                if seen.insert(member) {
                    queue.push_back(member);
                }
            }
        }
        false
    }
}

fn remap_graph_report(mut report: ValidationReport, edge_refs: &GraphEdgeRefs) -> ValidationReport {
    for diagnostic in &mut report.diagnostics {
        diagnostic.subject = diagnostic
            .subject
            .as_ref()
            .map(|at| remap_ref(at, edge_refs));
        diagnostic.elements = diagnostic
            .elements
            .iter()
            .map(|at| remap_ref(at, edge_refs))
            .collect();
        diagnostic.elements.sort();
        diagnostic.elements.dedup();
        remap_violation(&mut diagnostic.violation, edge_refs);
    }
    remap_incomplete(&mut report.incomplete, edge_refs);
    report
}

fn remap_incomplete(incomplete: &mut [IncompleteCheck], edge_refs: &GraphEdgeRefs) {
    for entry in incomplete {
        entry.subject = entry.subject.as_ref().map(|at| remap_ref(at, edge_refs));
        if let IncompleteReason::Budget { from, .. } = &mut entry.reason {
            *from = from.as_ref().map(|at| remap_ref(at, edge_refs));
        }
    }
}

fn remap_ref(at: &ViewRef, edge_refs: &GraphEdgeRefs) -> ViewRef {
    match at {
        ViewRef::Edge(id) => edge_refs
            .get(id)
            .map_or_else(|| at.clone(), |edge| edge.at.clone()),
        _ => at.clone(),
    }
}

fn remap_edge(id: &mut EdgeId, edge_refs: &GraphEdgeRefs) {
    if let Some(original) = edge_refs.get(id).and_then(|edge| edge.original) {
        *id = original;
    }
}

fn remap_fan_violation(reason: &mut FanViolation, edge_refs: &GraphEdgeRefs) {
    match reason {
        FanViolation::MultipleParents { edges } | FanViolation::DuplicateLabel { edges, .. } => {
            for edge in &mut *edges {
                remap_edge(edge, edge_refs);
            }
            edges.sort_unstable();
        }
        FanViolation::ReservedKindOutsideFan { edge }
        | FanViolation::ForeignIncidentEdge { edge } => remap_edge(edge, edge_refs),
        FanViolation::MissingParent
        | FanViolation::UnknownLabel { .. }
        | FanViolation::UnknownHyperEdge { .. }
        | FanViolation::MissingChild { .. } => {}
    }
}

fn remap_violation(violation: &mut Violation, edge_refs: &GraphEdgeRefs) {
    match violation {
        Violation::MalformedFan { reason, .. } => remap_fan_violation(reason, edge_refs),
        Violation::OutDegree { edges, .. } => {
            for edge in &mut *edges {
                *edge = remap_ref(edge, edge_refs);
            }
            edges.sort();
            edges.dedup();
        }
        Violation::DuplicatePosition { edges, .. } => {
            for edge in &mut *edges {
                remap_edge(edge, edge_refs);
            }
            edges.sort_unstable();
        }
        Violation::Cycle { witness, .. } => {
            for edge in witness {
                remap_edge(edge, edge_refs);
            }
        }
        Violation::DuplicateKey { elems, .. } => {
            for elem in &mut *elems {
                *elem = remap_ref(elem, edge_refs);
            }
            elems.sort();
            elems.dedup();
        }
        Violation::AmbiguousReference { candidates, .. } => {
            for candidate in &mut *candidates {
                *candidate = remap_ref(candidate, edge_refs);
            }
            candidates.sort();
            candidates.dedup();
        }
        Violation::UnknownAnchor { .. }
        | Violation::UndeclaredEdge { .. }
        | Violation::IncoherentEndpoint { .. }
        | Violation::ReservedVocabulary
        | Violation::MissingRequiredEdge { .. }
        | Violation::ValueConstraint { .. }
        | Violation::MissingValue { .. }
        | Violation::UndeclaredAttribute { .. }
        | Violation::MistypedAttribute { .. }
        | Violation::DanglingReference { .. }
        | Violation::MissingRoot
        | Violation::Unreachable
        | Violation::ParentMismatch { .. }
        | Violation::ExternalRejected { .. }
        | Violation::OutsideKeyScope { .. } => {}
    }
}

fn collect_logical_nodes(segment: &AnySegment) -> Vec<(ViewRef, u32, &panproto_gat::Name, bool)> {
    match segment {
        AnySegment::Graph(graph) => graph
            .nodes
            .iter()
            .map(|(id, node)| {
                (
                    ViewRef::Node(*id),
                    *id,
                    &node.anchor,
                    node.shape == NodeShape::HyperEdge,
                )
            })
            .collect(),
        AnySegment::Table(table) => table
            .rows
            .iter()
            .map(|(id, (anchor, _))| (ViewRef::Row(*id), id.0, anchor, false))
            .collect(),
        AnySegment::Tree(tree) => {
            let fan_nodes = tree.fan_node_ids();
            tree.nodes
                .iter()
                .filter(|(id, _)| !fan_nodes.contains(id))
                .map(|(id, node)| (ViewRef::Node(*id), *id, &node.anchor, false))
                .collect()
        }
    }
}

fn collect_ordinary_edges(segment: &AnySegment) -> Vec<(ViewRef, u32, &panproto_schema::Edge)> {
    match segment {
        AnySegment::Graph(graph) => graph
            .edges
            .iter()
            .filter(|(_, edge)| !reserved(&edge.edge))
            .map(|(id, edge)| (ViewRef::Edge(*id), edge.src, &edge.edge))
            .collect(),
        AnySegment::Table(table) => table
            .pairs
            .iter()
            .map(|(edge, src, tgt)| {
                (
                    ViewRef::Pair {
                        edge: edge.clone(),
                        src: *src,
                        tgt: *tgt,
                    },
                    src.0,
                    edge,
                )
            })
            .collect(),
        AnySegment::Tree(tree) => {
            let fan_edges = tree.fan_edge_ids();
            tree.arcs
                .iter()
                // `check_segment` rejects ordinary arcs incident to a fan node
                // before this index is built. Excluding the fan-edge carrier is
                // therefore the complete ordinary-edge test on admitted trees.
                .filter(|(id, _)| !fan_edges.contains(id))
                .map(|(_, (src, tgt, edge))| {
                    (
                        ViewRef::Arc {
                            parent: *src,
                            child: *tgt,
                        },
                        *src,
                        edge,
                    )
                })
                .collect()
        }
    }
}

fn reserved(edge: &panproto_schema::Edge) -> bool {
    edge.kind.as_str() == HYPER_PARENT_KIND || edge.kind.as_str() == HYPER_LABEL_KIND
}

fn anchored_subjects(
    index: &LocalIndex<'_>,
    plan: &ValidationPlan,
    spec: &CheckSpec,
) -> Vec<ViewRef> {
    index
        .logical_nodes
        .iter()
        .filter(|(_, _, anchor, fan)| {
            !*fan && plan.schema().has_vertex(anchor.as_ref()) && spec.anchors.contains(*anchor)
        })
        .map(|(at, _, _, _)| at.clone())
        .collect()
}

fn value_subjects(index: &LocalIndex<'_>, plan: &ValidationPlan, spec: &CheckSpec) -> Vec<ViewRef> {
    index
        .logical_nodes
        .iter()
        .filter(|(_, id, anchor, fan)| {
            !*fan
                && spec
                    .anchors
                    .iter()
                    .filter(|value_vertex| plan.schema().has_vertex(value_vertex.as_ref()))
                    .any(|value_vertex| {
                        *anchor == value_vertex
                            || plan
                                .schema()
                                .outgoing_edges(anchor.as_ref())
                                .iter()
                                .filter(|edge| {
                                    edge.kind == "prop"
                                        && edge.name.is_some()
                                        && &edge.tgt == value_vertex
                                })
                                .any(|field| {
                                    index.ordinary_labels.get(id).is_none_or(|labels| {
                                        labels.iter().all(|edge| *edge != field)
                                    })
                                })
                    })
        })
        .map(|(at, _, _, _)| at.clone())
        .collect()
}

fn check_subjects(
    index: &LocalIndex<'_>,
    plan: &ValidationPlan,
    key: &CheckKey,
    spec: &CheckSpec,
) -> Vec<Option<ViewRef>> {
    let some = |mut refs: Vec<ViewRef>| {
        refs.sort();
        refs.dedup();
        refs.into_iter().map(Some).collect()
    };
    match key.kind {
        CheckKind::SchemaWellFormed
        | CheckKind::DeclarationInterpretable
        | CheckKind::TreeRoot
        | CheckKind::KeyUnique
        | CheckKind::Acyclic => vec![None],
        CheckKind::NodeAnchor => some(
            index
                .logical_nodes
                .iter()
                .filter(|(_, _, _, fan)| !fan)
                .map(|(at, _, _, _)| at.clone())
                .collect(),
        ),
        CheckKind::ReservedVocabulary => {
            let mut refs: Vec<ViewRef> = index
                .logical_nodes
                .iter()
                .map(|(at, _, _, _)| at.clone())
                .collect();
            refs.extend(index.ordinary_edges.iter().map(|(at, _, _)| at.clone()));
            some(refs)
        }
        CheckKind::EdgeDeclared | CheckKind::EndpointCoherence => some(
            index
                .ordinary_edges
                .iter()
                .map(|(at, _, _)| at.clone())
                .collect(),
        ),
        CheckKind::FanWellFormed => match index.segment {
            AnySegment::Graph(graph) => {
                let mut refs: Vec<ViewRef> = graph
                    .nodes
                    .iter()
                    .filter(|(_, node)| is_fan_node(node))
                    .map(|(id, _)| ViewRef::Node(*id))
                    .collect();
                refs.extend(
                    graph
                        .edges
                        .values()
                        .filter(|edge| reserved(&edge.edge))
                        .map(|edge| ViewRef::Node(edge.src)),
                );
                some(refs)
            }
            AnySegment::Tree(tree) => {
                some(tree.fan_node_ids().into_iter().map(ViewRef::Node).collect())
            }
            AnySegment::Table(_) => Vec::new(),
        },
        CheckKind::TreeParentMap | CheckKind::TreeReachable => some(
            index
                .logical_nodes
                .iter()
                .cloned()
                .map(|(at, _, _, _)| at)
                .collect(),
        ),
        CheckKind::RequiredValue if matches!(key.decl, SchemaRef::Key { .. }) => {
            some(anchored_subjects(index, plan, spec))
        }
        CheckKind::ValueConstraint | CheckKind::RequiredValue => {
            some(value_subjects(index, plan, spec))
        }
        CheckKind::External if matches!(key.decl, SchemaRef::Constraint { .. }) => {
            some(value_subjects(index, plan, spec))
        }
        CheckKind::RequiredEdge
        | CheckKind::OutDegree
        | CheckKind::PositionDistinct
        | CheckKind::Attribute
        | CheckKind::ReferenceResolves
        | CheckKind::External => some(anchored_subjects(index, plan, spec)),
    }
}

/// Whether an ordinary edge inhabits the concrete validation view. Graph and
/// checked-tree ordinary edges do. A table view cannot carry a pair whose rows
/// are not in the endpoint tables named by its edge; that pair is retained only
/// for the direct structural checks.
fn edge_visible_in_validation_view(segment: &AnySegment, at: &ViewRef, src: u32, tgt: u32) -> bool {
    match (segment, at) {
        (AnySegment::Table(table), ViewRef::Pair { edge, .. }) => {
            table_pair_representable(table, edge, RowId(src), RowId(tgt))
        }
        _ => true,
    }
}

/// The ordinary/native edges visible to checks about logical elements. A
/// graph keeps its reified fan edges; a tree's native view does not.
/// Raw owned adjacency for the local index. For trees, fan filtering belongs
/// to [`collect_ordinary_edges`]; the other adjacency maps are consumed only by
/// direct table checks, so filtering the tree carrier here is observationally
/// redundant.
fn collect_context_edges(segment: &AnySegment) -> Vec<(ViewRef, u32, u32)> {
    match segment {
        AnySegment::Graph(graph) => graph
            .edges
            .iter()
            .map(|(id, edge)| (ViewRef::Edge(*id), edge.src, edge.tgt))
            .collect(),
        AnySegment::Table(table) => table
            .pairs
            .iter()
            .map(|(edge, src, tgt)| {
                (
                    ViewRef::Pair {
                        edge: edge.clone(),
                        src: *src,
                        tgt: *tgt,
                    },
                    src.0,
                    tgt.0,
                )
            })
            .collect(),
        AnySegment::Tree(tree) => tree
            .arcs
            .iter()
            .map(|(_, (src, tgt, _))| {
                (
                    ViewRef::Arc {
                        parent: *src,
                        child: *tgt,
                    },
                    *src,
                    *tgt,
                )
            })
            .collect(),
    }
}

/// Count at most the residual budget plus its first unpaid distinct read.
/// The local adjacency index was built in the segment's one carrier pass;
/// this bounded merge does not rescan an arbitrarily large incident set for
/// an instance that cannot be admitted.
fn direct_reads(index: &LocalIndex<'_>, instance: &CheckInstance, residual: Option<u64>) -> u64 {
    let mut reads = BTreeSet::new();
    let add = |reads: &mut BTreeSet<ViewRef>, candidate: ViewRef| {
        reads.insert(candidate);
        let count = u64::try_from(reads.len()).unwrap_or(u64::MAX);
        residual.is_some_and(|limit| count > limit)
    };
    match (instance.check.kind, instance.subject.as_ref()) {
        (CheckKind::EdgeDeclared, Some(at)) => {
            let _ = add(&mut reads, at.clone());
        }
        (CheckKind::EndpointCoherence, Some(at)) => {
            if add(&mut reads, at.clone()) {
                return u64::try_from(reads.len()).unwrap_or(u64::MAX);
            }
            if let Some((src, tgt)) = index.endpoints.get(at) {
                for endpoint in [*src, *tgt] {
                    if add(&mut reads, index.segment.node_ref(endpoint)) {
                        return u64::try_from(reads.len()).unwrap_or(u64::MAX);
                    }
                }
            }
        }
        (CheckKind::External, Some(at @ ViewRef::Row(row))) => {
            if add(&mut reads, at.clone()) {
                return u64::try_from(reads.len()).unwrap_or(u64::MAX);
            }
            for candidates in [
                index.outgoing.get(&row.0),
                index.incoming.get(&row.0),
                index.boundary_incoming.get(&row.0),
            ] {
                for candidate in candidates.into_iter().flatten() {
                    if add(&mut reads, candidate.clone()) {
                        return u64::try_from(reads.len()).unwrap_or(u64::MAX);
                    }
                }
            }
        }
        _ => unreachable!("only direct table check instances are charged here"),
    }
    u64::try_from(reads.len()).unwrap_or(u64::MAX)
}

fn coverage(
    index: &LocalIndex<'_>,
    plan: &ValidationPlan,
    diagnostics: &[Diagnostic],
    incomplete: &[IncompleteCheck],
) -> BTreeMap<CheckKey, CheckCoverage> {
    let shape = index.segment.shape();
    let mut failed_by_check: BTreeMap<CheckKey, BTreeSet<Option<ViewRef>>> = BTreeMap::new();
    for entry in diagnostics {
        failed_by_check
            .entry(entry.check.clone())
            .or_default()
            .insert(entry.subject.clone());
    }
    let mut incomplete_by_check: BTreeMap<CheckKey, Vec<&IncompleteCheck>> = BTreeMap::new();
    for entry in incomplete {
        incomplete_by_check
            .entry(entry.check.clone())
            .or_default()
            .push(entry);
    }
    plan.checks()
        .iter()
        .filter_map(|(key, spec)| {
            let locality = spec.locality.get(&shape)?;
            let subjects = check_subjects(index, plan, key, spec);
            let instances = subjects.len() as u64;
            if plan.is_statically_incomplete(key) {
                return Some((
                    key.clone(),
                    CheckCoverage {
                        class: spec.class,
                        requirement: spec.requirement,
                        instances,
                        passed: 0,
                        failed: 0,
                        incomplete: instances,
                    },
                ));
            }
            if locality != &Locality::Local {
                return Some((
                    key.clone(),
                    CheckCoverage {
                        class: spec.class,
                        requirement: spec.requirement,
                        instances,
                        passed: 0,
                        failed: 0,
                        incomplete: instances,
                    },
                ));
            }

            let failed = failed_by_check.get(key);
            let mut undecided: BTreeSet<Option<ViewRef>> = BTreeSet::new();
            for entry in incomplete_by_check.get(key).into_iter().flatten() {
                match &entry.reason {
                    IncompleteReason::Budget { from, .. } => {
                        undecided
                            .extend(subjects.iter().filter(|subject| *subject >= from).cloned());
                    }
                    _ => {
                        undecided.insert(entry.subject.clone());
                    }
                }
            }
            undecided.retain(|subject| failed.is_none_or(|subjects| !subjects.contains(subject)));
            let failed = failed.map_or(0, BTreeSet::len) as u64;
            let incomplete = undecided.len() as u64;
            Some((
                key.clone(),
                CheckCoverage {
                    class: spec.class,
                    requirement: spec.requirement,
                    instances,
                    passed: instances.saturating_sub(failed + incomplete),
                    failed,
                    incomplete,
                },
            ))
        })
        .collect()
}
