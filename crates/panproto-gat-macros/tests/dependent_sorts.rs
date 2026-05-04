//! Verify that the `class!` and `inductive!` macros accept dependent
//! sorts in argument and output positions, as requested in
//! panproto/panproto#59.
//!
//! The fixture mirrors `crates/panproto-theory-dsl/tests/fixtures/stlc.json`
//! at the operation-shape level: same op names, same arity, same
//! dependent-sort structure. We do not assert structural equality with
//! the JSON-compiled theory because the JSON surface uses sort
//! parameters (`Tm` declared with `params: [G: Ctx, A: Ty]`) and the
//! macro's `class!<Sort, ..>` surface declares simple sorts. The two
//! representations agree on signatures but not on sort metadata; this
//! test exercises the new dependent-sort surface, not full
//! cross-surface equivalence.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use panproto_gat::SortExpr;
use panproto_gat_macros::{class, inductive};

class! {
    StlcMacro<Ctx, Ty, Tm> {
        // Type formers.
        arrow(a: Ty, b: Ty) -> Ty;

        // Context formers.
        extend(g: Ctx, a: Ty) -> Ctx;
        emptyCtx() -> Ctx;

        // Term formers, all sorted in the dependent sort `Tm(_, _)`.
        var_zero(g: Ctx, a: Ty) -> Tm(extend(g, a), a);
        lam(g: Ctx, a: Ty, b: Ty, body: Tm(extend(g, a), b)) -> Tm(g, arrow(a, b));
        app(g: Ctx, a: Ty, b: Ty, f: Tm(g, arrow(a, b)), x: Tm(g, a)) -> Tm(g, b);
        subst(g: Ctx, a: Ty, b: Ty, body: Tm(extend(g, a), b), x: Tm(g, a)) -> Tm(g, b);

        axiom beta:
            app(g, a, b, lam(g, a, b, body), x) = subst(g, a, b, body, x);
    }
}

#[test]
fn class_macro_accepts_dependent_sorts_in_signatures() {
    let th = theory_stlcmacro();
    assert_eq!(&*th.name, "StlcMacro");

    // `app`'s output sort is `Tm(g, b)`, a SortExpr::App with two args.
    let app_op = th.find_op("app").expect("app should be defined");
    let SortExpr::App { name, args } = &app_op.output else {
        panic!("expected dependent output sort, got {:?}", app_op.output);
    };
    assert_eq!(name.as_ref(), "Tm");
    assert_eq!(args.len(), 2);

    // `lam`'s body argument is sorted `Tm(extend(g, a), b)` — the first
    // term-arg is itself an op application, not a bare variable.
    let lam_op = th.find_op("lam").expect("lam should be defined");
    let body_arg = lam_op
        .inputs
        .iter()
        .find(|(name, _, _)| name.as_ref() == "body")
        .expect("lam should have a body arg");
    let SortExpr::App { name, args } = &body_arg.1 else {
        panic!("expected dependent arg sort, got {:?}", body_arg.1);
    };
    assert_eq!(name.as_ref(), "Tm");
    assert_eq!(args.len(), 2);
    // The first arg of `Tm(extend(g, a), b)` is the term `extend(g, a)`.
    match &args[0] {
        panproto_gat::Term::App { op, args: targs } => {
            assert_eq!(op.as_ref(), "extend");
            assert_eq!(targs.len(), 2);
        }
        other => panic!("expected Term::App for extend(g, a), got {other:?}"),
    }

    // `var_zero`'s output is `Tm(extend(g, a), a)`, second arg is a
    // bare variable `a` (Term::Var), not a Term::App.
    let vz = th.find_op("var_zero").expect("var_zero should be defined");
    let SortExpr::App { args, .. } = &vz.output else {
        panic!("expected dependent output sort");
    };
    assert!(matches!(args[1], panproto_gat::Term::Var(_)));

    // The `beta` axiom survived parsing.
    assert_eq!(th.eqs.len(), 1);
    assert_eq!(&*th.eqs[0].name, "beta");
}

inductive! {
    ListNat {
        nil : ListNat,
        cons(x: Nat, xs: ListNat) : ListNat,
    }
}

#[test]
fn inductive_macro_accepts_simple_signatures_unchanged() {
    // Sanity check: the simple-sort path still works, since the
    // dependent-sort change goes through `SortExpr::app` which collapses
    // empty arg lists to `SortExpr::Name`.
    let th = theory_listnat();
    assert_eq!(&*th.name, "ListNat");
    assert_eq!(th.ops.len(), 2);
    let cons = th.find_op("cons").expect("cons should be defined");
    assert!(matches!(cons.output, SortExpr::Name(_)));
}
