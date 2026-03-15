//! Garbage collection: remove unreachable objects.
//!
//! Walks all refs (branches, tags, stash) and marks all reachable
//! objects. Objects not reached from any ref are deleted.

use std::collections::HashSet;

use crate::error::VcsError;
use crate::hash::ObjectId;
use crate::object::Object;
use crate::store::Store;

/// Options controlling garbage collection behavior.
#[derive(Clone, Debug, Default)]
pub struct GcOptions {
    /// If true, only report what would be deleted without deleting.
    pub dry_run: bool,
}

/// Report from a garbage collection run.
#[derive(Clone, Debug, Default)]
pub struct GcReport {
    /// Number of objects marked as reachable.
    pub reachable: usize,
    /// Object IDs that were deleted.
    pub deleted: Vec<ObjectId>,
}

/// Mark all objects reachable from the given roots.
///
/// Follows commit → schema, commit → migration, and commit → parent
/// links transitively.
///
/// # Errors
///
/// Returns an error if loading objects fails.
pub fn mark_reachable(
    store: &dyn Store,
    roots: &[ObjectId],
) -> Result<HashSet<ObjectId>, VcsError> {
    let mut reachable = HashSet::new();
    let mut queue: Vec<ObjectId> = roots.to_vec();

    while let Some(id) = queue.pop() {
        if !reachable.insert(id) {
            continue;
        }
        if !store.has(&id) {
            continue;
        }

        match store.get(&id)? {
            Object::Commit(commit) => {
                queue.push(commit.schema_id);
                if let Some(mig_id) = commit.migration_id {
                    queue.push(mig_id);
                }
                for parent in commit.parents {
                    queue.push(parent);
                }
            }
            Object::Migration { src, tgt, .. } => {
                queue.push(src);
                queue.push(tgt);
            }
            Object::Schema(_) => {}
            Object::Tag(tag) => {
                queue.push(tag.target);
            }
        }
    }

    Ok(reachable)
}

/// Collect all ref targets (branches, tags, stash, HEAD).
///
/// # Errors
///
/// Returns an error on I/O failure.
pub fn collect_roots(store: &dyn Store) -> Result<Vec<ObjectId>, VcsError> {
    let mut roots = Vec::new();

    if let Some(id) = crate::store::resolve_head(store)? {
        roots.push(id);
    }

    for (_, id) in store.list_refs("refs/heads/")? {
        roots.push(id);
    }

    for (_, id) in store.list_refs("refs/tags/")? {
        roots.push(id);
    }

    if let Some(id) = store.get_ref("refs/stash")? {
        roots.push(id);
    }

    roots.dedup();
    Ok(roots)
}

/// Run garbage collection: mark reachable objects, delete the rest.
///
/// # Errors
///
/// Returns an error on I/O failure.
pub fn gc(store: &mut dyn Store) -> Result<GcReport, VcsError> {
    let roots = collect_roots(store)?;
    let reachable = mark_reachable(store, &roots)?;
    let all_objects = store.list_objects()?;

    let mut deleted = Vec::new();
    for id in all_objects {
        if !reachable.contains(&id) {
            store.delete_object(&id)?;
            deleted.push(id);
        }
    }

    Ok(GcReport {
        reachable: reachable.len(),
        deleted,
    })
}

/// Run garbage collection with options.
///
/// # Errors
///
/// Returns an error on I/O failure.
pub fn gc_with_options(store: &mut dyn Store, options: &GcOptions) -> Result<GcReport, VcsError> {
    if options.dry_run {
        let roots = collect_roots(store)?;
        let reachable = mark_reachable(store, &roots)?;
        let all_objects = store.list_objects()?;
        let deleted: Vec<ObjectId> = all_objects
            .into_iter()
            .filter(|id| !reachable.contains(id))
            .collect();
        Ok(GcReport {
            reachable: reachable.len(),
            deleted,
        })
    } else {
        gc(store)
    }
}

/// Compute reachability without deleting anything.
///
/// # Errors
///
/// Returns an error on I/O failure.
pub fn gc_report(store: &dyn Store) -> Result<GcReport, VcsError> {
    let roots = collect_roots(store)?;
    let reachable = mark_reachable(store, &roots)?;

    Ok(GcReport {
        reachable: reachable.len(),
        deleted: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MemStore;
    use crate::error::VcsError;
    use crate::object::CommitObject;

    fn empty_schema() -> panproto_schema::Schema {
        panproto_schema::Schema {
            protocol: "test".into(),
            vertices: std::collections::HashMap::new(),
            edges: std::collections::HashMap::new(),
            hyper_edges: std::collections::HashMap::new(),
            constraints: std::collections::HashMap::new(),
            required: std::collections::HashMap::new(),
            nsids: std::collections::HashMap::new(),
            variants: std::collections::HashMap::new(),
            orderings: std::collections::HashMap::new(),
            recursion_points: std::collections::HashMap::new(),
            spans: std::collections::HashMap::new(),
            usage_modes: std::collections::HashMap::new(),
            nominal: std::collections::HashMap::new(),
            outgoing: std::collections::HashMap::new(),
            incoming: std::collections::HashMap::new(),
            between: std::collections::HashMap::new(),
        }
    }

    #[test]
    fn mark_reachable_follows_commits() -> Result<(), VcsError> {
        let mut store = MemStore::new();

        let schema_id = store.put(&Object::Schema(Box::new(empty_schema())))?;

        let c0 = CommitObject {
            schema_id,
            parents: vec![],
            migration_id: None,
            protocol: "test".into(),
            author: "test".into(),
            timestamp: 100,
            message: "initial".into(),
        };
        let c0_id = store.put(&Object::Commit(c0))?;

        let c1 = CommitObject {
            schema_id,
            parents: vec![c0_id],
            migration_id: None,
            protocol: "test".into(),
            author: "test".into(),
            timestamp: 200,
            message: "second".into(),
        };
        let c1_id = store.put(&Object::Commit(c1))?;

        let reachable = mark_reachable(&store, &[c1_id])?;
        assert!(reachable.contains(&c1_id));
        assert!(reachable.contains(&c0_id));
        assert!(reachable.contains(&schema_id));
        Ok(())
    }

    #[test]
    fn gc_deletes_unreachable() -> Result<(), VcsError> {
        let mut store = MemStore::new();

        let schema_id = store.put(&Object::Schema(Box::new(empty_schema())))?;

        let c0 = CommitObject {
            schema_id,
            parents: vec![],
            migration_id: None,
            protocol: "test".into(),
            author: "test".into(),
            timestamp: 100,
            message: "initial".into(),
        };
        let c0_id = store.put(&Object::Commit(c0))?;
        store.set_ref("refs/heads/main", c0_id)?;

        // Add an orphan object not reachable from any ref.
        let orphan_schema_id = store.put(&Object::Schema(Box::new(empty_schema())))?;
        let orphan = CommitObject {
            schema_id: orphan_schema_id,
            parents: vec![],
            migration_id: None,
            protocol: "test".into(),
            author: "test".into(),
            timestamp: 300,
            message: "orphan".into(),
        };
        let orphan_id = store.put(&Object::Commit(orphan))?;

        // Before GC: orphan exists.
        assert!(store.has(&orphan_id));

        let report = gc(&mut store)?;
        assert_eq!(report.reachable, 2); // c0 + schema
        assert!(report.deleted.contains(&orphan_id));

        // After GC: orphan is gone.
        assert!(!store.has(&orphan_id));
        Ok(())
    }

    #[test]
    fn gc_report_counts_reachable() -> Result<(), VcsError> {
        let mut store = MemStore::new();

        let schema_id = store.put(&Object::Schema(Box::new(empty_schema())))?;

        let c0 = CommitObject {
            schema_id,
            parents: vec![],
            migration_id: None,
            protocol: "test".into(),
            author: "test".into(),
            timestamp: 100,
            message: "initial".into(),
        };
        let c0_id = store.put(&Object::Commit(c0))?;
        store.set_ref("refs/heads/main", c0_id)?;

        let report = gc_report(&store)?;
        assert_eq!(report.reachable, 2);
        Ok(())
    }
}
