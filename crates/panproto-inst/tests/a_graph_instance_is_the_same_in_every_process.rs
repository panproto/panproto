//! Writing one graph instance twice must produce the same bytes twice.
//!
//! A graph instance's node and edge tables are `HashMap`s, and a `HashMap`
//! enumerates its entries in an order the process's hash seed decides. Graph
//! instances are content-addressed the way trees are, so an encoding that
//! followed that order would store one graph under a different object id in
//! every process. Each test here writes its fixture in many separate
//! processes and compares the bytes. Run a child directly with
//! `PP_GRAPH_BYTES_DUMP` set to the fixture's name to see one process's
//! answer.

#![allow(
    clippy::expect_used,
    reason = "a fixture that will not build is a defect in this file"
)]

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::process::Command;

use panproto_inst::{EdgeId, GInstance, Node, Value, to_node_link_json};
use panproto_schema::Edge;

/// How many separate processes each answer is compared across.
const PROCESSES: usize = 32;

/// How many nodes the fixture holds, each with as many edges out.
const WIDTH: u32 = 24;

/// A dense graph: every node carries extra fields and annotations, and has
/// an edge to every node including itself, some positioned, with a parallel
/// edge beside each self-loop.
fn wide_graph() -> GInstance {
    let mut graph = GInstance::new();
    for id in 0..WIDTH {
        let mut node = Node::new(id, "item");
        for k in 0..4 {
            node.extra_fields
                .insert(format!("f{id}_{k}"), Value::Int(i64::from(k)));
            node.annotations
                .insert(format!("a{id}_{k}"), Value::Str(format!("n{k}")));
        }
        graph = graph
            .with_node(node)
            .with_value(id, Value::Int(i64::from(id)));
    }
    let link = Edge {
        src: "item".into(),
        tgt: "item".into(),
        kind: "link".into(),
        name: None,
    };
    for src in 0..WIDTH {
        for tgt in 0..WIDTH {
            let id = graph
                .add_edge(src, tgt, link.clone())
                .expect("both endpoints exist");
            if tgt % 3 == 0 {
                graph
                    .set_edge_position(id, Some(WIDTH - tgt))
                    .expect("the edge exists");
            }
        }
        graph = graph.with_edge(src, src, link.clone());
    }
    graph
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut out, byte| {
        let _ = write!(out, "{byte:02x}");
        out
    })
}

/// The named encoding of the fixture.
fn dump(fixture: &str) -> String {
    let graph = wide_graph();
    match fixture {
        "msgpack" => hex(&rmp_serde::to_vec(&graph).expect("the fixture serializes")),
        "msgpack_named" => hex(&rmp_serde::to_vec_named(&graph).expect("the fixture serializes")),
        "json" => serde_json::to_string(&graph).expect("the fixture serializes"),
        "node_link" => to_node_link_json(&graph).to_string(),
        other => panic!("no fixture named {other}"),
    }
}

/// The child half: one process's bytes, printed.
#[test]
fn one_process_answer() {
    let Some(fixture) = std::env::var_os("PP_GRAPH_BYTES_DUMP") else {
        return;
    };
    print!("<<<{}>>>", dump(&fixture.to_string_lossy()));
}

fn child_answer(exe: &std::path::Path, fixture: &str) -> String {
    let output = Command::new(exe)
        .args(["one_process_answer", "--exact", "--nocapture"])
        .env("PP_GRAPH_BYTES_DUMP", fixture)
        .output()
        .expect("the test binary re-runs");
    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    let start = text.find("<<<").expect("the child printed its answer");
    let end = text.rfind(">>>").expect("the child closed its answer");
    text[start + 3..end].to_owned()
}

fn distinct_encodings(fixture: &str) -> usize {
    let exe = std::env::current_exe().expect("the test binary knows its own path");
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    for _ in 0..PROCESSES {
        *seen.entry(child_answer(&exe, fixture)).or_insert(0) += 1;
    }
    seen.len()
}

fn assert_one_encoding(fixture: &str) {
    if std::env::var_os("PP_GRAPH_BYTES_DUMP").is_some() {
        return;
    }
    let distinct = distinct_encodings(fixture);
    assert_eq!(
        distinct, 1,
        "{PROCESSES} processes wrote {distinct} different {fixture} encodings of one graph"
    );
}

#[test]
fn a_graph_encodes_to_positional_msgpack_the_same_whatever_the_hash_seed() {
    assert_one_encoding("msgpack");
}

#[test]
fn a_graph_encodes_to_named_msgpack_the_same_whatever_the_hash_seed() {
    assert_one_encoding("msgpack_named");
}

#[test]
fn a_graph_encodes_to_json_the_same_whatever_the_hash_seed() {
    assert_one_encoding("json");
}

#[test]
fn a_graph_encodes_to_node_link_json_the_same_whatever_the_hash_seed() {
    assert_one_encoding("node_link");
}

#[test]
fn the_fixture_still_round_trips() {
    let graph = wide_graph();
    let bytes = rmp_serde::to_vec(&graph).expect("the fixture serializes");
    let back: GInstance = rmp_serde::from_slice(&bytes).expect("the encoding parses");
    assert!(GInstance::identical(&back, &graph));
    assert_eq!(
        back.out_edges(0).len(),
        usize::try_from(WIDTH).expect("small") + 1
    );
    assert_eq!(back.edges_between(3, 3), vec![EdgeId(78), EdgeId(99)]);
}
