//! Complete finite scopes for the sql-ddl page.

use crate::pages::sql_ddl::{Column, ColumnConstraint, Script, Table, TableForeignKey, TableKey};

/// The type names of the bounded scope: every name and alias of the
/// types the route reads, with and without modifiers, modifiers out of
/// range, and types the route refuses or does not know.
pub const TYPE_BASES: &[&str] = &[
    "smallint",
    "int2",
    "integer",
    "int",
    "int4",
    "bigint",
    "int8",
    "smallserial",
    "serial",
    "bigserial",
    "numeric",
    "decimal",
    "numeric(7)",
    "numeric(7,2)",
    "decimal(5,5)",
    "numeric(3,4)",
    "numeric(0)",
    "real",
    "float4",
    "double precision",
    "float8",
    "float",
    "float(24)",
    "float(25)",
    "float(54)",
    "text",
    "varchar",
    "varchar(5)",
    "character varying(5)",
    "varchar(0)",
    "char",
    "character",
    "character(3)",
    "bpchar(2)",
    "bpchar",
    "boolean",
    "bool",
    "bytea",
    "date",
    "time",
    "time without time zone",
    "time with time zone",
    "timetz",
    "time(3)",
    "timestamp",
    "timestamp without time zone",
    "timestamp with time zone",
    "timestamptz",
    "timestamp(3)",
    "uuid",
    "json",
    "jsonb",
    "integer(5)",
    "money",
    "interval",
    "xml",
    "tsvector",
    "inet",
    "point",
    "geometry",
];

/// The array suffixes of the bounded scope.
pub const TYPE_SUFFIXES: &[&str] = &["", "[]", "[3]", " array", "[][]"];

/// Every type spelling: each of [`TYPE_BASES`] with each of
/// [`TYPE_SUFFIXES`].
#[must_use]
pub fn type_spellings() -> Vec<String> {
    TYPE_BASES
        .iter()
        .flat_map(|base| TYPE_SUFFIXES.iter().map(move |s| format!("{base}{s}")))
        .collect()
}

/// Every column: four constraints, each with no reference or a reference
/// to one of `tables` tables.
fn columns(tables: usize) -> Vec<Column> {
    let constraints = [
        ColumnConstraint::None,
        ColumnConstraint::NotNull,
        ColumnConstraint::PrimaryKey,
        ColumnConstraint::Unique,
    ];
    constraints
        .into_iter()
        .flat_map(|constraint| {
            std::iter::once(None)
                .chain((0..tables).map(Some))
                .map(move |references| Column {
                    constraint,
                    references,
                })
        })
        .collect()
}

/// Every table of one or two columns, a two-column table with each table
/// key and table foreign key, over references to `tables` tables.
fn tables_of(tables: usize, max_columns: usize) -> Vec<Table> {
    let cols = columns(tables);
    let mut out: Vec<Table> = cols
        .iter()
        .map(|c| Table {
            columns: vec![*c],
            key: TableKey::None,
            foreign_key: TableForeignKey::None,
        })
        .collect();
    if max_columns < 2 {
        return out;
    }
    let keys = [TableKey::None, TableKey::PrimaryKey, TableKey::Unique];
    let foreign: Vec<TableForeignKey> = std::iter::once(TableForeignKey::None)
        .chain((0..tables).map(TableForeignKey::Implicit))
        .chain(std::iter::once(TableForeignKey::Swapped(0)))
        .collect();
    for a in &cols {
        for b in &cols {
            for key in keys {
                for fk in &foreign {
                    out.push(Table {
                        columns: vec![*a, *b],
                        key,
                        foreign_key: *fk,
                    });
                }
            }
        }
    }
    out
}

/// Every script of the bounded scope.
///
/// A table `t` of one or two columns, alone or followed by a one-column
/// table `u`, each column with one of
/// four constraints and a reference to no table, `t` or `u`, and a
/// two-column `t` with each of three table keys and four table foreign
/// keys (none, `(a, b)` to `t` or to `u`, `(b, a)` to `t (a, b)`).
#[must_use]
pub fn ddl_scripts() -> Vec<Script> {
    let firsts = tables_of(2, 2);
    let seconds = tables_of(2, 1);
    let mut out: Vec<Script> = firsts.iter().map(|t| vec![t.clone()]).collect();
    for t in &firsts {
        for u in &seconds {
            out.push(vec![t.clone(), u.clone()]);
        }
    }
    out
}

/// The size of [`ddl_scripts`] by its closed form: a column has `4 * 3`
/// choices, so `t` has `12 + 12^2 * 3 * 4` and `u` has `12`, and a script
/// is `t` alone or `t` then `u`.
#[must_use]
pub const fn ddl_scripts_count() -> usize {
    let column = 4 * 3;
    let t = column + column * column * 3 * 4;
    t + t * column
}
