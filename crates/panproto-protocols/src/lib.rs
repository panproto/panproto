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
//! - **Serialization**: Protobuf, Avro, Thrift, Cap'n Proto, `FlatBuffers`, ASN.1, Bond, `MsgPack`
//! - **Data Schema**: JSON Schema, XML/XSD, CSV/Table Schema, YAML, TOML, CDDL, INI, BSON
//! - **API**: GraphQL, `OpenAPI`, `AsyncAPI`, RAML, JSON:API
//! - **Database**: SQL, `MongoDB`, Cassandra, `DynamoDB`, Neo4j, Redis
//! - **Type System**: TypeScript, Python, Rust, Java, Go, Swift, Kotlin, C#
//! - **Web/Document**: `ATProto`, HTML, CSS, DOCX, ODF, Markdown, JSX, Vue, Svelte
//! - **Data Science**: Parquet, Arrow, `DataFrame`
//! - **Domain**: `GeoJSON`, FHIR, RSS/Atom, vCard/iCal, EDI X12, SWIFT MT
//! - **Config**: HCL, K8s CRD, Docker Compose, `CloudFormation`, Ansible

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
/// Serialization and IDL protocol definitions.
pub mod serialization;
/// Shared component theory definitions (building-block GATs).
pub mod theories;
/// Programming language type system protocol definitions.
pub mod type_system;
/// Web and document format protocol definitions.
pub mod web_document;

pub use error::ProtocolError;

// Re-export existing protocols at crate root for backward compatibility.
pub use api::graphql;
pub use data_schema::json_schema;
pub use database::sql;
pub use serialization::protobuf;
pub use web_document::atproto;
