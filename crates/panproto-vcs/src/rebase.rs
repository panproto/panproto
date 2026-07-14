//! Rebase: replay commits onto a new base.

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
        // Nothing to rebase; HEAD is already at or before onto.
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

    // Verify the replay pushout cocone conditions before rebuilding the
    // commit: both migrations must be total and the base-to-merged paths
    // must commute. A violation aborts the rebase rather than replaying a
    // mathematically invalid schema.
    let resolved = merge::ResolvedMerge {
        schema: result.merged_schema.clone(),
        migration_from_ours: result.migration_from_ours.clone(),
        migration_from_theirs: result.migration_from_theirs.clone(),
    };
    merge::verify_pushout(&base_schema, &ours_schema, &theirs_schema, &resolved)
        .map_err(VcsError::PushoutVerification)?;

    // Lift the replayed commit's versioned data through the replay schema
    // change (its own schema -> the merged schema) so rebase carries data
    // forward instead of silently dropping it.
    let (lifted_data_ids, lifted_complement_ids) =
        crate::data_mig::lift_commit_data(store, &commit, &theirs_schema, &result.merged_schema)?;

    // Square coherence: each lifted data set must migrate back through its
    // complement to reconstruct the original, so the replay square commutes.
    crate::square::verify_lifted_squares(
        store,
        &commit,
        &lifted_data_ids,
        &lifted_complement_ids,
        &theirs_schema,
        &result.merged_schema,
    )?;

    let mig_src = store.put(&Object::FlatSchema(Box::new(ours_schema)))?;
    let mig_tgt = store.put(&Object::FlatSchema(Box::new(result.merged_schema.clone())))?;
    let merged_schema_id = crate::tree::store_schema_as_tree(store, result.merged_schema)?;
    let migration_id = store.put(&Object::Migration {
        src: mig_src,
        tgt: mig_tgt,
        mapping: result.migration_from_ours,
    })?;

    let mut builder = CommitObject::builder(
        merged_schema_id,
        commit.protocol.clone(),
        author,
        commit.message.clone(),
    )
    .parents(vec![current_tip])
    .migration_id(migration_id);
    if !lifted_data_ids.is_empty() {
        builder = builder.data_ids(lifted_data_ids);
    }
    if !lifted_complement_ids.is_empty() {
        builder = builder.complement_ids(lifted_complement_ids);
    }
    // CST complements are keyed by content, not schema, so they carry
    // through the replay unchanged.
    if !commit.cst_complement_ids.is_empty() {
        builder = builder.cst_complement_ids(commit.cst_complement_ids.clone());
    }
    let new_commit = builder.build();
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
    let proto = crate::tree::project_coproduct_protocol();
    crate::tree::assemble_schema_dyn(store, &schema_id, &proto)
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
            entries: Vec::new(),
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
        let s0_id = crate::tree::store_schema_as_tree(&mut store, s0)?;
        let c0 = CommitObject::builder(s0_id, "test", "alice", "initial")
            .timestamp(100)
            .build();
        let c0_id = store.put(&Object::Commit(c0))?;

        // c1: main branch adds vertex b
        let s1 = make_schema(&[("a", "object"), ("b", "string")]);
        let s1_id = crate::tree::store_schema_as_tree(&mut store, s1)?;
        let c1 = CommitObject::builder(s1_id, "test", "alice", "add b")
            .parents(vec![c0_id])
            .timestamp(200)
            .build();
        let c1_id = store.put(&Object::Commit(c1))?;

        // c2: feature branch (off c0) adds vertex c
        let s2 = make_schema(&[("a", "object"), ("c", "integer")]);
        let s2_id = crate::tree::store_schema_as_tree(&mut store, s2)?;
        let c2 = CommitObject::builder(s2_id, "test", "bob", "add c")
            .parents(vec![c0_id])
            .timestamp(300)
            .build();
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
        let new_schema = crate::tree::resolve_commit_schema(&store, &new_commit)?;
        assert!(new_schema.vertices.contains_key("a"));
        assert!(new_schema.vertices.contains_key("b"));
        assert!(new_schema.vertices.contains_key("c"));
        // Parent should be c1.
        assert_eq!(new_commit.parents[0], c1_id);
        Ok(())
    }

    /// Store a one-record data set (a single node anchored at `a`) valid
    /// against `schema`, returning its object id.
    fn single_record_dataset(
        store: &mut MemStore,
        schema: &Schema,
        key: &str,
    ) -> Result<ObjectId, VcsError> {
        use panproto_inst::{Node, WInstance};
        let mut nodes = HashMap::new();
        nodes.insert(0_u32, Node::new(0, "a"));
        let inst = WInstance::new(nodes, vec![], vec![], 0, Name::from("a"));
        let ds = crate::object::DataSetObject {
            schema_id: crate::hash::hash_schema(schema)?,
            data: rmp_serde::to_vec(&vec![inst])?,
            record_count: 1,
            key: Some(key.to_owned()),
        };
        store.put(&Object::DataSet(ds))
    }

    #[test]
    fn rebase_preserves_and_lifts_data() -> Result<(), VcsError> {
        let mut store = MemStore::new();

        // c0: base {a}
        let s0 = make_schema(&[("a", "object")]);
        let s0_id = crate::tree::store_schema_as_tree(&mut store, s0)?;
        let c0 = CommitObject::builder(s0_id, "test", "alice", "initial")
            .timestamp(100)
            .build();
        let c0_id = store.put(&Object::Commit(c0))?;

        // c1: main advances with a compatible change (adds vertex "mainf").
        let s1 = make_schema(&[("a", "object"), ("mainf", "string")]);
        let s1_id = crate::tree::store_schema_as_tree(&mut store, s1)?;
        let c1 = CommitObject::builder(s1_id, "test", "alice", "add mainf")
            .parents(vec![c0_id])
            .timestamp(200)
            .build();
        let c1_id = store.put(&Object::Commit(c1))?;

        // c2: feature (off c0) adds "featf" and carries a data set.
        let s2 = make_schema(&[("a", "object"), ("featf", "string")]);
        let ds_id = single_record_dataset(&mut store, &s2, "rec-key")?;
        let s2_id = crate::tree::store_schema_as_tree(&mut store, s2)?;
        let c2 = CommitObject::builder(s2_id, "test", "bob", "add featf + data")
            .parents(vec![c0_id])
            .timestamp(300)
            .data_ids(vec![ds_id])
            .build();
        let c2_id = store.put(&Object::Commit(c2))?;

        store.set_ref("refs/heads/main", c2_id)?;

        // Rebase feature onto c1: the feature schema ({a, featf}) is lifted
        // to the merged schema ({a, mainf, featf}), so the data must be
        // lifted too rather than dropped.
        let new_tip = rebase(&mut store, c1_id, "bob")?;

        let new_commit = load_commit(&store, new_tip)?;
        assert_eq!(new_commit.parents[0], c1_id);
        assert_eq!(
            new_commit.data_ids.len(),
            1,
            "rebased commit must carry the lifted data set"
        );
        assert_ne!(
            new_commit.data_ids[0], ds_id,
            "data was lifted to a new object, not passed through stale"
        );
        assert!(
            !new_commit.complement_ids.is_empty(),
            "lifting records a backward-migration complement"
        );

        match store.get(&new_commit.data_ids[0])? {
            Object::DataSet(ds) => {
                assert_eq!(ds.record_count, 1);
                assert_eq!(ds.key.as_deref(), Some("rec-key"));
            }
            other => {
                return Err(VcsError::WrongObjectType {
                    expected: "dataset",
                    found: other.type_name(),
                });
            }
        }
        Ok(())
    }

    #[test]
    fn verify_square_rejects_non_commuting_round_trip() -> Result<(), VcsError> {
        use panproto_inst::{Node, WInstance};

        let mut store = MemStore::new();
        let src = make_schema(&[("a", "object")]);
        let tgt = make_schema(&[("a", "object"), ("b", "string")]);
        let base = single_record_dataset(&mut store, &src, "rec")?;
        let protocol = crate::data_mig::protocol_for_schema(&src);
        let (replayed, complement) =
            crate::data_mig::migrate_forward(&mut store, base, &src, &tgt, &protocol)?;

        // The well-behaved migration reconstructs the base under round-trip.
        crate::square::verify_square(
            &mut store, base, replayed, complement, &src, &tgt, &protocol,
        )?;

        // Comparing the reconstruction against a different data set (two nodes
        // rather than one) diverges, so the square does not commute.
        let mut nodes = HashMap::new();
        nodes.insert(0_u32, Node::new(0, "a"));
        nodes.insert(1_u32, Node::new(1, "a"));
        let inst = WInstance::new(nodes, vec![], vec![], 0, Name::from("a"));
        let other = store.put(&Object::DataSet(crate::object::DataSetObject {
            schema_id: crate::hash::hash_schema(&src)?,
            data: rmp_serde::to_vec(&vec![inst])?,
            record_count: 1,
            key: None,
        }))?;
        let err = crate::square::verify_square(
            &mut store, other, replayed, complement, &src, &tgt, &protocol,
        );
        assert!(
            matches!(err, Err(VcsError::SquareNonCommuting { .. })),
            "expected SquareNonCommuting, got {err:?}"
        );
        Ok(())
    }

    #[test]
    fn rebase_identical_schema_propagates_data_ids_unchanged() -> Result<(), VcsError> {
        let mut store = MemStore::new();

        // A single shared schema tree used by every commit, so the replayed
        // schema equals the tip schema and no lifting is needed.
        let s0 = make_schema(&[("a", "object")]);
        let s0_id = crate::tree::store_schema_as_tree(&mut store, s0.clone())?;

        let c0 = CommitObject::builder(s0_id, "test", "alice", "initial")
            .timestamp(100)
            .build();
        let c0_id = store.put(&Object::Commit(c0))?;

        // main advances with a commit that keeps the same schema.
        let c1 = CommitObject::builder(s0_id, "test", "alice", "no schema change")
            .parents(vec![c0_id])
            .timestamp(200)
            .build();
        let c1_id = store.put(&Object::Commit(c1))?;

        // feature (off c0) keeps the same schema and carries data.
        let ds_id = single_record_dataset(&mut store, &s0, "verbatim-key")?;
        let c2 = CommitObject::builder(s0_id, "test", "bob", "data only")
            .parents(vec![c0_id])
            .timestamp(300)
            .data_ids(vec![ds_id])
            .build();
        let c2_id = store.put(&Object::Commit(c2))?;

        store.set_ref("refs/heads/main", c2_id)?;

        let new_tip = rebase(&mut store, c1_id, "bob")?;
        let new_commit = load_commit(&store, new_tip)?;

        // The replayed schema equals the tip schema, so the original data id
        // appears verbatim on the new commit.
        assert_eq!(new_commit.data_ids, vec![ds_id]);
        Ok(())
    }
}
