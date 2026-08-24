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
//! provides the **instance presentations**: the functors mapping between
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
//! ## Format-preservation limits
//!
//! Lossless round-trips (`emit(parse(bytes)) == bytes`) are available only for
//! the **text** formats whose grammar the tree-sitter pathway captures as a
//! concrete syntax tree: JSON, XML, YAML, TOML, CSV, and TSV, and only when the
//! `tree-sitter` feature is enabled. The CST records the incidental layout
//! (whitespace, key order, quoting, comments) that the abstract instance model
//! discards, so it can be replayed on emit.
//!
//! **Binary** formats have no such CST pathway. `MessagePack`, Avro, Parquet,
//! protobuf wire, and the other binary codecs parse to and emit from the
//! abstract instance model directly, so their round-trip is preservation *up to
//! the model*: the decoded values are faithful, but byte-level encoder choices
//! (field ordering, integer width, optional-field presence, framing) are the
//! emitter's, not the source's. Callers needing byte-identical binary output
//! must retain the original bytes; panproto-io does not reconstruct them.
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

/// Generic JSON-based codec reused by many protocols.
///
/// Deprecated: enable the `tree-sitter` feature and use `UnifiedCodec` instead.
#[allow(deprecated)]
pub mod json_codec;

/// Canonical TOML emission for instances with no layout complement.
pub mod toml_pathway;

/// Zero-copy XML pathway for schema-guided instance parsing via `quick-xml`.
pub mod xml_pathway;

/// Generic XML-based codec reused by several protocols.
///
/// Deprecated: enable the `tree-sitter` feature and use `UnifiedCodec` instead.
#[allow(deprecated)]
pub mod xml_codec;

/// Shared tabular pathway for line/field-delimited formats via `memchr`.
pub mod tabular_pathway;

/// Generic tabular codec for delimited text protocols.
///
/// Deprecated: enable the `tree-sitter` feature and use `UnifiedCodec` instead.
#[allow(deprecated)]
pub mod tabular_codec;

/// Byte-faithful codec for delimited line-oriented formats with no tree-sitter
/// grammar (redis/RESP, SWIFT MT, EDI X12, CoNLL-U).
pub mod byte_tabular;

/// CST-to-Instance extraction lens for format-preserving round-trips.
pub mod cst_extract;

/// Unified tree-sitter-based codec for all protocols.
#[cfg(feature = "tree-sitter")]
pub mod unified_codec;

// ── Protocol category modules ──────────────────────────────────────────

/// Linguistic annotation protocols (brat, CoNLL-U, NAF, etc.).
pub mod annotation;
/// API specification protocols (OpenAPI, AsyncAPI, JSON:API, RAML).
pub mod api;
/// Configuration protocols (CloudFormation, Ansible, K8s CRD).
pub mod config;
/// Data schema protocols (CDDL, BSON).
pub mod data_schema;
/// Data science protocols (DataFrame, Parquet, Arrow).
pub mod data_science;
/// Database protocols (MongoDB, DynamoDB, Cassandra, Neo4j, Redis).
pub mod database;
/// Domain-specific protocols (GeoJSON, FHIR, RSS/Atom, vCard/iCal, etc.).
pub mod domain;
/// Serialization protocols (Avro, FlatBuffers, ASN.1, Bond, MsgPack).
pub mod serialization;
/// Generic text serialization formats (YAML, TOML, CSV) with
/// format-preserving codecs; requires the `tree-sitter` feature.
#[cfg(feature = "tree-sitter")]
pub mod text_format;
/// Web and document protocols (ATProto, DOCX, ODF).
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
/// ```text
/// let registry = panproto_io::default_registry();
/// let instance = registry.parse_wtype("openapi", &schema, &bytes)?;
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
    #[cfg(feature = "tree-sitter")]
    text_format::register_all(&mut registry);
    web_document::register_all(&mut registry);
    registry
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_registry_has_expected_protocols() {
        let registry = default_registry();

        // 50 protocols in the default build, plus the three generic
        // text-format protocols (yaml, toml, csv) when the tree-sitter
        // feature enables their format-preserving codecs.
        let base: &[&str] = &[
            // API (4)
            "openapi",
            "asyncapi",
            "jsonapi",
            "raml",
            // Data schema (2)
            "cddl",
            "bson",
            // Database (5)
            "mongodb",
            "dynamodb",
            "cassandra",
            "neo4j",
            "redis",
            // Config (3)
            "cloudformation",
            "ansible",
            "k8s_crd",
            // Data science (3)
            "dataframe",
            "parquet",
            "arrow",
            // Serialization (5)
            "avro",
            "flatbuffers",
            "asn1",
            "bond",
            "msgpack_schema",
            // Annotation: JSON-based (6)
            "brat",
            "decomp",
            "ucca",
            "fovea",
            "bead",
            "web_annotation",
            // Annotation: XML-based (9)
            "naf",
            "uima",
            "folia",
            "tei",
            "timeml",
            "elan",
            "iso_space",
            "paula",
            "laf_graf",
            // Annotation: tab/line (2) + other (2)
            "conllu",
            "amr",
            "concrete",
            "nif",
            // Web/Document (3)
            "atproto",
            "docx",
            "odf",
            // Domain (6)
            "geojson",
            "fhir",
            "rss_atom",
            "vcard_ical",
            "swift_mt",
            "edi_x12",
        ];

        // The generic text-format codecs are format-preserving and thus
        // only registered under the tree-sitter feature.
        #[cfg(feature = "tree-sitter")]
        let extra: &[&str] = &["yaml", "toml", "csv"];
        #[cfg(not(feature = "tree-sitter"))]
        let extra: &[&str] = &[];

        for name in base.iter().chain(extra) {
            assert!(
                registry.native_repr(name).is_ok(),
                "protocol '{name}' should be registered in default_registry"
            );
        }

        let expected_len = base.len() + extra.len();
        assert_eq!(
            registry.len(),
            expected_len,
            "registry should have exactly {expected_len} protocols, got {actual}",
            actual = registry.len(),
        );
    }

    #[test]
    fn unknown_protocol_returns_error() {
        let registry = default_registry();
        assert!(registry.native_repr("nonexistent").is_err());
    }
}
