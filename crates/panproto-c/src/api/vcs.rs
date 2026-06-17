//! Schematic version control operations.
//!
//! The repo handle is a mutable
//! [`Resource::VcsRepo`](crate::handle::Resource) wrapping a
//! `panproto_core::vcs::Repository`: the on-disk store backed by an
//! `FsStore`, with a real staging index. The porcelain methods of
//! [`Repository`] drive every operation; the lower-level plumbing
//! modules (`refs`, `stash`, `blame`, `dag`, `store`) act on the
//! repository's `Store` via its `store` / `store_mut` accessors.
//!
//! # Wire format
//!
//! Each operation that returns data writes a CBOR-encoded result record
//! to its `out` parameter. The record shapes here mirror the Haskell
//! `Panproto.Vcs` decoders: object ids cross the boundary as their
//! lowercase-hex `Display` rendering (a `String`), never as the raw
//! `[u8; 32]` `serde` array, and HEAD state crosses as the
//! externally-tagged `panproto_core::vcs::HeadState` enum
//! (`{"Branch": "main"}` / `{"Detached": "<hex>"}`). The local result
//! types below own that wire shape.

use panproto_core::vcs::{self, Repository, Store as _};
use safer_ffi::prelude::*;
use serde::Serialize;

use crate::error::{FfiError, PpStatus};
use crate::handle::{self, Resource};
use crate::panic::guard;

// ---------------------------------------------------------------------------
// Wire result records
//
// These mirror the Haskell `Panproto.Vcs` decoders. They serialize
// object ids as hex strings. Missing data is filled with the same
// neutral defaults the decoders fall back to, so the two sides agree.

/// The `vcs_add` result, matching the Haskell `VcsAddResult` decoder:
/// the staged schema's object id, whether a migration from HEAD was
/// auto-derived, the validation verdict, and any validation messages.
#[derive(Debug, Serialize)]
struct AddResultRecord {
    schema_id: String,
    auto_derived: bool,
    valid: bool,
    validation_messages: Vec<String>,
}

/// The `vcs_commit` result, matching the Haskell `VcsCommitResult`
/// decoder: the new commit id plus the metadata recorded for it.
#[derive(Debug, Serialize)]
struct CommitResultRecord {
    commit_id: String,
    message: String,
    author: String,
    timestamp: u64,
}

/// A single commit-log entry, matching the Haskell `LogEntry` decoder.
#[derive(Debug, Serialize)]
struct LogEntryRecord {
    commit_id: String,
    parents: Vec<String>,
    author: String,
    timestamp: u64,
    message: String,
    protocol: String,
    schema_id: String,
}

/// The `vcs_log` result, matching the Haskell `VcsLogResult` decoder
/// (a map with an `entries` list, not a bare array).
#[derive(Debug, Serialize)]
struct LogResultRecord {
    entries: Vec<LogEntryRecord>,
}

/// The `vcs_status` result, matching the Haskell `VcsStatus` decoder:
/// HEAD state as the externally-tagged `HeadState` enum plus the
/// resolved HEAD commit and the staging / working booleans.
#[derive(Debug, Serialize)]
struct StatusRecord {
    head_ref: vcs::HeadState,
    head_commit: Option<String>,
    has_staged: bool,
    working_dirty: bool,
}

/// A single branch entry, matching the Haskell `BranchInfo` decoder.
#[derive(Debug, Serialize)]
struct BranchInfoRecord {
    name: String,
    target: String,
    is_current: bool,
}

/// The `vcs_branch` result, matching the Haskell `VcsBranchResult`
/// decoder: the full branch listing after the operation.
#[derive(Debug, Serialize)]
struct BranchResultRecord {
    branches: Vec<BranchInfoRecord>,
}

/// The `vcs_diff` result, matching the Haskell `VcsDiffResult` decoder:
/// structural counts plus human-readable change descriptions, computed
/// from the `panproto_check::diff::SchemaDiff` between two refs.
#[derive(Debug, Serialize)]
struct DiffResultRecord {
    added: u64,
    removed: u64,
    modified: u64,
    changes: Vec<String>,
}

/// A generic op result, matching the Haskell `VcsOpResult` decoder:
/// success flag, resulting HEAD state, and informational messages.
#[derive(Debug, Serialize)]
struct OpResultRecord {
    ok: bool,
    head: vcs::HeadState,
    messages: Vec<String>,
}

/// The `vcs_merge` result, matching the Haskell `VcsMergeResult`
/// decoder.
#[derive(Debug, Serialize)]
struct MergeResultRecord {
    fast_forward: bool,
    merge_commit: Option<String>,
    conflicts: Vec<String>,
}

/// A single stash entry, matching the Haskell `StashEntry` decoder.
#[derive(Debug, Serialize)]
struct StashEntryRecord {
    index: u64,
    commit_id: String,
    message: String,
    timestamp: u64,
}

/// The `vcs_stash` result, matching the Haskell `VcsStashResult`
/// decoder.
#[derive(Debug, Serialize)]
struct StashResultRecord {
    stashed: StashEntryRecord,
    stack: Vec<StashEntryRecord>,
}

/// The `vcs_stash_pop` result, matching the Haskell `VcsStashPopResult`
/// decoder.
#[derive(Debug, Serialize)]
struct StashPopResultRecord {
    restored_schema_id: String,
    stack: Vec<StashEntryRecord>,
}

/// The `vcs_blame` result, matching the Haskell `BlameReport` decoder.
#[derive(Debug, Serialize)]
struct BlameRecord {
    commit_id: String,
    author: String,
    timestamp: u64,
    message: String,
}

/// Read the full branch listing as wire records, tagging the branch HEAD
/// currently tracks.
fn list_branch_records(repo: &Repository) -> Result<Vec<BranchInfoRecord>, FfiError> {
    let store = repo.store();
    let current = match store
        .get_head()
        .map_err(|e| FfiError::Operation(e.to_string()))?
    {
        vcs::HeadState::Branch(name) => Some(name),
        vcs::HeadState::Detached(_) => None,
    };
    let branches =
        vcs::refs::list_branches(store).map_err(|e| FfiError::Operation(e.to_string()))?;
    Ok(branches
        .into_iter()
        .map(|(name, id)| BranchInfoRecord {
            is_current: current.as_deref() == Some(name.as_str()),
            name,
            target: id.to_string(),
        })
        .collect())
}

/// Read the stash stack as indexed wire records.
fn stash_stack_records(repo: &Repository) -> Result<Vec<StashEntryRecord>, FfiError> {
    Ok(vcs::stash::stash_list(repo.store())
        .map_err(|e| FfiError::Operation(e.to_string()))?
        .into_iter()
        .map(|entry| StashEntryRecord {
            index: entry.index as u64,
            commit_id: entry.commit_id.to_string(),
            message: entry.message,
            timestamp: entry.timestamp,
        })
        .collect())
}

// ---------------------------------------------------------------------------
// VCS operations
// ---------------------------------------------------------------------------

/// Open or initialize an on-disk VCS repository at a filesystem path.
///
/// `path` is the UTF-8 path bytes of the repository's working directory.
/// If a `.panproto/` store already exists there, the repository is opened
/// via `Repository::open`; otherwise it is created via `Repository::init`
/// (which writes the `.panproto/` directory structure and sets HEAD to
/// `main`). On success, `out_handle` receives a fresh
/// [`Resource::VcsRepo`](crate::handle::Resource) handle wrapping the
/// `Repository`.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_vcs_init(path: c_slice::Ref<'_, u8>, out_handle: &mut u32) -> i32 {
    guard(|| {
        let path_str = std::str::from_utf8(path.as_slice())
            .map_err(|e| FfiError::Operation(format!("invalid path: {e}")))?;
        let dir = std::path::Path::new(path_str);

        // An existing `.panproto/` store means the repo is already there.
        let repo = if dir.join(".panproto").is_dir() {
            Repository::open(dir).map_err(|e| FfiError::Operation(e.to_string()))?
        } else {
            Repository::init(dir).map_err(|e| FfiError::Operation(e.to_string()))?
        };

        *out_handle = handle::alloc(Resource::VcsRepo(Box::new(repo)));
        Ok(PpStatus::Ok)
    })
}

/// Stage a schema in a VCS repository.
///
/// `repo` is a VCS repo handle; `schema` is a
/// [`Resource::Schema`](crate::handle::Resource) handle. On success,
/// `out` receives a CBOR-encoded add result carrying the staged schema's
/// object id, whether a migration from HEAD was auto-derived, the
/// validation verdict, and any validation messages. Calls
/// `Repository::add`.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_vcs_add(repo: u32, schema: u32, out: &mut repr_c::Vec<u8>) -> i32 {
    guard(|| {
        // Clone the schema out of its handle first, then take the
        // mutable borrow on the repo (two distinct slab entries).
        let schema_val = handle::with_resource(schema, |r| Ok(r.as_schema()?.clone()))?;

        let result = handle::with_resource_mut(repo, |r| {
            let repository = r.as_vcs_repo_mut()?;
            let index = repository
                .add(&schema_val)
                .map_err(|e| FfiError::Operation(e.to_string()))?;
            let staged = index.staged.ok_or_else(|| {
                FfiError::Operation("add did not record a staged schema".to_owned())
            })?;
            let (valid, messages) = match staged.validation {
                vcs::index::ValidationStatus::Valid | vcs::index::ValidationStatus::Pending => {
                    (true, Vec::new())
                }
                vcs::index::ValidationStatus::Invalid(reasons) => (false, reasons),
            };
            Ok(AddResultRecord {
                schema_id: staged.schema_id.to_string(),
                auto_derived: staged.auto_derived,
                valid,
                validation_messages: messages,
            })
        })?;

        *out = crate::canonical::encode(&result)?.into();
        Ok(PpStatus::Ok)
    })
}

/// Commit the staged schema in a VCS repository.
///
/// `repo` is a VCS repo handle; `message` and `author` are UTF-8 bytes.
/// Calls `Repository::commit`, which builds a commit from the staging
/// index and advances HEAD. On success, `out` receives a CBOR-encoded
/// commit result carrying the new commit id, the recorded message and
/// author, and the commit timestamp.
///
/// Returns [`PpStatus::Operation`] when there is nothing staged or when
/// GAT validation blocks the commit.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_vcs_commit(
    repo: u32,
    message: c_slice::Ref<'_, u8>,
    author: c_slice::Ref<'_, u8>,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    guard(|| {
        let message_str = std::str::from_utf8(message.as_slice())
            .map_err(|e| FfiError::Operation(format!("invalid message: {e}")))?;
        let author_str = std::str::from_utf8(author.as_slice())
            .map_err(|e| FfiError::Operation(format!("invalid author: {e}")))?;

        let result = handle::with_resource_mut(repo, |r| {
            let repository = r.as_vcs_repo_mut()?;
            let commit_id = repository
                .commit(message_str, author_str)
                .map_err(|e| FfiError::Operation(e.to_string()))?;
            // Read the committed object back for its recorded timestamp.
            let timestamp = match repository.store().get(&commit_id) {
                Ok(vcs::Object::Commit(c)) => c.timestamp,
                _ => 0,
            };
            Ok(CommitResultRecord {
                commit_id: commit_id.to_string(),
                message: message_str.to_owned(),
                author: author_str.to_owned(),
                timestamp,
            })
        })?;

        *out = crate::canonical::encode(&result)?.into();
        Ok(PpStatus::Ok)
    })
}

/// Walk the commit log from HEAD.
///
/// `repo` is a VCS repo handle; `count` caps the walk length. On success,
/// `out` receives a CBOR-encoded log result (a map with an `entries`
/// list, newest first). Calls `Repository::log`. Each entry's
/// `commit_id` is recomputed from the commit object via
/// `vcs::hash::hash_commit` (the `CommitObject` does not carry its own
/// id). An empty repository yields an empty entry list.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_vcs_log(repo: u32, count: u32, out: &mut repr_c::Vec<u8>) -> i32 {
    guard(|| {
        let result = handle::with_resource(repo, |r| {
            let repository = r.as_vcs_repo()?;
            // An unborn HEAD (empty repo) has no commits; report empty.
            let head_id = vcs::store::resolve_head(repository.store())
                .map_err(|e| FfiError::Operation(e.to_string()))?;
            let mut entries = Vec::new();
            if head_id.is_some() {
                let commits = repository
                    .log(Some(count as usize))
                    .map_err(|e| FfiError::Operation(e.to_string()))?;
                entries.reserve(commits.len());
                for c in commits {
                    let commit_id = vcs::hash::hash_commit(&c)
                        .map_err(|e| FfiError::Operation(e.to_string()))?
                        .to_string();
                    entries.push(LogEntryRecord {
                        commit_id,
                        parents: c.parents.iter().map(ToString::to_string).collect(),
                        author: c.author,
                        timestamp: c.timestamp,
                        message: c.message,
                        protocol: c.protocol,
                        schema_id: c.schema_id.to_string(),
                    });
                }
            }
            Ok(LogResultRecord { entries })
        })?;

        *out = crate::canonical::encode(&result)?.into();
        Ok(PpStatus::Ok)
    })
}

/// Get repository status (HEAD, staging, working state).
///
/// `repo` is a VCS repo handle. On success, `out` receives a
/// CBOR-encoded status record: HEAD state, the resolved HEAD commit
/// (absent for an empty repo), and `has_staged` / `working_dirty`
/// booleans read from the staging index.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_vcs_status(repo: u32, out: &mut repr_c::Vec<u8>) -> i32 {
    guard(|| {
        let result = handle::with_resource(repo, |r| {
            let repository = r.as_vcs_repo()?;
            let store = repository.store();
            let head_state = store
                .get_head()
                .map_err(|e| FfiError::Operation(e.to_string()))?;
            let head_commit =
                vcs::store::resolve_head(store).map_err(|e| FfiError::Operation(e.to_string()))?;
            // The staging index records whether a schema is staged; the
            // working tree is considered clean when nothing is staged.
            let index = repository
                .read_index()
                .map_err(|e| FfiError::Operation(e.to_string()))?;
            let has_staged = index.staged.is_some();

            Ok(StatusRecord {
                head_ref: head_state,
                head_commit: head_commit.map(|id| id.to_string()),
                has_staged,
                working_dirty: has_staged,
            })
        })?;

        *out = crate::canonical::encode(&result)?.into();
        Ok(PpStatus::Ok)
    })
}

/// Structural diff between two refs, or of the most recent schema change
/// at HEAD.
///
/// `repo` is a VCS repo handle; `from` and `to` are UTF-8 refs (a branch
/// name, tag name, or full hex commit id). When both are empty, the HEAD
/// commit's schema is diffed against its first parent's schema via
/// `panproto_check::diff`, surfacing the change the latest commit
/// introduced (a root commit is diffed against the empty schema, so every
/// element reads as added; an empty repository yields a zero-change
/// record). When at least one ref is non-empty, each is resolved through
/// `vcs::refs::resolve_ref` to a commit and its schema is assembled; an
/// empty ref on either side resolves to the empty schema, so a single ref
/// diffs that revision against nothing. On success, `out` receives a
/// CBOR-encoded diff record carrying the added / removed / modified counts
/// and the human-readable change descriptions.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_vcs_diff(
    repo: u32,
    from: c_slice::Ref<'_, u8>,
    to: c_slice::Ref<'_, u8>,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    guard(|| {
        let from_ref = std::str::from_utf8(from.as_slice())
            .map_err(|e| FfiError::Operation(format!("invalid from ref: {e}")))?;
        let to_ref = std::str::from_utf8(to.as_slice())
            .map_err(|e| FfiError::Operation(format!("invalid to ref: {e}")))?;

        let result = handle::with_resource(repo, |r| {
            let repository = r.as_vcs_repo()?;
            let store = repository.store();

            if from_ref.is_empty() && to_ref.is_empty() {
                // No refs given: diff HEAD against its first parent.
                let head_id = vcs::store::resolve_head(store)
                    .map_err(|e| FfiError::Operation(e.to_string()))?;
                let Some(head_id) = head_id else {
                    // Unborn HEAD: nothing to diff.
                    return Ok(DiffResultRecord {
                        added: 0,
                        removed: 0,
                        modified: 0,
                        changes: Vec::new(),
                    });
                };

                let head_commit = load_commit(store, head_id)?;
                let to_schema = assemble_schema(store, head_commit.schema_id)?;

                // Diff against the first parent's schema (the prior
                // revision), or the empty schema for a root commit.
                let from_schema = match head_commit.parents.first() {
                    Some(parent_id) => {
                        let parent = load_commit(store, *parent_id)?;
                        assemble_schema(store, parent.schema_id)?
                    }
                    None => empty_diff_baseline(),
                };

                let diff = panproto_core::check::diff(&from_schema, &to_schema);
                return Ok(diff_record(&diff));
            }

            // At least one ref given: resolve each to a schema (an empty
            // ref is the empty schema baseline) and diff them.
            let from_schema = schema_at_ref(store, from_ref)?;
            let to_schema = schema_at_ref(store, to_ref)?;
            let diff = panproto_core::check::diff(&from_schema, &to_schema);
            Ok(diff_record(&diff))
        })?;

        *out = crate::canonical::encode(&result)?.into();
        Ok(PpStatus::Ok)
    })
}

/// Resolve a ref to its commit's assembled schema. An empty ref is the
/// empty-schema baseline (so a single-sided diff reads every element of
/// the other side as added or removed).
fn schema_at_ref(
    store: &vcs::FsStore,
    reference: &str,
) -> Result<panproto_core::schema::Schema, FfiError> {
    if reference.is_empty() {
        return Ok(empty_diff_baseline());
    }
    let commit_id =
        vcs::refs::resolve_ref(store, reference).map_err(|e| FfiError::Operation(e.to_string()))?;
    let commit = load_commit(store, commit_id)?;
    assemble_schema(store, commit.schema_id)
}

/// Summarize a `SchemaDiff` into the wire diff record: counts of added /
/// removed / modified elements (vertices, edges, and constraints) plus a
/// human-readable change description per element.
fn diff_record(diff: &panproto_core::check::SchemaDiff) -> DiffResultRecord {
    let mut changes = Vec::new();

    for v in &diff.added_vertices {
        changes.push(format!("+ vertex {v}"));
    }
    for v in &diff.removed_vertices {
        changes.push(format!("- vertex {v}"));
    }
    for c in &diff.kind_changes {
        changes.push(format!(
            "~ vertex {} kind {} -> {}",
            c.vertex_id, c.old_kind, c.new_kind
        ));
    }
    for e in &diff.added_edges {
        changes.push(format!("+ edge {} -> {} ({})", e.src, e.tgt, e.kind));
    }
    for e in &diff.removed_edges {
        changes.push(format!("- edge {} -> {} ({})", e.src, e.tgt, e.kind));
    }
    for vertex in diff.modified_constraints.keys() {
        changes.push(format!("~ constraints on {vertex}"));
    }

    let added = (diff.added_vertices.len() + diff.added_edges.len()) as u64;
    let removed = (diff.removed_vertices.len() + diff.removed_edges.len()) as u64;
    let modified = (diff.kind_changes.len() + diff.modified_constraints.len()) as u64;

    DiffResultRecord {
        added,
        removed,
        modified,
        changes,
    }
}

/// Load a commit object by id, erroring if the object is not a commit.
fn load_commit(store: &vcs::FsStore, id: vcs::ObjectId) -> Result<vcs::CommitObject, FfiError> {
    match store
        .get(&id)
        .map_err(|e| FfiError::Operation(e.to_string()))?
    {
        vcs::Object::Commit(c) => Ok(c),
        other => Err(FfiError::Operation(format!(
            "object {id} is not a commit (found {})",
            other.type_name()
        ))),
    }
}

/// Assemble the flat schema for a schema-tree object id.
fn assemble_schema(
    store: &vcs::FsStore,
    schema_id: vcs::ObjectId,
) -> Result<panproto_core::schema::Schema, FfiError> {
    let proto = vcs::tree::project_coproduct_protocol();
    vcs::tree::assemble_schema(store, &schema_id, &proto)
        .map_err(|e| FfiError::Operation(e.to_string()))
}

/// The baseline schema a root commit is diffed against: an empty schema
/// under the project coproduct protocol, so every element of the root
/// commit's schema reads as added.
fn empty_diff_baseline() -> panproto_core::schema::Schema {
    use std::collections::HashMap;
    panproto_core::schema::Schema {
        protocol: String::new(),
        vertices: HashMap::new(),
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

/// List all branches and the commit each points at.
///
/// `repo` is a VCS repo handle. Calls `vcs::refs::list_branches` and
/// reports the full branch listing as a CBOR-encoded branch result,
/// tagging the branch HEAD currently tracks. An empty repository (no
/// branches yet) yields an empty listing. This is the create-free
/// listing op; `pp_vcs_branch` creates a branch and returns the same
/// listing shape after the create.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_vcs_list_branches(repo: u32, out: &mut repr_c::Vec<u8>) -> i32 {
    guard(|| {
        let result = handle::with_resource(repo, |r| {
            let repository = r.as_vcs_repo()?;
            Ok(BranchResultRecord {
                branches: list_branch_records(repository)?,
            })
        })?;

        *out = crate::canonical::encode(&result)?.into();
        Ok(PpStatus::Ok)
    })
}

/// Create a new branch from HEAD.
///
/// `repo` is a VCS repo handle; `name` is the UTF-8 branch name. Calls
/// `vcs::refs::create_branch` against the current HEAD commit, then
/// returns the full branch listing as a CBOR-encoded branch result so
/// the caller sees the new branch in context.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_vcs_branch(repo: u32, name: c_slice::Ref<'_, u8>, out: &mut repr_c::Vec<u8>) -> i32 {
    guard(|| {
        let branch_name = std::str::from_utf8(name.as_slice())
            .map_err(|e| FfiError::Operation(format!("invalid branch name: {e}")))?;

        let result = handle::with_resource_mut(repo, |r| {
            let repository = r.as_vcs_repo_mut()?;
            let head_id = vcs::store::resolve_head(repository.store())
                .map_err(|e| FfiError::Operation(e.to_string()))?
                .ok_or_else(|| FfiError::Operation("no commits to branch from".to_owned()))?;

            vcs::refs::create_branch(repository.store_mut(), branch_name, head_id)
                .map_err(|e| FfiError::Operation(e.to_string()))?;

            Ok(BranchResultRecord {
                branches: list_branch_records(repository)?,
            })
        })?;

        *out = crate::canonical::encode(&result)?.into();
        Ok(PpStatus::Ok)
    })
}

/// Checkout a branch or commit.
///
/// `repo` is a VCS repo handle; `target` is the UTF-8 branch/commit
/// reference. Calls `vcs::refs::checkout_branch`, then reports the
/// resulting HEAD state as a CBOR-encoded op result.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_vcs_checkout(repo: u32, target: c_slice::Ref<'_, u8>, out: &mut repr_c::Vec<u8>) -> i32 {
    guard(|| {
        let target_str = std::str::from_utf8(target.as_slice())
            .map_err(|e| FfiError::Operation(format!("invalid target: {e}")))?;

        let result = handle::with_resource_mut(repo, |r| {
            let repository = r.as_vcs_repo_mut()?;
            vcs::refs::checkout_branch(repository.store_mut(), target_str)
                .map_err(|e| FfiError::Operation(e.to_string()))?;
            let head = repository
                .store()
                .get_head()
                .map_err(|e| FfiError::Operation(e.to_string()))?;
            Ok(OpResultRecord {
                ok: true,
                head,
                messages: vec![format!("switched to '{target_str}'")],
            })
        })?;

        *out = crate::canonical::encode(&result)?.into();
        Ok(PpStatus::Ok)
    })
}

/// Merge a branch into the current branch.
///
/// `repo` is a VCS repo handle; `branch` is the UTF-8 branch name;
/// `author` is the UTF-8 author the merge commit is attributed to. Calls
/// `Repository::merge`, a real three-way merge that fast-forwards or
/// creates a merge commit as appropriate. The merge commit (when one is
/// created) records `author`. On success, `out` receives a CBOR-encoded
/// merge result carrying the fast-forward flag, the resulting HEAD
/// commit, and the conflict descriptions (empty on a clean merge).
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_vcs_merge(
    repo: u32,
    branch: c_slice::Ref<'_, u8>,
    author: c_slice::Ref<'_, u8>,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    guard(|| {
        let branch_name = std::str::from_utf8(branch.as_slice())
            .map_err(|e| FfiError::Operation(format!("invalid branch name: {e}")))?;
        let author_str = std::str::from_utf8(author.as_slice())
            .map_err(|e| FfiError::Operation(format!("invalid author: {e}")))?;

        let result = handle::with_resource_mut(repo, |r| {
            let repository = r.as_vcs_repo_mut()?;

            // Was the pre-merge HEAD an ancestor of the branch tip? If so
            // a clean merge fast-forwards. Resolve both before merging.
            let ours_pre = vcs::store::resolve_head(repository.store())
                .map_err(|e| FfiError::Operation(e.to_string()))?;
            let theirs = vcs::refs::resolve_ref(repository.store(), branch_name)
                .map_err(|e| FfiError::Operation(e.to_string()))?;
            let fast_forward = match ours_pre {
                Some(ours) => vcs::dag::is_ancestor(repository.store(), ours, theirs)
                    .map_err(|e| FfiError::Operation(e.to_string()))?,
                None => false,
            };

            let merge = repository
                .merge(branch_name, author_str)
                .map_err(|e| FfiError::Operation(e.to_string()))?;

            let conflicts: Vec<String> = merge.conflicts.iter().map(|c| format!("{c:?}")).collect();

            // The merge commit is the resulting HEAD when the merge was
            // clean (fast-forward or a real merge commit); on conflicts
            // HEAD is unchanged and there is no merge commit.
            let merge_commit = if conflicts.is_empty() {
                vcs::store::resolve_head(repository.store())
                    .map_err(|e| FfiError::Operation(e.to_string()))?
                    .map(|id| id.to_string())
            } else {
                None
            };

            Ok(MergeResultRecord {
                fast_forward,
                merge_commit,
                conflicts,
            })
        })?;

        *out = crate::canonical::encode(&result)?.into();
        Ok(PpStatus::Ok)
    })
}

/// Stash the current staged schema.
///
/// `repo` is a VCS repo handle. Pushes the currently staged schema onto
/// the stash stack via `vcs::stash::stash_push`, clears the staging
/// index, and reports the new stash entry plus the full stack. On
/// success, `out` receives a CBOR-encoded stash result.
///
/// Returns [`PpStatus::Operation`] when there is nothing staged to stash.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_vcs_stash(repo: u32, out: &mut repr_c::Vec<u8>) -> i32 {
    guard(|| {
        let result = handle::with_resource_mut(repo, |r| {
            let repository = r.as_vcs_repo_mut()?;
            let index = repository
                .read_index()
                .map_err(|e| FfiError::Operation(e.to_string()))?;
            let staged = index
                .staged
                .ok_or_else(|| FfiError::Operation("nothing staged to stash".to_owned()))?;
            let schema_id = staged.schema_id;

            let stash_id = vcs::stash::stash_push(repository.store_mut(), schema_id, "stash", None)
                .map_err(|e| FfiError::Operation(e.to_string()))?;
            // The staged change is now stashed; clear the index.
            repository
                .clear_index()
                .map_err(|e| FfiError::Operation(e.to_string()))?;

            let stack = stash_stack_records(repository)?;
            let stashed = stack.first().map_or_else(
                || StashEntryRecord {
                    index: 0,
                    commit_id: stash_id.to_string(),
                    message: String::new(),
                    timestamp: 0,
                },
                |e| StashEntryRecord {
                    index: e.index,
                    commit_id: e.commit_id.clone(),
                    message: e.message.clone(),
                    timestamp: e.timestamp,
                },
            );
            Ok(StashResultRecord { stashed, stack })
        })?;

        *out = crate::canonical::encode(&result)?.into();
        Ok(PpStatus::Ok)
    })
}

/// Pop the most recent stash entry.
///
/// `repo` is a VCS repo handle. Calls `vcs::stash::stash_pop`, restoring
/// the schema staged in the popped stash into the index, then reports
/// the restored schema id and the remaining stack as a CBOR-encoded
/// stash-pop result.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_vcs_stash_pop(repo: u32, out: &mut repr_c::Vec<u8>) -> i32 {
    guard(|| {
        let result = handle::with_resource_mut(repo, |r| {
            let repository = r.as_vcs_repo_mut()?;
            let schema_id = vcs::stash::stash_pop(repository.store_mut())
                .map_err(|e| FfiError::Operation(e.to_string()))?;
            let stack = stash_stack_records(repository)?;
            Ok(StashPopResultRecord {
                restored_schema_id: schema_id.to_string(),
                stack,
            })
        })?;

        *out = crate::canonical::encode(&result)?.into();
        Ok(PpStatus::Ok)
    })
}

/// Blame a vertex: find the commit that introduced it.
///
/// `repo` is a VCS repo handle; `vertex` is the UTF-8 vertex ID. Calls
/// `vcs::blame::blame_vertex` from HEAD. On success, `out` receives a
/// CBOR-encoded blame record.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_vcs_blame(repo: u32, vertex: c_slice::Ref<'_, u8>, out: &mut repr_c::Vec<u8>) -> i32 {
    guard(|| {
        let vertex_id = std::str::from_utf8(vertex.as_slice())
            .map_err(|e| FfiError::Operation(format!("invalid vertex id: {e}")))?;

        let result = handle::with_resource(repo, |r| {
            let repository = r.as_vcs_repo()?;
            let store = repository.store();
            let head_id = vcs::store::resolve_head(store)
                .map_err(|e| FfiError::Operation(e.to_string()))?
                .ok_or_else(|| FfiError::Operation("no commits".to_owned()))?;

            let entry = vcs::blame::blame_vertex(store, head_id, vertex_id)
                .map_err(|e| FfiError::Operation(e.to_string()))?;

            Ok(BlameRecord {
                commit_id: entry.commit_id.to_string(),
                author: entry.author,
                timestamp: entry.timestamp,
                message: entry.message,
            })
        })?;

        *out = crate::canonical::encode(&result)?.into();
        Ok(PpStatus::Ok)
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use panproto_core::schema::{Schema, Vertex};
    use safer_ffi::prelude::c_slice;

    use super::*;
    use crate::api::{pp_buf_free, pp_handle_free};
    use crate::canonical::decode;

    fn empty_schema() -> Schema {
        Schema {
            protocol: "vcs-test".into(),
            vertices: HashMap::new(),
            edges: HashMap::new(),
            hyper_edges: HashMap::new(),
            constraints: HashMap::new(),
            required: HashMap::new(),
            nsids: HashMap::new(),
            entries: vec![],
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

    fn schema_with_vertices(ids: &[&str]) -> Schema {
        let mut s = empty_schema();
        for id in ids {
            s.vertices.insert(
                (*id).into(),
                Vertex {
                    id: (*id).into(),
                    kind: "record".into(),
                    nsid: None,
                },
            );
        }
        s
    }

    fn alloc_schema_handle(s: Schema) -> u32 {
        handle::alloc(Resource::Schema(Arc::new(s)))
    }

    /// Initialize a repository rooted at a temp dir. Returns the repo
    /// handle and the `TempDir` guard (which must outlive the handle).
    fn init_repo(dir: &std::path::Path) -> u32 {
        let path = dir.to_str().unwrap();
        let bytes: Box<[u8]> = path.as_bytes().to_vec().into_boxed_slice();
        let slice: c_slice::Box<u8> = bytes.into();
        let mut h: u32 = u32::MAX;
        assert_eq!(pp_vcs_init(slice.as_ref(), &mut h), PpStatus::Ok as i32);
        assert_ne!(h, u32::MAX);
        h
    }

    fn slice_of(bytes: &[u8]) -> c_slice::Box<u8> {
        bytes.to_vec().into_boxed_slice().into()
    }

    /// Stage a schema then commit it, returning the commit hex id.
    fn add_and_commit(repo: u32, schema: Schema, message: &str, author: &str) -> String {
        let schema_h = alloc_schema_handle(schema);
        let mut add_out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            pp_vcs_add(repo, schema_h, &mut add_out),
            PpStatus::Ok as i32
        );
        pp_buf_free(add_out);
        assert_eq!(pp_handle_free(schema_h), PpStatus::Ok as i32);

        let mut commit_out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            pp_vcs_commit(
                repo,
                slice_of(message.as_bytes()).as_ref(),
                slice_of(author.as_bytes()).as_ref(),
                &mut commit_out,
            ),
            PpStatus::Ok as i32
        );
        let value: ciborium::value::Value = decode(&commit_out).unwrap();
        pp_buf_free(commit_out);
        // Pull the commit id out of the map.
        let map = value.as_map().expect("commit result is a map");
        let mut commit_id = String::new();
        for (k, v) in map {
            if k.as_text() == Some("commit_id") {
                commit_id = v.as_text().unwrap().to_owned();
            }
        }
        assert!(!commit_id.is_empty(), "commit id should be non-empty");
        commit_id
    }

    #[test]
    fn init_open_existing_repo() {
        let dir = tempfile::tempdir().unwrap();
        let repo = init_repo(dir.path());
        assert_eq!(pp_handle_free(repo), PpStatus::Ok as i32);

        // Re-initializing at the same path opens (rather than fails on)
        // the existing repo.
        let repo2 = init_repo(dir.path());
        assert_eq!(pp_handle_free(repo2), PpStatus::Ok as i32);
    }

    #[test]
    fn add_commit_then_log() {
        let dir = tempfile::tempdir().unwrap();
        let repo = init_repo(dir.path());

        let id1 = add_and_commit(repo, schema_with_vertices(&["a"]), "initial", "alice");
        let id2 = add_and_commit(repo, schema_with_vertices(&["a", "b"]), "add b", "alice");
        assert_ne!(id1, id2);

        // Log lists both commits, newest first.
        let mut log_out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(pp_vcs_log(repo, 10, &mut log_out), PpStatus::Ok as i32);
        let value: ciborium::value::Value = decode(&log_out).unwrap();
        let text = format!("{value:?}");
        assert!(text.contains("entries"), "log map: {text}");
        assert!(text.contains("add b"), "log should contain newest: {text}");
        assert!(
            text.contains("initial"),
            "log should contain oldest: {text}"
        );
        pp_buf_free(log_out);

        assert_eq!(pp_handle_free(repo), PpStatus::Ok as i32);
    }

    #[test]
    fn add_returns_real_schema_id() {
        let dir = tempfile::tempdir().unwrap();
        let repo = init_repo(dir.path());
        let schema = alloc_schema_handle(schema_with_vertices(&["post"]));

        let mut out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(pp_vcs_add(repo, schema, &mut out), PpStatus::Ok as i32);
        let value: ciborium::value::Value = decode(&out).unwrap();
        let map = value.as_map().expect("add result is a map");
        let mut schema_id = String::new();
        for (k, v) in map {
            if k.as_text() == Some("schema_id") {
                schema_id = v.as_text().unwrap().to_owned();
            }
        }
        // 64-char lowercase hex blake3 digest.
        assert_eq!(schema_id.len(), 64, "id: {schema_id}");
        assert!(schema_id.chars().all(|c| c.is_ascii_hexdigit()));
        pp_buf_free(out);

        assert_eq!(pp_handle_free(schema), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(repo), PpStatus::Ok as i32);
    }

    #[test]
    fn log_on_empty_repo_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        let repo = init_repo(dir.path());
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(pp_vcs_log(repo, 10, &mut out), PpStatus::Ok as i32);
        let value: ciborium::value::Value = decode(&out).unwrap();
        let text = format!("{value:?}");
        assert!(text.contains("entries"), "log map: {text}");
        pp_buf_free(out);
        assert_eq!(pp_handle_free(repo), PpStatus::Ok as i32);
    }

    #[test]
    fn status_reflects_head_after_commit() {
        let dir = tempfile::tempdir().unwrap();
        let repo = init_repo(dir.path());
        add_and_commit(repo, schema_with_vertices(&["a"]), "initial", "alice");

        let mut out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(pp_vcs_status(repo, &mut out), PpStatus::Ok as i32);
        let value: ciborium::value::Value = decode(&out).unwrap();
        let text = format!("{value:?}");
        assert!(text.contains("head_ref"), "status: {text}");
        assert!(text.contains("Branch"), "status: {text}");
        assert!(text.contains("main"), "status: {text}");
        // head_commit is now present (non-null) after a commit.
        assert!(text.contains("head_commit"), "status: {text}");
        pp_buf_free(out);
        assert_eq!(pp_handle_free(repo), PpStatus::Ok as i32);
    }

    #[test]
    fn commit_with_nothing_staged_errors() {
        let _ = crate::error::take_last_error();
        let dir = tempfile::tempdir().unwrap();
        let repo = init_repo(dir.path());

        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_vcs_commit(
            repo,
            slice_of(b"empty").as_ref(),
            slice_of(b"alice").as_ref(),
            &mut out,
        );
        assert_eq!(status, PpStatus::Operation as i32);
        let env = crate::error::take_last_error().expect("error stashed");
        assert_eq!(env.tag, "operation");
        pp_buf_free(out);
        assert_eq!(pp_handle_free(repo), PpStatus::Ok as i32);
    }

    #[test]
    fn branch_and_checkout() {
        let dir = tempfile::tempdir().unwrap();
        let repo = init_repo(dir.path());
        add_and_commit(repo, schema_with_vertices(&["a"]), "initial", "alice");

        // Create a feature branch from HEAD.
        let mut branch_out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            pp_vcs_branch(repo, slice_of(b"feature").as_ref(), &mut branch_out),
            PpStatus::Ok as i32
        );
        let value: ciborium::value::Value = decode(&branch_out).unwrap();
        let text = format!("{value:?}");
        assert!(text.contains("feature"), "branch listing: {text}");
        assert!(text.contains("main"), "branch listing: {text}");
        pp_buf_free(branch_out);

        // Checkout the feature branch.
        let mut co_out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            pp_vcs_checkout(repo, slice_of(b"feature").as_ref(), &mut co_out),
            PpStatus::Ok as i32
        );
        let co: ciborium::value::Value = decode(&co_out).unwrap();
        assert!(format!("{co:?}").contains("feature"));
        pp_buf_free(co_out);

        assert_eq!(pp_handle_free(repo), PpStatus::Ok as i32);
    }

    #[test]
    fn list_branches_after_create_and_checkout() {
        let dir = tempfile::tempdir().unwrap();
        let repo = init_repo(dir.path());
        add_and_commit(repo, schema_with_vertices(&["a"]), "initial", "alice");

        // Create a feature branch from HEAD.
        let mut branch_out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            pp_vcs_branch(repo, slice_of(b"feature").as_ref(), &mut branch_out),
            PpStatus::Ok as i32
        );
        pp_buf_free(branch_out);

        // The create-free listing op reports both branches; "main" is
        // current (HEAD still tracks it) and "feature" is not.
        let mut list_out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            pp_vcs_list_branches(repo, &mut list_out),
            PpStatus::Ok as i32
        );
        let value: ciborium::value::Value = decode(&list_out).unwrap();
        let map = value.as_map().expect("branch result is a map");
        let mut names: Vec<String> = Vec::new();
        let mut current: Option<String> = None;
        for (k, v) in map {
            if k.as_text() == Some("branches") {
                for entry in v.as_array().expect("branches is an array") {
                    let entry_map = entry.as_map().expect("branch entry is a map");
                    let mut name = String::new();
                    let mut is_current = false;
                    for (ek, ev) in entry_map {
                        match ek.as_text() {
                            Some("name") => name = ev.as_text().unwrap().to_owned(),
                            Some("is_current") => {
                                is_current = ev == &ciborium::value::Value::Bool(true);
                            }
                            _ => {}
                        }
                    }
                    if is_current {
                        current = Some(name.clone());
                    }
                    names.push(name);
                }
            }
        }
        pp_buf_free(list_out);
        assert!(
            names.iter().any(|n| n == "feature"),
            "listing should include the new branch: {names:?}"
        );
        assert!(
            names.iter().any(|n| n == "main"),
            "listing should include the default branch: {names:?}"
        );
        assert_eq!(
            current.as_deref(),
            Some("main"),
            "HEAD still tracks main before checkout"
        );

        // Checkout the feature branch; the listing now marks it current.
        let mut co_out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            pp_vcs_checkout(repo, slice_of(b"feature").as_ref(), &mut co_out),
            PpStatus::Ok as i32
        );
        pp_buf_free(co_out);

        let mut list_out2: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            pp_vcs_list_branches(repo, &mut list_out2),
            PpStatus::Ok as i32
        );
        let value2: ciborium::value::Value = decode(&list_out2).unwrap();
        let map2 = value2.as_map().expect("branch result is a map");
        let mut current2: Option<String> = None;
        for (k, v) in map2 {
            if k.as_text() == Some("branches") {
                for entry in v.as_array().expect("branches is an array") {
                    let entry_map = entry.as_map().expect("branch entry is a map");
                    let mut name = String::new();
                    let mut is_current = false;
                    for (ek, ev) in entry_map {
                        match ek.as_text() {
                            Some("name") => name = ev.as_text().unwrap().to_owned(),
                            Some("is_current") => {
                                is_current = ev == &ciborium::value::Value::Bool(true);
                            }
                            _ => {}
                        }
                    }
                    if is_current {
                        current2 = Some(name);
                    }
                }
            }
        }
        pp_buf_free(list_out2);
        assert_eq!(
            current2.as_deref(),
            Some("feature"),
            "HEAD tracks feature after checkout"
        );

        assert_eq!(pp_handle_free(repo), PpStatus::Ok as i32);
    }

    #[test]
    fn branch_requires_a_commit() {
        let _ = crate::error::take_last_error();
        let dir = tempfile::tempdir().unwrap();
        let repo = init_repo(dir.path());

        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_vcs_branch(repo, slice_of(b"feature").as_ref(), &mut out);
        assert_eq!(status, PpStatus::Operation as i32);
        let env = crate::error::take_last_error().expect("error stashed");
        assert_eq!(env.tag, "operation");
        pp_buf_free(out);
        assert_eq!(pp_handle_free(repo), PpStatus::Ok as i32);
    }

    #[test]
    fn diff_reports_head_change() {
        let dir = tempfile::tempdir().unwrap();
        let repo = init_repo(dir.path());
        add_and_commit(repo, schema_with_vertices(&["a"]), "first", "alice");
        add_and_commit(repo, schema_with_vertices(&["a", "b"]), "second", "alice");

        // The HEAD commit added vertex `b` over its parent, so the diff
        // records exactly one added element.
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            pp_vcs_diff(
                repo,
                slice_of(b"").as_ref(),
                slice_of(b"").as_ref(),
                &mut out,
            ),
            PpStatus::Ok as i32
        );
        let value: ciborium::value::Value = decode(&out).unwrap();
        let map = value.as_map().expect("diff result is a map");
        let mut added = 0u64;
        let mut has_changes = false;
        for (k, v) in map {
            match k.as_text() {
                Some("added") => {
                    added = v
                        .as_integer()
                        .and_then(|i| u64::try_from(i).ok())
                        .unwrap_or(0);
                }
                Some("changes") => has_changes = v.as_array().is_some_and(|a| !a.is_empty()),
                _ => {}
            }
        }
        assert_eq!(added, 1, "diff should report one added vertex");
        assert!(has_changes, "diff should describe the change");
        pp_buf_free(out);

        assert_eq!(pp_handle_free(repo), PpStatus::Ok as i32);
    }

    #[test]
    fn blame_attributes_a_vertex() {
        let dir = tempfile::tempdir().unwrap();
        let repo = init_repo(dir.path());
        add_and_commit(repo, schema_with_vertices(&["post"]), "add post", "alice");

        let mut out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            pp_vcs_blame(repo, slice_of(b"post").as_ref(), &mut out),
            PpStatus::Ok as i32
        );
        let value: ciborium::value::Value = decode(&out).unwrap();
        let text = format!("{value:?}");
        assert!(
            text.contains("alice"),
            "blame should attribute to alice: {text}"
        );
        assert!(text.contains("add post"), "blame message: {text}");
        pp_buf_free(out);

        assert_eq!(pp_handle_free(repo), PpStatus::Ok as i32);
    }

    #[test]
    fn merge_fast_forward() {
        let dir = tempfile::tempdir().unwrap();
        let repo = init_repo(dir.path());
        add_and_commit(repo, schema_with_vertices(&["a"]), "initial", "alice");

        // Create a feature branch, switch to it, and commit a change.
        let mut bout: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            pp_vcs_branch(repo, slice_of(b"feature").as_ref(), &mut bout),
            PpStatus::Ok as i32
        );
        pp_buf_free(bout);
        let mut cout: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            pp_vcs_checkout(repo, slice_of(b"feature").as_ref(), &mut cout),
            PpStatus::Ok as i32
        );
        pp_buf_free(cout);
        add_and_commit(repo, schema_with_vertices(&["a", "b"]), "add b", "bob");

        // Switch back to main and merge feature: fast-forward.
        let mut cout2: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            pp_vcs_checkout(repo, slice_of(b"main").as_ref(), &mut cout2),
            PpStatus::Ok as i32
        );
        pp_buf_free(cout2);

        let mut out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            pp_vcs_merge(
                repo,
                slice_of(b"feature").as_ref(),
                slice_of(b"merger").as_ref(),
                &mut out,
            ),
            PpStatus::Ok as i32
        );
        let value: ciborium::value::Value = decode(&out).unwrap();
        let map = value.as_map().expect("merge result is a map");
        let mut conflicts_empty = true;
        let mut ff = false;
        for (k, v) in map {
            match k.as_text() {
                Some("conflicts") => {
                    conflicts_empty = v.as_array().is_none_or(Vec::is_empty);
                }
                Some("fast_forward") => ff = v == &ciborium::value::Value::Bool(true),
                _ => {}
            }
        }
        assert!(conflicts_empty, "expected a clean merge");
        assert!(ff, "expected a fast-forward merge");
        pp_buf_free(out);

        assert_eq!(pp_handle_free(repo), PpStatus::Ok as i32);
    }

    #[test]
    fn merge_commit_records_the_named_author() {
        // Build divergent history so the merge creates a real merge
        // commit (not a fast-forward), then assert the merge commit is
        // attributed to the author passed to pp_vcs_merge.
        let dir = tempfile::tempdir().unwrap();
        let repo = init_repo(dir.path());
        add_and_commit(repo, schema_with_vertices(&["a"]), "initial", "alice");

        // Branch feature at the initial commit and advance it.
        let mut bout: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            pp_vcs_branch(repo, slice_of(b"feature").as_ref(), &mut bout),
            PpStatus::Ok as i32
        );
        pp_buf_free(bout);
        let mut cout: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            pp_vcs_checkout(repo, slice_of(b"feature").as_ref(), &mut cout),
            PpStatus::Ok as i32
        );
        pp_buf_free(cout);
        add_and_commit(repo, schema_with_vertices(&["a", "b"]), "add b", "bob");

        // Advance main divergently so the merge cannot fast-forward.
        let mut cout2: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            pp_vcs_checkout(repo, slice_of(b"main").as_ref(), &mut cout2),
            PpStatus::Ok as i32
        );
        pp_buf_free(cout2);
        add_and_commit(repo, schema_with_vertices(&["a", "c"]), "add c", "carol");

        // Merge feature into main, attributing the merge to "merlin".
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            pp_vcs_merge(
                repo,
                slice_of(b"feature").as_ref(),
                slice_of(b"merlin").as_ref(),
                &mut out,
            ),
            PpStatus::Ok as i32
        );
        let value: ciborium::value::Value = decode(&out).unwrap();
        let map = value.as_map().expect("merge result is a map");
        let mut ff = true;
        let mut conflicts_empty = true;
        for (k, v) in map {
            match k.as_text() {
                Some("fast_forward") => ff = v == &ciborium::value::Value::Bool(true),
                Some("conflicts") => conflicts_empty = v.as_array().is_none_or(Vec::is_empty),
                _ => {}
            }
        }
        assert!(!ff, "divergent history should not fast-forward");
        assert!(conflicts_empty, "non-overlapping changes merge cleanly");
        pp_buf_free(out);

        // The newest log entry is the merge commit; its author is the one
        // passed to pp_vcs_merge.
        let mut log_out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(pp_vcs_log(repo, 1, &mut log_out), PpStatus::Ok as i32);
        let log_text = format!("{:?}", decode::<ciborium::value::Value>(&log_out).unwrap());
        assert!(
            log_text.contains("merlin"),
            "merge commit should record the named author, got: {log_text}"
        );
        pp_buf_free(log_out);

        assert_eq!(pp_handle_free(repo), PpStatus::Ok as i32);
    }

    #[test]
    fn diff_between_two_refs() {
        // Commit two revisions on two branches, then diff one branch ref
        // against the other: the schema that adds vertices reads as added.
        let dir = tempfile::tempdir().unwrap();
        let repo = init_repo(dir.path());
        add_and_commit(repo, schema_with_vertices(&["a"]), "base", "alice");

        // feature branch adds vertex b on top of base.
        let mut bout: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            pp_vcs_branch(repo, slice_of(b"feature").as_ref(), &mut bout),
            PpStatus::Ok as i32
        );
        pp_buf_free(bout);
        let mut cout: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            pp_vcs_checkout(repo, slice_of(b"feature").as_ref(), &mut cout),
            PpStatus::Ok as i32
        );
        pp_buf_free(cout);
        add_and_commit(repo, schema_with_vertices(&["a", "b"]), "add b", "bob");

        // Diff main (one vertex) against feature (two vertices): b added.
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            pp_vcs_diff(
                repo,
                slice_of(b"main").as_ref(),
                slice_of(b"feature").as_ref(),
                &mut out,
            ),
            PpStatus::Ok as i32
        );
        let value: ciborium::value::Value = decode(&out).unwrap();
        let map = value.as_map().expect("diff result is a map");
        let mut added = 0u64;
        let mut has_changes = false;
        for (k, v) in map {
            match k.as_text() {
                Some("added") => {
                    added = v
                        .as_integer()
                        .and_then(|i| u64::try_from(i).ok())
                        .unwrap_or(0);
                }
                Some("changes") => has_changes = v.as_array().is_some_and(|a| !a.is_empty()),
                _ => {}
            }
        }
        assert_eq!(added, 1, "feature adds exactly vertex b over main");
        assert!(has_changes, "diff should describe the added vertex");
        pp_buf_free(out);

        assert_eq!(pp_handle_free(repo), PpStatus::Ok as i32);
    }

    #[test]
    fn stash_and_pop_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let repo = init_repo(dir.path());
        add_and_commit(repo, schema_with_vertices(&["a"]), "initial", "alice");

        // Stage a change, then stash it.
        let schema_h = alloc_schema_handle(schema_with_vertices(&["a", "b"]));
        let mut add_out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            pp_vcs_add(repo, schema_h, &mut add_out),
            PpStatus::Ok as i32
        );
        pp_buf_free(add_out);
        assert_eq!(pp_handle_free(schema_h), PpStatus::Ok as i32);

        let mut stash_out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(pp_vcs_stash(repo, &mut stash_out), PpStatus::Ok as i32);
        let value: ciborium::value::Value = decode(&stash_out).unwrap();
        assert!(format!("{value:?}").contains("stashed"));
        pp_buf_free(stash_out);

        // Pop it back.
        let mut pop_out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(pp_vcs_stash_pop(repo, &mut pop_out), PpStatus::Ok as i32);
        let popped: ciborium::value::Value = decode(&pop_out).unwrap();
        let text = format!("{popped:?}");
        assert!(text.contains("restored_schema_id"), "pop result: {text}");
        pp_buf_free(pop_out);

        assert_eq!(pp_handle_free(repo), PpStatus::Ok as i32);
    }

    #[test]
    fn add_on_non_repo_handle_is_type_mismatch() {
        let _ = crate::error::take_last_error();
        let schema = alloc_schema_handle(empty_schema());
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_vcs_add(schema, schema, &mut out);
        assert_eq!(status, PpStatus::TypeMismatch as i32);
        pp_buf_free(out);
        assert_eq!(pp_handle_free(schema), PpStatus::Ok as i32);
    }
}
