//! Constraint-semantics rows for the configuration protocols
//! (`cloudformation`, `ansible`, `k8s-crd`).
//!
//! `required` is a presence flag of a parameter or property and `default` a
//! default value; both describe (see the presence reading of the
//! [`common`](super::common) module). A `cloudformation` parameter's
//! `AllowedValues` lists the permitted values, and a CRD property's `format`
//! is a value format of `OpenAPI` v3.

use panproto_gat::Name;
use panproto_schema::ConstraintSemantics;

use super::ConstraintRow;
use super::common::{annotation, enumeration, row};

/// The rows of this family, one per declaration.
pub(super) const ROWS: &[ConstraintRow] = &[
    row("cloudformation", "required", annotation),
    row("cloudformation", "default", annotation),
    row("cloudformation", "allowed-values", enumeration),
    row("ansible", "required", annotation),
    row("ansible", "default", annotation),
    row("k8s-crd", "required", annotation),
    row("k8s-crd", "format", || {
        ConstraintSemantics::Format(Name::from("k8s-crd"))
    }),
];
