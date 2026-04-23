//! Verify that the `inductive!` proc-macro expands to a builder for a
//! closed-sort theory with the expected constructors.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use panproto_gat::SortClosure;
use panproto_gat_macros::inductive;

inductive! {
    Nat {
        zero : Nat,
        succ(n: Nat) : Nat,
    }
}

#[test]
fn inductive_macro_builds_closed_nat_theory() {
    let th = theory_nat();
    assert_eq!(&*th.name, "Nat");
    let sort = th.find_sort("Nat").expect("Nat sort present");
    match &sort.closure {
        SortClosure::Closed(ctors) => {
            let names: Vec<&str> = ctors.iter().map(|c| &**c).collect();
            assert_eq!(names, vec!["zero", "succ"]);
        }
        SortClosure::Open => panic!("Nat sort must be closed"),
    }
    assert!(th.find_op("zero").is_some());
    assert!(th.find_op("succ").is_some());
    panproto_gat::typecheck_theory(&th).expect("inductive Nat must typecheck");
}
