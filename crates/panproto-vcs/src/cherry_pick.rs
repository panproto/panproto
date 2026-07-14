//! Cherry-pick: apply a single commit's migration to the current branch.

use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::VcsError;
use crate::hash::ObjectId;
use crate::merge;
use crate::object::{CommitObject, Object};
use crate::store::{self, ReflogEntry, Store};

/// Options for cherry-pick operations.
#[derive(Clone, Debug, Default)]
pub struct CherryPickOptions {
    /// Apply the changes but don't create a commit.
    pub no_commit: bool,
    /// Append "(cherry picked from commit ...)" to the message.
    pub record_origin: bool,
}

/// Apply a single commit's schema changes to the current HEAD.
///
/// Extracts the migration represented by `commit_id` (the diff between
/// its parent's schema and its own schema), then performs a three-way
/// merge with the current HEAD schema using the parent's schema as the
/// base.
///
/// # Algorithm
///
/// 1. Load the commit and its first parent.
/// 2. Load all three schemas: parent's, commit's, and HEAD's.
/// 3. Three-way merge: base = parent's schema, ours = HEAD's schema,
///    theirs = commit's schema.
/// 4. If clean, create a new commit on the current branch.
///
/// # Errors
///
/// Returns an error if the merge has conflicts, or if the commit is a
/// root commit (no parent to diff against).
pub fn cherry_pick(
    store: &mut dyn Store,
    commit_id: ObjectId,
    author: &str,
) -> Result<ObjectId, VcsError> {
    // Load the commit being cherry-picked.
    let commit = match store.get(&commit_id)? {
        Object::Commit(c) => c,
        other => {
            return Err(VcsError::WrongObjectType {
                expected: "commit",
                found: other.type_name(),
            });
        }
    };

    // Need at least one parent to compute the diff.
    let parent_id = commit.parents.first().ok_or(VcsError::NoPath)?;

    let parent_commit = match store.get(parent_id)? {
        Object::Commit(c) => c,
        other => {
            return Err(VcsError::WrongObjectType {
                expected: "commit",
                found: other.type_name(),
            });
        }
    };

    // Load schemas.
    let base_schema = load_schema_dyn(store, &parent_commit.schema_id)?;
    let theirs_schema = load_schema_dyn(store, &commit.schema_id)?;

    // Load HEAD's schema.
    let head_id = store::resolve_head(store)?.ok_or_else(|| VcsError::RefNotFound {
        name: "HEAD".to_owned(),
    })?;
    let head_commit = match store.get(&head_id)? {
        Object::Commit(c) => c,
        other => {
            return Err(VcsError::WrongObjectType {
                expected: "commit",
                found: other.type_name(),
            });
        }
    };
    let ours_schema = load_schema_dyn(store, &head_commit.schema_id)?;

    // Three-way merge.
    let result = merge::three_way_merge(&base_schema, &ours_schema, &theirs_schema);
    if !result.conflicts.is_empty() {
        return Err(VcsError::MergeConflicts {
            count: result.conflicts.len(),
        });
    }

    // Verify the pushout cocone conditions before rebuilding the commit.
    let resolved = merge::ResolvedMerge {
        schema: result.merged_schema.clone(),
        migration_from_ours: result.migration_from_ours.clone(),
        migration_from_theirs: result.migration_from_theirs.clone(),
    };
    merge::verify_pushout(&base_schema, &ours_schema, &theirs_schema, &resolved)
        .map_err(VcsError::PushoutVerification)?;

    // Lift the picked commit's versioned data through the schema change
    // (its own schema -> the merged schema) so cherry-pick carries data
    // forward instead of silently dropping it.
    let (lifted_data_ids, lifted_complement_ids) =
        crate::data_mig::lift_commit_data(store, &commit, &theirs_schema, &result.merged_schema)?;

    // Square coherence: each lifted data set must round-trip through its
    // complement, so the cherry-pick square commutes on data.
    crate::square::verify_lifted_squares(
        store,
        &commit,
        &lifted_data_ids,
        &lifted_complement_ids,
        &theirs_schema,
        &result.merged_schema,
    )?;

    // Store the merged schema.
    let mig_src = store.put(&Object::FlatSchema(Box::new(ours_schema)))?;
    let mig_tgt = store.put(&Object::FlatSchema(Box::new(result.merged_schema.clone())))?;
    let merged_schema_id = crate::tree::store_schema_as_tree(store, result.merged_schema)?;

    // Store the migration from ours to merged.
    let migration_id = store.put(&Object::Migration {
        src: mig_src,
        tgt: mig_tgt,
        mapping: result.migration_from_ours,
    })?;

    // Create the new commit.
    let mut builder = CommitObject::builder(
        merged_schema_id,
        commit.protocol.clone(),
        author,
        format!("cherry-pick: {}", commit.message),
    )
    .parents(vec![head_id])
    .migration_id(migration_id);
    if !lifted_data_ids.is_empty() {
        builder = builder.data_ids(lifted_data_ids);
    }
    if !lifted_complement_ids.is_empty() {
        builder = builder.complement_ids(lifted_complement_ids);
    }
    // CST complements are keyed by content, not schema, so they carry
    // through the cherry-pick unchanged.
    if !commit.cst_complement_ids.is_empty() {
        builder = builder.cst_complement_ids(commit.cst_complement_ids.clone());
    }
    let new_commit = builder.build();
    let new_commit_id = store.put(&Object::Commit(new_commit))?;

    // Advance HEAD.
    advance_head(store, head_id, new_commit_id, author, "cherry-pick")?;

    Ok(new_commit_id)
}

/// Apply a single commit's schema changes with options.
///
/// See [`cherry_pick`] for the algorithm. Additional options control
/// whether to auto-commit and whether to record the source commit.
///
/// # Errors
///
/// Returns an error if the merge has conflicts.
pub fn cherry_pick_with_options(
    store: &mut dyn Store,
    commit_id: ObjectId,
    author: &str,
    options: &CherryPickOptions,
) -> Result<ObjectId, VcsError> {
    // Load the commit being cherry-picked.
    let commit = match store.get(&commit_id)? {
        Object::Commit(c) => c,
        other => {
            return Err(VcsError::WrongObjectType {
                expected: "commit",
                found: other.type_name(),
            });
        }
    };

    let parent_id = commit.parents.first().ok_or(VcsError::NoPath)?;
    let parent_commit = match store.get(parent_id)? {
        Object::Commit(c) => c,
        other => {
            return Err(VcsError::WrongObjectType {
                expected: "commit",
                found: other.type_name(),
            });
        }
    };

    let base_schema = load_schema_dyn(store, &parent_commit.schema_id)?;
    let theirs_schema = load_schema_dyn(store, &commit.schema_id)?;

    let head_id = store::resolve_head(store)?.ok_or_else(|| VcsError::RefNotFound {
        name: "HEAD".to_owned(),
    })?;
    let head_commit = match store.get(&head_id)? {
        Object::Commit(c) => c,
        other => {
            return Err(VcsError::WrongObjectType {
                expected: "commit",
                found: other.type_name(),
            });
        }
    };
    let ours_schema = load_schema_dyn(store, &head_commit.schema_id)?;

    let result = merge::three_way_merge(&base_schema, &ours_schema, &theirs_schema);
    if !result.conflicts.is_empty() {
        return Err(VcsError::MergeConflicts {
            count: result.conflicts.len(),
        });
    }

    // Verify the pushout cocone conditions before rebuilding the commit.
    let resolved = merge::ResolvedMerge {
        schema: result.merged_schema.clone(),
        migration_from_ours: result.migration_from_ours.clone(),
        migration_from_theirs: result.migration_from_theirs.clone(),
    };
    merge::verify_pushout(&base_schema, &ours_schema, &theirs_schema, &resolved)
        .map_err(VcsError::PushoutVerification)?;

    let mig_src = store.put(&Object::FlatSchema(Box::new(ours_schema)))?;
    let mig_tgt = store.put(&Object::FlatSchema(Box::new(result.merged_schema.clone())))?;
    let merged_schema_id = crate::tree::store_schema_as_tree(store, result.merged_schema.clone())?;

    if options.no_commit {
        return Ok(merged_schema_id);
    }

    // Lift the picked commit's versioned data through the schema change
    // (its own schema -> the merged schema) so the rebuilt commit carries
    // data forward instead of silently dropping it.
    let (lifted_data_ids, lifted_complement_ids) =
        crate::data_mig::lift_commit_data(store, &commit, &theirs_schema, &result.merged_schema)?;

    // Square coherence: each lifted data set must round-trip through its
    // complement, so the cherry-pick square commutes on data.
    crate::square::verify_lifted_squares(
        store,
        &commit,
        &lifted_data_ids,
        &lifted_complement_ids,
        &theirs_schema,
        &result.merged_schema,
    )?;

    let migration_id = store.put(&Object::Migration {
        src: mig_src,
        tgt: mig_tgt,
        mapping: result.migration_from_ours,
    })?;

    let mut message = format!("cherry-pick: {}", commit.message);
    if options.record_origin {
        use std::fmt::Write as _;
        let _ = write!(message, "\n\n(cherry picked from commit {commit_id})");
    }

    let mut builder =
        CommitObject::builder(merged_schema_id, commit.protocol.clone(), author, message)
            .parents(vec![head_id])
            .migration_id(migration_id);
    if !lifted_data_ids.is_empty() {
        builder = builder.data_ids(lifted_data_ids);
    }
    if !lifted_complement_ids.is_empty() {
        builder = builder.complement_ids(lifted_complement_ids);
    }
    // CST complements are keyed by content, not schema, so they carry
    // through the cherry-pick unchanged.
    if !commit.cst_complement_ids.is_empty() {
        builder = builder.cst_complement_ids(commit.cst_complement_ids.clone());
    }
    let new_commit = builder.build();
    let new_commit_id = store.put(&Object::Commit(new_commit))?;

    advance_head(store, head_id, new_commit_id, author, "cherry-pick")?;

    Ok(new_commit_id)
}

/// Advance HEAD (or the branch it points to) and append a reflog entry.
pub(crate) fn advance_head(
    store: &mut dyn Store,
    old_id: ObjectId,
    new_id: ObjectId,
    author: &str,
    action: &str,
) -> Result<(), VcsError> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    match store.get_head()? {
        crate::HeadState::Branch(name) => {
            let ref_name = format!("refs/heads/{name}");
            store.set_ref(&ref_name, new_id)?;
            store.append_reflog(
                &ref_name,
                ReflogEntry {
                    old_id: Some(old_id),
                    new_id,
                    author: author.to_owned(),
                    timestamp,
                    message: action.to_owned(),
                },
            )?;
        }
        crate::HeadState::Detached(_) => {
            store.set_head(crate::HeadState::Detached(new_id))?;
        }
    }
    store.append_reflog(
        "HEAD",
        ReflogEntry {
            old_id: Some(old_id),
            new_id,
            author: author.to_owned(),
            timestamp,
            message: action.to_owned(),
        },
    )?;
    Ok(())
}

fn load_schema_dyn(
    store: &dyn Store,
    schema_id: &ObjectId,
) -> Result<panproto_schema::Schema, VcsError> {
    let proto = crate::tree::project_coproduct_protocol();
    crate::tree::assemble_schema_dyn(store, schema_id, &proto)
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
    fn cherry_pick_applies_change() -> Result<(), VcsError> {
        let mut store = MemStore::new();

        // c0: base with vertex a
        let s0 = make_schema(&[("a", "object")]);
        let s0_id = crate::tree::store_schema_as_tree(&mut store, s0)?;
        let c0 = CommitObject::builder(s0_id, "test", "alice", "initial")
            .timestamp(100)
            .build();
        let c0_id = store.put(&Object::Commit(c0))?;

        // c1: adds vertex b (on a separate branch)
        let s1 = make_schema(&[("a", "object"), ("b", "string")]);
        let s1_id = crate::tree::store_schema_as_tree(&mut store, s1)?;
        let c1 = CommitObject::builder(s1_id, "test", "bob", "add b")
            .parents(vec![c0_id])
            .timestamp(200)
            .build();
        let c1_id = store.put(&Object::Commit(c1))?;

        // HEAD points to c0 (our branch).
        store.set_ref("refs/heads/main", c0_id)?;

        // Cherry-pick c1 onto HEAD.
        let new_id = cherry_pick(&mut store, c1_id, "alice")?;

        // Verify the new commit has vertex b.
        let new_commit = match store.get(&new_id)? {
            Object::Commit(c) => c,
            other => {
                return Err(VcsError::WrongObjectType {
                    expected: "commit",
                    found: other.type_name(),
                });
            }
        };
        let new_schema = crate::tree::resolve_commit_schema(&store, &new_commit)?;
        assert!(new_schema.vertices.contains_key("b"));
        assert!(new_schema.vertices.contains_key("a"));
        assert!(new_commit.message.contains("cherry-pick"));
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

    /// Build a cherry-pick scenario: base `c0` {a}; a pickable commit `c1`
    /// {a, pickedf} carrying a one-record data set; and a HEAD commit `c2`
    /// {a, headf} with an independent compatible change. Returns
    /// `(c1_id, ds_id)`.
    fn pick_scenario(store: &mut MemStore) -> Result<(ObjectId, ObjectId), VcsError> {
        let s0 = make_schema(&[("a", "object")]);
        let s0_id = crate::tree::store_schema_as_tree(store, s0)?;
        let c0 = CommitObject::builder(s0_id, "test", "alice", "initial")
            .timestamp(100)
            .build();
        let c0_id = store.put(&Object::Commit(c0))?;

        let s1 = make_schema(&[("a", "object"), ("pickedf", "string")]);
        let ds_id = single_record_dataset(store, &s1, "rec-key")?;
        let s1_id = crate::tree::store_schema_as_tree(store, s1)?;
        let c1 = CommitObject::builder(s1_id, "test", "bob", "add pickedf + data")
            .parents(vec![c0_id])
            .timestamp(200)
            .data_ids(vec![ds_id])
            .build();
        let c1_id = store.put(&Object::Commit(c1))?;

        let s2 = make_schema(&[("a", "object"), ("headf", "string")]);
        let s2_id = crate::tree::store_schema_as_tree(store, s2)?;
        let c2 = CommitObject::builder(s2_id, "test", "alice", "add headf")
            .parents(vec![c0_id])
            .timestamp(300)
            .build();
        let c2_id = store.put(&Object::Commit(c2))?;

        // HEAD is on "main" at c2.
        store.set_ref("refs/heads/main", c2_id)?;
        Ok((c1_id, ds_id))
    }

    fn load_commit_obj(store: &MemStore, id: ObjectId) -> Result<CommitObject, VcsError> {
        match store.get(&id)? {
            Object::Commit(c) => Ok(c),
            other => Err(VcsError::WrongObjectType {
                expected: "commit",
                found: other.type_name(),
            }),
        }
    }

    fn assert_lifted_dataset(store: &MemStore, commit: &CommitObject, ds_id: ObjectId) {
        assert_eq!(
            commit.data_ids.len(),
            1,
            "cherry-picked commit must carry the lifted data set"
        );
        assert_ne!(
            commit.data_ids[0], ds_id,
            "data was lifted to a new object, not passed through stale"
        );
        assert!(
            !commit.complement_ids.is_empty(),
            "lifting records a backward-migration complement"
        );
        match store.get(&commit.data_ids[0]) {
            Ok(Object::DataSet(ds)) => {
                assert_eq!(ds.record_count, 1);
                assert_eq!(ds.key.as_deref(), Some("rec-key"));
            }
            _ => panic!("lifted data id must resolve to a DataSet"),
        }
    }

    #[test]
    fn cherry_pick_preserves_and_lifts_data() -> Result<(), VcsError> {
        let mut store = MemStore::new();
        let (c1_id, ds_id) = pick_scenario(&mut store)?;

        // Picking onto a HEAD with a different schema means the picked
        // commit's data ({a, pickedf}) is lifted to the merged schema
        // ({a, headf, pickedf}), not dropped.
        let new_id = cherry_pick(&mut store, c1_id, "alice")?;
        let new_commit = load_commit_obj(&store, new_id)?;
        assert_lifted_dataset(&store, &new_commit, ds_id);
        Ok(())
    }

    #[test]
    fn cherry_pick_with_options_record_origin_lifts_data() -> Result<(), VcsError> {
        let mut store = MemStore::new();
        let (c1_id, ds_id) = pick_scenario(&mut store)?;

        let opts = CherryPickOptions {
            no_commit: false,
            record_origin: true,
        };
        let new_id = cherry_pick_with_options(&mut store, c1_id, "alice", &opts)?;
        let new_commit = load_commit_obj(&store, new_id)?;
        assert!(
            new_commit.message.contains("cherry picked from commit"),
            "record_origin should append the origin line"
        );
        assert_lifted_dataset(&store, &new_commit, ds_id);
        Ok(())
    }
}
