//! Reset: move HEAD, unstage, or restore working state.

use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::VcsError;
use crate::hash::ObjectId;
use crate::store::{HeadState, ReflogEntry, Store};

/// Reset mode, matching git's `--soft`, `--mixed`, and `--hard`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResetMode {
    /// Move HEAD only. Index and working schema unchanged.
    Soft,
    /// Move HEAD and clear the index. Working schema unchanged.
    Mixed,
    /// Move HEAD, clear the index, and overwrite the working schema.
    Hard,
}

/// Reset HEAD (or the branch it points to) to `target`.
///
/// - **Soft**: Only moves the ref. Index and working schema are untouched.
/// - **Mixed**: Moves the ref and clears the staging index.
/// - **Hard**: Moves the ref, clears the index, and writes the target
///   schema to the working schema file (not implemented at this layer —
///   the `repo.rs` porcelain handles the filesystem write).
///
/// All modes append a reflog entry.
///
/// # Errors
///
/// Returns an error if the current HEAD state cannot be resolved or
/// if I/O fails.
pub fn reset(
    store: &mut dyn Store,
    target: ObjectId,
    mode: ResetMode,
    author: &str,
) -> Result<ResetOutcome, VcsError> {
    let old_head = crate::store::resolve_head(store)?;

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // Move the ref.
    match store.get_head()? {
        HeadState::Branch(name) => {
            let ref_name = format!("refs/heads/{name}");
            store.set_ref(&ref_name, target)?;
            store.append_reflog(
                &ref_name,
                ReflogEntry {
                    old_id: old_head,
                    new_id: target,
                    author: author.to_owned(),
                    timestamp,
                    message: format!("reset: moving to {}", target.short()),
                },
            )?;
        }
        HeadState::Detached(_) => {
            store.set_head(HeadState::Detached(target))?;
        }
    }

    store.append_reflog(
        "HEAD",
        ReflogEntry {
            old_id: old_head,
            new_id: target,
            author: author.to_owned(),
            timestamp,
            message: format!("reset: moving to {}", target.short()),
        },
    )?;

    Ok(ResetOutcome {
        old_head,
        new_head: target,
        mode,
        should_clear_index: mode != ResetMode::Soft,
        should_write_working: mode == ResetMode::Hard,
    })
}

/// Outcome of a reset operation.
///
/// The caller (repo.rs porcelain) uses this to determine what else
/// needs to happen (clearing the index file, writing the working schema).
#[derive(Clone, Debug)]
pub struct ResetOutcome {
    /// Previous HEAD commit (if any).
    pub old_head: Option<ObjectId>,
    /// New HEAD commit.
    pub new_head: ObjectId,
    /// The mode used.
    pub mode: ResetMode,
    /// Whether the index should be cleared.
    pub should_clear_index: bool,
    /// Whether the working schema file should be overwritten.
    pub should_write_working: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::object::{CommitObject, Object};
    use crate::MemStore;

    #[test]
    fn reset_soft_moves_ref() {
        let mut store = MemStore::new();
        let c0_id = store
            .put(&Object::Commit(CommitObject {
                schema_id: ObjectId::from_bytes([0; 32]),
                parents: vec![],
                migration_id: None,
                protocol: "test".into(),
                author: "test".into(),
                timestamp: 100,
                message: "c0".into(),
            }))
            .unwrap();
        let c1_id = store
            .put(&Object::Commit(CommitObject {
                schema_id: ObjectId::from_bytes([1; 32]),
                parents: vec![c0_id],
                migration_id: None,
                protocol: "test".into(),
                author: "test".into(),
                timestamp: 200,
                message: "c1".into(),
            }))
            .unwrap();

        store.set_ref("refs/heads/main", c1_id).unwrap();

        let outcome = reset(&mut store, c0_id, ResetMode::Soft, "test").unwrap();
        assert!(!outcome.should_clear_index);
        assert!(!outcome.should_write_working);

        // Ref should now point to c0.
        assert_eq!(
            store.get_ref("refs/heads/main").unwrap(),
            Some(c0_id)
        );
    }

    #[test]
    fn reset_mixed_clears_index() {
        let mut store = MemStore::new();
        let c0_id = store
            .put(&Object::Commit(CommitObject {
                schema_id: ObjectId::from_bytes([0; 32]),
                parents: vec![],
                migration_id: None,
                protocol: "test".into(),
                author: "test".into(),
                timestamp: 100,
                message: "c0".into(),
            }))
            .unwrap();

        store.set_ref("refs/heads/main", c0_id).unwrap();

        let outcome = reset(&mut store, c0_id, ResetMode::Mixed, "test").unwrap();
        assert!(outcome.should_clear_index);
        assert!(!outcome.should_write_working);
    }

    #[test]
    fn reset_hard_writes_working() {
        let mut store = MemStore::new();
        let c0_id = store
            .put(&Object::Commit(CommitObject {
                schema_id: ObjectId::from_bytes([0; 32]),
                parents: vec![],
                migration_id: None,
                protocol: "test".into(),
                author: "test".into(),
                timestamp: 100,
                message: "c0".into(),
            }))
            .unwrap();

        store.set_ref("refs/heads/main", c0_id).unwrap();

        let outcome = reset(&mut store, c0_id, ResetMode::Hard, "test").unwrap();
        assert!(outcome.should_clear_index);
        assert!(outcome.should_write_working);
    }

    #[test]
    fn reset_appends_reflog() {
        let mut store = MemStore::new();
        let c0_id = store
            .put(&Object::Commit(CommitObject {
                schema_id: ObjectId::from_bytes([0; 32]),
                parents: vec![],
                migration_id: None,
                protocol: "test".into(),
                author: "test".into(),
                timestamp: 100,
                message: "c0".into(),
            }))
            .unwrap();

        store.set_ref("refs/heads/main", c0_id).unwrap();

        reset(&mut store, c0_id, ResetMode::Soft, "alice").unwrap();

        let log = store.read_reflog("HEAD", None).unwrap();
        assert_eq!(log.len(), 1);
        assert!(log[0].message.contains("reset"));
    }
}
