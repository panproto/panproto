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
use panproto_schema::Schema;

use crate::data_mig::{
    Fibre, LiftTarget, LiftedData, TransportedData, data_protocol, load_data_set, migrate_backward,
};
use crate::error::VcsError;
use crate::hash::ObjectId;
use crate::object::CommitObject;
use crate::store::Store;

/// Verify the square of every data set `lifted` transported.
///
/// # Errors
///
/// Returns [`VcsError::SquareNonCommuting`] for the first data set whose
/// round-trip diverges, and propagates migration or store errors.
pub fn verify_lifted_squares(
    store: &mut dyn Store,
    commit: &CommitObject,
    lifted: &LiftedData,
    src_schema: &Schema,
    target: LiftTarget<'_>,
) -> Result<(), VcsError> {
    for transported in &lifted.transported {
        let base = load_data_set(store, transported.base)?;
        let image = load_data_set(store, transported.image)?;
        let src_protocol = data_protocol(store, commit, src_schema, base.protocol_id)?;
        let tgt_protocol = data_protocol(store, commit, target.schema, image.protocol_id)?;
        verify_square(
            store,
            *transported,
            Fibre {
                schema_id: base.schema_id,
                schema: src_schema,
                protocol_id: base.protocol_id,
                protocol: &src_protocol,
            },
            Fibre {
                schema_id: image.schema_id,
                schema: target.schema,
                protocol_id: image.protocol_id,
                protocol: &tgt_protocol,
            },
        )?;
    }
    Ok(())
}

/// Verify that migrating the image of `transported` back through its
/// complement reconstructs its base, the round-trip law the vertical arrow of
/// a replayed square owes.
///
/// `src` and `tgt` are the forward migration's endpoints (the same order as
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
    transported: TransportedData,
    src: Fibre<'_>,
    tgt: Fibre<'_>,
) -> Result<(), VcsError> {
    let TransportedData {
        base,
        image,
        complement,
    } = transported;
    let roundtrip = migrate_backward(store, image, complement, src, tgt)?;

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
    rmp_serde::from_slice(&load_data_set(store, id)?.data).map_err(|e| {
        VcsError::DataMigrationFailed {
            reason: format!("deserialize data set: {e}"),
        }
    })
}
