//! One record of what each protocol supports, and everything derives
//! from it.
//!
//! Capabilities used to be described independently in the parser
//! dispatches, the CLI, the C API, the WASM API and the book, and those
//! registries had already diverged. What follows are the properties
//! that keep them from diverging again: a name a caller can read back
//! is a name the dispatch accepts, an alias resolves, and the listings
//! are exactly the descriptors that claim the capability.

#![allow(clippy::expect_used)]

use panproto_protocols::registry::{Parser, descriptor, descriptors, protocol_names};
use panproto_protocols::{
    bundle_parser_protocols, bundle_project_protocols, document_parser_protocols,
    parse_schema_document, parse_schema_source, source_parser_protocols,
};

#[test]
fn canonical_names_are_unique() {
    let mut names: Vec<&str> = protocol_names();
    let before = names.len();
    names.sort_unstable();
    names.dedup();
    assert_eq!(
        names.len(),
        before,
        "two descriptors share a canonical name"
    );
}

/// An alias must not collide with another protocol's canonical name, or
/// a lookup would silently resolve to the wrong protocol.
#[test]
fn aliases_are_unique_and_do_not_shadow_a_canonical_name() {
    let canonical: Vec<&str> = protocol_names();
    let mut seen: Vec<&str> = Vec::new();
    for d in descriptors() {
        for alias in d.aliases {
            assert!(
                !canonical.contains(alias),
                "alias {alias:?} of {} is another protocol's canonical name",
                d.name,
            );
            assert!(!seen.contains(alias), "alias {alias:?} is claimed twice");
            seen.push(alias);
        }
    }
}

/// Every name a listing reports resolves. The C list used to advertise
/// nine protocols under underscore spellings that are not canonical
/// names, so a caller reading the list back got a name no listing
/// mentions.
#[test]
fn every_listed_name_resolves_to_its_own_descriptor() {
    for name in protocol_names() {
        let d = descriptor(name).expect("a listed name resolves");
        assert_eq!(d.name, name);
    }
}

/// Underscores and hyphens are the same separator to the dispatch, so
/// they must be the same to the lookup too.
#[test]
fn an_underscore_spelling_resolves_to_the_hyphenated_descriptor() {
    for name in protocol_names() {
        if !name.contains('-') {
            continue;
        }
        let underscored = name.replace('-', "_");
        let d = descriptor(&underscored)
            .unwrap_or_else(|| panic!("{underscored:?} must resolve to {name:?}"));
        assert_eq!(d.name, name);
    }
}

#[test]
fn every_alias_resolves() {
    for d in descriptors() {
        for alias in d.aliases {
            assert_eq!(
                descriptor(alias).expect("an alias resolves").name,
                d.name,
                "alias {alias:?} must resolve to {}",
                d.name,
            );
        }
    }
}

// ---------------------------------------------------------------------------
// The listings are exactly the descriptors claiming the capability
// ---------------------------------------------------------------------------

#[test]
fn the_document_listing_is_exactly_the_document_parsers() {
    let listed = document_parser_protocols();
    let expected: Vec<&str> = descriptors()
        .iter()
        .filter(|d| matches!(d.parser, Parser::Document(_)))
        .map(|d| d.name)
        .collect();
    assert_eq!(listed, expected);
    assert!(!listed.is_empty());
}

#[test]
fn the_source_listing_is_exactly_the_source_parsers() {
    let listed = source_parser_protocols();
    let expected: Vec<&str> = descriptors()
        .iter()
        .filter(|d| matches!(d.parser, Parser::Source(_)))
        .map(|d| d.name)
        .collect();
    assert_eq!(listed, expected);
    assert!(!listed.is_empty());
}

#[test]
fn the_bundle_listings_are_exactly_the_bundle_parsers() {
    assert_eq!(
        bundle_parser_protocols(),
        descriptors()
            .iter()
            .filter(|d| d.bundle)
            .map(|d| d.name)
            .collect::<Vec<_>>(),
    );
    assert_eq!(
        bundle_project_protocols(),
        descriptors()
            .iter()
            .filter(|d| d.bundle_project)
            .map(|d| d.name)
            .collect::<Vec<_>>(),
    );
}

/// A protocol that resolves cross-document references must be readable
/// one document at a time as well, or the bundle flag names a
/// capability with no parser behind it.
#[test]
fn every_bundle_parser_is_also_a_document_parser() {
    for d in descriptors().iter().filter(|d| d.bundle) {
        assert!(
            matches!(d.parser, Parser::Document(_)),
            "{} claims bundle support but is not read as a document",
            d.name,
        );
    }
}

// ---------------------------------------------------------------------------
// Dispatch reachability
// ---------------------------------------------------------------------------

/// Every advertised protocol is reachable through the dispatch its form
/// selects. The input is deliberately empty, so most parsers refuse it;
/// what is asserted is that none is refused for being *unregistered*,
/// which is what a listing containing an unreachable name would cause.
#[test]
fn every_listed_protocol_is_reachable_through_its_dispatch() {
    for d in descriptors() {
        let refusal = match d.parser {
            Parser::Document(_) => parse_schema_document(d.name, &serde_json::json!({}))
                .err()
                .map(|e| e.to_string()),
            Parser::Source(_) => parse_schema_source(d.name, "").err().map(|e| e.to_string()),
        };
        if let Some(message) = refusal {
            assert!(
                !message.contains("no document parser registered")
                    && !message.contains("no source parser registered"),
                "{} is advertised but unreachable: {message}",
                d.name,
            );
        }
    }
}

/// Asking for a source protocol through the document dispatch says so,
/// rather than reporting it unregistered. The two are different
/// mistakes and only one of them is the caller misspelling a name.
#[test]
fn using_the_wrong_dispatch_says_which_one_to_use() {
    let source_protocol = descriptors()
        .iter()
        .find(|d| matches!(d.parser, Parser::Source(_)))
        .expect("at least one source protocol");
    let err = parse_schema_document(source_protocol.name, &serde_json::json!({}))
        .expect_err("a source protocol is not a document protocol");
    assert!(
        err.to_string().contains("parse_schema_source"),
        "the error must name the right entry point, got: {err}",
    );

    let document_protocol = descriptors()
        .iter()
        .find(|d| matches!(d.parser, Parser::Document(_)))
        .expect("at least one document protocol");
    let err = parse_schema_source(document_protocol.name, "")
        .expect_err("a document protocol is not a source protocol");
    assert!(
        err.to_string().contains("parse_schema_document"),
        "the error must name the right entry point, got: {err}",
    );
}

#[test]
fn an_unregistered_protocol_is_refused_by_both_dispatches() {
    assert!(
        parse_schema_document("nonexistent", &serde_json::json!({}))
            .expect_err("unknown protocol")
            .to_string()
            .contains("no document parser registered"),
    );
    assert!(
        parse_schema_source("nonexistent", "")
            .expect_err("unknown protocol")
            .to_string()
            .contains("no source parser registered"),
    );
}
