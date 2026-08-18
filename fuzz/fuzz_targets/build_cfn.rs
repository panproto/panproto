#![no_main]
//! Fuzz target for the constraint-network builder.
//!
//! The builder is where the schema pair becomes an optimisation problem, so
//! everything the solver assumes about its input is established here:
//!
//! 1. Building never panics, and either poses a network or reports why not.
//! 2. Every cost lies in `[⊥, ⊤]`, and no *finite* cost reaches the top
//!    sentinel by accident: a cost at `⊤` declares an assignment infeasible.
//! 3. Scopes are strictly ascending and no two cost functions share one.
//! 4. `⊥` is in every domain on the span path, which is what makes the
//!    all-`⊥` assignment feasible and the search unable to refuse.
//! 5. The all-`⊥` assignment scores below `⊤`, which is that claim executed
//!    rather than argued.
//! 6. Weights are a probability vector, and an arbitrary legal weighting does
//!    not change which assignments are feasible, only their costs.
//!
//! Run with:
//!
//! ```text
//! cargo fuzz run build_cfn -- -max_total_time=300
//! ```

use arbitrary::Unstructured;
use libfuzzer_sys::fuzz_target;
use panproto_mig::solve::DEFAULT_MEM_BYTES;
use panproto_mig::solve::build::{NoEvidence, build_cfn};
use panproto_mig::{
    Assignment, Cfn, Cost, CostWeights, DEFAULT_WEIGHTS, DomainConstraints, SearchOptions, ValId,
};

mod model;

/// The structural invariants of a posed network.
fn check_network(cfn: &Cfn, bottom_feasible: bool) {
    let top = Cost::TOP_SENTINEL;

    // 2. Costs stay inside the structure.
    for var in cfn.variable_ids() {
        let table = cfn
            .unary(var)
            .expect("every variable of the network has a unary table");
        let variable = cfn.variable(var).expect("and a description");
        assert_eq!(
            table.len(),
            variable.slots(),
            "a unary table must have one entry per slot"
        );
        for cost in table {
            assert!(*cost <= top, "a unary cost exceeded ⊤");
        }
    }

    // 3. Scope uniqueness and ordering.
    let mut scopes: Vec<Vec<_>> = Vec::new();
    for function in cfn.functions() {
        let scope = function.scope();
        assert!(scope.len() >= 2, "a cost function has arity below two");
        for pair in scope.windows(2) {
            assert!(
                pair[0] < pair[1],
                "scope {scope:?} is not strictly ascending"
            );
        }
        let owned: Vec<_> = scope.to_vec();
        assert!(
            !scopes.contains(&owned),
            "two cost functions share the scope {owned:?}"
        );
        scopes.push(owned);

        let mut expected = 1usize;
        for var in scope {
            expected = expected.saturating_mul(
                cfn.variable(*var)
                    .expect("a scope names variables of this network")
                    .slots(),
            );
        }
        assert_eq!(
            function.table().len(),
            expected,
            "the table of {scope:?} is not the product of its slot counts"
        );
        for cost in function.table() {
            assert!(*cost <= top, "a function cost exceeded ⊤");
        }
    }

    if bottom_feasible {
        // 4. `⊥` is in every domain.
        for var in cfn.variable_ids() {
            let domain = cfn
                .domain(var)
                .expect("every variable of the network has a domain");
            assert!(
                domain.contains(ValId::BOTTOM),
                "⊥ left the domain of variable {var:?}, so dropping it is impossible"
            );
        }
        // 5. And the all-`⊥` assignment is feasible.
        let nothing = Assignment::all_bottom(cfn.n_variables());
        assert!(
            cfn.evaluate(&nothing) < top,
            "the all-⊥ assignment is infeasible, so the search can refuse"
        );
    }
}

fuzz_target!(|data: &[u8]| {
    let mut u = Unstructured::new(data);
    let Some((src, tgt)) = model::pair(&mut u, 9) else {
        return;
    };

    let monic = u.arbitrary::<bool>().unwrap_or(false);
    let iso = u.arbitrary::<bool>().unwrap_or(false);
    let opts = SearchOptions {
        monic,
        iso,
        ..SearchOptions::default()
    };

    // Arbitrary weights, taken through the constructor so that only legal
    // weightings reach the builder.
    let raw: [f64; 5] = [
        u.arbitrary::<f64>().unwrap_or(1.0),
        u.arbitrary::<f64>().unwrap_or(1.0),
        u.arbitrary::<f64>().unwrap_or(1.0),
        u.arbitrary::<f64>().unwrap_or(1.0),
        u.arbitrary::<f64>().unwrap_or(1.0),
    ];
    let weights = match CostWeights::new(raw[0], raw[1], raw[2], raw[3], raw[4]) {
        Ok(w) => {
            let sum = w.name() + w.edge() + w.prop() + w.degree() + w.anchor();
            assert!((sum - 1.0).abs() < 1e-9, "weights sum to {sum}");
            w
        }
        Err(_) => DEFAULT_WEIGHTS,
    };

    let Ok(cfn) = build_cfn(
        &src.schema,
        &tgt.schema,
        &opts,
        &DomainConstraints::default(),
        &NoEvidence,
        weights,
        DEFAULT_MEM_BYTES,
    ) else {
        // A refusal names the domain ceiling and is a legitimate answer.
        return;
    };

    // The span path is the one that keeps `⊥`; the iso path removes it, which
    // the module documents and the search compensates for elsewhere.
    check_network(&cfn, !opts.iso);

    // 6. Reweighting changes costs, not feasibility.
    let Ok(reference) = build_cfn(
        &src.schema,
        &tgt.schema,
        &opts,
        &DomainConstraints::default(),
        &NoEvidence,
        DEFAULT_WEIGHTS,
        DEFAULT_MEM_BYTES,
    ) else {
        return;
    };
    assert_eq!(
        cfn.n_variables(),
        reference.n_variables(),
        "the weighting changed how many variables the network has"
    );
    for var in cfn.variable_ids() {
        let a = cfn.domain(var).expect("a domain");
        let b = reference.domain(var).expect("a domain");
        let left: Vec<_> = a.iter().collect();
        let right: Vec<_> = b.iter().collect();
        assert_eq!(
            left, right,
            "the weighting changed the domain of variable {var:?}"
        );
    }
});
