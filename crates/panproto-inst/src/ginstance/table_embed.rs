//! Tables as graph instances.
//!
//! [`GInstance::from_finstance`] lays a table instance out as a graph whose
//! nodes are rows and whose edges are foreign-key pairs, and returns the
//! [`TableEmbedding`] that records which edge each pair became.
//! [`GInstance::table_shape`] decides whether a graph is such a layout, and
//! [`GInstance::try_to_finstance`] reads the table instance back out.
//!
//! For relational foreign keys this is the typed-graph encoding (Brown et al.
//! 2022, Section 2, stated: typed-graph categories are the C-set categories
//! whose instances are sets and relations): rows correspond one to one with
//! nodes and pairs with edge ids, by construction (inferred, and pinned by a
//! property test). Only where the schema declares a foreign key functional and
//! total is the layout the category of elements (Spivak and Wisnesky 2015,
//! Def 8.4.2, with the equivalence of Prop 8.4.7, stated).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;

use panproto_gat::Name;
use panproto_schema::{Cardinality, Edge, Schema};
use serde::{Deserialize, Serialize};

use super::reify::{HYPER_LABEL_KIND, HYPER_PARENT_KIND};
use super::{EdgeId, GEdge, GInstance};
use crate::error::InstError;
use crate::functor::FInstance;
use crate::metadata::{Node, NodeShape};
use crate::segment::{IdAllocator, IdKind, IdRange};
use crate::value::Value;
use crate::view_ref::{RowId, ViewRef};

/// How a table instance sits inside its graph form.
///
/// A row keeps its id as its node id (row `r` is node `r.0`); the pair
/// `(edge, src, tgt)` of a foreign key is the graph edge `pairs[(edge, src,
/// tgt)]`. `next_edge` is the floor from which the embedding allocates the
/// edge id of a pair it does not yet record; it never decreases, and every
/// allocation goes through an [`IdAllocator`] resumed from it. The embedding
/// is durable: it is threaded through table edits so that a pair keeps its
/// edge id while the table changes. Serialized as a struct with named fields,
/// the pair map as a list of entries in ascending key order.
///
/// ```
/// use std::collections::HashMap;
///
/// use panproto_inst::{EdgeId, FInstance, GInstance, RowId, ViewRef};
/// use panproto_schema::Edge;
///
/// let fk = Edge { src: "post".into(), tgt: "user".into(), kind: "fk".into(), name: None };
/// let tables = FInstance::new()
///     .with_rows("user", [(RowId(4), HashMap::new())])?
///     .with_rows("post", [(RowId(9), HashMap::new())])?
///     .with_foreign_key(fk.clone(), [(RowId(9), RowId(4))])?;
/// let (graph, embedding) = GInstance::from_finstance(&tables)?;
/// assert_eq!(graph.edge(EdgeId(0)).map(|e| (e.src, e.tgt)), Some((9, 4)));
/// assert_eq!(embedding.next_edge, 1);
/// assert_eq!(
///     embedding.map_ref(&ViewRef::Pair { edge: fk, src: RowId(9), tgt: RowId(4) }),
///     Some(ViewRef::Edge(EdgeId(0)))
/// );
/// assert_eq!(embedding.map_ref(&ViewRef::Row(RowId(9))), Some(ViewRef::Node(9)));
/// # Ok::<(), panproto_inst::InstError>(())
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TableEmbedding {
    /// The graph edge of each foreign-key pair.
    #[serde(with = "panproto_schema::serde_helpers::btree_map_as_vec")]
    pub pairs: BTreeMap<(Edge, RowId, RowId), EdgeId>,
    /// The least edge id above every recorded pair's, saturated at
    /// `u32::MAX`: allocation never hands out `u32::MAX` itself, so an
    /// embedding whose floor has reached it allocates nothing, while a graph
    /// that already uses that id still reads back with its embedding.
    pub next_edge: u32,
}

impl TableEmbedding {
    /// Translate a table's element reference into its graph form's.
    ///
    /// `Row(r)` is `Node(r.0)`, since rows keep their ids; a `Pair` is the
    /// `Edge` the embedding records for it. A graph reference (`Node`,
    /// `Edge`) is returned unchanged. A tree reference (`Arc`), or a pair the
    /// embedding does not record, has no image.
    #[must_use]
    pub fn map_ref(&self, reference: &ViewRef) -> Option<ViewRef> {
        match reference {
            ViewRef::Row(r) => Some(ViewRef::Node(r.0)),
            ViewRef::Pair { edge, src, tgt } => self
                .pairs
                .get(&(edge.clone(), *src, *tgt))
                .map(|e| ViewRef::Edge(*e)),
            ViewRef::Node(n) => Some(ViewRef::Node(*n)),
            ViewRef::Edge(e) => Some(ViewRef::Edge(*e)),
            ViewRef::Arc { .. } => None,
        }
    }

    /// Allocate a fresh edge id from `next_edge` and raise the floor past it.
    ///
    /// # Errors
    ///
    /// Returns [`InstError::EdgeIdsExhausted`] when the floor is `u32::MAX`
    /// (no floor could be stored after that id); the embedding is then
    /// unchanged.
    pub fn allocate_edge(&mut self) -> Result<EdgeId, InstError> {
        let mut allocator = IdAllocator::from_floor(IdRange::FULL, self.next_edge);
        let id = allocator
            .allocate()
            .map_err(|e| e.whole_state(IdKind::Edge))?;
        self.next_edge = allocator.floor().ok_or(InstError::EdgeIdsExhausted)?;
        Ok(EdgeId(id))
    }

    /// The edge of a pair: the recorded one, or a fresh one that is then
    /// recorded.
    ///
    /// # Errors
    ///
    /// As [`Self::allocate_edge`].
    pub fn edge_for(&mut self, edge: &Edge, src: RowId, tgt: RowId) -> Result<EdgeId, InstError> {
        let key = (edge.clone(), src, tgt);
        if let Some(id) = self.pairs.get(&key) {
            return Ok(*id);
        }
        let id = self.allocate_edge()?;
        self.pairs.insert(key, id);
        Ok(id)
    }
}

/// Why a graph instance is not the graph form of a table instance.
///
/// Serialized externally tagged in `snake_case`, for instance
/// `{"fan_node": {"node": 3}}`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotTableReason {
    /// A node carries data a row does not have: a value, a discriminator, a
    /// position, a shape other than plain, or annotations.
    NodeDataNotRepresentable {
        /// The node.
        node: u32,
    },
    /// A node's field holds a nested record or list, which a cell does not.
    NestedCell {
        /// The node.
        node: u32,
        /// The field.
        field: String,
    },
    /// A node is a reified fan, or the source of an edge of a reserved fan
    /// kind.
    FanNode {
        /// The node.
        node: u32,
    },
    /// An edge's label is not an edge of the schema.
    EdgeNotForeignKey {
        /// The edge.
        edge: EdgeId,
    },
    /// An edge's endpoints are not anchored at its label's source and target.
    IncoherentEndpoints {
        /// The edge.
        edge: EdgeId,
    },
    /// An edge carries a position; a foreign-key pair has none.
    EdgePositioned {
        /// The edge.
        edge: EdgeId,
    },
    /// Two or more edges share their label and endpoints; a foreign key is a
    /// set of pairs.
    ParallelForeignKey {
        /// The edges, ascending.
        edges: Vec<EdgeId>,
    },
    /// A node has more than one edge of a foreign key the schema declares
    /// functional.
    MultipleLifts {
        /// The node.
        node: u32,
        /// The foreign key.
        edge: Edge,
        /// Its edges out of the node, ascending.
        edges: Vec<EdgeId>,
    },
    /// A node has no edge of a foreign key the schema declares required.
    MissingLift {
        /// The node.
        node: u32,
        /// The foreign key.
        edge: Edge,
    },
}

impl fmt::Display for NotTableReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NodeDataNotRepresentable { node } => {
                write!(f, "node {node} carries data a row does not have")
            }
            Self::NestedCell { node, field } => {
                write!(f, "field `{field}` of node {node} is not a flat cell")
            }
            Self::FanNode { node } => write!(f, "node {node} is a fan"),
            Self::EdgeNotForeignKey { edge } => {
                write!(f, "edge {edge} is not labelled with a schema edge")
            }
            Self::IncoherentEndpoints { edge } => {
                write!(f, "edge {edge} does not join its label's endpoint tables")
            }
            Self::EdgePositioned { edge } => write!(f, "edge {edge} carries a position"),
            Self::ParallelForeignKey { edges } => {
                write!(f, "edges {edges:?} are one foreign-key pair")
            }
            Self::MultipleLifts { node, edges, .. } => {
                write!(
                    f,
                    "node {node} has {} edges of a functional foreign key",
                    edges.len()
                )
            }
            Self::MissingLift { node, edge } => {
                write!(
                    f,
                    "node {node} has no edge of the required foreign key {edge:?}"
                )
            }
        }
    }
}

/// The table shape of a graph instance.
///
/// Serialized externally tagged in `snake_case`: `"table"` or
/// `{"not_table": {"reason": ...}}`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TableShapeReport {
    /// The graph is the graph form of a table instance.
    Table,
    /// The graph is not.
    NotTable {
        /// Why not.
        reason: NotTableReason,
    },
}

/// The foreign keys a schema declares functional (some `Reference` with this
/// edge has cardinality one) and, among those, the ones it declares required
/// (such a reference is also not nullable).
fn declared_lifts(schema: &Schema) -> (BTreeSet<Edge>, BTreeSet<Edge>) {
    let mut functional = BTreeSet::new();
    let mut required = BTreeSet::new();
    for reference in schema.declarations.references.values() {
        let Some(edge) = &reference.edge else {
            continue;
        };
        if reference.cardinality == Cardinality::One {
            functional.insert(edge.clone());
            if !reference.nullable {
                required.insert(edge.clone());
            }
        }
    }
    (functional, required)
}

/// Whether a node carries only what a row has: an id, an anchor and cells.
fn row_like(node: &Node) -> bool {
    node.value.is_none()
        && node.discriminator.is_none()
        && node.position.is_none()
        && node.shape == NodeShape::Plain
        && node.annotations.is_empty()
}

/// Whether an edge kind is one of the reserved fan kinds.
fn reserved_kind(kind: &Name) -> bool {
    kind.as_ref() == HYPER_PARENT_KIND || kind.as_ref() == HYPER_LABEL_KIND
}

impl GInstance {
    /// The graph form of a table instance, with the embedding that relates
    /// the two.
    ///
    /// Row `r` becomes node `r.0`, anchored at its table, with the row's
    /// cells as its extra fields and no value, discriminator, position,
    /// shape or annotations. The pair `(e, s, t)` becomes an edge from node
    /// `s.0` to node `t.0` labelled `e` with no position; the pairs are taken
    /// in ascending `(e, s, t)` and receive the edge ids `0, 1, 2, ...` in
    /// that order, which the returned [`TableEmbedding`] records. This is
    /// [`Self::from_finstance_along`] from the empty embedding.
    ///
    /// # Errors
    ///
    /// Returns [`InstError::EdgeIdsExhausted`] when the pairs outnumber the
    /// edge ids.
    pub fn from_finstance(tables: &FInstance) -> Result<(Self, TableEmbedding), InstError> {
        let mut embedding = TableEmbedding::default();
        let graph = Self::from_finstance_along(tables, &mut embedding)?;
        Ok((graph, embedding))
    }

    /// The graph form of a table instance along a durable embedding.
    ///
    /// Rows become nodes as in [`Self::from_finstance`]. A pair the embedding
    /// records keeps its recorded edge id; a pair it does not record takes
    /// `EdgeId(next_edge)`, the pairs taken in ascending `(e, s, t)`, and is
    /// recorded. So the graph a table was read from ([`Self::try_to_finstance`])
    /// is returned exactly when the table is laid out along the embedding
    /// read with it.
    ///
    /// # Errors
    ///
    /// Returns [`InstError::EdgeIdsExhausted`] when no edge id is left for
    /// a new pair, and [`InstError::DuplicateEdge`] when the embedding
    /// records one edge id for two pairs of the table.
    pub fn from_finstance_along(
        tables: &FInstance,
        embedding: &mut TableEmbedding,
    ) -> Result<Self, InstError> {
        let mut graph = Self::new();
        for (table, rows) in tables.tables() {
            for (id, row) in rows.rows() {
                let mut node = Node::new(id.0, table.clone());
                node.extra_fields.clone_from(row);
                graph.insert_node(node)?;
            }
        }
        for (edge, src, tgt) in tables.all_pairs() {
            let id = embedding.edge_for(&edge, src, tgt)?;
            graph.insert_edge(id, GEdge::new(src.0, tgt.0, edge))?;
        }
        Ok(graph)
    }

    /// Decide whether this graph is the graph form of a table instance of
    /// `schema`, in time linear in its nodes and edges (and, per node, in
    /// the foreign keys the schema declares required out of its table).
    ///
    /// The failure reported is the first in this order: nodes in ascending
    /// id, then edges in ascending [`EdgeId`], then parallel edges, then
    /// lifts. A node must be a plain row ([`NotTableReason::FanNode`],
    /// [`NotTableReason::NodeDataNotRepresentable`]) whose fields hold no
    /// nested record or list ([`NotTableReason::NestedCell`], naming the
    /// least such field). An edge must carry no position, be labelled with a
    /// schema edge and join nodes anchored at the label's endpoints. No two
    /// edges may share label and endpoints. Where the schema declares a
    /// foreign key functional, a node has at most one edge of it; where it
    /// declares one required, every node of its source table has one.
    #[must_use]
    pub fn table_shape(&self, schema: &Schema) -> TableShapeReport {
        self.table_violation(schema)
            .map_or(TableShapeReport::Table, |reason| {
                TableShapeReport::NotTable { reason }
            })
    }

    /// The violation of the least node, if any node violates.
    fn node_violation(&self) -> Option<NotTableReason> {
        let mut first: Option<(u32, NotTableReason)> = None;
        for (&id, node) in self.nodes() {
            if first.as_ref().is_some_and(|(least, _)| *least < id) {
                continue;
            }
            let fan_source = self
                .out_edges(id)
                .iter()
                .filter_map(|e| self.edge(*e))
                .any(|e| reserved_kind(&e.edge.kind));
            let reason = if node.shape == NodeShape::HyperEdge || fan_source {
                Some(NotTableReason::FanNode { node: id })
            } else if !row_like(node) {
                Some(NotTableReason::NodeDataNotRepresentable { node: id })
            } else {
                node.extra_fields
                    .iter()
                    .filter(|(_, v)| matches!(v, Value::Unknown(_) | Value::List(_)))
                    .map(|(field, _)| field)
                    .min()
                    .map(|field| NotTableReason::NestedCell {
                        node: id,
                        field: field.clone(),
                    })
            };
            if let Some(reason) = reason {
                first = Some((id, reason));
            }
        }
        first.map(|(_, reason)| reason)
    }

    /// The violation of a single edge, checked in the documented order.
    fn edge_violation(&self, schema: &Schema, id: EdgeId, edge: &GEdge) -> Option<NotTableReason> {
        if edge.position.is_some() {
            return Some(NotTableReason::EdgePositioned { edge: id });
        }
        if !schema.edges.contains_key(&edge.edge) {
            return Some(NotTableReason::EdgeNotForeignKey { edge: id });
        }
        let coherent = self.node(edge.src).map(|n| &n.anchor) == Some(&edge.edge.src)
            && self.node(edge.tgt).map(|n| &n.anchor) == Some(&edge.edge.tgt);
        (!coherent).then_some(NotTableReason::IncoherentEndpoints { edge: id })
    }

    fn table_violation(&self, schema: &Schema) -> Option<NotTableReason> {
        if let Some(reason) = self.node_violation() {
            return Some(reason);
        }

        let mut first_edge: Option<(EdgeId, NotTableReason)> = None;
        let mut seen: HashMap<(&Edge, u32, u32), Vec<EdgeId>> = HashMap::new();
        let mut lifts: HashMap<(u32, &Edge), Vec<EdgeId>> = HashMap::new();
        for (id, edge) in self.edges() {
            if first_edge.as_ref().is_some_and(|(least, _)| least < id) {
                continue;
            }
            if let Some(reason) = self.edge_violation(schema, *id, edge) {
                first_edge = Some((*id, reason));
                continue;
            }
            seen.entry((&edge.edge, edge.src, edge.tgt))
                .or_default()
                .push(*id);
            lifts.entry((edge.src, &edge.edge)).or_default().push(*id);
        }
        if let Some((_, reason)) = first_edge {
            return Some(reason);
        }
        if let Some(mut edges) = seen
            .into_values()
            .filter(|edges| edges.len() > 1)
            .min_by_key(|edges| edges.iter().min().copied())
        {
            edges.sort_unstable();
            return Some(NotTableReason::ParallelForeignKey { edges });
        }

        let (functional, required) = declared_lifts(schema);
        if let Some(((node, edge), edges)) = lifts
            .iter()
            .filter(|((_, edge), edges)| edges.len() > 1 && functional.contains(*edge))
            .min_by_key(|(key, _)| *key)
        {
            let mut edges = edges.clone();
            edges.sort_unstable();
            return Some(NotTableReason::MultipleLifts {
                node: *node,
                edge: (*edge).clone(),
                edges,
            });
        }
        let mut required_from: HashMap<&Name, Vec<&Edge>> = HashMap::new();
        for edge in &required {
            required_from.entry(&edge.src).or_default().push(edge);
        }
        let mut missing: Option<(&Edge, u32)> = None;
        for (&id, node) in self.nodes() {
            for &edge in required_from.get(&node.anchor).into_iter().flatten() {
                if !lifts.contains_key(&(id, edge))
                    && missing.is_none_or(|least| (edge, id) < least)
                {
                    missing = Some((edge, id));
                }
            }
        }
        missing.map(|(edge, node)| NotTableReason::MissingLift {
            node,
            edge: edge.clone(),
        })
    }

    /// Read the table instance out of a graph that [`Self::table_shape`]
    /// accepts, with the embedding that relates the two. Node ids become row
    /// ids and each edge's id is recorded for its pair; `next_edge` is the
    /// least edge id above every edge of the graph, saturated at `u32::MAX`
    /// (see [`TableEmbedding::next_edge`]).
    ///
    /// # Errors
    ///
    /// Returns [`InstError::NotATable`] with the reason when
    /// [`Self::table_shape`] rejects the graph.
    pub fn try_to_finstance(
        &self,
        schema: &Schema,
    ) -> Result<(FInstance, TableEmbedding), InstError> {
        if let Some(reason) = self.table_violation(schema) {
            return Err(InstError::NotATable { reason });
        }
        let mut tables = FInstance::new();
        for (&id, node) in self.nodes() {
            tables.insert_row(node.anchor.clone(), RowId(id), node.extra_fields.clone())?;
        }
        let mut embedding = TableEmbedding::default();
        let mut allocator = IdAllocator::new(IdRange::FULL);
        let mut pairs: HashMap<&Edge, Vec<(RowId, RowId)>> = HashMap::new();
        for (id, edge) in self.edges() {
            pairs
                .entry(&edge.edge)
                .or_default()
                .push((RowId(edge.src), RowId(edge.tgt)));
            embedding
                .pairs
                .insert((edge.edge.clone(), RowId(edge.src), RowId(edge.tgt)), *id);
            allocator.observe(id.0);
        }
        for (edge, pairs) in pairs {
            tables.insert_foreign_keys(edge.clone(), pairs)?;
        }
        // Saturate: a graph using `EdgeId(u32::MAX)` has no floor above its
        // edges, and the saturated floor allocates nothing.
        embedding.next_edge = allocator.floor().unwrap_or(u32::MAX);
        Ok((tables, embedding))
    }
}
