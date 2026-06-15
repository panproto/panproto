//! Schematic version control operations.
//!
//! Frozen-signature scaffold. Every entry point currently returns
//! [`PpStatus::Operation`]; the engine-wiring pass fills in the bodies
//! against `panproto_core::vcs`. The repo handle is a mutable
//! [`Resource::VcsRepo`](crate::handle::Resource) accessed via
//! [`crate::handle::with_resource_mut`].

use safer_ffi::prelude::*;

use crate::error::FfiError;
use crate::panic::guard;

/// Initialize an in-memory VCS repository.
///
/// `protocol_name` is the UTF-8 protocol name bytes. On success,
/// `out_handle` receives a fresh
/// [`Resource::VcsRepo`](crate::handle::Resource) handle. Will call
/// `vcs::MemStore::new`.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_vcs_init(protocol_name: c_slice::Ref<'_, u8>, out_handle: &mut u32) -> i32 {
    let _ = (protocol_name, out_handle);
    guard(|| Err(FfiError::Operation("unimplemented: pp_vcs_init".into())))
}

/// Stage a schema in a VCS repository.
///
/// `repo` is a VCS repo handle; `schema` is a
/// [`Resource::Schema`](crate::handle::Resource) handle. On success,
/// `out` receives a CBOR-encoded
/// [`VcsAddResult`](crate::api::helpers::VcsAddResult). Will call
/// `vcs::tree::store_schema_as_tree`.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_vcs_add(repo: u32, schema: u32, out: &mut repr_c::Vec<u8>) -> i32 {
    let _ = (repo, schema, out);
    guard(|| Err(FfiError::Operation("unimplemented: pp_vcs_add".into())))
}

/// Commit the staged schema in a VCS repository.
///
/// `repo` is a VCS repo handle; `message` and `author` are UTF-8 bytes.
/// On success, `out` receives a CBOR-encoded commit ID string.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_vcs_commit(
    repo: u32,
    message: c_slice::Ref<'_, u8>,
    author: c_slice::Ref<'_, u8>,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    let _ = (repo, message, author, out);
    guard(|| Err(FfiError::Operation("unimplemented: pp_vcs_commit".into())))
}

/// Walk the commit log from HEAD.
///
/// `repo` is a VCS repo handle; `count` caps the walk length. On
/// success, `out` receives a CBOR-encoded vector of
/// [`VcsLogEntry`](crate::api::helpers::VcsLogEntry) records. Will call
/// `vcs::dag::log_walk`.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_vcs_log(repo: u32, count: u32, out: &mut repr_c::Vec<u8>) -> i32 {
    let _ = (repo, count, out);
    guard(|| Err(FfiError::Operation("unimplemented: pp_vcs_log".into())))
}

/// Get repository status (branch and HEAD).
///
/// `repo` is a VCS repo handle. On success, `out` receives a
/// CBOR-encoded
/// [`VcsStatusResult`](crate::api::helpers::VcsStatusResult).
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_vcs_status(repo: u32, out: &mut repr_c::Vec<u8>) -> i32 {
    let _ = (repo, out);
    guard(|| Err(FfiError::Operation("unimplemented: pp_vcs_status".into())))
}

/// List branches and the commit each points at.
///
/// `repo` is a VCS repo handle. On success, `out` receives a
/// CBOR-encoded
/// [`VcsDiffResult`](crate::api::helpers::VcsDiffResult). Will call
/// `vcs::refs::list_branches`.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_vcs_diff(repo: u32, out: &mut repr_c::Vec<u8>) -> i32 {
    let _ = (repo, out);
    guard(|| Err(FfiError::Operation("unimplemented: pp_vcs_diff".into())))
}

/// Create a new branch from HEAD.
///
/// `repo` is a VCS repo handle; `name` is the UTF-8 branch name. On
/// success, `out` receives a CBOR-encoded
/// [`VcsOpResult`](crate::api::helpers::VcsOpResult). Will call
/// `vcs::refs::create_branch`.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_vcs_branch(repo: u32, name: c_slice::Ref<'_, u8>, out: &mut repr_c::Vec<u8>) -> i32 {
    let _ = (repo, name, out);
    guard(|| Err(FfiError::Operation("unimplemented: pp_vcs_branch".into())))
}

/// Checkout a branch or commit.
///
/// `repo` is a VCS repo handle; `target` is the UTF-8 branch/commit
/// reference. On success, `out` receives a CBOR-encoded
/// [`VcsOpResult`](crate::api::helpers::VcsOpResult). Will call
/// `vcs::refs::checkout_branch`.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_vcs_checkout(repo: u32, target: c_slice::Ref<'_, u8>, out: &mut repr_c::Vec<u8>) -> i32 {
    let _ = (repo, target, out);
    guard(|| Err(FfiError::Operation("unimplemented: pp_vcs_checkout".into())))
}

/// Merge a branch into the current branch.
///
/// `repo` is a VCS repo handle; `branch` is the UTF-8 branch name. On
/// success, `out` receives a CBOR-encoded
/// [`VcsOpResult`](crate::api::helpers::VcsOpResult).
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_vcs_merge(repo: u32, branch: c_slice::Ref<'_, u8>, out: &mut repr_c::Vec<u8>) -> i32 {
    let _ = (repo, branch, out);
    guard(|| Err(FfiError::Operation("unimplemented: pp_vcs_merge".into())))
}

/// Stash the current working state.
///
/// `repo` is a VCS repo handle. On success, `out` receives a
/// CBOR-encoded [`VcsOpResult`](crate::api::helpers::VcsOpResult). Will
/// call `vcs::stash::stash_list`.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_vcs_stash(repo: u32, out: &mut repr_c::Vec<u8>) -> i32 {
    let _ = (repo, out);
    guard(|| Err(FfiError::Operation("unimplemented: pp_vcs_stash".into())))
}

/// Pop the most recent stash entry.
///
/// `repo` is a VCS repo handle. On success, `out` receives a
/// CBOR-encoded [`VcsOpResult`](crate::api::helpers::VcsOpResult). Will
/// call `vcs::stash::stash_pop`.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_vcs_stash_pop(repo: u32, out: &mut repr_c::Vec<u8>) -> i32 {
    let _ = (repo, out);
    guard(|| {
        Err(FfiError::Operation(
            "unimplemented: pp_vcs_stash_pop".into(),
        ))
    })
}

/// Blame a vertex: find the commit that introduced it.
///
/// `repo` is a VCS repo handle; `vertex` is the UTF-8 vertex ID. On
/// success, `out` receives a CBOR-encoded
/// [`VcsBlameResult`](crate::api::helpers::VcsBlameResult). Will call
/// `vcs::blame::blame_vertex`.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_vcs_blame(repo: u32, vertex: c_slice::Ref<'_, u8>, out: &mut repr_c::Vec<u8>) -> i32 {
    let _ = (repo, vertex, out);
    guard(|| Err(FfiError::Operation("unimplemented: pp_vcs_blame".into())))
}
