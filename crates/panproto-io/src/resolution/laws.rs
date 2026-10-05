//! Finite five-law checker for provider-indexed resolutions.

use std::collections::HashMap;

use panproto_inst::{
    AnyInstance, EdgeId, FieldPresence, GInstance, Node, ResolutionComplement, RowId, Value,
};
use panproto_lens::laws::{FamilyReport, Mutation, SkipReason};
use panproto_schema::Edge;
use serde::{Deserialize, Serialize};

use super::{
    Resolution, ResolveContext, ResolveError, ResolveOptions, instances_equivalent,
    rekey_resolution_complement_edges,
};

/// Observations from all mutation families in the finite Single-Step Basis.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolutionLawReport {
    /// Families for which at least one candidate was generated.
    pub ran: Vec<FamilyReport>,
    /// Families this target shape or concrete view could not instantiate.
    pub skipped: Vec<(Mutation, SkipReason)>,
}

/// The first failed resolution gate, in contract order.
#[derive(Clone, Debug, Serialize, Deserialize, thiserror::Error)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionLawViolation {
    /// `put(get(source))` did not reproduce the source exactly.
    #[error("resolution GetPut failed: {detail}")]
    GetPut {
        /// Stable diagnostic detail.
        detail: String,
    },
    /// A successful put did not get back to the edited view.
    #[error("resolution PutGet failed for {mutation:?}: {detail}")]
    PutGet {
        /// Mutation family, absent for the unedited view.
        mutation: Option<Mutation>,
        /// Stable diagnostic detail.
        detail: String,
    },
    /// Put failed after the complete domain checker admitted the candidate.
    #[error("resolution Totality failed for {mutation:?}: {error}")]
    Totality {
        /// Mutation family.
        mutation: Mutation,
        /// Typed operation error.
        error: ResolveError,
    },
    /// Replaying put with the complement produced by get changed the source.
    #[error("resolution PutTwice failed for {mutation:?}: {detail}")]
    PutTwice {
        /// Mutation family, absent for the unedited view.
        mutation: Option<Mutation>,
        /// Stable diagnostic detail.
        detail: String,
    },
    /// A coherent synthesized-id rekey changed admission or put.
    #[error("resolution Congruence failed: {detail}")]
    Congruence {
        /// Stable diagnostic detail.
        detail: String,
    },
    /// Get or a second get failed outside the Totality gate.
    #[error("resolution law operation failed: {0}")]
    Operation(ResolveError),
}

struct CandidateRun {
    mutation: Mutation,
    candidate: AnyInstance,
    put: Result<AnyInstance, ResolveError>,
}

/// Run GetPut, PutGet, Totality, PutTwice and Congruence, in that order.
///
/// A successful report means that no counterexample was found in the finite
/// deterministic Single-Step Basis for this source. It is not a proof over
/// arbitrary instances.
pub fn check_resolution_laws(
    resolution: &Resolution,
    source: &AnyInstance,
    options: &ResolveOptions,
    context: &ResolveContext<'_>,
) -> Result<ResolutionLawReport, ResolutionLawViolation> {
    let (view, complement) = resolution
        .get(source, options, context)
        .map_err(ResolutionLawViolation::Operation)?;
    let restored = resolution
        .put(&view, &complement, context)
        .map_err(ResolutionLawViolation::Operation)?;
    if !instances_identical(source, &restored) {
        return Err(ResolutionLawViolation::GetPut {
            detail: "the reconstructed syntactic instance differs from the input".to_owned(),
        });
    }

    let mut ran = Vec::new();
    let mut skipped = Vec::new();
    let mut runs = Vec::new();
    for mutation in Mutation::ALL {
        let Some(candidates) = mutation_candidates(resolution, &view, &complement, mutation) else {
            skipped.push((mutation, SkipReason::ShapeCannotExpress));
            continue;
        };
        if candidates.is_empty() {
            skipped.push((mutation, SkipReason::NoCandidate));
            continue;
        }
        let mut report = FamilyReport {
            mutation,
            in_domain: 0,
            in_domain_ok: 0,
            negative: 0,
            negative_ok: 0,
        };
        for candidate in candidates {
            if resolution
                .check_put_domain(&candidate, &complement, context)
                .is_err()
            {
                report.negative = report.negative.saturating_add(1);
                report.negative_ok = report.negative_ok.saturating_add(1);
                continue;
            }
            report.in_domain = report.in_domain.saturating_add(1);
            runs.push(CandidateRun {
                mutation,
                put: resolution.put(&candidate, &complement, context),
                candidate,
            });
        }
        ran.push(report);
    }

    let mut regotten = Vec::with_capacity(runs.len());
    for run in &runs {
        let Ok(restored) = &run.put else {
            regotten.push(None);
            continue;
        };
        let result = resolution
            .get(restored, options, context)
            .map_err(ResolutionLawViolation::Operation)?;
        if !instances_equivalent(&run.candidate, &result.0) {
            return Err(ResolutionLawViolation::PutGet {
                mutation: Some(run.mutation),
                detail: "get did not reproduce the admitted edited view".to_owned(),
            });
        }
        regotten.push(Some(result));
    }

    for run in &runs {
        if let Err(error) = &run.put {
            return Err(ResolutionLawViolation::Totality {
                mutation: run.mutation,
                error: error.clone(),
            });
        }
    }

    for (run, regotten) in runs.iter().zip(&regotten) {
        let restored = run
            .put
            .as_ref()
            .map_err(|error| ResolutionLawViolation::Totality {
                mutation: run.mutation,
                error: error.clone(),
            })?;
        let Some((regotten_view, produced)) = regotten else {
            continue;
        };
        let twice = resolution
            .put(regotten_view, produced, context)
            .map_err(ResolutionLawViolation::Operation)?;
        if !instances_identical(restored, &twice) {
            return Err(ResolutionLawViolation::PutTwice {
                mutation: Some(run.mutation),
                detail: "put with the newly produced complement changed the source".to_owned(),
            });
        }
        if let Some(report) = ran
            .iter_mut()
            .find(|report| report.mutation == run.mutation)
        {
            report.in_domain_ok = report.in_domain_ok.saturating_add(1);
        }
    }

    check_congruence(resolution, &view, &complement, context)?;
    Ok(ResolutionLawReport { ran, skipped })
}

fn instances_identical(left: &AnyInstance, right: &AnyInstance) -> bool {
    match (left, right) {
        (AnyInstance::Tree(left), AnyInstance::Tree(right)) => {
            panproto_inst::segment::canonical_winstance_bytes(left)
                == panproto_inst::segment::canonical_winstance_bytes(right)
        }
        (AnyInstance::Table(left), AnyInstance::Table(right)) => left == right,
        (AnyInstance::Graph(left), AnyInstance::Graph(right)) => GInstance::identical(left, right),
        _ => false,
    }
}

fn check_congruence(
    resolution: &Resolution,
    view: &AnyInstance,
    complement: &ResolutionComplement,
    context: &ResolveContext<'_>,
) -> Result<(), ResolutionLawViolation> {
    let AnyInstance::Graph(graph) = view else {
        return Ok(());
    };
    let Some((&old, _)) = complement.synth_index().edges.iter().next() else {
        return Ok(());
    };
    let new = EdgeId(
        graph
            .max_edge_id()
            .map_or(Some(0), |id| id.0.checked_add(1))
            .ok_or_else(|| ResolutionLawViolation::Congruence {
                detail: "no fresh edge id exists for the congruence witness".to_owned(),
            })?,
    );
    let mut rekeyed_graph = graph.clone();
    let edge =
        rekeyed_graph
            .remove_edge(old)
            .map_err(|error| ResolutionLawViolation::Congruence {
                detail: error.to_string(),
            })?;
    rekeyed_graph
        .insert_edge(new, edge)
        .map_err(|error| ResolutionLawViolation::Congruence {
            detail: error.to_string(),
        })?;
    let edge_ids = std::collections::BTreeMap::from([(old, new)]);
    let rekeyed = rekey_resolution_complement_edges(complement.clone(), &edge_ids);
    let original_domain = resolution
        .check_put_domain(view, complement, context)
        .is_ok();
    let next_view = AnyInstance::Graph(rekeyed_graph);
    let rekeyed_domain = resolution
        .check_put_domain(&next_view, &rekeyed, context)
        .is_ok();
    if original_domain != rekeyed_domain {
        return Err(ResolutionLawViolation::Congruence {
            detail: "a coherent edge-id rekey changed the domain verdict".to_owned(),
        });
    }
    if original_domain {
        let original = resolution
            .put(view, complement, context)
            .map_err(ResolutionLawViolation::Operation)?;
        let next = resolution
            .put(&next_view, &rekeyed, context)
            .map_err(ResolutionLawViolation::Operation)?;
        if !instances_identical(&original, &next) {
            return Err(ResolutionLawViolation::Congruence {
                detail: "a coherent edge-id rekey changed put's source".to_owned(),
            });
        }
    }
    Ok(())
}

fn mutation_candidates(
    resolution: &Resolution,
    view: &AnyInstance,
    complement: &ResolutionComplement,
    mutation: Mutation,
) -> Option<Vec<AnyInstance>> {
    match view {
        AnyInstance::Graph(graph) => graph_candidates(resolution, graph, complement, mutation),
        AnyInstance::Tree(tree) => tree_candidates(resolution, tree, mutation),
        AnyInstance::Table(table) => table_candidates(resolution, table, mutation),
    }
}

fn graph_candidates(
    resolution: &Resolution,
    graph: &GInstance,
    complement: &ResolutionComplement,
    mutation: Mutation,
) -> Option<Vec<AnyInstance>> {
    let one = |graph| vec![AnyInstance::Graph(graph)];
    match mutation {
        Mutation::ValueEdit | Mutation::FieldEdit => {
            let id = graph.nodes().keys().copied().min()?;
            let mut next = graph.clone();
            let node = next.node_mut(id)?;
            if mutation == Mutation::ValueEdit {
                node.value = Some(toggled_presence(node.value.as_ref()));
            } else {
                node.extra_fields
                    .insert("__resolution_law".to_owned(), Value::Bool(true));
            }
            Some(one(next))
        }
        Mutation::AddNode => {
            let id = graph
                .max_node_id()
                .map_or(Some(0), |id| id.checked_add(1))?;
            let anchor = sorted_vertices(resolution).into_iter().next()?;
            let mut next = graph.clone();
            next.insert_node(Node::new(id, anchor)).ok()?;
            Some(one(next))
        }
        Mutation::RemoveNode => {
            let id = graph.nodes().keys().copied().min()?;
            let mut next = graph.clone();
            let incident: Vec<_> = next
                .out_edges(id)
                .iter()
                .chain(next.in_edges(id))
                .copied()
                .collect();
            for edge in incident {
                next.remove_edge(edge).ok()?;
            }
            next.remove_node(id).ok()?;
            Some(one(next))
        }
        Mutation::RelabelNode => {
            let id = graph.nodes().keys().copied().min()?;
            let current = graph.node(id)?.anchor.clone();
            let anchor = sorted_vertices(resolution)
                .into_iter()
                .find(|anchor| *anchor != current)?;
            let mut next = graph.clone();
            next.node_mut(id)?.anchor = anchor;
            Some(one(next))
        }
        Mutation::AddEdge | Mutation::AddSelfLoop => {
            let (src, tgt, edge) =
                edge_placement(resolution, graph, mutation == Mutation::AddSelfLoop)?;
            let mut next = graph.clone();
            next.add_edge(src, tgt, edge).ok()?;
            Some(one(next))
        }
        Mutation::RemoveEdge => {
            let id = graph.edges().keys().copied().min()?;
            let mut next = graph.clone();
            next.remove_edge(id).ok()?;
            Some(one(next))
        }
        Mutation::Reconnect => {
            let id = graph.edges().keys().copied().min()?;
            let edge = graph.edge(id)?;
            let src = graph
                .nodes()
                .iter()
                .filter_map(|(&id, node)| (node.anchor == edge.edge.src).then_some(id))
                .find(|id| *id != edge.src)
                .unwrap_or(edge.src);
            let tgt = graph
                .nodes()
                .iter()
                .filter_map(|(&id, node)| (node.anchor == edge.edge.tgt).then_some(id))
                .find(|id| *id != edge.tgt)
                .unwrap_or(edge.tgt);
            if (src, tgt) == (edge.src, edge.tgt) {
                return Some(Vec::new());
            }
            let mut next = graph.clone();
            next.set_endpoints(id, src, tgt).ok()?;
            Some(one(next))
        }
        Mutation::RelabelEdge => {
            let id = graph.edges().keys().copied().min()?;
            let old = graph.edge(id)?;
            let label = sorted_edges(resolution).into_iter().find(|edge| {
                *edge != old.edge
                    && graph
                        .node(old.src)
                        .is_some_and(|node| node.anchor == edge.src)
                    && graph
                        .node(old.tgt)
                        .is_some_and(|node| node.anchor == edge.tgt)
            })?;
            let mut next = graph.clone();
            let mut edge = next.remove_edge(id).ok()?;
            edge.edge = label;
            next.insert_edge(id, edge).ok()?;
            Some(one(next))
        }
        Mutation::Reorder => {
            let id = graph.edges().keys().copied().min()?;
            let mut next = graph.clone();
            next.set_edge_position(id, Some(0)).ok()?;
            Some(one(next))
        }
        Mutation::AddParallelEdge => {
            let edge = graph.edges_sorted().next()?.1.clone();
            let mut next = graph.clone();
            next.add_edge(edge.src, edge.tgt, edge.edge).ok()?;
            Some(one(next))
        }
        Mutation::RemoveSynthesized => {
            if let Some((&id, _)) = complement.synth_index().edges.iter().next() {
                let mut next = graph.clone();
                next.remove_edge(id).ok()?;
                Some(one(next))
            } else if let Some((&id, _)) = complement.synth_index().nodes.iter().next() {
                let mut next = graph.clone();
                let incident: Vec<_> = next
                    .out_edges(id)
                    .iter()
                    .chain(next.in_edges(id))
                    .copied()
                    .collect();
                for edge in incident {
                    next.remove_edge(edge).ok()?;
                }
                next.remove_node(id).ok()?;
                Some(one(next))
            } else {
                Some(Vec::new())
            }
        }
        Mutation::EditSynthesized => {
            if let Some((&id, _)) = complement.synth_index().edges.iter().next() {
                let mut next = graph.clone();
                next.set_edge_position(id, Some(0)).ok()?;
                Some(one(next))
            } else if let Some((&id, _)) = complement.synth_index().nodes.iter().next() {
                let mut next = graph.clone();
                next.node_mut(id)?.value = Some(FieldPresence::Present(Value::Bool(true)));
                Some(one(next))
            } else {
                Some(Vec::new())
            }
        }
        Mutation::AddContractedEdge | Mutation::ReservedIdReuse | Mutation::MalformedFan => None,
    }
}

fn tree_candidates(
    resolution: &Resolution,
    tree: &panproto_inst::WInstance,
    mutation: Mutation,
) -> Option<Vec<AnyInstance>> {
    let one = |tree| vec![AnyInstance::Tree(tree)];
    match mutation {
        Mutation::ValueEdit | Mutation::FieldEdit => {
            let id = tree.nodes.keys().copied().min()?;
            let mut next = tree.clone();
            let node = next.nodes.get_mut(&id)?;
            if mutation == Mutation::ValueEdit {
                node.value = Some(toggled_presence(node.value.as_ref()));
            } else {
                node.extra_fields
                    .insert("__resolution_law".to_owned(), Value::Bool(true));
            }
            Some(one(next))
        }
        Mutation::AddNode => {
            let edge = sorted_edges(resolution).into_iter().find(|edge| {
                tree.node(tree.root)
                    .is_some_and(|node| node.anchor == edge.src)
            })?;
            let id = tree
                .nodes
                .keys()
                .copied()
                .max()
                .map_or(Some(0), |id| id.checked_add(1))?;
            let mut nodes = tree.nodes.clone();
            nodes.insert(id, Node::new(id, edge.tgt.clone()));
            let mut arcs = tree.arcs.clone();
            arcs.push((tree.root, id, edge));
            Some(one(panproto_inst::WInstance::new(
                nodes,
                arcs,
                tree.fans.clone(),
                tree.root,
                tree.schema_root.clone(),
            )))
        }
        Mutation::RemoveNode => {
            let id = tree
                .nodes
                .keys()
                .copied()
                .filter(|id| *id != tree.root && tree.children(*id).is_empty())
                .min()?;
            let mut nodes = tree.nodes.clone();
            nodes.remove(&id);
            let arcs = tree
                .arcs
                .iter()
                .filter(|(_, child, _)| *child != id)
                .cloned()
                .collect();
            let fans = tree
                .fans
                .iter()
                .filter(|fan| fan.parent != id && !fan.children.values().any(|child| *child == id))
                .cloned()
                .collect();
            Some(one(panproto_inst::WInstance::new(
                nodes,
                arcs,
                fans,
                tree.root,
                tree.schema_root.clone(),
            )))
        }
        Mutation::RelabelNode => {
            let id = tree.nodes.keys().copied().min()?;
            let current = tree.node(id)?.anchor.clone();
            let anchor = sorted_vertices(resolution)
                .into_iter()
                .find(|anchor| *anchor != current)?;
            let mut next = tree.clone();
            next.nodes.get_mut(&id)?.anchor = anchor;
            Some(one(next))
        }
        Mutation::Reorder => {
            let parent = tree
                .children_map
                .iter()
                .find_map(|(&parent, children)| (children.len() > 1).then_some(parent))?;
            let mut arcs = tree.arcs.clone();
            let mut indices: Vec<_> = arcs
                .iter()
                .enumerate()
                .filter_map(|(index, (source, _, _))| (*source == parent).then_some(index))
                .collect();
            indices.sort_unstable();
            arcs.swap(indices[0], indices[1]);
            Some(one(panproto_inst::WInstance::new(
                tree.nodes.clone(),
                arcs,
                tree.fans.clone(),
                tree.root,
                tree.schema_root.clone(),
            )))
        }
        Mutation::AddEdge
        | Mutation::RemoveEdge
        | Mutation::Reconnect
        | Mutation::RelabelEdge
        | Mutation::AddParallelEdge
        | Mutation::AddSelfLoop
        | Mutation::RemoveSynthesized
        | Mutation::EditSynthesized
        | Mutation::AddContractedEdge
        | Mutation::ReservedIdReuse
        | Mutation::MalformedFan => None,
    }
}

fn table_candidates(
    resolution: &Resolution,
    table: &panproto_inst::FInstance,
    mutation: Mutation,
) -> Option<Vec<AnyInstance>> {
    let one = |table| vec![AnyInstance::Table(table)];
    match mutation {
        Mutation::ValueEdit | Mutation::FieldEdit => {
            let row = table
                .tables()
                .into_iter()
                .flat_map(|(_, rows)| rows.ids())
                .min()?;
            let mut next = table.clone();
            let record = next.row_mut(row)?;
            let column = record
                .keys()
                .min()
                .cloned()
                .unwrap_or_else(|| "__resolution_law".to_owned());
            record.insert(column, Value::Bool(true));
            Some(one(next))
        }
        Mutation::AddNode => {
            let protocol = resolution.provider().protocol();
            let vertex = resolution
                .semantic_schema()
                .vertices
                .iter()
                .filter(|(_, vertex)| protocol.non_value_kinds.contains(vertex.kind.as_ref()))
                .map(|(name, _)| name.clone())
                .min()?;
            let mut next = table.clone();
            let id = next.fresh_row_id().ok()?;
            next.insert_row(vertex, id, HashMap::new()).ok()?;
            Some(one(next))
        }
        Mutation::RemoveNode => {
            let row = table
                .tables()
                .into_iter()
                .flat_map(|(_, rows)| rows.ids())
                .min()?;
            let mut next = table.clone();
            next.remove_row(row)?;
            Some(one(next))
        }
        Mutation::AddEdge => {
            let (edge, src, tgt) = table_pair_placement(resolution, table)?;
            let mut next = table.clone();
            next.insert_foreign_key(edge, src, tgt).ok()?;
            Some(one(next))
        }
        Mutation::RemoveEdge => {
            let (edge, src, tgt) = table
                .foreign_keys()
                .into_iter()
                .flat_map(|(edge, pairs)| pairs.iter().map(|&(src, tgt)| (edge.clone(), src, tgt)))
                .min()?;
            let mut next = table.clone();
            if !next.remove_foreign_key(&edge, src, tgt) {
                return Some(Vec::new());
            }
            Some(one(next))
        }
        Mutation::RelabelNode
        | Mutation::Reconnect
        | Mutation::RelabelEdge
        | Mutation::Reorder
        | Mutation::AddParallelEdge
        | Mutation::AddSelfLoop
        | Mutation::RemoveSynthesized
        | Mutation::EditSynthesized
        | Mutation::AddContractedEdge
        | Mutation::ReservedIdReuse
        | Mutation::MalformedFan => None,
    }
}

fn toggled_presence(previous: Option<&FieldPresence>) -> FieldPresence {
    match previous {
        Some(FieldPresence::Present(Value::Bool(true))) => {
            FieldPresence::Present(Value::Bool(false))
        }
        _ => FieldPresence::Present(Value::Bool(true)),
    }
}

fn sorted_vertices(resolution: &Resolution) -> Vec<panproto_gat::Name> {
    let mut vertices: Vec<_> = resolution
        .semantic_schema()
        .vertices
        .keys()
        .cloned()
        .collect();
    vertices.sort_unstable();
    vertices
}

fn sorted_edges(resolution: &Resolution) -> Vec<Edge> {
    let mut edges: Vec<_> = resolution.semantic_schema().edges.keys().cloned().collect();
    edges.sort_unstable();
    edges
}

fn edge_placement(
    resolution: &Resolution,
    graph: &GInstance,
    self_loop: bool,
) -> Option<(u32, u32, Edge)> {
    for edge in sorted_edges(resolution) {
        for (&src, source) in graph.nodes() {
            if source.anchor != edge.src {
                continue;
            }
            for (&tgt, target) in graph.nodes() {
                if target.anchor == edge.tgt && (!self_loop || src == tgt) {
                    return Some((src, tgt, edge));
                }
            }
        }
    }
    None
}

fn table_pair_placement(
    resolution: &Resolution,
    table: &panproto_inst::FInstance,
) -> Option<(Edge, RowId, RowId)> {
    for edge in sorted_edges(resolution) {
        for (src, _) in table.rows(edge.src.as_ref()) {
            for (tgt, _) in table.rows(edge.tgt.as_ref()) {
                if !table.pairs(&edge).contains(&(src, tgt)) {
                    return Some((edge, src, tgt));
                }
            }
        }
    }
    None
}
