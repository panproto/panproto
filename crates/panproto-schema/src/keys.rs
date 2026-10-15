//! Keys and references declared by a schema.
//!
//! A [`Key`] says which scalar paths identify the elements anchored at a
//! vertex; a [`Reference`] says how a field of one element names another
//! element through a key. Both are schema data, carried in
//! [`Schema::declarations`](crate::Schema::declarations) as one
//! [`SchemaDeclarations`] value.
//!
//! Enums serialize externally tagged in `snake_case`; structs serialize with
//! named fields in declaration order.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use panproto_gat::Name;
use serde::{Deserialize, Serialize};

use crate::error::SchemaError;
use crate::schema::Edge;

/// A key on a vertex: the parts that identify an element anchored there.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Key {
    /// The key's id, unique among the keys of its vertex.
    pub id: Name,
    /// The key's parts, in key order.
    pub parts: Vec<KeyPart>,
    /// What the key is used for.
    pub role: KeyRole,
    /// Where the key's values must be unique.
    pub scope: KeyScope,
}

/// One part of a [`Key`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyPart {
    /// A path of edges from the keyed vertex to a scalar.
    Path(Vec<Edge>),
    /// The element's index among the targets of the ordered edge `within`,
    /// for array-indexed formats.
    Position {
        /// The ordered edge whose targets are indexed.
        within: Edge,
    },
}

/// The role a [`Key`] plays.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyRole {
    /// The primary key of the vertex.
    Primary,
    /// A uniqueness constraint that is not the primary key.
    Unique,
    /// An identifier other elements refer to (`xml:id`, `@id`, a field of
    /// an `ID` scalar type, a variable binder).
    Identifier,
}

/// Where the values of a [`Key`] must be unique.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyScope {
    /// Unique within one document.
    Document,
    /// Unique across documents.
    Global,
    /// Unique within each element anchored at the named ancestor vertex.
    Within(Name),
}

/// A reference: a field of a holder element that names a target element.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reference {
    /// The reference's id, unique in the schema.
    pub id: Name,
    /// The vertex whose elements hold the reference.
    pub holder: Name,
    /// How the referencing value is read from the holder.
    pub source: RefSource,
    /// The vertex whose elements the reference resolves to.
    pub target_vertex: Name,
    /// The key of `target_vertex` the reference is resolved through, if any.
    pub target_key: Option<Name>,
    /// The schema edge from `holder` to `target_vertex` that resolution adds
    /// or redirects, if any.
    pub edge: Option<Edge>,
    /// Whether one holder names one target or many.
    pub cardinality: Cardinality,
    /// Whether the referencing value may be absent.
    pub nullable: bool,
    /// What resolution does with a value that names no target.
    pub on_unresolved: OnUnresolved,
    /// How resolution rewrites the instance.
    pub mode: ResolveMode,
}

/// How a [`Reference`] reads its referencing value.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefSource {
    /// Key values held by the holder, aligned positionally with the target
    /// key's parts.
    KeyValue {
        /// One edge path per target key part.
        parts: Vec<Vec<Edge>>,
    },
    /// A path expression held in one field.
    Path {
        /// The field holding the path expression.
        field: Vec<Edge>,
        /// The syntax of the path expression.
        syntax: PathSyntax,
    },
    /// A standoff span over a text, resolved against elements keyed by their
    /// offsets.
    Standoff {
        /// The text the offsets index into.
        text: RefTextSource,
        /// The field holding the start offset.
        start: Vec<Edge>,
        /// The field holding the end offset.
        end: Vec<Edge>,
        /// The unit the offsets count.
        unit: OffsetUnit,
        /// Whether the end offset is inclusive.
        end_inclusive: bool,
    },
    /// A variable: the first binding occurrence defines, later occurrences
    /// refer.
    Variable {
        /// The field holding the variable name.
        field: Vec<Edge>,
        /// The identifier key the variable binds.
        binder_key: Name,
    },
}

/// The syntax of a path-valued reference.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PathSyntax {
    /// An RFC 6901 JSON Pointer.
    JsonPointer,
    /// An `XPath` expression.
    XPath,
    /// A URI, resolved against the value at `base` when present.
    Uri {
        /// The field holding the base URI, if any.
        base: Option<Vec<Edge>>,
    },
    /// An XML `IDREF`.
    IdRef,
}

/// Where the text of a standoff reference lives.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefTextSource {
    /// A field of the same document.
    Local(Vec<Edge>),
    /// A field naming an external text.
    External(Vec<Edge>),
}

/// The unit standoff offsets count.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OffsetUnit {
    /// Bytes.
    Byte,
    /// Unicode scalar values.
    Utf8Char,
    /// UTF-16 code units.
    Utf16,
    /// Tokens.
    Token,
}

/// How many targets one holder names.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cardinality {
    /// Exactly one target.
    One,
    /// Any number of targets (`IDREFS`, arrays of references).
    Many,
}

/// What resolution does with a referencing value that names no target.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OnUnresolved {
    /// Fail.
    Error,
    /// Keep the value in the resolution complement.
    Complement,
}

/// How resolution rewrites an instance.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolveMode {
    /// Keep the holder's reference field and add the reference's edge from
    /// the holder to the target.
    AddEdge,
    /// Remove the referencing occurrence node, keeping its literal in the
    /// resolution complement, and point its incoming arc at the target.
    Redirect,
}

/// The keys and references a schema declares.
///
/// Each key list is sorted by key id and key ids are unique per vertex.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchemaDeclarations {
    /// Keys per vertex.
    pub keys: BTreeMap<Name, Vec<Key>>,
    /// References per reference id.
    pub references: BTreeMap<Name, Reference>,
}

impl SchemaDeclarations {
    /// Returns `true` iff the schema declares no key and no reference.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty() && self.references.is_empty()
    }

    /// The keys declared on `vertex`, in ascending key id order.
    #[must_use]
    pub fn keys_of(&self, vertex: &str) -> &[Key] {
        self.keys.get(vertex).map_or(&[], Vec::as_slice)
    }

    /// The key `id` declared on `vertex`, if any.
    #[must_use]
    pub fn key(&self, vertex: &str, id: &str) -> Option<&Key> {
        self.keys_of(vertex).iter().find(|key| key.id == id)
    }

    /// Declare `key` on `vertex`, keeping the vertex's keys sorted by id
    /// and their ids unique.
    ///
    /// Returns the key `key` replaced, which had the same id, or `None` when
    /// the id was new on `vertex`.
    ///
    /// # Examples
    ///
    /// ```
    /// use panproto_schema::{Key, KeyRole, KeyScope, SchemaDeclarations};
    ///
    /// let key = |id: &str| Key {
    ///     id: id.into(),
    ///     parts: vec![],
    ///     role: KeyRole::Unique,
    ///     scope: KeyScope::Document,
    /// };
    /// let mut decls = SchemaDeclarations::default();
    /// assert!(decls.insert_key("film".into(), key("b")).is_none());
    /// assert!(decls.insert_key("film".into(), key("a")).is_none());
    /// assert!(decls.insert_key("film".into(), key("b")).is_some());
    /// let ids: Vec<&str> = decls.keys_of("film").iter().map(|k| k.id.as_ref()).collect();
    /// assert_eq!(ids, ["a", "b"]);
    /// ```
    pub fn insert_key(&mut self, vertex: Name, key: Key) -> Option<Key> {
        let keys = self.keys.entry(vertex).or_default();
        match keys.binary_search_by(|probe| probe.id.cmp(&key.id)) {
            Ok(at) => Some(std::mem::replace(&mut keys[at], key)),
            Err(at) => {
                keys.insert(at, key);
                None
            }
        }
    }

    /// Declare `reference` under its own id.
    ///
    /// Returns the reference it replaced, which had the same id, or `None`
    /// when the id was new.
    pub fn insert_reference(&mut self, reference: Reference) -> Option<Reference> {
        self.references.insert(reference.id.clone(), reference)
    }

    /// Declare `key` on `vertex` unless a key with its id is already there.
    ///
    /// An equal key already there is left in place.
    ///
    /// # Errors
    ///
    /// Returns [`SchemaError::KeyConflict`] when a key with the same id but
    /// different parts, role or scope is already declared on `vertex`.
    pub fn merge_key(&mut self, vertex: Name, key: Key) -> Result<(), SchemaError> {
        match self.key(&vertex, &key.id) {
            Some(existing) if *existing == key => Ok(()),
            Some(_) => Err(SchemaError::KeyConflict {
                vertex,
                key: key.id,
            }),
            None => {
                self.insert_key(vertex, key);
                Ok(())
            }
        }
    }

    /// Declare `reference` unless a reference with its id is already there.
    ///
    /// An equal reference already there is left in place.
    ///
    /// # Errors
    ///
    /// Returns [`SchemaError::ReferenceConflict`] when a different reference
    /// with the same id is already declared.
    pub fn merge_reference(&mut self, reference: Reference) -> Result<(), SchemaError> {
        match self.references.get(&reference.id) {
            Some(existing) if *existing == reference => Ok(()),
            Some(_) => Err(SchemaError::ReferenceConflict {
                reference: reference.id,
            }),
            None => {
                self.insert_reference(reference);
                Ok(())
            }
        }
    }

    /// The key a reference resolves through: its `target_key`, the binder
    /// key of a `Variable` source, or otherwise the first `Primary` key of
    /// its target vertex. `None` when that names no key of the target
    /// vertex.
    #[must_use]
    pub fn resolving_key(&self, reference: &Reference) -> Option<&Key> {
        let target = reference.target_vertex.as_ref();
        if let RefSource::Variable { binder_key, .. } = &reference.source {
            return self.key(target, binder_key);
        }
        reference.target_key.as_ref().map_or_else(
            || {
                self.keys_of(target)
                    .iter()
                    .find(|key| key.role == KeyRole::Primary)
            },
            |id| self.key(target, id),
        )
    }

    /// Carry these declarations along a map of vertices and a map of edges,
    /// such as a schema morphism's.
    ///
    /// Every vertex and every edge a declaration names is sent through the
    /// maps; a declaration that names a vertex or an edge the maps do not
    /// send anywhere is dropped and reported, and so is a reference whose
    /// resolving key (see [`resolving_key`](Self::resolving_key)) was
    /// dropped. Ids are kept. Two declarations that land on the same id
    /// (of the same image vertex, for keys) merge when they are equal.
    ///
    /// # Errors
    ///
    /// Returns [`SchemaError::KeyConflict`] or
    /// [`SchemaError::ReferenceConflict`] when two declarations land on the
    /// same id and differ.
    pub fn map_through(
        &self,
        vertex: impl Fn(&Name) -> Option<Name>,
        edge: impl Fn(&Edge) -> Option<Edge>,
    ) -> Result<(Self, DeclarationReport), SchemaError> {
        let carry = DeclarationMapper {
            vertex: &vertex,
            edge: &edge,
        };
        let mut out = Self::default();
        let mut report = DeclarationReport::default();
        let mut dropped_keys: BTreeSet<(Name, Name)> = BTreeSet::new();

        for (owner, keys) in &self.keys {
            for key in keys {
                match carry.key(owner, key) {
                    Some((image, mapped)) => out.merge_key(image, mapped)?,
                    None => {
                        dropped_keys.insert((owner.clone(), key.id.clone()));
                    }
                }
            }
        }

        let mut dropped_references: BTreeSet<Name> = BTreeSet::new();
        for (id, reference) in &self.references {
            let key_dropped = self.resolving_key(reference).is_some_and(|key| {
                dropped_keys.contains(&(reference.target_vertex.clone(), key.id.clone()))
            });
            match carry.reference(reference).filter(|_| !key_dropped) {
                Some(mapped) => out.merge_reference(mapped)?,
                None => {
                    dropped_references.insert(id.clone());
                }
            }
        }

        report.dropped_keys = dropped_keys.into_iter().collect();
        report.dropped_references = dropped_references.into_iter().collect();
        Ok((out, report))
    }

    /// Remove the keys named in `doomed`, as `(vertex, key id)`, together
    /// with every reference whose resolving key is among them, and report
    /// what was removed.
    pub(crate) fn drop_keys(&mut self, doomed: &BTreeSet<(Name, Name)>) -> DeclarationReport {
        let mut report = DeclarationReport::default();
        if doomed.is_empty() {
            return report;
        }
        let orphaned: Vec<Name> = self
            .references
            .values()
            .filter(|reference| {
                self.resolving_key(reference).is_some_and(|key| {
                    doomed.contains(&(reference.target_vertex.clone(), key.id.clone()))
                })
            })
            .map(|reference| reference.id.clone())
            .collect();
        for id in orphaned {
            self.references.remove(&id);
            report.dropped_references.push(id);
        }
        for (vertex, key) in doomed {
            if let Some(keys) = self.keys.get_mut(vertex) {
                let before = keys.len();
                keys.retain(|candidate| candidate.id != *key);
                if keys.len() < before {
                    report.dropped_keys.push((vertex.clone(), key.clone()));
                }
                if keys.is_empty() {
                    self.keys.remove(vertex);
                }
            }
        }
        report.dropped_references.sort();
        report
    }

    /// Merge `other` into these declarations: equal declarations with the
    /// same id are kept once.
    ///
    /// # Errors
    ///
    /// Returns [`SchemaError::KeyConflict`] or
    /// [`SchemaError::ReferenceConflict`] when the two sides declare the
    /// same id differently.
    pub fn merge(&mut self, other: &Self) -> Result<(), SchemaError> {
        for (vertex, keys) in &other.keys {
            for key in keys {
                self.merge_key(vertex.clone(), key.clone())?;
            }
        }
        for reference in other.references.values() {
            self.merge_reference(reference.clone())?;
        }
        Ok(())
    }
}

/// The declarations an operation on schemas dropped because a vertex or an
/// edge they name did not survive it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeclarationReport {
    /// The dropped keys, as `(vertex, key id)` in the source schema, sorted.
    pub dropped_keys: Vec<(Name, Name)>,
    /// The ids of the dropped references, sorted.
    pub dropped_references: Vec<Name>,
}

impl DeclarationReport {
    /// Returns `true` iff nothing was dropped.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.dropped_keys.is_empty() && self.dropped_references.is_empty()
    }

    /// Add the drops of `other`, keeping both lists sorted and free of
    /// duplicates.
    pub fn absorb(&mut self, other: Self) {
        self.dropped_keys.extend(other.dropped_keys);
        self.dropped_keys.sort();
        self.dropped_keys.dedup();
        self.dropped_references.extend(other.dropped_references);
        self.dropped_references.sort();
        self.dropped_references.dedup();
    }
}

/// The vertex and edge maps [`SchemaDeclarations::map_through`] carries
/// declarations along.
struct DeclarationMapper<'a> {
    /// Where each vertex goes, or `None` when it does not survive.
    vertex: &'a dyn Fn(&Name) -> Option<Name>,
    /// Where each edge goes, or `None` when it does not survive.
    edge: &'a dyn Fn(&Edge) -> Option<Edge>,
}

impl DeclarationMapper<'_> {
    /// Map every edge of a path, or `None` if one does not survive.
    fn path(&self, path: &[Edge]) -> Option<Vec<Edge>> {
        path.iter().map(|edge| (self.edge)(edge)).collect()
    }

    /// Map a key declared on `owner`, returning the image vertex with it.
    fn key(&self, owner: &Name, key: &Key) -> Option<(Name, Key)> {
        let image = (self.vertex)(owner)?;
        let parts = key
            .parts
            .iter()
            .map(|part| match part {
                KeyPart::Path(path) => self.path(path).map(KeyPart::Path),
                KeyPart::Position { within } => {
                    (self.edge)(within).map(|within| KeyPart::Position { within })
                }
            })
            .collect::<Option<Vec<_>>>()?;
        let scope = match &key.scope {
            KeyScope::Within(ancestor) => KeyScope::Within((self.vertex)(ancestor)?),
            other => other.clone(),
        };
        Some((
            image,
            Key {
                id: key.id.clone(),
                parts,
                role: key.role.clone(),
                scope,
            },
        ))
    }

    /// Map a reference.
    fn reference(&self, reference: &Reference) -> Option<Reference> {
        let edge = match &reference.edge {
            Some(edge) => Some((self.edge)(edge)?),
            None => None,
        };
        Some(Reference {
            id: reference.id.clone(),
            holder: (self.vertex)(&reference.holder)?,
            source: self.source(&reference.source)?,
            target_vertex: (self.vertex)(&reference.target_vertex)?,
            target_key: reference.target_key.clone(),
            edge,
            cardinality: reference.cardinality.clone(),
            nullable: reference.nullable,
            on_unresolved: reference.on_unresolved.clone(),
            mode: reference.mode.clone(),
        })
    }

    /// Map the paths of a reference source.
    fn source(&self, source: &RefSource) -> Option<RefSource> {
        Some(match source {
            RefSource::KeyValue { parts } => RefSource::KeyValue {
                parts: parts
                    .iter()
                    .map(|path| self.path(path))
                    .collect::<Option<Vec<_>>>()?,
            },
            RefSource::Path { field, syntax } => RefSource::Path {
                field: self.path(field)?,
                syntax: match syntax {
                    PathSyntax::Uri { base: Some(base) } => PathSyntax::Uri {
                        base: Some(self.path(base)?),
                    },
                    other => other.clone(),
                },
            },
            RefSource::Standoff {
                text,
                start,
                end,
                unit,
                end_inclusive,
            } => RefSource::Standoff {
                text: match text {
                    RefTextSource::Local(path) => RefTextSource::Local(self.path(path)?),
                    RefTextSource::External(path) => RefTextSource::External(self.path(path)?),
                },
                start: self.path(start)?,
                end: self.path(end)?,
                unit: unit.clone(),
                end_inclusive: *end_inclusive,
            },
            RefSource::Variable { field, binder_key } => RefSource::Variable {
                field: self.path(field)?,
                binder_key: binder_key.clone(),
            },
        })
    }
}

/// A key or reference declaration that is not well formed.
///
/// [`validate`](crate::validate()) reports these as
/// [`ValidationError::KeyRef`](crate::ValidationError::KeyRef).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyRefError {
    /// A declaration names a vertex the schema does not have.
    UnknownVertex {
        /// The missing vertex.
        vertex: Name,
    },
    /// Two keys of one vertex share an id.
    DuplicateKeyId {
        /// The keyed vertex.
        vertex: Name,
        /// The repeated key id.
        key: Name,
    },
    /// A path is not a chain of schema edges starting at its owner.
    PathDoesNotResolve {
        /// The vertex the path starts from: the keyed vertex for a key part,
        /// the holder for a reference.
        owner: Name,
        /// The path.
        path: Vec<Edge>,
    },
    /// A `Position` key part indexes the targets of an edge that is not
    /// ordered.
    PositionNotOrdered {
        /// The key.
        key: Name,
        /// The edge that has no ordering.
        edge: Edge,
    },
    /// A reference names a key its target vertex does not declare.
    UnknownTargetKey {
        /// The reference.
        reference: Name,
        /// The key it names.
        key: Name,
    },
    /// A key-value reference holds a different number of parts than the key
    /// it resolves through.
    ArityMismatch {
        /// The reference.
        reference: Name,
        /// The number of parts of the resolving key; zero when the target
        /// vertex has no key to resolve through.
        expected: usize,
        /// The number of parts the reference holds.
        found: usize,
    },
    /// A variable reference binds a key whose role is not `Identifier`.
    BinderNotIdentifier {
        /// The reference.
        reference: Name,
        /// The binder key.
        key: Name,
    },
    /// A reference's edge does not run from its holder to its target vertex.
    EdgeMismatch {
        /// The reference.
        reference: Name,
        /// The edge.
        edge: Edge,
    },
    /// A reference of a protocol whose semantic shape differs from its
    /// syntactic shape declares no edge.
    EdgeRequired {
        /// The reference.
        reference: Name,
    },
    /// A key scoped `Within` a vertex that is not a proper ancestor of the
    /// keyed vertex.
    WithinNotAncestor {
        /// The key.
        key: Name,
        /// The named scope vertex.
        vertex: Name,
    },
}

impl fmt::Display for KeyRefError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let path_text = |path: &[Edge]| -> String {
            path.iter()
                .map(|edge| format!("{} -> {} ({})", edge.src, edge.tgt, edge.kind))
                .collect::<Vec<_>>()
                .join(", ")
        };
        match self {
            Self::UnknownVertex { vertex } => {
                write!(f, "declaration names unknown vertex {vertex}")
            }
            Self::DuplicateKeyId { vertex, key } => {
                write!(f, "vertex {vertex} declares key {key} more than once")
            }
            Self::PathDoesNotResolve { owner, path } => write!(
                f,
                "path [{}] does not resolve from {owner}",
                path_text(path)
            ),
            Self::PositionNotOrdered { key, edge } => write!(
                f,
                "key {key} indexes the targets of unordered edge {} -> {} ({})",
                edge.src, edge.tgt, edge.kind
            ),
            Self::UnknownTargetKey { reference, key } => {
                write!(f, "reference {reference} names unknown target key {key}")
            }
            Self::ArityMismatch {
                reference,
                expected,
                found,
            } => write!(
                f,
                "reference {reference} holds {found} key parts where its key has {expected}"
            ),
            Self::BinderNotIdentifier { reference, key } => write!(
                f,
                "reference {reference} binds key {key}, which is not an identifier"
            ),
            Self::EdgeMismatch { reference, edge } => write!(
                f,
                "reference {reference} edge {} -> {} ({}) does not run from its holder to its target",
                edge.src, edge.tgt, edge.kind
            ),
            Self::EdgeRequired { reference } => write!(
                f,
                "reference {reference} needs an edge: its protocol's semantic shape differs from its syntactic shape"
            ),
            Self::WithinNotAncestor { key, vertex } => write!(
                f,
                "key {key} is scoped within {vertex}, which is not an ancestor of its vertex"
            ),
        }
    }
}

impl std::error::Error for KeyRefError {}
