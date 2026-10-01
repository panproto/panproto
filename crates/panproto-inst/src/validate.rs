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
//! validation, defined in [`attributes`](crate::attributes).

pub use crate::attributes::validate_attributes;
pub use crate::validation::validate_wtype;
