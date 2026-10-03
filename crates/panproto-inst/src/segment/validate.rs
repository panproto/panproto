//! Validation of one segment against a compiled validation plan.
//!
//! The ordinary checks of a tree run on its native local forest. Fan
//! well-formedness instead runs on the segment's stored reified graph: that
//! is the view in which a remote in-edge can be seen to enter a fan node.
//! In every shape, ghosts and in-stubs are context only. A report retains
//! check instances whose subjects are owned by the segment.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use super::body::{AnySegment, is_fan_node};
use super::check::check_segment;
use super::error::SegmentError;
use super::partition::Partition;
use crate::fan_violation::FanViolation;
use crate::functor::FInstance;
use crate::ginstance::{EdgeId, GEdge, GInstance, HYPER_LABEL_KIND, HYPER_PARENT_KIND};
use crate::locality::Locality;
use crate::metadata::{Node, NodeShape};
use crate::validation::{
    CheckCoverage, CheckKey, CheckKind, CheckSpec, Diagnostic, ExternalVerdicts, IncompleteCheck,
    IncompleteReason, ReportScope, SchemaRef, ValidationBudget, ValidationPlan, ValidationReport,
    Violation,
};
use crate::view_ref::{RowId, ViewRef};
use crate::wtype::WInstance;

/// Validate one segment against `plan`.
///
/// The segment contract is checked first. Only check instances local to one
/// segment are evaluated; each other instance is reported as
/// [`IncompleteReason::OutsideSegment`]. Remote endpoints are materialized
/// only as anchor-bearing context, and never become subjects in the report.
///
/// # Errors
///
/// As [`super::check_segment`]. In particular, violations of the segment's
/// own contract are returned together, in deterministic element order, as
/// [`SegmentError::Invalid`].
pub fn validate_segment(
    segment: &AnySegment,
    partition: &Partition,
    plan: &ValidationPlan,
    budget: &ValidationBudget,
) -> Result<ValidationReport, SegmentError> {
    check_segment(segment, partition)?;

    let shape = segment.shape();
    let reports = LocalReports::new(segment, plan);
    let mut findings = LocalFindings::new(segment, plan, budget);
    for (key, spec) in plan.checks() {
        let Some(locality) = spec.locality.get(&shape) else {
            continue;
        };
        let subjects = check_subjects(segment, plan, key, spec);
        if locality != &Locality::Local {
            findings.outside_segment(key, spec, locality, subjects);
            continue;
        }
        if let Some(report) = reports.for_check(segment, key.kind) {
            findings.local(key, spec, &subjects, report);
        }
    }

    let (diagnostics, incomplete) = findings.finish();
    let coverage = coverage(segment, plan, &diagnostics, &incomplete);
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

struct LocalReports {
    graph: Option<ValidationReport>,
    tree: Option<ValidationReport>,
    table: Option<ValidationReport>,
}

#[derive(Clone)]
struct GraphEdgeRef {
    at: ViewRef,
    original: Option<EdgeId>,
}

type GraphEdgeRefs = BTreeMap<EdgeId, GraphEdgeRef>;

impl LocalReports {
    fn new(segment: &AnySegment, plan: &ValidationPlan) -> Self {
        let external = ExternalVerdicts::default();
        let budget = ValidationBudget::default();
        let mut reports = Self {
            graph: None,
            tree: None,
            table: None,
        };
        match segment {
            AnySegment::Graph(_) => {
                let (graph, edge_refs) = graph_view(segment);
                reports.graph = Some(remap_graph_report(
                    plan.validate_graph(&graph, &budget, &external),
                    &edge_refs,
                ));
            }
            AnySegment::Table(_) => {
                let table = table_view(segment);
                reports.table = Some(plan.validate_table(&table, &budget, &external));
            }
            AnySegment::Tree(_) => {
                let tree = tree_view(segment);
                reports.tree = Some(plan.validate_tree(&tree, &budget, &external));
                let (graph, edge_refs) = graph_view(segment);
                reports.graph = Some(remap_graph_report(
                    plan.validate_graph(&graph, &budget, &external),
                    &edge_refs,
                ));
            }
        }
        reports
    }

    fn for_check(&self, segment: &AnySegment, kind: CheckKind) -> Option<&ValidationReport> {
        match segment {
            AnySegment::Graph(_) => self.graph.as_ref(),
            AnySegment::Table(_) => self.table.as_ref(),
            AnySegment::Tree(_) if kind == CheckKind::FanWellFormed => self.graph.as_ref(),
            AnySegment::Tree(_) => self.tree.as_ref(),
        }
    }
}

struct LocalFindings<'a> {
    segment: &'a AnySegment,
    plan: &'a ValidationPlan,
    bounded: bool,
    limit: u64,
    spent: u64,
    exhausted: bool,
    diagnostics: Vec<Diagnostic>,
    incomplete: Vec<IncompleteCheck>,
}

impl<'a> LocalFindings<'a> {
    fn new(segment: &'a AnySegment, plan: &'a ValidationPlan, budget: &ValidationBudget) -> Self {
        Self {
            segment,
            plan,
            bounded: budget.max_reads.is_some(),
            limit: budget.max_reads.unwrap_or(u64::MAX),
            spent: 0,
            exhausted: false,
            diagnostics: Vec::new(),
            incomplete: Vec::new(),
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

    fn local(
        &mut self,
        key: &CheckKey,
        spec: &CheckSpec,
        subjects: &[Option<ViewRef>],
        report: &ValidationReport,
    ) {
        let statically_incomplete = self
            .plan
            .static_findings()
            .iter()
            .any(|entry| entry.check == *key);
        let mut evaluated = BTreeSet::new();
        let mut cut = None;
        if !statically_incomplete {
            for subject in subjects {
                let reads = local_reads(self.segment, key.kind, subject.as_ref());
                if self.exhausted || (self.bounded && self.spent.saturating_add(reads) > self.limit)
                {
                    self.exhausted = true;
                    cut.get_or_insert_with(|| subject.clone());
                } else {
                    self.spent = self.spent.saturating_add(reads);
                    evaluated.insert(subject.clone());
                }
            }
        }
        self.diagnostics.extend(
            report
                .diagnostics
                .iter()
                .filter(|entry| entry.check == *key && evaluated.contains(&entry.subject))
                .cloned(),
        );
        self.incomplete
            .extend(report.incomplete.iter().filter_map(|entry| {
                if entry.check != *key {
                    return None;
                }
                if self.plan.static_findings().contains(entry) {
                    return Some(entry.clone());
                }
                evaluated.contains(&entry.subject).then(|| entry.clone())
            }));
        if let Some(from) = cut {
            self.incomplete.push(IncompleteCheck {
                check: key.clone(),
                class: spec.class,
                requirement: spec.requirement,
                subject: None,
                reason: IncompleteReason::Budget {
                    limit: self.limit,
                    spent: self.spent,
                    from,
                },
            });
        }
    }

    fn finish(self) -> (Vec<Diagnostic>, Vec<IncompleteCheck>) {
        (self.diagnostics, self.incomplete)
    }
}

/// A graph view of a graph segment or of a tree segment's stored reified
/// graph. Edge ids are made dense so a malformed cross-segment allocation
/// cannot collide with an in-stub; `edge_refs` restores global references in
/// the report.
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

    let fan_nodes: BTreeSet<u32> = nodes
        .iter()
        .filter(|(_, node)| is_fan_node(node))
        .map(|(id, _)| *id)
        .collect();
    let mut edges: Vec<(ViewRef, Option<EdgeId>, GEdge)> = match segment {
        AnySegment::Graph(graph) => graph
            .edges
            .iter()
            .map(|(id, edge)| (ViewRef::Edge(*id), Some(*id), edge.clone()))
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
                    (at, Some(*id), GEdge::new(*src, *tgt, edge.clone()))
                })
                .collect()
        }
        AnySegment::Table(_) => Vec::new(),
    };
    for (at, stub) in &segment.boundary().in_stubs {
        if !fan_nodes.contains(&stub.tgt) {
            continue;
        }
        nodes
            .entry(stub.src)
            .or_insert_with(|| Node::new(stub.src, stub.src_anchor.clone()));
        edges.push((
            at.clone(),
            match at {
                ViewRef::Edge(id) => Some(*id),
                _ => None,
            },
            GEdge::new(stub.src, stub.tgt, stub.label.clone()),
        ));
    }
    edges.sort_by(|(at_a, id_a, a), (at_b, id_b, b)| {
        (at_a, id_a, a.src, a.tgt, &a.edge, a.position)
            .cmp(&(at_b, id_b, b.src, b.tgt, &b.edge, b.position))
    });

    let mut graph = GInstance::new();
    for node in nodes.into_values() {
        graph = graph.with_node(node);
    }
    let mut edge_refs = BTreeMap::new();
    for (index, (at, original, edge)) in edges.into_iter().enumerate() {
        let id = EdgeId(u32::try_from(index).unwrap_or(u32::MAX));
        if graph.insert_edge(id, edge).is_err() {
            unreachable!("a checked segment makes a well-formed local graph view");
        }
        edge_refs.insert(id, GraphEdgeRef { at, original });
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
        .filter(|(id, (src, tgt, _))| {
            !fan_edges.contains(id) && !fan_nodes.contains(src) && !fan_nodes.contains(tgt)
        })
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
        .map(|(edge, src, tgt)| (edge.clone(), vec![(*src, *tgt)]));
    FInstance::from_parts(tables, pairs)
        .unwrap_or_else(|_| unreachable!("a checked segment makes a well-formed local table view"))
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
    for entry in &mut report.incomplete {
        entry.subject = entry.subject.as_ref().map(|at| remap_ref(at, edge_refs));
        if let IncompleteReason::Budget { from, .. } = &mut entry.reason {
            *from = from.as_ref().map(|at| remap_ref(at, edge_refs));
        }
    }
    report
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
        | Violation::ExternalRejected { .. } => {}
    }
}

fn logical_nodes(segment: &AnySegment) -> Vec<(ViewRef, u32, &panproto_gat::Name, bool)> {
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

fn ordinary_edges(segment: &AnySegment) -> Vec<(ViewRef, u32, &panproto_schema::Edge)> {
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
            let fan_nodes = tree.fan_node_ids();
            tree.arcs
                .iter()
                .filter(|(id, (src, tgt, _))| {
                    !fan_edges.contains(id) && !fan_nodes.contains(src) && !fan_nodes.contains(tgt)
                })
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
    segment: &AnySegment,
    plan: &ValidationPlan,
    spec: &CheckSpec,
) -> Vec<ViewRef> {
    logical_nodes(segment)
        .into_iter()
        .filter(|(_, _, anchor, fan)| {
            !fan && plan.schema().has_vertex(anchor.as_ref()) && spec.anchors.contains(*anchor)
        })
        .map(|(at, _, _, _)| at)
        .collect()
}

fn value_subjects(segment: &AnySegment, plan: &ValidationPlan, spec: &CheckSpec) -> Vec<ViewRef> {
    let edges = ordinary_edges(segment);
    logical_nodes(segment)
        .into_iter()
        .filter(|(_, id, anchor, fan)| {
            !fan && spec
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
                                edges
                                    .iter()
                                    .all(|(_, src, edge)| src != id || *edge != field)
                            })
                })
        })
        .map(|(at, _, _, _)| at)
        .collect()
}

fn check_subjects(
    segment: &AnySegment,
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
            logical_nodes(segment)
                .into_iter()
                .filter(|(_, _, _, fan)| !fan)
                .map(|(at, _, _, _)| at)
                .collect(),
        ),
        CheckKind::ReservedVocabulary => {
            let mut refs: Vec<ViewRef> = logical_nodes(segment)
                .into_iter()
                .map(|(at, _, _, _)| at)
                .collect();
            refs.extend(ordinary_edges(segment).into_iter().map(|(at, _, _)| at));
            some(refs)
        }
        CheckKind::EdgeDeclared | CheckKind::EndpointCoherence => some(
            ordinary_edges(segment)
                .into_iter()
                .map(|(at, _, _)| at)
                .collect(),
        ),
        CheckKind::FanWellFormed => match segment {
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
            logical_nodes(segment)
                .into_iter()
                .map(|(at, _, _, _)| at)
                .collect(),
        ),
        CheckKind::RequiredValue if matches!(key.decl, SchemaRef::Key { .. }) => {
            some(anchored_subjects(segment, plan, spec))
        }
        CheckKind::ValueConstraint | CheckKind::RequiredValue => {
            some(value_subjects(segment, plan, spec))
        }
        CheckKind::External if matches!(key.decl, SchemaRef::Constraint { .. }) => {
            some(value_subjects(segment, plan, spec))
        }
        CheckKind::RequiredEdge
        | CheckKind::OutDegree
        | CheckKind::PositionDistinct
        | CheckKind::Attribute
        | CheckKind::ReferenceResolves
        | CheckKind::External => some(anchored_subjects(segment, plan, spec)),
    }
}

/// The ordinary/native edges visible to checks about logical elements. A
/// graph keeps its reified fan edges; a tree's native view does not.
fn context_edges(segment: &AnySegment) -> Vec<(ViewRef, u32, u32)> {
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
        AnySegment::Tree(tree) => {
            let fan_edges = tree.fan_edge_ids();
            let fan_nodes = tree.fan_node_ids();
            tree.arcs
                .iter()
                .filter(|(id, (src, tgt, _))| {
                    !fan_edges.contains(id) && !fan_nodes.contains(src) && !fan_nodes.contains(tgt)
                })
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
                .collect()
        }
    }
}

const fn subject_id(subject: Option<&ViewRef>) -> Option<u32> {
    match subject {
        Some(ViewRef::Node(id)) => Some(*id),
        Some(ViewRef::Row(id)) => Some(id.0),
        _ => None,
    }
}

/// The number of distinct segment elements one local check instance reads.
/// This is the validation footprint table restricted to local kinds, over
/// the same native/reified views used for evaluation.
fn local_reads(segment: &AnySegment, kind: CheckKind, subject: Option<&ViewRef>) -> u64 {
    let mut reads = BTreeSet::new();
    match (kind, subject) {
        (
            CheckKind::NodeAnchor
            | CheckKind::ReservedVocabulary
            | CheckKind::EdgeDeclared
            | CheckKind::Attribute,
            Some(at),
        ) => {
            reads.insert(at.clone());
        }
        (CheckKind::EndpointCoherence, Some(at)) => {
            add_endpoint_reads(&mut reads, segment, at);
        }
        (CheckKind::FanWellFormed, Some(at @ ViewRef::Node(id))) => {
            add_fan_reads(&mut reads, segment, at, *id);
        }
        (
            CheckKind::RequiredEdge
            | CheckKind::OutDegree
            | CheckKind::PositionDistinct
            | CheckKind::ValueConstraint
            | CheckKind::RequiredValue,
            Some(at),
        ) => {
            add_out_reads(&mut reads, segment, at);
        }
        (CheckKind::TreeParentMap, Some(at)) => {
            add_parent_reads(&mut reads, segment, at);
        }
        (CheckKind::External, Some(at)) => {
            add_external_reads(&mut reads, segment, at);
        }
        _ => {}
    }
    u64::try_from(reads.len()).unwrap_or(u64::MAX)
}

fn add_endpoint_reads(reads: &mut BTreeSet<ViewRef>, segment: &AnySegment, at: &ViewRef) {
    reads.insert(at.clone());
    for (_, src, tgt) in context_edges(segment)
        .into_iter()
        .filter(|(edge, _, _)| edge == at)
    {
        reads.insert(segment.node_ref(src));
        reads.insert(segment.node_ref(tgt));
    }
}

fn add_fan_reads(reads: &mut BTreeSet<ViewRef>, segment: &AnySegment, at: &ViewRef, id: u32) {
    reads.insert(at.clone());
    reads.extend(
        segment
            .owned_edges()
            .into_iter()
            .filter(|edge| edge.src == id || edge.tgt == id)
            .map(|edge| edge.elem),
    );
    reads.extend(
        segment
            .boundary()
            .in_stubs
            .iter()
            .filter(|(_, stub)| stub.tgt == id)
            .map(|(edge, _)| edge.clone()),
    );
}

fn add_out_reads(reads: &mut BTreeSet<ViewRef>, segment: &AnySegment, at: &ViewRef) {
    reads.insert(at.clone());
    if let Some(id) = subject_id(Some(at)) {
        reads.extend(
            context_edges(segment)
                .into_iter()
                .filter(|(_, src, _)| *src == id)
                .map(|(edge, _, _)| edge),
        );
    }
}

fn add_parent_reads(reads: &mut BTreeSet<ViewRef>, segment: &AnySegment, at: &ViewRef) {
    reads.insert(at.clone());
    if let Some(id) = subject_id(Some(at)) {
        reads.extend(
            context_edges(segment)
                .into_iter()
                .filter(|(_, _, tgt)| *tgt == id)
                .map(|(edge, _, _)| edge),
        );
        reads.extend(
            segment
                .boundary()
                .in_stubs
                .iter()
                .filter(|(edge, stub)| matches!(edge, ViewRef::Arc { .. }) && stub.tgt == id)
                .map(|(edge, _)| edge.clone()),
        );
    }
}

fn add_external_reads(reads: &mut BTreeSet<ViewRef>, segment: &AnySegment, at: &ViewRef) {
    reads.insert(at.clone());
    if let Some(id) = subject_id(Some(at)) {
        reads.extend(
            context_edges(segment)
                .into_iter()
                .filter(|(_, src, tgt)| *src == id || *tgt == id)
                .map(|(edge, _, _)| edge),
        );
        reads.extend(
            segment
                .boundary()
                .in_stubs
                .iter()
                .filter(|(_, stub)| stub.tgt == id)
                .map(|(edge, _)| edge.clone()),
        );
    }
}

fn coverage(
    segment: &AnySegment,
    plan: &ValidationPlan,
    diagnostics: &[Diagnostic],
    incomplete: &[IncompleteCheck],
) -> BTreeMap<CheckKey, CheckCoverage> {
    let shape = segment.shape();
    plan.checks()
        .iter()
        .filter_map(|(key, spec)| {
            let locality = spec.locality.get(&shape)?;
            let subjects = check_subjects(segment, plan, key, spec);
            let instances = subjects.len() as u64;
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
            if plan
                .static_findings()
                .iter()
                .any(|entry| entry.check == *key)
            {
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

            let failed: BTreeSet<Option<ViewRef>> = diagnostics
                .iter()
                .filter(|entry| entry.check == *key)
                .map(|entry| entry.subject.clone())
                .collect();
            let mut undecided: BTreeSet<Option<ViewRef>> = BTreeSet::new();
            for entry in incomplete.iter().filter(|entry| entry.check == *key) {
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
            undecided.retain(|subject| !failed.contains(subject));
            let failed = failed.len() as u64;
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
