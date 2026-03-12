//! Symmetric lenses via span composition.
//!
//! A symmetric lens between schemas S and T is a pair of asymmetric lenses
//! that share a common complement. This module provides the span-based
//! construction where the "middle" schema M serves as the shared state.

use panproto_inst::WInstance;
use panproto_schema::Schema;

use crate::Lens;
use crate::asymmetric::{Complement, get, put};
use crate::error::LensError;

/// A symmetric lens between two schemas, built from a shared middle schema.
///
/// The left leg is a lens from M to S, and the right leg is a lens from M
/// to T. Together they synchronize S and T via the common state M.
pub struct SymmetricLens {
    /// Lens from the middle schema to the left schema.
    pub left: Lens,
    /// Lens from the middle schema to the right schema.
    pub right: Lens,
    /// The shared middle schema.
    pub middle: Schema,
}

impl SymmetricLens {
    /// Create a symmetric lens from two asymmetric lenses that share the
    /// same source schema (the "middle").
    ///
    /// # Errors
    ///
    /// Returns `LensError::CompositionMismatch` if the source schemas of
    /// the two lenses do not match.
    pub fn from_span(left: Lens, right: Lens) -> Result<Self, LensError> {
        // Verify that both lenses have the same source schema (middle)
        if left.src_schema.protocol != right.src_schema.protocol
            || left.src_schema.vertex_count() != right.src_schema.vertex_count()
        {
            return Err(LensError::CompositionMismatch);
        }
        // Check that vertex IDs match exactly
        if left
            .src_schema
            .vertices
            .keys()
            .collect::<std::collections::BTreeSet<_>>()
            != right
                .src_schema
                .vertices
                .keys()
                .collect::<std::collections::BTreeSet<_>>()
        {
            return Err(LensError::CompositionMismatch);
        }
        let middle = left.src_schema.clone();
        Ok(Self {
            left,
            right,
            middle,
        })
    }

    /// Synchronize from left to right: given a left view, produce a right view.
    ///
    /// Puts the left view back into the middle, then gets the right view.
    ///
    /// # Errors
    ///
    /// Returns `LensError` if either the put or get operation fails.
    pub fn sync_left_to_right(
        &self,
        left_view: &WInstance,
        left_complement: &Complement,
    ) -> Result<(WInstance, Complement), LensError> {
        let middle_instance = put(&self.left, left_view, left_complement)?;
        get(&self.right, &middle_instance)
    }

    /// Synchronize from right to left: given a right view, produce a left view.
    ///
    /// Puts the right view back into the middle, then gets the left view.
    ///
    /// # Errors
    ///
    /// Returns `LensError` if either the put or get operation fails.
    pub fn sync_right_to_left(
        &self,
        right_view: &WInstance,
        right_complement: &Complement,
    ) -> Result<(WInstance, Complement), LensError> {
        let middle_instance = put(&self.right, right_view, right_complement)?;
        get(&self.left, &middle_instance)
    }
}
