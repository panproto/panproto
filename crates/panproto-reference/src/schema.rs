//! The schema model: vertices with kinds, schema edges, hyper-edges,
//! constraints, required and ordered edges, entries, and the key and
//! reference declarations.

use std::collections::{BTreeMap, BTreeSet};

use crate::model::{Name, SEdge};

/// A hyper-edge: a kind, a signature of labels to vertices, and the label
/// naming its parent vertex.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct HyperEdge {
    /// Kind.
    pub kind: Name,
    /// Label to vertex.
    pub signature: BTreeMap<Name, Name>,
    /// The parent label.
    pub parent_label: Name,
}

/// A part of a key: a path of edges from the keyed vertex to a scalar, or
/// the element's position among the targets of an ordered edge.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum KeyPart {
    /// A path of edges.
    Path(Vec<SEdge>),
    /// The index among the targets of `within`.
    Position {
        /// The ordered edge.
        within: SEdge,
    },
}

/// The role of a key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum KeyRole {
    /// A primary key (its parts are required).
    Primary,
    /// A unique key.
    Unique,
    /// An identifier.
    Identifier,
}

/// The uniqueness domain of a key.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum KeyScope {
    /// The document (the whole instance for a single-document instance).
    Document,
    /// The whole instance.
    Global,
    /// Elements under the same nearest ancestor anchored at the vertex.
    Within(Name),
}

/// A key declaration.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Key {
    /// Id, unique per vertex.
    pub id: Name,
    /// Parts in key order.
    pub parts: Vec<KeyPart>,
    /// Role.
    pub role: KeyRole,
    /// Scope.
    pub scope: KeyScope,
}

/// Where a reference reads its key.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum RefSource {
    /// Key values at these paths, aligned with the target key's parts.
    KeyValue {
        /// One path per target key part.
        parts: Vec<Vec<SEdge>>,
    },
    /// A path expression (resolved by a provider).
    Path {
        /// The field holding it.
        field: Vec<SEdge>,
    },
    /// A variable name bound by its first occurrence.
    Variable {
        /// The field holding the name.
        field: Vec<SEdge>,
        /// The identifier key binding it.
        binder_key: Name,
    },
}

/// Cardinality of a reference.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Cardinality {
    /// One target.
    One,
    /// A sequence of targets.
    Many,
}

/// What happens to an unresolved reference.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OnUnresolved {
    /// It is an error (unless lenient).
    Error,
    /// It is recorded in the complement.
    Complement,
}

/// How a resolved reference is represented.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ResolveMode {
    /// Keep the holder's field and add an edge.
    AddEdge,
    /// Remove the occurrence node and point its arc at the target.
    Redirect,
}

/// A reference declaration.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Reference {
    /// Id.
    pub id: Name,
    /// The holder vertex.
    pub holder: Name,
    /// The source of the key.
    pub source: RefSource,
    /// The target vertex.
    pub target_vertex: Name,
    /// The target key.
    pub target_key: Option<Name>,
    /// The resolved edge.
    pub edge: Option<SEdge>,
    /// Cardinality.
    pub cardinality: Cardinality,
    /// Whether no target is allowed.
    pub nullable: bool,
    /// Unresolved policy.
    pub on_unresolved: OnUnresolved,
    /// Mode.
    pub mode: ResolveMode,
}

/// A schema.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Schema {
    /// Vertex id to kind.
    pub vertices: BTreeMap<Name, Name>,
    /// Schema edges.
    pub edges: BTreeSet<SEdge>,
    /// Hyper-edges by id.
    pub hyper_edges: BTreeMap<Name, HyperEdge>,
    /// Constraints per vertex: `(sort, value)`.
    pub constraints: BTreeMap<Name, Vec<(Name, String)>>,
    /// Required edges per vertex.
    pub required: BTreeMap<Name, Vec<SEdge>>,
    /// Ordered edges with their position.
    pub orderings: BTreeMap<SEdge, u32>,
    /// Entry vertices, in order.
    pub entries: Vec<Name>,
    /// Keys per vertex, sorted by id.
    pub keys: BTreeMap<Name, Vec<Key>>,
    /// References by id.
    pub references: BTreeMap<Name, Reference>,
}

impl Schema {
    /// Add a vertex of `kind`.
    #[must_use]
    pub fn vertex(mut self, id: &str, kind: &str) -> Self {
        self.vertices.insert(id.to_owned(), kind.to_owned());
        self
    }

    /// Add an edge.
    #[must_use]
    pub fn edge(mut self, src: &str, tgt: &str, kind: &str, name: Option<&str>) -> Self {
        self.edges.insert(SEdge::new(src, tgt, kind, name));
        self
    }

    /// The edges from `a` to `b`.
    #[must_use]
    pub fn edges_between(&self, a: &str, b: &str) -> Vec<&SEdge> {
        self.edges
            .iter()
            .filter(|e| e.src == a && e.tgt == b)
            .collect()
    }
}
