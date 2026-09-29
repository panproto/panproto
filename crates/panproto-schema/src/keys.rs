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

use std::collections::BTreeMap;

use panproto_gat::Name;
use serde::{Deserialize, Serialize};

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
    /// An identifier other elements refer to (`xml:id`, `@id`, a GraphQL
    /// `ID`, a variable binder).
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
}
