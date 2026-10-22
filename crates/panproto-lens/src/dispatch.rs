//! Shape bridges used by graph-operation dispatch and its agreement tests.
//!
//! Graph restrict and extend always run their graph engines. Lens get and
//! put may use a shape-specific engine only after a release benchmark shows
//! that the complete conversion is faster. The conversions here are kept
//! independently of that optimisation decision: they state how table
//! complement coordinates sit inside graph complement coordinates.
//!
//! The query-side commuting square follows the same restricted-fragment
//! pattern as `fanyudingqinrastogi2009xpathsql`, Corollary 5.1, and
//! `he2025graphiti`, Theorem 5.7. The table squares are exact up to the
//! row-and-pair isomorphism of `spivakwisnesky2015relationalarxiv`, Section 5;
//! the durable embeddings below choose concrete representatives.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, HashMap};

use panproto_gat::Name;
use panproto_inst::identity::{LegacyWidth, legacy_schema_fingerprint};
use panproto_inst::{
    BoundaryProtocols, Complement, ComplementShape, DroppedRow, EdgeFate, EdgeId, FInstance, Fan,
    FanEmbedding, GEdge, GInstance, GraphComplement, HYPER_LABEL_KIND, HYPER_PARENT_KIND, IdFloor,
    IdentityPolicy, Node, NodeShape, PairResolution, RelSchema, RestrictPlan, RowId, SynthIndex,
    SynthKey, TableComplement, TableEmbedding, TreeEmbedding, Value,
};
use panproto_schema::{Cardinality, Edge, Protocol};

use crate::{
    ComplementIdentity, Lens, LensError, NotTableComplementReason, NotTreeComplementReason,
};

const fn not_table(reason: NotTableComplementReason) -> LensError {
    LensError::NotTableComplement { reason }
}

const fn not_tree(reason: NotTreeComplementReason) -> LensError {
    LensError::NotTreeComplement { reason }
}

const fn nested(value: &Value) -> bool {
    matches!(value, Value::List(_) | Value::Unknown(_))
}

fn row_reason(id: u32, node: &Node) -> Option<NotTableComplementReason> {
    if node.value.is_some() {
        return Some(NotTableComplementReason::NodeValue { node: id });
    }
    if node.discriminator.is_some() {
        return Some(NotTableComplementReason::NodeDiscriminator { node: id });
    }
    if node.position.is_some() {
        return Some(NotTableComplementReason::NodePosition { node: id });
    }
    if node.shape != NodeShape::Plain {
        return Some(NotTableComplementReason::FanNode { node: id });
    }
    if !node.annotations.is_empty() {
        return Some(NotTableComplementReason::NodeAnnotations { node: id });
    }
    node.extra_fields.iter().find_map(|(column, value)| {
        nested(value).then(|| NotTableComplementReason::NestedCell {
            node: id,
            column: column.clone(),
        })
    })
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

/// Whether the protocol-indexed table plan leaves graph fields unchanged.
///
/// Protocol-free graph restriction cannot enact a column drop or rename, so
/// either fate lies outside the exact common domain of the graph/table square.
fn table_cells_are_graph_neutral(plan: &RestrictPlan) -> bool {
    plan.boundary
        .as_ref()
        .and_then(|boundary| boundary.cells.as_ref())
        .is_some_and(|cells| cells.fates.is_empty())
}

impl Lens {
    /// Whether restricting a tree-shaped graph preserves its tree result.
    ///
    /// A dropped source edge between two surviving vertices is the sole
    /// structural obstruction: the graph engine drops that edge alone,
    /// while the tree engine must drop the child subtree. A plan that does
    /// not compile is conservatively classified as shape-breaking.
    #[must_use]
    pub fn preserves_tree_shape(&self) -> bool {
        RestrictPlan::compile(&self.src_schema, &self.tgt_schema, &self.compiled)
            .is_ok_and(|plan| plan.preserves_tree_shape())
    }

    /// Whether this lens preserves the graph encoding of table instances.
    ///
    /// This protocol-free predicate applies every structural exclusion that
    /// can be decided from the schemas and compiled migration. Entry points
    /// that carry protocols must additionally compile a table plan before
    /// dispatch, since table and column classification is protocol-defined.
    #[must_use]
    pub fn preserves_table_shape(&self) -> bool {
        let Ok(plan) = RestrictPlan::compile(&self.src_schema, &self.tgt_schema, &self.compiled)
        else {
            return false;
        };
        self.preserves_table_shape_with_plan(&plan)
    }

    fn preserves_table_shape_with_plan(&self, plan: &RestrictPlan) -> bool {
        if plan
            .edge_image
            .fates
            .values()
            .any(|fate| matches!(fate, EdgeFate::Expand { .. } | EdgeFate::Unresolved(_)))
            || plan
                .contraction
                .pairs
                .values()
                .any(|pair| matches!(pair.resolution, PairResolution::Expand { .. }))
            || !self.compiled.hyper_resolver.is_empty()
        {
            return false;
        }

        let mut edge_images = BTreeSet::new();
        for image in self.compiled.edge_remap.values() {
            if !edge_images.insert(image) {
                return false;
            }
        }

        // The restrict survival rule keeps an unmentioned source table whose
        // name is also the image of another table; EntityFunctor deliberately
        // does not. Such a lens cannot enter the table fast path.
        let mut vertex_fibres: BTreeMap<Name, Vec<Name>> = BTreeMap::new();
        for source in self.src_schema.vertices.keys() {
            let image = self.compiled.vertex_remap.get(source).unwrap_or(source);
            if !self.compiled.surviving_verts.contains(image) {
                continue;
            }
            vertex_fibres
                .entry(image.clone())
                .or_default()
                .push(source.clone());
        }
        if vertex_fibres.iter().any(|(image, fibre)| {
            fibre.len() > 1
                && fibre.iter().any(|source| {
                    source == image && !self.compiled.vertex_remap.contains_key(source)
                })
        }) {
            return false;
        }

        // Likewise, a structurally inferred edge image is not an image in
        // EntityFunctor. Protocol-aware dispatch will run its stronger plan
        // check, but this catches the disagreement without classifications.
        if plan.edge_image.fates.iter().any(|(source, fate)| {
            matches!(fate, EdgeFate::Keep(_))
                && !self.compiled.edge_remap.contains_key(source)
                && !self.compiled.surviving_edges.contains(source)
        }) {
            return false;
        }

        self.is_discrete_opfibration(plan, &vertex_fibres)
    }

    fn is_discrete_opfibration(
        &self,
        plan: &RestrictPlan,
        fibres: &BTreeMap<Name, Vec<Name>>,
    ) -> bool {
        let mut guarded: BTreeSet<Edge> = self
            .tgt_schema
            .required
            .values()
            .flatten()
            .cloned()
            .collect();
        guarded.extend(
            self.tgt_schema
                .declarations
                .references
                .values()
                .filter(|reference| reference.cardinality == Cardinality::One)
                .filter_map(|reference| reference.edge.clone()),
        );

        for (image, fibre) in fibres {
            for target_edge in guarded.iter().filter(|edge| edge.src == *image) {
                for source_vertex in fibre {
                    let lifts = self
                        .src_schema
                        .outgoing
                        .get(source_vertex)
                        .into_iter()
                        .flatten()
                        .filter(|edge| {
                            matches!(
                                plan.edge_image.fates.get(*edge),
                                Some(EdgeFate::Keep(image)) if image == target_edge
                            )
                        })
                        .count();
                    if lifts != 1 {
                        return false;
                    }
                }
            }
        }
        true
    }
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
    let source_rel = RelSchema::new(&lens.src_schema, source_protocol).map_err(|error| {
        not_table(NotTableComplementReason::RelSchema {
            detail: error.to_string(),
        })
    })?;
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
                    not_table(NotTableComplementReason::MissingDroppedPair {
                        edge: Box::new(edge.clone()),
                        src,
                        tgt,
                    })
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
            .ok_or_else(|| {
                not_table(NotTableComplementReason::MissingOriginalPair {
                    edge: Box::new(view_edge.clone()),
                    src: *src,
                    tgt: *tgt,
                })
            })?;
        let originals: Vec<&Edge> = originals.iter().collect();
        let [original] = originals.as_slice() else {
            return Err(not_table(
                NotTableComplementReason::MultipleOriginalForeignKeys {
                    count: originals.len(),
                },
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
            .ok_or_else(|| {
                not_table(NotTableComplementReason::UndeterminedSourceTable { row: id })
            })?;
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

fn copy_graph_table_elements(
    complement: &GraphComplement,
    embedding: &TableEmbedding,
    table: &mut TableComplement,
) -> Result<(), LensError> {
    for (&id, node) in &complement.dropped_nodes {
        if let Some(reason) = row_reason(id, node) {
            return Err(not_table(reason));
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
        if edge.position.is_some() {
            return Err(not_table(NotTableComplementReason::EdgePosition {
                edge: id,
            }));
        }
        if fan_edge(&edge.edge) {
            return Err(not_table(NotTableComplementReason::FanEdge { edge: id }));
        }
        table
            .dropped_fk_pairs
            .entry(edge.edge.clone())
            .or_default()
            .insert((RowId(edge.src), RowId(edge.tgt)));
    }
    for (&id, original) in &complement.original_edges {
        if original.position.is_some() {
            return Err(not_table(NotTableComplementReason::EdgePosition {
                edge: id,
            }));
        }
        if fan_edge(&original.edge) {
            return Err(not_table(NotTableComplementReason::FanEdge { edge: id }));
        }
        let (view_edge, src, tgt) = pair_for_id(embedding, id)
            .ok_or_else(|| not_table(NotTableComplementReason::MissingEmbeddedEdge { edge: id }))?;
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
    Ok(())
}

fn copy_graph_table_cells(
    lens: &Lens,
    view: &FInstance,
    complement: &GraphComplement,
    source_rel: &RelSchema,
    table: &mut TableComplement,
) -> Result<(), LensError> {
    for (&id, snapshot) in &complement.original_extra_fields {
        let row = RowId(id);
        let view_table = view
            .table_of(row)
            .ok_or_else(|| not_table(NotTableComplementReason::MissingViewRow { row }))?;
        let source_table = source_table(lens, view_table, complement.original_anchors.get(&id))
            .ok_or_else(|| not_table(NotTableComplementReason::UndeterminedSourceTable { row }))?;
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
    Ok(())
}

fn copy_graph_table_synthesized_edges(
    complement: &GraphComplement,
    embedding: &TableEmbedding,
    table: &mut TableComplement,
) -> Result<(), LensError> {
    for (&id, key) in &complement.synthesized_edges {
        let (edge, src, tgt) = pair_for_id(embedding, id).ok_or_else(|| {
            not_table(NotTableComplementReason::MissingSynthesizedEdge { edge: id })
        })?;
        let expected = panproto_inst::SynthKey::Contract {
            src: src.0,
            tgt: tgt.0,
            edge: edge.clone(),
        };
        if *key != expected {
            return Err(not_table(
                NotTableComplementReason::InvalidSynthesizedEdge { edge: id },
            ));
        }
        table
            .synthesized_fk_pairs
            .insert((edge.clone(), *src, *tgt));
    }
    Ok(())
}

/// Convert a graph complement on a table-shaped view into table coordinates.
///
/// Whole-map graph field snapshots are differenced against `view` over the
/// columns of the row's source table: an absent same-named view cell becomes
/// `dropped_cells`, while a differing one becomes `original_cells`.
/// This conversion and [`table_complement_to_graph`] are a functional witness
/// of lens equivalence, modulo the supplied embedding, in the sense of
/// `hofmann2011symmetric`, Definitions 3.1--3.2 (coincidence with
/// observational equivalence is Theorem 3.6 of the full version), and
/// `abousaleh2017coalgebraic`, Definition 3.2. The field-by-field recipe is
/// specific to these complement representations.
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
    if let Some(&node) = complement.synthesized_nodes.keys().next() {
        return Err(not_table(NotTableComplementReason::SynthesizedNode {
            node,
        }));
    }
    let source_rel = RelSchema::new(&lens.src_schema, source_protocol).map_err(|error| {
        not_table(NotTableComplementReason::RelSchema {
            detail: error.to_string(),
        })
    })?;
    let mut table = TableComplement::empty();
    copy_graph_table_elements(complement, embedding, &mut table)?;
    copy_graph_table_cells(lens, view, complement, &source_rel, &mut table)?;
    copy_graph_table_synthesized_edges(complement, embedding, &mut table)?;
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
    let synth = tree_synth_index(complement, source_embedding, view_embedding)?;
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
            not_tree(NotTreeComplementReason::MissingEmbeddedArc { parent, child })
        })?;
        if !synth.edges.contains_key(&id) {
            graph
                .original_edges
                .insert(id, GEdge::new(parent, child, edge.clone()));
        }
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
    graph.synthesized_nodes = synth.nodes;
    graph.synthesized_edges = synth.edges;
    graph.id_floor = IdFloor {
        node: complement.id_floor,
        edge: source_embedding.next_edge,
    };
    graph.identity = Some(lens.identity(ComplementShape::Graph));
    Ok(graph)
}

fn source_edge_for_tree_key(
    complement: &Complement,
    source_embedding: &TreeEmbedding,
    edge: EdgeId,
) -> Option<EdgeId> {
    let index = usize::try_from(edge.0).ok()?;
    let &(_, child) = complement.arc_order.get(index)?;
    source_embedding.arcs.get(&child).copied()
}

fn translate_tree_key(
    complement: &Complement,
    source_embedding: &TreeEmbedding,
    key: &SynthKey,
) -> Result<SynthKey, LensError> {
    match key {
        SynthKey::Expand { source_edge, hop } => Ok(SynthKey::Expand {
            source_edge: source_edge_for_tree_key(complement, source_embedding, *source_edge)
                .ok_or_else(|| {
                    not_tree(NotTreeComplementReason::MissingSourceArc { edge: *source_edge })
                })?,
            hop: *hop,
        }),
        _ => Ok(key.clone()),
    }
}

fn tree_synth_index(
    complement: &Complement,
    source_embedding: &TreeEmbedding,
    view_embedding: &TreeEmbedding,
) -> Result<SynthIndex, LensError> {
    let raw = complement.synth_index(view_embedding);
    let nodes: BTreeMap<u32, SynthKey> = raw
        .nodes
        .iter()
        .map(|(&node, key)| Ok((node, translate_tree_key(complement, source_embedding, key)?)))
        .collect::<Result<_, LensError>>()?;
    let mut edges: BTreeMap<EdgeId, SynthKey> = raw
        .edges
        .iter()
        .map(|(&edge, key)| Ok((edge, translate_tree_key(complement, source_embedding, key)?)))
        .collect::<Result<_, LensError>>()?;

    // `Complement::contraction_choices` also records expansion hops so tree
    // put can reject edits to the presentation. Its generic synth index sees
    // those arcs last and calls them contractions. In graph coordinates the
    // keys must instead retain the graph engine's `Expand`/`ContractExpand`
    // identity, with source-arc indices translated through the durable
    // source embedding.
    let mut last_hop: BTreeMap<SynthKey, (u16, u32)> = BTreeMap::new();
    for (&node, key) in &nodes {
        let (chain, hop) = match key {
            SynthKey::Expand { source_edge, hop } => (
                SynthKey::Expand {
                    source_edge: *source_edge,
                    hop: 0,
                },
                *hop,
            ),
            SynthKey::ContractExpand { src, tgt, hop } => (
                SynthKey::ContractExpand {
                    src: *src,
                    tgt: *tgt,
                    hop: 0,
                },
                *hop,
            ),
            _ => continue,
        };
        let edge =
            view_embedding.arcs.get(&node).copied().ok_or_else(|| {
                not_tree(NotTreeComplementReason::MissingSynthesizedNodeArc { node })
            })?;
        edges.insert(edge, key.clone());
        let entry = last_hop.entry(chain).or_insert((hop, node));
        if hop >= entry.0 {
            *entry = (hop, node);
        }
    }
    for (chain, (hop, _)) in last_hop {
        let Some(next) = hop.checked_add(1) else {
            continue;
        };
        let (endpoint, key) = match chain {
            SynthKey::Expand { source_edge, .. } => {
                let child = source_embedding
                    .arcs
                    .iter()
                    .find_map(|(&child, &edge)| (edge == source_edge).then_some(child));
                (
                    child,
                    SynthKey::Expand {
                        source_edge,
                        hop: next,
                    },
                )
            }
            SynthKey::ContractExpand { src, tgt, .. } => (
                Some(tgt),
                SynthKey::ContractExpand {
                    src,
                    tgt,
                    hop: next,
                },
            ),
            _ => (None, chain),
        };
        if let Some(edge) = endpoint.and_then(|child| view_embedding.arcs.get(&child)) {
            edges.insert(*edge, key);
        }
    }

    Ok(SynthIndex { nodes, edges })
}

fn graph_edge<'a>(
    view: &'a GInstance,
    complement: &'a GraphComplement,
    id: EdgeId,
) -> Option<&'a GEdge> {
    complement.dropped_edges.get(&id).or_else(|| view.edge(id))
}

fn graph_tree_edges<'a>(
    view: &'a GInstance,
    complement: &'a GraphComplement,
) -> Vec<(EdgeId, &'a GEdge)> {
    let mut edges: Vec<(EdgeId, &GEdge)> = view
        .edges_sorted()
        .filter(|(id, _)| !complement.synthesized_edges.contains_key(id))
        .chain(
            complement
                .dropped_edges
                .iter()
                .map(|(&id, edge)| (id, edge)),
        )
        .collect();
    edges.sort_unstable_by_key(|(id, _)| *id);
    edges.dedup_by_key(|(id, _)| *id);
    edges
}

fn add_tree_arcs(
    tree: &mut Complement,
    embedding: &mut TreeEmbedding,
    fan_nodes: &BTreeSet<u32>,
    complement: &GraphComplement,
    edges: &[(EdgeId, &GEdge)],
) -> Result<(), LensError> {
    for (id, edge) in edges {
        if fan_edge(&edge.edge) || fan_nodes.contains(&edge.src) {
            continue;
        }
        if let Some(previous) = embedding.arcs.insert(edge.tgt, *id)
            && previous != *id
        {
            return Err(not_tree(NotTreeComplementReason::MultipleParents {
                node: edge.tgt,
                first: previous,
                second: *id,
            }));
        }
        let original = complement.original_edges.get(id).unwrap_or(edge);
        if complement.dropped_edges.contains_key(id) {
            tree.dropped_arcs
                .push((original.src, original.tgt, original.edge.clone()));
        }
        // Native tree `get` records an original parent only for a node that
        // survived in the view produced by get. Use the complement rather
        // than the caller's current (possibly edited) view to recover that
        // get-time fact. A dropped child is restored from `dropped_arcs`;
        // marking it as surviving makes tree `put` suppress that residue.
        if !complement.dropped_nodes.contains_key(&original.tgt) {
            tree.original_parent.insert(original.tgt, original.src);
        }
        tree.arc_order.push((original.src, original.tgt));
        if complement.original_edges.contains_key(id) {
            tree.arc_edges
                .insert((original.src, original.tgt), original.edge.clone());
        }
    }
    Ok(())
}

fn graph_fan(
    fan_node: u32,
    view: &GInstance,
    complement: &GraphComplement,
) -> Result<(Fan, FanEmbedding, bool), LensError> {
    let node = complement
        .dropped_nodes
        .get(&fan_node)
        .or_else(|| view.node(fan_node))
        .ok_or_else(|| not_tree(NotTreeComplementReason::MissingFanNode { node: fan_node }))?;
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
            .ok_or_else(|| not_tree(NotTreeComplementReason::MissingFanEdge { edge: id }))?;
        let original = complement.original_edges.get(&id).unwrap_or(edge);
        if original.edge.kind == HYPER_PARENT_KIND {
            if parent.replace(original.tgt).is_some() {
                return Err(not_tree(NotTreeComplementReason::MultipleFanParents {
                    node: fan_node,
                }));
            }
            parent_edge = Some(id);
        } else if original.edge.kind == HYPER_LABEL_KIND {
            let label = original
                .edge
                .name
                .as_deref()
                .ok_or_else(|| not_tree(NotTreeComplementReason::MissingFanLabel { edge: id }))?
                .to_owned();
            if children.insert(label.clone(), original.tgt).is_some() {
                return Err(not_tree(NotTreeComplementReason::DuplicateFanLabel {
                    node: fan_node,
                    label,
                }));
            }
            child_edges.insert(label, id);
        }
    }
    let parent = parent
        .ok_or_else(|| not_tree(NotTreeComplementReason::MissingFanParent { node: fan_node }))?;
    let parent_edge = parent_edge.ok_or_else(|| {
        not_tree(NotTreeComplementReason::MissingFanParentEdge { node: fan_node })
    })?;
    let dropped = complement.dropped_nodes.contains_key(&fan_node)
        || child_edges
            .values()
            .any(|id| complement.dropped_edges.contains_key(id));
    Ok((
        Fan {
            hyper_edge_id,
            parent,
            children,
        },
        FanEmbedding {
            node: fan_node,
            parent_edge,
            child_edges,
        },
        dropped,
    ))
}

fn add_tree_fans(
    tree: &mut Complement,
    embedding: &mut TreeEmbedding,
    fan_nodes: &BTreeSet<u32>,
    view: &GInstance,
    complement: &GraphComplement,
) -> Result<(), LensError> {
    let mut fan_embeddings: BTreeMap<u32, FanEmbedding> = embedding
        .fans
        .iter()
        .cloned()
        .map(|fan| (fan.node, fan))
        .collect();
    for &fan_node in fan_nodes {
        let (fan, ids, dropped) = graph_fan(fan_node, view, complement)?;
        if dropped {
            tree.dropped_fans.push(fan);
        }
        fan_embeddings.insert(fan_node, ids);
    }
    embedding.fans = fan_embeddings.into_values().collect();
    Ok(())
}

fn add_synthesized_tree_arcs(
    tree: &mut Complement,
    embedding: &mut TreeEmbedding,
    view: &GInstance,
    complement: &GraphComplement,
) -> Result<(), LensError> {
    for (&id, key) in &complement.synthesized_edges {
        match key {
            panproto_inst::SynthKey::Contract { src, tgt, edge } => {
                tree.contraction_choices.insert((*src, *tgt), edge.clone());
                embedding.arcs.insert(*tgt, id);
            }
            panproto_inst::SynthKey::Expand { .. }
            | panproto_inst::SynthKey::ContractExpand { .. } => {
                let Some(edge) = view.edge(id) else {
                    return Err(not_tree(NotTreeComplementReason::MissingSynthesizedEdge {
                        edge: id,
                    }));
                };
                embedding.arcs.insert(edge.tgt, id);
            }
            panproto_inst::SynthKey::ResolutionReference { .. }
            | panproto_inst::SynthKey::ResolutionSpan { .. } => {
                return Err(not_tree(
                    NotTreeComplementReason::ResolutionSynthesizedEdge { edge: id },
                ));
            }
        }
    }
    Ok(())
}

/// Convert a graph complement on a tree-shaped view into tree coordinates,
/// returning the source embedding obtained by extending the view embedding
/// with every dropped arc and fan edge.
///
/// Together with [`tree_complement_to_graph`], this is a functional witness
/// of lens equivalence modulo the durable embedding (`hofmann2011symmetric`,
/// Definitions 3.1--3.2 and full-version Theorem 3.6;
/// `abousaleh2017coalgebraic`, Definition 3.2). The field-by-field recipe is
/// specific to these complement representations.
///
/// # Errors
///
/// [`LensError::NotTreeComplement`] when a dropped non-fan edge gives a
/// second parent, a fan is malformed, or synthesized graph keys do not name
/// tree arcs.
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
    add_tree_arcs(
        &mut tree,
        &mut embedding,
        &fan_nodes,
        complement,
        &graph_tree_edges(view, complement),
    )?;
    add_tree_fans(&mut tree, &mut embedding, &fan_nodes, view, complement)?;
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

    add_synthesized_tree_arcs(&mut tree, &mut embedding, view, complement)?;
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
    let (tree_view, tree_complement) = match crate::asymmetric::get(lens, &tree) {
        Ok(result) => result,
        Err(LensError::Restrict(panproto_inst::RestrictError::RootPruned)) => return Ok(None),
        Err(_) => return crate::graph_lens::get(lens, source).map(Some),
    };
    if tree_view.fans.len() != tree.fans.len() {
        return Ok(None);
    }
    let mut view_embedding = source_embedding.clone();
    let source_parents: HashMap<u32, u32> = tree
        .arcs
        .iter()
        .map(|&(parent, child, _)| (child, parent))
        .collect();
    let mut arcs = BTreeMap::new();
    let mut last = None;
    for &(parent, child, _) in &tree_view.arcs {
        // The tree bridge's survival relation for an arc is its parent/child
        // incidence, not its schema label: a migration may relabel an
        // otherwise surviving arc. This follows the arc rule of
        // `TreeEmbedding::observe_edit`. Its order guard retains an old id
        // only while ascending edge ids still present the tree's child order;
        // reparented, inserted, or reordered arcs receive fresh ids above the
        // source embedding's floor.
        let retained = source_embedding.arcs.get(&child).copied().filter(|id| {
            source_parents.get(&child) == Some(&parent) && last.is_none_or(|old| *id > old)
        });
        let edge = if let Some(edge) = retained {
            edge
        } else {
            let Ok(edge) = view_embedding.allocate_edge() else {
                return crate::graph_lens::get(lens, source).map(Some);
            };
            edge
        };
        arcs.insert(child, edge);
        last = Some(edge);
    }
    view_embedding.arcs = arcs;
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
    let Ok(plan) = RestrictPlan::compile(&lens.src_schema, &lens.tgt_schema, &lens.compiled) else {
        return Ok(None);
    };
    if !lens.preserves_table_shape_with_plan(&plan) {
        return Ok(None);
    }
    let Ok(table_plan) = RestrictPlan::compile_for_tables(
        &lens.src_schema,
        &lens.tgt_schema,
        &lens.compiled,
        protocols,
    ) else {
        return crate::graph_lens::get(lens, source).map(Some);
    };
    if !table_cells_are_graph_neutral(&table_plan) {
        return Ok(None);
    }
    let Ok((tables, source_embedding)) = source.try_to_finstance(&lens.src_schema) else {
        return Ok(None);
    };
    let Ok((table_view, table_complement)) = crate::table_lens::get(lens, &tables, protocols)
    else {
        return crate::graph_lens::get(lens, source).map(Some);
    };
    if table_complement
        .synthesized_fk_pairs
        .iter()
        .any(|(image, src, tgt)| {
            tables
                .foreign_keys()
                .into_iter()
                .any(|(source_edge, pairs)| {
                    pairs.contains(&(*src, *tgt))
                        && matches!(
                            plan.edge_image.fate(source_edge),
                            Some(EdgeFate::Keep(direct)) if direct == image
                        )
                })
        })
    {
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
    let Ok(table_plan) = RestrictPlan::compile_for_tables(
        &lens.src_schema,
        &lens.tgt_schema,
        &lens.compiled,
        protocols,
    ) else {
        return crate::graph_lens::put(lens, view, complement).map(Some);
    };
    if !table_cells_are_graph_neutral(&table_plan) {
        return Ok(None);
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
    embedding.next_edge = embedding.next_edge.max(complement.id_floor.edge);
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
mod tests {
    use std::collections::{BTreeMap, BTreeSet, HashMap};
    use std::fmt::Debug;

    use panproto_inst::{
        CompiledMigration, FieldPresence, IdFloor, PutDomainError, SynthKey, ViewRef, WInstance,
    };
    use panproto_schema::{Protocol, ScalarKind, Schema, SchemaBuilder};
    use proptest::prelude::*;

    use crate::lens_shape::{GraphShape, LensShape};

    use super::*;

    trait TestUnwrap {
        type Value;

        fn test_unwrap(self, context: &str) -> Self::Value;
    }

    impl<T, E: Debug> TestUnwrap for Result<T, E> {
        type Value = T;

        fn test_unwrap(self, context: &str) -> T {
            match self {
                Ok(value) => value,
                Err(error) => panic!("{context}: {error:?}"),
            }
        }
    }

    impl<T> TestUnwrap for Option<T> {
        type Value = T;

        fn test_unwrap(self, context: &str) -> T {
            self.unwrap_or_else(|| panic!("{context}"))
        }
    }

    trait TestUnwrapErr {
        type Error;

        fn test_unwrap_err(self, context: &str) -> Self::Error;
    }

    impl<T, E> TestUnwrapErr for Result<T, E> {
        type Error = E;

        fn test_unwrap_err(self, context: &str) -> E {
            match self {
                Err(error) => error,
                Ok(_) => panic!("{context}"),
            }
        }
    }

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
            .test_unwrap("test fixture")
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
            .test_unwrap("test fixture")
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
        .test_unwrap("test fixture");
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
            graph_complement_to_table(&lens, &view, &graph, &view_embedding, &protocol)
                .test_unwrap("test fixture");
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
        let table = graph_complement_to_table(&lens, &view, &graph, &embedding, &protocol)
            .test_unwrap("test fixture");
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
            Err(LensError::NotTableComplement {
                reason: NotTableComplementReason::FanNode { node: 5 }
            })
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
            Err(LensError::NotTableComplement {
                reason: NotTableComplementReason::SynthesizedNode { node: 5 }
            })
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
        .test_unwrap("test fixture");
        assert_eq!(graph.dropped_nodes[&2].anchor, "u");
        assert_eq!(graph.dropped_edges[&EdgeId(7)].edge, relation);

        let (round_trip, result_embedding) =
            graph_complement_to_tree(&lens, &view, &graph, &view_embedding)
                .test_unwrap("test fixture");
        assert_eq!(round_trip.dropped_nodes[&2].anchor, "u");
        assert_eq!(round_trip.dropped_arcs, tree.dropped_arcs);
        assert_eq!(result_embedding.arcs[&2], EdgeId(7));
    }

    #[test]
    fn graph_complement_converts_to_the_tree_complement_tree_get_writes() {
        tree_and_graph_complements_convert_both_ways();
    }

    #[test]
    fn graph_complement_converts_to_the_table_complement_table_get_writes() {
        table_and_graph_complements_convert_both_ways();
    }

    #[test]
    fn complement_conversions_invert_on_their_images() {
        tree_and_graph_complements_convert_both_ways();
        table_and_graph_complements_convert_both_ways();
    }

    #[test]
    fn tree_synthesized_expansion_keys_use_durable_graph_edge_ids_and_last_hops() {
        let mut complement = Complement::empty();
        complement.arc_order.push((0, 4));
        complement.synthesized_keys = BTreeMap::from([
            (
                8,
                SynthKey::Expand {
                    source_edge: EdgeId(0),
                    hop: 0,
                },
            ),
            (
                9,
                SynthKey::Expand {
                    source_edge: EdgeId(0),
                    hop: 1,
                },
            ),
        ]);
        let source_embedding = TreeEmbedding {
            arcs: BTreeMap::from([(4, EdgeId(41))]),
            ..TreeEmbedding::default()
        };
        let view_embedding = TreeEmbedding {
            arcs: BTreeMap::from([(8, EdgeId(50)), (9, EdgeId(51)), (4, EdgeId(52))]),
            ..TreeEmbedding::default()
        };

        let index = tree_synth_index(&complement, &source_embedding, &view_embedding)
            .test_unwrap("translated synthesized index");
        assert_eq!(
            index.nodes,
            BTreeMap::from([
                (
                    8,
                    SynthKey::Expand {
                        source_edge: EdgeId(41),
                        hop: 0,
                    },
                ),
                (
                    9,
                    SynthKey::Expand {
                        source_edge: EdgeId(41),
                        hop: 1,
                    },
                ),
            ])
        );
        assert_eq!(
            index.edges,
            BTreeMap::from([
                (
                    EdgeId(50),
                    SynthKey::Expand {
                        source_edge: EdgeId(41),
                        hop: 0,
                    },
                ),
                (
                    EdgeId(51),
                    SynthKey::Expand {
                        source_edge: EdgeId(41),
                        hop: 1,
                    },
                ),
                (
                    EdgeId(52),
                    SynthKey::Expand {
                        source_edge: EdgeId(41),
                        hop: 2,
                    },
                ),
            ])
        );
    }

    #[test]
    fn tree_synthesized_contract_expansion_keys_replace_the_generic_contract_key() {
        let mut complement = Complement::empty();
        complement.synthesized_keys = BTreeMap::from([
            (
                8,
                SynthKey::ContractExpand {
                    src: 0,
                    tgt: 4,
                    hop: 0,
                },
            ),
            (
                9,
                SynthKey::ContractExpand {
                    src: 0,
                    tgt: 4,
                    hop: 1,
                },
            ),
        ]);
        complement.contraction_choices.insert((0, 4), relation());
        let view_embedding = TreeEmbedding {
            arcs: BTreeMap::from([(8, EdgeId(50)), (9, EdgeId(51)), (4, EdgeId(52))]),
            ..TreeEmbedding::default()
        };

        let index = tree_synth_index(&complement, &TreeEmbedding::default(), &view_embedding)
            .test_unwrap("contract-expansion synthesized index");
        assert_eq!(
            index.edges[&EdgeId(52)],
            SynthKey::ContractExpand {
                src: 0,
                tgt: 4,
                hop: 2,
            }
        );
    }

    #[test]
    fn graph_synthesized_contract_arcs_enter_the_tree_complement_and_embedding() {
        let lens = lens();
        let relation = relation();
        let view = GInstance::new()
            .with_node(Node::new(1, "t"))
            .with_node(Node::new(2, "u"))
            .with_edge_id(EdgeId(7), 1, 2, relation.clone());
        let view_embedding = TreeEmbedding {
            arcs: BTreeMap::from([(2, EdgeId(7))]),
            next_node: 3,
            next_edge: 8,
            ..TreeEmbedding::default()
        };
        let mut graph = GraphComplement::empty();
        graph.synthesized_edges.insert(
            EdgeId(7),
            SynthKey::Contract {
                src: 1,
                tgt: 2,
                edge: relation.clone(),
            },
        );
        graph.id_floor = IdFloor { node: 3, edge: 8 };

        let (tree, source_embedding) =
            graph_complement_to_tree(&lens, &view, &graph, &view_embedding)
                .test_unwrap("synthesized contract conversion");
        assert_eq!(tree.contraction_choices[&(1, 2)], relation);
        assert_eq!(source_embedding.arcs[&2], EdgeId(7));
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
        let (source, source_embedding) =
            GInstance::from_wtype_embedded(&tree, None).test_unwrap("test fixture");
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
        graph.dropped_nodes.insert(
            fan.node,
            source.node(fan.node).test_unwrap("test fixture").clone(),
        );
        graph.dropped_edges.insert(
            fan.parent_edge,
            source
                .edge(fan.parent_edge)
                .test_unwrap("test fixture")
                .clone(),
        );
        for edge in fan.child_edges.values() {
            graph.dropped_edges.insert(
                *edge,
                source.edge(*edge).test_unwrap("test fixture").clone(),
            );
        }
        graph.id_floor = IdFloor {
            node: source_embedding.next_node,
            edge: source_embedding.next_edge,
        };

        let (converted, embedding) =
            graph_complement_to_tree(&lens, &view, &graph, &view_embedding)
                .test_unwrap("test fixture");
        assert_eq!(converted.dropped_fans, tree.fans);
        assert_eq!(embedding.fans, source_embedding.fans);
    }

    #[test]
    fn a_partially_dropped_graph_fan_converts_to_tree_residue() {
        let lens = lens();
        let original = Fan::new("membership", 0)
            .with_child("left", 1)
            .with_child("right", 2);
        let tree = WInstance::new(
            HashMap::from([
                (0, Node::new(0, "t")),
                (1, Node::new(1, "u")),
                (2, Node::new(2, "u")),
            ]),
            vec![],
            vec![original.clone()],
            0,
            "t".into(),
        );
        let (mut view, source_embedding) =
            GInstance::from_wtype_embedded(&tree, None).test_unwrap("fan fixture");
        let mut view_embedding = source_embedding.clone();
        let dropped_id = view_embedding.fans[0]
            .child_edges
            .remove("left")
            .test_unwrap("left child edge");
        let dropped = view
            .remove_edge(dropped_id)
            .test_unwrap("drop one fan child");
        let mut graph = GraphComplement::empty();
        graph.dropped_edges.insert(dropped_id, dropped);
        graph.id_floor = IdFloor {
            node: source_embedding.next_node,
            edge: source_embedding.next_edge,
        };

        let (converted, embedding) =
            graph_complement_to_tree(&lens, &view, &graph, &view_embedding)
                .test_unwrap("partial fan conversion");
        assert_eq!(converted.dropped_fans, vec![original]);
        assert_eq!(embedding.fans, source_embedding.fans);
    }

    #[test]
    fn the_direct_tree_get_and_put_square_is_identical() {
        let lens = lens();
        let source = GInstance::new()
            .with_node(Node::new(1, "t"))
            .with_node(Node::new(3, "u"))
            .with_edge_id(EdgeId(8), 1, 3, relation());
        let (graph_view, graph_complement) =
            crate::graph_lens::get(&lens, &source).test_unwrap("test fixture");
        let (tree_view, tree_complement) = graph_get_via_tree(&lens, &source)
            .test_unwrap("test fixture")
            .test_unwrap("test fixture");
        assert!(
            GInstance::identical(&tree_view, &graph_view),
            "tree view: {tree_view:#?}\ngraph view: {graph_view:#?}"
        );
        assert_eq!(
            GraphShape::complement_divergence(&tree_complement, &graph_complement),
            None,
            "tree complement: {tree_complement:#?}\ngraph complement: {graph_complement:#?}"
        );

        let graph_source = crate::graph_lens::put(&lens, &graph_view, &graph_complement)
            .test_unwrap("test fixture");
        let tree_source = graph_put_via_tree(&lens, &graph_view, &graph_complement)
            .test_unwrap("test fixture")
            .test_unwrap("test fixture");
        assert!(GInstance::identical(&tree_source, &graph_source));
    }

    #[test]
    fn the_tree_graph_get_square_is_identical() {
        the_direct_tree_get_and_put_square_is_identical();
    }

    #[test]
    fn the_tree_graph_put_square_is_identical() {
        the_direct_tree_get_and_put_square_is_identical();
    }

    #[test]
    fn tree_dispatch_preserves_the_edge_id_of_a_relabelled_arc() {
        let source_edge = structural_edge("a", "b", "e");
        let target_edge = structural_edge("b", "a", "e");
        let lens = Lens {
            src_schema: structural_schema(
                &[("a", "object"), ("b", "object")],
                std::slice::from_ref(&source_edge),
            ),
            tgt_schema: structural_schema(
                &[("a", "object"), ("b", "object")],
                std::slice::from_ref(&target_edge),
            ),
            compiled: CompiledMigration {
                surviving_verts: ["a".into(), "b".into()].into(),
                surviving_edges: [target_edge.clone()].into(),
                vertex_remap: [("a".into(), "b".into()), ("b".into(), "a".into())].into(),
                edge_remap: [(source_edge.clone(), target_edge)].into(),
                ..CompiledMigration::default()
            },
        };
        let source = GInstance::new()
            .with_node(Node::new(0, "a"))
            .with_node(Node::new(1, "b"))
            .with_edge_id(EdgeId(0), 0, 1, source_edge);

        let (direct_view, direct_complement) =
            crate::graph_lens::get(&lens, &source).test_unwrap("direct graph get");
        let (tree_view, tree_complement) = graph_get_via_tree(&lens, &source)
            .test_unwrap("tree dispatch get")
            .test_unwrap("the relabelled source remains tree-shaped");

        assert!(
            GInstance::identical(&tree_view, &direct_view),
            "tree view: {tree_view:#?}\ndirect view: {direct_view:#?}"
        );
        assert_eq!(
            GraphShape::complement_divergence(&tree_complement, &direct_complement),
            None,
            "tree complement: {tree_complement:#?}\ndirect complement: {direct_complement:#?}"
        );
        let restored = graph_put_via_tree(&lens, &tree_view, &tree_complement)
            .test_unwrap("tree dispatch put")
            .test_unwrap("the relabelled view remains tree-shaped");
        assert!(
            GInstance::identical(&restored, &source),
            "restored graph: {restored:#?}\nsource graph: {source:#?}"
        );
    }

    #[test]
    fn tree_dispatch_restores_every_arc_into_a_pruned_subtree() {
        let relation = structural_edge("a", "b", "e");
        let lens = Lens {
            src_schema: structural_schema(
                &[("a", "object"), ("b", "object")],
                std::slice::from_ref(&relation),
            ),
            tgt_schema: structural_schema(&[("a", "object")], &[]),
            compiled: CompiledMigration {
                surviving_verts: ["a".into()].into(),
                ..CompiledMigration::default()
            },
        };
        let source = GInstance::new()
            .with_node(Node::new(1, "a"))
            .with_node(Node::new(0, "b"))
            .with_node(Node::new(2, "b"))
            .with_edge_id(EdgeId(0), 1, 0, relation.clone())
            .with_edge_id(EdgeId(1), 1, 2, relation);
        let (view, complement) = graph_get_via_tree(&lens, &source)
            .test_unwrap("tree dispatch get")
            .test_unwrap("the source is tree-shaped");
        let restored = graph_put_via_tree(&lens, &view, &complement)
            .test_unwrap("tree dispatch put")
            .test_unwrap("the pruned view is tree-shaped");

        assert!(
            GInstance::identical(&restored, &source),
            "restored graph: {restored:#?}\nsource graph: {source:#?}"
        );
    }

    #[test]
    fn graph_to_tree_parent_residue_uses_get_time_survival_not_the_edited_view() {
        let surviving_child_edge = structural_edge("a", "b", "surviving-child");
        let dropped_child_edge = structural_edge("a", "c", "dropped-child");
        let lens = Lens {
            src_schema: structural_schema(
                &[("a", "object"), ("b", "object"), ("c", "object")],
                &[surviving_child_edge.clone(), dropped_child_edge.clone()],
            ),
            tgt_schema: structural_schema(&[("a", "object"), ("b", "object")], &[]),
            compiled: CompiledMigration {
                surviving_verts: ["a".into(), "b".into()].into(),
                ..CompiledMigration::default()
            },
        };
        let source = GInstance::new()
            .with_node(Node::new(0, "a"))
            .with_node(Node::new(1, "b"))
            .with_node(Node::new(2, "c"))
            .with_edge_id(EdgeId(0), 0, 1, surviving_child_edge.clone())
            .with_edge_id(EdgeId(1), 0, 2, dropped_child_edge.clone());
        let (mut edited_view, complement) = crate::graph_lens::get(&lens, &source)
            .test_unwrap("graph get with one surviving and one dropped child");
        assert!(edited_view.node(1).is_some());
        assert!(complement.dropped_nodes.contains_key(&2));
        assert!(!complement.dropped_nodes.contains_key(&1));

        // Invert current-view presence relative to the get-time survival
        // facts. The converter must read those facts from the complement:
        // node 1 survived get even though this edited view removes it, while
        // node 2 was dropped even though this edited view reuses its id.
        edited_view
            .remove_node(1)
            .test_unwrap("remove the get-time surviving child");
        edited_view
            .insert_node(Node::new(2, "b"))
            .test_unwrap("reuse the get-time dropped child's id");

        let (tree, embedding) =
            graph_complement_to_tree(&lens, &edited_view, &complement, &TreeEmbedding::default())
                .test_unwrap("mixed graph complement converts to tree coordinates");
        assert_eq!(
            tree.original_parent,
            HashMap::from([(1, 0)]),
            "only the child that survived get has an original parent"
        );
        assert_eq!(
            tree.dropped_arcs,
            vec![(0, 1, surviving_child_edge), (0, 2, dropped_child_edge)],
            "both schema-dropped arcs remain available as residue"
        );
        assert_eq!(
            embedding.arcs,
            BTreeMap::from([(1, EdgeId(0)), (2, EdgeId(1))]),
            "both source arcs remain embedded independently of current node presence"
        );
    }

    fn structural_schema(vertices: &[(&str, &str)], edges: &[Edge]) -> Schema {
        let protocol = Protocol::default();
        let mut builder = SchemaBuilder::new(&protocol);
        for (vertex, kind) in vertices {
            builder = builder
                .vertex(vertex, kind, None)
                .test_unwrap("test fixture");
        }
        for edge in edges {
            builder = builder
                .edge(
                    edge.src.as_str(),
                    edge.tgt.as_str(),
                    edge.kind.as_str(),
                    edge.name.as_deref(),
                )
                .test_unwrap("test fixture");
        }
        builder.build().test_unwrap("test fixture")
    }

    fn structural_edge(src: &str, tgt: &str, name: &str) -> Edge {
        Edge {
            src: src.into(),
            tgt: tgt.into(),
            kind: "prop".into(),
            name: Some(name.into()),
        }
    }

    fn structural_schema_with_membership(vertices: &[(&str, &str)], edges: &[Edge]) -> Schema {
        let protocol = Protocol::default();
        let mut builder = SchemaBuilder::new(&protocol);
        for (vertex, kind) in vertices {
            builder = builder
                .vertex(vertex, kind, None)
                .test_unwrap("test fixture");
        }
        for edge in edges {
            builder = builder
                .edge(
                    edge.src.as_str(),
                    edge.tgt.as_str(),
                    edge.kind.as_str(),
                    edge.name.as_deref(),
                )
                .test_unwrap("test fixture");
        }
        builder
            .hyper_edge(
                "membership",
                "relation",
                HashMap::from([
                    ("parent".to_owned(), "r".to_owned()),
                    ("member".to_owned(), "leaf".to_owned()),
                ]),
                "parent",
            )
            .test_unwrap("test fixture")
            .build()
            .test_unwrap("test fixture")
    }

    #[test]
    fn a_recursive_dropped_wrapper_contracts_to_one_edge_and_restores() {
        let rm = structural_edge("r", "m", "m");
        let mm = structural_edge("m", "m", "m");
        let ml = structural_edge("m", "leaf", "leaf");
        let contracted = structural_edge("r", "leaf", "label");
        let lens = Lens {
            src_schema: structural_schema(
                &[("r", "object"), ("m", "object"), ("leaf", "string")],
                &[rm.clone(), mm.clone(), ml.clone()],
            ),
            tgt_schema: structural_schema(
                &[("r", "object"), ("leaf", "string")],
                std::slice::from_ref(&contracted),
            ),
            compiled: CompiledMigration {
                surviving_verts: ["r".into(), "leaf".into()].into(),
                resolver: std::iter::once((("r".into(), "leaf".into()), contracted)).collect(),
                ..CompiledMigration::default()
            },
        };
        let source_tree = WInstance::new(
            HashMap::from([
                (0, Node::new(0, "r")),
                (1, Node::new(1, "m")),
                (2, Node::new(2, "m")),
                (3, Node::new(3, "m")),
                (
                    4,
                    Node::new(4, "leaf").with_value(panproto_inst::FieldPresence::Present(
                        Value::Str("x".to_owned()),
                    )),
                ),
            ]),
            vec![(0, 1, rm), (1, 2, mm.clone()), (2, 3, mm), (3, 4, ml)],
            vec![],
            0,
            "r".into(),
        );
        let (source, _) = GInstance::from_wtype_embedded(&source_tree, Some(&lens.src_schema))
            .test_unwrap("test fixture");
        let (graph_view, graph_complement) =
            crate::graph_lens::get(&lens, &source).test_unwrap("test fixture");
        let (tree_view, tree_complement) = graph_get_via_tree(&lens, &source)
            .test_unwrap("test fixture")
            .test_unwrap("the recursive source is tree-shaped");
        assert_eq!((tree_view.node_count(), tree_view.edge_count()), (2, 1));
        assert!(GInstance::identical(&tree_view, &graph_view));
        assert_eq!(
            GraphShape::complement_divergence(&tree_complement, &graph_complement),
            None,
            "tree complement: {tree_complement:#?}\ngraph complement: {graph_complement:#?}"
        );
        let restored = graph_put_via_tree(&lens, &graph_view, &graph_complement)
            .test_unwrap("test fixture")
            .test_unwrap("the contracted view is tree-shaped");
        assert_eq!((restored.node_count(), restored.edge_count()), (5, 4));
        assert!(GInstance::identical(&restored, &source));
    }

    fn assert_tree_bridge_round_trip(lens: &Lens, source_tree: &WInstance) -> GInstance {
        let (source, _) = GInstance::from_wtype_embedded(source_tree, Some(&lens.src_schema))
            .test_unwrap("test fixture");
        let (graph_view, graph_complement) =
            crate::graph_lens::get(lens, &source).test_unwrap("test fixture");
        let (tree_view, tree_complement) = graph_get_via_tree(lens, &source)
            .test_unwrap("test fixture")
            .test_unwrap("the source is in the tree bridge's domain");
        assert!(
            GInstance::identical(&tree_view, &graph_view),
            "tree view: {tree_view:#?}\ngraph view: {graph_view:#?}"
        );
        assert_eq!(
            GraphShape::complement_divergence(&tree_complement, &graph_complement),
            None,
            "tree complement: {tree_complement:#?}\ngraph complement: {graph_complement:#?}"
        );
        let graph_source = crate::graph_lens::put(lens, &graph_view, &graph_complement)
            .test_unwrap("test fixture");
        let tree_source = graph_put_via_tree(lens, &graph_view, &graph_complement)
            .test_unwrap("test fixture")
            .test_unwrap("the view is in the tree bridge's domain");
        assert!(GInstance::identical(&tree_source, &graph_source));
        assert!(GInstance::identical(&tree_source, &source));
        graph_view
    }

    fn expanding_structural_lens() -> (Lens, Edge) {
        let direct = structural_edge("r", "leaf", "leaf");
        let first = structural_edge("r", "mid", "mid");
        let second = structural_edge("mid", "leaf", "leaf");
        (
            Lens {
                src_schema: structural_schema_with_membership(
                    &[("r", "object"), ("leaf", "string")],
                    std::slice::from_ref(&direct),
                ),
                tgt_schema: structural_schema_with_membership(
                    &[("r", "object"), ("mid", "object"), ("leaf", "string")],
                    &[first, second],
                ),
                compiled: CompiledMigration {
                    surviving_verts: ["r".into(), "leaf".into()].into(),
                    expansion_path: std::iter::once((
                        ("r".into(), "leaf".into()),
                        vec!["mid".into()],
                    ))
                    .collect(),
                    ..CompiledMigration::default()
                },
            },
            direct,
        )
    }

    #[test]
    fn fans_with_an_expansion_commute() {
        let (lens, direct) = expanding_structural_lens();
        let source = WInstance::new(
            HashMap::from([(0, Node::new(0, "r")), (4, Node::new(4, "leaf"))]),
            vec![(0, 4, direct)],
            vec![Fan::new("membership", 0).with_child("member", 4)],
            0,
            "r".into(),
        );
        let view = assert_tree_bridge_round_trip(&lens, &source);
        assert_eq!((view.node_count(), view.edge_count()), (4, 4));
        assert!(view.nodes().keys().max().is_some_and(|id| *id >= 6));
    }

    #[test]
    fn two_contracted_leaves_keep_sibling_order() {
        let root_mid = structural_edge("r", "mid", "mid");
        let mid_leaf = structural_edge("mid", "leaf", "leaf");
        let contracted = structural_edge("r", "leaf", "leaf");
        let lens = Lens {
            src_schema: structural_schema(
                &[("r", "object"), ("mid", "object"), ("leaf", "string")],
                &[root_mid.clone(), mid_leaf.clone()],
            ),
            tgt_schema: structural_schema(
                &[("r", "object"), ("leaf", "string")],
                std::slice::from_ref(&contracted),
            ),
            compiled: CompiledMigration {
                surviving_verts: ["r".into(), "leaf".into()].into(),
                resolver: std::iter::once((("r".into(), "leaf".into()), contracted)).collect(),
                ..CompiledMigration::default()
            },
        };
        let source = WInstance::new(
            HashMap::from([
                (0, Node::new(0, "r")),
                (1, Node::new(1, "mid")),
                (2, Node::new(2, "mid")),
                (3, Node::new(3, "leaf")),
                (4, Node::new(4, "leaf")),
            ]),
            vec![
                (0, 1, root_mid.clone()),
                (0, 2, root_mid),
                (1, 3, mid_leaf.clone()),
                (2, 4, mid_leaf),
            ],
            vec![],
            0,
            "r".into(),
        );
        let view = assert_tree_bridge_round_trip(&lens, &source);
        let ordered: Vec<u32> = view
            .ordered_out_edges(0)
            .iter()
            .filter_map(|id| view.edge(*id).map(|edge| edge.tgt))
            .collect();
        assert_eq!(ordered, [3, 4]);
    }

    #[test]
    fn a_direct_expand_between_siblings_commutes() {
        let (lens, direct) = expanding_structural_lens();
        let source = WInstance::new(
            HashMap::from([
                (0, Node::new(0, "r")),
                (1, Node::new(1, "leaf")),
                (2, Node::new(2, "leaf")),
            ]),
            vec![(0, 1, direct.clone()), (0, 2, direct)],
            vec![],
            0,
            "r".into(),
        );
        let view = assert_tree_bridge_round_trip(&lens, &source);
        let children: Vec<u32> = view
            .ordered_out_edges(0)
            .iter()
            .filter_map(|id| view.edge(*id).map(|edge| edge.tgt))
            .collect();
        assert_eq!(children.len(), 2);
        assert_ne!(children[0], children[1]);
    }

    #[test]
    fn a_fast_path_error_reruns_the_graph_path() {
        let lens = lens();
        let relation = relation();
        let view = GInstance::new()
            .with_node(Node::new(1, "t"))
            .with_node(Node::new(3, "u"))
            .with_edge_id(EdgeId(8), 1, 3, relation.clone());
        let mut complement = GraphComplement::empty();
        complement
            .dropped_edges
            .insert(EdgeId(7), GEdge::new(1, 3, relation));
        complement.id_floor = IdFloor { node: 4, edge: 9 };
        complement.identity = Some(lens.identity(ComplementShape::Graph));

        let graph = crate::graph_lens::put(&lens, &view, &complement).test_unwrap("test fixture");
        let dispatched = graph_put_via_tree(&lens, &view, &complement)
            .test_unwrap("test fixture")
            .test_unwrap("a conversion error reruns the graph path");
        assert!(GInstance::identical(&dispatched, &graph));
        assert_eq!(dispatched.in_edges(3).len(), 2);
    }

    fn dropped_edge_lens() -> (Lens, Edge) {
        let relation = relation();
        (
            Lens {
                src_schema: schema(),
                tgt_schema: structural_schema(&[("t", "record"), ("u", "record")], &[]),
                compiled: CompiledMigration {
                    surviving_verts: ["t".into(), "u".into()].into(),
                    ..CompiledMigration::default()
                },
            },
            relation,
        )
    }

    #[test]
    fn a_dispatched_put_rejects_a_reused_dropped_edge_id() {
        let (lens, relation) = dropped_edge_lens();
        let source = GInstance::new()
            .with_node(Node::new(1, "t"))
            .with_node(Node::new(3, "u"))
            .with_edge_id(EdgeId(7), 1, 3, relation.clone());
        let (mut view, complement) =
            crate::graph_lens::get(&lens, &source).test_unwrap("test fixture");
        view.insert_edge(EdgeId(7), GEdge::new(1, 3, relation))
            .test_unwrap("test fixture");
        let graph =
            crate::graph_lens::put(&lens, &view, &complement).test_unwrap_err("expected an error");
        let dispatched =
            graph_put_via_tree(&lens, &view, &complement).test_unwrap_err("expected an error");
        assert!(matches!(
            &graph,
            LensError::PutDomain(error)
                if **error == PutDomainError::IdCollision {
                    elem: ViewRef::Edge(EdgeId(7)),
                }
        ));
        assert!(matches!(
            &dispatched,
            LensError::PutDomain(error)
                if **error == PutDomainError::IdCollision {
                    elem: ViewRef::Edge(EdgeId(7)),
                }
        ));
        assert_eq!(graph.to_string(), dispatched.to_string());
    }

    #[test]
    fn a_dispatched_put_rejects_an_edited_synthesized_edge() {
        let (lens, direct) = expanding_structural_lens();
        let source = GInstance::new()
            .with_node(Node::new(0, "r"))
            .with_node(Node::new(1, "leaf"))
            .with_edge_id(EdgeId(4), 0, 1, direct);
        let (mut view, complement) =
            crate::graph_lens::get(&lens, &source).test_unwrap("test fixture");
        let synthesized = *complement
            .synthesized_edges
            .keys()
            .next()
            .test_unwrap("the expansion synthesizes edges");
        let mut edited = view
            .edge(synthesized)
            .test_unwrap("synthesized edge")
            .edge
            .clone();
        edited.name = Some("edited".into());
        view.relabel_edge(synthesized, edited)
            .test_unwrap("relabel synthesized edge");
        let graph =
            crate::graph_lens::put(&lens, &view, &complement).test_unwrap_err("expected an error");
        let dispatched =
            graph_put_via_tree(&lens, &view, &complement).test_unwrap_err("expected an error");
        let expected = PutDomainError::SynthesizedEdited {
            elem: ViewRef::Edge(synthesized),
        };
        assert!(matches!(&graph, LensError::PutDomain(error) if error.as_ref() == &expected));
        assert!(matches!(&dispatched, LensError::PutDomain(error) if error.as_ref() == &expected));
        assert_eq!(graph.to_string(), dispatched.to_string());
    }

    #[test]
    fn dispatched_and_graph_puts_report_the_same_first_error() {
        a_dispatched_put_rejects_a_reused_dropped_edge_id();
        a_dispatched_put_rejects_an_edited_synthesized_edge();
    }

    #[test]
    fn a_root_pruned_tree_get_declines_the_fast_route() {
        let lens = Lens {
            src_schema: structural_schema(&[("root", "object"), ("kept", "object")], &[]),
            tgt_schema: structural_schema(&[("kept", "object")], &[]),
            compiled: CompiledMigration {
                surviving_verts: ["kept".into()].into(),
                ..CompiledMigration::default()
            },
        };
        let source = GInstance::new().with_node(Node::new(9, "root"));
        assert!(
            graph_get_via_tree(&lens, &source)
                .test_unwrap("test fixture")
                .is_none(),
            "RootPruned must decline so the caller performs a coherent graph get and put"
        );
    }

    #[test]
    fn a_pruned_root_above_a_surviving_child_declines_the_fast_route() {
        let edge = structural_edge("b", "a", "e");
        let lens = Lens {
            src_schema: structural_schema(
                &[("a", "obj"), ("b", "obj")],
                std::slice::from_ref(&edge),
            ),
            tgt_schema: structural_schema(&[("a", "obj")], &[]),
            compiled: CompiledMigration {
                surviving_verts: ["a".into()].into(),
                ..CompiledMigration::default()
            },
        };
        let source = GInstance::new()
            .with_node(Node::new(0, "b"))
            .with_node(Node::new(1, "a"))
            .with_edge_id(EdgeId(0), 0, 1, edge);
        let (graph_view, graph_complement) =
            crate::graph_lens::get(&lens, &source).test_unwrap("the graph route is defined");
        let restored = crate::graph_lens::put(&lens, &graph_view, &graph_complement)
            .test_unwrap("the graph route restores the dropped root and arc");
        assert!(GInstance::identical(&restored, &source));
        assert!(
            graph_get_via_tree(&lens, &source)
                .test_unwrap("the tree bridge classifies its exact domain")
                .is_none(),
            "the root-pruned tree route must not return the graph complement as a fast-route complement"
        );
    }

    #[test]
    fn dispatch_declines_when_a_contraction_pair_coincides_with_a_direct_pair() {
        let fk = |src: &str, tgt: &str, name: &str| Edge {
            src: src.into(),
            tgt: tgt.into(),
            kind: "fk".into(),
            name: Some(name.into()),
        };
        let root_mid = fk("r", "m", "root_mid");
        let mid_leaf = fk("m", "leaf", "mid_leaf");
        let joined = fk("r", "leaf", "joined");
        let lens = Lens {
            src_schema: structural_schema(
                &[("r", "record"), ("m", "record"), ("leaf", "record")],
                &[root_mid.clone(), mid_leaf.clone(), joined.clone()],
            ),
            tgt_schema: structural_schema(
                &[("r", "record"), ("leaf", "record")],
                std::slice::from_ref(&joined),
            ),
            compiled: CompiledMigration {
                surviving_verts: ["r".into(), "leaf".into()].into(),
                surviving_edges: [joined.clone()].into(),
                resolver: std::iter::once((("r".into(), "leaf".into()), joined.clone())).collect(),
                ..CompiledMigration::default()
            },
        };
        let source = GInstance::new()
            .with_node(Node::new(0, "r"))
            .with_node(Node::new(1, "m"))
            .with_node(Node::new(2, "leaf"))
            .with_edge_id(EdgeId(7), 0, 2, joined)
            .with_edge_id(EdgeId(8), 0, 1, root_mid)
            .with_edge_id(EdgeId(9), 1, 2, mid_leaf);
        let protocol = protocol();
        let protocols = BoundaryProtocols {
            src: &protocol,
            tgt: &protocol,
        };

        assert!(lens.preserves_table_shape());
        let graph = crate::graph_lens::get(&lens, &source).test_unwrap("graph get");
        assert_eq!(graph.0.edge_count(), 2);
        assert!(
            graph_get_via_table(&lens, &source, protocols)
                .test_unwrap("table bridge")
                .is_none()
        );
    }

    #[test]
    fn dispatch_accepts_distinct_direct_and_contracted_pairs_with_the_same_edge_image() {
        let fk = |src: &str, tgt: &str, name: &str| Edge {
            src: src.into(),
            tgt: tgt.into(),
            kind: "fk".into(),
            name: Some(name.into()),
        };
        let root_mid = fk("r", "m", "root_mid");
        let mid_leaf = fk("m", "leaf", "mid_leaf");
        let joined = fk("r", "leaf", "joined");
        let lens = Lens {
            src_schema: structural_schema(
                &[("r", "record"), ("m", "record"), ("leaf", "record")],
                &[root_mid.clone(), mid_leaf.clone(), joined.clone()],
            ),
            tgt_schema: structural_schema(
                &[("r", "record"), ("leaf", "record")],
                std::slice::from_ref(&joined),
            ),
            compiled: CompiledMigration {
                surviving_verts: ["r".into(), "leaf".into()].into(),
                surviving_edges: [joined.clone()].into(),
                resolver: std::iter::once((("r".into(), "leaf".into()), joined.clone())).collect(),
                ..CompiledMigration::default()
            },
        };
        let source = GInstance::new()
            .with_node(Node::new(0, "r"))
            .with_node(Node::new(1, "m"))
            .with_node(Node::new(2, "leaf"))
            .with_node(Node::new(3, "leaf"))
            .with_edge_id(EdgeId(7), 0, 3, joined)
            .with_edge_id(EdgeId(8), 0, 1, root_mid)
            .with_edge_id(EdgeId(9), 1, 2, mid_leaf);
        let protocol = protocol();
        let protocols = BoundaryProtocols {
            src: &protocol,
            tgt: &protocol,
        };

        let graph = crate::graph_lens::get(&lens, &source).test_unwrap("graph get");
        let dispatched = graph_get_via_table(&lens, &source, protocols)
            .test_unwrap("table bridge")
            .test_unwrap("distinct pairs do not collide");
        assert!(GInstance::identical(&dispatched.0, &graph.0));
        assert_eq!(
            GraphShape::complement_divergence(&dispatched.1, &graph.1),
            None
        );
    }

    #[test]
    fn the_direct_table_get_and_put_square_is_identical() {
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
            .test_unwrap("test fixture");
        let (source, _) = GInstance::from_finstance(&tables).test_unwrap("test fixture");
        let (graph_view, graph_complement) =
            crate::graph_lens::get(&lens, &source).test_unwrap("test fixture");
        let (table_view, table_complement) = graph_get_via_table(&lens, &source, protocols)
            .test_unwrap("test fixture")
            .test_unwrap("test fixture");
        assert!(GInstance::identical(&table_view, &graph_view));
        assert_eq!(
            GraphShape::complement_divergence(&table_complement, &graph_complement),
            None
        );

        let graph_source = crate::graph_lens::put(&lens, &graph_view, &graph_complement)
            .test_unwrap("test fixture");
        let table_source = graph_put_via_table(&lens, &graph_view, &graph_complement, protocols)
            .test_unwrap("test fixture")
            .test_unwrap("test fixture");
        assert!(GInstance::identical(&table_source, &graph_source));
    }

    #[test]
    fn table_dispatch_runs_laws_only_where_table_shape_accepts_both_sides() {
        let lens = lens();
        let protocol = protocol();
        let protocols = BoundaryProtocols {
            src: &protocol,
            tgt: &protocol,
        };
        let tables = view();
        let (source, _) = GInstance::from_finstance(&tables).test_unwrap("table encoding");
        let (mut graph_view, complement) =
            crate::graph_lens::get(&lens, &source).test_unwrap("graph get");
        assert!(
            graph_get_via_table(&lens, &source, protocols)
                .test_unwrap("table bridge get")
                .is_some()
        );

        graph_view.node_mut(1).test_unwrap("view row").value =
            Some(FieldPresence::Present(Value::Int(17)));
        assert!(crate::graph_lens::put(&lens, &graph_view, &complement).is_ok());
        assert!(
            graph_put_via_table(&lens, &graph_view, &complement, protocols)
                .test_unwrap("table bridge put")
                .is_none()
        );
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]

        #[test]
        fn the_direct_tree_square_is_identical_for_arbitrary_distinct_ids(
            first in any::<u16>(),
            second in any::<u16>(),
            edge_id in any::<u16>(),
        ) {
            prop_assume!(first != second);
            let first = u32::from(first);
            let second = u32::from(second);
            let source = GInstance::new()
                .with_node(Node::new(first, "t"))
                .with_node(Node::new(second, "u"))
                .with_edge_id(EdgeId(u32::from(edge_id)), first, second, relation());
            let lens = lens();
            let (graph_view, graph_complement) = crate::graph_lens::get(&lens, &source).test_unwrap("test fixture");
            let (tree_view, tree_complement) = graph_get_via_tree(&lens, &source)
                .test_unwrap("test fixture")
                .test_unwrap("the source is a tree");
            prop_assert!(GInstance::identical(&tree_view, &graph_view));
            prop_assert_eq!(
                GraphShape::complement_divergence(&tree_complement, &graph_complement),
                None
            );
            let graph_source =
                crate::graph_lens::put(&lens, &graph_view, &graph_complement).test_unwrap("test fixture");
            let tree_source = graph_put_via_tree(&lens, &graph_view, &graph_complement)
                .test_unwrap("test fixture")
                .test_unwrap("the view is a tree");
            prop_assert!(GInstance::identical(&tree_source, &graph_source));
        }

        #[test]
        fn the_direct_table_square_is_identical_for_arbitrary_rows(
            first in any::<u16>(),
            second in any::<u16>(),
            a in any::<i16>(),
            c in any::<i16>(),
        ) {
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
                .test_unwrap("test fixture");
            let (source, _) = GInstance::from_finstance(&tables).test_unwrap("test fixture");
            let lens = lens();
            let protocol = protocol();
            let protocols = BoundaryProtocols {
                src: &protocol,
                tgt: &protocol,
            };
            let (graph_view, graph_complement) = crate::graph_lens::get(&lens, &source).test_unwrap("test fixture");
            let (table_view, table_complement) = graph_get_via_table(&lens, &source, protocols)
                .test_unwrap("test fixture")
                .test_unwrap("the source is a table");
            prop_assert!(GInstance::identical(&table_view, &graph_view));
            prop_assert_eq!(
                GraphShape::complement_divergence(&table_complement, &graph_complement),
                None
            );
            let graph_source =
                crate::graph_lens::put(&lens, &graph_view, &graph_complement).test_unwrap("test fixture");
            let table_source =
                graph_put_via_table(&lens, &graph_view, &graph_complement, protocols)
                    .test_unwrap("test fixture")
                    .test_unwrap("the view is a table");
            prop_assert!(GInstance::identical(&table_source, &graph_source));
        }
    }
}
