//! High-level repository orchestration (porcelain).
//!
//! [`Repository`] composes all plumbing modules into a convenient
//! API for performing version control operations on schemas.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use panproto_check::diff;
use panproto_mig::hom_search::{SearchOptions, find_best_morphism, morphism_to_migration};
use panproto_schema::Schema;

use crate::auto_mig;
use crate::cherry_pick::{self, advance_head};
use crate::dag;
use crate::error::VcsError;
use crate::fs_store::FsStore;
use crate::gat_validate;
use crate::gc;
use crate::hash::{self, ObjectId};
use crate::index::{Index, StagedData, StagedSchema, ValidationStatus};
use crate::merge;
use crate::object::{CommitObject, DataSetObject, Object};
use crate::refs;
use crate::store::{self, HeadState, Store};

/// Options for creating a commit.
#[derive(Clone, Debug, Default)]
pub struct CommitOptions {
    /// Skip GAT equation verification (escape hatch for advanced users).
    pub skip_verify: bool,
}

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
        let schema_id = self.store.put(&Object::Schema(Box::new(schema.clone())))?;

        let (migration_id, auto_derived, validation, gat_diagnostics) =
            match store::resolve_head(&self.store)? {
                None => {
                    // First commit — no migration needed.
                    (None, false, ValidationStatus::Valid, None)
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

                    let mut migration =
                        auto_mig::derive_migration(&head_schema, schema, &schema_diff);

                    // If the auto-derived migration maps very few vertices
                    // (less than half of old schema vertices), try
                    // `find_best_morphism` as a fallback.
                    let old_vertex_count = head_schema.vertex_count();
                    if old_vertex_count > 0 && migration.vertex_map.len() * 2 < old_vertex_count {
                        let opts = SearchOptions::default();
                        if let Some(best) = find_best_morphism(&head_schema, schema, &opts) {
                            if best.vertex_map.len() > migration.vertex_map.len() {
                                let mut hom_mig = morphism_to_migration(&best);
                                hom_mig.hyper_edge_map = migration.hyper_edge_map;
                                hom_mig.label_map = migration.label_map;
                                migration = hom_mig;
                            }
                        }
                    }

                    // Run GAT-level validation on the derived migration.
                    let gat_diag =
                        gat_validate::validate_migration(&head_schema, schema, &migration);

                    let mig_src_id = hash::hash_schema(&head_schema)?;
                    let migration_id = self.store.put(&Object::Migration {
                        src: mig_src_id,
                        tgt: schema_id,
                        mapping: migration,
                    })?;

                    // If GAT validation found errors, mark as invalid.
                    let validation = if gat_diag.has_errors() {
                        ValidationStatus::Invalid(gat_diag.all_errors())
                    } else {
                        ValidationStatus::Valid
                    };

                    (Some(migration_id), true, validation, Some(gat_diag))
                }
            };

        let mut index = self.read_index()?;
        index.staged = Some(StagedSchema {
            schema_id,
            migration_id,
            auto_derived,
            validation,
            gat_diagnostics,
        });

        self.write_index(&index)?;
        Ok(index)
    }

    /// Create a commit from the current staging area.
    ///
    /// Equivalent to calling [`commit_with_options`](Self::commit_with_options)
    /// with default options (GAT verification enabled).
    ///
    /// # Errors
    ///
    /// Returns [`VcsError::NothingStaged`] if the index is empty, or
    /// [`VcsError::ValidationFailed`] if GAT diagnostics have errors.
    pub fn commit(&mut self, message: &str, author: &str) -> Result<ObjectId, VcsError> {
        self.commit_with_options(message, author, &CommitOptions::default())
    }

    /// Create a commit from the current staging area with options.
    ///
    /// When `options.skip_verify` is `false` (the default), this method
    /// checks the staged GAT diagnostics and blocks the commit if there
    /// are type errors or equation violations.
    ///
    /// # Errors
    ///
    /// Returns [`VcsError::NothingStaged`] if the index is empty, or
    /// [`VcsError::ValidationFailed`] if GAT diagnostics have errors
    /// and `skip_verify` is `false`.
    pub fn commit_with_options(
        &mut self,
        message: &str,
        author: &str,
        options: &CommitOptions,
    ) -> Result<ObjectId, VcsError> {
        let index = self.read_index()?;
        let staged = index.staged.ok_or(VcsError::NothingStaged)?;

        // Check GAT diagnostics unless skip_verify is set.
        if !options.skip_verify {
            // Check validation status.
            if let ValidationStatus::Invalid(reasons) = &staged.validation {
                return Err(VcsError::ValidationFailed {
                    reasons: reasons.clone(),
                });
            }
            // Check GAT diagnostics directly (covers type errors and equation violations).
            if let Some(ref diag) = staged.gat_diagnostics {
                if diag.has_errors() {
                    return Err(VcsError::ValidationFailed {
                        reasons: diag.all_errors(),
                    });
                }
            }
        }

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
            renames: vec![],
            protocol_id: index.staged_protocol,
            data_ids: index.staged_data.iter().map(|sd| sd.data_id).collect(),
            complement_ids: vec![],
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

    /// Merge a branch into the current branch with default options.
    ///
    /// # Errors
    ///
    /// Returns an error if HEAD or the branch cannot be resolved.
    pub fn merge(&mut self, branch: &str, author: &str) -> Result<merge::MergeResult, VcsError> {
        self.merge_with_options(branch, author, &merge::MergeOptions::default())
    }

    /// Merge a branch into the current branch with options.
    ///
    /// Performs a three-way merge using the merge base as the common
    /// ancestor. Behavior is controlled by [`merge::MergeOptions`].
    ///
    /// # Errors
    ///
    /// Returns an error if HEAD or the branch cannot be resolved.
    pub fn merge_with_options(
        &mut self,
        branch: &str,
        author: &str,
        options: &merge::MergeOptions,
    ) -> Result<merge::MergeResult, VcsError> {
        let ours_id = store::resolve_head(&self.store)?.ok_or_else(|| VcsError::RefNotFound {
            name: "HEAD".to_owned(),
        })?;
        let theirs_id = refs::resolve_ref(&self.store, branch)?;

        // Fast-forward check.
        if dag::is_ancestor(&self.store, ours_id, theirs_id)? {
            if options.no_ff {
                // Force a merge commit even though we could fast-forward.
                // Fall through to three-way merge logic below.
            } else {
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
                    pullback_overlap: None,
                });
            }
        } else if options.ff_only {
            return Err(VcsError::FastForwardOnly);
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

        if result.conflicts.is_empty() && !options.no_commit && !options.squash {
            // Auto-commit the merge.
            let merged_schema_id = self
                .store
                .put(&Object::Schema(Box::new(result.merged_schema.clone())))?;
            let migration_id = self.store.put(&Object::Migration {
                src: ours_commit.schema_id,
                tgt: merged_schema_id,
                mapping: result.migration_from_ours.clone(),
            })?;

            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();

            let msg = options
                .message
                .clone()
                .unwrap_or_else(|| format!("merge branch '{branch}'"));

            let merge_commit = CommitObject {
                schema_id: merged_schema_id,
                parents: vec![ours_id, theirs_id],
                migration_id: Some(migration_id),
                protocol: ours_commit.protocol,
                author: author.to_owned(),
                timestamp,
                message: msg,
                renames: vec![],
                protocol_id: None,
                data_ids: vec![],
                complement_ids: vec![],
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

    /// Amend the most recent commit.
    ///
    /// Replaces HEAD commit with a new commit that has the same parents
    /// but the currently staged schema (or the same schema if nothing
    /// is staged) and the given message.
    ///
    /// # Errors
    ///
    /// Returns [`VcsError::NothingToAmend`] if there are no commits.
    pub fn amend(&mut self, message: &str, author: &str) -> Result<ObjectId, VcsError> {
        let head_id = store::resolve_head(&self.store)?.ok_or(VcsError::NothingToAmend)?;
        let old_commit = self.load_commit(head_id)?;

        // Use staged schema if available, otherwise keep the old one.
        let index = self.read_index()?;
        let (schema_id, migration_id) = if let Some(staged) = index.staged {
            (staged.schema_id, staged.migration_id)
        } else {
            (old_commit.schema_id, old_commit.migration_id)
        };

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let new_commit = CommitObject {
            schema_id,
            parents: old_commit.parents,
            migration_id,
            protocol: old_commit.protocol,
            author: author.to_owned(),
            timestamp,
            message: message.to_owned(),
            renames: vec![],
            protocol_id: old_commit.protocol_id,
            data_ids: old_commit.data_ids,
            complement_ids: old_commit.complement_ids,
        };
        let new_id = self.store.put(&Object::Commit(new_commit))?;

        // Replace HEAD.
        advance_head(
            &mut self.store,
            head_id,
            new_id,
            author,
            &format!("commit (amend): {message}"),
        )?;

        // Clear index.
        self.write_index(&Index::default())?;

        Ok(new_id)
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

    /// Stage a data file for the next commit.
    ///
    /// Reads the file, determines the schema (from staged schema or HEAD),
    /// counts records if the data is a JSON array, stores a `DataSetObject`,
    /// and updates the index.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read, or if no schema is
    /// available (nothing staged and no HEAD commit).
    pub fn add_data(&mut self, path: &Path) -> Result<Index, VcsError> {
        let data_bytes = std::fs::read(path)?;

        // Determine schema: use staged schema if present, otherwise HEAD.
        let index = self.read_index()?;
        let schema_id = if let Some(ref staged) = index.staged {
            staged.schema_id
        } else {
            let head_id = store::resolve_head(&self.store)?.ok_or(VcsError::NothingStaged)?;
            let commit = self.load_commit(head_id)?;
            commit.schema_id
        };

        let record_count = count_records(&data_bytes);

        let dataset = DataSetObject {
            schema_id,
            data: data_bytes,
            record_count,
        };
        let data_id = self.store.put(&Object::DataSet(dataset))?;

        let mut updated_index = index;
        updated_index.staged_data.push(StagedData {
            source_path: path.to_owned(),
            data_id,
            schema_id,
        });
        self.write_index(&updated_index)?;

        Ok(updated_index)
    }

    /// Stage a protocol definition for the next commit.
    ///
    /// Stores the protocol as a `Protocol` object and records it in the
    /// index for inclusion in the next commit.
    ///
    /// # Errors
    ///
    /// Returns an error if the protocol cannot be stored.
    pub fn add_protocol(
        &mut self,
        protocol: &panproto_schema::Protocol,
    ) -> Result<Index, VcsError> {
        let protocol_id = self
            .store
            .put(&Object::Protocol(Box::new(protocol.clone())))?;
        let mut index = self.read_index()?;
        index.staged_protocol = Some(protocol_id);
        self.write_index(&index)?;
        Ok(index)
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
            Object::Schema(s) => Ok(*s),
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

/// Count records in a data blob.
///
/// Tries to parse as a JSON array and returns the number of elements.
/// Falls back to 1 for non-array JSON or non-JSON data.
fn count_records(data: &[u8]) -> u64 {
    serde_json::from_slice::<serde_json::Value>(data).map_or(1, |value| match &value {
        serde_json::Value::Array(arr) => arr.len() as u64,
        _ => 1,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use panproto_gat::Name;
    use panproto_schema::Vertex;
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
            outgoing: HashMap::new(),
            incoming: HashMap::new(),
            between: HashMap::new(),
        }
    }

    #[test]
    fn init_add_commit() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let mut repo = Repository::init(dir.path())?;

        let s = make_schema(&[("a", "object"), ("b", "string")]);
        repo.add(&s)?;
        let commit_id = repo.commit("initial commit", "alice")?;

        // Verify commit exists.
        let log = repo.log(None)?;
        assert_eq!(log.len(), 1);
        assert_eq!(log[0].message, "initial commit");
        assert_eq!(log[0].author, "alice");

        // Verify HEAD points to the commit.
        let head = store::resolve_head(repo.store())?;
        assert_eq!(head, Some(commit_id));
        Ok(())
    }

    #[test]
    fn add_commit_second_schema() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let mut repo = Repository::init(dir.path())?;

        let s1 = make_schema(&[("a", "object")]);
        repo.add(&s1)?;
        repo.commit("first", "alice")?;

        let s2 = make_schema(&[("a", "object"), ("b", "string")]);
        repo.add(&s2)?;
        repo.commit("second", "alice")?;

        let log = repo.log(None)?;
        assert_eq!(log.len(), 2);
        assert_eq!(log[0].message, "second");
        assert_eq!(log[1].message, "first");
        Ok(())
    }

    #[test]
    fn merge_fast_forward() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let mut repo = Repository::init(dir.path())?;

        let s1 = make_schema(&[("a", "object")]);
        repo.add(&s1)?;
        let c1 = repo.commit("initial", "alice")?;

        // Create a branch at c1.
        refs::create_branch(repo.store_mut(), "feature", c1)?;

        // Add a commit on feature.
        refs::checkout_branch(repo.store_mut(), "feature")?;
        let s2 = make_schema(&[("a", "object"), ("b", "string")]);
        repo.add(&s2)?;
        repo.commit("add b", "bob")?;

        // Switch back to main and merge feature.
        refs::checkout_branch(repo.store_mut(), "main")?;
        let result = repo.merge("feature", "alice")?;
        assert!(result.conflicts.is_empty());

        // main should now have vertex b.
        let log = repo.log(None)?;
        let head_schema = repo.load_schema(log[0].schema_id)?;
        assert!(head_schema.vertices.contains_key("b"));
        Ok(())
    }

    #[test]
    fn nothing_staged_errors() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let mut repo = Repository::init(dir.path())?;
        assert!(matches!(
            repo.commit("empty", "alice"),
            Err(VcsError::NothingStaged)
        ));
        Ok(())
    }

    #[test]
    fn commit_blocked_by_gat_errors() -> Result<(), Box<dyn std::error::Error>> {
        use crate::gat_validate::GatDiagnostics;
        use crate::index::{Index, StagedSchema, ValidationStatus};

        let dir = tempfile::tempdir()?;
        let mut repo = Repository::init(dir.path())?;

        // Stage a valid first schema so we have an object in the store.
        let s = make_schema(&[("a", "object")]);
        repo.add(&s)?;
        repo.commit("initial", "alice")?;

        // Now manually write an index with GAT errors to simulate
        // a staging result that has equation violations.
        let staged_schema = make_schema(&[("a", "object"), ("b", "string")]);
        let schema_id = repo
            .store
            .put(&crate::object::Object::Schema(Box::new(staged_schema)))?;

        let diag = GatDiagnostics {
            type_errors: vec!["sort mismatch: expected Ob, got Hom".to_owned()],
            equation_errors: vec![],
            migration_warnings: vec![],
        };

        let index = Index {
            staged: Some(StagedSchema {
                schema_id,
                migration_id: None,
                auto_derived: false,
                validation: ValidationStatus::Invalid(diag.all_errors()),
                gat_diagnostics: Some(diag),
            }),
            staged_data: vec![],
            staged_protocol: None,
        };
        repo.write_index(&index)?;

        // Default commit should be blocked.
        let Err(err) = repo.commit("should fail", "alice") else {
            panic!("commit should fail when validation status is invalid");
        };
        assert!(
            matches!(&err, VcsError::ValidationFailed { reasons } if !reasons.is_empty()),
            "expected ValidationFailed, got: {err:?}"
        );

        // skip_verify should bypass the check.
        let opts = CommitOptions { skip_verify: true };
        let commit_id = repo.commit_with_options("forced commit", "alice", &opts)?;
        let log = repo.log(None)?;
        assert_eq!(log[0].message, "forced commit");
        assert_eq!(store::resolve_head(repo.store())?, Some(commit_id));
        Ok(())
    }

    #[test]
    fn commit_blocked_by_gat_diagnostics_only() -> Result<(), Box<dyn std::error::Error>> {
        use crate::gat_validate::GatDiagnostics;
        use crate::index::{Index, StagedSchema, ValidationStatus};

        let dir = tempfile::tempdir()?;
        let mut repo = Repository::init(dir.path())?;

        // First commit.
        let s = make_schema(&[("a", "object")]);
        repo.add(&s)?;
        repo.commit("initial", "alice")?;

        // Write index where validation is Valid but gat_diagnostics has errors.
        let staged_schema = make_schema(&[("a", "object"), ("c", "number")]);
        let schema_id = repo
            .store
            .put(&crate::object::Object::Schema(Box::new(staged_schema)))?;

        let diag = GatDiagnostics {
            type_errors: vec![],
            equation_errors: vec!["equation 'assoc' violated when f=id: LHS=a, RHS=b".to_owned()],
            migration_warnings: vec![],
        };

        let index = Index {
            staged: Some(StagedSchema {
                schema_id,
                migration_id: None,
                auto_derived: false,
                validation: ValidationStatus::Valid,
                gat_diagnostics: Some(diag),
            }),
            staged_data: vec![],
            staged_protocol: None,
        };
        repo.write_index(&index)?;

        // Should still be blocked because gat_diagnostics has errors.
        let Err(err) = repo.commit("should fail", "alice") else {
            panic!("commit should fail when GAT diagnostics has equation errors");
        };
        assert!(
            matches!(&err, VcsError::ValidationFailed { reasons } if reasons.iter().any(|r| r.contains("equation violation"))),
            "expected ValidationFailed with equation violation, got: {err:?}"
        );

        // skip_verify bypasses.
        let opts = CommitOptions { skip_verify: true };
        let id = repo.commit_with_options("bypassed", "alice", &opts)?;
        assert_eq!(store::resolve_head(repo.store())?, Some(id));
        Ok(())
    }

    #[test]
    fn add_data_and_commit() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let mut repo = Repository::init(dir.path())?;

        // First commit: a schema.
        let s = make_schema(&[("a", "object"), ("b", "string")]);
        repo.add(&s)?;
        repo.commit("initial schema", "alice")?;

        // Write a data file.
        let data_path = dir.path().join("data.json");
        std::fs::write(&data_path, r#"[{"a": 1}, {"a": 2}, {"a": 3}]"#)?;

        // Stage data.
        let index = repo.add_data(&data_path)?;
        assert_eq!(index.staged_data.len(), 1);
        assert_eq!(index.staged_data[0].source_path, data_path);

        // Need a schema change to commit (or stage a schema).
        let s2 = make_schema(&[("a", "object"), ("b", "string"), ("c", "number")]);
        repo.add(&s2)?;
        let commit_id = repo.commit("add data", "alice")?;

        // Verify commit has data_ids.
        let log = repo.log(None)?;
        assert_eq!(log[0].message, "add data");
        assert_eq!(log[0].data_ids.len(), 1);

        // Verify the data object exists in the store.
        let data_obj = repo.store().get(&log[0].data_ids[0])?;
        match data_obj {
            Object::DataSet(ds) => {
                assert_eq!(ds.record_count, 3);
            }
            _ => panic!("expected DataSet object"),
        }

        assert_eq!(store::resolve_head(repo.store())?, Some(commit_id));
        Ok(())
    }

    #[test]
    fn add_protocol_and_commit() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let mut repo = Repository::init(dir.path())?;

        // First commit.
        let s = make_schema(&[("a", "object")]);
        repo.add(&s)?;
        repo.commit("initial", "alice")?;

        // Stage a protocol.
        let protocol = panproto_schema::Protocol {
            name: "test-protocol".into(),
            schema_theory: "ThGraph".into(),
            instance_theory: "ThInst".into(),
            ..Default::default()
        };
        let index = repo.add_protocol(&protocol)?;
        assert!(index.staged_protocol.is_some());

        // Evolve schema and commit.
        let s2 = make_schema(&[("a", "object"), ("b", "string")]);
        repo.add(&s2)?;
        let commit_id = repo.commit("add protocol", "alice")?;

        // Verify commit has protocol_id.
        let log = repo.log(None)?;
        assert_eq!(log[0].message, "add protocol");
        assert!(log[0].protocol_id.is_some());

        // Verify the protocol object exists in the store.
        let protocol_id = log[0].protocol_id.ok_or("missing protocol_id")?;
        let proto_obj = repo.store().get(&protocol_id)?;
        match proto_obj {
            Object::Protocol(p) => {
                assert_eq!(p.name, "test-protocol");
            }
            _ => panic!("expected Protocol object"),
        }

        assert_eq!(store::resolve_head(repo.store())?, Some(commit_id));
        Ok(())
    }

    #[test]
    fn count_records_json_array() {
        assert_eq!(count_records(b"[1, 2, 3]"), 3);
    }

    #[test]
    fn count_records_json_object() {
        assert_eq!(count_records(b"{\"a\": 1}"), 1);
    }

    #[test]
    fn count_records_non_json() {
        assert_eq!(count_records(b"not json"), 1);
    }
}
