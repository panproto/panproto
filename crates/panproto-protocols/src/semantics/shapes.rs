//! The syntactic and semantic shape each protocol declares.
//!
//! One [`ShapeRow`] per protocol, kept in one file per protocol family under
//! `shapes/` and listed in `FAMILIES` below. [`attach`](super::attach)
//! copies a protocol's row into its
//! [`syntactic_shape`](panproto_schema::Protocol::syntactic_shape) and
//! [`semantic_shape`](panproto_schema::Protocol::semantic_shape); a
//! protocol with no row keeps the defaults.

use panproto_schema::InstanceShapeKind;

mod code;
mod rdf;
mod sql;
mod text;

/// The shapes one protocol declares.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShapeRow {
    /// The protocol's canonical name.
    pub protocol: &'static str,
    /// The shape its instance codec produces.
    pub syntactic: InstanceShapeKind,
    /// The shape its instances have once references are resolved.
    pub semantic: InstanceShapeKind,
}

/// The row tables of every family.
const FAMILIES: &[&[ShapeRow]] = &[code::ROWS, rdf::ROWS, sql::ROWS, text::ROWS];

/// Every shape row, family by family.
pub fn rows() -> impl Iterator<Item = &'static ShapeRow> {
    FAMILIES.iter().flat_map(|family| family.iter())
}

/// The shape row of the protocol whose canonical name is `protocol`, if it
/// declares one.
#[must_use]
pub fn declared(protocol: &str) -> Option<&'static ShapeRow> {
    rows().find(|row| row.protocol == protocol)
}
