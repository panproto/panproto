//! Regression test for panproto/panproto#34: Rust `fn` items were invisible
//! to named-scope detection because the hardcoded `SCOPE_INTRODUCING_KINDS`
//! list didn't include `function_item`. With grammar-driven detection via
//! `tree-sitter-tags`, every Rust function appears as a named scope.
//!
//! This test reproduces the exact scenario from the issue using the
//! `push_auth.rs` shape (a small file with an enum, a struct, and two
//! functions including the `verify_push` named in the bug report).

#![cfg(feature = "grammars")]

use panproto_grammars::Grammar;
use panproto_parse::{AstWalker, ScopeDetector, WalkerConfig, extract_theory_from_node_types};
use panproto_schema::Protocol;

const PUSH_AUTH_SNIPPET: &[u8] = br"
use serde::Deserialize;

pub enum PushAuth {
    Authenticated(String),
    NoCredentials,
    Denied(String),
}

#[derive(Debug, Deserialize)]
struct PushTokenClaims {
    sub: String,
    scope: String,
    exp: u64,
}

fn extract_basic_auth(headers: &axum::http::HeaderMap) -> Option<(String, String)> {
    None
}

pub fn verify_push(headers: &axum::http::HeaderMap, expected_did: &str) -> PushAuth {
    PushAuth::NoCredentials
}
";

fn rust_grammar() -> Option<Grammar> {
    panproto_grammars::grammars()
        .into_iter()
        .find(|g| g.name == "rust")
}

fn open_protocol() -> Protocol {
    Protocol {
        name: "rust".into(),
        schema_theory: "ThRustFullAST".into(),
        instance_theory: "ThRustFullASTInstance".into(),
        schema_composition: None,
        instance_composition: None,
        obj_kinds: vec![],
        edge_rules: vec![],
        constraint_sorts: vec![],
        has_order: true,
        has_coproducts: false,
        has_recursion: true,
        has_causal: false,
        nominal_identity: false,
        has_defaults: false,
        has_coercions: false,
        has_mergers: false,
        has_policies: false,
    }
}

fn parse_and_walk(
    grammar: &Grammar,
    source: &[u8],
    file_path: &str,
) -> Result<panproto_schema::Schema, Box<dyn std::error::Error>> {
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&grammar.language)?;
    let tree = parser
        .parse(source, None)
        .ok_or("tree-sitter parse returned None")?;

    let theory_meta = extract_theory_from_node_types("ThRustFullAST", grammar.node_types)?;
    let protocol = open_protocol();
    let mut detector = ScopeDetector::new(&grammar.language, grammar.tags_query, None)?;
    let walker = AstWalker::new(
        source,
        &theory_meta,
        &protocol,
        WalkerConfig::standard(),
        Some(&mut detector),
    );

    Ok(walker.walk(&tree, file_path)?)
}

#[test]
fn rust_functions_structs_enums_appear_as_named_scopes() -> Result<(), Box<dyn std::error::Error>> {
    let Some(grammar) = rust_grammar() else {
        return Ok(()); // rust grammar not enabled in this feature set
    };
    assert!(
        grammar.tags_query.is_some(),
        "rust grammar must ship tags.scm, otherwise scope detection is a no-op"
    );

    let schema = parse_and_walk(&grammar, PUSH_AUTH_SNIPPET, "push_auth.rs")?;
    let ids: Vec<String> = schema.vertices.keys().map(ToString::to_string).collect();

    // The regression: before the fix, verify_push and extract_basic_auth
    // would never appear here. After the fix, both are named scopes.
    for expected in [
        "verify_push",
        "extract_basic_auth",
        "PushAuth",
        "PushTokenClaims",
    ] {
        let suffix = format!("::{expected}");
        assert!(
            ids.iter().any(|id| id.ends_with(&suffix)),
            "expected vertex ID ending in {suffix} for {expected}, got: {ids:?}"
        );
    }
    Ok(())
}

#[test]
fn rust_scope_ids_preserve_file_prefix() -> Result<(), Box<dyn std::error::Error>> {
    let Some(grammar) = rust_grammar() else {
        return Ok(());
    };
    if grammar.tags_query.is_none() {
        return Ok(());
    }

    let schema = parse_and_walk(
        &grammar,
        PUSH_AUTH_SNIPPET,
        "crates/cospan-node/src/auth/push_auth.rs",
    )?;
    let ids: Vec<String> = schema.vertices.keys().map(ToString::to_string).collect();

    // The scope ID format (`file::scope`) must be preserved so that
    // panproto-check's nearest_named_scope keeps working.
    let verify_id = ids
        .iter()
        .find(|id| id.ends_with("::verify_push"))
        .ok_or("verify_push scope vertex missing")?;
    assert!(
        verify_id.starts_with("crates/cospan-node/src/auth/push_auth.rs"),
        "scope id must carry the file path prefix, got {verify_id}"
    );
    Ok(())
}
