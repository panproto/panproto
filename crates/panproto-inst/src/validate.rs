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
//! [`validate_attributes`], defined in [`attributes`](crate::attributes),
//! is the attribute-only projection of a tree's validation under
//! [`Protocol::default`](panproto_schema::Protocol::default), which has no
//! value kinds and so declares the schema's leaf edges alone. Under a
//! protocol that classifies a structured column, read the `Attribute`
//! diagnostics of [`validate_wtype`] instead.

pub use crate::attributes::validate_attributes;
pub use crate::validation::validate_wtype;
