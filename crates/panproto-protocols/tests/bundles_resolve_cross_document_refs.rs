//! A reference into a sibling document resolves to that document's own
//! definition.
//!
//! `parse_schema_bundle` resolved cross-document references for `ATProto`
//! alone, while several other protocols had the same latent gap: their
//! parsers take one document, so a reference into a sibling resolved to
//! an opaque placeholder vertex carrying no fields, and a lens or
//! migration had nothing typed to bind to.
//!
//! Each protocol below is tested the same four ways, because a bundle
//! parse can appear to succeed while resolving nothing:
//!
//! 1. a single-document parse leaves the cross-document target opaque,
//!    which is the behaviour being improved on and is worth pinning so
//!    the contrast stays honest;
//! 2. the bundle resolves the reference to the definition's *own
//!    fields*, not merely to something of the right kind, since a
//!    placeholder has a kind too;
//! 3. a reference to a document outside the bundle still yields a
//!    placeholder, which is what marks it genuinely external;
//! 4. a one-document bundle agrees with the single-document parser, so
//!    the two cannot drift apart.

#![allow(clippy::expect_used)]

use panproto_protocols::{bundle_parser_protocols, parse_schema_bundle};

/// Every protocol the dispatch advertises is one it actually accepts.
#[test]
fn every_advertised_protocol_has_a_bundle_parser() {
    for protocol in bundle_parser_protocols() {
        let result = parse_schema_bundle(protocol, &[serde_json::json!({})]);
        assert!(
            !matches!(&result, Err(e) if e.to_string().contains("no bundle parser")),
            "{protocol} is advertised but has no parser: {result:?}",
        );
    }
}

#[test]
fn an_unregistered_protocol_names_the_ones_that_are() {
    let err = parse_schema_bundle("nonexistent", &[serde_json::json!({})])
        .expect_err("an unknown protocol must be refused");
    let message = err.to_string();
    assert!(message.contains("no bundle parser"), "got: {message}");
    for protocol in bundle_parser_protocols() {
        assert!(
            message.contains(protocol),
            "the error must name {protocol}, got: {message}",
        );
    }
}

// ---------------------------------------------------------------------------
// Avro: a named type referenced by its fullname from another document
// ---------------------------------------------------------------------------

mod avro {
    use super::*;

    /// `Address` lives in one document; `User` references it by
    /// fullname from another.
    fn docs() -> Vec<serde_json::Value> {
        vec![
            serde_json::json!({
                "type": "record",
                "name": "Address",
                "namespace": "com.example",
                "fields": [
                    {"name": "street", "type": "string"},
                    {"name": "city", "type": "string"}
                ]
            }),
            serde_json::json!({
                "type": "record",
                "name": "User",
                "namespace": "com.example",
                "fields": [
                    {"name": "name", "type": "string"},
                    {"name": "home", "type": "com.example.Address"}
                ]
            }),
        ]
    }

    #[test]
    fn a_lone_document_cannot_resolve_a_sibling_fullname() {
        let schema =
            parse_schema_bundle("avro", &docs()[1..]).expect("the referring document parses");
        assert!(
            !schema.vertices.contains_key("Address"),
            "a lone document cannot type a fullname it does not define",
        );
        assert!(
            !schema
                .edges
                .keys()
                .any(|e| &*e.kind == "type-of" && e.tgt.contains("Address")),
            "and cannot bind a type-of edge to it",
        );
    }

    #[test]
    fn a_bundle_binds_the_fullname_to_the_defining_record() {
        let schema = parse_schema_bundle("avro", &docs()).expect("the bundle parses");

        // The referenced record's own fields are present. A placeholder
        // would have a kind but none of these.
        for field in ["Address.street", "Address.city"] {
            assert!(
                schema.vertices.contains_key(field),
                "expected the referenced record's field {field}, got: {:?}",
                sorted_keys(&schema),
            );
        }

        assert!(
            schema
                .edges
                .keys()
                .any(|e| &*e.kind == "type-of" && &*e.src == "User.home" && &*e.tgt == "Address"),
            "the field must bind to the sibling document's record, got: {:?}",
            schema.edges.keys().collect::<Vec<_>>(),
        );
    }

    #[test]
    fn a_fullname_outside_the_bundle_stays_unresolved() {
        let orphan = serde_json::json!({
            "type": "record",
            "name": "Order",
            "namespace": "com.example",
            "fields": [{"name": "ship_to", "type": "com.elsewhere.Address"}]
        });
        let schema = parse_schema_bundle("avro", &[orphan]).expect("the document parses");
        assert!(
            !schema
                .edges
                .keys()
                .any(|e| &*e.kind == "type-of" && e.tgt.contains("elsewhere")),
            "a fullname in no document of the bundle must not bind",
        );
    }

    /// Pulling a sibling's definitions in must not promote them to
    /// entry sorts: `Address` is referenced, so it is not a basepoint.
    #[test]
    fn entry_selection_ranges_over_the_whole_bundle() {
        let schema = parse_schema_bundle("avro", &docs()).expect("the bundle parses");
        let entries: Vec<&str> = schema.entry_vertices().iter().map(AsRef::as_ref).collect();
        assert!(
            entries.contains(&"User"),
            "the unreferenced record is an entry, got: {entries:?}",
        );
        assert!(
            !entries.contains(&"Address"),
            "a record some field references is not an entry, got: {entries:?}",
        );
    }

    #[test]
    fn a_one_document_bundle_agrees_with_the_single_document_parser() {
        let one = &docs()[..1];
        let bundled = parse_schema_bundle("avro", one).expect("the bundle parses");
        let single =
            panproto_protocols::serialization::avro::parse_avsc(&one[0]).expect("the doc parses");
        assert_eq!(sorted_keys(&bundled), sorted_keys(&single));
        assert_eq!(bundled.entry_vertices(), single.entry_vertices());
    }
}

// ---------------------------------------------------------------------------
// JSON Schema: a `$ref` against a sibling document's `$id`
// ---------------------------------------------------------------------------

mod json_schema {
    use super::*;

    fn docs() -> Vec<serde_json::Value> {
        vec![
            serde_json::json!({
                "$id": "https://example.com/common.json",
                "$defs": {
                    "Address": {
                        "type": "object",
                        "properties": {
                            "street": {"type": "string"},
                            "postcode": {"type": "string"}
                        }
                    }
                }
            }),
            serde_json::json!({
                "$id": "https://example.com/user.json",
                "type": "object",
                "properties": {
                    "name": {"type": "string"},
                    "home": {"$ref": "https://example.com/common.json#/$defs/Address"}
                }
            }),
        ]
    }

    #[test]
    fn a_lone_document_leaves_the_cross_document_ref_opaque() {
        let schema = parse_schema_bundle("json-schema", &docs()[1..])
            .expect("the referring document parses");
        assert!(
            !schema
                .vertices
                .keys()
                .any(|k| k.contains("common.json:$defs/Address")),
            "a lone document cannot type a ref into a sibling, got: {:?}",
            sorted_keys(&schema),
        );
    }

    #[test]
    fn a_bundle_resolves_the_ref_to_the_definition_and_its_fields() {
        let schema = parse_schema_bundle("json-schema", &docs()).expect("the bundle parses");

        let address = "https://example.com/common.json:$defs/Address";
        assert!(
            schema.vertices.contains_key(address),
            "expected the referenced definition, got: {:?}",
            sorted_keys(&schema),
        );

        // Its own properties, which the placeholder never carried.
        let props: Vec<&str> = schema
            .edges
            .keys()
            .filter(|e| &*e.src == address && &*e.kind == "prop")
            .filter_map(|e| e.name.as_deref())
            .collect();
        assert!(
            props.contains(&"street") && props.contains(&"postcode"),
            "expected the definition's own properties, got: {props:?}",
        );

        // The ref edge lands on it rather than on a placeholder.
        assert!(
            schema
                .edges
                .keys()
                .any(|e| &*e.kind == "ref" && &*e.tgt == address),
            "the ref must bind to the sibling's definition, got: {:?}",
            schema
                .edges
                .keys()
                .filter(|e| &*e.kind == "ref")
                .collect::<Vec<_>>(),
        );
    }

    #[test]
    fn a_ref_outside_the_bundle_stays_a_placeholder() {
        let orphan = serde_json::json!({
            "$id": "https://example.com/user.json",
            "type": "object",
            "properties": {
                "home": {"$ref": "https://elsewhere.example/common.json#/$defs/Address"}
            }
        });
        let schema = parse_schema_bundle("json-schema", &[orphan]).expect("the document parses");
        assert!(
            schema
                .edges
                .keys()
                .any(|e| &*e.kind == "ref" && e.tgt.contains(":ref")),
            "a ref to a document outside the bundle stays a placeholder, got: {:?}",
            schema
                .edges
                .keys()
                .filter(|e| &*e.kind == "ref")
                .collect::<Vec<_>>(),
        );
    }

    #[test]
    fn a_one_document_bundle_agrees_with_the_single_document_parser() {
        let one = &docs()[..1];
        let bundled = parse_schema_bundle("json-schema", one).expect("the bundle parses");
        let single = panproto_protocols::data_schema::json_schema::parse_json_schema(&one[0])
            .expect("the doc parses");
        assert_eq!(sorted_keys(&bundled), sorted_keys(&single));
    }
}

// ---------------------------------------------------------------------------
// OpenAPI: a `$ref` against a sibling document's `$id`
// ---------------------------------------------------------------------------

mod openapi {
    use super::*;

    fn docs() -> Vec<serde_json::Value> {
        vec![
            serde_json::json!({
                "$id": "https://example.com/common.json",
                "openapi": "3.1.0",
                "components": {"schemas": {
                    "Address": {
                        "type": "object",
                        "properties": {
                            "street": {"type": "string"},
                            "postcode": {"type": "string"}
                        }
                    }
                }}
            }),
            serde_json::json!({
                "$id": "https://example.com/api.json",
                "openapi": "3.1.0",
                "paths": {"/users": {"get": {"responses": {"200": {"content": {
                    "application/json": {"schema": {
                        "$ref": "https://example.com/common.json#/components/schemas/Address"
                    }}
                }}}}}}
            }),
        ]
    }

    #[test]
    fn a_lone_document_leaves_the_cross_document_ref_opaque() {
        let schema =
            parse_schema_bundle("openapi", &docs()[1..]).expect("the referring document parses");
        assert!(
            !schema
                .vertices
                .keys()
                .any(|k| k.contains("common.json::components/schemas/Address")),
            "a lone document cannot type a ref into a sibling, got: {:?}",
            sorted_keys(&schema),
        );
    }

    #[test]
    fn a_bundle_resolves_the_ref_to_the_component_and_its_fields() {
        let schema = parse_schema_bundle("openapi", &docs()).expect("the bundle parses");

        let address = "https://example.com/common.json::components/schemas/Address";
        assert!(
            schema.vertices.contains_key(address),
            "expected the referenced component, got: {:?}",
            sorted_keys(&schema),
        );

        let props: Vec<&str> = schema
            .edges
            .keys()
            .filter(|e| &*e.src == address && &*e.kind == "prop")
            .filter_map(|e| e.name.as_deref())
            .collect();
        assert!(
            props.contains(&"street") && props.contains(&"postcode"),
            "expected the component's own properties, got: {props:?}",
        );

        assert!(
            schema
                .edges
                .keys()
                .any(|e| &*e.kind == "ref" && &*e.tgt == address),
            "the ref must bind to the sibling's component",
        );
    }

    #[test]
    fn a_one_document_bundle_agrees_with_the_single_document_parser() {
        let one = &docs()[..1];
        let bundled = parse_schema_bundle("openapi", one).expect("the bundle parses");
        let single =
            panproto_protocols::api::openapi::parse_openapi(&one[0]).expect("the doc parses");
        assert_eq!(sorted_keys(&bundled), sorted_keys(&single));
    }

    #[test]
    fn two_documents_declaring_one_id_are_refused() {
        let dupes = vec![docs()[0].clone(), docs()[0].clone()];
        let err = parse_schema_bundle("openapi", &dupes)
            .expect_err("two documents claiming one identity must be refused");
        assert!(err.to_string().contains("duplicate $id"), "got: {err}");
    }
}

/// Vertex ids in a stable order, for comparing two parses.
fn sorted_keys(schema: &panproto_schema::Schema) -> Vec<String> {
    let mut keys: Vec<String> = schema.vertices.keys().map(ToString::to_string).collect();
    keys.sort();
    keys
}
