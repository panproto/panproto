//! High-level repository orchestration (porcelain).
//!
//! [`Repository`] composes all plumbing modules into a convenient
//! API for performing version control operations on schemas.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use panproto_check::diff;
use panproto_gat::Theory;
use panproto_mig::hom_search::{SearchOptions, find_best_morphism, morphism_to_migration};
use panproto_schema::Schema;

use crate::auto_mig;
use crate::cherry_pick::{self, advance_head};
use crate::dag;
use crate::error::VcsError;
use crate::fs_store::FsStore;
use crate::gat_validate;
use crate::gc;
use crate::hash::ObjectId;
use crate::index::{Index, StagedData, StagedSchema, ValidationStatus};
use crate::merge;
use crate::object::{CommitObject, DataSetObject, Object};
use crate::refs;
use crate::store::{self, HeadState, Store};

/// The versioned-data references a merge commit carries, as
/// `(data_ids, complement_ids, cst_complement_ids)`.
type MergedCommitData = (Vec<ObjectId>, Vec<ObjectId>, Vec<ObjectId>);

/// The context needed to record a clean three-way merge commit.
struct MergeCommitCtx<'a> {
    branch: &'a str,
    author: &'a str,
    options: &'a merge::MergeOptions,
    ours_id: ObjectId,
    theirs_id: ObjectId,
    ours_schema: Schema,
    result: &'a merge::MergeResult,
    data_ids: Vec<ObjectId>,
    complement_ids: Vec<ObjectId>,
    cst_complement_ids: Vec<ObjectId>,
}

/// Options for creating a commit.
#[derive(Clone, Debug, Default)]
pub struct CommitOptions {
    /// Skip GAT equation verification (escape hatch for advanced users).
    pub skip_verify: bool,
}

/// Options for staging a schema.
#[derive(Clone, Debug, Default)]
pub struct AddOptions {
    /// Skip GAT migration validation and schema-equation checks while
    /// staging. The migration is still derived and recorded; only the
    /// (bounded model-checking) validation is skipped, and the stage is
    /// left [`ValidationStatus::Pending`]. This is the escape hatch for
    /// bulk historical VCS builds, where every version was already
    /// validated at its own release and re-checking each `add` against
    /// HEAD is the dominant cost.
    pub skip_verify: bool,
}

/// A panproto repository backed by a filesystem store.
#[allow(dead_code)]
pub struct Repository {
    store: FsStore,
    working_dir: PathBuf,
    /// Registered protocol theories, keyed by protocol name. When a
    /// schema's protocol is registered here, its equations are checked at
    /// commit and merge; otherwise only structural validation runs. The
    /// CLI populates this from the `panproto-protocols` registry before
    /// operating on a repository.
    protocol_theories: HashMap<String, Theory>,
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
            protocol_theories: HashMap::new(),
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
            protocol_theories: HashMap::new(),
        })
    }

    /// Register a protocol theory so that schemas of that protocol are
    /// checked against its equations at commit and merge.
    ///
    /// Without a registered theory the commit pipeline validates only the
    /// schema's structure (its equation-free extracted theory) and records
    /// an advisory note that no equations were checked. A caller with the
    /// `panproto-protocols` registry passes each registered theory here.
    pub fn set_protocol_theory(&mut self, protocol: impl Into<String>, theory: Theory) {
        self.protocol_theories.insert(protocol.into(), theory);
    }

    /// Validate `schema` against its registered protocol theory's
    /// equations, or record an advisory note when no theory is registered.
    ///
    /// Returns diagnostics whose `equation_errors` are blocking and whose
    /// `equation_notes` are advisory.
    #[must_use]
    fn schema_equation_diagnostics(&self, schema: &Schema) -> gat_validate::GatDiagnostics {
        let mut diag = gat_validate::GatDiagnostics::default();
        if let Some(theory) = self.protocol_theories.get(&schema.protocol) {
            diag.extend(gat_validate::validate_schema_against_theory(schema, theory));
        } else {
            diag.equation_notes.push(format!(
                "no protocol theory registered for '{}'; schema equations were not checked",
                schema.protocol
            ));
        }
        diag
    }

    /// Stage a schema for the next commit.
    ///
    /// Equivalent to calling [`add_with_options`](Self::add_with_options)
    /// with default options (GAT migration validation enabled).
    ///
    /// # Errors
    ///
    /// Returns an error if the schema cannot be hashed or stored.
    pub fn add(&mut self, schema: &Schema) -> Result<Index, VcsError> {
        self.add_with_options(schema, &AddOptions::default())
    }

    /// Stage a schema for the next commit, with options.
    ///
    /// Computes the diff from HEAD's schema (if any), auto-derives a
    /// migration, and writes the index. When `options.skip_verify` is
    /// `false` (the default), the derived migration and the staged
    /// schema's equations are GAT-validated (a bounded model check).
    /// When `true`, the migration is still derived and recorded but that
    /// validation is skipped and the stage is left
    /// [`ValidationStatus::Pending`]; a default [`commit`](Self::commit)
    /// treats `Pending` as non-blocking. This lets a caller replaying
    /// already-validated versions build a historical VCS without paying
    /// the per-`add` model check.
    ///
    /// # Errors
    ///
    /// Returns an error if the schema cannot be hashed or stored.
    pub fn add_with_options(
        &mut self,
        schema: &Schema,
        options: &AddOptions,
    ) -> Result<Index, VcsError> {
        let schema_id = crate::tree::store_schema_as_tree(&mut self.store, schema.clone())?;

        let (migration_id, auto_derived, validation, gat_diagnostics) = match store::resolve_head(
            &self.store,
        )? {
            None => {
                // First commit: no migration, but (unless skipping) the
                // schema is checked against its protocol theory's equations.
                if options.skip_verify {
                    (None, false, ValidationStatus::Pending, None)
                } else {
                    let gat_diag = self.schema_equation_diagnostics(schema);
                    let validation = if gat_diag.has_errors() {
                        ValidationStatus::Invalid(gat_diag.all_errors())
                    } else {
                        ValidationStatus::Valid
                    };
                    (None, false, validation, Some(gat_diag))
                }
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

                let mut migration = auto_mig::derive_migration(&head_schema, schema, &schema_diff);

                // If the auto-derived migration maps very few vertices
                // (less than half of old schema vertices), try
                // `find_best_morphism` as a fallback. The spliced
                // candidate is validated as a morphism before adoption;
                // a candidate that fails falls back to the diff-derived
                // migration.
                let mut hom_rejection: Option<String> = None;
                let old_vertex_count = head_schema.vertex_count();
                if old_vertex_count > 0 && migration.vertex_map.len() * 2 < old_vertex_count {
                    let opts = SearchOptions::default();
                    if let Some(best) = find_best_morphism(&head_schema, schema, &opts) {
                        if best.vertex_map.len() > migration.vertex_map.len() {
                            let mut hom_mig = morphism_to_migration(&best);
                            hom_mig.hyper_edge_map.clone_from(&migration.hyper_edge_map);
                            hom_mig.label_map.clone_from(&migration.label_map);
                            // Validate the actual spliced candidate as a
                            // theory morphism before adopting it.
                            let (dom, cod, morph) = panproto_mig::induced_theory_morphism(
                                &head_schema,
                                schema,
                                &hom_mig,
                            );
                            match panproto_gat::check_morphism(&morph, &dom, &cod) {
                                Ok(()) => migration = hom_mig,
                                Err(e) => {
                                    hom_rejection = Some(format!(
                                        "hom_search candidate rejected (not a theory morphism): {e}; kept diff-derived migration"
                                    ));
                                }
                            }
                        }
                    }
                }

                // Stamp the migration with the source and target schema
                // identities, and (unless skipping) GAT-validate the
                // derived migration and the staged schema's equations.
                let mig_src_id = self
                    .store
                    .put(&Object::FlatSchema(Box::new(head_schema.clone())))?;
                let mig_tgt_id = self
                    .store
                    .put(&Object::FlatSchema(Box::new(schema.clone())))?;

                let (validation, gat_diagnostics) = if options.skip_verify {
                    (ValidationStatus::Pending, None)
                } else {
                    let mut gat_diag =
                        gat_validate::validate_migration(&head_schema, schema, &migration);
                    if let Some(note) = hom_rejection {
                        gat_diag.migration_warnings.push(note);
                    }
                    gat_diag.extend(self.schema_equation_diagnostics(schema));
                    // If GAT validation found errors, mark as invalid.
                    let validation = if gat_diag.has_errors() {
                        ValidationStatus::Invalid(gat_diag.all_errors())
                    } else {
                        ValidationStatus::Valid
                    };
                    (validation, Some(gat_diag))
                };

                let migration = migration.with_endpoints(
                    Some(panproto_gat::Name::from(mig_src_id.to_string())),
                    Some(panproto_gat::Name::from(mig_tgt_id.to_string())),
                );
                let migration_id = self.store.put(&Object::Migration {
                    src: mig_src_id,
                    tgt: mig_tgt_id,
                    mapping: migration,
                })?;

                (Some(migration_id), true, validation, gat_diagnostics)
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
        if !index.has_staged() {
            return Err(VcsError::NothingStaged);
        }

        let head_id = store::resolve_head(&self.store)?;

        // The commit's schema is the staged schema when one is staged, or
        // HEAD's schema carried forward for a data-only or protocol-only
        // commit (re-recording data or a protocol against the existing
        // type, which has no migration). This keeps `commit` in agreement
        // with `Index::has_staged`: a data-only stage now commits instead
        // of failing with `NothingStaged`.
        let (schema_id, migration_id) = if let Some(ref staged) = index.staged {
            // Check staged validation unless skip_verify is set.
            if !options.skip_verify {
                if let ValidationStatus::Invalid(reasons) = &staged.validation {
                    return Err(VcsError::ValidationFailed {
                        reasons: reasons.clone(),
                    });
                }
                // Covers type errors and equation violations.
                if let Some(ref diag) = staged.gat_diagnostics {
                    if diag.has_errors() {
                        return Err(VcsError::ValidationFailed {
                            reasons: diag.all_errors(),
                        });
                    }
                }
            }
            (staged.schema_id, staged.migration_id)
        } else {
            // Data/protocol-only commit: carry HEAD's schema forward.
            let head = head_id.ok_or(VcsError::NothingStaged)?;
            (self.load_commit(head)?.schema_id, None)
        };

        // Determine protocol from the schema.
        let schema = self.load_schema(schema_id)?;

        // Store the implicit theory derived from the schema.
        let theory_ids = self.store_schema_theory(&schema)?;

        let parents: Vec<ObjectId> = head_id.into_iter().collect();
        let data_ids: Vec<ObjectId> = index.staged_data.iter().map(|sd| sd.data_id).collect();

        let mut builder = CommitObject::builder(schema_id, schema.protocol, author, message)
            .theory_ids(theory_ids);
        if !parents.is_empty() {
            builder = builder.parents(parents);
        }
        if let Some(mid) = migration_id {
            builder = builder.migration_id(mid);
        }
        if let Some(pid) = index.staged_protocol {
            builder = builder.protocol_id(pid);
        }
        if !data_ids.is_empty() {
            builder = builder.data_ids(data_ids);
        }
        let commit = builder.build();
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
            // First commit: set the branch ref.
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
                // Theirs is ahead of ours; fast-forward.
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
                    pullback_error: None,
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
            // Verify the merge pushout cocone conditions before committing:
            // both migrations must be total and the base-to-merged paths
            // must commute. A violation fails the merge rather than
            // recording a mathematically invalid schema.
            let resolved = merge::ResolvedMerge {
                schema: result.merged_schema.clone(),
                migration_from_ours: result.migration_from_ours.clone(),
                migration_from_theirs: result.migration_from_theirs.clone(),
            };
            merge::verify_pushout(&base_schema, &ours_schema, &theirs_schema, &resolved)
                .map_err(VcsError::PushoutVerification)?;

            // Validate the merged schema against its registered protocol
            // theory's equations before recording the merge commit.
            let eq_diag = self.schema_equation_diagnostics(&result.merged_schema);
            if eq_diag.has_errors() {
                return Err(VcsError::ValidationFailed {
                    reasons: eq_diag.all_errors(),
                });
            }

            // Lift both parents' data through the merged schema and union
            // the results (data, complements, and CST complements), deduped
            // by ObjectId, rather than set-unioning stale parent data_ids:
            // the merge commit references the merged schema, so the data it
            // carries must conform to that schema, not to a parent's.
            let (data_ids, complement_ids, cst_complement_ids) = self.lift_and_union_parent_data(
                &ours_commit,
                &theirs_commit,
                &ours_schema,
                &theirs_schema,
                &result.merged_schema,
            )?;

            self.write_merge_commit(MergeCommitCtx {
                branch,
                author,
                options,
                ours_id,
                theirs_id,
                ours_schema,
                result: &result,
                data_ids,
                complement_ids,
                cst_complement_ids,
            })?;
        }

        Ok(result)
    }

    /// Assemble and record a clean three-way merge commit, advancing HEAD.
    fn write_merge_commit(&mut self, ctx: MergeCommitCtx<'_>) -> Result<(), VcsError> {
        let merged_schema_id =
            crate::tree::store_schema_as_tree(&mut self.store, ctx.result.merged_schema.clone())?;
        let mig_src = self
            .store
            .put(&Object::FlatSchema(Box::new(ctx.ours_schema)))?;
        let mig_tgt = self.store.put(&Object::FlatSchema(Box::new(
            ctx.result.merged_schema.clone(),
        )))?;
        let merge_migration = ctx.result.migration_from_ours.clone().with_endpoints(
            Some(panproto_gat::Name::from(mig_src.to_string())),
            Some(panproto_gat::Name::from(mig_tgt.to_string())),
        );
        let migration_id = self.store.put(&Object::Migration {
            src: mig_src,
            tgt: mig_tgt,
            mapping: merge_migration,
        })?;

        let msg = ctx
            .options
            .message
            .clone()
            .unwrap_or_else(|| format!("merge branch '{}'", ctx.branch));

        // Store theory for the merged schema.
        let merged_schema = self.load_schema(merged_schema_id)?;
        let merge_theory_ids = self.store_schema_theory(&merged_schema)?;

        let mut merge_builder =
            CommitObject::builder(merged_schema_id, merged_schema.protocol, ctx.author, msg)
                .parents(vec![ctx.ours_id, ctx.theirs_id])
                .migration_id(migration_id)
                .theory_ids(merge_theory_ids);
        if !ctx.data_ids.is_empty() {
            merge_builder = merge_builder.data_ids(ctx.data_ids);
        }
        if !ctx.complement_ids.is_empty() {
            merge_builder = merge_builder.complement_ids(ctx.complement_ids);
        }
        if !ctx.cst_complement_ids.is_empty() {
            merge_builder = merge_builder.cst_complement_ids(ctx.cst_complement_ids);
        }
        let merge_commit = merge_builder.build();
        let merge_id = self.store.put(&Object::Commit(merge_commit))?;
        advance_head(
            &mut self.store,
            ctx.ours_id,
            merge_id,
            ctx.author,
            &format!("merge {}", ctx.branch),
        )?;
        Ok(())
    }

    /// Store the schema's extracted theory and return a protocol→theory-id
    /// map suitable for a commit's `theory_ids`.
    fn store_schema_theory(
        &mut self,
        schema: &Schema,
    ) -> Result<std::collections::BTreeMap<String, ObjectId>, VcsError> {
        let theory = crate::gat_validate::schema_to_theory(&schema.protocol, schema);
        let theory_id = self.store.put(&Object::Theory(Box::new(theory)))?;
        let mut theory_ids = std::collections::BTreeMap::new();
        theory_ids.insert(schema.protocol.clone(), theory_id);
        Ok(theory_ids)
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

        let mut builder = CommitObject::builder(schema_id, old_commit.protocol, author, message);
        if !old_commit.parents.is_empty() {
            builder = builder.parents(old_commit.parents);
        }
        if let Some(mid) = migration_id {
            builder = builder.migration_id(mid);
        }
        if let Some(pid) = old_commit.protocol_id {
            builder = builder.protocol_id(pid);
        }
        if !old_commit.data_ids.is_empty() {
            builder = builder.data_ids(old_commit.data_ids);
        }
        if !old_commit.complement_ids.is_empty() {
            builder = builder.complement_ids(old_commit.complement_ids);
        }
        if !old_commit.edit_log_ids.is_empty() {
            builder = builder.edit_log_ids(old_commit.edit_log_ids);
        }
        let new_commit = builder.build();
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
    /// and updates the index. The data set is keyed by `key`, or by the
    /// source path when `key` is `None`, so a committed set read back via
    /// [`data_at`](Self::data_at) can be mapped to its origin.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read, or if no schema is
    /// available (nothing staged and no HEAD commit).
    pub fn add_data(&mut self, path: &Path, key: Option<&str>) -> Result<Index, VcsError> {
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

        // Fall back to the source path so every staged set carries a key.
        let key = key.map_or_else(|| path.to_string_lossy().into_owned(), str::to_owned);

        let dataset = DataSetObject {
            schema_id,
            data: data_bytes,
            record_count,
            key: Some(key),
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

    /// Checkout a branch and migrate data files to match.
    ///
    /// Resolves the target ref, switches HEAD, and, when the target
    /// schema differs from the current schema, migrates every `.json`
    /// file in `data_dir` forward through a lens.
    ///
    /// # Errors
    ///
    /// Returns an error if the ref cannot be resolved, HEAD cannot be
    /// read, or data migration fails.
    pub fn checkout_with_data(&mut self, target: &str, data_dir: &Path) -> Result<(), VcsError> {
        // 1. Resolve current HEAD
        let current_id = store::resolve_head(&self.store)?.ok_or(VcsError::NothingStaged)?;
        let current_commit = self.load_commit(current_id)?;
        let current_schema = self.load_schema(current_commit.schema_id)?;

        // 2. Resolve the target and do the checkout
        let target_id = refs::resolve_ref(&self.store, target)?;
        let target_commit = self.load_commit(target_id)?;
        let target_schema = self.load_schema(target_commit.schema_id)?;

        // Switch HEAD to the target branch/commit
        refs::checkout_branch(&mut self.store, target)?;

        // 3. If schemas differ, migrate data files
        if current_commit.schema_id != target_commit.schema_id && data_dir.is_dir() {
            let protocol = crate::data_mig::protocol_for_schema(&current_schema);
            crate::data_mig::migrate_data_directory(
                &mut self.store,
                data_dir,
                &current_schema,
                &target_schema,
                &protocol,
            )?;
        }

        Ok(())
    }

    /// Read the data sets committed at `reference` without moving `HEAD`.
    ///
    /// Resolves `reference` (branch, tag, or commit-id prefix) to a
    /// commit and returns every [`DataSetObject`] recorded at it. This is
    /// the data counterpart to reading a committed schema: unlike
    /// [`checkout_with_data`](Self::checkout_with_data) it never changes
    /// `HEAD`, the index, or any file in the working tree. The data is
    /// already content-addressed, so this is a plain store walk.
    ///
    /// # Errors
    ///
    /// Returns an error if `reference` cannot be resolved, the resolved
    /// object is not a commit, or one of its recorded data sets is
    /// missing or of the wrong object type.
    pub fn data_at(&self, reference: &str) -> Result<Vec<DataSetObject>, VcsError> {
        let commit_id = refs::resolve_ref(&self.store, reference)?;
        let commit = self.load_commit(commit_id)?;
        let mut datasets = Vec::with_capacity(commit.data_ids.len());
        for data_id in &commit.data_ids {
            match self.store.get(data_id)? {
                Object::DataSet(ds) => datasets.push(ds),
                other => {
                    return Err(VcsError::WrongObjectType {
                        expected: "dataset",
                        found: other.type_name(),
                    });
                }
            }
        }
        Ok(datasets)
    }

    /// Merge a branch into the current branch and migrate data files.
    ///
    /// Performs the schema merge via [`merge_with_options`](Self::merge_with_options),
    /// then, if the merge produced a schema change and `data_dir`
    /// exists, migrates every `.json` file in `data_dir` to the
    /// merged schema.
    ///
    /// # Errors
    ///
    /// Returns an error if the merge fails or data migration fails.
    pub fn merge_with_data(
        &mut self,
        branch: &str,
        author: &str,
        data_dir: &Path,
        opts: &merge::MergeOptions,
    ) -> Result<merge::MergeResult, VcsError> {
        // Capture pre-merge HEAD schema
        let pre_merge_id =
            store::resolve_head(&self.store)?.ok_or_else(|| VcsError::RefNotFound {
                name: "HEAD".to_owned(),
            })?;
        let pre_merge_commit = self.load_commit(pre_merge_id)?;
        let pre_merge_schema = self.load_schema(pre_merge_commit.schema_id)?;

        // Do the schema merge
        let result = self.merge_with_options(branch, author, opts)?;

        // If merge succeeded and data_dir exists, migrate data
        if data_dir.is_dir() {
            let head_id = store::resolve_head(&self.store)?.ok_or(VcsError::NothingStaged)?;
            let head_commit = self.load_commit(head_id)?;

            if pre_merge_commit.schema_id != head_commit.schema_id {
                let merged_schema = self.load_schema(head_commit.schema_id)?;
                let protocol = crate::data_mig::protocol_for_schema(&pre_merge_schema);
                crate::data_mig::migrate_data_directory(
                    &mut self.store,
                    data_dir,
                    &pre_merge_schema,
                    &merged_schema,
                    &protocol,
                )?;
            }
        }

        Ok(result)
    }

    // -- internal helpers --

    /// Lift both parents' versioned data through the merged schema and
    /// union the results, deduped by [`ObjectId`].
    ///
    /// Returns `(data_ids, complement_ids, cst_complement_ids)` for the
    /// merge commit: each parent's data sets lifted from its own schema to
    /// `merged_schema` (returned unchanged when a parent did not change the
    /// schema), its complements extended with the fresh backward-migration
    /// complements, and both parents' CST complements carried through
    /// unchanged (they are keyed by content, not schema).
    fn lift_and_union_parent_data(
        &mut self,
        ours_commit: &CommitObject,
        theirs_commit: &CommitObject,
        ours_schema: &Schema,
        theirs_schema: &Schema,
        merged_schema: &Schema,
    ) -> Result<MergedCommitData, VcsError> {
        let (ours_data_ids, ours_complement_ids) = crate::data_mig::lift_commit_data(
            &mut self.store,
            ours_commit,
            ours_schema,
            merged_schema,
        )?;
        let (theirs_data_ids, theirs_complement_ids) = crate::data_mig::lift_commit_data(
            &mut self.store,
            theirs_commit,
            theirs_schema,
            merged_schema,
        )?;

        let mut data_ids = ours_data_ids;
        for id in theirs_data_ids {
            if !data_ids.contains(&id) {
                data_ids.push(id);
            }
        }
        let mut complement_ids = ours_complement_ids;
        for id in theirs_complement_ids {
            if !complement_ids.contains(&id) {
                complement_ids.push(id);
            }
        }
        let mut cst_complement_ids = ours_commit.cst_complement_ids.clone();
        for id in &theirs_commit.cst_complement_ids {
            if !cst_complement_ids.contains(id) {
                cst_complement_ids.push(*id);
            }
        }
        Ok((data_ids, complement_ids, cst_complement_ids))
    }

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
        let proto = crate::tree::project_coproduct_protocol();
        crate::tree::assemble_schema(&self.store, &id, &proto)
    }

    fn index_path(&self) -> PathBuf {
        self.store.root().join("index.json")
    }

    /// Read the staging index from the working tree.
    ///
    /// Returns an empty `Index` if no index file exists yet.
    ///
    /// # Errors
    ///
    /// Returns an error if the index file exists but cannot be parsed.
    pub fn read_index(&self) -> Result<Index, VcsError> {
        let path = self.index_path();
        if !path.exists() {
            return Ok(Index::default());
        }
        let json = std::fs::read_to_string(&path)?;
        serde_json::from_str(&json)
            .map_err(|e| VcsError::Serialization(crate::error::SerializationError(e.to_string())))
    }

    /// Write `index` to the working tree's index file.
    ///
    /// # Errors
    ///
    /// Returns an error if serialisation or I/O fails.
    pub fn write_index(&self, index: &Index) -> Result<(), VcsError> {
        let json = serde_json::to_string_pretty(index).map_err(|e| {
            VcsError::Serialization(crate::error::SerializationError(e.to_string()))
        })?;
        std::fs::write(self.index_path(), json)?;
        Ok(())
    }

    /// Clear the staging index, persisting the empty state to disk.
    ///
    /// # Errors
    ///
    /// Returns an error if writing the index file fails.
    pub fn clear_index(&self) -> Result<(), VcsError> {
        self.write_index(&Index::default())
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
    fn add_skip_verify_leaves_stage_pending_but_records_migration()
    -> Result<(), Box<dyn std::error::Error>> {
        use crate::index::ValidationStatus;

        let dir = tempfile::tempdir()?;
        let mut repo = Repository::init(dir.path())?;

        let s1 = make_schema(&[("a", "object")]);
        repo.add(&s1)?;
        repo.commit("first", "alice")?;

        // Staged with skip_verify: the migration is still derived and
        // recorded, but the (bounded model-checking) validation is skipped
        // and the stage is left Pending.
        let s2 = make_schema(&[("a", "object"), ("b", "string")]);
        let index = repo.add_with_options(&s2, &AddOptions { skip_verify: true })?;
        let staged = index.staged.as_ref().ok_or("nothing staged")?;
        assert!(
            matches!(staged.validation, ValidationStatus::Pending),
            "skip_verify should leave the stage pending, got {:?}",
            staged.validation
        );
        assert!(
            staged.gat_diagnostics.is_none(),
            "no diagnostics are computed when skipping"
        );
        assert!(
            staged.migration_id.is_some(),
            "the derived migration is still recorded"
        );

        // A default commit accepts a Pending stage (it is non-blocking).
        repo.commit("second", "alice")?;
        assert_eq!(repo.log(None)?.len(), 2);

        // The default add path still runs validation (not Pending).
        let s3 = make_schema(&[("a", "object"), ("b", "string"), ("c", "string")]);
        let index = repo.add(&s3)?;
        let staged = index.staged.as_ref().ok_or("nothing staged")?;
        assert!(
            !matches!(staged.validation, ValidationStatus::Pending),
            "the default add must run validation, but the stage is Pending"
        );
        assert!(staged.gat_diagnostics.is_some());
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
        let schema_id = crate::tree::store_schema_as_tree(&mut repo.store, staged_schema)?;

        let diag = GatDiagnostics {
            type_errors: vec!["sort mismatch: expected Ob, got Hom".to_owned()],
            equation_errors: vec![],
            migration_warnings: vec![],
            ..Default::default()
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
        let schema_id = crate::tree::store_schema_as_tree(&mut repo.store, staged_schema)?;

        let diag = GatDiagnostics {
            type_errors: vec![],
            equation_errors: vec!["equation 'assoc' violated when f=id: LHS=a, RHS=b".to_owned()],
            migration_warnings: vec![],
            ..Default::default()
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
        let index = repo.add_data(&data_path, None)?;
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
    fn data_at_reads_committed_data_without_moving_head() -> Result<(), Box<dyn std::error::Error>>
    {
        let dir = tempfile::tempdir()?;
        let mut repo = Repository::init(dir.path())?;

        // First commit: a schema, no data.
        let s = make_schema(&[("a", "object"), ("b", "string")]);
        repo.add(&s)?;
        let schema_only = repo.commit("initial schema", "alice")?;

        // A ref with no committed data returns an empty list.
        assert!(repo.data_at(&schema_only.to_string())?.is_empty());

        // Stage and commit a data file with an explicit key.
        let payload = br#"[{"a": 1}, {"a": 2}, {"a": 3}]"#;
        let data_path = dir.path().join("data.json");
        std::fs::write(&data_path, payload)?;
        repo.add_data(&data_path, Some("records-key"))?;
        let s2 = make_schema(&[("a", "object"), ("b", "string"), ("c", "number")]);
        repo.add(&s2)?;
        let commit_id = repo.commit("add data", "alice")?;

        let head_before = store::resolve_head(repo.store())?;

        // The committed data is readable by branch name, "HEAD", and commit id.
        for reference in ["main", "HEAD", &commit_id.to_string()] {
            let datasets = repo.data_at(reference)?;
            assert_eq!(datasets.len(), 1, "ref {reference}");
            assert_eq!(datasets[0].record_count, 3, "ref {reference}");
            assert_eq!(datasets[0].data, payload, "ref {reference}");
            assert_eq!(
                datasets[0].key.as_deref(),
                Some("records-key"),
                "ref {reference}"
            );
        }

        // Reading never moved HEAD.
        assert_eq!(store::resolve_head(repo.store())?, head_before);

        // An unresolvable ref is an error, not a panic or empty result.
        assert!(repo.data_at("no-such-ref").is_err());
        Ok(())
    }

    #[test]
    fn data_only_commit_carries_schema_forward() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let mut repo = Repository::init(dir.path())?;

        // First commit: a schema, no data.
        let s = make_schema(&[("a", "object"), ("b", "string")]);
        repo.add(&s)?;
        repo.commit("schema", "alice")?;

        // Stage data with no schema change.
        let payload = br#"[{"a": 1}, {"a": 2}]"#;
        let data_path = dir.path().join("rec.json");
        std::fs::write(&data_path, payload)?;
        let index = repo.add_data(&data_path, Some("at://rec/1"))?;
        assert!(index.has_staged(), "data should register as staged");
        assert!(index.staged.is_none(), "no schema is staged");

        // `commit` and `has_staged` now agree: a data-only stage commits
        // instead of failing with NothingStaged.
        repo.commit("data only", "alice")?;

        // The data-only commit carries the parent's schema forward, with
        // no migration, and tracks the data.
        let log = repo.log(None)?;
        assert_eq!(log[0].message, "data only");
        assert_eq!(
            log[0].schema_id, log[1].schema_id,
            "schema carried forward unchanged"
        );
        assert!(
            log[0].migration_id.is_none(),
            "a data-only commit has no migration"
        );
        assert_eq!(log[0].data_ids.len(), 1);

        // The committed data reads back with its key (issue #198 path).
        let datasets = repo.data_at("HEAD")?;
        assert_eq!(datasets.len(), 1);
        assert_eq!(datasets[0].record_count, 2);
        assert_eq!(datasets[0].key.as_deref(), Some("at://rec/1"));

        // A truly empty index still rejects.
        assert!(matches!(
            repo.commit("nothing", "alice"),
            Err(VcsError::NothingStaged)
        ));
        Ok(())
    }

    #[test]
    fn add_data_defaults_key_to_source_path() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let mut repo = Repository::init(dir.path())?;
        let s = make_schema(&[("a", "object")]);
        repo.add(&s)?;
        repo.commit("schema", "alice")?;

        let data_path = dir.path().join("rec.json");
        std::fs::write(&data_path, br#"[{"a": 1}]"#)?;
        repo.add_data(&data_path, None)?;
        repo.commit("data", "alice")?;

        let datasets = repo.data_at("HEAD")?;
        assert_eq!(
            datasets[0].key.as_deref(),
            Some(data_path.to_string_lossy().as_ref()),
            "key defaults to the source path when none is given"
        );
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

    /// Store a one-record data set (a single node anchored at `a`) valid
    /// against `schema`, returning its object id.
    fn single_record_dataset(
        store: &mut FsStore,
        schema: &Schema,
        key: &str,
    ) -> Result<ObjectId, VcsError> {
        use panproto_inst::{Node, WInstance};
        let mut nodes = HashMap::new();
        nodes.insert(0_u32, Node::new(0, "a"));
        let inst = WInstance::new(nodes, vec![], vec![], 0, Name::from("a"));
        let ds = DataSetObject {
            schema_id: crate::hash::hash_schema(schema)?,
            data: rmp_serde::to_vec(&vec![inst])?,
            record_count: 1,
            key: Some(key.to_owned()),
        };
        store.put(&Object::DataSet(ds))
    }

    #[test]
    fn merge_lifts_data_instead_of_unioning() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let mut repo = Repository::init(dir.path())?;

        // Base {a}.
        let s0 = make_schema(&[("a", "object")]);
        let s0_id = crate::tree::store_schema_as_tree(repo.store_mut(), s0)?;
        let c0 = CommitObject::builder(s0_id, "test", "alice", "base")
            .timestamp(100)
            .build();
        let c0_id = repo.store_mut().put(&Object::Commit(c0))?;

        // ours: {a, oursf} with its own data set.
        let s_ours = make_schema(&[("a", "object"), ("oursf", "string")]);
        let ds_ours = single_record_dataset(repo.store_mut(), &s_ours, "ours-key")?;
        let s_ours_id = crate::tree::store_schema_as_tree(repo.store_mut(), s_ours)?;
        let c_ours = CommitObject::builder(s_ours_id, "test", "alice", "ours change + data")
            .parents(vec![c0_id])
            .timestamp(200)
            .data_ids(vec![ds_ours])
            .build();
        let c_ours_id = repo.store_mut().put(&Object::Commit(c_ours))?;

        // theirs: {a, theirsf} with its own data set.
        let s_theirs = make_schema(&[("a", "object"), ("theirsf", "string")]);
        let ds_theirs = single_record_dataset(repo.store_mut(), &s_theirs, "theirs-key")?;
        let s_theirs_id = crate::tree::store_schema_as_tree(repo.store_mut(), s_theirs)?;
        let c_theirs = CommitObject::builder(s_theirs_id, "test", "bob", "theirs change + data")
            .parents(vec![c0_id])
            .timestamp(300)
            .data_ids(vec![ds_theirs])
            .build();
        let c_theirs_id = repo.store_mut().put(&Object::Commit(c_theirs))?;

        // HEAD (main) is ours; feature is theirs.
        repo.store_mut().set_ref("refs/heads/main", c_ours_id)?;
        refs::create_branch(repo.store_mut(), "feature", c_theirs_id)?;

        let result = repo.merge("feature", "alice")?;
        assert!(result.conflicts.is_empty(), "clean merge expected");

        // The merge commit's data was lifted, not unioned stale: neither
        // original data id survives verbatim.
        let Some(head_id) = store::resolve_head(repo.store())? else {
            panic!("merge should leave HEAD set");
        };
        let head_commit = repo.load_commit(head_id)?;
        assert_eq!(head_commit.parents.len(), 2);
        assert_eq!(head_commit.data_ids.len(), 2, "both parents' data carried");
        assert!(
            !head_commit.data_ids.contains(&ds_ours) && !head_commit.data_ids.contains(&ds_theirs),
            "data ids were lifted to fresh objects, not unioned stale"
        );

        // Every merged data set deserializes as Vec<WInstance> valid against
        // the merged schema and preserves its source record_count and key.
        let merged_schema = repo.load_schema(head_commit.schema_id)?;
        let datasets = repo.data_at("HEAD")?;
        assert_eq!(datasets.len(), 2);
        let mut keys: Vec<String> = Vec::new();
        for ds in &datasets {
            assert_eq!(ds.record_count, 1);
            let insts: Vec<panproto_inst::WInstance> = rmp_serde::from_slice(&ds.data)?;
            assert_eq!(insts.len(), 1);
            for inst in &insts {
                assert!(
                    panproto_inst::validate_wtype(&merged_schema, inst).is_empty(),
                    "lifted instance must be valid against the merged schema"
                );
            }
            if let Some(k) = &ds.key {
                keys.push(k.clone());
            }
        }
        keys.sort();
        assert_eq!(keys, vec!["ours-key".to_owned(), "theirs-key".to_owned()]);
        Ok(())
    }

    #[test]
    fn merge_identical_schemas_propagates_data_unchanged() -> Result<(), Box<dyn std::error::Error>>
    {
        let dir = tempfile::tempdir()?;
        let mut repo = Repository::init(dir.path())?;

        // A single shared schema tree: neither parent changes the schema.
        let s0 = make_schema(&[("a", "object")]);
        let s0_id = crate::tree::store_schema_as_tree(repo.store_mut(), s0.clone())?;
        let c0 = CommitObject::builder(s0_id, "test", "alice", "base")
            .timestamp(100)
            .build();
        let c0_id = repo.store_mut().put(&Object::Commit(c0))?;

        let ds_ours = single_record_dataset(repo.store_mut(), &s0, "ours-key")?;
        let c_ours = CommitObject::builder(s0_id, "test", "alice", "ours data")
            .parents(vec![c0_id])
            .timestamp(200)
            .data_ids(vec![ds_ours])
            .build();
        let c_ours_id = repo.store_mut().put(&Object::Commit(c_ours))?;

        let ds_theirs = single_record_dataset(repo.store_mut(), &s0, "theirs-key")?;
        let c_theirs = CommitObject::builder(s0_id, "test", "bob", "theirs data")
            .parents(vec![c0_id])
            .timestamp(300)
            .data_ids(vec![ds_theirs])
            .build();
        let c_theirs_id = repo.store_mut().put(&Object::Commit(c_theirs))?;

        repo.store_mut().set_ref("refs/heads/main", c_ours_id)?;
        refs::create_branch(repo.store_mut(), "feature", c_theirs_id)?;

        repo.merge("feature", "alice")?;

        // Neither parent changed the schema, so the original data ids appear
        // verbatim on the merge commit.
        let Some(head_id) = store::resolve_head(repo.store())? else {
            panic!("merge should leave HEAD set");
        };
        let head_commit = repo.load_commit(head_id)?;
        assert_eq!(head_commit.data_ids.len(), 2);
        assert!(head_commit.data_ids.contains(&ds_ours));
        assert!(head_commit.data_ids.contains(&ds_theirs));
        Ok(())
    }

    /// Build a schema of `protocol` from `(id, kind)` vertices and
    /// `(src, tgt, name)` edges.
    fn schema_with_edges(
        protocol: &str,
        vertices: &[(&str, &str)],
        edges: &[(&str, &str, &str)],
    ) -> Schema {
        let mut schema = make_schema(vertices);
        schema.protocol = protocol.to_owned();
        for (src, tgt, name) in edges {
            let e = panproto_schema::Edge {
                src: Name::from(*src),
                tgt: Name::from(*tgt),
                kind: "prop".into(),
                name: Some(Name::from(*name)),
            };
            schema.edges.insert(e.clone(), e.kind.clone());
        }
        schema
    }

    fn f_identity_theory() -> Theory {
        use panproto_gat::{Equation, Operation, Sort, Term};
        Theory::new(
            "p",
            vec![Sort::simple("Node")],
            vec![Operation::unary("f", "x", "Node", "Node")],
            vec![Equation::new(
                "f_is_identity",
                Term::app("f", vec![Term::var("x")]),
                Term::var("x"),
            )],
        )
    }

    #[test]
    fn commit_blocks_on_invalid_migration_structure() -> Result<(), Box<dyn std::error::Error>> {
        use crate::gat_validate::GatDiagnostics;
        use crate::index::{Index, StagedSchema, ValidationStatus};

        let dir = tempfile::tempdir()?;
        let mut repo = Repository::init(dir.path())?;
        let s = make_schema(&[("a", "object")]);
        repo.add(&s)?;
        repo.commit("initial", "alice")?;

        // Stage an index whose migration validation carries a blocking
        // structural error (a nonexistent source vertex).
        let staged_schema = make_schema(&[("a", "object"), ("b", "string")]);
        let schema_id = crate::tree::store_schema_as_tree(&mut repo.store, staged_schema)?;
        let diag = GatDiagnostics {
            migration_errors: vec![
                "vertex map references source vertex 'ghost' which does not exist in source schema"
                    .to_owned(),
            ],
            ..Default::default()
        };
        assert!(!diag.is_clean(), "migration_errors must block");
        let index = Index {
            staged: Some(StagedSchema {
                schema_id,
                migration_id: None,
                auto_derived: true,
                validation: ValidationStatus::Invalid(diag.all_errors()),
                gat_diagnostics: Some(diag),
            }),
            staged_data: vec![],
            staged_protocol: None,
        };
        repo.write_index(&index)?;

        let Err(err) = repo.commit("should fail", "alice") else {
            panic!("commit must fail on an invalid migration structure");
        };
        assert!(matches!(&err, VcsError::ValidationFailed { reasons } if !reasons.is_empty()));

        // skip_verify bypasses the structural gate.
        let opts = CommitOptions { skip_verify: true };
        repo.commit_with_options("forced", "alice", &opts)?;
        Ok(())
    }

    #[test]
    fn commit_blocks_on_protocol_equation_violation() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let mut repo = Repository::init(dir.path())?;
        repo.set_protocol_theory("p", f_identity_theory());

        // Schema whose `f` edge is not the identity, violating f(x) = x.
        let schema = schema_with_edges(
            "p",
            &[("root", "Node"), ("a", "Node")],
            &[("root", "a", "f")],
        );
        repo.add(&schema)?;

        let Err(err) = repo.commit("bad", "alice") else {
            panic!("commit must fail on a protocol equation violation");
        };
        assert!(
            matches!(&err, VcsError::ValidationFailed { reasons }
                if reasons.iter().any(|r| r.contains("equation violation"))),
            "expected an equation violation, got: {err:?}"
        );

        // skip_verify bypasses the equation gate.
        let opts = CommitOptions { skip_verify: true };
        repo.commit_with_options("forced", "alice", &opts)?;
        Ok(())
    }

    #[test]
    fn unregistered_protocol_reports_unchecked_equations() -> Result<(), Box<dyn std::error::Error>>
    {
        let dir = tempfile::tempdir()?;
        let mut repo = Repository::init(dir.path())?;
        // No theory registered for protocol "test".
        let s = make_schema(&[("a", "object")]);
        let index = repo.add(&s)?;
        let diag = index
            .staged
            .as_ref()
            .and_then(|st| st.gat_diagnostics.as_ref())
            .ok_or("staged diagnostics expected")?;
        assert!(
            diag.equation_notes
                .iter()
                .any(|n| n.contains("no protocol theory registered")),
            "expected an advisory note, got: {diag:?}"
        );
        assert!(diag.is_clean(), "an advisory note must not block");
        // The commit still succeeds without a registered theory.
        repo.commit("ok", "alice")?;
        Ok(())
    }

    #[test]
    fn hom_search_candidate_rejected_falls_back() {
        // The staging path only adopts a hom_search candidate that is a
        // valid theory morphism. A crossed candidate — one whose edge map
        // does not connect the images of its endpoints — is rejected by
        // the same check the staging path applies, so the diff-derived
        // migration is used instead.
        let old = schema_with_edges(
            "test",
            &[("a", "object"), ("b", "string")],
            &[("a", "b", "x")],
        );
        let new = schema_with_edges(
            "test",
            &[("a2", "object"), ("b2", "string"), ("c2", "object")],
            &[("c2", "b2", "x")],
        );

        let crossed_edge_src = panproto_schema::Edge {
            src: "a".into(),
            tgt: "b".into(),
            kind: "prop".into(),
            name: Some("x".into()),
        };
        let crossed_edge_tgt = panproto_schema::Edge {
            src: "c2".into(),
            tgt: "b2".into(),
            kind: "prop".into(),
            name: Some("x".into()),
        };
        let crossed = panproto_mig::Migration {
            vertex_map: HashMap::from([
                (Name::from("a"), Name::from("a2")),
                (Name::from("b"), Name::from("b2")),
            ]),
            edge_map: HashMap::from([(crossed_edge_src, crossed_edge_tgt)]),
            hyper_edge_map: HashMap::new(),
            label_map: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            expr_resolvers: HashMap::new(),
            domain: None,
            codomain: None,
        };

        let (dom, cod, morph) = panproto_mig::induced_theory_morphism(&old, &new, &crossed);
        assert!(
            panproto_gat::check_morphism(&morph, &dom, &cod).is_err(),
            "the guard must reject a crossed candidate"
        );

        // Auto-derivation of a genuine rename yields a valid morphism, so
        // the stored migration always passes the guard.
        let rename_old = schema_with_edges("test", &[("post", "object"), ("text", "string")], &[]);
        let rename_new = schema_with_edges("test", &[("note", "object"), ("text", "string")], &[]);
        let d = panproto_check::diff::diff(&rename_old, &rename_new);
        let mig = auto_mig::derive_migration(&rename_old, &rename_new, &d);
        let (dom2, cod2, morph2) =
            panproto_mig::induced_theory_morphism(&rename_old, &rename_new, &mig);
        assert!(
            panproto_gat::check_morphism(&morph2, &dom2, &cod2).is_ok(),
            "an adopted auto-derived migration must be a valid morphism"
        );
    }
}
