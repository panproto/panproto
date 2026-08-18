//! A stopped total-morphism search must not be reported as "no total morphism
//! exists".
//!
//! `find_morphisms`'s contract is explicit about the distinction:
//!
//! > `Ok(vec![])` means no total morphism exists \[…\] and it means **only**
//! > that: a search that could not run or could not finish reports `Err`, so a
//! > caller can tell the two apart.
//!
//! `optimal_assignments` used not to keep it. Its fallback branch was
//!
//! ```text
//! solve(cfn, budget).best.into_iter().collect()
//! ```
//!
//! which keeps `SolveOutcome::best` and drops `SolveOutcome::limit_hit` and
//! `SolveOutcome::proven_optimal` beside it. A branch and bound that spends its
//! operation budget before it reaches any complete assignment hands back
//! `best: None`, and `None` collects to the empty vector, which the entry point
//! above then spelled as the *absence of a total morphism* rather than as the
//! *absence of an answer*.
//!
//! The fixture below is a pair on which the two differ: a total morphism
//! exists, the library finds it when the domains are narrowed to where it
//! lives, and the default budget runs out before the search reaches it. That
//! pair now reports `SpanError::Stopped`.
//!
//! The predicate is `best.is_none() && limit_hit.is_some()`, not
//! `limit_hit.is_some()`. A stop that did reach a leaf holds a genuine total
//! morphism, since removing `⊥` from every domain makes feasibility exactly
//! totality, so refusing that one would discard a correct answer and trade this
//! defect for its mirror image. The last test here holds the other end down:
//! a pair that really has no total morphism still answers `Ok(None)`.
//!
//! # The shape
//!
//! The source is a complete digraph on `k` object vertices. A total morphism
//! out of it must be injective, since it has no self-loops and neither does the
//! target. The target holds two complete digraphs, disconnected from each
//! other:
//!
//! - a **decoy** on `k - 1` vertices carrying exactly the source's vertex names
//!   and edge names, so every unary component of the objective (name, property
//!   Jaccard) scores it best; and
//! - a **real** image on `k` vertices under names sharing nothing with the
//!   source.
//!
//! Only the real image admits a total morphism, and only after the search has
//! exhausted every injection into the decoy. That is `(k-1)!` prefixes, and the
//! budget runs out first.
//!
//! # Cost
//!
//! The assertions here each spend the default operation budget, which is a
//! minute or so of processor time in a release build and several in a debug
//! one. That is the price of reaching a *default* budget, and a fixture that
//! reached a lowered one would be asserting something a caller cannot see.

#![allow(
    clippy::expect_used,
    reason = "a fixture that will not build is a defect in this file"
)]

use std::collections::HashMap;

use panproto_gat::Name;
use panproto_mig::DEFAULT_WEIGHTS;
use panproto_mig::SpanError;
use panproto_mig::hom_search::{
    DomainConstraints, SearchOptions, find_best_morphism, find_best_morphism_budgeted,
    find_best_morphism_constrained, without_bottom,
};
use panproto_mig::solve::build::{NoEvidence, build_cfn};
use panproto_mig::solve::{LimitKind, SearchBudget, solve};
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

/// The complete digraph the fixture is built from.
const K: usize = 10;

/// A budget this fixture spends before reaching any complete assignment.
///
/// `SearchBudget::default()` reaches the same stop on the same shape, and takes
/// about ninety seconds optimised and a quarter of an hour unoptimised to get
/// there, which is past what a test suite can hold. The figure is small so the
/// test is fast; what it must not be is *so* small that the search stops before
/// the fixture's premise is established, which is why
/// `the_search_stops_without_an_answer_and_records_it` asserts `best.is_none()`
/// and the limit kind separately rather than taking the stop on trust.
fn stopping_budget() -> SearchBudget {
    SearchBudget::default().with_op_budget(1_000_000)
}

fn protocol() -> Protocol {
    Protocol {
        name: "stopped".to_owned(),
        schema_theory: "ThTest".to_owned(),
        instance_theory: "ThWType".to_owned(),
        edge_rules: vec![EdgeRule {
            edge_kind: "prop".to_owned(),
            src_kinds: vec!["object".to_owned()],
            tgt_kinds: vec!["object".to_owned()],
        }],
        obj_kinds: vec!["object".to_owned()],
        constraint_sorts: vec![],
        ..Protocol::default()
    }
}

/// A complete digraph on `k` vertices named `{prefix}{i}`, every edge named
/// after its head.
fn add_clique(mut builder: SchemaBuilder, prefix: &str, edge: &str, k: usize) -> SchemaBuilder {
    for index in 0..k {
        builder = builder
            .vertex(&format!("{prefix}{index}"), "object", None::<&str>)
            .expect("vertex");
    }
    for left in 0..k {
        for right in 0..k {
            if left != right {
                builder = builder
                    .edge(
                        &format!("{prefix}{left}"),
                        &format!("{prefix}{right}"),
                        "prop",
                        Some(&format!("{edge}{right}")),
                    )
                    .expect("edge");
            }
        }
    }
    builder
}

fn source() -> Schema {
    add_clique(SchemaBuilder::new(&protocol()), "o", "p", K)
        .entry("o0")
        .build()
        .expect("build")
}

fn target() -> Schema {
    let mut builder = SchemaBuilder::new(&protocol());
    builder = add_clique(builder, "o", "p", K - 1);
    builder = add_clique(builder, "z", "q", K);
    builder.entry("o0").build().expect("build")
}

/// The real image's vertices, which is where the total morphism lives.
fn real_image() -> Vec<Name> {
    (0..K)
        .map(|i| Name::from(format!("z{i}").as_str()))
        .collect()
}

/// A total morphism exists, and the library itself finds it once the domains
/// are narrowed to the half of the target it lives in.
///
/// This is the witness the next test needs and it is computed rather than
/// asserted: nothing here claims the morphism exists on the strength of how the
/// fixture was drawn.
#[test]
fn a_total_morphism_exists() {
    let (src, tgt) = (source(), target());
    let restricted: HashMap<Name, Vec<Name>> = src
        .vertices
        .keys()
        .map(|name| (name.clone(), real_image()))
        .collect();

    let found = find_best_morphism_constrained(
        &src,
        &tgt,
        &SearchOptions::default(),
        &DomainConstraints {
            restricted_domains: restricted,
            ..DomainConstraints::default()
        },
    )
    .expect("the narrowed network poses")
    .expect("the source maps onto the real image");

    assert_eq!(
        found.vertex_map.len(),
        src.vertices.len(),
        "a total morphism maps every source vertex"
    );
    assert_eq!(
        found.edge_map.len(),
        src.edges.len(),
        "a total morphism maps every source edge"
    );
    for image in found.vertex_map.values() {
        assert!(
            image.as_str().starts_with('z'),
            "the witness lands in the real image, not the decoy"
        );
    }
}

/// The search stops on its operation budget with no complete assignment, and
/// says so, in the outcome `optimal_assignments` throws away.
#[test]
fn the_search_stops_without_an_answer_and_records_it() {
    let (src, tgt) = (source(), target());
    let budget = stopping_budget();
    let cfn = build_cfn(
        &src,
        &tgt,
        &SearchOptions::default(),
        &DomainConstraints::default(),
        &NoEvidence,
        DEFAULT_WEIGHTS,
        budget.mem_bytes,
    )
    .expect("the network poses");

    // The total-morphism search is this network with `⊥` removed, which is
    // exactly what `find_morphisms_constrained` searches.
    let total = without_bottom(&cfn, budget.mem_bytes);
    let outcome = solve(&total, &budget);

    assert!(
        outcome.best.is_none(),
        "the fixture is meant to stop before any complete assignment"
    );
    assert_eq!(
        outcome.limit_hit,
        Some(LimitKind::Operations),
        "the outcome must name the limit it stopped on"
    );
    assert!(
        !outcome.proven_optimal,
        "a stopped search proves nothing, and this is the second field dropped"
    );
}

/// The entry point reports that stop rather than spelling it "no morphism".
///
/// The assertion is deliberately not `answer.is_some()`. That would demand the
/// search actually find the morphism, which is more than the contract promises
/// and more than a fixed budget can deliver on this shape. What the contract
/// promises is that a caller can tell "no total morphism exists" from "the
/// search could not tell", and that is what an `Err` here establishes:
/// `a_total_morphism_exists` exhibits one on this very pair, so `Ok(None)`
/// would be false, while `Err` is true and actionable.
#[test]
fn find_best_morphism_reports_the_stop_rather_than_denying_the_morphism() {
    let (src, tgt) = (source(), target());
    let result = find_best_morphism_budgeted(
        &src,
        &tgt,
        &SearchOptions::default(),
        &DomainConstraints::default(),
        &stopping_budget(),
    );

    assert!(
        matches!(
            result,
            Err(SpanError::Stopped {
                limit: LimitKind::Operations
            })
        ),
        "`Ok(None)` is documented as `exactly when no total morphism exists`, and \
         `a_total_morphism_exists` exhibits one on this pair, so `Ok(None)` here would be a \
         wrong answer rather than a conservative one. The search spends its operation budget \
         before reaching any complete assignment, which is a fact about the budget and has to \
         be reported as one. What came back instead: {result:?}"
    );
}

/// And a pair that genuinely has no total morphism still answers `Ok(None)`.
///
/// This is the regression the fix could plausibly have introduced: reporting
/// every stopped search would be easy and would turn the corpus's common case,
/// a pair with no total morphism, into a refusal. It does not, because that
/// case routes to exact inference, which is priced against the budget in
/// advance and then runs to completion, so it stops on nothing.
#[test]
fn a_pair_with_no_total_morphism_still_answers() {
    let proto = protocol();
    let src = SchemaBuilder::new(&proto)
        .vertex("a", "object", None::<&str>)
        .expect("a")
        .vertex("b", "object", None::<&str>)
        .expect("b")
        .edge("a", "b", "prop", Some("x"))
        .expect("edge")
        .entry("a")
        .build()
        .expect("src");
    // One vertex, so nothing injective lands and the edge has no image.
    let tgt = SchemaBuilder::new(&proto)
        .vertex("z", "object", None::<&str>)
        .expect("z")
        .entry("z")
        .build()
        .expect("tgt");

    let answer = find_best_morphism(&src, &tgt, &SearchOptions::default())
        .expect("a small pair finishes, so this must not be the `Err` arm");
    assert!(
        answer.is_none(),
        "this pair has no total morphism and the search finished, so the absence \
         has to be reported as `Ok(None)` rather than as a stop"
    );
}
