//! The row constructor and the meanings the family row files share.
//!
//! A [`ConstraintRow`] builds its meaning with a function, since
//! [`ConstraintSemantics::Format`] carries a name; the functions here build
//! the meanings that carry none, so a family file names each meaning once.
//!
//! Three readings recur across the families and are fixed here so every
//! family applies them alike:
//!
//! - A keyword that a property be present (`required`, `optional`,
//!   `exists`, a cardinality's `min`) is about the parent element, which must
//!   have the property, not about the property's own value; a present
//!   property of record or list type has no value of its own, so reading the
//!   keyword as [`ConstraintSemantics::RequiredValue`] would refute every
//!   valid instance that holds one. It is
//!   [`ConstraintSemantics::Annotation`]: presence is the schema's required
//!   edge of the parent. A `NOT NULL` column of a table is the exception: a
//!   row always has a cell for each column, and the keyword constrains the
//!   cell's value, so it is [`ConstraintSemantics::RequiredValue`].
//! - A key or a reference written as a constraint (`PRIMARY KEY`, `UNIQUE`,
//!   `FOREIGN KEY`, `key-type`, `node-key`) is
//!   [`ConstraintSemantics::Annotation`]: keys and references are schema
//!   declarations, never constraint sorts, and are checked from those
//!   declarations.
//! - A constraint that panproto cannot decide from the value (a SQL `CHECK`
//!   expression, an exclusive bound, which no meaning expresses) is
//!   [`ConstraintSemantics::External`]: an application's verdict decides it,
//!   and an instance with no verdict is undecided.

use panproto_schema::{ConstraintSemantics, LengthUnit};

use super::ConstraintRow;

/// The row giving sort `sort` of protocol `protocol` the meaning `semantics`
/// builds.
pub(super) const fn row(
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

/// Descriptive only; no check.
pub(super) const fn annotation() -> ConstraintSemantics {
    ConstraintSemantics::Annotation
}

/// An upper bound on a string's length in Unicode scalar values.
pub(super) const fn max_chars() -> ConstraintSemantics {
    ConstraintSemantics::MaxLength(LengthUnit::Chars)
}

/// A lower bound on a string's length in Unicode scalar values.
pub(super) const fn min_chars() -> ConstraintSemantics {
    ConstraintSemantics::MinLength(LengthUnit::Chars)
}

/// An upper bound on a length in UTF-8 bytes.
pub(super) const fn max_utf8_bytes() -> ConstraintSemantics {
    ConstraintSemantics::MaxLength(LengthUnit::Utf8Bytes)
}

/// A lower bound on a length in UTF-8 bytes.
pub(super) const fn min_utf8_bytes() -> ConstraintSemantics {
    ConstraintSemantics::MinLength(LengthUnit::Utf8Bytes)
}

/// An upper bound on a string's length in extended grapheme clusters.
pub(super) const fn max_graphemes() -> ConstraintSemantics {
    ConstraintSemantics::MaxLength(LengthUnit::Graphemes)
}

/// An inclusive lower bound on a value.
pub(super) const fn minimum() -> ConstraintSemantics {
    ConstraintSemantics::Minimum
}

/// An inclusive upper bound on a value.
pub(super) const fn maximum() -> ConstraintSemantics {
    ConstraintSemantics::Maximum
}

/// The set of permitted values.
pub(super) const fn enumeration() -> ConstraintSemantics {
    ConstraintSemantics::Enum
}

/// The one permitted value.
pub(super) const fn constant() -> ConstraintSemantics {
    ConstraintSemantics::Const
}

/// A regular expression a value matches.
pub(super) const fn pattern() -> ConstraintSemantics {
    ConstraintSemantics::Pattern
}

/// A lower bound on the number of out-edges of a node: the items of a list.
pub(super) const fn min_out_degree() -> ConstraintSemantics {
    ConstraintSemantics::MinOutDegree
}

/// An upper bound on the number of out-edges of a node: the items of a list.
pub(super) const fn max_out_degree() -> ConstraintSemantics {
    ConstraintSemantics::MaxOutDegree
}

/// A value must be present.
pub(super) const fn required_value() -> ConstraintSemantics {
    ConstraintSemantics::RequiredValue
}

/// Checked outside panproto.
pub(super) const fn external() -> ConstraintSemantics {
    ConstraintSemantics::External
}

/// A parameter of value coercion.
pub(super) const fn value_parameter() -> ConstraintSemantics {
    ConstraintSemantics::ValueParameter
}
