//! Verify that the `class!` proc-macro expands to a theory builder
//! whose output has the expected shape.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use panproto_gat_macros::class;

class! {
    ThEq<A, Bool> {
        eq(x: A, y: A) -> Bool;
        neq(x: A, y: A) -> Bool;

        axiom sym: eq(x, y) = eq(y, x);
    }
}

#[test]
fn class_macro_builds_expected_theory() {
    let th = theory_theq();
    assert_eq!(&*th.name, "ThEq");
    assert_eq!(th.sorts.len(), 2);
    assert!(th.find_sort("A").is_some());
    assert!(th.find_sort("Bool").is_some());
    assert_eq!(th.ops.len(), 2);
    assert!(th.find_op("eq").is_some());
    assert!(th.find_op("neq").is_some());
    assert_eq!(th.eqs.len(), 1);
    assert_eq!(&*th.eqs[0].name, "sym");
}
