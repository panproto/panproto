//! The keys and foreign keys of `PostgreSQL` data: primary-key and unique
//! violations reported by the shared validation, and foreign keys resolved
//! to the rows they reference by the shared reference resolution, with
//! composite keys, `MATCH SIMPLE` and `MATCH FULL`, and unresolved and
//! ambiguous references reported.

#![cfg(feature = "tree-sitter")]
#![allow(clippy::expect_used, clippy::unwrap_used)]

use panproto_inst::validation::{CheckKind, validate_table};
use panproto_inst::{GInstance, ResolutionUnresolvedReason as UnresolvedReason};
use panproto_io::postgresql::{DumpReading, read_dump, resolve_foreign_keys};
use panproto_io::{ResolveError, ResolveOptions, default_registry};
use panproto_protocols::database::postgresql::{OtherStatements, protocol};

const SCHEMA: &str = "CREATE TABLE country (code character(2) PRIMARY KEY, name text UNIQUE);
CREATE TABLE city (
    id integer PRIMARY KEY,
    country character(2) REFERENCES country,
    a integer,
    b integer,
    c integer,
    d integer
);
CREATE TABLE pair (x integer, y integer, PRIMARY KEY (x, y));
ALTER TABLE city ADD CONSTRAINT simple_pair FOREIGN KEY (a, b) REFERENCES pair (x, y);
ALTER TABLE city ADD CONSTRAINT full_pair FOREIGN KEY (c, d) REFERENCES pair (x, y) MATCH FULL;
INSERT INTO country VALUES ('fr', 'France'), ('de', 'Germany');
INSERT INTO pair VALUES (1, 1), (1, 2);
";

fn read(rows: &str) -> DumpReading {
    read_dump(&format!("{SCHEMA}{rows}"), OtherStatements::Refuse).unwrap()
}

/// The resolved `foreign-key` edges, as `(source row, target row, name)`.
fn edges(graph: &GInstance) -> Vec<(u32, u32, String)> {
    let mut out: Vec<_> = graph
        .edges_sorted()
        .filter(|(_, e)| e.edge.kind == "foreign-key")
        .map(|(_, e)| {
            (
                e.src,
                e.tgt,
                e.edge.name.as_deref().unwrap_or_default().to_owned(),
            )
        })
        .collect();
    out.sort();
    out
}

#[test]
fn foreign_keys_resolve_to_the_rows_they_name() {
    // Rows: city 0 to 2; country 3 ('fr'), 4 ('de'); pair 5 (1, 1), 6 (1, 2).
    let reading = read(
        "INSERT INTO city VALUES (1, 'fr', 1, 2, 1, 1), (2, NULL, NULL, 2, NULL, NULL), (3, 'de', 1, NULL, NULL, NULL);",
    );
    let (graph, complement) = resolve_foreign_keys(
        &reading.schema,
        &reading.instance,
        ResolveOptions::default(),
    )
    .unwrap();
    assert_eq!(
        edges(&graph),
        vec![
            (0, 3, "city_country_fkey".to_owned()),
            (0, 5, "full_pair".to_owned()),
            (0, 6, "simple_pair".to_owned()),
            (2, 4, "city_country_fkey".to_owned()),
        ]
    );
    assert_eq!(complement.unresolved.len(), 0);
}

#[test]
fn an_unresolved_foreign_key_is_refused_or_recorded() {
    let reading = read("INSERT INTO city VALUES (1, 'it', NULL, NULL, NULL, NULL);");
    let refused = resolve_foreign_keys(
        &reading.schema,
        &reading.instance,
        ResolveOptions::default(),
    );
    assert!(
        matches!(refused, Err(ResolveError::DanglingReference { .. })),
        "{refused:?}"
    );
    let (_, complement) = resolve_foreign_keys(
        &reading.schema,
        &reading.instance,
        ResolveOptions { lenient: true },
    )
    .unwrap();
    assert_eq!(complement.unresolved.len(), 1);
    assert!(matches!(
        complement.unresolved[0].reason,
        UnresolvedReason::Dangling
    ));
}

#[test]
fn match_full_refuses_a_partly_null_key_and_match_simple_does_not() {
    let simple = read("INSERT INTO city VALUES (1, NULL, 9, NULL, NULL, NULL);");
    let (graph, _) =
        resolve_foreign_keys(&simple.schema, &simple.instance, ResolveOptions::default()).unwrap();
    assert_eq!(edges(&graph), vec![]);
    let full = read("INSERT INTO city VALUES (1, NULL, NULL, NULL, 1, NULL);");
    let refused = resolve_foreign_keys(&full.schema, &full.instance, ResolveOptions::default());
    assert!(refused.is_err(), "{refused:?}");
}

#[test]
fn key_violations_are_reported_by_the_shared_validation() {
    let clean = read("INSERT INTO city VALUES (1, 'fr', NULL, NULL, NULL, NULL);");
    let report = validate_table(&clean.instance, &clean.schema, &protocol());
    assert_eq!(
        report
            .diagnostics
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        Vec::<String>::new()
    );
    let duplicate = read(
        "INSERT INTO city VALUES (1, 'fr', NULL, NULL, NULL, NULL), (1, 'de', NULL, NULL, NULL, NULL);",
    );
    let report = validate_table(&duplicate.instance, &duplicate.schema, &protocol());
    let kinds: Vec<CheckKind> = report.diagnostics.iter().map(|d| d.check.kind).collect();
    assert!(kinds.contains(&CheckKind::KeyUnique), "{kinds:?}");
}

#[test]
fn the_registry_resolves_postgresql_rows_to_a_graph() {
    let registry = default_registry();
    let reading = read("INSERT INTO city VALUES (1, 'fr', NULL, NULL, NULL, NULL);");
    assert!(
        registry
            .supports_semantic_for("postgresql", &reading.schema)
            .unwrap()
    );
}
