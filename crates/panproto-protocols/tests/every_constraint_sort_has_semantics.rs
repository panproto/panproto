//! Every constraint sort a protocol declares, or its parser emits, has a
//! meaning.
//!
//! Validation reads a constraint through its protocol's
//! [`constraint_semantics`](panproto_schema::Protocol::constraint_semantics)
//! map, and a sort the map lacks makes every instance of that constraint
//! undecided. These tests keep the maps complete: every sort in a registered
//! protocol's `constraint_sorts` has an entry, and no parser emits, on any
//! of the repository's schema fixtures, a sort its protocol's map lacks. A
//! parser that starts emitting a new sort fails here until the sort is given
//! a meaning.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use panproto_gat::CheckStatus;
use panproto_inst::validation::{CheckKind, Violation, validate_table};
use panproto_inst::{FInstance, RowId, Value, ViewRef};
use panproto_protocols::registry::{descriptor, descriptors};
use panproto_protocols::semantics::constraint_semantics;
use panproto_protocols::{parse_schema_document, parse_schema_source};
use panproto_schema::{ConstraintSemantics, LengthUnit, Schema};

/// The fixture files of each protocol, relative to `fixtures/`, and
/// whether the protocol reads a JSON document (else source text).
const FIXTURES: &[(&str, bool, &[&str])] = &[
    (
        "atproto",
        true,
        &[
            "atproto/lexicons/app.bsky.actor.profile.json",
            "atproto/lexicons/app.bsky.feed.like.json",
            "atproto/lexicons/app.bsky.feed.post.json",
            "atproto/lexicons/app.bsky.feed.repost.json",
            "atproto/lexicons/app.bsky.graph.follow.json",
            "atproto/lexicons/com.atproto.repo.createRecord.json",
            "atproto/lexicons/site.standard.document.json",
        ],
    ),
    ("graphql", false, &["graphql/swapi.graphql"]),
    (
        "json-schema",
        true,
        &[
            "jsonschema/github-workflow.json",
            "jsonschema/package.json",
            "jsonschema/tsconfig.json",
        ],
    ),
    (
        "protobuf",
        false,
        &[
            "protobuf/common.proto",
            "protobuf/descriptor.proto",
            "protobuf/resource.proto",
            "protobuf/trace.proto",
        ],
    ),
    (
        "sql",
        false,
        &["sql/sakila-postgres.sql", "sql/sakila-sqlite.sql"],
    ),
];

fn fixture(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(relative)
}

fn parse(protocol: &str, document: bool, relative: &str) -> Schema {
    let text = std::fs::read_to_string(fixture(relative)).expect("the fixture exists");
    if document {
        let doc: serde_json::Value = serde_json::from_str(&text).expect("the fixture is JSON");
        parse_schema_document(protocol, &doc)
    } else {
        parse_schema_source(protocol, &text)
    }
    .unwrap_or_else(|e| panic!("{relative}: {e}"))
}

#[test]
fn every_registered_constraint_sort_has_semantics() {
    let mut missing: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for d in descriptors() {
        let built = (d.protocol)();
        let lacking: Vec<String> = built
            .constraint_sorts
            .iter()
            .filter(|sort| !built.constraint_semantics.contains_key(sort.as_str()))
            .cloned()
            .collect();
        if !lacking.is_empty() {
            missing.insert(d.name, lacking);
        }
    }
    assert!(
        missing.is_empty(),
        "constraint sorts with no semantics: {missing:#?}"
    );
}

/// Every constraint-semantics row names a registered protocol by its
/// canonical name and one of that protocol's constraint sorts, and no
/// protocol has two rows for one sort.
#[test]
fn every_constraint_semantics_row_names_a_declared_sort() {
    let mut seen = BTreeSet::new();
    for row in constraint_semantics::rows() {
        assert!(
            seen.insert((row.protocol, row.sort)),
            "{} {} has two rows",
            row.protocol,
            row.sort
        );
        if row.protocol.starts_with("code:") {
            continue;
        }
        let d = descriptor(row.protocol)
            .unwrap_or_else(|| panic!("{} is not registered", row.protocol));
        assert_eq!(d.name, row.protocol, "rows use the canonical name");
        assert!(
            (d.protocol)()
                .constraint_sorts
                .iter()
                .any(|s| s == row.sort),
            "{} declares no constraint sort {}",
            row.protocol,
            row.sort
        );
    }
}

#[test]
fn no_protocol_fixture_emits_a_sort_without_semantics() {
    let mut missing: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
    let mut emitted = 0usize;
    for (protocol, document, files) in FIXTURES {
        let built = (descriptor(protocol)
            .expect("a registered protocol")
            .protocol)();
        for file in *files {
            let schema = parse(protocol, *document, file);
            for constraint in schema.constraints.values().flatten() {
                emitted += 1;
                if !built
                    .constraint_semantics
                    .contains_key(constraint.sort.as_ref())
                {
                    missing
                        .entry(protocol)
                        .or_default()
                        .insert(constraint.sort.to_string());
                }
            }
        }
    }
    assert!(emitted > 0, "the fixtures emit constraints");
    assert!(
        missing.is_empty(),
        "sorts emitted on the fixtures with no semantics: {missing:#?}"
    );
}

#[test]
fn sql_not_null_is_required_value() {
    let sql = (descriptor("sql").expect("sql is registered").protocol)();
    assert_eq!(
        sql.constraint_semantics.get("NOT NULL"),
        Some(&ConstraintSemantics::RequiredValue)
    );
    // The parser writes `NOT NULL` as a constraint the validator reads as a
    // required value.
    let schema = parse_schema_source("sql", "CREATE TABLE t (a INTEGER NOT NULL, b TEXT);")
        .expect("the table parses");
    let not_null: Vec<(&str, &str)> = schema
        .constraints
        .iter()
        .flat_map(|(v, cs)| cs.iter().map(move |c| (v.as_ref(), c)))
        .filter(|(_, c)| c.sort == "NOT NULL")
        .map(|(v, c)| (v, c.value.as_str()))
        .collect();
    assert_eq!(not_null.len(), 1, "{not_null:?}");
    assert!(
        sql.constraint_semantics["NOT NULL"]
            .parse_value("NOT NULL", not_null[0].1)
            .is_ok(),
        "the parser's value reads as a required-value flag"
    );

    // Validated, a row whose NOT NULL cell is absent or null fails with a
    // missing value for that column, and a row that fills it passes.
    let column = schema
        .edges
        .keys()
        .find(|e| e.kind == "prop" && e.tgt == not_null[0].0)
        .expect("the column's edge");
    let table = column.src.clone();
    let cell = column.name.as_ref().expect("a named column").to_string();
    let rows = |a: Option<Value>| {
        let mut row = HashMap::from([("b".to_owned(), Value::Str("x".into()))]);
        if let Some(a) = a {
            row.insert(cell.clone(), a);
        }
        row
    };
    let filled = FInstance::new()
        .with_rows(table.clone(), [(RowId(0), rows(Some(Value::Int(1))))])
        .expect("the row is new");
    let report = validate_table(&filled, &schema, &sql);
    assert_eq!(report.status, CheckStatus::Passed, "{report:#?}");
    for missing in [None, Some(Value::Null)] {
        let unfilled = FInstance::new()
            .with_rows(table.clone(), [(RowId(0), rows(missing))])
            .expect("the row is new");
        let report = validate_table(&unfilled, &schema, &sql);
        assert_eq!(report.status, CheckStatus::Failed, "{report:#?}");
        assert_eq!(report.diagnostics.len(), 1, "{report:#?}");
        let d = &report.diagnostics[0];
        assert_eq!(d.check.kind, CheckKind::RequiredValue);
        assert_eq!(d.subject, Some(ViewRef::Row(RowId(0))));
        assert_eq!(
            d.violation,
            Violation::MissingValue {
                field: Some(cell.clone())
            }
        );
    }
}

#[test]
fn length_units_follow_each_protocol() {
    let semantics = |protocol: &str, sort: &str| {
        (descriptor(protocol).expect("registered").protocol)()
            .constraint_semantics
            .get(sort)
            .cloned()
    };
    assert_eq!(
        semantics("atproto", "maxLength"),
        Some(ConstraintSemantics::MaxLength(LengthUnit::Utf8Bytes))
    );
    assert_eq!(
        semantics("atproto", "maxGraphemes"),
        Some(ConstraintSemantics::MaxLength(LengthUnit::Graphemes))
    );
    assert_eq!(
        semantics("json-schema", "maxLength"),
        Some(ConstraintSemantics::MaxLength(LengthUnit::Chars))
    );
}

/// Every constraint the fixtures declare reads under its protocol's
/// meaning: compiling a validation plan for each fixture finds no
/// ill-formed declaration. The only static findings are value formats
/// outside the fixed set validation evaluates (such as atproto's `at-uri`),
/// which are unsupported declarations by design.
#[test]
fn fixture_constraints_read_under_their_semantics() {
    use panproto_inst::validation::{IncompleteReason, ValidationOptions, ValidationPlan};
    let mut unexpected = Vec::new();
    let mut unsupported_formats = 0usize;
    for (protocol, document, files) in FIXTURES {
        let built = (descriptor(protocol)
            .expect("a registered protocol")
            .protocol)();
        for file in *files {
            let schema = parse(protocol, *document, file);
            let plan = ValidationPlan::compile(&schema, &built, &ValidationOptions::default());
            for finding in plan.static_findings() {
                match &finding.reason {
                    IncompleteReason::UnsupportedDeclaration { sort } if sort == "format" => {
                        unsupported_formats += 1;
                    }
                    other => unexpected.push(format!("{file}: {:?} {other:?}", finding.check)),
                }
            }
        }
    }
    assert!(
        unexpected.is_empty(),
        "fixture declarations that do not read: {unexpected:#?}"
    );
    assert!(
        unsupported_formats > 0,
        "the fixtures name formats outside the set"
    );
}

/// An enum type's labels are SQL string literals in their quotes, which an
/// enumeration constraint would compare with the bare cell value and
/// refute; the labels compile to a check decided outside panproto instead,
/// with no static finding.
#[test]
fn sql_enum_labels_are_decided_outside_panproto() {
    use panproto_inst::validation::{SchemaRef, ValidationOptions, ValidationPlan};
    let sql = (descriptor("sql").expect("sql is registered").protocol)();
    let schema = parse_schema_source(
        "sql",
        "CREATE TYPE rating AS ENUM ('G', 'PG');\nCREATE TABLE film (r rating);",
    )
    .expect("the script parses");
    let column = schema
        .edges
        .keys()
        .find(|e| e.kind == "prop")
        .expect("the column's edge")
        .clone();
    let labels = &schema.constraints[&column.tgt];
    assert_eq!(labels.len(), 1, "{labels:?}");
    assert_eq!(labels[0].sort, "ENUM");
    assert_eq!(labels[0].value, "'G','PG'");

    let plan = ValidationPlan::compile(&schema, &sql, &ValidationOptions::default());
    let enum_checks: Vec<CheckKind> = plan
        .checks()
        .keys()
        .filter(|k| matches!(&k.decl, SchemaRef::Constraint { constraint, .. } if constraint.sort == "ENUM"))
        .map(|k| k.kind)
        .collect();
    assert_eq!(enum_checks, vec![CheckKind::External]);
    assert!(
        plan.static_findings().is_empty(),
        "{:#?}",
        plan.static_findings()
    );

    // Read as an enumeration, the quoted labels refute the valid cell.
    let film = FInstance::new()
        .with_rows(
            column.src.clone(),
            [(
                RowId(0),
                HashMap::from([(
                    column.name.as_ref().expect("named").to_string(),
                    Value::Str("PG".into()),
                )]),
            )],
        )
        .expect("the row is new");
    let mut as_enum = sql;
    as_enum
        .constraint_semantics
        .insert("ENUM".to_owned(), ConstraintSemantics::Enum);
    let report = validate_table(&film, &schema, &as_enum);
    assert_eq!(report.status, CheckStatus::Failed, "{report:#?}");
    assert_eq!(report.diagnostics[0].check.kind, CheckKind::ValueConstraint);
}
