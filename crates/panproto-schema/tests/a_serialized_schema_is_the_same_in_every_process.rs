//! Writing one schema twice must produce the same bytes twice.
//!
//! A schema's edges, its between-index, its orderings, usage modes and
//! coercions are all `HashMap`s whose keys cannot be JSON object keys, so they
//! are written as arrays of pairs. Writing them in the map's own enumeration
//! order hands the process's hash seed a say in the bytes: the same schema
//! serializes differently from run to run, so every write to disk is a
//! spurious diff and every digest taken over those bytes is a different
//! digest.
//!
//! This writes one schema in many separate processes and compares the bytes.
//!
//! Run the child directly with `PP_SCHEMA_BYTES_DUMP=1` to see one process's
//! answer.

#![allow(
    clippy::expect_used,
    reason = "a fixture that will not build is a defect in this file"
)]

use std::collections::BTreeMap;
use std::collections::HashMap;
use std::process::Command;

use panproto_gat::Name;
use panproto_schema::{Edge, Schema, UsageMode, Vertex};

/// How many separate processes the answer is compared across.
const PROCESSES: usize = 48;

/// One object vertex with many named properties, so the edge table, the
/// between-index, the ordering table and the usage-mode table each hold enough
/// entries for their enumeration order to be visible.
fn wide_schema() -> Schema {
    let mut vertices = HashMap::new();
    vertices.insert(
        Name::from("rec"),
        Vertex {
            id: "rec".into(),
            kind: "object".into(),
            nsid: None,
        },
    );

    let mut edges = HashMap::new();
    let mut between: HashMap<(Name, Name), smallvec::SmallVec<Edge, 2>> = HashMap::new();
    let mut outgoing: HashMap<Name, smallvec::SmallVec<Edge, 4>> = HashMap::new();
    let mut incoming: HashMap<Name, smallvec::SmallVec<Edge, 4>> = HashMap::new();
    let mut orderings = HashMap::new();
    let mut usage_modes = HashMap::new();

    for index in 0..32u32 {
        let field = format!("rec.f{index}");
        vertices.insert(
            Name::from(field.as_str()),
            Vertex {
                id: Name::from(field.as_str()),
                kind: "string".into(),
                nsid: None,
            },
        );
        let edge = Edge {
            src: "rec".into(),
            tgt: Name::from(field.as_str()),
            kind: "prop".into(),
            name: Some(Name::from(format!("f{index}").as_str())),
        };
        edges.insert(edge.clone(), Name::from("prop"));
        orderings.insert(edge.clone(), index);
        usage_modes.insert(edge.clone(), UsageMode::Structural);
        between
            .entry(("rec".into(), Name::from(field.as_str())))
            .or_default()
            .push(edge.clone());
        outgoing.entry("rec".into()).or_default().push(edge.clone());
        incoming
            .entry(Name::from(field.as_str()))
            .or_default()
            .push(edge);
    }

    Schema {
        protocol: "test".into(),
        vertices,
        edges,
        hyper_edges: HashMap::new(),
        constraints: HashMap::new(),
        required: HashMap::new(),
        nsids: HashMap::new(),
        entries: vec!["rec".into()],
        variants: HashMap::new(),
        orderings,
        recursion_points: HashMap::new(),
        spans: HashMap::new(),
        usage_modes,
        nominal: HashMap::new(),
        coercions: HashMap::new(),
        mergers: HashMap::new(),
        defaults: HashMap::new(),
        policies: HashMap::new(),
        outgoing,
        incoming,
        between,
    }
}

/// The bytes of the schema's `MessagePack` encoding, as hex.
fn dump() -> String {
    use std::fmt::Write as _;
    let bytes = rmp_serde::to_vec(&wide_schema()).expect("the fixture serializes");
    bytes.iter().fold(String::new(), |mut hex, byte| {
        let _ = write!(hex, "{byte:02x}");
        hex
    })
}

/// The child half: one process's bytes, printed.
#[test]
fn one_process_answer() {
    if std::env::var_os("PP_SCHEMA_BYTES_DUMP").is_none() {
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
        .env("PP_SCHEMA_BYTES_DUMP", "1")
        .output()
        .expect("the test binary re-runs");
    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    let start = text.find("<<<").expect("the child printed its answer");
    let end = text.find(">>>").expect("the child closed its answer");
    text[start + 3..end].to_owned()
}

#[test]
fn the_bytes_do_not_depend_on_the_hash_seed() {
    if std::env::var_os("PP_SCHEMA_BYTES_DUMP").is_some() {
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
        "{PROCESSES} processes wrote {} different encodings of one schema",
        distinct.len()
    );
}

#[test]
fn the_schema_still_round_trips() {
    let schema = wide_schema();
    let bytes = rmp_serde::to_vec(&schema).expect("the fixture serializes");
    let back: Schema = rmp_serde::from_slice(&bytes).expect("the encoding parses");
    assert_eq!(back.edges.len(), schema.edges.len());
    assert_eq!(back.between.len(), schema.between.len());
    assert_eq!(back.orderings.len(), schema.orderings.len());
    assert_eq!(back.usage_modes.len(), schema.usage_modes.len());

    let json = serde_json::to_string(&schema).expect("the fixture serializes as JSON");
    let from_json: Schema = serde_json::from_str(&json).expect("the JSON parses");
    assert_eq!(from_json.edges.len(), schema.edges.len());
}
