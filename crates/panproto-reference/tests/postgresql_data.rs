//! Direct witnesses for the postgresql-data reference: input forms the
//! documentation gives for each type, the rows an `INSERT` adds, and the
//! bounded scopes' closed-form counts.

#![allow(clippy::expect_used)]

use panproto_reference::enumerate::postgresql_data::{
    SCRIPTS_COUNT, SPELLINGS, STATEMENTS_COUNT, scripts, statements, value_cases,
};
use panproto_reference::pages::postgresql_data::{
    Cell, Insert, Refusal, ScriptRefusal, TYPES, canonical_text, input_value, read_script, render,
};

#[test]
fn input_forms_read_as_the_type_documentation_gives_them() {
    assert_eq!(input_value("smallint", " -7 "), Ok("int:-7".to_owned()));
    assert_eq!(input_value("integer", "0x1F"), Ok("int:31".to_owned()));
    assert_eq!(input_value("smallint", "32768"), Err(Refusal::Overflow));
    assert_eq!(input_value("integer", "1__0"), Err(Refusal::Syntax));
    assert_eq!(input_value("numeric", "1e2"), Ok("numeric:100".to_owned()));
    assert_eq!(
        input_value("numeric", "1.50e1"),
        Ok("numeric:15.0".to_owned())
    );
    assert_eq!(
        input_value("numeric", "-0.00"),
        Ok("numeric:0.00".to_owned())
    );
    assert_eq!(
        input_value("numeric(5,2)", "1.5"),
        Ok("numeric:1.50".to_owned())
    );
    assert_eq!(input_value("numeric(5,2)", "999.995"), Err(Refusal::Scale));
    assert_eq!(
        input_value("numeric(5,2)", "1234.5"),
        Err(Refusal::Precision)
    );
    assert_eq!(input_value("numeric(5,2)", "-inf"), Err(Refusal::NonFinite));
    assert_eq!(input_value("boolean", "of"), Ok("bool:false".to_owned()));
    assert_eq!(input_value("boolean", "o"), Err(Refusal::Syntax));
    assert_eq!(
        input_value("bytea", "\\x 0a 0B"),
        Ok("bytes:0a0b".to_owned())
    );
    assert_eq!(
        input_value("bytea", "a\\\\b"),
        Ok("bytes:615c62".to_owned())
    );
    assert_eq!(
        input_value("varchar(3)", "abc  "),
        Ok("text:abc".to_owned())
    );
    assert_eq!(input_value("character(3)", "ab"), Ok("text:ab ".to_owned()));
    assert_eq!(input_value("varchar(3)", "abcd"), Err(Refusal::TooLong));
    assert_eq!(
        input_value("uuid", "{A0EEBC999C0B4EF8BB6D6BB9BD380A11}"),
        Ok("text:a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11".to_owned())
    );
}

#[test]
fn an_insert_adds_rows_as_the_insert_page_defines_them() {
    let into_a = |columns: Option<Vec<usize>>, row: Vec<Cell>| Insert {
        into_b: false,
        columns,
        rows: vec![row],
    };
    // INSERT INTO a (x, y, z) VALUES (-1, 'it''s', NULL);
    let script = vec![into_a(
        Some(vec![0, 1, 2]),
        vec![Cell::Int(-1), Cell::Text("it's".to_owned()), Cell::Null],
    )];
    assert_eq!(
        render(&script),
        "INSERT INTO a (x, y, z) VALUES (-1, 'it''s', NULL);\n"
    );
    let (a, b) = read_script(&script).expect("rows");
    assert_eq!(
        a,
        vec![vec![
            Some("int:-1".to_owned()),
            Some("text:it's".to_owned()),
            None
        ]]
    );
    assert_eq!(b, Vec::<Vec<Option<String>>>::new());
    assert_eq!(
        canonical_text(&a, &b),
        "INSERT INTO a (x, y, z) VALUES\n    (-1, 'it''s', NULL);\n"
    );
    // z declares a DEFAULT, which a row may not leave to be evaluated.
    assert_eq!(
        read_script(&[into_a(Some(vec![0]), vec![Cell::Int(1)])]),
        Err(ScriptRefusal::Unsupported)
    );
    assert_eq!(
        read_script(&[into_a(None, vec![Cell::Null, Cell::Int(1), Cell::Null])]),
        Err(ScriptRefusal::Value)
    );
    assert_eq!(
        read_script(&[into_a(Some(vec![1, 0]), vec![Cell::Null])]),
        Err(ScriptRefusal::Syntax)
    );
}

#[test]
fn the_scopes_have_their_closed_form_counts() {
    assert_eq!(value_cases().len(), TYPES.len() * SPELLINGS.len());
    assert_eq!(value_cases().len(), 11 * 64);
    assert_eq!(statements().len(), STATEMENTS_COUNT);
    assert_eq!(STATEMENTS_COUNT, 835);
    assert_eq!(scripts().len(), SCRIPTS_COUNT);
    assert_eq!(SCRIPTS_COUNT, 2060);
}
