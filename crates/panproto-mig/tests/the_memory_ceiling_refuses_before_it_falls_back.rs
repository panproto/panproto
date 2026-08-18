//! Which of the two things [`SearchBudget::mem_bytes`] does happens when.
//!
//! The field reads as one thing, "bytes of message table exact inference may
//! allocate before the dispatcher falls back to search", and does two. It is
//! read at four sites, two of which refuse rather than fall back, and the one
//! that binds first is a refusal. `build_cfn` poses every network through
//! `CfnBuilder::with_mem_bytes`, which bounds the *cost* tables and checks the
//! figure before allocating anything, so a ceiling below what a pair's cost
//! tables need comes back as [`SpanError::Build`]. There is no slower answer
//! behind that refusal, because every search entry point takes an
//! already-built `&Cfn`: a network that cannot be held cannot be searched.
//!
//! The fallback reading is nonetheless true above the build floor, and the
//! measured schema corpus cannot show it. Every corpus pair poses a network of
//! induced width two, where the message tables are smaller than the cost
//! tables, so on that corpus the build ceiling always binds first and the
//! fallback is unreachable. It takes a dense pair to separate them, and the
//! second test builds one.
//!
//! Both directions are asserted, because each is the trap for the other. A
//! reader who takes the fallback for the whole behaviour would relax the build
//! ceiling to turn the refusal into a fallback, which deletes the only
//! pre-allocation guard and hands an embedded host an unbounded allocation
//! where it asked for a bound. A reader who takes the refusal as the whole
//! story would conclude the memory ceiling can never route to search, which the
//! clique refutes.

#![allow(clippy::expect_used)]

use panproto_mig::hom_search::{DomainConstraints, SearchOptions};
use panproto_mig::solve::build::{NoEvidence, build_cfn};
use panproto_mig::solve::cost::DEFAULT_WEIGHTS;
use panproto_mig::solve::{SearchBudget, SearchWarning, SolverPath, solve};
use panproto_mig::span::SpanSearch;
use panproto_mig::{SpanError, find_span};
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

/// One object kind joined to itself, so a complete digraph is well formed.
fn protocol() -> Protocol {
    Protocol {
        name: "test-mem".to_owned(),
        schema_theory: "ThTest".to_owned(),
        instance_theory: "ThWType".to_owned(),
        edge_rules: vec![EdgeRule {
            edge_kind: "link".to_owned(),
            src_kinds: vec!["object".to_owned()],
            tgt_kinds: vec!["object".to_owned()],
        }],
        obj_kinds: vec!["object".to_owned()],
        constraint_sorts: vec![],
        ..Protocol::default()
    }
}

/// `n` vertices with an edge between every ordered pair.
///
/// The primal graph of the network this poses is complete, so its induced width
/// is `n - 1` and the message tables bucket elimination would build are
/// exponential in it, while the cost tables stay quadratic. That gap is what
/// puts the fallback within reach of a ceiling the build accepts.
fn clique(prefix: &str, n: usize) -> Schema {
    let protocol = protocol();
    let mut builder = SchemaBuilder::new(&protocol);
    for index in 0..n {
        builder = builder
            .vertex(&format!("{prefix}{index}"), "object", None::<&str>)
            .expect("vertex");
    }
    for from in 0..n {
        for to in 0..n {
            if from == to {
                continue;
            }
            builder = builder
                .edge(
                    &format!("{prefix}{from}"),
                    &format!("{prefix}{to}"),
                    "link",
                    Some(format!("e{from}_{to}").as_str()),
                )
                .expect("edge");
        }
    }
    builder.build().expect("build")
}

/// A record body carrying `n` string properties, which is the corpus's shape.
fn record(prefix: &str, n: usize) -> Schema {
    let protocol = Protocol {
        edge_rules: vec![EdgeRule {
            edge_kind: "link".to_owned(),
            src_kinds: vec!["object".to_owned()],
            tgt_kinds: vec!["object".to_owned()],
        }],
        ..protocol()
    };
    let mut builder = SchemaBuilder::new(&protocol)
        .vertex(&format!("{prefix}root"), "object", None::<&str>)
        .expect("root");
    for index in 0..n {
        builder = builder
            .vertex(&format!("{prefix}root.f{index}"), "object", None::<&str>)
            .expect("field")
            .edge(
                &format!("{prefix}root"),
                &format!("{prefix}root.f{index}"),
                "link",
                Some(format!("f{index}").as_str()),
            )
            .expect("edge");
    }
    builder.build().expect("build")
}

/// Below the build floor the ceiling refuses, and the refusal names the
/// measurement rather than a decomposition it never made.
#[test]
fn a_ceiling_under_the_cost_tables_refuses_rather_than_falling_back() {
    let protocol = protocol();
    let src = record("a.", 8);
    let tgt = record("b.", 8);

    let answered = find_span(&src, &tgt, &protocol, &SearchOptions::default())
        .expect("the default ceiling holds this pair");
    assert!(
        matches!(answered.certificate.path, SolverPath::Eliminate { .. }),
        "the premise is that this pair takes the exact route at the default \
         ceiling, so that lowering the ceiling is the only change made"
    );

    let search = SpanSearch::new(&protocol).with_budget(SearchBudget::default().with_mem_bytes(1));
    let refused = search
        .run(&src, &tgt)
        .expect_err("one byte cannot hold any cost table");

    let SpanError::Build { source } = &refused else {
        panic!("the memory ceiling must surface as a build refusal, got {refused:?}");
    };
    let message = source.to_string();
    assert!(
        message.contains("bytes"),
        "the refusal must name the measurement the caller can move, got {message}"
    );
    assert!(
        !message.contains("decomposition"),
        "a top-level `build_cfn` decomposes nothing, so the message must not \
         blame one: {message}"
    );
}

/// Above the build floor the same ceiling routes to search, with `op_budget`
/// left alone. This is the half the corpus cannot exhibit.
#[test]
fn a_ceiling_over_the_cost_tables_routes_a_wide_network_to_search() {
    let protocol = protocol();
    let src = clique("a", 8);
    let tgt = clique("b", 8);

    // Find the smallest power of two the build accepts. Everything at or above
    // it is a ceiling the cost tables fit, so any fallback seen there is the
    // message tables and not the build.
    let mut floor = 1usize;
    loop {
        let search =
            SpanSearch::new(&protocol).with_budget(SearchBudget::default().with_mem_bytes(floor));
        if search.run(&src, &tgt).is_ok() {
            break;
        }
        floor = floor
            .checked_mul(2)
            .expect("the default ceiling accepts this pair, so the loop terminates");
    }

    let search =
        SpanSearch::new(&protocol).with_budget(SearchBudget::default().with_mem_bytes(floor));
    let span = search
        .run(&src, &tgt)
        .expect("the floor is by construction accepted");

    assert!(
        matches!(span.certificate.path, SolverPath::BranchAndBound { .. }),
        "at the build floor the message tables do not fit, so the dispatcher \
         must fall back, got {:?}",
        span.certificate.path
    );

    // The span carries the path but not the warnings, so the reason is read off
    // the same network posed directly. This is the same `build_cfn` call the
    // search makes, at the same ceiling.
    let cfn = build_cfn(
        &src,
        &tgt,
        &SearchOptions::default(),
        &DomainConstraints::default(),
        &NoEvidence,
        DEFAULT_WEIGHTS,
        floor,
    )
    .expect("the floor is by construction accepted");
    let outcome = solve(&cfn, &SearchBudget::default().with_mem_bytes(floor));
    assert!(
        outcome
            .warnings
            .iter()
            .any(|w| matches!(w, SearchWarning::EliminationOutOfBudget { .. })),
        "the fallback must say it was the memory ceiling that caused it, got {:?}",
        outcome.warnings
    );
    assert!(
        outcome.limit_hit.is_none(),
        "`op_budget` was left at its default and elimination wanted far less \
         than it, so the fallback is the memory ceiling talking and nothing else"
    );

    // And the same pair takes the exact route once the message tables fit, so
    // the fallback above is the ceiling talking rather than the network's shape.
    let roomy = SpanSearch::new(&protocol)
        .with_budget(SearchBudget::default().with_mem_bytes(usize::MAX >> 1));
    let exact = roomy.run(&src, &tgt).expect("no ceiling binds");
    assert!(
        matches!(exact.certificate.path, SolverPath::Eliminate { .. }),
        "with room the same network eliminates, got {:?}",
        exact.certificate.path
    );
}
