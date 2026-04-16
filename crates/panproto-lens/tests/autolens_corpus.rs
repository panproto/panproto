//! Cross-protocol autolens corpus harness.
//!
//! Exercises `panproto_lens::auto_generate` against a suite of realistic
//! schema pairs at every [`Stringency`] tier, pinning the current behavior.
//! The corpus is organized by pattern:
//!
//! * `corpus/generic_records/` — identity, pure structural rename, casing.
//! * `corpus/rename_cluster/`  — alias-driven field-name renames.
//! * `corpus/sql_like/`        — SQL-style snake_case rename patterns.
//! * `corpus/nested_vs_flat/`  — record flattening (awaiting wrap/unwrap).
//! * `corpus/wrap_unwrap/`     — drop-only / add-only (awaiting span search).
//!
//! The corpus itself is programmatically-built [`Schema`] pairs rather than
//! on-disk JSON: panproto-schema JSON serialization is brittle across
//! protocols, and the test-protocol pattern already used across the
//! codebase is the canonical way to construct them.
//!
//! Expectations below pin CURRENT (v0.32) behavior. Cases that the plan
//! marks as future upgrades (drop/add-only via span search — Task 4;
//! nested↔flat via wrap/unwrap — Task 5; sort coercion — Task 7) are
//! pinned to `Fails` so a later PR lighting up that path will force the
//! expectation to be tightened.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::too_many_lines,
    clippy::doc_markdown,
    clippy::needless_pass_by_value,
    clippy::missing_const_for_fn,
    clippy::option_if_let_else,
    clippy::explicit_auto_deref
)]

use panproto_lens::auto_lens::{AutoLensConfig, AutoLensResult, Stringency, auto_generate};
use panproto_lens::error::LensError;
use panproto_schema::{Protocol, Schema, SchemaBuilder};

/// Outcome expected for a given case at a given stringency tier.
#[derive(Clone, Copy, Debug)]
enum ExpectedOutcome {
    /// Alignment succeeds; quality must be at least the provided floor.
    AlignsWithQualityAtLeast(f64),
    /// Alignment fails (returns Err) OR succeeds with quality 0.
    Fails,
}

struct CorpusCase {
    name: &'static str,
    #[allow(dead_code)]
    protocol: String,
    src: Schema,
    tgt: Schema,
    expected_morphism_at_strict: Option<ExpectedOutcome>,
    expected_morphism_at_balanced: Option<ExpectedOutcome>,
    expected_morphism_at_lenient: Option<ExpectedOutcome>,
    expected_morphism_at_exploratory: Option<ExpectedOutcome>,
    /// Tolerance for quality regressions between adjacent tiers. Pins a
    /// known, documented regression in current behavior so a later fix
    /// can tighten the tolerance back to zero. Most cases use `0.0`.
    monotonicity_tolerance: f64,
}

// -----------------------------------------------------------------------
// Protocol + schema builders
// -----------------------------------------------------------------------

fn generic_protocol() -> Protocol {
    Protocol {
        name: "generic".into(),
        schema_theory: "ThGraph".into(),
        instance_theory: "ThWType".into(),
        edge_rules: vec![],
        obj_kinds: vec![
            "record".into(),
            "string".into(),
            "integer".into(),
            "boolean".into(),
            "array".into(),
        ],
        constraint_sorts: vec![],
        ..Protocol::default()
    }
}

fn build(vertices: &[(&str, &str)], edges: &[(&str, &str, &str, &str)]) -> Schema {
    let proto = generic_protocol();
    let mut b = SchemaBuilder::new(&proto);
    for (id, kind) in vertices {
        b = b.vertex(*id, *kind, None::<&str>).unwrap();
    }
    for (src, tgt, kind, name) in edges {
        b = b.edge(*src, *tgt, *kind, Some(*name)).unwrap();
    }
    b.build().unwrap()
}

// -----------------------------------------------------------------------
// Cases
// -----------------------------------------------------------------------

/// (a) Identical schemas — trivial success at all tiers.
fn case_identical() -> CorpusCase {
    let s = build(
        &[
            ("post", "record"),
            ("post.text", "string"),
            ("post.createdAt", "string"),
        ],
        &[
            ("post", "post.text", "prop", "text"),
            ("post", "post.createdAt", "prop", "createdAt"),
        ],
    );
    let t = s.clone();
    CorpusCase {
        name: "identical_generic_record",
        protocol: "generic".into(),
        src: s,
        tgt: t,
        expected_morphism_at_strict: Some(ExpectedOutcome::AlignsWithQualityAtLeast(0.9)),
        expected_morphism_at_balanced: Some(ExpectedOutcome::AlignsWithQualityAtLeast(0.9)),
        expected_morphism_at_lenient: Some(ExpectedOutcome::AlignsWithQualityAtLeast(0.9)),
        expected_morphism_at_exploratory: Some(ExpectedOutcome::AlignsWithQualityAtLeast(0.9)),
        monotonicity_tolerance: 0.0,
    }
}

/// (b) Pure field rename: `post.text` → `post.body`. Balanced alias dict
/// recognizes text ≡ body. Empirically, even Strict finds a structural
/// morphism here with modest quality (~0.39) because kind signatures
/// unique-match `post.text: string` to `post.body: string` via the CSP.
fn case_pure_rename_text_body() -> CorpusCase {
    let src = build(
        &[("post", "record"), ("post.text", "string")],
        &[("post", "post.text", "prop", "text")],
    );
    let tgt = build(
        &[("post", "record"), ("post.body", "string")],
        &[("post", "post.body", "prop", "body")],
    );
    CorpusCase {
        name: "pure_rename_text_body",
        protocol: "generic".into(),
        src,
        tgt,
        expected_morphism_at_strict: Some(ExpectedOutcome::AlignsWithQualityAtLeast(0.3)),
        expected_morphism_at_balanced: Some(ExpectedOutcome::AlignsWithQualityAtLeast(0.3)),
        expected_morphism_at_lenient: Some(ExpectedOutcome::AlignsWithQualityAtLeast(0.3)),
        expected_morphism_at_exploratory: Some(ExpectedOutcome::AlignsWithQualityAtLeast(0.3)),
        monotonicity_tolerance: 0.0,
    }
}

/// (c) Temporal rename: `createdAt` ↔ `sentAt`. Both names fall in the
/// temporal alias cluster, but the current engine still only reports
/// quality ~0.26 across all tiers (structural CSP match; alias priors
/// don't lift the quality score itself).
fn case_temporal_rename() -> CorpusCase {
    let src = build(
        &[
            ("post", "record"),
            ("post.text", "string"),
            ("post.createdAt", "string"),
        ],
        &[
            ("post", "post.text", "prop", "text"),
            ("post", "post.createdAt", "prop", "createdAt"),
        ],
    );
    let tgt = build(
        &[
            ("message", "record"),
            ("message.body", "string"),
            ("message.sentAt", "string"),
        ],
        &[
            ("message", "message.body", "prop", "body"),
            ("message", "message.sentAt", "prop", "sentAt"),
        ],
    );
    CorpusCase {
        name: "temporal_rename_created_sent",
        protocol: "generic".into(),
        src,
        tgt,
        expected_morphism_at_strict: Some(ExpectedOutcome::AlignsWithQualityAtLeast(0.2)),
        expected_morphism_at_balanced: Some(ExpectedOutcome::AlignsWithQualityAtLeast(0.2)),
        expected_morphism_at_lenient: Some(ExpectedOutcome::AlignsWithQualityAtLeast(0.2)),
        expected_morphism_at_exploratory: Some(ExpectedOutcome::AlignsWithQualityAtLeast(0.2)),
        monotonicity_tolerance: 0.0,
    }
}

/// (d) Casing change: `createdAt` ↔ `created_at`. The alias dict's casing
/// normalization catches this even without a cluster entry.
fn case_casing_rename() -> CorpusCase {
    let src = build(
        &[("post", "record"), ("post.createdAt", "string")],
        &[("post", "post.createdAt", "prop", "createdAt")],
    );
    let tgt = build(
        &[("post", "record"), ("post.created_at", "string")],
        &[("post", "post.created_at", "prop", "created_at")],
    );
    CorpusCase {
        name: "casing_created_at",
        protocol: "generic".into(),
        src,
        tgt,
        expected_morphism_at_strict: Some(ExpectedOutcome::AlignsWithQualityAtLeast(0.3)),
        expected_morphism_at_balanced: Some(ExpectedOutcome::AlignsWithQualityAtLeast(0.3)),
        expected_morphism_at_lenient: Some(ExpectedOutcome::AlignsWithQualityAtLeast(0.3)),
        expected_morphism_at_exploratory: Some(ExpectedOutcome::AlignsWithQualityAtLeast(0.3)),
        monotonicity_tolerance: 0.0,
    }
}

/// (e) Cross-namespace records: different root names, children alias.
fn case_cross_namespace() -> CorpusCase {
    let src = build(
        &[
            ("entry", "record"),
            ("entry.content", "string"),
            ("entry.timestamp", "string"),
        ],
        &[
            ("entry", "entry.content", "prop", "content"),
            ("entry", "entry.timestamp", "prop", "timestamp"),
        ],
    );
    let tgt = build(
        &[
            ("note", "record"),
            ("note.body", "string"),
            ("note.createdAt", "string"),
        ],
        &[
            ("note", "note.body", "prop", "body"),
            ("note", "note.createdAt", "prop", "createdAt"),
        ],
    );
    CorpusCase {
        name: "cross_namespace_entry_note",
        protocol: "generic".into(),
        src,
        tgt,
        expected_morphism_at_strict: Some(ExpectedOutcome::AlignsWithQualityAtLeast(0.2)),
        expected_morphism_at_balanced: Some(ExpectedOutcome::AlignsWithQualityAtLeast(0.2)),
        expected_morphism_at_lenient: Some(ExpectedOutcome::AlignsWithQualityAtLeast(0.2)),
        expected_morphism_at_exploratory: Some(ExpectedOutcome::AlignsWithQualityAtLeast(0.2)),
        monotonicity_tolerance: 0.0,
    }
}

/// (f) SQL-style snake_case rename: `user_id` ↔ `customer_id`. The alias
/// dict has no `user ≡ customer` cluster; Balanced adds alias anchors that
/// actually introduce *slightly worse* target assignments for the CSP
/// (quality 0.645 vs. Strict 0.656). We pin this known regression with a
/// small monotonicity tolerance; it should tighten when Task 4's span
/// search / coverage scoring lands.
fn case_sql_user_customer() -> CorpusCase {
    let src = build(
        &[
            ("orders", "record"),
            ("orders.id", "integer"),
            ("orders.user_id", "integer"),
        ],
        &[
            ("orders", "orders.id", "prop", "id"),
            ("orders", "orders.user_id", "prop", "user_id"),
        ],
    );
    let tgt = build(
        &[
            ("orders", "record"),
            ("orders.id", "integer"),
            ("orders.customer_id", "integer"),
        ],
        &[
            ("orders", "orders.id", "prop", "id"),
            ("orders", "orders.customer_id", "prop", "customer_id"),
        ],
    );
    CorpusCase {
        name: "sql_user_id_to_customer_id",
        protocol: "sql_like".into(),
        src,
        tgt,
        expected_morphism_at_strict: Some(ExpectedOutcome::AlignsWithQualityAtLeast(0.5)),
        expected_morphism_at_balanced: Some(ExpectedOutcome::AlignsWithQualityAtLeast(0.5)),
        expected_morphism_at_lenient: Some(ExpectedOutcome::AlignsWithQualityAtLeast(0.5)),
        expected_morphism_at_exploratory: Some(ExpectedOutcome::AlignsWithQualityAtLeast(0.5)),
        // Known Strict→Balanced quality dip: 0.656 → 0.645. Remove when
        // the coverage term (plan §3) is wired into the ranker.
        monotonicity_tolerance: 0.05,
    }
}

/// (g) Drop-only: src has an extra field absent in tgt. The CSP happily
/// maps what it can and scores ~0.67 today; the extra source field is
/// silently unmapped. This is NOT a categorically sound span (no leg is
/// emitted for the drop) but the engine currently accepts it.
///
/// TODO: plan Task 4 (Lenient span search) will replace this with a
/// principled span `A ← C → B` and emit a `DropOp` factor. Expected
/// outcomes will tighten to carry coverage info at that point.
fn case_drop_only() -> CorpusCase {
    let src = build(
        &[
            ("post", "record"),
            ("post.text", "string"),
            ("post.extra", "string"),
        ],
        &[
            ("post", "post.text", "prop", "text"),
            ("post", "post.extra", "prop", "extra"),
        ],
    );
    let tgt = build(
        &[("post", "record"), ("post.text", "string")],
        &[("post", "post.text", "prop", "text")],
    );
    CorpusCase {
        name: "drop_only_extra_field",
        protocol: "wrap_unwrap".into(),
        src,
        tgt,
        expected_morphism_at_strict: Some(ExpectedOutcome::AlignsWithQualityAtLeast(0.5)),
        expected_morphism_at_balanced: Some(ExpectedOutcome::AlignsWithQualityAtLeast(0.5)),
        expected_morphism_at_lenient: Some(ExpectedOutcome::AlignsWithQualityAtLeast(0.5)),
        expected_morphism_at_exploratory: Some(ExpectedOutcome::AlignsWithQualityAtLeast(0.5)),
        monotonicity_tolerance: 0.0,
    }
}

/// (h) Add-only: tgt has an extra field. The engine currently succeeds
/// with quality ~0.8 but the added target field is handled by zero-value
/// defaults rather than an explicit `AddOp` factor in the span.
///
/// TODO: plan Task 4 will introduce the leg explicitly.
fn case_add_only() -> CorpusCase {
    let src = build(
        &[("post", "record"), ("post.text", "string")],
        &[("post", "post.text", "prop", "text")],
    );
    let tgt = build(
        &[
            ("post", "record"),
            ("post.text", "string"),
            ("post.extra", "string"),
        ],
        &[
            ("post", "post.text", "prop", "text"),
            ("post", "post.extra", "prop", "extra"),
        ],
    );
    CorpusCase {
        name: "add_only_extra_field",
        protocol: "wrap_unwrap".into(),
        src,
        tgt,
        expected_morphism_at_strict: Some(ExpectedOutcome::AlignsWithQualityAtLeast(0.5)),
        expected_morphism_at_balanced: Some(ExpectedOutcome::AlignsWithQualityAtLeast(0.5)),
        expected_morphism_at_lenient: Some(ExpectedOutcome::AlignsWithQualityAtLeast(0.5)),
        expected_morphism_at_exploratory: Some(ExpectedOutcome::AlignsWithQualityAtLeast(0.5)),
        monotonicity_tolerance: 0.0,
    }
}

/// (i) Nested vs. flat: `reply.parent.uri` etc. vs. `reply.parentUri`.
/// Currently Fails at every tier; upgrades with Task 5 (wrap/unwrap
/// strategy + SortLens library).
fn case_nested_vs_flat() -> CorpusCase {
    let src = build(
        &[
            ("reply", "record"),
            ("reply.parent", "record"),
            ("reply.parent.uri", "string"),
            ("reply.parent.cid", "string"),
        ],
        &[
            ("reply", "reply.parent", "prop", "parent"),
            ("reply.parent", "reply.parent.uri", "prop", "uri"),
            ("reply.parent", "reply.parent.cid", "prop", "cid"),
        ],
    );
    let tgt = build(
        &[
            ("reply", "record"),
            ("reply.parentUri", "string"),
            ("reply.parentCid", "string"),
        ],
        &[
            ("reply", "reply.parentUri", "prop", "parentUri"),
            ("reply", "reply.parentCid", "prop", "parentCid"),
        ],
    );
    CorpusCase {
        name: "nested_vs_flat_reply",
        protocol: "nested_vs_flat".into(),
        src,
        tgt,
        expected_morphism_at_strict: Some(ExpectedOutcome::Fails),
        expected_morphism_at_balanced: Some(ExpectedOutcome::Fails),
        expected_morphism_at_lenient: Some(ExpectedOutcome::Fails),
        expected_morphism_at_exploratory: Some(ExpectedOutcome::Fails),
        monotonicity_tolerance: 0.0,
    }
}

/// (j) Pure identifier rename: `id` ↔ `uuid`, same kind. Alias cluster
/// covers this.
fn case_id_uuid_rename() -> CorpusCase {
    let src = build(
        &[("row", "record"), ("row.id", "string")],
        &[("row", "row.id", "prop", "id")],
    );
    let tgt = build(
        &[("row", "record"), ("row.uuid", "string")],
        &[("row", "row.uuid", "prop", "uuid")],
    );
    CorpusCase {
        name: "id_to_uuid",
        protocol: "rename_cluster".into(),
        src,
        tgt,
        expected_morphism_at_strict: Some(ExpectedOutcome::AlignsWithQualityAtLeast(0.3)),
        expected_morphism_at_balanced: Some(ExpectedOutcome::AlignsWithQualityAtLeast(0.3)),
        expected_morphism_at_lenient: Some(ExpectedOutcome::AlignsWithQualityAtLeast(0.3)),
        expected_morphism_at_exploratory: Some(ExpectedOutcome::AlignsWithQualityAtLeast(0.3)),
        monotonicity_tolerance: 0.0,
    }
}

fn all_cases() -> Vec<CorpusCase> {
    vec![
        case_identical(),
        case_pure_rename_text_body(),
        case_temporal_rename(),
        case_casing_rename(),
        case_cross_namespace(),
        case_sql_user_customer(),
        case_drop_only(),
        case_add_only(),
        case_nested_vs_flat(),
        case_id_uuid_rename(),
    ]
}

// -----------------------------------------------------------------------
// Engine invocation helpers
// -----------------------------------------------------------------------

fn run_case(case: &CorpusCase, tier: Stringency) -> Result<AutoLensResult, LensError> {
    let protocol = generic_protocol();
    let config = AutoLensConfig {
        stringency: tier,
        ..Default::default()
    };
    auto_generate(&case.src, &case.tgt, &protocol, &config)
}

/// Quality of a result: Ok quality, or 0.0 on Err.
fn quality_of(result: &Result<AutoLensResult, LensError>) -> f64 {
    match result {
        Ok(r) => r.alignment_quality,
        Err(_) => 0.0,
    }
}

fn assert_matches(
    case: &CorpusCase,
    tier: Stringency,
    expected: ExpectedOutcome,
    result: &Result<AutoLensResult, LensError>,
) {
    match expected {
        ExpectedOutcome::AlignsWithQualityAtLeast(q) => {
            let actual = match result {
                Ok(r) => r.alignment_quality,
                Err(e) => panic!(
                    "case `{}` at {tier:?}: expected alignment (quality >= {q}) but got error: {e}",
                    case.name
                ),
            };
            assert!(
                actual >= q,
                "case `{}` at {tier:?}: quality {actual} < expected floor {q}",
                case.name,
            );
        }
        ExpectedOutcome::Fails => {
            let q = quality_of(result);
            assert!(
                result.is_err() || q == 0.0,
                "case `{}` at {tier:?}: expected failure, but got Ok with quality {q}",
                case.name,
            );
        }
    }
}

fn expectation(case: &CorpusCase, tier: Stringency) -> Option<ExpectedOutcome> {
    match tier {
        Stringency::Strict => case.expected_morphism_at_strict,
        Stringency::Balanced => case.expected_morphism_at_balanced,
        Stringency::Lenient => case.expected_morphism_at_lenient,
        Stringency::Exploratory => case.expected_morphism_at_exploratory,
    }
}

// -----------------------------------------------------------------------
// Per-tier baseline tests
// -----------------------------------------------------------------------

#[test]
#[ignore = "diagnostic utility: run with --ignored when debugging corpus scores"]
fn dump_all_qualities() {
    let cases = all_cases();
    for case in &cases {
        for tier in [
            Stringency::Strict,
            Stringency::Balanced,
            Stringency::Lenient,
            Stringency::Exploratory,
        ] {
            let r = run_case(case, tier);
            let q = quality_of(&r);
            let ok = r.is_ok();
            eprintln!("case={} tier={tier:?} ok={ok} quality={q}", case.name);
        }
    }
}

#[test]
fn corpus_strict_baseline() {
    let cases = all_cases();
    let mut checked = 0usize;
    for case in &cases {
        let expected = expectation(case, Stringency::Strict)
            .unwrap_or_else(|| panic!("case `{}` has no Strict expectation", case.name));
        let result = run_case(case, Stringency::Strict);
        assert_matches(case, Stringency::Strict, expected, &result);
        checked += 1;
    }
    assert_eq!(checked, cases.len(), "every case must be exercised");
}

#[test]
fn corpus_balanced_improvements() {
    let cases = all_cases();
    let mut checked = 0usize;
    for case in &cases {
        let expected = expectation(case, Stringency::Balanced)
            .unwrap_or_else(|| panic!("case `{}` has no Balanced expectation", case.name));
        let result = run_case(case, Stringency::Balanced);
        assert_matches(case, Stringency::Balanced, expected, &result);
        checked += 1;
    }
    assert_eq!(checked, cases.len());
}

#[test]
fn corpus_lenient_improvements() {
    // Lenient currently shares Balanced's engine behavior. The assertion is
    // identical in shape; a stricter expectation lands when Task 4/5/7
    // upgrade the engine.
    let cases = all_cases();
    for case in &cases {
        let expected = expectation(case, Stringency::Lenient)
            .unwrap_or_else(|| panic!("case `{}` has no Lenient expectation", case.name));
        let result = run_case(case, Stringency::Lenient);
        assert_matches(case, Stringency::Lenient, expected, &result);
    }
}

#[test]
fn corpus_exploratory_improvements() {
    let cases = all_cases();
    for case in &cases {
        let expected = expectation(case, Stringency::Exploratory)
            .unwrap_or_else(|| panic!("case `{}` has no Exploratory expectation", case.name));
        let result = run_case(case, Stringency::Exploratory);
        assert_matches(case, Stringency::Exploratory, expected, &result);
    }
}

// -----------------------------------------------------------------------
// Monotonicity: a higher tier never regresses below a lower tier.
// -----------------------------------------------------------------------

fn assert_not_worse(
    case: &CorpusCase,
    lower_tier: Stringency,
    higher_tier: Stringency,
    lower: &Result<AutoLensResult, LensError>,
    higher: &Result<AutoLensResult, LensError>,
) {
    let lower_q = quality_of(lower);
    let higher_q = quality_of(higher);
    // If lower fails, higher may fail too; that's not a regression.
    // If lower succeeds, higher must match or exceed its quality (modulo
    // the per-case documented tolerance).
    if lower.is_ok() {
        assert!(
            higher.is_ok(),
            "case `{}`: {lower_tier:?} succeeded (q={lower_q}) but {higher_tier:?} failed",
            case.name,
        );
        let tol = case.monotonicity_tolerance;
        assert!(
            higher_q + tol + 1e-9 >= lower_q,
            "case `{}`: {higher_tier:?} quality {higher_q} < {lower_tier:?} quality {lower_q} \
             (tolerance {tol}); this is a regression that must be justified by a documented \
             tolerance on the case",
            case.name,
        );
    }
}

#[test]
fn corpus_balanced_dominates_strict() {
    let cases = all_cases();
    for case in &cases {
        let strict = run_case(case, Stringency::Strict);
        let balanced = run_case(case, Stringency::Balanced);
        assert_not_worse(
            case,
            Stringency::Strict,
            Stringency::Balanced,
            &strict,
            &balanced,
        );
    }
}

#[test]
fn corpus_lenient_dominates_balanced() {
    // Lenient currently shares the Balanced engine, so the quality must
    // equal Balanced's; any later strategies we add can only widen the
    // search and therefore may not degrade quality.
    let cases = all_cases();
    for case in &cases {
        let balanced = run_case(case, Stringency::Balanced);
        let lenient = run_case(case, Stringency::Lenient);
        assert_not_worse(
            case,
            Stringency::Balanced,
            Stringency::Lenient,
            &balanced,
            &lenient,
        );
    }
}

#[test]
fn corpus_exploratory_dominates_lenient() {
    let cases = all_cases();
    for case in &cases {
        let lenient = run_case(case, Stringency::Lenient);
        let exploratory = run_case(case, Stringency::Exploratory);
        assert_not_worse(
            case,
            Stringency::Lenient,
            Stringency::Exploratory,
            &lenient,
            &exploratory,
        );
    }
}

// -----------------------------------------------------------------------
// Explanation snapshot: case (b) pure_rename_text_body must produce an
// alias-strategy anchor with a human-readable explanation at Balanced.
// -----------------------------------------------------------------------

#[test]
fn balanced_emits_alias_explanation_for_pure_rename() {
    let case = case_pure_rename_text_body();
    let result = run_case(&case, Stringency::Balanced)
        .expect("pure_rename_text_body should succeed at Balanced");

    // Collect only alias-driven anchors, sorted for stability.
    let mut alias_expl: Vec<String> = result
        .seed_anchors
        .iter()
        .filter(|a| matches!(a.strategy, panproto_mig::align::StrategyTag::Alias))
        .map(|a| a.explanation.clone())
        .collect();
    alias_expl.sort();

    assert!(
        !alias_expl.is_empty(),
        "pure_rename_text_body at Balanced should surface at least one Alias-tagged anchor; \
         got anchors: {:?}",
        result
            .seed_anchors
            .iter()
            .map(|a| (
                a.strategy,
                a.src.as_str().to_owned(),
                a.tgt.as_str().to_owned()
            ))
            .collect::<Vec<_>>(),
    );

    insta::assert_yaml_snapshot!("pure_rename_text_body_alias_explanations", alias_expl);
}
