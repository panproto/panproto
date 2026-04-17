//! End-to-end coverage for the Exploratory-tier coerce proposals
//! surfacing path. Exercises the library entry point and re-checks
//! the serializable shape used by the CLI / SDK bindings.

#![allow(clippy::expect_used)]

use panproto_lens::auto_lens::{AutoLensConfig, Stringency, auto_generate_with_hints};
use panproto_mig::hom_search::DomainConstraints;
use panproto_schema::{Protocol, Schema, SchemaBuilder};

fn protocol() -> Protocol {
    Protocol {
        name: "test".into(),
        schema_theory: "ThGraph".into(),
        instance_theory: "ThWType".into(),
        edge_rules: vec![],
        obj_kinds: vec!["record".into(), "integer".into(), "string".into()],
        constraint_sorts: vec![],
        ..Protocol::default()
    }
}

fn build(verts: &[(&str, &str)], edges: &[(&str, &str, &str, &str)]) -> Schema {
    let p = protocol();
    let mut b = SchemaBuilder::new(&p);
    for (id, k) in verts {
        b = b.vertex(id, k, None::<&str>).expect("vertex");
    }
    for (s, t, k, n) in edges {
        b = b.edge(s, t, k, Some(n)).expect("edge");
    }
    b.build().expect("build")
}

#[test]
fn exploratory_coerce_proposals_are_serializable_to_json() {
    let src = build(
        &[("r", "record"), ("r.n", "integer")],
        &[("r", "r.n", "prop", "n")],
    );
    let tgt = build(
        &[("r", "record"), ("r.n", "string")],
        &[("r", "r.n", "prop", "n")],
    );
    let protocol = protocol();
    let config = AutoLensConfig {
        stringency: Stringency::Exploratory,
        ..Default::default()
    };
    let hints = std::collections::HashMap::new();
    let dc = DomainConstraints::default();
    let result =
        auto_generate_with_hints(&src, &tgt, &protocol, &config, &hints, &dc, None).expect("ok");
    assert!(
        !result.coerce_proposals.is_empty(),
        "Exploratory with int↔str mismatch should surface coerce proposals"
    );
    // Recreate the exact shape the CLI / Python / WASM surfaces.
    let serialized: Vec<serde_json::Value> = result
        .coerce_proposals
        .iter()
        .map(|p| {
            serde_json::json!({
                "src": p.anchor.src.as_str(),
                "tgt": p.anchor.tgt.as_str(),
                "witness_name": p.witness_name,
                "witness_class": p.witness_class,
                "confidence": p.anchor.confidence,
                "explanation": p.anchor.explanation,
            })
        })
        .collect();
    let entry = serialized.first().expect("at least one entry");
    for key in [
        "src",
        "tgt",
        "witness_name",
        "witness_class",
        "confidence",
        "explanation",
    ] {
        assert!(
            entry.get(key).is_some(),
            "coerce proposal JSON missing `{key}`; got {entry}"
        );
    }
}
