//! Replaying a commit on another one, as cherry-pick and rebase do.
//!
//! A commit is a change of its parent's schema and of its parent's protocol
//! environment. Replaying it on a tip is the pushout of that change along the
//! one from the parent to the tip, taken on schemas and on environments, with
//! the commit's data transported to the result.

use panproto_schema::Schema;

use crate::data_mig::{LiftTarget, lift_commit_data};
use crate::environment::{ProtocolEnvironment, load_environment, store_environment};
use crate::error::VcsError;
use crate::hash::ObjectId;
use crate::merge::{self, MergeResult};
use crate::object::{CommitObject, Object};
use crate::square::verify_lifted_squares;
use crate::store::Store;

/// A commit replayed on a tip, before anything records it.
pub struct Replay {
    /// The replayed commit.
    pub commit: CommitObject,
    /// The object ID of the resulting schema.
    pub schema_id: ObjectId,
    tip_id: ObjectId,
    ours_schema: Schema,
    theirs_schema: Schema,
    result: MergeResult,
    environment: ProtocolEnvironment,
}

/// Replay the commit `commit_id` on the commit `tip_id`.
///
/// # Errors
///
/// [`VcsError::NoPath`] for a root commit, [`VcsError::MergeConflicts`] or
/// [`VcsError::ProtocolConflicts`] when the pushout of schemas or of
/// environments does not exist, and [`VcsError::PushoutVerification`] when the
/// schema merge is not one.
pub fn replay(
    store: &mut dyn Store,
    commit_id: ObjectId,
    tip_id: ObjectId,
) -> Result<Replay, VcsError> {
    let commit = load_commit(store, commit_id)?;
    let parent = load_commit(store, *commit.parents.first().ok_or(VcsError::NoPath)?)?;
    let tip = load_commit(store, tip_id)?;

    let base_schema = load_schema(store, &parent.schema_id)?;
    let theirs_schema = load_schema(store, &commit.schema_id)?;
    let ours_schema = load_schema(store, &tip.schema_id)?;

    let result = merge::three_way_merge(&base_schema, &ours_schema, &theirs_schema);
    let environment = ProtocolEnvironment::merge(
        &load_environment(store, parent.environment_id)?,
        &load_environment(store, tip.environment_id)?,
        &load_environment(store, commit.environment_id)?,
    )?;
    if !result.conflicts.is_empty() {
        return Err(VcsError::MergeConflicts {
            count: result.conflicts.len(),
        });
    }

    // Verify the pushout cocone conditions before rebuilding the commit:
    // both migrations must be total and the base-to-merged paths must
    // commute.
    let resolved = merge::ResolvedMerge {
        schema: result.merged_schema.clone(),
        migration_from_ours: result.migration_from_ours.clone(),
        migration_from_theirs: result.migration_from_theirs.clone(),
    };
    merge::verify_pushout(&base_schema, &ours_schema, &theirs_schema, &resolved)
        .map_err(VcsError::PushoutVerification)?;

    let schema_id = crate::tree::store_schema_as_tree(store, result.merged_schema.clone())?;
    Ok(Replay {
        commit,
        schema_id,
        tip_id,
        ours_schema,
        theirs_schema,
        result,
        environment,
    })
}

impl Replay {
    /// Store the commit this replay makes on its tip.
    ///
    /// # Errors
    ///
    /// The errors of [`lift_commit_data`] and [`verify_lifted_squares`], and
    /// store errors.
    pub fn into_commit(
        self,
        store: &mut dyn Store,
        author: &str,
        message: String,
    ) -> Result<ObjectId, VcsError> {
        let Self {
            commit,
            schema_id,
            tip_id,
            ours_schema,
            theirs_schema,
            result,
            environment,
        } = self;

        let target = LiftTarget {
            schema_id,
            schema: &result.merged_schema,
            environment: &environment,
        };
        let lifted = lift_commit_data(store, &commit, &theirs_schema, target)?;
        // Each transported data set must migrate back through its complement
        // to what it was, so the replay square commutes on data.
        verify_lifted_squares(store, &commit, &lifted, &theirs_schema, target)?;

        let src = store.put(&Object::FlatSchema(Box::new(ours_schema)))?;
        let tgt = store.put(&Object::FlatSchema(Box::new(result.merged_schema.clone())))?;
        let migration_id = store.put(&Object::Migration {
            src,
            tgt,
            mapping: result.migration_from_ours,
        })?;
        let environment_id = store_environment(store, environment)?;

        let replayed =
            CommitObject::builder(schema_id, result.merged_schema.protocol, author, message)
                .parents(vec![tip_id])
                .migration_id(migration_id)
                .environment_id(environment_id)
                .data_ids(lifted.data_ids)
                .complement_ids(lifted.complement_ids)
                // CST complements are keyed by content, not schema, so they
                // carry through unchanged.
                .cst_complement_ids(commit.cst_complement_ids)
                .build();
        store.put(&Object::Commit(replayed))
    }
}

/// The commit object at `id`.
///
/// # Errors
///
/// A store error, or [`VcsError::WrongObjectType`] when `id` names another
/// kind of object.
pub fn load_commit(store: &dyn Store, id: ObjectId) -> Result<CommitObject, VcsError> {
    match store.get(&id)? {
        Object::Commit(commit) => Ok(commit),
        other => Err(VcsError::WrongObjectType {
            expected: "commit",
            found: other.type_name(),
        }),
    }
}

/// The schema a commit's schema tree assembles to.
fn load_schema(store: &dyn Store, id: &ObjectId) -> Result<Schema, VcsError> {
    let protocol = crate::tree::project_coproduct_protocol();
    crate::tree::assemble_schema_dyn(store, id, &protocol)
}
