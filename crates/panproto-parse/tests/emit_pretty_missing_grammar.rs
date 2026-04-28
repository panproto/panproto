//! `emit_pretty` returns a typed error when the language has no
//! vendored `grammar.json`.
//!
//! Constructs a `LanguageParser` with `grammar_json = None` and walks
//! the resulting `AstParser` trait method to verify the error path is
//! exercised end-to-end. Currently every grammar in `grammars.toml`
//! ships `grammar.json`, so this test guards the carve-out path that
//! kicks in if a future grammar lands without one.

#![cfg(feature = "grammars")]
#![allow(clippy::expect_used, clippy::panic)]

use panproto_grammars::Grammar;
use panproto_parse::error::ParseError;
use panproto_parse::languages::common::LanguageParser;
use panproto_parse::languages::walker_configs::walker_config_for;
use panproto_parse::registry::AstParser;
use panproto_schema::SchemaBuilder;

/// Pick any vendored grammar at random (the first by name). The test
/// is grammar-agnostic: it only needs a real `Language` so the parser
/// can construct, and then drives the no-grammar.json carve-out path.
fn any_grammar() -> Option<Grammar> {
    panproto_grammars::grammars().into_iter().next()
}

#[test]
fn emit_pretty_without_grammar_json_returns_typed_error() {
    let Some(grammar) = any_grammar() else {
        // No grammars enabled in this test build (e.g. someone built
        // panproto-parse with `--no-default-features`). The carve-out
        // path is unreachable without at least one grammar; skip.
        return;
    };
    let protocol_name = grammar.name;
    let parser = LanguageParser::from_language_with_grammar_json(
        protocol_name,
        grammar.extensions.to_vec(),
        grammar.language,
        grammar.node_types,
        grammar.tags_query,
        walker_config_for(protocol_name),
        None, // intentionally absent
    )
    .expect("parser construction should succeed without grammar.json");

    let protocol = build_minimal_protocol(protocol_name);
    let schema = SchemaBuilder::new(&protocol)
        .vertex("v0", "any", None)
        .expect("any-kind vertex builds")
        .build()
        .expect("minimal schema builds");

    let err = parser
        .emit_pretty(&schema)
        .expect_err("missing grammar.json must return Err");
    match err {
        ParseError::EmitFailed { protocol, reason } => {
            assert_eq!(protocol, protocol_name);
            assert!(
                reason.contains("grammar.json"),
                "reason should mention grammar.json: {reason:?}"
            );
        }
        other => panic!("unexpected error variant: {other:?}"),
    }
}

fn build_minimal_protocol(name: &str) -> panproto_schema::Protocol {
    panproto_schema::Protocol {
        name: name.into(),
        schema_theory: format!("Th{name}FullAST"),
        instance_theory: format!("Th{name}FullASTInstance"),
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
