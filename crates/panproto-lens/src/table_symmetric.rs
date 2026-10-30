//! Symmetric-lens operations for relational table instances.

use panproto_inst::{FInstance, TableComplement};
use panproto_schema::Protocol;

use crate::Lens;
use crate::error::{LawViolation, LensError};
use crate::symmetric::{
    CoherenceViolation, SymmetricLens, check_symmetric_laws_shape, sync_left_to_right_shape,
    sync_right_to_left_shape, verify_complement_coherence_shape,
};
use crate::table_shape::{TableContext, TableShape};

/// Protocols at the middle, left, and right boundaries of a table span.
#[derive(Clone, Copy, Debug)]
pub struct TableSymmetricContext<'a> {
    /// Protocol describing the shared middle instance.
    pub middle: &'a Protocol,
    /// Protocol describing the left view.
    pub left: &'a Protocol,
    /// Protocol describing the right view.
    pub right: &'a Protocol,
}

impl TableSymmetricContext<'_> {
    fn left_leg(&self) -> TableContext {
        TableContext {
            src: self.middle.clone(),
            tgt: self.left.clone(),
        }
    }

    fn right_leg(&self) -> TableContext {
        TableContext {
            src: self.middle.clone(),
            tgt: self.right.clone(),
        }
    }
}

/// Construct a table-capable symmetric lens from two legs with one canonical middle.
///
/// # Errors
///
/// Returns [`LensError::NoSharedMiddle`] if the source schemas differ canonically.
pub fn from_span(left: Lens, right: Lens) -> Result<SymmetricLens, LensError> {
    SymmetricLens::from_span(left, right)
}

/// Synchronize a table view from the left boundary to the right boundary.
///
/// # Errors
///
/// Returns an error if the left put or right get fails.
pub fn sync_left_to_right(
    lens: &SymmetricLens,
    view: &FInstance,
    complement: &TableComplement,
    context: &TableSymmetricContext<'_>,
) -> Result<(FInstance, TableComplement), LensError> {
    sync_left_to_right_shape::<TableShape>(
        lens,
        view,
        complement,
        &context.left_leg(),
        &context.right_leg(),
    )
}

/// Synchronize a table view from the right boundary to the left boundary.
///
/// # Errors
///
/// Returns an error if the right put or left get fails.
pub fn sync_right_to_left(
    lens: &SymmetricLens,
    view: &FInstance,
    complement: &TableComplement,
    context: &TableSymmetricContext<'_>,
) -> Result<(FInstance, TableComplement), LensError> {
    sync_right_to_left_shape::<TableShape>(
        lens,
        view,
        complement,
        &context.left_leg(),
        &context.right_leg(),
    )
}

/// Observe both table sync equations and their complete complements.
#[must_use]
pub fn verify_complement_coherence(
    lens: &SymmetricLens,
    middle: &FInstance,
    context: &TableSymmetricContext<'_>,
) -> Vec<CoherenceViolation> {
    verify_complement_coherence_shape::<TableShape>(
        lens,
        middle,
        &context.left_leg(),
        &context.right_leg(),
    )
}

/// Check table-shaped `GetPut` and both symmetric sync equations.
///
/// # Errors
///
/// Returns the first operational failure or observational mismatch.
pub fn check_symmetric_laws(
    lens: &SymmetricLens,
    middle: &FInstance,
    context: &TableSymmetricContext<'_>,
) -> Result<(), LawViolation> {
    check_symmetric_laws_shape::<TableShape>(
        lens,
        middle,
        &context.left_leg(),
        &context.right_leg(),
    )
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use std::collections::HashMap;

    use super::*;
    use crate::symmetric::{TraceObservation, check_observational_equality};
    use crate::tests::identity_lens;
    use panproto_inst::{BoundaryProtocols, RowId};
    use panproto_schema::{Schema, SchemaBuilder};

    #[test]
    fn context_preserves_all_three_ordered_protocol_boundaries() {
        let middle = Protocol {
            name: "middle".into(),
            ..Protocol::default()
        };
        let left = Protocol {
            name: "left".into(),
            ..Protocol::default()
        };
        let right = Protocol {
            name: "right".into(),
            ..Protocol::default()
        };
        let context = TableSymmetricContext {
            middle: &middle,
            left: &left,
            right: &right,
        };
        assert_eq!(context.left_leg().src.name, "middle");
        assert_eq!(context.left_leg().tgt.name, "left");
        assert_eq!(context.right_leg().src.name, "middle");
        assert_eq!(context.right_leg().tgt.name, "right");
    }

    #[test]
    fn identity_table_span_satisfies_both_sync_equations() {
        let schema = Schema::default();
        let protocol = Protocol::default();
        let symmetric = from_span(identity_lens(&schema), identity_lens(&schema)).unwrap();
        let context = TableSymmetricContext {
            middle: &protocol,
            left: &protocol,
            right: &protocol,
        };
        let middle = FInstance::default();
        assert_eq!(
            verify_complement_coherence(&symmetric, &middle, &context),
            [] as [crate::symmetric::CoherenceViolation; 0]
        );
        check_symmetric_laws(&symmetric, &middle, &context).unwrap();
    }

    #[test]
    fn table_words_observe_the_same_stable_keyed_mutation() {
        let protocol = Protocol {
            non_value_kinds: std::iter::once("record".to_owned()).collect(),
            ..Protocol::default()
        };
        let schema = SchemaBuilder::new(&protocol)
            .vertex("post", "record", None::<&str>)
            .unwrap()
            .build()
            .unwrap();
        let first = from_span(identity_lens(&schema), identity_lens(&schema)).unwrap();
        let second = from_span(identity_lens(&schema), identity_lens(&schema)).unwrap();
        let mut middle = FInstance::new();
        middle
            .insert_row("post", RowId(7), HashMap::default())
            .unwrap();
        let protocols = BoundaryProtocols {
            src: &protocol,
            tgt: &protocol,
        };
        let mut first_state = crate::table_lens::get(&first.left, &middle, protocols).unwrap();
        let mut second_state = crate::table_lens::get(&second.left, &middle, protocols).unwrap();
        first_state
            .0
            .insert_row("post", RowId(8), HashMap::default())
            .unwrap();
        second_state
            .0
            .insert_row("post", RowId(8), HashMap::default())
            .unwrap();
        let shape_context = TableContext {
            src: protocol.clone(),
            tgt: protocol,
        };

        check_observational_equality::<TableShape>(
            &TraceObservation {
                lens: &first,
                left_context: &shape_context,
                right_context: &shape_context,
                view: &first_state.0,
                complement: &first_state.1,
            },
            &TraceObservation {
                lens: &second,
                left_context: &shape_context,
                right_context: &shape_context,
                view: &second_state.0,
                complement: &second_state.1,
            },
        )
        .unwrap();
    }
}
