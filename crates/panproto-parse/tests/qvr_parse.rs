//! QVR (Quivers DSL) tree-sitter grammar parses end-to-end.
//!
//! QVR declares categorical theories: quantales, objects, morphisms,
//! continuous and stochastic spaces, and monadic programs over them.
//! These tests parse representative `.qvr` sources and assert the
//! recovered schema contains the structural vertex kinds QVR exposes.

#![cfg(all(feature = "grammars", feature = "lang-qvr"))]
#![allow(clippy::expect_used, clippy::panic)]

use panproto_parse::ParserRegistry;

const QVR_HMM: &[u8] = br"
composition product_fuzzy [level=algebra]

object State : FinSet 8
object Obs : FinSet 16

morphism initial : State -> State [role=latent]
morphism transition : State -> State [role=latent]
morphism emission : State -> Obs [role=latent]

define n_step = repeat(transition) >> emission
define hmm = initial >> n_step

export hmm
";

const QVR_PROGRAM: &[u8] = br"
object State : Real 16

morphism transition : State -> State [role=kernel] ~ Normal(0.0, 0.1)

program step : State -> State
    sample s <- transition
    return s
";

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
        "composition_decl",
        "object_decl",
        "morphism_decl",
        "define_decl",
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
        "object_decl",
        "morphism_decl",
        "program_decl",
        "sample_step",
        "return_step",
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
