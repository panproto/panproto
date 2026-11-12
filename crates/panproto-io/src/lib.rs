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
//! - [`json_pathway`]: Shared lossless JSON → `WInstance` builder
//! - [`error`]: Error types for parse and emit operations
//! - [`arena`]: Arena allocation helpers for zero-copy hot paths
//! - [`schema_documents`]: Schema documents read by protocol name, through
//!   the grammar readers of this crate or the parsers of `panproto-protocols`
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
//! - SIMD byte scanning via `memchr` for delimited formats
//! - Arena allocation via `bumpalo` for hot paths
//! - Zero-copy parsing where format permits

// Allow concrete HashMap in public API per workspace conventions.
#![allow(clippy::implicit_hasher)]

pub mod error;

pub mod traits;

pub mod registry;

pub mod resolution;

pub mod json_pathway;

pub mod arena;

#[allow(deprecated)]
pub mod json_codec;

pub mod toml_pathway;

pub mod xml_pathway;

#[allow(deprecated)]
pub mod xml_codec;

pub mod tabular_pathway;

#[allow(deprecated)]
pub mod tabular_codec;

pub mod byte_tabular;

pub mod cst_extract;

pub mod rdf;

pub mod edgelist;

pub mod schema_documents;

#[cfg(feature = "tree-sitter")]
pub mod unified_codec;

#[cfg(feature = "tree-sitter")]
pub mod postgresql;

// ── Protocol category modules ──────────────────────────────────────────

pub mod annotation;
pub mod api;
pub mod config;
pub mod data_schema;
pub mod data_science;
pub mod database;
pub mod domain;
pub mod serialization;
#[cfg(feature = "tree-sitter")]
pub mod text_format;
pub mod web_document;

// Re-exports for convenience.
pub use error::{EmitInstanceError, ParseInstanceError};
pub use registry::{PresentationComplement, ProtocolRegistry, ResolvedState};
pub use resolution::{
    AnyInstance, AssemblyDomainReason, AssemblyPlanId, AssemblyResidue, AssemblyTrace,
    DefinitionKey, ExternalTextStore, OffsetUnitReason, PathResolution, RedirectedArc,
    RedirectedAttachment, RedirectedFragment, RefOccurrence, Resolution, ResolutionComplement,
    ResolutionContextKind, ResolutionFeature, ResolutionInstanceError, ResolutionKind,
    ResolutionLawReport, ResolutionLawViolation, ResolutionProvider, ResolutionProviderId,
    ResolutionSpan, ResolutionSpec, ResolutionTextId, ResolutionValueSite, ResolveContext,
    ResolveDomainError, ResolveError, ResolveOptions, TokenBoundaryProvider, UnresolvedReason,
    UnresolvedRef, check_resolution_laws,
};
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
    #[cfg(feature = "tree-sitter")]
    rdf::register_all(&mut registry);
    #[cfg(feature = "tree-sitter")]
    edgelist::register_all(&mut registry);
    web_document::register_all(&mut registry);
    registry
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_registry_has_expected_protocols() {
        let registry = default_registry();

        // 50 protocols in the default build, plus, under the tree-sitter
        // feature, the four generic text formats, the PostgreSQL table codec
        // the N-Triples and Turtle graph codecs and the two edge-list graph
        // codecs.
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

        // The generic text-format codecs are format-preserving, the
        // PostgreSQL codec reads through the PostgreSQL grammar and the
        // N-Triples and Turtle codecs through the Turtle grammar and the JSON-LD
        // codec through the JSON grammar, so they are
        // only registered under the tree-sitter feature.
        #[cfg(feature = "tree-sitter")]
        let extra: &[&str] = &[
            "yaml",
            "toml",
            "csv",
            "tsv",
            "postgresql",
            "ntriples",
            "turtle",
            "jsonld",
            "edgelist-csv",
            "edgelist-tsv",
        ];
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
