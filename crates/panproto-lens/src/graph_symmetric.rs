//! Symmetric-lens operations for graph-shaped instances.

use panproto_inst::{GInstance, GraphComplement};

use crate::Lens;
use crate::error::{LawViolation, LensError};
use crate::lens_shape::GraphShape;
use crate::symmetric::{
    CoherenceViolation, SymmetricLens, check_symmetric_laws_shape, sync_left_to_right_shape,
    sync_right_to_left_shape, verify_complement_coherence_shape,
};

/// Construct a graph-capable symmetric lens from two legs with one canonical middle.
///
/// # Errors
///
/// Returns [`LensError::NoSharedMiddle`] if the source schemas differ canonically.
pub fn from_span(left: Lens, right: Lens) -> Result<SymmetricLens, LensError> {
    SymmetricLens::from_span(left, right)
}

/// Synchronize a graph view from the left boundary to the right boundary.
///
/// # Errors
///
/// Returns an error if the left put or right get fails.
pub fn sync_left_to_right(
    lens: &SymmetricLens,
    view: &GInstance,
    complement: &GraphComplement,
) -> Result<(GInstance, GraphComplement), LensError> {
    sync_left_to_right_shape::<GraphShape>(lens, view, complement, &(), &())
}

/// Synchronize a graph view from the right boundary to the left boundary.
///
/// # Errors
///
/// Returns an error if the right put or left get fails.
pub fn sync_right_to_left(
    lens: &SymmetricLens,
    view: &GInstance,
    complement: &GraphComplement,
) -> Result<(GInstance, GraphComplement), LensError> {
    sync_right_to_left_shape::<GraphShape>(lens, view, complement, &(), &())
}

/// Observe both graph sync equations and their complete complements.
#[must_use]
pub fn verify_complement_coherence(
    lens: &SymmetricLens,
    middle: &GInstance,
) -> Vec<CoherenceViolation> {
    verify_complement_coherence_shape::<GraphShape>(lens, middle, &(), &())
}

/// Check graph-shaped `GetPut` and both symmetric sync equations.
///
/// # Errors
///
/// Returns the first operational failure or observational mismatch.
pub fn check_symmetric_laws(lens: &SymmetricLens, middle: &GInstance) -> Result<(), LawViolation> {
    check_symmetric_laws_shape::<GraphShape>(lens, middle, &(), &())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::symmetric::{TraceObservation, check_observational_equality};
    use crate::tests::{identity_lens, three_node_schema};
    use panproto_inst::{Node, Value};

    #[test]
    fn identity_graph_span_satisfies_both_sync_equations() {
        let schema = three_node_schema();
        let symmetric = from_span(identity_lens(&schema), identity_lens(&schema)).unwrap();
        let middle = GInstance::default();
        assert_eq!(
            verify_complement_coherence(&symmetric, &middle),
            [] as [crate::symmetric::CoherenceViolation; 0]
        );
        check_symmetric_laws(&symmetric, &middle).unwrap();
    }

    #[test]
    fn graph_words_observe_the_same_stable_keyed_mutation() {
        let schema = three_node_schema();
        let first = from_span(identity_lens(&schema), identity_lens(&schema)).unwrap();
        let second = from_span(identity_lens(&schema), identity_lens(&schema)).unwrap();
        let middle = GInstance::new().with_node(Node::new(7, "post:body"));
        let mut first_state = crate::graph_lens::get(&first.left, &middle).unwrap();
        let mut second_state = crate::graph_lens::get(&second.left, &middle).unwrap();
        first_state
            .0
            .node_mut(7)
            .unwrap()
            .extra_fields
            .insert("edited".into(), Value::Int(1));
        second_state
            .0
            .node_mut(7)
            .unwrap()
            .extra_fields
            .insert("edited".into(), Value::Int(1));

        check_observational_equality::<GraphShape>(
            &TraceObservation {
                lens: &first,
                left_context: &(),
                right_context: &(),
                view: &first_state.0,
                complement: &first_state.1,
            },
            &TraceObservation {
                lens: &second,
                left_context: &(),
                right_context: &(),
                view: &second_state.0,
                complement: &second_state.1,
            },
        )
        .unwrap();
    }
}
