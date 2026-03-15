//! Stash: save and restore working state.
//!
//! Stashes are stored as special commits. The stash ref (`refs/stash`)
//! points to the latest stash entry. Previous stashes are accessible
//! via reflog entries on `refs/stash`.

use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::VcsError;
use crate::hash::ObjectId;
use crate::object::{CommitObject, Object};
use crate::store::{self, ReflogEntry, Store};

/// A stash entry for display.
#[derive(Clone, Debug)]
pub struct StashEntry {
    /// Stash index (0 = most recent).
    pub index: usize,
    /// The stash commit ID.
    pub commit_id: ObjectId,
    /// The message attached to the stash.
    pub message: String,
    /// When it was stashed.
    pub timestamp: u64,
}

/// Save the current staged schema as a stash entry.
///
/// Creates a special commit with `parent[0] = HEAD` and stores it
/// at `refs/stash`. The previous stash (if any) is preserved in the
/// reflog for `refs/stash`.
///
/// # Errors
///
/// Returns an error if HEAD cannot be resolved or if there is nothing
/// to stash (the `schema_id` must be provided by the caller — typically
/// the index's staged schema).
pub fn stash_push(
    store: &mut dyn Store,
    schema_id: ObjectId,
    author: &str,
    message: Option<&str>,
) -> Result<ObjectId, VcsError> {
    let head_id = store::resolve_head(store)?.ok_or_else(|| VcsError::RefNotFound {
        name: "HEAD".to_owned(),
    })?;

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let msg = message.unwrap_or("WIP on stash").to_owned();

    let stash_commit = CommitObject {
        schema_id,
        parents: vec![head_id],
        migration_id: None,
        protocol: String::new(), // stash commits don't track protocol
        author: author.to_owned(),
        timestamp,
        message: msg.clone(),
    };
    let stash_id = store.put(&Object::Commit(stash_commit))?;

    // Get old stash ref (if any) for the reflog.
    let old_stash = store.get_ref("refs/stash")?;

    store.set_ref("refs/stash", stash_id)?;
    store.append_reflog(
        "refs/stash",
        ReflogEntry {
            old_id: old_stash,
            new_id: stash_id,
            author: author.to_owned(),
            timestamp,
            message: msg,
        },
    )?;

    Ok(stash_id)
}

/// Pop the most recent stash entry.
///
/// Returns the schema ID from the stash commit and removes the stash
/// ref. If there are older stashes in the reflog, the ref is updated
/// to the previous one.
///
/// # Errors
///
/// Returns an error if there is no stash.
pub fn stash_pop(store: &mut dyn Store) -> Result<ObjectId, VcsError> {
    let stash_id = store
        .get_ref("refs/stash")?
        .ok_or_else(|| VcsError::RefNotFound {
            name: "refs/stash".to_owned(),
        })?;

    let stash_commit = match store.get(&stash_id)? {
        Object::Commit(c) => c,
        other => {
            return Err(VcsError::WrongObjectType {
                expected: "commit",
                found: other.type_name(),
            });
        }
    };

    let schema_id = stash_commit.schema_id;

    // Check the reflog for a previous stash.
    let reflog = store.read_reflog("refs/stash", Some(2))?;
    if reflog.len() > 1 {
        // Restore previous stash.
        store.set_ref("refs/stash", reflog[1].new_id)?;
    } else {
        // No more stashes — remove the ref.
        // Use set_ref to a sentinel then delete, or just delete.
        let _ = store.delete_ref("refs/stash");
    }

    Ok(schema_id)
}

/// List all stash entries.
///
/// # Errors
///
/// Returns an error on I/O failure.
pub fn stash_list(store: &dyn Store) -> Result<Vec<StashEntry>, VcsError> {
    let reflog = store.read_reflog("refs/stash", None)?;
    Ok(reflog
        .into_iter()
        .enumerate()
        .map(|(i, entry)| StashEntry {
            index: i,
            commit_id: entry.new_id,
            message: entry.message,
            timestamp: entry.timestamp,
        })
        .collect())
}

/// Apply a stash entry without removing it.
///
/// Like [`stash_pop`] but preserves the stash entry in the stack.
///
/// # Errors
///
/// Returns an error if there is no stash at the given index.
pub fn stash_apply(store: &dyn Store, index: usize) -> Result<ObjectId, VcsError> {
    let entries = stash_list(store)?;
    let entry = entries.get(index).ok_or_else(|| VcsError::RefNotFound {
        name: format!("stash@{{{index}}}"),
    })?;

    let stash_commit = match store.get(&entry.commit_id)? {
        Object::Commit(c) => c,
        other => {
            return Err(VcsError::WrongObjectType {
                expected: "commit",
                found: other.type_name(),
            });
        }
    };

    Ok(stash_commit.schema_id)
}

/// Show the schema from a stash entry.
///
/// Returns the schema ID stored in the stash commit at the given index.
///
/// # Errors
///
/// Returns an error if the stash index is out of range.
pub fn stash_show(store: &dyn Store, index: usize) -> Result<ObjectId, VcsError> {
    stash_apply(store, index)
}

/// Remove all stash entries.
///
/// # Errors
///
/// Returns an error on I/O failure.
pub fn stash_clear(store: &mut dyn Store) -> Result<(), VcsError> {
    if store.get_ref("refs/stash")?.is_some() {
        let _ = store.delete_ref("refs/stash");
    }
    Ok(())
}

/// Drop a specific stash entry by index.
///
/// Currently only supports dropping stash@{0} (the most recent).
/// Dropping older stashes would require reflog rewriting.
///
/// # Errors
///
/// Returns an error if the stash index is out of range.
pub fn stash_drop(store: &mut dyn Store, index: usize) -> Result<(), VcsError> {
    if index != 0 {
        return Err(VcsError::RefNotFound {
            name: format!("stash@{{{index}}} (only stash@{{0}} can be dropped)"),
        });
    }
    stash_pop(store)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MemStore;
    use crate::error::VcsError;

    #[test]
    fn stash_push_and_pop() -> Result<(), VcsError> {
        let mut store = MemStore::new();

        // Need a HEAD commit.
        let head_commit = CommitObject {
            schema_id: ObjectId::from_bytes([0; 32]),
            parents: vec![],
            migration_id: None,
            protocol: "test".into(),
            author: "test".into(),
            timestamp: 100,
            message: "initial".into(),
        };
        let head_id = store.put(&Object::Commit(head_commit))?;
        store.set_ref("refs/heads/main", head_id)?;

        // Stash a schema.
        let stashed_schema_id = ObjectId::from_bytes([42; 32]);
        let _stash_id = stash_push(&mut store, stashed_schema_id, "alice", Some("my stash"))?;
        assert!(store.get_ref("refs/stash")?.is_some());

        // List stashes.
        let stashes = stash_list(&store)?;
        assert_eq!(stashes.len(), 1);
        assert_eq!(stashes[0].message, "my stash");

        // Pop the stash.
        let popped = stash_pop(&mut store)?;
        assert_eq!(popped, stashed_schema_id);

        // Stash ref should be gone.
        assert!(store.get_ref("refs/stash")?.is_none());
        Ok(())
    }

    #[test]
    fn stash_multiple() -> Result<(), VcsError> {
        let mut store = MemStore::new();

        let head_commit = CommitObject {
            schema_id: ObjectId::from_bytes([0; 32]),
            parents: vec![],
            migration_id: None,
            protocol: "test".into(),
            author: "test".into(),
            timestamp: 100,
            message: "initial".into(),
        };
        let head_id = store.put(&Object::Commit(head_commit))?;
        store.set_ref("refs/heads/main", head_id)?;

        // Push two stashes.
        stash_push(
            &mut store,
            ObjectId::from_bytes([1; 32]),
            "alice",
            Some("first"),
        )?;
        stash_push(
            &mut store,
            ObjectId::from_bytes([2; 32]),
            "alice",
            Some("second"),
        )?;

        let stashes = stash_list(&store)?;
        assert_eq!(stashes.len(), 2);
        assert_eq!(stashes[0].message, "second");
        assert_eq!(stashes[1].message, "first");

        // Pop most recent.
        let popped = stash_pop(&mut store)?;
        assert_eq!(popped, ObjectId::from_bytes([2; 32]));
        Ok(())
    }

    #[test]
    fn stash_pop_empty_fails() {
        let mut store = MemStore::new();
        assert!(stash_pop(&mut store).is_err());
    }
}
