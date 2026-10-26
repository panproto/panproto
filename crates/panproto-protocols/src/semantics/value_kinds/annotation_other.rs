//! Value-kind rows for the annotation protocols with non-XML codecs
//! (`conllu`).
//!
//! A CoNLL-U sentence, word, multiword token and empty node are records;
//! the tag vertices hanging from a word (its part-of-speech tags and the
//! relation of a root dependency) hold a token of a closed vocabulary, and
//! its lemma and features hold text.

use panproto_schema::ScalarKind;

use super::{NonScalarRow, ValueKindRow};

/// The rows of this family, one per declaration.
pub(super) const ROWS: &[ValueKindRow] = &[
    ValueKindRow {
        protocol: "conllu",
        vertex_kind: "upos-tag",
        kind: ScalarKind::Token,
    },
    ValueKindRow {
        protocol: "conllu",
        vertex_kind: "xpos-tag",
        kind: ScalarKind::Token,
    },
    ValueKindRow {
        protocol: "conllu",
        vertex_kind: "deprel",
        kind: ScalarKind::Token,
    },
    ValueKindRow {
        protocol: "conllu",
        vertex_kind: "lemma",
        kind: ScalarKind::Str,
    },
    ValueKindRow {
        protocol: "conllu",
        vertex_kind: "feature",
        kind: ScalarKind::Str,
    },
];

/// The vertex kinds of this family that hold no scalar.
pub(super) const NON_SCALAR: &[NonScalarRow] = &[
    NonScalarRow {
        protocol: "conllu",
        vertex_kind: "sentence",
    },
    NonScalarRow {
        protocol: "conllu",
        vertex_kind: "word",
    },
    NonScalarRow {
        protocol: "conllu",
        vertex_kind: "multiword",
    },
    NonScalarRow {
        protocol: "conllu",
        vertex_kind: "empty",
    },
];
