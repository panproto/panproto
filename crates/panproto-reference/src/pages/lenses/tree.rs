//! Tree-lens conversion, restoration, and carrier allocation.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use super::domain::{
    PutDomainError, ViewRef, key_part_present, part_column, required_value_violations,
    key_uniqueness_violations, structural_required_cell_violations, uniqueness_role,
};
use super::graph::{GraphLens, LensError};
use crate::fan::{candidates as fan_candidates, violations_at};
use crate::model::{Fan, GEdge, Graph, Name, Node, NodeShape, SEdge, Tree};
use crate::pages::representation::canonical::ComplementShape;
use crate::restrict::{
    Complement, EdgeFate, EngineError, ErrorKey, HYPER_LABEL, HYPER_PARENT, PairResolution,
    PutFieldRefusal, RestrictError, SynthKey, floor_of, is_fan_kind, reverse_anchor,
};
use crate::schema::KeyRole;

/// A failure of the tree carrier machinery rather than a violation of the
/// mathematical put domain.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TreeOperationalError {
    /// The restored graph carrier has no tree form.
    RestoredNotATree {
        /// The first structural obstruction.
        reason: crate::encode::NotATree,
    },
    /// No non-reserved id remains for a synthesized tree-view carrier.
    IdsExhausted {
        /// Whether node ids, rather than edge ids, ran out.
        nodes: bool,
    },
}

enum TreeRestoreError {
    Domain(Vec<PutDomainError>),
    Operational(TreeOperationalError),
}

#[derive(Clone, Copy)]
enum NativeDomainProjection {
    Graph,
    Tree,
}

#[derive(Clone)]
struct NativeDomainError {
    graph: PutDomainError,
    tree: PutDomainError,
}

impl NativeDomainError {
    fn same(error: PutDomainError) -> Self {
        Self {
            graph: error.clone(),
            tree: error,
        }
    }

    const fn coordinates(graph: PutDomainError, tree: PutDomainError) -> Self {
        Self { graph, tree }
    }
}

fn native_domain_report(
    errors: Vec<NativeDomainError>,
    projection: NativeDomainProjection,
) -> Vec<PutDomainError> {
    let mut errors: Vec<_> = errors
        .into_iter()
        .map(|error| match projection {
            NativeDomainProjection::Graph => error.graph,
            NativeDomainProjection::Tree => error.tree,
        })
        .collect();
    PutDomainError::sort_report(&mut errors);
    errors
}

#[derive(Clone)]
enum NativeEndpoint {
    Source(u32),
    Synth(SynthKey),
}

#[derive(Clone)]
enum NativePending {
    Node(Name),
    Edge(Box<NativePendingEdge>),
}

#[derive(Clone)]
struct NativePendingEdge {
    src: NativeEndpoint,
    tgt: NativeEndpoint,
    label: SEdge,
}

const fn native_at(stage: u8, id: u32, id2: u32, error: EngineError) -> RestrictError {
    RestrictError::At {
        key: ErrorKey { stage, id, id2 },
        error,
    }
}

fn native_unresolved(
    src: &str,
    tgt: &str,
    reason: &crate::restrict::UnresolvedReason,
) -> EngineError {
    match reason {
        crate::restrict::UnresolvedReason::NoEdge => EngineError::NoEdgeFound {
            src: src.to_owned(),
            tgt: tgt.to_owned(),
        },
        crate::restrict::UnresolvedReason::Ambiguous { count } => EngineError::AmbiguousEdge {
            src: src.to_owned(),
            tgt: tgt.to_owned(),
            count: *count,
        },
    }
}

fn native_path_objects(
    src: u32,
    tgt: u32,
    path_src: &str,
    path_tgt: &str,
    intermediates: &[Name],
    labels: &[SEdge],
    key: &dyn Fn(u16) -> SynthKey,
) -> Result<Vec<(SynthKey, NativePending)>, EngineError> {
    let hops = labels.len().max(intermediates.len().saturating_add(1));
    if hops > (1usize << 16) {
        return Err(EngineError::ExpansionTooLong {
            src: path_src.to_owned(),
            tgt: path_tgt.to_owned(),
            hops,
        });
    }
    let mut objects = Vec::new();
    for (index, anchor) in intermediates.iter().enumerate() {
        let hop = u16::try_from(index).map_err(|_| EngineError::ExpansionTooLong {
            src: path_src.to_owned(),
            tgt: path_tgt.to_owned(),
            hops,
        })?;
        objects.push((key(hop), NativePending::Node(anchor.clone())));
    }
    for (index, label) in labels.iter().enumerate() {
        let hop = u16::try_from(index).map_err(|_| EngineError::ExpansionTooLong {
            src: path_src.to_owned(),
            tgt: path_tgt.to_owned(),
            hops,
        })?;
        let from = if index == 0 {
            NativeEndpoint::Source(src)
        } else {
            NativeEndpoint::Synth(key(hop - 1))
        };
        let to = if index == intermediates.len() {
            NativeEndpoint::Source(tgt)
        } else {
            NativeEndpoint::Synth(key(hop))
        };
        objects.push((
            key(hop),
            NativePending::Edge(Box::new(NativePendingEdge {
                src: from,
                tgt: to,
                label: label.clone(),
            })),
        ));
    }
    Ok(objects)
}

fn native_take(counter: &mut u32) -> Option<u32> {
    let id = *counter;
    *counter = id.checked_add(1)?;
    Some(id)
}

/// Independently materialize only the inert graph carrier of a tree. This
/// intentionally does not call either representation bridge used by the
/// graph leg of an F.B square.
fn native_source_carrier(
    tree: &Tree,
    schema: &crate::schema::Schema,
) -> Option<(Graph, crate::encode::TreeEmbedding)> {
    if tree
        .nodes
        .values()
        .any(|node| node.shape == NodeShape::HyperEdge)
        || tree.arcs.iter().any(|(_, _, edge)| is_fan_kind(&edge.kind))
    {
        return None;
    }
    let mut graph = Graph {
        nodes: tree.nodes.clone(),
        edges: BTreeMap::new(),
    };
    let mut embedding = crate::encode::TreeEmbedding::default();
    let mut next_edge = 0;
    for (parent, child, label) in &tree.arcs {
        if !tree.nodes.contains_key(parent)
            || !tree.nodes.contains_key(child)
            || embedding.arcs.contains_key(child)
        {
            return None;
        }
        let edge = native_take(&mut next_edge)?;
        graph.edges.insert(
            edge,
            GEdge {
                src: *parent,
                tgt: *child,
                label: label.clone(),
                position: None,
            },
        );
        embedding.arcs.insert(*child, edge);
    }
    let mut next_node = floor_of(tree.nodes.keys().copied());
    for fan in &tree.fans {
        let fan_node = native_take(&mut next_node)?;
        if fan_node == u32::MAX {
            return None;
        }
        let mut node = Node::new(fan_node, &fan.hyper_edge);
        node.shape = NodeShape::HyperEdge;
        graph.nodes.insert(fan_node, node);
        let parent_edge = native_take(&mut next_edge)?;
        if parent_edge == u32::MAX {
            return None;
        }
        let parent_anchor = tree.nodes.get(&fan.parent)?.anchor.clone();
        let parent_name = schema
            .hyper_edges
            .get(&fan.hyper_edge)
            .map(|decl| decl.parent_label.clone());
        graph.edges.insert(
            parent_edge,
            fan_edge(
                fan_node,
                &fan.hyper_edge,
                HYPER_PARENT,
                parent_name,
                (fan.parent, parent_anchor),
            ),
        );
        let mut children = BTreeMap::new();
        for (label, child) in &fan.children {
            let edge = native_take(&mut next_edge)?;
            if edge == u32::MAX {
                return None;
            }
            let child_anchor = tree.nodes.get(child)?.anchor.clone();
            graph.edges.insert(
                edge,
                fan_edge(
                    fan_node,
                    &fan.hyper_edge,
                    HYPER_LABEL,
                    Some(label.clone()),
                    (*child, child_anchor),
                ),
            );
            children.insert(label.clone(), edge);
        }
        embedding.fans.push((fan_node, parent_edge, children));
    }
    embedding.next_node = next_node;
    embedding.next_edge = next_edge;
    Some((graph, embedding))
}

fn native_tree_structure_is_valid(tree: &Tree) -> bool {
    if !tree.nodes.contains_key(&tree.root)
        || tree
            .nodes
            .values()
            .any(|node| node.shape == NodeShape::HyperEdge)
        || tree.arcs.iter().any(|(_, _, edge)| is_fan_kind(&edge.kind))
    {
        return false;
    }
    let mut parent = BTreeMap::new();
    for (from, to, _) in &tree.arcs {
        if !tree.nodes.contains_key(from)
            || !tree.nodes.contains_key(to)
            || *to == tree.root
            || parent.insert(*to, *from).is_some()
        {
            return false;
        }
    }
    for node in tree.nodes.keys() {
        let mut current = *node;
        let mut steps = 0usize;
        while current != tree.root {
            let Some(next) = parent.get(&current) else {
                return false;
            };
            if steps >= tree.nodes.len() {
                return false;
            }
            current = *next;
            steps += 1;
        }
    }
    tree.fans.iter().all(|fan| {
        tree.nodes.contains_key(&fan.parent)
            && fan
                .children
                .values()
                .all(|child| tree.nodes.contains_key(child))
    })
}

fn native_fan_resolution(
    lens: &GraphLens<'_>,
    source: &str,
    surviving: &BTreeSet<Name>,
    full: &BTreeSet<Name>,
) -> (Name, BTreeMap<Name, Name>) {
    let surviving_key = (source.to_owned(), surviving.clone());
    if let Some(resolution) = lens.m.hyper_resolver.get(&surviving_key) {
        return resolution.clone();
    }
    let full_key = (source.to_owned(), full.clone());
    if full != surviving
        && let Some(resolution) = lens.m.hyper_resolver.get(&full_key)
    {
        return resolution.clone();
    }
    let mut entries = lens
        .m
        .hyper_resolver
        .iter()
        .filter(|((hyper, _), _)| hyper == source)
        .map(|(_, resolution)| resolution);
    let sole = entries.next().filter(|_| entries.next().is_none());
    sole.cloned()
        .unwrap_or_else(|| (source.to_owned(), BTreeMap::new()))
}

fn native_map_back_label(
    lens: &GraphLens<'_>,
    view: &Graph,
    edge: &GEdge,
    source_anchor: &dyn Fn(u32) -> Option<Name>,
) -> Result<SEdge, usize> {
    if is_fan_kind(&edge.label.kind) {
        let (Some(from), Some(to)) = (source_anchor(edge.src), source_anchor(edge.tgt)) else {
            return Err(0);
        };
        let hyper = lens.src.hyper_edges.get(&from).ok_or(0usize)?;
        let name = if edge.label.kind == HYPER_PARENT {
            edge.label.name.as_ref().map(|_| hyper.parent_label.clone())
        } else {
            edge.label.name.clone()
        };
        let valid = name.as_ref().map_or_else(
            || edge.label.kind == HYPER_PARENT,
            |label| {
                (edge.label.kind == HYPER_PARENT && *label == hyper.parent_label)
                    || hyper.signature.contains_key(label)
            },
        );
        return valid
            .then_some(SEdge {
                src: from,
                tgt: to,
                kind: edge.label.kind.clone(),
                name,
            })
            .ok_or(0);
    }
    let (Some(from), Some(to)) = (view.nodes.get(&edge.src), view.nodes.get(&edge.tgt)) else {
        return Err(0);
    };
    let candidates: Vec<_> = lens
        .plan
        .fates
        .iter()
        .filter(|(source, fate)| {
            matches!(fate, EdgeFate::Keep(image) if *image == edge.label)
                && lens.m.vertex_image(&source.src) == from.anchor
                && lens.m.vertex_image(&source.tgt) == to.anchor
        })
        .map(|(source, _)| source)
        .collect();
    match candidates.as_slice() {
        [source] => Ok((*source).clone()),
        other => Err(other.len()),
    }
}

type NativeTreeGet =
    Option<Result<(Tree, Complement, crate::encode::TreeEmbedding), RestrictError>>;

struct NativeGetState<'a> {
    source: &'a Tree,
    source_graph: Graph,
    source_embedding: crate::encode::TreeEmbedding,
    complement: Complement,
    view_graph: Graph,
    pending: Vec<(SynthKey, NativePending)>,
    view_fans: Vec<Fan>,
    view_fan_embedding: Vec<(u32, u32, BTreeMap<Name, u32>)>,
}

fn native_get_nodes(
    lens: &GraphLens<'_>,
    state: &mut NativeGetState<'_>,
) -> Result<(), RestrictError> {
    for (id, node) in &state.source.nodes {
        let mut survives = lens.m.survives(&node.anchor);
        if survives && lens.m.conditional.contains(&node.anchor) {
            survives = lens
                .vl
                .survives(&state.source_graph, node)
                .map_err(|error| native_at(0, *id, 0, EngineError::Predicate(error)))?;
        }
        if !survives {
            state.complement.dropped_nodes.insert(*id, node.clone());
            continue;
        }
        let mut view_node = node.clone();
        lens.m
            .vertex_image(&node.anchor)
            .clone_into(&mut view_node.anchor);
        if lens.m.transformed.contains(&node.anchor) {
            let (fields, value) = lens
                .vl
                .transform(&state.source_graph, node)
                .map_err(|error| native_at(0, *id, 0, EngineError::Layer(error)))?;
            view_node.fields = fields;
            view_node.value = value;
        }
        state.view_graph.nodes.insert(*id, view_node);
    }
    Ok(())
}

fn native_get_arcs(
    lens: &GraphLens<'_>,
    state: &mut NativeGetState<'_>,
) -> Result<(), RestrictError> {
    for (parent, child, label) in &state.source.arcs {
        let edge_id = state.source_embedding.arcs[child];
        let source_edge = state.source_graph.edges[&edge_id].clone();
        if !(state.view_graph.nodes.contains_key(parent)
            && state.view_graph.nodes.contains_key(child))
        {
            state.complement.dropped_edges.insert(edge_id, source_edge);
            continue;
        }
        let Some(fate) = lens.plan.fates.get(label) else {
            return Err(native_at(
                1,
                edge_id,
                0,
                EngineError::IncoherentEdgeImage {
                    label: Box::new(label.clone()),
                },
            ));
        };
        match fate {
            EdgeFate::Keep(image) => {
                state.view_graph.edges.insert(
                    edge_id,
                    GEdge {
                        label: image.clone(),
                        ..source_edge
                    },
                );
            }
            EdgeFate::Drop => {
                state.complement.dropped_edges.insert(edge_id, source_edge);
            }
            EdgeFate::Expand {
                intermediates,
                labels,
            } => {
                state.pending.extend(
                    native_path_objects(
                        *parent,
                        *child,
                        lens.m.vertex_image(&label.src),
                        lens.m.vertex_image(&label.tgt),
                        intermediates,
                        labels,
                        &|hop| SynthKey::Expand {
                            source_edge: edge_id,
                            hop,
                        },
                    )
                    .map_err(|error| native_at(1, edge_id, 0, error))?,
                );
                state.complement.dropped_edges.insert(edge_id, source_edge);
            }
            EdgeFate::Unresolved(from, to, reason) => {
                return Err(native_at(
                    1,
                    edge_id,
                    0,
                    native_unresolved(from, to, reason),
                ));
            }
        }
    }
    Ok(())
}

fn native_contracted_pairs(state: &NativeGetState<'_>) -> BTreeSet<(u32, u32)> {
    let dropped = |node: u32| {
        state.source.nodes.contains_key(&node) && !state.view_graph.nodes.contains_key(&node)
    };
    let children = |node: u32| {
        state
            .source
            .arcs
            .iter()
            .filter(move |(parent, _, _)| *parent == node)
            .map(|(_, child, _)| *child)
    };
    let mut realized = BTreeSet::new();
    for start in state.view_graph.nodes.keys().copied() {
        let mut seen = BTreeSet::new();
        let mut queue: VecDeque<_> = children(start).filter(|child| dropped(*child)).collect();
        while let Some(node) = queue.pop_front() {
            if !seen.insert(node) {
                continue;
            }
            for child in children(node) {
                if dropped(child) {
                    queue.push_back(child);
                } else if state.view_graph.nodes.contains_key(&child) {
                    realized.insert((start, child));
                }
            }
        }
    }
    realized
}

fn native_get_contractions(
    lens: &GraphLens<'_>,
    state: &mut NativeGetState<'_>,
) -> Result<(), RestrictError> {
    for (from_node, to_node) in native_contracted_pairs(state) {
        let source_pair = (
            state.source.nodes[&from_node].anchor.clone(),
            state.source.nodes[&to_node].anchor.clone(),
        );
        let Some(pair) = lens.plan.pairs.get(&source_pair) else {
            return Err(native_at(
                2,
                from_node,
                to_node,
                EngineError::NoEdgeFound {
                    src: lens.m.vertex_image(&source_pair.0).to_owned(),
                    tgt: lens.m.vertex_image(&source_pair.1).to_owned(),
                },
            ));
        };
        match &pair.resolution {
            PairResolution::Label(label) => state.pending.push((
                SynthKey::Contract {
                    src: from_node,
                    tgt: to_node,
                    edge: label.clone(),
                },
                NativePending::Edge(Box::new(NativePendingEdge {
                    src: NativeEndpoint::Source(from_node),
                    tgt: NativeEndpoint::Source(to_node),
                    label: label.clone(),
                })),
            )),
            PairResolution::Expand {
                intermediates,
                labels,
            } => state.pending.extend(
                native_path_objects(
                    from_node,
                    to_node,
                    &pair.target.0,
                    &pair.target.1,
                    intermediates,
                    labels,
                    &|hop| SynthKey::ContractExpand {
                        src: from_node,
                        tgt: to_node,
                        hop,
                    },
                )
                .map_err(|error| native_at(2, from_node, to_node, error))?,
            ),
            PairResolution::Unresolved(from, to, reason) => {
                return Err(native_at(
                    2,
                    from_node,
                    to_node,
                    native_unresolved(from, to, reason),
                ));
            }
        }
    }
    Ok(())
}

fn native_drop_fan(
    state: &mut NativeGetState<'_>,
    fan_node: u32,
    parent_edge: u32,
    child_edges: &BTreeMap<Name, u32>,
) {
    state
        .complement
        .dropped_nodes
        .insert(fan_node, state.source_graph.nodes[&fan_node].clone());
    state
        .complement
        .dropped_edges
        .insert(parent_edge, state.source_graph.edges[&parent_edge].clone());
    for edge in child_edges.values() {
        state
            .complement
            .dropped_edges
            .insert(*edge, state.source_graph.edges[edge].clone());
    }
}

fn native_get_fan(
    lens: &GraphLens<'_>,
    state: &mut NativeGetState<'_>,
    fan_index: usize,
) -> Result<(), RestrictError> {
    let fan = state.source.fans[fan_index].clone();
    let (fan_node, parent_edge, child_edges) = state.source_embedding.fans[fan_index].clone();
    let surviving_labels: BTreeSet<_> = fan
        .children
        .iter()
        .filter(|(_, child)| state.view_graph.nodes.contains_key(child))
        .map(|(label, _)| label.clone())
        .collect();
    let full_labels = fan.children.keys().cloned().collect();
    let (hyper, label_map) =
        native_fan_resolution(lens, &fan.hyper_edge, &surviving_labels, &full_labels);
    let target = (state.view_graph.nodes.contains_key(&fan.parent) && !surviving_labels.is_empty())
        .then(|| lens.tgt.hyper_edges.get(&hyper))
        .flatten();
    let Some(target_hyper) = target else {
        native_drop_fan(state, fan_node, parent_edge, &child_edges);
        return Ok(());
    };
    let mut fan_node_value = state.source_graph.nodes[&fan_node].clone();
    fan_node_value.anchor.clone_from(&hyper);
    state.view_graph.nodes.insert(fan_node, fan_node_value);
    let parent_source = &state.source_graph.edges[&parent_edge];
    let parent_label = SEdge {
        src: hyper.clone(),
        tgt: state.view_graph.nodes[&fan.parent].anchor.clone(),
        kind: HYPER_PARENT.to_owned(),
        name: parent_source
            .label
            .name
            .as_ref()
            .map(|_| target_hyper.parent_label.clone()),
    };
    state.view_graph.edges.insert(
        parent_edge,
        GEdge {
            label: parent_label,
            ..parent_source.clone()
        },
    );
    let mut resolved_children = BTreeMap::new();
    let mut resolved_edges = BTreeMap::new();
    for (label, child) in &fan.children {
        let edge_id = child_edges[label];
        let source_edge = &state.source_graph.edges[&edge_id];
        if !state.view_graph.nodes.contains_key(child) {
            state
                .complement
                .dropped_edges
                .insert(edge_id, source_edge.clone());
            continue;
        }
        let resolved = label_map
            .get(label)
            .cloned()
            .unwrap_or_else(|| label.clone());
        if resolved_children.contains_key(&resolved) {
            return Err(native_at(
                3,
                fan_node,
                0,
                EngineError::FanReconstructionFailed {
                    hyper_edge: hyper,
                    label: resolved,
                },
            ));
        }
        state.view_graph.edges.insert(
            edge_id,
            GEdge {
                label: SEdge {
                    src: hyper.clone(),
                    tgt: state.view_graph.nodes[child].anchor.clone(),
                    kind: HYPER_LABEL.to_owned(),
                    name: Some(resolved.clone()),
                },
                ..source_edge.clone()
            },
        );
        resolved_children.insert(resolved.clone(), *child);
        resolved_edges.insert(resolved, edge_id);
    }
    state.view_fans.push(Fan {
        parent: fan.parent,
        hyper_edge: hyper,
        children: resolved_children,
    });
    state
        .view_fan_embedding
        .push((fan_node, parent_edge, resolved_edges));
    Ok(())
}

fn native_get_fans(
    lens: &GraphLens<'_>,
    state: &mut NativeGetState<'_>,
) -> Result<(), RestrictError> {
    for index in 0..state.source.fans.len() {
        native_get_fan(lens, state, index)?;
    }
    Ok(())
}

fn native_get_synthesized(state: &mut NativeGetState<'_>) -> Result<(), RestrictError> {
    let mut node_objects = Vec::new();
    let mut edge_objects = Vec::new();
    for (key, object) in std::mem::take(&mut state.pending) {
        match object {
            NativePending::Node(anchor) => node_objects.push((key, anchor)),
            NativePending::Edge(edge) => {
                edge_objects.push((key, edge.src, edge.tgt, edge.label));
            }
        }
    }
    node_objects.sort_by(|left, right| left.0.cmp(&right.0));
    edge_objects.sort_by(|left, right| left.0.cmp(&right.0));
    let mut synthesized_ids = BTreeMap::new();
    let mut fresh_nodes = state.complement.floor.0..u32::MAX;
    for (key, anchor) in node_objects {
        let Some(id) = fresh_nodes.next() else {
            return Err(native_at(
                5,
                0,
                0,
                EngineError::IdsExhausted { nodes: true },
            ));
        };
        state.view_graph.nodes.insert(id, Node::new(id, &anchor));
        state.complement.synthesized_nodes.insert(id, key.clone());
        synthesized_ids.insert(key, id);
    }
    let mut fresh_edges = state.complement.floor.1..u32::MAX;
    for (key, from, to, label) in edge_objects {
        let Some(id) = fresh_edges.next() else {
            return Err(native_at(
                5,
                1,
                0,
                EngineError::IdsExhausted { nodes: false },
            ));
        };
        let resolve = |endpoint: &NativeEndpoint| match endpoint {
            NativeEndpoint::Source(node) => Some(*node),
            NativeEndpoint::Synth(key) => synthesized_ids.get(key).copied(),
        };
        let (Some(from), Some(to)) = (resolve(&from), resolve(&to)) else {
            return Err(native_at(
                5,
                0,
                0,
                EngineError::IdsExhausted { nodes: true },
            ));
        };
        state.view_graph.edges.insert(
            id,
            GEdge {
                src: from,
                tgt: to,
                label,
                position: None,
            },
        );
        state.complement.synthesized_edges.insert(id, key);
    }
    Ok(())
}

fn native_record_originals(lens: &GraphLens<'_>, state: &mut NativeGetState<'_>) {
    for (id, node) in &state.view_graph.nodes {
        if state.complement.synthesized_nodes.contains_key(id) {
            continue;
        }
        let source_node = &state.source_graph.nodes[id];
        if reverse_anchor(lens.src, lens.m, &node.anchor) != vec![source_node.anchor.clone()] {
            state
                .complement
                .original_anchors
                .insert(*id, source_node.anchor.clone());
        }
        if node.fields != source_node.fields {
            state
                .complement
                .original_fields
                .insert(*id, source_node.fields.clone());
        }
        if node.value != source_node.value {
            state
                .complement
                .original_values
                .insert(*id, source_node.value.clone());
        }
    }
    let source_anchor = |node: u32| {
        state
            .source_graph
            .nodes
            .get(&node)
            .map(|node| node.anchor.clone())
    };
    for (id, edge) in &state.view_graph.edges {
        if state.complement.synthesized_edges.contains_key(id) {
            continue;
        }
        let source_edge = &state.source_graph.edges[id];
        if native_map_back_label(lens, &state.view_graph, edge, &source_anchor).as_ref()
            != Ok(&source_edge.label)
        {
            state
                .complement
                .original_edges
                .insert(*id, source_edge.clone());
        }
    }
}

fn native_finish_get(
    lens: &GraphLens<'_>,
    mut state: NativeGetState<'_>,
) -> Result<(Tree, Complement, crate::encode::TreeEmbedding), RestrictError> {
    if !state.view_graph.nodes.contains_key(&state.source.root) {
        return Err(RestrictError::RootPruned {
            root: state.source.root,
        });
    }
    state.complement.identity = Some(lens.identity_for_shape(ComplementShape::WType));
    let arcs: BTreeMap<_, _> = state
        .view_graph
        .edges
        .iter()
        .filter(|(_, edge)| !is_fan_kind(&edge.label.kind))
        .map(|(id, edge)| (edge.tgt, *id))
        .collect();
    let embedding = crate::encode::TreeEmbedding {
        next_node: floor_of(
            state
                .view_graph
                .nodes
                .keys()
                .copied()
                .chain(state.complement.dropped_nodes.keys().copied())
                .chain(state.complement.synthesized_nodes.keys().copied()),
        )
        .max(state.complement.floor.0),
        next_edge: floor_of(
            state
                .view_graph
                .edges
                .keys()
                .copied()
                .chain(state.complement.dropped_edges.keys().copied())
                .chain(state.complement.synthesized_edges.keys().copied()),
        )
        .max(state.complement.floor.1),
        arcs,
        fans: state.view_fan_embedding,
    };
    let tree = Tree {
        nodes: state
            .view_graph
            .nodes
            .iter()
            .filter(|(_, node)| node.shape != NodeShape::HyperEdge)
            .map(|(id, node)| (*id, node.clone()))
            .collect(),
        arcs: state
            .view_graph
            .edges
            .values()
            .filter(|edge| !is_fan_kind(&edge.label.kind))
            .map(|edge| (edge.src, edge.tgt, edge.label.clone()))
            .collect(),
        fans: state.view_fans,
        root: state.source.root,
    };
    Ok((tree, state.complement, embedding))
}

fn native_tree_get(lens: &GraphLens<'_>, source: &Tree) -> NativeTreeGet {
    if !native_tree_structure_is_valid(source) {
        return None;
    }
    if let Err(error) = lens.plan.validate_for(lens.src, lens.tgt, lens.m) {
        return Some(Err(RestrictError::Plan(error)));
    }
    let (source_graph, source_embedding) = native_source_carrier(source, lens.src)?;
    let complement = Complement {
        floor: (
            floor_of(source_graph.nodes.keys().copied()),
            floor_of(source_graph.edges.keys().copied()),
        ),
        ..Complement::default()
    };
    let mut state = NativeGetState {
        source,
        source_graph,
        source_embedding,
        complement,
        view_graph: Graph::default(),
        pending: Vec::new(),
        view_fans: Vec::new(),
        view_fan_embedding: Vec::new(),
    };
    if let Err(error) = native_get_nodes(lens, &mut state) {
        return Some(Err(error));
    }
    if let Err(error) = native_get_arcs(lens, &mut state) {
        return Some(Err(error));
    }

    if let Err(error) = native_get_contractions(lens, &mut state) {
        return Some(Err(error));
    }

    if let Err(error) = native_get_fans(lens, &mut state) {
        return Some(Err(error));
    }

    if let Err(error) = native_get_synthesized(&mut state) {
        return Some(Err(error));
    }
    native_record_originals(lens, &mut state);
    Some(native_finish_get(lens, state))
}

/// Definition: The tree lens on the domain where the tree and graph engines agree.
///
/// The lens keeps the tree shape (no source edge between surviving vertices is dropped) and the
/// root survives. Tree get is the graph get of the tree's graph form, read back as a tree with the
/// same root; it also returns the view's embedding, which put takes to name the view's edges.
///
/// `None` outside that domain (or when the tree has no graph form).
///
/// Book anchors: `L-lenses-11`.
#[must_use]
pub fn tree_get(
    lens: &GraphLens<'_>,
    t: &crate::model::Tree,
) -> Option<
    Result<
        (crate::model::Tree, Complement, crate::encode::TreeEmbedding),
        crate::restrict::RestrictError,
    >,
> {
    let keeps_shape =
        !lens.plan.fates.iter().any(|(e, f)| {
            *f == EdgeFate::Drop && lens.m.survives(&e.src) && lens.m.survives(&e.tgt)
        });
    if !keeps_shape {
        return None;
    }
    native_tree_get(lens, t)
}

enum NativeExpected {
    Node(Name),
    Edge(Box<NativeExpectedEdge>),
}

struct NativeExpectedEdge {
    label: SEdge,
    from: NativeEndpoint,
    to: NativeEndpoint,
}

enum NativePathKey {
    Expand { source_edge: u32 },
    ContractExpand { src: u32, tgt: u32 },
}

impl NativePathKey {
    const fn at(&self, hop: u16) -> SynthKey {
        match self {
            Self::Expand { source_edge } => SynthKey::Expand {
                source_edge: *source_edge,
                hop,
            },
            Self::ContractExpand { src, tgt } => SynthKey::ContractExpand {
                src: *src,
                tgt: *tgt,
                hop,
            },
        }
    }
}

struct NativeExpectedPath {
    source: u32,
    target: u32,
    intermediates: Vec<Name>,
    labels: Vec<SEdge>,
    hop: u16,
    key: NativePathKey,
}

fn native_chain_ends(complement: &Complement, key: &SynthKey) -> Vec<u32> {
    match key {
        SynthKey::Contract { src, tgt, .. } | SynthKey::ContractExpand { src, tgt, .. } => {
            vec![*src, *tgt]
        }
        SynthKey::Expand { source_edge, .. } => complement
            .dropped_edges
            .get(source_edge)
            .map(|edge| vec![edge.src, edge.tgt])
            .unwrap_or_default(),
    }
}

fn native_expected_synth(
    lens: &GraphLens<'_>,
    complement: &Complement,
    key: &SynthKey,
    is_node: bool,
    anchor_of: &dyn Fn(u32) -> Option<Name>,
) -> Option<NativeExpected> {
    let path = match key {
        SynthKey::Contract { src, tgt, edge } => {
            return (!is_node).then(|| {
                NativeExpected::Edge(Box::new(NativeExpectedEdge {
                    label: edge.clone(),
                    from: NativeEndpoint::Source(*src),
                    to: NativeEndpoint::Source(*tgt),
                }))
            });
        }
        SynthKey::Expand { source_edge, hop } => {
            let edge = complement.dropped_edges.get(source_edge)?;
            let EdgeFate::Expand {
                intermediates,
                labels,
            } = lens.plan.fates.get(&edge.label)?
            else {
                return None;
            };
            NativeExpectedPath {
                source: edge.src,
                target: edge.tgt,
                intermediates: intermediates.clone(),
                labels: labels.clone(),
                hop: *hop,
                key: NativePathKey::Expand {
                    source_edge: *source_edge,
                },
            }
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
            NativeExpectedPath {
                source: *src,
                target: *tgt,
                intermediates: intermediates.clone(),
                labels: labels.clone(),
                hop: *hop,
                key: NativePathKey::ContractExpand {
                    src: *src,
                    tgt: *tgt,
                },
            }
        }
    };
    let index = usize::from(path.hop);
    if is_node {
        return path
            .intermediates
            .get(index)
            .map(|anchor| NativeExpected::Node(anchor.clone()));
    }
    let label = path.labels.get(index)?;
    let from = if index == 0 {
        NativeEndpoint::Source(path.source)
    } else {
        NativeEndpoint::Synth(path.key.at(path.hop - 1))
    };
    let to = if index == path.intermediates.len() {
        NativeEndpoint::Source(path.target)
    } else {
        NativeEndpoint::Synth(path.key.at(path.hop))
    };
    Some(NativeExpected::Edge(Box::new(NativeExpectedEdge {
        label: label.clone(),
        from,
        to,
    })))
}

fn native_field_refusal(elem: ViewRef, refusal: PutFieldRefusal) -> PutDomainError {
    match refusal {
        PutFieldRefusal::DerivedCellEdited { field } => {
            PutDomainError::DerivedCellEdited { elem, field }
        }
        PutFieldRefusal::TransformNotInvertible { field } => {
            PutDomainError::TransformNotInvertible { elem, field }
        }
    }
}

fn native_arc_error(error: &PutDomainError, parent: u32, child: u32) -> PutDomainError {
    let elem = ViewRef::Arc { parent, child };
    match error {
        PutDomainError::ContractedEdgeInserted { .. } => {
            PutDomainError::ContractedEdgeInserted { elem }
        }
        PutDomainError::UnrepresentableViewEdge { .. } => {
            PutDomainError::UnrepresentableViewEdge { elem }
        }
        _ => error.clone(),
    }
}

fn native_source_anchor(
    lens: &GraphLens<'_>,
    complement: &Complement,
    node: &Node,
) -> Result<Name, Vec<Name>> {
    if let Some(anchor) = complement.original_anchors.get(&node.id)
        && lens.m.vertex_image(anchor) == node.anchor
    {
        return Ok(anchor.clone());
    }
    let candidates = reverse_anchor(lens.src, lens.m, &node.anchor);
    match candidates.as_slice() {
        [anchor] => Ok(anchor.clone()),
        _ => Err(candidates),
    }
}

fn native_fan_shapes(
    view: &Graph,
    complement: &Complement,
    fan: u32,
) -> (BTreeSet<Name>, BTreeSet<Name>) {
    let mut surviving = BTreeSet::new();
    let mut full = BTreeSet::new();
    for (id, edge) in &view.edges {
        if edge.src != fan || edge.label.kind != HYPER_LABEL {
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
            .filter(|edge| edge.src == fan && edge.label.kind == HYPER_LABEL)
            .filter_map(|edge| edge.label.name.clone()),
    );
    (surviving, full)
}

fn native_expected_fan_label(
    lens: &GraphLens<'_>,
    complement: &Complement,
    view: &Graph,
    recorded: &GEdge,
) -> Option<SEdge> {
    let fan = complement
        .original_anchors
        .get(&recorded.src)
        .or_else(|| view.nodes.get(&recorded.src).map(|node| &node.anchor))?;
    let target = view.nodes.get(&recorded.tgt)?;
    let (surviving, full) = native_fan_shapes(view, complement, recorded.src);
    let (hyper, labels) = native_fan_resolution(lens, fan, &surviving, &full);
    let name = if recorded.label.kind == HYPER_PARENT {
        recorded.label.name.as_ref().map(|_| {
            lens.tgt.hyper_edges.get(&hyper).map_or_else(
                || recorded.label.name.clone().unwrap_or_default(),
                |decl| decl.parent_label.clone(),
            )
        })
    } else {
        recorded
            .label
            .name
            .as_ref()
            .map(|name| labels.get(name).cloned().unwrap_or_else(|| name.clone()))
    };
    Some(SEdge {
        src: hyper,
        tgt: target.anchor.clone(),
        kind: recorded.label.kind.clone(),
        name,
    })
}

fn native_recorded_edge_matches(
    lens: &GraphLens<'_>,
    complement: &Complement,
    view: &Graph,
    recorded: &GEdge,
    current: &GEdge,
) -> bool {
    if is_fan_kind(&recorded.label.kind) {
        return native_expected_fan_label(lens, complement, view, recorded).as_ref()
            == Some(&current.label);
    }
    matches!(lens.plan.fates.get(&recorded.label), Some(EdgeFate::Keep(image)) if image == &current.label)
}

fn native_edge_label(
    lens: &GraphLens<'_>,
    complement: &Complement,
    view: &Graph,
    id: u32,
    edge: &GEdge,
    source_anchor: &dyn Fn(u32) -> Option<Name>,
) -> Result<SEdge, PutDomainError> {
    if let Some(recorded) = complement.original_edges.get(&id)
        && recorded.src == edge.src
        && recorded.tgt == edge.tgt
        && native_recorded_edge_matches(lens, complement, view, recorded, edge)
    {
        return Ok(recorded.label.clone());
    }
    match native_map_back_label(lens, view, edge, source_anchor) {
        Ok(label) => Ok(label),
        Err(0) if !is_fan_kind(&edge.label.kind) => {
            let contracted = source_anchor(edge.src)
                .zip(source_anchor(edge.tgt))
                .and_then(|pair| lens.plan.pairs.get(&pair))
                .is_some_and(|pair| pair.resolution == PairResolution::Label(edge.label.clone()));
            if contracted {
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

fn native_synth_violations(
    lens: &GraphLens<'_>,
    view: &Graph,
    complement: &Complement,
    source_anchor: &dyn Fn(u32) -> Option<Name>,
    current_arcs: &BTreeMap<u32, (u32, u32)>,
) -> Vec<NativeDomainError> {
    let id_of_key = |key: &SynthKey| {
        complement
            .synthesized_nodes
            .iter()
            .find(|(_, candidate)| *candidate == key)
            .map(|(id, _)| *id)
    };
    let resolve = |endpoint: &NativeEndpoint| match endpoint {
        NativeEndpoint::Source(node) => Some(*node),
        NativeEndpoint::Synth(key) => id_of_key(key),
    };
    let explained = |key: &SynthKey| {
        native_chain_ends(complement, key)
            .iter()
            .any(|node| !view.nodes.contains_key(node))
    };
    let mut errors = Vec::new();
    for (id, key) in &complement.synthesized_nodes {
        let unchanged = view.nodes.get(id).map_or_else(
            || explained(key),
            |node| {
                matches!(
                    native_expected_synth(lens, complement, key, true, source_anchor),
                    Some(NativeExpected::Node(anchor))
                        if node.anchor == anchor
                            && node.value.is_none()
                            && node.fields.is_empty()
                            && node.discriminator.is_none()
                            && node.position.is_none()
                            && node.shape == NodeShape::Plain
                            && node.annotations.is_empty()
                )
            },
        );
        if !unchanged {
            errors.push(NativeDomainError::same(PutDomainError::SynthesizedEdited {
                elem: ViewRef::Node(*id),
            }));
        }
    }
    for (id, key) in &complement.synthesized_edges {
        let unchanged = view.edges.get(id).map_or_else(
            || explained(key),
            |edge| {
                matches!(
                    native_expected_synth(lens, complement, key, false, source_anchor),
                    Some(NativeExpected::Edge(expected))
                        if edge.label == expected.label
                            && resolve(&expected.from) == Some(edge.src)
                            && resolve(&expected.to) == Some(edge.tgt)
                            && edge.position.is_none()
                )
            },
        );
        if !unchanged {
            let graph = PutDomainError::SynthesizedEdited {
                elem: ViewRef::Edge(*id),
            };
            let tree = native_original_synth_arc(complement, key).map_or_else(
                || graph.clone(),
                |(parent, child)| PutDomainError::SynthesizedEdited {
                    elem: ViewRef::Arc { parent, child },
                },
            );
            errors.push(NativeDomainError::coordinates(graph, tree));
        }
    }
    for (id, (parent, child)) in current_arcs {
        let touches_synthesized = complement.synthesized_nodes.contains_key(parent)
            || complement.synthesized_nodes.contains_key(child);
        let recorded = complement
            .synthesized_edges
            .get(id)
            .and_then(|key| native_original_synth_arc(complement, key))
            == Some((*parent, *child));
        if touches_synthesized && !recorded {
            errors.push(NativeDomainError::coordinates(
                PutDomainError::SynthesizedEdited {
                    elem: ViewRef::Edge(*id),
                },
                PutDomainError::SynthesizedEdited {
                    elem: ViewRef::Arc {
                        parent: *parent,
                        child: *child,
                    },
                },
            ));
        }
    }
    errors
}

fn native_synth_node(complement: &Complement, key: &SynthKey) -> Option<u32> {
    complement
        .synthesized_nodes
        .iter()
        .find_map(|(node, candidate)| (candidate == key).then_some(*node))
}

fn native_original_synth_arc(complement: &Complement, key: &SynthKey) -> Option<(u32, u32)> {
    match key {
        SynthKey::Contract { src, tgt, .. } => Some((*src, *tgt)),
        SynthKey::Expand { source_edge, hop } => {
            let edge = complement.dropped_edges.get(source_edge)?;
            let parent = if *hop == 0 {
                edge.src
            } else {
                native_synth_node(
                    complement,
                    &SynthKey::Expand {
                        source_edge: *source_edge,
                        hop: *hop - 1,
                    },
                )?
            };
            let child = native_synth_node(complement, key).unwrap_or(edge.tgt);
            Some((parent, child))
        }
        SynthKey::ContractExpand { src, tgt, hop } => {
            let parent = if *hop == 0 {
                *src
            } else {
                native_synth_node(
                    complement,
                    &SynthKey::ContractExpand {
                        src: *src,
                        tgt: *tgt,
                        hop: *hop - 1,
                    },
                )?
            };
            let child = native_synth_node(complement, key).unwrap_or(*tgt);
            Some((parent, child))
        }
    }
}

fn native_synth_order_violation(
    view: &Tree,
    embedding: &crate::encode::TreeEmbedding,
    complement: &Complement,
) -> Option<NativeDomainError> {
    #[derive(Default)]
    struct SiblingOrder<'a> {
        previous_key: Option<&'a SynthKey>,
        synthesized: Vec<(u32, u32)>,
        invalid: bool,
    }

    let mut parents: BTreeMap<u32, SiblingOrder<'_>> = BTreeMap::new();
    for (parent, child, _) in &view.arcs {
        let state = parents.entry(*parent).or_default();
        let synthesized = embedding.arcs.get(child).and_then(|edge| {
            complement
                .synthesized_edges
                .get(edge)
                .map(|key| (*edge, key))
        });
        let Some((edge, key)) = synthesized else {
            if !state.synthesized.is_empty() {
                state.invalid = true;
            }
            continue;
        };
        if state.previous_key.is_some_and(|previous| previous > key) {
            state.invalid = true;
        }
        state.previous_key = Some(key);
        state.synthesized.push((edge, *child));
    }
    let candidates: Vec<_> = parents
        .into_iter()
        .filter(|(_, state)| state.invalid)
        .flat_map(|(parent, state)| {
            state
                .synthesized
                .into_iter()
                .filter_map(move |(edge, child)| {
                    let key = complement.synthesized_edges.get(&edge)?.clone();
                    Some((key, parent, child, edge))
                })
        })
        .collect();
    let graph_edge = candidates.iter().map(|candidate| candidate.3).min()?;
    candidates
        .into_iter()
        .min_by(|left, right| {
            left.0
                .cmp(&right.0)
                .then_with(|| left.1.cmp(&right.1))
                .then_with(|| left.2.cmp(&right.2))
        })
        .map(|(_, parent, child, _)| {
            NativeDomainError::coordinates(
                PutDomainError::SynthesizedEdited {
                    elem: ViewRef::Edge(graph_edge),
                },
                PutDomainError::SynthesizedEdited {
                    elem: ViewRef::Arc { parent, child },
                },
            )
        })
}

fn native_source_violations(
    lens: &GraphLens<'_>,
    view: &Graph,
    complement: &Complement,
    restored: &Graph,
) -> Vec<PutDomainError> {
    let mut errors = Vec::new();
    for (id, node) in restored.nodes.iter().filter(|(id, _)| {
        view.nodes.contains_key(id) && !complement.synthesized_nodes.contains_key(id)
    }) {
        if !lens.m.conditional.contains(&node.anchor) {
            continue;
        }
        match lens.vl.survives(restored, node) {
            Ok(true) => {}
            Ok(false) => errors.push(PutDomainError::SurvivalPredicateFails {
                elem: ViewRef::Node(*id),
            }),
            Err(refusal) => errors.push(PutDomainError::SurvivalPredicateRefused {
                elem: ViewRef::Node(*id),
                refusal,
            }),
        }
    }
    errors.extend(required_value_violations(
        lens.src,
        lens.constraint_semantics(),
        restored,
        ViewRef::Node,
    ));
    errors.extend(structural_required_cell_violations(
        lens.src,
        restored,
        ViewRef::Node,
    ));
    for (vertex, keys_for_vertex) in &lens.src.keys {
        for key in keys_for_vertex {
            if uniqueness_role(key.role) {
                errors.extend(key_uniqueness_violations(
                    restored,
                    vertex,
                    key,
                    ViewRef::Node,
                ));
            }
            if key.role != KeyRole::Primary {
                continue;
            }
            for id in restored
                .nodes
                .iter()
                .filter(|(id, node)| node.anchor == *vertex && view.nodes.contains_key(id))
                .map(|(id, _)| *id)
            {
                for part in &key.parts {
                    if !key_part_present(restored, id, part) {
                        errors.push(PutDomainError::MissingRequiredCell {
                            elem: ViewRef::Node(id),
                            field: part_column(part),
                        });
                    }
                }
            }
        }
    }
    errors.extend(fan_candidates(restored).into_iter().flat_map(|node| {
        violations_at(restored, node, Some(lens.src))
            .into_iter()
            .map(move |reason| PutDomainError::MalformedFan { node, reason })
    }));
    errors
}

fn native_is_fan(graph: &Graph, node: u32) -> bool {
    graph
        .nodes
        .get(&node)
        .is_some_and(|node| node.shape == NodeShape::HyperEdge)
}

fn native_tree_parents(
    graph: &Graph,
    root: u32,
    parent_name: &dyn Fn(&str) -> crate::encode::ParentNameRule,
) -> Result<BTreeMap<u32, u32>, crate::encode::NotATree> {
    use crate::encode::NotATree;

    let mut parents = BTreeMap::new();
    for (id, edge) in &graph.edges {
        let (Some(from), Some(to)) = (graph.nodes.get(&edge.src), graph.nodes.get(&edge.tgt))
        else {
            return Err(NotATree::DanglingEdge { edge: *id });
        };
        if edge.position.is_some() {
            return Err(NotATree::PositionedEdge { edge: *id });
        }
        if native_is_fan(graph, edge.src) {
            let name_matches = edge.label.kind != HYPER_PARENT
                || match parent_name(&from.anchor) {
                    crate::encode::ParentNameRule::Any => true,
                    crate::encode::ParentNameRule::Exact(expected) => edge.label.name == expected,
                };
            if edge.label.src != from.anchor || edge.label.tgt != to.anchor || !name_matches {
                return Err(NotATree::FanLabel { edge: *id });
            }
            continue;
        }
        if is_fan_kind(&edge.label.kind) {
            return Err(NotATree::ReservedArc { edge: *id });
        }
        if native_is_fan(graph, edge.tgt) {
            return Err(NotATree::ArcIntoFan { edge: *id });
        }
        if edge.tgt == root {
            return Err(NotATree::RootHasParent { edge: *id });
        }
        if parents.insert(edge.tgt, edge.src).is_some() {
            return Err(NotATree::SecondParent { node: edge.tgt });
        }
    }
    Ok(parents)
}

fn native_tree_fans(graph: &Graph) -> Result<Vec<Fan>, crate::encode::NotATree> {
    use crate::encode::NotATree;

    let mut fans = Vec::new();
    for (id, node) in graph
        .nodes
        .iter()
        .filter(|(id, _)| native_is_fan(graph, **id))
    {
        let bare = node.value.is_none()
            && node.discriminator.is_none()
            && node.fields.is_empty()
            && node.position.is_none()
            && node.annotations.is_empty();
        if !bare {
            return Err(NotATree::FanData { node: *id });
        }
        if let Some(violation) = violations_at(graph, *id, None).into_iter().next() {
            return Err(NotATree::Fan {
                node: *id,
                violation,
            });
        }
        let outgoing: Vec<_> = graph
            .edges
            .values()
            .filter(|edge| edge.src == *id)
            .collect();
        fans.push(Fan {
            parent: outgoing
                .iter()
                .find(|edge| edge.label.kind == HYPER_PARENT)
                .map_or(*id, |edge| edge.tgt),
            hyper_edge: node.anchor.clone(),
            children: outgoing
                .iter()
                .filter(|edge| edge.label.kind == HYPER_LABEL)
                .filter_map(|edge| edge.label.name.clone().map(|label| (label, edge.tgt)))
                .collect(),
        });
    }
    Ok(fans)
}

fn native_tree_reachable(
    nodes: &BTreeMap<u32, Node>,
    parents: &BTreeMap<u32, u32>,
    root: u32,
) -> Result<(), crate::encode::NotATree> {
    for node in nodes.keys() {
        let mut current = *node;
        let mut steps = 0usize;
        while current != root {
            match parents.get(&current) {
                Some(parent) if steps < nodes.len() => {
                    current = *parent;
                    steps += 1;
                }
                _ => return Err(crate::encode::NotATree::Unreached { node: *node }),
            }
        }
    }
    Ok(())
}

fn native_tree_form(
    graph: &Graph,
    root: u32,
    parent_name: &dyn Fn(&str) -> crate::encode::ParentNameRule,
) -> Result<Tree, crate::encode::NotATree> {
    use crate::encode::NotATree;

    if !graph.nodes.contains_key(&root) || native_is_fan(graph, root) {
        return Err(NotATree::MissingRoot);
    }
    let parents = native_tree_parents(graph, root, parent_name)?;
    let fans = native_tree_fans(graph)?;
    let nodes: BTreeMap<_, _> = graph
        .nodes
        .iter()
        .filter(|(id, _)| !native_is_fan(graph, **id))
        .map(|(id, node)| (*id, node.clone()))
        .collect();
    native_tree_reachable(&nodes, &parents, root)?;
    let arcs = graph
        .edges
        .values()
        .filter(|edge| !native_is_fan(graph, edge.src))
        .map(|edge| (edge.src, edge.tgt, edge.label.clone()))
        .collect();
    Ok(Tree {
        nodes,
        arcs,
        fans,
        root,
    })
}

fn tree_shape_error(reason: crate::encode::NotATree) -> TreeRestoreError {
    match reason {
        crate::encode::NotATree::Fan { node, violation } => {
            TreeRestoreError::Domain(vec![PutDomainError::MalformedFan {
                node,
                reason: violation,
            }])
        }
        other => {
            TreeRestoreError::Operational(TreeOperationalError::RestoredNotATree { reason: other })
        }
    }
}

struct NativeRestoreState {
    presented: Graph,
    current_arcs: BTreeMap<u32, (u32, u32)>,
    anchors: BTreeMap<u32, Name>,
    restored: Graph,
    errors: Vec<NativeDomainError>,
}

fn native_restore_start(
    lens: &GraphLens<'_>,
    view: &Tree,
    embedding: &crate::encode::TreeEmbedding,
    complement: &Complement,
) -> Result<NativeRestoreState, TreeOperationalError> {
    let (presented, current_arcs) = native_view_graph(lens, view, embedding, complement)?;
    let node_collisions = presented
        .nodes
        .keys()
        .filter(|id| complement.dropped_nodes.contains_key(id))
        .map(|id| {
            NativeDomainError::same(PutDomainError::IdCollision {
                elem: ViewRef::Node(*id),
            })
        });
    let edge_collisions = presented
        .edges
        .keys()
        .filter(|id| complement.dropped_edges.contains_key(id))
        .map(|id| {
            let graph = PutDomainError::IdCollision {
                elem: ViewRef::Edge(*id),
            };
            let tree = current_arcs.get(id).map_or_else(
                || graph.clone(),
                |(parent, child)| PutDomainError::IdCollision {
                    elem: ViewRef::Arc {
                        parent: *parent,
                        child: *child,
                    },
                },
            );
            NativeDomainError::coordinates(graph, tree)
        });
    let errors = node_collisions.chain(edge_collisions).collect();
    let mut state = NativeRestoreState {
        presented,
        current_arcs,
        anchors: BTreeMap::new(),
        restored: Graph::default(),
        errors,
    };
    for (id, node) in state
        .presented
        .nodes
        .iter()
        .filter(|(id, _)| !complement.synthesized_nodes.contains_key(id))
    {
        match native_source_anchor(lens, complement, node) {
            Ok(anchor) => {
                state.anchors.insert(*id, anchor);
            }
            Err(candidates) => state.errors.push(NativeDomainError::same(
                PutDomainError::AmbiguousReverseAnchor {
                    elem: ViewRef::Node(*id),
                    candidates,
                },
            )),
        }
    }
    let anchor_of = |node: u32| {
        state.anchors.get(&node).cloned().or_else(|| {
            complement
                .dropped_nodes
                .get(&node)
                .map(|node| node.anchor.clone())
        })
    };
    if let Some(error) = native_synth_order_violation(view, embedding, complement) {
        state.errors.push(error);
    }
    state.errors.extend(native_synth_violations(
        lens,
        &state.presented,
        complement,
        &anchor_of,
        &state.current_arcs,
    ));
    Ok(state)
}

fn native_restore_visible(
    lens: &GraphLens<'_>,
    complement: &Complement,
    carrier_complement: &Complement,
    state: &mut NativeRestoreState,
) {
    let anchor_of = |node: u32| {
        state.anchors.get(&node).cloned().or_else(|| {
            complement
                .dropped_nodes
                .get(&node)
                .map(|node| node.anchor.clone())
        })
    };
    for (id, node) in state
        .presented
        .nodes
        .iter()
        .filter(|(id, _)| !carrier_complement.synthesized_nodes.contains_key(id))
    {
        match lens.vl.restore(
            node,
            complement.original_fields.get(id),
            complement.original_values.get(id),
        ) {
            Ok((fields, value)) => {
                let anchor = state
                    .anchors
                    .get(id)
                    .cloned()
                    .unwrap_or_else(|| node.anchor.clone());
                state.restored.nodes.insert(
                    *id,
                    Node {
                        anchor,
                        value,
                        fields,
                        ..node.clone()
                    },
                );
            }
            Err(refusal) => state
                .errors
                .push(NativeDomainError::same(native_field_refusal(
                    ViewRef::Node(*id),
                    refusal,
                ))),
        }
    }
    for (id, edge) in state.presented.edges.iter().filter(|(id, edge)| {
        !carrier_complement.synthesized_edges.contains_key(id)
            && !carrier_complement.synthesized_nodes.contains_key(&edge.src)
            && !carrier_complement.synthesized_nodes.contains_key(&edge.tgt)
    }) {
        match native_edge_label(lens, complement, &state.presented, *id, edge, &anchor_of) {
            Ok(label) => {
                state.restored.edges.insert(
                    *id,
                    GEdge {
                        label,
                        ..edge.clone()
                    },
                );
            }
            Err(error) => {
                let tree = state.current_arcs.get(id).map_or_else(
                    || error.clone(),
                    |(parent, child)| native_arc_error(&error, *parent, *child),
                );
                state
                    .errors
                    .push(NativeDomainError::coordinates(error, tree));
            }
        }
    }
}

fn native_synth_key_depends_on_collision(
    key: &SynthKey,
    collisions: &BTreeSet<u32>,
    incident_source_edges: &BTreeSet<u32>,
) -> bool {
    match key {
        SynthKey::Contract { src, tgt, .. } | SynthKey::ContractExpand { src, tgt, .. } => {
            collisions.contains(src) || collisions.contains(tgt)
        }
        SynthKey::Expand { source_edge, .. } => incident_source_edges.contains(source_edge),
    }
}

/// Remove only source residue whose meaning is shadowed by a visible node
/// with the same id. Structural checks have already read the original
/// complement; this ephemeral copy lets the restored-source clauses inspect
/// the visible replacement instead of the dropped source node.
fn native_collision_counterfactual(
    presented: &Graph,
    complement: &Complement,
) -> Option<(Complement, BTreeSet<u32>)> {
    let collisions: BTreeSet<_> = presented
        .nodes
        .keys()
        .filter(|id| complement.dropped_nodes.contains_key(id))
        .copied()
        .collect();
    if collisions.is_empty() {
        return None;
    }

    let incident_source_edges: BTreeSet<_> = complement
        .dropped_edges
        .iter()
        .chain(&complement.original_edges)
        .filter(|(_, edge)| collisions.contains(&edge.src) || collisions.contains(&edge.tgt))
        .map(|(id, _)| *id)
        .collect();
    let mut counterfactual = complement.clone();
    counterfactual
        .dropped_nodes
        .retain(|id, _| !collisions.contains(id));
    counterfactual
        .dropped_edges
        .retain(|_, edge| !collisions.contains(&edge.src) && !collisions.contains(&edge.tgt));
    counterfactual
        .original_edges
        .retain(|_, edge| !collisions.contains(&edge.src) && !collisions.contains(&edge.tgt));
    counterfactual
        .original_anchors
        .retain(|id, _| !collisions.contains(id));
    counterfactual
        .original_fields
        .retain(|id, _| !collisions.contains(id));
    counterfactual
        .original_values
        .retain(|id, _| !collisions.contains(id));
    counterfactual.synthesized_nodes.retain(|_, key| {
        !native_synth_key_depends_on_collision(key, &collisions, &incident_source_edges)
    });
    counterfactual.synthesized_edges.retain(|_, key| {
        !native_synth_key_depends_on_collision(key, &collisions, &incident_source_edges)
    });
    Some((counterfactual, collisions))
}

fn native_reanchor_collisions(
    lens: &GraphLens<'_>,
    complement: &Complement,
    collisions: &BTreeSet<u32>,
    state: &mut NativeRestoreState,
) {
    for id in collisions {
        state.anchors.remove(id);
        let Some(node) = state.presented.nodes.get(id) else {
            continue;
        };
        match native_source_anchor(lens, complement, node) {
            Ok(anchor) => {
                state.anchors.insert(*id, anchor);
            }
            Err(candidates) => state.errors.push(NativeDomainError::same(
                PutDomainError::AmbiguousReverseAnchor {
                    elem: ViewRef::Node(*id),
                    candidates,
                },
            )),
        }
    }
}

fn native_restore_residue(complement: &Complement, state: &mut NativeRestoreState) {
    let removed_source_nodes: BTreeSet<_> = complement
        .dropped_edges
        .values()
        .flat_map(|edge| [edge.src, edge.tgt])
        .filter(|node| {
            !complement.dropped_nodes.contains_key(node)
                && !state.presented.nodes.contains_key(node)
        })
        .collect();
    state.restored.nodes.extend(
        complement
            .dropped_nodes
            .iter()
            .map(|(id, node)| (*id, node.clone())),
    );
    state.restored.edges.extend(
        complement
            .dropped_edges
            .iter()
            .filter(|(_, edge)| {
                !removed_source_nodes.contains(&edge.src)
                    && !removed_source_nodes.contains(&edge.tgt)
            })
            .map(|(id, edge)| (*id, edge.clone())),
    );
}

fn native_restore(
    lens: &GraphLens<'_>,
    view: &Tree,
    embedding: &crate::encode::TreeEmbedding,
    complement: &Complement,
    projection: NativeDomainProjection,
) -> Result<(Tree, Graph), TreeRestoreError> {
    let mut state = native_restore_start(lens, view, embedding, complement)
        .map_err(TreeRestoreError::Operational)?;
    let collision_counterfactual = native_collision_counterfactual(&state.presented, complement);
    let restore_complement = collision_counterfactual
        .as_ref()
        .map_or(complement, |(counterfactual, _)| counterfactual);
    if let Some((counterfactual, collisions)) = &collision_counterfactual {
        native_reanchor_collisions(lens, counterfactual, collisions, &mut state);
    }
    native_restore_visible(lens, restore_complement, complement, &mut state);
    native_restore_residue(restore_complement, &mut state);
    state.errors.extend(
        native_source_violations(lens, &state.presented, complement, &state.restored)
            .into_iter()
            .map(NativeDomainError::same),
    );
    let errors = native_domain_report(state.errors, projection);
    if !errors.is_empty() {
        return Err(TreeRestoreError::Domain(errors));
    }

    let parent_name =
        |hyper: &str| crate::encode::ParentNameRule::Exact(source_parent_name(lens, hyper));
    let mut tree =
        native_tree_form(&state.restored, view.root, &parent_name).map_err(tree_shape_error)?;
    project_visible_arc_order(&mut tree, view, embedding, complement);
    Ok((tree, state.restored))
}

/// Definition: Tree put.
///
/// The view tree is laid out as a graph with the edge ids its embedding records (an arc whose child
/// the embedding names keeps that id while its parent is unchanged, fans keep their nodes and edges
/// in fan order), new arcs and fans taking fresh ids above every view, reserved and synthesized id;
/// the graph put's result is read back as a tree with the view's root.
///
/// # Errors
///
/// A complement-identity mismatch, the graph put's first violation, or a
/// typed tree-shape refusal when the restored graph has no tree form.
///
/// Book anchors: `L-lenses-11`.
pub fn tree_put(
    lens: &GraphLens<'_>,
    view: &crate::model::Tree,
    emb: &crate::encode::TreeEmbedding,
    c: &Complement,
) -> Result<crate::model::Tree, LensError> {
    tree_put_with_graph(lens, view, emb, c).map(|(tree, _)| tree)
}

/// Execute tree put while retaining the restored graph carrier.
///
/// The tree representation forgets arc and fan carrier ids. Representation
/// squares use this internal form so that the fast route observes the exact
/// graph produced before that deliberate forgetting step.
pub(super) fn tree_put_with_graph(
    lens: &GraphLens<'_>,
    view: &crate::model::Tree,
    emb: &crate::encode::TreeEmbedding,
    c: &Complement,
) -> Result<(crate::model::Tree, Graph), LensError> {
    lens.verify_identity(c, ComplementShape::WType)?;
    native_restore(lens, view, emb, c, NativeDomainProjection::Graph).map_err(|error| match error {
        TreeRestoreError::Domain(errors) => LensError::PutDomain(errors[0].clone()),
        TreeRestoreError::Operational(error) => LensError::TreeOperational(error),
    })
}

/// Execute tree put with native tree-coordinate domain diagnostics.
///
/// This route is private to the law oracle. Public tree operations and
/// representation squares retain their graph-coordinate diagnostics.
pub(super) fn tree_put_law(
    lens: &GraphLens<'_>,
    view: &crate::model::Tree,
    emb: &crate::encode::TreeEmbedding,
    c: &Complement,
) -> Result<crate::model::Tree, LensError> {
    lens.verify_identity(c, ComplementShape::WType)?;
    native_restore(lens, view, emb, c, NativeDomainProjection::Tree)
        .map(|(tree, _)| tree)
        .map_err(|error| match error {
            TreeRestoreError::Domain(errors) => LensError::PutDomain(errors[0].clone()),
            TreeRestoreError::Operational(error) => LensError::TreeOperational(error),
        })
}

/// Definition: the identity-free tree put-domain predicate.
///
/// The supplied embedding gives every visible arc and fan its graph carrier.
/// The predicate reports graph-restoration violations in the common report
/// order. Carrier allocation and non-fan tree readback failures remain
/// operational errors rather than mathematical domain elements.
///
/// # Errors
///
/// Every put-domain violation in report order, or a typed operational error.
pub fn tree_check_put_domain(
    lens: &GraphLens<'_>,
    view: &crate::model::Tree,
    emb: &crate::encode::TreeEmbedding,
    c: &Complement,
) -> Result<(), LensError> {
    native_restore(lens, view, emb, c, NativeDomainProjection::Graph)
        .map(|_| ())
        .map_err(|error| match error {
            TreeRestoreError::Domain(errors) => LensError::PutDomainReport(errors),
            TreeRestoreError::Operational(error) => LensError::TreeOperational(error),
        })
}

/// Check the tree put domain in native tree coordinates for the law oracle.
pub(super) fn tree_check_put_domain_law(
    lens: &GraphLens<'_>,
    view: &crate::model::Tree,
    emb: &crate::encode::TreeEmbedding,
    c: &Complement,
) -> Result<(), LensError> {
    native_restore(lens, view, emb, c, NativeDomainProjection::Tree)
        .map(|_| ())
        .map_err(|error| match error {
            TreeRestoreError::Domain(errors) => LensError::PutDomainReport(errors),
            TreeRestoreError::Operational(error) => LensError::TreeOperational(error),
        })
}

/// Put the source arcs visible in `view` into the view's child order.
///
/// Graph restoration keeps a source arc's edge id, while graph-to-tree
/// reads arcs in edge-id order. An edited tree order must not overwrite
/// those graph identities merely to affect presentation. Instead, this
/// stable projection replaces only the slots occupied by visible arcs,
/// keyed by their unique child ids. Complement-only arcs retain both
/// their slots and relative order. Thus an unedited put restores the
/// source order exactly, while a fresh get projects the edited visible
/// child sequence exactly, including when labels are equal.
fn project_visible_arc_order(
    restored: &mut crate::model::Tree,
    view: &crate::model::Tree,
    embedding: &crate::encode::TreeEmbedding,
    complement: &Complement,
) {
    let visible_children: BTreeSet<u32> = view
        .arcs
        .iter()
        .filter(|(_, child, _)| {
            embedding
                .arcs
                .get(child)
                .is_none_or(|edge| !complement.synthesized_edges.contains_key(edge))
        })
        .map(|(_, child, _)| *child)
        .collect();
    let arcs_by_child: BTreeMap<u32, (u32, u32, SEdge)> = restored
        .arcs
        .iter()
        .filter(|(_, child, _)| visible_children.contains(child))
        .map(|arc| (arc.1, arc.clone()))
        .collect();
    let mut ordered = view
        .arcs
        .iter()
        .filter_map(|(_, child, _)| arcs_by_child.get(child).cloned());
    for arc in &mut restored.arcs {
        if visible_children.contains(&arc.1)
            && let Some(next) = ordered.next()
        {
            *arc = next;
        }
    }
}

/// The edge of kind `kind` and name `name` from fan node `fan` of
/// hyper-edge `hyper_edge` to the target node with its anchor.
fn fan_edge(
    fan: u32,
    hyper_edge: &str,
    kind: &str,
    name: Option<Name>,
    (tgt, tgt_anchor): (u32, Name),
) -> GEdge {
    GEdge {
        src: fan,
        tgt,
        label: SEdge {
            src: hyper_edge.to_owned(),
            tgt: tgt_anchor,
            kind: kind.to_owned(),
            name,
        },
        position: None,
    }
}

struct NativeViewState {
    graph: Graph,
    next_node: Option<u32>,
    next_edge: Option<u32>,
    used_edges: BTreeSet<u32>,
}

type NativeViewGraph = Result<(Graph, BTreeMap<u32, (u32, u32)>), TreeOperationalError>;

fn native_view_fan(
    lens: &GraphLens<'_>,
    view: &Tree,
    embedding: &crate::encode::TreeEmbedding,
    index: usize,
    state: &mut NativeViewState,
) -> Result<(), TreeOperationalError> {
    let fan = &view.fans[index];
    let known = embedding.fans.get(index);
    let fan_id = if let Some((node, _, _)) =
        known.filter(|(node, _, _)| !state.graph.nodes.contains_key(node))
    {
        *node
    } else {
        take_fresh(&mut state.next_node).ok_or(ids_exhausted(true))?
    };
    let mut node = Node::new(fan_id, &fan.hyper_edge);
    node.shape = NodeShape::HyperEdge;
    state.graph.nodes.insert(fan_id, node);
    let anchor = |node: u32| {
        view.nodes
            .get(&node)
            .map(|node| node.anchor.clone())
            .unwrap_or_default()
    };
    let parent_id = if let Some(parent) = known
        .map(|(_, parent, _)| *parent)
        .filter(|parent| state.used_edges.insert(*parent))
    {
        parent
    } else {
        let fresh = take_fresh(&mut state.next_edge).ok_or(ids_exhausted(false))?;
        state.used_edges.insert(fresh);
        fresh
    };
    state.graph.edges.insert(
        parent_id,
        fan_edge(
            fan_id,
            &fan.hyper_edge,
            HYPER_PARENT,
            target_parent_name(lens, &fan.hyper_edge),
            (fan.parent, anchor(fan.parent)),
        ),
    );
    for (label, child) in &fan.children {
        let child_id = if let Some(id) = known
            .and_then(|(_, _, children)| children.get(label).copied())
            .filter(|id| state.used_edges.insert(*id))
        {
            id
        } else {
            let fresh = take_fresh(&mut state.next_edge).ok_or(ids_exhausted(false))?;
            state.used_edges.insert(fresh);
            fresh
        };
        state.graph.edges.insert(
            child_id,
            fan_edge(
                fan_id,
                &fan.hyper_edge,
                HYPER_LABEL,
                Some(label.clone()),
                (*child, anchor(*child)),
            ),
        );
    }
    Ok(())
}

fn native_view_graph(
    lens: &GraphLens<'_>,
    view: &crate::model::Tree,
    emb: &crate::encode::TreeEmbedding,
    c: &Complement,
) -> NativeViewGraph {
    let next_node = fresh_after(
        view.nodes
            .keys()
            .copied()
            .chain(c.dropped_nodes.keys().copied())
            .chain(c.synthesized_nodes.keys().copied()),
        c.floor.0.max(emb.next_node),
    );
    let next_edge = fresh_after(
        emb.arcs
            .values()
            .copied()
            .chain(c.dropped_edges.keys().copied())
            .chain(c.synthesized_edges.keys().copied())
            .chain(
                emb.fans
                    .iter()
                    .flat_map(|(_, p, ch)| std::iter::once(*p).chain(ch.values().copied())),
            ),
        c.floor.1.max(emb.next_edge),
    );
    let mut state = NativeViewState {
        graph: Graph {
            nodes: view.nodes.clone(),
            edges: BTreeMap::new(),
        },
        next_node,
        next_edge,
        used_edges: BTreeSet::new(),
    };
    let mut current_arcs = BTreeMap::new();
    for (p, ch, label) in &view.arcs {
        let id = if let Some(id) = emb
            .arcs
            .get(ch)
            .copied()
            .filter(|id| state.used_edges.insert(*id))
        {
            id
        } else {
            take_fresh(&mut state.next_edge).ok_or(ids_exhausted(false))?
        };
        state.graph.edges.insert(
            id,
            GEdge {
                src: *p,
                tgt: *ch,
                label: label.clone(),
                position: None,
            },
        );
        current_arcs.insert(id, (*p, *ch));
    }
    for index in 0..view.fans.len() {
        native_view_fan(lens, view, emb, index, &mut state)?;
    }
    Ok((state.graph, current_arcs))
}

const fn ids_exhausted(nodes: bool) -> TreeOperationalError {
    TreeOperationalError::IdsExhausted { nodes }
}

fn fresh_after(ids: impl Iterator<Item = u32>, floor: u32) -> Option<u32> {
    let existing = ids.max();
    let reserved = floor.checked_sub(1);
    existing
        .into_iter()
        .chain(reserved)
        .max()
        .map_or(Some(0), |id| id.checked_add(1))
        .filter(|id| *id != u32::MAX)
}

fn take_fresh(next: &mut Option<u32>) -> Option<u32> {
    let id = (*next)?;
    *next = id.checked_add(1).filter(|next| *next != u32::MAX);
    Some(id)
}

/// The name a view fan's parent edge carries: the source parent label of
/// the source hyper-edge it comes from (itself, or the hyper-edge a
/// resolver entry sends to it), as the tree's graph form names it.
fn source_parent_name(lens: &GraphLens<'_>, hyper: &str) -> Option<Name> {
    lens.src
        .hyper_edges
        .get(hyper)
        .map(|declaration| declaration.parent_label.clone())
}

fn target_parent_name(lens: &GraphLens<'_>, hyper: &str) -> Option<Name> {
    lens.tgt
        .hyper_edges
        .get(hyper)
        .map(|declaration| declaration.parent_label.clone())
}

#[cfg(test)]
mod allocation_tests {
    use std::collections::{BTreeMap, BTreeSet};

    use super::{
        fresh_after, native_collision_counterfactual, native_view_graph, take_fresh,
        tree_check_put_domain, tree_get, tree_put,
    };
    use crate::migration::Migration;
    use crate::model::{Fan, GEdge, Graph, Node, SEdge, Tree};
    use crate::pages::lenses::domain::{PutDomainError, ViewRef};
    use crate::pages::lenses::graph::{GraphLens, LensError};
    use crate::restrict::{Complement, HYPER_LABEL, HYPER_PARENT, NoValueLayer, SynthKey, compile};
    use crate::schema::{HyperEdge, Key, KeyPart, KeyRole, KeyScope, Schema};
    use crate::value::Value;

    #[test]
    fn tree_view_carriers_use_max_minus_one_but_never_reserved_max() {
        let mut next = fresh_after([u32::MAX - 2].into_iter(), 0);
        assert_eq!(take_fresh(&mut next), Some(u32::MAX - 1));
        assert_eq!(take_fresh(&mut next), None);
        assert_eq!(fresh_after([u32::MAX - 1].into_iter(), 0), None);
        assert_eq!(fresh_after([u32::MAX].into_iter(), 0), None);
        assert_eq!(fresh_after(std::iter::empty(), u32::MAX), None);
    }

    #[test]
    fn native_get_returns_exact_non_decreasing_embedding_counters() {
        let edge = SEdge::new("r", "leaf", "child", Some("leaf"));
        let schema = Schema::default()
            .vertex("r", "obj")
            .vertex("leaf", "obj")
            .edge("r", "leaf", "child", Some("leaf"));
        let migration = Migration {
            surviving_verts: schema.vertices.keys().cloned().collect(),
            surviving_edges: schema.edges.clone(),
            ..Migration::default()
        };
        let Ok(plan) = compile(&schema, &schema, &migration) else {
            panic!("identity plan must compile");
        };
        let Ok(lens) = GraphLens::new(
            &schema,
            &schema,
            &migration,
            &plan,
            &BTreeMap::new(),
            &NoValueLayer,
        ) else {
            panic!("identity lens must bind");
        };
        let source = Tree {
            nodes: [(0, Node::new(0, "r")), (1, Node::new(1, "leaf"))].into(),
            arcs: vec![(0, 1, edge)],
            fans: Vec::new(),
            root: 0,
        };
        let Some(Ok((_, complement, embedding))) = tree_get(&lens, &source) else {
            panic!("valid identity tree get must succeed");
        };
        assert_eq!(embedding.next_node, 2);
        assert_eq!(embedding.next_edge, 1);
        assert_eq!(embedding.next_node, complement.floor.0);
        assert_eq!(embedding.next_edge, complement.floor.1);
    }

    #[test]
    fn native_fan_edges_never_replace_an_ordinary_or_prior_fan_edge() {
        let edge = SEdge::new("r", "leaf", "child", Some("leaf"));
        let mut schema = Schema::default()
            .vertex("r", "obj")
            .vertex("leaf", "obj")
            .edge("r", "leaf", "child", Some("leaf"));
        schema.hyper_edges.insert(
            "h".to_owned(),
            HyperEdge {
                kind: "h".to_owned(),
                signature: [("x".to_owned(), "leaf".to_owned())].into(),
                parent_label: "parent".to_owned(),
            },
        );
        let migration = Migration {
            surviving_verts: schema.vertices.keys().cloned().collect(),
            surviving_edges: schema.edges.clone(),
            ..Migration::default()
        };
        let Ok(plan) = compile(&schema, &schema, &migration) else {
            panic!("identity plan must compile");
        };
        let Ok(lens) = GraphLens::new(
            &schema,
            &schema,
            &migration,
            &plan,
            &BTreeMap::new(),
            &NoValueLayer,
        ) else {
            panic!("identity lens must bind");
        };
        let view = Tree {
            nodes: [(0, Node::new(0, "r")), (1, Node::new(1, "leaf"))].into(),
            arcs: vec![(0, 1, edge.clone())],
            fans: vec![Fan {
                parent: 0,
                hyper_edge: "h".to_owned(),
                children: [("x".to_owned(), 1)].into(),
            }],
            root: 0,
        };
        let embedding = crate::encode::TreeEmbedding {
            arcs: [(1, 0)].into(),
            fans: vec![(2, 0, [("x".to_owned(), 0)].into())],
            next_node: 3,
            next_edge: 1,
        };
        let Ok((graph, _)) = native_view_graph(&lens, &view, &embedding, &Complement::default())
        else {
            panic!("duplicate presentation ids must be reallocated");
        };
        assert_eq!(
            graph.edges.keys().copied().collect::<Vec<_>>(),
            vec![0, 1, 2]
        );
        assert_eq!(graph.edges[&0].label, edge);
        assert_eq!(
            graph
                .edges
                .values()
                .map(|edge| edge.label.kind.as_str())
                .collect::<BTreeSet<_>>(),
            BTreeSet::from(["child", HYPER_LABEL, HYPER_PARENT])
        );
    }

    #[test]
    fn native_allocation_never_reuses_ids_below_durable_embedding_counters() {
        let mut schema = Schema::default().vertex("r", "obj").vertex("leaf", "obj");
        schema.hyper_edges.insert(
            "h".to_owned(),
            HyperEdge {
                kind: "h".to_owned(),
                signature: [("x".to_owned(), "leaf".to_owned())].into(),
                parent_label: "parent".to_owned(),
            },
        );
        let migration = Migration {
            surviving_verts: schema.vertices.keys().cloned().collect(),
            ..Migration::default()
        };
        let Ok(plan) = compile(&schema, &schema, &migration) else {
            panic!("identity plan must compile");
        };
        let Ok(lens) = GraphLens::new(
            &schema,
            &schema,
            &migration,
            &plan,
            &BTreeMap::new(),
            &NoValueLayer,
        ) else {
            panic!("identity lens must bind");
        };
        let view = Tree {
            nodes: [(0, Node::new(0, "r")), (1, Node::new(1, "leaf"))].into(),
            arcs: Vec::new(),
            fans: vec![Fan {
                parent: 0,
                hyper_edge: "h".to_owned(),
                children: [("x".to_owned(), 1)].into(),
            }],
            root: 0,
        };
        let embedding = crate::encode::TreeEmbedding {
            arcs: BTreeMap::new(),
            fans: Vec::new(),
            next_node: 40,
            next_edge: 70,
        };
        let Ok((graph, _)) = native_view_graph(&lens, &view, &embedding, &Complement::default())
        else {
            panic!("high durable counters must remain allocatable");
        };
        assert_eq!(
            graph.nodes.keys().copied().collect::<Vec<_>>(),
            vec![0, 1, 40]
        );
        assert_eq!(
            graph.edges.keys().copied().collect::<Vec<_>>(),
            vec![70, 71]
        );
    }

    #[test]
    fn native_tree_collision_reports_the_visible_replacements_duplicate_key() {
        let hidden = SEdge::new("r", "d", "child", Some("hidden"));
        let visible = SEdge::new("r", "r", "child", Some("visible"));
        let mut source_schema = Schema::default()
            .vertex("r", "string")
            .vertex("d", "string")
            .edge("r", "d", "child", Some("hidden"))
            .edge("r", "r", "child", Some("visible"));
        source_schema.keys.insert(
            "r".to_owned(),
            vec![Key {
                id: "r-key".to_owned(),
                parts: vec![KeyPart::Path(Vec::new())],
                role: KeyRole::Unique,
                scope: KeyScope::Global,
                equality: Vec::new(),
            }],
        );
        let target_schema =
            Schema::default()
                .vertex("r", "string")
                .edge("r", "r", "child", Some("visible"));
        let migration = Migration {
            surviving_verts: ["r".to_owned()].into(),
            surviving_edges: [visible.clone()].into(),
            ..Migration::default()
        };
        let Ok(plan) = compile(&source_schema, &target_schema, &migration) else {
            panic!("the collision fixture must compile");
        };
        let Ok(lens) = GraphLens::new(
            &source_schema,
            &target_schema,
            &migration,
            &plan,
            &BTreeMap::new(),
            &NoValueLayer,
        ) else {
            panic!("the collision fixture must bind");
        };
        let source = Tree {
            nodes: [
                (
                    0,
                    Node::new(0, "r").with_value(Value::Str("same".to_owned())),
                ),
                (1, Node::new(1, "d")),
            ]
            .into(),
            arcs: vec![(0, 1, hidden)],
            fans: Vec::new(),
            root: 0,
        };
        let Some(Ok((mut view, complement, embedding))) = tree_get(&lens, &source) else {
            panic!("the collision fixture must have a tree view");
        };
        assert_eq!(
            complement.dropped_nodes.keys().copied().collect::<Vec<_>>(),
            vec![1]
        );
        view.nodes.insert(
            1,
            Node::new(1, "r").with_value(Value::Str("same".to_owned())),
        );
        view.arcs.push((0, 1, visible));
        let expected = vec![
            PutDomainError::IdCollision {
                elem: ViewRef::Node(1),
            },
            PutDomainError::DuplicateKey {
                table: "r".to_owned(),
                key: vec![(String::new(), Value::Str("same".to_owned()))],
                elems: vec![ViewRef::Node(0), ViewRef::Node(1)],
            },
        ];
        assert_eq!(
            tree_check_put_domain(&lens, &view, &embedding, &complement),
            Err(LensError::PutDomainReport(expected.clone()))
        );
        assert_eq!(
            tree_put(&lens, &view, &embedding, &complement),
            Err(LensError::PutDomain(expected[0].clone()))
        );
    }

    fn insert_collision_synth_keys(complement: &mut Complement, label: &SEdge) {
        complement.synthesized_nodes = [
            (
                30,
                SynthKey::Contract {
                    src: 1,
                    tgt: 7,
                    edge: label.clone(),
                },
            ),
            (
                31,
                SynthKey::ContractExpand {
                    src: 7,
                    tgt: 3,
                    hop: 0,
                },
            ),
            (
                32,
                SynthKey::Expand {
                    source_edge: 10,
                    hop: 0,
                },
            ),
            (
                33,
                SynthKey::Expand {
                    source_edge: 12,
                    hop: 0,
                },
            ),
            (
                34,
                SynthKey::Contract {
                    src: 7,
                    tgt: 8,
                    edge: label.clone(),
                },
            ),
        ]
        .into();
        complement.synthesized_edges = [
            (
                40,
                SynthKey::Expand {
                    source_edge: 20,
                    hop: 0,
                },
            ),
            (
                41,
                SynthKey::Expand {
                    source_edge: 12,
                    hop: 1,
                },
            ),
        ]
        .into();
    }

    fn multiple_collision_fixture() -> (Graph, Complement) {
        let label = SEdge::new("r", "r", "child", Some("visible"));
        let edge = |src, tgt| GEdge {
            src,
            tgt,
            label: label.clone(),
            position: None,
        };
        let presented = Graph {
            nodes: [(1, Node::new(1, "r")), (3, Node::new(3, "r"))].into(),
            edges: BTreeMap::new(),
        };
        let mut complement = Complement {
            dropped_nodes: [
                (1, Node::new(1, "d")),
                (3, Node::new(3, "d")),
                (9, Node::new(9, "d")),
            ]
            .into(),
            dropped_edges: [(10, edge(0, 1)), (11, edge(3, 4)), (12, edge(5, 6))].into(),
            original_edges: [(20, edge(1, 8)), (21, edge(5, 6))].into(),
            original_anchors: [(1, "d".to_owned()), (7, "r".to_owned())].into(),
            original_fields: [(3, BTreeMap::new()), (7, BTreeMap::new())].into(),
            original_values: [(1, None), (7, None)].into(),
            ..Complement::default()
        };
        insert_collision_synth_keys(&mut complement, &label);
        (presented, complement)
    }

    #[test]
    fn collision_counterfactual_strips_multiple_nodes_and_incident_keys_only() {
        let (presented, complement) = multiple_collision_fixture();
        let Some((counterfactual, collisions)) =
            native_collision_counterfactual(&presented, &complement)
        else {
            panic!("both visible ids collide with dropped source nodes");
        };
        assert_eq!(collisions, BTreeSet::from([1, 3]));
        assert_eq!(
            counterfactual
                .dropped_nodes
                .keys()
                .copied()
                .collect::<Vec<_>>(),
            vec![9]
        );
        assert_eq!(
            counterfactual
                .dropped_edges
                .keys()
                .copied()
                .collect::<Vec<_>>(),
            vec![12]
        );
        assert_eq!(
            counterfactual
                .original_edges
                .keys()
                .copied()
                .collect::<Vec<_>>(),
            vec![21]
        );
        assert_eq!(
            counterfactual
                .synthesized_nodes
                .keys()
                .copied()
                .collect::<Vec<_>>(),
            vec![33, 34]
        );
        assert_eq!(
            counterfactual.synthesized_nodes[&33],
            SynthKey::Expand {
                source_edge: 12,
                hop: 0,
            },
            "reference graph edge ids are stable, so a surviving expansion key is not reindexed"
        );
        assert_eq!(
            counterfactual.synthesized_edges,
            [(
                41,
                SynthKey::Expand {
                    source_edge: 12,
                    hop: 1,
                },
            )]
            .into()
        );
        assert_eq!(
            counterfactual
                .original_anchors
                .keys()
                .copied()
                .collect::<Vec<_>>(),
            vec![7]
        );
        assert_eq!(
            counterfactual
                .original_fields
                .keys()
                .copied()
                .collect::<Vec<_>>(),
            vec![7]
        );
        assert_eq!(
            counterfactual
                .original_values
                .keys()
                .copied()
                .collect::<Vec<_>>(),
            vec![7]
        );
    }
}
