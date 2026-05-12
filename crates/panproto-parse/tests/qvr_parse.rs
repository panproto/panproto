//! QVR (Quivers DSL) tree-sitter grammar parses end-to-end.
//!
//! QVR declares categorical theories: quantales, objects, morphisms,
//! continuous and stochastic spaces, and monadic programs over them.
//! These tests parse representative `.qvr` sources and assert the
//! recovered schema contains the structural vertex kinds QVR exposes.

#![cfg(all(feature = "grammars", feature = "lang-qvr"))]
#![allow(clippy::expect_used, clippy::panic)]

use panproto_parse::ParserRegistry;

const QVR_HMM: &[u8] = br#"
quantale product_fuzzy

object State : 8
object Obs   : 16

stochastic transition : State -> State
stochastic emission   : State -> Obs

let n_step = repeat(transition) >> emission
let hmm    = transition >> n_step

export hmm
"#;

const QVR_PROGRAM: &[u8] = br#"
type State = Euclidean 16

continuous transition : State -> State ~ Normal [scale=0.1]

program step : State -> State
    s <- transition
    return s
"#;

fn vertex_kinds(schema: &panproto_schema::Schema) -> std::collections::BTreeSet<String> {
    schema
        .vertices
        .values()
        .map(|v| v.kind.to_string())
        .collect()
}

#[test]
fn qvr_hmm_parses_with_expected_blocks() {
    let registry = ParserRegistry::new();
    let schema = registry
        .parse_with_protocol("qvr", QVR_HMM, "hmm.qvr")
        .expect("qvr parser must accept a well-formed HMM model");

    let kinds = vertex_kinds(&schema);
    for required in [
        "source_file",
        "quantale_decl",
        "object_decl",
        "stochastic_decl",
        "let_decl",
        "export_decl",
    ] {
        assert!(
            kinds.contains(required),
            "qvr schema missing required kind {required:?}; got {kinds:?}"
        );
    }
    assert!(schema.vertices.len() >= 8, "non-trivial schema");
    assert!(!schema.edges.is_empty(), "edges connect declarations");
}

#[test]
fn qvr_program_block_parses() {
    let registry = ParserRegistry::new();
    let schema = registry
        .parse_with_protocol("qvr", QVR_PROGRAM, "demo.qvr")
        .expect("qvr parser must accept a program block");

    let kinds = vertex_kinds(&schema);
    for required in [
        "type_alias_decl",
        "continuous_decl",
        "program_decl",
        "bind_step",
    ] {
        assert!(
            kinds.contains(required),
            "qvr schema missing required kind {required:?}; got {kinds:?}"
        );
    }
}

#[test]
fn qvr_extension_dispatch_matches_protocol() {
    let registry = ParserRegistry::new();
    let detected = registry.detect_language(std::path::Path::new("foo.qvr"));
    assert_eq!(detected, Some("qvr"));
}
