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
    match protocol {
        "atproto" => atproto::parse_lexicon_bundle(docs),
        other => Err(ProtocolError::Parse(format!(
            "no bundle parser registered for protocol {other:?}; supported: [\"atproto\"]"
        ))),
    }
}

/// The protocol names [`parse_schema_bundle`] accepts.
///
/// Lets a caller report or validate bundle support without hard-coding a
/// protocol name outside this crate.
#[must_use]
pub const fn bundle_parser_protocols() -> &'static [&'static str] {
    &["atproto"]
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
    match protocol.replace('_', "-").as_str() {
        // annotation
        "amr" => annotation::amr::parse_amr_schema(doc),
        "bead" => annotation::bead::parse_bead(doc),
        "brat" => annotation::brat::parse_brat(doc),
        "concrete" => annotation::concrete::parse_concrete_schema(doc),
        "decomp" => annotation::decomp::parse_decomp(doc),
        "elan" => annotation::elan::parse_elan(doc),
        "folia" => annotation::folia::parse_folia(doc),
        "fovea" => annotation::fovea::parse_fovea(doc),
        "iso-space" => annotation::iso_space::parse_iso_space(doc),
        "laf-graf" => annotation::laf_graf::parse_laf_graf(doc),
        "naf" => annotation::naf::parse_naf(doc),
        "nif" => annotation::nif::parse_nif_schema(doc),
        "paula" => annotation::paula::parse_paula_schema(doc),
        "tei" => annotation::tei::parse_tei(doc),
        "timeml" => annotation::timeml::parse_timeml(doc),
        "ucca" => annotation::ucca::parse_ucca(doc),
        "uima" | "uima-cas" => annotation::uima::parse_uima_schema(doc),
        "web-annotation" => annotation::web_annotation::parse_web_annotation_schema(doc),
        // api
        "asyncapi" => api::asyncapi::parse_asyncapi(doc),
        "jsonapi" => api::jsonapi::parse_jsonapi(doc),
        "openapi" => api::openapi::parse_openapi(doc),
        "raml" => api::raml::parse_raml_schema(doc),
        // config
        "ansible" => config::ansible::parse_ansible_schema(doc),
        "cloudformation" => config::cloudformation::parse_cfn_schema(doc),
        "k8s-crd" => config::k8s_crd::parse_k8s_crd_schema(doc),
        // data_schema
        "bson" => data_schema::bson::parse_bson_schema(doc),
        "json-schema" => data_schema::json_schema::parse_json_schema(doc),
        // data_science
        "arrow" => data_science::arrow::parse_arrow_schema(doc),
        "dataframe" => data_science::dataframe::parse_dataframe_schema(doc),
        "parquet" => data_science::parquet::parse_parquet_schema(doc),
        // database
        "dynamodb" => database::dynamodb::parse_dynamodb(doc),
        "mongodb" => database::mongodb::parse_mongodb_schema(doc),
        // domain
        "edi-x12" => domain::edi_x12::parse_edi_schema(doc),
        "fhir" => domain::fhir::parse_fhir_schema(doc),
        "geojson" => domain::geojson::parse_geojson_schema(doc),
        "rss-atom" => domain::rss_atom::parse_rss_atom_schema(doc),
        "swift-mt" => domain::swift_mt::parse_swift_mt_schema(doc),
        "vcard-ical" => domain::vcard_ical::parse_vcard_ical_schema(doc),
        // serialization
        "avro" => serialization::avro::parse_avsc(doc),
        "msgpack-schema" => serialization::msgpack_schema::parse_msgpack_schema(doc),
        // web_document
        "atproto" => web_document::atproto::parse_lexicon(doc),
        "docx" => web_document::docx::parse_docx_schema(doc),
        "odf" => web_document::odf::parse_odf_schema(doc),
        other => Err(ProtocolError::Parse(format!(
            "no document parser registered for protocol {other:?}; \
             a text-source schema (SQL DDL, GraphQL SDL, .proto, CDDL, and \
             the like) is loaded with parse_schema_source instead"
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
    match protocol.replace('_', "-").as_str() {
        "conllu" => annotation::conllu::parse_conllu(source),
        "cddl" => data_schema::cddl::parse_cddl(source),
        "cassandra" => database::cassandra::parse_cql(source),
        "neo4j" => database::neo4j::parse_cypher_schema(source),
        "redis" => database::redis::parse_redis_schema(source),
        "asn1" => serialization::asn1::parse_asn1(source),
        "bond" => serialization::bond::parse_bond(source),
        "flatbuffers" => serialization::flatbuffers::parse_fbs(source),
        "graphql" => api::graphql::parse_sdl(source),
        "sql" => database::sql::parse_ddl(source),
        "protobuf" => serialization::protobuf::parse_proto(source),
        other => Err(ProtocolError::Parse(format!(
            "no source parser registered for protocol {other:?}; \
             a JSON-document schema is loaded with parse_schema_document instead"
        ))),
    }
}

/// The protocol names [`parse_schema_document`] accepts (canonical,
/// hyphenated form).
#[must_use]
pub const fn document_parser_protocols() -> &'static [&'static str] {
    &[
        "amr",
        "bead",
        "brat",
        "concrete",
        "decomp",
        "elan",
        "folia",
        "fovea",
        "iso-space",
        "laf-graf",
        "naf",
        "nif",
        "paula",
        "tei",
        "timeml",
        "ucca",
        "uima-cas",
        "web-annotation",
        "asyncapi",
        "jsonapi",
        "openapi",
        "raml",
        "ansible",
        "cloudformation",
        "k8s-crd",
        "bson",
        "json-schema",
        "arrow",
        "dataframe",
        "parquet",
        "dynamodb",
        "mongodb",
        "edi-x12",
        "fhir",
        "geojson",
        "rss-atom",
        "swift-mt",
        "vcard-ical",
        "avro",
        "msgpack-schema",
        "atproto",
        "docx",
        "odf",
    ]
}

/// The protocol names [`parse_schema_source`] accepts (canonical,
/// hyphenated form).
#[must_use]
pub const fn source_parser_protocols() -> &'static [&'static str] {
    &[
        "conllu",
        "cddl",
        "cassandra",
        "neo4j",
        "redis",
        "asn1",
        "bond",
        "flatbuffers",
        "graphql",
        "sql",
        "protobuf",
    ]
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
