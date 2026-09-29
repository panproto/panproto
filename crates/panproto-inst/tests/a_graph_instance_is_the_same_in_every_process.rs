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
//!
//! The processes share one binary, so they cannot see a difference that
//! comes from how the binary was built. The JSON tests at the end cover
//! that: a node's fields must be written in declaration order whether or not
//! `serde_json`'s `preserve_order` feature is part of the build.

#![allow(
    clippy::expect_used,
    reason = "a fixture that will not build is a defect in this file"
)]

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::process::Command;

use panproto_inst::{EdgeId, GInstance, Node, Value, canonical_ginstance_bytes, to_node_link_json};
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
        "digest" => graph.digest().to_hex(),
        "canonical" => hex(&canonical_ginstance_bytes(&graph)),
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
fn a_graph_digest_is_the_same_whatever_the_hash_seed() {
    assert_one_encoding("digest");
}

#[test]
fn canonical_graph_bytes_are_the_same_whatever_the_hash_seed() {
    assert_one_encoding("canonical");
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

/// The JSON text of node `id` inside a graph's JSON encoding.
fn node_text_in_json(graph: &GInstance, id: u32) -> String {
    let text = serde_json::to_string(graph).expect("the graph serializes");
    let start = text
        .find(&format!("\"{id}\":{{\"id\":{id},"))
        .expect("the node is written with its id first")
        + format!("\"{id}\":").len();
    let mut depth = 0_usize;
    let mut in_string = false;
    let mut escaped = false;
    for (offset, c) in text[start..].char_indices() {
        match c {
            _ if escaped => escaped = false,
            '\\' if in_string => escaped = true,
            '"' => in_string = !in_string,
            '{' if !in_string => depth += 1,
            '}' if !in_string => {
                depth -= 1;
                if depth == 0 {
                    return text[start..=start + offset].to_owned();
                }
            }
            _ => {}
        }
    }
    panic!("the node's object is closed")
}

#[test]
fn a_node_in_a_graphs_json_is_written_as_the_node_writes_itself() {
    let mut node = Node::new(7, "item");
    node.discriminator = Some("kind".into());
    node.position = Some(2);
    node.extra_fields.insert(
        "blob".into(),
        Value::Blob {
            ref_: "r".into(),
            mime: "m".into(),
            size: 1,
        },
    );
    node.annotations.insert(
        "opaque".into(),
        Value::Opaque {
            type_: "t".into(),
            fields: HashMap::from([("x".to_owned(), Value::Float(0.5))]),
        },
    );
    let graph = GInstance::new()
        .with_node(node.clone())
        .with_value(7, Value::Int(3));
    let node = graph.node(7).expect("the node exists").clone();
    assert_eq!(
        node_text_in_json(&graph, 7),
        serde_json::to_string(&node).expect("the node serializes"),
        "a node inside a graph must keep its fields in declaration order"
    );
}

#[test]
fn a_non_finite_float_leaves_a_nodes_field_order_alone() {
    let mut node = Node::new(0, "n");
    node.annotations.insert(
        "o".into(),
        Value::Opaque {
            type_: "t".into(),
            fields: HashMap::from([
                (
                    "b".to_owned(),
                    Value::Blob {
                        ref_: "r".into(),
                        mime: "m".into(),
                        size: 1,
                    },
                ),
                ("x".to_owned(), Value::Float(f64::INFINITY)),
            ]),
        },
    );
    let graph = GInstance::new()
        .with_node(node)
        .with_value(0, Value::Float(f64::NAN));
    assert_eq!(
        node_text_in_json(&graph, 0),
        concat!(
            r#"{"id":0,"anchor":"n","value":{"Present":{"Float":"NaN"}},"#,
            r#""discriminator":null,"extra_fields":{},"position":null,"#,
            r#""shape":{"kind":"plain"},"annotations":{"o":{"Opaque":{"type_":"t","#,
            r#""fields":{"b":{"Blob":{"ref_":"r","mime":"m","size":1}},"#,
            r#""x":{"Float":"Infinity"}}}}}}"#,
        )
    );
}
