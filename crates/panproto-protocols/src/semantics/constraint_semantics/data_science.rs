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
//! meaning expresses, so they are decided outside panproto: an
//! application's verdict decides each, and an instance with no verdict is
//! undecided. `coerce` and `regex` (whether the column name is a pattern)
//! describe.

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
    row("arrow", "nullable", || ConstraintSemantics::Annotation),
    row("arrow", "metadata", || ConstraintSemantics::Annotation),
    row("arrow", "timezone", || ConstraintSemantics::Annotation),
    row("arrow", "unit", || ConstraintSemantics::Annotation),
    row("arrow", "precision", || ConstraintSemantics::ValueParameter),
    row("arrow", "scale", || ConstraintSemantics::ValueParameter),
    row("arrow", "bit-width", || ConstraintSemantics::Annotation),
    row("parquet", "repetition", || ConstraintSemantics::Annotation),
    row("parquet", "logical-type", || {
        ConstraintSemantics::Annotation
    }),
    row("parquet", "converted-type", || {
        ConstraintSemantics::Annotation
    }),
    row("parquet", "field-id", || ConstraintSemantics::Annotation),
    row("dataframe", "nullable", || ConstraintSemantics::Annotation),
    row("dataframe", "unique", || ConstraintSemantics::Annotation),
    row("dataframe", "coerce", || ConstraintSemantics::Annotation),
    row("dataframe", "regex", || ConstraintSemantics::Annotation),
    row("dataframe", "ge", || ConstraintSemantics::Minimum),
    row("dataframe", "le", || ConstraintSemantics::Maximum),
    row("dataframe", "gt", || ConstraintSemantics::External),
    row("dataframe", "lt", || ConstraintSemantics::External),
    row("dataframe", "isin", || ConstraintSemantics::Enum),
];
