//! The single-step basis of edited views the law checker visits.
//!
//! For every [`Mutation`] family this generates, deterministically, the
//! views one edit away from a resolution's view, under the semantic schema.
//! The shape-only families are the lens checker's own families under an
//! identity lens; the families that depend on the lens read the
//! resolution's complement instead: its span nodes and occurrence edges
//! are the synthesized carriers, its redirected holders mark edits whose
//! closure depends on source reassembly, and a resolution contracts
//! nothing. Families without a complement-independent exact certificate
//! are skipped.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use panproto_inst::resolution_complement::{RefOccurrence, ResolutionComplement};
use panproto_inst::{
    EdgeId, FInstance, FieldPresence, GEdge, GInstance, HYPER_PARENT_KIND, Node, NodeShape, RowId,
    Value, WInstance,
};
use panproto_lens::{Mutation, SkipReason};
use panproto_schema::{Edge, Protocol, Schema};

use super::error::ResolveDomainError;

/// One deliberately refused view and its complete ordered domain report.
#[derive(Clone, Debug)]
pub struct NegativeCase<I> {
    /// The view presented to the put-domain predicate and raw `put`.
    pub view: I,
    /// Every expected refusal, in canonical report order.
    pub expected: Vec<ResolveDomainError>,
}

/// The candidates of one family, or why it has none.
#[derive(Clone, Debug)]
pub enum Cases<I> {
    /// Candidate views, in deterministic order.
    Views(Vec<I>),
    /// Deliberately refused views with independent exact certificates.
    Negative(Vec<NegativeCase<I>>),
    /// Why the family is not run.
    Skip(SkipReason),
}

fn collected<I>(views: Vec<I>) -> Cases<I> {
    if views.is_empty() {
        Cases::Skip(SkipReason::NoCandidate)
    } else {
        Cases::Views(views)
    }
}

fn collected_negative<I>(cases: Vec<NegativeCase<I>>) -> Cases<I> {
    if cases.is_empty() {
        Cases::Skip(SkipReason::NoCandidate)
    } else {
        Cases::Negative(cases)
    }
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

const fn alternate_shape(shape: &NodeShape) -> NodeShape {
    if matches!(shape, NodeShape::Plain) {
        NodeShape::List
    } else {
        NodeShape::Plain
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

fn sorted_labels(schema: &Schema) -> Vec<Edge> {
    let mut labels: Vec<Edge> = schema.edges.keys().cloned().collect();
    labels.sort_unstable();
    labels
}

fn sorted_vertices(schema: &Schema) -> Vec<panproto_gat::Name> {
    let mut anchors: Vec<_> = schema.vertices.keys().cloned().collect();
    anchors.sort_unstable();
    anchors
}

/// The synthesized carriers and redirect-sensitive holders a complement
/// names.
#[derive(Debug, Default)]
pub struct Presentation {
    /// Synthesized nodes: span nodes.
    pub nodes: BTreeSet<u32>,
    /// Synthesized edges: occurrence edges.
    pub edges: BTreeSet<EdgeId>,
    /// Recorded occurrence data by synthesized edge id.
    pub occurrences: BTreeMap<EdgeId, Vec<RefOccurrence>>,
    /// Holders whose complete source fragments live in the redirect residue.
    ///
    /// Edits of occurrences at these holders are deliberately absent from
    /// the exact-negative basis. Whether such an edit is admitted, and which
    /// collateral carriers a refusal names, depends on fragment reassembly
    /// rank rather than on the occurrence record alone.
    pub redirect_holders: BTreeSet<u32>,
    /// Every edge id the complement names, in or out of the view.
    pub edge_ids: BTreeSet<EdgeId>,
}

impl Presentation {
    /// The presentation of a resolution complement.
    #[must_use]
    pub fn of(complement: &ResolutionComplement) -> Self {
        let index = complement.synth_index();
        let edge_ids = complement
            .occurrences
            .keys()
            .chain(complement.spanning.values())
            .filter_map(|carrier| match carrier {
                panproto_inst::ViewRef::Edge(id) => Some(*id),
                _ => None,
            })
            .chain(complement.redirected.values().flat_map(|fragment| {
                fragment
                    .attachment
                    .iter()
                    .map(|attachment| EdgeId(attachment.source_slot))
                    .chain(fragment.arcs.iter().map(|arc| EdgeId(arc.source_slot)))
            }))
            .collect();
        let occurrences = complement
            .occurrences
            .iter()
            .filter_map(|(carrier, records)| match carrier {
                panproto_inst::ViewRef::Edge(id) => Some((*id, records.clone())),
                _ => None,
            })
            .collect();
        let redirect_holders = complement.redirected.keys().copied().collect();
        Self {
            nodes: index.nodes.keys().copied().collect(),
            edges: index.edges.keys().copied().collect(),
            occurrences,
            redirect_holders,
            edge_ids,
        }
    }
}

fn sorted_nodes(view: &GInstance) -> Vec<u32> {
    let mut ids: Vec<_> = view.nodes().keys().copied().collect();
    ids.sort_unstable();
    ids
}

fn sorted_edges(view: &GInstance) -> Vec<EdgeId> {
    let mut ids: Vec<_> = view.edges().keys().copied().collect();
    ids.sort_unstable();
    ids
}

/// The graph views one edit of `mutation` away from `view`.
#[must_use]
pub fn graph_cases(
    schema: &Schema,
    view: &GInstance,
    presentation: &Presentation,
    mutation: Mutation,
) -> Cases<GInstance> {
    let synth = presentation;
    match mutation {
        Mutation::RemoveSynthesized => {
            return collected_negative(graph_synthesized(view, synth, true));
        }
        Mutation::EditSynthesized => {
            return collected_negative(graph_synthesized(view, synth, false));
        }
        // A resolution contracts nothing. A redirected root id also cannot
        // be isolated from its source-slot residue by a local graph edit: a
        // real attachment at a fresh edge id can re-embed at another compact
        // arc rank, so there is no complement-independent exact witness.
        Mutation::AddContractedEdge | Mutation::ReservedIdReuse => {
            return Cases::Skip(SkipReason::NoCandidate);
        }
        Mutation::MalformedFan => return collected_negative(graph_malformed_fan(view, synth)),
        _ => {}
    }
    let views = match mutation {
        Mutation::ValueEdit => graph_value(schema, view, synth),
        Mutation::FieldEdit => graph_field(view, synth),
        Mutation::AddNode => graph_add_node(schema, view),
        Mutation::RemoveNode => graph_remove_node(view, synth),
        Mutation::RelabelNode => graph_relabel_node(schema, view, synth),
        Mutation::AddEdge => graph_add_edges(schema, view, synth, false, false),
        Mutation::RemoveEdge => graph_remove_edge(view, synth),
        Mutation::Reconnect => graph_reconnect(view, synth),
        Mutation::RelabelEdge => graph_relabel_edge(schema, view, synth),
        Mutation::Reorder => graph_reorder(view, synth),
        Mutation::AddParallelEdge => graph_add_edges(schema, view, synth, true, false),
        Mutation::AddSelfLoop => graph_add_edges(schema, view, synth, false, true),
        Mutation::RemoveSynthesized
        | Mutation::EditSynthesized
        | Mutation::AddContractedEdge
        | Mutation::ReservedIdReuse
        | Mutation::MalformedFan => unreachable!("negative families returned above"),
    };
    collected(views)
}

fn graph_value(schema: &Schema, view: &GInstance, synth: &Presentation) -> Vec<GInstance> {
    let mut views = Vec::new();
    for id in sorted_nodes(view) {
        if synth.nodes.contains(&id) {
            continue;
        }
        let replacement = view.node(id).and_then(|node| {
            let kind = schema.vertices.get(&node.anchor)?.kind.as_ref();
            alternate_value(node.value.as_ref()?, kind)
        });
        if let Some(replacement) = replacement {
            let mut next = view.clone();
            if let Some(node) = next.node_mut(id) {
                node.value = Some(replacement);
            }
            views.push(next);
        }
    }
    views
}

fn graph_field(view: &GInstance, synth: &Presentation) -> Vec<GInstance> {
    let mut views = Vec::new();
    for id in sorted_nodes(view) {
        if synth.nodes.contains(&id) {
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
                views.push(next);
            }
        }
        let mut discriminator = view.clone();
        if let Some(node) = discriminator.node_mut(id) {
            node.discriminator = node
                .discriminator
                .as_ref()
                .map_or_else(|| Some("__law__".into()), |_| None);
            views.push(discriminator);
        }
        let mut shape = view.clone();
        if let Some(node) = shape.node_mut(id) {
            node.shape = alternate_shape(&node.shape);
            views.push(shape);
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
                views.push(next);
            }
        }
    }
    views
}

fn graph_add_node(schema: &Schema, view: &GInstance) -> Vec<GInstance> {
    let Some(id) = fresh_u32(view.nodes().keys().copied()) else {
        return Vec::new();
    };
    let mut views = Vec::new();
    for anchor in sorted_vertices(schema) {
        let mut next = view.clone();
        if next
            .insert_node(node_for_anchor(id, schema, &anchor))
            .is_ok()
        {
            views.push(next);
        }
    }
    views
}

fn graph_remove_node(view: &GInstance, synth: &Presentation) -> Vec<GInstance> {
    let mut views = Vec::new();
    for id in sorted_nodes(view) {
        if synth.nodes.contains(&id) {
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
                if synth.nodes.contains(&other) && removed.insert(other) {
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
            views.push(next);
        }
    }
    views
}

fn graph_relabel_node(schema: &Schema, view: &GInstance, synth: &Presentation) -> Vec<GInstance> {
    let anchors = sorted_vertices(schema);
    let mut views = Vec::new();
    for id in sorted_nodes(view) {
        if synth.nodes.contains(&id) {
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
                    views.push(next);
                }
            }
        }
    }
    views
}

fn graph_add_edges(
    schema: &Schema,
    view: &GInstance,
    synth: &Presentation,
    parallel_only: bool,
    self_loop_only: bool,
) -> Vec<GInstance> {
    let Some(id) = fresh_u32(view.edges().keys().map(|id| id.0)).map(EdgeId) else {
        return Vec::new();
    };
    let mut views = Vec::new();
    for label in sorted_labels(schema) {
        let endpoints = |vertex: &panproto_gat::Name| -> Vec<u32> {
            let mut ids: Vec<u32> = view
                .nodes()
                .iter()
                .filter(|(id, node)| node.anchor == *vertex && !synth.nodes.contains(id))
                .map(|(&id, _)| id)
                .collect();
            ids.sort_unstable();
            ids
        };
        let (srcs, tgts) = (endpoints(&label.src), endpoints(&label.tgt));
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
                    views.push(next);
                }
            }
        }
    }
    views
}

fn graph_remove_edge(view: &GInstance, synth: &Presentation) -> Vec<GInstance> {
    let mut views = Vec::new();
    for id in sorted_edges(view) {
        if !synth.edges.contains(&id) {
            let mut next = view.clone();
            if next.remove_edge(id).is_ok() {
                views.push(next);
            }
        }
    }
    views
}

fn graph_reconnect(view: &GInstance, synth: &Presentation) -> Vec<GInstance> {
    let nodes = sorted_nodes(view);
    let mut views = Vec::new();
    for id in sorted_edges(view) {
        if synth.edges.contains(&id) {
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
                        views.push(next);
                    }
                }
            }
        }
    }
    views
}

fn graph_relabel_edge(schema: &Schema, view: &GInstance, synth: &Presentation) -> Vec<GInstance> {
    let labels = sorted_labels(schema);
    let mut views = Vec::new();
    for id in sorted_edges(view) {
        if synth.edges.contains(&id) {
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
                    views.push(next);
                }
            }
        }
    }
    views
}

fn graph_reorder(view: &GInstance, synth: &Presentation) -> Vec<GInstance> {
    let mut views = Vec::new();
    for id in sorted_nodes(view) {
        if !synth.nodes.contains(&id) {
            let mut next = view.clone();
            if let Some(node) = next.node_mut(id) {
                node.position = Some(alternate_position(node.position));
                views.push(next);
            }
        }
    }
    for id in sorted_edges(view) {
        if !synth.edges.contains(&id) {
            let mut next = view.clone();
            let position = Some(alternate_position(
                next.edge(id).and_then(|edge| edge.position),
            ));
            if next.set_edge_position(id, position).is_ok() {
                views.push(next);
            }
        }
    }
    views
}

fn certifiable_occurrence<'a>(
    view: &GInstance,
    synth: &'a Presentation,
    edge: EdgeId,
) -> Option<&'a RefOccurrence> {
    let target = view.edge(edge)?.tgt;
    if synth.nodes.contains(&target) {
        // Materialized standoff spans can disappear with their occurrence,
        // so their complete refusal is not intrinsic to the edge record.
        return None;
    }
    let [record] = synth.occurrences.get(&edge)?.as_slice() else {
        return None;
    };
    if matches!(record.holder, panproto_inst::ViewRef::Node(holder) if synth.redirect_holders.contains(&holder))
    {
        // Removing a redirect occurrence is a semantic edit. Editing it can
        // shift arbitrary later fragment, fan and structural carriers during
        // reassembly. Neither case has an edge-local exact certificate.
        return None;
    }
    let group_size = synth
        .occurrences
        .values()
        .flatten()
        .filter(|candidate| {
            candidate.holder == record.holder && candidate.reference == record.reference
        })
        .count();
    (group_size == 1).then_some(record)
}

fn graph_synthesized(
    view: &GInstance,
    synth: &Presentation,
    remove: bool,
) -> Vec<NegativeCase<GInstance>> {
    let mut cases = Vec::new();
    for &id in &synth.edges {
        if certifiable_occurrence(view, synth, id).is_none() {
            continue;
        }
        let mut next = view.clone();
        let changed = if remove {
            next.remove_edge(id).is_ok()
        } else {
            next.edge(id).is_some() && next.set_edge_position(id, Some(0)).is_ok()
        };
        if changed {
            cases.push(NegativeCase {
                view: next,
                expected: vec![ResolveDomainError::SynthesizedEdited {
                    carrier: panproto_inst::ViewRef::Edge(id),
                }],
            });
        }
    }
    cases
}

fn fan_edges(view: &GInstance, fan: u32) -> Vec<EdgeId> {
    let mut edges: Vec<EdgeId> = view
        .out_edges(fan)
        .iter()
        .copied()
        .filter(|edge| {
            view.edge(*edge).is_some_and(|edge| {
                matches!(
                    edge.edge.kind.as_ref(),
                    HYPER_PARENT_KIND | panproto_inst::HYPER_LABEL_KIND
                )
            })
        })
        .collect();
    edges.sort_unstable();
    edges
}

fn graph_malformed_fan(view: &GInstance, synth: &Presentation) -> Vec<NegativeCase<GInstance>> {
    let Some(fresh) =
        fresh_u32(view.edges().keys().chain(&synth.edge_ids).map(|id| id.0)).map(EdgeId)
    else {
        return Vec::new();
    };
    let malformed = sorted_nodes(view).into_iter().rfind(|id| {
        view.node(*id)
            .is_some_and(|node| matches!(node.shape, NodeShape::HyperEdge))
    });
    let Some(malformed) = malformed else {
        return Vec::new();
    };
    // A malformed earlier fan can shift every later fan during source
    // reconstruction. The last fan is the one position whose refusal is
    // independent of that rank-sensitive suffix.
    let Some(parent) = fan_edges(view, malformed).into_iter().find(|edge| {
        view.edge(*edge)
            .is_some_and(|edge| edge.edge.kind.as_ref() == HYPER_PARENT_KIND)
    }) else {
        return Vec::new();
    };
    let Some(duplicate) = view.edge(parent).cloned() else {
        return Vec::new();
    };
    let mut next = view.clone();
    if next.insert_edge(fresh, duplicate).is_err() {
        return Vec::new();
    }
    vec![NegativeCase {
        view: next,
        expected: vec![ResolveDomainError::NoPlacement {
            carrier: panproto_inst::ViewRef::Node(malformed),
        }],
    }]
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

fn sorted_tree_ids(view: &WInstance) -> Vec<u32> {
    let mut ids: Vec<_> = view.nodes.keys().copied().collect();
    ids.sort_unstable();
    ids
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

fn tree_descendants(instance: &WInstance, root: u32) -> BTreeSet<u32> {
    let mut found = BTreeSet::new();
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

/// The tree views one edit of `mutation` away from `view`, which has no
/// synthesized carriers and no reserved ids.
#[must_use]
pub fn tree_cases(schema: &Schema, view: &WInstance, mutation: Mutation) -> Cases<WInstance> {
    let views = match mutation {
        Mutation::ValueEdit => tree_value(schema, view),
        Mutation::FieldEdit => tree_field(view),
        Mutation::AddNode | Mutation::AddEdge => tree_insert(schema, view),
        Mutation::RemoveNode => tree_remove_node(view),
        Mutation::RelabelNode => tree_relabel_node(schema, view),
        Mutation::RemoveEdge => tree_remove_edge(view),
        Mutation::Reconnect => tree_reconnect(view),
        Mutation::RelabelEdge => tree_relabel_edge(schema, view),
        Mutation::Reorder => tree_reorder(view),
        Mutation::AddParallelEdge | Mutation::AddSelfLoop | Mutation::MalformedFan => {
            return Cases::Skip(SkipReason::ShapeCannotExpress);
        }
        Mutation::RemoveSynthesized
        | Mutation::EditSynthesized
        | Mutation::AddContractedEdge
        | Mutation::ReservedIdReuse => Vec::new(),
    };
    collected(views)
}

fn tree_value(schema: &Schema, view: &WInstance) -> Vec<WInstance> {
    let mut views = Vec::new();
    for id in sorted_tree_ids(view) {
        let replacement = view.nodes.get(&id).and_then(|node| {
            let kind = schema.vertices.get(&node.anchor)?.kind.as_ref();
            alternate_value(node.value.as_ref()?, kind)
        });
        if let Some(replacement) = replacement {
            let mut next = view.clone();
            if let Some(node) = next.nodes.get_mut(&id) {
                node.value = Some(replacement);
            }
            views.push(next);
        }
    }
    views
}

fn tree_field(view: &WInstance) -> Vec<WInstance> {
    let mut views = Vec::new();
    for id in sorted_tree_ids(view) {
        let Some(node) = view.nodes.get(&id) else {
            continue;
        };
        let mut fields: Vec<_> = node.extra_fields.keys().cloned().collect();
        fields.sort_unstable();
        for field in fields {
            let mut next = view.clone();
            if let Some(node) = next.nodes.get_mut(&id) {
                let replacement = node.extra_fields.get(&field).map_or_else(
                    || Value::Bool(true),
                    |value| alternate_flat_value(value, None).unwrap_or(Value::Bool(true)),
                );
                node.extra_fields.insert(field, replacement);
                views.push(next);
            }
        }
        let mut discriminator = view.clone();
        if let Some(node) = discriminator.nodes.get_mut(&id) {
            node.discriminator = node
                .discriminator
                .as_ref()
                .map_or_else(|| Some("__law__".into()), |_| None);
            views.push(discriminator);
        }
        let mut shape = view.clone();
        if let Some(node) = shape.nodes.get_mut(&id) {
            node.shape = alternate_shape(&node.shape);
            views.push(shape);
        }
        let mut annotations: Vec<_> = node.annotations.keys().cloned().collect();
        annotations.sort_unstable();
        for annotation in annotations {
            let mut next = view.clone();
            if let Some(node) = next.nodes.get_mut(&id) {
                let replacement = node.annotations.get(&annotation).map_or_else(
                    || Value::Bool(true),
                    |value| alternate_flat_value(value, None).unwrap_or(Value::Bool(true)),
                );
                node.annotations.insert(annotation, replacement);
                views.push(next);
            }
        }
    }
    views
}

fn tree_insert(schema: &Schema, view: &WInstance) -> Vec<WInstance> {
    let Some(id) = fresh_u32(view.nodes.keys().copied()) else {
        return Vec::new();
    };
    let mut views = Vec::new();
    for parent in sorted_tree_ids(view) {
        let mut outgoing = schema.outgoing_edges(&view.nodes[&parent].anchor).to_vec();
        outgoing.sort_unstable();
        for edge in outgoing {
            let mut nodes = view.nodes.clone();
            nodes.insert(id, node_for_anchor(id, schema, &edge.tgt));
            let mut arcs = view.arcs.clone();
            arcs.push((parent, id, edge));
            views.push(rebuild_tree(view, nodes, arcs));
        }
    }
    views
}

fn tree_remove_node(view: &WInstance) -> Vec<WInstance> {
    let mut views = Vec::new();
    for id in sorted_tree_ids(view)
        .into_iter()
        .filter(|id| *id != view.root && view.children(*id).is_empty())
    {
        let mut nodes = view.nodes.clone();
        nodes.remove(&id);
        let arcs = view
            .arcs
            .iter()
            .filter(|(parent, child, _)| *parent != id && *child != id)
            .cloned()
            .collect();
        views.push(rebuild_tree(view, nodes, arcs));
    }
    views
}

fn tree_relabel_node(schema: &Schema, view: &WInstance) -> Vec<WInstance> {
    let anchors = sorted_vertices(schema);
    let mut views = Vec::new();
    for id in sorted_tree_ids(view) {
        for anchor in &anchors {
            let old = &view.nodes[&id].anchor;
            let arcs_fit = view.arcs.iter().all(|(parent, child, edge)| {
                (*parent != id || edge.src == *anchor) && (*child != id || edge.tgt == *anchor)
            });
            if old != anchor && arcs_fit {
                let mut next = view.clone();
                if let Some(node) = next.nodes.get_mut(&id) {
                    node.anchor = anchor.clone();
                    views.push(next);
                }
            }
        }
    }
    views
}

fn tree_remove_edge(view: &WInstance) -> Vec<WInstance> {
    let mut views = Vec::new();
    for (_, _, child, _) in canonical_tree_arcs(view) {
        if view.children(child).is_empty() {
            let mut nodes = view.nodes.clone();
            nodes.remove(&child);
            let arcs = view
                .arcs
                .iter()
                .filter(|(_, candidate, _)| *candidate != child)
                .cloned()
                .collect();
            views.push(rebuild_tree(view, nodes, arcs));
        }
    }
    views
}

fn tree_reconnect(view: &WInstance) -> Vec<WInstance> {
    let mut views = Vec::new();
    for (index, old_parent, child, edge) in canonical_tree_arcs(view) {
        let descendants = tree_descendants(view, child);
        for parent in sorted_tree_ids(view) {
            if parent != old_parent
                && parent != child
                && !descendants.contains(&parent)
                && view.nodes[&parent].anchor == edge.src
            {
                let mut arcs = view.arcs.clone();
                arcs[index].0 = parent;
                views.push(rebuild_tree(view, view.nodes.clone(), arcs));
            }
        }
    }
    views
}

fn tree_relabel_edge(schema: &Schema, view: &WInstance) -> Vec<WInstance> {
    let mut views = Vec::new();
    for (index, parent, child, old) in canonical_tree_arcs(view) {
        let mut replacements = schema
            .edges_between(&view.nodes[&parent].anchor, &view.nodes[&child].anchor)
            .to_vec();
        replacements.sort_unstable();
        for edge in replacements {
            if edge != *old {
                let mut arcs = view.arcs.clone();
                arcs[index].2 = edge;
                views.push(rebuild_tree(view, view.nodes.clone(), arcs));
            }
        }
    }
    views
}

fn tree_reorder(view: &WInstance) -> Vec<WInstance> {
    let mut views = Vec::new();
    for id in sorted_tree_ids(view) {
        let mut next = view.clone();
        if let Some(node) = next.nodes.get_mut(&id) {
            node.position = Some(alternate_position(node.position));
            views.push(next);
        }
    }
    let arcs = canonical_tree_arcs(view);
    for (left, (i, left_parent, _, _)) in arcs.iter().enumerate() {
        for (j, right_parent, _, _) in arcs.iter().skip(left + 1) {
            if left_parent == right_parent {
                let mut reordered = view.arcs.clone();
                reordered.swap(*i, *j);
                views.push(rebuild_tree(view, view.nodes.clone(), reordered));
            }
        }
    }
    views
}

/// The table views one edit of `mutation` away from `view`, which has no
/// synthesized pairs and no reserved rows; `protocol` gives the scalar
/// kinds of cells.
#[must_use]
pub fn table_cases(
    schema: &Schema,
    protocol: &Protocol,
    view: &FInstance,
    mutation: Mutation,
) -> Cases<FInstance> {
    let views = match mutation {
        Mutation::ValueEdit => table_payload(schema, protocol, view, true),
        Mutation::FieldEdit => table_payload(schema, protocol, view, false),
        Mutation::AddNode => table_add_row(schema, view),
        Mutation::RemoveNode => table_remove_row(view),
        Mutation::RelabelNode => table_relabel_row(schema, view),
        Mutation::AddEdge => table_add_pairs(schema, view, false),
        Mutation::AddSelfLoop => table_add_pairs(schema, view, true),
        Mutation::RemoveEdge => table_remove_pair(view),
        Mutation::Reconnect => table_reconnect(view),
        Mutation::RelabelEdge => table_relabel_pair(schema, view),
        Mutation::Reorder | Mutation::AddParallelEdge | Mutation::MalformedFan => {
            return Cases::Skip(SkipReason::ShapeCannotExpress);
        }
        Mutation::RemoveSynthesized
        | Mutation::EditSynthesized
        | Mutation::AddContractedEdge
        | Mutation::ReservedIdReuse => Vec::new(),
    };
    collected(views)
}

fn table_cell_scalar_kind(
    schema: &Schema,
    protocol: &Protocol,
    table: &str,
    cell: &str,
) -> Option<panproto_schema::ScalarKind> {
    let mut columns: Vec<_> = schema
        .edges
        .keys()
        .filter(|edge| {
            edge.src.as_ref() == table
                && edge.name.as_ref().is_some_and(|name| name.as_ref() == cell)
        })
        .collect();
    columns.sort_unstable();
    columns
        .into_iter()
        .next()
        .map(|edge| panproto_schema::ValueSpec::for_vertex(schema, protocol, &edge.tgt).kind)
}

fn table_cell_coordinates(
    schema: &Schema,
    protocol: &Protocol,
    view: &FInstance,
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
    cells.extend(schema.edges.keys().filter_map(|edge| {
        let target = schema.vertices.get(&edge.tgt)?;
        (edge.src == *table && protocol.value_kinds.contains_key(target.kind.as_ref()))
            .then(|| edge.name.as_ref().map(ToString::to_string))
            .flatten()
    }));
    cells
}

fn table_payload(
    schema: &Schema,
    protocol: &Protocol,
    view: &FInstance,
    value_edit: bool,
) -> Vec<FInstance> {
    let mut views = Vec::new();
    for id in view.row_ids() {
        for (index, cell) in table_cell_coordinates(schema, protocol, view, id)
            .into_iter()
            .enumerate()
        {
            if value_edit != (index == 0) {
                continue;
            }
            let scalar_kind = view
                .table_of(id)
                .and_then(|table| table_cell_scalar_kind(schema, protocol, table, &cell));
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
                views.push(next);
            }
        }
    }
    views
}

fn table_add_row(schema: &Schema, view: &FInstance) -> Vec<FInstance> {
    let Some(id) = fresh_u32(view.row_ids().iter().map(|id| id.0)).map(RowId) else {
        return Vec::new();
    };
    let mut views = Vec::new();
    for table in sorted_vertices(schema) {
        let mut next = view.clone();
        if next.insert_row(table, id, HashMap::new()).is_ok() {
            views.push(next);
        }
    }
    views
}

fn table_remove_row(view: &FInstance) -> Vec<FInstance> {
    let mut views = Vec::new();
    for id in view.row_ids() {
        let mut next = view.clone();
        if next.remove_row(id).is_some() {
            views.push(next);
        }
    }
    views
}

fn table_relabel_row(schema: &Schema, view: &FInstance) -> Vec<FInstance> {
    let tables = sorted_vertices(schema);
    let mut views = Vec::new();
    for id in view.row_ids() {
        for table in &tables {
            if view.table_of(id).is_some_and(|old| old != table) {
                let mut next = view.clone();
                if next.move_row(id, table.clone()).is_ok() {
                    views.push(next);
                }
            }
        }
    }
    views
}

fn table_add_pairs(schema: &Schema, view: &FInstance, loops_only: bool) -> Vec<FInstance> {
    let mut views = Vec::new();
    for edge in sorted_labels(schema) {
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
                    views.push(next);
                }
            }
        }
    }
    views
}

fn table_remove_pair(view: &FInstance) -> Vec<FInstance> {
    let mut views = Vec::new();
    for (edge, pairs) in view.foreign_keys() {
        for &(src, tgt) in pairs {
            let mut next = view.clone();
            if next.remove_foreign_key(edge, src, tgt) {
                views.push(next);
            }
        }
    }
    views
}

fn table_reconnect(view: &FInstance) -> Vec<FInstance> {
    let ids = view.row_ids();
    let mut views = Vec::new();
    for (edge, pairs) in view.foreign_keys() {
        for &(src, tgt) in pairs {
            for other in &ids {
                if *other != tgt && view.table_of(*other) == Some(&edge.tgt) {
                    let mut next = view.clone();
                    next.remove_foreign_key(edge, src, tgt);
                    if next.insert_foreign_key(edge.clone(), src, *other).is_ok() {
                        views.push(next);
                    }
                }
                if *other != src && view.table_of(*other) == Some(&edge.src) {
                    let mut next = view.clone();
                    next.remove_foreign_key(edge, src, tgt);
                    if next.insert_foreign_key(edge.clone(), *other, tgt).is_ok() {
                        views.push(next);
                    }
                }
            }
        }
    }
    views
}

fn table_relabel_pair(schema: &Schema, view: &FInstance) -> Vec<FInstance> {
    let labels = sorted_labels(schema);
    let mut views = Vec::new();
    for (edge, pairs) in view.foreign_keys() {
        for &(src, tgt) in pairs {
            for label in &labels {
                if label != edge
                    && view.table_of(src) == Some(&label.src)
                    && view.table_of(tgt) == Some(&label.tgt)
                {
                    let mut next = view.clone();
                    next.remove_foreign_key(edge, src, tgt);
                    if next.insert_foreign_key(label.clone(), src, tgt).is_ok() {
                        views.push(next);
                    }
                }
            }
        }
    }
    views
}

#[cfg(test)]
pub(super) mod tests {
    use std::collections::{BTreeMap, BTreeSet, HashMap};

    use panproto_inst::resolution_complement::RefOccurrence;
    use panproto_inst::{
        EdgeId, FInstance, FieldPresence, GEdge, GInstance, HYPER_PARENT_KIND, Node, NodeShape,
        RowId, Value, ViewRef, WInstance,
    };
    use panproto_lens::{
        GraphShape, LawShape, Lens, LensShape, Mutation, MutationCases, SkipReason, TableShape,
        TreeShape,
    };
    use panproto_schema::{Edge, Protocol, Schema, SchemaBuilder};

    use super::{Cases, Presentation, ResolveDomainError, graph_cases, table_cases, tree_cases};

    fn prop(src: &str, tgt: &str, name: &str) -> Edge {
        Edge {
            src: src.into(),
            tgt: tgt.into(),
            kind: "prop".into(),
            name: Some(name.into()),
        }
    }

    /// A document with one user and one post, the post's author written as a
    /// key and as a reference edge `4`.
    pub(in crate::resolution) fn small_schema() -> Result<Schema, panproto_schema::SchemaError> {
        let protocol = Protocol {
            name: "small".to_owned(),
            ..Protocol::default()
        };
        SchemaBuilder::new(&protocol)
            .vertex("doc", "object", None)?
            .vertex("user", "object", None)?
            .vertex("user_id", "string", None)?
            .vertex("post", "object", None)?
            .vertex("author", "string", None)?
            .edge("doc", "user", "prop", Some("users"))?
            .edge("user", "user_id", "prop", Some("id"))?
            .edge("doc", "post", "prop", Some("posts"))?
            .edge("post", "author", "prop", Some("author"))?
            .edge("post", "user", "ref", Some("author_ref"))?
            .entry("doc")
            .build()
    }

    pub(in crate::resolution) fn small_view() -> Result<GInstance, panproto_inst::InstError> {
        let mut view = GInstance::new();
        let text = |value: &str| FieldPresence::Present(Value::Str(value.to_owned()));
        for node in [
            Node::new(0, "doc"),
            Node::new(1, "user"),
            Node::new(2, "user_id").with_value(text("u1")),
            Node::new(3, "post"),
            Node::new(4, "author").with_value(text("u1")),
        ] {
            view.insert_node(node)?;
        }
        let reference = Edge {
            src: "post".into(),
            tgt: "user".into(),
            kind: "ref".into(),
            name: Some("author_ref".into()),
        };
        for (id, src, tgt, label) in [
            (0, 0, 1, prop("doc", "user", "users")),
            (1, 1, 2, prop("user", "user_id", "id")),
            (2, 0, 3, prop("doc", "post", "posts")),
            (3, 3, 4, prop("post", "author", "author")),
            (4, 3, 1, reference),
        ] {
            view.insert_edge(EdgeId(id), GEdge::new(src, tgt, label))?;
        }
        Ok(view)
    }

    fn identity_lens(schema: &Schema) -> Lens {
        Lens {
            compiled: panproto_inst::CompiledMigration {
                surviving_verts: schema.vertices.keys().cloned().collect(),
                surviving_edges: schema.edges.keys().cloned().collect(),
                ..panproto_inst::CompiledMigration::default()
            },
            src_schema: schema.clone(),
            tgt_schema: schema.clone(),
        }
    }

    /// The families whose candidates depend on the view's shape alone, not
    /// on which carriers are synthesized or reserved.
    const SHAPE_ONLY: [Mutation; 12] = [
        Mutation::ValueEdit,
        Mutation::FieldEdit,
        Mutation::AddNode,
        Mutation::RemoveNode,
        Mutation::RelabelNode,
        Mutation::AddEdge,
        Mutation::RemoveEdge,
        Mutation::Reconnect,
        Mutation::RelabelEdge,
        Mutation::Reorder,
        Mutation::AddParallelEdge,
        Mutation::AddSelfLoop,
    ];

    #[test]
    fn unique_nonredirect_occurrence_has_an_exact_nonvacuous_refusal_certificate()
    -> Result<(), Box<dyn std::error::Error>> {
        let schema = small_schema()?;
        let mut view = GInstance::new();
        view.insert_node(Node::new(1, "post"))?;
        view.insert_node(Node::new(5, "user"))?;
        let edge = EdgeId(7);
        view.insert_edge(
            edge,
            GEdge::new(
                1,
                5,
                Edge {
                    src: "post".into(),
                    tgt: "user".into(),
                    kind: "ref".into(),
                    name: Some("author_ref".into()),
                },
            ),
        )?;
        let record = RefOccurrence {
            holder: ViewRef::Node(1),
            reference: "author_ref".into(),
            index: 0,
            literal: Value::Str("u1".to_owned()),
            form: None,
            context: None,
        };
        let mut presentation = Presentation {
            edges: BTreeSet::from([edge]),
            occurrences: BTreeMap::from([(edge, vec![record])]),
            ..Presentation::default()
        };

        let Cases::Negative(cases) =
            graph_cases(&schema, &view, &presentation, Mutation::RemoveSynthesized)
        else {
            panic!("a unique nonredirect occurrence must produce a refusal witness");
        };
        assert_eq!(cases.len(), 1);
        assert_eq!(
            cases[0].expected,
            vec![ResolveDomainError::SynthesizedEdited {
                carrier: ViewRef::Edge(edge),
            }]
        );
        assert!(cases[0].view.edge(edge).is_none());

        presentation.redirect_holders.insert(1);
        assert!(matches!(
            graph_cases(&schema, &view, &presentation, Mutation::RemoveSynthesized),
            Cases::Skip(SkipReason::NoCandidate)
        ));
        Ok(())
    }

    fn occurrence(holder: u32, reference: &str, index: u32) -> RefOccurrence {
        RefOccurrence {
            holder: ViewRef::Node(holder),
            reference: reference.into(),
            index,
            literal: Value::Str("literal".to_owned()),
            form: None,
            context: None,
        }
    }

    fn occurrence_view(edges: &[(u32, u32, u32)]) -> Result<GInstance, panproto_inst::InstError> {
        let mut view = GInstance::new();
        for id in edges.iter().flat_map(|(_, src, tgt)| [*src, *tgt]) {
            if view.node(id).is_none() {
                view.insert_node(Node::new(id, if id < 10 { "holder" } else { "target" }))?;
            }
        }
        let label = Edge {
            src: "holder".into(),
            tgt: "target".into(),
            kind: "ref".into(),
            name: Some("reference".into()),
        };
        for &(edge, src, tgt) in edges {
            view.insert_edge(EdgeId(edge), GEdge::new(src, tgt, label.clone()))?;
        }
        Ok(view)
    }

    #[test]
    fn redirect_occurrences_are_omitted_before_collateral_slots_or_fans_can_shift()
    -> Result<(), Box<dyn std::error::Error>> {
        let schema = small_schema()?;
        let view = occurrence_view(&[(20, 1, 10), (21, 1, 11)])?;
        let presentation = Presentation {
            edges: [EdgeId(20), EdgeId(21)].into_iter().collect(),
            occurrences: BTreeMap::from([
                (EdgeId(20), vec![occurrence(1, "first", 0)]),
                (EdgeId(21), vec![occurrence(1, "second", 0)]),
            ]),
            redirect_holders: BTreeSet::from([1]),
            ..Presentation::default()
        };
        for mutation in [Mutation::RemoveSynthesized, Mutation::EditSynthesized] {
            assert!(
                matches!(
                    graph_cases(&schema, &view, &presentation, mutation),
                    Cases::Skip(SkipReason::NoCandidate)
                ),
                "{mutation:?} must not guess a redirect reassembly closure"
            );
        }
        Ok(())
    }

    #[test]
    fn duplicate_occurrence_rank_and_standoff_span_targets_are_omitted()
    -> Result<(), Box<dyn std::error::Error>> {
        let schema = small_schema()?;
        let view = occurrence_view(&[(20, 1, 10), (21, 1, 11), (22, 2, 12)])?;
        let presentation = Presentation {
            nodes: BTreeSet::from([12]),
            edges: [EdgeId(20), EdgeId(21), EdgeId(22)].into_iter().collect(),
            occurrences: BTreeMap::from([
                (EdgeId(20), vec![occurrence(1, "alias", 0)]),
                (EdgeId(21), vec![occurrence(1, "alias", 1)]),
                (EdgeId(22), vec![occurrence(2, "standoff", 0)]),
            ]),
            ..Presentation::default()
        };
        for mutation in [Mutation::RemoveSynthesized, Mutation::EditSynthesized] {
            assert!(
                matches!(
                    graph_cases(&schema, &view, &presentation, mutation),
                    Cases::Skip(SkipReason::NoCandidate)
                ),
                "{mutation:?} must omit rank-sensitive duplicates and span targets"
            );
        }
        Ok(())
    }

    #[test]
    fn distinct_nonredirect_aliases_keep_independent_exact_certificates()
    -> Result<(), Box<dyn std::error::Error>> {
        let schema = small_schema()?;
        let view = occurrence_view(&[(20, 1, 10), (21, 1, 11)])?;
        let presentation = Presentation {
            edges: [EdgeId(20), EdgeId(21)].into_iter().collect(),
            occurrences: BTreeMap::from([
                (EdgeId(20), vec![occurrence(1, "alias-a", 0)]),
                (EdgeId(21), vec![occurrence(1, "alias-b", 0)]),
            ]),
            ..Presentation::default()
        };
        for mutation in [Mutation::RemoveSynthesized, Mutation::EditSynthesized] {
            let Cases::Negative(cases) = graph_cases(&schema, &view, &presentation, mutation)
            else {
                panic!("{mutation:?} must retain independent aliases");
            };
            assert_eq!(cases.len(), 2, "{mutation:?}");
            assert_eq!(
                cases
                    .iter()
                    .map(|case| case.expected.clone())
                    .collect::<Vec<_>>(),
                vec![
                    vec![ResolveDomainError::SynthesizedEdited {
                        carrier: ViewRef::Edge(EdgeId(20)),
                    }],
                    vec![ResolveDomainError::SynthesizedEdited {
                        carrier: ViewRef::Edge(EdgeId(21)),
                    }],
                ],
                "{mutation:?}"
            );
        }
        Ok(())
    }

    #[test]
    fn malformed_fan_basis_uses_only_the_last_fan_to_avoid_cascade_prediction()
    -> Result<(), Box<dyn std::error::Error>> {
        let schema = small_schema()?;
        let mut view = GInstance::new();
        view.insert_node(Node::new(0, "doc"))?;
        for fan in [2, 4] {
            view.insert_node(Node::new(fan, "fan").with_shape(NodeShape::HyperEdge))?;
            view.insert_edge(
                EdgeId(10 + fan),
                GEdge::new(
                    fan,
                    0,
                    Edge {
                        src: "fan".into(),
                        tgt: "doc".into(),
                        kind: HYPER_PARENT_KIND.into(),
                        name: None,
                    },
                ),
            )?;
        }
        let Cases::Negative(cases) = graph_cases(
            &schema,
            &view,
            &Presentation::default(),
            Mutation::MalformedFan,
        ) else {
            panic!("the last well-formed fan must have a malformed witness");
        };
        assert_eq!(cases.len(), 1);
        assert_eq!(
            cases[0].expected,
            vec![ResolveDomainError::NoPlacement {
                carrier: ViewRef::Node(4),
            }]
        );
        assert_eq!(cases[0].view.out_edges(2).len(), 1);
        assert_eq!(cases[0].view.out_edges(4).len(), 2);
        Ok(())
    }

    #[test]
    fn shape_only_families_match_the_lens_checker_under_an_identity_lens()
    -> Result<(), Box<dyn std::error::Error>> {
        let schema = small_schema()?;
        let lens = identity_lens(&schema);
        let view = small_view()?;
        let (got, complement) = panproto_lens::graph_lens::get(&lens, &view)?;
        assert!(
            GInstance::identical(&got, &view),
            "the identity lens keeps the view"
        );
        let mut compared = 0;
        for mutation in SHAPE_ONLY {
            let ours = match graph_cases(&schema, &view, &Presentation::default(), mutation) {
                Cases::Views(views) => views,
                Cases::Negative(_) => {
                    panic!("shape-only graph family {mutation:?} became negative")
                }
                Cases::Skip(_) => Vec::new(),
            };
            assert!(
                GraphShape::negative_mutation_cases(
                    &lens,
                    &view,
                    &view,
                    &complement,
                    &(),
                    mutation,
                )
                .is_empty(),
                "shape-only graph family {mutation:?} became negative"
            );
            let theirs =
                match GraphShape::mutation_cases(&lens, &view, &view, &complement, &(), mutation) {
                    MutationCases::Cases(views) => views,
                    MutationCases::Skip(_) => Vec::new(),
                };
            assert_eq!(ours.len(), theirs.len(), "{mutation:?}");
            for (a, b) in ours.iter().zip(&theirs) {
                assert!(GInstance::identical(a, b), "{mutation:?}: {a:?} != {b:?}");
            }
            compared += ours.len();
        }
        assert!(compared > 0);
        Ok(())
    }

    /// Every vertex kind the value generators name, and two they do not.
    const KINDS: [&str; 18] = [
        "string",
        "date-time",
        "uuid",
        "float",
        "decimal",
        "boolean",
        "bytes",
        "octet-string",
        "token",
        "enum",
        "cid-link",
        "array",
        "integer-list",
        "object",
        "map",
        "null",
        "integer",
        "unheard-of",
    ];

    /// A schema with a `doc` root and one vertex per kind, each under a
    /// `prop` edge named after its index.
    fn kinds_schema() -> Result<Schema, panproto_schema::SchemaError> {
        let protocol = Protocol {
            name: "kinds".to_owned(),
            ..Protocol::default()
        };
        let mut builder = SchemaBuilder::new(&protocol).vertex("doc", "object", None)?;
        for (index, kind) in KINDS.iter().enumerate() {
            let vertex = format!("v{index}");
            builder = builder.vertex(&vertex, kind, None)?.edge(
                "doc",
                &vertex,
                "prop",
                Some(&format!("f{index}")),
            )?;
        }
        builder.entry("doc").build()
    }

    /// The values a node of the kinds view holds: null at every kind, then
    /// one of each value variant.
    fn kinds_nodes() -> Vec<Node> {
        let mut extra = HashMap::new();
        for (field, value) in [
            ("b", Value::Bool(true)),
            ("i", Value::Int(7)),
            ("z", Value::Float(0.0)),
            ("x", Value::Float(2.5)),
            ("s", Value::Str("s".to_owned())),
            ("n", Value::Null),
            ("y", Value::Bytes(vec![1])),
        ] {
            extra.insert(field.to_owned(), value);
        }
        let mut root = Node::new(0, "doc");
        root.extra_fields = extra;
        let mut nodes = vec![root];
        let presents = [
            Value::Bool(false),
            Value::Int(1),
            Value::Float(0.0),
            Value::Float(1.5),
            Value::Str("t".to_owned()),
            Value::Bytes(vec![2]),
        ];
        for (index, _) in KINDS.iter().enumerate() {
            let id = u32::try_from(index).unwrap_or(u32::MAX).saturating_add(1);
            let node = Node::new(id, format!("v{index}")).with_value(FieldPresence::Null);
            nodes.push(node);
        }
        for (offset, value) in presents.into_iter().enumerate() {
            let id = u32::try_from(KINDS.len() + offset + 1).unwrap_or(u32::MAX);
            let vertex = format!("v{}", offset % KINDS.len());
            let mut node = Node::new(id, vertex).with_value(FieldPresence::Present(value));
            node.position = u32::try_from(offset).ok();
            nodes.push(node);
        }
        nodes
    }

    fn kinds_label(node: &Node) -> Edge {
        let index = node.anchor.trim_start_matches('v');
        prop("doc", &node.anchor, &format!("f{index}"))
    }

    fn kinds_graph() -> Result<GInstance, panproto_inst::InstError> {
        let mut view = GInstance::new();
        let nodes = kinds_nodes();
        for node in &nodes {
            view.insert_node(node.clone())?;
        }
        for (id, node) in nodes.iter().skip(1).enumerate() {
            let id = u32::try_from(id).unwrap_or(u32::MAX);
            view.insert_edge(EdgeId(id), GEdge::new(0, node.id, kinds_label(node)))?;
        }
        Ok(view)
    }

    fn kinds_tree() -> WInstance {
        let nodes = kinds_nodes();
        let arcs = nodes
            .iter()
            .skip(1)
            .map(|node| (0, node.id, kinds_label(node)))
            .collect();
        let nodes = nodes.into_iter().map(|node| (node.id, node)).collect();
        WInstance::new(nodes, arcs, Vec::new(), 0, "doc".into())
    }

    #[test]
    fn graph_value_families_match_the_lens_checker_on_every_kind_and_value()
    -> Result<(), Box<dyn std::error::Error>> {
        let schema = kinds_schema()?;
        let lens = identity_lens(&schema);
        let view = kinds_graph()?;
        let (_, complement) = panproto_lens::graph_lens::get(&lens, &view)?;
        let mut compared = 0;
        for mutation in SHAPE_ONLY {
            let ours = match graph_cases(&schema, &view, &Presentation::default(), mutation) {
                Cases::Views(views) => views,
                Cases::Negative(_) => {
                    panic!("shape-only graph family {mutation:?} became negative")
                }
                Cases::Skip(_) => Vec::new(),
            };
            assert!(
                GraphShape::negative_mutation_cases(
                    &lens,
                    &view,
                    &view,
                    &complement,
                    &(),
                    mutation,
                )
                .is_empty(),
                "shape-only graph family {mutation:?} became negative"
            );
            let theirs =
                match GraphShape::mutation_cases(&lens, &view, &view, &complement, &(), mutation) {
                    MutationCases::Cases(views) => views,
                    MutationCases::Skip(_) => Vec::new(),
                };
            assert_eq!(ours.len(), theirs.len(), "{mutation:?}");
            for (a, b) in ours.iter().zip(&theirs) {
                assert!(GInstance::identical(a, b), "{mutation:?}: {a:?} != {b:?}");
            }
            compared += ours.len();
        }
        assert!(compared > KINDS.len());
        Ok(())
    }

    #[test]
    fn tree_value_families_match_the_lens_checker_on_every_kind_and_value()
    -> Result<(), Box<dyn std::error::Error>> {
        let schema = kinds_schema()?;
        let lens = identity_lens(&schema);
        let view = kinds_tree();
        let (_, complement) = TreeShape::get(&lens, &view, &())?;
        let mut compared = 0;
        for mutation in SHAPE_ONLY {
            let ours = match tree_cases(&schema, &view, mutation) {
                Cases::Views(views) => views,
                Cases::Negative(_) => {
                    panic!("shape-only tree family {mutation:?} became negative")
                }
                Cases::Skip(_) => Vec::new(),
            };
            assert!(
                TreeShape::negative_mutation_cases(
                    &lens,
                    &view,
                    &view,
                    &complement,
                    &(),
                    mutation,
                )
                .is_empty(),
                "shape-only tree family {mutation:?} became negative"
            );
            let theirs =
                match TreeShape::mutation_cases(&lens, &view, &view, &complement, &(), mutation) {
                    MutationCases::Cases(views) => views,
                    MutationCases::Skip(_) => Vec::new(),
                };
            assert_eq!(ours.len(), theirs.len(), "{mutation:?}");
            for (a, b) in ours.iter().zip(&theirs) {
                assert!(TreeShape::identical(a, b), "{mutation:?}: {a:?} != {b:?}");
            }
            compared += ours.len();
        }
        assert!(compared > KINDS.len());
        Ok(())
    }

    fn table_schema(protocol: &Protocol) -> Result<Schema, panproto_schema::SchemaError> {
        SchemaBuilder::new(protocol)
            .vertex("customer", "table", None)?
            .vertex("order", "table", None)?
            .vertex("label", "string", None)?
            .vertex("num", "integer", None)?
            .vertex("flag", "boolean", None)?
            .vertex("amount", "number", None)?
            .vertex("blob", "bytes", None)?
            .edge("customer", "label", "prop", Some("cid"))?
            .edge("customer", "num", "prop", Some("rank"))?
            .edge("order", "label", "prop", Some("customer"))?
            .edge("order", "flag", "prop", Some("paid"))?
            .edge("order", "amount", "prop", Some("total"))?
            .edge("order", "blob", "prop", Some("raw"))?
            .edge("order", "customer", "fk", Some("region"))?
            .build()
    }

    fn table_view() -> Result<FInstance, panproto_inst::InstError> {
        let row = |cells: &[(&str, Value)]| {
            cells
                .iter()
                .map(|(name, value)| ((*name).to_owned(), value.clone()))
                .collect::<HashMap<String, Value>>()
        };
        let region = Edge {
            src: "order".into(),
            tgt: "customer".into(),
            kind: "fk".into(),
            name: Some("region".into()),
        };
        FInstance::new()
            .with_rows(
                "customer",
                [
                    (
                        RowId(0),
                        row(&[("cid", Value::Str("a".to_owned())), ("rank", Value::Int(1))]),
                    ),
                    (RowId(1), row(&[("cid", Value::Str("b".to_owned()))])),
                ],
            )?
            .with_rows(
                "order",
                [
                    (
                        RowId(2),
                        row(&[
                            ("customer", Value::Str("a".to_owned())),
                            ("paid", Value::Bool(true)),
                            ("total", Value::Float(2.5)),
                            ("raw", Value::Bytes(vec![1])),
                        ]),
                    ),
                    (RowId(3), row(&[("total", Value::Null)])),
                ],
            )?
            .with_foreign_key(region, [(RowId(2), RowId(1))])
    }

    #[test]
    fn table_families_match_the_lens_checker_under_an_identity_lens()
    -> Result<(), Box<dyn std::error::Error>> {
        use panproto_schema::{BytesText, ScalarKind};

        let protocol = Protocol {
            name: "tables".to_owned(),
            value_kinds: [
                ("string", ScalarKind::Str),
                (
                    "integer",
                    ScalarKind::Int {
                        bits: 64,
                        signed: true,
                    },
                ),
                ("boolean", ScalarKind::Bool),
                ("number", ScalarKind::Float { bits: 64 }),
                ("bytes", ScalarKind::Bytes(BytesText::Base64)),
            ]
            .into_iter()
            .map(|(kind, scalar)| (kind.to_owned(), scalar))
            .collect(),
            non_value_kinds: std::iter::once("table".to_owned()).collect(),
            ..Protocol::default()
        };
        let schema = table_schema(&protocol)?;
        let lens = identity_lens(&schema);
        let view = table_view()?;
        let context = panproto_lens::TableContext {
            src: protocol.clone(),
            tgt: protocol.clone(),
        };
        let (_, complement) = TableShape::get(&lens, &view, &context)?;
        let mut compared = 0;
        for mutation in SHAPE_ONLY {
            let ours = match table_cases(&schema, &protocol, &view, mutation) {
                Cases::Views(views) => views,
                Cases::Negative(_) => {
                    panic!("shape-only table family {mutation:?} became negative")
                }
                Cases::Skip(_) => Vec::new(),
            };
            assert!(
                TableShape::negative_mutation_cases(
                    &lens,
                    &view,
                    &view,
                    &complement,
                    &context,
                    mutation,
                )
                .is_empty(),
                "shape-only table family {mutation:?} became negative"
            );
            let theirs = match TableShape::mutation_cases(
                &lens,
                &view,
                &view,
                &complement,
                &context,
                mutation,
            ) {
                MutationCases::Cases(views) => views,
                MutationCases::Skip(_) => Vec::new(),
            };
            assert_eq!(ours.len(), theirs.len(), "{mutation:?}");
            for (a, b) in ours.iter().zip(&theirs) {
                assert!(TableShape::identical(a, b), "{mutation:?}: {a:?} != {b:?}");
            }
            compared += ours.len();
        }
        assert!(compared > 0);
        Ok(())
    }
}
