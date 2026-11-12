//! `PostgreSQL` data scripts: `INSERT ... VALUES` rows read into a table
//! instance against the schema the script's DDL declares, each value exact
//! for its column's type, refused with its byte offset otherwise, and
//! written back canonically.

#![cfg(feature = "tree-sitter")]
#![allow(clippy::expect_used, clippy::unwrap_used)]

use panproto_inst::value::{DecimalValue, FidelityReason, Temporal};
use panproto_inst::{FInstance, Value};
use panproto_io::default_registry;
use panproto_io::postgresql::{
    SqlDataError, ValueRefusal, read_data, read_dump, write_data, write_dump,
};
use panproto_protocols::database::postgresql::{OtherStatements, SqlDdlError, emit_ddl};

const TYPES: &str = "CREATE TABLE t (
    id integer PRIMARY KEY,
    small smallint,
    big bigint,
    amount numeric(7,2),
    free numeric,
    ratio real,
    wide double precision,
    label varchar(5),
    code character(3),
    body text,
    flag boolean,
    blob bytea,
    day date,
    at time,
    stamp timestamp,
    instant timestamp with time zone,
    tag uuid,
    doc json,
    docb jsonb,
    list integer[],
    grid text[][]
);
";

/// The single row's cell `column` after reading `values` into table `t`.
fn cell(column: &str, literal: &str) -> Result<Value, SqlDataError> {
    let src = format!("{TYPES}INSERT INTO t (id, {column}) VALUES (1, {literal});");
    let reading = read_dump(&src, OtherStatements::Refuse)?;
    let rows = reading.instance.rows("t");
    assert_eq!(rows.len(), 1);
    Ok(rows[0].1[column].clone())
}

fn refusal(column: &str, literal: &str) -> ValueRefusal {
    match cell(column, literal) {
        Err(SqlDataError::Value { reason, offset, .. }) => {
            let src = format!("{TYPES}INSERT INTO t (id, {column}) VALUES (1, ");
            assert_eq!(offset, src.len(), "{column} {literal}");
            reason
        }
        other => panic!("{column} {literal}: expected a value refusal, got {other:?}"),
    }
}

fn decimal(text: &str) -> Value {
    Value::Decimal(DecimalValue::parse(text).unwrap())
}

#[test]
fn integers_read_in_their_type_range() {
    assert_eq!(cell("small", "-32768").unwrap(), Value::Int(-32768));
    assert_eq!(cell("small", "'  12 '").unwrap(), Value::Int(12));
    assert_eq!(cell("small", "'0x1F'").unwrap(), Value::Int(31));
    assert_eq!(cell("small", "'1_000'").unwrap(), Value::Int(1000));
    assert_eq!(cell("small", "1_000").unwrap(), Value::Int(1000));
    assert_eq!(cell("small", "2.0").unwrap(), Value::Int(2));
    assert_eq!(
        cell("big", "-9223372036854775808").unwrap(),
        Value::Int(i64::MIN)
    );
    assert_eq!(
        refusal("small", "32768"),
        ValueRefusal::Fidelity(FidelityReason::Overflow)
    );
    assert_eq!(
        refusal("big", "9223372036854775808"),
        ValueRefusal::Fidelity(FidelityReason::Overflow)
    );
    assert_eq!(refusal("small", "2.5"), ValueRefusal::Rounded);
    assert!(matches!(
        refusal("small", "'1__0'"),
        ValueRefusal::Syntax { .. }
    ));
}

#[test]
fn numerics_keep_their_scale_and_pad_to_a_declared_one() {
    assert_eq!(cell("free", "1.50").unwrap(), decimal("1.50"));
    assert_eq!(cell("free", "1e2").unwrap(), decimal("100"));
    assert_eq!(cell("free", "1.50e1").unwrap(), decimal("15.0"));
    assert_eq!(cell("free", "'-0.00'").unwrap(), decimal("0.00"));
    assert_eq!(
        cell("free", "123456789012345678901234567890").unwrap(),
        decimal("123456789012345678901234567890")
    );
    assert_eq!(cell("free", "'NaN'").unwrap(), decimal("NaN"));
    assert_eq!(cell("free", "'-inf'").unwrap(), decimal("-Infinity"));
    assert_eq!(cell("amount", "1.5").unwrap(), decimal("1.50"));
    assert_eq!(cell("amount", "-12345").unwrap(), decimal("-12345.00"));
    assert_eq!(
        refusal("amount", "1.005"),
        ValueRefusal::Fidelity(FidelityReason::ScaleExceeded { max: 2 })
    );
    assert_eq!(
        refusal("amount", "123456"),
        ValueRefusal::Fidelity(FidelityReason::PrecisionExceeded { max: 7 })
    );
    assert_eq!(
        refusal("amount", "'Infinity'"),
        ValueRefusal::Fidelity(FidelityReason::NonFinite)
    );
}

#[test]
fn floats_read_exactly_and_a_negative_numeric_zero_is_positive() {
    assert_eq!(cell("wide", "0.1").unwrap(), Value::Float(0.1));
    assert_eq!(
        cell("ratio", "'0.1'").unwrap(),
        Value::Float(f64::from(0.1_f32))
    );
    let negative_zero = cell("wide", "'-0'").unwrap();
    assert!(matches!(negative_zero, Value::Float(x) if x == 0.0 && x.is_sign_negative()));
    let numeric_zero = cell("wide", "-0.0").unwrap();
    assert!(matches!(numeric_zero, Value::Float(x) if x == 0.0 && x.is_sign_positive()));
    assert!(matches!(cell("wide", "'NaN'").unwrap(), Value::Float(x) if x.is_nan()));
    assert_eq!(
        cell("wide", "'-Infinity'").unwrap(),
        Value::Float(f64::NEG_INFINITY)
    );
    assert!(matches!(
        refusal("ratio", "16777217"),
        ValueRefusal::Fidelity(FidelityReason::PrecisionExceeded { .. })
    ));
    assert_eq!(
        refusal("wide", "1e400"),
        ValueRefusal::Fidelity(FidelityReason::Overflow)
    );
}

#[test]
fn strings_decode_by_their_lexical_rules_and_null_is_not_empty() {
    assert_eq!(cell("body", "'it''s'").unwrap(), Value::Str("it's".into()));
    assert_eq!(cell("body", "'a\\nb'").unwrap(), Value::Str("a\\nb".into()));
    assert_eq!(cell("body", "E'a\\nb'").unwrap(), Value::Str("a\nb".into()));
    assert_eq!(
        cell("body", "E'\\x41\\101\\u00e9\\U0001F600'").unwrap(),
        Value::Str("AA\u{e9}\u{1F600}".into())
    );
    assert_eq!(cell("body", "$q$a'b$q$").unwrap(), Value::Str("a'b".into()));
    assert_eq!(
        cell("body", "'ab'\n  'cd'").unwrap(),
        Value::Str("abcd".into())
    );
    assert_eq!(cell("body", "''").unwrap(), Value::Str(String::new()));
    assert_eq!(cell("body", "NULL").unwrap(), Value::Null);
    assert_eq!(
        cell("label", "'ab   '").unwrap(),
        Value::Str("ab   ".into())
    );
    assert_eq!(
        cell("label", "'abcde   '").unwrap(),
        Value::Str("abcde".into())
    );
    assert_eq!(cell("code", "'a'").unwrap(), Value::Str("a  ".into()));
    assert_eq!(
        refusal("label", "'abcdef'"),
        ValueRefusal::TooLong { max: 5 }
    );
    assert!(matches!(
        cell("body", "E'\\000'"),
        Err(SqlDataError::Script(SqlDdlError::Syntax { .. }))
    ));
}

#[test]
fn standard_conforming_strings_off_reads_backslash_escapes() {
    let src = format!(
        "{TYPES}SET standard_conforming_strings = off;\nINSERT INTO t (id, body) VALUES (1, 'a\\tb');"
    );
    let reading = read_dump(&src, OtherStatements::List).unwrap();
    assert_eq!(
        reading.instance.rows("t")[0].1["body"],
        Value::Str("a\tb".into())
    );
    let quoted = format!(
        "{TYPES}SET standard_conforming_strings = off;\nINSERT INTO t (id, body) VALUES (1, 'a\\'');"
    );
    assert!(matches!(
        read_dump(&quoted, OtherStatements::List),
        Err(SqlDataError::Script(SqlDdlError::Unsupported { .. }))
    ));
}

#[test]
fn booleans_bytea_and_uuids_read_their_input_forms() {
    assert_eq!(cell("flag", "TRUE").unwrap(), Value::Bool(true));
    for spelling in ["'t'", "'yes'", "' On '", "'1'", "'TRU'"] {
        assert_eq!(
            cell("flag", spelling).unwrap(),
            Value::Bool(true),
            "{spelling}"
        );
    }
    for spelling in ["'f'", "'no'", "'off'", "'0'"] {
        assert_eq!(
            cell("flag", spelling).unwrap(),
            Value::Bool(false),
            "{spelling}"
        );
    }
    assert!(matches!(
        refusal("flag", "'o'"),
        ValueRefusal::Syntax { .. }
    ));
    assert_eq!(
        refusal("flag", "1"),
        ValueRefusal::Unsupported("a numeric constant for a column of a non-numeric type")
    );
    assert_eq!(
        cell("blob", "'\\x00ff 10'").unwrap(),
        Value::Bytes(vec![0, 255, 16])
    );
    assert_eq!(
        cell("blob", "'a\\\\b\\001'").unwrap(),
        Value::Bytes(b"a\\b\x01".to_vec())
    );
    assert!(matches!(
        refusal("blob", "'\\x0'"),
        ValueRefusal::Syntax { .. }
    ));
    assert_eq!(
        cell("tag", "'{A0EEBC99-9C0B4EF8-BB6D6BB9-BD380A11}'").unwrap(),
        Value::Str("a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11".into())
    );
}

#[test]
fn temporals_keep_their_written_form_and_offset() {
    assert_eq!(
        cell("day", "'1994-11-27'").unwrap(),
        Value::Temporal(Temporal::parse_date("1994-11-27").unwrap())
    );
    assert_eq!(
        cell("stamp", "'2006-02-15 09:34:33.000'").unwrap(),
        Value::Temporal(Temporal::parse_date_time("2006-02-15 09:34:33.000").unwrap())
    );
    assert_eq!(
        cell("instant", "'2006-02-15 09:34:33+05:30'").unwrap(),
        Value::Temporal(Temporal::parse_date_time("2006-02-15 09:34:33+05:30").unwrap())
    );
    assert_eq!(
        refusal("instant", "'2006-02-15 09:34:33'"),
        ValueRefusal::Fidelity(FidelityReason::OffsetRequired)
    );
    assert_eq!(
        refusal("stamp", "'2006-02-15 09:34:33Z'"),
        ValueRefusal::Fidelity(FidelityReason::OffsetForbidden)
    );
    assert_eq!(refusal("at", "'10:00:00.1234567'"), ValueRefusal::Rounded);
    assert_eq!(
        refusal("day", "'January 8, 1999'"),
        ValueRefusal::Fidelity(FidelityReason::InvalidTemporal)
    );
}

#[test]
fn arrays_and_json_read_as_the_column_declares() {
    assert_eq!(
        cell("list", "'{1, NULL ,3}'").unwrap(),
        Value::List(vec![Value::Int(1), Value::Null, Value::Int(3)])
    );
    assert_eq!(cell("list", "'{}'").unwrap(), Value::List(vec![]));
    assert_eq!(
        cell("grid", "'{{a,\"b c\"},{\"NULL\",NULL}}'").unwrap(),
        Value::List(vec![
            Value::List(vec![Value::Str("a".into()), Value::Str("b c".into())]),
            Value::List(vec![Value::Str("NULL".into()), Value::Null]),
        ])
    );
    assert_eq!(
        refusal("grid", "'{a,b}'"),
        ValueRefusal::Dimensions {
            declared: 2,
            found: 1
        }
    );
    assert!(matches!(
        refusal("grid", "'{{a},{b,c}}'"),
        ValueRefusal::Syntax { .. }
    ));
    assert_eq!(
        refusal("list", "'[0:1]={1,2}'"),
        ValueRefusal::Unsupported("an array with explicit bounds")
    );
    assert_eq!(
        cell("doc", "'{\"b\": 1,  \"a\": [1e400]}'").unwrap(),
        Value::Str("{\"b\": 1,  \"a\": [1e400]}".into())
    );
    assert!(matches!(
        refusal("docb", "'{'"),
        ValueRefusal::Syntax { .. }
    ));
}

#[test]
fn statements_the_reader_does_not_read_are_refused_with_their_offset() {
    let refused = |insert: &str| -> SqlDdlError {
        let src = format!("{TYPES}{insert}");
        match read_dump(&src, OtherStatements::List) {
            Err(SqlDataError::Script(error)) => {
                assert!(error.offset() >= TYPES.len(), "{insert}: {error}");
                error
            }
            other => panic!("{insert}: {other:?}"),
        }
    };
    let feature = |insert: &str| match refused(insert) {
        SqlDdlError::Unsupported { feature, .. } => feature,
        other => panic!("{insert}: {other}"),
    };
    assert_eq!(
        feature("INSERT INTO t VALUES (1, DEFAULT);"),
        "DEFAULT in VALUES"
    );
    assert_eq!(feature("INSERT INTO t SELECT 1;"), "INSERT ... SELECT");
    assert_eq!(
        feature("INSERT INTO t (id) VALUES (1) ON CONFLICT DO NOTHING;"),
        "ON CONFLICT"
    );
    assert_eq!(
        feature("INSERT INTO t (id) VALUES (1) RETURNING id;"),
        "RETURNING"
    );
    assert_eq!(feature("INSERT INTO t DEFAULT VALUES;"), "DEFAULT VALUES");
    assert_eq!(
        feature("INSERT INTO t (id) VALUES (1 + 1);"),
        "an expression other than a constant"
    );
    assert_eq!(
        feature("INSERT INTO t (id) VALUES ('1'::integer);"),
        "a type cast"
    );
    assert_eq!(
        feature("INSERT INTO t (day) VALUES (DATE '2020-01-01');"),
        "a typed constant"
    );
    assert_eq!(
        feature("INSERT INTO t (id) VALUES (1); DROP TABLE t;"),
        "DROP TABLE of a table holding inserted rows"
    );
    assert_eq!(
        feature("INSERT INTO t (id) VALUES (1); ALTER TABLE t ADD COLUMN extra integer;"),
        "ALTER TABLE ADD COLUMN on a table holding inserted rows"
    );
    assert!(feature("UPDATE t SET id = 2;").starts_with("statement UPDATE"));
    assert!(matches!(
        refused("INSERT INTO nowhere VALUES (1);"),
        SqlDdlError::UnknownTable { .. }
    ));
    assert!(matches!(
        refused("INSERT INTO t (id, nope) VALUES (1, 2);"),
        SqlDdlError::UnknownColumn { .. }
    ));
    assert!(matches!(
        refused("INSERT INTO t (id, id) VALUES (1, 2);"),
        SqlDdlError::Duplicate { .. }
    ));
    assert!(matches!(
        refused("INSERT INTO t (id, small) VALUES (1);"),
        SqlDdlError::Syntax { .. }
    ));
    assert!(matches!(
        refused("INSERT INTO t (id) VALUES (U&'d\\0061t');"),
        SqlDdlError::Unsupported { .. }
    ));
    let early = "INSERT INTO later VALUES (1); CREATE TABLE later (a integer);";
    assert!(matches!(
        read_dump(early, OtherStatements::List),
        Err(SqlDataError::Script(SqlDdlError::UnknownTable {
            offset: 12,
            ..
        }))
    ));
}

#[test]
fn a_left_out_column_is_null_unless_it_has_a_default() {
    let src = "CREATE TABLE a (x integer, y text);\nINSERT INTO a (x) VALUES (1);\nINSERT INTO a VALUES (2);";
    let reading = read_dump(src, OtherStatements::Refuse).unwrap();
    let rows = reading.instance.rows("a");
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|(_, row)| row["y"] == Value::Null));
    let defaulted = "CREATE TABLE a (x serial, y text);\nINSERT INTO a (y) VALUES ('v');";
    assert!(matches!(
        read_dump(defaulted, OtherStatements::Refuse),
        Err(SqlDataError::Script(SqlDdlError::Unsupported { feature, .. }))
            if feature == "column x left to its DEFAULT"
    ));
}

const DUMP: &str = "SET client_encoding = 'UTF8';
SET standard_conforming_strings = on;
CREATE TYPE mood AS ENUM ('sad', 'ok');
CREATE TABLE author (id integer PRIMARY KEY, name text NOT NULL, mood mood);
CREATE TABLE book (
    id bigint PRIMARY KEY,
    author integer REFERENCES author,
    price numeric(6,2),
    tags text[],
    added timestamp with time zone,
    cover bytea,
    ratio real
);
INSERT INTO book VALUES (10, 1, 12.5, '{a,\"b,c\"}', '2020-01-01 10:00:00+00', '\\xdead', '0.1');
INSERT INTO author (id, name, mood) VALUES (2, 'O''Brien', 'ok'), (1, E'Ada\\n', NULL);
INSERT INTO book VALUES (11, NULL, -0.5, NULL, NULL, NULL, '-0');
SELECT pg_catalog.setval('book_seq', 11, true);
";

#[test]
fn a_dump_writes_canonically_and_reads_back_to_the_same_instance() {
    let first = read_dump(DUMP, OtherStatements::List).unwrap();
    assert_eq!(first.not_read.len(), 3);
    // Rows are numbered by table name, then in script order.
    let authors = first.instance.rows("author");
    assert_eq!(
        authors.iter().map(|(id, _)| id.0).collect::<Vec<_>>(),
        [0, 1]
    );
    assert_eq!(authors[0].1["name"], Value::Str("O'Brien".into()));
    assert_eq!(
        first
            .instance
            .rows("book")
            .iter()
            .map(|(id, _)| id.0)
            .collect::<Vec<_>>(),
        [2, 3]
    );
    let written = write_dump(&first.schema, &first.instance).unwrap();
    let second = read_dump(&written, OtherStatements::Refuse).unwrap();
    assert_eq!(
        emit_ddl(&second.schema).unwrap(),
        emit_ddl(&first.schema).unwrap()
    );
    assert_eq!(second.instance, first.instance);
    assert_eq!(
        write_dump(&second.schema, &second.instance).unwrap(),
        written
    );
    assert_eq!(
        write_data(&first.schema, &first.instance).unwrap(),
        "INSERT INTO author (id, name, mood) VALUES\n    (2, 'O''Brien', 'ok'),\n    (1, 'Ada\n', NULL);\nINSERT INTO book (id, author, price, tags, added, cover, ratio) VALUES\n    (10, 1, 12.50, '{a,\"b,c\"}', '2020-01-01 10:00:00+00', '\\xdead', '0.1'),\n    (11, NULL, -0.50, NULL, NULL, NULL, '-0');\n"
    );
}

#[test]
fn a_data_only_script_reads_against_a_given_schema() {
    let schema = read_dump(DUMP, OtherStatements::List).unwrap().schema;
    let data = "ALTER TABLE author DISABLE TRIGGER ALL;\nINSERT INTO author VALUES (1, 'x', 'sad');\nALTER TABLE author ENABLE TRIGGER ALL;";
    let reading = read_data(data, &schema, OtherStatements::List).unwrap();
    assert_eq!(reading.instance.rows("author").len(), 1);
    assert_eq!(
        reading.instance.rows("author")[0].1["mood"],
        Value::Str("sad".into())
    );
    assert!(matches!(
        read_data(
            "INSERT INTO author VALUES (1, 'x', 'glad');",
            &schema,
            OtherStatements::List
        ),
        Err(SqlDataError::Value {
            reason: ValueRefusal::NotALabel,
            ..
        })
    ));
    assert!(matches!(
        read_data("CREATE TABLE z (a integer);", &schema, OtherStatements::List),
        Err(SqlDataError::Script(SqlDdlError::Unsupported { feature, .. }))
            if feature == "a schema statement in a data-only script"
    ));
}

#[test]
fn the_registry_reads_and_writes_postgresql_tables() {
    let registry = default_registry();
    assert!(registry.supports_table("postgresql"));
    let first = read_dump(DUMP, OtherStatements::List).unwrap();
    let bytes = registry
        .emit_table("postgresql", &first.schema, &first.instance)
        .unwrap();
    let back: FInstance = registry
        .parse_table("postgresql", &first.schema, &bytes)
        .unwrap();
    assert_eq!(back, first.instance);
    let data_only = write_data(&first.schema, &first.instance).unwrap();
    let back = registry
        .parse_table("postgresql", &first.schema, data_only.as_bytes())
        .unwrap();
    assert_eq!(back, first.instance);
    let other = read_dump("CREATE TABLE q (a integer);", OtherStatements::List)
        .unwrap()
        .schema;
    assert!(registry.parse_table("postgresql", &other, &bytes).is_err());
}
