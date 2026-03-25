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
    let base_schema = match store.get(&parent_commit.schema_id)? {
        Object::Schema(s) => *s,
        other => {
            return Err(VcsError::WrongObjectType {
                expected: "schema",
                found: other.type_name(),
            });
        }
    };

    let theirs_schema = match store.get(&commit.schema_id)? {
        Object::Schema(s) => *s,
        other => {
            return Err(VcsError::WrongObjectType {
                expected: "schema",
                found: other.type_name(),
            });
        }
    };

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
    let ours_schema = match store.get(&head_commit.schema_id)? {
        Object::Schema(s) => *s,
        other => {
            return Err(VcsError::WrongObjectType {
                expected: "schema",
                found: other.type_name(),
            });
        }
    };

    // Three-way merge.
    let result = merge::three_way_merge(&base_schema, &ours_schema, &theirs_schema);
    if !result.conflicts.is_empty() {
        return Err(VcsError::MergeConflicts {
            count: result.conflicts.len(),
        });
    }

    // Store the merged schema.
    let merged_schema_id = store.put(&Object::Schema(Box::new(result.merged_schema)))?;

    // Store the migration from ours to merged.
    let migration_id = store.put(&Object::Migration {
        src: head_commit.schema_id,
        tgt: merged_schema_id,
        mapping: result.migration_from_ours,
    })?;

    // Create the new commit.
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let new_commit = CommitObject {
        schema_id: merged_schema_id,
        parents: vec![head_id],
        migration_id: Some(migration_id),
        protocol: commit.protocol.clone(),
        author: author.to_owned(),
        timestamp,
        message: format!("cherry-pick: {}", commit.message),
        renames: vec![],
        protocol_id: None,
        data_ids: vec![],
        complement_ids: vec![],
        edit_log_ids: vec![],
    };
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

    let base_schema = match store.get(&parent_commit.schema_id)? {
        Object::Schema(s) => *s,
        other => {
            return Err(VcsError::WrongObjectType {
                expected: "schema",
                found: other.type_name(),
            });
        }
    };

    let theirs_schema = match store.get(&commit.schema_id)? {
        Object::Schema(s) => *s,
        other => {
            return Err(VcsError::WrongObjectType {
                expected: "schema",
                found: other.type_name(),
            });
        }
    };

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
    let ours_schema = match store.get(&head_commit.schema_id)? {
        Object::Schema(s) => *s,
        other => {
            return Err(VcsError::WrongObjectType {
                expected: "schema",
                found: other.type_name(),
            });
        }
    };

    let result = merge::three_way_merge(&base_schema, &ours_schema, &theirs_schema);
    if !result.conflicts.is_empty() {
        return Err(VcsError::MergeConflicts {
            count: result.conflicts.len(),
        });
    }

    let merged_schema_id = store.put(&Object::Schema(Box::new(result.merged_schema)))?;

    if options.no_commit {
        return Ok(merged_schema_id);
    }

    let migration_id = store.put(&Object::Migration {
        src: head_commit.schema_id,
        tgt: merged_schema_id,
        mapping: result.migration_from_ours,
    })?;

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let mut message = format!("cherry-pick: {}", commit.message);
    if options.record_origin {
        use std::fmt::Write as _;
        let _ = write!(message, "\n\n(cherry picked from commit {commit_id})");
    }

    let new_commit = CommitObject {
        schema_id: merged_schema_id,
        parents: vec![head_id],
        migration_id: Some(migration_id),
        protocol: commit.protocol.clone(),
        author: author.to_owned(),
        timestamp,
        message,
        renames: vec![],
        protocol_id: None,
        data_ids: vec![],
        complement_ids: vec![],
        edit_log_ids: vec![],
    };
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
    fn cherry_pick_applies_change() -> Result<(), VcsError> {
        let mut store = MemStore::new();

        // c0: base with vertex a
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
            edit_log_ids: vec![],
        };
        let c0_id = store.put(&Object::Commit(c0))?;

        // c1: adds vertex b (on a separate branch)
        let s1 = make_schema(&[("a", "object"), ("b", "string")]);
        let s1_id = store.put(&Object::Schema(Box::new(s1)))?;
        let c1 = CommitObject {
            schema_id: s1_id,
            parents: vec![c0_id],
            migration_id: None,
            protocol: "test".into(),
            author: "bob".into(),
            timestamp: 200,
            message: "add b".into(),
            renames: vec![],
            protocol_id: None,
            data_ids: vec![],
            complement_ids: vec![],
            edit_log_ids: vec![],
        };
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
        let new_schema = match store.get(&new_commit.schema_id)? {
            Object::Schema(s) => *s,
            other => {
                return Err(VcsError::WrongObjectType {
                    expected: "schema",
                    found: other.type_name(),
                });
            }
        };
        assert!(new_schema.vertices.contains_key("b"));
        assert!(new_schema.vertices.contains_key("a"));
        assert!(new_commit.message.contains("cherry-pick"));
        Ok(())
    }
}
