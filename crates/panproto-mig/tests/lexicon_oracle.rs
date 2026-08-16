//! Every corpus pair small enough to enumerate, checked against the true
//! optimum.
//!
//! `solver_agreement.rs` in the integration crate holds each search path to the
//! brute-force oracle on *generated* networks. A generator is not the corpus,
//! though. It produces the shapes it was written to produce, and the objective
//! reads schema facts (out-degrees, edge names, kinds, recursion points) that a
//! generator supplies uniformly and a real lexicon supplies in a heavily skewed
//! way. So the property those tests establish is "the solver is right on
//! networks of the generator's shape", and the inputs that matter are of another
//! shape entirely.
//!
//! This file closes that gap where it can be closed. For every ordered lexicon
//! pair whose network is small enough for [`brute_force`] to enumerate, it takes
//! the true minimum over every assignment and holds [`solve`] to the four claims
//! `solver_agreement.rs` makes:
//!
//! 1. the optimum the solver reports is the true minimum,
//! 2. the assignment it returns scores exactly the cost it reported, against a
//!    clone of the network taken before the solver ran,
//! 3. that assignment is one of the true argmins, and
//! 4. it is the argmin the documented tie-break names, which is the
//!    lexicographically smallest value vector read in decode order.
//!
//! # Two networks, and why the second one is the point
//!
//! [`build_cfn`] poses the **span** network, in which every variable may take
//! `⊥` and be dropped from the apex. [`without_bottom`] forbids `⊥`, which is
//! the **total-morphism** restriction and is what `find_morphisms` searches.
//! Both are checked, in that order.
//!
//! The span network is the one that had no corpus-scale oracle at all. It is
//! what [`SpanSearch::run`](panproto_mig::SpanSearch::run) minimises over, its
//! feasible set is never empty, so it always has an answer to be wrong about,
//! and its optimum is what every reported span quality is derived from. The
//! total network is checked alongside it because it is a different feasible set
//! over the same tables: `⊥` at `⊤` can make the network infeasible outright,
//! which is a case the span network never reaches and which exercises the
//! solver's handling of an empty feasible set on real inputs.
//!
//! Both networks have the same domains, since [`without_bottom`] forbids `⊥`
//! with a `⊤`-valued cost rather than by removing the value, so one
//! enumerability test decides both. That is asserted rather than assumed.
//!
//! # What this covers, and what it does not
//!
//! 2773 of the 5852 ordered pairs are enumerable at [`MAX_ORACLE_ASSIGNMENTS`],
//! which is 47%, and all seventy-seven lexicons are the source of at least one
//! of them. On those pairs the oracle scores 65 583 836 assignments across the
//! two networks.
//!
//! That fraction reads better than it is. `MAX_ORACLE_ASSIGNMENTS` is a ceiling
//! on `∏_v |D_v|`, and that product grows exponentially in the number of source
//! vertices with a kind-compatible image in the target, so the subset it admits
//! is biased in the worst available direction: a pair is enumerable roughly when
//! *most of its source vertices can be sent nowhere*, which is to say when the
//! two lexicons have little in common. The branching-variable counts the report
//! prints name that property directly, a branching variable being one with at
//! least one real target: the enumerable pairs run 1 to 16 with a median of 6,
//! the unenumerable ones 5 to 37 with a median of 11. Structural similarity is
//! what puts a pair out of reach, and structural similarity is what a migration
//! is written across. Two lexicons alike enough that aligning them is worth
//! doing are two lexicons this test says nothing about.
//!
//! The four pairs `benches/span_bench.rs` reports timings for are the concrete
//! case. Not one of them is enumerable, and three of the four have an assignment
//! count that saturates `u64`: they run 32 to 39 variables with 31 to 36 of them
//! branching, against a median of 6 branching variables among the pairs this
//! test can reach. The pairs whose *speed* is the headline claim are exactly the
//! pairs whose *answer* has no oracle.
//!
//! The bias falls harder on the total network than on the span network. Only 334
//! of the 2773 checked pairs admit any total morphism at all, and on those the
//! four claims are checked against a real argmin. On the other 2439 the
//! enumeration finds nothing feasible, so what is established there is the
//! narrower claim that the solver agrees: it returns no assignment and reports
//! `⊤`. The span network has no such shortfall, since its feasible set is never
//! empty, and it is the network the search returns.
//!
//! # Cost, and where this runs
//!
//! One enumerable pair costs up to `MAX_ORACLE_ASSIGNMENTS` evaluations of each
//! of the two networks, and `Cfn::evaluate` is linear in the cost functions, so
//! the ceiling is not a bound anyone should read as cheap. Measured on one idle
//! machine: 7.1 seconds over the whole corpus in a release build and 162 seconds
//! in a debug one. The wall time is printed on every run.
//!
//! The debug figure is what decides where this runs, since CI builds tests
//! unoptimised. It is past the sixty second threshold at which the `ci` nextest
//! profile starts reporting a test as slow, so this test is excluded from that
//! profile by `default-filter` in `.config/nextest.toml` and runs in
//! `.github/workflows/corpus-gate.yml` instead, as the full-corpus emit gates
//! do. It still runs by default for a local `cargo nextest run`.
//!
//! Set `PP_DUMP_ORACLE=1` to print the twenty most expensive enumerable pairs.

#![allow(
    clippy::expect_used,
    reason = "a malformed committed fixture should fail the test loudly"
)]

use std::time::{Duration, Instant};

use panproto_mig::solve::build::{NoEvidence, build_cfn};
use panproto_mig::solve::oracle::{MAX_ORACLE_ASSIGNMENTS, assignment_count, brute_force};
use panproto_mig::solve::solve;
use panproto_mig::{
    Assignment, Cfn, Cost, DEFAULT_WEIGHTS, Domain, DomainConstraints, SearchBudget, SearchOptions,
    ValId, VarId, without_bottom,
};

#[path = "support/lexicons.rs"]
mod lexicons;

/// How many ordered pairs seventy-seven lexicons give.
const ORDERED_PAIRS: usize = 77 * 76;

/// How many of those pairs [`brute_force`] can enumerate at
/// [`MAX_ORACLE_ASSIGNMENTS`].
///
/// Pinned so that a change which silently shrinks the checked subset fails
/// rather than passing quietly on fewer pairs. It is a property of the corpus
/// and of kind compatibility, not of the objective, so it moves only when a
/// lexicon is added or removed or when which targets a source vertex may take
/// changes. Attribute a moved number to one of those before restating it.
const ENUMERABLE_PAIRS: usize = 2773;

/// Which of the two networks a check ran on.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum Network {
    /// `⊥` forbidden: the total-morphism restriction, which `find_morphisms`
    /// searches and whose feasible set may be empty.
    Total,
    /// `⊥` available: what the span search minimises over, whose feasible set
    /// never is.
    Span,
}

impl Network {
    /// The name a disagreement is reported under.
    const fn label(self) -> &'static str {
        match self {
            Self::Total => "total",
            Self::Span => "span",
        }
    }
}

/// One property that failed, on one pair, on one network.
struct Disagreement {
    src: String,
    tgt: String,
    network: Network,
    detail: String,
}

/// What one pair's network looked like before anything was solved.
struct Shape {
    src: String,
    tgt: String,
    /// Source vertices, which is the variable count.
    variables: usize,
    /// Variables with at least one real target, so with something to decide.
    branching: usize,
    /// The largest single domain, `⊥` included, which is the enumeration's
    /// widest factor.
    max_domain: usize,
    /// `∏_v |D_v|`, saturating.
    assignments: u64,
    /// Whether that product sits at or under [`MAX_ORACLE_ASSIGNMENTS`].
    enumerable: bool,
}

/// What checking one network against the oracle established.
struct Checked {
    /// One entry per property that failed. Empty when the solver agreed.
    failures: Vec<String>,
    /// Whether the solver proved optimality on this network.
    proven: bool,
    /// Whether the oracle found any feasible assignment at all.
    feasible: bool,
}

/// The value vector read in decode order, which is the elimination order
/// backwards and so the order the tie-break is stated in.
///
/// The key is [`ValId::order_key`] and **not** the stored slot: `⊥` is stored
/// first and ordered last, so the two disagree, and it is the domain order the
/// tie-break is stated in. Comparing these vectors is then exactly the
/// documented rule: smallest under the elimination order used, values ascending
/// by target and `⊥` last.
fn decode_key(assignment: &Assignment, order: &[VarId]) -> Vec<u32> {
    order
        .iter()
        .rev()
        .filter_map(|var| assignment.get(*var).map(ValId::order_key))
        .collect()
}

/// Hold one network to the four claims, against a copy taken before the solver
/// ran.
fn check_network(cfn: &Cfn, budget: &SearchBudget) -> Checked {
    // Kept before anything runs: the scorer the claim is checked with must be
    // one the solver never touched, and so must the enumeration.
    let pristine = cfn.clone();
    let outcome = solve(cfn, budget);
    let (optimum, argmins) = brute_force(&pristine);

    let mut failures = Vec::new();
    let feasible = optimum != Cost::TOP_SENTINEL;

    // (1) The cost the solver reports is the true minimum, and it says so.
    if outcome.upper_bound != optimum {
        failures.push(format!(
            "the optimum disagrees: solver {:?}, oracle {optimum:?}",
            outcome.upper_bound
        ));
    }
    if !outcome.proven_optimal {
        failures.push(format!(
            "the solver did not prove optimality: bounds {:?}..{:?}, limit {:?}, path {:?}",
            outcome.lower_bound, outcome.upper_bound, outcome.limit_hit, outcome.path
        ));
    }
    if outcome.lower_bound != outcome.upper_bound {
        failures.push(format!(
            "the bounds did not meet: lower {:?}, upper {:?}",
            outcome.lower_bound, outcome.upper_bound
        ));
    }
    if outcome.best.is_some() != feasible {
        failures.push(format!(
            "feasibility disagrees: the solver {} an assignment and the oracle found {} argmins",
            if outcome.best.is_some() {
                "returned"
            } else {
                "returned no"
            },
            argmins.len()
        ));
    }

    if let Some(best) = &outcome.best {
        // (2) The assignment scores the reported cost against the copy.
        let scored = pristine.evaluate(best);
        if scored != outcome.upper_bound {
            failures.push(format!(
                "the returned assignment scores {scored:?} against a pristine network, not the \
                 reported {:?}",
                outcome.upper_bound
            ));
        }

        // (3) It is one of the true argmins.
        if !argmins.contains(best) {
            failures.push(format!(
                "the returned assignment is not one of the {} true argmins",
                argmins.len()
            ));
        }

        // (4) It is the one the tie-break names. The rule is stated relative to
        // the order actually used, so it is checked only where the solver
        // reports one; a component routed to search reports none and makes no
        // tie-break claim.
        if let Some(order) = &outcome.elimination_order {
            let mut keys: Vec<Vec<u32>> = argmins.iter().map(|a| decode_key(a, order)).collect();
            keys.sort();
            let got = decode_key(best, order);
            if keys.first() != Some(&got) {
                failures.push(format!(
                    "the returned assignment is not the canonical argmin: decode key {got:?}, \
                     smallest of {} is {:?}",
                    keys.len(),
                    keys.first()
                ));
            }
        }
    }

    Checked {
        failures,
        proven: outcome.proven_optimal,
        feasible,
    }
}

/// The value at the given percentile of an ascending slice.
fn percentile<T: Copy>(sorted: &[T], p: f64) -> T {
    assert!(!sorted.is_empty(), "no values were measured");
    let last = sorted.len() - 1;
    #[expect(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "an index into a slice of at most 5852 elements is exact in f64 and the product \
                  of two values in range is in range"
    )]
    let index = (p * last as f64).round() as usize;
    sorted[index]
}

/// A count as a rate in thousandths of its denominator.
///
/// Integer arithmetic throughout: a rate printed beside a count and a
/// denominator should be derivable from them by hand.
const fn per_mille(count: usize, total: usize) -> usize {
    match (count * 1000).checked_div(total) {
        Some(rate) => rate,
        None => 0,
    }
}

/// Everything one pass over the corpus accumulated.
struct Sweep {
    /// One entry per ordered pair, enumerable or not.
    shapes: Vec<Shape>,
    /// Every property that failed, on whichever pair and network produced it.
    disagreements: Vec<Disagreement>,
    /// Assignments the oracle scored, over both networks of every checked pair.
    enumerated: u64,
    /// Pairs both of whose networks were checked.
    checked: usize,
    /// Checked pairs whose total network admits no assignment at all, which is
    /// the pairs with no total morphism.
    infeasible_total: usize,
    /// Checked pairs whose span network admits none, which is documented to be
    /// impossible and is asserted to be.
    infeasible_span: usize,
    /// Checks the solver did not prove optimal.
    unproven: usize,
    /// Wall time over the whole pass.
    elapsed: Duration,
}

/// Build both networks of every ordered pair and check the enumerable ones.
fn sweep(corpus: &[lexicons::Lexicon]) -> Sweep {
    let budget = SearchBudget::default();
    let options = SearchOptions::default();
    let constraints = DomainConstraints::default();

    let mut out = Sweep {
        shapes: Vec::with_capacity(ORDERED_PAIRS),
        disagreements: Vec::new(),
        enumerated: 0,
        checked: 0,
        infeasible_total: 0,
        infeasible_span: 0,
        unproven: 0,
        elapsed: Duration::ZERO,
    };

    let started = Instant::now();
    for (i, src) in corpus.iter().enumerate() {
        for (j, tgt) in corpus.iter().enumerate() {
            if i == j {
                continue;
            }

            let span = build_cfn(
                &src.schema,
                &tgt.schema,
                &options,
                &constraints,
                &NoEvidence,
                DEFAULT_WEIGHTS,
                budget.mem_bytes,
            )
            .unwrap_or_else(|e| {
                panic!(
                    "{} -> {}: the network would not pose: {e}",
                    src.nsid, tgt.nsid
                )
            });
            let total = without_bottom(&span, budget.mem_bytes);

            let assignments = assignment_count(&span);
            assert_eq!(
                assignment_count(&total),
                assignments,
                "{} -> {}: forbidding `⊥` changed the domains, so one enumerability test no \
                 longer decides both networks",
                src.nsid,
                tgt.nsid
            );

            let domains: Vec<usize> = span
                .variable_ids()
                .filter_map(|var| span.domain(var).map(Domain::len))
                .collect();
            let shape = Shape {
                src: src.nsid.clone(),
                tgt: tgt.nsid.clone(),
                variables: span.n_variables(),
                branching: domains.iter().filter(|len| **len > 1).count(),
                max_domain: domains.iter().copied().max().unwrap_or(0),
                assignments,
                enumerable: assignments <= MAX_ORACLE_ASSIGNMENTS,
            };

            if shape.enumerable {
                for (network, cfn) in [(Network::Total, &total), (Network::Span, &span)] {
                    let result = check_network(cfn, &budget);
                    for detail in result.failures {
                        out.disagreements.push(Disagreement {
                            src: src.nsid.clone(),
                            tgt: tgt.nsid.clone(),
                            network,
                            detail,
                        });
                    }
                    if !result.proven {
                        out.unproven += 1;
                    }
                    if !result.feasible {
                        match network {
                            Network::Total => out.infeasible_total += 1,
                            Network::Span => out.infeasible_span += 1,
                        }
                    }
                }
                out.enumerated = out.enumerated.saturating_add(assignments.saturating_mul(2));
                out.checked += 1;
            }

            out.shapes.push(shape);
        }
    }
    out.elapsed = started.elapsed();
    out
}

#[test]
fn every_enumerable_lexicon_pair_agrees_with_exhaustive_enumeration() {
    let corpus = lexicons::corpus();
    let sweep = sweep(&corpus);

    assert_eq!(
        sweep.shapes.len(),
        ORDERED_PAIRS,
        "the corpus no longer gives {ORDERED_PAIRS} ordered pairs, so the counts this file states \
         are stale"
    );

    report(&sweep);
    eprintln!(
        "lexicon oracle: {} of {} checked pairs admit a total morphism and {} admit none; the \
         span network was infeasible on {}; {} checks were not proven optimal",
        sweep.checked - sweep.infeasible_total,
        sweep.checked,
        sweep.infeasible_total,
        sweep.infeasible_span,
        sweep.unproven
    );

    assert!(
        sweep.infeasible_span == 0,
        "the span network was infeasible on {} pairs, and the all-`⊥` assignment is documented to \
         be feasible in every one of them",
        sweep.infeasible_span
    );

    assert!(
        sweep.disagreements.is_empty(),
        "{} of the {} enumerable pairs disagreed with exhaustive enumeration:\n{}",
        sweep.disagreements.len(),
        sweep.checked,
        sweep
            .disagreements
            .iter()
            .map(|d| format!(
                "  {} -> {} [{}]: {}",
                d.src,
                d.tgt,
                d.network.label(),
                d.detail
            ))
            .collect::<Vec<_>>()
            .join("\n"),
    );

    assert_eq!(
        sweep.checked, ENUMERABLE_PAIRS,
        "the enumerable subset moved, so the coverage this test claims is not the coverage it was \
         written against"
    );
}

/// Print what was covered, what was not, and the structural property that
/// separates them.
fn report(sweep: &Sweep) {
    let shapes = &sweep.shapes;
    let (checked, enumerated, elapsed) = (sweep.checked, sweep.enumerated, sweep.elapsed);
    let total = shapes.len();
    let sources: std::collections::BTreeSet<&str> = shapes
        .iter()
        .filter(|s| s.enumerable)
        .map(|s| s.src.as_str())
        .collect();

    let branching = |enumerable: bool| {
        let mut values: Vec<usize> = shapes
            .iter()
            .filter(|s| s.enumerable == enumerable)
            .map(|s| s.branching)
            .collect();
        values.sort_unstable();
        values
    };
    let inside = branching(true);
    let outside = branching(false);

    eprintln!(
        "lexicon oracle: {checked} of {total} ordered pairs enumerable at {MAX_ORACLE_ASSIGNMENTS} \
         assignments ({}pm), {enumerated} assignments scored across both networks, {elapsed:?}",
        per_mille(checked, total)
    );
    eprintln!(
        "lexicon oracle: {} of 77 lexicons are the source of at least one enumerable pair",
        sources.len()
    );
    if !inside.is_empty() {
        eprintln!(
            "lexicon oracle: branching variables, enumerable pairs: min {} p50 {} p95 {} max {}",
            inside[0],
            percentile(&inside, 0.50),
            percentile(&inside, 0.95),
            inside[inside.len() - 1],
        );
    }
    if !outside.is_empty() {
        eprintln!(
            "lexicon oracle: branching variables, unenumerable pairs: min {} p50 {} p95 {} max {}",
            outside[0],
            percentile(&outside, 0.50),
            percentile(&outside, 0.95),
            outside[outside.len() - 1],
        );
    }

    if std::env::var("PP_DUMP_ORACLE").is_ok() {
        let mut ranked: Vec<&Shape> = shapes.iter().filter(|s| s.enumerable).collect();
        ranked.sort_unstable_by_key(|shape| std::cmp::Reverse(shape.assignments));
        for shape in ranked.iter().take(20) {
            eprintln!(
                "{} assignments  {} -> {}  n={} branching={} maxdom={}",
                shape.assignments,
                shape.src,
                shape.tgt,
                shape.variables,
                shape.branching,
                shape.max_domain
            );
        }
    }
}
