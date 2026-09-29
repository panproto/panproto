//! # panproto-schema
//!
//! Schema representation for panproto.
//!
//! A schema is a model of a protocol's schema theory GAT (from
//! `panproto-gat`). This crate provides:
//!
//! - **[`Schema`]**: The core schema data structure with precomputed
//!   adjacency indices for efficient graph traversal.
//! - **[`SchemaBuilder`]**: A fluent, protocol-aware builder that
//!   validates each element as it is added.
//! - **[`Protocol`]**: Configuration describing which schema/instance
//!   theories and edge rules a data format uses.
//! - **[`normalize`]**: Ref-chain collapse for schemas with `Ref` vertices.
//! - **[`validate`]**: Post-hoc validation of a schema against a protocol.
//! - **[`induce()`]**: The supported way to cut a sub-schema, accounting for
//!   every field in its own key space.
//! - **[`canonical_bytes`]** and **[`canonical_digest`]**: A total,
//!   deterministic encoding of a schema, and the blake3 over it, for content
//!   identity.

mod abstract_schema;
mod builder;
pub mod canonical;
mod colimit;
pub mod constraint_semantics;
pub mod equivalence;
mod error;
pub mod induce;
pub mod keys;
mod morphism;
mod normalize;
mod protocol;
mod schema;
pub mod serde_helpers;
pub mod shape_kind;
mod validate;
pub mod value_kinds;

pub use abstract_schema::{
    AbstractSchema, DecoratedSchema, LayoutConstraintsPresent, LayoutWitness,
};
pub use builder::SchemaBuilder;
pub use canonical::{CanonicalWriter, canonical_bytes, canonical_digest};
pub use colimit::{SchemaOverlap, schema_pushout};
pub use constraint_semantics::{
    ConstraintSemantics, ConstraintValue, ConstraintValueError, ConstraintValueReason, LengthUnit,
    ValueParameter,
};
pub use equivalence::{edge_multiset, kind_multiset};
pub use error::{SchemaError, ValidationError};
pub use induce::{induce, induce_on_vertices};
pub use keys::{
    Cardinality, Key, KeyPart, KeyRole, KeyScope, OffsetUnit, OnUnresolved, PathSyntax, RefSource,
    RefTextSource, Reference, ResolveMode, SchemaDeclarations,
};
pub use morphism::SchemaMorphism;
pub use normalize::normalize;
pub use protocol::{EdgeRule, Protocol};
pub use schema::{
    CoercionSpec, Constraint, Edge, HyperEdge, Ordering, RecursionPoint, Schema, Span, UsageMode,
    Variant, Vertex, primary_entry,
};
pub use shape_kind::InstanceShapeKind;
pub use validate::validate;
pub use value_kinds::{BytesText, ScalarKind, ValueSpec};
