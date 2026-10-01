//! Constraint-semantics rows for the `sql` protocol.
//!
//! `NOT NULL` requires a value in the column's cell of every row. `PRIMARY
//! KEY`, `UNIQUE` and `FOREIGN KEY` are keys and references, which a schema
//! declares as such and checks from those declarations, and `DEFAULT` names
//! the value a missing cell takes, which constrains no cell. These four are
//! therefore annotations here, from which no check is compiled. A `CHECK` constraint is an SQL expression,
//! which validation does not evaluate, so it is decided outside panproto: an
//! application's verdict decides it, and an instance with no verdict is
//! undecided.
//!
//! An enum type's `ENUM` labels are written as SQL string literals, each in
//! its quotes (`'G','PG'`), while an enumeration constraint reads its members
//! as a JSON array or a comma-separated list of the members themselves; read
//! as an enumeration, every label would keep its quotes and every valid cell
//! would fail. The labels are therefore decided outside panproto.

use panproto_schema::ConstraintSemantics;

use super::ConstraintRow;

/// The row giving sort `sort` of protocol `protocol` the meaning `semantics`
/// builds.
const fn row(
    protocol: &'static str,
    sort: &'static str,
    semantics: fn() -> ConstraintSemantics,
) -> ConstraintRow {
    ConstraintRow {
        protocol,
        sort,
        semantics,
    }
}

/// The rows of this family, one per declaration.
pub(super) const ROWS: &[ConstraintRow] = &[
    row("sql", "NOT NULL", || ConstraintSemantics::RequiredValue),
    row("sql", "UNIQUE", || ConstraintSemantics::Annotation),
    row("sql", "CHECK", || ConstraintSemantics::External),
    row("sql", "PRIMARY KEY", || ConstraintSemantics::Annotation),
    row("sql", "DEFAULT", || ConstraintSemantics::Annotation),
    row("sql", "FOREIGN KEY", || ConstraintSemantics::Annotation),
    row("sql", "ENUM", || ConstraintSemantics::External),
];
