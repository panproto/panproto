//! Constraint-semantics rows for the data science protocols (`arrow`,
//! `parquet`, `dataframe`).
//!
//! An Arrow decimal's `precision` and `scale` parameterize value coercion;
//! its other type parameters (`bit-width`, `unit`, `timezone`) and field
//! metadata describe. A `nullable` flag is the inverse of a required value
//! (`nullable = false` requires one), which no meaning reads from the flag as
//! written, so it describes; a pandera `unique` column is a key, which a
//! schema declares as such. Parquet's repetition, logical and converted
//! types and field ids describe the physical layout.
//!
//! A pandera column's `ge` and `le` checks are inclusive bounds and `isin`
//! lists the permitted values; `gt` and `lt` are exclusive bounds, which no
//! meaning expresses, so they are decided outside panproto (see the
//! [`common`](super::common) module). `coerce` and `regex` (whether the
//! column name is a pattern) describe.

use super::ConstraintRow;
use super::common::{annotation, enumeration, external, maximum, minimum, row, value_parameter};

/// The rows of this family, one per declaration.
pub(super) const ROWS: &[ConstraintRow] = &[
    row("arrow", "nullable", annotation),
    row("arrow", "metadata", annotation),
    row("arrow", "timezone", annotation),
    row("arrow", "unit", annotation),
    row("arrow", "precision", value_parameter),
    row("arrow", "scale", value_parameter),
    row("arrow", "bit-width", annotation),
    row("parquet", "repetition", annotation),
    row("parquet", "logical-type", annotation),
    row("parquet", "converted-type", annotation),
    row("parquet", "field-id", annotation),
    row("dataframe", "nullable", annotation),
    row("dataframe", "unique", annotation),
    row("dataframe", "coerce", annotation),
    row("dataframe", "regex", annotation),
    row("dataframe", "ge", minimum),
    row("dataframe", "le", maximum),
    row("dataframe", "gt", external),
    row("dataframe", "lt", external),
    row("dataframe", "isin", enumeration),
];
