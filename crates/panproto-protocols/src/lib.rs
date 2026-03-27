#![allow(
    unknown_lints,
    clippy::match_same_arms,
    clippy::similar_names,
    clippy::only_used_in_recursion,
    clippy::option_if_let_else,
    clippy::collapsible_else_if,
    clippy::branches_sharing_code,
    clippy::explicit_iter_loop,
    clippy::manual_let_else,
    clippy::hashset_insert_after_contains,
    clippy::set_contains_or_insert
)]

//! # panproto-protocols
//!
//! Built-in protocol definitions for panproto.
//!
//! Each protocol is defined by a schema theory GAT and an instance theory GAT,
//! composed via colimit from reusable building-block theories. Every protocol
//! provides both a parser (native format → `Schema`) and an emitter
//! (`Schema` → native format) for bidirectional format conversion.
//!
//! ## Protocol Categories
//!
//! - **Serialization**: Avro, `FlatBuffers`, ASN.1, Bond, `MsgPack`
//! - **Data Schema**: CDDL, BSON
//! - **API**: `OpenAPI`, `AsyncAPI`, RAML, JSON:API
//! - **Database**: `MongoDB`, Cassandra, `DynamoDB`, Neo4j, Redis
//! - **Web/Document**: `ATProto`, DOCX, ODF
//! - **Data Science**: Parquet, Arrow, `DataFrame`
//! - **Domain**: `GeoJSON`, FHIR, RSS/Atom, vCard/iCal, EDI X12, SWIFT MT
//! - **Config**: K8s CRD, Docker Compose, `CloudFormation`, Ansible

/// Linguistic annotation format protocol definitions.
pub mod annotation;
/// API specification protocol definitions.
pub mod api;
/// Configuration format protocol definitions.
pub mod config;
/// Data schema protocol definitions.
pub mod data_schema;
/// Data science and analytics protocol definitions.
pub mod data_science;
/// Database schema protocol definitions.
pub mod database;
/// Domain-specific protocol definitions.
pub mod domain;
/// Shared emit helpers for protocol serialization.
pub mod emit;
/// Error types for protocol operations.
pub mod error;
/// Raw file protocol for non-code files (README, LICENSE, images, etc.).
pub mod raw_file;
/// Serialization and IDL protocol definitions.
pub mod serialization;
/// Shared component theory definitions (building-block GATs).
pub mod theories;
/// Web and document format protocol definitions.
pub mod web_document;

pub use error::ProtocolError;

// Re-export existing protocols at crate root for backward compatibility.
pub use web_document::atproto;
