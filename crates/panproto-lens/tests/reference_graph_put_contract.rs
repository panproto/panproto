//! Focused identity, isomorphism, plan-compilation and fresh-id evidence for L.3.

#![allow(clippy::expect_used)]

#[path = "support/graph_put_contract.rs"]
mod graph_put_contract;

use std::collections::BTreeSet;

use panproto_reference::enumerate::lenses::{
    for_each_graph_put_contract_case, graph_put_contract_case_count_certificate,
};
use proptest::prelude::*;

use graph_put_contract::{assert_graph_put_contract_case, assert_randomized_graph_put_contract};

#[test]
fn graph_put_contract_certificate() {
    let mut count = 0;
    for_each_graph_put_contract_case(|case| {
        count += 1;
        assert_graph_put_contract_case(case);
    });
    assert_eq!(count, graph_put_contract_case_count_certificate());
    assert_eq!(count, 21);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn randomized_graph_put_contract(
        floor in 0_u16..=4_000,
        taken in proptest::collection::vec(0_u16..=4_000, 0..8),
        dropped_nodes in proptest::collection::vec(0_u16..=4_000, 0..8),
        dropped_edges in proptest::collection::vec(0_u16..=4_000, 0..8),
        synthesized_nodes in proptest::collection::vec(0_u16..=4_000, 0..8),
        synthesized_edges in proptest::collection::vec(0_u16..=4_000, 0..8),
        below_delta in any::<u16>(),
        above_delta in 1_u16..=4_000,
        missing_suffix in "[a-z0-9]{1,12}",
    ) {
        assert_randomized_graph_put_contract(
            u32::from(floor),
            taken.into_iter().map(u32::from).collect::<BTreeSet<_>>(),
            dropped_nodes.into_iter().map(u32::from).collect::<BTreeSet<_>>(),
            dropped_edges.into_iter().map(u32::from).collect::<BTreeSet<_>>(),
            synthesized_nodes.into_iter().map(u32::from).collect::<BTreeSet<_>>(),
            synthesized_edges.into_iter().map(u32::from).collect::<BTreeSet<_>>(),
            below_delta,
            above_delta,
            &missing_suffix,
        );
    }
}
