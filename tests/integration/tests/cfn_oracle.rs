//! The brute-force oracle, checked before anything is checked against it.
//!
//! [`brute_force`] is the measuring instrument a search path will be compared
//! to, so a bug in it is a bug that makes every later comparison meaningless.
//! It is deliberately written to share nothing with the solver beyond the
//! network's accessors and [`Cfn::evaluate`]: it decodes each domain with its
//! own bit loop rather than through [`Domain`]'s iterator and walks the product
//! space with its own odometer.
//!
//! This file checks it against a second enumeration written the other way
//! round, through `Domain::iter` and `Assignment`, so the two walks agree only
//! if both are right. What they cannot check between them is
//! [`Cfn::evaluate`]'s own indexing, which both consult; that is pinned
//! directly by `solve::cfn`'s arity-three table index test.
//!
//! # The constant term
//!
//! Every network [`build_cfn`](panproto_mig::solve::build::build_cfn) produces
//! has `c_∅ = ⊥`, because both vacuous components read their best value and a
//! component at its best value costs nothing. So no generated instance can
//! notice a scorer that dropped `c_∅` altogether, and `c_∅` is exactly the term
//! soft local consistency will later raise to carry the certified lower bound.
//! [`the_constant_term_shifts_the_optimum`] is the hand-built instance that
//! covers it.

// A test binary is test code throughout, so the panicking helpers the
// workspace denies in library code are the right spelling here: a failed
// setup step should abort the case rather than be threaded through a
// `Result`. This is the convention every integration test in this crate
// follows.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use panproto_gat::Name;
use panproto_integration::arb_small_cfn_instance;
use panproto_mig::solve::cfn::CfnBuilder;
use panproto_mig::solve::oracle::{MAX_ORACLE_ASSIGNMENTS, assignment_count, brute_force};
use panproto_mig::{Assignment, Cfn, Cost, DEFAULT_WEIGHTS, ValId, VarId};
use proptest::prelude::*;

/// Every total assignment of a network, walked through `Domain::iter`.
///
/// The oracle deliberately avoids that iterator, so this enumeration and the
/// oracle's share no domain-walking code.
fn every_assignment(cfn: &Cfn) -> Vec<Assignment> {
    let mut out = vec![Vec::new()];
    for index in 0..cfn.n_variables() {
        let Ok(raw) = u32::try_from(index) else {
            return Vec::new();
        };
        let Some(domain) = cfn.domain(VarId::new(raw)) else {
            return Vec::new();
        };
        let mut next = Vec::with_capacity(out.len() * domain.len());
        for partial in &out {
            for value in domain {
                let mut extended = partial.clone();
                extended.push(value);
                next.push(extended);
            }
        }
        out = next;
    }
    out.into_iter().map(Assignment::from_values).collect()
}

/// The optimum and every argmin, found by the independent walk.
///
/// `⊤` means infeasible and is left out of the argmin set, which is the reading
/// the oracle documents.
fn independent_minimum(cfn: &Cfn) -> (Cost, Vec<Assignment>) {
    let mut best = Cost::TOP_SENTINEL;
    let mut argmins = Vec::new();
    for assignment in every_assignment(cfn) {
        let cost = cfn.evaluate(&assignment);
        if cost == Cost::TOP_SENTINEL {
            continue;
        }
        if cost < best {
            best = cost;
            argmins.clear();
        }
        if cost == best {
            argmins.push(assignment);
        }
    }
    (best, argmins)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// The oracle's answer is the answer a second enumeration reaches.
    #[test]
    fn the_oracle_agrees_with_an_independent_enumeration(
        (_, _, _, _, cfn) in arb_small_cfn_instance()
    ) {
        prop_assert!(assignment_count(&cfn) <= MAX_ORACLE_ASSIGNMENTS);

        let (best, argmins) = brute_force(&cfn);
        let (expected, expected_argmins) = independent_minimum(&cfn);

        prop_assert_eq!(best, expected, "the optimum");
        prop_assert_eq!(argmins.len(), expected_argmins.len(), "the argmin count");

        // Every network over a schema pair has the all-`⊥` assignment, so the
        // optimum is finite and there is at least one argmin.
        prop_assert!(best != Cost::TOP_SENTINEL, "no feasible assignment");
        prop_assert!(!argmins.is_empty());

        // Each argmin really attains the reported optimum, which is the shape
        // of the anytime contract's fourth guarantee.
        for argmin in &argmins {
            prop_assert_eq!(cfn.evaluate(argmin), best);
            prop_assert!(expected_argmins.contains(argmin));
        }

        // And nothing outside the set beats it.
        prop_assert!(
            every_assignment(&cfn)
                .iter()
                .all(|a| cfn.evaluate(a) >= best),
            "an assignment beat the reported optimum"
        );
    }
}

/// A non-zero `c_∅` shifts every score by exactly itself.
///
/// No network the schema builder produces has one, so this is the only place
/// the constant is anything but `⊥`. When soft local consistency starts raising
/// `c_∅` as the certified lower bound, this is the term the scorer must add
/// back, and a search path that forgot it would still agree with the oracle on
/// every generated instance.
#[test]
fn the_constant_term_shifts_the_optimum() {
    let variables = vec![
        (Name::from("a"), vec![Name::from("x"), Name::from("y")]),
        (Name::from("b"), vec![Name::from("p")]),
    ];
    let unary_a = [Cost::from_raw(5), Cost::from_raw(2), Cost::from_raw(9)];
    let unary_b = [Cost::from_raw(1), Cost::from_raw(5)];

    let mut without = CfnBuilder::new(variables.clone(), DEFAULT_WEIGHTS).expect("builder");
    without
        .add_unary_table(VarId::new(0), &unary_a)
        .expect("unary a");
    without
        .add_unary_table(VarId::new(1), &unary_b)
        .expect("unary b");
    let without = without.build();

    let mut with = CfnBuilder::new(variables, DEFAULT_WEIGHTS).expect("builder");
    with.add_empty(Cost::from_raw(1_000));
    with.add_unary_table(VarId::new(0), &unary_a)
        .expect("unary a");
    with.add_unary_table(VarId::new(1), &unary_b)
        .expect("unary b");
    let with = with.build();

    let (bare, bare_argmins) = brute_force(&without);
    let (shifted, shifted_argmins) = brute_force(&with);

    // `y` then `p`: 2 + 1 = 3, and 1003 with the constant.
    assert_eq!(bare, Cost::from_raw(3));
    assert_eq!(shifted, Cost::from_raw(1_003));
    assert_eq!(
        bare_argmins,
        vec![Assignment::from_values(vec![
            ValId::real(1),
            ValId::real(0)
        ])]
    );
    // The constant moves every score by the same amount, so the argmin set is
    // untouched and only the reported cost moves.
    assert_eq!(bare_argmins, shifted_argmins);
    assert_eq!(with.c_empty(), Cost::from_raw(1_000));
    assert_eq!(without.c_empty(), Cost::BOT);

    // And the independent walk reaches the same pair of answers.
    assert_eq!(independent_minimum(&without).0, bare);
    assert_eq!(independent_minimum(&with).0, shifted);
}
