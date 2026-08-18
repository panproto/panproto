//! What happens past the budget, which must not be nothing.
//!
//! Refusing exact inference is a decision the dispatcher takes on a measured
//! number, and the search it takes instead used to be unbounded in every way
//! that matters: its ceiling was ten million nodes, a node of a large network
//! costs what a whole small network costs, and reaching a node on the frontier
//! replays the decisions that reach it. On an eight hundred leaf star that came
//! to over nine minutes of processor time with no answer, which is worse than a
//! refusal, because a caller cannot tell a slow answer from no answer.
//!
//! So the operation budget binds both paths. It prices exact inference before
//! anything is allocated, and it is charged against the search that replaces
//! it as that search filters, so the work either path may do is the same
//! number. Three things follow, and each is asserted below:
//!
//! 1. a search past the budget **stops**;
//! 2. it **says so**, in [`SolveOutcome::limit_hit`] and in the warning naming
//!    what exact inference was priced at; and
//! 3. it stops in the **same place every time**, because the count is of
//!    operations performed rather than of seconds elapsed.
//!
//! The third is why the ceiling is not a wall clock.
//! [`SearchBudget::max_millis`] would bound the run just as well and its own
//! documentation says what it costs: a result that depends on the machine it
//! ran on. Nothing here reads a clock, and the equality in
//! [`the_same_network_stops_in_the_same_place`] is the property that would
//! break if anything did.

#![allow(clippy::expect_used)]

use std::time::{Duration, Instant};

use panproto_mig::DEFAULT_WEIGHTS;
use panproto_mig::hom_search::{DomainConstraints, SearchOptions};
use panproto_mig::solve::build::{NoEvidence, build_cfn};
use panproto_mig::solve::{
    Cfn, DEFAULT_MEM_BYTES, LimitKind, SearchBudget, SearchWarning, SolverPath, choose_order,
    elimination_cost, solve,
};
use panproto_protocols::raw_file;
use panproto_schema::{EdgeRule, Protocol, Schema, SchemaBuilder};

/// A liveness ceiling, not a performance one.
///
/// The measured failure it stands against is nine minutes without an answer,
/// so a bound this loose can only fail on a run that has stopped bounding
/// itself. It is deliberately far above what any assertion here needs, because
/// a tight timing bound on a shared runner fails for reasons that have nothing
/// to do with the property.
const LIVENESS: Duration = Duration::from_secs(60);

fn protocol() -> Protocol {
    Protocol {
        name: "fallback".to_owned(),
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

/// `k` objects, each carrying a property pointing at every other one.
///
/// The primal graph is a clique on `k` variables, so the induced width is
/// `k - 1` and no elimination order does better. This is the shape the
/// dispatcher's fallback is *for*: a star is width one however many leaves it
/// grows, so nothing of that shape ever needs a search.
fn clique(k: usize) -> Schema {
    let mut builder = SchemaBuilder::new(&protocol());
    for index in 0..k {
        builder = builder
            .vertex(&format!("o{index}"), "object", None::<&str>)
            .expect("vertex");
    }
    for left in 0..k {
        for right in 0..k {
            if left != right {
                builder = builder
                    .edge(
                        &format!("o{left}"),
                        &format!("o{right}"),
                        "prop",
                        Some(&format!("p{right}")),
                    )
                    .expect("edge");
            }
        }
    }
    builder.entry("o0").build().expect("build")
}

fn network(schema: &Schema) -> Cfn {
    build_cfn(
        schema,
        schema,
        &SearchOptions::default(),
        &DomainConstraints::default(),
        &NoEvidence,
        DEFAULT_WEIGHTS,
        DEFAULT_MEM_BYTES,
    )
    .expect("the fixture poses")
}

/// A file of `lines` numbered lines, parsed one vertex to the line.
fn file_network(lines: usize) -> Cfn {
    use std::fmt::Write as _;
    let text = (0..lines).fold(String::new(), |mut out, index| {
        let _ = writeln!(out, "line {index} of the file");
        out
    });
    let parsed = raw_file::parse_text(&text, "sample.txt").expect("parse");
    build_cfn(
        &parsed,
        &parsed,
        &SearchOptions::default(),
        &DomainConstraints::default(),
        &NoEvidence,
        DEFAULT_WEIGHTS,
        DEFAULT_MEM_BYTES,
    )
    .expect("a line-per-vertex parse poses")
}

/// A search that cannot finish inside the budget stops, and reports both the
/// stop and the price that sent it there.
#[test]
fn a_search_past_the_budget_stops_and_says_so() {
    let cfn = network(&clique(10));
    let (order, width) = choose_order(&cfn);
    let refused = elimination_cost(&cfn, &order);
    let budget = SearchBudget::default().with_op_budget(1_000_000);
    assert!(
        !refused.fits(&budget),
        "the fixture is meant to be past the budget"
    );

    let started = Instant::now();
    let found = solve(&cfn, &budget);
    let elapsed = started.elapsed();

    assert!(matches!(found.path, SolverPath::BranchAndBound { .. }));
    assert_eq!(found.limit_hit, Some(LimitKind::Operations));
    assert!(!found.proven_optimal, "a stopped search proves nothing");
    assert!(
        elapsed < LIVENESS,
        "the fallback took {elapsed:?}, which is not a bounded search"
    );

    // The refusal carries the measurement that caused it, so a caller reading
    // the warning can see how far past the budget the network was.
    let named = found.warnings.iter().find_map(|warning| match warning {
        SearchWarning::EliminationOutOfBudget {
            width: reported,
            entries,
            operations,
        } => Some((*reported, *entries, *operations)),
        _ => None,
    });
    assert_eq!(named, Some((width, refused.entries, refused.operations)));
    assert!(refused.operations > budget.op_budget);
}

/// The budget is what stops it, so a larger one gets further.
///
/// Monotone rather than proportional: what a node costs depends on how far the
/// filtering gets before it reaches a fixpoint, and nothing here claims that is
/// uniform. What it claims is that the ceiling is the thing in control, which a
/// ceiling that changed nothing would fail.
#[test]
fn a_larger_budget_gets_further() {
    let cfn = network(&clique(10));
    let tight = solve(&cfn, &SearchBudget::default().with_op_budget(10_000));
    let loose = solve(&cfn, &SearchBudget::default().with_op_budget(10_000_000));

    assert_eq!(tight.limit_hit, Some(LimitKind::Operations));
    assert!(loose.nodes >= tight.nodes);
    assert!(
        loose.limit_hit.is_none() || loose.nodes > tight.nodes,
        "a budget ten thousand times larger neither finished nor got further"
    );
}

/// Given enough budget the same fallback finishes and proves optimality.
///
/// The stop is a budget being spent, not a search that cannot answer, and this
/// is the pair to the test above: one fixture, two budgets, two outcomes, both
/// of them stated in the outcome rather than inferred from how long it took.
#[test]
fn the_same_search_finishes_when_the_budget_allows() {
    let cfn = network(&clique(10));
    let found = solve(&cfn, &SearchBudget::default());

    assert!(matches!(found.path, SolverPath::BranchAndBound { .. }));
    assert_eq!(found.limit_hit, None);
    assert!(found.proven_optimal);
    assert!(found.best.is_some());
}

/// Two runs of one network stop at the same operation.
///
/// The whole reason the ceiling counts operations rather than milliseconds.
/// A wall-clock ceiling would make this equality hold only on an idle machine,
/// and the search's contract is that identical inputs give identical answers
/// with no such qualification.
#[test]
fn the_same_network_stops_in_the_same_place() {
    let cfn = network(&clique(10));
    let budget = SearchBudget::default().with_op_budget(2_000_000);

    let first = solve(&cfn, &budget);
    for _ in 0..2 {
        let again = solve(&cfn, &budget);
        assert_eq!(first.nodes, again.nodes);
        assert_eq!(first.limit_hit, again.limit_hit);
        assert_eq!(first.best, again.best);
        assert_eq!(first.lower_bound, again.lower_bound);
        assert_eq!(first.upper_bound, again.upper_bound);
    }
    assert_eq!(first.limit_hit, Some(LimitKind::Operations));
}

/// The star, forced past the budget: the shape that used to run for minutes.
///
/// Its exact price is *below* what one filtering pass of the search costs, so
/// pushing this network onto the fallback takes a budget small enough to stop
/// the search almost at once. That is the honest shape of the answer here: on
/// a star, exact inference is the cheap path and the search is the expensive
/// one, which is exactly what the reading this replaced got backwards.
#[test]
fn the_star_that_used_to_hang_now_returns() {
    let cfn = file_network(200);
    let (order, _) = choose_order(&cfn);
    let refused = elimination_cost(&cfn, &order);
    let budget = SearchBudget::default().with_op_budget(refused.operations / 2);

    let started = Instant::now();
    let found = solve(&cfn, &budget);
    let elapsed = started.elapsed();

    assert!(matches!(found.path, SolverPath::BranchAndBound { .. }));
    assert_eq!(found.limit_hit, Some(LimitKind::Operations));
    assert!(
        elapsed < LIVENESS,
        "the star took {elapsed:?}, which is the failure this test exists for"
    );

    // And at the shipped budget the same network never reaches the search at
    // all, because its elimination costs a fraction of a percent of it.
    let whole = solve(&cfn, &SearchBudget::default());
    assert!(matches!(whole.path, SolverPath::Eliminate { width: 1 }));
    assert!(whole.proven_optimal);
}
