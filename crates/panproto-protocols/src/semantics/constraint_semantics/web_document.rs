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

use panproto_schema::{ConstraintSemantics, LengthUnit};

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
    row("atproto", "minLength", || {
        ConstraintSemantics::MinLength(LengthUnit::Utf8Bytes)
    }),
    row("atproto", "maxLength", || {
        ConstraintSemantics::MaxLength(LengthUnit::Utf8Bytes)
    }),
    row("atproto", "minimum", || ConstraintSemantics::Minimum),
    row("atproto", "maximum", || ConstraintSemantics::Maximum),
    row("atproto", "maxGraphemes", || {
        ConstraintSemantics::MaxLength(LengthUnit::Graphemes)
    }),
    row("atproto", "enum", || ConstraintSemantics::Enum),
    row("atproto", "const", || ConstraintSemantics::Const),
    row("atproto", "default", || ConstraintSemantics::Annotation),
    row("atproto", "closed", || ConstraintSemantics::Annotation),
    row("atproto", "format", || {
        ConstraintSemantics::Format(Name::from("atproto"))
    }),
    row("atproto", "knownValues", || ConstraintSemantics::Annotation),
    row("atproto", "ref", || ConstraintSemantics::Annotation),
    row("docx", "required", || ConstraintSemantics::Annotation),
    row("docx", "style-type", || ConstraintSemantics::Annotation),
    row("docx", "numbering-format", || {
        ConstraintSemantics::Annotation
    }),
    row("odf", "style-family", || ConstraintSemantics::Annotation),
    row("odf", "master-page-name", || {
        ConstraintSemantics::Annotation
    }),
];
