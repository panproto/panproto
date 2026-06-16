//! Git bridge: import a git repository into a panproto-vcs store.
//!
//! Available only under the `git` feature. Ported from the working
//! reference in `crates/panproto-py/src/git.rs` (the Python `git_import`),
//! with the `PyO3` result class and exception type replaced by the canonical
//! CBOR codec and [`FfiError`]. The on-disk repository is opened here with
//! [`git2`] (exactly as the Python surface does); the walk itself is driven
//! by `panproto_core::git::import_git_repo`, which reads the git commit DAG
//! into a `panproto_core::vcs::Store`.
//!
//! The imported history lands in a fresh on-disk
//! `panproto_core::vcs::Repository`, rooted at a process-lifetime
//! directory under the system temp dir. The repository becomes a
//! [`Resource::VcsRepo`](crate::handle::Resource) handle, the same resource
//! [`pp_vcs_init`](crate::api::pp_vcs_init) allocates, so the caller can
//! drive the result with the `pp_vcs_*` porcelain (log, branch, diff, …) or
//! release it with [`pp_handle_free`](crate::api::pp_handle_free).
//!
//! # Wire format
//!
//! On success `out` carries a CBOR map `{ commit_count, head_id }`: the
//! number of commits walked, and the imported HEAD's
//! `panproto_core::vcs::ObjectId` rendered as its lowercase-hex `Display`
//! string (never the raw `[u8; 32]` `serde` array). This matches the
//! `vcs` surface, where object ids always cross the boundary as hex
//! strings, and the Haskell `Panproto.Git.decodeGitImportResult` decoder
//! that reads it.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use panproto_core::git::import_git_repo;
use panproto_core::vcs::Repository;
use safer_ffi::prelude::*;
use serde::Serialize;

use crate::error::{FfiError, PpStatus};
use crate::handle::{self, Resource};
use crate::panic::guard;

/// Monotonic counter disambiguating concurrent import roots within a
/// single process (the timestamp alone is not unique enough under fast
/// repeated imports).
static IMPORT_SEQ: AtomicU64 = AtomicU64::new(0);

/// Allocate a fresh, process-lifetime directory under the system temp
/// dir for an imported repository's on-disk store. The directory is not
/// auto-removed: it backs the `Repository` handle for as long as the
/// caller holds it, which is the lifetime of an import inspection.
fn fresh_import_root() -> std::path::PathBuf {
    let seq = IMPORT_SEQ.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    std::env::temp_dir().join(format!("panproto-git-import-{nanos}-{seq}"))
}

/// The `git_import` summary, matching the Haskell `GitImportResult`
/// decoder: the commit count plus the imported HEAD's object id as a
/// lowercase-hex string. Mirrors the wire-relevant fields of
/// `panproto_core::git::ImportResult` (the per-commit oid map it also
/// carries does not cross the boundary).
#[derive(Debug, Serialize)]
struct GitImportResultRecord {
    commit_count: usize,
    head_id: String,
}

/// Import a git repository into a fresh on-disk VCS repository.
///
/// `repo_path` is the UTF-8 path to the git repository; `revspec` is the
/// UTF-8 revision specifier to import (e.g. `"HEAD"`, `"main"`,
/// `"HEAD~10..HEAD"`). On success, `out_handle` receives a fresh
/// [`Resource::VcsRepo`](crate::handle::Resource) handle wrapping a
/// `Repository` rooted at a process-lifetime temp directory, and `out`
/// receives a CBOR-encoded `{ commit_count, head_id }` summary.
///
/// Opens the source repository with [`git2::Repository::open`] and walks
/// it via `panproto_core::git::import_git_repo`, which writes the commit
/// DAG into the new repository's `FsStore`. Both arguments are validated
/// as UTF-8 at the boundary; a malformed path, an unopenable repository,
/// a store init failure, or a failed walk surfaces as
/// [`PpStatus::Operation`]. The out-handle slot is only written on
/// success, so a failed call leaves it untouched.
#[must_use = "FFI status codes should not be discarded"]
#[ffi_export]
pub fn pp_git_import(
    repo_path: c_slice::Ref<'_, u8>,
    revspec: c_slice::Ref<'_, u8>,
    out_handle: &mut u32,
    out: &mut repr_c::Vec<u8>,
) -> i32 {
    guard(|| {
        let repo_path_str = std::str::from_utf8(repo_path.as_slice())
            .map_err(|e| FfiError::Operation(format!("invalid repo path: {e}")))?;
        let revspec_str = std::str::from_utf8(revspec.as_slice())
            .map_err(|e| FfiError::Operation(format!("invalid revspec: {e}")))?;

        let git_repo = git2::Repository::open(repo_path_str).map_err(|e| {
            FfiError::Operation(format!("failed to open git repo at {repo_path_str:?}: {e}"))
        })?;

        // Fresh on-disk repository to receive the imported history.
        let root = fresh_import_root();
        let mut repository =
            Repository::init(&root).map_err(|e| FfiError::Operation(e.to_string()))?;
        let result = import_git_repo(&git_repo, repository.store_mut(), revspec_str)
            .map_err(|e| FfiError::Operation(e.to_string()))?;

        let summary = GitImportResultRecord {
            commit_count: result.commit_count,
            head_id: result.head_id.to_string(),
        };
        *out = crate::canonical::encode(&summary)?.into();

        // Allocate the imported repository as a VcsRepo handle only after
        // the summary has encoded cleanly, so a serialization failure
        // does not leak a slab slot.
        *out_handle = handle::alloc(Resource::VcsRepo(Box::new(repository)));
        Ok(PpStatus::Ok)
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::path::Path;

    use safer_ffi::prelude::c_slice;
    use serde::Deserialize;

    use super::*;
    use crate::api::{pp_buf_free, pp_handle_free};
    use crate::canonical::decode;

    /// Mirror of the Haskell decoder's view of the summary, used to read
    /// back the CBOR the entry point writes.
    #[derive(Debug, Deserialize)]
    struct SummaryView {
        commit_count: usize,
        head_id: String,
    }

    /// Build a throwaway on-disk git repository with one commit per file
    /// set, returning the temp dir (kept alive for its `Drop`) and its
    /// path. Each entry in `commits` is the file list for one commit,
    /// applied in order so the history has `commits.len()` commits.
    fn create_test_git_repo(commits: &[&[(&str, &[u8])]]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(dir.path()).unwrap();
        let sig =
            git2::Signature::new("Test", "test@example.com", &git2::Time::new(1000, 0)).unwrap();

        let mut parent_oid: Option<git2::Oid> = None;
        for files in commits {
            let mut index = repo.index().unwrap();
            for (path, content) in *files {
                let full_path = dir.path().join(path);
                if let Some(parent) = full_path.parent() {
                    std::fs::create_dir_all(parent).unwrap();
                }
                std::fs::write(&full_path, content).unwrap();
                index.add_path(Path::new(path)).unwrap();
            }
            index.write().unwrap();
            let tree_oid = index.write_tree().unwrap();
            let tree = repo.find_tree(tree_oid).unwrap();
            let parents: Vec<git2::Commit<'_>> = parent_oid
                .map(|oid| repo.find_commit(oid).unwrap())
                .into_iter()
                .collect();
            let parent_refs: Vec<&git2::Commit<'_>> = parents.iter().collect();
            let oid = repo
                .commit(Some("HEAD"), &sig, &sig, "Test commit", &tree, &parent_refs)
                .unwrap();
            parent_oid = Some(oid);
        }
        dir
    }

    fn slice_box(bytes: &[u8]) -> c_slice::Box<u8> {
        let boxed: Box<[u8]> = bytes.to_vec().into_boxed_slice();
        boxed.into()
    }

    #[test]
    fn import_single_commit_allocates_repo_and_summarizes() {
        let dir = create_test_git_repo(&[&[("main.ts", b"export const x: number = 1;" as &[u8])]]);
        let path = dir.path().to_str().unwrap();

        let path_slice = slice_box(path.as_bytes());
        let rev_slice = slice_box(b"HEAD");
        let mut h: u32 = u32::MAX;
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_git_import(path_slice.as_ref(), rev_slice.as_ref(), &mut h, &mut out);
        assert_eq!(status, PpStatus::Ok as i32);

        let summary: SummaryView = decode(&out).unwrap();
        assert_eq!(summary.commit_count, 1, "one commit expected");
        // head_id is a 64-char lowercase-hex blake3 digest.
        assert_eq!(summary.head_id.len(), 64, "head_id: {}", summary.head_id);
        assert!(
            summary.head_id.chars().all(|c| c.is_ascii_hexdigit()),
            "head_id not hex: {}",
            summary.head_id
        );

        // The returned handle is a usable VcsRepo: a status read succeeds.
        let mut status_out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            crate::api::pp_vcs_status(h, &mut status_out),
            PpStatus::Ok as i32
        );
        pp_buf_free(status_out);

        pp_buf_free(out);
        assert_eq!(pp_handle_free(h), PpStatus::Ok as i32);
    }

    #[test]
    fn import_walks_multiple_commits() {
        let dir = create_test_git_repo(&[
            &[("a.txt", b"one" as &[u8])],
            &[("a.txt", b"one" as &[u8]), ("b.txt", b"two" as &[u8])],
        ]);
        let path = dir.path().to_str().unwrap();

        let path_slice = slice_box(path.as_bytes());
        let rev_slice = slice_box(b"HEAD");
        let mut h: u32 = u32::MAX;
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        assert_eq!(
            pp_git_import(path_slice.as_ref(), rev_slice.as_ref(), &mut h, &mut out),
            PpStatus::Ok as i32
        );
        let summary: SummaryView = decode(&out).unwrap();
        assert_eq!(summary.commit_count, 2, "two commits expected");

        pp_buf_free(out);
        assert_eq!(pp_handle_free(h), PpStatus::Ok as i32);
    }

    #[test]
    fn open_failure_is_an_operation_error() {
        let _ = crate::error::take_last_error();
        let dir = tempfile::tempdir().unwrap(); // not a git repo
        let path = dir.path().to_str().unwrap();

        let path_slice = slice_box(path.as_bytes());
        let rev_slice = slice_box(b"HEAD");
        let mut h: u32 = u32::MAX;
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_git_import(path_slice.as_ref(), rev_slice.as_ref(), &mut h, &mut out);
        assert_eq!(status, PpStatus::Operation as i32);
        // The handle slot is left untouched on failure.
        assert_eq!(h, u32::MAX);

        let env = crate::error::take_last_error().expect("error stashed");
        assert_eq!(env.tag, "operation");
        assert!(
            env.message.contains("open git repo"),
            "unexpected message: {}",
            env.message
        );

        pp_buf_free(out);
    }

    #[test]
    fn invalid_utf8_path_is_an_operation_error() {
        let _ = crate::error::take_last_error();
        // 0xFF is not valid UTF-8.
        let path_slice = slice_box(&[0xFF, 0xFE]);
        let rev_slice = slice_box(b"HEAD");
        let mut h: u32 = u32::MAX;
        let mut out: repr_c::Vec<u8> = Vec::new().into();
        let status = pp_git_import(path_slice.as_ref(), rev_slice.as_ref(), &mut h, &mut out);
        assert_eq!(status, PpStatus::Operation as i32);
        let env = crate::error::take_last_error().expect("error stashed");
        assert!(
            env.message.contains("invalid repo path"),
            "unexpected message: {}",
            env.message
        );
        pp_buf_free(out);
    }
}
