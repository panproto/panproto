//! The SQL DDL reader against the independent reference of the sql-ddl
//! page: column types on every bounded spelling, keys and references and
//! the canonical text on every bounded script, and keys and references on
//! generated larger scripts.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use panproto_protocols::database::sql::{OtherStatements, emit_ddl, parse_ddl, read_ddl};
use panproto_reference::enumerate::sql_ddl::{ddl_scripts, ddl_scripts_count, type_spellings};
use panproto_reference::pages::sql_ddl::{
    Column, ColumnConstraint, KeyReading, ReferenceReading, Script, ScriptRefusal, Table,
    TableForeignKey, TableKey, TableReading, TypeRefusal, canonical_text, column_type, read_script,
    render,
};
use panproto_schema::{KeyPart, KeyRole, RefSource, Schema, canonical_bytes};
use proptest::prelude::*;

const PARAMETER_SORTS: &[&str] = &[
    "int_bits",
    "precision",
    "scale",
    "max_length",
    "with_time_zone",
];

/// The sorted `(sort, value)` constraints of `vertex` whose sort is one of
/// `sorts`.
fn sorted_constraints(schema: &Schema, vertex: &str, sorts: &[&str]) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = schema
        .constraints
        .get(vertex)
        .map(|cs| {
            cs.iter()
                .filter(|c| sorts.contains(&c.sort.as_str()))
                .map(|c| (c.sort.to_string(), c.value.clone()))
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

fn owned(pairs: &[(&'static str, String)]) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = pairs
        .iter()
        .map(|(s, v)| ((*s).to_owned(), v.clone()))
        .collect();
    out.sort();
    out
}

#[test]
fn column_types_read_as_the_reference_on_every_bounded_spelling() {
    let spellings = type_spellings();
    assert_eq!(spellings.len(), 60 * 5);
    for spelling in &spellings {
        let ddl = format!("CREATE TABLE t (c {spelling});");
        let production = read_ddl(&ddl, OtherStatements::Refuse);
        match (column_type(spelling), production) {
            (Ok(expected), Ok(reading)) => {
                let schema = reading.schema;
                let mut element = "t.c".to_owned();
                for _ in 0..expected.dims {
                    assert_eq!(schema.vertices[element.as_str()].kind, "array", "{ddl}");
                    element.push_str(":items");
                }
                assert!(
                    !schema.has_vertex(&format!("{element}:items")),
                    "{ddl}: too many dimensions"
                );
                assert_eq!(
                    schema.vertices[element.as_str()].kind,
                    expected.kind,
                    "{ddl}"
                );
                assert_eq!(
                    sorted_constraints(&schema, &element, PARAMETER_SORTS),
                    owned(&expected.parameters),
                    "{ddl}"
                );
                assert_eq!(
                    sorted_constraints(&schema, "t.c", &["NOT NULL", "DEFAULT"]),
                    owned(&expected.column),
                    "{ddl}"
                );
            }
            (Err(refusal), Err(error)) => {
                let class = match refusal {
                    TypeRefusal::Unsupported => "Unsupported",
                    TypeRefusal::UnknownType => "UnknownType",
                    TypeRefusal::Syntax => "Syntax",
                };
                assert_eq!(error.class(), class, "{ddl}: {error}");
            }
            (expected, production) => {
                panic!("{ddl}: reference {expected:?}, production {production:?}")
            }
        }
    }
    println!("compared {} type spellings", spellings.len());
}

/// The tables of a production schema in the reference's terms.
fn tables_of(schema: &Schema) -> Vec<TableReading> {
    let column_names = |parts: &[KeyPart]| -> Vec<String> {
        parts
            .iter()
            .map(|p| match p {
                KeyPart::Path(path) => path[0].name.as_deref().unwrap().to_owned(),
                KeyPart::Position { .. } => panic!("a positional key part"),
            })
            .collect()
    };
    let mut tables: Vec<TableReading> = schema
        .vertices
        .values()
        .filter(|v| v.kind == "table")
        .map(|v| {
            let t = v.id.as_str();
            let mut columns: Vec<(u32, String, bool)> = schema
                .outgoing_edges(t)
                .iter()
                .filter(|e| e.kind == "prop")
                .map(|e| {
                    let not_null = schema
                        .constraints
                        .get(&e.tgt)
                        .is_some_and(|cs| cs.iter().any(|c| c.sort == "NOT NULL"));
                    (
                        schema.orderings[e],
                        e.name.as_deref().unwrap().to_owned(),
                        not_null,
                    )
                })
                .collect();
            columns.sort();
            let mut keys: Vec<KeyReading> = schema
                .declarations
                .keys
                .get(t)
                .map(|ks| {
                    ks.iter()
                        .map(|k| KeyReading {
                            name: k.id.to_string(),
                            primary: k.role == KeyRole::Primary,
                            columns: column_names(&k.parts),
                        })
                        .collect()
                })
                .unwrap_or_default();
            keys.sort();
            let mut references: Vec<ReferenceReading> = schema
                .declarations
                .references
                .values()
                .filter(|r| r.holder.as_str() == t)
                .map(|r| {
                    let RefSource::KeyValue { parts } = &r.source else {
                        panic!("a key-value reference")
                    };
                    ReferenceReading {
                        name: r.edge.as_ref().unwrap().name.as_deref().unwrap().to_owned(),
                        columns: parts
                            .iter()
                            .map(|p| p[0].name.as_deref().unwrap().to_owned())
                            .collect(),
                        target: r.target_vertex.to_string(),
                        target_key: r.target_key.as_deref().unwrap().to_owned(),
                        nullable: r.nullable,
                    }
                })
                .collect();
            references.sort();
            TableReading {
                name: t.to_owned(),
                columns: columns.into_iter().map(|(_, n, nn)| (n, nn)).collect(),
                keys,
                references,
            }
        })
        .collect();
    tables.sort();
    tables
}

const fn refusal_class(refusal: ScriptRefusal) -> &'static str {
    match refusal {
        ScriptRefusal::Duplicate => "Duplicate",
        ScriptRefusal::UnknownTable => "UnknownTable",
        ScriptRefusal::UnknownColumn => "UnknownColumn",
        ScriptRefusal::NoMatchingKey => "NoMatchingKey",
    }
}

/// Compare one script; with `canonical`, also the canonical text and its
/// reading. Whether the script reads.
fn compare(script: &Script, canonical: bool) -> bool {
    let text = render(script);
    match (read_script(script), parse_ddl(&text)) {
        (Ok(expected), Ok(schema)) => {
            assert_eq!(tables_of(&schema), expected, "{text}");
            if canonical {
                let emitted = emit_ddl(&schema).expect("emit");
                assert_eq!(emitted, canonical_text(&expected), "{text}");
                let again = parse_ddl(&emitted).expect("the canonical text reads");
                assert_eq!(canonical_bytes(&again), canonical_bytes(&schema), "{text}");
            }
            true
        }
        (Err((statement, refusal)), Err(error)) => {
            let panproto_protocols::ProtocolError::SqlDdl(error) = error else {
                panic!("{text}: {error}")
            };
            let at = text[..error.offset()].matches('\n').count();
            assert_eq!(
                (at, error.class()),
                (statement, refusal_class(refusal)),
                "{text}: {error}"
            );
            false
        }
        (expected, production) => {
            panic!("{text}: reference {expected:?}, production {production:?}")
        }
    }
}

#[test]
fn keys_and_references_match_the_reference_on_every_bounded_script() {
    let scripts = ddl_scripts();
    assert_eq!(scripts.len(), ddl_scripts_count());
    assert_eq!(scripts.len(), 22_620);
    let read = scripts.iter().filter(|s| compare(s, false)).count();
    println!("compared {} scripts, {read} read", scripts.len());
}

#[test]
fn canonical_text_matches_the_reference_on_every_bounded_script() {
    let scripts = ddl_scripts();
    assert_eq!(scripts.len(), 22_620);
    let read = scripts.iter().filter(|s| compare(s, true)).count();
    println!("compared {read} canonical texts");
}

fn column_strategy(tables: usize) -> impl Strategy<Value = Column> {
    (0..4usize, 0..=tables).prop_map(move |(c, r)| Column {
        constraint: [
            ColumnConstraint::None,
            ColumnConstraint::NotNull,
            ColumnConstraint::PrimaryKey,
            ColumnConstraint::Unique,
        ][c],
        references: (r < tables).then_some(r),
    })
}

fn table_strategy(tables: usize) -> impl Strategy<Value = Table> {
    (
        prop::collection::vec(column_strategy(tables), 1..=4),
        0..3usize,
        0..(1 + 2 * tables),
    )
        .prop_map(move |(columns, key, fk)| {
            let two = columns.len() >= 2;
            Table {
                key: if two {
                    [TableKey::None, TableKey::PrimaryKey, TableKey::Unique][key]
                } else {
                    TableKey::None
                },
                foreign_key: match fk {
                    _ if !two => TableForeignKey::None,
                    0 => TableForeignKey::None,
                    n if n <= tables => TableForeignKey::Implicit(n - 1),
                    n => TableForeignKey::Swapped(n - 1 - tables),
                },
                columns,
            }
        })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// Scripts of up to four tables of up to four columns, each column with
    /// one of four constraints and any reference, each table of two or
    /// more columns with any table key and table foreign key.
    #[test]
    fn randomized_keys_and_references_match_the_reference(
        script in (1..=4usize).prop_flat_map(|n| prop::collection::vec(table_strategy(n), n))
    ) {
        compare(&script, true);
    }
}
