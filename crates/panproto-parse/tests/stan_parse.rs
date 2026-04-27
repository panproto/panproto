//! Stan and stanfunctions tree-sitter grammars parse end-to-end.
//!
//! The vendored `WardBrian/tree-sitter-stan` grammar ships two
//! variants: `stan` (`.stan` files, the canonical model language) and
//! `stanfunctions` (`.stanfunctions` files, standalone user-defined
//! function libraries). These tests parse a small representative
//! source for each variant and assert the recovered schema contains
//! the structural vertex kinds Stan exposes.

#![cfg(all(feature = "grammars", feature = "lang-stan"))]
#![allow(clippy::expect_used, clippy::panic)]

use panproto_parse::ParserRegistry;

const STAN_MODEL: &[u8] = br#"
data {
  int<lower=0> N;
  vector[N] y;
}
parameters {
  real mu;
  real<lower=0> sigma;
}
model {
  y ~ normal(mu, sigma);
}
"#;

const STAN_FUNCTIONS: &[u8] = br#"
real square(real x) {
  return x * x;
}
"#;

fn vertex_kinds(schema: &panproto_schema::Schema) -> std::collections::BTreeSet<String> {
    schema
        .vertices
        .values()
        .map(|v| v.kind.to_string())
        .collect()
}

#[test]
fn stan_model_parses_with_expected_blocks() {
    let registry = ParserRegistry::new();
    let schema = registry
        .parse_with_protocol("stan", STAN_MODEL, "demo.stan")
        .expect("stan parser must accept a well-formed model");

    let kinds = vertex_kinds(&schema);
    for required in ["data", "parameters", "model", "sampling_statement"] {
        assert!(
            kinds.contains(required),
            "stan schema missing required kind {required:?}; got {kinds:?}"
        );
    }
    assert!(schema.vertices.len() >= 10, "non-trivial schema");
    assert!(!schema.edges.is_empty(), "edges connect blocks");
}

#[test]
fn stan_extension_dispatch_matches_protocol() {
    let registry = ParserRegistry::new();
    let detected = registry.detect_language(std::path::Path::new("foo.stan"));
    assert_eq!(detected, Some("stan"));
}

#[cfg(feature = "lang-stanfunctions")]
#[test]
fn stanfunctions_parses_user_defined_function_file() {
    let registry = ParserRegistry::new();
    let schema = registry
        .parse_with_protocol("stanfunctions", STAN_FUNCTIONS, "lib.stanfunctions")
        .expect("stanfunctions parser must accept a function definition");
    assert!(!schema.vertices.is_empty(), "vertices recovered");
    assert!(!schema.edges.is_empty(), "edges recovered");
}

#[cfg(feature = "lang-stanfunctions")]
#[test]
fn stanfunctions_extension_dispatch() {
    let registry = ParserRegistry::new();
    let detected = registry.detect_language(std::path::Path::new("lib.stanfunctions"));
    assert_eq!(detected, Some("stanfunctions"));
}
