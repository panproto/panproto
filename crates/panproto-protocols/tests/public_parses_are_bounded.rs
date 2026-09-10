//! A public parse is bounded by default, and says what ran out.
//!
//! Large bundles, schemas and documents used to reach different limits
//! depending on whether they entered through Rust, the CLI, Python, C
//! or WASM, because each subsystem had its own local bound and no
//! surface had a policy. These are the properties that make the
//! default a policy: it applies without being asked for, it names what
//! it refused, and a caller can widen or remove it deliberately.

#![allow(clippy::expect_used)]

use panproto_expr::limits::{Budget, Resource, ResourceLimits};
use panproto_protocols::{
    ProtocolError, parse_schema_bundle, parse_schema_bundle_within, parse_schema_document_within,
    parse_schema_source_within,
};

fn tiny_budget(entries: u64, bytes: u64) -> Budget {
    let mut l = ResourceLimits::unbounded();
    l.bundle_entries = entries;
    l.input_bytes = bytes;
    Budget::new(l)
}

#[test]
fn a_bundle_of_too_many_documents_is_refused_by_its_shape() {
    let docs: Vec<serde_json::Value> = (0..10).map(|_| serde_json::json!({})).collect();
    let err = parse_schema_bundle_within("atproto", &docs, &tiny_budget(3, 0))
        .expect_err("ten documents against a bound of three");
    let ProtocolError::LimitExceeded(limit) = err else {
        panic!("expected a limit failure, got {err}");
    };
    assert_eq!(limit.resource, Resource::BundleEntries);
    assert_eq!(limit.limit, 3);
}

#[test]
fn an_oversized_document_is_refused_and_the_error_names_the_resource() {
    let big = serde_json::json!({ "id": "x".repeat(4_096) });
    let err = parse_schema_document_within("atproto", &big, &tiny_budget(0, 64))
        .expect_err("a document past the byte bound");
    let ProtocolError::LimitExceeded(limit) = err else {
        panic!("expected a limit failure, got {err}");
    };
    assert_eq!(limit.resource, Resource::InputBytes);
    assert_eq!(limit.limit, 64);
    assert!(
        limit.to_string().contains("64") && limit.to_string().contains("input bytes"),
        "the message must name both, got: {limit}",
    );
}

#[test]
fn oversized_source_text_is_refused() {
    let err = parse_schema_source_within("graphql", &"a".repeat(4_096), &tiny_budget(0, 64))
        .expect_err("source past the byte bound");
    assert!(matches!(err, ProtocolError::LimitExceeded(l) if l.resource == Resource::InputBytes));
}

/// The bundle's documents are charged against one allowance, so a
/// bundle cannot slip past a byte bound by splitting its content across
/// entries.
#[test]
fn a_bundle_charges_every_document_against_one_allowance() {
    let each = serde_json::json!({ "id": "x".repeat(100) });
    let docs = vec![each.clone(), each.clone(), each];
    let err = parse_schema_bundle_within("atproto", &docs, &tiny_budget(0, 200))
        .expect_err("three documents of a hundred bytes against a bound of two hundred");
    assert!(matches!(err, ProtocolError::LimitExceeded(l) if l.resource == Resource::InputBytes));
}

/// The default is generous enough that ordinary input is unaffected:
/// the bound exists for documents built to exhaust a machine, not for
/// ones a person would write.
#[test]
fn an_ordinary_bundle_passes_the_default_budget() {
    let defs = serde_json::json!({
        "lexicon": 1,
        "id": "com.example.defs",
        "defs": { "main": { "type": "object", "properties": { "v": { "type": "string" } } } }
    });
    let result = parse_schema_bundle("atproto", &[defs]);
    assert!(
        !matches!(result, Err(ProtocolError::LimitExceeded(_))),
        "the default must not refuse an ordinary document: {result:?}",
    );
}

/// A Rust caller processing input it produced itself can say so. What
/// the type prevents is inheriting that at a boundary reading input
/// from elsewhere.
#[test]
fn an_unbounded_budget_admits_what_the_default_would_refuse() {
    let docs: Vec<serde_json::Value> = (0..10_000).map(|_| serde_json::json!({})).collect();
    let err =
        parse_schema_bundle_within("atproto", &docs, &Budget::new(ResourceLimits::unbounded()));
    assert!(
        !matches!(err, Err(ProtocolError::LimitExceeded(_))),
        "an explicitly unbounded budget charges nothing",
    );
}

/// Charges accumulate across calls sharing a budget, which is what
/// stops a caller from processing an oversized input in pieces.
#[test]
fn separate_parses_sharing_a_budget_share_its_allowance() {
    let budget = tiny_budget(0, 300);
    let doc = serde_json::json!({ "id": "x".repeat(100) });

    parse_schema_document_within("atproto", &doc, &budget).ok();
    parse_schema_document_within("atproto", &doc, &budget).ok();
    let err = parse_schema_document_within("atproto", &doc, &budget)
        .expect_err("the third parse exhausts the shared allowance");
    assert!(matches!(err, ProtocolError::LimitExceeded(l) if l.resource == Resource::InputBytes));
}
