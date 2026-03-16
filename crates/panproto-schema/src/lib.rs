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

mod builder;
mod colimit;
mod error;
mod morphism;
mod normalize;
mod protocol;
mod schema;
pub mod serde_helpers;
mod validate;

pub use builder::SchemaBuilder;
pub use colimit::{SchemaOverlap, schema_pushout};
pub use error::{SchemaError, ValidationError};
pub use morphism::SchemaMorphism;
pub use normalize::normalize;
pub use protocol::{EdgeRule, Protocol};
pub use schema::{
    Constraint, Edge, HyperEdge, Ordering, RecursionPoint, Schema, Span, UsageMode, Variant, Vertex,
};
pub use validate::validate;
