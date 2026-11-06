//! Constraint-semantics rows for the configuration protocols
//! (`cloudformation`, `ansible`, `k8s-crd`).
//!
//! `required` is a presence flag of a parameter or property and `default` a
//! default value; both describe. The flag is about the element that holds
//! the parameter or property, which must have it (the schema's required
//! edge), not about its own value. A `cloudformation` parameter's
//! `AllowedValues` lists the permitted values, and a CRD property's `format`
//! is a value format of `OpenAPI` v3.

use panproto_gat::Name;
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
    row("cloudformation", "required", || {
        ConstraintSemantics::Annotation
    }),
    row("cloudformation", "default", || {
        ConstraintSemantics::Annotation
    }),
    row("cloudformation", "allowed-values", || {
        ConstraintSemantics::Enum
    }),
    row("ansible", "required", || ConstraintSemantics::Annotation),
    row("ansible", "default", || ConstraintSemantics::Annotation),
    row("k8s-crd", "required", || ConstraintSemantics::Annotation),
    row("k8s-crd", "format", || {
        ConstraintSemantics::Format(Name::from("k8s-crd"))
    }),
];
