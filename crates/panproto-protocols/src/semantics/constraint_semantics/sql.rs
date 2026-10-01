//! Constraint-semantics rows for the `sql` protocol.
//!
//! `NOT NULL` requires a value in the column's cell of every row. `PRIMARY
//! KEY`, `UNIQUE` and `FOREIGN KEY` are keys and references, which a schema
//! declares as such, and `DEFAULT` gives a default; they describe here (see
//! the [`common`](super::common) module). A `CHECK` constraint is an SQL
//! expression, which validation does not evaluate, so it is decided outside
//! panproto.
//!
//! An enum type's `ENUM` labels are written as SQL string literals, each in
//! its quotes (`'G','PG'`), while an enumeration constraint reads its members
//! as a JSON array or a comma-separated list of the members themselves; read
//! as an enumeration, every label would keep its quotes and every valid cell
//! would fail. The labels are therefore decided outside panproto.

use super::ConstraintRow;
use super::common::{annotation, external, required_value, row};

/// The rows of this family, one per declaration.
pub(super) const ROWS: &[ConstraintRow] = &[
    row("sql", "NOT NULL", required_value),
    row("sql", "UNIQUE", annotation),
    row("sql", "CHECK", external),
    row("sql", "PRIMARY KEY", annotation),
    row("sql", "DEFAULT", annotation),
    row("sql", "FOREIGN KEY", annotation),
    row("sql", "ENUM", external),
];
