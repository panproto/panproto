//! The schema model: vertices with kinds, schema edges, hyper-edges,
//! constraints, required and ordered edges, entries, and the key and
//! reference declarations.

use std::collections::{BTreeMap, BTreeSet};

use crate::canonical::Writer;
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

fn write_edges(writer: &mut Writer, edges: impl ExactSizeIterator<Item = SEdge>) {
    writer.len(edges.len());
    for edge in edges {
        writer.edge(&edge);
    }
}

fn write_key(writer: &mut Writer, key: &Key) {
    writer.str(&key.id);
    writer.len(key.parts.len());
    for part in &key.parts {
        match part {
            KeyPart::Path(path) => {
                writer.tag(0);
                write_edges(writer, path.iter().cloned());
            }
            KeyPart::Position { within } => {
                writer.tag(1);
                writer.edge(within);
            }
        }
    }
    writer.tag(match key.role {
        KeyRole::Primary => 0,
        KeyRole::Unique => 1,
        KeyRole::Identifier => 2,
    });
    match &key.scope {
        KeyScope::Document => writer.tag(0),
        KeyScope::Global => writer.tag(1),
        KeyScope::Within(vertex) => {
            writer.tag(2);
            writer.str(vertex);
        }
    }
}

fn write_reference(writer: &mut Writer, reference: &Reference) {
    writer.str(&reference.id);
    writer.str(&reference.holder);
    match &reference.source {
        RefSource::KeyValue { parts } => {
            writer.tag(0);
            writer.len(parts.len());
            for path in parts {
                write_edges(writer, path.iter().cloned());
            }
        }
        RefSource::Path { field } => {
            writer.tag(1);
            write_edges(writer, field.iter().cloned());
        }
        RefSource::Variable { field, binder_key } => {
            writer.tag(2);
            write_edges(writer, field.iter().cloned());
            writer.str(binder_key);
        }
    }
    writer.str(&reference.target_vertex);
    writer.opt_name(reference.target_key.as_deref());
    match &reference.edge {
        None => writer.tag(0),
        Some(edge) => {
            writer.tag(1);
            writer.edge(edge);
        }
    }
    writer.tag(match reference.cardinality {
        Cardinality::One => 0,
        Cardinality::Many => 1,
    });
    writer.tag(u8::from(reference.nullable));
    writer.tag(match reference.on_unresolved {
        OnUnresolved::Error => 0,
        OnUnresolved::Complement => 1,
    });
    writer.tag(match reference.mode {
        ResolveMode::AddEdge => 0,
        ResolveMode::Redirect => 1,
    });
}

/// Canonical bytes for the finite reference schema.
///
/// The layout writes every field in declaration order, every ordered map
/// or set in key order, every sequence behind a `u64` length, and every sum
/// behind a one-byte discriminant. Strings and edges use [`Writer`]'s
/// canonical primitive layout.
#[must_use]
pub fn canonical_bytes(schema: &Schema) -> Vec<u8> {
    let mut writer = Writer::default();
    writer.len(schema.vertices.len());
    for (vertex, kind) in &schema.vertices {
        writer.str(vertex);
        writer.str(kind);
    }
    write_edges(&mut writer, schema.edges.iter().cloned());
    writer.len(schema.hyper_edges.len());
    for (id, hyper) in &schema.hyper_edges {
        writer.str(id);
        writer.str(&hyper.kind);
        writer.len(hyper.signature.len());
        for (label, vertex) in &hyper.signature {
            writer.str(label);
            writer.str(vertex);
        }
        writer.str(&hyper.parent_label);
    }
    writer.len(schema.constraints.len());
    for (vertex, constraints) in &schema.constraints {
        writer.str(vertex);
        writer.len(constraints.len());
        for (sort, value) in constraints {
            writer.str(sort);
            writer.str(value);
        }
    }
    writer.len(schema.required.len());
    for (vertex, required) in &schema.required {
        writer.str(vertex);
        write_edges(&mut writer, required.iter().cloned());
    }
    writer.len(schema.orderings.len());
    for (edge, position) in &schema.orderings {
        writer.edge(edge);
        writer.u32(*position);
    }
    writer.len(schema.entries.len());
    for entry in &schema.entries {
        writer.str(entry);
    }
    writer.len(schema.keys.len());
    for (vertex, keys) in &schema.keys {
        writer.str(vertex);
        writer.len(keys.len());
        for key in keys {
            write_key(&mut writer, key);
        }
    }
    writer.len(schema.references.len());
    for (id, reference) in &schema.references {
        writer.str(id);
        write_reference(&mut writer, reference);
    }
    writer.bytes
}

/// Compatibility of the finite reference schema under a change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    /// No semantic declaration changed.
    Identical,
    /// Existing instances remain valid, though the inverse may not hold.
    BackwardCompatible,
    /// Some previously valid instance may be rejected.
    Breaking,
}

fn key_is_tightened(old: &Key, new: &Key) -> bool {
    let role_rank = |role: &KeyRole| match role {
        KeyRole::Primary => 2,
        KeyRole::Unique | KeyRole::Identifier => 1,
    };
    let scope_rank = |scope: &KeyScope| match scope {
        KeyScope::Global => 3,
        KeyScope::Document => 2,
        KeyScope::Within(_) => 1,
    };
    old.parts != new.parts
        || role_rank(&new.role) > role_rank(&old.role)
        || (role_rank(&new.role) == role_rank(&old.role) && new.role != old.role)
        || scope_rank(&new.scope) > scope_rank(&old.scope)
        || (scope_rank(&new.scope) == scope_rank(&old.scope) && new.scope != old.scope)
}

fn reference_is_tightened(old: &Reference, new: &Reference) -> bool {
    (old.cardinality == Cardinality::Many && new.cardinality == Cardinality::One)
        || (old.nullable && !new.nullable)
        || (old.on_unresolved == OnUnresolved::Complement
            && new.on_unresolved == OnUnresolved::Error)
        || old.holder != new.holder
        || old.source != new.source
        || old.target_vertex != new.target_vertex
        || old.target_key != new.target_key
        || old.edge != new.edge
        || old.mode != new.mode
}

fn keyed_keys(schema: &Schema) -> BTreeMap<(&str, &str), &Key> {
    schema
        .keys
        .iter()
        .flat_map(|(vertex, keys)| {
            keys.iter()
                .map(move |key| ((vertex.as_str(), key.id.as_str()), key))
        })
        .collect()
}

/// Classify key and reference changes by backward compatibility.
///
/// Adding a key on an existing vertex is breaking and removing one is a
/// relaxation. A changed key is breaking exactly when it adds or changes
/// parts, strengthens its role, widens its scope, or moves between two
/// distinct `Within` scopes. Reference changes follow the corresponding
/// cardinality, nullability and unresolved-value rules. Changes elsewhere
/// in this finite carrier fail closed as breaking.
#[must_use]
pub fn classify_change(before: &Schema, after: &Schema) -> Change {
    if before == after {
        return Change::Identical;
    }
    let before_keys = keyed_keys(before);
    let after_keys = keyed_keys(after);
    let added_key_breaks = after_keys
        .keys()
        .any(|key| !before_keys.contains_key(key) && before.vertices.contains_key(key.0));
    let changed_key_breaks = before_keys.iter().any(|(key, old)| {
        after_keys
            .get(key)
            .is_some_and(|new| key_is_tightened(old, new))
    });
    let added_reference_breaks = after.references.iter().any(|(id, reference)| {
        !before.references.contains_key(id)
            && before.vertices.contains_key(&reference.holder)
            && (!reference.nullable || reference.on_unresolved == OnUnresolved::Error)
    });
    let changed_reference_breaks = before.references.iter().any(|(id, old)| {
        after
            .references
            .get(id)
            .is_some_and(|new| reference_is_tightened(old, new))
    });
    let declarations_only = before.vertices == after.vertices
        && before.edges == after.edges
        && before.hyper_edges == after.hyper_edges
        && before.constraints == after.constraints
        && before.required == after.required
        && before.orderings == after.orderings
        && before.entries == after.entries;
    if added_key_breaks
        || changed_key_breaks
        || added_reference_breaks
        || changed_reference_breaks
        || !declarations_only
    {
        Change::Breaking
    } else {
        Change::BackwardCompatible
    }
}

/// A registration conflict.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegistrationConflict {
    /// The already-registered name.
    pub name: Name,
}

/// Register a schema by name, accepting an idempotent replay and refusing
/// a different schema at the same name.
///
/// # Errors
///
/// [`RegistrationConflict`] when `name` already denotes different bytes.
pub fn register(
    registry: &mut BTreeMap<Name, Schema>,
    name: &str,
    schema: Schema,
) -> Result<(), RegistrationConflict> {
    if let Some(existing) = registry.get(name) {
        return if canonical_bytes(existing) == canonical_bytes(&schema) {
            Ok(())
        } else {
            Err(RegistrationConflict {
                name: name.to_owned(),
            })
        };
    }
    registry.insert(name.to_owned(), schema);
    Ok(())
}
