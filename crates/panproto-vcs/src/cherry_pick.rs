//! Cherry-pick: apply a single commit's migration to the current branch.

use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::VcsError;
use crate::hash::ObjectId;
use crate::replay::replay;
use crate::store::{self, ReflogEntry, Store};

/// Options for cherry-pick operations.
#[derive(Clone, Debug, Default)]
pub struct CherryPickOptions {
    /// Apply the changes but don't create a commit.
    pub no_commit: bool,
    /// Append "(cherry picked from commit ...)" to the message.
    pub record_origin: bool,
}

/// Apply a single commit's changes to the current HEAD.
///
/// [`cherry_pick_with_options`] with the default options.
///
/// # Errors
///
/// The errors of [`cherry_pick_with_options`].
pub fn cherry_pick(
    store: &mut dyn Store,
    commit_id: ObjectId,
    author: &str,
) -> Result<ObjectId, VcsError> {
    cherry_pick_with_options(store, commit_id, author, &CherryPickOptions::default())
}

/// Apply a single commit's changes to the current HEAD with options.
///
/// The commit's changes of its first parent's schema and protocol environment
/// are merged into HEAD's, and its data is transported to the merged schema
/// (see [`replay`]). Returns the new commit, or the merged schema when
/// `options.no_commit`.
///
/// # Errors
///
/// Returns an error if the merge has conflicts, if the commit is a root
/// commit (no parent to diff against), or if its data cannot be transported.
pub fn cherry_pick_with_options(
    store: &mut dyn Store,
    commit_id: ObjectId,
    author: &str,
    options: &CherryPickOptions,
) -> Result<ObjectId, VcsError> {
    let head_id = store::resolve_head(store)?.ok_or_else(|| VcsError::RefNotFound {
        name: "HEAD".to_owned(),
    })?;
    let replay = replay(store, commit_id, head_id)?;
    if options.no_commit {
        return Ok(replay.schema_id);
    }

    let mut message = format!("cherry-pick: {}", replay.commit.message);
    if options.record_origin {
        use std::fmt::Write as _;
        let _ = write!(message, "\n\n(cherry picked from commit {commit_id})");
    }
    let new_commit_id = replay.into_commit(store, author, message)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MemStore;
    use crate::error::VcsError;
    use panproto_gat::Name;
    use panproto_schema::{Schema, Vertex};
    use std::collections::HashMap;

    /// The protocol the tests' schemas name, registered by name: no
    /// registry declares "test".
    fn registered() -> HashMap<String, panproto_schema::Protocol> {
        HashMap::from([(
            "test".to_owned(),
            panproto_schema::Protocol {
                name: "test".into(),
                schema_theory: "ThGraph".into(),
                instance_theory: "ThWType".into(),
                obj_kinds: vec!["object".into(), "string".into(), "record".into()],
                ..Default::default()
            },
        )])
    }

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
            ..Default::default()
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
        let new_id = cherry_pick(&mut store, c1_id, "alice", &registered())?;

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
        let new_id = cherry_pick(&mut store, c1_id, "alice", &registered())?;
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
        let new_id = cherry_pick_with_options(&mut store, c1_id, "alice", &opts, &registered())?;
        let new_commit = load_commit_obj(&store, new_id)?;
        assert!(
            new_commit.message.contains("cherry picked from commit"),
            "record_origin should append the origin line"
        );
        assert_lifted_dataset(&store, &new_commit, ds_id);
        Ok(())
    }
}
