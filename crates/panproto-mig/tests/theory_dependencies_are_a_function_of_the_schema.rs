//! The dependencies a theory yields must name the same vertices every run.
//!
//! `dependencies_from_theory` turns each equation into a row-existence
//! dependency between the vertex tables of the two sides' sorts. Choosing
//! which vertex stands for a sort is a real choice whenever a schema holds
//! several vertices of one kind, and it used to be made by taking the first
//! match out of the vertex table — that is, by the process's hash seed. The
//! dependency list then differs run to run, and so does everything the chase
//! derives from it.
//!
//! Run the child directly with `PP_CHASE_DEPS_DUMP=1` to see one process's
//! answer.

#![allow(
    clippy::expect_used,
    reason = "a fixture that will not build is a defect in this file"
)]

use std::collections::{BTreeMap, HashMap};
use std::process::Command;

use panproto_gat::{Equation, Name, Operation, Sort, Term, Theory};
use panproto_mig::dependencies_from_theory;
use panproto_schema::{Schema, Vertex};

/// How many separate processes the answer is compared across.
const PROCESSES: usize = 48;

/// Four vertices of kind `A` and four of kind `B`, so naming a vertex for a
/// sort is a choice among several rather than a lookup.
fn schema_with_repeated_kinds() -> Schema {
    let mut vertices = HashMap::new();
    for kind in ["A", "B"] {
        for tag in ["one", "two", "three", "four"] {
            let id = format!("{kind}_{tag}");
            vertices.insert(
                Name::from(id.as_str()),
                Vertex {
                    id: Name::from(id.as_str()),
                    kind: Name::from(kind),
                    nsid: None,
                },
            );
        }
    }

    Schema {
        protocol: "test".into(),
        vertices,
        edges: HashMap::new(),
        hyper_edges: HashMap::new(),
        constraints: HashMap::new(),
        required: HashMap::new(),
        nsids: HashMap::new(),
        entries: Vec::new(),
        variants: HashMap::new(),
        orderings: HashMap::new(),
        recursion_points: HashMap::new(),
        spans: HashMap::new(),
        usage_modes: HashMap::new(),
        nominal: HashMap::new(),
        coercions: HashMap::new(),
        mergers: HashMap::new(),
        defaults: HashMap::new(),
        policies: HashMap::new(),
        outgoing: HashMap::new(),
        incoming: HashMap::new(),
        between: HashMap::new(),
    }
}

/// `A -f-> B -r-> A` with the retraction law `r(f(x)) = x`.
fn retraction_theory() -> Theory {
    let f = Operation::unary("f", "x", "A", "B");
    let r = Operation::unary("r", "y", "B", "A");
    let retract = Equation::new(
        "retract",
        Term::app("r", vec![Term::app("f", vec![Term::var("x")])]),
        Term::var("x"),
    );
    Theory::new(
        "Retract",
        vec![Sort::simple("A"), Sort::simple("B")],
        vec![f, r],
        vec![retract],
    )
}

fn dump() -> String {
    let deps = dependencies_from_theory(&retraction_theory(), &schema_with_repeated_kinds());
    deps.iter()
        .map(|d| format!("{} => {}", d.pattern_vertex, d.consequence_vertex))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The child half: one process's dependency list, printed.
#[test]
fn one_process_answer() {
    if std::env::var_os("PP_CHASE_DEPS_DUMP").is_none() {
        // Nothing to do: the parent below is what runs this with the variable
        // set. Left as a normal test so it type-checks in every run.
        return;
    }
    print!("<<<{}>>>", dump());
}

/// Extract what the child printed between the markers.
fn child_answer(exe: &std::path::Path) -> String {
    let output = Command::new(exe)
        .args(["one_process_answer", "--exact", "--nocapture"])
        .env("PP_CHASE_DEPS_DUMP", "1")
        .output()
        .expect("the test binary re-runs");
    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    let start = text.find("<<<").expect("the child printed its answer");
    let end = text.find(">>>").expect("the child closed its answer");
    text[start + 3..end].to_owned()
}

#[test]
fn the_dependency_list_does_not_depend_on_the_hash_seed() {
    if std::env::var_os("PP_CHASE_DEPS_DUMP").is_some() {
        return;
    }
    let exe = std::env::current_exe().expect("the test binary knows its own path");
    let mut distinct: BTreeMap<String, usize> = BTreeMap::new();
    for _ in 0..PROCESSES {
        *distinct.entry(child_answer(&exe)).or_insert(0) += 1;
    }

    assert_eq!(
        distinct.len(),
        1,
        "{PROCESSES} processes gave {} different dependency lists for one theory and schema:\n{}",
        distinct.len(),
        distinct
            .iter()
            .map(|(answer, count)| format!("--- seen {count} times ---\n{answer}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// The vertex chosen for a sort is the least one of that kind by ID, which is
/// the rule that makes the choice a function of the schema.
#[test]
fn a_sort_is_represented_by_the_least_vertex_of_its_kind() {
    let deps = dependencies_from_theory(&retraction_theory(), &schema_with_repeated_kinds());
    assert!(!deps.is_empty(), "the retraction law yields a dependency");
    for dep in &deps {
        for vertex in [&dep.pattern_vertex, &dep.consequence_vertex] {
            assert!(
                vertex == "A_four" || vertex == "B_four",
                "expected the least vertex of each kind, got {vertex}"
            );
        }
    }
}
