//! Hint-file stringency integration: a JSON hint file that sets
//! `stringency: "lenient"` must drive the engine to emit Lenient-tier
//! behaviour (in particular, the sort-drop span step when a source
//! kind has no target counterpart).

#![allow(clippy::expect_used)]

use panproto_gat::TheoryTransform;
use panproto_lens::{
    auto_lens::{AutoLensConfig, Stringency, auto_generate_with_hints},
    hint,
};
use panproto_lens_dsl::HintSpec;
use panproto_schema::{Protocol, Schema, SchemaBuilder};

fn generic_protocol() -> Protocol {
    Protocol {
        name: "generic".into(),
        schema_theory: "ThGraph".into(),
        instance_theory: "ThWType".into(),
        edge_rules: vec![],
        obj_kinds: vec!["record".into(), "string".into(), "boolean".into()],
        constraint_sorts: vec![],
        ..Protocol::default()
    }
}

fn build(verts: &[(&str, &str)], edges: &[(&str, &str, &str, &str)]) -> Schema {
    let proto = generic_protocol();
    let mut b = SchemaBuilder::new(&proto);
    for (id, k) in verts {
        b = b.vertex(id, k, None::<&str>).expect("vertex");
    }
    for (s, t, k, n) in edges {
        b = b.edge(s, t, k, Some(n)).expect("edge");
    }
    b.build().expect("build")
}

#[test]
fn hint_file_with_lenient_stringency_triggers_span_drop() {
    let src = build(
        &[("r", "record"), ("r.keep", "string"), ("r.flag", "boolean")],
        &[
            ("r", "r.keep", "prop", "keep"),
            ("r", "r.flag", "prop", "flag"),
        ],
    );
    let tgt = build(
        &[("r", "record"), ("r.keep", "string")],
        &[("r", "r.keep", "prop", "keep")],
    );
    let protocol = generic_protocol();

    // Hint file encoded as JSON, as it would live on disk.
    let hint_json = r#"{
        "anchors": {},
        "constraints": [],
        "stringency": "lenient"
    }"#;
    let hint_spec: HintSpec = serde_json::from_str(hint_json).expect("parse hint");

    // Mimic the CLI plumbing: lift the DSL stringency into the engine's.
    let engine_stringency = match hint_spec.stringency.expect("stringency present") {
        panproto_lens_dsl::HintStringency::Strict => Stringency::Strict,
        panproto_lens_dsl::HintStringency::Balanced => Stringency::Balanced,
        panproto_lens_dsl::HintStringency::Lenient => Stringency::Lenient,
        panproto_lens_dsl::HintStringency::Exploratory => Stringency::Exploratory,
    };
    let config = AutoLensConfig {
        stringency: engine_stringency,
        ..Default::default()
    };

    let parts = hint::HintParts {
        anchors: hint_spec.anchors.clone(),
        scope_pairs: hint_spec.scope_pairs(),
        excluded_targets: hint_spec.excluded_target_names(),
        excluded_sources: hint_spec.excluded_source_names(),
        scoring_weights: hint_spec.scoring_weights(),
        name_similarity_threshold: hint_spec.name_similarity_threshold(),
    };
    let (derived, domain) = hint::resolve_hints(&parts, &src, &tgt);

    let result = auto_generate_with_hints(&src, &tgt, &protocol, &config, &derived, &domain, None)
        .expect("Lenient should find a span");

    let has_drop_boolean = result.chain.steps.iter().any(|step| {
        matches!(
            &step.target.transform,
            TheoryTransform::DropSort(name) if name.as_ref() == "boolean"
        )
    });
    assert!(
        has_drop_boolean,
        "Lenient via hint file must drive a DropSort(boolean) in the chain; got {:?}",
        result
            .chain
            .steps
            .iter()
            .map(|s| s.name.to_string())
            .collect::<Vec<_>>()
    );
}

/// When the CLI flag `--stringency strict` is passed alongside a hint
/// file that says `"stringency": "lenient"`, the CLI flag must win:
/// `Strict` forbids the lenient span drop, so we should fail to find
/// a morphism rather than silently emitting `DropSort(boolean)`.
///
/// This mirrors the CLI plumbing in `crates/panproto-cli/src/cmd/lens.rs`:
/// if `stringency_arg.is_some()`, the hint-file `stringency` is ignored.
#[test]
fn cli_stringency_flag_overrides_hint_file_stringency() {
    let src = build(
        &[("r", "record"), ("r.keep", "string"), ("r.flag", "boolean")],
        &[
            ("r", "r.keep", "prop", "keep"),
            ("r", "r.flag", "prop", "flag"),
        ],
    );
    let tgt = build(
        &[("r", "record"), ("r.keep", "string")],
        &[("r", "r.keep", "prop", "keep")],
    );
    let protocol = generic_protocol();

    // Hint file says "lenient" but CLI flag says "strict".
    let hint_json = r#"{ "anchors": {}, "constraints": [], "stringency": "lenient" }"#;
    let hint_spec: HintSpec = serde_json::from_str(hint_json).expect("parse hint");

    // Mimic the CLI: `stringency_arg.is_some()` => ignore hint stringency.
    let cli_stringency = Some(Stringency::Strict);
    let mut config = AutoLensConfig::default();
    if let Some(s) = cli_stringency {
        config.stringency = s;
    } else if let Some(s) = hint_spec.stringency {
        config.stringency = match s {
            panproto_lens_dsl::HintStringency::Strict => Stringency::Strict,
            panproto_lens_dsl::HintStringency::Balanced => Stringency::Balanced,
            panproto_lens_dsl::HintStringency::Lenient => Stringency::Lenient,
            panproto_lens_dsl::HintStringency::Exploratory => Stringency::Exploratory,
        };
    }
    assert_eq!(
        config.stringency,
        Stringency::Strict,
        "CLI flag must win over hint file stringency",
    );

    let parts = hint::HintParts {
        anchors: hint_spec.anchors.clone(),
        scope_pairs: hint_spec.scope_pairs(),
        excluded_targets: hint_spec.excluded_target_names(),
        excluded_sources: hint_spec.excluded_source_names(),
        scoring_weights: hint_spec.scoring_weights(),
        name_similarity_threshold: hint_spec.name_similarity_threshold(),
    };
    let (derived, domain) = hint::resolve_hints(&parts, &src, &tgt);

    // With Strict, no span drop: should fail to find a morphism.
    let err = auto_generate_with_hints(&src, &tgt, &protocol, &config, &derived, &domain, None)
        .err()
        .expect("Strict must fail where Lenient would succeed");
    let _ = err; // non-empty is all we need.
}
