//! The SQL DDL reader against real DDL: the `CREATE TABLE` and constraint
//! examples of the `PostgreSQL` 17 documentation, and the Sakila sample
//! schemas.
//!
//! A case passes when the reader reads it, with statements outside a table
//! schema listed, every reference resolves to a key of its target, and the
//! canonical text it writes reads back as the same schema. A case that
//! does not pass is a declared limit: `sql-ddl-limits.txt` lists it with
//! the limit's name and the refusal class it shows, and the test fails if
//! the list and the failing cases differ in either direction.

#![cfg(feature = "tree-sitter")]
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use panproto_io::postgresql::{parse_ddl, read_ddl};
use panproto_protocols::database::sql::{OtherStatements, emit_ddl};
use panproto_schema::canonical_bytes;

fn corpus() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/conformance")
}

/// Every case, by its path under the corpus directory.
fn cases() -> Vec<(String, String)> {
    let mut out = Vec::new();
    let docs = corpus().join("postgresql-docs/cases");
    for entry in std::fs::read_dir(&docs).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        out.push((
            format!("postgresql-docs/{name}"),
            std::fs::read_to_string(&path).unwrap(),
        ));
    }
    for name in ["sakila-postgres.sql", "sakila-sqlite.sql"] {
        let text = std::fs::read_to_string(corpus().join("sakila").join(name)).unwrap();
        out.push((format!("sakila/{name}"), text));
    }
    out.sort();
    out
}

/// The outcome of one case: `None` when it passes, else the refusal class
/// and its message.
fn outcome(text: &str) -> Option<(String, String)> {
    let reading = match read_ddl(text, OtherStatements::List) {
        Ok(reading) => reading,
        Err(error) => return Some((error.class().to_owned(), error.to_string())),
    };
    let schema = reading.schema;
    for reference in schema.declarations.references.values() {
        let key = reference.target_key.as_ref().expect("a target key");
        assert!(
            schema
                .declarations
                .key(&reference.target_vertex, key)
                .is_some(),
            "{} names no key",
            reference.id
        );
    }
    let canonical = emit_ddl(&schema).expect("emit");
    let again = parse_ddl(&canonical).unwrap_or_else(|e| panic!("{canonical}\n{e}"));
    assert_eq!(
        canonical_bytes(&schema),
        canonical_bytes(&again),
        "{canonical}"
    );
    None
}

/// The declared limits: case to (limit, refusal class).
fn limits() -> BTreeMap<String, (String, String)> {
    let text = std::fs::read_to_string(corpus().join("sql-ddl-limits.txt")).unwrap();
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
fn every_case_reads_or_is_a_declared_limit() {
    let cases = cases();
    assert_eq!(
        cases.len(),
        53,
        "51 documentation examples and 2 Sakila schemas"
    );
    let limits = limits();
    let mut failing = BTreeMap::new();
    let mut passed = 0;
    for (name, text) in &cases {
        match outcome(text) {
            None => passed += 1,
            Some((class, message)) => {
                failing.insert(name.clone(), (class, message));
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
    println!("{passed} of {} cases read", cases.len());
}
