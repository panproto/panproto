//! Exhaustive enumeration, kept deliberately naive.
//!
//! [`brute_force`] visits every assignment the network admits, scores each one
//! with [`Cfn::evaluate`], and returns the smallest cost together with every
//! assignment attaining it. On a network of any interesting size that is
//! hopeless, which is why the enumeration is capped at
//! [`MAX_ORACLE_ASSIGNMENTS`] and why this is compiled only under `cfg(test)`
//! or the `oracle` feature. It is a measuring instrument, not a solver.
//!
//! # Why it shares no code with the solver
//!
//! An oracle exists to catch the solver being wrong. It can only do that if the
//! two disagree when the solver is wrong, so any logic they share is logic
//! neither one checks. A bug in a domain walk used by both would move both
//! answers the same way and the comparison would pass.
//!
//! So this module reuses nothing beyond the network's own accessors and the one
//! scorer. It reads [`Cfn::n_variables`] and [`Cfn::domain`], decodes each
//! domain's bits with its own loop rather than through
//! [`Domain`](super::cfn::Domain)'s iterator,
//! walks the product space with a hand-written odometer rather than through any
//! assignment builder, and computes each cost with [`Cfn::evaluate`], which
//! consults no transformed table, no propagation state, and no bound. The
//! naivety is the specification: every line here should be checkable by reading
//! it, because nothing else checks it.
//!
//! The cost is that the odometer, the bit decoding, and the running minimum are
//! written out longhand where a shared helper would be shorter. That is the
//! intended trade.
//!
//! # The `⊤` reading
//!
//! An assignment whose cost is [`Cost::TOP_SENTINEL`] is infeasible: some hard
//! constraint rejects it. Infeasible assignments are excluded from the argmin
//! set, so a network no assignment satisfies returns
//! `(Cost::TOP_SENTINEL, vec![])`, matching the shape of an outcome with no
//! `best`. Every network a schema pair produces has at least the all-`⊥`
//! assignment, so that return is reachable only from a hand-built network.
//!
//! # The order of the argmins
//!
//! The odometer runs the last variable fastest and walks each domain in the
//! domain order — real targets ascending, then `⊥` — so the returned argmins are
//! in ascending lexicographic order on the value vector, read left to right.
//! That order is decoded here rather than inherited: `⊥` is stored at slot zero,
//! so a walk that read the slots ascending would put it first and the head of
//! this list would be the wrong argmin. Since `⊥` is
//! ordered last in every domain, the first argmin is the one that prefers a
//! real image to a dropped vertex, and the alphabetically earlier target among
//! real images, at the earliest position where two argmins differ. That is the
//! tie-break relative to the variable order rather than to an elimination
//! order, so a test of the elimination-order tie-break re-sorts this set rather
//! than reading its head.

#![cfg(any(test, feature = "oracle"))]

use super::cfn::Cfn;
use super::cost::Cost;
use super::{Assignment, ValId, VarId};

/// The largest product of domain sizes [`brute_force`] will enumerate.
///
/// Chosen so that a proptest can run hundreds of cases against the oracle in
/// well under a second: at this ceiling one instance is a hundred thousand
/// evaluations of a network with a handful of variables. A generator feeding
/// the oracle keeps its instances under this bound by construction; the guard
/// exists to make a generator that stops doing so fail loudly rather than hang.
pub const MAX_ORACLE_ASSIGNMENTS: u64 = 100_000;

/// How many assignments [`brute_force`] would enumerate over this network.
///
/// The product of the domain sizes, `⊥` included, since `⊥` is a value of every
/// domain rather than an extra option beside them. Saturating, so a network far
/// past the ceiling reports `u64::MAX` rather than wrapping to a small number
/// that would slip past the guard.
///
/// A network with no variables has one assignment, the empty one. A network
/// with an empty domain has none.
#[must_use]
pub fn assignment_count(cfn: &Cfn) -> u64 {
    let count = u32::try_from(cfn.n_variables()).unwrap_or(u32::MAX);
    let mut total = 1u64;
    for index in 0..count {
        let size = cfn
            .domain(VarId::new(index))
            .map_or(0, |domain| u64::try_from(domain.len()).unwrap_or(u64::MAX));
        total = total.saturating_mul(size);
    }
    total
}

/// Exhaustively enumerate every assignment and return the true minimum
/// together with the full set of argmins.
///
/// The minimum is over all `∏_v |D_v|` assignments, with `⊥` already a member
/// of each `D_v`. Every assignment attaining it is returned, in the ascending
/// lexicographic order the module docs describe, so a tie-break rule can be
/// tested against the whole tied set rather than against one representative.
///
/// Infeasible assignments are excluded. When every assignment is infeasible the
/// result is `(Cost::TOP_SENTINEL, vec![])`.
///
/// # Panics
///
/// If the number of assignments exceeds [`MAX_ORACLE_ASSIGNMENTS`]. The panic
/// message names the actual product, so a generator that has drifted past the
/// bound reports how far past it went rather than merely that it did.
#[must_use]
pub fn brute_force(cfn: &Cfn) -> (Cost, Vec<Assignment>) {
    let total = assignment_count(cfn);
    assert!(
        total <= MAX_ORACLE_ASSIGNMENTS,
        "the oracle will not enumerate {total} assignments, above the limit of \
         {MAX_ORACLE_ASSIGNMENTS}"
    );

    let choices = domain_values(cfn);
    if choices.iter().any(Vec::is_empty) {
        return (Cost::TOP_SENTINEL, Vec::new());
    }

    // One cursor per variable, each a position in that variable's value list.
    // The odometer below only ever leaves a cursor below its list's length, so
    // indexing a list by its cursor is in range at every point it is read.
    let mut cursor = vec![0usize; choices.len()];
    let mut best = Cost::TOP_SENTINEL;
    let mut argmins: Vec<Assignment> = Vec::new();

    loop {
        let values: Vec<ValId> = cursor
            .iter()
            .zip(&choices)
            .map(|(slot, values)| values[*slot])
            .collect();
        let assignment = Assignment::from_values(values);
        let cost = cfn.evaluate(&assignment);

        if cost != Cost::TOP_SENTINEL {
            if cost < best {
                best = cost;
                argmins.clear();
                argmins.push(assignment);
            } else if cost == best {
                argmins.push(assignment);
            }
        }

        // Increment the odometer, last variable fastest. Carrying past the
        // first variable means every assignment has been visited.
        let mut position = choices.len();
        loop {
            if position == 0 {
                return (best, argmins);
            }
            position -= 1;
            cursor[position] += 1;
            if cursor[position] < choices[position].len() {
                break;
            }
            cursor[position] = 0;
        }
    }
}

/// The values of every variable's domain, in the domain order, decoded by hand.
///
/// Written as a bit test per slot rather than through
/// [`Domain`](super::cfn::Domain)'s iterator so
/// that the solver and the oracle do not walk domains through one piece of
/// code. It reproduces the order that iterator promises — reals ascending, then
/// `⊥` — because the argmin the oracle reports has to be the same argmin under
/// the same tie-break, and a decoder that walked the bits in storage order
/// would put `⊥` first. A variable the network does not have contributes an
/// empty list, which [`brute_force`] reads as an unsatisfiable network.
fn domain_values(cfn: &Cfn) -> Vec<Vec<ValId>> {
    let count = u32::try_from(cfn.n_variables()).unwrap_or(u32::MAX);
    let mut choices = Vec::with_capacity(cfn.n_variables());
    for index in 0..count {
        let mut values = Vec::new();
        if let Some(domain) = cfn.domain(VarId::new(index)) {
            let bits = domain.bits();
            let held = |slot: u32| {
                let word = (slot / u64::BITS) as usize;
                bits.get(word)
                    .is_some_and(|word| word & (1u64 << (slot % u64::BITS)) != 0)
            };
            let slots = u32::try_from(bits.len())
                .unwrap_or(u32::MAX)
                .saturating_mul(u64::BITS);
            for slot in 1..slots {
                if held(slot) {
                    values.push(ValId::from_index(slot));
                }
            }
            if held(ValId::BOTTOM.raw()) {
                values.push(ValId::BOTTOM);
            }
        }
        choices.push(values);
    }
    choices
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::solve::cfn::CfnBuilder;
    use crate::solve::cost::DEFAULT_WEIGHTS;
    use panproto_gat::Name;

    const A: VarId = VarId::new(0);
    const B: VarId = VarId::new(1);

    /// The value standing for the alphabetically `index`th target.
    fn real(index: u32) -> ValId {
        ValId::real(index)
    }

    fn cost(units: u64) -> Cost {
        Cost::from_raw(units)
    }

    fn assignment(values: &[ValId]) -> Assignment {
        Assignment::from_values(values.to_vec())
    }

    /// Two variables: `a` over targets `x` and `y`, `b` over target `p`.
    ///
    /// So `a` has slots `[x, y, ⊥]`, `b` has slots `[p, ⊥]`, and the six
    /// assignments come out in the order `(x,p) (x,⊥) (y,p) (y,⊥) (⊥,p) (⊥,⊥)`.
    fn two_variables() -> CfnBuilder {
        CfnBuilder::new(
            vec![
                (Name::new("a"), vec![Name::new("y"), Name::new("x")]),
                (Name::new("b"), vec![Name::new("p")]),
            ],
            DEFAULT_WEIGHTS,
        )
        .unwrap()
    }

    /// `n` variables, each over `targets` distinct targets and no cost at all.
    fn uniform(variables: u32, targets: u32) -> CfnBuilder {
        let spec = (0..variables)
            .map(|v| {
                (
                    Name::new(format!("v{v}")),
                    (0..targets).map(|t| Name::new(format!("t{t}"))).collect(),
                )
            })
            .collect();
        CfnBuilder::new(spec, DEFAULT_WEIGHTS).unwrap()
    }

    // -- Counting ----------------------------------------------------------

    #[test]
    fn a_network_with_no_variables_has_one_assignment() {
        let cfn = CfnBuilder::new(Vec::new(), DEFAULT_WEIGHTS)
            .unwrap()
            .build();
        assert_eq!(assignment_count(&cfn), 1);
    }

    #[test]
    fn the_assignment_count_is_the_product_of_the_domain_sizes() {
        let cfn = two_variables().build();
        // Three slots on `a`, two on `b`, `⊥` included in both.
        assert_eq!(assignment_count(&cfn), 6);
        assert_eq!(assignment_count(&uniform(3, 2).build()), 27);
        assert_eq!(assignment_count(&uniform(5, 9).build()), 100_000);
    }

    #[test]
    fn the_count_is_the_number_of_assignments_actually_visited() {
        // With no cost anywhere every assignment ties, so the argmin set is the
        // whole enumeration and its size is the count.
        let cfn = uniform(3, 2).build();
        let (best, argmins) = brute_force(&cfn);
        assert_eq!(best, Cost::BOT);
        assert_eq!(
            u64::try_from(argmins.len()).unwrap(),
            assignment_count(&cfn)
        );
    }

    // -- The minimum -------------------------------------------------------

    #[test]
    fn the_minimum_of_a_hand_computed_network_is_found() {
        let mut builder = two_variables();
        builder.add_empty(cost(3));
        builder
            .add_unary_table(A, &[cost(5), cost(2), cost(9)])
            .unwrap();
        builder.add_unary_table(B, &[cost(1), cost(5)]).unwrap();
        // Row-major over `[a, b]` with `b` fastest: only `(y, p)` is penalised.
        builder
            .add_function(
                &[A, B],
                vec![cost(0), cost(0), cost(10), cost(0), cost(0), cost(0)],
            )
            .unwrap();
        let cfn = builder.build();

        // By hand, adding `c_∅ = 3` to every row:
        //   (x, p) 3+5+1+0  =  9      (x, ⊥) 3+5+5+0  = 13
        //   (y, p) 3+2+1+10 = 16      (y, ⊥) 3+2+5+0  = 10
        //   (⊥, p) 3+9+1+0  = 13      (⊥, ⊥) 3+9+5+0  = 17
        let (best, argmins) = brute_force(&cfn);
        assert_eq!(best, cost(9));
        assert_eq!(argmins, vec![assignment(&[real(0), real(0)])]);
        assert_eq!(cfn.evaluate(&argmins[0]), best);
    }

    #[test]
    fn every_argmin_is_returned_when_the_optimum_is_tied() {
        let mut builder = two_variables();
        builder.add_empty(cost(3));
        builder
            .add_unary_table(A, &[cost(5), cost(2), cost(9)])
            .unwrap();
        builder.add_unary_table(B, &[cost(1), cost(4)]).unwrap();
        builder
            .add_function(
                &[A, B],
                vec![cost(0), cost(0), cost(10), cost(0), cost(0), cost(0)],
            )
            .unwrap();
        let cfn = builder.build();

        // Now `(x, p)` and `(y, ⊥)` both come to 9, and nothing beats them.
        let (best, argmins) = brute_force(&cfn);
        assert_eq!(best, cost(9));
        assert_eq!(
            argmins,
            vec![
                assignment(&[real(0), real(0)]),
                assignment(&[real(1), ValId::BOTTOM]),
            ]
        );
        for argmin in &argmins {
            assert_eq!(cfn.evaluate(argmin), best);
        }
    }

    #[test]
    fn the_argmins_come_out_in_ascending_order_with_bottom_last() {
        // Every assignment ties, so the argmin set is the enumeration itself
        // and its order is the odometer's.
        let cfn = two_variables().build();
        let (_, argmins) = brute_force(&cfn);
        let seen: Vec<Vec<u32>> = argmins
            .iter()
            .map(|a| a.values().iter().map(|v| v.order_key()).collect())
            .collect();
        // `⊥` sorts last in every position. Reading the sort key rather than
        // the stored slot keeps the assertion about the ordering: `⊥` is
        // stored first and ordered last, and it is the ordering the odometer
        // walks in.
        let bottom = ValId::BOTTOM.order_key();
        assert_eq!(
            seen,
            vec![
                vec![0, 0],
                vec![0, bottom],
                vec![1, 0],
                vec![1, bottom],
                vec![bottom, 0],
                vec![bottom, bottom],
            ]
        );
    }

    #[test]
    fn a_network_with_no_variables_returns_its_constant() {
        let mut builder = CfnBuilder::new(Vec::new(), DEFAULT_WEIGHTS).unwrap();
        builder.add_empty(cost(7));
        let cfn = builder.build();

        let (best, argmins) = brute_force(&cfn);
        assert_eq!(best, cost(7));
        assert_eq!(argmins, vec![assignment(&[])]);
    }

    #[test]
    fn an_infeasible_network_returns_no_argmin() {
        let mut builder = two_variables();
        builder
            .add_unary_table(A, &[Cost::TOP_SENTINEL; 3])
            .unwrap();
        let cfn = builder.build();

        let (best, argmins) = brute_force(&cfn);
        assert_eq!(best, Cost::TOP_SENTINEL);
        assert!(argmins.is_empty());
    }

    #[test]
    fn a_partly_infeasible_network_returns_only_the_feasible_optimum() {
        let mut builder = two_variables();
        // Only `a = x` survives, and `b` is free.
        builder
            .add_unary_table(A, &[cost(0), Cost::TOP_SENTINEL, Cost::TOP_SENTINEL])
            .unwrap();
        builder.add_unary_table(B, &[cost(2), cost(1)]).unwrap();
        let cfn = builder.build();

        let (best, argmins) = brute_force(&cfn);
        assert_eq!(best, cost(1));
        assert_eq!(argmins, vec![assignment(&[real(0), ValId::BOTTOM])]);
    }

    // -- The guard ---------------------------------------------------------

    #[test]
    fn the_guard_admits_a_network_exactly_at_the_ceiling() {
        // Five variables of nine targets each: ten slots apiece, 10^5 = the cap
        // exactly. A strictly increasing unary table makes slot zero the unique
        // optimum, so the argmin set stays one element rather than a hundred
        // thousand.
        let mut builder = uniform(5, 9);
        let table: Vec<Cost> = (0..10u64).map(cost).collect();
        for v in 0..5u32 {
            builder.add_unary_table(VarId::new(v), &table).unwrap();
        }
        let cfn = builder.build();
        assert_eq!(assignment_count(&cfn), MAX_ORACLE_ASSIGNMENTS);

        let (best, argmins) = brute_force(&cfn);
        assert_eq!(best, Cost::BOT);
        assert_eq!(argmins, vec![assignment(&[real(0); 5])]);
    }

    #[test]
    #[should_panic(expected = "will not enumerate 279936 assignments, above the limit of 100000")]
    fn the_guard_refuses_a_network_above_the_ceiling() {
        // Seven variables of five targets each: six slots apiece, 6^7 = 279 936.
        // The expected message pins both the actual product and the ceiling, so
        // a guard that reported neither would not satisfy this test.
        let cfn = uniform(7, 5).build();
        assert!(assignment_count(&cfn) > MAX_ORACLE_ASSIGNMENTS);
        let _ = brute_force(&cfn);
    }
}
