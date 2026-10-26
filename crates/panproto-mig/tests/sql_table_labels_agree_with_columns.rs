#![allow(clippy::unwrap_used, clippy::expect_used)]
//! A SQL table's hyper-edge and its column edges never disagree: each label
//! of the table's hyper-edge names the column vertex that a `prop` edge of
//! that name joins the table to, and each `prop` edge of the table is a
//! label. Lowering a label rename onto the column's attribute arrow thus
//! renames exactly the cell the label names.

use std::collections::BTreeSet;
use std::path::Path;

use panproto_gat::Name;
use panproto_protocols::database::sql::parse_ddl;
use panproto_schema::Schema;

/// Pairs `(label, column vertex)`.
type Columns = BTreeSet<(Name, Name)>;

/// The pairs of every table hyper-edge, and of every `prop` edge leaving
/// that table, by table.
fn agreement(schema: &Schema) -> Vec<(Name, Columns, Columns)> {
    let mut tables: Vec<&panproto_schema::HyperEdge> = schema
        .hyper_edges
        .values()
        .filter(|h| h.kind == "table")
        .collect();
    tables.sort_by(|a, b| a.id.cmp(&b.id));
    tables
        .into_iter()
        .map(|h| {
            let table = h.parent_label.clone();
            let labels: Columns = h
                .signature
                .iter()
                .map(|(label, col)| (label.clone(), col.clone()))
                .collect();
            let props: Columns = schema
                .edges
                .keys()
                .filter(|e| e.src == table && e.kind == "prop")
                .map(|e| (e.name.clone().unwrap_or_default(), e.tgt.clone()))
                .collect();
            (table, labels, props)
        })
        .collect()
}

fn check(ddl: &str, what: &str) -> usize {
    let schema = parse_ddl(ddl).unwrap_or_else(|e| panic!("{what}: {e}"));
    let tables = agreement(&schema);
    for (table, labels, props) in &tables {
        assert_eq!(labels, props, "{what}: table {table}");
    }
    tables.len()
}

#[test]
fn every_sql_fixture_table_labels_its_columns_by_their_edges() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut total = 0;
    for file in [
        "fixtures/sql/sakila-postgres.sql",
        "fixtures/sql/sakila-sqlite.sql",
        "tests/fixtures/sql/users.sql",
        "tests/fixtures/sql/users_posts.sql",
    ] {
        let ddl = std::fs::read_to_string(root.join(file)).unwrap();
        total += check(&ddl, file);
    }
    assert!(total >= 16, "{total} tables checked");
}

#[test]
fn inherited_quoted_and_constrained_columns_agree_too() {
    let ddl = r#"
        CREATE TABLE base (id INTEGER PRIMARY KEY, "Created At" TEXT NOT NULL DEFAULT 'now');
        CREATE TABLE child (extra TEXT, PRIMARY KEY (extra)) INHERITS (base);
        CREATE TABLE refs (base_id INTEGER REFERENCES base(id), note VARCHAR(10));
    "#;
    assert_eq!(check(ddl, "inline"), 3);
}
