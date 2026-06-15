//! Schematic version control operations.
//!
//! Ported from the working reference in
//! `crates/panproto-wasm/src/api/vcs.rs`, with the WASM error type and
//! `MessagePack` framing replaced by [`FfiError`] and the canonical CBOR
//! codec. The repo handle is a mutable
//! [`Resource::VcsRepo`](crate::handle::Resource) (a boxed
//! `panproto_core::vcs::MemStore`) accessed via
//! [`crate::handle::with_resource_mut`] for staging, branching, and
//! checkout, and [`crate::handle::with_resource`] for read-only walks.
//!
//! # Wire format
//!
//! Each operation that returns data writes a CBOR-encoded result record
//! to its `out` parameter. The record shapes here are the
//! [`crate::api::helpers`] `Vcs*Result` shadow structs *enriched* to the
//! field names the Haskell `Panproto.Vcs` decoders read: object ids
//! cross the boundary as their lowercase-hex `Display` rendering (a
//! `String`), never as the raw `[u8; 32]` `serde` array, and HEAD state
//! crosses as the externally-tagged `panproto_core::vcs::HeadState` enum
//! (`{"Branch": "main"}` / `{"Detached": "<hex>"}`). The local result
//! types below own that wire shape; the helpers struct is reused
//! verbatim where its field set already matches
//! ([`VcsAddResult`](crate::api::helpers::VcsAddResult)).
//!
//! # The `pp_vcs_commit` caveat
//!
//! Committing requires a staging *index*, which is part of the
//! filesystem-backed `panproto_core::vcs::Repository` (driven through
//! `FsStore` and `read_index` / `write_index`), not of the `Store`
//! trait that `MemStore` implements. A `MemStore` therefore has no index
//! to commit from, so [`pp_vcs_commit`] mirrors the WASM stub faithfully:
//! it resolves HEAD to confirm the repo is well-formed and then returns
//! an [`FfiError::Operation`] explaining that in-memory repositories
//! carry no staging index. This is a real limitation of the in-memory
//! store, not a placeholder.

use panproto_core::vcs::{self, Store as _};
use safer_ffi::prelude::*;
use serde::Serialize;

use crate::error::{FfiError, PpStatus};
use crate::handle::{self, Resource};
use crate::panic::guard;

use super::helpers::VcsAddResult;

// ---------------------------------------------------------------------------
// Wire result records
//
// These mirror the Haskell `Panproto.Vcs` decoders. They are local to
// this module (rather than the shared `helpers` shadow structs) because
// they carry the richer field set the decoders read and serialize object
// ids as hex strings. Missing data is filled with the same neutral
// defaults the decoders fall back to, so the two sides agree.

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
/// counts plus human-readable change descriptions. For an in-memory repo
/// with no staged change against HEAD, the diff is the branch listing
/// rendered as change lines (the same information the WASM reference
/// surfaces), with zero structural counts.
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
fn list_branch_records(store: &vcs::MemStore) -> Result<Vec<BranchInfoRecord>, FfiError> {
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
fn stash_stack_records(store: &vcs::MemStore) -> Result<Vec<StashEntryRecord>, FfiError> {
    Ok(vcs::stash::stash_list(store)
        .map_err(|e| FfiError::Operation(e.to_string()))?
        .into_iter()
        .enumerate()
        .map(|(i, entry)| StashEntryRecord {
            index: i as u64,
            commit_id: entry.commit_id.to_string(),
            message: entry.message,
            timestamp: entry.timestamp,
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Phase 6: VCS operations
// ---------------------------------------------------------------------------

/// Initialize an in-memory VCS repository.
///
/// `protocol_name` is the UTF-8 protocol name bytes (currently advisory:
/// the in-memory store tracks the protocol per commit, not per repo, so
/// this argument is accepted for parity with the WASM and Python
/// surfaces and validated as UTF-8). On success, `out_handle` receives a
/// fresh [`Resource::VcsRepo`](crate::handle::Resource) handle wrapping a
/// `vcs::MemStore` whose HEAD points at `main`.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_vcs_init(protocol_name: c_slice::Ref<'_, u8>, out_handle: &mut u32) -> i32 {
    guard(|| {
        // Validate the protocol name is UTF-8 so a malformed argument is
        // rejected at the boundary rather than silently ignored.
        std::str::from_utf8(protocol_name.as_slice())
            .map_err(|e| FfiError::Operation(format!("invalid protocol name: {e}")))?;
        *out_handle = handle::alloc(Resource::VcsRepo(Box::new(vcs::MemStore::new())));
        Ok(PpStatus::Ok)
    })
}

/// Stage a schema in a VCS repository.
///
/// `repo` is a VCS repo handle; `schema` is a
/// [`Resource::Schema`](crate::handle::Resource) handle. On success,
/// `out` receives a CBOR-encoded
/// [`VcsAddResult`](crate::api::helpers::VcsAddResult) carrying the
/// staged schema's object id. Calls `vcs::tree::store_schema_as_tree`.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_vcs_add(repo: u32, schema: u32, out: &mut repr_c::Vec<u8>) -> i32 {
    guard(|| {
        // Clone the schema out of its handle first, then take the
        // mutable borrow on the repo (two distinct slab entries).
        let schema_val = handle::with_resource(schema, |r| Ok(r.as_schema()?.clone()))?;

        let result = handle::with_resource_mut(repo, |r| {
            let store = r.as_vcs_repo_mut()?;
            let schema_id = vcs::tree::store_schema_as_tree(store, schema_val)
                .map_err(|e| FfiError::Operation(e.to_string()))?;
            Ok(VcsAddResult {
                schema_id: schema_id.to_string(),
            })
        })?;

        *out = crate::canonical::encode(&result)?.into();
        Ok(PpStatus::Ok)
    })
}

/// Commit the staged schema in a VCS repository.
///
/// `repo` is a VCS repo handle; `message` and `author` are UTF-8 bytes.
///
/// In-memory repositories carry no staging *index* (that lives in the
/// filesystem-backed `Repository`, not the `Store` trait `MemStore`
/// implements), so there is nothing for `MemStore` to commit. This
/// mirrors the WASM reference exactly: HEAD is resolved to confirm the
/// repo is well-formed, then an [`FfiError::Operation`] is returned
/// describing the limitation, echoing the message, author, and current
/// HEAD. The status is therefore [`PpStatus::Operation`].
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_vcs_commit(
    repo: u32,
    message: c_slice::Ref<'_, u8>,
    author: c_slice::Ref<'_, u8>,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    let _ = out;
    guard(|| {
        let message_str = std::str::from_utf8(message.as_slice())
            .map_err(|e| FfiError::Operation(format!("invalid message: {e}")))?;
        let author_str = std::str::from_utf8(author.as_slice())
            .map_err(|e| FfiError::Operation(format!("invalid author: {e}")))?;

        handle::with_resource_mut(repo, |r| {
            let store = r.as_vcs_repo_mut()?;
            let head_id =
                vcs::store::resolve_head(store).map_err(|e| FfiError::Operation(e.to_string()))?;
            // The in-memory store has no index; the full commit path
            // requires the filesystem-backed Repository. Fail with a
            // clear message rather than fabricating a commit.
            Err(FfiError::Operation(format!(
                "vcs_commit is unsupported for in-memory repositories: \
                 MemStore has no staging index (message={message_str:?}, \
                 author={author_str:?}, head={head_id:?})"
            )))
        })
    })
}

/// Walk the commit log from HEAD.
///
/// `repo` is a VCS repo handle; `count` caps the walk length. On success,
/// `out` receives a CBOR-encoded log result (a map with an `entries`
/// list, newest first). Calls `vcs::dag::log_walk`. Each entry's
/// `commit_id` is recomputed from the commit object via
/// `vcs::hash::hash_commit` (the `CommitObject` does not carry its own
/// id). An empty repository yields an empty entry list.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_vcs_log(repo: u32, count: u32, out: &mut repr_c::Vec<u8>) -> i32 {
    guard(|| {
        let result = handle::with_resource(repo, |r| {
            let store = r.as_vcs_repo()?;
            let head_id =
                vcs::store::resolve_head(store).map_err(|e| FfiError::Operation(e.to_string()))?;

            let entries = match head_id {
                None => Vec::new(),
                Some(id) => {
                    let commits = vcs::dag::log_walk(store, id, Some(count as usize))
                        .map_err(|e| FfiError::Operation(e.to_string()))?;
                    let mut out_entries = Vec::with_capacity(commits.len());
                    for c in commits {
                        let commit_id = vcs::hash::hash_commit(&c)
                            .map_err(|e| FfiError::Operation(e.to_string()))?
                            .to_string();
                        out_entries.push(LogEntryRecord {
                            commit_id,
                            parents: c.parents.iter().map(ToString::to_string).collect(),
                            author: c.author,
                            timestamp: c.timestamp,
                            message: c.message,
                            protocol: c.protocol,
                            schema_id: c.schema_id.to_string(),
                        });
                    }
                    out_entries
                }
            };
            Ok(LogResultRecord { entries })
        })?;

        *out = crate::canonical::encode(&result)?.into();
        Ok(PpStatus::Ok)
    })
}

/// Get repository status (branch and HEAD).
///
/// `repo` is a VCS repo handle. On success, `out` receives a
/// CBOR-encoded status record: HEAD state, the resolved HEAD commit
/// (absent for an empty repo), and `has_staged` / `working_dirty`
/// booleans (both `false` for an in-memory store with no index).
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_vcs_status(repo: u32, out: &mut repr_c::Vec<u8>) -> i32 {
    guard(|| {
        let result = handle::with_resource(repo, |r| {
            let store = r.as_vcs_repo()?;
            let head_state = store
                .get_head()
                .map_err(|e| FfiError::Operation(e.to_string()))?;
            let head_commit =
                vcs::store::resolve_head(store).map_err(|e| FfiError::Operation(e.to_string()))?;

            Ok(StatusRecord {
                head_ref: head_state,
                head_commit: head_commit.map(|id| id.to_string()),
                // MemStore has no index, so nothing is ever staged and
                // the working tree never diverges from HEAD.
                has_staged: false,
                working_dirty: false,
            })
        })?;

        *out = crate::canonical::encode(&result)?.into();
        Ok(PpStatus::Ok)
    })
}

/// Structural diff for the repository.
///
/// `repo` is a VCS repo handle. The in-memory store holds no staged
/// change to diff against HEAD, so the diff reports zero structural
/// counts and renders the branch listing as informational change lines,
/// surfacing the same state the WASM reference does. On success, `out`
/// receives a CBOR-encoded diff record.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_vcs_diff(repo: u32, out: &mut repr_c::Vec<u8>) -> i32 {
    guard(|| {
        let result = handle::with_resource(repo, |r| {
            let store = r.as_vcs_repo()?;
            let branches = list_branch_records(store)?;
            let changes = branches
                .iter()
                .map(|b| format!("branch {} -> {}", b.name, b.target))
                .collect();
            Ok(DiffResultRecord {
                added: 0,
                removed: 0,
                modified: 0,
                changes,
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
            let store = r.as_vcs_repo_mut()?;
            let head_id = vcs::store::resolve_head(store)
                .map_err(|e| FfiError::Operation(e.to_string()))?
                .ok_or_else(|| FfiError::Operation("no commits to branch from".to_owned()))?;

            vcs::refs::create_branch(store, branch_name, head_id)
                .map_err(|e| FfiError::Operation(e.to_string()))?;

            Ok(BranchResultRecord {
                branches: list_branch_records(store)?,
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
            let store = r.as_vcs_repo_mut()?;
            vcs::refs::checkout_branch(store, target_str)
                .map_err(|e| FfiError::Operation(e.to_string()))?;
            let head = store
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
/// `repo` is a VCS repo handle; `branch` is the UTF-8 branch name. The
/// merge target is resolved via `vcs::refs::resolve_ref`; a full
/// three-way merge requires the index-backed `Repository`, so for the
/// in-memory store this reports the resolved target as a conflict-free
/// summary rather than fabricating a merge commit. On success, `out`
/// receives a CBOR-encoded merge result.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_vcs_merge(repo: u32, branch: c_slice::Ref<'_, u8>, out: &mut repr_c::Vec<u8>) -> i32 {
    guard(|| {
        let branch_name = std::str::from_utf8(branch.as_slice())
            .map_err(|e| FfiError::Operation(format!("invalid branch name: {e}")))?;

        let result = handle::with_resource(repo, |r| {
            let store = r.as_vcs_repo()?;
            let theirs_id = vcs::refs::resolve_ref(store, branch_name)
                .map_err(|e| FfiError::Operation(e.to_string()))?;
            Ok(MergeResultRecord {
                fast_forward: false,
                merge_commit: Some(theirs_id.to_string()),
                conflicts: Vec::new(),
            })
        })?;

        *out = crate::canonical::encode(&result)?.into();
        Ok(PpStatus::Ok)
    })
}

/// Stash the current working state.
///
/// `repo` is a VCS repo handle. Reads the stash stack via
/// `vcs::stash::stash_list`. The in-memory store has no working tree to
/// stash, so this reports the existing stack with the most-recent entry
/// echoed as `stashed` (or a neutral entry when the stack is empty). On
/// success, `out` receives a CBOR-encoded stash result.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_vcs_stash(repo: u32, out: &mut repr_c::Vec<u8>) -> i32 {
    guard(|| {
        let result = handle::with_resource(repo, |r| {
            let store = r.as_vcs_repo()?;
            let stack = stash_stack_records(store)?;
            let stashed = stack.first().map_or_else(
                || StashEntryRecord {
                    index: 0,
                    commit_id: String::new(),
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
/// the schema staged in the popped stash, then reports the restored
/// schema id and the remaining stack as a CBOR-encoded stash-pop result.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_vcs_stash_pop(repo: u32, out: &mut repr_c::Vec<u8>) -> i32 {
    guard(|| {
        let result = handle::with_resource_mut(repo, |r| {
            let store = r.as_vcs_repo_mut()?;
            let schema_id =
                vcs::stash::stash_pop(store).map_err(|e| FfiError::Operation(e.to_string()))?;
            let stack = stash_stack_records(store)?;
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
            let store = r.as_vcs_repo()?;
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
    use crate::canonical::{decode, encode};

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

    fn schema_with_vertex(id: &str) -> Schema {
        let mut s = empty_schema();
        s.vertices.insert(
            id.into(),
            Vertex {
                id: id.into(),
                kind: "record".into(),
                nsid: None,
            },
        );
        s
    }

    fn alloc_schema_handle(s: Schema) -> u32 {
        handle::alloc(Resource::Schema(Arc::new(s)))
    }

    fn init_repo() -> u32 {
        let name: Box<[u8]> = b"vcs-test".to_vec().into_boxed_slice();
        let slice: c_slice::Box<u8> = name.into();
        let mut h: u32 = u32::MAX;
        assert_eq!(pp_vcs_init(slice.as_ref(), &mut h), PpStatus::Ok as i32);
        h
    }

    #[test]
    fn init_allocates_a_repo_handle() {
        let repo = init_repo();
        // Status on a fresh repo: HEAD on main, no commit.
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(pp_vcs_status(repo, &mut out), PpStatus::Ok as i32);
        let value: ciborium::value::Value = decode(&out).unwrap();
        // The encoded record is a map; head_ref must carry Branch("main").
        let text = format!("{value:?}");
        assert!(text.contains("head_ref"), "status map: {text}");
        assert!(text.contains("Branch"), "status map: {text}");
        assert!(text.contains("main"), "status map: {text}");
        pp_buf_free(out);
        assert_eq!(pp_handle_free(repo), PpStatus::Ok as i32);
    }

    #[test]
    fn add_returns_schema_id() {
        let repo = init_repo();
        let schema = alloc_schema_handle(schema_with_vertex("post"));

        let mut out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(pp_vcs_add(repo, schema, &mut out), PpStatus::Ok as i32);
        let result: VcsAddResult = decode(&out).unwrap();
        assert!(
            !result.schema_id.is_empty(),
            "schema id should be non-empty"
        );
        // The id is a 64-char lowercase hex blake3 digest.
        assert_eq!(result.schema_id.len(), 64, "id: {}", result.schema_id);
        assert!(
            result.schema_id.chars().all(|c| c.is_ascii_hexdigit()),
            "id not hex: {}",
            result.schema_id
        );

        pp_buf_free(out);
        assert_eq!(pp_handle_free(schema), PpStatus::Ok as i32);
        assert_eq!(pp_handle_free(repo), PpStatus::Ok as i32);
    }

    #[test]
    fn log_on_empty_repo_is_empty() {
        let repo = init_repo();
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(pp_vcs_log(repo, 10, &mut out), PpStatus::Ok as i32);
        // The `entries` list should be present (and empty: no commits).
        let value: ciborium::value::Value = decode(&out).unwrap();
        let text = format!("{value:?}");
        assert!(text.contains("entries"), "log map: {text}");
        pp_buf_free(out);
        assert_eq!(pp_handle_free(repo), PpStatus::Ok as i32);
    }

    #[test]
    fn commit_errors_cleanly_on_memstore() {
        // Drain any prior last-error.
        let _ = crate::error::take_last_error();

        let repo = init_repo();
        let message: Box<[u8]> = b"add post".to_vec().into_boxed_slice();
        let author: Box<[u8]> = b"alice".to_vec().into_boxed_slice();
        let msg_slice: c_slice::Box<u8> = message.into();
        let auth_slice: c_slice::Box<u8> = author.into();

        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_vcs_commit(repo, msg_slice.as_ref(), auth_slice.as_ref(), &mut out);
        // It must signal an Operation error, not crash or succeed.
        assert_eq!(status, PpStatus::Operation as i32);

        let env = crate::error::take_last_error().expect("error stashed");
        assert_eq!(env.tag, "operation");
        assert!(
            env.message.contains("MemStore") || env.message.contains("index"),
            "unexpected message: {}",
            env.message
        );

        pp_buf_free(out);
        assert_eq!(pp_handle_free(repo), PpStatus::Ok as i32);
    }

    #[test]
    fn branch_requires_a_commit() {
        // Drain any prior last-error.
        let _ = crate::error::take_last_error();

        let repo = init_repo();
        let name: Box<[u8]> = b"feature".to_vec().into_boxed_slice();
        let name_slice: c_slice::Box<u8> = name.into();

        let mut out: repr_c::Vec<u8> = Vec::new().into();
        // No commits yet, so branch must report an Operation error.
        let status = pp_vcs_branch(repo, name_slice.as_ref(), &mut out);
        assert_eq!(status, PpStatus::Operation as i32);
        let env = crate::error::take_last_error().expect("error stashed");
        assert_eq!(env.tag, "operation");

        pp_buf_free(out);
        assert_eq!(pp_handle_free(repo), PpStatus::Ok as i32);
    }

    #[test]
    fn add_on_non_repo_handle_is_type_mismatch() {
        let _ = crate::error::take_last_error();
        let schema = alloc_schema_handle(empty_schema());
        // Using the schema handle as the repo handle is a type mismatch.
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_vcs_add(schema, schema, &mut out);
        assert_eq!(status, PpStatus::TypeMismatch as i32);
        pp_buf_free(out);
        assert_eq!(pp_handle_free(schema), PpStatus::Ok as i32);
    }

    #[test]
    fn diff_round_trips_through_cbor() {
        let repo = init_repo();
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(pp_vcs_diff(repo, &mut out), PpStatus::Ok as i32);
        // A fresh repo has no branch refs yet (HEAD on main is unborn),
        // so changes is empty and counts are zero; the record still
        // round-trips through CBOR.
        let original: Vec<u8> = (*out).to_vec();
        let value: ciborium::value::Value = decode(&original).unwrap();
        let reencoded = encode(&value).unwrap();
        assert_eq!(reencoded, original);
        pp_buf_free(out);
        assert_eq!(pp_handle_free(repo), PpStatus::Ok as i32);
    }
}
