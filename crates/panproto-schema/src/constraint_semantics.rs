//! What a protocol's constraint sorts mean.
//!
//! A [`Protocol`](crate::Protocol) maps each of its constraint sorts to a
//! [`ConstraintSemantics`] in
//! [`Protocol::constraint_semantics`](crate::Protocol::constraint_semantics).
//! Identifiers and uniqueness are not constraint sorts: a schema declares
//! them in [`SchemaDeclarations`](crate::SchemaDeclarations).
//!
//! Enums serialize externally tagged in `snake_case`.

use panproto_gat::Name;
use serde::{Deserialize, Serialize};

/// The meaning of one constraint sort.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConstraintSemantics {
    /// Descriptive only; no check.
    Annotation,
    /// An upper bound on a value's length, counted in the given unit.
    MaxLength(LengthUnit),
    /// A lower bound on a value's length, counted in the given unit.
    MinLength(LengthUnit),
    /// An inclusive lower bound on a value.
    Minimum,
    /// An inclusive upper bound on a value.
    Maximum,
    /// The set of permitted values.
    Enum,
    /// The one permitted value.
    Const,
    /// A regular expression a value matches.
    Pattern,
    /// A named value format.
    Format(Name),
    /// A lower bound on the number of out-edges of a node.
    MinOutDegree,
    /// An upper bound on the number of out-edges of a node.
    MaxOutDegree,
    /// A value must be present.
    RequiredValue,
    /// The edges labelled by the named self-edge form no cycle.
    Acyclic,
    /// Checked outside panproto.
    External,
    /// A parameter of value coercion (`precision`, `scale`, `max_length`,
    /// `with_time_zone`, `int_bits`, `unsigned`); no check.
    ValueParameter,
}

/// The unit a length constraint counts.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LengthUnit {
    /// Unicode scalar values.
    Chars,
    /// UTF-8 bytes.
    Utf8Bytes,
    /// Extended grapheme clusters.
    Graphemes,
}
