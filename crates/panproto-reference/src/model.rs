//! The instance models the reference operates on: schema edges, nodes,
//! graphs, tables and trees, each a plain finite structure with ordered
//! containers so that equality is structural and iteration deterministic.

use std::collections::{BTreeMap, BTreeSet};

use crate::value::{Presence, Value};

/// A name (vertex id, edge kind, label).
pub type Name = String;

/// A schema edge: source vertex, target vertex, kind and optional name.
/// Ordered by those fields in that order.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SEdge {
    /// Source vertex.
    pub src: Name,
    /// Target vertex.
    pub tgt: Name,
    /// Edge kind.
    pub kind: Name,
    /// Optional label.
    pub name: Option<Name>,
}

impl SEdge {
    /// The edge `src -kind(name)-> tgt`.
    #[must_use]
    pub fn new(src: &str, tgt: &str, kind: &str, name: Option<&str>) -> Self {
        Self {
            src: src.to_owned(),
            tgt: tgt.to_owned(),
            kind: kind.to_owned(),
            name: name.map(str::to_owned),
        }
    }

    /// The column name of an edge used as an attribute or child binding:
    /// its name, else its target vertex.
    #[must_use]
    pub fn column(&self) -> &str {
        self.name.as_deref().unwrap_or(&self.tgt)
    }
}

/// The structural shape of a node.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Default)]
pub enum NodeShape {
    /// An ordinary node.
    #[default]
    Plain,
    /// An ordered-collection node.
    List,
    /// An XML element with its written tag.
    XmlElement(Name),
    /// An inline XML text run.
    XmlTextSegment,
    /// A reified hyper-edge (fan) node.
    HyperEdge,
}

/// An instance node: id, anchor vertex, value and metadata.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Node {
    /// Id.
    pub id: u32,
    /// Anchor vertex.
    pub anchor: Name,
    /// The node's own value.
    pub value: Option<Presence>,
    /// Discriminator.
    pub discriminator: Option<Name>,
    /// Extra fields (a table row's cells, a graph node's attributes).
    pub fields: BTreeMap<String, Value>,
    /// Opaque position datum.
    pub position: Option<u32>,
    /// Shape.
    pub shape: NodeShape,
    /// Annotations.
    pub annotations: BTreeMap<String, Value>,
}

impl Node {
    /// A plain node with no value or metadata.
    #[must_use]
    pub fn new(id: u32, anchor: &str) -> Self {
        Self {
            id,
            anchor: anchor.to_owned(),
            value: None,
            discriminator: None,
            fields: BTreeMap::new(),
            position: None,
            shape: NodeShape::Plain,
            annotations: BTreeMap::new(),
        }
    }

    /// The same node holding `value`.
    #[must_use]
    pub fn with_value(mut self, value: Value) -> Self {
        self.value = Some(Presence::Present(value));
        self
    }

    /// The same node with field `key` set.
    #[must_use]
    pub fn with_field(mut self, key: &str, value: Value) -> Self {
        self.fields.insert(key.to_owned(), value);
        self
    }
}

/// A graph edge.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct GEdge {
    /// Source node.
    pub src: u32,
    /// Target node.
    pub tgt: u32,
    /// Its schema edge label.
    pub label: SEdge,
    /// Its position among the source's ordered out-edges.
    pub position: Option<u32>,
}

/// A graph: nodes by id and edges by edge id. Parallel edges and self-loops
/// are allowed; every endpoint is a node (checked by [`Graph::well_formed`]).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Graph {
    /// Nodes by id (each node's `id` equals its key).
    pub nodes: BTreeMap<u32, Node>,
    /// Edges by edge id.
    pub edges: BTreeMap<u32, GEdge>,
}

impl Graph {
    /// Whether every node's id equals its key and every edge endpoint is a
    /// node.
    #[must_use]
    pub fn well_formed(&self) -> bool {
        self.nodes.iter().all(|(k, n)| *k == n.id)
            && self
                .edges
                .values()
                .all(|e| self.nodes.contains_key(&e.src) && self.nodes.contains_key(&e.tgt))
    }

    /// The out-edges of `n` in `ordered_out_edges` order: by position with
    /// `None` first, then by edge id.
    #[must_use]
    pub fn ordered_out_edges(&self, n: u32) -> Vec<u32> {
        let mut out: Vec<(Option<u32>, u32)> = self
            .edges
            .iter()
            .filter(|(_, e)| e.src == n)
            .map(|(id, e)| (e.position, *id))
            .collect();
        out.sort();
        out.into_iter().map(|(_, id)| id).collect()
    }
}

/// A table instance: rows by row id (globally unique) with their table and
/// cells, and foreign-key pairs as a set of `(edge, src row, tgt row)`.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Table {
    /// Rows: id to (table, cells).
    pub rows: BTreeMap<u32, (Name, BTreeMap<String, Value>)>,
    /// Foreign-key pairs.
    pub pairs: BTreeSet<(SEdge, u32, u32)>,
}

impl Table {
    /// Whether every pair names two rows.
    #[must_use]
    pub fn well_formed(&self) -> bool {
        self.pairs
            .iter()
            .all(|(_, s, t)| self.rows.contains_key(s) && self.rows.contains_key(t))
    }
}

/// A tree fan: a hyper-edge occurrence with a parent and labelled children.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Fan {
    /// Parent node.
    pub parent: u32,
    /// Hyper-edge id.
    pub hyper_edge: Name,
    /// Children by label.
    pub children: BTreeMap<Name, u32>,
}

/// A tree: nodes, arcs in child order, fans in their listed order, and the
/// root.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tree {
    /// Nodes by id.
    pub nodes: BTreeMap<u32, Node>,
    /// Arcs `(parent, child, label)` in arc (child) order.
    pub arcs: Vec<(u32, u32, SEdge)>,
    /// Fans.
    pub fans: Vec<Fan>,
    /// The root node.
    pub root: u32,
}
