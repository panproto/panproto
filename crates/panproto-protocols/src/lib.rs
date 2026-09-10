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

/// The canonical record of what each protocol supports.
pub mod registry;
/// Serialization and IDL protocol definitions.
pub mod serialization;
/// Shared component theory definitions (building-block GATs).
pub mod theories;
/// Web and document format protocol definitions.
pub mod web_document;

use panproto_schema::Schema;

pub use error::ProtocolError;

// Re-export existing protocols at crate root for backward compatibility.
pub use web_document::atproto;

/// Parse a bundle of schema documents into one [`Schema`], resolving
/// cross-document references across the whole bundle.
///
/// A single-document parser sees one document at a time, so a reference
/// into another document resolves to an opaque placeholder vertex
/// carrying no fields, and a lens has nothing typed to bind to. Passing
/// the referenced documents alongside the referring one resolves each
/// such reference to the definition's real, typed vertex. A reference
/// whose target is in no document of the bundle stays a placeholder,
/// which is what marks it as genuinely external.
///
/// This is the protocol-dispatching entry point the generic crates call,
/// so that protocol names stay inside this crate. A protocol gains
/// bundle support by adding an arm here; no binding surface changes.
///
/// # Errors
///
/// Returns [`ProtocolError::Parse`] if no bundle parser is registered
/// for `protocol`, or the protocol's own error if the documents are not
/// a well-formed bundle for it.
pub fn parse_schema_bundle(
    protocol: &str,
    docs: &[serde_json::Value],
) -> Result<Schema, ProtocolError> {
    match protocol.replace('_', "-").as_str() {
        "atproto" => atproto::parse_lexicon_bundle(docs),
        "openapi" => api::openapi::parse_openapi_bundle(docs),
        "json-schema" => data_schema::json_schema::parse_json_schema_bundle(docs),
        "avro" => serialization::avro::parse_avsc_bundle(docs),
        other => Err(ProtocolError::Parse(format!(
            "no bundle parser registered for protocol {other:?}; supported: {:?}",
            bundle_parser_protocols()
        ))),
    }
}

/// The protocol names [`parse_schema_bundle`] accepts.
///
/// Lets a caller report or validate bundle support without hard-coding a
/// protocol name outside this crate.
#[must_use]
pub fn bundle_parser_protocols() -> Vec<&'static str> {
    registry::names_where(|d| d.bundle)
}

/// Parse a set of schema documents into per-file schemas, keyed by path.
///
/// The result also carries the edges that cross document boundaries: the
/// shape [`build_project_tree`](https://docs.rs/panproto-project)
/// consumes to store a document set as the per-file tree the VCS diffs
/// incrementally.
///
/// Where [`parse_schema_bundle`] fuses a document set into one flat
/// [`Schema`], this keeps each document a separate schema, so a
/// version-controlled lexicon set can reuse unchanged per-file object
/// ids across commits. Dispatch normalizes an underscore key to its
/// canonical hyphenated protocol name, matching [`parse_schema_bundle`].
/// Only the protocols in [`bundle_project_protocols`] retain per-file
/// provenance today; any other returns an error.
///
/// # Errors
///
/// Returns [`ProtocolError::Parse`] for a protocol with no per-file
/// bundle parser, or the underlying parser's error.
pub fn parse_schema_bundle_project(
    protocol: &str,
    docs: &[(std::path::PathBuf, serde_json::Value)],
) -> Result<atproto::LexiconProject, ProtocolError> {
    match protocol.replace('_', "-").as_str() {
        "atproto" => {
            let lexicon_docs: Vec<atproto::LexiconDoc> = docs
                .iter()
                .map(|(path, value)| atproto::LexiconDoc {
                    path: path.clone(),
                    value: value.clone(),
                })
                .collect();
            atproto::parse_lexicon_project(&lexicon_docs)
        }
        other => Err(ProtocolError::Parse(format!(
            "no per-file bundle parser registered for protocol {other:?}; supported: [\"atproto\"]"
        ))),
    }
}

/// Protocols whose bundle parse retains per-file provenance for the VCS
/// (via [`parse_schema_bundle_project`]).
#[must_use]
pub fn bundle_project_protocols() -> Vec<&'static str> {
    registry::names_where(|d| d.bundle_project)
}

/// Parse a single JSON schema *document* into a [`Schema`], dispatching
/// on protocol name.
///
/// This is the generic entry point that exposes every JSON-document
/// schema parser through one call, so a binding forwards a protocol
/// string here rather than reaching each protocol's parser directly.
/// Protocols whose source is text rather than JSON (SQL DDL, GraphQL
/// SDL, `.proto`, CDDL, CQL, Cypher, `ASN.1`, Bond, `FlatBuffers`, `CoNLL-U`)
/// are served by [`parse_schema_source`] instead.
///
/// The `protocol` argument is matched against each protocol's canonical
/// [`Protocol::name`](panproto_schema::Protocol) (hyphenated). An
/// underscore is normalized to a hyphen first, so the underscore
/// registry keys that [`crate`] callers list (`iso_space`,
/// `msgpack_schema`, …) resolve too; `uima` is accepted as an alias of
/// its canonical `uima-cas`.
///
/// # Errors
///
/// Returns [`ProtocolError::Parse`] if no JSON-document parser is
/// registered for `protocol` (a text-source protocol, or an unknown
/// name), or the protocol's own error if the document is malformed.
pub fn parse_schema_document(
    protocol: &str,
    doc: &serde_json::Value,
) -> Result<Schema, ProtocolError> {
    match registry::descriptor(protocol) {
        Some(d) => match d.parser {
            registry::Parser::Document(parse) => parse(doc),
            registry::Parser::Source(_) => Err(ProtocolError::Parse(format!(
                "protocol {protocol:?} is read from source text, not a JSON document; \
                 use parse_schema_source"
            ))),
        },
        None => Err(ProtocolError::Parse(format!(
            "no document parser registered for protocol {protocol:?}; supported: {:?}",
            document_parser_protocols()
        ))),
    }
}

/// Parse a *text/source* schema (an IDL or DDL string) into a
/// [`Schema`], dispatching on protocol name.
///
/// The text counterpart to [`parse_schema_document`], for the protocols
/// whose source is a language rather than a JSON document: SQL DDL,
/// GraphQL SDL, Protocol Buffers `.proto`, CDDL, Cassandra CQL, Cypher,
/// `ASN.1`, Microsoft Bond, `FlatBuffers` `.fbs`, and `CoNLL-U`. Name matching
/// is the same normalization as [`parse_schema_document`].
///
/// # Errors
///
/// Returns [`ProtocolError::Parse`] if no text-source parser is
/// registered for `protocol`, or the protocol's own error if the source
/// is malformed.
pub fn parse_schema_source(protocol: &str, source: &str) -> Result<Schema, ProtocolError> {
    match registry::descriptor(protocol) {
        Some(d) => match d.parser {
            registry::Parser::Source(parse) => parse(source),
            registry::Parser::Document(_) => Err(ProtocolError::Parse(format!(
                "protocol {protocol:?} is read from a JSON document, not source text; \
                 use parse_schema_document"
            ))),
        },
        None => Err(ProtocolError::Parse(format!(
            "no source parser registered for protocol {protocol:?}; supported: {:?}",
            source_parser_protocols()
        ))),
    }
}

/// The protocol names [`parse_schema_document`] accepts (canonical,
/// hyphenated form).
#[must_use]
pub fn document_parser_protocols() -> Vec<&'static str> {
    registry::names_where(|d| matches!(d.parser, registry::Parser::Document(_)))
}

/// The protocol names [`parse_schema_source`] accepts (canonical,
/// hyphenated form).
#[must_use]
pub fn source_parser_protocols() -> Vec<&'static str> {
    registry::names_where(|d| matches!(d.parser, registry::Parser::Source(_)))
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod dispatch_tests {
    use super::*;

    #[test]
    fn document_dispatch_routes_json_schema() {
        let doc = serde_json::json!({
            "type": "object",
            "properties": { "name": { "type": "string" }, "age": { "type": "integer" } }
        });
        let schema = parse_schema_document("json-schema", &doc).expect("json-schema should parse");
        assert!(schema.has_vertex("root"));
        assert!(schema.has_vertex("root.name"));
        assert!(schema.has_vertex("root.age"));
    }

    #[test]
    fn document_dispatch_normalizes_underscore_to_hyphen() {
        // The underscore registry-key spelling resolves to the same
        // canonical hyphenated parser.
        let doc = serde_json::json!({ "type": "object" });
        let via_hyphen = parse_schema_document("json-schema", &doc).expect("hyphen form");
        let via_underscore = parse_schema_document("json_schema", &doc).expect("underscore form");
        assert_eq!(via_hyphen.vertex_count(), via_underscore.vertex_count());
    }

    #[test]
    fn source_dispatch_routes_graphql_sql_protobuf() {
        let g = parse_schema_source("graphql", "type Query { hello: String }")
            .expect("graphql sdl should parse");
        assert!(g.has_vertex("Query"));

        let s = parse_schema_source("sql", "CREATE TABLE users (id INTEGER PRIMARY KEY);")
            .expect("sql ddl should parse");
        assert!(s.has_vertex("users"));

        let p = parse_schema_source("protobuf", "message User { string name = 1; }")
            .expect("proto should parse");
        assert!(p.has_vertex("User"));
    }

    #[test]
    fn uima_is_accepted_under_both_names() {
        // The `uima` registry key aliases the canonical `uima-cas`; both
        // route to the parser rather than the unknown-protocol arm.
        let doc = serde_json::json!({});
        // A malformed doc may error, but never with the "no parser" message.
        for name in ["uima", "uima-cas"] {
            if let Err(ProtocolError::Parse(msg)) = parse_schema_document(name, &doc) {
                assert!(
                    !msg.contains("no document parser"),
                    "{name} must route to the uima parser, got: {msg}"
                );
            }
        }
    }

    #[test]
    fn cross_category_calls_point_at_the_other_dispatch() {
        // A text-source protocol passed to the document dispatch is told
        // to use the source dispatch, and vice versa.
        let doc = serde_json::json!({});
        let err = parse_schema_document("sql", &doc).expect_err("sql is text-source");
        assert!(err.to_string().contains("parse_schema_source"));

        let err = parse_schema_source("json-schema", "{}").expect_err("json-schema is a document");
        assert!(err.to_string().contains("parse_schema_document"));
    }

    #[test]
    fn parser_protocol_lists_have_expected_sizes() {
        assert_eq!(document_parser_protocols().len(), 43);
        assert_eq!(source_parser_protocols().len(), 11);
        assert!(document_parser_protocols().contains(&"json-schema"));
        assert!(source_parser_protocols().contains(&"graphql"));
        assert!(source_parser_protocols().contains(&"sql"));
        assert!(source_parser_protocols().contains(&"protobuf"));
    }
}
