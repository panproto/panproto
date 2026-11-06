//! Constraint-semantics rows for the XML annotation protocols (`laf-graf`,
//! `paula`, `elan`, `naf`, `tei`, `timeml`, `iso-space`, `uima`, `folia`).
//!
//! The constraint sorts of these protocols record the attributes, feature
//! declarations, offsets, time points, identifiers and provenance a schema
//! document declares for an annotation layer, as the document writes them;
//! none bounds the values an instance may hold, so every sort describes.

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
    row("laf-graf", "id", || ConstraintSemantics::Annotation),
    row("laf-graf", "type", || ConstraintSemantics::Annotation),
    row("laf-graf", "value", || ConstraintSemantics::Annotation),
    row("laf-graf", "from", || ConstraintSemantics::Annotation),
    row("laf-graf", "to", || ConstraintSemantics::Annotation),
    row("laf-graf", "label", || ConstraintSemantics::Annotation),
    row("paula", "name", || ConstraintSemantics::Annotation),
    row("paula", "value", || ConstraintSemantics::Annotation),
    row("paula", "namespace", || ConstraintSemantics::Annotation),
    row("paula", "type", || ConstraintSemantics::Annotation),
    row("paula", "layer", || ConstraintSemantics::Annotation),
    row("paula", "document-id", || ConstraintSemantics::Annotation),
    row("paula", "corpus-id", || ConstraintSemantics::Annotation),
    row("paula", "start", || ConstraintSemantics::Annotation),
    row("paula", "end", || ConstraintSemantics::Annotation),
    row("paula", "begin-time", || ConstraintSemantics::Annotation),
    row("paula", "end-time", || ConstraintSemantics::Annotation),
    row("elan", "time-value", || ConstraintSemantics::Annotation),
    row("elan", "annotation-value", || {
        ConstraintSemantics::Annotation
    }),
    row("elan", "tier-id", || ConstraintSemantics::Annotation),
    row("elan", "participant", || ConstraintSemantics::Annotation),
    row("elan", "annotator", || ConstraintSemantics::Annotation),
    row("elan", "linguistic-type-id", || {
        ConstraintSemantics::Annotation
    }),
    row("elan", "stereotype", || ConstraintSemantics::Annotation),
    row("elan", "xmin", || ConstraintSemantics::Annotation),
    row("elan", "xmax", || ConstraintSemantics::Annotation),
    row("elan", "text", || ConstraintSemantics::Annotation),
    row("elan", "mark", || ConstraintSemantics::Annotation),
    row("naf", "id", || ConstraintSemantics::Annotation),
    row("naf", "type", || ConstraintSemantics::Annotation),
    row("naf", "lemma", || ConstraintSemantics::Annotation),
    row("naf", "pos", || ConstraintSemantics::Annotation),
    row("naf", "morphofeat", || ConstraintSemantics::Annotation),
    row("naf", "case", || ConstraintSemantics::Annotation),
    row("naf", "head", || ConstraintSemantics::Annotation),
    row("naf", "sentiment", || ConstraintSemantics::Annotation),
    row("naf", "resource", || ConstraintSemantics::Annotation),
    row("naf", "reference", || ConstraintSemantics::Annotation),
    row("naf", "confidence", || ConstraintSemantics::Annotation),
    row("naf", "offset", || ConstraintSemantics::Annotation),
    row("naf", "length", || ConstraintSemantics::Annotation),
    row("naf", "sent", || ConstraintSemantics::Annotation),
    row("naf", "polarity", || ConstraintSemantics::Annotation),
    row("naf", "value", || ConstraintSemantics::Annotation),
    row("naf", "uri", || ConstraintSemantics::Annotation),
    row("naf", "prediction", || ConstraintSemantics::Annotation),
    row("tei", "xml-id", || ConstraintSemantics::Annotation),
    row("tei", "type", || ConstraintSemantics::Annotation),
    row("tei", "subtype", || ConstraintSemantics::Annotation),
    row("tei", "lang", || ConstraintSemantics::Annotation),
    row("tei", "n", || ConstraintSemantics::Annotation),
    row("tei", "value", || ConstraintSemantics::Annotation),
    row("tei", "rend", || ConstraintSemantics::Annotation),
    row("tei", "resp", || ConstraintSemantics::Annotation),
    row("tei", "when", || ConstraintSemantics::Annotation),
    row("tei", "who", || ConstraintSemantics::Annotation),
    row("tei", "scheme", || ConstraintSemantics::Annotation),
    row("tei", "target", || ConstraintSemantics::Annotation),
    row("tei", "corresp", || ConstraintSemantics::Annotation),
    row("tei", "wit", || ConstraintSemantics::Annotation),
    row("tei", "cause", || ConstraintSemantics::Annotation),
    row("timeml", "eid", || ConstraintSemantics::Annotation),
    row("timeml", "eiid", || ConstraintSemantics::Annotation),
    row("timeml", "class", || ConstraintSemantics::Annotation),
    row("timeml", "tense", || ConstraintSemantics::Annotation),
    row("timeml", "aspect", || ConstraintSemantics::Annotation),
    row("timeml", "polarity", || ConstraintSemantics::Annotation),
    row("timeml", "modality", || ConstraintSemantics::Annotation),
    row("timeml", "pos", || ConstraintSemantics::Annotation),
    row("timeml", "type", || ConstraintSemantics::Annotation),
    row("timeml", "value", || ConstraintSemantics::Annotation),
    row("timeml", "mod", || ConstraintSemantics::Annotation),
    row("timeml", "temporal-function", || {
        ConstraintSemantics::Annotation
    }),
    row("timeml", "function-in-document", || {
        ConstraintSemantics::Annotation
    }),
    row("timeml", "begin-point", || ConstraintSemantics::Annotation),
    row("timeml", "end-point", || ConstraintSemantics::Annotation),
    row("timeml", "quant", || ConstraintSemantics::Annotation),
    row("timeml", "freq", || ConstraintSemantics::Annotation),
    row("timeml", "rel-type", || ConstraintSemantics::Annotation),
    row("timeml", "signal-id", || ConstraintSemantics::Annotation),
    row("timeml", "subordinate-signal", || {
        ConstraintSemantics::Annotation
    }),
    row("iso-space", "type", || ConstraintSemantics::Annotation),
    row("iso-space", "form", || ConstraintSemantics::Annotation),
    row("iso-space", "mod", || ConstraintSemantics::Annotation),
    row("iso-space", "countable", || ConstraintSemantics::Annotation),
    row("iso-space", "dimensionality", || {
        ConstraintSemantics::Annotation
    }),
    row("iso-space", "gquant", || ConstraintSemantics::Annotation),
    row("iso-space", "scopes", || ConstraintSemantics::Annotation),
    row("iso-space", "dcl", || ConstraintSemantics::Annotation),
    row("iso-space", "domain", || ConstraintSemantics::Annotation),
    row("iso-space", "lat-long", || ConstraintSemantics::Annotation),
    row("iso-space", "elevation", || ConstraintSemantics::Annotation),
    row("iso-space", "value", || ConstraintSemantics::Annotation),
    row("iso-space", "direction", || ConstraintSemantics::Annotation),
    row("iso-space", "distance", || ConstraintSemantics::Annotation),
    row("iso-space", "gazref", || ConstraintSemantics::Annotation),
    row("uima", "name", || ConstraintSemantics::Annotation),
    row("uima", "range-type", || ConstraintSemantics::Annotation),
    row("uima", "element-type", || ConstraintSemantics::Annotation),
    row("uima", "multi-ref", || ConstraintSemantics::Annotation),
    row("uima", "sofa-num", || ConstraintSemantics::Annotation),
    row("uima", "mime-type", || ConstraintSemantics::Annotation),
    row("uima", "begin", || ConstraintSemantics::Annotation),
    row("uima", "end", || ConstraintSemantics::Annotation),
    row("folia", "xml-id", || ConstraintSemantics::Annotation),
    row("folia", "class", || ConstraintSemantics::Annotation),
    row("folia", "set", || ConstraintSemantics::Annotation),
    row("folia", "annotator", || ConstraintSemantics::Annotation),
    row("folia", "annotator-type", || {
        ConstraintSemantics::Annotation
    }),
    row("folia", "confidence", || ConstraintSemantics::Annotation),
    row("folia", "datetime", || ConstraintSemantics::Annotation),
    row("folia", "n", || ConstraintSemantics::Annotation),
    row("folia", "processor", || ConstraintSemantics::Annotation),
    row("folia", "deprel", || ConstraintSemantics::Annotation),
];
