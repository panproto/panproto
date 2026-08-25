//! Writing one instance twice must produce the same bytes twice.
//!
//! A committed data set is content-addressed: the version-control layer
//! serializes the instances, stores the bytes, and names the object by their
//! digest. An instance's node table, its parent and children indices, a fan's
//! labeled children, a node's extra fields and annotations, and the fields of
//! an unanchored record are all `HashMap`s, and a `HashMap` enumerates its
//! entries in an order the process's hash seed decides. Writing them out as
//! they come hands that seed a say in the bytes, so one and the same instance
//! is stored under a different object id in every process: the same data
//! committed twice reads as a change, and the store fills with duplicates of
//! one value.
//!
//! The same argument applies to a complement, which is what a backward
//! migration reads, and is content-addressed the same way.
//!
//! Each test here writes its fixture in many separate processes and compares
//! the bytes. Run a child directly with `PP_INST_BYTES_DUMP` set to the
//! fixture's name to see one process's answer.

#![allow(
    clippy::expect_used,
    reason = "a fixture that will not build is a defect in this file"
)]

use std::collections::BTreeMap;
use std::collections::HashMap;
use std::process::Command;

use panproto_gat::Name;
use panproto_inst::{Complement, Fan, FieldPresence, Node, Value, WInstance};
use panproto_schema::Edge;

/// How many separate processes each answer is compared across.
const PROCESSES: usize = 32;

/// How many entries each map under test holds. Enough that a seed-dependent
/// enumeration order is overwhelmingly unlikely to land on one fixed order.
const WIDTH: u32 = 24;

fn prop_edge(name: &str) -> Edge {
    Edge {
        src: "rec".into(),
        tgt: Name::from(format!("rec.{name}").as_str()),
        kind: "prop".into(),
        name: Some(Name::from(name)),
    }
}

/// A record node with `WIDTH` children, each carrying extra fields, an
/// annotation and an unanchored sub-record, plus a fan over every child.
fn wide_instance() -> WInstance {
    let mut nodes: HashMap<u32, Node> = HashMap::new();
    let mut arcs: Vec<(u32, u32, Edge)> = Vec::new();
    let mut fan = Fan::new("rec.all", 0);

    let mut root = Node::new(0, "rec");
    for index in 0..WIDTH {
        root.extra_fields.insert(
            format!("kept{index}"),
            Value::Unknown(
                (0..4)
                    .map(|inner| (format!("u{index}_{inner}"), Value::Int(i64::from(inner))))
                    .collect(),
            ),
        );
        root.annotations
            .insert(format!("note{index}"), Value::Str(format!("n{index}")));
    }
    nodes.insert(0, root);

    for index in 0..WIDTH {
        let child_id = index + 1;
        let mut child = Node::new(child_id, Name::from(format!("rec.f{index}").as_str()));
        child.value = Some(FieldPresence::Present(Value::Int(i64::from(index))));
        child
            .extra_fields
            .insert(format!("x{index}"), Value::Int(i64::from(index)));
        nodes.insert(child_id, child);
        arcs.push((0, child_id, prop_edge(&format!("f{index}"))));
        fan = fan.with_child(format!("l{index}"), child_id);
    }

    WInstance::new(nodes, arcs, vec![fan], 0, "rec".into())
}

/// A complement with every one of its tables populated.
fn wide_complement() -> Complement {
    let mut complement = Complement::empty();
    for index in 0..WIDTH {
        let id = index + 1;
        let mut dropped = Node::new(id, Name::from(format!("rec.d{index}").as_str()));
        dropped
            .extra_fields
            .insert(format!("y{index}"), Value::Str(format!("v{index}")));
        complement.dropped_nodes.insert(id, dropped);
        complement.original_parent.insert(id, 0);
        complement.contracted_into.insert(id, 0);
        complement
            .contraction_choices
            .insert((0, id), prop_edge(&format!("f{index}")));
        complement
            .arc_edges
            .insert((0, id), prop_edge(&format!("f{index}")));
        complement.original_values.insert(
            id,
            Some(FieldPresence::Present(Value::Int(i64::from(index)))),
        );
        complement.original_extra_fields.insert(
            id,
            (0..4)
                .map(|inner| (format!("e{index}_{inner}"), Value::Int(i64::from(inner))))
                .collect(),
        );
    }
    complement
}

/// The named fixture's `MessagePack` encoding, as hex. `MessagePack` is what
/// the version-control layer stores and hashes.
fn dump(fixture: &str) -> String {
    use std::fmt::Write as _;
    let bytes = match fixture {
        "instance" => rmp_serde::to_vec(&wide_instance()).expect("the fixture serializes"),
        "complement" => rmp_serde::to_vec(&wide_complement()).expect("the fixture serializes"),
        other => panic!("no fixture named {other}"),
    };
    bytes.iter().fold(String::new(), |mut hex, byte| {
        let _ = write!(hex, "{byte:02x}");
        hex
    })
}

/// The child half: one process's bytes, printed.
#[test]
fn one_process_answer() {
    let Some(fixture) = std::env::var_os("PP_INST_BYTES_DUMP") else {
        // Nothing to do: the parents below are what run this with the
        // variable set. Left as a normal test so it type-checks in every run.
        return;
    };
    print!("<<<{}>>>", dump(&fixture.to_string_lossy()));
}

/// Extract what a child process said about `fixture`.
fn child_answer(exe: &std::path::Path, fixture: &str) -> String {
    let output = Command::new(exe)
        .args(["one_process_answer", "--exact", "--nocapture"])
        .env("PP_INST_BYTES_DUMP", fixture)
        .output()
        .expect("the test binary re-runs");
    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    let start = text.find("<<<").expect("the child printed its answer");
    let end = text.find(">>>").expect("the child closed its answer");
    text[start + 3..end].to_owned()
}

/// How many distinct encodings `PROCESSES` separate processes produced.
fn distinct_encodings(fixture: &str) -> usize {
    let exe = std::env::current_exe().expect("the test binary knows its own path");
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    for _ in 0..PROCESSES {
        *seen.entry(child_answer(&exe, fixture)).or_insert(0) += 1;
    }
    seen.len()
}

#[test]
fn an_instance_encodes_the_same_whatever_the_hash_seed() {
    if std::env::var_os("PP_INST_BYTES_DUMP").is_some() {
        return;
    }
    let distinct = distinct_encodings("instance");
    assert_eq!(
        distinct, 1,
        "{PROCESSES} processes wrote {distinct} different encodings of one instance"
    );
}

#[test]
fn a_complement_encodes_the_same_whatever_the_hash_seed() {
    if std::env::var_os("PP_INST_BYTES_DUMP").is_some() {
        return;
    }
    let distinct = distinct_encodings("complement");
    assert_eq!(
        distinct, 1,
        "{PROCESSES} processes wrote {distinct} different encodings of one complement"
    );
}

#[test]
fn an_instance_still_round_trips() {
    let instance = wide_instance();
    let bytes = rmp_serde::to_vec(&instance).expect("the fixture serializes");
    let back: WInstance = rmp_serde::from_slice(&bytes).expect("the encoding parses");
    assert_eq!(back.node_count(), instance.node_count());
    assert_eq!(back.arcs.len(), instance.arcs.len());
    assert_eq!(back.fans[0].children, instance.fans[0].children);
    assert_eq!(
        back.nodes[&0].extra_fields, instance.nodes[&0].extra_fields,
        "an unanchored record survives the round trip"
    );
    assert_eq!(back.nodes[&0].annotations, instance.nodes[&0].annotations);
    assert_eq!(back.parent_map, instance.parent_map);
    assert_eq!(back.children_map, instance.children_map);
}

#[test]
fn a_complement_still_round_trips() {
    let complement = wide_complement();
    let bytes = rmp_serde::to_vec(&complement).expect("the fixture serializes");
    let back: Complement = rmp_serde::from_slice(&bytes).expect("the encoding parses");
    assert_eq!(back.dropped_nodes.len(), complement.dropped_nodes.len());
    assert_eq!(back.original_parent, complement.original_parent);
    assert_eq!(back.contracted_into, complement.contracted_into);
    assert_eq!(back.contraction_choices, complement.contraction_choices);
    assert_eq!(back.arc_edges, complement.arc_edges);
    assert_eq!(back.original_values, complement.original_values);
    assert_eq!(back.original_extra_fields, complement.original_extra_fields);
}
