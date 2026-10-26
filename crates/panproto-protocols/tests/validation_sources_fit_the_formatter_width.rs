//! The sources of constraint semantics, the graph instance model, failure
//! witnesses and the validation corpus keep every line within the
//! workspace's `max_width`.
//!
//! `rustfmt` leaves comments, doctests and the bodies of macros such as
//! `proptest!` as written, so `cargo fmt --check` passes over a line of any
//! length there; this test reads the lines themselves.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};

/// The workspace root, two levels above this crate's manifest.
fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the workspace root exists")
}

/// The `max_width` that `rustfmt.toml` at the workspace root sets.
fn max_width(root: &Path) -> usize {
    let config = std::fs::read_to_string(root.join("rustfmt.toml")).expect("rustfmt.toml");
    config
        .lines()
        .find_map(|line| {
            let (key, value) = line.split_once('=')?;
            (key.trim() == "max_width").then(|| value.trim().parse().expect("an integer width"))
        })
        .expect("rustfmt.toml sets max_width")
}

/// Every `.rs` file under `dir`, recursively.
fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display())) {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|x| x == "rs") {
            out.push(path);
        }
    }
}

/// The checked sources, relative to the workspace root: whole directories
/// end in `/`, and every `.rs` file under one is checked.
const SOURCES: &[&str] = &[
    "crates/panproto-protocols/src/semantics/constraint_semantics/",
    "crates/panproto-protocols/src/graph_instance_model.rs",
    "crates/panproto-protocols/src/bin/validation-fixtures/",
    "crates/panproto-protocols/tests/endpoint_coherence_is_the_graph_instance_theory.rs",
    "crates/panproto-protocols/tests/every_constraint_sort_has_semantics.rs",
    "crates/panproto-protocols/tests/validation_fixtures.rs",
    "crates/panproto-protocols/tests/validation_sources_fit_the_formatter_width.rs",
    "crates/panproto-inst/src/validation/witness.rs",
    "crates/panproto-inst/tests/validation_laws.rs",
    "crates/panproto-inst/tests/validation_support/",
];

#[test]
fn validation_sources_fit_the_formatter_width() {
    let root = workspace();
    let width = max_width(&root);
    let mut files = Vec::new();
    for source in SOURCES {
        let path = root.join(source);
        if source.ends_with('/') {
            rust_files(&path, &mut files);
        } else {
            assert!(path.is_file(), "{source} is not a file");
            files.push(path);
        }
    }
    files.sort();
    assert!(files.len() >= SOURCES.len(), "too few files: {files:?}");
    let mut over = Vec::new();
    for file in &files {
        let text = std::fs::read_to_string(file).unwrap();
        for (n, line) in text.lines().enumerate() {
            let columns = line.chars().count();
            if columns > width {
                let relative = file.strip_prefix(&root).unwrap().display();
                over.push(format!("{relative}:{}: {columns} columns", n + 1));
            }
        }
    }
    assert!(
        over.is_empty(),
        "lines wider than {width} columns:\n{}",
        over.join("\n")
    );
}
