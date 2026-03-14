//! DAG traversal algorithms.
//!
//! Operations on the commit DAG: finding merge bases, paths between
//! commits, walking history, and composing migrations along a path.

use std::collections::{BinaryHeap, HashSet, VecDeque};

use panproto_mig::Migration;

use crate::error::VcsError;
use crate::hash::ObjectId;
use crate::object::{CommitObject, Object};
use crate::store::Store;

/// Find the merge base (lowest common ancestor) of two commits.
///
/// Computes all ancestors of both commits, finds their intersection,
/// then filters to the *lowest* (most recent) common ancestors — those
/// that are not proper ancestors of any other common ancestor.
///
/// If multiple LCAs exist (criss-cross merges), returns the one with
/// the highest timestamp for determinism.
///
/// Returns `None` if the commits have disjoint histories.
///
/// # Errors
///
/// Returns an error if loading commits from the store fails.
pub fn merge_base(
    store: &dyn Store,
    a: ObjectId,
    b: ObjectId,
) -> Result<Option<ObjectId>, VcsError> {
    if a == b {
        return Ok(Some(a));
    }

    // 1. Compute all ancestors of both commits (including themselves).
    let ancestors_a = all_ancestors(store, a)?;
    let ancestors_b = all_ancestors(store, b)?;

    // 2. Common ancestors.
    let common: HashSet<ObjectId> = ancestors_a.intersection(&ancestors_b).copied().collect();
    if common.is_empty() {
        return Ok(None);
    }

    // 3. Filter to LCAs: keep C where no other common ancestor is a
    //    proper descendant of C (i.e., C is maximal).
    let lcas: Vec<ObjectId> = common
        .iter()
        .filter(|&&c| {
            // c is an LCA if no other common ancestor d has c as a proper ancestor.
            !common
                .iter()
                .any(|&d| d != c && ancestors_of_contains(store, d, c))
        })
        .copied()
        .collect();

    // 4. Deterministic pick: highest timestamp, then lexicographic ObjectId.
    Ok(lcas.into_iter().max_by(|x, y| {
        let tx = get_commit(store, *x).map(|c| c.timestamp).unwrap_or(0);
        let ty = get_commit(store, *y).map(|c| c.timestamp).unwrap_or(0);
        tx.cmp(&ty).then_with(|| x.cmp(y))
    }))
}

/// Compute all ancestors of a commit (including itself) via BFS.
fn all_ancestors(store: &dyn Store, start: ObjectId) -> Result<HashSet<ObjectId>, VcsError> {
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    visited.insert(start);
    queue.push_back(start);

    while let Some(current) = queue.pop_front() {
        let commit = get_commit(store, current)?;
        for &parent in &commit.parents {
            if visited.insert(parent) {
                queue.push_back(parent);
            }
        }
    }

    Ok(visited)
}

/// Check whether `ancestor` is a proper ancestor of `descendant`.
/// (Walks parents of `descendant` looking for `ancestor`.)
fn ancestors_of_contains(store: &dyn Store, descendant: ObjectId, ancestor: ObjectId) -> bool {
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();

    // Start from descendant's parents (proper ancestor, not self).
    if let Ok(commit) = get_commit(store, descendant) {
        for &parent in &commit.parents {
            if parent == ancestor {
                return true;
            }
            if visited.insert(parent) {
                queue.push_back(parent);
            }
        }
    }

    while let Some(current) = queue.pop_front() {
        if let Ok(commit) = get_commit(store, current) {
            for &parent in &commit.parents {
                if parent == ancestor {
                    return true;
                }
                if visited.insert(parent) {
                    queue.push_back(parent);
                }
            }
        }
    }

    false
}

/// Find a path from `from` to `to` in the commit DAG.
///
/// Returns commits in chronological order (`from` first, `to` last).
/// Uses BFS from `to` walking parent edges until `from` is found.
///
/// # Errors
///
/// Returns [`VcsError::NoPath`] if no path exists.
pub fn find_path(
    store: &dyn Store,
    from: ObjectId,
    to: ObjectId,
) -> Result<Vec<ObjectId>, VcsError> {
    if from == to {
        return Ok(vec![from]);
    }

    // BFS backwards from `to`, recording parent chains.
    let mut visited: HashMap<ObjectId, ObjectId> = HashMap::new(); // child → parent used to reach it
    let mut queue: VecDeque<ObjectId> = VecDeque::new();
    queue.push_back(to);
    visited.insert(to, to); // sentinel

    while let Some(current) = queue.pop_front() {
        let commit = get_commit(store, current)?;
        for &parent in &commit.parents {
            if visited.contains_key(&parent) {
                continue;
            }
            visited.insert(parent, current);
            if parent == from {
                // Reconstruct path.
                let mut path = vec![from];
                let mut cursor = from;
                while cursor != to {
                    cursor = visited[&cursor];
                    path.push(cursor);
                }
                return Ok(path);
            }
            queue.push_back(parent);
        }
    }

    Err(VcsError::NoPath)
}

use std::collections::HashMap;

/// Walk the commit log starting from `start`, yielding commits in
/// reverse chronological order (newest first).
///
/// Uses a max-heap by timestamp for topological-chronological ordering.
/// Handles merge commits correctly by visiting each commit only once.
///
/// # Errors
///
/// Returns an error if loading commits fails.
pub fn log_walk(
    store: &dyn Store,
    start: ObjectId,
    limit: Option<usize>,
) -> Result<Vec<CommitObject>, VcsError> {
    let mut result = Vec::new();
    let mut visited: HashSet<ObjectId> = HashSet::new();
    let mut heap: BinaryHeap<(u64, ObjectId)> = BinaryHeap::new();

    let first = get_commit(store, start)?;
    heap.push((first.timestamp, start));
    visited.insert(start);

    while let Some((_, commit_id)) = heap.pop() {
        let commit = get_commit(store, commit_id)?;
        for &parent in &commit.parents {
            if visited.insert(parent) {
                let parent_commit = get_commit(store, parent)?;
                heap.push((parent_commit.timestamp, parent));
            }
        }
        result.push(commit);

        if let Some(n) = limit {
            if result.len() >= n {
                break;
            }
        }
    }

    Ok(result)
}

/// Compose all migrations along a path of commits.
///
/// Given a path `[c0, c1, c2, ..., cn]` (in chronological order),
/// composes the migrations `c0→c1`, `c1→c2`, ..., `c(n-1)→cn` into
/// a single migration `c0→cn`.
///
/// # Errors
///
/// Returns an error if any commit lacks a migration or composition fails.
pub fn compose_path(store: &dyn Store, path: &[ObjectId]) -> Result<Migration, VcsError> {
    if path.len() < 2 {
        return Ok(Migration::empty());
    }

    // Load the first migration.
    let first_commit = get_commit(store, path[1])?;
    let mut composed = get_migration(store, first_commit.migration_id)?;

    // Compose subsequent migrations.
    for window in path.windows(2).skip(1) {
        let commit = get_commit(store, window[1])?;
        let mig = get_migration(store, commit.migration_id)?;
        composed = panproto_mig::compose(&composed, &mig)?;
    }

    Ok(composed)
}

/// Check whether `ancestor` is an ancestor of `descendant` in the DAG.
///
/// # Errors
///
/// Returns an error if loading commits fails.
pub fn is_ancestor(
    store: &dyn Store,
    ancestor: ObjectId,
    descendant: ObjectId,
) -> Result<bool, VcsError> {
    if ancestor == descendant {
        return Ok(true);
    }
    let mut visited: HashSet<ObjectId> = HashSet::new();
    let mut queue: VecDeque<ObjectId> = VecDeque::new();
    queue.push_back(descendant);
    visited.insert(descendant);

    while let Some(current) = queue.pop_front() {
        let commit = get_commit(store, current)?;
        for &parent in &commit.parents {
            if parent == ancestor {
                return Ok(true);
            }
            if visited.insert(parent) {
                queue.push_back(parent);
            }
        }
    }
    Ok(false)
}

/// Count the number of commits between `from` and `to` (exclusive of `from`).
///
/// Returns 0 if `from == to`. Returns the path length minus 1.
///
/// # Errors
///
/// Returns [`VcsError::NoPath`] if no path exists.
pub fn commit_count(store: &dyn Store, from: ObjectId, to: ObjectId) -> Result<usize, VcsError> {
    let path = find_path(store, from, to)?;
    Ok(path.len().saturating_sub(1))
}

// -- helpers --

/// Load a commit from the store, returning an error if not found or wrong type.
fn get_commit(store: &dyn Store, id: ObjectId) -> Result<CommitObject, VcsError> {
    match store.get(&id)? {
        Object::Commit(c) => Ok(c),
        other => Err(VcsError::WrongObjectType {
            expected: "commit",
            found: other.type_name(),
        }),
    }
}

/// Load a migration from the store by optional ID.
fn get_migration(store: &dyn Store, id: Option<ObjectId>) -> Result<Migration, VcsError> {
    let id = id.ok_or(VcsError::NoPath)?;
    match store.get(&id)? {
        Object::Migration { mapping, .. } => Ok(mapping),
        other => Err(VcsError::WrongObjectType {
            expected: "migration",
            found: other.type_name(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MemStore, Store};

    /// Build a linear chain of commits: c0 → c1 → c2 → ...
    /// Returns (store, vec of commit IDs).
    fn build_linear_history(n: usize) -> (MemStore, Vec<ObjectId>) {
        let mut store = MemStore::new();
        let mut ids = Vec::new();

        for i in 0..n {
            let parents = if i == 0 { vec![] } else { vec![ids[i - 1]] };

            let commit = CommitObject {
                schema_id: ObjectId::from_bytes([i as u8; 32]),
                parents,
                migration_id: None,
                protocol: "test".into(),
                author: "test".into(),
                timestamp: i as u64 * 100,
                message: format!("commit {i}"),
            };
            let id = store.put(&Object::Commit(commit)).unwrap();
            ids.push(id);
        }

        (store, ids)
    }

    /// Build a diamond history:
    /// ```text
    ///   c0
    ///  / \
    /// c1  c2
    ///  \ /
    ///   c3
    /// ```
    fn build_diamond_history() -> (MemStore, Vec<ObjectId>) {
        let mut store = MemStore::new();

        let c0 = CommitObject {
            schema_id: ObjectId::from_bytes([0; 32]),
            parents: vec![],
            migration_id: None,
            protocol: "test".into(),
            author: "test".into(),
            timestamp: 100,
            message: "c0".into(),
        };
        let id0 = store.put(&Object::Commit(c0)).unwrap();

        let c1 = CommitObject {
            schema_id: ObjectId::from_bytes([1; 32]),
            parents: vec![id0],
            migration_id: None,
            protocol: "test".into(),
            author: "test".into(),
            timestamp: 200,
            message: "c1".into(),
        };
        let id1 = store.put(&Object::Commit(c1)).unwrap();

        let c2 = CommitObject {
            schema_id: ObjectId::from_bytes([2; 32]),
            parents: vec![id0],
            migration_id: None,
            protocol: "test".into(),
            author: "test".into(),
            timestamp: 300,
            message: "c2".into(),
        };
        let id2 = store.put(&Object::Commit(c2)).unwrap();

        let c3 = CommitObject {
            schema_id: ObjectId::from_bytes([3; 32]),
            parents: vec![id1, id2],
            migration_id: None,
            protocol: "test".into(),
            author: "test".into(),
            timestamp: 400,
            message: "c3".into(),
        };
        let id3 = store.put(&Object::Commit(c3)).unwrap();

        (store, vec![id0, id1, id2, id3])
    }

    #[test]
    fn merge_base_same_commit() {
        let (store, ids) = build_linear_history(3);
        assert_eq!(merge_base(&store, ids[1], ids[1]).unwrap(), Some(ids[1]));
    }

    #[test]
    fn merge_base_linear() {
        let (store, ids) = build_linear_history(5);
        // merge_base of c4 and c2 should be c2 (c2 is ancestor of c4).
        assert_eq!(merge_base(&store, ids[4], ids[2]).unwrap(), Some(ids[2]));
    }

    #[test]
    fn merge_base_diamond() {
        let (store, ids) = build_diamond_history();
        // merge_base of c1 and c2 should be c0.
        assert_eq!(merge_base(&store, ids[1], ids[2]).unwrap(), Some(ids[0]));
    }

    #[test]
    fn merge_base_disjoint() {
        let mut store = MemStore::new();
        let c1 = CommitObject {
            schema_id: ObjectId::from_bytes([1; 32]),
            parents: vec![],
            migration_id: None,
            protocol: "test".into(),
            author: "test".into(),
            timestamp: 100,
            message: "orphan1".into(),
        };
        let c2 = CommitObject {
            schema_id: ObjectId::from_bytes([2; 32]),
            parents: vec![],
            migration_id: None,
            protocol: "test".into(),
            author: "test".into(),
            timestamp: 200,
            message: "orphan2".into(),
        };
        let id1 = store.put(&Object::Commit(c1)).unwrap();
        let id2 = store.put(&Object::Commit(c2)).unwrap();
        assert_eq!(merge_base(&store, id1, id2).unwrap(), None);
    }

    #[test]
    fn find_path_linear() {
        let (store, ids) = build_linear_history(4);
        let path = find_path(&store, ids[0], ids[3]).unwrap();
        assert_eq!(path, vec![ids[0], ids[1], ids[2], ids[3]]);
    }

    #[test]
    fn find_path_same() {
        let (store, ids) = build_linear_history(1);
        let path = find_path(&store, ids[0], ids[0]).unwrap();
        assert_eq!(path, vec![ids[0]]);
    }

    #[test]
    fn log_walk_linear() {
        let (store, ids) = build_linear_history(3);
        let log = log_walk(&store, ids[2], None).unwrap();
        assert_eq!(log.len(), 3);
        // Newest first.
        assert_eq!(log[0].message, "commit 2");
        assert_eq!(log[1].message, "commit 1");
        assert_eq!(log[2].message, "commit 0");
    }

    #[test]
    fn log_walk_with_limit() {
        let (store, ids) = build_linear_history(5);
        let log = log_walk(&store, ids[4], Some(2)).unwrap();
        assert_eq!(log.len(), 2);
    }

    #[test]
    fn log_walk_diamond() {
        let (store, ids) = build_diamond_history();
        let log = log_walk(&store, ids[3], None).unwrap();
        // All 4 commits should be visited exactly once.
        assert_eq!(log.len(), 4);
    }

    #[test]
    fn is_ancestor_true() {
        let (store, ids) = build_linear_history(4);
        assert!(is_ancestor(&store, ids[0], ids[3]).unwrap());
    }

    #[test]
    fn is_ancestor_false() {
        let (store, ids) = build_linear_history(4);
        assert!(!is_ancestor(&store, ids[3], ids[0]).unwrap());
    }

    #[test]
    fn is_ancestor_self() {
        let (store, ids) = build_linear_history(1);
        assert!(is_ancestor(&store, ids[0], ids[0]).unwrap());
    }

    #[test]
    fn commit_count_linear() {
        let (store, ids) = build_linear_history(5);
        assert_eq!(commit_count(&store, ids[0], ids[4]).unwrap(), 4);
    }

    /// Build a criss-cross history:
    /// ```text
    ///     c0
    ///    / \
    ///   c1  c2
    ///   |\ /|
    ///   | X |
    ///   |/ \|
    ///   c3  c4
    /// ```
    /// c3 = merge(c1, c2), c4 = merge(c2, c1)
    /// Both c1 and c2 are LCAs of c3 and c4.
    fn build_criss_cross_history() -> (MemStore, Vec<ObjectId>) {
        let mut store = MemStore::new();

        let c0 = CommitObject {
            schema_id: ObjectId::from_bytes([0; 32]),
            parents: vec![],
            migration_id: None,
            protocol: "test".into(),
            author: "test".into(),
            timestamp: 100,
            message: "c0".into(),
        };
        let id0 = store.put(&Object::Commit(c0)).unwrap();

        let c1 = CommitObject {
            schema_id: ObjectId::from_bytes([1; 32]),
            parents: vec![id0],
            migration_id: None,
            protocol: "test".into(),
            author: "test".into(),
            timestamp: 200,
            message: "c1".into(),
        };
        let id1 = store.put(&Object::Commit(c1)).unwrap();

        let c2 = CommitObject {
            schema_id: ObjectId::from_bytes([2; 32]),
            parents: vec![id0],
            migration_id: None,
            protocol: "test".into(),
            author: "test".into(),
            timestamp: 300,
            message: "c2".into(),
        };
        let id2 = store.put(&Object::Commit(c2)).unwrap();

        // c3 = merge(c1, c2)
        let c3 = CommitObject {
            schema_id: ObjectId::from_bytes([3; 32]),
            parents: vec![id1, id2],
            migration_id: None,
            protocol: "test".into(),
            author: "test".into(),
            timestamp: 400,
            message: "c3".into(),
        };
        let id3 = store.put(&Object::Commit(c3)).unwrap();

        // c4 = merge(c2, c1)
        let c4 = CommitObject {
            schema_id: ObjectId::from_bytes([4; 32]),
            parents: vec![id2, id1],
            migration_id: None,
            protocol: "test".into(),
            author: "test".into(),
            timestamp: 500,
            message: "c4".into(),
        };
        let id4 = store.put(&Object::Commit(c4)).unwrap();

        (store, vec![id0, id1, id2, id3, id4])
    }

    #[test]
    fn merge_base_criss_cross() {
        let (store, ids) = build_criss_cross_history();
        // LCA of c3 and c4: both c1 and c2 are common ancestors.
        // c0 is also a common ancestor but it's dominated by c1 and c2.
        // The algorithm should return c1 or c2 (not c0).
        let result = merge_base(&store, ids[3], ids[4]).unwrap().unwrap();
        assert!(
            result == ids[1] || result == ids[2],
            "LCA should be c1 or c2, got {:?}",
            result,
        );
        // Should NOT return c0.
        assert_ne!(
            result, ids[0],
            "should not return c0 (dominated by c1 and c2)"
        );
    }
}
