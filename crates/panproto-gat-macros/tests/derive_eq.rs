//! Verify that `derive_theory!` expands to a theory builder plus a
//! working `Eq` instance function.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use panproto_gat::{Implicit, Operation, Sort, SortExpr, Theory};
use panproto_gat_macros::{class, derive_theory};

// The class `ThEq<A, Bool> { eq(x: A, y: A) -> Bool; }` so that the
// derived instance has a class theory to point at.
class! {
    ThEq<A, Bool> {
        eq(x: A, y: A) -> Bool;
    }
}

// `derive_theory!` introduces ThVertex plus instance_vertex_eq.
derive_theory! {
    #[derive(Eq)]
    ThVertex<Vertex, Bool> {
        name(x: Vertex) -> Bool;
    }
}

fn target_with_eq() -> Theory {
    let sorts = vec![Sort::simple("MyVertex"), Sort::simple("Bool")];
    let ops = vec![Operation::with_implicit(
        "vertex_eq",
        vec![
            (
                Arc::from("x"),
                SortExpr::Name(Arc::from("MyVertex")),
                Implicit::No,
            ),
            (
                Arc::from("y"),
                SortExpr::Name(Arc::from("MyVertex")),
                Implicit::No,
            ),
        ],
        SortExpr::Name(Arc::from("Bool")),
    )];
    Theory::new("ThTarget", sorts, ops, vec![])
}

#[test]
fn derive_eq_builds_valid_instance_morphism() {
    let class_theory = theory_theq();
    let vertex_theory = theory_thvertex();
    let _ = vertex_theory; // The theory function itself just needs to exist.
    let target = target_with_eq();
    let morph = instance_vertex_eq(&class_theory, &target).expect("eq instance must validate");
    assert_eq!(&*morph.domain, "ThEq");
    assert_eq!(&*morph.codomain, "ThTarget");
    assert_eq!(
        morph.op_map.get(&Arc::from("eq")).map(|s| &**s),
        Some("vertex_eq"),
    );
}
