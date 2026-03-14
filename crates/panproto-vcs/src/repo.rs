//! High-level repository orchestration (porcelain).
//!
//! [`Repository`] composes all plumbing modules into a convenient
//! API for performing version control operations on schemas.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use panproto_check::diff;
use panproto_schema::Schema;

use crate::auto_mig;
use crate::cherry_pick::{self, advance_head};
use crate::dag;
use crate::error::VcsError;
use crate::fs_store::FsStore;
use crate::gc;
use crate::hash::{self, ObjectId};
use crate::index::{Index, StagedSchema, ValidationStatus};
use crate::merge;
use crate::object::{CommitObject, Object};
use crate::refs;
use crate::store::{self, HeadState, Store};

/// A panproto repository backed by a filesystem store.
#[allow(dead_code)]
pub struct Repository {
    store: FsStore,
    working_dir: PathBuf,
}

impl Repository {
    /// Initialize a new repository at the given path.
    ///
    /// Creates the `.panproto/` directory structure and sets HEAD to `main`.
    ///
    /// # Errors
    ///
    /// Returns an error if the directory already exists or I/O fails.
    pub fn init(path: &Path) -> Result<Self, VcsError> {
        let store = FsStore::init(path)?;
        Ok(Self {
            store,
            working_dir: path.to_owned(),
        })
    }

    /// Open an existing repository.
    ///
    /// # Errors
    ///
    /// Returns [`VcsError::NotARepository`] if `.panproto/` does not exist.
    pub fn open(path: &Path) -> Result<Self, VcsError> {
        let store = FsStore::open(path)?;
        Ok(Self {
            store,
            working_dir: path.to_owned(),
        })
    }

    /// Stage a schema for the next commit.
    ///
    /// Computes the diff from HEAD's schema (if any), auto-derives a
    /// migration, validates it, and writes the index.
    ///
    /// # Errors
    ///
    /// Returns an error if the schema cannot be hashed or stored.
    pub fn add(&mut self, schema: &Schema) -> Result<Index, VcsError> {
        let schema_id = self.store.put(&Object::Schema(schema.clone()))?;

        let (migration_id, auto_derived, validation) = match store::resolve_head(&self.store)? {
            None => {
                // First commit — no migration needed.
                (None, false, ValidationStatus::Valid)
            }
            Some(head_id) => {
                let head_commit = self.load_commit(head_id)?;
                let head_schema = self.load_schema(head_commit.schema_id)?;

                let schema_diff = diff::diff(&head_schema, schema);
                if schema_diff.is_empty() {
                    return Err(VcsError::ValidationFailed {
                        reasons: vec!["no changes detected".to_owned()],
                    });
                }

                let migration = auto_mig::derive_migration(&head_schema, schema, &schema_diff);
                let mig_src_id = hash::hash_schema(&head_schema)?;
                let migration_id = self.store.put(&Object::Migration {
                    src: mig_src_id,
                    tgt: schema_id,
                    mapping: migration,
                })?;

                (Some(migration_id), true, ValidationStatus::Valid)
            }
        };

        let index = Index {
            staged: Some(StagedSchema {
                schema_id,
                migration_id,
                auto_derived,
                validation,
            }),
        };

        self.write_index(&index)?;
        Ok(index)
    }

    /// Create a commit from the current staging area.
    ///
    /// # Errors
    ///
    /// Returns [`VcsError::NothingStaged`] if the index is empty.
    pub fn commit(&mut self, message: &str, author: &str) -> Result<ObjectId, VcsError> {
        let index = self.read_index()?;
        let staged = index.staged.ok_or(VcsError::NothingStaged)?;

        let head_id = store::resolve_head(&self.store)?;

        // Determine protocol from the schema.
        let schema = self.load_schema(staged.schema_id)?;

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let commit = CommitObject {
            schema_id: staged.schema_id,
            parents: head_id.into_iter().collect(),
            migration_id: staged.migration_id,
            protocol: schema.protocol,
            author: author.to_owned(),
            timestamp,
            message: message.to_owned(),
        };
        let commit_id = self.store.put(&Object::Commit(commit))?;

        // Advance HEAD.
        if let Some(old) = head_id {
            advance_head(
                &mut self.store,
                old,
                commit_id,
                author,
                &format!("commit: {message}"),
            )?;
        } else {
            // First commit — set the branch ref.
            match self.store.get_head()? {
                HeadState::Branch(name) => {
                    let ref_name = format!("refs/heads/{name}");
                    self.store.set_ref(&ref_name, commit_id)?;
                }
                HeadState::Detached(_) => {
                    self.store.set_head(HeadState::Detached(commit_id))?;
                }
            }
        }

        // Clear the index.
        self.write_index(&Index::default())?;

        Ok(commit_id)
    }

    /// Merge a branch into the current branch.
    ///
    /// Performs a three-way merge using the merge base as the common
    /// ancestor. If the merge is clean, auto-commits. If there are
    /// conflicts, returns them.
    ///
    /// # Errors
    ///
    /// Returns an error if HEAD or the branch cannot be resolved.
    pub fn merge(&mut self, branch: &str, author: &str) -> Result<merge::MergeResult, VcsError> {
        let ours_id = store::resolve_head(&self.store)?.ok_or_else(|| VcsError::RefNotFound {
            name: "HEAD".to_owned(),
        })?;
        let theirs_id = refs::resolve_ref(&self.store, branch)?;

        // Fast-forward check.
        if dag::is_ancestor(&self.store, ours_id, theirs_id)? {
            // Theirs is ahead of ours — fast-forward.
            advance_head(
                &mut self.store,
                ours_id,
                theirs_id,
                author,
                &format!("merge {branch}: fast-forward"),
            )?;
            let theirs_commit = self.load_commit(theirs_id)?;
            let theirs_schema = self.load_schema(theirs_commit.schema_id)?;
            return Ok(merge::MergeResult {
                merged_schema: theirs_schema,
                conflicts: Vec::new(),
                migration_from_ours: panproto_mig::Migration::empty(),
                migration_from_theirs: panproto_mig::Migration::empty(),
            });
        }

        // Find merge base.
        let base_id =
            dag::merge_base(&self.store, ours_id, theirs_id)?.ok_or(VcsError::NoCommonAncestor)?;

        let base_commit = self.load_commit(base_id)?;
        let ours_commit = self.load_commit(ours_id)?;
        let theirs_commit = self.load_commit(theirs_id)?;

        let base_schema = self.load_schema(base_commit.schema_id)?;
        let ours_schema = self.load_schema(ours_commit.schema_id)?;
        let theirs_schema = self.load_schema(theirs_commit.schema_id)?;

        let result = merge::three_way_merge(&base_schema, &ours_schema, &theirs_schema);

        if result.conflicts.is_empty() {
            // Auto-commit the merge.
            let merged_schema_id = self
                .store
                .put(&Object::Schema(result.merged_schema.clone()))?;
            let migration_id = self.store.put(&Object::Migration {
                src: ours_commit.schema_id,
                tgt: merged_schema_id,
                mapping: result.migration_from_ours.clone(),
            })?;

            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();

            let merge_commit = CommitObject {
                schema_id: merged_schema_id,
                parents: vec![ours_id, theirs_id],
                migration_id: Some(migration_id),
                protocol: ours_commit.protocol,
                author: author.to_owned(),
                timestamp,
                message: format!("merge branch '{branch}'"),
            };
            let merge_id = self.store.put(&Object::Commit(merge_commit))?;
            advance_head(
                &mut self.store,
                ours_id,
                merge_id,
                author,
                &format!("merge {branch}"),
            )?;
        }

        Ok(result)
    }

    /// Walk the commit log from HEAD.
    ///
    /// # Errors
    ///
    /// Returns an error if HEAD cannot be resolved.
    pub fn log(&self, limit: Option<usize>) -> Result<Vec<CommitObject>, VcsError> {
        let head_id = store::resolve_head(&self.store)?.ok_or_else(|| VcsError::RefNotFound {
            name: "HEAD".to_owned(),
        })?;
        dag::log_walk(&self.store, head_id, limit)
    }

    /// Cherry-pick a commit onto the current branch.
    ///
    /// # Errors
    ///
    /// Returns an error if the cherry-pick produces conflicts.
    pub fn cherry_pick(&mut self, commit_id: ObjectId, author: &str) -> Result<ObjectId, VcsError> {
        cherry_pick::cherry_pick(&mut self.store, commit_id, author)
    }

    /// Rebase the current branch onto `onto`.
    ///
    /// # Errors
    ///
    /// Returns an error if rebase produces conflicts.
    pub fn rebase(&mut self, onto: ObjectId, author: &str) -> Result<ObjectId, VcsError> {
        crate::rebase::rebase(&mut self.store, onto, author)
    }

    /// Reset HEAD to a target commit.
    ///
    /// # Errors
    ///
    /// Returns an error on I/O failure.
    pub fn reset(
        &mut self,
        target: ObjectId,
        mode: crate::reset::ResetMode,
        author: &str,
    ) -> Result<crate::reset::ResetOutcome, VcsError> {
        let outcome = crate::reset::reset(&mut self.store, target, mode, author)?;
        if outcome.should_clear_index {
            self.write_index(&Index::default())?;
        }
        Ok(outcome)
    }

    /// Run garbage collection: delete unreachable objects.
    ///
    /// # Errors
    ///
    /// Returns an error on I/O failure.
    pub fn gc(&mut self) -> Result<gc::GcReport, VcsError> {
        gc::gc(&mut self.store)
    }

    /// Get a reference to the underlying store.
    #[must_use]
    pub const fn store(&self) -> &FsStore {
        &self.store
    }

    /// Get a mutable reference to the underlying store.
    pub const fn store_mut(&mut self) -> &mut FsStore {
        &mut self.store
    }

    // -- internal helpers --

    fn load_commit(&self, id: ObjectId) -> Result<CommitObject, VcsError> {
        match self.store.get(&id)? {
            Object::Commit(c) => Ok(c),
            other => Err(VcsError::WrongObjectType {
                expected: "commit",
                found: other.type_name(),
            }),
        }
    }

    fn load_schema(&self, id: ObjectId) -> Result<Schema, VcsError> {
        match self.store.get(&id)? {
            Object::Schema(s) => Ok(s),
            other => Err(VcsError::WrongObjectType {
                expected: "schema",
                found: other.type_name(),
            }),
        }
    }

    fn index_path(&self) -> PathBuf {
        self.store.root().join("index.json")
    }

    fn read_index(&self) -> Result<Index, VcsError> {
        let path = self.index_path();
        if !path.exists() {
            return Ok(Index::default());
        }
        let json = std::fs::read_to_string(&path)?;
        serde_json::from_str(&json)
            .map_err(|e| VcsError::Serialization(crate::error::SerializationError(e.to_string())))
    }

    fn write_index(&self, index: &Index) -> Result<(), VcsError> {
        let json = serde_json::to_string_pretty(index).map_err(|e| {
            VcsError::Serialization(crate::error::SerializationError(e.to_string()))
        })?;
        std::fs::write(self.index_path(), json)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use panproto_schema::Vertex;
    use std::collections::HashMap;

    fn make_schema(vertices: &[(&str, &str)]) -> Schema {
        let mut vert_map = HashMap::new();
        for (id, kind) in vertices {
            vert_map.insert(
                id.to_string(),
                Vertex {
                    id: id.to_string(),
                    kind: kind.to_string(),
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
            outgoing: HashMap::new(),
            incoming: HashMap::new(),
            between: HashMap::new(),
        }
    }

    #[test]
    fn init_add_commit() {
        let dir = tempfile::tempdir().unwrap();
        let mut repo = Repository::init(dir.path()).unwrap();

        let s = make_schema(&[("a", "object"), ("b", "string")]);
        repo.add(&s).unwrap();
        let commit_id = repo.commit("initial commit", "alice").unwrap();

        // Verify commit exists.
        let log = repo.log(None).unwrap();
        assert_eq!(log.len(), 1);
        assert_eq!(log[0].message, "initial commit");
        assert_eq!(log[0].author, "alice");

        // Verify HEAD points to the commit.
        let head = store::resolve_head(repo.store()).unwrap();
        assert_eq!(head, Some(commit_id));
    }

    #[test]
    fn add_commit_second_schema() {
        let dir = tempfile::tempdir().unwrap();
        let mut repo = Repository::init(dir.path()).unwrap();

        let s1 = make_schema(&[("a", "object")]);
        repo.add(&s1).unwrap();
        repo.commit("first", "alice").unwrap();

        let s2 = make_schema(&[("a", "object"), ("b", "string")]);
        repo.add(&s2).unwrap();
        repo.commit("second", "alice").unwrap();

        let log = repo.log(None).unwrap();
        assert_eq!(log.len(), 2);
        assert_eq!(log[0].message, "second");
        assert_eq!(log[1].message, "first");
    }

    #[test]
    fn merge_fast_forward() {
        let dir = tempfile::tempdir().unwrap();
        let mut repo = Repository::init(dir.path()).unwrap();

        let s1 = make_schema(&[("a", "object")]);
        repo.add(&s1).unwrap();
        let c1 = repo.commit("initial", "alice").unwrap();

        // Create a branch at c1.
        refs::create_branch(repo.store_mut(), "feature", c1).unwrap();

        // Add a commit on feature.
        refs::checkout_branch(repo.store_mut(), "feature").unwrap();
        let s2 = make_schema(&[("a", "object"), ("b", "string")]);
        repo.add(&s2).unwrap();
        repo.commit("add b", "bob").unwrap();

        // Switch back to main and merge feature.
        refs::checkout_branch(repo.store_mut(), "main").unwrap();
        let result = repo.merge("feature", "alice").unwrap();
        assert!(result.conflicts.is_empty());

        // main should now have vertex b.
        let log = repo.log(None).unwrap();
        let head_schema = repo.load_schema(log[0].schema_id).unwrap();
        assert!(head_schema.vertices.contains_key("b"));
    }

    #[test]
    fn nothing_staged_errors() {
        let dir = tempfile::tempdir().unwrap();
        let mut repo = Repository::init(dir.path()).unwrap();
        assert!(matches!(
            repo.commit("empty", "alice"),
            Err(VcsError::NothingStaged)
        ));
    }
}
