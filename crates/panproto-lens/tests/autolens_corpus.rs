//! Cross-protocol autolens corpus harness.
//!
//! Ten realistic schema pairs, searched at every [`Stringency`] tier, with the
//! shape of the answer pinned per tier and per case.
//!
//! * `corpus/generic_records/` — identity, pure structural rename, casing.
//! * `corpus/rename_cluster/`  — alias-driven field-name renames.
//! * `corpus/sql_like/`        — SQL-style snake_case rename patterns.
//! * `corpus/nested_vs_flat/`  — record flattening.
//! * `corpus/wrap_unwrap/`     — drop-only and add-only.
//!
//! The corpus is programmatically-built [`Schema`] pairs rather than on-disk
//! JSON: panproto-schema JSON serialization is brittle across protocols, and
//! the test-protocol pattern already used across the codebase is the canonical
//! way to construct them.
//!
//! # What is asserted, and against which entry point
//!
//! Two things are measured here and they are not the same thing.
//!
//! **The span.** [`ExpectedOutcome`] is stated about the optimal span, computed
//! by handing [`SpanSearch`] the tier's aggregated anchor pool as evidence.
//! This is the search itself, and it is where the monotonicity claim lives: the
//! anchor term is a reward-only unary cost, so a larger pool can change which
//! assignment is optimal and can never make one infeasible. Quality and apex
//! size are therefore monotone non-decreasing in the tier **exactly**, with no
//! tolerance, and `assert_span_dominance` asserts that.
//!
//! There is no `Fails` outcome. A span always exists, because the empty apex is
//! always feasible, so "no morphism found" is not one of the answers this
//! search can give. What varies is how much of the source survives, which is
//! what [`ExpectedOutcome::SpanWithApexAtLeast`] states.
//!
//! **The lens.** `auto_generate` is the shipped entry point and it is not the
//! same search at every tier: `Strict` and `Balanced` ask for a *total*
//! morphism and fail when none exists, while `Lenient` and `Exploratory` take
//! the span. Its quality is recorded in the `span_selection_by_case` snapshot
//! and its existence is asserted to be monotone, but no floor is stated on it,
//! because the two halves of the ladder are answering different questions.
//!
//! # The snapshot is reviewed, not accepted
//!
//! `cargo insta accept` is not appropriate on this file. Every row records a
//! selection the objective chose, so a moved row means the objective changed,
//! and which of its components moved it is a fact worth writing down. Use
//! `cargo insta review`, and run with `PP_DUMP_SELECTION=1` to get the moved
//! rows printed against the committed baseline rather than reading the diff by
//! hand.

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

use panproto_gat::TheoryTransform;
use panproto_lens::auto_lens::{
    AutoLensConfig, AutoLensResult, Stringency, auto_generate, run_strategies_for_tests,
};
use panproto_lens::error::LensError;
use panproto_mig::SchemaSpan;
use panproto_mig::align::evidence::{AggregationPolicy, aggregate};
use panproto_mig::{CostWeights, SolverPath, SpanSearch};
use panproto_schema::{Protocol, Schema, SchemaBuilder};

/// The four tiers in ascending order of what they will consider.
const TIERS: [Stringency; 4] = [
    Stringency::Strict,
    Stringency::Balanced,
    Stringency::Lenient,
    Stringency::Exploratory,
];

/// What the optimal span for a case at a tier must look like.
///
/// There is no failure variant. §4.3's corollary is that a span always exists,
/// so the question is never whether one was found but how much of the source it
/// covers and how well the covered part matches.
#[derive(Clone, Copy, Debug)]
enum ExpectedOutcome {
    /// [`SchemaSpan::quality`] is at least the given floor.
    ///
    /// The floor is a ranking signal over one source schema and has no absolute
    /// reading, so it is stated per case and never compared between cases.
    SpanWithQualityAtLeast(f64),
    /// The apex keeps at least the given number of source vertices.
    ///
    /// The statement to prefer where the interesting fact is how much survived
    /// rather than how well it matched.
    SpanWithApexAtLeast(usize),
    /// The two schemas share nothing the search can align.
    ///
    /// No case in this corpus is that far apart, so nothing constructs it
    /// today. It is stated because it is the third answer the search can give
    /// and a case that reached it would otherwise have to be written as
    /// `SpanWithApexAtLeast(0)`, which asserts nothing at all.
    #[allow(dead_code)]
    EmptyApex,
}

struct CorpusCase {
    name: &'static str,
    #[allow(dead_code)]
    protocol: String,
    src: Schema,
    tgt: Schema,
    expected_at_strict: ExpectedOutcome,
    expected_at_balanced: ExpectedOutcome,
    expected_at_lenient: ExpectedOutcome,
    expected_at_exploratory: ExpectedOutcome,
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
        expected_at_strict: ExpectedOutcome::SpanWithQualityAtLeast(0.9),
        expected_at_balanced: ExpectedOutcome::SpanWithQualityAtLeast(0.9),
        expected_at_lenient: ExpectedOutcome::SpanWithQualityAtLeast(0.9),
        expected_at_exploratory: ExpectedOutcome::SpanWithQualityAtLeast(0.9),
    }
}

/// (b) Pure field rename: `post.text` → `post.body`.
///
/// The alias dictionary recognises `text ≡ body` from Balanced up, but the span
/// is the same at Strict: with one string property on each side there is one
/// kind-compatible assignment and the search does not need the alias to find
/// it. What the alias buys is the explanation, which
/// `balanced_emits_alias_explanation_for_pure_rename` pins.
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
        expected_at_strict: ExpectedOutcome::SpanWithQualityAtLeast(0.3),
        expected_at_balanced: ExpectedOutcome::SpanWithQualityAtLeast(0.3),
        expected_at_lenient: ExpectedOutcome::SpanWithQualityAtLeast(0.3),
        expected_at_exploratory: ExpectedOutcome::SpanWithQualityAtLeast(0.3),
    }
}

/// (c) Temporal rename: `createdAt` ↔ `sentAt`.
///
/// The alignment the case is named for is **not** the one the objective picks.
/// It sends both `post.text` and `post.createdAt` to `message.sentAt` and
/// leaves `message.body` unused. Neither source name shares a token with either
/// target name, so the name components of the objective score every assignment
/// here identically, the choice is a tie, and the tie-break takes it.
///
/// The alias cluster does propose `createdAt ↦ sentAt`, and it reaches the
/// search as evidence rather than as a pin. Evidence is weighted by
/// [`W_ANCHOR`](panproto_mig::align::defaults::W_ANCHOR), which ships at zero,
/// so it cannot break the tie it exists to break. That is a deliberate shipping
/// state rather than a defect in the search, and this row is where it is
/// visible: raising the anchor weight is what would move it.
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
        expected_at_strict: ExpectedOutcome::SpanWithQualityAtLeast(0.2),
        expected_at_balanced: ExpectedOutcome::SpanWithQualityAtLeast(0.2),
        expected_at_lenient: ExpectedOutcome::SpanWithQualityAtLeast(0.2),
        expected_at_exploratory: ExpectedOutcome::SpanWithQualityAtLeast(0.2),
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
        expected_at_strict: ExpectedOutcome::SpanWithQualityAtLeast(0.3),
        expected_at_balanced: ExpectedOutcome::SpanWithQualityAtLeast(0.3),
        expected_at_lenient: ExpectedOutcome::SpanWithQualityAtLeast(0.3),
        expected_at_exploratory: ExpectedOutcome::SpanWithQualityAtLeast(0.3),
    }
}

/// (e) Cross-namespace records: different root names, children alias.
///
/// As with the temporal case, the objective picks the crossed assignment,
/// sending `entry.content` to `note.createdAt` and `entry.timestamp` to
/// `note.body`. The two source properties are both strings, neither shares a
/// token with either target, and the alias evidence that would separate them is
/// weighted at zero, so the two assignments cost the same and the tie-break
/// decides.
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
        expected_at_strict: ExpectedOutcome::SpanWithQualityAtLeast(0.2),
        expected_at_balanced: ExpectedOutcome::SpanWithQualityAtLeast(0.2),
        expected_at_lenient: ExpectedOutcome::SpanWithQualityAtLeast(0.2),
        expected_at_exploratory: ExpectedOutcome::SpanWithQualityAtLeast(0.2),
    }
}

/// (f) SQL-style snake_case rename: `user_id` ↔ `customer_id`.
///
/// The alias dictionary has no `user ≡ customer` cluster, so the alignment is
/// carried by `orders.id` matching exactly and `orders.user_id` matching
/// `orders.customer_id` on everything but its name. This case used to record a
/// Strict → Balanced quality drop of 0.6565 to 0.6452, because Balanced's extra
/// alias anchors collapsed a domain onto a worse target. All four tiers now
/// return the same span.
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
        expected_at_strict: ExpectedOutcome::SpanWithQualityAtLeast(0.5),
        expected_at_balanced: ExpectedOutcome::SpanWithQualityAtLeast(0.5),
        expected_at_lenient: ExpectedOutcome::SpanWithQualityAtLeast(0.5),
        expected_at_exploratory: ExpectedOutcome::SpanWithQualityAtLeast(0.5),
    }
}

/// (g) Drop-only: src has an extra field absent in tgt.
///
/// Nothing is dropped. The apex keeps all three source vertices and the span
/// sends both `post.text` and `post.extra` to `post.text`, which is a
/// many-to-one homomorphism rather than a partial map. A collapse costs less
/// here than a drop does, so the objective prefers it, and the case exercises
/// the collapse rather than the drop it is named for.
///
/// The drop path is reached when the target has no vertex of the source's kind
/// at all, which is what `lenient_orphan_source_sort_emits_drop_sort` sets up
/// and what makes it the test that actually covers `DropSort`.
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
        expected_at_strict: ExpectedOutcome::SpanWithQualityAtLeast(0.5),
        expected_at_balanced: ExpectedOutcome::SpanWithQualityAtLeast(0.5),
        expected_at_lenient: ExpectedOutcome::SpanWithQualityAtLeast(0.5),
        expected_at_exploratory: ExpectedOutcome::SpanWithQualityAtLeast(0.5),
    }
}

/// (h) Add-only: tgt has an extra field.
///
/// The mirror of the drop-only case, and the asymmetric one: a span says what
/// the *source* keeps, so the whole source survives and the apex is total. The
/// target's extra field is the right leg's business, and it is carried by a
/// default rather than by a left-leg drop.
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
        expected_at_strict: ExpectedOutcome::SpanWithQualityAtLeast(0.5),
        expected_at_balanced: ExpectedOutcome::SpanWithQualityAtLeast(0.5),
        expected_at_lenient: ExpectedOutcome::SpanWithQualityAtLeast(0.5),
        expected_at_exploratory: ExpectedOutcome::SpanWithQualityAtLeast(0.5),
    }
}

/// (i) Nested vs. flat: `reply.parent.uri` etc. vs. `reply.parentUri`.
///
/// The apex keeps three of the four source vertices and **none** of the three
/// source edges: `reply.parent` is the one vertex the flat target has nothing
/// to match, and every source edge has it as an endpoint. So the alignment is
///
///     reply            ↦ reply
///     reply.parent.uri ↦ reply.parentUri
///     reply.parent.cid ↦ reply.parentCid
///
/// over three isolated vertices, with the nesting itself dropped. That is the
/// honest reading of a structure change this size, and a caller who wants the
/// nesting preserved writes the flatten lens rather than accepting this.
///
/// The expectation is stated as apex size rather than as a quality floor
/// because what is interesting here is how much of the nesting survived. It is
/// stated at every tier, including the two where `auto_generate` refuses: a
/// span exists whatever the tier, and the refusal is the *total*-morphism entry
/// point saying no total morphism exists, which is true and is a different
/// claim.
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
        expected_at_strict: ExpectedOutcome::SpanWithApexAtLeast(3),
        expected_at_balanced: ExpectedOutcome::SpanWithApexAtLeast(3),
        expected_at_lenient: ExpectedOutcome::SpanWithApexAtLeast(3),
        expected_at_exploratory: ExpectedOutcome::SpanWithApexAtLeast(3),
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
        expected_at_strict: ExpectedOutcome::SpanWithQualityAtLeast(0.3),
        expected_at_balanced: ExpectedOutcome::SpanWithQualityAtLeast(0.3),
        expected_at_lenient: ExpectedOutcome::SpanWithQualityAtLeast(0.3),
        expected_at_exploratory: ExpectedOutcome::SpanWithQualityAtLeast(0.3),
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

fn config_for(tier: Stringency) -> AutoLensConfig {
    AutoLensConfig {
        stringency: tier,
        ..Default::default()
    }
}

/// The optimal span for a case at a tier.
///
/// The tier reaches the search as *evidence* rather than as a domain
/// restriction: the anchor pool is aggregated into an
/// [`EvidenceTable`](panproto_mig::align::evidence::EvidenceTable) and handed
/// to [`SpanSearch`], which reads it as a reward-only unary cost. Nothing here
/// pins a vertex, so no strategy can remove a value from a domain and no tier
/// can make an assignment infeasible that a lower tier could reach. That is the
/// property `assert_span_dominance` asserts, and routing the pool any other way
/// would break it.
fn span_for(case: &CorpusCase, tier: Stringency) -> SchemaSpan {
    let protocol = generic_protocol();
    let (anchors, _) = run_strategies_for_tests(&case.src, &case.tgt, &config_for(tier));
    let table = aggregate(&anchors, AggregationPolicy::StrictPriority);
    SpanSearch::new(&protocol)
        .with_evidence(&table)
        .run(&case.src, &case.tgt)
        .unwrap_or_else(|e| {
            panic!(
                "case `{}` at {tier:?}: the span search refused, and it is documented never to \
                 refuse for want of a match: {e}",
                case.name
            )
        })
}

fn run_case(case: &CorpusCase, tier: Stringency) -> Result<AutoLensResult, LensError> {
    let protocol = generic_protocol();
    auto_generate(&case.src, &case.tgt, &protocol, &config_for(tier))
}

/// Quality of an `auto_generate` result: Ok quality, or 0.0 on Err.
fn quality_of(result: &Result<AutoLensResult, LensError>) -> f64 {
    match result {
        Ok(r) => r.alignment_quality,
        Err(_) => 0.0,
    }
}

/// Quality as an integer, which is the scale the objective is minimised on.
///
/// The comparison across tiers is made here rather than on the `f64` because
/// the objective is a fixed-point integer sum: two costs one unit apart are two
/// distinct optima that an `f64` comparison can report as equal, and a
/// tolerance introduced to paper over that would hide exactly the regressions
/// this file exists to catch.
fn quality_millis(quality: f64) -> i64 {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "quality is documented to lie in [0, 1], so the rounded product is in [0, 1000]"
    )]
    let scaled = (quality * 1000.0).round() as i64;
    scaled
}

fn assert_matches(
    case: &CorpusCase,
    tier: Stringency,
    expected: ExpectedOutcome,
    span: &SchemaSpan,
) {
    match expected {
        ExpectedOutcome::SpanWithQualityAtLeast(floor) => assert!(
            span.quality >= floor,
            "case `{}` at {tier:?}: span quality {} < expected floor {floor} (apex {} of {} \
             vertices)",
            case.name,
            span.quality,
            span.apex.vertices.len(),
            case.src.vertices.len(),
        ),
        ExpectedOutcome::SpanWithApexAtLeast(k) => assert!(
            span.apex.vertices.len() >= k,
            "case `{}` at {tier:?}: the apex kept {} of {} source vertices, below the expected {k}",
            case.name,
            span.apex.vertices.len(),
            case.src.vertices.len(),
        ),
        ExpectedOutcome::EmptyApex => assert!(
            span.apex.vertices.is_empty(),
            "case `{}` at {tier:?}: expected the two schemas to share nothing, but the apex kept \
             {} vertices",
            case.name,
            span.apex.vertices.len(),
        ),
    }
}

fn expectation(case: &CorpusCase, tier: Stringency) -> ExpectedOutcome {
    match tier {
        Stringency::Strict => case.expected_at_strict,
        Stringency::Balanced => case.expected_at_balanced,
        Stringency::Lenient => case.expected_at_lenient,
        Stringency::Exploratory => case.expected_at_exploratory,
    }
}

// -----------------------------------------------------------------------
// Per-tier baseline tests
// -----------------------------------------------------------------------

#[test]
fn dump_all_qualities() {
    // Diagnostic utility: set `PP_DUMP_CORPUS_QUALITIES=1` to print the per-case
    // span and lens readings. The default `cargo test` run is an early-return
    // no-op so there is no `#[ignore]` masking it.
    if std::env::var("PP_DUMP_CORPUS_QUALITIES").is_err() {
        return;
    }
    for case in &all_cases() {
        for tier in TIERS {
            let span = span_for(case, tier);
            let lens = run_case(case, tier);
            eprintln!(
                "case={} tier={tier:?} span_q={:.9} apex={}v/{}e of {}v optimal={} lens_ok={} \
                 lens_q={:.9}",
                case.name,
                span.quality,
                span.apex.vertices.len(),
                span.apex.edge_count(),
                case.src.vertices.len(),
                span.certificate.proven_optimal,
                lens.is_ok(),
                quality_of(&lens),
            );
        }
    }
}

#[test]
fn corpus_strict_baseline() {
    assert_tier_expectations(Stringency::Strict);
}

#[test]
fn corpus_balanced_improvements() {
    assert_tier_expectations(Stringency::Balanced);
}

#[test]
fn corpus_lenient_improvements() {
    assert_tier_expectations(Stringency::Lenient);
}

#[test]
fn corpus_exploratory_improvements() {
    assert_tier_expectations(Stringency::Exploratory);
}

/// Every case's expectation at one tier, plus the two facts every span must
/// carry whatever the case.
fn assert_tier_expectations(tier: Stringency) {
    let cases = all_cases();
    for case in &cases {
        let span = span_for(case, tier);
        assert!(
            span.certificate.proven_optimal,
            "case `{}` at {tier:?}: the search did not prove its answer optimal; it took the {:?} \
             path and reported {:?}. Every case here is a handful of vertices, so a fallback means \
             the dispatcher is wrong about what they cost",
            case.name, span.certificate.path, span.certificate.limit_hit,
        );
        assert!(
            span.certificate.legs_are_functorial,
            "case `{}` at {tier:?}: a leg of the span is not a schema morphism",
            case.name,
        );
        assert_matches(case, tier, expectation(case, tier), &span);
    }
    assert_eq!(cases.len(), 10, "every case must be exercised");
}

// -----------------------------------------------------------------------
// Monotonicity: a higher tier never returns a worse span, exactly.
// -----------------------------------------------------------------------

/// Quality and apex size are non-decreasing in the tier, with no tolerance.
///
/// There used to be four per-case tolerances here, three of them non-zero, and
/// they existed because ranking was done by enumerating the hom-set under a
/// node budget: a tier with more anchors could exhaust the budget sooner and
/// return the best found rather than the best there is. The search is now an
/// exact optimiser over a cost function network, and the anchor pool reaches it
/// only through a reward-only unary term, so the optimum cannot fall as the
/// pool grows. A tolerance would be a place for that guarantee to quietly stop
/// holding.
///
/// # What these comparisons currently are
///
/// [`span_for`] runs the search under the shipped weights, and the shipped
/// anchor weight is **zero**. The tier reaches the objective through that
/// weight and through nothing else, so all four tiers minimise one objective
/// over one feasible set and return the *same* span on every case here: the
/// comparisons below hold as equalities and cannot fail while that is true.
/// The test says so rather than reading as a monotonicity check that bites,
/// and asserts the identity directly, which is the stronger statement and the
/// one that would catch evidence acquiring a route into the objective that is
/// not the anchor weight.
///
/// `panproto-core`'s `stringency_monotonicity` is where monotonicity is
/// asserted with the anchor term weighted in, on real lexicons, and where the
/// optimum genuinely moves across the ladder.
#[test]
fn corpus_span_quality_is_monotone_across_tiers() {
    assert_eq!(
        CostWeights::default().anchor().to_bits(),
        0.0_f64.to_bits(),
        "the shipped anchor weight is no longer zero, so the tier can now reach the objective \
         and the equalities asserted below are no longer the right claim. Read the doc comment \
         and decide whether this test should assert monotonicity instead"
    );

    for case in &all_cases() {
        let spans: Vec<SchemaSpan> = TIERS.iter().map(|t| span_for(case, *t)).collect();
        for step in TIERS.iter().zip(&spans).collect::<Vec<_>>().windows(2) {
            let (lower_tier, lower) = step[0];
            let (higher_tier, higher) = step[1];
            assert!(
                quality_millis(higher.quality) >= quality_millis(lower.quality),
                "case `{}`: span quality fell from {lower_tier:?} to {higher_tier:?} ({} -> {} in \
                 thousandths). Evidence enters the objective as a reward-only unary cost, so a \
                 larger anchor pool cannot raise the optimum",
                case.name,
                quality_millis(lower.quality),
                quality_millis(higher.quality),
            );
            assert!(
                higher.apex.vertices.len() >= lower.apex.vertices.len(),
                "case `{}`: the apex shrank from {lower_tier:?} to {higher_tier:?} ({} -> {} \
                 vertices)",
                case.name,
                lower.apex.vertices.len(),
                higher.apex.vertices.len(),
            );
            // The sharp form. With the anchor weight at zero the tier cannot
            // change the objective, so the two spans are not merely ordered:
            // they are the same span. Asserting the digest is what makes this
            // fail if evidence ever reaches the search by some other route.
            assert_eq!(
                higher.certificate.apex_digest, lower.certificate.apex_digest,
                "case `{}`: the apex at {higher_tier:?} differs from the one at {lower_tier:?}, \
                 but with the anchor term weighted at zero the tier cannot reach the objective",
                case.name,
            );
            assert_eq!(
                quality_millis(higher.quality),
                quality_millis(lower.quality),
                "case `{}`: the quality at {higher_tier:?} differs from the one at \
                 {lower_tier:?} with the anchor term weighted at zero",
                case.name,
            );
        }
    }
}

/// `auto_generate` never reports a quality the search cannot justify.
///
/// Both entry points minimise the same objective over the same source, and a
/// total morphism is a span whose apex is the whole source, so whatever
/// `auto_generate` returns is a feasible point of the problem
/// [`SpanSearch`] optimises and cannot score above the optimum. An
/// `auto_generate` reading above the span's is therefore not a better answer
/// but a number computed on a different scale, which is the failure mode this
/// catches.
///
/// It is an inequality rather than an equality on purpose. The two do differ
/// today at `Exploratory` on `temporal_rename_created_sent` and
/// `cross_namespace_entry_note`, where `auto_generate` returns a feasible but
/// suboptimal alignment; the `lens=` column of the `span_selection_by_case`
/// snapshot is where that gap is recorded, so it moves visibly rather than
/// silently.
#[test]
fn auto_generate_never_beats_the_optimal_span() {
    for case in &all_cases() {
        for tier in TIERS {
            let span = span_for(case, tier);
            let Ok(lens) = run_case(case, tier) else {
                continue;
            };
            assert!(
                quality_millis(lens.alignment_quality) <= quality_millis(span.quality),
                "case `{}` at {tier:?}: auto_generate reports {} where the optimal span is {} (in \
                 thousandths). Nothing feasible can score above the optimum, so the two are \
                 reading different scales",
                case.name,
                quality_millis(lens.alignment_quality),
                quality_millis(span.quality),
            );
        }
    }
}

// -----------------------------------------------------------------------
// The selection snapshot
// -----------------------------------------------------------------------

/// What the solver path is called in a snapshot row.
fn path_name(path: SolverPath) -> &'static str {
    match path {
        SolverPath::Eliminate { .. } => "eliminate",
        SolverPath::BranchAndBound { .. } => "branch-and-bound",
        SolverPath::Monic => "monic",
        SolverPath::Iso => "iso",
    }
}

/// One row per case and tier: the span's shape, the lens reading, and the whole
/// per-source-vertex selection.
///
/// The selection is what makes this a regression net rather than a summary. Two
/// different assignments can score the same, and a change that swapped them
/// would move nothing in the numbers and everything in what the generated lens
/// does.
fn selection_rows() -> Vec<String> {
    let mut rows = Vec::new();
    for case in &all_cases() {
        for tier in TIERS {
            let span = span_for(case, tier);
            let lens = run_case(case, tier);

            let mut selection: Vec<String> = span
                .right
                .vertex_map
                .iter()
                .map(|(src, tgt)| format!("{src}->{tgt}"))
                .collect();
            selection.sort();

            let lens_reading = match &lens {
                Ok(result) => quality_millis(result.alignment_quality).to_string(),
                Err(_) => "none".to_owned(),
            };

            rows.push(format!(
                "{}  {tier:?}  apex={}v/{}e  q={}  lens={lens_reading}  optimal={}  path={}  [{}]",
                case.name,
                span.apex.vertices.len(),
                span.apex.edge_count(),
                quality_millis(span.quality),
                span.certificate.proven_optimal,
                path_name(span.certificate.path),
                selection.join(", "),
            ));
        }
    }
    rows.sort();
    rows
}

#[test]
fn span_selection_by_case() {
    insta::assert_yaml_snapshot!("span_selection_by_case", selection_rows());
}

/// The moved rows, against the committed baseline, printed rather than diffed.
///
/// Env-gated with `PP_DUMP_SELECTION=1`, following the early-return precedent
/// of `dump_all_qualities` rather than `#[ignore]`. It reads the committed
/// snapshot and reports every row whose case and tier it recognises but whose
/// reading has changed, which is the list a reviewer needs to attribute the
/// diff to a change in the objective.
#[test]
fn dump_selection_diff() {
    if std::env::var("PP_DUMP_SELECTION").is_err() {
        return;
    }

    let baseline_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/snapshots/autolens_corpus__span_selection_by_case.snap"
    );
    let baseline = std::fs::read_to_string(baseline_path).unwrap_or_default();

    // An insta yaml snapshot is a metadata header, a `---` line, and then one
    // `- ` item per row.
    let recorded: Vec<String> = baseline
        .rsplit("\n---\n")
        .next()
        .unwrap_or_default()
        .lines()
        .filter_map(|line| line.trim().strip_prefix("- "))
        .map(|item| item.trim_matches('"').replace("\\\"", "\""))
        .collect();

    // The case and tier are the first two whitespace-separated fields, and they
    // are the key: everything after them is the reading that may have moved.
    let key_of = |row: &str| -> String {
        row.split_whitespace()
            .take(2)
            .collect::<Vec<_>>()
            .join("  ")
    };
    let old: std::collections::BTreeMap<String, String> = recorded
        .iter()
        .map(|row| (key_of(row), row.clone()))
        .collect();

    let mut moved = 0usize;
    for row in selection_rows() {
        let key = key_of(&row);
        match old.get(&key) {
            Some(previous) if *previous == row => {}
            Some(previous) => {
                moved += 1;
                eprintln!("MOVED {key}\n  was {previous}\n  now {row}");
            }
            None => {
                moved += 1;
                eprintln!("NEW   {key}\n  now {row}");
            }
        }
    }
    eprintln!("{moved} of 40 rows moved against {baseline_path}");
}

// -----------------------------------------------------------------------
// Span-search chain-step assertions: drop-only and add-only cases must
// emit real DropOp/DropSort or AddOp/AddSort endofunctors at Lenient+.
// -----------------------------------------------------------------------

#[test]
fn lenient_orphan_source_sort_emits_drop_sort() {
    // Source has a kind the target lacks; Lenient span-search must
    // emit a real `DropSort` endofunctor. The corpus `drop_only` case
    // keeps all kinds on both sides (field-name drop, not sort drop)
    // so this explicit fixture drives the sort-level path.
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
    let result = auto_generate(
        &src,
        &tgt,
        &protocol,
        &AutoLensConfig {
            stringency: Stringency::Lenient,
            ..Default::default()
        },
    )
    .expect("Lenient should find a span");
    let has_drop_sort_boolean = result.chain.steps.iter().any(|step| {
        matches!(
            &step.target.transform,
            TheoryTransform::DropSort(name) if &**name == "boolean"
        )
    });
    assert!(
        has_drop_sort_boolean,
        "Lenient span must emit DropSort(boolean); chain: {:?}",
        result
            .chain
            .steps
            .iter()
            .map(|s| s.name.to_string())
            .collect::<Vec<_>>()
    );
}

#[test]
fn lenient_orphan_target_sort_emits_add_sort() {
    // Mirror: target has a kind the source lacks; factorize must emit
    // `AddSort(boolean)`.
    let src = build(
        &[("r", "record"), ("r.keep", "string")],
        &[("r", "r.keep", "prop", "keep")],
    );
    let tgt = build(
        &[("r", "record"), ("r.keep", "string"), ("r.flag", "boolean")],
        &[
            ("r", "r.keep", "prop", "keep"),
            ("r", "r.flag", "prop", "flag"),
        ],
    );
    let protocol = generic_protocol();
    let result = auto_generate(
        &src,
        &tgt,
        &protocol,
        &AutoLensConfig {
            stringency: Stringency::Lenient,
            ..Default::default()
        },
    )
    .expect("Lenient should find a span");
    let has_add_sort_boolean = result
        .chain
        .steps
        .iter()
        .any(|step| match &step.target.transform {
            TheoryTransform::AddSort { sort, .. }
            | TheoryTransform::AddSortWithDefault { sort, .. } => &*sort.name == "boolean",
            _ => false,
        });
    assert!(
        has_add_sort_boolean,
        "Lenient span must emit AddSort(boolean); chain: {:?}",
        result
            .chain
            .steps
            .iter()
            .map(|s| s.name.to_string())
            .collect::<Vec<_>>()
    );
}

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

/// Every tier finds an alignment wherever a lower tier finds one.
///
/// [`Stringency`] documents that higher tiers form a superset of lower
/// ones, and for a long time they did not. A tier runs more alignment
/// strategies than the tier below it, selection keeps one winner per
/// source vertex, and a strategy that fires only at the higher tier can
/// outrank and displace an anchor the lower tier relied on. A displaced
/// anchor that is pinned collapses its vertex's domain to the wrong
/// target, and two individually plausible pins can be jointly
/// infeasible, so the search reported no morphism: `Exploratory` failed
/// on schema pairs `Lenient` aligned.
///
/// The pinned attempt is now followed by one with the strategy pins
/// released, keeping only the anchors the caller supplied, so a
/// displaced anchor costs an attempt rather than a solution.
///
/// This asserts existence, not quality. A higher tier may well find a
/// different morphism, and a lower-quality one, because it tries a
/// different candidate first; what it must not do is find nothing.
#[test]
fn alignment_existence_is_monotone_across_tiers() {
    let tiers = [
        Stringency::Strict,
        Stringency::Balanced,
        Stringency::Lenient,
        Stringency::Exploratory,
    ];

    for case in &all_cases() {
        let aligned: Vec<bool> = tiers
            .iter()
            .map(|tier| run_case(case, *tier).is_ok())
            .collect();

        for (lower, upper) in (0..tiers.len()).zip(1..tiers.len()) {
            assert!(
                !aligned[lower] || aligned[upper],
                "case `{}` aligns at {:?} but not at {:?}; higher tiers must be a superset",
                case.name,
                tiers[lower],
                tiers[upper],
            );
        }
    }
}
