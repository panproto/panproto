//! Focused validation witnesses for the native tree put domain (L.5/L.9).

#![allow(clippy::expect_used)]

#[path = "support/tree_validation.rs"]
mod tree_validation;

use panproto_reference::enumerate::lenses::{
    for_each_tree_validation_case, tree_validation_case_count_certificate,
};
use proptest::prelude::*;

use tree_validation::{assert_randomized_tree_validation, assert_tree_validation_case};

#[test]
fn tree_validation_reference_certificate() {
    let mut count = 0;
    for_each_tree_validation_case(|case| {
        count += 1;
        assert_tree_validation_case(case);
    });
    assert_eq!(count, tree_validation_case_count_certificate());
    assert_eq!(count, 3);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn randomized_tree_validation_square(
        pair in (any::<i16>(), any::<i16>())
            .prop_filter("source keys differ", |(left, right)| left != right),
        id_seed in 0_u16..=10_000,
        required_field in prop_oneof![Just("b"), Just("child")],
        malformed_x in any::<bool>(),
    ) {
        let (left, right) = pair;
        assert_randomized_tree_validation(
            i64::from(left),
            i64::from(right),
            u32::from(id_seed) * 4,
            required_field,
            if malformed_x { "x" } else { "y" },
        );
    }
}
