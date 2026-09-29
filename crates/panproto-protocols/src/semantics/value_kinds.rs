//! The scalar kind of each protocol's scalar vertex kinds.
//!
//! One [`ValueKindRow`] per scalar vertex kind of a protocol, kept in one
//! file per protocol family under `value_kinds/` and listed in `FAMILIES`
//! below. [`attach`](super::attach) collects a protocol's rows into its
//! [`value_kinds`](panproto_schema::Protocol::value_kinds).

use std::collections::BTreeMap;

use panproto_schema::ScalarKind;

mod code;
mod rdf;
mod sql;
mod text;

/// The scalar kind of one vertex kind of one protocol.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValueKindRow {
    /// The protocol's canonical name.
    pub protocol: &'static str,
    /// The vertex kind.
    pub vertex_kind: &'static str,
    /// The scalar kind its vertices hold.
    pub kind: ScalarKind,
}

/// The row tables of every family.
const FAMILIES: &[&[ValueKindRow]] = &[code::ROWS, rdf::ROWS, sql::ROWS, text::ROWS];

/// Every value-kind row, family by family.
pub fn rows() -> impl Iterator<Item = &'static ValueKindRow> {
    FAMILIES.iter().flat_map(|family| family.iter())
}

/// The scalar kind of each vertex kind the protocol whose canonical name is
/// `protocol` classifies, keyed by vertex kind.
#[must_use]
pub fn declared(protocol: &str) -> BTreeMap<String, ScalarKind> {
    rows()
        .filter(|row| row.protocol == protocol)
        .map(|row| (row.vertex_kind.to_owned(), row.kind.clone()))
        .collect()
}
