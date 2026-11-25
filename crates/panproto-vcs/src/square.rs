//! The round-trip check a replayed data migration has to pass.
//!
//! Commits are the horizontal arrows and data migrations the vertical arrows of
//! a double category whose objects pair a schema with its data sets, and a
//! square in that category commutes when the two paths around it agree on data.
//!
//! What this module checks is one necessary condition of that, not the
//! condition itself. [`verify_square`] takes a data set, its forward image, and
//! the complement the forward migration produced, migrates the image back
//! through that complement, and requires the result to be the data set it
//! started from. That is the `GetPut` law of the vertical arrow, which is the
//! square condition specialized to an identity horizontal arrow. It is what a
//! replay can check with the objects a replay has: the two paths around a
//! genuine square need the horizontal arrow's action on data, and a rebase
//! never materializes it.
//!
//! So a divergence here is decisive and the replay is rejected: a vertical
//! arrow that loses data cannot sit in a commuting square. Agreement here is
//! not a proof that the square commutes.

use std::collections::HashMap;
use std::hash::BuildHasher;

use panproto_schema::{Protocol, Schema};

use crate::data_mig::migrate_backward;
use crate::error::VcsError;
use crate::hash::ObjectId;
use crate::object::{CommitObject, Object};
use crate::payload::Instance;
use crate::store::Store;

/// Run the round-trip check on every data set a replay or cherry-pick lifts
/// from `commit` out of the schema `src_schema`.
///
/// See the module documentation for what the check does and does not
/// establish about the square it guards.
///
/// A real migration appends one backward-migration complement per lifted data
/// set, aligned after the commit's existing complements. The identity lift
/// appends none and commutes trivially, so is skipped.
///
/// # Errors
///
/// Returns [`VcsError::SquareNonCommuting`] for the first data set whose
/// round-trip diverges, [`VcsError::UnknownProtocol`] when the source
/// schema's protocol name resolves neither through the commit's protocol
/// object, nor `registered`, nor the protocol registry; propagates
/// migration or store errors.
pub fn verify_lifted_squares<S: BuildHasher>(
    store: &mut dyn Store,
    commit: &CommitObject,
    lifted_data_ids: &[ObjectId],
    lifted_complement_ids: &[ObjectId],
    src_schema: &Schema,
    registered: &HashMap<String, Protocol, S>,
) -> Result<(), VcsError> {
    let existing = commit.complement_ids.len();
    if lifted_data_ids.is_empty() || lifted_complement_ids.len() != existing + lifted_data_ids.len()
    {
        return Ok(());
    }
    let stored = crate::data_mig::commit_protocol(store, commit)?;
    let protocol = crate::data_mig::protocol_for_schema(
        src_schema,
        crate::data_mig::ProtocolSources {
            stored: stored.as_ref(),
            registered,
        },
    )?;
    for (i, (&base_id, &replayed_id)) in commit.data_ids.iter().zip(lifted_data_ids).enumerate() {
        verify_square(
            store,
            base_id,
            replayed_id,
            lifted_complement_ids[existing + i],
            &protocol,
        )?;
    }
    Ok(())
}

/// Verify that migrating `replayed` back through `complement` reconstructs
/// `base`, the round-trip law the vertical arrow of a replayed square owes.
///
/// `base` is the data set before the migration, `replayed` its forward image,
/// and `complement` the complement the forward migration produced, which
/// names the migration ([`migrate_backward`]); `protocol` describes a table
/// lens's boundaries.
///
/// A failure means the migration loses data the complement did not record, so
/// no square around it commutes. Success establishes the round-trip law alone;
/// see the module documentation for why that is short of the full square
/// condition.
///
/// # Errors
///
/// Returns [`VcsError::SquareNonCommuting`] naming the divergent data set when
/// the round-trip does not reconstruct `base`, and propagates any migration or
/// store error.
pub fn verify_square(
    store: &mut dyn Store,
    base: ObjectId,
    replayed: ObjectId,
    complement: ObjectId,
    protocol: &Protocol,
) -> Result<(), VcsError> {
    let roundtrip = migrate_backward(store, replayed, complement, protocol)?;
    let before = load_instance(store, base)?;
    let after = load_instance(store, roundtrip)?;
    match (&before, &after) {
        (Instance::Tree(before), Instance::Tree(after)) => {
            if before.len() != after.len() {
                return Err(VcsError::SquareNonCommuting {
                    detail: format!(
                        "data set count changed under round-trip: {} then {}",
                        before.len(),
                        after.len()
                    ),
                });
            }
            for (i, (before, after)) in before.iter().zip(after).enumerate() {
                if !panproto_lens::instances_equivalent(before, after) {
                    return Err(VcsError::SquareNonCommuting {
                        detail: format!("data set {i} diverges from its forward-then-back image"),
                    });
                }
            }
            Ok(())
        }
        (Instance::Table(before), Instance::Table(after)) if before == after => Ok(()),
        (Instance::Graph(before), Instance::Graph(after))
            if panproto_inst::GInstance::identical(before, after) =>
        {
            Ok(())
        }
        _ => Err(VcsError::SquareNonCommuting {
            detail: format!(
                "the {} data set diverges from its forward-then-back image",
                before.shape().name()
            ),
        }),
    }
}

/// Load and decode a data set object.
fn load_instance(store: &dyn Store, id: ObjectId) -> Result<Instance, VcsError> {
    match store.get(&id)? {
        Object::DataSet(dataset) => dataset.decode(),
        other => Err(VcsError::WrongObjectType {
            expected: "dataset",
            found: other.type_name(),
        }),
    }
}
