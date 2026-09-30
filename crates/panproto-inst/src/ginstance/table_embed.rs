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
    /// The least edge id not yet used by a recorded pair.
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
    /// Returns [`InstError::EdgeIdsExhausted`] when no edge id is left, the
    /// id `u32::MAX` included (no floor could be stored after it).
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
    /// that order, which the returned [`TableEmbedding`] records.
    ///
    /// # Errors
    ///
    /// Returns [`InstError::EdgeIdsExhausted`] when the pairs outnumber the
    /// edge ids.
    pub fn from_finstance(tables: &FInstance) -> Result<(Self, TableEmbedding), InstError> {
        let mut graph = Self::new();
        for id in tables.row_ids() {
            let (Some(table), Some(row)) = (tables.table_of(id), tables.row(id)) else {
                continue;
            };
            let mut node = Node::new(id.0, table.clone());
            node.extra_fields.clone_from(row);
            graph.insert_node(node)?;
        }
        let mut embedding = TableEmbedding::default();
        for (edge, src, tgt) in tables.all_pairs() {
            let id = embedding.edge_for(&edge, src, tgt)?;
            graph.insert_edge(id, GEdge::new(src.0, tgt.0, edge))?;
        }
        Ok((graph, embedding))
    }

    /// Decide whether this graph is the graph form of a table instance of
    /// `schema`, in time linear in its nodes and edges.
    ///
    /// Nodes are examined in ascending id and then edges in ascending
    /// [`EdgeId`]; the first failure is reported. A node must be a plain row
    /// ([`NotTableReason::FanNode`], [`NotTableReason::NodeDataNotRepresentable`])
    /// whose fields hold no nested record or list
    /// ([`NotTableReason::NestedCell`]). An edge must carry no position, be
    /// labelled with a schema edge and join nodes anchored at the label's
    /// endpoints. No two edges may share label and endpoints. Where the
    /// schema declares a foreign key functional, a node has at most one edge
    /// of it; where it declares one required, every node of its source table
    /// has one.
    #[must_use]
    pub fn table_shape(&self, schema: &Schema) -> TableShapeReport {
        self.table_violation(schema)
            .map_or(TableShapeReport::Table, |reason| {
                TableShapeReport::NotTable { reason }
            })
    }

    fn table_violation(&self, schema: &Schema) -> Option<NotTableReason> {
        let mut ids: Vec<u32> = self.nodes().keys().copied().collect();
        ids.sort_unstable();
        for &id in &ids {
            let node = &self.nodes()[&id];
            let fan_source = self
                .out_edges(id)
                .iter()
                .filter_map(|e| self.edge(*e))
                .any(|e| reserved_kind(&e.edge.kind));
            if node.shape == NodeShape::HyperEdge || fan_source {
                return Some(NotTableReason::FanNode { node: id });
            }
            if !row_like(node) {
                return Some(NotTableReason::NodeDataNotRepresentable { node: id });
            }
            let mut fields: Vec<(&String, &Value)> = node.extra_fields.iter().collect();
            fields.sort_unstable_by(|a, b| a.0.cmp(b.0));
            if let Some((field, _)) = fields
                .into_iter()
                .find(|(_, v)| matches!(v, Value::Unknown(_) | Value::List(_)))
            {
                return Some(NotTableReason::NestedCell {
                    node: id,
                    field: field.clone(),
                });
            }
        }

        let mut seen: HashMap<(&Edge, u32, u32), Vec<EdgeId>> = HashMap::new();
        let mut lifts: BTreeMap<(u32, &Edge), Vec<EdgeId>> = BTreeMap::new();
        for (id, edge) in self.edges_sorted() {
            if edge.position.is_some() {
                return Some(NotTableReason::EdgePositioned { edge: id });
            }
            if !schema.edges.contains_key(&edge.edge) {
                return Some(NotTableReason::EdgeNotForeignKey { edge: id });
            }
            let coherent = self.node(edge.src).map(|n| &n.anchor) == Some(&edge.edge.src)
                && self.node(edge.tgt).map(|n| &n.anchor) == Some(&edge.edge.tgt);
            if !coherent {
                return Some(NotTableReason::IncoherentEndpoints { edge: id });
            }
            seen.entry((&edge.edge, edge.src, edge.tgt))
                .or_default()
                .push(id);
            lifts.entry((edge.src, &edge.edge)).or_default().push(id);
        }
        if let Some(edges) = seen
            .into_values()
            .filter(|edges| edges.len() > 1)
            .min_by_key(|edges| edges[0])
        {
            return Some(NotTableReason::ParallelForeignKey { edges });
        }

        let (functional, required) = declared_lifts(schema);
        for ((node, edge), edges) in &lifts {
            if edges.len() > 1 && functional.contains(*edge) {
                return Some(NotTableReason::MultipleLifts {
                    node: *node,
                    edge: (*edge).clone(),
                    edges: edges.clone(),
                });
            }
        }
        for edge in &required {
            for &id in &ids {
                if self.nodes()[&id].anchor == edge.src && !lifts.contains_key(&(id, edge)) {
                    return Some(NotTableReason::MissingLift {
                        node: id,
                        edge: edge.clone(),
                    });
                }
            }
        }
        None
    }

    /// Read the table instance out of a graph that [`Self::table_shape`]
    /// accepts, with the embedding that relates the two. Node ids become row
    /// ids and each edge's id is recorded for its pair; `next_edge` is the
    /// least edge id above every edge of the graph.
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
        let mut ids: Vec<u32> = self.nodes().keys().copied().collect();
        ids.sort_unstable();
        for id in ids {
            let node = &self.nodes()[&id];
            tables.insert_row(node.anchor.clone(), RowId(id), node.extra_fields.clone())?;
        }
        let mut embedding = TableEmbedding::default();
        let mut allocator = IdAllocator::new(IdRange::FULL);
        for (id, edge) in self.edges_sorted() {
            tables.insert_foreign_key(edge.edge.clone(), RowId(edge.src), RowId(edge.tgt))?;
            embedding
                .pairs
                .insert((edge.edge.clone(), RowId(edge.src), RowId(edge.tgt)), id);
            allocator.observe(id.0);
        }
        embedding.next_edge = allocator.floor().ok_or(InstError::EdgeIdsExhausted)?;
        Ok((tables, embedding))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use panproto_schema::{CanonicalWriter, OnUnresolved, RefSource, Reference, ResolveMode};
    use proptest::prelude::*;

    use super::*;
    use crate::functor::Row;
    use crate::ginstance::canonical::{write_edge_record, write_node_record};
    use crate::value::FieldPresence;

    fn author() -> Edge {
        Edge {
            src: "post".into(),
            tgt: "user".into(),
            kind: "fk".into(),
            name: Some("author".into()),
        }
    }

    fn cell(v: i64) -> Row {
        HashMap::from([("n".to_owned(), Value::Int(v))])
    }

    /// Users 4 and 1, posts 9 and 6; post 9 by user 4, post 6 by users 1
    /// and 4.
    fn tables() -> FInstance {
        FInstance::new()
            .with_rows("user", [(RowId(4), cell(40)), (RowId(1), cell(10))])
            .and_then(|f| f.with_rows("post", [(RowId(9), cell(90)), (RowId(6), cell(60))]))
            .and_then(|f| {
                f.with_foreign_key(
                    author(),
                    [
                        (RowId(9), RowId(4)),
                        (RowId(6), RowId(4)),
                        (RowId(6), RowId(1)),
                    ],
                )
            })
            .unwrap()
    }

    fn schema() -> Schema {
        let mut schema = Schema::default();
        schema.edges.insert(author(), Name::from("author"));
        schema
    }

    /// The schema with `author` declared functional, and required unless
    /// `nullable`.
    fn declaring(nullable: bool) -> Schema {
        let mut schema = schema();
        schema.declarations.references.insert(
            Name::from("author"),
            Reference {
                id: Name::from("author"),
                holder: Name::from("post"),
                source: RefSource::KeyValue { parts: Vec::new() },
                target_vertex: Name::from("user"),
                target_key: None,
                edge: Some(author()),
                cardinality: Cardinality::One,
                nullable,
                on_unresolved: OnUnresolved::Error,
                mode: ResolveMode::AddEdge,
            },
        );
        schema
    }

    #[test]
    fn from_finstance_maps_rows_to_nodes_with_cells_as_fields() {
        let (graph, _) = GInstance::from_finstance(&tables()).unwrap();
        assert_eq!(graph.node_count(), 4);
        for id in [1, 4, 6, 9] {
            let node = graph.node(id).unwrap();
            let mut expected = Node::new(id, if id < 5 { "user" } else { "post" });
            expected.extra_fields = cell(i64::from(id) * 10);
            assert_eq!(
                serde_json::to_value(node).unwrap(),
                serde_json::to_value(&expected).unwrap(),
                "row {id} is node {id} with default node data"
            );
        }
    }

    #[test]
    fn from_finstance_maps_pairs_to_labelled_edges() {
        let (graph, embedding) = GInstance::from_finstance(&tables()).unwrap();
        assert_eq!(graph.edge_count(), 3);
        for ((edge, src, tgt), id) in &embedding.pairs {
            assert_eq!(
                graph.edge(*id),
                Some(&GEdge::new(src.0, tgt.0, edge.clone())),
                "each pair is an unpositioned edge labelled with its foreign key"
            );
        }
    }

    #[test]
    fn the_initial_table_encoding_allocates_edges_in_pair_order() {
        let (_, embedding) = GInstance::from_finstance(&tables()).unwrap();
        let order: Vec<((RowId, RowId), EdgeId)> = embedding
            .pairs
            .iter()
            .map(|((_, s, t), id)| ((*s, *t), *id))
            .collect();
        assert_eq!(
            order,
            vec![
                ((RowId(6), RowId(1)), EdgeId(0)),
                ((RowId(6), RowId(4)), EdgeId(1)),
                ((RowId(9), RowId(4)), EdgeId(2)),
            ]
        );
        assert_eq!(embedding.next_edge, 3);
    }

    #[test]
    fn table_map_ref_sends_rows_and_pairs_to_graph_refs() {
        let (_, embedding) = GInstance::from_finstance(&tables()).unwrap();
        assert_eq!(
            embedding.map_ref(&ViewRef::Row(RowId(9))),
            Some(ViewRef::Node(9))
        );
        assert_eq!(
            embedding.map_ref(&ViewRef::Pair {
                edge: author(),
                src: RowId(6),
                tgt: RowId(4),
            }),
            Some(ViewRef::Edge(EdgeId(1)))
        );
        assert_eq!(
            embedding.map_ref(&ViewRef::Pair {
                edge: author(),
                src: RowId(9),
                tgt: RowId(1),
            }),
            None,
            "a pair the embedding does not record has no image"
        );
        assert_eq!(
            embedding.map_ref(&ViewRef::Arc {
                parent: 0,
                child: 1
            }),
            None
        );
        assert_eq!(embedding.map_ref(&ViewRef::Node(3)), Some(ViewRef::Node(3)));
    }

    #[test]
    fn a_table_embedding_allocates_through_an_id_allocator() {
        let mut embedding = TableEmbedding {
            pairs: BTreeMap::new(),
            next_edge: 7,
        };
        assert_eq!(embedding.allocate_edge().unwrap(), EdgeId(7));
        assert_eq!(embedding.next_edge, 8);
        let id = embedding.edge_for(&author(), RowId(1), RowId(2)).unwrap();
        assert_eq!(id, EdgeId(8));
        assert_eq!(
            embedding.edge_for(&author(), RowId(1), RowId(2)).unwrap(),
            id
        );
        assert_eq!(embedding.next_edge, 9, "a recorded pair allocates nothing");
        let mut full = TableEmbedding {
            pairs: BTreeMap::new(),
            next_edge: u32::MAX,
        };
        assert!(matches!(
            full.allocate_edge(),
            Err(InstError::EdgeIdsExhausted)
        ));
        assert_eq!(
            full.next_edge,
            u32::MAX,
            "a failed allocation leaves the floor"
        );
        let json = serde_json::to_string(&embedding).unwrap();
        assert_eq!(
            serde_json::from_str::<TableEmbedding>(&json).unwrap(),
            embedding
        );
    }

    #[test]
    fn the_graph_form_of_a_table_is_a_table() {
        let (graph, embedding) = GInstance::from_finstance(&tables()).unwrap();
        assert_eq!(graph.table_shape(&schema()), TableShapeReport::Table);
        let (back, read_embedding) = graph.try_to_finstance(&schema()).unwrap();
        assert_eq!(back, tables());
        assert_eq!(read_embedding, embedding);
    }

    #[test]
    fn table_shape_names_the_first_violation() {
        let (graph, _) = GInstance::from_finstance(&tables()).unwrap();
        let reason = |g: &GInstance, s: &Schema| match g.table_shape(s) {
            TableShapeReport::NotTable { reason } => reason,
            TableShapeReport::Table => panic!("expected a violation"),
        };

        let mut valued = graph.clone();
        valued.node_mut(4).unwrap().value = Some(FieldPresence::Present(Value::Int(1)));
        assert_eq!(
            reason(&valued, &schema()),
            NotTableReason::NodeDataNotRepresentable { node: 4 }
        );

        let mut nested = graph.clone();
        nested
            .node_mut(6)
            .unwrap()
            .extra_fields
            .insert("tags".into(), Value::List(vec![Value::Int(1)]));
        assert_eq!(
            reason(&nested, &schema()),
            NotTableReason::NestedCell {
                node: 6,
                field: "tags".into()
            }
        );

        let fan = graph
            .clone()
            .with_node(Node::new(20, "rel").with_shape(NodeShape::HyperEdge));
        assert_eq!(
            reason(&fan, &schema()),
            NotTableReason::FanNode { node: 20 }
        );

        let mut positioned = graph.clone();
        positioned.set_edge_position(EdgeId(2), Some(0)).unwrap();
        assert_eq!(
            reason(&positioned, &schema()),
            NotTableReason::EdgePositioned { edge: EdgeId(2) }
        );

        assert_eq!(
            reason(&graph, &Schema::default()),
            NotTableReason::EdgeNotForeignKey { edge: EdgeId(0) }
        );

        let backwards = graph.clone().with_edge_id(EdgeId(10), 4, 9, author());
        assert_eq!(
            reason(&backwards, &schema()),
            NotTableReason::IncoherentEndpoints { edge: EdgeId(10) }
        );

        let parallel = graph.clone().with_edge_id(EdgeId(11), 9, 4, author());
        assert_eq!(
            reason(&parallel, &schema()),
            NotTableReason::ParallelForeignKey {
                edges: vec![EdgeId(2), EdgeId(11)]
            }
        );

        assert_eq!(
            reason(&graph, &declaring(true)),
            NotTableReason::MultipleLifts {
                node: 6,
                edge: author(),
                edges: vec![EdgeId(0), EdgeId(1)],
            }
        );

        let mut single = tables();
        assert!(single.remove_foreign_key(&author(), RowId(6), RowId(1)));
        assert!(single.remove_foreign_key(&author(), RowId(9), RowId(4)));
        let (graph, _) = GInstance::from_finstance(&single).unwrap();
        assert_eq!(graph.table_shape(&declaring(true)), TableShapeReport::Table);
        assert_eq!(
            reason(&graph, &declaring(false)),
            NotTableReason::MissingLift {
                node: 9,
                edge: author()
            }
        );
        assert!(matches!(
            graph.try_to_finstance(&declaring(false)),
            Err(InstError::NotATable {
                reason: NotTableReason::MissingLift { node: 9, .. }
            })
        ));
    }

    #[test]
    fn renumbering_positions_changes_no_digest_field_but_position() {
        // Two graphs that differ only in the positions their edges carry:
        // the canonical bytes differ only inside the position fields, since
        // no derived positional code is ever written.
        let (graph, _) = GInstance::from_finstance(&tables()).unwrap();
        let mut first = graph.clone();
        let mut second = graph;
        for (k, id) in [EdgeId(0), EdgeId(1), EdgeId(2)].into_iter().enumerate() {
            let k = u32::try_from(k).unwrap();
            first.set_edge_position(id, Some(k)).unwrap();
            second.set_edge_position(id, Some(100 + k)).unwrap();
        }
        let a = crate::ginstance::canonical_ginstance_bytes(&first);
        let b = crate::ginstance::canonical_ginstance_bytes(&second);
        assert_eq!(a.len(), b.len());
        // The byte ranges of the position fields: the last four bytes of
        // each edge record, located by writing the records one by one.
        let mut offset = 16;
        let mut ids: Vec<u32> = first.nodes().keys().copied().collect();
        ids.sort_unstable();
        for id in ids {
            let mut w = CanonicalWriter::new();
            write_node_record(&mut w, &first.nodes()[&id]);
            offset += w.into_bytes().len();
        }
        let mut positions = Vec::new();
        for (id, edge) in first.edges_sorted() {
            let mut w = CanonicalWriter::new();
            write_edge_record(&mut w, id, edge);
            offset += w.into_bytes().len();
            positions.push(offset - 4..offset);
        }
        assert_eq!(offset, a.len());
        for (i, (x, y)) in a.iter().zip(&b).enumerate() {
            if x != y {
                assert!(
                    positions.iter().any(|r| r.contains(&i)),
                    "byte {i} differs outside a position field"
                );
            }
        }
        assert_ne!(first.digest(), second.digest());
        // A table carries no position at all: its bytes and digest are the
        // same however its rows and pairs were inserted.
        let mut reordered = FInstance::new();
        for (id, table, row) in [
            (RowId(6), "post", cell(60)),
            (RowId(1), "user", cell(10)),
            (RowId(9), "post", cell(90)),
            (RowId(4), "user", cell(40)),
        ] {
            reordered.insert_row(table, id, row).unwrap();
        }
        for (s, t) in [(9, 4), (6, 1), (6, 4)] {
            reordered
                .insert_foreign_key(author(), RowId(s), RowId(t))
                .unwrap();
        }
        assert_eq!(reordered.digest(), tables().digest());
    }

    /// A table instance over `user` and `post` with arbitrary distinct row
    /// ids and arbitrary pairs.
    fn arb_tables() -> impl Strategy<Value = FInstance> {
        (
            proptest::collection::btree_set(0_u32..64, 0..12),
            proptest::collection::vec(any::<bool>(), 12),
            proptest::collection::vec((any::<u8>(), any::<u8>()), 0..16),
        )
            .prop_map(|(ids, is_post, links)| {
                let mut f = FInstance::new();
                f.add_table("user");
                f.add_table("post");
                for (k, id) in ids.iter().enumerate() {
                    let table = if is_post[k] { "post" } else { "user" };
                    f.insert_row(table, RowId(*id), cell(i64::from(*id)))
                        .unwrap();
                }
                let posts = f.table("post").unwrap().ids();
                let users = f.table("user").unwrap().ids();
                if !posts.is_empty() && !users.is_empty() {
                    for (a, b) in links {
                        let src = posts[usize::from(a) % posts.len()];
                        let tgt = users[usize::from(b) % users.len()];
                        if !f.contains_pair(&author(), src, tgt) {
                            f.insert_foreign_key(author(), src, tgt).unwrap();
                        }
                    }
                }
                f
            })
    }

    /// The same instance without its empty tables, which a graph does not
    /// record.
    fn without_empty_tables(f: &FInstance) -> FInstance {
        FInstance::from_parts(
            f.tables()
                .into_iter()
                .filter(|(_, t)| !t.is_empty())
                .map(|(n, t)| {
                    (
                        n.clone(),
                        t.rows()
                            .into_iter()
                            .map(|(id, r)| (id, r.clone()))
                            .collect(),
                    )
                }),
            f.foreign_keys()
                .into_iter()
                .map(|(e, p)| (e.clone(), p.to_vec())),
        )
        .unwrap()
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        #[test]
        fn the_table_graph_round_trip_is_a_bijection(f in arb_tables()) {
            let (graph, embedding) = GInstance::from_finstance(&f).unwrap();
            // Rows correspond one to one with nodes, pairs with edge ids.
            let nodes: Vec<u32> = {
                let mut ids: Vec<u32> = graph.nodes().keys().copied().collect();
                ids.sort_unstable();
                ids
            };
            prop_assert_eq!(nodes, f.row_ids().into_iter().map(|r| r.0).collect::<Vec<_>>());
            let pairs: Vec<ForeignKeyPairOf> = embedding.pairs.keys().cloned().collect();
            prop_assert_eq!(pairs, f.all_pairs());
            let edge_ids: BTreeSet<EdgeId> = embedding.pairs.values().copied().collect();
            prop_assert_eq!(edge_ids.len(), graph.edge_count());
            let (back, back_embedding) = graph.try_to_finstance(&schema()).unwrap();
            prop_assert_eq!(back, without_empty_tables(&f));
            prop_assert_eq!(back_embedding, embedding);
        }

        #[test]
        fn a_table_shaped_graph_round_trips_through_its_table(f in arb_tables()) {
            let (graph, _) = GInstance::from_finstance(&f).unwrap();
            prop_assert_eq!(graph.table_shape(&schema()), TableShapeReport::Table);
            let (tables, embedding) = graph.try_to_finstance(&schema()).unwrap();
            let (again, again_embedding) = GInstance::from_finstance(&tables).unwrap();
            prop_assert!(GInstance::identical(&again, &graph));
            prop_assert_eq!(again_embedding, embedding);
        }

        #[test]
        fn table_embedding_next_edge_never_decreases(
            f in arb_tables(),
            extra in proptest::collection::vec((0_u32..64, 0_u32..64), 0..12),
        ) {
            let (_, mut embedding) = GInstance::from_finstance(&f).unwrap();
            let mut floor = embedding.next_edge;
            for (s, t) in extra {
                let before = embedding.pairs.len();
                let id = embedding.edge_for(&author(), RowId(s), RowId(t)).unwrap();
                prop_assert!(embedding.next_edge >= floor);
                if embedding.pairs.len() > before {
                    prop_assert_eq!(id, EdgeId(floor));
                }
                floor = embedding.next_edge;
            }
        }
    }

    type ForeignKeyPairOf = crate::functor::ForeignKeyPair;
}
