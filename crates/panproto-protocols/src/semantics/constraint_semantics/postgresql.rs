//! Constraint-semantics rows for the `postgresql` protocol.
//!
//! `NOT NULL` requires a value in the column's cell of every row. Keys and
//! foreign keys are schema declarations, not constraints. `DEFAULT` names
//! the value a missing cell takes, `TYPE` names the enum or domain type a
//! column is declared with, and `FOREIGN KEY ACTION` the referential
//! actions of a foreign key; none constrains a cell, so these are
//! annotations. A `CHECK` or domain `CHECK` constraint is an SQL
//! expression, which validation does not evaluate, so it is decided
//! outside panproto: an application's verdict decides it, and an instance
//! with no verdict is undecided. An enum type's labels are written as a
//! JSON array of strings, so `ENUM` is an enumeration. `int_bits`,
//! `precision`, `scale`, `max_length` and `with_time_zone` are the value
//! parameters of the column's type.

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
    row("postgresql", "NOT NULL", || {
        ConstraintSemantics::RequiredValue
    }),
    row("postgresql", "DEFAULT", || ConstraintSemantics::Annotation),
    row("postgresql", "CHECK", || ConstraintSemantics::External),
    row("postgresql", "ENUM", || ConstraintSemantics::Enum),
    row("postgresql", "TYPE", || ConstraintSemantics::Annotation),
    row("postgresql", "DOMAIN CHECK", || {
        ConstraintSemantics::External
    }),
    row("postgresql", "FOREIGN KEY ACTION", || {
        ConstraintSemantics::Annotation
    }),
    row("postgresql", "int_bits", || {
        ConstraintSemantics::ValueParameter
    }),
    row("postgresql", "precision", || {
        ConstraintSemantics::ValueParameter
    }),
    row("postgresql", "scale", || {
        ConstraintSemantics::ValueParameter
    }),
    row("postgresql", "max_length", || {
        ConstraintSemantics::ValueParameter
    }),
    row("postgresql", "with_time_zone", || {
        ConstraintSemantics::ValueParameter
    }),
];
