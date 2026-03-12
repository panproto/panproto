//! # panproto-protocols
//!
//! Built-in protocol definitions for panproto.
//!
//! Each protocol is defined by a schema theory GAT and an instance theory GAT,
//! composed via colimit from reusable building-block theories.
//!
//! Supported protocols:
//! - **`ATProto`**: Constrained multigraph schemas with W-type instances
//! - **SQL**: Hypergraph schemas with set-valued functor instances
//! - **Protobuf**: Simple graph schemas with flat instances
//! - **GraphQL**: Typed graph schemas with W-type instances
//! - **JSON Schema**: Constrained multigraph schemas with W-type instances

/// ATProto protocol definition and lexicon parser.
pub mod atproto;
/// Error types for protocol operations.
pub mod error;
/// GraphQL protocol definition and SDL parser.
pub mod graphql;
/// JSON Schema protocol definition and parser.
pub mod json_schema;
/// Protobuf protocol definition and `.proto` parser.
pub mod protobuf;
/// SQL protocol definition and DDL parser.
pub mod sql;
/// Shared component theory definitions (building-block GATs).
pub mod theories;

pub use error::ProtocolError;
