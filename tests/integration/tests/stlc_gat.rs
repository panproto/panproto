//! Simply-typed lambda calculus (STLC) encoded as a GAT.
//!
//! Tests the full dependent-sort stack: context, type, and term-in-context
//! sorts; type constructors; typed introduction/elimination rules; and a
//! representative equation (beta via explicit substitution).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::similar_names,
    clippy::too_many_lines
)]

use std::sync::Arc;

use panproto_gat::{
    Equation, Operation, Sort, SortExpr, SortParam, Term, Theory, VarContext, typecheck_term,
    typecheck_theory,
};

fn stlc_theory() -> Theory {
    // Sorts.
    let ctx = Sort::simple("Ctx");
    let ty = Sort::simple("Ty");
    let tm = Sort::dependent(
        "Tm",
        vec![SortParam::new("G", "Ctx"), SortParam::new("A", "Ty")],
    );

    // Operations.
    let arrow = Operation::new(
        "arrow",
        vec![
            (Arc::from("A"), SortExpr::from("Ty")),
            (Arc::from("B"), SortExpr::from("Ty")),
        ],
        "Ty",
    );
    let extend = Operation::new(
        "extend",
        vec![
            (Arc::from("G"), SortExpr::from("Ctx")),
            (Arc::from("A"), SortExpr::from("Ty")),
        ],
        "Ctx",
    );
    let empty_ctx = Operation::nullary("emptyCtx", "Ctx");

    // var_zero : (G: Ctx, A: Ty) -> Tm(extend(G, A), A)
    let var_zero_out = SortExpr::App {
        name: Arc::from("Tm"),
        args: vec![
            Term::app("extend", vec![Term::var("G"), Term::var("A")]),
            Term::var("A"),
        ],
    };
    let var_zero = Operation::new(
        "var_zero",
        vec![
            (Arc::from("G"), SortExpr::from("Ctx")),
            (Arc::from("A"), SortExpr::from("Ty")),
        ],
        var_zero_out,
    );

    // lam : (G: Ctx, A: Ty, B: Ty, body: Tm(extend(G, A), B)) -> Tm(G, arrow(A, B))
    let lam_body_sort = SortExpr::App {
        name: Arc::from("Tm"),
        args: vec![
            Term::app("extend", vec![Term::var("G"), Term::var("A")]),
            Term::var("B"),
        ],
    };
    let lam_out = SortExpr::App {
        name: Arc::from("Tm"),
        args: vec![
            Term::var("G"),
            Term::app("arrow", vec![Term::var("A"), Term::var("B")]),
        ],
    };
    let lam = Operation::new(
        "lam",
        vec![
            (Arc::from("G"), SortExpr::from("Ctx")),
            (Arc::from("A"), SortExpr::from("Ty")),
            (Arc::from("B"), SortExpr::from("Ty")),
            (Arc::from("body"), lam_body_sort),
        ],
        lam_out,
    );

    // app : (G, A, B, f: Tm(G, arrow(A, B)), x: Tm(G, A)) -> Tm(G, B)
    let app_f_sort = SortExpr::App {
        name: Arc::from("Tm"),
        args: vec![
            Term::var("G"),
            Term::app("arrow", vec![Term::var("A"), Term::var("B")]),
        ],
    };
    let app_x_sort = SortExpr::App {
        name: Arc::from("Tm"),
        args: vec![Term::var("G"), Term::var("A")],
    };
    let app_out = SortExpr::App {
        name: Arc::from("Tm"),
        args: vec![Term::var("G"), Term::var("B")],
    };
    let app_op = Operation::new(
        "app",
        vec![
            (Arc::from("G"), SortExpr::from("Ctx")),
            (Arc::from("A"), SortExpr::from("Ty")),
            (Arc::from("B"), SortExpr::from("Ty")),
            (Arc::from("f"), app_f_sort),
            (Arc::from("x"), app_x_sort),
        ],
        app_out,
    );

    // subst : (G, A, B, body: Tm(extend(G, A), B), x: Tm(G, A)) -> Tm(G, B)
    let subst_body_sort = SortExpr::App {
        name: Arc::from("Tm"),
        args: vec![
            Term::app("extend", vec![Term::var("G"), Term::var("A")]),
            Term::var("B"),
        ],
    };
    let subst_x_sort = SortExpr::App {
        name: Arc::from("Tm"),
        args: vec![Term::var("G"), Term::var("A")],
    };
    let subst_out = SortExpr::App {
        name: Arc::from("Tm"),
        args: vec![Term::var("G"), Term::var("B")],
    };
    let subst_op = Operation::new(
        "subst",
        vec![
            (Arc::from("G"), SortExpr::from("Ctx")),
            (Arc::from("A"), SortExpr::from("Ty")),
            (Arc::from("B"), SortExpr::from("Ty")),
            (Arc::from("body"), subst_body_sort),
            (Arc::from("x"), subst_x_sort),
        ],
        subst_out,
    );

    // Beta equation: app(G, A, B, lam(G, A, B, body), x) = subst(G, A, B, body, x)
    let beta = Equation::new(
        "beta",
        Term::app(
            "app",
            vec![
                Term::var("G"),
                Term::var("A"),
                Term::var("B"),
                Term::app(
                    "lam",
                    vec![
                        Term::var("G"),
                        Term::var("A"),
                        Term::var("B"),
                        Term::var("body"),
                    ],
                ),
                Term::var("x"),
            ],
        ),
        Term::app(
            "subst",
            vec![
                Term::var("G"),
                Term::var("A"),
                Term::var("B"),
                Term::var("body"),
                Term::var("x"),
            ],
        ),
    );

    Theory::new(
        "STLC",
        vec![ctx, ty, tm],
        vec![arrow, extend, empty_ctx, var_zero, lam, app_op, subst_op],
        vec![beta],
    )
}

#[test]
fn stlc_theory_typechecks() -> Result<(), Box<dyn std::error::Error>> {
    let th = stlc_theory();
    typecheck_theory(&th)?;
    Ok(())
}

#[test]
fn stlc_var_zero_has_expected_sort() -> Result<(), Box<dyn std::error::Error>> {
    let th = stlc_theory();
    let mut ctx = VarContext::default();
    ctx.insert(Arc::from("G"), SortExpr::from("Ctx"));
    ctx.insert(Arc::from("A"), SortExpr::from("Ty"));
    let t = Term::app("var_zero", vec![Term::var("G"), Term::var("A")]);
    let sort = typecheck_term(&t, &ctx, &th)?;
    assert_eq!(&**sort.head(), "Tm");
    assert_eq!(sort.args().len(), 2);
    // First arg should be `extend(G, A)`; second `A`.
    assert_eq!(
        sort.args()[0],
        Term::app("extend", vec![Term::var("G"), Term::var("A")])
    );
    assert_eq!(sort.args()[1], Term::var("A"));
    Ok(())
}

#[test]
fn stlc_well_formed_application_typechecks() -> Result<(), Box<dyn std::error::Error>> {
    let th = stlc_theory();
    let mut ctx = VarContext::default();
    ctx.insert(Arc::from("G"), SortExpr::from("Ctx"));
    ctx.insert(Arc::from("A"), SortExpr::from("Ty"));
    ctx.insert(Arc::from("B"), SortExpr::from("Ty"));
    ctx.insert(
        Arc::from("f"),
        SortExpr::App {
            name: Arc::from("Tm"),
            args: vec![
                Term::var("G"),
                Term::app("arrow", vec![Term::var("A"), Term::var("B")]),
            ],
        },
    );
    ctx.insert(
        Arc::from("x"),
        SortExpr::App {
            name: Arc::from("Tm"),
            args: vec![Term::var("G"), Term::var("A")],
        },
    );
    let t = Term::app(
        "app",
        vec![
            Term::var("G"),
            Term::var("A"),
            Term::var("B"),
            Term::var("f"),
            Term::var("x"),
        ],
    );
    let sort = typecheck_term(&t, &ctx, &th)?;
    // Result sort must be Tm(G, B).
    assert_eq!(&**sort.head(), "Tm");
    assert_eq!(sort.args()[0], Term::var("G"));
    assert_eq!(sort.args()[1], Term::var("B"));
    Ok(())
}

#[test]
fn stlc_ill_typed_application_rejected() {
    // app expects x : Tm(G, A); give it Tm(G, B) instead (wrong index).
    let th = stlc_theory();
    let mut ctx = VarContext::default();
    ctx.insert(Arc::from("G"), SortExpr::from("Ctx"));
    ctx.insert(Arc::from("A"), SortExpr::from("Ty"));
    ctx.insert(Arc::from("B"), SortExpr::from("Ty"));
    ctx.insert(
        Arc::from("f"),
        SortExpr::App {
            name: Arc::from("Tm"),
            args: vec![
                Term::var("G"),
                Term::app("arrow", vec![Term::var("A"), Term::var("B")]),
            ],
        },
    );
    // x has sort Tm(G, B), but `app` expects its last argument to have
    // sort Tm(G, A).
    ctx.insert(
        Arc::from("x"),
        SortExpr::App {
            name: Arc::from("Tm"),
            args: vec![Term::var("G"), Term::var("B")],
        },
    );
    let t = Term::app(
        "app",
        vec![
            Term::var("G"),
            Term::var("A"),
            Term::var("B"),
            Term::var("f"),
            Term::var("x"),
        ],
    );
    let result = typecheck_term(&t, &ctx, &th);
    assert!(
        result.is_err(),
        "ill-typed application should be rejected, got {result:?}",
    );
}

#[test]
fn stlc_lam_constructs_arrow_term() -> Result<(), Box<dyn std::error::Error>> {
    let th = stlc_theory();
    let mut ctx = VarContext::default();
    ctx.insert(Arc::from("G"), SortExpr::from("Ctx"));
    ctx.insert(Arc::from("A"), SortExpr::from("Ty"));
    ctx.insert(Arc::from("B"), SortExpr::from("Ty"));
    ctx.insert(
        Arc::from("body"),
        SortExpr::App {
            name: Arc::from("Tm"),
            args: vec![
                Term::app("extend", vec![Term::var("G"), Term::var("A")]),
                Term::var("B"),
            ],
        },
    );
    let t = Term::app(
        "lam",
        vec![
            Term::var("G"),
            Term::var("A"),
            Term::var("B"),
            Term::var("body"),
        ],
    );
    let sort = typecheck_term(&t, &ctx, &th)?;
    // Result sort is Tm(G, arrow(A, B)).
    assert_eq!(&**sort.head(), "Tm");
    assert_eq!(sort.args()[0], Term::var("G"));
    assert_eq!(
        sort.args()[1],
        Term::app("arrow", vec![Term::var("A"), Term::var("B")])
    );
    Ok(())
}
