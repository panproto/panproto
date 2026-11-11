//! Binder names in signatures.
//!
//! The parameters of a dependent sort and the inputs of an operation form
//! a telescope whose variables are pairwise distinct: palmgren2019categories
//! Def 4.1 asks `x_k` to lie outside `{x_1, ..., x_(k-1)}`, and rule R2 extends
//! a context only by a fresh variable (stated). `typecheck_signature`
//! reports a repeated name as `DuplicateBinder` at the later declaration,
//! and `compose` refuses a node that has one as `InvalidNode`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use panproto_gat::{
    ClashPolicy, ComposeOptions, CompositionError, CompositionMode, DEFAULT_VERIFY_BUDGET,
    DiagramNode, Operation, Sort, SortExpr, SortParam, Term, Theory, TheoryDiagram, compose,
    typecheck_signature,
};

fn sorts_xyh() -> Vec<Sort> {
    vec![
        Sort::simple("X"),
        Sort::dependent("Y", vec![SortParam::new("x", "X")]),
        Sort::dependent(
            "H",
            vec![SortParam::new("x", "X"), SortParam::new("y", "X")],
        ),
    ]
}

fn sort_app(head: &str, vars: &[&str]) -> SortExpr {
    SortExpr::app(head, vars.iter().map(|v| Term::var(*v)).collect())
}

fn binary(name: &str, a: &str, b: &str, out: SortExpr) -> Operation {
    Operation::new(
        name,
        vec![(a.into(), "X".into()), (b.into(), "X".into())],
        out,
    )
}

#[test]
fn typecheck_signature_refuses_repeated_binder_names() {
    let op_theory = Theory::new(
        "T",
        sorts_xyh(),
        vec![binary("f", "z", "z", sort_app("H", &["z", "z"]))],
        vec![],
    );
    let observed = typecheck_signature(&op_theory);
    eprintln!("observed (operation): {observed:?}");
    let json = serde_json::to_value(observed.as_ref().err()).unwrap();
    assert_eq!(
        json,
        serde_json::json!([{"duplicate_binder": {
            "site": {"op_input": {"op": "f", "index": 1}},
            "name": "z"
        }}])
    );

    let sort_theory = Theory::new(
        "T",
        vec![
            Sort::simple("X"),
            Sort::dependent(
                "Y",
                vec![SortParam::new("x", "X"), SortParam::new("x", "X")],
            ),
        ],
        vec![],
        vec![],
    );
    let observed = typecheck_signature(&sort_theory);
    eprintln!("observed (sort): {observed:?}");
    let json = serde_json::to_value(observed.as_ref().err()).unwrap();
    assert_eq!(
        json,
        serde_json::json!([{"duplicate_binder": {
            "site": {"sort_param": {"sort": "Y", "index": 1}},
            "name": "x"
        }}])
    );
    if let Err(defects) = &observed {
        let rendered = defects[0].to_string();
        eprintln!("observed message: {rendered}");
        assert!(
            rendered.contains("parameter 1 of sort `Y`") && rendered.contains("`x`"),
            "the message names the site and the binder: {rendered}"
        );
    }
}

#[test]
fn compose_refuses_a_node_with_a_repeated_binder() {
    let node = Theory::new(
        "R",
        sorts_xyh(),
        vec![binary("f", "z", "z", sort_app("H", &["z", "z"]))],
        vec![],
    );
    let diagram = TheoryDiagram {
        nodes: vec![DiagramNode {
            label: Arc::from("R"),
            theory: node,
        }],
        edges: vec![],
    };
    let outcome = compose(
        &diagram,
        &ComposeOptions {
            mode: CompositionMode::Amalgamate,
            result_name: Arc::from("Q"),
            clash: ClashPolicy::Freshen,
            rewrite_budget: DEFAULT_VERIFY_BUDGET,
        },
    );
    eprintln!("observed: {outcome:?}");
    let Err(CompositionError::InvalidNode { label, defects }) = &outcome else {
        panic!("expected InvalidNode, observed {outcome:?}");
    };
    assert_eq!(label.as_ref(), "R");
    assert_eq!(
        serde_json::to_value(defects).unwrap(),
        serde_json::json!([{"duplicate_binder": {
            "site": {"op_input": {"op": "f", "index": 1}},
            "name": "z"
        }}])
    );
}
