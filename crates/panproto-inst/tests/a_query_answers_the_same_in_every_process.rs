//! A query answers the same way in every process.
//!
//! Node tables are `HashMap`s, and a `HashMap` enumerates its entries in an
//! order the process's hash seed decides. A query that takes its candidates in
//! that order, and then applies `limit`, returns a different subset in every
//! process; one that follows the first matching edge picks a different edge.
//! The answers here are computed in many separate processes and compared as
//! text. Run a child directly with `PP_QUERY_DUMP` set to a fixture name to see
//! one process's answer.

#![allow(
    clippy::expect_used,
    reason = "a fixture that will not build is a defect in this file"
)]

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::process::Command;

use panproto_expr::{BuiltinOp, Literal};
use panproto_gat::Name;
use panproto_inst::{
    ElementOps, GInstance, InstanceQuery, Node, Value, WInstance, execute_graph, execute_query,
};
use panproto_schema::{Edge, Protocol, Schema, SchemaBuilder};

/// How many separate processes each answer is compared across.
const PROCESSES: usize = 16;

/// How many candidates each fixture holds.
const WIDTH: u32 = 24;

fn schema() -> Schema {
    SchemaBuilder::new(&Protocol::default())
        .vertex("item", "record", None)
        .expect("vertex")
        .vertex("hub", "record", None)
        .expect("vertex")
        .edge("hub", "item", "has", None)
        .expect("edge")
        .build()
        .expect("schema")
}

fn has() -> Edge {
    Edge {
        src: "hub".into(),
        tgt: "item".into(),
        kind: "has".into(),
        name: None,
    }
}

fn tree() -> WInstance {
    let mut nodes = HashMap::new();
    nodes.insert(0, Node::new(0, "hub"));
    let mut arcs = Vec::new();
    for id in 1..=WIDTH {
        nodes.insert(id, Node::new(id, "item"));
        arcs.push((0, id, has()));
    }
    WInstance::new(nodes, arcs, vec![], 0, Name::from("hub"))
}

/// The extra fields every graph item carries: enough keys that an order
/// taken from the hash seed is overwhelmingly unlikely to be sorted.
fn item_fields() -> Vec<(String, Value)> {
    (0..8).map(|k| (format!("f{k}"), Value::Int(k))).collect()
}

/// A hub with `WIDTH` items, reached by edges added in descending target
/// order, plus a parallel second edge to the first item.
fn graph() -> GInstance {
    let mut g = GInstance::new().with_node(Node::new(0, "hub"));
    for id in 1..=WIDTH {
        let node = item_fields()
            .into_iter()
            .fold(Node::new(id, "item"), |n, (k, v)| n.with_extra_field(k, v));
        g = g.with_node(node);
    }
    for id in (1..=WIDTH).rev() {
        g = g.with_edge(0, id, has());
    }
    g.with_edge(0, WIDTH, has())
}

fn limited(anchor: &str) -> InstanceQuery {
    InstanceQuery {
        anchor: Name::from(anchor),
        limit: Some(3),
        ..Default::default()
    }
}

fn answer(fixture: &str) -> String {
    let mut out = String::new();
    match fixture {
        "tree_query" => {
            for m in execute_query(&limited("item"), &tree(), &schema()) {
                let _ = write!(out, "{},", m.node_id);
            }
        }
        "graph_query" => {
            for m in execute_graph(&limited("item"), &graph(), &schema()) {
                let _ = write!(out, "{},", m.node_id);
            }
        }
        "fibers" => {
            let _ = write!(out, "{:?}|", tree().fiber(&Name::from("item")));
            let _ = write!(out, "{:?}", graph().fiber(&Name::from("item")));
        }
        "pushforward" => {
            let _ = write!(out, "{:?}", graph().pushforward(&[0], &Name::from("has")));
        }
        "edges_builtin" => {
            let lit = graph()
                .eval_graph_builtin(
                    BuiltinOp::Edges,
                    &[Literal::Int(0), Literal::Str("has".into())],
                    Some(0),
                )
                .expect("edges evaluates");
            let _ = write!(out, "{lit:?}");
        }
        "edge_builtin" => {
            let lit = graph()
                .eval_graph_builtin(
                    BuiltinOp::Edge,
                    &[Literal::Int(0), Literal::Str("has".into())],
                    Some(0),
                )
                .expect("edge evaluates");
            let _ = write!(out, "{lit:?}");
        }
        other => panic!("no fixture named {other}"),
    }
    out
}

/// The child half: one process's answer, printed.
#[test]
fn one_process_answer() {
    let Some(fixture) = std::env::var_os("PP_QUERY_DUMP") else {
        return;
    };
    print!("<<<{}>>>", answer(&fixture.to_string_lossy()));
}

fn child_answer(exe: &std::path::Path, fixture: &str) -> String {
    let output = Command::new(exe)
        .args(["one_process_answer", "--exact", "--nocapture"])
        .env("PP_QUERY_DUMP", fixture)
        .output()
        .expect("the test binary re-runs");
    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    let start = text.find("<<<").expect("the child printed its answer");
    let end = text.find(">>>").expect("the child closed its answer");
    text[start + 3..end].to_owned()
}

/// Every distinct answer `PROCESSES` separate processes gave, with its count.
fn answers(fixture: &str) -> BTreeMap<String, usize> {
    let exe = std::env::current_exe().expect("the test binary knows its own path");
    let mut seen = BTreeMap::new();
    for _ in 0..PROCESSES {
        *seen.entry(child_answer(&exe, fixture)).or_insert(0) += 1;
    }
    seen
}

fn assert_one_answer(fixture: &str, expected: &str) {
    if std::env::var_os("PP_QUERY_DUMP").is_some() {
        return;
    }
    let seen = answers(fixture);
    assert_eq!(
        seen.keys().collect::<Vec<_>>(),
        vec![expected],
        "{PROCESSES} processes answered `{fixture}` as {seen:?}"
    );
}

#[test]
fn a_limited_tree_query_keeps_the_least_node_ids() {
    assert_one_answer("tree_query", "1,2,3,");
}

#[test]
fn a_limited_graph_query_keeps_the_least_node_ids() {
    assert_one_answer("graph_query", "1,2,3,");
}

#[test]
fn a_fiber_lists_its_elements_in_ascending_id() {
    let ids: Vec<u32> = (1..=WIDTH).collect();
    assert_one_answer("fibers", &format!("{ids:?}|{ids:?}"));
}

#[test]
fn a_pushforward_yields_one_target_per_edge_in_edge_order() {
    // Edges were added to targets WIDTH, WIDTH-1, .., 1 and then WIDTH again.
    let mut targets: Vec<u32> = (1..=WIDTH).rev().collect();
    targets.push(WIDTH);
    assert_one_answer("pushforward", &format!("{targets:?}"));
}

/// The record the graph builtins give for item `id`: id, anchor, then the
/// extra fields in ascending key order.
fn item_record(id: u32) -> Literal {
    let mut fields = vec![
        ("_id".into(), Literal::Int(i64::from(id))),
        ("_anchor".into(), Literal::Str("item".into())),
    ];
    fields.extend(
        item_fields()
            .into_iter()
            .map(|(k, v)| (k.as_str().into(), panproto_inst::value_to_expr_literal(&v))),
    );
    Literal::Record(fields)
}

#[test]
fn the_edge_builtin_follows_the_least_matching_edge() {
    assert_one_answer("edge_builtin", &format!("{:?}", item_record(WIDTH)));
}

#[test]
fn the_edges_builtin_lists_every_matching_edge_in_edge_order() {
    let targets = (1..=WIDTH).rev().chain(std::iter::once(WIDTH));
    let expected = Literal::List(targets.map(item_record).collect());
    assert_one_answer("edges_builtin", &format!("{expected:?}"));
}
