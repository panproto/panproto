//! Search paths, checked against the brute-force oracle.
//!
//! Each module here takes one path and holds it to the same standard: on a
//! network small enough to enumerate, the path's answer must be the oracle's
//! answer, and the assignment it returns must score, against a pristine copy of
//! the network, exactly the cost it reported.
//!
//! Modules are independent and are meant to stay that way. A path is a separate
//! body of code and its agreement with the oracle is a separate claim, so
//! nothing here is shared between them beyond the generator.

// A test binary is test code throughout, so the panicking helpers the workspace
// denies in library code are the right spelling here: a failed setup step
// should abort the case rather than be threaded through a `Result`. This is the
// convention every integration test in this crate follows.
#![allow(clippy::unwrap_used, clippy::expect_used)]

/// Exact bucket elimination: [`eliminate`] then [`decode`].
///
/// [`eliminate`]: panproto_mig::solve::elim::eliminate
/// [`decode`]: panproto_mig::solve::elim::decode
mod exact_inference {
    use panproto_integration::arb_small_cfn_instance;
    use panproto_mig::solve::elim::{
        ProductVerdict, all_optima_traced, count_solutions, decode_traced, detect_product,
        eliminate,
    };
    use panproto_mig::solve::oracle::brute_force;
    use panproto_mig::solve::order::{choose_order, induced_width, primal_graph};
    use panproto_mig::{Assignment, Cfn, Cost, ValId, VarId};
    use proptest::prelude::*;

    /// The value vector read in decode order, which is the elimination order
    /// backwards and so the order the tie-break is stated in.
    ///
    /// `ValId` orders real targets by ascending target vertex name with `⊥`
    /// last, so comparing these vectors is exactly the documented rule:
    /// smallest under the elimination order used, values ascending by target
    /// and `⊥` last.
    fn decode_key(assignment: &Assignment, order: &[VarId]) -> Vec<u32> {
        order
            .iter()
            .rev()
            .filter_map(|var| assignment.get(*var).map(ValId::raw))
            .collect()
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(512))]

        /// The exit criterion for exact inference.
        ///
        /// Four claims, in the order they build on each other. First, the
        /// optimum elimination reports is the true minimum over every
        /// assignment. Second, the assignment decode returns scores exactly
        /// that cost against a copy of the network taken before the search ran,
        /// so the reported cost is the cost of a real assignment rather than a
        /// number the search carried along. Third, it is one of the true
        /// argmins. Fourth, among the argmins it is the smallest one under the
        /// documented tie-break, so two runs over one schema pair agree on
        /// which optimum they return.
        #[test]
        fn oracle_agrees_with_eliminate(
            (_, _, _, _, cfn) in arb_small_cfn_instance()
        ) {
            // Kept before anything runs: the scorer the claim is checked with
            // must be one the search never touched.
            let pristine = cfn.clone();

            let (order, width) = choose_order(&cfn);
            let buckets = eliminate(&cfn, &order);
            let (optimum, argmins) = brute_force(&cfn);

            // (1) The reported cost is the true minimum.
            prop_assert_eq!(buckets.optimum(), optimum);

            // The width the dispatcher would have allocated against is the
            // width the sweep actually paid.
            prop_assert_eq!(buckets.width(), width);
            prop_assert_eq!(buckets.width(), induced_width(&primal_graph(&cfn), &order));

            if optimum == Cost::TOP_SENTINEL {
                prop_assert!(argmins.is_empty());
                prop_assert!(all_optima_traced(&cfn, &buckets, 4096).0.is_empty());
                return Ok(());
            }

            let (best, trace) = decode_traced(&cfn, &buckets, &order);

            // (2) The assignment scores the reported cost against the copy.
            prop_assert_eq!(pristine.evaluate(&best), optimum);

            // (3) It is one of the true argmins.
            prop_assert!(argmins.contains(&best));

            // (4) It is the smallest of them under the elimination order.
            let mut keys: Vec<Vec<u32>> =
                argmins.iter().map(|a| decode_key(a, &order)).collect();
            keys.sort();
            prop_assert_eq!(decode_key(&best, &order), keys[0].clone());

            // Decode is backtrack-free, so it never met a variable whose every
            // value was forbidden.
            prop_assert_eq!(trace.dead_ends, 0);
            prop_assert_eq!(trace.steps, cfn.n_variables());
        }

        /// Enumerating the optima produces exactly the tied set, with no dead
        /// ends and in the same order the tie-break puts them in.
        #[test]
        fn all_optima_enumerates_the_whole_argmin_set(
            (_, _, _, _, cfn) in arb_small_cfn_instance()
        ) {
            let (order, _) = choose_order(&cfn);
            let buckets = eliminate(&cfn, &order);
            let (optimum, argmins) = brute_force(&cfn);
            prop_assume!(optimum != Cost::TOP_SENTINEL);

            let (optima, trace) = all_optima_traced(&cfn, &buckets, 100_000);

            prop_assert_eq!(optima.len(), argmins.len());
            prop_assert_eq!(trace.leaves, argmins.len());
            prop_assert_eq!(trace.dead_ends, 0);
            prop_assert!(!trace.truncated);

            for optimum_found in &optima {
                prop_assert_eq!(cfn.evaluate(optimum_found), optimum);
                prop_assert!(argmins.contains(optimum_found));
            }

            let mut keys: Vec<Vec<u32>> =
                optima.iter().map(|a| decode_key(a, &order)).collect();
            let sorted = {
                let mut copy = keys.clone();
                copy.sort();
                copy
            };
            prop_assert_eq!(&keys, &sorted);
            keys.dedup();
            prop_assert_eq!(keys.len(), optima.len(), "no optimum is produced twice");
        }

        /// The count the `(Σ, ×)` sweep reports is the number of assignments
        /// the oracle finds feasible, and the product diagnostic agrees with it
        /// whenever it claims a product.
        #[test]
        fn counting_agrees_with_the_oracle(
            (_, _, _, _, cfn) in arb_small_cfn_instance()
        ) {
            let (order, _) = choose_order(&cfn);
            let counted = count_solutions(&cfn, &order);

            let feasible = every_assignment(&cfn)
                .into_iter()
                .filter(|a| cfn.evaluate(a) != Cost::TOP_SENTINEL)
                .count();
            prop_assert_eq!(counted, u128::try_from(feasible).unwrap());

            match detect_product(&cfn) {
                ProductVerdict::Product { count, .. } => prop_assert_eq!(count, counted),
                ProductVerdict::Empty { .. } => prop_assert_eq!(counted, 0),
                // A network with something genuinely forbidden has to be
                // counted rather than multiplied out, which is what the sweep
                // above just did.
                _ => (),
            }
        }
    }

    /// Every total assignment of a network, walked independently of the solver.
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
}

/// The injective paths: weighted maximum common induced sub-schema search, and
/// the counting Hall propagator the injective-morphism path filters with.
///
/// [`solve_iso`]: panproto_mig::solve::mcsplit::solve_iso
/// [`propagate_all_different`]: panproto_mig::solve::mcsplit::propagate_all_different
mod injective {
    use panproto_gat::Name;
    use panproto_integration::arb_small_cfn_instance;
    use panproto_mig::solve::mcsplit::{
        HallOutcome, TargetId, ValueIndex, arc_descriptor, propagate_all_different, solve_iso,
    };
    use panproto_mig::solve::solve_monic;
    use panproto_mig::{Assignment, Cfn, Cost, Domain, SearchBudget, SolverPath, ValId, VarId};
    use panproto_schema::Schema;
    use proptest::prelude::*;

    /// Every total assignment of a network, walked independently of the solver.
    fn every_assignment(cfn: &Cfn) -> Vec<Assignment> {
        walk(cfn, |domain| domain)
    }

    /// Every total assignment of a network with `⊥` removed from every domain,
    /// which is the total-morphism restriction.
    fn every_total_assignment(cfn: &Cfn) -> Vec<Assignment> {
        walk(cfn, |mut domain| {
            domain.remove(ValId::BOTTOM);
            domain
        })
    }

    fn walk(cfn: &Cfn, restrict: impl Fn(Domain) -> Domain) -> Vec<Assignment> {
        let mut out = vec![Vec::new()];
        for index in 0..cfn.n_variables() {
            let Ok(raw) = u32::try_from(index) else {
                return Vec::new();
            };
            let Some(domain) = cfn.domain(VarId::new(raw)).map(&restrict) else {
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

    /// The `(source vertex, target vertex)` pairs an assignment maps.
    fn pairs_of(cfn: &Cfn, assignment: &Assignment) -> Vec<(Name, Name)> {
        assignment
            .pairs()
            .filter_map(|(var, value)| {
                let variable = cfn.variable(var)?;
                let target = variable.value_name(value)?;
                Some((variable.name().clone(), target.clone()))
            })
            .collect()
    }

    /// Whether an assignment is injective and structure-reflecting.
    ///
    /// Written straight off the definition, against the two schemas: the
    /// non-dropped images are pairwise distinct, and the arcs between every
    /// ordered pair of mapped source vertices are the arcs between their
    /// images, as multisets. It shares nothing with the search, which decides
    /// the same question by interning descriptors into label classes and never
    /// compares two multisets directly.
    fn is_iso(cfn: &Cfn, src: &Schema, tgt: &Schema, assignment: &Assignment) -> bool {
        let pairs = pairs_of(cfn, assignment);
        let mut images: Vec<&Name> = pairs.iter().map(|(_, target)| target).collect();
        images.sort();
        let mapped = images.len();
        images.dedup();
        if images.len() != mapped {
            return false;
        }
        pairs.iter().all(|(first_source, first_target)| {
            pairs.iter().all(|(second_source, second_target)| {
                arc_descriptor(src, first_source, second_source)
                    == arc_descriptor(tgt, first_target, second_target)
            })
        })
    }

    /// The true optimum over the mappings `iso` admits, and every argmin.
    fn iso_oracle(cfn: &Cfn, src: &Schema, tgt: &Schema) -> (Cost, Vec<Assignment>) {
        let mut best = Cost::TOP_SENTINEL;
        let mut argmins = Vec::new();
        for assignment in every_assignment(cfn) {
            if !is_iso(cfn, src, tgt, &assignment) {
                continue;
            }
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

    /// The domains of a network with `⊥` removed.
    fn total_domains(cfn: &Cfn) -> Vec<Domain> {
        cfn.variable_ids()
            .filter_map(|var| {
                let mut domain = cfn.domain(var)?;
                domain.remove(ValId::BOTTOM);
                Some(domain)
            })
            .collect()
    }

    /// Whether an assignment gives every variable a distinct target vertex.
    fn is_injective(index: &ValueIndex, assignment: &Assignment) -> bool {
        let mut targets: Vec<TargetId> = assignment
            .pairs()
            .filter_map(|(var, value)| index.global(var, value))
            .collect();
        let mapped = targets.len();
        targets.sort_unstable();
        targets.dedup();
        targets.len() == mapped
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(512))]

        /// The exit criterion for the maximum common induced sub-schema path.
        ///
        /// Four claims, in the order they build on each other. First, the cost
        /// the search reports is the true minimum over the assignments that are
        /// injective *and* structure-reflecting, as decided by an enumeration
        /// that reads the schemas directly. Second, the assignment it returns
        /// scores exactly that cost against a copy of the network taken before
        /// the search ran, so the reported cost belongs to a real assignment.
        /// Third, it is one of the true argmins. Fourth, the search proved it:
        /// with no limit hit the two bounds meet.
        #[test]
        fn oracle_agrees_with_mcsplit(
            (_, src, tgt, _, cfn) in arb_small_cfn_instance()
        ) {
            // Kept before anything runs: the scorer the claim is checked with
            // must be one the search never touched.
            let pristine = cfn.clone();

            let outcome = solve_iso(&cfn, &src, &tgt, &SearchBudget::default()).unwrap();
            let (optimum, argmins) = iso_oracle(&cfn, &src, &tgt);

            // Dropping every source vertex is injective and reflecting
            // vacuously, so the feasible set is never empty.
            prop_assert!(optimum != Cost::TOP_SENTINEL);
            prop_assert!(!argmins.is_empty());

            // (1) The reported cost is the true minimum.
            prop_assert_eq!(outcome.upper_bound, optimum);

            let best = outcome.best.as_ref().unwrap();

            // (2) The assignment scores the reported cost against the copy.
            prop_assert_eq!(pristine.evaluate(best), outcome.upper_bound);

            // (3) It is one of the true argmins.
            prop_assert!(argmins.contains(best));
            prop_assert!(is_iso(&pristine, &src, &tgt, best));

            // (4) The search proved it.
            prop_assert!(outcome.limit_hit.is_none());
            prop_assert!(outcome.proven_optimal);
            prop_assert_eq!(outcome.lower_bound, outcome.upper_bound);
            prop_assert_eq!(outcome.path, SolverPath::Iso);
        }

        /// The same inputs give the same answer, node count and bounds.
        ///
        /// A span is content-addressed downstream, so a search that broke ties
        /// on a hash map's iteration order would produce a different apex
        /// digest from one run to the next with nothing else changed.
        /// The exit criterion for the injective search path.
        ///
        /// `solve_monic` is branch and bound with the all-different filter run
        /// at every node, so the claim is that the composition is exact, not
        /// merely that each half is. Four claims, as everywhere else. First,
        /// the cost it reports is the true minimum over the *injective*
        /// assignments, decided by enumerating every assignment and keeping the
        /// ones that give distinct targets. Second, the assignment it returns
        /// scores that cost against a copy of the network taken before the
        /// search ran. Third, it is one of the true argmins. Fourth, it is
        /// itself injective, which the filter is there to guarantee and which
        /// no cost function in the network can state.
        #[test]
        fn oracle_agrees_with_solve_monic(
            (_, _, _, _, cfn) in arb_small_cfn_instance()
        ) {
            let pristine = cfn.clone();
            let index = ValueIndex::of(&cfn);

            let outcome = solve_monic(&cfn, &SearchBudget::default());
            prop_assert_eq!(outcome.path, SolverPath::Monic);

            let mut optimum = Cost::TOP_SENTINEL;
            let mut argmins: Vec<Assignment> = Vec::new();
            for assignment in every_assignment(&cfn) {
                if !is_injective(&index, &assignment) {
                    continue;
                }
                let cost = cfn.evaluate(&assignment);
                if cost == Cost::TOP_SENTINEL {
                    continue;
                }
                if cost < optimum {
                    optimum = cost;
                    argmins.clear();
                }
                if cost == optimum {
                    argmins.push(assignment);
                }
            }

            // Dropping every source vertex is injective (`⊥` is not a target)
            // and always feasible, so there is always something to find.
            prop_assert!(!argmins.is_empty());

            // (1) The reported cost is the true minimum over injective maps.
            prop_assert_eq!(outcome.upper_bound, optimum);

            let best = outcome.best.as_ref().unwrap();

            // (2) The assignment scores the reported cost against the copy.
            prop_assert_eq!(pristine.evaluate(best), outcome.upper_bound);

            // (3) It is one of the true argmins.
            prop_assert!(argmins.contains(best));

            // (4) It is injective, and it proved its optimality.
            prop_assert!(is_injective(&index, best));
            prop_assert!(outcome.proven_optimal);
            prop_assert_eq!(outcome.lower_bound, outcome.upper_bound);
        }

        #[test]
        fn mcsplit_is_deterministic(
            (_, src, tgt, _, cfn) in arb_small_cfn_instance()
        ) {
            let first = solve_iso(&cfn, &src, &tgt, &SearchBudget::default()).unwrap();
            for _ in 0..3 {
                let again = solve_iso(&cfn, &src, &tgt, &SearchBudget::default()).unwrap();
                prop_assert_eq!(&first.best, &again.best);
                prop_assert_eq!(first.nodes, again.nodes);
                prop_assert_eq!(first.upper_bound, again.upper_bound);
                prop_assert_eq!(first.lower_bound, again.lower_bound);
            }
        }

        /// The exit criterion for the all-different propagator the injective
        /// morphism path filters with, in the total-morphism regime.
        ///
        /// The propagator is not a search, so what the oracle certifies about
        /// it is soundness in both directions, which is what a search built on
        /// it needs. Four claims. First, it only ever removes values, and the
        /// count it reports is the number it removed. Second, every injective
        /// assignment the oracle finds survives it, so no answer is pruned
        /// away. Third, in particular the cheapest one survives, so an
        /// optimising search over the filtered domains still reaches the
        /// optimum. Fourth, a reported wipeout means the oracle finds no
        /// injective assignment at all.
        ///
        /// The domains here are the total-morphism restriction, `⊥` removed
        /// from every one. That is one of the two regimes the propagator is
        /// for, and on this generator, where the source usually has more
        /// vertices than the target, it is mostly the pigeonhole: the wipeout
        /// claim is the one under load. `monic_prunes_against_a_decision`
        /// takes the other regime, where it prunes rather than fails.
        #[test]
        fn oracle_agrees_with_monic(
            (_, _, _, _, cfn) in arb_small_cfn_instance()
        ) {
            let index = ValueIndex::of(&cfn);
            let before = total_domains(&cfn);
            let mut after = before.clone();
            let outcome = propagate_all_different(&index, &mut after);

            let injective: Vec<Assignment> = every_total_assignment(&cfn)
                .into_iter()
                .filter(|assignment| is_injective(&index, assignment))
                .collect();

            // (1) Propagation only prunes, and says how much.
            for (filtered, original) in after.iter().zip(&before) {
                prop_assert_eq!(filtered.bits() & !original.bits(), 0);
            }

            match outcome {
                // (4) A wipeout is a claim the oracle has to agree with.
                HallOutcome::Wipeout => prop_assert!(injective.is_empty()),
                HallOutcome::Filtered { removed } => {
                    prop_assert_eq!(
                        removed,
                        before
                            .iter()
                            .zip(&after)
                            .map(|(original, filtered)| original.len() - filtered.len())
                            .sum::<usize>()
                    );

                    // (2) Every injective assignment survives.
                    let mut cheapest = Cost::TOP_SENTINEL;
                    let mut witness: Option<Assignment> = None;
                    for assignment in &injective {
                        for (var, value) in assignment.pairs() {
                            prop_assert!(after[var.index()].contains(value));
                        }
                        let cost = cfn.evaluate(assignment);
                        if cost != Cost::TOP_SENTINEL && cost < cheapest {
                            cheapest = cost;
                            witness = Some(assignment.clone());
                        }
                    }

                    // (3) The cheapest one in particular.
                    if let Some(witness) = witness {
                        for (var, value) in witness.pairs() {
                            prop_assert!(after[var.index()].contains(value));
                        }
                    }
                }
            }
        }

        /// The propagator in the regime a search actually calls it from: one
        /// variable decided, the rest still free to be dropped.
        ///
        /// A decided variable is a singleton that must take a target, which is
        /// a Hall set of size one, so its target leaves every other domain.
        /// That is where the propagator does its work inside search, and it is
        /// the case the total-morphism restriction above barely reaches on a
        /// generator whose sources are mostly larger than its targets.
        ///
        /// Three claims per decision, against the oracle. First, the decided
        /// target is gone from every other variable. Second, nothing else is:
        /// no value outside the Hall set is touched. Third, every injective
        /// assignment agreeing with the decision survives, `⊥` included, so
        /// the propagator has removed only what no answer could use.
        #[test]
        fn monic_prunes_against_a_decision(
            (_, _, _, _, cfn) in arb_small_cfn_instance()
        ) {
            let index = ValueIndex::of(&cfn);
            let full: Vec<Domain> = cfn
                .variable_ids()
                .filter_map(|var| cfn.domain(var))
                .collect();
            let injective: Vec<Assignment> = every_assignment(&cfn)
                .into_iter()
                .filter(|assignment| is_injective(&index, assignment))
                .collect();

            // At most eight decisions per case, taken in variable then value
            // order, so the work is bounded and the choice is deterministic.
            let decisions: Vec<(VarId, ValId)> = cfn
                .variable_ids()
                .flat_map(|var| {
                    full[var.index()]
                        .into_iter()
                        .filter(|value| !value.is_bottom())
                        .map(move |value| (var, value))
                })
                .take(8)
                .collect();

            for (decided, chosen) in decisions {
                let mut domains = full.clone();
                domains[decided.index()] = Domain::EMPTY;
                domains[decided.index()].insert(chosen);

                let filtered = propagate_all_different(&index, &mut domains);
                prop_assert_ne!(filtered, HallOutcome::Wipeout);
                let target = index.global(decided, chosen).unwrap();

                // The decision itself is left alone.
                prop_assert_eq!(domains[decided.index()].len(), 1);
                prop_assert!(domains[decided.index()].contains(chosen));

                for var in cfn.variable_ids().filter(|var| *var != decided) {
                    for value in full[var.index()] {
                        let kept = domains[var.index()].contains(value);
                        if index.global(var, value) == Some(target) {
                            // (1) The decided target is gone from everyone else.
                            prop_assert!(!kept);
                        } else {
                            // (2) And nothing else was touched.
                            prop_assert!(kept);
                        }
                    }
                }

                // (3) Every injective assignment agreeing with the decision
                // still has every one of its values.
                for assignment in &injective {
                    if assignment.get(decided) != Some(chosen) {
                        continue;
                    }
                    for (var, value) in assignment.pairs() {
                        prop_assert!(domains[var.index()].contains(value));
                    }
                }
            }
        }
    }
}

/// The search fallback: depth-first branch and bound with soft local
/// consistency maintained at every node, and the hybrid best-first wrapper that
/// turns it into an anytime algorithm with a certified lower bound.
///
/// [`solve_dfbb`]: panproto_mig::solve::dfbb::solve_dfbb
/// [`solve_hbfs`]: panproto_mig::solve::hbfs::solve_hbfs
mod search_fallback {
    use panproto_integration::arb_small_cfn_instance;
    use panproto_mig::solve::consistency::ConsistencyLevel;
    use panproto_mig::solve::dfbb::{SearchParameters, solve_dfbb};
    use panproto_mig::solve::hbfs::{HbfsParameters, solve_hbfs};
    use panproto_mig::solve::oracle::brute_force;
    use panproto_mig::{Cost, SolverPath};
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(512))]

        /// The exit criterion for the search fallback.
        ///
        /// Four claims, in the order they build on each other. First, the cost
        /// the search reports is the true minimum over every assignment.
        /// Second, the assignment it returns scores exactly that cost against a
        /// copy of the network taken before the search ran: the transformations
        /// the search maintains move cost between functions, so a solver can
        /// report a number that is right in its working copy while the
        /// assignment it hands back does not achieve that number in the
        /// original. Third, it is one of the true argmins. Fourth, the search
        /// proved it: with no limit hit the two bounds meet.
        #[test]
        fn oracle_agrees_with_dfbb(
            (_, _, _, _, cfn) in arb_small_cfn_instance()
        ) {
            // Kept before anything runs: the scorer the claim is checked with
            // must be one the search never touched.
            let pristine = cfn.clone();

            let outcome = solve_dfbb(&cfn, &SearchParameters::default());
            let (optimum, argmins) = brute_force(&cfn);

            // Dropping every source vertex is always feasible, so there is
            // always something to find.
            prop_assert!(optimum != Cost::TOP_SENTINEL);
            prop_assert!(!argmins.is_empty());

            // (1) The reported cost is the true minimum.
            prop_assert_eq!(outcome.upper_bound, optimum);

            let best = outcome.best.as_ref().unwrap();

            // (2) The assignment scores the reported cost against the copy.
            prop_assert_eq!(pristine.evaluate(best), outcome.upper_bound);

            // (3) It is one of the true argmins.
            prop_assert!(argmins.contains(best));

            // (4) The search proved it.
            prop_assert!(outcome.limit_hit.is_none());
            prop_assert!(outcome.proven_optimal);
            prop_assert_eq!(outcome.lower_bound, outcome.upper_bound);
            prop_assert_eq!(outcome.path, SolverPath::BranchAndBound { width: 0 });
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(192))]

        /// Every consistency level returns the same optimum.
        ///
        /// The levels differ in how much cost they move into the bound before a
        /// node is judged, so they differ in how many nodes the search opens.
        /// They cannot differ in the answer. This is what separates "the
        /// consistency is wrong" from "the search is wrong": a level-dependent
        /// optimum is a transformation that did not preserve the cost of every
        /// assignment, and a level-independent wrong optimum is the search.
        #[test]
        fn consistency_level_invariance(
            (_, _, _, _, cfn) in arb_small_cfn_instance()
        ) {
            let pristine = cfn.clone();
            let (optimum, argmins) = brute_force(&cfn);

            for level in ConsistencyLevel::ALL {
                let outcome =
                    solve_dfbb(&cfn, &SearchParameters::default().with_level(level));
                prop_assert_eq!(
                    outcome.upper_bound,
                    optimum,
                    "{} reported a different optimum",
                    level.label()
                );
                prop_assert!(outcome.proven_optimal, "{} did not finish", level.label());
                let best = outcome.best.as_ref().unwrap();
                prop_assert_eq!(pristine.evaluate(best), optimum);
                prop_assert!(argmins.contains(best));
            }
        }

        /// The primal bound is a strict improvement threshold, at every one of
        /// the four places an off-by-one could hide.
        ///
        /// Below the optimum and at it there is nothing to find; one above it
        /// and with no bound at all there is. The two interesting bounds are
        /// the middle two: a search that treated the bound as inclusive would
        /// return the optimum at `c*` and a search that treated it as exclusive
        /// one step too early would refuse at `c* + 1`, and neither shows up if
        /// the bound is only ever tested far from the answer.
        #[test]
        fn initial_upper_bound_sweep(
            (_, _, _, _, cfn) in arb_small_cfn_instance()
        ) {
            let (optimum, _) = brute_force(&cfn);
            prop_assert!(optimum != Cost::TOP_SENTINEL);

            let mut bounds = vec![optimum, Cost::from_raw(optimum.raw() + 1), Cost::TOP_SENTINEL];
            if optimum > Cost::BOT {
                bounds.push(Cost::from_raw(optimum.raw() - 1));
            }

            for bound in bounds {
                let outcome =
                    solve_dfbb(&cfn, &SearchParameters::default().with_upper_bound(bound));
                prop_assert!(outcome.proven_optimal);
                if bound > optimum {
                    prop_assert_eq!(outcome.upper_bound, optimum);
                    prop_assert_eq!(
                        cfn.evaluate(outcome.best.as_ref().unwrap()),
                        optimum
                    );
                } else {
                    prop_assert!(
                        outcome.best.is_none(),
                        "a bound at or below the optimum admits nothing"
                    );
                    prop_assert_eq!(outcome.lower_bound, bound);
                    prop_assert_eq!(outcome.upper_bound, Cost::TOP_SENTINEL);
                }
            }
        }

        /// The anytime contract, at every observation point rather than only at
        /// the end.
        ///
        /// The lower bound is the part that has to be earned: an interrupted
        /// depth-first search hands back a solution with no statement of how
        /// wrong it might be, and the whole point of the hybrid is that it
        /// hands back a solution together with a proof that nothing better than
        /// `lower_bound` exists.
        #[test]
        fn anytime_bounds_bracket_the_optimum(
            (_, _, _, _, cfn) in arb_small_cfn_instance()
        ) {
            let pristine = cfn.clone();
            let (optimum, argmins) = brute_force(&cfn);
            let found = solve_hbfs(&cfn, &HbfsParameters::default());

            prop_assert!(!found.trace.is_empty());
            let mut earlier: Option<(Cost, Cost)> = None;
            for observation in &found.trace {
                prop_assert!(observation.lower_bound <= optimum);
                prop_assert!(observation.upper_bound >= optimum);
                if let Some((lower, upper)) = earlier {
                    prop_assert!(observation.lower_bound >= lower);
                    prop_assert!(observation.upper_bound <= upper);
                }
                earlier = Some((observation.lower_bound, observation.upper_bound));
            }

            prop_assert!(found.outcome.limit_hit.is_none());
            prop_assert!(found.outcome.proven_optimal);
            prop_assert_eq!(found.outcome.lower_bound, optimum);
            prop_assert_eq!(found.outcome.upper_bound, optimum);
            let best = found.outcome.best.as_ref().unwrap();
            prop_assert_eq!(pristine.evaluate(best), optimum);
            prop_assert!(argmins.contains(best));
        }

        /// The two drivers over one depth-first core agree.
        #[test]
        fn best_first_and_depth_first_agree(
            (_, _, _, _, cfn) in arb_small_cfn_instance()
        ) {
            let depth = solve_dfbb(&cfn, &SearchParameters::default());
            let hybrid = solve_hbfs(&cfn, &HbfsParameters::default());
            prop_assert_eq!(depth.upper_bound, hybrid.outcome.upper_bound);
            prop_assert_eq!(depth.lower_bound, hybrid.outcome.lower_bound);
            prop_assert_eq!(depth.proven_optimal, hybrid.outcome.proven_optimal);
        }

        /// Restarting after one backtrack, which turns the nogood machinery on
        /// and keeps it on, still returns the optimum.
        ///
        /// At the shipped restart scale neither a restart nor a nogood ever
        /// fires on an instance small enough to enumerate, so without this the
        /// two of them would ship untested. They are the parts of the search
        /// that can quietly delete an answer: a nogood asserts that a region
        /// holds nothing better than the bound, and a wrong one prunes the
        /// optimum with no error signal.
        #[test]
        fn restarts_and_nogoods_preserve_the_optimum(
            (_, _, _, _, cfn) in arb_small_cfn_instance()
        ) {
            let pristine = cfn.clone();
            let (optimum, argmins) = brute_force(&cfn);
            let parameters = SearchParameters::default()
                .with_restarts(true)
                .with_restart_scale(1);

            let outcome = solve_dfbb(&cfn, &parameters);
            prop_assert!(outcome.proven_optimal);
            prop_assert_eq!(outcome.upper_bound, optimum);
            let best = outcome.best.as_ref().unwrap();
            prop_assert_eq!(pristine.evaluate(best), optimum);
            prop_assert!(argmins.contains(best));
        }

        /// The same input gives the same answer, node count and bound trace.
        ///
        /// A span is content-addressed downstream, so a search that broke a tie
        /// on a hash map's iteration order would produce a different apex from
        /// one run to the next with nothing else changed.
        #[test]
        fn the_search_is_deterministic(
            (_, _, _, _, cfn) in arb_small_cfn_instance()
        ) {
            let first = solve_dfbb(&cfn, &SearchParameters::default());
            let anytime = solve_hbfs(&cfn, &HbfsParameters::default());
            for _ in 0..3 {
                let again = solve_dfbb(&cfn, &SearchParameters::default());
                prop_assert_eq!(&first.best, &again.best);
                prop_assert_eq!(first.nodes, again.nodes);
                prop_assert_eq!(first.lower_bound, again.lower_bound);
                prop_assert_eq!(first.upper_bound, again.upper_bound);

                let repeated = solve_hbfs(&cfn, &HbfsParameters::default());
                prop_assert_eq!(&anytime.trace, &repeated.trace);
                prop_assert_eq!(&anytime.outcome.best, &repeated.outcome.best);
            }
        }
    }
}

/// The dispatcher: [`solve`], which routes each component of a network to a
/// path and folds the answers back together.
///
/// [`solve`]: panproto_mig::solve::solve
mod dispatcher {
    use panproto_integration::arb_small_cfn_instance;
    use panproto_mig::solve::oracle::brute_force;
    use panproto_mig::solve::order::primal_graph;
    use panproto_mig::solve::{SearchWarning, solve};
    use panproto_mig::{Cost, SearchBudget, SolverPath};
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(512))]

        /// The exit criterion for the dispatcher.
        ///
        /// Four claims, in the order they build on each other. First, the cost
        /// it reports is the true minimum over every assignment, which for a
        /// decomposed network is the claim that `OPT = c_∅ ⊕ ⨁_K OPT(K)` rather
        /// than a claim about any one component. Second, the assignment it
        /// returns scores exactly that cost against a copy of the network taken
        /// before it ran: the components are solved over rebuilt sub-networks,
        /// so a mistake in the translation back would show as a reported cost
        /// no assignment achieves. Third, it is one of the true argmins.
        /// Fourth, it proved it.
        #[test]
        fn oracle_agrees_with_solve(
            (_, _, _, _, cfn) in arb_small_cfn_instance()
        ) {
            // Kept before anything runs: the scorer the claim is checked with
            // must be one the dispatcher never touched.
            let pristine = cfn.clone();

            let outcome = solve(&cfn, &SearchBudget::default());
            let (optimum, argmins) = brute_force(&cfn);

            prop_assert!(optimum != Cost::TOP_SENTINEL);
            prop_assert!(!argmins.is_empty());

            // (1) The reported cost is the true minimum.
            prop_assert_eq!(outcome.upper_bound, optimum);

            let best = outcome.best.as_ref().unwrap();

            // (2) The assignment scores the reported cost against the copy.
            prop_assert_eq!(pristine.evaluate(best), outcome.upper_bound);

            // (3) It is one of the true argmins.
            prop_assert!(argmins.contains(best));

            // (4) It proved it.
            prop_assert!(outcome.limit_hit.is_none());
            prop_assert!(outcome.proven_optimal);
            prop_assert_eq!(outcome.lower_bound, outcome.upper_bound);
        }

        /// The decomposition changes nothing but the route.
        ///
        /// Solving component by component and solving whole are the same
        /// problem, so they agree on the cost *and* on which argmin is reached:
        /// the tie-break is per variable, and each component settles its own
        /// variables, so concatenating the per-component least argmins gives
        /// the same assignment the undecomposed search would have reached.
        #[test]
        fn the_decomposition_agrees_with_solving_whole(
            (_, _, _, _, cfn) in arb_small_cfn_instance()
        ) {
            let components = primal_graph(&cfn).components().len();
            let decomposed = solve(&cfn, &SearchBudget::default());
            let (optimum, argmins) = brute_force(&cfn);

            prop_assert_eq!(decomposed.upper_bound, optimum);
            let best = decomposed.best.as_ref().unwrap();
            prop_assert!(argmins.contains(best));

            // The width reported is the largest of the components', and a
            // network of one component is reported unchanged.
            if components == 1 {
                let eliminated = matches!(decomposed.path, SolverPath::Eliminate { .. });
                prop_assert!(eliminated, "one narrow component is exact");
            }
        }

        /// Refusing exact inference changes the route and the warning, not the
        /// answer.
        ///
        /// At zero memory every component falls back to search, so this runs
        /// the whole fallback path against the oracle and checks that the
        /// fallback is announced rather than silent.
        #[test]
        fn a_starved_budget_still_reaches_the_optimum(
            (_, _, _, _, cfn) in arb_small_cfn_instance()
        ) {
            let pristine = cfn.clone();
            let starved = SearchBudget::default().with_mem_bytes(0);

            let outcome = solve(&cfn, &starved);
            let (optimum, argmins) = brute_force(&cfn);

            prop_assert_eq!(outcome.upper_bound, optimum);
            let searched = matches!(outcome.path, SolverPath::BranchAndBound { .. });
            prop_assert!(searched, "no component fits a zero-byte budget");
            prop_assert!(outcome.proven_optimal);

            let best = outcome.best.as_ref().unwrap();
            prop_assert_eq!(pristine.evaluate(best), optimum);
            prop_assert!(argmins.contains(best));

            prop_assert!(
                outcome.warnings.iter().any(|warning| matches!(
                    warning,
                    SearchWarning::EliminationOutOfBudget { .. }
                )),
                "a component routed away from exact inference says so"
            );
        }

        /// The same input gives the same answer, node count and route.
        #[test]
        fn the_dispatcher_is_deterministic(
            (_, _, _, _, cfn) in arb_small_cfn_instance()
        ) {
            let first = solve(&cfn, &SearchBudget::default());
            for _ in 0..3 {
                let again = solve(&cfn, &SearchBudget::default());
                prop_assert_eq!(&first.best, &again.best);
                prop_assert_eq!(first.nodes, again.nodes);
                prop_assert_eq!(first.path, again.path);
                prop_assert_eq!(&first.elimination_order, &again.elimination_order);
            }
        }
    }
}

/// The anytime contract under interruption, and the boundaries a caller-supplied
/// bound can be set at.
///
/// These are regressions. Each one reproduces a defect that shipped: a certified
/// lower bound that sat above the optimum when a budget fired mid-dive, and a
/// crash when the primal bound was set below the network's constant.
mod anytime_contract {
    use panproto_integration::arb_small_cfn_instance;
    use panproto_mig::solve::dfbb::{SearchParameters, solve_dfbb};
    use panproto_mig::solve::hbfs::{HbfsParameters, solve_hbfs};
    use panproto_mig::solve::oracle::brute_force;
    use panproto_mig::{Cost, SearchBudget};
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(512))]

        /// A node budget firing anywhere leaves the lower bound below the
        /// optimum.
        ///
        /// The frontier's least bound is a global lower bound only while the
        /// frontier partitions the assignment space. A dive abandoned by a
        /// budget used to return without recording the subtree it was giving
        /// up, so the frontier stopped covering the space, and once it emptied
        /// the bound was ratcheted to the incumbent's cost: on an instance
        /// whose optimum was eleven the search certified fourteen, and with a
        /// zero budget it certified `⊤`, which reads as "nothing is feasible"
        /// for a network that has an answer.
        ///
        /// Every node count from none to well past the whole search is swept,
        /// because the defect needed the budget to fire on the *last* frontier
        /// node and so hid at almost every other count.
        #[test]
        fn an_interrupted_search_never_certifies_too_much(
            (_, _, _, _, cfn) in arb_small_cfn_instance()
        ) {
            let (optimum, _) = brute_force(&cfn);
            prop_assert!(optimum != Cost::TOP_SENTINEL);

            for nodes in 0u64..24 {
                let budget = SearchBudget::default().with_max_nodes(Some(nodes));
                let parameters = HbfsParameters::default()
                    .with_search(SearchParameters::default().with_budget(budget));
                let found = solve_hbfs(&cfn, &parameters);

                prop_assert!(
                    found.outcome.lower_bound <= optimum,
                    "hbfs certified {:?} above the optimum {:?} at {} nodes",
                    found.outcome.lower_bound,
                    optimum,
                    nodes
                );

                // The whole trace carries the claim, not only its last entry.
                let mut previous: Option<(Cost, Cost)> = None;
                for observation in &found.trace {
                    prop_assert!(observation.lower_bound <= optimum);
                    prop_assert!(observation.upper_bound >= optimum);
                    if let Some((lower, upper)) = previous {
                        prop_assert!(observation.lower_bound >= lower);
                        prop_assert!(observation.upper_bound <= upper);
                    }
                    previous = Some((observation.lower_bound, observation.upper_bound));
                }

                // The upper bound does not rise on the way into the outcome.
                if let Some(last) = found.trace.last() {
                    prop_assert!(last.upper_bound >= found.outcome.upper_bound);
                }

                // An incumbent still scores what it claims to.
                if let Some(best) = &found.outcome.best {
                    prop_assert_eq!(cfn.evaluate(best), found.outcome.upper_bound);
                }

                let depth = solve_dfbb(&cfn, &SearchParameters::default().with_budget(budget));
                prop_assert!(
                    depth.lower_bound <= optimum,
                    "dfbb certified a bound above the optimum at {} nodes",
                    nodes
                );
            }
        }
    }

    /// A primal bound below the network's constant is answered, not crashed.
    ///
    /// `c_∅` carries the vacuous components of the objective, so it sits above
    /// `⊥` on any network a schema pair produces, and a caller may legitimately
    /// ask whether anything beats a bound below it. Reading `c_∅` unclamped
    /// then asked `(x ⊕ c_∅) ⊖ c_∅` for a difference no cell could pay: the `⊕`
    /// saturates at `⊤ ≺ c_∅` and the fairness precondition fires, in a release
    /// build, on both public search entry points.
    ///
    /// This is a fixture rather than a property because the generator builds
    /// every network with `c_∅ = ⊥`, so no number of generated cases can reach
    /// the boundary the defect lived on.
    #[test]
    fn a_bound_below_the_constant_is_answered_rather_than_crashed() {
        use panproto_gat::Name;
        use panproto_mig::solve::cfn::CfnBuilder;
        use panproto_mig::solve::{DEFAULT_WEIGHTS, ValId, VarId};

        const FIRST: VarId = VarId::new(0);
        const SECOND: VarId = VarId::new(1);

        let mut builder = CfnBuilder::new(
            vec![
                (Name::new("u"), vec![Name::new("t")]),
                (Name::new("v"), vec![Name::new("t")]),
            ],
            DEFAULT_WEIGHTS,
        )
        .unwrap();
        builder.add_empty(Cost::from_raw(7));
        builder
            .add_unary_table(FIRST, &[Cost::from_raw(1), Cost::from_raw(3)])
            .unwrap();
        builder
            .add_unary_table(SECOND, &[Cost::from_raw(2), Cost::BOT])
            .unwrap();
        builder
            .add_function(
                &[FIRST, SECOND],
                vec![
                    Cost::from_raw(1),
                    Cost::from_raw(2),
                    Cost::from_raw(3),
                    Cost::from_raw(4),
                ],
            )
            .unwrap();
        let cfn = builder.build();
        assert_eq!(cfn.c_empty(), Cost::from_raw(7));
        assert!(cfn.domain(FIRST).unwrap().contains(ValId::BOTTOM));

        // Above, at, and below the constant. Only the last two could crash, and
        // the last is the one that did.
        for raw in [9u64, 8, 7, 6, 5, 1, 0] {
            let parameters = SearchParameters::default().with_upper_bound(Cost::from_raw(raw));
            let depth = solve_dfbb(&cfn, &parameters);
            let anytime = solve_hbfs(&cfn, &HbfsParameters::default().with_search(parameters));

            // Nothing costs less than `c_∅`, so a bound at or below it admits
            // no answer, and the search says so rather than aborting.
            if raw <= 7 {
                assert!(depth.best.is_none(), "bound {raw} should admit nothing");
                assert!(anytime.outcome.best.is_none());
            }
            assert!(depth.lower_bound <= depth.upper_bound || depth.best.is_none());
        }

        // And the answer with no bound at all is still the true optimum.
        let (optimum, argmins) = brute_force(&cfn);
        let found = solve_dfbb(&cfn, &SearchParameters::default());
        assert_eq!(found.upper_bound, optimum);
        assert!(argmins.contains(found.best.as_ref().unwrap()));
    }
}

/// Determinism across processes, which is guarantee 6 of the search contract.
///
/// A regression. Descriptor identities used to be interned in the order a
/// `std::collections::HashMap` handed its keys over, so the per-process hash
/// seed reached the branching order and, where optima tie, the answer.
mod hash_seed {
    use panproto_mig::solve::build::{NoEvidence, build_cfn};
    use panproto_mig::solve::mcsplit::solve_iso;
    use panproto_mig::solve::{SearchBudget, solve};
    use panproto_mig::{DEFAULT_WEIGHTS, DomainConstraints, SearchOptions};
    use panproto_schema::{Protocol, Schema, SchemaBuilder};

    fn protocol() -> Protocol {
        Protocol {
            name: "test".into(),
            schema_theory: "ThTest".into(),
            instance_theory: "ThWType".into(),
            edge_rules: vec![],
            obj_kinds: vec!["object".into(), "string".into(), "integer".into()],
            constraint_sorts: vec![],
            ..Protocol::default()
        }
    }

    fn schema(vertices: &[(&str, &str)], edges: &[(&str, &str, &str)]) -> Schema {
        let protocol = protocol();
        let mut builder = SchemaBuilder::new(&protocol);
        for (id, kind) in vertices {
            builder = builder.vertex(id, kind, None::<&str>).unwrap();
        }
        for (from, to, kind) in edges {
            builder = builder.edge(from, to, kind, None::<&str>).unwrap();
        }
        builder.build().unwrap()
    }

    /// A dense nine-by-nine uniform-kind pair, built fresh on every call.
    ///
    /// Rebuilding is the whole point: `Schema::edges` draws a fresh hash key
    /// per map instance, so two schemas built from identical input iterate
    /// their edges in different orders within one process. Repeating a solve
    /// over one schema cannot see that, which is why the shipped repetition
    /// test could not catch the defect this pins.
    ///
    /// Uniform vertex kinds put every vertex in one root label class, so the
    /// search genuinely branches, and the irregular arc set gives enough
    /// distinct descriptor multisets for two of them to swap identities.
    fn instance(seed: u64) -> (Schema, Schema) {
        let mut state = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        let mut next = move || {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            usize::try_from(state >> 33).unwrap_or(0)
        };
        let kinds = ["prop", "item", "variant"];

        let src_names: Vec<String> = (0..9).map(|index| format!("s{index}")).collect();
        let tgt_names: Vec<String> = (0..9).map(|index| format!("t{index}")).collect();
        let src_vertices: Vec<(&str, &str)> = src_names
            .iter()
            .map(|name| (name.as_str(), "object"))
            .collect();
        let tgt_vertices: Vec<(&str, &str)> = tgt_names
            .iter()
            .map(|name| (name.as_str(), "object"))
            .collect();

        let mut src_edges = Vec::new();
        let mut tgt_edges = Vec::new();
        for from in 0..9usize {
            for to in 0..9usize {
                if from == to {
                    continue;
                }
                if next() % 4 == 0 {
                    src_edges.push((
                        src_names[from].as_str(),
                        src_names[to].as_str(),
                        kinds[next() % 3],
                    ));
                }
                if next() % 4 == 0 {
                    tgt_edges.push((
                        tgt_names[from].as_str(),
                        tgt_names[to].as_str(),
                        kinds[next() % 3],
                    ));
                }
            }
        }
        (
            schema(&src_vertices, &src_edges),
            schema(&tgt_vertices, &tgt_edges),
        )
    }

    fn network(src: &Schema, tgt: &Schema) -> panproto_mig::Cfn {
        build_cfn(
            src,
            tgt,
            &SearchOptions::default(),
            &DomainConstraints::default(),
            &NoEvidence,
            DEFAULT_WEIGHTS,
        )
        .unwrap()
    }

    #[test]
    fn the_injective_search_is_independent_of_the_hash_seed() {
        for seed in 0..120u64 {
            let (src, tgt) = instance(seed);
            let first =
                solve_iso(&network(&src, &tgt), &src, &tgt, &SearchBudget::default()).unwrap();
            for round in 0..12 {
                let (src, tgt) = instance(seed);
                let again =
                    solve_iso(&network(&src, &tgt), &src, &tgt, &SearchBudget::default()).unwrap();
                assert_eq!(
                    first.best, again.best,
                    "seed {seed} round {round}: answer drifted"
                );
                assert_eq!(
                    first.nodes, again.nodes,
                    "seed {seed} round {round}: node count drifted"
                );
                assert_eq!(first.upper_bound, again.upper_bound);
                assert_eq!(first.lower_bound, again.lower_bound);
            }
        }
    }

    #[test]
    fn the_network_and_the_dispatcher_are_independent_of_the_hash_seed() {
        // The objective is built by walking several hash maps of the schema,
        // and the dispatcher walks the network's own structure, so both are
        // held to the same claim as the search.
        for seed in 0..120u64 {
            let (src, tgt) = instance(seed);
            let first = network(&src, &tgt);
            let routed = solve(&first, &SearchBudget::default());
            for round in 0..4 {
                let (src, tgt) = instance(seed);
                let again = network(&src, &tgt);
                assert!(
                    first == again,
                    "seed {seed} round {round}: the network drifted"
                );

                let repeated = solve(&again, &SearchBudget::default());
                assert_eq!(routed.best, repeated.best);
                assert_eq!(routed.nodes, repeated.nodes);
                assert_eq!(routed.elimination_order, repeated.elimination_order);
            }
        }
    }
}
