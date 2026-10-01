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

/// The directories of `fixtures/` that hold schemas, each with the
/// protocol its files are read under and whether the protocol reads a JSON
/// document (else source text). Every file under one of them is a schema
/// fixture.
const SCHEMA_DIRS: &[(&str, &str, bool)] = &[
    ("atproto/lexicons", "atproto", true),
    ("graphql", "graphql", false),
    ("jsonschema", "json-schema", true),
    ("protobuf", "protobuf", false),
    ("sql", "sql", false),
];

/// The files and directories of `fixtures/` that hold no schema: instance
/// records, and the fixtures' documentation.
const NOT_SCHEMAS: &[&str] = &["FIXTURES.md", "atproto/records"];

/// One schema fixture: its protocol, whether the protocol reads a JSON
/// document, and its path relative to the fixture root.
type SchemaFixture = (&'static str, bool, String);

/// Every schema fixture under `root`, by walking it: each file under a
/// directory of [`SCHEMA_DIRS`], in path order. A file that is neither
/// under a schema directory nor one of [`NOT_SCHEMAS`] is an error, so a
/// fixture added anywhere is either checked or classified here.
fn schema_fixtures(root: &Path) -> Result<Vec<SchemaFixture>, Vec<String>> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display())) {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                let relative = path.strip_prefix(root).unwrap();
                let parts: Vec<String> = relative
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect();
                out.push(parts.join("/"));
            }
        }
    }
    let under = |file: &str, dir: &str| file == dir || file.starts_with(&format!("{dir}/"));
    let mut files = Vec::new();
    walk(root, root, &mut files);
    files.sort();
    let mut schemas = Vec::new();
    let mut unclassified = Vec::new();
    for file in files {
        if let Some((_, protocol, document)) = SCHEMA_DIRS.iter().find(|(d, _, _)| under(&file, d))
        {
            schemas.push((*protocol, *document, file));
        } else if !NOT_SCHEMAS.iter().any(|d| under(&file, d)) {
            unclassified.push(file);
        }
    }
    if unclassified.is_empty() {
        Ok(schemas)
    } else {
        Err(unclassified)
    }
}

/// The repository's schema fixtures.
fn fixtures() -> Vec<SchemaFixture> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures");
    schema_fixtures(&root)
        .unwrap_or_else(|files| panic!("fixtures that are neither schemas nor listed: {files:?}"))
}

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
    for (protocol, document, file) in fixtures() {
        let built = (descriptor(protocol)
            .expect("a registered protocol")
            .protocol)();
        let schema = parse(protocol, document, &file);
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
    for (protocol, document, file) in fixtures() {
        let built = (descriptor(protocol)
            .expect("a registered protocol")
            .protocol)();
        let schema = parse(protocol, document, &file);
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
    assert!(
        unexpected.is_empty(),
        "fixture declarations that do not read: {unexpected:#?}"
    );
    assert!(
        unsupported_formats > 0,
        "the fixtures name formats outside the set"
    );
}

/// The `sql` keys, references and defaults are annotations, from which no
/// check is compiled: keys and references are checked from the schema's
/// key and reference declarations, and a default constrains no cell.
#[test]
fn sql_keys_references_and_defaults_compile_to_no_check() {
    let sql = (descriptor("sql").expect("sql is registered").protocol)();
    for sort in ["PRIMARY KEY", "UNIQUE", "FOREIGN KEY", "DEFAULT"] {
        let meaning = &sql.constraint_semantics[sort];
        assert_eq!(meaning, &ConstraintSemantics::Annotation, "{sort}");
        assert!(!meaning.compiles_to_check(), "{sort}");
    }
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
        .filter(|k| {
            matches!(
                &k.decl,
                SchemaRef::Constraint { constraint, .. } if constraint.sort == "ENUM"
            )
        })
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

/// Two rows of the table holding the column constrained by `sort` in
/// `schema`, with the cells `cells`, under `protocol`: with no verdict each
/// row is undecided for want of a witness, so the status is not `Passed`,
/// and a verdict rejecting row 1 fails that row alone.
fn check_external_column_rows(protocol: &str, sort: &str, schema: &Schema, cells: [Value; 2]) {
    use panproto_gat::Name;
    use panproto_inst::Digest;
    use panproto_inst::validation::{
        ExternalVerdict, ExternalVerdicts, IncompleteReason, ValidationBudget, ValidationOptions,
        ValidationPlan,
    };
    let built = (descriptor(protocol).expect("registered").protocol)();
    assert_eq!(
        built.constraint_semantics.get(sort),
        Some(&ConstraintSemantics::External),
        "{protocol} {sort}"
    );
    let column = schema
        .edges
        .keys()
        .find(|e| {
            e.kind == "prop"
                && schema
                    .constraints
                    .get(&e.tgt)
                    .is_some_and(|cs| cs.iter().any(|c| c.sort == sort))
        })
        .expect("the constrained column's edge")
        .clone();
    let cell = column.name.as_ref().expect("a named column").to_string();
    let rows = cells.into_iter().enumerate().map(|(i, v)| {
        (
            RowId(u32::try_from(i).unwrap()),
            HashMap::from([(cell.clone(), v)]),
        )
    });
    let table = FInstance::new()
        .with_rows(column.src, rows)
        .expect("the rows are new");
    let plan = ValidationPlan::compile(schema, &built, &ValidationOptions::default());
    let budget = ValidationBudget::default();
    let report = plan.validate_table(&table, &budget, &ExternalVerdicts::default());
    let undecided: Vec<(Option<ViewRef>, IncompleteReason)> = report
        .incomplete
        .iter()
        .filter(|i| i.check.kind == CheckKind::External)
        .map(|i| (i.subject.clone(), i.reason.clone()))
        .collect();
    let missing = IncompleteReason::MissingExternalWitness {
        name: Name::from(sort),
    };
    assert_eq!(
        undecided,
        vec![
            (Some(ViewRef::Row(RowId(0))), missing.clone()),
            (Some(ViewRef::Row(RowId(1))), missing),
        ],
        "{protocol} {sort}: {report:#?}"
    );
    assert_ne!(report.status, CheckStatus::Passed, "{protocol} {sort}");
    let mut verdicts = ExternalVerdicts::default();
    for (row, passed) in [(0, true), (1, false)] {
        verdicts.insert(
            Name::from(sort),
            ViewRef::Row(RowId(row)),
            ExternalVerdict {
                passed,
                witness: Digest([7; 32]),
            },
        );
    }
    let report = plan.validate_table(&table, &budget, &verdicts);
    assert_eq!(report.status, CheckStatus::Failed, "{protocol} {sort}");
    let rejected: Vec<(CheckKind, Option<ViewRef>, Violation)> = report
        .diagnostics
        .iter()
        .map(|d| (d.check.kind, d.subject.clone(), d.violation.clone()))
        .collect();
    assert_eq!(
        rejected,
        vec![(
            CheckKind::External,
            Some(ViewRef::Row(RowId(1))),
            Violation::ExternalRejected {
                witness: Digest([7; 32])
            }
        )],
        "{protocol} {sort}"
    );
}

/// The registered `External` column rows, sql `ENUM` and dataframe `gt`,
/// compile to a check with one instance per row holding the column's cell
/// (see [`check_external_column_rows`]).
#[test]
fn external_column_rows_have_an_instance_per_row() {
    use panproto_protocols::data_science::dataframe::parse_dataframe_schema;
    let sql = parse_schema_source(
        "sql",
        "CREATE TYPE rating AS ENUM ('G', 'PG');\nCREATE TABLE film (r rating);",
    )
    .expect("the script parses");
    check_external_column_rows(
        "sql",
        "ENUM",
        &sql,
        [Value::Str("PG".into()), Value::Str("R".into())],
    );
    let dataframe = parse_dataframe_schema(&serde_json::json!({
        "name": "df",
        "columns": {"x": {"dtype": "int64", "checks": {"gt": 0}}},
    }))
    .expect("the dataframe schema parses");
    check_external_column_rows(
        "dataframe",
        "gt",
        &dataframe,
        [Value::Int(1), Value::Int(-1)],
    );
}

/// The schema fixtures are found by walking `fixtures/`: the repository's
/// tree yields its seventeen schemas, and a file added under a schema
/// directory is read under that directory's protocol, while a file in a
/// directory the walk does not classify is an error rather than skipped.
#[test]
fn schema_fixtures_are_every_file_under_a_schema_directory() {
    let found = fixtures();
    assert_eq!(found.len(), 17, "{found:#?}");
    for (dir, protocol, _) in SCHEMA_DIRS {
        assert!(
            found
                .iter()
                .any(|(p, _, f)| p == protocol && f.starts_with(dir)),
            "no fixture under {dir}"
        );
    }

    let root =
        std::env::temp_dir().join(format!("panproto-schema-fixtures-{}", std::process::id()));
    let write = |relative: &str| {
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "").unwrap();
    };
    write("FIXTURES.md");
    write("atproto/records/post.json");
    write("sql/added.sql");
    write("jsonschema/nested/added.json");
    let classified = schema_fixtures(&root);
    write("avro/added.avsc");
    let unclassified = schema_fixtures(&root);
    std::fs::remove_dir_all(&root).unwrap();
    assert_eq!(
        classified,
        Ok(vec![
            (
                "json-schema",
                true,
                "jsonschema/nested/added.json".to_owned()
            ),
            ("sql", false, "sql/added.sql".to_owned()),
        ])
    );
    assert_eq!(unclassified, Err(vec!["avro/added.avsc".to_owned()]));
}

/// The file names of the Rust sources directly under `dir`, relative to the
/// crate root.
fn rust_files(dir: &str) -> BTreeSet<String> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(dir);
    std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|e| e == "rs"))
        .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
        .collect()
}

/// The constraint-semantics rows are kept in exactly the family files the
/// shape rows are, one per protocol family, with no file shared between
/// families: each family file holds its rows and everything they are built
/// from.
#[test]
fn constraint_semantics_rows_are_kept_one_file_per_family() {
    assert_eq!(
        rust_files("src/semantics/constraint_semantics"),
        rust_files("src/semantics/shapes"),
    );
}
