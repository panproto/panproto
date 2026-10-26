//! Constraint-semantics rows for the annotation protocols outside the XML
//! family (`conllu`, `amr`, `ucca`, `nif`, `web-annotation`, `brat`,
//! `concrete`, `decomp`, `fovea`, `bead`).
//!
//! The constraint sorts of these protocols record the features, attributes,
//! offsets, labels and provenance a schema document declares for an
//! annotation type, as the document writes them; none bounds the values an
//! instance may hold, so every sort describes. A `required` flag of a bead
//! template slot is about the template that holds the slot, which must have
//! the slot (the schema's required edge of the template), not about the
//! slot's own value.

use panproto_schema::ConstraintSemantics;

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
    row("conllu", "form", || ConstraintSemantics::Annotation),
    row("conllu", "id-range", || ConstraintSemantics::Annotation),
    row("conllu", "head", || ConstraintSemantics::Annotation),
    row("conllu", "misc", || ConstraintSemantics::Annotation),
    row("conllu", "sent-id", || ConstraintSemantics::Annotation),
    row("conllu", "text", || ConstraintSemantics::Annotation),
    row("conllu", "newpar", || ConstraintSemantics::Annotation),
    row("conllu", "newdoc", || ConstraintSemantics::Annotation),
    row("conllu", "value", || ConstraintSemantics::Annotation),
    row("amr", "value", || ConstraintSemantics::Annotation),
    row("amr", "alignment", || ConstraintSemantics::Annotation),
    row("amr", "wiki", || ConstraintSemantics::Annotation),
    row("amr", "polarity", || ConstraintSemantics::Annotation),
    row("amr", "mode", || ConstraintSemantics::Annotation),
    row("ucca", "id", || ConstraintSemantics::Annotation),
    row("ucca", "tag", || ConstraintSemantics::Annotation),
    row("ucca", "type", || ConstraintSemantics::Annotation),
    row("ucca", "paragraph", || ConstraintSemantics::Annotation),
    row("ucca", "is-remote", || ConstraintSemantics::Annotation),
    row("ucca", "is-implicit", || ConstraintSemantics::Annotation),
    row("ucca", "position", || ConstraintSemantics::Annotation),
    row("ucca", "text", || ConstraintSemantics::Annotation),
    row("nif", "begin-index", || ConstraintSemantics::Annotation),
    row("nif", "end-index", || ConstraintSemantics::Annotation),
    row("nif", "anchor-of", || ConstraintSemantics::Annotation),
    row("nif", "is-string", || ConstraintSemantics::Annotation),
    row("nif", "pred-lang", || ConstraintSemantics::Annotation),
    row("nif", "pos-tag", || ConstraintSemantics::Annotation),
    row("nif", "lemma", || ConstraintSemantics::Annotation),
    row("nif", "stem", || ConstraintSemantics::Annotation),
    row("nif", "sentiment-value", || ConstraintSemantics::Annotation),
    row("nif", "confidence", || ConstraintSemantics::Annotation),
    row("nif", "ta-ident-ref", || ConstraintSemantics::Annotation),
    row("nif", "ta-class-ref", || ConstraintSemantics::Annotation),
    row("web-annotation", "type", || ConstraintSemantics::Annotation),
    row("web-annotation", "format", || {
        ConstraintSemantics::Annotation
    }),
    row("web-annotation", "language", || {
        ConstraintSemantics::Annotation
    }),
    row("web-annotation", "processing-language", || {
        ConstraintSemantics::Annotation
    }),
    row("web-annotation", "text-direction", || {
        ConstraintSemantics::Annotation
    }),
    row("web-annotation", "value", || {
        ConstraintSemantics::Annotation
    }),
    row("web-annotation", "body-value", || {
        ConstraintSemantics::Annotation
    }),
    row("web-annotation", "exact", || {
        ConstraintSemantics::Annotation
    }),
    row("web-annotation", "prefix", || {
        ConstraintSemantics::Annotation
    }),
    row("web-annotation", "suffix", || {
        ConstraintSemantics::Annotation
    }),
    row("web-annotation", "start", || {
        ConstraintSemantics::Annotation
    }),
    row("web-annotation", "end", || ConstraintSemantics::Annotation),
    row("web-annotation", "conforms-to", || {
        ConstraintSemantics::Annotation
    }),
    row("web-annotation", "motivation", || {
        ConstraintSemantics::Annotation
    }),
    row("web-annotation", "purpose", || {
        ConstraintSemantics::Annotation
    }),
    row("web-annotation", "created", || {
        ConstraintSemantics::Annotation
    }),
    row("web-annotation", "modified", || {
        ConstraintSemantics::Annotation
    }),
    row("web-annotation", "generated", || {
        ConstraintSemantics::Annotation
    }),
    row("web-annotation", "rights", || {
        ConstraintSemantics::Annotation
    }),
    row("web-annotation", "canonical", || {
        ConstraintSemantics::Annotation
    }),
    row("web-annotation", "via", || ConstraintSemantics::Annotation),
    row("web-annotation", "accessibility", || {
        ConstraintSemantics::Annotation
    }),
    row("brat", "offset-start", || ConstraintSemantics::Annotation),
    row("brat", "offset-end", || ConstraintSemantics::Annotation),
    row("brat", "value", || ConstraintSemantics::Annotation),
    row("brat", "type", || ConstraintSemantics::Annotation),
    row("brat", "source-db", || ConstraintSemantics::Annotation),
    row("brat", "source-id", || ConstraintSemantics::Annotation),
    row("brat", "text", || ConstraintSemantics::Annotation),
    row("concrete", "uuid", || ConstraintSemantics::Annotation),
    row("concrete", "kind", || ConstraintSemantics::Annotation),
    row("concrete", "text", || ConstraintSemantics::Annotation),
    row("concrete", "tag", || ConstraintSemantics::Annotation),
    row("concrete", "confidence", || ConstraintSemantics::Annotation),
    row("concrete", "tool", || ConstraintSemantics::Annotation),
    row("concrete", "kbest-index", || {
        ConstraintSemantics::Annotation
    }),
    row("concrete", "timestamp", || ConstraintSemantics::Annotation),
    row("concrete", "role", || ConstraintSemantics::Annotation),
    row("concrete", "subkind", || ConstraintSemantics::Annotation),
    row("decomp", "domain", || ConstraintSemantics::Annotation),
    row("decomp", "type", || ConstraintSemantics::Annotation),
    row("decomp", "position", || ConstraintSemantics::Annotation),
    row("decomp", "form", || ConstraintSemantics::Annotation),
    row("decomp", "lemma", || ConstraintSemantics::Annotation),
    row("decomp", "upos", || ConstraintSemantics::Annotation),
    row("decomp", "xpos", || ConstraintSemantics::Annotation),
    row("decomp", "deprel", || ConstraintSemantics::Annotation),
    row("decomp", "frompredpatt", || ConstraintSemantics::Annotation),
    row("decomp", "value", || ConstraintSemantics::Annotation),
    row("decomp", "confidence", || ConstraintSemantics::Annotation),
    row("decomp", "subspace", || ConstraintSemantics::Annotation),
    row("decomp", "property", || ConstraintSemantics::Annotation),
    row("fovea", "name", || ConstraintSemantics::Annotation),
    row("fovea", "role", || ConstraintSemantics::Annotation),
    row("fovea", "information-need", || {
        ConstraintSemantics::Annotation
    }),
    row("fovea", "confidence", || ConstraintSemantics::Annotation),
    row("fovea", "justification", || ConstraintSemantics::Annotation),
    row("fovea", "symmetric", || ConstraintSemantics::Annotation),
    row("fovea", "transitive", || ConstraintSemantics::Annotation),
    row("fovea", "collection-type", || {
        ConstraintSemantics::Annotation
    }),
    row("fovea", "location-type", || ConstraintSemantics::Annotation),
    row("fovea", "time-type", || ConstraintSemantics::Annotation),
    row("fovea", "frame-number", || ConstraintSemantics::Annotation),
    row("fovea", "x", || ConstraintSemantics::Annotation),
    row("fovea", "y", || ConstraintSemantics::Annotation),
    row("fovea", "width", || ConstraintSemantics::Annotation),
    row("fovea", "height", || ConstraintSemantics::Annotation),
    row("fovea", "label", || ConstraintSemantics::Annotation),
    row("fovea", "source", || ConstraintSemantics::Annotation),
    row("bead", "name", || ConstraintSemantics::Annotation),
    row("bead", "description", || ConstraintSemantics::Annotation),
    row("bead", "language-code", || ConstraintSemantics::Annotation),
    row("bead", "source", || ConstraintSemantics::Annotation),
    row("bead", "expression", || ConstraintSemantics::Annotation),
    row("bead", "required", || ConstraintSemantics::Annotation),
    row("bead", "rendered-text", || ConstraintSemantics::Annotation),
    row("bead", "strategy", || ConstraintSemantics::Annotation),
    row("bead", "judgment-type", || ConstraintSemantics::Annotation),
    row("bead", "task-type", || ConstraintSemantics::Annotation),
    row("bead", "label", || ConstraintSemantics::Annotation),
    row("bead", "confidence", || ConstraintSemantics::Annotation),
    row("bead", "span-type", || ConstraintSemantics::Annotation),
    row("bead", "directed", || ConstraintSemantics::Annotation),
    row("bead", "list-number", || ConstraintSemantics::Annotation),
    row("bead", "response-value", || ConstraintSemantics::Annotation),
    row("bead", "response-time", || ConstraintSemantics::Annotation),
];
