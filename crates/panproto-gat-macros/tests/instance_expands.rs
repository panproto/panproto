//! Verify that the `instance!` proc-macro produces a function that
//! builds a valid `TheoryMorphism` against a matching target theory.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use panproto_gat::{Equation, Implicit, Operation, Sort, SortExpr, Term, Theory};
use panproto_gat_macros::{class, instance};

class! {
    ThEq<A, Bool> {
        eq(x: A, y: A) -> Bool;
        neq(x: A, y: A) -> Bool;

        axiom sym: eq(x, y) = eq(y, x);
    }
}

instance! {
    EqInt: ThEq<Int, Bool> in ThArith {
        eq = int_eq;
        neq = int_neq;
    }
}

fn th_arith() -> Theory {
    let int_sort = Sort::simple("Int");
    let bool_sort = Sort::simple("Bool");
    let eq_op = Operation::with_implicit(
        "int_eq",
        vec![
            (
                Arc::from("x"),
                SortExpr::Name(Arc::from("Int")),
                Implicit::No,
            ),
            (
                Arc::from("y"),
                SortExpr::Name(Arc::from("Int")),
                Implicit::No,
            ),
        ],
        SortExpr::Name(Arc::from("Bool")),
    );
    let ne_op = Operation::with_implicit(
        "int_neq",
        vec![
            (
                Arc::from("x"),
                SortExpr::Name(Arc::from("Int")),
                Implicit::No,
            ),
            (
                Arc::from("y"),
                SortExpr::Name(Arc::from("Int")),
                Implicit::No,
            ),
        ],
        SortExpr::Name(Arc::from("Bool")),
    );
    let sym = Equation::new(
        "int_eq_sym",
        Term::App {
            op: Arc::from("int_eq"),
            args: vec![Term::Var(Arc::from("x")), Term::Var(Arc::from("y"))],
        },
        Term::App {
            op: Arc::from("int_eq"),
            args: vec![Term::Var(Arc::from("y")), Term::Var(Arc::from("x"))],
        },
    );
    Theory::new(
        "ThArith",
        vec![int_sort, bool_sort],
        vec![eq_op, ne_op],
        vec![sym],
    )
}

#[test]
fn instance_macro_builds_valid_morphism() -> Result<(), Box<dyn std::error::Error>> {
    let class_th = theory_theq();
    let target = th_arith();
    let morph = instance_eqint(&class_th, &target)?;
    assert_eq!(&*morph.domain, "ThEq");
    assert_eq!(&*morph.codomain, "ThArith");
    assert_eq!(
        morph.sort_map.get(&Arc::from("A")).map(|s| &**s),
        Some("Int")
    );
    assert_eq!(
        morph.sort_map.get(&Arc::from("Bool")).map(|s| &**s),
        Some("Bool")
    );
    assert_eq!(
        morph
            .op_map
            .get(&Arc::from("eq"))
            .and_then(|s| s.as_op())
            .map(|s| &**s),
        Some("int_eq")
    );
    assert_eq!(
        morph
            .op_map
            .get(&Arc::from("neq"))
            .and_then(|s| s.as_op())
            .map(|s| &**s),
        Some("int_neq")
    );
    Ok(())
}
