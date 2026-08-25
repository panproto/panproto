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

use panproto_inst::WInstance;
use panproto_schema::{Protocol, Schema};

use crate::data_mig::migrate_backward;
use crate::error::VcsError;
use crate::hash::ObjectId;
use crate::object::{CommitObject, Object};
use crate::store::Store;

/// Run the round-trip check on every data set a replay or cherry-pick lifts
/// from `commit` through the schema change `src_schema -> tgt_schema`.
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
/// round-trip diverges; propagates migration or store errors.
pub fn verify_lifted_squares(
    store: &mut dyn Store,
    commit: &CommitObject,
    lifted_data_ids: &[ObjectId],
    lifted_complement_ids: &[ObjectId],
    src_schema: &Schema,
    tgt_schema: &Schema,
) -> Result<(), VcsError> {
    let existing = commit.complement_ids.len();
    if lifted_complement_ids.len() != existing + lifted_data_ids.len() {
        return Ok(());
    }
    let protocol = crate::data_mig::protocol_for_schema(src_schema);
    for (i, (&base_id, &replayed_id)) in commit.data_ids.iter().zip(lifted_data_ids).enumerate() {
        verify_square(
            store,
            base_id,
            replayed_id,
            lifted_complement_ids[existing + i],
            src_schema,
            tgt_schema,
            &protocol,
        )?;
    }
    Ok(())
}

/// Verify that migrating `replayed` back through `complement` reconstructs
/// `base`, the round-trip law the vertical arrow of a replayed square owes.
///
/// `base` is the data set before the migration, `replayed` its forward image,
/// and `complement` the complement the forward migration produced. `src_schema`
/// and `tgt_schema` are the forward migration's endpoints (the same order as
/// [`migrate_backward`]).
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
    src_schema: &Schema,
    tgt_schema: &Schema,
    protocol: &Protocol,
) -> Result<(), VcsError> {
    let roundtrip = migrate_backward(
        store, replayed, complement, src_schema, tgt_schema, protocol,
    )?;

    let base_instances = load_instances(store, base)?;
    let roundtrip_instances = load_instances(store, roundtrip)?;

    if base_instances.len() != roundtrip_instances.len() {
        return Err(VcsError::SquareNonCommuting {
            detail: format!(
                "data set count changed under round-trip: {} then {}",
                base_instances.len(),
                roundtrip_instances.len()
            ),
        });
    }

    for (i, (before, after)) in base_instances.iter().zip(&roundtrip_instances).enumerate() {
        if !panproto_lens::instances_equivalent(before, after) {
            return Err(VcsError::SquareNonCommuting {
                detail: format!("data set {i} diverges from its forward-then-back image"),
            });
        }
    }

    Ok(())
}

/// Load a data set object's W-type instances.
fn load_instances(store: &dyn Store, id: ObjectId) -> Result<Vec<WInstance>, VcsError> {
    match store.get(&id)? {
        Object::DataSet(ds) => {
            rmp_serde::from_slice(&ds.data).map_err(|e| VcsError::DataMigrationFailed {
                reason: format!("deserialize data set: {e}"),
            })
        }
        other => Err(VcsError::TypeMismatch {
            expected: "DataSet".into(),
            got: other.type_name().into(),
        }),
    }
}
