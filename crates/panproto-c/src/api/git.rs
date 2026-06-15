//! Git bridge: import a git repository into a panproto-vcs store.
//!
//! Available only under the `git` feature. Frozen-signature scaffold;
//! the entry point currently returns [`PpStatus::Operation`]. The
//! engine-wiring pass fills in the body against `panproto_core::git`
//! (`import_git_repo`), producing a
//! [`Resource::VcsRepo`](crate::handle::Resource).

use safer_ffi::prelude::*;

use crate::error::FfiError;
use crate::panic::guard;

/// Import a git repository into a fresh in-memory VCS store.
///
/// `repo_path` is the UTF-8 path to the git repository; `revspec` is
/// the UTF-8 revision specifier to import. On success, `out_handle`
/// receives a fresh [`Resource::VcsRepo`](crate::handle::Resource)
/// handle, and `out` receives a CBOR-encoded
/// `{ commit_count, head_id }` summary. Will open the repo with `git2`
/// and call `git::import_git_repo`.
///
/// Stub: returns [`PpStatus::Operation`](crate::error::PpStatus::Operation)
/// until implemented in the engine-wiring pass.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_git_import(
    repo_path: c_slice::Ref<'_, u8>,
    revspec: c_slice::Ref<'_, u8>,
    out_handle: &mut u32,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    let _ = (repo_path, revspec, out_handle, out);
    guard(|| Err(FfiError::Operation("unimplemented: pp_git_import".into())))
}
