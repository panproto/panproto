//! Format-independent value-I/O law functions.
//!
//! These definitions take parsers and writers as arguments. Thus the
//! reference states the route law once without importing a production
//! codec or silently implementing a second format engine.

use std::collections::BTreeSet;

/// Apply a route parser.
///
/// # Errors
///
/// The injected parser's error, unchanged.
pub fn parse<T, E>(bytes: &[u8], parser: impl FnOnce(&[u8]) -> Result<T, E>) -> Result<T, E> {
    parser(bytes)
}

/// Apply a route writer.
///
/// # Errors
///
/// The injected writer's error, unchanged.
pub fn emit<T, E>(value: &T, writer: impl FnOnce(&T) -> Result<Vec<u8>, E>) -> Result<Vec<u8>, E> {
    writer(value)
}

/// Canonical round trip: parsing emitted values is the identity.
#[must_use]
pub fn round_trip<T: Eq, E>(
    value: &T,
    writer: impl FnOnce(&T) -> Result<Vec<u8>, E>,
    parser: impl FnOnce(&[u8]) -> Result<T, E>,
) -> bool {
    writer(value)
        .and_then(|bytes| parser(&bytes))
        .is_ok_and(|parsed| parsed == *value)
}

/// Preserving round trip: an unedited document is byte-exact.
#[must_use]
pub fn preserve(source: &[u8], emitted: &[u8]) -> bool {
    source == emitted
}

/// Whether a route can represent a temporal value by parsing its display
/// back to the same value.
#[must_use]
pub fn temporal(kind: crate::temporal::TemporalKind, value: &crate::temporal::Temporal) -> bool {
    crate::temporal::display_parse(kind, value)
}

/// Agreement of the generic byte writer with the route-specific writer.
#[must_use]
pub fn bytes(generic: &[u8], route: &[u8]) -> bool {
    generic == route
}

/// Agreement of the exact-decimal writer with the route-specific writer.
#[must_use]
pub fn decimal(generic: &str, route: &str) -> bool {
    generic == route
}

/// Raw numeric fidelity for an unedited preserving cell.
#[must_use]
pub fn raw_numeric(source_lexeme: &str, emitted_lexeme: &str) -> bool {
    source_lexeme == emitted_lexeme
}

/// Whether object members contain a duplicate name and therefore require
/// the route's duplicate-member refusal.
#[must_use]
pub fn duplicate_member<T>(members: &[(String, T)]) -> bool {
    let mut names = BTreeSet::new();
    members.iter().any(|(name, _)| !names.insert(name))
}

/// Encoding and byte-order-mark preservation.
#[must_use]
pub fn encoding(source: &[u8], emitted: &[u8]) -> bool {
    preserve(source, emitted)
}

/// TOML scalar token fidelity.
#[must_use]
pub fn toml_scalar(source_token: &str, emitted_token: &str) -> bool {
    source_token == emitted_token
}

/// XML entity expansion delegates to the bounded XML reference.
pub use crate::xml::expand_entities as xml_entity;

/// XML source-encoding preservation.
pub use encoding as xml_encoding;

/// TSV has no implicit comment syntax.
#[must_use]
pub const fn tsv_comment(line: &str) -> bool {
    !line.is_empty()
}

/// `_` is an ordinary tabular text cell.
#[must_use]
pub fn tabular_missing(cell: &str) -> bool {
    cell == "_"
}

/// An empty line after a one-column header is one empty cell.
#[must_use]
pub const fn tabular_empty_row(column_count: usize, line: &str) -> bool {
    column_count == 1 && line.is_empty()
}

/// A TSV cell is writable exactly when it contains no delimiter or line
/// break; `"` is ordinary data.
#[must_use]
pub fn tsv_quote(cell: &str) -> bool {
    !cell
        .bytes()
        .any(|byte| matches!(byte, b'\t' | b'\r' | b'\n'))
}

/// XML expanded-name identity.
#[must_use]
pub fn xml_expanded_name(namespace: &str, local: &str) -> (String, String) {
    (namespace.to_owned(), local.to_owned())
}

/// Agreement of a generic blob writer with a route arm.
pub use bytes as blob_writer;

/// CST duplicate-member refusal uses the same member-name predicate.
pub use duplicate_member as duplicate_member_cst;
/// CST encoding and BOM preservation uses the same byte equality.
pub use encoding as encoding_bom_cst;
/// CST parsing uses the same injected parser.
pub use parse as parse_cst;
/// CST byte preservation uses the same byte equality.
pub use preserve as preserve_cst;
/// CST canonical round trip uses the same semantic predicate.
pub use round_trip as round_trip_cst;
