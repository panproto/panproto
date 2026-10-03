//! Shape bridges used by graph-operation dispatch and its agreement tests.
//!
//! Graph restrict and extend always run their graph engines. Lens get and
//! put may use a shape-specific engine only after a release benchmark shows
//! that the complete conversion is faster. The conversions here are kept
//! independently of that optimisation decision: they state how table
//! complement coordinates sit inside graph complement coordinates.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, HashMap};

use panproto_gat::Name;
use panproto_inst::identity::{LegacyWidth, legacy_schema_fingerprint};
use panproto_inst::{
    BoundaryProtocols, Complement, ComplementShape, DroppedRow, EdgeId, FInstance, Fan,
    FanEmbedding, GEdge, GInstance, GraphComplement, HYPER_LABEL_KIND, HYPER_PARENT_KIND, IdFloor,
    IdentityPolicy, Node, NodeShape, RelSchema, RowId, TableComplement, TableEmbedding,
    TreeEmbedding, Value,
};
use panproto_schema::{Edge, Protocol};

use crate::{ComplementIdentity, Lens, LensError};

fn not_table(reason: impl Into<String>) -> LensError {
    LensError::NotTableComplement {
        reason: reason.into(),
    }
}

fn not_tree(reason: impl Into<String>) -> LensError {
    LensError::NotTreeComplement {
        reason: reason.into(),
    }
}

const fn nested(value: &Value) -> bool {
    matches!(value, Value::List(_) | Value::Unknown(_))
}

fn row_like(node: &Node) -> bool {
    node.value.is_none()
        && node.discriminator.is_none()
        && node.position.is_none()
        && node.shape == NodeShape::Plain
        && node.annotations.is_empty()
        && node.extra_fields.values().all(|value| !nested(value))
}

fn fan_edge(edge: &Edge) -> bool {
    edge.kind == HYPER_PARENT_KIND || edge.kind == HYPER_LABEL_KIND
}

fn same_value(left: &Value, right: &Value) -> bool {
    left.canonical_cmp(right) == Ordering::Equal
}

fn same_row(left: &BTreeMap<String, Value>, right: &HashMap<String, Value>) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .all(|(column, value)| right.get(column).is_some_and(|v| same_value(value, v)))
}

fn source_table(lens: &Lens, view_table: &Name, original: Option<&Name>) -> Option<Name> {
    if let Some(original) = original {
        return Some(original.clone());
    }
    let mut preimages: Vec<&Name> = lens
        .src_schema
        .vertices
        .keys()
        .filter(|source| lens.compiled.vertex_remap.get(*source).unwrap_or(*source) == view_table)
        .collect();
    preimages.sort_unstable();
    preimages.dedup();
    match preimages.as_slice() {
        [source] => Some((*source).clone()),
        [] if lens.src_schema.vertices.contains_key(view_table) => Some(view_table.clone()),
        _ => None,
    }
}

fn pair_for_id(embedding: &TableEmbedding, id: EdgeId) -> Option<&(Edge, RowId, RowId)> {
    embedding
        .pairs
        .iter()
        .find_map(|(pair, &edge_id)| (edge_id == id).then_some(pair))
}

/// Convert a table complement into its graph coordinates.
///
/// `view_embedding` presents the table view as a graph. `source_embedding`
/// presents the source table, and is needed because dropped pairs retain
/// their source `EdgeId`s. Cell residue is rebuilt as the whole source-row
/// snapshot that the graph complement records, column by column of the
/// source relational schema.
///
/// # Errors
///
/// [`LensError::NotTableComplement`] when either embedding lacks an element
/// named by the complement or a kept row has no determined source table.
pub fn table_complement_to_graph(
    lens: &Lens,
    view: &FInstance,
    complement: &TableComplement,
    source_embedding: &TableEmbedding,
    view_embedding: &TableEmbedding,
    source_protocol: &Protocol,
) -> Result<GraphComplement, LensError> {
    let source_rel = RelSchema::new(&lens.src_schema, source_protocol)
        .map_err(|error| not_table(error.to_string()))?;
    let mut graph = GraphComplement::empty();

    for (&row, dropped) in &complement.dropped_rows {
        let mut node = Node::new(row.0, dropped.table.clone());
        node.extra_fields.clone_from(&dropped.row);
        graph.dropped_nodes.insert(row.0, node);
    }
    for (edge, pairs) in &complement.dropped_fk_pairs {
        for &(src, tgt) in pairs {
            let id = source_embedding
                .pairs
                .get(&(edge.clone(), src, tgt))
                .copied()
                .ok_or_else(|| {
                    not_table(format!(
                        "source embedding has no dropped pair {edge:?} ({}, {})",
                        src.0, tgt.0
                    ))
                })?;
            graph
                .dropped_edges
                .insert(id, GEdge::new(src.0, tgt.0, edge.clone()));
        }
    }
    for ((view_edge, src, tgt), originals) in &complement.original_fk_edges {
        let id = view_embedding
            .pairs
            .get(&(view_edge.clone(), *src, *tgt))
            .copied()
            .ok_or_else(|| not_table("view embedding has no original foreign-key pair"))?;
        let originals: Vec<&Edge> = originals.iter().collect();
        let [original] = originals.as_slice() else {
            return Err(not_table(
                "one graph edge cannot carry several original foreign keys",
            ));
        };
        graph
            .original_edges
            .insert(id, GEdge::new(src.0, tgt.0, (*original).clone()));
    }
    graph.original_anchors = complement
        .original_tables
        .iter()
        .map(|(row, table)| (row.0, table.clone()))
        .collect();

    for id in view.row_ids() {
        let Some(view_table) = view.table_of(id) else {
            continue;
        };
        let source_table = source_table(lens, view_table, complement.original_tables.get(&id))
            .ok_or_else(|| not_table(format!("row {} has no determined source table", id.0)))?;
        let mut snapshot = BTreeMap::new();
        for column in source_rel.columns(source_table.as_str()).keys() {
            let key = (id, column.clone());
            let value = complement
                .dropped_cells
                .get(&key)
                .or_else(|| complement.original_cells.get(&key))
                .or_else(|| view.row(id).and_then(|row| row.get(column)));
            if let Some(value) = value {
                snapshot.insert(column.clone(), value.clone());
            }
        }
        let view_row = view.row(id).cloned().unwrap_or_default();
        if !same_row(&snapshot, &view_row) {
            graph.original_extra_fields.insert(id.0, snapshot);
        }
    }

    let synth = complement.synth_index(view_embedding);
    graph.synthesized_edges = synth.edges;
    graph.id_floor.node = complement.id_floor;
    graph.id_floor.edge = source_embedding.next_edge;
    graph.identity = Some(lens.identity(ComplementShape::Graph));
    Ok(graph)
}

/// Convert a graph complement on a table-shaped view into table coordinates.
///
/// Whole-map graph field snapshots are differenced against `view` over the
/// columns of the row's source table: an absent same-named view cell becomes
/// `dropped_cells`, while a differing one becomes `original_cells`.
///
/// # Errors
///
/// [`LensError::NotTableComplement`] for graph-only state: fan or positioned
/// edges, non-row nodes, synthesized nodes, elements outside `embedding`, or
/// a row whose source table is not determined.
pub fn graph_complement_to_table(
    lens: &Lens,
    view: &FInstance,
    complement: &GraphComplement,
    embedding: &TableEmbedding,
    source_protocol: &Protocol,
) -> Result<TableComplement, LensError> {
    if !complement.synthesized_nodes.is_empty() {
        return Err(not_table(
            "a table complement cannot name synthesized nodes",
        ));
    }
    let source_rel = RelSchema::new(&lens.src_schema, source_protocol)
        .map_err(|error| not_table(error.to_string()))?;
    let mut table = TableComplement::empty();

    for (&id, node) in &complement.dropped_nodes {
        if !row_like(node) {
            return Err(not_table(format!("dropped node {id} is not a row")));
        }
        table.dropped_rows.insert(
            RowId(id),
            DroppedRow {
                table: node.anchor.clone(),
                row: node.extra_fields.clone(),
            },
        );
    }
    for (&id, edge) in &complement.dropped_edges {
        if edge.position.is_some() || fan_edge(&edge.edge) {
            return Err(not_table(format!("dropped edge {id} is not a foreign key")));
        }
        table
            .dropped_fk_pairs
            .entry(edge.edge.clone())
            .or_default()
            .insert((RowId(edge.src), RowId(edge.tgt)));
    }
    for (&id, original) in &complement.original_edges {
        if original.position.is_some() || fan_edge(&original.edge) {
            return Err(not_table(format!(
                "original edge {id} is not a foreign key"
            )));
        }
        let (view_edge, src, tgt) = pair_for_id(embedding, id)
            .ok_or_else(|| not_table(format!("view embedding has no edge {id}")))?;
        table
            .original_fk_edges
            .entry((view_edge.clone(), *src, *tgt))
            .or_default()
            .insert(original.edge.clone());
    }
    table.original_tables = complement
        .original_anchors
        .iter()
        .map(|(&id, table)| (RowId(id), table.clone()))
        .collect();

    for (&id, snapshot) in &complement.original_extra_fields {
        let row = RowId(id);
        let view_table = view
            .table_of(row)
            .ok_or_else(|| not_table(format!("view has no row {id}")))?;
        let source_table = source_table(lens, view_table, complement.original_anchors.get(&id))
            .ok_or_else(|| not_table(format!("row {id} has no determined source table")))?;
        let view_row = view.row(row).cloned().unwrap_or_default();
        for column in source_rel.columns(source_table.as_str()).keys() {
            let Some(source_value) = snapshot.get(column) else {
                continue;
            };
            match view_row.get(column) {
                None => {
                    table
                        .dropped_cells
                        .insert((row, column.clone()), source_value.clone());
                }
                Some(view_value) if !same_value(source_value, view_value) => {
                    table
                        .original_cells
                        .insert((row, column.clone()), source_value.clone());
                }
                Some(_) => {}
            }
        }
    }

    for (&id, key) in &complement.synthesized_edges {
        let (edge, src, tgt) = pair_for_id(embedding, id)
            .ok_or_else(|| not_table(format!("view embedding has no synthesized edge {id}")))?;
        let expected = panproto_inst::SynthKey::Contract {
            src: src.0,
            tgt: tgt.0,
            edge: edge.clone(),
        };
        if *key != expected {
            return Err(not_table(format!(
                "synthesized edge {id} is not a contracted table pair"
            )));
        }
        table
            .synthesized_fk_pairs
            .insert((edge.clone(), *src, *tgt));
    }
    table.id_floor = complement.id_floor.node;
    table.identity = Some(lens.identity(ComplementShape::Functor));
    Ok(table)
}

/// Convert a tree complement into graph coordinates under recorded source
/// and view embeddings.
///
/// Dropped graph objects are the exact set difference from `source` to
/// `view`; this includes reified fan nodes and edges, and includes a source
/// edge replaced by an expansion. Arc disambiguators, snapshots and
/// synthesized keys are translated field by field.
///
/// # Errors
///
/// [`LensError::NotTreeComplement`] when the view embedding does not name an
/// arc for which the tree complement records an original edge.
pub fn tree_complement_to_graph(
    lens: &Lens,
    source: &GInstance,
    view: &GInstance,
    complement: &Complement,
    source_embedding: &TreeEmbedding,
    view_embedding: &TreeEmbedding,
) -> Result<GraphComplement, LensError> {
    let mut graph = GraphComplement::empty();
    for (&id, node) in source.nodes() {
        if view.node(id).is_none() {
            graph.dropped_nodes.insert(id, node.clone());
        }
    }
    for (id, edge) in source.edges_sorted() {
        if view.edge(id).is_none() {
            graph.dropped_edges.insert(id, edge.clone());
        }
    }
    for (&(parent, child), edge) in &complement.arc_edges {
        let id = view_embedding.arcs.get(&child).copied().ok_or_else(|| {
            not_tree(format!(
                "view embedding has no arc ({parent}, {child}) with an original edge"
            ))
        })?;
        graph
            .original_edges
            .insert(id, GEdge::new(parent, child, edge.clone()));
    }
    graph.original_anchors = complement
        .original_anchors
        .iter()
        .map(|(&id, anchor)| (id, anchor.clone()))
        .collect();
    graph.original_extra_fields = complement
        .original_extra_fields
        .iter()
        .map(|(&id, fields)| {
            (
                id,
                fields
                    .iter()
                    .map(|(field, value)| (field.clone(), value.clone()))
                    .collect(),
            )
        })
        .collect();
    graph.original_values = complement
        .original_values
        .iter()
        .map(|(&id, value)| (id, value.clone()))
        .collect();
    let synth = complement.synth_index(view_embedding);
    graph.synthesized_nodes = synth.nodes;
    graph.synthesized_edges = synth.edges;
    graph.id_floor = IdFloor {
        node: complement.id_floor,
        edge: source_embedding.next_edge,
    };
    graph.identity = Some(lens.identity(ComplementShape::Graph));
    Ok(graph)
}

fn graph_edge<'a>(
    view: &'a GInstance,
    complement: &'a GraphComplement,
    id: EdgeId,
) -> Option<&'a GEdge> {
    complement.dropped_edges.get(&id).or_else(|| view.edge(id))
}

/// Convert a graph complement on a tree-shaped view into tree coordinates,
/// returning the source embedding obtained by extending the view embedding
/// with every dropped arc and fan edge.
///
/// # Errors
///
/// [`LensError::NotTreeComplement`] when a dropped non-fan edge gives a
/// second parent, a fan is malformed, or synthesized graph keys do not name
/// tree arcs.
#[allow(clippy::too_many_lines)]
pub fn graph_complement_to_tree(
    lens: &Lens,
    view: &GInstance,
    complement: &GraphComplement,
    view_embedding: &TreeEmbedding,
) -> Result<(Complement, TreeEmbedding), LensError> {
    let mut tree = Complement::empty();
    let fan_nodes: BTreeSet<u32> = view
        .nodes()
        .iter()
        .chain(complement.dropped_nodes.iter())
        .filter_map(|(&id, node)| (node.shape == NodeShape::HyperEdge).then_some(id))
        .collect();

    for (&id, node) in &complement.dropped_nodes {
        if !fan_nodes.contains(&id) {
            tree.dropped_nodes.insert(id, node.clone());
        }
    }

    let mut embedding = view_embedding.clone();
    let mut source_edges: Vec<(EdgeId, &GEdge)> = view
        .edges_sorted()
        .filter(|(id, _)| !complement.synthesized_edges.contains_key(id))
        .chain(
            complement
                .dropped_edges
                .iter()
                .map(|(&id, edge)| (id, edge)),
        )
        .collect();
    source_edges.sort_unstable_by_key(|(id, _)| *id);
    source_edges.dedup_by_key(|(id, _)| *id);
    for (id, edge) in &source_edges {
        if fan_edge(&edge.edge) || fan_nodes.contains(&edge.src) {
            continue;
        }
        if let Some(previous) = embedding.arcs.insert(edge.tgt, *id)
            && previous != *id
        {
            return Err(not_tree(format!(
                "node {} has graph edges {previous} and {id} as parents",
                edge.tgt
            )));
        }
        let original = complement.original_edges.get(id).unwrap_or(edge);
        if complement.dropped_edges.contains_key(id) {
            tree.dropped_arcs
                .push((original.src, original.tgt, original.edge.clone()));
        }
        tree.original_parent.insert(original.tgt, original.src);
        tree.arc_order.push((original.src, original.tgt));
        if complement.original_edges.contains_key(id) {
            tree.arc_edges
                .insert((original.src, original.tgt), original.edge.clone());
        }
    }

    let mut fan_embeddings: BTreeMap<u32, FanEmbedding> = embedding
        .fans
        .iter()
        .cloned()
        .map(|fan| (fan.node, fan))
        .collect();
    for fan_node in fan_nodes {
        let node = complement
            .dropped_nodes
            .get(&fan_node)
            .or_else(|| view.node(fan_node))
            .ok_or_else(|| not_tree(format!("fan node {fan_node} has no node record")))?;
        let hyper_edge_id = complement
            .original_anchors
            .get(&fan_node)
            .unwrap_or(&node.anchor)
            .to_string();
        let mut parent = None;
        let mut parent_edge = None;
        let mut children = HashMap::new();
        let mut child_edges = BTreeMap::new();
        let mut ids: BTreeSet<EdgeId> = view.out_edges(fan_node).iter().copied().collect();
        ids.extend(
            complement
                .dropped_edges
                .iter()
                .filter_map(|(&id, edge)| (edge.src == fan_node).then_some(id)),
        );
        for id in ids {
            let edge = graph_edge(view, complement, id)
                .ok_or_else(|| not_tree(format!("fan edge {id} has no edge record")))?;
            let original = complement.original_edges.get(&id).unwrap_or(edge);
            if original.edge.kind == HYPER_PARENT_KIND {
                if parent.replace(original.tgt).is_some() {
                    return Err(not_tree(format!(
                        "fan node {fan_node} has two parent edges"
                    )));
                }
                parent_edge = Some(id);
            } else if original.edge.kind == HYPER_LABEL_KIND {
                let label = original
                    .edge
                    .name
                    .as_deref()
                    .ok_or_else(|| not_tree(format!("fan edge {id} has no label")))?
                    .to_owned();
                if children.insert(label.clone(), original.tgt).is_some() {
                    return Err(not_tree(format!(
                        "fan node {fan_node} repeats label `{label}`"
                    )));
                }
                child_edges.insert(label, id);
            }
        }
        let parent =
            parent.ok_or_else(|| not_tree(format!("fan node {fan_node} has no parent")))?;
        let parent_edge = parent_edge
            .ok_or_else(|| not_tree(format!("fan node {fan_node} has no parent edge")))?;
        if complement.dropped_nodes.contains_key(&fan_node)
            || child_edges
                .values()
                .any(|id| complement.dropped_edges.contains_key(id))
        {
            tree.dropped_fans.push(Fan {
                hyper_edge_id,
                parent,
                children,
            });
        }
        fan_embeddings.insert(
            fan_node,
            FanEmbedding {
                node: fan_node,
                parent_edge,
                child_edges,
            },
        );
    }
    embedding.fans = fan_embeddings.into_values().collect();
    embedding.next_node = complement.id_floor.node;
    embedding.next_edge = complement.id_floor.edge;

    tree.source_fingerprint = legacy_schema_fingerprint(&lens.src_schema, LegacyWidth::Bits64);
    tree.original_extra_fields = complement
        .original_extra_fields
        .iter()
        .map(|(&id, fields)| {
            (
                id,
                fields
                    .iter()
                    .map(|(field, value)| (field.clone(), value.clone()))
                    .collect(),
            )
        })
        .collect();
    tree.original_values = complement
        .original_values
        .iter()
        .map(|(&id, value)| (id, value.clone()))
        .collect();
    tree.original_anchors = complement
        .original_anchors
        .iter()
        .filter(|(id, _)| !embedding.fans.iter().any(|fan| fan.node == **id))
        .map(|(&id, anchor)| (id, anchor.clone()))
        .collect();
    tree.synthesized_nodes = complement.synthesized_nodes.keys().copied().collect();
    tree.synthesized_keys = complement.synthesized_nodes.clone();
    tree.id_floor = complement.id_floor.node;
    tree.identity = Some(lens.identity(ComplementShape::WType));

    for (&id, key) in &complement.synthesized_edges {
        match key {
            panproto_inst::SynthKey::Contract { src, tgt, edge } => {
                tree.contraction_choices.insert((*src, *tgt), edge.clone());
                embedding.arcs.insert(*tgt, id);
            }
            panproto_inst::SynthKey::Expand { .. }
            | panproto_inst::SynthKey::ContractExpand { .. } => {
                let Some(edge) = view.edge(id) else {
                    return Err(not_tree(format!(
                        "synthesized edge {id} is absent from the view"
                    )));
                };
                embedding.arcs.insert(edge.tgt, id);
            }
        }
    }
    Ok((tree, embedding))
}

/// Run graph get through the tree engine when the complete direct bridge is
/// defined.
///
/// This is the candidate measured by the dispatch benchmark. `None` means
/// the call must use the graph engine: the lens does not preserve trees, the
/// graph is not one tree, or get synthesized structural arcs whose durable
/// edge embedding needs the graph allocator directly.
///
/// # Errors
///
/// Errors from the graph engine when a failed tree attempt falls back to it.
pub fn graph_get_via_tree(
    lens: &Lens,
    source: &GInstance,
) -> Result<Option<(GInstance, GraphComplement)>, LensError> {
    if !lens.preserves_tree_shape() {
        return Ok(None);
    }
    let Ok((tree, source_embedding)) = source.to_wtype_embedded(&lens.src_schema) else {
        return Ok(None);
    };
    let Ok((tree_view, tree_complement)) = crate::asymmetric::get(lens, &tree) else {
        return crate::graph_lens::get(lens, source).map(Some);
    };
    if !tree_complement.synthesized_keys.is_empty()
        || !tree_complement.contraction_choices.is_empty()
        || tree_view.fans.len() != tree.fans.len()
    {
        return Ok(None);
    }
    let view_embedding = source_embedding.clone();
    let Ok(view) = GInstance::from_wtype_under(&tree_view, &view_embedding, Some(&lens.tgt_schema))
    else {
        return Ok(None);
    };
    let Ok(graph_complement) = tree_complement_to_graph(
        lens,
        source,
        &view,
        &tree_complement,
        &source_embedding,
        &view_embedding,
    ) else {
        return crate::graph_lens::get(lens, source).map(Some);
    };
    Ok(Some((view, graph_complement)))
}

/// Run graph put through the tree engine when the complete bridge is
/// defined.
///
/// This is the put half of the benchmark candidate. `None` declines to the
/// graph engine; it is never a shape error exposed to a caller.
///
/// # Errors
///
/// Identity-verification errors, or errors from the graph engine when a
/// failed tree attempt falls back to it.
pub fn graph_put_via_tree(
    lens: &Lens,
    view: &GInstance,
    complement: &GraphComplement,
) -> Result<Option<GInstance>, LensError> {
    complement.verify(lens, IdentityPolicy::AllowLegacy)?;
    if crate::graph_lens::check_put_domain(lens, view, complement).is_err() {
        return crate::graph_lens::put(lens, view, complement).map(Some);
    }
    let Ok((tree_view, view_embedding)) = view.to_wtype_embedded(&lens.tgt_schema) else {
        return Ok(None);
    };
    let Ok((tree_complement, source_embedding)) =
        graph_complement_to_tree(lens, view, complement, &view_embedding)
    else {
        return crate::graph_lens::put(lens, view, complement).map(Some);
    };
    let Ok(tree_source) = crate::asymmetric::put(lens, &tree_view, &tree_complement) else {
        return crate::graph_lens::put(lens, view, complement).map(Some);
    };
    let Ok(source) =
        GInstance::from_wtype_under(&tree_source, &source_embedding, Some(&lens.src_schema))
    else {
        return Ok(None);
    };
    Ok(Some(source))
}

/// Run graph get through the table engine when the direct durable embedding
/// is defined.
///
/// `None` declines to the graph engine. Protocols are explicit because table
/// and column classification is a boundary operation.
///
/// # Errors
///
/// Errors from the graph engine when a failed table attempt falls back to it.
pub fn graph_get_via_table(
    lens: &Lens,
    source: &GInstance,
    protocols: BoundaryProtocols<'_>,
) -> Result<Option<(GInstance, GraphComplement)>, LensError> {
    if !lens.preserves_table_shape() {
        return Ok(None);
    }
    let Ok((tables, source_embedding)) = source.try_to_finstance(&lens.src_schema) else {
        return Ok(None);
    };
    let Ok((table_view, table_complement)) = crate::table_lens::get(lens, &tables, protocols)
    else {
        return crate::graph_lens::get(lens, source).map(Some);
    };
    if table_view.all_pairs().iter().any(|(edge, src, tgt)| {
        !source_embedding
            .pairs
            .contains_key(&(edge.clone(), *src, *tgt))
    }) {
        return Ok(None);
    }
    let mut view_embedding = source_embedding.clone();
    let Ok(view) = GInstance::from_finstance_along(&table_view, &mut view_embedding) else {
        return Ok(None);
    };
    let Ok(graph_complement) = table_complement_to_graph(
        lens,
        &table_view,
        &table_complement,
        &source_embedding,
        &view_embedding,
        protocols.src,
    ) else {
        return crate::graph_lens::get(lens, source).map(Some);
    };
    Ok(Some((view, graph_complement)))
}

/// Run graph put through the table engine when the direct durable embedding
/// is defined.
///
/// # Errors
///
/// Identity-verification errors, or errors from the graph engine when a
/// failed table attempt falls back to it. `None` declines to the graph
/// engine.
pub fn graph_put_via_table(
    lens: &Lens,
    view: &GInstance,
    complement: &GraphComplement,
    protocols: BoundaryProtocols<'_>,
) -> Result<Option<GInstance>, LensError> {
    complement.verify(lens, IdentityPolicy::AllowLegacy)?;
    if crate::graph_lens::check_put_domain(lens, view, complement).is_err() {
        return crate::graph_lens::put(lens, view, complement).map(Some);
    }
    let Ok((table_view, mut embedding)) = view.try_to_finstance(&lens.tgt_schema) else {
        return Ok(None);
    };
    let Ok(table_complement) =
        graph_complement_to_table(lens, &table_view, complement, &embedding, protocols.src)
    else {
        return crate::graph_lens::put(lens, view, complement).map(Some);
    };
    for (&id, edge) in &complement.dropped_edges {
        if fan_edge(&edge.edge) || edge.position.is_some() {
            return Ok(None);
        }
        if embedding
            .pairs
            .insert((edge.edge.clone(), RowId(edge.src), RowId(edge.tgt)), id)
            .is_some()
        {
            return Ok(None);
        }
    }
    embedding.next_edge = complement.id_floor.edge;
    let Ok(table_source) = crate::table_lens::put(lens, &table_view, &table_complement, protocols)
    else {
        return crate::graph_lens::put(lens, view, complement).map(Some);
    };
    let Ok(source) = GInstance::from_finstance_along(&table_source, &mut embedding) else {
        return Ok(None);
    };
    Ok(Some(source))
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet, HashMap};

    use panproto_inst::{CompiledMigration, IdFloor, SynthKey, WInstance};
    use panproto_schema::{Protocol, ScalarKind, Schema, SchemaBuilder};
    use proptest::prelude::*;

    use super::*;

    fn protocol() -> Protocol {
        Protocol {
            non_value_kinds: BTreeSet::from(["record".to_owned()]),
            value_kinds: BTreeMap::from([("value".to_owned(), ScalarKind::Any)]),
            ..Protocol::default()
        }
    }

    fn relation() -> Edge {
        Edge {
            src: "t".into(),
            tgt: "u".into(),
            kind: "fk".into(),
            name: Some("owner".into()),
        }
    }

    fn schema() -> Schema {
        let protocol = protocol();
        SchemaBuilder::new(&protocol)
            .vertex("t", "record", None)
            .and_then(|b| b.vertex("u", "record", None))
            .and_then(|b| b.vertex("t.a", "value", None))
            .and_then(|b| b.vertex("t.c", "value", None))
            .and_then(|b| b.edge("t", "t.a", "prop", Some("a")))
            .and_then(|b| b.edge("t", "t.c", "prop", Some("c")))
            .and_then(|b| b.edge("t", "u", "fk", Some("owner")))
            .and_then(SchemaBuilder::build)
            .unwrap()
    }

    fn lens() -> Lens {
        let schema = schema();
        Lens {
            src_schema: schema.clone(),
            tgt_schema: schema,
            compiled: CompiledMigration {
                surviving_verts: ["t".into(), "u".into(), "t.a".into(), "t.c".into()].into(),
                surviving_edges: [
                    Edge {
                        src: "t".into(),
                        tgt: "t.a".into(),
                        kind: "prop".into(),
                        name: Some("a".into()),
                    },
                    Edge {
                        src: "t".into(),
                        tgt: "t.c".into(),
                        kind: "prop".into(),
                        name: Some("c".into()),
                    },
                    relation(),
                ]
                .into(),
                ..CompiledMigration::default()
            },
        }
    }

    fn view() -> FInstance {
        FInstance::new()
            .with_rows(
                "t",
                [(RowId(1), HashMap::from([("a".to_owned(), Value::Int(9))]))],
            )
            .and_then(|view| view.with_rows("u", [(RowId(3), HashMap::new())]))
            .and_then(|view| view.with_foreign_key(relation(), [(RowId(1), RowId(3))]))
            .unwrap()
    }

    fn embeddings() -> (TableEmbedding, TableEmbedding) {
        let source = TableEmbedding {
            pairs: BTreeMap::from([
                ((relation(), RowId(1), RowId(2)), EdgeId(7)),
                ((relation(), RowId(1), RowId(3)), EdgeId(8)),
            ]),
            next_edge: 9,
        };
        let view = TableEmbedding {
            pairs: BTreeMap::from([((relation(), RowId(1), RowId(3)), EdgeId(8))]),
            next_edge: 9,
        };
        (source, view)
    }

    #[test]
    fn table_and_graph_complements_convert_both_ways() {
        let lens = lens();
        let view = view();
        let protocol = protocol();
        let (source_embedding, view_embedding) = embeddings();
        let relation = relation();
        let mut table = TableComplement::empty();
        table.dropped_rows.insert(
            RowId(2),
            DroppedRow {
                table: "u".into(),
                row: HashMap::new(),
            },
        );
        table
            .dropped_fk_pairs
            .entry(relation.clone())
            .or_default()
            .insert((RowId(1), RowId(2)));
        table
            .original_fk_edges
            .entry((relation.clone(), RowId(1), RowId(3)))
            .or_default()
            .insert(relation.clone());
        table
            .dropped_cells
            .insert((RowId(1), "c".to_owned()), Value::Int(2));
        table
            .original_cells
            .insert((RowId(1), "a".to_owned()), Value::Int(1));
        table
            .synthesized_fk_pairs
            .insert((relation, RowId(1), RowId(3)));
        table.id_floor = 4;
        table.identity = Some(lens.identity(ComplementShape::Functor));

        let graph = table_complement_to_graph(
            &lens,
            &view,
            &table,
            &source_embedding,
            &view_embedding,
            &protocol,
        )
        .unwrap();
        assert_eq!(
            graph.dropped_edges.keys().copied().collect::<Vec<_>>(),
            [EdgeId(7)]
        );
        assert_eq!(
            graph.original_extra_fields[&1],
            BTreeMap::from([
                ("a".to_owned(), Value::Int(1)),
                ("c".to_owned(), Value::Int(2))
            ])
        );
        let round_trip =
            graph_complement_to_table(&lens, &view, &graph, &view_embedding, &protocol).unwrap();
        assert_eq!(round_trip, table);
    }

    #[test]
    fn cell_residue_converts_by_difference_against_the_view_row() {
        let lens = lens();
        let view = view();
        let protocol = protocol();
        let (_, embedding) = embeddings();
        let mut graph = GraphComplement::empty();
        graph.original_extra_fields.insert(
            1,
            BTreeMap::from([
                ("a".to_owned(), Value::Int(1)),
                ("c".to_owned(), Value::Int(2)),
            ]),
        );
        graph.id_floor = IdFloor { node: 4, edge: 9 };
        let table = graph_complement_to_table(&lens, &view, &graph, &embedding, &protocol).unwrap();
        assert_eq!(
            table.original_cells.get(&(RowId(1), "a".to_owned())),
            Some(&Value::Int(1))
        );
        assert_eq!(
            table.dropped_cells.get(&(RowId(1), "c".to_owned())),
            Some(&Value::Int(2))
        );
    }

    #[test]
    fn a_fan_is_not_a_table_complement() {
        let lens = lens();
        let view = view();
        let protocol = protocol();
        let (_, embedding) = embeddings();
        let mut fan = GraphComplement::empty();
        fan.dropped_nodes
            .insert(5, Node::new(5, "h").with_shape(NodeShape::HyperEdge));
        assert!(matches!(
            graph_complement_to_table(&lens, &view, &fan, &embedding, &protocol),
            Err(LensError::NotTableComplement { .. })
        ));

        let mut synthesized = GraphComplement::empty();
        synthesized.synthesized_nodes.insert(
            5,
            SynthKey::Contract {
                src: 1,
                tgt: 3,
                edge: relation(),
            },
        );
        assert!(matches!(
            graph_complement_to_table(&lens, &view, &synthesized, &embedding, &protocol),
            Err(LensError::NotTableComplement { .. })
        ));
    }

    #[test]
    fn tree_and_graph_complements_convert_both_ways() {
        let lens = lens();
        let relation = relation();
        let source = GInstance::new()
            .with_node(Node::new(1, "t"))
            .with_node(Node::new(2, "u"))
            .with_edge_id(EdgeId(7), 1, 2, relation.clone());
        let view = GInstance::new().with_node(Node::new(1, "t"));
        let source_embedding = TreeEmbedding {
            arcs: BTreeMap::from([(2, EdgeId(7))]),
            fans: Vec::new(),
            next_node: 3,
            next_edge: 8,
        };
        let view_embedding = TreeEmbedding {
            arcs: BTreeMap::new(),
            fans: Vec::new(),
            next_node: 3,
            next_edge: 8,
        };
        let mut tree = Complement::empty();
        tree.dropped_nodes.insert(2, Node::new(2, "u"));
        tree.dropped_arcs.push((1, 2, relation.clone()));
        tree.id_floor = 3;
        tree.identity = Some(lens.identity(ComplementShape::WType));

        let graph = tree_complement_to_graph(
            &lens,
            &source,
            &view,
            &tree,
            &source_embedding,
            &view_embedding,
        )
        .unwrap();
        assert_eq!(graph.dropped_nodes[&2].anchor, "u");
        assert_eq!(graph.dropped_edges[&EdgeId(7)].edge, relation);

        let (round_trip, result_embedding) =
            graph_complement_to_tree(&lens, &view, &graph, &view_embedding).unwrap();
        assert_eq!(round_trip.dropped_nodes[&2].anchor, "u");
        assert_eq!(round_trip.dropped_arcs, tree.dropped_arcs);
        assert_eq!(result_embedding.arcs[&2], EdgeId(7));
    }

    #[test]
    fn graph_fan_residue_converts_to_a_dropped_tree_fan() {
        let lens = lens();
        let tree = WInstance::new(
            HashMap::from([(0, Node::new(0, "t")), (1, Node::new(1, "u"))]),
            vec![],
            vec![Fan::new("membership", 0).with_child("member", 1)],
            0,
            "t".into(),
        );
        let (source, source_embedding) = GInstance::from_wtype_embedded(&tree, None).unwrap();
        let fan = source_embedding.fans[0].clone();
        let view = GInstance::new()
            .with_node(Node::new(0, "t"))
            .with_node(Node::new(1, "u"));
        let view_embedding = TreeEmbedding {
            arcs: BTreeMap::new(),
            fans: Vec::new(),
            next_node: source_embedding.next_node,
            next_edge: source_embedding.next_edge,
        };
        let mut graph = GraphComplement::empty();
        graph
            .dropped_nodes
            .insert(fan.node, source.node(fan.node).unwrap().clone());
        graph.dropped_edges.insert(
            fan.parent_edge,
            source.edge(fan.parent_edge).unwrap().clone(),
        );
        for edge in fan.child_edges.values() {
            graph
                .dropped_edges
                .insert(*edge, source.edge(*edge).unwrap().clone());
        }
        graph.id_floor = IdFloor {
            node: source_embedding.next_node,
            edge: source_embedding.next_edge,
        };

        let (converted, embedding) =
            graph_complement_to_tree(&lens, &view, &graph, &view_embedding).unwrap();
        assert_eq!(converted.dropped_fans, tree.fans);
        assert_eq!(embedding.fans, source_embedding.fans);
    }

    #[test]
    fn the_direct_tree_get_and_put_square_is_identical() {
        use crate::lens_shape::{GraphShape, LensShape};

        let lens = lens();
        let source = GInstance::new()
            .with_node(Node::new(1, "t"))
            .with_node(Node::new(3, "u"))
            .with_edge_id(EdgeId(8), 1, 3, relation());
        let (graph_view, graph_complement) = crate::graph_lens::get(&lens, &source).unwrap();
        let (tree_view, tree_complement) = graph_get_via_tree(&lens, &source).unwrap().unwrap();
        assert!(GInstance::identical(&tree_view, &graph_view));
        assert_eq!(
            GraphShape::complement_divergence(&tree_complement, &graph_complement),
            None
        );

        let graph_source = crate::graph_lens::put(&lens, &graph_view, &graph_complement).unwrap();
        let tree_source = graph_put_via_tree(&lens, &graph_view, &graph_complement)
            .unwrap()
            .unwrap();
        assert!(GInstance::identical(&tree_source, &graph_source));
    }

    #[test]
    fn the_direct_table_get_and_put_square_is_identical() {
        use crate::lens_shape::{GraphShape, LensShape};

        let lens = lens();
        let protocol = protocol();
        let protocols = BoundaryProtocols {
            src: &protocol,
            tgt: &protocol,
        };
        let tables = FInstance::new()
            .with_rows(
                "t",
                [(
                    RowId(1),
                    HashMap::from([
                        ("a".to_owned(), Value::Int(1)),
                        ("c".to_owned(), Value::Int(2)),
                    ]),
                )],
            )
            .and_then(|source| source.with_rows("u", [(RowId(3), HashMap::new())]))
            .and_then(|source| source.with_foreign_key(relation(), [(RowId(1), RowId(3))]))
            .unwrap();
        let (source, _) = GInstance::from_finstance(&tables).unwrap();
        let (graph_view, graph_complement) = crate::graph_lens::get(&lens, &source).unwrap();
        let (table_view, table_complement) = graph_get_via_table(&lens, &source, protocols)
            .unwrap()
            .unwrap();
        assert!(GInstance::identical(&table_view, &graph_view));
        assert_eq!(
            GraphShape::complement_divergence(&table_complement, &graph_complement),
            None
        );

        let graph_source = crate::graph_lens::put(&lens, &graph_view, &graph_complement).unwrap();
        let table_source = graph_put_via_table(&lens, &graph_view, &graph_complement, protocols)
            .unwrap()
            .unwrap();
        assert!(GInstance::identical(&table_source, &graph_source));
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]

        #[test]
        fn the_direct_tree_square_is_identical_for_arbitrary_distinct_ids(
            first in any::<u16>(),
            second in any::<u16>(),
            edge_id in any::<u16>(),
        ) {
            use crate::lens_shape::{GraphShape, LensShape};

            prop_assume!(first != second);
            let first = u32::from(first);
            let second = u32::from(second);
            let source = GInstance::new()
                .with_node(Node::new(first, "t"))
                .with_node(Node::new(second, "u"))
                .with_edge_id(EdgeId(u32::from(edge_id)), first, second, relation());
            let lens = lens();
            let (graph_view, graph_complement) = crate::graph_lens::get(&lens, &source).unwrap();
            let (tree_view, tree_complement) = graph_get_via_tree(&lens, &source)
                .unwrap()
                .expect("the source is a tree");
            prop_assert!(GInstance::identical(&tree_view, &graph_view));
            prop_assert_eq!(
                GraphShape::complement_divergence(&tree_complement, &graph_complement),
                None
            );
            let graph_source =
                crate::graph_lens::put(&lens, &graph_view, &graph_complement).unwrap();
            let tree_source = graph_put_via_tree(&lens, &graph_view, &graph_complement)
                .unwrap()
                .expect("the view is a tree");
            prop_assert!(GInstance::identical(&tree_source, &graph_source));
        }

        #[test]
        fn the_direct_table_square_is_identical_for_arbitrary_rows(
            first in any::<u16>(),
            second in any::<u16>(),
            a in any::<i16>(),
            c in any::<i16>(),
        ) {
            use crate::lens_shape::{GraphShape, LensShape};

            prop_assume!(first != second);
            let first = RowId(u32::from(first));
            let second = RowId(u32::from(second));
            let tables = FInstance::new()
                .with_rows(
                    "t",
                    [(
                        first,
                        HashMap::from([
                            ("a".to_owned(), Value::Int(i64::from(a))),
                            ("c".to_owned(), Value::Int(i64::from(c))),
                        ]),
                    )],
                )
                .and_then(|source| source.with_rows("u", [(second, HashMap::new())]))
                .and_then(|source| source.with_foreign_key(relation(), [(first, second)]))
                .unwrap();
            let (source, _) = GInstance::from_finstance(&tables).unwrap();
            let lens = lens();
            let protocol = protocol();
            let protocols = BoundaryProtocols {
                src: &protocol,
                tgt: &protocol,
            };
            let (graph_view, graph_complement) = crate::graph_lens::get(&lens, &source).unwrap();
            let (table_view, table_complement) = graph_get_via_table(&lens, &source, protocols)
                .unwrap()
                .expect("the source is a table");
            prop_assert!(GInstance::identical(&table_view, &graph_view));
            prop_assert_eq!(
                GraphShape::complement_divergence(&table_complement, &graph_complement),
                None
            );
            let graph_source =
                crate::graph_lens::put(&lens, &graph_view, &graph_complement).unwrap();
            let table_source =
                graph_put_via_table(&lens, &graph_view, &graph_complement, protocols)
                    .unwrap()
                    .expect("the view is a table");
            prop_assert!(GInstance::identical(&table_source, &graph_source));
        }
    }
}
