#![no_main]
//! Fuzz target for soft local consistency enforcement.
//!
//! The network here is built directly from arbitrary cost tables rather than
//! from a schema pair, because the enforcing algorithms are stated over
//! valuation structures and not over schemas: a generator that can only reach
//! the costs a schema pair produces never poses the tables where an
//! equivalence preserving transformation loses money.
//!
//! The invariants:
//!
//! 1. Enforcement terminates inside its own step budget. A budget overrun is
//!    a failure and not a timeout, because the budget exists to back up two
//!    preconditions that cannot be checked from inside the loop.
//! 2. A feasible result satisfies the predicate checker for the level
//!    enforced, which is written from the definition rather than from the
//!    algorithm.
//! 3. `c_∅` never decreases, at any level, over any sequence.
//! 4. The bound orderings that hold, `NC* ⪯ AC* ⪯ FDAC* ⪯ EDAC*` and
//!    `NC* ⪯ DAC*`, hold. `DAC* ⪯ FDAC*` is not among them: it was documented
//!    and this target refuted it.
//! 5. Enforcement is a *equivalence preserving* transformation: the cost of
//!    every surviving complete assignment is unchanged.
//!
//! Run with:
//!
//! ```text
//! cargo fuzz run consistency -- -max_total_time=300
//! ```

use arbitrary::Unstructured;
use libfuzzer_sys::fuzz_target;
use panproto_gat::Name;
use panproto_mig::solve::{ConsistencyLevel, Network};
use panproto_mig::{Cfn, CfnBuilder, Cost, DEFAULT_WEIGHTS, ValId, VarId};

/// Draw a cost inside `[⊥, ⊤]`, weighted toward the ends where the algorithms
/// change behaviour: `⊥` is the support every level looks for and `⊤` is the
/// refutation every level propagates.
fn draw_cost(u: &mut Unstructured<'_>) -> Cost {
    match u.arbitrary::<u8>().unwrap_or(0) % 8 {
        0 | 1 | 2 => Cost::BOT,
        3 => Cost::TOP_SENTINEL,
        _ => Cost::from_raw(u64::from(u.arbitrary::<u16>().unwrap_or(1))),
    }
}

/// An arbitrary small network: at most five variables of at most three real
/// values each, with unary tables and binary functions drawn per entry.
fn draw_network(u: &mut Unstructured<'_>) -> Option<Cfn> {
    let n_vars = 1 + usize::from(u.arbitrary::<u8>().ok()? % 5);
    let mut spec: Vec<(Name, Vec<Name>)> = Vec::with_capacity(n_vars);
    for i in 0..n_vars {
        let n_vals = 1 + usize::from(u.arbitrary::<u8>().ok()? % 3);
        let values: Vec<Name> = (0..n_vals)
            .map(|j| Name::from(format!("t{j}").as_str()))
            .collect();
        spec.push((Name::from(format!("s{i}").as_str()), values));
    }
    let mut builder = CfnBuilder::new(spec, DEFAULT_WEIGHTS).ok()?;

    for i in 0..n_vars {
        let var = VarId::new(u32::try_from(i).ok()?);
        let slots = builder.variable(var)?.slots();
        let table: Vec<Cost> = (0..slots).map(|_| draw_cost(u)).collect();
        builder.add_unary_table(var, &table).ok()?;
    }

    let n_functions = usize::from(u.arbitrary::<u8>().ok()? % 6);
    for _ in 0..n_functions {
        let a = usize::from(u.arbitrary::<u8>().ok()?) % n_vars;
        let b = usize::from(u.arbitrary::<u8>().ok()?) % n_vars;
        if a == b {
            continue;
        }
        let (low, high) = if a < b { (a, b) } else { (b, a) };
        let scope = [
            VarId::new(u32::try_from(low).ok()?),
            VarId::new(u32::try_from(high).ok()?),
        ];
        let len = builder.table_length(&scope)?;
        let table: Vec<Cost> = (0..len).map(|_| draw_cost(u)).collect();
        builder.add_function(&scope, table).ok()?;
    }

    Some(builder.build())
}

/// The network as Rust source, so that a failure is a unit test rather than a
/// corpus file.
fn describe(cfn: &Cfn) -> String {
    let mut out = String::new();
    out.push_str("let spec = vec![");
    for var in cfn.variable_ids() {
        let variable = cfn.variable(var).expect("a variable");
        out.push_str(&format!(
            "(Name::from(\"{}\"), vec![{}]), ",
            variable.name(),
            variable
                .values()
                .iter()
                .map(|v| format!("Name::from(\"{v}\")"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    out.push_str("];\n");
    for var in cfn.variable_ids() {
        let table = cfn.unary(var).expect("a unary table");
        out.push_str(&format!(
            "// unary {}: {:?}\n",
            var.raw(),
            table.iter().map(|c| c.raw()).collect::<Vec<_>>()
        ));
    }
    for function in cfn.functions() {
        out.push_str(&format!(
            "// scope {:?}: {:?}\n",
            function.scope().iter().map(|v| v.raw()).collect::<Vec<_>>(),
            function.table().iter().map(|c| c.raw()).collect::<Vec<_>>()
        ));
    }
    out
}

/// Every complete assignment over the network's *current* domains, with its
/// cost. Small by construction: at most five variables of at most four slots.
fn enumerate(network: &Network) -> Vec<(Vec<ValId>, Cost)> {
    let vars: Vec<VarId> = network.variable_ids().collect();
    let mut rows: Vec<Vec<ValId>> = vec![Vec::new()];
    for var in &vars {
        let values: Vec<ValId> = network.domain(*var).iter().collect();
        let mut next = Vec::new();
        for row in &rows {
            for value in &values {
                let mut extended = row.clone();
                extended.push(*value);
                next.push(extended);
            }
        }
        rows = next;
    }
    rows.into_iter()
        .map(|row| {
            let cost = network.valuation(&row);
            (row, cost)
        })
        .collect()
}

fuzz_target!(|data: &[u8]| {
    let mut u = Unstructured::new(data);
    let Some(cfn) = draw_network(&mut u) else {
        return;
    };
    let top = Cost::TOP_SENTINEL;

    // The whole-assignment cost of every tuple, before anything moves. The
    // network holds no domain pruning yet, so this is the full product space.
    let baseline: std::collections::HashMap<Vec<ValId>, Cost> = {
        let network = Network::from_cfn(&cfn, top);
        enumerate(&network).into_iter().collect()
    };

    let mut bounds = Vec::new();
    for level in ConsistencyLevel::ALL {
        let mut network = Network::from_cfn(&cfn, top);
        let before = network.c_empty();
        let feasible = network.enforce(level);

        // 1. The budget is a failure condition, not a timeout.
        assert!(
            !network.budget_exhausted(),
            "{} exhausted its step budget of {} on a network of {} variables and {} functions",
            level.label(),
            network.step_budget(),
            network.n_variables(),
            network.n_functions()
        );

        // 3. `c_∅` never decreases.
        let after = network.c_empty();
        assert!(
            after >= before,
            "{} lowered c_∅ from {before:?} to {after:?}",
            level.label()
        );
        bounds.push((level, after, feasible));

        if !feasible {
            continue;
        }

        // 2. The predicate checker agrees.
        let holds = match level {
            ConsistencyLevel::Node => network.is_nc_star(),
            ConsistencyLevel::Arc => network.is_ac_star(),
            ConsistencyLevel::DirectionalArc => network.is_dac_star(),
            ConsistencyLevel::FullDirectionalArc => network.is_fdac_star(),
            ConsistencyLevel::ExistentialDirectionalArc => network.is_edac_star(),
        };
        assert!(
            holds,
            "{} reported success but its own predicate does not hold",
            level.label()
        );

        // 5. Enforcement preserves the cost of every surviving assignment.
        for (row, cost) in enumerate(&network) {
            let Some(original) = baseline.get(&row) else {
                panic!("{} invented the assignment {row:?}", level.label());
            };
            assert_eq!(
                cost,
                *original,
                "{} changed the cost of {row:?} from {original:?} to {cost:?}",
                level.label()
            );
        }
    }

    // 4. The ordered chain, on the bound. Only the pairs the module claims
    //    are ordered are compared: AC* and DAC* are incomparable.
    let bound_of = |wanted: ConsistencyLevel| {
        bounds
            .iter()
            .find(|(level, _, _)| *level == wanted)
            .map(|(_, bound, _)| *bound)
    };
    let chain = [
        (ConsistencyLevel::Node, ConsistencyLevel::Arc),
        (ConsistencyLevel::Arc, ConsistencyLevel::FullDirectionalArc),
        (
            ConsistencyLevel::FullDirectionalArc,
            ConsistencyLevel::ExistentialDirectionalArc,
        ),
        (ConsistencyLevel::Node, ConsistencyLevel::DirectionalArc),
        // `(DirectionalArc, FullDirectionalArc)` is deliberately absent. It was
        // once documented as ordered and is not: `enforce_fdac_star` shares its
        // prefix with `enforce_ac_star` rather than with `enforce_dac_star`, and
        // closures are not unique, so DAC* can reach the strictly larger bound.
        // This target is what refuted it, at roughly one input in 1,100; the
        // `dac_star_can_beat_fdac_star_on_the_bound` unit test now pins a
        // counterexample, which is where the fact belongs.
    ];
    for (weaker, stronger) in chain {
        let (Some(low), Some(high)) = (bound_of(weaker), bound_of(stronger)) else {
            continue;
        };
        assert!(
            high >= low,
            "{} bound {high:?} is below the {} bound {low:?}\n{}",
            stronger.label(),
            weaker.label(),
            describe(&cfn)
        );
    }
});
