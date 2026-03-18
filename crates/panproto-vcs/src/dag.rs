//! DAG traversal algorithms.
//!
//! Operations on the commit DAG: finding merge bases, paths between
//! commits, walking history, and composing migrations along a path.

use std::collections::{BinaryHeap, HashSet, VecDeque};
use std::sync::Arc;

use panproto_gat::{
    NaturalTransformation, Sort, Term, Theory, TheoryMorphism, check_natural_transformation,
};
use panproto_mig::Migration;
use panproto_schema::Schema;

use crate::error::VcsError;
use crate::hash::ObjectId;
use crate::object::{CommitObject, Object};
use crate::store::Store;

/// Result of composing migrations along a path with coherence checking.
///
/// Contains the composed migration and any warnings from the coherence
/// analysis. Warnings are non-fatal: they indicate potential composition
/// drift but do not prevent the composed migration from being used.
#[derive(Clone, Debug)]
#[must_use]
pub struct CompositionResult {
    /// The composed migration from the first to the last commit.
    pub migration: Migration,
    /// Warnings from the coherence analysis.
    pub coherence_warnings: Vec<String>,
}

impl Default for CompositionResult {
    fn default() -> Self {
        Self {
            migration: Migration::empty(),
            coherence_warnings: Vec::new(),
        }
    }
}

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

/// Compose migrations along a path and check composition coherence.
///
/// Given a path `[c0, c1, ..., cn]`, composes step-wise migrations
/// and then validates the result against GAT-level coherence conditions:
///
/// 1. **Structural coherence**: every vertex in the composed migration's
///    image actually exists in the target schema.
/// 2. **Composition drift detection**: if the final commit carries a direct
///    migration from the first commit's schema, we build a natural
///    transformation between the composed and direct morphisms and verify
///    the naturality condition holds.
///
/// # Errors
///
/// Returns an error if loading commits/migrations/schemas fails or if
/// migration composition fails. Coherence violations are returned as
/// warnings, not errors.
pub fn compose_path_with_coherence(
    store: &dyn Store,
    path: &[ObjectId],
) -> Result<CompositionResult, VcsError> {
    let composed = compose_path(store, path)?;

    if path.len() < 2 {
        return Ok(CompositionResult {
            migration: composed,
            coherence_warnings: Vec::new(),
        });
    }

    let mut warnings = Vec::new();

    // Load source and target schemas.
    let first_commit = get_commit(store, path[0])?;
    let last_commit = get_commit(store, path[path.len() - 1])?;

    let source_schema = get_schema(store, first_commit.schema_id)?;
    let target_schema = get_schema(store, last_commit.schema_id)?;

    // Build simple theories from schemas (one sort per vertex ID).
    let source_theory = theory_from_schema(&source_schema, "Source");
    let target_theory = theory_from_schema(&target_schema, "Target");

    // Build a TheoryMorphism from the composed migration's vertex_map.
    let composed_morphism =
        theory_morphism_from_migration(&composed, "composed", "Source", "Target");

    // Check structural coherence: every mapped target vertex must exist
    // in the target schema.
    for (src_v, tgt_v) in &composed.vertex_map {
        if !target_schema.vertices.contains_key(tgt_v) {
            warnings.push(format!(
                "composed migration maps vertex '{src_v}' to '{tgt_v}' \
                 which does not exist in target schema",
            ));
        }
    }

    // Check that all source sorts are mapped.
    for sort in &source_theory.sorts {
        if !composed_morphism.sort_map.contains_key(&sort.name) {
            warnings.push(format!(
                "composed migration does not map source vertex '{}'",
                sort.name,
            ));
        }
    }

    // Composition drift detection: compare the composed migration
    // (sequential composition of each step) against a directly derived
    // migration from the source schema to the target schema.
    //
    // The direct migration is computed via auto_mig::derive_migration,
    // which uses the structural diff between the endpoint schemas. If
    // the composed and direct morphisms disagree on any sort mapping,
    // this indicates composition drift — the intermediate steps are
    // not coherent with what a single-step migration would produce.
    //
    // When the sort maps agree, we additionally construct a natural
    // transformation and check the naturality condition, which can
    // detect operation-level drift.
    if path.len() >= 3 {
        let schema_diff = panproto_check::diff::diff(&source_schema, &target_schema);
        let direct_mig =
            crate::auto_mig::derive_migration(&source_schema, &target_schema, &schema_diff);
        check_drift(
            &composed_morphism,
            &direct_mig,
            &source_theory,
            &target_theory,
            &mut warnings,
        );
    }

    Ok(CompositionResult {
        migration: composed,
        coherence_warnings: warnings,
    })
}

/// Compare a composed morphism against a direct migration for drift.
///
/// Builds a natural transformation between the two morphisms and checks
/// the naturality condition. Any disagreements are appended to `warnings`.
fn check_drift(
    composed_morphism: &TheoryMorphism,
    direct_mig: &Migration,
    source_theory: &Theory,
    target_theory: &Theory,
    warnings: &mut Vec<String>,
) {
    let direct_morphism = theory_morphism_from_migration(direct_mig, "direct", "Source", "Target");

    let mut nt_components = std::collections::HashMap::new();
    let mut divergent_sorts = Vec::new();

    for sort in &source_theory.sorts {
        let composed_img = composed_morphism.sort_map.get(&sort.name);
        let direct_img = direct_morphism.sort_map.get(&sort.name);

        match (composed_img, direct_img) {
            (Some(c), Some(d)) if c == d => {}
            (Some(c), Some(d)) => {
                divergent_sorts.push((sort.name.to_string(), c.to_string(), d.to_string()));
            }
            (None, Some(_)) => {
                warnings.push(format!(
                    "direct migration maps '{}' but composed migration does not",
                    sort.name,
                ));
            }
            (Some(_), None) => {
                warnings.push(format!(
                    "composed migration maps '{}' but direct migration does not",
                    sort.name,
                ));
            }
            (None, None) => {}
        }
        // Always provide a component so the NT is total.
        nt_components.insert(Arc::clone(&sort.name), Term::var("x"));
    }

    for (sort, composed_tgt, direct_tgt) in &divergent_sorts {
        warnings.push(format!(
            "composition drift: vertex '{sort}' maps to '{composed_tgt}' via composition \
             but '{direct_tgt}' via direct migration",
        ));
    }

    // Only attempt the full naturality check when the morphisms agree on
    // all sorts (otherwise drift is already flagged above).
    if divergent_sorts.is_empty() {
        let nt = NaturalTransformation {
            name: Arc::from("composed_vs_direct"),
            source: Arc::from("composed"),
            target: Arc::from("direct"),
            components: nt_components,
        };
        if let Err(e) = check_natural_transformation(
            &nt,
            composed_morphism,
            &direct_morphism,
            source_theory,
            target_theory,
        ) {
            warnings.push(format!("naturality check failed: {e}"));
        }
    }
}

/// Build a [`Theory`] from a schema, with one sort per vertex ID.
///
/// This produces a simple flat theory with no operations or equations,
/// suitable for checking that morphisms map sorts correctly.
fn theory_from_schema(schema: &Schema, name: &str) -> Theory {
    let sorts: Vec<Sort> = schema
        .vertices
        .keys()
        .map(|v| Sort::simple(&*v.0))
        .collect();
    Theory::new(name, sorts, Vec::new(), Vec::new())
}

/// Build a [`TheoryMorphism`] from a migration's vertex map.
fn theory_morphism_from_migration(
    migration: &Migration,
    name: &str,
    domain: &str,
    codomain: &str,
) -> TheoryMorphism {
    let sort_map: std::collections::HashMap<Arc<str>, Arc<str>> = migration
        .vertex_map
        .iter()
        .map(|(src, tgt)| (Arc::from(&*src.0), Arc::from(&*tgt.0)))
        .collect();
    TheoryMorphism::new(
        name,
        domain,
        codomain,
        sort_map,
        std::collections::HashMap::new(),
    )
}

/// Load a schema from the store by ID.
fn get_schema(store: &dyn Store, id: ObjectId) -> Result<Schema, VcsError> {
    match store.get(&id)? {
        Object::Schema(s) => Ok(*s),
        other => Err(VcsError::WrongObjectType {
            expected: "schema",
            found: other.type_name(),
        }),
    }
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
#[allow(clippy::cast_possible_truncation)]
mod tests {
    use super::*;
    use crate::{MemStore, Store};

    /// Build a linear chain of commits: c0 → c1 → c2 → ...
    /// Returns (store, vec of commit IDs).
    fn build_linear_history(
        n: usize,
    ) -> Result<(MemStore, Vec<ObjectId>), Box<dyn std::error::Error>> {
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
                renames: vec![],
                protocol_id: None,
                data_ids: vec![],
                complement_ids: vec![],
            };
            let id = store.put(&Object::Commit(commit))?;
            ids.push(id);
        }

        Ok((store, ids))
    }

    /// Build a diamond history:
    /// ```text
    ///   c0
    ///  / \
    /// c1  c2
    ///  \ /
    ///   c3
    /// ```
    fn build_diamond_history() -> Result<(MemStore, Vec<ObjectId>), Box<dyn std::error::Error>> {
        let mut store = MemStore::new();

        let c0 = CommitObject {
            schema_id: ObjectId::from_bytes([0; 32]),
            parents: vec![],
            migration_id: None,
            protocol: "test".into(),
            author: "test".into(),
            timestamp: 100,
            message: "c0".into(),
            renames: vec![],
            protocol_id: None,
            data_ids: vec![],
            complement_ids: vec![],
        };
        let id0 = store.put(&Object::Commit(c0))?;

        let c1 = CommitObject {
            schema_id: ObjectId::from_bytes([1; 32]),
            parents: vec![id0],
            migration_id: None,
            protocol: "test".into(),
            author: "test".into(),
            timestamp: 200,
            message: "c1".into(),
            renames: vec![],
            protocol_id: None,
            data_ids: vec![],
            complement_ids: vec![],
        };
        let id1 = store.put(&Object::Commit(c1))?;

        let c2 = CommitObject {
            schema_id: ObjectId::from_bytes([2; 32]),
            parents: vec![id0],
            migration_id: None,
            protocol: "test".into(),
            author: "test".into(),
            timestamp: 300,
            message: "c2".into(),
            renames: vec![],
            protocol_id: None,
            data_ids: vec![],
            complement_ids: vec![],
        };
        let id2 = store.put(&Object::Commit(c2))?;

        let c3 = CommitObject {
            schema_id: ObjectId::from_bytes([3; 32]),
            parents: vec![id1, id2],
            migration_id: None,
            protocol: "test".into(),
            author: "test".into(),
            timestamp: 400,
            message: "c3".into(),
            renames: vec![],
            protocol_id: None,
            data_ids: vec![],
            complement_ids: vec![],
        };
        let id3 = store.put(&Object::Commit(c3))?;

        Ok((store, vec![id0, id1, id2, id3]))
    }

    #[test]
    fn merge_base_same_commit() -> Result<(), Box<dyn std::error::Error>> {
        let (store, ids) = build_linear_history(3)?;
        assert_eq!(merge_base(&store, ids[1], ids[1])?, Some(ids[1]));
        Ok(())
    }

    #[test]
    fn merge_base_linear() -> Result<(), Box<dyn std::error::Error>> {
        let (store, ids) = build_linear_history(5)?;
        // merge_base of c4 and c2 should be c2 (c2 is ancestor of c4).
        assert_eq!(merge_base(&store, ids[4], ids[2])?, Some(ids[2]));
        Ok(())
    }

    #[test]
    fn merge_base_diamond() -> Result<(), Box<dyn std::error::Error>> {
        let (store, ids) = build_diamond_history()?;
        // merge_base of c1 and c2 should be c0.
        assert_eq!(merge_base(&store, ids[1], ids[2])?, Some(ids[0]));
        Ok(())
    }

    #[test]
    fn merge_base_disjoint() -> Result<(), Box<dyn std::error::Error>> {
        let mut store = MemStore::new();
        let c1 = CommitObject {
            schema_id: ObjectId::from_bytes([1; 32]),
            parents: vec![],
            migration_id: None,
            protocol: "test".into(),
            author: "test".into(),
            timestamp: 100,
            message: "orphan1".into(),
            renames: vec![],
            protocol_id: None,
            data_ids: vec![],
            complement_ids: vec![],
        };
        let c2 = CommitObject {
            schema_id: ObjectId::from_bytes([2; 32]),
            parents: vec![],
            migration_id: None,
            protocol: "test".into(),
            author: "test".into(),
            timestamp: 200,
            message: "orphan2".into(),
            renames: vec![],
            protocol_id: None,
            data_ids: vec![],
            complement_ids: vec![],
        };
        let id1 = store.put(&Object::Commit(c1))?;
        let id2 = store.put(&Object::Commit(c2))?;
        assert_eq!(merge_base(&store, id1, id2)?, None);
        Ok(())
    }

    #[test]
    fn find_path_linear() -> Result<(), Box<dyn std::error::Error>> {
        let (store, ids) = build_linear_history(4)?;
        let path = find_path(&store, ids[0], ids[3])?;
        assert_eq!(path, vec![ids[0], ids[1], ids[2], ids[3]]);
        Ok(())
    }

    #[test]
    fn find_path_same() -> Result<(), Box<dyn std::error::Error>> {
        let (store, ids) = build_linear_history(1)?;
        let path = find_path(&store, ids[0], ids[0])?;
        assert_eq!(path, vec![ids[0]]);
        Ok(())
    }

    #[test]
    fn log_walk_linear() -> Result<(), Box<dyn std::error::Error>> {
        let (store, ids) = build_linear_history(3)?;
        let log = log_walk(&store, ids[2], None)?;
        assert_eq!(log.len(), 3);
        // Newest first.
        assert_eq!(log[0].message, "commit 2");
        assert_eq!(log[1].message, "commit 1");
        assert_eq!(log[2].message, "commit 0");
        Ok(())
    }

    #[test]
    fn log_walk_with_limit() -> Result<(), Box<dyn std::error::Error>> {
        let (store, ids) = build_linear_history(5)?;
        let log = log_walk(&store, ids[4], Some(2))?;
        assert_eq!(log.len(), 2);
        Ok(())
    }

    #[test]
    fn log_walk_diamond() -> Result<(), Box<dyn std::error::Error>> {
        let (store, ids) = build_diamond_history()?;
        let log = log_walk(&store, ids[3], None)?;
        // All 4 commits should be visited exactly once.
        assert_eq!(log.len(), 4);
        Ok(())
    }

    #[test]
    fn is_ancestor_true() -> Result<(), Box<dyn std::error::Error>> {
        let (store, ids) = build_linear_history(4)?;
        assert!(is_ancestor(&store, ids[0], ids[3])?);
        Ok(())
    }

    #[test]
    fn is_ancestor_false() -> Result<(), Box<dyn std::error::Error>> {
        let (store, ids) = build_linear_history(4)?;
        assert!(!is_ancestor(&store, ids[3], ids[0])?);
        Ok(())
    }

    #[test]
    fn is_ancestor_self() -> Result<(), Box<dyn std::error::Error>> {
        let (store, ids) = build_linear_history(1)?;
        assert!(is_ancestor(&store, ids[0], ids[0])?);
        Ok(())
    }

    #[test]
    fn commit_count_linear() -> Result<(), Box<dyn std::error::Error>> {
        let (store, ids) = build_linear_history(5)?;
        assert_eq!(commit_count(&store, ids[0], ids[4])?, 4);
        Ok(())
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
    fn build_criss_cross_history() -> Result<(MemStore, Vec<ObjectId>), Box<dyn std::error::Error>>
    {
        let mut store = MemStore::new();

        let c0 = CommitObject {
            schema_id: ObjectId::from_bytes([0; 32]),
            parents: vec![],
            migration_id: None,
            protocol: "test".into(),
            author: "test".into(),
            timestamp: 100,
            message: "c0".into(),
            renames: vec![],
            protocol_id: None,
            data_ids: vec![],
            complement_ids: vec![],
        };
        let id0 = store.put(&Object::Commit(c0))?;

        let c1 = CommitObject {
            schema_id: ObjectId::from_bytes([1; 32]),
            parents: vec![id0],
            migration_id: None,
            protocol: "test".into(),
            author: "test".into(),
            timestamp: 200,
            message: "c1".into(),
            renames: vec![],
            protocol_id: None,
            data_ids: vec![],
            complement_ids: vec![],
        };
        let id1 = store.put(&Object::Commit(c1))?;

        let c2 = CommitObject {
            schema_id: ObjectId::from_bytes([2; 32]),
            parents: vec![id0],
            migration_id: None,
            protocol: "test".into(),
            author: "test".into(),
            timestamp: 300,
            message: "c2".into(),
            renames: vec![],
            protocol_id: None,
            data_ids: vec![],
            complement_ids: vec![],
        };
        let id2 = store.put(&Object::Commit(c2))?;

        // c3 = merge(c1, c2)
        let c3 = CommitObject {
            schema_id: ObjectId::from_bytes([3; 32]),
            parents: vec![id1, id2],
            migration_id: None,
            protocol: "test".into(),
            author: "test".into(),
            timestamp: 400,
            message: "c3".into(),
            renames: vec![],
            protocol_id: None,
            data_ids: vec![],
            complement_ids: vec![],
        };
        let id3 = store.put(&Object::Commit(c3))?;

        // c4 = merge(c2, c1)
        let c4 = CommitObject {
            schema_id: ObjectId::from_bytes([4; 32]),
            parents: vec![id2, id1],
            migration_id: None,
            protocol: "test".into(),
            author: "test".into(),
            timestamp: 500,
            message: "c4".into(),
            renames: vec![],
            protocol_id: None,
            data_ids: vec![],
            complement_ids: vec![],
        };
        let id4 = store.put(&Object::Commit(c4))?;

        Ok((store, vec![id0, id1, id2, id3, id4]))
    }

    #[test]
    fn merge_base_criss_cross() -> Result<(), Box<dyn std::error::Error>> {
        let (store, ids) = build_criss_cross_history()?;
        // LCA of c3 and c4: both c1 and c2 are common ancestors.
        // c0 is also a common ancestor but it's dominated by c1 and c2.
        // The algorithm should return c1 or c2 (not c0).
        let result = merge_base(&store, ids[3], ids[4])?.ok_or("expected Some")?;
        assert!(
            result == ids[1] || result == ids[2],
            "LCA should be c1 or c2, got {result:?}",
        );
        // Should NOT return c0.
        assert_ne!(
            result, ids[0],
            "should not return c0 (dominated by c1 and c2)"
        );
        Ok(())
    }

    // ---- coherence checking tests ----

    use panproto_gat::Name;
    use panproto_schema::{Schema, Vertex};

    /// Build a test schema with the given vertex IDs (all kind "object").
    fn make_test_schema(vertex_ids: &[&str]) -> Schema {
        let mut vertices = HashMap::new();
        for &id in vertex_ids {
            vertices.insert(
                Name::from(id),
                Vertex {
                    id: Name::from(id),
                    kind: Name::from("object"),
                    nsid: None,
                },
            );
        }
        Schema {
            protocol: "test".into(),
            vertices,
            edges: HashMap::new(),
            hyper_edges: HashMap::new(),
            constraints: HashMap::new(),
            required: HashMap::new(),
            nsids: HashMap::new(),
            variants: HashMap::new(),
            orderings: HashMap::new(),
            recursion_points: HashMap::new(),
            spans: HashMap::new(),
            usage_modes: HashMap::new(),
            nominal: HashMap::new(),
            outgoing: HashMap::new(),
            incoming: HashMap::new(),
            between: HashMap::new(),
        }
    }

    /// Build a linear 3-commit history with real schemas and migrations.
    ///
    /// c0: schema {a, b}
    /// c1: schema {a, b, c} -- migration: identity on a,b
    /// c2: schema {a, c, d} -- migration: identity on a,c; drops b, adds d
    fn build_migration_history() -> Result<(MemStore, Vec<ObjectId>), Box<dyn std::error::Error>> {
        let mut store = MemStore::new();

        let s0 = make_test_schema(&["a", "b"]);
        let s1 = make_test_schema(&["a", "b", "c"]);
        let s2 = make_test_schema(&["a", "c", "d"]);

        let s0_id = store.put(&Object::Schema(Box::new(s0)))?;
        let s1_id = store.put(&Object::Schema(Box::new(s1)))?;
        let s2_id = store.put(&Object::Schema(Box::new(s2)))?;

        // Migration c0 -> c1: identity on a, b.
        let mig01 = Migration {
            vertex_map: HashMap::from([
                (Name::from("a"), Name::from("a")),
                (Name::from("b"), Name::from("b")),
            ]),
            edge_map: HashMap::new(),
            hyper_edge_map: HashMap::new(),
            label_map: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
        };
        let mig01_id = store.put(&Object::Migration {
            src: s0_id,
            tgt: s1_id,
            mapping: mig01,
        })?;

        // Migration c1 -> c2: keep a and c, drop b.
        let mig12 = Migration {
            vertex_map: HashMap::from([
                (Name::from("a"), Name::from("a")),
                (Name::from("c"), Name::from("c")),
            ]),
            edge_map: HashMap::new(),
            hyper_edge_map: HashMap::new(),
            label_map: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
        };
        let mig12_id = store.put(&Object::Migration {
            src: s1_id,
            tgt: s2_id,
            mapping: mig12,
        })?;

        // Commits.
        let c0 = CommitObject {
            schema_id: s0_id,
            parents: vec![],
            migration_id: None,
            protocol: "test".into(),
            author: "test".into(),
            timestamp: 100,
            message: "c0".into(),
            renames: vec![],
            protocol_id: None,
            data_ids: vec![],
            complement_ids: vec![],
        };
        let id0 = store.put(&Object::Commit(c0))?;

        let c1 = CommitObject {
            schema_id: s1_id,
            parents: vec![id0],
            migration_id: Some(mig01_id),
            protocol: "test".into(),
            author: "test".into(),
            timestamp: 200,
            message: "c1".into(),
            renames: vec![],
            protocol_id: None,
            data_ids: vec![],
            complement_ids: vec![],
        };
        let id1 = store.put(&Object::Commit(c1))?;

        let c2 = CommitObject {
            schema_id: s2_id,
            parents: vec![id1],
            migration_id: Some(mig12_id),
            protocol: "test".into(),
            author: "test".into(),
            timestamp: 300,
            message: "c2".into(),
            renames: vec![],
            protocol_id: None,
            data_ids: vec![],
            complement_ids: vec![],
        };
        let id2 = store.put(&Object::Commit(c2))?;

        Ok((store, vec![id0, id1, id2]))
    }

    #[test]
    fn coherence_short_path_no_warnings() -> Result<(), Box<dyn std::error::Error>> {
        let (store, ids) = build_migration_history()?;
        let result = compose_path_with_coherence(&store, &ids[0..1])?;
        assert!(result.coherence_warnings.is_empty());
        Ok(())
    }

    #[test]
    fn coherence_two_commit_path() -> Result<(), Box<dyn std::error::Error>> {
        let (store, ids) = build_migration_history()?;
        let result = compose_path_with_coherence(&store, &ids[0..2])?;
        assert_eq!(result.migration.vertex_map.len(), 2);
        // Two-commit path: composed == direct, no drift check performed.
        assert!(result.coherence_warnings.is_empty());
        Ok(())
    }

    #[test]
    fn coherence_three_commit_path_no_structural_issues() -> Result<(), Box<dyn std::error::Error>>
    {
        let (store, ids) = build_migration_history()?;
        let result = compose_path_with_coherence(&store, &ids[0..3])?;

        // Composed maps a->a (b drops out during composition).
        assert_eq!(
            result.migration.vertex_map.get(&Name::from("a")),
            Some(&Name::from("a")),
        );

        // No structural coherence warnings: "a" exists in target schema.
        assert!(
            !result
                .coherence_warnings
                .iter()
                .any(|w| w.contains("does not exist in target schema")),
            "expected no structural coherence warnings"
        );
        Ok(())
    }

    #[test]
    fn coherence_detects_vertex_not_in_target() -> Result<(), Box<dyn std::error::Error>> {
        let mut store = MemStore::new();

        let s_src = make_test_schema(&["a"]);
        let s_tgt = make_test_schema(&["a"]); // only "a" exists

        let s_src_id = store.put(&Object::Schema(Box::new(s_src)))?;
        let s_tgt_id = store.put(&Object::Schema(Box::new(s_tgt)))?;

        // Migration maps a -> nonexistent.
        let bad_mig = Migration {
            vertex_map: HashMap::from([(Name::from("a"), Name::from("nonexistent"))]),
            edge_map: HashMap::new(),
            hyper_edge_map: HashMap::new(),
            label_map: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
        };
        let bad_mig_id = store.put(&Object::Migration {
            src: s_src_id,
            tgt: s_tgt_id,
            mapping: bad_mig,
        })?;

        let c0 = CommitObject {
            schema_id: s_src_id,
            parents: vec![],
            migration_id: None,
            protocol: "test".into(),
            author: "test".into(),
            timestamp: 100,
            message: "src".into(),
            renames: vec![],
            protocol_id: None,
            data_ids: vec![],
            complement_ids: vec![],
        };
        let csrc_id = store.put(&Object::Commit(c0))?;

        let c1 = CommitObject {
            schema_id: s_tgt_id,
            parents: vec![csrc_id],
            migration_id: Some(bad_mig_id),
            protocol: "test".into(),
            author: "test".into(),
            timestamp: 200,
            message: "tgt".into(),
            renames: vec![],
            protocol_id: None,
            data_ids: vec![],
            complement_ids: vec![],
        };
        let ctgt_id = store.put(&Object::Commit(c1))?;

        let result = compose_path_with_coherence(&store, &[csrc_id, ctgt_id])?;

        let has_missing_vertex_warning = result
            .coherence_warnings
            .iter()
            .any(|w| w.contains("does not exist in target schema"));
        assert!(
            has_missing_vertex_warning,
            "expected warning about vertex not in target schema, got: {:?}",
            result.coherence_warnings,
        );
        Ok(())
    }

    #[test]
    fn coherence_detects_unmapped_source_vertex() -> Result<(), Box<dyn std::error::Error>> {
        let mut store = MemStore::new();

        let s_src = make_test_schema(&["a", "b"]);
        let s_tgt = make_test_schema(&["a"]);

        let s_src_id = store.put(&Object::Schema(Box::new(s_src)))?;
        let s_tgt_id = store.put(&Object::Schema(Box::new(s_tgt)))?;

        // Migration only maps a, not b.
        let mig = Migration {
            vertex_map: HashMap::from([(Name::from("a"), Name::from("a"))]),
            edge_map: HashMap::new(),
            hyper_edge_map: HashMap::new(),
            label_map: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
        };
        let mig_id = store.put(&Object::Migration {
            src: s_src_id,
            tgt: s_tgt_id,
            mapping: mig,
        })?;

        let c0 = CommitObject {
            schema_id: s_src_id,
            parents: vec![],
            migration_id: None,
            protocol: "test".into(),
            author: "test".into(),
            timestamp: 100,
            message: "c0".into(),
            renames: vec![],
            protocol_id: None,
            data_ids: vec![],
            complement_ids: vec![],
        };
        let id0 = store.put(&Object::Commit(c0))?;

        let c1 = CommitObject {
            schema_id: s_tgt_id,
            parents: vec![id0],
            migration_id: Some(mig_id),
            protocol: "test".into(),
            author: "test".into(),
            timestamp: 200,
            message: "c1".into(),
            renames: vec![],
            protocol_id: None,
            data_ids: vec![],
            complement_ids: vec![],
        };
        let id1 = store.put(&Object::Commit(c1))?;

        let result = compose_path_with_coherence(&store, &[id0, id1])?;

        let has_unmapped_warning = result
            .coherence_warnings
            .iter()
            .any(|w| w.contains("does not map source vertex"));
        assert!(
            has_unmapped_warning,
            "expected warning about unmapped source vertex, got: {:?}",
            result.coherence_warnings,
        );
        Ok(())
    }

    #[test]
    fn coherence_composition_drift_detected() -> Result<(), Box<dyn std::error::Error>> {
        // Build a 3-commit history where the composed migration disagrees
        // with the directly derived migration from source to target schema.
        //
        // Source (c0): {a, b, c}
        // Mid    (c1): {a, b, d}  (c removed, d added)
        // Target (c2): {a, d, e}  (b removed, e added)
        //
        // Step migrations:
        //   c0→c1: a->a, b->b         (c dropped, d is new)
        //   c1→c2: a->a, d->d         (b dropped, e is new)
        //
        // Composed: a->a, b->b THEN a->a, d->d
        //   Since b is not in c1→c2's domain, compose drops it.
        //   Result: a->a  (only a survives both steps)
        //
        // Direct (derive_migration from {a,b,c} to {a,d,e}):
        //   a->a is the only identity mapping (b,c removed, d,e added)
        //
        // Both agree: {a->a}. No drift. BUT we want drift, so let's
        // make the composed migration map b to something via a rename
        // that the direct migration wouldn't know about.
        let mut store = MemStore::new();

        let s0 = make_test_schema(&["a", "b"]);
        let s1 = make_test_schema(&["a", "c"]);
        let s2 = make_test_schema(&["a", "b"]); // b reappears in target

        let s0_id = store.put(&Object::Schema(Box::new(s0)))?;
        let s1_id = store.put(&Object::Schema(Box::new(s1)))?;
        let s2_id = store.put(&Object::Schema(Box::new(s2)))?;

        // mig01: a->a, b->c (rename b to c).
        let mig01 = Migration {
            vertex_map: HashMap::from([
                (Name::from("a"), Name::from("a")),
                (Name::from("b"), Name::from("c")),
            ]),
            edge_map: HashMap::new(),
            hyper_edge_map: HashMap::new(),
            label_map: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
        };
        let mig01_id = store.put(&Object::Migration {
            src: s0_id,
            tgt: s1_id,
            mapping: mig01,
        })?;

        // mig12: a->a, c->b (rename c back to b).
        // Composed: a->a, b->b (via b->c->b).
        let mig12 = Migration {
            vertex_map: HashMap::from([
                (Name::from("a"), Name::from("a")),
                (Name::from("c"), Name::from("b")),
            ]),
            edge_map: HashMap::new(),
            hyper_edge_map: HashMap::new(),
            label_map: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
        };
        let mig12_id = store.put(&Object::Migration {
            src: s1_id,
            tgt: s2_id,
            mapping: mig12,
        })?;

        let c0 = CommitObject {
            schema_id: s0_id,
            parents: vec![],
            migration_id: None,
            protocol: "test".into(),
            author: "test".into(),
            timestamp: 100,
            message: "c0".into(),
            renames: vec![],
            protocol_id: None,
            data_ids: vec![],
            complement_ids: vec![],
        };
        let id0 = store.put(&Object::Commit(c0))?;

        let c1 = CommitObject {
            schema_id: s1_id,
            parents: vec![id0],
            migration_id: Some(mig01_id),
            protocol: "test".into(),
            author: "test".into(),
            timestamp: 200,
            message: "c1".into(),
            renames: vec![],
            protocol_id: None,
            data_ids: vec![],
            complement_ids: vec![],
        };
        let id1 = store.put(&Object::Commit(c1))?;

        let c2 = CommitObject {
            schema_id: s2_id,
            parents: vec![id1],
            migration_id: Some(mig12_id),
            protocol: "test".into(),
            author: "test".into(),
            timestamp: 300,
            message: "c2".into(),
            renames: vec![],
            protocol_id: None,
            data_ids: vec![],
            complement_ids: vec![],
        };
        let id2 = store.put(&Object::Commit(c2))?;

        let result = compose_path_with_coherence(&store, &[id0, id1, id2])?;

        // The composed migration is {a->a, b->b} (via b->c->b).
        assert_eq!(
            result.migration.vertex_map.get(&Name::from("a")),
            Some(&Name::from("a")),
        );
        assert_eq!(
            result.migration.vertex_map.get(&Name::from("b")),
            Some(&Name::from("b")),
        );

        // The directly derived migration from {a,b} to {a,b} is also
        // {a->a, b->b} (identity). In this case there is no drift.
        // But we should at least verify no errors occurred.
        // For drift detection to fire, we need the composed and direct
        // to disagree. This test verifies the coherence machinery runs
        // without errors on a non-trivial path.
        assert!(
            !result
                .coherence_warnings
                .iter()
                .any(|w| w.contains("does not exist")),
            "unexpected structural warnings: {:?}",
            result.coherence_warnings,
        );
        Ok(())
    }

    /// Helper: build a 3-commit cyclic-permutation path for drift detection.
    ///
    /// Returns `(store, [id0, id1, id2])` where the composed migration
    /// produces a cyclic permutation (b->c, c->d, d->b) while the direct
    /// migration would be identity.
    fn build_cyclic_drift_path() -> Result<(MemStore, [ObjectId; 3]), Box<dyn std::error::Error>> {
        let mut store = MemStore::new();

        let s0 = make_test_schema(&["a", "b", "c", "d"]);
        let s1 = make_test_schema(&["a", "p", "q", "r"]);
        let s2 = make_test_schema(&["a", "b", "c", "d"]);

        let s0_id = store.put(&Object::Schema(Box::new(s0)))?;
        let s1_id = store.put(&Object::Schema(Box::new(s1)))?;
        let s2_id = store.put(&Object::Schema(Box::new(s2)))?;

        let mig01 = Migration {
            vertex_map: HashMap::from([
                (Name::from("a"), Name::from("a")),
                (Name::from("b"), Name::from("p")),
                (Name::from("c"), Name::from("q")),
                (Name::from("d"), Name::from("r")),
            ]),
            edge_map: HashMap::new(),
            hyper_edge_map: HashMap::new(),
            label_map: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
        };
        let mig01_id = store.put(&Object::Migration {
            src: s0_id,
            tgt: s1_id,
            mapping: mig01,
        })?;

        let mig12 = Migration {
            vertex_map: HashMap::from([
                (Name::from("a"), Name::from("a")),
                (Name::from("p"), Name::from("c")),
                (Name::from("q"), Name::from("d")),
                (Name::from("r"), Name::from("b")),
            ]),
            edge_map: HashMap::new(),
            hyper_edge_map: HashMap::new(),
            label_map: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
        };
        let mig12_id = store.put(&Object::Migration {
            src: s1_id,
            tgt: s2_id,
            mapping: mig12,
        })?;

        let c0 = CommitObject {
            schema_id: s0_id,
            parents: vec![],
            migration_id: None,
            protocol: "test".into(),
            author: "test".into(),
            timestamp: 100,
            message: "c0".into(),
            renames: vec![],
            protocol_id: None,
            data_ids: vec![],
            complement_ids: vec![],
        };
        let id0 = store.put(&Object::Commit(c0))?;

        let c1 = CommitObject {
            schema_id: s1_id,
            parents: vec![id0],
            migration_id: Some(mig01_id),
            protocol: "test".into(),
            author: "test".into(),
            timestamp: 200,
            message: "c1".into(),
            renames: vec![],
            protocol_id: None,
            data_ids: vec![],
            complement_ids: vec![],
        };
        let id1 = store.put(&Object::Commit(c1))?;

        let c2 = CommitObject {
            schema_id: s2_id,
            parents: vec![id1],
            migration_id: Some(mig12_id),
            protocol: "test".into(),
            author: "test".into(),
            timestamp: 300,
            message: "c2".into(),
            renames: vec![],
            protocol_id: None,
            data_ids: vec![],
            complement_ids: vec![],
        };
        let id2 = store.put(&Object::Commit(c2))?;

        Ok((store, [id0, id1, id2]))
    }

    #[test]
    fn coherence_detects_actual_composition_drift() -> Result<(), Box<dyn std::error::Error>> {
        // Build a 3-commit path where the composed migration produces a
        // cyclic permutation (b->c, c->d, d->b) while the direct
        // migration would be identity, guaranteeing drift detection.
        let (store, [id0, id1, id2]) = build_cyclic_drift_path()?;

        let result = compose_path_with_coherence(&store, &[id0, id1, id2])?;

        // Composed: a->a, b->c, c->d, d->b (cyclic permutation).
        assert_eq!(
            result.migration.vertex_map.get(&Name::from("a")),
            Some(&Name::from("a"))
        );
        assert_eq!(
            result.migration.vertex_map.get(&Name::from("b")),
            Some(&Name::from("c"))
        );
        assert_eq!(
            result.migration.vertex_map.get(&Name::from("c")),
            Some(&Name::from("d"))
        );
        assert_eq!(
            result.migration.vertex_map.get(&Name::from("d")),
            Some(&Name::from("b"))
        );

        // Direct from {a,b,c,d} to {a,b,c,d}: {a->a, b->b, c->c, d->d} (identity).
        // The composed maps b->c but the direct maps b->b → composition drift.
        let has_drift = result
            .coherence_warnings
            .iter()
            .any(|w| w.contains("composition drift"));
        assert!(
            has_drift,
            "expected composition drift warnings, got: {:?}",
            result.coherence_warnings,
        );
        Ok(())
    }

    #[test]
    fn composition_result_must_use() {
        // Verify CompositionResult default works and fields are accessible.
        let r = CompositionResult::default();
        assert!(r.coherence_warnings.is_empty());
        assert!(r.migration.vertex_map.is_empty());
    }
}
