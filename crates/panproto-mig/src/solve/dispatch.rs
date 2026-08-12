//! The dispatcher: which algorithm each part of a network is solved by.
//!
//! [`solve`] is the entry point a caller who does not want to choose an
//! algorithm uses. It splits the network into independent parts, prices exact
//! inference on each, and routes each part to bucket elimination or to search
//! accordingly. Every path reports the same [`SolveOutcome`], so the choice is
//! observable in [`SolveOutcome::path`] without being something the caller had
//! to make.
//!
//! # The decomposition
//!
//! The primal graph's connected components partition the variables, and no cost
//! function spans two of them, so the objective is a sum of one term per
//! component plus the constant:
//!
//! ```text
//! OPT(C) = c_∅ ⊕ ⨁_K OPT(K)
//! ```
//!
//! Solving each component separately is therefore exact rather than a
//! heuristic, and it is worth doing because both costs the routing decision
//! turns on are exponential in the component's width: two components of width
//! `w` cost `2·d^w`, while the union of them, whose width is also `w` but whose
//! variable count is the sum, costs the same in space and strictly more in the
//! search that would otherwise run over both at once. It also lets one hard
//! component fall back to search without dragging the rest with it, which is
//! why [`SolveOutcome::path`] reports the whole answer's path rather than one
//! per component: a network is routed to [`SolverPath::Eliminate`] only when
//! every one of its components was.
//!
//! Concatenating the per-component argmins gives a global argmin because the
//! components are independent, and it preserves the tie-break, because the
//! order the tie-break reads is per variable and each component's decode
//! settles its own variables.
//!
//! # Which width is read
//!
//! The width is read off the primal graph of the *cost functions*, not of the
//! schema's edges. Recursion points, schema spans and hyper-edge signature
//! cliques constrain vertex sets that need not be joined by an edge, so those
//! apex hard constraints raise the width, and a routing decision taken before
//! they were added would allocate against a number that is too small. Building
//! the graph from `cfn.functions()` reads the width after they are in, which is
//! the only reading that can be trusted with an allocation.
//!
//! # Injectivity is not routed here
//!
//! [`SolverPath::Monic`] and [`SolverPath::Iso`] are reached through
//! [`solve_monic`] and [`mcsplit::solve_iso`](super::mcsplit::solve_iso) rather
//! than from here. Injectivity is not a property of a network: it constrains
//! how variables may share values, which no cost function in the network states
//! and which [`build_cfn`](super::build::build_cfn) deliberately does not
//! encode. So a caller who wants an injective answer says so by calling the
//! injective entry point, and `solve` is the non-injective dispatcher. The iso
//! path additionally needs both schemas, since the edge-reflection condition it
//! decides is stated over arcs rather than over costs.

use panproto_gat::Name;

use super::cfn::{Cfn, CfnBuilder};
use super::cost::Cost;
use super::dfbb::SearchParameters;
use super::elim::{decode, eliminate};
use super::hbfs::{HbfsParameters, solve_hbfs};
use super::order::{choose_order, elimination_cost, fits_budget, primal_graph};
use super::{Assignment, SearchBudget, SearchWarning, SolveOutcome, SolverPath, ValId, VarId};

/// Solve a network, choosing the algorithm from its shape and the budget.
///
/// The network is split into its independent components; each is solved by
/// bucket elimination when its induced width prices inside `budget`, and by
/// hybrid best-first branch and bound when it does not. The answer is exact
/// whenever every component proved optimality, which on the elimination path is
/// always and on the search path is whenever no limit was hit.
///
/// `budget` governs both decisions: [`SearchBudget::mem_bytes`] and
/// [`SearchBudget::op_budget`] price exact inference, and
/// [`SearchBudget::max_nodes`] bounds the search that runs when it is refused.
/// Exact inference never consults the node budget, because it never prunes and
/// so has no node to count.
///
/// A component routed to search contributes a
/// [`SearchWarning::EliminationOutOfBudget`] naming the width and what exact
/// inference would have cost, so a caller can tell a deliberate fallback from
/// an accidental one.
///
/// # Examples
///
/// ```
/// use panproto_mig::solve::build::{NoEvidence, build_cfn};
/// use panproto_mig::solve::{SearchBudget, SolverPath, solve};
/// use panproto_mig::{DEFAULT_WEIGHTS, DomainConstraints, SearchOptions};
/// use panproto_schema::{Protocol, SchemaBuilder};
///
/// let protocol = Protocol {
///     name: "demo".into(),
///     schema_theory: "ThTest".into(),
///     instance_theory: "ThWType".into(),
///     obj_kinds: vec!["object".into(), "string".into()],
///     ..Protocol::default()
/// };
/// let schema = SchemaBuilder::new(&protocol)
///     .vertex("root", "object", None::<&str>)?
///     .vertex("root.label", "string", None::<&str>)?
///     .edge("root", "root.label", "prop", Some("label"))?
///     .build()?;
///
/// let cfn = build_cfn(
///     &schema,
///     &schema,
///     &SearchOptions::default(),
///     &DomainConstraints::default(),
///     &NoEvidence,
///     DEFAULT_WEIGHTS,
/// )?;
///
/// let found = solve(&cfn, &SearchBudget::default());
///
/// // A two-vertex schema is one narrow component, so exact inference takes it
/// // and proves the answer optimal.
/// assert!(matches!(found.path, SolverPath::Eliminate { .. }));
/// assert!(found.proven_optimal);
/// assert_eq!(found.lower_bound, found.upper_bound);
///
/// let best = found.best.expect("the all-bottom assignment is always feasible");
/// assert_eq!(cfn.evaluate(&best), found.upper_bound);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[must_use]
pub fn solve(cfn: &Cfn, budget: &SearchBudget) -> SolveOutcome {
    if cfn.n_variables() == 0 {
        return constant_only(cfn);
    }
    let components = primal_graph(cfn).components();
    if components.len() <= 1 {
        return solve_part(cfn, budget);
    }
    // A restriction of a network the builder already accepted is a network the
    // builder accepts, so the fallback is unreachable. Taking it would still
    // give the right answer, only without the decomposition's saving, which is
    // why it is a fallback rather than a panic.
    decompose(cfn, &components).map_or_else(
        || solve_part(cfn, budget),
        |parts| combine(cfn, &components, &parts, budget),
    )
}

/// The outcome of a network with no variables: the constant, and nothing to
/// choose.
fn constant_only(cfn: &Cfn) -> SolveOutcome {
    let constant = cfn.c_empty();
    let feasible = constant < Cost::TOP_SENTINEL;
    SolveOutcome {
        best: feasible.then(|| Assignment::all_bottom(0)),
        lower_bound: constant,
        upper_bound: if feasible {
            constant
        } else {
            Cost::TOP_SENTINEL
        },
        proven_optimal: true,
        path: SolverPath::Eliminate { width: 0 },
        elimination_order: Some(Vec::new()),
        nodes: 0,
        limit_hit: None,
        warnings: Vec::new(),
    }
}

/// Solve one network whole, routed on its width against the budget.
fn solve_part(cfn: &Cfn, budget: &SearchBudget) -> SolveOutcome {
    let (order, width) = choose_order(cfn);
    if fits_budget(cfn, width, budget) {
        return eliminate_whole(cfn, order, width);
    }

    let refused = elimination_cost(cfn, width);
    let parameters = HbfsParameters::default().with_search(
        SearchParameters::default()
            .with_width(width)
            .with_budget(*budget),
    );
    let mut outcome = solve_hbfs(cfn, &parameters).outcome;
    outcome
        .warnings
        .push(SearchWarning::EliminationOutOfBudget {
            width,
            entries: refused.entries,
            operations: refused.operations,
        });
    outcome
}

/// Run bucket elimination and read the answer off it.
fn eliminate_whole(cfn: &Cfn, order: Vec<VarId>, width: usize) -> SolveOutcome {
    let buckets = eliminate(cfn, &order);
    let optimum = buckets.optimum();
    // Exact inference proves optimality whatever the budget: it never prunes,
    // so there is no branch it could have failed to rule out.
    if optimum >= Cost::TOP_SENTINEL {
        return SolveOutcome {
            best: None,
            lower_bound: Cost::TOP_SENTINEL,
            upper_bound: Cost::TOP_SENTINEL,
            proven_optimal: true,
            path: SolverPath::Eliminate { width },
            elimination_order: Some(order),
            nodes: 0,
            limit_hit: None,
            warnings: Vec::new(),
        };
    }
    let best = decode(cfn, &buckets, &order);
    SolveOutcome {
        best: Some(best),
        lower_bound: optimum,
        upper_bound: optimum,
        proven_optimal: true,
        path: SolverPath::Eliminate { width },
        elimination_order: Some(order),
        nodes: 0,
        limit_hit: None,
        warnings: Vec::new(),
    }
}

/// One network per component, each over that component's variables alone.
///
/// The constant stays with the caller rather than being copied into every part,
/// since copying it would count it once per component.
fn decompose(cfn: &Cfn, components: &[Vec<VarId>]) -> Option<Vec<Cfn>> {
    let mut local: Vec<Option<VarId>> = vec![None; cfn.n_variables()];
    let mut parts = Vec::with_capacity(components.len());

    for component in components {
        let mut spec: Vec<(Name, Vec<Name>)> = Vec::with_capacity(component.len());
        for (slot, var) in component.iter().enumerate() {
            let variable = cfn.variable(*var)?;
            let index = u32::try_from(slot).ok()?;
            *local.get_mut(var.index())? = Some(VarId::new(index));
            spec.push((variable.name().clone(), variable.values().to_vec()));
        }

        let mut builder = CfnBuilder::new(spec, cfn.weights()).ok()?;
        for var in component {
            let Some(table) = cfn.unary(*var) else {
                continue;
            };
            let target = (*local.get(var.index())?)?;
            builder.add_unary_table(target, table).ok()?;
        }
        for function in cfn.functions() {
            let scope = function.scope();
            // Every variable of a scope is a clique in the primal graph, so a
            // scope lies wholly inside one component or wholly outside this
            // one; testing its first variable decides which.
            let first = scope.first()?;
            if local.get(first.index()).copied().flatten().is_none() {
                continue;
            }
            let mut mapped = Vec::with_capacity(scope.len());
            for var in scope {
                mapped.push((*local.get(var.index())?)?);
            }
            builder
                .add_function(&mapped, function.table().to_vec())
                .ok()?;
        }
        parts.push(builder.build());

        for var in component {
            *local.get_mut(var.index())? = None;
        }
    }
    Some(parts)
}

/// Solve every part and fold the answers into one outcome.
fn combine(
    cfn: &Cfn,
    components: &[Vec<VarId>],
    parts: &[Cfn],
    budget: &SearchBudget,
) -> SolveOutcome {
    let top = Cost::TOP_SENTINEL;
    let mut values = vec![ValId::BOTTOM; cfn.n_variables()];
    let mut order: Option<Vec<VarId>> = Some(Vec::with_capacity(cfn.n_variables()));
    let mut lower = cfn.c_empty();
    let mut upper = cfn.c_empty();
    let mut width = 0usize;
    let mut nodes = 0u64;
    let mut warnings: Vec<SearchWarning> = Vec::new();
    let mut limit_hit = None;
    let mut proven = true;
    let mut exact = true;
    let mut complete = true;

    for (component, part) in components.iter().zip(parts) {
        let outcome = solve_part(part, budget);

        match outcome.path {
            SolverPath::Eliminate { width: found } => width = width.max(found),
            SolverPath::BranchAndBound { width: found } => {
                width = width.max(found);
                exact = false;
            }
            SolverPath::Monic | SolverPath::Iso => exact = false,
        }

        // The concatenation is a valid global elimination sequence: eliminating
        // one component whole before starting the next adds no fill between
        // them, since no cost function joins them. It is reported only when
        // every component was eliminated, because the tie-break the field
        // stands for is one an interrupted search does not make.
        match (&mut order, &outcome.elimination_order) {
            (Some(sequence), Some(local)) => {
                for var in local {
                    if let Some(global) = component.get(var.index()) {
                        sequence.push(*global);
                    }
                }
            }
            (slot, _) => *slot = None,
        }

        if let Some(best) = &outcome.best {
            for (slot, value) in best.values().iter().enumerate() {
                if let Some(target) = component
                    .get(slot)
                    .and_then(|var| values.get_mut(var.index()))
                {
                    *target = *value;
                }
            }
        } else {
            complete = false;
        }

        lower = lower.combine(outcome.lower_bound, top);
        upper = upper.combine(outcome.upper_bound, top);
        nodes = nodes.saturating_add(outcome.nodes);
        proven = proven && outcome.proven_optimal;
        limit_hit = limit_hit.or(outcome.limit_hit);
        warnings.extend(outcome.warnings);
    }

    SolveOutcome {
        best: complete.then(|| Assignment::from_values(values)),
        lower_bound: lower,
        upper_bound: if complete { upper } else { top },
        proven_optimal: proven,
        path: if exact {
            SolverPath::Eliminate { width }
        } else {
            SolverPath::BranchAndBound { width }
        },
        elimination_order: if exact { order } else { None },
        nodes,
        limit_hit,
        warnings,
    }
}

// ---------------------------------------------------------------------------
// The injective path
// ---------------------------------------------------------------------------

/// Solve a network under the constraint that no two variables share a target.
///
/// This is [`SolverPath::Monic`]: branch and bound with an all-different
/// constraint over the real values, `⊥` exempt. Injectivity joins every pair of
/// variables that could collide, so it completes the primal graph and puts
/// exact inference out of reach by construction rather than by budget, which is
/// why no width is read here and no elimination order is reported.
///
/// `⊥` is exempt because it is not a target: any number of source vertices may
/// be left out of the apex, and a rule that made them collide would forbid
/// every span but the total ones.
///
/// The filter is the same Hall-set propagator the maximum common sub-schema
/// search uses, so what it prunes is a pigeonhole rather than a heuristic. It
/// runs after the consistency enforcement at each node, and a wipe-out closes
/// the node. A complete assignment that reaches the incumbent is injective by
/// construction: with every domain a singleton, two variables holding one
/// target are a Hall set of one target with two members.
///
/// # Examples
///
/// ```
/// use panproto_mig::solve::build::{NoEvidence, build_cfn};
/// use panproto_mig::solve::{SearchBudget, SolverPath, solve_monic};
/// use panproto_mig::{DEFAULT_WEIGHTS, DomainConstraints, SearchOptions};
/// use panproto_schema::{Protocol, SchemaBuilder};
///
/// let protocol = Protocol {
///     name: "demo".into(),
///     schema_theory: "ThTest".into(),
///     instance_theory: "ThWType".into(),
///     obj_kinds: vec!["object".into(), "string".into()],
///     ..Protocol::default()
/// };
/// let schema = SchemaBuilder::new(&protocol)
///     .vertex("root", "object", None::<&str>)?
///     .vertex("root.a", "string", None::<&str>)?
///     .vertex("root.b", "string", None::<&str>)?
///     .edge("root", "root.a", "prop", Some("a"))?
///     .edge("root", "root.b", "prop", Some("b"))?
///     .build()?;
///
/// let cfn = build_cfn(
///     &schema,
///     &schema,
///     &SearchOptions::default(),
///     &DomainConstraints::default(),
///     &NoEvidence,
///     DEFAULT_WEIGHTS,
/// )?;
///
/// let found = solve_monic(&cfn, &SearchBudget::default());
/// assert_eq!(found.path, SolverPath::Monic);
///
/// // No two source vertices share a target, though any number may take `⊥`.
/// // The comparison is on target *names*: a `ValId` numbers one variable's own
/// // domain, so equal raw values on two variables need not be the same target.
/// let best = found.best.expect("the all-bottom assignment is always feasible");
/// let mut taken: Vec<&str> = best
///     .pairs()
///     .filter(|(_, value)| !value.is_bottom())
///     .filter_map(|(var, value)| cfn.variable(var)?.value_name(value))
///     .map(panproto_gat::Name::as_str)
///     .collect();
/// let before = taken.len();
/// taken.sort_unstable();
/// taken.dedup();
/// assert_eq!(taken.len(), before, "the map is injective on real targets");
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[must_use]
pub fn solve_monic(cfn: &Cfn, budget: &SearchBudget) -> SolveOutcome {
    let parameters = HbfsParameters::default().with_search(
        SearchParameters::default()
            .with_budget(*budget)
            .with_all_different(true),
    );
    let mut outcome = solve_hbfs(cfn, &parameters).outcome;
    outcome.path = SolverPath::Monic;
    outcome.elimination_order = None;
    outcome
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::solve::cost::DEFAULT_WEIGHTS;
    use crate::solve::oracle::brute_force;

    fn cost(units: u64) -> Cost {
        Cost::from_raw(units)
    }

    fn var(index: u32) -> VarId {
        VarId::new(index)
    }

    /// `count` variables over `targets` targets each, named so that ascending
    /// name order is ascending index order.
    fn builder(count: u32, targets: u32) -> CfnBuilder {
        let spec = (0..count)
            .map(|index| {
                let values = (0..targets)
                    .map(|slot| Name::new(format!("t{slot}")))
                    .collect();
                (Name::new(format!("v{index}")), values)
            })
            .collect();
        CfnBuilder::new(spec, DEFAULT_WEIGHTS).unwrap()
    }

    #[test]
    fn a_network_with_no_variables_is_its_constant() {
        let mut b = CfnBuilder::new(Vec::new(), DEFAULT_WEIGHTS).unwrap();
        b.add_empty(cost(7));
        let found = solve(&b.build(), &SearchBudget::default());
        assert_eq!(found.lower_bound, cost(7));
        assert_eq!(found.upper_bound, cost(7));
        assert!(found.proven_optimal);
        assert_eq!(found.best, Some(Assignment::all_bottom(0)));
    }

    #[test]
    fn one_narrow_component_is_eliminated_and_proved() {
        let mut b = builder(3, 2);
        b.add_unary_table(var(0), &[cost(4), cost(1), cost(9)])
            .unwrap();
        b.add_unary_table(var(1), &[cost(2), cost(6), cost(3)])
            .unwrap();
        b.add_unary_table(var(2), &[cost(5), cost(0), cost(7)])
            .unwrap();
        b.add_function(&[var(0), var(1)], vec![Cost::BOT; 9])
            .unwrap();
        b.add_function(&[var(1), var(2)], vec![Cost::BOT; 9])
            .unwrap();
        let cfn = b.build();

        let found = solve(&cfn, &SearchBudget::default());
        let (optimum, argmins) = brute_force(&cfn);
        assert!(matches!(found.path, SolverPath::Eliminate { width: 1 }));
        assert!(found.proven_optimal);
        assert_eq!(found.lower_bound, optimum);
        assert_eq!(found.upper_bound, optimum);
        let best = found.best.unwrap();
        assert_eq!(cfn.evaluate(&best), optimum);
        assert!(argmins.contains(&best));
    }

    #[test]
    fn independent_components_are_solved_apart_and_summed() {
        // Two disjoint pairs and one isolated variable: five variables, three
        // components, and an optimum that is the sum of the three.
        let mut b = builder(5, 2);
        b.add_empty(cost(3));
        b.add_unary_table(var(0), &[cost(4), cost(1), cost(9)])
            .unwrap();
        b.add_unary_table(var(1), &[cost(2), cost(6), cost(3)])
            .unwrap();
        b.add_unary_table(var(2), &[cost(5), cost(0), cost(7)])
            .unwrap();
        b.add_unary_table(var(3), &[cost(8), cost(2), cost(1)])
            .unwrap();
        b.add_unary_table(var(4), &[cost(6), cost(3), cost(4)])
            .unwrap();
        b.add_function(&[var(0), var(1)], vec![Cost::BOT; 9])
            .unwrap();
        b.add_function(&[var(2), var(3)], vec![Cost::BOT; 9])
            .unwrap();
        let cfn = b.build();

        assert_eq!(primal_graph(&cfn).components().len(), 3);

        let found = solve(&cfn, &SearchBudget::default());
        let (optimum, argmins) = brute_force(&cfn);
        assert!(found.proven_optimal, "every component proved its own");
        assert_eq!(found.lower_bound, optimum);
        assert_eq!(found.upper_bound, optimum);

        let best = found.best.unwrap();
        assert_eq!(
            cfn.evaluate(&best),
            optimum,
            "the concatenation scores what the parts summed to"
        );
        assert!(argmins.contains(&best));
    }

    #[test]
    fn the_decomposed_answer_is_the_lexicographically_least_argmin() {
        // Every unary cost equal, so every assignment ties and the tie-break is
        // the whole content of the answer. `⊥` sorts last, so the least argmin
        // takes the first real target everywhere.
        let mut b = builder(4, 2);
        for index in 0..4 {
            b.add_unary_table(var(index), &[Cost::BOT; 3]).unwrap();
        }
        b.add_function(&[var(0), var(1)], vec![Cost::BOT; 9])
            .unwrap();
        b.add_function(&[var(2), var(3)], vec![Cost::BOT; 9])
            .unwrap();
        let cfn = b.build();

        assert_eq!(primal_graph(&cfn).components().len(), 2);
        let found = solve(&cfn, &SearchBudget::default());
        let best = found.best.unwrap();
        let (_, argmins) = brute_force(&cfn);
        assert_eq!(argmins.len(), 81, "every assignment ties");
        assert_eq!(
            Some(&best),
            argmins.first(),
            "the concatenation of per-component least argmins is the least argmin"
        );
    }

    #[test]
    fn an_infeasible_component_makes_the_whole_network_infeasible() {
        let mut b = builder(4, 1);
        b.add_function(&[var(0), var(1)], vec![Cost::BOT; 4])
            .unwrap();
        b.add_function(&[var(2), var(3)], vec![Cost::TOP_SENTINEL; 4])
            .unwrap();
        let cfn = b.build();

        assert_eq!(primal_graph(&cfn).components().len(), 2);
        let found = solve(&cfn, &SearchBudget::default());
        assert_eq!(found.best, None);
        assert_eq!(found.upper_bound, Cost::TOP_SENTINEL);
        assert!(found.proven_optimal, "elimination proves infeasibility too");
    }

    #[test]
    fn a_refused_budget_routes_to_search_and_says_so() {
        let mut b = builder(4, 2);
        b.add_unary_table(var(0), &[cost(4), cost(1), cost(9)])
            .unwrap();
        b.add_unary_table(var(1), &[cost(2), cost(6), cost(3)])
            .unwrap();
        b.add_unary_table(var(2), &[cost(5), cost(0), cost(7)])
            .unwrap();
        b.add_unary_table(var(3), &[cost(8), cost(2), cost(1)])
            .unwrap();
        b.add_function(&[var(0), var(1)], vec![Cost::BOT; 9])
            .unwrap();
        b.add_function(&[var(1), var(2)], vec![Cost::BOT; 9])
            .unwrap();
        b.add_function(&[var(2), var(3)], vec![Cost::BOT; 9])
            .unwrap();
        let cfn = b.build();

        let starved = SearchBudget::default().with_mem_bytes(0);
        let found = solve(&cfn, &starved);
        assert!(matches!(found.path, SolverPath::BranchAndBound { .. }));
        assert!(
            found
                .warnings
                .iter()
                .any(|warning| matches!(warning, SearchWarning::EliminationOutOfBudget { .. })),
            "a fallback is reported rather than silent"
        );

        let (optimum, argmins) = brute_force(&cfn);
        assert_eq!(found.upper_bound, optimum, "the fallback is still exact");
        assert!(argmins.contains(&found.best.unwrap()));
    }

    #[test]
    fn the_dispatcher_agrees_with_the_oracle_on_both_routes() {
        let mut b = builder(4, 2);
        b.add_unary_table(var(0), &[cost(4), cost(1), cost(9)])
            .unwrap();
        b.add_unary_table(var(1), &[cost(2), cost(6), cost(3)])
            .unwrap();
        b.add_unary_table(var(2), &[cost(5), cost(0), cost(7)])
            .unwrap();
        b.add_unary_table(var(3), &[cost(8), cost(2), cost(1)])
            .unwrap();
        b.add_function(
            &[var(0), var(1)],
            vec![
                cost(1),
                cost(0),
                cost(2),
                cost(3),
                cost(1),
                cost(0),
                cost(2),
                cost(4),
                cost(1),
            ],
        )
        .unwrap();
        b.add_function(&[var(2), var(3)], vec![Cost::BOT; 9])
            .unwrap();
        let cfn = b.build();

        let (optimum, _) = brute_force(&cfn);
        let exact = solve(&cfn, &SearchBudget::default());
        let searched = solve(&cfn, &SearchBudget::default().with_mem_bytes(0));
        assert_eq!(exact.upper_bound, optimum);
        assert_eq!(searched.upper_bound, optimum);
        assert_eq!(exact.best, searched.best, "both routes agree on the answer");
    }

    #[test]
    fn the_monic_path_refuses_a_collision() {
        // Two source vertices, one target between them: no injective assignment
        // maps both, so the answer leaves one out.
        let mut b = builder(2, 1);
        b.add_unary_table(var(0), &[Cost::BOT, cost(10)]).unwrap();
        b.add_unary_table(var(1), &[Cost::BOT, cost(10)]).unwrap();
        b.add_function(&[var(0), var(1)], vec![Cost::BOT; 4])
            .unwrap();
        let cfn = b.build();

        let unconstrained = solve(&cfn, &SearchBudget::default());
        let best = unconstrained.best.unwrap();
        assert_eq!(
            best.values(),
            &[ValId::real(0), ValId::real(0)],
            "without injectivity both take the one cheap target"
        );

        let monic = solve_monic(&cfn, &SearchBudget::default());
        assert_eq!(monic.path, SolverPath::Monic);
        assert_eq!(monic.elimination_order, None);
        let best = monic.best.unwrap();
        let reals = best.values().iter().filter(|v| !v.is_bottom()).count();
        assert!(reals <= 1, "one target cannot serve two variables");
    }
}
