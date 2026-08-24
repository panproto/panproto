//! `derive_theory!` states the axioms its body declares, exactly as `class!`
//! does, so a theory built through it is not silently lawless.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use panproto_gat_macros::{class, derive_theory};

// The same body, once through each macro.
class! {
    ThSemigroupClass<A> {
        op(x: A, y: A) -> A;

        axiom assoc: op(op(x, y), z) = op(x, op(y, z));
    }
}

derive_theory! {
    #[derive(Eq)]
    ThSemigroupDerived<A> {
        op(x: A, y: A) -> A;

        axiom assoc: op(op(x, y), z) = op(x, op(y, z));
    }
}

#[test]
fn derive_theory_states_the_axioms_its_body_declares() {
    let from_class = theory_thsemigroupclass();
    let from_derive = theory_thsemigroupderived();

    assert_eq!(
        from_class.eqs.len(),
        1,
        "the `class!` baseline must carry the declared axiom",
    );
    assert_eq!(
        from_derive.eqs.len(),
        from_class.eqs.len(),
        "an identical body through `derive_theory!` must carry the same equations",
    );

    let derived = &from_derive.eqs[0];
    let baseline = &from_class.eqs[0];
    assert_eq!(&*derived.name, &*baseline.name);
    assert_eq!(derived.lhs, baseline.lhs);
    assert_eq!(derived.rhs, baseline.rhs);
}
