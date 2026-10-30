//! Writing one table instance twice must produce the same bytes twice.
//!
//! A table instance keeps its tables, rows, cells and foreign keys in
//! `HashMap`s, and a `HashMap` enumerates its entries in an order the
//! process's hash seed decides. Table instances are content-addressed like
//! the other shapes, so an encoding that followed that order would store one
//! instance under a different object id in every process. Each test here
//! writes its fixture in many separate processes and compares the bytes. Run
//! a child directly with `PP_TABLE_BYTES_DUMP` set to the fixture's name to
//! see one process's answer.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::process::Command;

use panproto_inst::{FInstance, RowId, Value, canonical_finstance_bytes};
use panproto_schema::Edge;

/// How many separate processes each answer is compared across.
const PROCESSES: usize = 32;

/// How many rows each table of the fixture holds.
const WIDTH: u32 = 24;

fn fk(src: &str, tgt: &str, name: &str) -> Edge {
    Edge {
        src: src.into(),
        tgt: tgt.into(),
        kind: "fk".into(),
        name: Some(name.into()),
    }
}

/// Three tables of wide rows, inserted in a scrambled order, with dense
/// foreign keys between them.
fn wide_tables() -> FInstance {
    let mut tables = FInstance::new();
    for k in 0..WIDTH {
        let id = (k * 7) % WIDTH;
        for (offset, table) in [(0, "user"), (WIDTH, "post"), (2 * WIDTH, "tag")] {
            let row: HashMap<String, Value> = (0..6)
                .map(|c| (format!("c{c}_{id}"), Value::Int(i64::from(id * 10 + c))))
                .collect();
            tables.insert_row(table, RowId(offset + id), row).unwrap();
        }
    }
    for s in 0..WIDTH {
        for t in (0..WIDTH).step_by(3) {
            tables
                .insert_foreign_key(fk("post", "user", "author"), RowId(WIDTH + s), RowId(t))
                .unwrap();
            tables
                .insert_foreign_key(
                    fk("tag", "post", "on"),
                    RowId(2 * WIDTH + t),
                    RowId(WIDTH + s),
                )
                .unwrap();
        }
    }
    tables
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut out, byte| {
        let _ = write!(out, "{byte:02x}");
        out
    })
}

/// The named encoding of the fixture.
fn dump(fixture: &str) -> String {
    let tables = wide_tables();
    match fixture {
        "msgpack" => hex(&rmp_serde::to_vec(&tables).unwrap()),
        "msgpack_named" => hex(&rmp_serde::to_vec_named(&tables).unwrap()),
        "json" => serde_json::to_string(&tables).unwrap(),
        "digest" => tables.digest().to_hex(),
        "canonical" => hex(&canonical_finstance_bytes(&tables)),
        other => panic!("no fixture named {other}"),
    }
}

/// The child half: one process's bytes, printed.
#[test]
fn one_process_answer() {
    let Some(fixture) = std::env::var_os("PP_TABLE_BYTES_DUMP") else {
        return;
    };
    print!("<<<{}>>>", dump(&fixture.to_string_lossy()));
}

fn child_answer(exe: &std::path::Path, fixture: &str) -> String {
    let output = Command::new(exe)
        .args(["one_process_answer", "--exact", "--nocapture"])
        .env("PP_TABLE_BYTES_DUMP", fixture)
        .output()
        .expect("the test binary re-runs");
    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    let start = text.find("<<<").expect("the child printed its answer");
    let end = text.rfind(">>>").expect("the child closed its answer");
    text[start + 3..end].to_owned()
}

fn assert_one_encoding(fixture: &str) {
    if std::env::var_os("PP_TABLE_BYTES_DUMP").is_some() {
        return;
    }
    let exe = std::env::current_exe().expect("the test binary knows its own path");
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    for _ in 0..PROCESSES {
        *seen.entry(child_answer(&exe, fixture)).or_insert(0) += 1;
    }
    assert_eq!(
        seen.len(),
        1,
        "{PROCESSES} processes wrote {} different {fixture} encodings of one table instance",
        seen.len()
    );
}

#[test]
fn a_table_instance_encodes_to_positional_msgpack_the_same_whatever_the_hash_seed() {
    assert_one_encoding("msgpack");
}

#[test]
fn a_table_instance_encodes_to_named_msgpack_the_same_whatever_the_hash_seed() {
    assert_one_encoding("msgpack_named");
}

#[test]
fn a_table_instance_encodes_to_json_the_same_whatever_the_hash_seed() {
    assert_one_encoding("json");
}

#[test]
fn a_table_digest_is_the_same_whatever_the_hash_seed() {
    assert_one_encoding("digest");
}

#[test]
fn canonical_table_bytes_are_the_same_whatever_the_hash_seed() {
    assert_one_encoding("canonical");
}

#[test]
fn the_fixture_still_round_trips() {
    let tables = wide_tables();
    let bytes = rmp_serde::to_vec(&tables).unwrap();
    let back: FInstance = rmp_serde::from_slice(&bytes).unwrap();
    assert_eq!(back, tables);
    assert_eq!(back.total_row_count(), 3 * usize::try_from(WIDTH).unwrap());
}
