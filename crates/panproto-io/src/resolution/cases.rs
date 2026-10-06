//! The single-step basis of edited views the law checker visits.
//!
//! For every [`Mutation`] family this generates, deterministically, the
//! views one edit away from a resolution's view, under the semantic schema.
//! The shape-only families are the lens checker's own families under an
//! identity lens; the families that depend on the lens read the
//! resolution's complement instead: its span nodes and occurrence edges
//! are the synthesized carriers, the node ids held in its redirected
//! fragments are the reserved ids, and a resolution contracts nothing.

use std::collections::{BTreeSet, HashMap};

use panproto_inst::resolution_complement::ResolutionComplement;
use panproto_inst::{
    EdgeId, FInstance, FieldPresence, GEdge, GInstance, HYPER_PARENT_KIND, Node, NodeShape, RowId,
    Value, WInstance,
};
use panproto_lens::{Mutation, SkipReason};
use panproto_schema::{Edge, Protocol, Schema};

/// The candidates of one family, or why it has none.
#[derive(Clone, Debug)]
pub enum Cases<I> {
    /// Candidate views, in deterministic order.
    Views(Vec<I>),
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

/// The synthesized carriers and reserved ids a complement names.
#[derive(Debug, Default)]
pub struct Presentation {
    /// Synthesized nodes: span nodes.
    pub nodes: BTreeSet<u32>,
    /// Synthesized edges: occurrence edges.
    pub edges: BTreeSet<EdgeId>,
    /// Node ids reserved by redirected fragments, with their anchors.
    pub reserved: Vec<(u32, panproto_gat::Name)>,
    /// Every edge id the complement names, in or out of the view.
    pub edge_ids: BTreeSet<EdgeId>,
}

impl Presentation {
    /// The presentation of a resolution complement.
    #[must_use]
    pub fn of(complement: &ResolutionComplement) -> Self {
        let index = complement.synth_index();
        let mut reserved: Vec<(u32, panproto_gat::Name)> = complement
            .redirected
            .values()
            .flat_map(|fragment| {
                fragment
                    .nodes
                    .iter()
                    .map(|(id, node)| (*id, node.anchor.clone()))
            })
            .collect();
        reserved.sort();
        let edge_ids = complement
            .occurrences
            .keys()
            .chain(complement.spanning.values())
            .filter_map(|carrier| match carrier {
                panproto_inst::ViewRef::Edge(id) => Some(*id),
                _ => None,
            })
            .collect();
        Self {
            nodes: index.nodes.keys().copied().collect(),
            edges: index.edges.keys().copied().collect(),
            reserved,
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
        Mutation::RemoveSynthesized => graph_synthesized(view, synth, true),
        Mutation::EditSynthesized => graph_synthesized(view, synth, false),
        Mutation::AddContractedEdge => Vec::new(),
        Mutation::ReservedIdReuse => graph_reserved(view, synth),
        Mutation::MalformedFan => graph_malformed_fan(view, synth),
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

fn graph_synthesized(view: &GInstance, synth: &Presentation, remove: bool) -> Vec<GInstance> {
    let mut views = Vec::new();
    for &id in &synth.nodes {
        let mut next = view.clone();
        if remove {
            let incident: Vec<_> = next
                .out_edges(id)
                .iter()
                .chain(next.in_edges(id))
                .copied()
                .collect();
            for edge in incident {
                let _ = next.remove_edge(edge);
            }
            if next.remove_node(id).is_ok() {
                views.push(next);
            }
        } else if let Some(node) = next.node_mut(id) {
            node.value = Some(FieldPresence::Present(Value::Int(1)));
            views.push(next);
        }
    }
    for &id in &synth.edges {
        let mut next = view.clone();
        if remove && next.remove_edge(id).is_ok() {
            views.push(next);
        } else if !remove && next.edge(id).is_some() {
            let _ = next.set_edge_position(id, Some(1));
            views.push(next);
        }
    }
    views
}

fn graph_reserved(view: &GInstance, synth: &Presentation) -> Vec<GInstance> {
    let mut views = Vec::new();
    for (id, anchor) in &synth.reserved {
        if view.node(*id).is_none() {
            let mut next = view.clone();
            if next.insert_node(Node::new(*id, anchor.clone())).is_ok() {
                views.push(next);
            }
        }
    }
    views
}

fn graph_malformed_fan(view: &GInstance, synth: &Presentation) -> Vec<GInstance> {
    let Some(fresh) =
        fresh_u32(view.edges().keys().chain(&synth.edge_ids).map(|id| id.0)).map(EdgeId)
    else {
        return Vec::new();
    };
    let mut views = Vec::new();
    for id in sorted_nodes(view) {
        let Some(node) = view.node(id) else {
            continue;
        };
        if matches!(node.shape, NodeShape::HyperEdge) {
            let mut parents: Vec<_> = view
                .out_edges(id)
                .iter()
                .copied()
                .filter(|edge| {
                    view.edge(*edge)
                        .is_some_and(|edge| edge.edge.kind.as_ref() == HYPER_PARENT_KIND)
                })
                .collect();
            parents.sort_unstable();
            for edge in parents {
                let mut next = view.clone();
                let Some(duplicate) = view.edge(edge).cloned() else {
                    continue;
                };
                if next.insert_edge(fresh, duplicate).is_ok() {
                    views.push(next);
                }
            }
        }
    }
    views
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
    use panproto_inst::{EdgeId, FieldPresence, GEdge, GInstance, Node, Value};
    use panproto_lens::{GraphShape, LawShape, Lens, Mutation, MutationCases};
    use panproto_schema::{Edge, Protocol, Schema, SchemaBuilder};

    use super::{Cases, Presentation, graph_cases};

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
                Cases::Skip(_) => Vec::new(),
            };
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
}
