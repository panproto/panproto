//! Regression test for the theory DSL's handling of cyclic references
//! between bundle theories.
//!
//! `compile_bundle` compiles a bundle's theories in listing order,
//! resolving each theory's imports against the theories already compiled
//! plus the external resolver. A mutual import cycle therefore fails at
//! the first theory, which imports a sibling that has not yet been
//! compiled and is absent from the resolver. The DSL deliberately carries
//! no dedicated cycle-error variant: a reference cycle degrades to an
//! unresolved reference ([`TheoryDslError::TheoryNotFound`]) rather than a
//! distinct diagnostic.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use panproto_gat::Theory;
use panproto_theory_dsl::{TheoryDslError, compile, document::TheoryDocument};

#[test]
fn bundle_reference_cycle_surfaces_as_theory_not_found() {
    let source = r#"{
        "id": "dev.panproto.test.bundle_cycle",
        "description": "Two theories that import each other.",
        "bundle": "ThCycle",
        "theories": [
            { "theory": "ThA", "imports": [{ "from": "ThB" }], "sorts": [] },
            { "theory": "ThB", "imports": [{ "from": "ThA" }], "sorts": [] }
        ]
    }"#;
    let doc: TheoryDocument = serde_json::from_str(source).expect("valid bundle document");
    // Empty resolver: neither theory is a builtin, so ThA's import of ThB
    // can only resolve if ThB were already compiled, which it is not (ThB
    // is listed after ThA).
    let resolver = |_name: &str| -> Option<Theory> { None };
    let err = compile(&doc, &resolver).expect_err("mutual import cycle must not compile");
    match err {
        TheoryDslError::TheoryNotFound { name, .. } => {
            assert_eq!(
                name, "ThB",
                "the first bundle theory fails resolving its forward import"
            );
        }
        other => panic!("expected TheoryNotFound for the cyclic import, got {other:?}"),
    }
}
