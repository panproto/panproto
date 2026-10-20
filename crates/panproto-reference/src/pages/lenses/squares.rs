//! Executable F.B representation squares.
//!
//! Each square derives both routes from one source. The observations include
//! get, the converted complement, and put; two caller-supplied outcomes are
//! never accepted as evidence that a route commutes.

use std::collections::{BTreeMap, BTreeSet};

use super::domain::{PutDomainError, ViewRef};
use super::graph::{GraphLens, IdentityError as GraphIdentityError, LensError};
use super::table::{
    self, IdentityError as TableIdentityError, TableComplement, TableLens, TableOperationalError,
    TablePutError,
};
use super::tree::{TreeOperationalError, tree_get, tree_put_with_graph};
use crate::encode::{
    TableEncodeError, TreeEmbedding, TreeEncodeError, graph_to_tree, schema_parent_name,
    table_to_graph, tree_to_graph,
};
use crate::model::{GEdge, Graph, Node, SEdge, Table, Tree};
use crate::pages::representation::canonical::ComplementShape;
use crate::restrict::{Complement, HYPER_LABEL, HYPER_PARENT, RestrictError, SynthKey, floor_of};

/// A route-level refusal in a representation square.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SquareError {
    /// The input tree has no graph form.
    TreeEncoding(TreeEncodeError),
    /// The input table has no graph form.
    TableEncoding(TableEncodeError),
    /// A restrict route refused.
    Restrict(RestrictError),
    /// The first put-domain error, mapped into graph coordinates.
    PutDomain(PutDomainError),
    /// A complete graph put-domain report.
    PutDomainReport(Vec<PutDomainError>),
    /// A graph or tree complement has the wrong identity.
    GraphIdentity(GraphIdentityError),
    /// Tree carrier construction or readback failed operationally.
    TreeOperational(TreeOperationalError),
    /// A table complement has the wrong identity.
    TableIdentity(TableIdentityError),
    /// Table carrier construction or forward evaluation failed operationally.
    TableOperational(TableOperationalError),
    /// Named collation semantics are unavailable to the reference evaluator.
    NamedCollationUnresolved {
        /// Every unresolved collation name, in ascending order.
        names: BTreeSet<String>,
    },
    /// Complement-free table put was requested for a non-isomorphism.
    NotAnIsomorphism {
        /// The first obstructing coordinate.
        detail: String,
    },
    /// The fast representation cannot preserve the graph route's exact
    /// carrier on this input.
    OutsideExactDomain(&'static str),
}

/// Exact get and put observations made by one leg of a representation square.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SquareRouteObservation {
    /// The view and complement, in graph coordinates.
    pub get: Result<(Graph, Complement), SquareError>,
    /// Put of the observed (or explicitly supplied) view. `None` means get
    /// did not produce a complement on which put could run.
    pub put: Option<Result<Graph, SquareError>>,
}

/// Exact observations of the graph/tree F.B square.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GraphSquareObservation {
    /// Direct graph-lens route.
    pub graph: SquareRouteObservation,
    /// Tree route, converted back with its recorded ids and order.
    pub fast: SquareRouteObservation,
    /// Native tree get result, retaining child order that graph equality
    /// deliberately cannot observe.
    pub fast_tree_get: Result<Tree, SquareError>,
    /// Direct graph get converted to a tree in `ordered_out_edges` order.
    pub graph_tree_get: Result<Tree, SquareError>,
    /// Direct graph put, converted independently back to a tree so child
    /// order remains an explicit square observation.
    pub graph_tree_put: Option<Result<Tree, SquareError>>,
    /// Native tree put result, including its exact child order.
    pub fast_tree_put: Option<Result<Tree, SquareError>>,
}

/// Exact observations of the graph/table F.B square.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableSquareObservation {
    /// Direct graph-lens route.
    pub graph: SquareRouteObservation,
    /// Table-lens route, converted back with its recorded ids.
    pub fast: SquareRouteObservation,
}

/// Result of executing both legs of an F.B square.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SquareResult<O> {
    /// Both exact observations, including complements or equal errors, agree.
    Commutes(O),
    /// Both routes completed but their exact observations differ.
    Mismatch(O),
    /// At least one route is outside the exact common domain.
    Refused(O),
}

impl<O> SquareResult<O> {
    /// Whether both routes ran and agreed exactly.
    #[must_use]
    pub const fn commutes(&self) -> bool {
        matches!(self, Self::Commutes(_))
    }

    /// The observations made by both routes.
    #[must_use]
    pub const fn observation(&self) -> &O {
        match self {
            Self::Commutes(observation)
            | Self::Mismatch(observation)
            | Self::Refused(observation) => observation,
        }
    }
}

fn graph_put_error(error: LensError) -> SquareError {
    match error {
        LensError::Identity(error) => SquareError::GraphIdentity(error),
        LensError::PutDomain(error) => SquareError::PutDomain(error),
        LensError::PutDomainReport(errors) => SquareError::PutDomainReport(errors),
        LensError::TreeOperational(error) => SquareError::TreeOperational(error),
    }
}

const fn route_is_outside(route: &SquareRouteObservation) -> bool {
    matches!(&route.get, Err(SquareError::OutsideExactDomain(_)))
        || matches!(&route.put, Some(Err(SquareError::OutsideExactDomain(_))))
}

struct FastTreeState {
    view: Tree,
    complement: Complement,
    embedding: TreeEmbedding,
    graph_view: Graph,
    graph_complement: Complement,
}

fn tree_shape_error(reason: crate::encode::NotATree) -> SquareError {
    match reason {
        crate::encode::NotATree::Fan { node, violation } => {
            SquareError::PutDomain(PutDomainError::MalformedFan {
                node,
                reason: violation,
            })
        }
        other => {
            SquareError::TreeOperational(TreeOperationalError::RestoredNotATree { reason: other })
        }
    }
}

fn square_fresh_after(ids: impl Iterator<Item = u32>, floor: u32) -> Option<u32> {
    ids.max()
        .into_iter()
        .chain(floor.checked_sub(1))
        .max()
        .map_or(Some(0), |id| id.checked_add(1))
        .filter(|id| *id != u32::MAX)
}

fn square_take_fresh(next: &mut Option<u32>, nodes: bool) -> Result<u32, TreeOperationalError> {
    let id = (*next).ok_or(TreeOperationalError::IdsExhausted { nodes })?;
    *next = id.checked_add(1).filter(|next| *next != u32::MAX);
    Ok(id)
}

fn square_parent_name(lens: &GraphLens<'_>, hyper: &str) -> Option<String> {
    lens.tgt
        .hyper_edges
        .get(hyper)
        .map(|declaration| declaration.parent_label.clone())
}

fn square_fan_edge(
    fan: u32,
    hyper_edge: &str,
    kind: &str,
    name: Option<String>,
    target: u32,
    target_anchor: String,
) -> GEdge {
    GEdge {
        src: fan,
        tgt: target,
        label: SEdge {
            src: hyper_edge.to_owned(),
            tgt: target_anchor,
            kind: kind.to_owned(),
            name,
        },
        position: None,
    }
}

fn square_append_fans(
    lens: &GraphLens<'_>,
    view: &Tree,
    embedding: &TreeEmbedding,
    graph: &mut Graph,
    used_edges: &mut BTreeSet<u32>,
    next_node: &mut Option<u32>,
    next_edge: &mut Option<u32>,
) -> Result<(), TreeOperationalError> {
    for (index, fan) in view.fans.iter().enumerate() {
        let known = embedding.fans.get(index);
        let fan_id = known
            .filter(|(node, _, _)| !graph.nodes.contains_key(node))
            .map_or_else(
                || square_take_fresh(next_node, true),
                |(node, _, _)| Ok(*node),
            )?;
        let mut node = Node::new(fan_id, &fan.hyper_edge);
        node.shape = crate::model::NodeShape::HyperEdge;
        graph.nodes.insert(fan_id, node);
        let parent_edge = known
            .map(|(_, edge, _)| *edge)
            .filter(|edge| used_edges.insert(*edge))
            .map_or_else(|| square_take_fresh(next_edge, false), Ok)?;
        used_edges.insert(parent_edge);
        let parent_anchor = view
            .nodes
            .get(&fan.parent)
            .map_or_else(String::new, |node| node.anchor.clone());
        graph.edges.insert(
            parent_edge,
            square_fan_edge(
                fan_id,
                &fan.hyper_edge,
                HYPER_PARENT,
                square_parent_name(lens, &fan.hyper_edge),
                fan.parent,
                parent_anchor,
            ),
        );
        for (label, child) in &fan.children {
            let edge = known
                .and_then(|(_, _, children)| children.get(label).copied())
                .filter(|edge| used_edges.insert(*edge))
                .map_or_else(|| square_take_fresh(next_edge, false), Ok)?;
            used_edges.insert(edge);
            let anchor = view
                .nodes
                .get(child)
                .map_or_else(String::new, |node| node.anchor.clone());
            graph.edges.insert(
                edge,
                square_fan_edge(
                    fan_id,
                    &fan.hyper_edge,
                    HYPER_LABEL,
                    Some(label.clone()),
                    *child,
                    anchor,
                ),
            );
        }
    }
    Ok(())
}

fn square_tree_view_graph(
    lens: &GraphLens<'_>,
    view: &Tree,
    embedding: &TreeEmbedding,
    complement: &Complement,
) -> Result<Graph, TreeOperationalError> {
    let mut graph = Graph {
        nodes: view.nodes.clone(),
        edges: BTreeMap::new(),
    };
    let mut next_node = square_fresh_after(
        view.nodes
            .keys()
            .copied()
            .chain(complement.dropped_nodes.keys().copied())
            .chain(complement.synthesized_nodes.keys().copied()),
        complement.floor.0.max(embedding.next_node),
    );
    let fan_edges = embedding.fans.iter().flat_map(|(_, parent, children)| {
        std::iter::once(*parent).chain(children.values().copied())
    });
    let mut next_edge = square_fresh_after(
        embedding
            .arcs
            .values()
            .copied()
            .chain(complement.dropped_edges.keys().copied())
            .chain(complement.synthesized_edges.keys().copied())
            .chain(fan_edges),
        complement.floor.1.max(embedding.next_edge),
    );
    let mut used = BTreeSet::new();
    for (parent, child, label) in &view.arcs {
        let edge = embedding
            .arcs
            .get(child)
            .copied()
            .filter(|edge| used.insert(*edge))
            .map_or_else(|| square_take_fresh(&mut next_edge, false), Ok)?;
        graph.edges.insert(
            edge,
            GEdge {
                src: *parent,
                tgt: *child,
                label: label.clone(),
                position: None,
            },
        );
    }
    square_append_fans(
        lens,
        view,
        embedding,
        &mut graph,
        &mut used,
        &mut next_node,
        &mut next_edge,
    )?;
    Ok(graph)
}

fn fast_tree_state(lens: &GraphLens<'_>, source: &Tree) -> Result<FastTreeState, SquareError> {
    let Some(get) = tree_get(lens, source) else {
        return Err(SquareError::OutsideExactDomain(
            "the lens does not preserve this tree representation",
        ));
    };
    let (view, complement, embedding) = match get {
        Err(error) => return Err(SquareError::Restrict(error)),
        Ok(state) => state,
    };
    let graph_view = square_tree_view_graph(lens, &view, &embedding, &complement)
        .map_err(SquareError::TreeOperational)?;
    let mut graph_complement = complement.clone();
    graph_complement.identity = Some(lens.identity_for_shape(ComplementShape::Graph));
    Ok(FastTreeState {
        view,
        complement,
        embedding,
        graph_view,
        graph_complement,
    })
}

fn graph_square_impl(
    lens: &GraphLens<'_>,
    source: &Tree,
    put_view: Option<(&Tree, &TreeEmbedding)>,
) -> SquareResult<GraphSquareObservation> {
    let encoded = tree_to_graph(source, Some(lens.src)).map_err(SquareError::TreeEncoding);
    let graph_get = match &encoded {
        Ok((source_graph, _)) => lens.get(source_graph).map_err(SquareError::Restrict),
        Err(error) => Err(error.clone()),
    };
    let fast_state = fast_tree_state(lens, source);

    let graph_put_pair = graph_get
        .as_ref()
        .ok()
        .and_then(|(default_view, complement)| {
            let (view, root) = match put_view {
                None => (Ok(default_view.clone()), source.root),
                Some((edited, embedding)) => {
                    let fast = fast_state.as_ref().ok()?;
                    (
                        square_tree_view_graph(lens, edited, embedding, &fast.complement)
                            .map_err(SquareError::TreeOperational),
                        edited.root,
                    )
                }
            };
            Some(view.and_then(|view| {
                lens.put(&view, complement)
                    .map_err(graph_put_error)
                    .and_then(|restored| {
                        graph_to_tree(&restored, root, &schema_parent_name(Some(lens.src)))
                            .map(|tree| (tree, restored))
                            .map_err(tree_shape_error)
                    })
            }))
        });

    let fast_put_pair = fast_state.as_ref().ok().map(|fast| {
        let (view, embedding) = put_view.unwrap_or((&fast.view, &fast.embedding));
        tree_put_with_graph(lens, view, embedding, &fast.complement).map_err(graph_put_error)
    });
    let graph_put = graph_put_pair.as_ref().map(|result| match result {
        Ok((_, graph)) => Ok(graph.clone()),
        Err(error) => Err(error.clone()),
    });
    let graph_tree_put = graph_put_pair.as_ref().map(|result| match result {
        Ok((tree, _)) => Ok(tree.clone()),
        Err(error) => Err(error.clone()),
    });
    let fast_put = fast_put_pair.as_ref().map(|result| match result {
        Ok((_, graph)) => Ok(graph.clone()),
        Err(error) => Err(error.clone()),
    });
    let fast_tree_put = fast_put_pair.as_ref().map(|result| match result {
        Ok((tree, _)) => Ok(tree.clone()),
        Err(error) => Err(error.clone()),
    });

    let fast_tree_get = fast_state
        .as_ref()
        .map(|state| state.view.clone())
        .map_err(Clone::clone);
    let graph_tree_get = match &graph_get {
        Ok((view, _)) => graph_to_tree(view, source.root, &schema_parent_name(Some(lens.tgt)))
            .map_err(tree_shape_error),
        Err(error) => Err(error.clone()),
    };
    let fast_get = fast_state.map(|state| (state.graph_view, state.graph_complement));
    let observation = GraphSquareObservation {
        graph: SquareRouteObservation {
            get: graph_get,
            put: graph_put,
        },
        fast: SquareRouteObservation {
            get: fast_get,
            put: fast_put,
        },
        fast_tree_get,
        graph_tree_get,
        graph_tree_put,
        fast_tree_put,
    };
    classify_graph_observation(observation)
}

fn classify_graph_observation(
    observation: GraphSquareObservation,
) -> SquareResult<GraphSquareObservation> {
    if route_is_outside(&observation.graph) || route_is_outside(&observation.fast) {
        SquareResult::Refused(observation)
    } else if observation.graph == observation.fast
        && observation.graph_tree_get == observation.fast_tree_get
        && observation.graph_tree_put == observation.fast_tree_put
    {
        SquareResult::Commutes(observation)
    } else {
        SquareResult::Mismatch(observation)
    }
}

/// Definition: Execute the tree fast route and the direct graph-lens route,
/// then compare exact node ids, edge ids, edge order, payloads, complements,
/// get errors, and put results or errors.
///
/// Book anchors: `L-lenses-17`.
#[must_use]
pub fn graph_square(lens: &GraphLens<'_>, source: &Tree) -> SquareResult<GraphSquareObservation> {
    graph_square_impl(lens, source, None)
}

/// The graph/tree put square for a caller-supplied edited tree presentation.
///
/// Both routes derive their complement from `source`. `embedding` is the
/// durable graph-carrier presentation of `view`, including caller-chosen ids
/// for fresh arcs and fans. Put-domain and restored-tree errors are compared
/// after mapping both routes to graph coordinates.
#[must_use]
pub fn graph_put_square(
    lens: &GraphLens<'_>,
    source: &Tree,
    view: &Tree,
    embedding: &TreeEmbedding,
) -> SquareResult<GraphSquareObservation> {
    graph_square_impl(lens, source, Some((view, embedding)))
}

type Pair = (SEdge, u32, u32);
type PairIds = BTreeMap<Pair, u32>;

fn source_pair_id(
    graph_lens: &GraphLens<'_>,
    source_embedding: &BTreeMap<Pair, u32>,
    complement: &TableComplement,
    pair: &Pair,
) -> Result<u32, SquareError> {
    if let Some(originals) = complement.original_fk_edges.get(pair) {
        let mut originals = originals.iter();
        let Some(original) = originals.next() else {
            return Err(SquareError::OutsideExactDomain(
                "a table pair has several source graph-edge identities",
            ));
        };
        if originals.next().is_some() {
            return Err(SquareError::OutsideExactDomain(
                "a table pair has several source graph-edge identities",
            ));
        }
        return source_embedding
            .get(&(original.clone(), pair.1, pair.2))
            .copied()
            .ok_or(SquareError::OutsideExactDomain(
                "a recorded source pair has no graph-edge identity",
            ));
    }
    let candidates: Vec<u32> = source_embedding
        .iter()
        .filter_map(|((source_label, src, tgt), id)| {
            (*src == pair.1
                && *tgt == pair.2
                && matches!(
                    graph_lens.plan.fates.get(source_label),
                    Some(crate::restrict::EdgeFate::Keep(image)) if image == &pair.0
                ))
            .then_some(*id)
        })
        .collect();
    match candidates.as_slice() {
        [id] => Ok(*id),
        [] => Err(SquareError::OutsideExactDomain(
            "a table view pair has no source graph-edge identity",
        )),
        _ => Err(SquareError::OutsideExactDomain(
            "several source graph edges collapse to one table pair",
        )),
    }
}

fn synthesized_pair_presentation(
    source_graph: &Graph,
    complement: &TableComplement,
) -> Result<(PairIds, BTreeMap<u32, SynthKey>), SquareError> {
    let mut synthesized = BTreeMap::new();
    for pair @ (edge, src, tgt) in &complement.synthesized_fk_pairs {
        if complement.original_fk_edges.contains_key(pair) {
            return Err(SquareError::OutsideExactDomain(
                "a synthesized table pair also has a surviving direct origin",
            ));
        }
        synthesized.insert(
            SynthKey::Contract {
                src: *src,
                tgt: *tgt,
                edge: edge.clone(),
            },
            pair.clone(),
        );
    }

    let mut pair_ids = PairIds::new();
    let mut synthesized_edges = BTreeMap::new();
    let mut next_synth = floor_of(source_graph.edges.keys().copied());
    for (key, pair) in synthesized {
        if next_synth == u32::MAX {
            return Err(SquareError::OutsideExactDomain(
                "synthesized table-pair graph ids are exhausted",
            ));
        }
        pair_ids.insert(pair, next_synth);
        synthesized_edges.insert(next_synth, key);
        next_synth = next_synth.saturating_add(1);
    }
    Ok((pair_ids, synthesized_edges))
}

fn add_table_residue(
    source: &Table,
    source_graph: &Graph,
    source_embedding: &BTreeMap<Pair, u32>,
    complement: &TableComplement,
    pair_ids: &PairIds,
    graph_complement: &mut Complement,
) -> Result<(), SquareError> {
    for (id, dropped) in &complement.dropped_rows {
        let mut node = Node::new(*id, &dropped.table);
        node.fields.clone_from(&dropped.row);
        graph_complement.dropped_nodes.insert(*id, node);
    }
    for (label, pairs) in &complement.dropped_fk_pairs {
        for (src, tgt) in pairs {
            let key = (label.clone(), *src, *tgt);
            let Some(id) = source_embedding.get(&key).copied() else {
                return Err(SquareError::OutsideExactDomain(
                    "a dropped table pair has no graph-edge identity",
                ));
            };
            graph_complement
                .dropped_edges
                .insert(id, source_graph.edges[&id].clone());
        }
    }
    graph_complement
        .original_anchors
        .clone_from(&complement.original_tables);
    for id in source.rows.keys() {
        let changed = complement
            .dropped_cells
            .keys()
            .chain(complement.original_cells.keys())
            .any(|(row, _)| row == id);
        if changed {
            graph_complement
                .original_fields
                .insert(*id, source.rows[id].1.clone());
        }
    }
    for (pair, originals) in &complement.original_fk_edges {
        if complement.synthesized_fk_pairs.contains(pair) {
            continue;
        }
        let mut originals = originals.iter();
        let Some(original) = originals.next() else {
            return Err(SquareError::OutsideExactDomain(
                "a table pair has several source graph-edge labels",
            ));
        };
        if originals.next().is_some() {
            return Err(SquareError::OutsideExactDomain(
                "a table pair has several source graph-edge labels",
            ));
        }
        let Some(id) = pair_ids.get(pair).copied() else {
            return Err(SquareError::OutsideExactDomain(
                "a remapped table pair has no stable graph-edge id",
            ));
        };
        let mut edge = source_graph.edges[&id].clone();
        edge.label = original.clone();
        graph_complement.original_edges.insert(id, edge);
    }
    Ok(())
}

fn table_presentation(
    graph_lens: &GraphLens<'_>,
    source: &Table,
    source_graph: &Graph,
    source_embedding: &BTreeMap<Pair, u32>,
    view: &Table,
    complement: &TableComplement,
) -> Result<(Graph, Complement, PairIds), SquareError> {
    let mut graph = Graph::default();
    for (id, (table, cells)) in &view.rows {
        let mut node = Node::new(*id, table);
        node.fields.clone_from(cells);
        graph.nodes.insert(*id, node);
    }

    let (mut pair_ids, synthesized_edges) =
        synthesized_pair_presentation(source_graph, complement)?;
    let mut graph_complement = Complement {
        floor: (
            complement.id_floor,
            floor_of(source_graph.edges.keys().copied()),
        ),
        identity: Some(graph_lens.identity_for_shape(ComplementShape::Graph)),
        synthesized_edges,
        ..Complement::default()
    };

    for pair in &view.pairs {
        if complement.synthesized_fk_pairs.contains(pair) {
            continue;
        }
        let id = source_pair_id(graph_lens, source_embedding, complement, pair)?;
        pair_ids.insert(pair.clone(), id);
    }
    for pair in complement.original_fk_edges.keys() {
        if complement.synthesized_fk_pairs.contains(pair) {
            continue;
        }
        let id = source_pair_id(graph_lens, source_embedding, complement, pair)?;
        pair_ids.insert(pair.clone(), id);
    }
    for pair @ (label, src, tgt) in &view.pairs {
        let Some(id) = pair_ids.get(pair).copied() else {
            return Err(SquareError::OutsideExactDomain(
                "a table view pair has no graph presentation",
            ));
        };
        graph.edges.insert(
            id,
            GEdge {
                src: *src,
                tgt: *tgt,
                label: label.clone(),
                position: None,
            },
        );
    }

    add_table_residue(
        source,
        source_graph,
        source_embedding,
        complement,
        &pair_ids,
        &mut graph_complement,
    )?;
    Ok((graph, graph_complement, pair_ids))
}

fn table_ref_in_graph(reference: ViewRef, pair_ids: &PairIds) -> Result<ViewRef, SquareError> {
    match reference {
        ViewRef::Row(row) => Ok(ViewRef::Node(row)),
        ViewRef::Pair(edge, src, tgt) => pair_ids
            .get(&(edge.as_ref().clone(), src, tgt))
            .copied()
            .map(ViewRef::Edge)
            .ok_or(SquareError::OutsideExactDomain(
                "a table put error names a pair without a graph presentation",
            )),
        other => Ok(other),
    }
}

fn table_domain_in_graph(
    error: PutDomainError,
    pair_ids: &PairIds,
) -> Result<PutDomainError, SquareError> {
    Ok(match error {
        PutDomainError::IdCollision { elem } => PutDomainError::IdCollision {
            elem: table_ref_in_graph(elem, pair_ids)?,
        },
        PutDomainError::SynthesizedEdited { elem } => PutDomainError::SynthesizedEdited {
            elem: table_ref_in_graph(elem, pair_ids)?,
        },
        PutDomainError::ContractedEdgeInserted { elem } => PutDomainError::ContractedEdgeInserted {
            elem: table_ref_in_graph(elem, pair_ids)?,
        },
        PutDomainError::UnrepresentableViewEdge { elem } => {
            PutDomainError::UnrepresentableViewEdge {
                elem: table_ref_in_graph(elem, pair_ids)?,
            }
        }
        PutDomainError::AmbiguousReverseAnchor { elem, candidates } => {
            PutDomainError::AmbiguousReverseAnchor {
                elem: table_ref_in_graph(elem, pair_ids)?,
                candidates,
            }
        }
        PutDomainError::SurvivalPredicateFails { elem } => PutDomainError::SurvivalPredicateFails {
            elem: table_ref_in_graph(elem, pair_ids)?,
        },
        PutDomainError::DerivedCellEdited { elem, field } => PutDomainError::DerivedCellEdited {
            elem: table_ref_in_graph(elem, pair_ids)?,
            field,
        },
        PutDomainError::TransformNotInvertible { elem, field } => {
            PutDomainError::TransformNotInvertible {
                elem: table_ref_in_graph(elem, pair_ids)?,
                field,
            }
        }
        PutDomainError::DuplicateKey { table, key, elems } => PutDomainError::DuplicateKey {
            table,
            key,
            elems: elems
                .into_iter()
                .map(|elem| table_ref_in_graph(elem, pair_ids))
                .collect::<Result<Vec<_>, _>>()?,
        },
        PutDomainError::MissingRequiredCell { elem, field } => {
            PutDomainError::MissingRequiredCell {
                elem: table_ref_in_graph(elem, pair_ids)?,
                field,
            }
        }
        PutDomainError::MalformedFan { node, reason } => {
            PutDomainError::MalformedFan { node, reason }
        }
        PutDomainError::SurvivalPredicateRefused { elem, refusal } => {
            PutDomainError::SurvivalPredicateRefused {
                elem: table_ref_in_graph(elem, pair_ids)?,
                refusal,
            }
        }
    })
}

fn table_put_error(error: TablePutError, pair_ids: &PairIds) -> SquareError {
    match error {
        TablePutError::Identity(error) => SquareError::TableIdentity(error),
        TablePutError::Operational(error) => SquareError::TableOperational(error),
        TablePutError::Domain(error) => table_domain_in_graph(error, pair_ids)
            .map_or_else(|mapping| mapping, SquareError::PutDomain),
        TablePutError::NamedCollationUnresolved { names } => {
            SquareError::NamedCollationUnresolved { names }
        }
        TablePutError::NotAnIsomorphism { detail } => SquareError::NotAnIsomorphism { detail },
    }
}

struct FastTableState {
    view: Table,
    complement: TableComplement,
    graph_view: Graph,
    graph_complement: Complement,
    pair_ids: PairIds,
}

fn table_square_impl(
    graph_lens: &GraphLens<'_>,
    table_lens: &TableLens<'_>,
    source: &Table,
    put_view: Option<&Table>,
) -> SquareResult<TableSquareObservation> {
    let encoded = table_to_graph(source).map_err(SquareError::TableEncoding);
    let graph_get = match &encoded {
        Err(error) => Err(error.clone()),
        Ok((source_graph, _)) => graph_lens.get(source_graph).map_err(SquareError::Restrict),
    };
    let fast_state = match &encoded {
        Err(error) => Err(error.clone()),
        Ok((source_graph, source_embedding)) => table_lens
            .get(source)
            .map_err(SquareError::Restrict)
            .and_then(|(view, complement)| {
                table_presentation(
                    graph_lens,
                    source,
                    source_graph,
                    source_embedding,
                    &view,
                    &complement,
                )
                .map(|(graph_view, graph_complement, pair_ids)| FastTableState {
                    view,
                    complement,
                    graph_view,
                    graph_complement,
                    pair_ids,
                })
            }),
    };

    let graph_put = graph_get
        .as_ref()
        .ok()
        .and_then(|(default_view, complement)| {
            let graph_view = match (put_view, &encoded, &fast_state) {
                (None, _, _) => Ok(default_view.clone()),
                (Some(edited), Ok((source_graph, source_embedding)), Ok(fast)) => {
                    table_presentation(
                        graph_lens,
                        source,
                        source_graph,
                        source_embedding,
                        edited,
                        &fast.complement,
                    )
                    .map(|(view, _, _)| view)
                }
                (Some(_), _, _) => return None,
            };
            Some(
                graph_view
                    .and_then(|view| graph_lens.put(&view, complement).map_err(graph_put_error)),
            )
        });
    let fast_put = fast_state.as_ref().ok().map(|fast| {
        let view = put_view.unwrap_or(&fast.view);
        let pair_ids = if put_view.is_some() {
            match &encoded {
                Ok((source_graph, source_embedding)) => table_presentation(
                    graph_lens,
                    source,
                    source_graph,
                    source_embedding,
                    view,
                    &fast.complement,
                )
                .map(|(_, _, pair_ids)| pair_ids),
                Err(error) => Err(error.clone()),
            }
        } else {
            Ok(fast.pair_ids.clone())
        }?;
        table::put(table_lens, view, &fast.complement)
            .map_err(|error| table_put_error(error, &pair_ids))
            .and_then(|restored| {
                table_to_graph(&restored)
                    .map(|(graph, _)| graph)
                    .map_err(SquareError::TableEncoding)
            })
    });

    let observation = TableSquareObservation {
        graph: SquareRouteObservation {
            get: graph_get,
            put: graph_put,
        },
        fast: SquareRouteObservation {
            get: fast_state.map(|state| (state.graph_view, state.graph_complement)),
            put: fast_put,
        },
    };
    if route_is_outside(&observation.graph) || route_is_outside(&observation.fast) {
        SquareResult::Refused(observation)
    } else if observation.graph == observation.fast {
        SquareResult::Commutes(observation)
    } else {
        SquareResult::Mismatch(observation)
    }
}

/// Definition: Execute the table fast route and the direct graph-lens route,
/// then compare exact row/node ids, pair/edge ids, payloads, complements,
/// get errors, and unedited put results or errors.
///
/// Book anchors: `L-lenses-18`.
#[must_use]
pub fn table_square(
    graph_lens: &GraphLens<'_>,
    table_lens: &TableLens<'_>,
    source: &Table,
) -> SquareResult<TableSquareObservation> {
    table_square_impl(graph_lens, table_lens, source, None)
}

/// The table put square for a caller-supplied edited view.
///
/// Both routes first derive their own complement from `source`. The graph
/// route presents `view` using source-edge preimages and synthesized keys;
/// the table route consumes the same view directly. Put-domain errors are
/// mapped back to graph node and edge coordinates before comparison.
#[must_use]
pub fn table_put_square(
    graph_lens: &GraphLens<'_>,
    table_lens: &TableLens<'_>,
    source: &Table,
    view: &Table,
) -> SquareResult<TableSquareObservation> {
    table_square_impl(graph_lens, table_lens, source, Some(view))
}

#[cfg(test)]
mod graph_square_tests {
    use std::collections::BTreeMap;

    use super::{
        GraphSquareObservation, SquareResult, SquareRouteObservation, classify_graph_observation,
        square_tree_view_graph,
    };
    use crate::migration::Migration;
    use crate::model::{Fan, Graph, Node, SEdge, Tree};
    use crate::pages::lenses::graph::GraphLens;
    use crate::restrict::{Complement, NoValueLayer, compile};
    use crate::schema::{HyperEdge, Schema};

    fn ordered(children: &[u32]) -> Tree {
        Tree {
            nodes: std::iter::once((0, Node::new(0, "r")))
                .chain(
                    children
                        .iter()
                        .copied()
                        .map(|child| (child, Node::new(child, "leaf"))),
                )
                .collect(),
            arcs: children
                .iter()
                .copied()
                .map(|child| (0, child, SEdge::new("r", "leaf", "child", Some("leaf"))))
                .collect(),
            fans: Vec::new(),
            root: 0,
        }
    }

    #[test]
    fn a_distinct_three_sibling_permutation_is_a_square_mismatch() {
        let route = SquareRouteObservation {
            get: Ok((Graph::default(), Complement::default())),
            put: Some(Ok(Graph::default())),
        };
        let observation = GraphSquareObservation {
            graph: route.clone(),
            fast: route,
            fast_tree_get: Ok(ordered(&[1, 2, 3])),
            graph_tree_get: Ok(ordered(&[1, 2, 3])),
            graph_tree_put: Some(Ok(ordered(&[2, 3, 1]))),
            fast_tree_put: Some(Ok(ordered(&[3, 1, 2]))),
        };
        assert!(matches!(
            classify_graph_observation(observation),
            SquareResult::Mismatch(_)
        ));
    }

    #[test]
    fn a_distinct_graph_get_order_is_a_square_mismatch() {
        let route = SquareRouteObservation {
            get: Ok((Graph::default(), Complement::default())),
            put: Some(Ok(Graph::default())),
        };
        let observation = GraphSquareObservation {
            graph: route.clone(),
            fast: route,
            fast_tree_get: Ok(ordered(&[2, 3, 1])),
            graph_tree_get: Ok(ordered(&[3, 1, 2])),
            graph_tree_put: Some(Ok(ordered(&[2, 3, 1]))),
            fast_tree_put: Some(Ok(ordered(&[2, 3, 1]))),
        };
        assert!(matches!(
            classify_graph_observation(observation),
            SquareResult::Mismatch(_)
        ));
    }

    #[test]
    fn square_fan_allocation_avoids_collisions_and_retired_ids_independently() {
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
            fans: vec![(0, 0, [("x".to_owned(), 0)].into())],
            next_node: 40,
            next_edge: 70,
        };
        let Ok(graph) = square_tree_view_graph(&lens, &view, &embedding, &Complement::default())
        else {
            panic!("square carrier allocation must succeed");
        };
        assert_eq!(
            graph.nodes.keys().copied().collect::<Vec<_>>(),
            vec![0, 1, 40]
        );
        assert_eq!(
            graph.edges.keys().copied().collect::<Vec<_>>(),
            vec![0, 70, 71]
        );
        assert_eq!(graph.edges[&0].label, edge);
    }
}
