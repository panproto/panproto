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

use panproto_expr::Value;
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
    /// Equality used for each key part, in key-part order.
    ///
    /// An empty list means [`KeyEquality::Representation`] for every part.
    /// It is the legacy form and is omitted from serialized declarations.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub equality: Vec<KeyEquality>,
}

/// Equality declared for one part of a [`Key`].
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyEquality {
    /// Equality of the value's canonical representation.
    #[default]
    Representation,
    /// Exact mathematical equality across numeric representations.
    NumericValue,
    /// Exact mathematical equality within the selected numeric primitive.
    NumericValueWithinKind(NumericPrimitive),
    /// SQL numeric equality and the declared text comparison.
    Sql {
        /// Text collation.
        collation: SqlCollation,
        /// Treatment of trailing U+0020 characters.
        pad: SqlPad,
    },
    /// XML non-CDATA whitespace normalization, then representation equality.
    XmlNonCdata,
}

/// Primitive used by [`KeyEquality::NumericValueWithinKind`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NumericPrimitive {
    /// The shared integer and decimal primitive.
    Decimal,
    /// A binary floating-point primitive of the given width.
    Float(u8),
    /// Select the primitive from the value variant.
    Any,
}

/// Text collation used by [`KeyEquality::Sql`].
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SqlCollation {
    /// Compare Unicode scalar values without folding.
    #[default]
    Binary,
    /// Fold ASCII `A` through `Z` only.
    AsciiCaseInsensitive,
    /// A named collation whose comparison panproto does not evaluate.
    Named(Name),
}

/// Treatment of trailing spaces under [`KeyEquality::Sql`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SqlPad {
    /// Keep trailing U+0020 characters.
    #[default]
    NoPad,
    /// Ignore trailing U+0020 characters.
    PadSpace,
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
    /// A stable-sort key, which imposes ordering but not uniqueness.
    SortKey,
    /// A non-unique index, which validates lookups but resolves no pair.
    Index,
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
    /// Optional representation-equality selector for this reference.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guard: Option<RefGuard>,
    /// Composite-null matching rule.
    #[serde(default, skip_serializing_if = "NullMatch::is_default")]
    pub null_match: NullMatch,
    /// Optional vertex used by a route as a null sentinel.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub null_vertex: Option<Name>,
    /// Optional traversal from the holder to the target key's scope owner.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope_via: Option<ScopeVia>,
}

/// A value selector that enables a reference only on matching holders.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RefGuard {
    /// Path from the holder to the selector value.
    pub field: Vec<Edge>,
    /// Canonical representation the selector must equal.
    pub equals: Value,
}

impl PartialEq for RefGuard {
    fn eq(&self, other: &Self) -> bool {
        self.field == other.field && self.equals.canonical_cmp(&other.equals).is_eq()
    }
}

impl Eq for RefGuard {}

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
        /// Whether `end` is an absolute offset or a length.
        #[serde(default, skip_serializing_if = "SpanEnd::is_default")]
        end_form: SpanEnd,
        /// Origin subtracted from each written coordinate.
        #[serde(default, skip_serializing_if = "is_zero")]
        origin: u64,
        /// Ordered edge whose targets carry discontinuous fragments.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fragments: Option<Edge>,
    },
    /// A variable: the first binding occurrence defines, later occurrences
    /// refer.
    Variable {
        /// The field holding the variable name.
        field: Vec<Edge>,
        /// The identifier key the variable binds.
        binder_key: Name,
        /// Which matching definition binds an occurrence.
        #[serde(default, skip_serializing_if = "VariableBinding::is_default")]
        binding: VariableBinding,
    },
    /// An identified occurrence reached inline from its holder.
    Inline {
        /// Field whose target is the occurrence.
        field: Vec<Edge>,
    },
}

/// How a variable occurrence selects its defining occurrence.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VariableBinding {
    /// The first matching definition binds every occurrence.
    #[default]
    FirstDefines,
    /// The closest matching preceding definition binds the occurrence.
    NearestPreceding,
}

impl VariableBinding {
    // Serde's `skip_serializing_if` callback receives a shared reference even
    // for a copyable field.
    #[allow(clippy::trivially_copy_pass_by_ref)]
    const fn is_default(&self) -> bool {
        matches!(self, Self::FirstDefines)
    }
}

/// The syntax of a path-valued reference.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PathSyntax {
    /// An RFC 6901 JSON Pointer.
    JsonPointer,
    /// An `XPath` expression.
    XPath,
    /// A URI or IRI resolved under the declared base environment.
    Uri {
        /// Source of the base environment.
        #[serde(default)]
        base: UriBase,
    },
    /// An XML `IDREF`.
    IdRef,
}

/// How a URI-valued reference obtains its base environment.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum UriBase {
    /// The written reference must be absolute.
    #[default]
    Absolute,
    /// Read a base URI from a holder field.
    Field(Vec<Edge>),
    /// Fold base and prefix directives preceding the holder.
    Positional {
        /// Vertex carrying a base directive, when the route has one.
        base: Option<Name>,
        /// Vertex carrying a prefix directive, when the route has one.
        prefix: Option<Name>,
    },
    /// Read a base on the nearest ancestor of a declared type.
    Ancestor {
        /// The ancestor vertex.
        ancestor: Name,
        /// Base-valued field of that ancestor.
        field: Vec<Edge>,
        /// Rule for joining the base and reference.
        join: UriJoin,
    },
    /// Fold a base-valued field over the ancestor-or-self chain.
    Inherited {
        /// The field carrying each base contribution.
        field: Vec<Edge>,
    },
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum UriBaseRef<'a> {
    Positional {
        base: &'a Option<Name>,
        prefix: &'a Option<Name>,
    },
    Ancestor {
        ancestor: &'a Name,
        field: &'a [Edge],
        join: UriJoin,
    },
    Inherited {
        field: &'a [Edge],
    },
}

impl Serialize for UriBase {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Self::Absolute => Option::<&[Edge]>::None.serialize(serializer),
            Self::Field(field) => field.serialize(serializer),
            Self::Positional { base, prefix } => {
                UriBaseRef::Positional { base, prefix }.serialize(serializer)
            }
            Self::Ancestor {
                ancestor,
                field,
                join,
            } => UriBaseRef::Ancestor {
                ancestor,
                field,
                join: *join,
            }
            .serialize(serializer),
            Self::Inherited { field } => UriBaseRef::Inherited { field }.serialize(serializer),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum ExtendedUriBase {
    Absolute,
    Field(Vec<Edge>),
    Positional {
        base: Option<Name>,
        prefix: Option<Name>,
    },
    Ancestor {
        ancestor: Name,
        field: Vec<Edge>,
        join: UriJoin,
    },
    Inherited {
        field: Vec<Edge>,
    },
}

#[derive(Deserialize)]
#[serde(untagged)]
enum UriBaseWire {
    Legacy(Option<Vec<Edge>>),
    Extended(ExtendedUriBase),
}

impl<'de> Deserialize<'de> for UriBase {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(match UriBaseWire::deserialize(deserializer)? {
            UriBaseWire::Legacy(None) | UriBaseWire::Extended(ExtendedUriBase::Absolute) => {
                Self::Absolute
            }
            UriBaseWire::Legacy(Some(field))
            | UriBaseWire::Extended(ExtendedUriBase::Field(field)) => Self::Field(field),
            UriBaseWire::Extended(ExtendedUriBase::Positional { base, prefix }) => {
                Self::Positional { base, prefix }
            }
            UriBaseWire::Extended(ExtendedUriBase::Ancestor {
                ancestor,
                field,
                join,
            }) => Self::Ancestor {
                ancestor,
                field,
                join,
            },
            UriBaseWire::Extended(ExtendedUriBase::Inherited { field }) => {
                Self::Inherited { field }
            }
        })
    }
}

/// Joining rule for an ancestor-provided URI base.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UriJoin {
    /// RFC 3986 section 5.2 resolution.
    #[default]
    Rfc3986,
    /// FHIR REST service-base joining.
    FhirServiceBase,
}

/// Where the text of a standoff reference lives.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefTextSource {
    /// A field of the same document.
    Local(Vec<Edge>),
    /// A field naming an external text.
    External(Vec<Edge>),
    /// Text reached through another resolved reference.
    Referenced {
        /// The reference declaration followed from the holder.
        reference: Name,
        /// Text path from the reference target.
        text: Vec<Edge>,
        /// Optional external-text key path used when `text` is absent.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        external: Option<Vec<Edge>>,
    },
    /// Text selected by another standoff reference on the same holder.
    Span {
        /// The outer standoff reference.
        reference: Name,
    },
    /// External text named by a URI-valued field.
    ExternalUri {
        /// URI-valued field.
        field: Vec<Edge>,
        /// Source of its base environment.
        base: UriBase,
    },
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

/// Interpretation of a standoff reference's written end coordinate.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpanEnd {
    /// An absolute end offset.
    #[default]
    Offset,
    /// A length added to the start offset.
    Length,
}

impl SpanEnd {
    // Serde's `skip_serializing_if` callback receives a shared reference even
    // for a copyable field.
    #[allow(clippy::trivially_copy_pass_by_ref)]
    const fn is_default(&self) -> bool {
        matches!(self, Self::Offset)
    }
}

/// Null matching rule for a composite foreign key.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NullMatch {
    /// Any null part makes the tuple absent.
    #[default]
    Simple,
    /// An all-null tuple is absent; a partly null tuple is unresolved.
    Full,
}

impl NullMatch {
    // Serde's `skip_serializing_if` callback receives a shared reference even
    // for a copyable field.
    #[allow(clippy::trivially_copy_pass_by_ref)]
    const fn is_default(&self) -> bool {
        matches!(self, Self::Simple)
    }
}

/// Optional traversal from a reference holder to its key-scope owner.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScopeVia {
    /// Vertex anchoring the holder's nearest syntactic scope ancestor.
    pub ancestor: Name,
    /// Single-valued references followed to the target key's scope owner.
    pub hops: Vec<Name>,
}

/// A key whose members range over several vertex types.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetSet {
    /// Member declarations as `(vertex, key id)` pairs.
    pub members: Vec<(Name, Name)>,
    /// Shared scope of all member keys.
    pub scope: KeyScope,
}

// Serde's `skip_serializing_if` callback receives a shared reference even for
// a copyable field.
#[allow(clippy::trivially_copy_pass_by_ref)]
const fn is_zero(value: &u64) -> bool {
    *value == 0
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
    /// Keys whose members range over several vertex types.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub target_sets: BTreeMap<Name, TargetSet>,
}

impl SchemaDeclarations {
    /// Returns `true` iff the schema declares no key, reference, or target
    /// set.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty() && self.references.is_empty() && self.target_sets.is_empty()
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
    ///     equality: vec![],
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
    /// different parts, role, scope, or equality is already declared on
    /// `vertex`.
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

    /// The target set declared as `id`, if any.
    #[must_use]
    pub fn target_set(&self, id: &str) -> Option<&TargetSet> {
        self.target_sets.get(id)
    }

    /// Declare `set` as `id`, returning the declaration it replaced.
    pub fn insert_target_set(&mut self, id: Name, set: TargetSet) -> Option<TargetSet> {
        self.target_sets.insert(id, set)
    }

    /// Declare `set` as `id` unless an equal declaration is already there.
    ///
    /// # Errors
    ///
    /// Returns [`SchemaError::TargetSetConflict`] when a different target
    /// set already has `id`.
    pub fn merge_target_set(&mut self, id: Name, set: TargetSet) -> Result<(), SchemaError> {
        match self.target_sets.get(&id) {
            Some(existing) if *existing == set => Ok(()),
            Some(_) => Err(SchemaError::TargetSetConflict { set: id }),
            None => {
                self.insert_target_set(id, set);
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

        for (id, set) in &self.target_sets {
            let mut members = Vec::with_capacity(set.members.len());
            for (owner, key) in &set.members {
                if dropped_keys.contains(&(owner.clone(), key.clone())) {
                    report.dropped_target_set_members.push((
                        id.clone(),
                        owner.clone(),
                        key.clone(),
                    ));
                    continue;
                }
                match (vertex(owner), self.key(owner, key)) {
                    (Some(image), Some(_)) => members.push((image, key.clone())),
                    _ => report.dropped_target_set_members.push((
                        id.clone(),
                        owner.clone(),
                        key.clone(),
                    )),
                }
            }
            if members.is_empty() {
                report.dropped_target_sets.push(id.clone());
                continue;
            }
            let scope = match &set.scope {
                KeyScope::Within(ancestor) => {
                    let Some(ancestor) = vertex(ancestor) else {
                        report.dropped_target_sets.push(id.clone());
                        continue;
                    };
                    KeyScope::Within(ancestor)
                }
                other => other.clone(),
            };
            out.merge_target_set(id.clone(), TargetSet { members, scope })?;
        }

        let mut dropped_references: BTreeSet<Name> = BTreeSet::new();
        for (id, reference) in &self.references {
            let key_dropped = self.resolving_key(reference).is_some_and(|key| {
                dropped_keys.contains(&(reference.target_vertex.clone(), key.id.clone()))
            });
            let targets_set = self.target_sets.contains_key(&reference.target_vertex);
            match carry
                .reference(reference, targets_set)
                .filter(|_| !key_dropped)
            {
                Some(mapped) => out.merge_reference(mapped)?,
                None => {
                    dropped_references.insert(id.clone());
                }
            }
        }

        report.dropped_keys = dropped_keys.into_iter().collect();
        report.dropped_references = dropped_references.into_iter().collect();
        report.dropped_target_set_members.sort();
        report.dropped_target_set_members.dedup();
        report.dropped_target_sets.sort();
        report.dropped_target_sets.dedup();
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
        for (set_id, set) in &mut self.target_sets {
            let removed: Vec<(Name, Name)> = set
                .members
                .iter()
                .filter(|member| doomed.contains(*member))
                .cloned()
                .collect();
            set.members.retain(|member| !doomed.contains(member));
            for (vertex, key) in removed {
                report
                    .dropped_target_set_members
                    .push((set_id.clone(), vertex, key));
            }
        }
        let empty_sets: Vec<Name> = self
            .target_sets
            .iter()
            .filter(|(_, set)| set.members.is_empty())
            .map(|(id, _)| id.clone())
            .collect();
        for id in &empty_sets {
            let orphaned: Vec<Name> = self
                .references
                .values()
                .filter(|reference| reference.target_vertex == *id)
                .map(|reference| reference.id.clone())
                .collect();
            for reference in orphaned {
                self.references.remove(&reference);
                report.dropped_references.push(reference);
            }
        }
        for id in empty_sets {
            self.target_sets.remove(&id);
            report.dropped_target_sets.push(id);
        }
        report.dropped_references.sort();
        report.dropped_target_set_members.sort();
        report.dropped_target_set_members.dedup();
        report.dropped_target_sets.sort();
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
        for (id, set) in &other.target_sets {
            self.merge_target_set(id.clone(), set.clone())?;
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
    /// Dropped target-set members as `(set id, vertex, key id)`, sorted.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dropped_target_set_members: Vec<(Name, Name, Name)>,
    /// Target-set ids dropped because no member survived, sorted.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dropped_target_sets: Vec<Name>,
}

impl DeclarationReport {
    /// Returns `true` iff nothing was dropped.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.dropped_keys.is_empty()
            && self.dropped_references.is_empty()
            && self.dropped_target_set_members.is_empty()
            && self.dropped_target_sets.is_empty()
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
        self.dropped_target_set_members
            .extend(other.dropped_target_set_members);
        self.dropped_target_set_members.sort();
        self.dropped_target_set_members.dedup();
        self.dropped_target_sets.extend(other.dropped_target_sets);
        self.dropped_target_sets.sort();
        self.dropped_target_sets.dedup();
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
                equality: key.equality.clone(),
            },
        ))
    }

    /// Map a reference.
    fn reference(&self, reference: &Reference, targets_set: bool) -> Option<Reference> {
        let edge = match &reference.edge {
            Some(edge) if targets_set => Some(Edge {
                src: (self.vertex)(&edge.src)?,
                tgt: edge.tgt.clone(),
                kind: edge.kind.clone(),
                name: edge.name.clone(),
            }),
            Some(edge) => Some((self.edge)(edge)?),
            None => None,
        };
        Some(Reference {
            id: reference.id.clone(),
            holder: (self.vertex)(&reference.holder)?,
            source: self.source(&reference.source)?,
            target_vertex: if targets_set {
                reference.target_vertex.clone()
            } else {
                (self.vertex)(&reference.target_vertex)?
            },
            target_key: reference.target_key.clone(),
            edge,
            cardinality: reference.cardinality.clone(),
            nullable: reference.nullable,
            on_unresolved: reference.on_unresolved.clone(),
            mode: reference.mode.clone(),
            guard: match &reference.guard {
                Some(guard) => Some(RefGuard {
                    field: self.path(&guard.field)?,
                    equals: guard.equals.clone(),
                }),
                None => None,
            },
            null_match: reference.null_match,
            null_vertex: match &reference.null_vertex {
                Some(vertex) => Some((self.vertex)(vertex)?),
                None => None,
            },
            scope_via: match &reference.scope_via {
                Some(via) => Some(ScopeVia {
                    ancestor: (self.vertex)(&via.ancestor)?,
                    hops: via.hops.clone(),
                }),
                None => None,
            },
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
                    PathSyntax::Uri { base } => PathSyntax::Uri {
                        base: self.uri_base(base)?,
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
                end_form,
                origin,
                fragments,
            } => RefSource::Standoff {
                text: self.text_source(text)?,
                start: self.path(start)?,
                end: self.path(end)?,
                unit: unit.clone(),
                end_inclusive: *end_inclusive,
                end_form: *end_form,
                origin: *origin,
                fragments: match fragments {
                    Some(edge) => Some((self.edge)(edge)?),
                    None => None,
                },
            },
            RefSource::Variable {
                field,
                binder_key,
                binding,
            } => RefSource::Variable {
                field: self.path(field)?,
                binder_key: binder_key.clone(),
                binding: *binding,
            },
            RefSource::Inline { field } => RefSource::Inline {
                field: self.path(field)?,
            },
        })
    }

    /// Map every edge and vertex named by a URI base declaration.
    fn uri_base(&self, base: &UriBase) -> Option<UriBase> {
        Some(match base {
            UriBase::Absolute => UriBase::Absolute,
            UriBase::Field(path) => UriBase::Field(self.path(path)?),
            UriBase::Positional { base, prefix } => UriBase::Positional {
                base: match base {
                    Some(id) => Some((self.vertex)(id)?),
                    None => None,
                },
                prefix: match prefix {
                    Some(id) => Some((self.vertex)(id)?),
                    None => None,
                },
            },
            UriBase::Ancestor {
                ancestor,
                field,
                join,
            } => UriBase::Ancestor {
                ancestor: (self.vertex)(ancestor)?,
                field: self.path(field)?,
                join: *join,
            },
            UriBase::Inherited { field } => UriBase::Inherited {
                field: self.path(field)?,
            },
        })
    }

    /// Map every path of a standoff text source.
    fn text_source(&self, source: &RefTextSource) -> Option<RefTextSource> {
        Some(match source {
            RefTextSource::Local(path) => RefTextSource::Local(self.path(path)?),
            RefTextSource::External(path) => RefTextSource::External(self.path(path)?),
            RefTextSource::Referenced {
                reference,
                text,
                external,
            } => RefTextSource::Referenced {
                reference: reference.clone(),
                text: self.path(text)?,
                external: match external {
                    Some(path) => Some(self.path(path)?),
                    None => None,
                },
            },
            RefTextSource::Span { reference } => RefTextSource::Span {
                reference: reference.clone(),
            },
            RefTextSource::ExternalUri { field, base } => RefTextSource::ExternalUri {
                field: self.path(field)?,
                base: self.uri_base(base)?,
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
    /// A key's nonempty equality vector has a different length than its
    /// parts.
    EqualityArity {
        /// Vertex carrying the key.
        vertex: Name,
        /// Key id.
        key: Name,
        /// Number of key parts.
        expected: usize,
        /// Number of equality declarations.
        found: usize,
    },
    /// A target-set id is also a vertex id.
    TargetSetNameClash {
        /// Conflicting target-set id.
        set: Name,
    },
    /// A target set has no member key.
    EmptyTargetSet {
        /// Empty target-set id.
        set: Name,
    },
    /// A target-set member is absent, repeated, or incompatible with the
    /// set's shared signature.
    TargetSetMemberMismatch {
        /// Target-set id.
        set: Name,
        /// Member vertex.
        vertex: Name,
        /// Member key id.
        key: Name,
    },
    /// A sort key is not carried by one ordered edge from its scope owner.
    SortKeyNotOrdered {
        /// Sort-key id.
        key: Name,
        /// Vertex carrying the sort key.
        vertex: Name,
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
    /// A reference names no resolvable key or target set.
    UnknownTarget {
        /// Reference id.
        reference: Name,
    },
    /// A reference names a sort key or index, which promises no unique
    /// resolution.
    KeyNotResolvable {
        /// Reference id.
        reference: Name,
        /// Non-resolvable key id.
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
    /// Inline identification must redirect its occurrence.
    InlineRequiresRedirect {
        /// Reference id.
        reference: Name,
    },
    /// Inline identification must range over a target set.
    InlineRequiresTargetSet {
        /// Reference id.
        reference: Name,
    },
    /// A standoff length cannot also use an inclusive end.
    LengthInclusive {
        /// Reference id.
        reference: Name,
    },
    /// A fragmented standoff is not many-valued over an ordered holder edge.
    InvalidFragments {
        /// Reference id.
        reference: Name,
    },
    /// Two guarded references in one selection group use the same canonical
    /// guard value.
    DuplicateGuard {
        /// Colliding reference ids, in declaration order.
        references: Vec<Name>,
    },
    /// A target-set reference redundantly names a per-vertex target key.
    TargetSetHasKey {
        /// Reference id.
        reference: Name,
    },
    /// One binder key is used under incompatible binding modes or also as a
    /// non-variable target.
    BinderModeConflict {
        /// Binder key id.
        key: Name,
        /// References using the key, in declaration order.
        references: Vec<Name>,
    },
    /// A referenced or span-relative text declaration is ill-typed or cyclic.
    TextReferenceInvalid {
        /// Standoff reference id.
        reference: Name,
        /// Referenced text-producing declaration.
        text_reference: Name,
    },
    /// A scope-via chain is empty, ill-typed, or cyclic.
    ScopeViaInvalid {
        /// Reference id.
        reference: Name,
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
    // A single exhaustive match keeps every public declaration error's text
    // auditable next to the variant definition.
    #[allow(clippy::too_many_lines)]
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
            Self::EqualityArity {
                vertex,
                key,
                expected,
                found,
            } => write!(
                f,
                "key {key} on {vertex} has {found} equality declarations for {expected} parts"
            ),
            Self::TargetSetNameClash { set } => {
                write!(f, "target set {set} is also a vertex id")
            }
            Self::EmptyTargetSet { set } => {
                write!(f, "target set {set} has no members")
            }
            Self::TargetSetMemberMismatch { set, vertex, key } => write!(
                f,
                "target set {set} has missing, repeated, or incompatible member {vertex}.{key}"
            ),
            Self::SortKeyNotOrdered { key, vertex } => write!(
                f,
                "sort key {key} on {vertex} is not carried by one ordered scope edge"
            ),
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
            Self::UnknownTarget { reference } => {
                write!(f, "reference {reference} names no resolvable target")
            }
            Self::KeyNotResolvable { reference, key } => {
                write!(f, "reference {reference} names non-resolvable key {key}")
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
            Self::InlineRequiresRedirect { reference } => {
                write!(f, "inline reference {reference} must redirect")
            }
            Self::InlineRequiresTargetSet { reference } => {
                write!(f, "inline reference {reference} must target a target set")
            }
            Self::LengthInclusive { reference } => write!(
                f,
                "standoff reference {reference} cannot combine a length with an inclusive end"
            ),
            Self::InvalidFragments { reference } => write!(
                f,
                "standoff reference {reference} has an invalid fragment carrier"
            ),
            Self::DuplicateGuard { references } => write!(
                f,
                "guard values collide for references {}",
                references
                    .iter()
                    .map(AsRef::<str>::as_ref)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Self::TargetSetHasKey { reference } => write!(
                f,
                "target-set reference {reference} redundantly names a target key"
            ),
            Self::BinderModeConflict { key, references } => write!(
                f,
                "binder key {key} has incompatible uses in references {}",
                references
                    .iter()
                    .map(AsRef::<str>::as_ref)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Self::TextReferenceInvalid {
                reference,
                text_reference,
            } => write!(
                f,
                "standoff reference {reference} has invalid text reference {text_reference}"
            ),
            Self::ScopeViaInvalid { reference } => {
                write!(f, "reference {reference} has an invalid scope-via chain")
            }
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
