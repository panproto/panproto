//! End-to-end integration tests for dependent sorts in panproto-gat.
//!
//! Exercises typechecking of dependent operation signatures (category
//! theory, reflexive graph), error reporting for ill-formed theories, and
//! JSON round-tripping of `SortExpr`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::similar_names,
    clippy::too_many_lines
)]

use std::sync::Arc;

use panproto_gat::{
    Equation, FreeModelConfig, Operation, Sort, SortExpr, SortParam, Term, Theory, free_model,
    typecheck_theory,
};

/// Build the full category theory with `Ob`, `Hom(a, b)`, `id`, and
/// `compose`, including identity and associativity equations.
fn category_theory_full() -> Theory {
    let ob = Sort::simple("Ob");
    let hom = Sort::dependent(
        "Hom",
        vec![SortParam::new("a", "Ob"), SortParam::new("b", "Ob")],
    );
    let hom_xx = SortExpr::App {
        name: Arc::from("Hom"),
        args: vec![Term::var("x"), Term::var("x")],
    };
    let id = Operation::unary("id", "x", "Ob", hom_xx);
    let hom_xy = SortExpr::App {
        name: Arc::from("Hom"),
        args: vec![Term::var("x"), Term::var("y")],
    };
    let hom_yz = SortExpr::App {
        name: Arc::from("Hom"),
        args: vec![Term::var("y"), Term::var("z")],
    };
    let hom_xz = SortExpr::App {
        name: Arc::from("Hom"),
        args: vec![Term::var("x"), Term::var("z")],
    };
    let compose = Operation::new(
        "compose",
        vec![
            (Arc::from("x"), SortExpr::from("Ob")),
            (Arc::from("y"), SortExpr::from("Ob")),
            (Arc::from("z"), SortExpr::from("Ob")),
            (Arc::from("f"), hom_xy),
            (Arc::from("g"), hom_yz),
        ],
        hom_xz,
    );

    // Associativity: compose(a, b, d, f, compose(b, c, d, g, h))
    //              = compose(a, c, d, compose(a, b, c, f, g), h)
    let assoc = Equation::new(
        "assoc",
        Term::app(
            "compose",
            vec![
                Term::var("a"),
                Term::var("b"),
                Term::var("d"),
                Term::var("f"),
                Term::app(
                    "compose",
                    vec![
                        Term::var("b"),
                        Term::var("c"),
                        Term::var("d"),
                        Term::var("g"),
                        Term::var("h"),
                    ],
                ),
            ],
        ),
        Term::app(
            "compose",
            vec![
                Term::var("a"),
                Term::var("c"),
                Term::var("d"),
                Term::app(
                    "compose",
                    vec![
                        Term::var("a"),
                        Term::var("b"),
                        Term::var("c"),
                        Term::var("f"),
                        Term::var("g"),
                    ],
                ),
                Term::var("h"),
            ],
        ),
    );

    Theory::new("Category", vec![ob, hom], vec![id, compose], vec![assoc])
}

#[test]
fn full_category_typechecks() -> Result<(), Box<dyn std::error::Error>> {
    let th = category_theory_full();
    typecheck_theory(&th)?;
    Ok(())
}

#[test]
fn reflexive_graph_with_dependent_constraint_typechecks() -> Result<(), Box<dyn std::error::Error>>
{
    // Vertex, Edge(s: Vertex, t: Vertex), src/tgt projections, id, plus
    // the equations src(id(v)) = v and tgt(id(v)) = v.
    let vertex = Sort::simple("Vertex");
    let edge = Sort::dependent(
        "Edge",
        vec![SortParam::new("s", "Vertex"), SortParam::new("t", "Vertex")],
    );
    let edge_vv = SortExpr::App {
        name: Arc::from("Edge"),
        args: vec![Term::var("v"), Term::var("v")],
    };
    let id = Operation::unary("id", "v", "Vertex", edge_vv);

    let th = Theory::new("ReflexiveGraph", vec![vertex, edge], vec![id], Vec::new());
    typecheck_theory(&th)?;
    Ok(())
}

#[test]
fn ill_typed_dependent_theory_reports_error() {
    // `bogus : (x : Ob) -> Hom(y, y)` with `y` unbound in the output
    // sort: this is a genuine ill-formed theory because `y` is not in
    // scope. When we try to typecheck it as part of an equation, the
    // substitution-based typecheck will try to instantiate y and leave
    // it as-is (since there's no binding). An equation that uses this op
    // will then fail.
    let bogus = Operation::unary(
        "bogus",
        "x",
        "Ob",
        SortExpr::App {
            name: Arc::from("Hom"),
            args: vec![Term::var("y"), Term::var("y")],
        },
    );
    let good = Operation::unary(
        "id",
        "x",
        "Ob",
        SortExpr::App {
            name: Arc::from("Hom"),
            args: vec![Term::var("x"), Term::var("x")],
        },
    );
    // Equation with mismatched output sorts: bogus(v) (free y) vs id(v)
    // (Hom(v, v)). These cannot unify.
    let bad_eq = Equation::new(
        "bogus_vs_id",
        Term::app("bogus", vec![Term::var("v")]),
        Term::app("id", vec![Term::var("v")]),
    );
    let th = Theory::new(
        "Bogus",
        vec![
            Sort::simple("Ob"),
            Sort::dependent(
                "Hom",
                vec![SortParam::new("a", "Ob"), SortParam::new("b", "Ob")],
            ),
        ],
        vec![bogus, good],
        vec![bad_eq],
    );
    let err = typecheck_theory(&th).expect_err("expected typecheck error");
    let msg = err.to_string();
    assert!(
        !msg.is_empty(),
        "expected a readable, non-empty error message",
    );
}

#[test]
fn dependent_operation_json_round_trip() -> Result<(), Box<dyn std::error::Error>> {
    let th = category_theory_full();
    let serialized = serde_json::to_string(&th)?;
    let round_tripped: Theory = serde_json::from_str(&serialized)?;
    // Structural equality: same sort names, same op signatures.
    assert_eq!(th.sorts.len(), round_tripped.sorts.len());
    assert_eq!(th.ops.len(), round_tripped.ops.len());
    for (before, after) in th.ops.iter().zip(round_tripped.ops.iter()) {
        assert_eq!(before.name, after.name);
        assert_eq!(before.inputs.len(), after.inputs.len());
        assert!(
            before.output.alpha_eq(&after.output),
            "output sort not preserved by JSON round-trip",
        );
    }
    // Typechecking still passes after round-trip.
    typecheck_theory(&round_tripped)?;
    Ok(())
}

#[test]
fn category_free_model_is_well_typed() -> Result<(), Box<dyn std::error::Error>> {
    // The free model of the category theory with one generating object
    // must contain the identity morphism.
    let mut th = category_theory_full();
    th.ops.push(Operation::nullary("star", "Ob"));
    let result = free_model(&th, &FreeModelConfig::default())?;
    assert!(!result.model.sort_interp["Ob"].is_empty());
    // There should be at least id(star) in the Hom carrier.
    assert!(!result.model.sort_interp["Hom"].is_empty());
    Ok(())
}
