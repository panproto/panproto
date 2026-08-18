#![no_main]
//! Fuzz target for the schema-document parser feeding the span search.
//!
//! Arbitrary bytes become a JSON document or a source string, that goes
//! through the protocol's parser, and whatever comes out is searched against
//! itself. This is the path a schema file from outside takes, and nothing has
//! fed it adversarial input.
//!
//! The invariants:
//!
//! 1. Parsing either yields a schema or reports an error. It does not panic,
//!    and it does not overflow the stack on a deeply nested document.
//! 2. A parsed schema validates against the protocol whose parser produced it.
//! 3. The parse is deterministic: two parses of one document agree on the
//!    canonical digest.
//! 4. A schema always spans onto itself, and the self-span is total. The
//!    identity attains quality cost zero and drop count zero, which is the
//!    minimum of a lexicographic objective whose components are both
//!    non-negative, so every optimum drops nothing.
//!
//! Run with:
//!
//! ```text
//! cargo fuzz run parse_and_search -- -max_total_time=300
//! ```

use arbitrary::Unstructured;
use libfuzzer_sys::fuzz_target;
use panproto_mig::{SearchOptions, check_migration_morphism, find_span};
use panproto_protocols::{
    api, config, data_schema, data_science, database, parse_schema_document, parse_schema_source,
    serialization, web_document,
};
use panproto_schema::{Protocol, Schema, canonical_digest, validate};

/// The document protocols, with the parser and the protocol each names.
type DocEntry = (
    &'static str,
    fn(&serde_json::Value) -> Result<Schema, panproto_protocols::ProtocolError>,
    fn() -> Protocol,
);

/// The text-source protocols, likewise.
type SourceEntry = (
    &'static str,
    fn(&str) -> Result<Schema, panproto_protocols::ProtocolError>,
    fn() -> Protocol,
);

const DOCUMENT_PROTOCOLS: [DocEntry; 7] = [
    (
        "atproto",
        web_document::atproto::parse_lexicon,
        web_document::atproto::protocol,
    ),
    (
        "json-schema",
        data_schema::json_schema::parse_json_schema,
        data_schema::json_schema::protocol,
    ),
    ("openapi", api::openapi::parse_openapi, api::openapi::protocol),
    (
        "avro",
        serialization::avro::parse_avsc,
        serialization::avro::protocol,
    ),
    (
        "k8s-crd",
        config::k8s_crd::parse_k8s_crd_schema,
        config::k8s_crd::protocol,
    ),
    (
        "mongodb",
        database::mongodb::parse_mongodb_schema,
        database::mongodb::protocol,
    ),
    (
        "arrow",
        data_science::arrow::parse_arrow_schema,
        data_science::arrow::protocol,
    ),
];

const SOURCE_PROTOCOLS: [SourceEntry; 3] = [
    ("sql", database::sql::parse_ddl, database::sql::protocol),
    ("graphql", api::graphql::parse_sdl, api::graphql::protocol),
    (
        "protobuf",
        serialization::protobuf::parse_proto,
        serialization::protobuf::protocol,
    ),
];

fuzz_target!(|data: &[u8]| {
    let mut u = Unstructured::new(data);
    let text_path = u.arbitrary::<bool>().unwrap_or(false);
    let selector = usize::from(u.arbitrary::<u8>().unwrap_or(0));
    let rest = u.take_rest();

    let (name, schema, protocol) = if text_path {
        let (name, parse, protocol) = SOURCE_PROTOCOLS[selector % SOURCE_PROTOCOLS.len()];
        let Ok(source) = std::str::from_utf8(rest) else {
            return;
        };
        // 1. Parsing either answers or reports.
        let Ok(schema) = parse(source) else {
            return;
        };
        // 3. And it is deterministic.
        let again = parse(source).expect("a source that parsed once parses again");
        assert_eq!(
            canonical_digest(&schema),
            canonical_digest(&again),
            "{name} parsed one source to two different schemas"
        );
        // The dispatcher must agree with the parser it dispatches to.
        let dispatched =
            parse_schema_source(name, source).expect("the dispatcher reaches the same parser");
        assert_eq!(
            canonical_digest(&schema),
            canonical_digest(&dispatched),
            "{name}: parse_schema_source disagrees with the protocol's own parser"
        );
        (name, schema, protocol())
    } else {
        let (name, parse, protocol) = DOCUMENT_PROTOCOLS[selector % DOCUMENT_PROTOCOLS.len()];
        let Ok(doc) = serde_json::from_slice::<serde_json::Value>(rest) else {
            return;
        };
        let Ok(schema) = parse(&doc) else {
            return;
        };
        let again = parse(&doc).expect("a document that parsed once parses again");
        assert_eq!(
            canonical_digest(&schema),
            canonical_digest(&again),
            "{name} parsed one document to two different schemas"
        );
        let dispatched =
            parse_schema_document(name, &doc).expect("the dispatcher reaches the same parser");
        assert_eq!(
            canonical_digest(&schema),
            canonical_digest(&dispatched),
            "{name}: parse_schema_document disagrees with the protocol's own parser"
        );
        (name, schema, protocol())
    };

    // 2. A parsed schema validates against its own protocol.
    let errors = validate(&schema, &protocol);
    assert!(
        errors.is_empty(),
        "{name} produced a schema its own protocol rejects: {errors:?}"
    );

    // Keep the search inside the shapes a fuzzer can afford to run millions of
    // times; a wide parse is the benchmark's business, not this target's.
    if schema.vertices.is_empty() || schema.vertices.len() > 20 {
        return;
    }

    // 4. A schema always spans onto itself, totally.
    let span = find_span(&schema, &schema, &protocol, &SearchOptions::default())
        .expect("a schema always spans onto itself");
    check_migration_morphism(&span.apex, &schema, &span.left)
        .expect("the left leg must be a schema morphism");
    check_migration_morphism(&span.apex, &schema, &span.right)
        .expect("the right leg must be a schema morphism");
    assert!(
        span.is_total(),
        "{name}: the identity was available and was not taken; \
         the apex holds {} of {} vertices",
        span.apex.vertices.len(),
        schema.vertices.len()
    );
});
