//! # panproto-io
//!
//! Instance-level presentation functors for panproto.
//!
//! This crate implements the parse/emit operations that connect raw format
//! bytes to abstract instance models ([`panproto_inst::WInstance`] and
//! [`panproto_inst::FInstance`]), completing the functorial data migration
//! pipeline. Each protocol's parser/emitter pair is a presentation functor
//! witnessing that the raw format syntax is a faithful encoding of the
//! algebraic model defined by the protocol's instance theory.
//!
//! ## Theoretical grounding
//!
//! panproto's two-parameter architecture defines each protocol P as a pair
//! of GATs: a schema theory `T_P^S` and an instance theory `T_P^I`. This crate
//! provides the **instance presentations** — the functors mapping between
//! concrete format syntax and models of `T_P^I`.
//!
//! Together with `panproto-protocols` (which provides schema presentations),
//! `panproto-mig` (which compiles schema migrations), and `panproto-inst`
//! (which executes the induced data migration functors), this crate
//! completes the pipeline:
//!
//! ```text
//! raw bytes ──panproto-io parse──→ Instance ──restrict──→ Instance ──panproto-io emit──→ raw bytes
//! ```
//!
//! The commutativity guarantee from Spivak 2012 ensures that
//! parse → restrict → emit composes correctly across all protocols.
//!
//! ## Architecture
//!
//! - [`traits`]: Core [`InstanceParser`] and [`InstanceEmitter`] traits
//! - [`registry`]: [`ProtocolRegistry`] mapping protocol names to implementations
//! - [`json_pathway`]: Shared SIMD-accelerated JSON → `WInstance` builder
//! - [`error`]: Error types for parse and emit operations
//! - [`arena`]: Arena allocation helpers for zero-copy hot paths
//!
//! ## Performance
//!
//! Parsing and emitting are designed to never be the bottleneck:
//! - SIMD JSON parsing via `simd-json` (2-4x over `serde_json`)
//! - SIMD byte scanning via `memchr` for delimited formats
//! - Arena allocation via `bumpalo` for hot paths
//! - Zero-copy parsing where format permits

// Allow concrete HashMap in public API per workspace conventions.
#![allow(clippy::implicit_hasher)]

/// Error types for instance parse and emit operations.
pub mod error;

/// Core traits: [`InstanceParser`] and [`InstanceEmitter`].
pub mod traits;

/// Protocol registry mapping names to parser/emitter implementations.
pub mod registry;

/// SIMD-accelerated JSON pathway for schema-guided instance parsing.
pub mod json_pathway;

/// Arena allocation helpers for zero-copy hot paths.
pub mod arena;

/// Generic JSON-based codec reused by ~30 protocols.
pub mod json_codec;

/// Zero-copy XML pathway for schema-guided instance parsing via `quick-xml`.
pub mod xml_pathway;

/// Generic XML-based codec reused by ~14 protocols.
pub mod xml_codec;

/// Shared tabular pathway for line/field-delimited formats via `memchr`.
pub mod tabular_pathway;

/// Generic tabular codec for delimited text protocols.
pub mod tabular_codec;

// ── Protocol category modules ──────────────────────────────────────────

/// API specification protocols (GraphQL, OpenAPI, AsyncAPI, JSON:API, RAML).
pub mod api;
/// Linguistic annotation protocols (brat, CoNLL-U, NAF, etc.).
pub mod annotation;
/// Configuration protocols (CloudFormation, Ansible, K8s CRD, HCL).
pub mod config;
/// Data schema protocols (JSON Schema, YAML Schema, TOML Schema, etc.).
pub mod data_schema;
/// Data science protocols (DataFrame, Parquet, Arrow).
pub mod data_science;
/// Database protocols (MongoDB, DynamoDB, Cassandra, Neo4j, SQL, Redis).
pub mod database;
/// Domain-specific protocols (GeoJSON, FHIR, RSS/Atom, vCard/iCal, etc.).
pub mod domain;
/// Serialization protocols (Protobuf, Avro, Thrift, Cap'n Proto, etc.).
pub mod serialization;
/// Type system protocols (TypeScript, Python, Rust, Java, Go, etc.).
pub mod type_system;
/// Web and document protocols (ATProto, HTML, Markdown, CSS, etc.).
pub mod web_document;

// Re-exports for convenience.
pub use error::{EmitInstanceError, ParseInstanceError};
pub use registry::ProtocolRegistry;
pub use traits::{InstanceEmitter, InstanceParser, NativeRepr};

/// Create a [`ProtocolRegistry`] with all available protocol codecs registered.
///
/// This is the primary entry point for consumers who want to parse/emit
/// instance data across all supported protocols.
///
/// # Example
///
/// ```ignore
/// let registry = panproto_io::default_registry();
/// let instance = registry.parse_wtype("graphql", &schema, &bytes)?;
/// ```
#[must_use]
pub fn default_registry() -> ProtocolRegistry {
    let mut registry = ProtocolRegistry::new();
    api::register_all(&mut registry);
    annotation::register_all(&mut registry);
    config::register_all(&mut registry);
    data_schema::register_all(&mut registry);
    data_science::register_all(&mut registry);
    database::register_all(&mut registry);
    domain::register_all(&mut registry);
    serialization::register_all(&mut registry);
    type_system::register_all(&mut registry);
    web_document::register_all(&mut registry);
    registry
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_registry_has_expected_protocols() {
        let registry = default_registry();

        // All 77 protocols.
        let expected = [
            // API (5)
            "graphql", "openapi", "asyncapi", "jsonapi", "raml",
            // Data schema (7)
            "json_schema", "yaml_schema", "toml_schema", "cddl", "bson", "csv_table", "ini_schema",
            // Database (6)
            "mongodb", "dynamodb", "cassandra", "neo4j", "sql", "redis",
            // Type system (8)
            "typescript", "python", "rust_serde", "java", "go_struct", "kotlin", "csharp", "swift",
            // Config (4)
            "cloudformation", "ansible", "k8s_crd", "hcl",
            // Data science (3)
            "dataframe", "parquet", "arrow",
            // Serialization (8)
            "protobuf", "avro", "thrift", "capnproto", "flatbuffers", "asn1", "bond", "msgpack_schema",
            // Annotation — JSON-based (6)
            "brat", "decomp", "ucca", "fovea", "bead", "web_annotation",
            // Annotation — XML-based (9)
            "naf", "uima", "folia", "tei", "timeml", "elan", "iso_space", "paula", "laf_graf",
            // Annotation — tab/line (2) + other (2)
            "conllu", "amr", "concrete", "nif",
            // Web/Document (10)
            "atproto", "jsx", "vue", "svelte", "css", "html", "markdown", "xml_xsd", "docx", "odf",
            // Domain (6)
            "geojson", "fhir", "rss_atom", "vcard_ical", "swift_mt", "edi_x12",
        ];

        for name in &expected {
            assert!(
                registry.native_repr(name).is_ok(),
                "protocol '{name}' should be registered in default_registry"
            );
        }

        assert_eq!(registry.len(), expected.len(),
            "registry should have exactly {expected_len} protocols, got {actual}",
            expected_len = expected.len(),
            actual = registry.len(),
        );
    }

    #[test]
    fn unknown_protocol_returns_error() {
        let registry = default_registry();
        assert!(registry.native_repr("nonexistent").is_err());
    }
}
