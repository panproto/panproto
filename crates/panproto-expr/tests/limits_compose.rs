//! A budget is one allowance, not one per subsystem.
//!
//! The gap this closes is not that bounds were missing. Several
//! subsystems already had good local ones. It is that they were
//! unrelated, so what an input was allowed to cost depended on which
//! door it came through, and an operation could be charged afresh at
//! every layer it descended through.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use panproto_expr::limits::{Budget, Resource, ResourceLimits};

fn limits_with(resource: Resource, bound: u64) -> ResourceLimits {
    let mut l = ResourceLimits::unbounded();
    match resource {
        Resource::InputBytes => l.input_bytes = bound,
        Resource::BundleEntries => l.bundle_entries = bound,
        Resource::GraphElements => l.graph_elements = bound,
        Resource::MetadataBytes => l.metadata_bytes = bound,
        Resource::Depth => l.depth = bound,
        Resource::Steps => l.steps = bound,
        Resource::OutputBytes => l.output_bytes = bound,
    }
    l
}

const CUMULATIVE: &[Resource] = &[
    Resource::InputBytes,
    Resource::BundleEntries,
    Resource::GraphElements,
    Resource::MetadataBytes,
    Resource::Steps,
    Resource::OutputBytes,
];

#[test]
fn a_charge_within_the_bound_succeeds() {
    for &r in CUMULATIVE {
        let budget = Budget::new(limits_with(r, 10));
        assert!(budget.charge(r, 10).is_ok(), "{r} at exactly the bound");
    }
}

#[test]
fn a_charge_past_the_bound_names_the_resource_and_the_bound() {
    for &r in CUMULATIVE {
        let budget = Budget::new(limits_with(r, 10));
        let err = budget.charge(r, 11).expect_err("{r} past the bound");
        assert_eq!(err.resource, r);
        assert_eq!(err.limit, 10);
        assert!(
            err.to_string().contains("10") && err.to_string().contains(r.as_str()),
            "the message must name both, got: {err}",
        );
    }
}

/// The property that makes this a policy: charges accumulate, so an
/// input cannot escape a bound by arriving in pieces.
#[test]
fn charges_accumulate_rather_than_being_measured_per_call() {
    for &r in CUMULATIVE {
        let budget = Budget::new(limits_with(r, 10));
        for _ in 0..10 {
            budget.charge(r, 1).expect("each piece is small");
        }
        assert!(
            budget.charge(r, 1).is_err(),
            "{r}: eleven charges of one must exhaust a bound of ten",
        );
    }
}

/// A nested operation draws from the same pool as the one containing
/// it. A budget that reset per subsystem would bound each step and
/// nothing overall, which is what several unrelated local limits
/// already achieved.
#[test]
fn a_cloned_budget_shares_the_allowance_it_was_cloned_from() {
    let budget = Budget::new(limits_with(Resource::Steps, 10));
    let nested = budget.clone();

    budget.charge(Resource::Steps, 6).expect("outer charge");
    assert_eq!(
        nested.consumed(Resource::Steps),
        6,
        "the nested view sees it"
    );

    assert!(
        nested.charge(Resource::Steps, 5).is_err(),
        "the nested operation must not get a fresh allowance",
    );
}

/// Depth rises and falls with a walk, so it is checked against a level
/// rather than a running total. Charging it cumulatively would refuse
/// a wide-but-shallow document.
#[test]
fn depth_is_a_level_not_a_running_total() {
    let budget = Budget::new(limits_with(Resource::Depth, 3));
    for _ in 0..100 {
        budget
            .enter(1)
            .expect("re-entering level one is always fine");
        budget.enter(3).expect("the bound itself is allowed");
    }
    let err = budget.enter(4).expect_err("past the bound");
    assert_eq!(err.resource, Resource::Depth);
    assert_eq!(err.limit, 3);
}

/// Zero is unbounded, and is a choice a Rust caller can make. What the
/// type prevents is inheriting it at a boundary that reads input from
/// elsewhere.
#[test]
fn a_zero_bound_means_unbounded() {
    let budget = Budget::new(ResourceLimits::unbounded());
    for &r in CUMULATIVE {
        assert!(budget.charge(r, u64::MAX / 2).is_ok(), "{r} is unbounded");
    }
    assert!(budget.enter(u64::MAX).is_ok(), "depth is unbounded");
}

#[test]
fn the_defaults_bound_every_resource() {
    let l = ResourceLimits::defaults();
    for &r in CUMULATIVE {
        assert_ne!(l.get(r), 0, "{r} must be bounded by default");
    }
    assert_ne!(l.depth, 0, "depth must be bounded by default");
}

/// A caller that ignores one failure and keeps going must not find the
/// next charge succeeding, or the bound is advisory.
#[test]
fn a_refused_charge_still_counts_against_the_allowance() {
    let budget = Budget::new(limits_with(Resource::InputBytes, 10));
    assert!(budget.charge(Resource::InputBytes, 50).is_err());
    assert!(
        budget.charge(Resource::InputBytes, 1).is_err(),
        "the allowance stays exhausted",
    );
}

/// One resource running out says nothing about the others, so a
/// diagnostic naming the wrong one would send a caller to the wrong
/// setting.
#[test]
fn exhausting_one_resource_leaves_the_others_alone() {
    let mut l = ResourceLimits::unbounded();
    l.steps = 1;
    l.input_bytes = 1_000;
    let budget = Budget::new(l);

    budget
        .charge(Resource::Steps, 2)
        .expect_err("steps run out");
    assert!(
        budget.charge(Resource::InputBytes, 500).is_ok(),
        "input bytes are a separate allowance",
    );
}
