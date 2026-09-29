//! The meaning of each protocol's constraint sorts.
//!
//! One [`ConstraintRow`] per constraint sort of a protocol, kept in one file
//! per protocol family under `constraint_semantics/` and listed in
//! `FAMILIES` below. [`attach`](super::attach) collects a protocol's rows
//! into its
//! [`constraint_semantics`](panproto_schema::Protocol::constraint_semantics).

use std::collections::BTreeMap;

use panproto_schema::ConstraintSemantics;

mod code;
mod rdf;
mod sql;
mod text;

/// The meaning of one constraint sort of one protocol.
///
/// The meaning is built by a function because
/// [`ConstraintSemantics::Format`] carries a name, which a constant cannot
/// hold.
#[derive(Clone, Copy, Debug)]
pub struct ConstraintRow {
    /// The protocol's canonical name.
    pub protocol: &'static str,
    /// The constraint sort.
    pub sort: &'static str,
    /// Builds the sort's meaning.
    pub semantics: fn() -> ConstraintSemantics,
}

/// The row tables of every family.
const FAMILIES: &[&[ConstraintRow]] = &[code::ROWS, rdf::ROWS, sql::ROWS, text::ROWS];

/// Every constraint-semantics row, family by family.
pub fn rows() -> impl Iterator<Item = &'static ConstraintRow> {
    FAMILIES.iter().flat_map(|family| family.iter())
}

/// The meaning of each constraint sort the protocol whose canonical name is
/// `protocol` declares, keyed by sort.
#[must_use]
pub fn declared(protocol: &str) -> BTreeMap<String, ConstraintSemantics> {
    rows()
        .filter(|row| row.protocol == protocol)
        .map(|row| (row.sort.to_owned(), (row.semantics)()))
        .collect()
}
