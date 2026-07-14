//! Verify that `derive_theory!` generates a working `Hash` instance
//! function.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use panproto_gat::{Implicit, Operation, Sort, SortExpr, Theory};
use panproto_gat_macros::{class, derive_theory};

class! {
    ThHash<A, Int> {
        hash(x: A) -> Int;
    }
}

derive_theory! {
    #[derive(Hash)]
    ThVertex<Vertex, Int> {
        name(x: Vertex) -> Int;
    }
}

fn target_with_hash() -> Theory {
    let sorts = vec![Sort::simple("MyVertex"), Sort::simple("Int")];
    let ops = vec![Operation::with_implicit(
        "vertex_hash",
        vec![(
            Arc::from("x"),
            SortExpr::Name(Arc::from("MyVertex")),
            Implicit::No,
        )],
        SortExpr::Name(Arc::from("Int")),
    )];
    Theory::new("ThTargetHash", sorts, ops, vec![])
}

#[test]
fn derive_hash_builds_valid_instance_morphism() {
    let class_theory = theory_thhash();
    let vertex_theory = theory_thvertex();
    let _ = vertex_theory;
    let target = target_with_hash();
    let morph = instance_vertex_hash(&class_theory, &target).expect("hash instance must validate");
    assert_eq!(&*morph.domain, "ThHash");
    assert_eq!(
        morph
            .op_map
            .get(&Arc::from("hash"))
            .and_then(|s| s.as_op())
            .map(|s| &**s),
        Some("vertex_hash"),
    );
}
