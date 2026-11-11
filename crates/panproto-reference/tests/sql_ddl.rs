//! Direct witnesses for the sql-ddl reference: column types on the names
//! of `PostgreSQL` 17 Table 8.1, keys and foreign keys on the examples of
//! the `CREATE TABLE` reference page, and the bounded scopes' closed-form
//! counts.

#![allow(clippy::expect_used)]

use panproto_reference::enumerate::sql_ddl::{
    TYPE_BASES, TYPE_SUFFIXES, ddl_scripts, ddl_scripts_count, type_spellings,
};
use panproto_reference::pages::sql_ddl::{
    Column, ColumnConstraint, ScriptRefusal, Table, TableForeignKey, TableKey, TypeRefusal,
    canonical_text, column_type, read_script, render,
};

const fn column(constraint: ColumnConstraint, references: Option<usize>) -> Column {
    Column {
        constraint,
        references,
    }
}

const fn table(columns: Vec<Column>, key: TableKey, foreign_key: TableForeignKey) -> Table {
    Table {
        columns,
        key,
        foreign_key,
    }
}

#[test]
fn column_types_read_as_table_8_1_names_them() {
    let int = column_type("int4").expect("int4");
    assert_eq!(int.kind, "integer");
    assert_eq!(int.parameters, vec![("int_bits", "32".to_owned())]);
    let money = column_type("numeric(7,2)[]").expect("numeric");
    assert_eq!(money.dims, 1);
    assert_eq!(
        money.parameters,
        vec![("precision", "7".to_owned()), ("scale", "2".to_owned())]
    );
    assert_eq!(column_type("float(24)").expect("float").kind, "real");
    assert_eq!(column_type("float(25)").expect("float").kind, "double");
    assert_eq!(column_type("serial").expect("serial").column.len(), 2);
    assert_eq!(column_type("serial[]"), Err(TypeRefusal::Unsupported));
    assert_eq!(column_type("money"), Err(TypeRefusal::Unsupported));
    assert_eq!(column_type("geometry"), Err(TypeRefusal::UnknownType));
    assert_eq!(column_type("varchar(0)"), Err(TypeRefusal::Syntax));
    assert_eq!(
        type_spellings().len(),
        TYPE_BASES.len() * TYPE_SUFFIXES.len()
    );
    assert_eq!(type_spellings().len(), 300);
}

#[test]
fn keys_and_foreign_keys_resolve_as_create_table_defines_them() {
    let none = TableForeignKey::None;
    // CREATE TABLE t (a integer PRIMARY KEY);
    // CREATE TABLE u (a integer REFERENCES t);
    let script = vec![
        table(
            vec![column(ColumnConstraint::PrimaryKey, None)],
            TableKey::None,
            none,
        ),
        table(
            vec![column(ColumnConstraint::None, Some(0))],
            TableKey::None,
            none,
        ),
    ];
    assert_eq!(
        render(&script),
        "CREATE TABLE t (a integer PRIMARY KEY);\nCREATE TABLE u (a integer REFERENCES t);\n"
    );
    let read = read_script(&script).expect("reads");
    let u = &read[1];
    assert_eq!(u.references[0].name, "u_a_fkey");
    assert_eq!(u.references[0].target_key, "t_pkey");
    assert!(u.references[0].nullable);
    assert_eq!(
        canonical_text(&read),
        "CREATE TABLE t (\n    a integer NOT NULL,\n    CONSTRAINT t_pkey PRIMARY KEY (a)\n);\n\n\
         CREATE TABLE u (\n    a integer\n);\n\n\
         ALTER TABLE u ADD CONSTRAINT u_a_fkey FOREIGN KEY (a) REFERENCES t (a);\n"
    );
    // A reference to a table declared later, or to a table without a
    // primary key, is refused.
    let forward = vec![
        table(
            vec![column(ColumnConstraint::None, Some(1))],
            TableKey::None,
            none,
        ),
        table(
            vec![column(ColumnConstraint::None, None)],
            TableKey::None,
            none,
        ),
    ];
    assert_eq!(read_script(&forward), Err((0, ScriptRefusal::UnknownTable)));
    let keyless = vec![table(
        vec![column(ColumnConstraint::Unique, Some(0))],
        TableKey::None,
        none,
    )];
    assert_eq!(
        read_script(&keyless),
        Err((0, ScriptRefusal::NoMatchingKey))
    );
    // `FOREIGN KEY (b, a) REFERENCES t (a, b)` pairs b with a and a with b.
    let swapped = vec![table(
        vec![
            column(ColumnConstraint::None, None),
            column(ColumnConstraint::None, None),
        ],
        TableKey::Unique,
        TableForeignKey::Swapped(0),
    )];
    let read = read_script(&swapped).expect("reads");
    assert_eq!(read[0].references[0].name, "t_b_a_fkey");
    assert_eq!(read[0].references[0].columns, ["b", "a"]);
    assert_eq!(read[0].references[0].target_key, "t_a_b_key");
}

#[test]
fn the_script_scope_has_its_closed_form_count() {
    assert_eq!(ddl_scripts_count(), 22_620);
    assert_eq!(ddl_scripts().len(), ddl_scripts_count());
}
