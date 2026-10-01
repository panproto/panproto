//! Constraint-semantics rows for the web document protocols (`atproto`,
//! `docx`, `odf`).
//!
//! AT Protocol lexicons count a string's `minLength` and `maxLength` in
//! UTF-8 bytes and its `maxGraphemes` in extended grapheme clusters, compare
//! `minimum` and `maximum` inclusively, and name string formats (`datetime`,
//! `did`, `uri`, ...) with `format`. `knownValues` lists recognized values of
//! an open set, so any value is valid; `closed`, `default` and the `ref`
//! target a property was declared with describe. The `docx` and `odf`
//! sorts name styles and numbering formats, which describe.

use panproto_gat::Name;
use panproto_schema::ConstraintSemantics;

use super::ConstraintRow;
use super::common::{
    annotation, constant, enumeration, max_graphemes, max_utf8_bytes, maximum, min_utf8_bytes,
    minimum, row,
};

/// The rows of this family, one per declaration.
pub(super) const ROWS: &[ConstraintRow] = &[
    row("atproto", "minLength", min_utf8_bytes),
    row("atproto", "maxLength", max_utf8_bytes),
    row("atproto", "minimum", minimum),
    row("atproto", "maximum", maximum),
    row("atproto", "maxGraphemes", max_graphemes),
    row("atproto", "enum", enumeration),
    row("atproto", "const", constant),
    row("atproto", "default", annotation),
    row("atproto", "closed", annotation),
    row("atproto", "format", || {
        ConstraintSemantics::Format(Name::from("atproto"))
    }),
    row("atproto", "knownValues", annotation),
    row("atproto", "ref", annotation),
    row("docx", "required", annotation),
    row("docx", "style-type", annotation),
    row("docx", "numbering-format", annotation),
    row("odf", "style-family", annotation),
    row("odf", "master-page-name", annotation),
];
