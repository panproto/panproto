//! Table directories: a manifest, a schema file of `PostgreSQL` DDL and one
//! CSV or `COPY` text file per table, read into the schema and its rows and
//! written back deterministically.

#![cfg(feature = "tree-sitter")]
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::path::Path;

use panproto_inst::Value;
use panproto_io::postgresql::{
    Dialect, MANIFEST, TableDirectoryError, ValueRefusal, load_table_directory, read_dump,
    read_table_directory, store_table_directory, write_table_directory,
};
use panproto_protocols::database::postgresql::{OtherStatements, emit_ddl};

const DUMP: &str = "CREATE TABLE author (id integer PRIMARY KEY, name text NOT NULL, note text);
CREATE TABLE \"Book/Shelf\" (id bigint PRIMARY KEY, author integer REFERENCES author, tags text[], cover bytea);
INSERT INTO author VALUES (1, 'Ada', NULL), (2, 'O''Brien, \"Pat\"', ''), (3, E'tab\\there\\nline', '\\.');
INSERT INTO \"Book/Shelf\" VALUES (10, 1, '{a,\"b c\"}', '\\x00ff'), (11, NULL, NULL, NULL);
";

type Files = BTreeMap<String, Vec<u8>>;

fn files(entries: &[(&str, &str)]) -> Files {
    entries
        .iter()
        .map(|(name, text)| ((*name).to_owned(), text.as_bytes().to_vec()))
        .collect()
}

#[test]
fn a_directory_writes_deterministically_and_reads_back_to_the_same_instance() {
    let first = read_dump(DUMP, OtherStatements::Refuse).unwrap();
    for dialect in [Dialect::Csv, Dialect::Text] {
        let written = write_table_directory(&first.schema, &first.instance, dialect).unwrap();
        assert_eq!(
            write_table_directory(&first.schema, &first.instance, dialect).unwrap(),
            written
        );
        let (schema, instance) = read_table_directory(&written).unwrap();
        assert_eq!(emit_ddl(&schema).unwrap(), emit_ddl(&first.schema).unwrap());
        assert_eq!(instance, first.instance, "{dialect:?}");
        assert_eq!(
            write_table_directory(&schema, &instance, dialect).unwrap(),
            written
        );
    }
    let csv = write_table_directory(&first.schema, &first.instance, Dialect::Csv).unwrap();
    assert_eq!(
        csv.keys().collect::<Vec<_>>(),
        [
            "%42ook%2F%53helf.csv",
            "author.csv",
            "manifest.json",
            "schema.sql"
        ]
    );
    assert_eq!(
        String::from_utf8(csv["author.csv"].clone()).unwrap(),
        "id,name,note\n1,Ada,\n2,\"O'Brien, \"\"Pat\"\"\",\"\"\n3,\"tab\there\nline\",\"\\.\"\n"
    );
    let text = write_table_directory(&first.schema, &first.instance, Dialect::Text).unwrap();
    assert_eq!(
        String::from_utf8(text["author.tsv"].clone()).unwrap(),
        "id\tname\tnote\n1\tAda\t\\N\n2\tO'Brien, \"Pat\"\t\n3\ttab\\there\\nline\t\\\\.\n"
    );
    assert_eq!(
        String::from_utf8(text[MANIFEST].clone()).unwrap(),
        "{\n  \"format\": \"panproto-table-directory\",\n  \"version\": 1,\n  \"protocol\": \"postgresql\",\n  \"schema\": \"schema.sql\",\n  \"dialect\": \"text\",\n  \"tables\": [\n    {\n      \"table\": \"Book/Shelf\",\n      \"file\": \"%42ook%2F%53helf.tsv\"\n    },\n    {\n      \"table\": \"author\",\n      \"file\": \"author.tsv\"\n    }\n  ]\n}\n"
    );
}

const MANIFEST_CSV: &str = r#"{"format": "panproto-table-directory", "version": 1, "protocol": "postgresql", "schema": "s.sql", "dialect": "csv", "tables": [{"table": "t", "file": "t.csv"}]}"#;
const MANIFEST_TEXT: &str = r#"{"format": "panproto-table-directory", "version": 1, "protocol": "postgresql", "schema": "s.sql", "dialect": "text", "tables": [{"table": "t", "file": "t.csv"}]}"#;
const SCHEMA: &str = "CREATE TABLE t (a integer, b text);";

fn read_csv(rows: &str) -> Result<Vec<(Value, Value)>, TableDirectoryError> {
    read_with(MANIFEST_CSV, rows)
}

fn read_with(manifest: &str, rows: &str) -> Result<Vec<(Value, Value)>, TableDirectoryError> {
    let directory = files(&[(MANIFEST, manifest), ("s.sql", SCHEMA), ("t.csv", rows)]);
    let (_, instance) = read_table_directory(&directory)?;
    Ok(instance
        .rows("t")
        .into_iter()
        .map(|(_, r)| (r["a"].clone(), r["b"].clone()))
        .collect())
}

#[test]
fn csv_reads_postgresql_copy_conventions() {
    let text = |s: &str| Value::Str(s.to_owned());
    assert_eq!(
        read_csv("b,a\n\"\",1\n,2\r\n\"x\ny\",\n").unwrap(),
        vec![
            (Value::Int(1), text("")),
            (Value::Int(2), Value::Null),
            (Value::Null, text("x\ny")),
        ]
    );
    assert!(matches!(
        read_csv("a,b\n\\.\n"),
        Err(TableDirectoryError::Record { .. })
    ));
    assert!(matches!(
        read_csv("a,b\n1,\"x\"y\n"),
        Err(TableDirectoryError::Record { .. })
    ));
    assert!(matches!(
        read_csv("a,b\n1\n"),
        Err(TableDirectoryError::Record { .. })
    ));
    assert!(matches!(
        read_csv("a,b\nx,y\n"),
        Err(TableDirectoryError::Value {
            offset: 4,
            reason: ValueRefusal::Syntax { .. },
            ..
        })
    ));
    assert!(matches!(
        read_csv("a\n1\n"),
        Err(TableDirectoryError::Header {
            reason: "leaves out",
            ..
        })
    ));
    assert!(matches!(
        read_csv("a,a,b\n"),
        Err(TableDirectoryError::Header {
            reason: "names twice",
            ..
        })
    ));
}

#[test]
fn text_reads_postgresql_copy_text_conventions() {
    let text = |s: &str| Value::Str(s.to_owned());
    assert_eq!(
        read_with(
            MANIFEST_TEXT,
            "a\tb\n1\t\\N\n\\N\t\n2\ta\\tb\\\\c\\101\\x41\\q\n"
        )
        .unwrap(),
        vec![
            (Value::Int(1), Value::Null),
            (Value::Null, text("")),
            (Value::Int(2), text("a\tb\\cAAq")),
        ]
    );
    assert!(matches!(
        read_with(MANIFEST_TEXT, "a\tb\n\\.\n"),
        Err(TableDirectoryError::Record { .. })
    ));
}

#[test]
fn a_directory_without_a_valid_manifest_is_refused() {
    let only_schema = files(&[("s.sql", SCHEMA)]);
    assert!(matches!(
        read_table_directory(&only_schema),
        Err(TableDirectoryError::NoManifest)
    ));
    let refused = |manifest: &str, extra: &[(&str, &str)]| {
        let mut directory = files(&[(MANIFEST, manifest), ("s.sql", SCHEMA), ("t.csv", "a,b\n")]);
        directory.extend(files(extra));
        read_table_directory(&directory).unwrap_err()
    };
    assert!(matches!(
        refused(
            &MANIFEST_CSV.replace("\"version\": 1", "\"version\": 2"),
            &[]
        ),
        TableDirectoryError::Manifest(_)
    ));
    assert!(matches!(
        refused(&MANIFEST_CSV.replace("\"csv\",", "\"tsv\","), &[]),
        TableDirectoryError::Manifest(_)
    ));
    assert!(matches!(
        refused(&MANIFEST_CSV.replace("\"t.csv\"", "\"u.csv\""), &[]),
        TableDirectoryError::MissingFile(_)
    ));
    assert!(matches!(
        refused(&MANIFEST_CSV.replace("\"t.csv\"", "\"../t.csv\""), &[]),
        TableDirectoryError::FileName(_)
    ));
    assert!(matches!(
        refused(
            &MANIFEST_CSV.replace("\"table\": \"t\"", "\"table\": \"u\""),
            &[]
        ),
        TableDirectoryError::Listing { what: "table", .. }
    ));
    assert!(matches!(
        refused(
            &MANIFEST_CSV.replace(
                "[{\"table\": \"t\", \"file\": \"t.csv\"}]",
                "[{\"table\": \"t\", \"file\": \"t.csv\"}, {\"table\": \"t\", \"file\": \"u.csv\"}]"
            ),
            &[("u.csv", "a,b\n")]
        ),
        TableDirectoryError::Listing { what: "table", .. }
    ));
    let with_rows = files(&[
        (MANIFEST, MANIFEST_CSV),
        (
            "s.sql",
            "CREATE TABLE t (a integer, b text); INSERT INTO t VALUES (1, 'x');",
        ),
        ("t.csv", "a,b\n"),
    ]);
    assert!(matches!(
        read_table_directory(&with_rows),
        Err(TableDirectoryError::Schema { .. })
    ));
}

#[test]
fn the_sakila_subset_round_trips_through_a_directory_on_disk() {
    let corpus = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/conformance/sakila/sakila-data-subset.sql");
    let text = std::fs::read_to_string(corpus).unwrap();
    let reading = read_dump(&text, OtherStatements::List).unwrap();
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("postgresql-table-directory");
    for dialect in [Dialect::Csv, Dialect::Text] {
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let written = write_table_directory(&reading.schema, &reading.instance, dialect).unwrap();
        store_table_directory(&dir, &written).unwrap();
        let loaded = load_table_directory(&dir).unwrap();
        assert_eq!(loaded, written);
        let (schema, instance) = read_table_directory(&loaded).unwrap();
        assert_eq!(
            emit_ddl(&schema).unwrap(),
            emit_ddl(&reading.schema).unwrap()
        );
        assert_eq!(instance, reading.instance);
        assert_eq!(instance.total_row_count(), 931);
    }
    let _ = std::fs::remove_dir_all(&dir);
}
