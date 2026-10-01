//! Validation of tree instances against schemas.
//!
//! [`validate_wtype`] returns a [`ValidationReport`](crate::ValidationReport)
//! with a three-valued status. A tree's structural invariants (anchors
//! declared, arcs declared with coherent endpoints, the root present, every
//! node reachable from it, required edges present, the parent map agreeing
//! with the arcs, fans well formed) are the `NodeAnchor`, `EdgeDeclared`,
//! `EndpointCoherence`, `TreeRoot`, `TreeReachable`, `RequiredEdge`,
//! `TreeParentMap` and `FanWellFormed` checks of
//! [`validation`](crate::validation).
//!
//! [`validate_attributes`] is the attribute-only projection of a tree's
//! validation.

use panproto_schema::Schema;

use crate::attributes::{AttributeSchema, AttributeViolation};
use crate::wtype::WInstance;

pub use crate::validation::validate_wtype;

/// The attribute violations of a tree instance against a schema.
///
/// It reports each node whose `extra_fields` carry an attribute key its
/// vertex does not declare, or a value whose kind does not match the
/// declaration. Nodes over vertices with no declared attributes are
/// unconstrained.
///
/// Not a validation boundary: it returns the attribute violations alone,
/// with no status. It derives the schema's [`AttributeSchema`] and calls
/// [`AttributeSchema::validate_wtype`], the projection the `Attribute` check
/// of [`validate_wtype`] reports as diagnostics.
#[must_use]
pub fn validate_attributes(schema: &Schema, instance: &WInstance) -> Vec<AttributeViolation> {
    AttributeSchema::from_schema(schema).validate_wtype(instance)
}
