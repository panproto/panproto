//! Complete reference operations assigned to `value-io`.

pub mod tabular {
    //! Exact tabular route edge cases.

    /// A one-column empty record contains one empty cell.
    /// Definition: This is the manifest-stable name of the referenced operation.
    ///
    /// Book anchors: `L-value-io-29`.
    pub use crate::value_io::tabular_empty_row as one_column_empty_cst;
    /// `_` is data, never an implicit missing token.
    /// Definition: This is the manifest-stable name of the referenced operation.
    ///
    /// Book anchors: `L-value-io-28`.
    pub use crate::value_io::tabular_missing as missing_token_cst;
    /// TSV quotes are ordinary data.
    /// Definition: This is the manifest-stable name of the referenced operation.
    ///
    /// Book anchors: `L-value-io-30`.
    pub use crate::value_io::tsv_quote as tsv_quote_cst;
}

pub mod toml {
    //! Exact TOML scalar-route laws.

    /// Decimal and big-integer CST token fidelity.
    /// Definition: This is the manifest-stable name of the referenced operation.
    ///
    /// Book anchors: `L-value-io-25`.
    pub use crate::value_io::toml_scalar as decimal_bigint_cst;
}

pub mod implementation {
    //! Format-independent value-I/O law functions.
    //!
    //! These definitions take parsers and writers as arguments. Thus the
    //! reference states the route law once without importing a production
    //! codec or silently implementing a second format engine.

    use std::collections::BTreeSet;

    /// Definition: Apply a route parser.
    ///
    /// # Errors
    ///
    /// The injected parser's error, unchanged.
    ///
    /// Book anchors: `L-value-io-4`.
    pub fn parse<T, E>(bytes: &[u8], parser: impl FnOnce(&[u8]) -> Result<T, E>) -> Result<T, E> {
        parser(bytes)
    }

    /// Definition: Apply a route writer.
    ///
    /// # Errors
    ///
    /// The injected writer's error, unchanged.
    ///
    /// Book anchors: `L-value-io-2`.
    pub fn emit<T, E>(
        value: &T,
        writer: impl FnOnce(&T) -> Result<Vec<u8>, E>,
    ) -> Result<Vec<u8>, E> {
        writer(value)
    }

    /// Definition: Canonical round trip: parsing emitted values is the identity.
    ///
    /// Book anchors: `L-value-io-1`.
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

    /// Definition: Preserving round trip: an unedited document is byte-exact.
    ///
    /// Book anchors: `L-value-io-3`.
    #[must_use]
    pub fn preserve(source: &[u8], emitted: &[u8]) -> bool {
        source == emitted
    }

    /// Definition: Whether a route can represent a temporal value by parsing its display
    /// back to the same value.
    ///
    /// Book anchors: `L-value-io-5`.
    #[must_use]
    pub fn temporal(
        kind: crate::temporal::TemporalKind,
        value: &crate::temporal::Temporal,
    ) -> bool {
        crate::temporal::display_parse(kind, value)
    }

    /// Definition: Agreement of the generic byte writer with the route-specific writer.
    ///
    /// Book anchors: `L-value-io-6`.
    #[must_use]
    pub fn bytes(generic: &[u8], route: &[u8]) -> bool {
        generic == route
    }

    /// Definition: Agreement of the exact-decimal writer with the route-specific writer.
    ///
    /// Book anchors: `L-value-io-7`.
    #[must_use]
    pub fn decimal(generic: &str, route: &str) -> bool {
        generic == route
    }

    /// Definition: Raw numeric fidelity for an unedited preserving cell.
    ///
    /// Book anchors: `L-value-io-8`.
    #[must_use]
    pub fn raw_numeric(source_lexeme: &str, emitted_lexeme: &str) -> bool {
        source_lexeme == emitted_lexeme
    }

    /// Definition: Whether object members contain a duplicate name and therefore require
    /// the route's duplicate-member refusal.
    ///
    /// Book anchors: `L-value-io-9`.
    #[must_use]
    pub fn duplicate_member<T>(members: &[(String, T)]) -> bool {
        let mut names = BTreeSet::new();
        members.iter().any(|(name, _)| !names.insert(name))
    }

    /// Definition: Encoding and byte-order-mark preservation.
    ///
    /// Book anchors: `L-value-io-10`.
    #[must_use]
    pub fn encoding(source: &[u8], emitted: &[u8]) -> bool {
        preserve(source, emitted)
    }

    /// Definition: TOML scalar token fidelity.
    ///
    /// Book anchors: `L-value-io-11`.
    #[must_use]
    pub fn toml_scalar(source_token: &str, emitted_token: &str) -> bool {
        source_token == emitted_token
    }

    /// XML entity expansion delegates to the bounded XML reference.
    /// Definition: This is the manifest-stable name of the referenced operation.
    ///
    /// Book anchors: `L-value-io-12`.
    pub use crate::xml::expand_entities as xml_entity;

    /// XML source-encoding preservation.
    /// Definition: This is the manifest-stable name of the referenced operation.
    ///
    /// Book anchors: `L-value-io-13`.
    pub use encoding as xml_encoding;

    /// Definition: TSV has no implicit comment syntax.
    ///
    /// Book anchors: `L-value-io-14`.
    #[must_use]
    pub const fn tsv_comment(line: &str) -> bool {
        !line.is_empty()
    }

    /// Definition: `_` is an ordinary tabular text cell.
    ///
    /// Book anchors: `L-value-io-15`.
    #[must_use]
    pub fn tabular_missing(cell: &str) -> bool {
        cell == "_"
    }

    /// Definition: An empty line after a one-column header is one empty cell.
    ///
    /// Book anchors: `L-value-io-16`.
    #[must_use]
    pub const fn tabular_empty_row(column_count: usize, line: &str) -> bool {
        column_count == 1 && line.is_empty()
    }

    /// Definition: A TSV cell is writable exactly when it contains no delimiter or line
    /// break; `"` is ordinary data.
    ///
    /// Book anchors: `L-value-io-17`.
    #[must_use]
    pub fn tsv_quote(cell: &str) -> bool {
        !cell
            .bytes()
            .any(|byte| matches!(byte, b'\t' | b'\r' | b'\n'))
    }

    /// Definition: XML expanded-name identity.
    ///
    /// Book anchors: `L-value-io-18`.
    #[must_use]
    pub fn xml_expanded_name(namespace: &str, local: &str) -> (String, String) {
        (namespace.to_owned(), local.to_owned())
    }

    /// Agreement of a generic blob writer with a route arm.
    /// Definition: This is the manifest-stable name of the referenced operation.
    ///
    /// Book anchors: `L-value-io-19`.
    pub use bytes as blob_writer;

    /// CST duplicate-member refusal uses the same member-name predicate.
    /// Definition: This is the manifest-stable name of the referenced operation.
    ///
    /// Book anchors: `L-value-io-23`.
    pub use duplicate_member as duplicate_member_cst;
    /// CST encoding and BOM preservation uses the same byte equality.
    /// Definition: This is the manifest-stable name of the referenced operation.
    ///
    /// Book anchors: `L-value-io-24`.
    pub use encoding as encoding_bom_cst;
    /// CST parsing uses the same injected parser.
    /// Definition: This is the manifest-stable name of the referenced operation.
    ///
    /// Book anchors: `L-value-io-22`.
    pub use parse as parse_cst;
    /// CST byte preservation uses the same byte equality.
    /// Definition: This is the manifest-stable name of the referenced operation.
    ///
    /// Book anchors: `L-value-io-21`.
    pub use preserve as preserve_cst;
    /// CST canonical round trip uses the same semantic predicate.
    /// Definition: This is the manifest-stable name of the referenced operation.
    ///
    /// Book anchors: `L-value-io-20`.
    pub use round_trip as round_trip_cst;
}

pub mod xml {
    //! Small, bounded XML reference operations.

    use std::collections::BTreeMap;

    /// Maximum recursively expanded bytes.
    pub const MAX_XML_ENTITY_EXPANSION_BYTES: usize = 1_048_576;
    /// Maximum recursive entity nesting.
    pub const MAX_XML_ENTITY_DEPTH: usize = 16;

    /// Why bounded entity expansion is undefined.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum EntityError {
        /// An entity name has no declaration.
        Undeclared(String),
        /// The declaration graph is recursive.
        Recursive(String),
        /// Nesting exceeds [`MAX_XML_ENTITY_DEPTH`].
        Depth,
        /// Expanded bytes exceed [`MAX_XML_ENTITY_EXPANSION_BYTES`].
        Bytes,
        /// A reference is syntactically incomplete.
        Malformed,
    }

    fn append(output: &mut String, text: &str) -> Result<(), EntityError> {
        let new_len = output
            .len()
            .checked_add(text.len())
            .ok_or(EntityError::Bytes)?;
        if new_len > MAX_XML_ENTITY_EXPANSION_BYTES {
            return Err(EntityError::Bytes);
        }
        output.push_str(text);
        Ok(())
    }

    fn expand(
        text: &str,
        declarations: &BTreeMap<String, String>,
        stack: &mut Vec<String>,
        output: &mut String,
    ) -> Result<(), EntityError> {
        if stack.len() > MAX_XML_ENTITY_DEPTH {
            return Err(EntityError::Depth);
        }
        let mut rest = text;
        while let Some(start) = rest.find('&') {
            append(output, &rest[..start])?;
            let after = &rest[start + 1..];
            let Some(end) = after.find(';') else {
                return Err(EntityError::Malformed);
            };
            let name = &after[..end];
            if let Some(replacement) = match name {
                "amp" => Some("&"),
                "lt" => Some("<"),
                "gt" => Some(">"),
                "apos" => Some("'"),
                "quot" => Some("\""),
                _ => None,
            } {
                append(output, replacement)?;
                rest = &after[end + 1..];
                continue;
            }
            let replacement = declarations
                .get(name)
                .ok_or_else(|| EntityError::Undeclared(name.to_owned()))?;
            if stack.iter().any(|item| item == name) {
                return Err(EntityError::Recursive(name.to_owned()));
            }
            stack.push(name.to_owned());
            expand(replacement, declarations, stack, output)?;
            stack.pop();
            rest = &after[end + 1..];
        }
        append(output, rest)
    }

    /// Expand internal general entities with explicit byte and depth bounds.
    ///
    /// # Errors
    ///
    /// [`EntityError`] for an undeclared, recursive, malformed or over-budget
    /// expansion.
    pub fn expand_entities(
        text: &str,
        declarations: &BTreeMap<String, String>,
    ) -> Result<String, EntityError> {
        let mut output = String::new();
        expand(text, declarations, &mut Vec::new(), &mut output)?;
        Ok(output)
    }

    /// The CST preserving byte round trip.
    /// Definition: This is the manifest-stable name of the referenced operation.
    ///
    /// Book anchors: `L-value-io-27`.
    pub use crate::value_io::preserve as round_trip_cst;
    /// The CST expanded-name function.
    /// Definition: This is the manifest-stable name of the referenced operation.
    ///
    /// Book anchors: `L-value-io-31`.
    pub use crate::value_io::xml_expanded_name as expanded_name_cst;
    /// The CST bounded-expansion operation.
    /// Definition: This is the manifest-stable name of the referenced operation.
    ///
    /// Book anchors: `L-value-io-26`.
    pub use expand_entities as bounded_expansion_cst;
}

pub use self::implementation::*;
pub use self::tabular::{missing_token_cst, one_column_empty_cst, tsv_quote_cst};
pub use self::toml::decimal_bigint_cst;
pub use self::xml::{bounded_expansion_cst, expanded_name_cst};
