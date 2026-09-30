//! `compose` gives the same serialized composition in every process.
//!
//! Hash maps are seeded per process, so a composition whose theory,
//! inclusions, provenance or report depended on a map's iteration order
//! would serialize differently from one process to the next. This runs
//! the same composition in 32 child processes and compares the bytes.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use panproto_gat::{
    ClashPolicy, ComposeOptions, CompositionMode, DEFAULT_VERIFY_BUDGET, DiagramEdge, DiagramNode,
    Equation, OpAssignment, Operation, Sort, Term, Theory, TheoryDiagram, TheoryMorphism, compose,
};

const CHILD: &str = "PP_GAT_COMPOSE_BYTES_CHILD";
const PROCESSES: usize = 32;
const WIDTH: usize = 24;

/// A side theory with `WIDTH` sorts and one operation and one equation per
/// sort, so every map in the result is wide enough to expose any
/// dependence on iteration order.
fn side(name: &str) -> Theory {
    let sorts = (0..WIDTH).map(|i| Sort::simple(format!("S{i}"))).collect();
    let ops = (0..WIDTH)
        .map(|i| Operation::unary(format!("f{i}"), "x", format!("S{i}"), format!("S{i}")))
        .collect();
    let eqs = (0..WIDTH)
        .map(|i| {
            Equation::new(
                format!("idem{i}"),
                Term::app(
                    format!("f{i}"),
                    vec![Term::app(format!("f{i}"), vec![Term::var("x")])],
                ),
                Term::app(format!("f{i}"), vec![Term::var("x")]),
            )
        })
        .collect();
    Theory::new(name, sorts, ops, eqs)
}

/// The shared theory: the first half of the sorts and their operations.
fn apex() -> Theory {
    let half = WIDTH / 2;
    Theory::new(
        "Apex",
        (0..half).map(|i| Sort::simple(format!("S{i}"))).collect(),
        (0..half)
            .map(|i| Operation::unary(format!("f{i}"), "x", format!("S{i}"), format!("S{i}")))
            .collect(),
        vec![],
    )
}

fn leg(tgt: &str) -> TheoryMorphism {
    let apex = apex();
    let sorts: HashMap<Arc<str>, Arc<str>> = apex
        .sorts
        .iter()
        .map(|s| (Arc::clone(&s.name), Arc::clone(&s.name)))
        .collect();
    let ops: HashMap<Arc<str>, OpAssignment> = apex
        .ops
        .iter()
        .map(|o| (Arc::clone(&o.name), OpAssignment::Op(Arc::clone(&o.name))))
        .collect();
    TheoryMorphism::new(format!("into_{tgt}"), "Apex", tgt, sorts, ops)
}

/// The composition's serialized bytes: a span whose unshared halves clash
/// by name and are freshened apart.
fn bytes() -> Vec<u8> {
    let diagram = TheoryDiagram {
        nodes: vec![
            DiagramNode {
                label: "Left".into(),
                theory: side("Left"),
            },
            DiagramNode {
                label: "Right".into(),
                theory: side("Right"),
            },
            DiagramNode {
                label: "Apex".into(),
                theory: apex(),
            },
        ],
        edges: vec![
            DiagramEdge {
                src: "Apex".into(),
                tgt: "Left".into(),
                morphism: leg("Left"),
            },
            DiagramEdge {
                src: "Apex".into(),
                tgt: "Right".into(),
                morphism: leg("Right"),
            },
        ],
    };
    let opts = ComposeOptions {
        mode: CompositionMode::ExplicitIdentity,
        result_name: "Composed".into(),
        clash: ClashPolicy::Freshen,
        rewrite_budget: DEFAULT_VERIFY_BUDGET,
    };
    let composition = compose(&diagram, &opts).expect("the span composes");
    assert!(!composition.report.freshened.is_empty());
    serde_json::to_vec(&composition).unwrap()
}

#[test]
fn compose_bytes_are_identical_across_processes() {
    let bytes = String::from_utf8(bytes()).unwrap();
    if std::env::var_os(CHILD).is_some() {
        println!("BYTES:{bytes}");
        return;
    }
    let exe = std::env::current_exe().unwrap();
    let mut seen = BTreeSet::new();
    for _ in 0..PROCESSES {
        let out = std::process::Command::new(&exe)
            .args([
                "--exact",
                "compose_bytes_are_identical_across_processes",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(CHILD, "1")
            .output()
            .unwrap();
        assert!(out.status.success(), "child failed: {out:?}");
        let stdout = String::from_utf8(out.stdout).unwrap();
        let line = stdout
            .lines()
            .find_map(|l| l.split_once("BYTES:").map(|(_, rest)| rest.to_owned()));
        assert!(line.is_some(), "child printed no bytes: {stdout}");
        seen.extend(line);
    }
    assert_eq!(seen.len(), 1, "{} distinct encodings", seen.len());
    assert_eq!(seen.into_iter().next(), Some(bytes));
}
