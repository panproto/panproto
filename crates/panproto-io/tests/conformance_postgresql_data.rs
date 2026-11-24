//! The data reader against real data scripts: the `INSERT` examples of the
//! `PostgreSQL` 17 documentation, the tutorial script of the `PostgreSQL`
//! source, and a bounded subset of the Sakila sample data.
//!
//! A case passes when the reader reads it, its foreign keys resolve, the
//! shared validation reports nothing, and the canonical script it writes
//! reads back as the same schema and instance. A case that does not pass
//! is a declared limit: `postgresql-data-limits.txt` lists it with the
//! limit's name and the refusal class it shows, and the test fails if the
//! list and the failing cases differ in either direction.

#![cfg(feature = "tree-sitter")]
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use panproto_inst::validation::validate_table;
use panproto_io::ResolveOptions;
use panproto_io::postgresql::{SqlDataError, read_dump, resolve_foreign_keys, write_dump};
use panproto_protocols::database::postgresql::{OtherStatements, emit_ddl, protocol};

fn corpus() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/conformance")
}

/// Every case, by its path under the corpus directory.
fn cases() -> Vec<(String, String)> {
    let mut out = Vec::new();
    let docs = corpus().join("postgresql-docs/data-cases");
    for entry in std::fs::read_dir(&docs).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        out.push((
            format!("postgresql-docs/data-cases/{name}"),
            std::fs::read_to_string(&path).unwrap(),
        ));
    }
    let sakila = "sakila/sakila-data-subset.sql";
    out.push((
        sakila.to_owned(),
        std::fs::read_to_string(corpus().join(sakila)).unwrap(),
    ));
    out.sort();
    out
}

fn class(error: &SqlDataError) -> String {
    match error {
        SqlDataError::Script(e) => e.class().to_owned(),
        SqlDataError::Value { .. } => "Value".to_owned(),
        SqlDataError::Unwritable { .. } => "Unwritable".to_owned(),
        SqlDataError::NoDdlForm(_) => "NoDdlForm".to_owned(),
        _ => "Other".to_owned(),
    }
}

/// The outcome of one case: the rows read and the foreign keys resolved,
/// or the refusal class and its message.
fn outcome(text: &str) -> Result<(usize, usize), (String, String)> {
    let reading = read_dump(text, OtherStatements::List).map_err(|e| (class(&e), e.to_string()))?;
    let (graph, _) = resolve_foreign_keys(
        &reading.schema,
        &reading.instance,
        ResolveOptions::default(),
    )
    .unwrap_or_else(|e| panic!("foreign keys: {e}"));
    let report = validate_table(&reading.instance, &reading.schema, &protocol());
    assert_eq!(
        report
            .diagnostics
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        Vec::<String>::new()
    );
    let written = write_dump(&reading.schema, &reading.instance).unwrap();
    let again = read_dump(&written, OtherStatements::Refuse).unwrap();
    assert_eq!(
        emit_ddl(&again.schema).unwrap(),
        emit_ddl(&reading.schema).unwrap()
    );
    assert_eq!(again.instance, reading.instance);
    assert_eq!(write_dump(&again.schema, &again.instance).unwrap(), written);
    let edges = graph
        .edges_sorted()
        .filter(|(_, e)| e.edge.kind == "foreign-key")
        .count();
    Ok((reading.instance.total_row_count(), edges))
}

/// The declared limits: case to (limit, refusal class).
fn limits() -> BTreeMap<String, (String, String)> {
    let text = std::fs::read_to_string(corpus().join("postgresql-data-limits.txt")).unwrap();
    text.lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            assert_eq!(fields.len(), 3, "{line}");
            (
                fields[0].to_owned(),
                (fields[1].to_owned(), fields[2].to_owned()),
            )
        })
        .collect()
}

#[test]
fn every_data_case_reads_or_is_a_declared_limit() {
    let cases = cases();
    assert_eq!(
        cases.len(),
        8,
        "6 documentation examples, the tutorial script and the Sakila subset"
    );
    let limits = limits();
    let mut failing = BTreeMap::new();
    let mut read = BTreeMap::new();
    for (name, text) in &cases {
        match outcome(text) {
            Ok(counts) => {
                read.insert(name.clone(), counts);
            }
            Err(refusal) => {
                failing.insert(name.clone(), refusal);
            }
        }
    }
    let mut report = String::new();
    for (name, (class, message)) in &failing {
        let _ = writeln!(report, "{name} {class}: {message}");
    }
    let failing_names: Vec<&String> = failing.keys().collect();
    let limit_names: Vec<&String> = limits.keys().collect();
    assert_eq!(failing_names, limit_names, "failing cases:\n{report}");
    let mismatched: Vec<String> = limits
        .iter()
        .filter(|(name, (_, class))| &failing[*name].0 != class)
        .map(|(name, (limit, class))| {
            format!(
                "{name} ({limit}): declared {class}, shows {}: {}",
                failing[name].0, failing[name].1
            )
        })
        .collect();
    assert_eq!(mismatched, Vec::<String>::new(), "refusal classes");
    // 931 rows (6 languages, 109 countries, 600 cities, 200 actors, 16
    // categories), each city referencing its country.
    assert_eq!(read["sakila/sakila-data-subset.sql"], (931, 600));
    println!("{} of {} cases read: {read:?}", read.len(), cases.len());
}
