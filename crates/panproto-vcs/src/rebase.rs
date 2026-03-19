//! Rebase: replay commits onto a new base.

use std::time::{SystemTime, UNIX_EPOCH};

use crate::ObjectId;
use crate::cherry_pick::advance_head;
use crate::dag;
use crate::error::VcsError;
use crate::merge;
use crate::object::{CommitObject, Object};
use crate::store::{self, Store};

/// Rebase the current branch onto `onto`.
///
/// Replays all commits from `merge_base(HEAD, onto)..HEAD` onto `onto`,
/// creating new commits with `onto` as the base.
///
/// # Algorithm
///
/// 1. Find merge base of HEAD and `onto`.
/// 2. Collect commits from `merge_base` to HEAD (exclusive of `merge_base`).
/// 3. Move HEAD to `onto`.
/// 4. For each commit in order: three-way merge (base = parent's schema,
///    ours = current tip, theirs = commit's schema). Create new commit.
///
/// # Errors
///
/// Returns an error if any step produces conflicts, or if no merge base
/// is found.
pub fn rebase(store: &mut dyn Store, onto: ObjectId, author: &str) -> Result<ObjectId, VcsError> {
    let head_id = store::resolve_head(store)?.ok_or_else(|| VcsError::RefNotFound {
        name: "HEAD".to_owned(),
    })?;

    // Find merge base.
    let base_id = dag::merge_base(store, head_id, onto)?.ok_or(VcsError::NoCommonAncestor)?;

    // Collect commits to replay (from merge_base to HEAD, exclusive of base).
    let path = dag::find_path(store, base_id, head_id)?;
    // path[0] = base_id, path[1..] = commits to replay.
    let commits_to_replay: Vec<ObjectId> = path.into_iter().skip(1).collect();

    if commits_to_replay.is_empty() {
        // Nothing to rebase — HEAD is already at or before onto.
        return Ok(head_id);
    }

    // Move HEAD to onto.
    let old_head = head_id;
    let mut current_tip = onto;

    // Update the branch ref to onto temporarily.
    match store.get_head()? {
        crate::HeadState::Branch(name) => {
            let ref_name = format!("refs/heads/{name}");
            store.set_ref(&ref_name, onto)?;
        }
        crate::HeadState::Detached(_) => {
            store.set_head(crate::HeadState::Detached(onto))?;
        }
    }

    // Replay each commit.
    for commit_id in &commits_to_replay {
        current_tip = replay_one(store, *commit_id, current_tip, author)?;
    }

    // Append reflog.
    advance_head(
        store,
        old_head,
        current_tip,
        author,
        &format!("rebase onto {}", onto.short()),
    )?;

    Ok(current_tip)
}

/// Replay a single commit on top of `current_tip`, returning the new commit ID.
fn replay_one(
    store: &mut dyn Store,
    commit_id: ObjectId,
    current_tip: ObjectId,
    author: &str,
) -> Result<ObjectId, VcsError> {
    let commit = load_commit(store, commit_id)?;
    let parent_id = commit.parents.first().ok_or(VcsError::NoPath)?;
    let parent_commit = load_commit(store, *parent_id)?;

    let base_schema = load_schema(store, parent_commit.schema_id)?;
    let theirs_schema = load_schema(store, commit.schema_id)?;
    let tip_commit = load_commit(store, current_tip)?;
    let ours_schema = load_schema(store, tip_commit.schema_id)?;

    let result = merge::three_way_merge(&base_schema, &ours_schema, &theirs_schema);
    if !result.conflicts.is_empty() {
        return Err(VcsError::MergeConflicts {
            count: result.conflicts.len(),
        });
    }

    let merged_schema_id = store.put(&Object::Schema(Box::new(result.merged_schema)))?;
    let migration_id = store.put(&Object::Migration {
        src: tip_commit.schema_id,
        tgt: merged_schema_id,
        mapping: result.migration_from_ours,
    })?;

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let new_commit = CommitObject {
        schema_id: merged_schema_id,
        parents: vec![current_tip],
        migration_id: Some(migration_id),
        protocol: commit.protocol.clone(),
        author: author.to_owned(),
        timestamp,
        message: commit.message.clone(),
        renames: vec![],
        protocol_id: None,
        data_ids: vec![],
        complement_ids: vec![],
    };
    let new_commit_id = store.put(&Object::Commit(new_commit))?;

    // Update branch ref.
    match store.get_head()? {
        crate::HeadState::Branch(name) => {
            let ref_name = format!("refs/heads/{name}");
            store.set_ref(&ref_name, new_commit_id)?;
        }
        crate::HeadState::Detached(_) => {
            store.set_head(crate::HeadState::Detached(new_commit_id))?;
        }
    }

    Ok(new_commit_id)
}

fn load_commit(store: &dyn Store, id: ObjectId) -> Result<CommitObject, VcsError> {
    match store.get(&id)? {
        Object::Commit(c) => Ok(c),
        other => Err(VcsError::WrongObjectType {
            expected: "commit",
            found: other.type_name(),
        }),
    }
}

fn load_schema(
    store: &dyn Store,
    schema_id: ObjectId,
) -> Result<panproto_schema::Schema, VcsError> {
    match store.get(&schema_id)? {
        Object::Schema(s) => Ok(*s),
        other => Err(VcsError::WrongObjectType {
            expected: "schema",
            found: other.type_name(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MemStore;
    use crate::error::VcsError;
    use panproto_gat::Name;
    use panproto_schema::{Schema, Vertex};
    use std::collections::HashMap;

    fn make_schema(vertices: &[(&str, &str)]) -> Schema {
        let mut vert_map = HashMap::new();
        for (id, kind) in vertices {
            vert_map.insert(
                Name::from(*id),
                Vertex {
                    id: Name::from(*id),
                    kind: Name::from(*kind),
                    nsid: None,
                },
            );
        }
        Schema {
            protocol: "test".into(),
            vertices: vert_map,
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
            coercions: HashMap::new(),
            mergers: HashMap::new(),
            defaults: HashMap::new(),
            policies: HashMap::new(),
            outgoing: HashMap::new(),
            incoming: HashMap::new(),
            between: HashMap::new(),
        }
    }

    #[test]
    fn rebase_linear_onto_diverged() -> Result<(), VcsError> {
        let mut store = MemStore::new();

        // c0: base
        let s0 = make_schema(&[("a", "object")]);
        let s0_id = store.put(&Object::Schema(Box::new(s0)))?;
        let c0 = CommitObject {
            schema_id: s0_id,
            parents: vec![],
            migration_id: None,
            protocol: "test".into(),
            author: "alice".into(),
            timestamp: 100,
            message: "initial".into(),
            renames: vec![],
            protocol_id: None,
            data_ids: vec![],
            complement_ids: vec![],
        };
        let c0_id = store.put(&Object::Commit(c0))?;

        // c1: main branch adds vertex b
        let s1 = make_schema(&[("a", "object"), ("b", "string")]);
        let s1_id = store.put(&Object::Schema(Box::new(s1)))?;
        let c1 = CommitObject {
            schema_id: s1_id,
            parents: vec![c0_id],
            migration_id: None,
            protocol: "test".into(),
            author: "alice".into(),
            timestamp: 200,
            message: "add b".into(),
            renames: vec![],
            protocol_id: None,
            data_ids: vec![],
            complement_ids: vec![],
        };
        let c1_id = store.put(&Object::Commit(c1))?;

        // c2: feature branch (off c0) adds vertex c
        let s2 = make_schema(&[("a", "object"), ("c", "integer")]);
        let s2_id = store.put(&Object::Schema(Box::new(s2)))?;
        let c2 = CommitObject {
            schema_id: s2_id,
            parents: vec![c0_id],
            migration_id: None,
            protocol: "test".into(),
            author: "bob".into(),
            timestamp: 300,
            message: "add c".into(),
            renames: vec![],
            protocol_id: None,
            data_ids: vec![],
            complement_ids: vec![],
        };
        let c2_id = store.put(&Object::Commit(c2))?;

        // HEAD is on feature branch (c2).
        store.set_ref("refs/heads/main", c2_id)?;

        // Rebase feature onto c1.
        let new_tip = rebase(&mut store, c1_id, "bob")?;

        // Verify the rebased commit has both b and c.
        let new_commit = match store.get(&new_tip)? {
            Object::Commit(c) => c,
            other => {
                return Err(VcsError::WrongObjectType {
                    expected: "commit",
                    found: other.type_name(),
                });
            }
        };
        let new_schema = match store.get(&new_commit.schema_id)? {
            Object::Schema(s) => s,
            other => {
                return Err(VcsError::WrongObjectType {
                    expected: "schema",
                    found: other.type_name(),
                });
            }
        };
        assert!(new_schema.vertices.contains_key("a"));
        assert!(new_schema.vertices.contains_key("b"));
        assert!(new_schema.vertices.contains_key("c"));
        // Parent should be c1.
        assert_eq!(new_commit.parents[0], c1_id);
        Ok(())
    }
}
