//! Tests for the git bridge.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::path::Path;

use panproto_vcs::{MemStore, Store};

use crate::export::export_to_git;
use crate::import::{
    BlobSchemaCache, import_git_repo, import_git_repo_incremental, import_git_repo_persistent,
    import_git_repo_with_cache, load_blob_cache, save_blob_cache,
};

/// Create a temporary git repository with a single commit containing
/// the given files.
fn create_test_git_repo(files: &[(&str, &[u8])]) -> (tempfile::TempDir, git2::Repository) {
    let dir = tempfile::tempdir().unwrap();
    let repo = git2::Repository::init(dir.path()).unwrap();

    // Create files and commit.
    let sig = git2::Signature::new("Test", "test@example.com", &git2::Time::new(1000, 0)).unwrap();

    let mut index = repo.index().unwrap();
    for (path, content) in files {
        let full_path = dir.path().join(path);
        if let Some(parent) = full_path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&full_path, content).unwrap();
        index.add_path(Path::new(path)).unwrap();
    }
    index.write().unwrap();

    let tree_oid = index.write_tree().unwrap();

    {
        let tree = repo.find_tree(tree_oid).unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "Initial commit", &tree, &[])
            .unwrap();
    }

    (dir, repo)
}

#[test]
fn import_single_typescript_file() {
    let (_dir, git_repo) = create_test_git_repo(&[(
        "main.ts",
        b"function greet(name: string): string { return 'Hello, ' + name; }",
    )]);

    let mut store = MemStore::new();
    let result = import_git_repo(&git_repo, &mut store, "HEAD").unwrap();

    assert_eq!(result.commit_count, 1);
    assert_ne!(result.head_id, panproto_vcs::ObjectId::ZERO);
    assert_eq!(result.oid_map.len(), 1);

    // Verify the commit was stored.
    let commit_obj = store.get(&result.head_id).unwrap();
    match &commit_obj {
        panproto_vcs::Object::Commit(c) => {
            assert_eq!(c.message, "Initial commit");
            assert_eq!(c.author, "Test");
        }
        other => panic!("expected commit, got {}", other.type_name()),
    }
}

#[test]
fn import_multi_file_project() {
    let (_dir, git_repo) = create_test_git_repo(&[
        (
            "src/main.ts",
            b"function main(): void { console.log('hello'); }",
        ),
        (
            "src/utils.ts",
            b"export function add(a: number, b: number): number { return a + b; }",
        ),
        ("README.md", b"# Test Project\n\nA test project.\n"),
    ]);

    let mut store = MemStore::new();
    let result = import_git_repo(&git_repo, &mut store, "HEAD").unwrap();

    assert_eq!(result.commit_count, 1);

    // Verify the schema contains vertices from all files.
    let commit_obj = store.get(&result.head_id).unwrap();
    let commit = match &commit_obj {
        panproto_vcs::Object::Commit(c) => c,
        other => panic!("expected commit, got {}", other.type_name()),
    };

    let schema = panproto_vcs::tree::resolve_commit_schema(&store, commit).unwrap();
    assert!(
        schema.vertices.len() > 5,
        "expected rich project schema, got {} vertices",
        schema.vertices.len()
    );
}

#[test]
fn import_multiple_commits() {
    let dir = tempfile::tempdir().unwrap();
    let repo = git2::Repository::init(dir.path()).unwrap();
    let sig = git2::Signature::new("Dev", "dev@test.com", &git2::Time::new(1000, 0)).unwrap();

    // First commit.
    let file_path = dir.path().join("main.py");
    std::fs::write(&file_path, b"x = 1\n").unwrap();
    let mut index = repo.index().unwrap();
    index.add_path(Path::new("main.py")).unwrap();
    index.write().unwrap();
    let tree_oid = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_oid).unwrap();
    let commit1_oid = repo
        .commit(Some("HEAD"), &sig, &sig, "First", &tree, &[])
        .unwrap();

    // Second commit.
    std::fs::write(&file_path, b"x = 1\ny = 2\n").unwrap();
    let mut index = repo.index().unwrap();
    index.add_path(Path::new("main.py")).unwrap();
    index.write().unwrap();
    let tree_oid = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_oid).unwrap();
    let commit1 = repo.find_commit(commit1_oid).unwrap();
    repo.commit(Some("HEAD"), &sig, &sig, "Second", &tree, &[&commit1])
        .unwrap();

    // Import.
    let mut store = MemStore::new();
    let result = import_git_repo(&repo, &mut store, "HEAD").unwrap();

    assert_eq!(result.commit_count, 2);
    assert_eq!(result.oid_map.len(), 2);

    // Verify second commit has first as parent.
    let second_commit_obj = store.get(&result.head_id).unwrap();
    match &second_commit_obj {
        panproto_vcs::Object::Commit(c) => {
            assert_eq!(c.message, "Second");
            assert_eq!(c.parents.len(), 1);
            // Parent should be the first commit's panproto ID.
            let first_panproto_id = result.oid_map[0].1;
            assert_eq!(c.parents[0], first_panproto_id);
        }
        other => panic!("expected commit, got {}", other.type_name()),
    }
}

/// Build a git repo with `n` sequential commits on a single branch.
fn create_linear_history(n: usize) -> (tempfile::TempDir, git2::Repository, Vec<git2::Oid>) {
    let dir = tempfile::tempdir().unwrap();
    let repo = git2::Repository::init(dir.path()).unwrap();
    let sig = git2::Signature::new("Dev", "dev@test.com", &git2::Time::new(1000, 0)).unwrap();
    let file_path = dir.path().join("main.py");

    let mut commit_oids = Vec::new();
    let mut parent: Option<git2::Oid> = None;

    for i in 0..n {
        std::fs::write(&file_path, format!("x = {i}\n").as_bytes()).unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("main.py")).unwrap();
        index.write().unwrap();
        let tree_oid = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_oid).unwrap();

        let parent_commit = parent.map(|p| repo.find_commit(p).unwrap());
        let parents: Vec<&git2::Commit<'_>> = parent_commit.iter().collect();
        let new_oid = repo
            .commit(
                Some("HEAD"),
                &sig,
                &sig,
                &format!("commit {i}"),
                &tree,
                &parents,
            )
            .unwrap();
        commit_oids.push(new_oid);
        parent = Some(new_oid);
    }

    (dir, repo, commit_oids)
}

#[test]
fn incremental_import_skips_known_ancestors() {
    let (_dir, repo, commit_oids) = create_linear_history(3);

    // First: import commits 0..=1 into a store.
    let mut store = MemStore::new();
    let first = import_git_repo(&repo, &mut store, &commit_oids[1].to_string()).unwrap();
    assert_eq!(first.commit_count, 2);

    // Build the known map from the first import's oid_map.
    let known: rustc_hash::FxHashMap<git2::Oid, panproto_vcs::ObjectId> =
        first.oid_map.iter().copied().collect();

    // Now incrementally import up to commit 2. Only one new commit should
    // be imported (commit 2), and its parent should be wired to the
    // already-known panproto ID for commit 1.
    let second =
        import_git_repo_incremental(&repo, &mut store, &commit_oids[2].to_string(), &known)
            .unwrap();

    assert_eq!(second.commit_count, 1, "expected only one new commit");
    assert_eq!(second.oid_map.len(), 1);
    assert_eq!(second.oid_map[0].0, commit_oids[2]);

    // The new HEAD commit should have the imported commit 1 as its
    // single panproto parent.
    let head_obj = store.get(&second.head_id).unwrap();
    match &head_obj {
        panproto_vcs::Object::Commit(c) => {
            assert_eq!(c.parents.len(), 1);
            assert_eq!(c.parents[0], known[&commit_oids[1]]);
        }
        other => panic!("expected commit, got {}", other.type_name()),
    }
}

#[test]
fn incremental_import_noop_when_head_is_known() {
    let (_dir, repo, commit_oids) = create_linear_history(2);

    // First: import everything.
    let mut store = MemStore::new();
    let first = import_git_repo(&repo, &mut store, "HEAD").unwrap();
    assert_eq!(first.commit_count, 2);

    // Re-import with HEAD already known: should be a no-op, head_id preserved.
    let known: rustc_hash::FxHashMap<git2::Oid, panproto_vcs::ObjectId> =
        first.oid_map.iter().copied().collect();
    let second = import_git_repo_incremental(&repo, &mut store, "HEAD", &known).unwrap();

    assert_eq!(second.commit_count, 0);
    assert_eq!(second.head_id, known[&commit_oids[1]]);
    assert!(second.oid_map.is_empty());
}

#[test]
fn incremental_import_sets_no_local_refs() {
    // The import functions should be pure with respect to refs: only
    // object insertion. Naming the imported tip is the caller's job.
    let (_dir, repo, _oids) = create_linear_history(2);

    let mut store = MemStore::new();
    let result = import_git_repo(&repo, &mut store, "HEAD").unwrap();
    assert!(result.head_id != panproto_vcs::ObjectId::ZERO);

    // No refs should exist under refs/ after an import (regression test
    // for the removed hardcoded "refs/heads/main" write).
    let refs = store.list_refs("refs/").unwrap();
    assert!(
        refs.is_empty(),
        "expected no refs after import, found: {refs:?}"
    );
}

#[test]
fn incremental_import_tolerates_stale_known_entries() {
    // A `known` map entry whose git OID is not reachable from the head
    // being imported should be silently ignored; revwalk.hide returns an
    // error in that case and we swallow it so stale caches don't break
    // subsequent imports.
    let (_dir, repo, commit_oids) = create_linear_history(2);

    // Build a known map containing a fabricated git OID that does not
    // exist in the repo, plus the real first commit (which should still
    // be honored as a skip target).
    let fake_oid = git2::Oid::from_str("0123456789abcdef0123456789abcdef01234567").unwrap();
    let mut first_store = MemStore::new();
    let first = import_git_repo(&repo, &mut first_store, &commit_oids[0].to_string()).unwrap();
    let first_panproto = first.oid_map[0].1;

    let mut known: rustc_hash::FxHashMap<git2::Oid, panproto_vcs::ObjectId> =
        rustc_hash::FxHashMap::default();
    known.insert(fake_oid, panproto_vcs::ObjectId::ZERO);
    known.insert(commit_oids[0], first_panproto);

    // Importing the second commit with this mixed known map should
    // succeed, yield exactly one new commit, and wire its parent to the
    // real first_panproto id.
    let mut store = first_store;
    let result = import_git_repo_incremental(&repo, &mut store, "HEAD", &known).unwrap();
    assert_eq!(result.commit_count, 1);
    let head_obj = store.get(&result.head_id).unwrap();
    match &head_obj {
        panproto_vcs::Object::Commit(c) => {
            assert_eq!(c.parents, vec![first_panproto]);
        }
        other => panic!("expected commit, got {}", other.type_name()),
    }
}

/// Build a fresh git repository with no commits, for testing export.
fn empty_git_repo() -> (tempfile::TempDir, git2::Repository) {
    let dir = tempfile::tempdir().unwrap();
    let repo = git2::Repository::init(dir.path()).unwrap();
    (dir, repo)
}

#[test]
fn export_with_update_ref_none_leaves_head_unborn() {
    // Import a single commit into a panproto store.
    let (_src_dir, src_repo, _oids) = create_linear_history(1);
    let mut store = MemStore::new();
    let import_result = import_git_repo(&src_repo, &mut store, "HEAD").unwrap();

    // Export into a fresh repo with update_ref = None. The commit object
    // should exist, but HEAD should not yet have been born.
    let (_dst_dir, dst_repo) = empty_git_repo();
    let parent_map: rustc_hash::FxHashMap<panproto_vcs::ObjectId, git2::Oid> =
        rustc_hash::FxHashMap::default();
    let result =
        export_to_git(&store, &dst_repo, import_result.head_id, &parent_map, None).unwrap();

    // The created commit exists as an object.
    assert!(dst_repo.find_commit(result.git_oid).is_ok());

    // HEAD still points to the unborn initial branch. `revparse_single`
    // on HEAD should fail because no commit has been attached to it.
    assert!(
        dst_repo.head().is_err(),
        "HEAD should remain unborn when update_ref is None"
    );
}

#[test]
fn export_with_update_ref_some_moves_named_ref() {
    let (_src_dir, src_repo, _oids) = create_linear_history(1);
    let mut store = MemStore::new();
    let import_result = import_git_repo(&src_repo, &mut store, "HEAD").unwrap();

    // Export into a fresh repo with update_ref = Some("HEAD"). HEAD
    // should now resolve to the new commit.
    let (_dst_dir, dst_repo) = empty_git_repo();
    let parent_map: rustc_hash::FxHashMap<panproto_vcs::ObjectId, git2::Oid> =
        rustc_hash::FxHashMap::default();
    let result = export_to_git(
        &store,
        &dst_repo,
        import_result.head_id,
        &parent_map,
        Some("HEAD"),
    )
    .unwrap();

    let head = dst_repo.head().unwrap();
    let head_commit = head.peel_to_commit().unwrap();
    assert_eq!(head_commit.id(), result.git_oid);
}

#[test]
fn export_parent_map_links_exported_parent() {
    // Import a 2-commit history so we have two panproto commit IDs with
    // a real parent relationship.
    let (_src_dir, src_repo, _oids) = create_linear_history(2);
    let mut store = MemStore::new();
    let import_result = import_git_repo(&src_repo, &mut store, "HEAD").unwrap();
    let first_panproto = import_result.oid_map[0].1;
    let second_panproto = import_result.oid_map[1].1;

    // Export into a fresh repo: first commit with empty map, second
    // commit with the first-commit mapping populated.
    let (_dst_dir, dst_repo) = empty_git_repo();
    let mut parent_map: rustc_hash::FxHashMap<panproto_vcs::ObjectId, git2::Oid> =
        rustc_hash::FxHashMap::default();

    let first_result = export_to_git(&store, &dst_repo, first_panproto, &parent_map, None).unwrap();
    parent_map.insert(first_panproto, first_result.git_oid);

    let second_result =
        export_to_git(&store, &dst_repo, second_panproto, &parent_map, None).unwrap();

    // The second git commit should have the first as its parent.
    let second_git = dst_repo.find_commit(second_result.git_oid).unwrap();
    assert_eq!(second_git.parent_count(), 1);
    assert_eq!(second_git.parent(0).unwrap().id(), first_result.git_oid);
}

#[test]
fn export_parent_map_empty_produces_root_commit() {
    // A panproto commit with parents, exported with an empty parent_map,
    // should produce a git commit with zero git parents (the panproto
    // parents are "invisible" to the export because they aren't mapped).
    let (_src_dir, src_repo, _oids) = create_linear_history(2);
    let mut store = MemStore::new();
    let import_result = import_git_repo(&src_repo, &mut store, "HEAD").unwrap();
    let second_panproto = import_result.oid_map[1].1;

    let (_dst_dir, dst_repo) = empty_git_repo();
    let parent_map: rustc_hash::FxHashMap<panproto_vcs::ObjectId, git2::Oid> =
        rustc_hash::FxHashMap::default();

    let result = export_to_git(&store, &dst_repo, second_panproto, &parent_map, None).unwrap();

    let git_commit = dst_repo.find_commit(result.git_oid).unwrap();
    assert_eq!(
        git_commit.parent_count(),
        0,
        "unmapped panproto parents should not produce git parents"
    );
}

/// Build a git repo with 3 commits that each touch a different file,
/// while 4 other files remain unchanged across all commits.
fn create_dedup_history() -> (tempfile::TempDir, git2::Repository, Vec<git2::Oid>) {
    let dir = tempfile::tempdir().unwrap();
    let repo = git2::Repository::init(dir.path()).unwrap();
    let sig = git2::Signature::new("Dev", "dev@test.com", &git2::Time::new(1000, 0)).unwrap();

    // Seed the 5 files.
    for (path, content) in [
        ("a.py", "x = 1\n"),
        ("b.py", "y = 2\n"),
        ("c.py", "z = 3\n"),
        ("d.py", "w = 4\n"),
        ("e.py", "v = 5\n"),
    ] {
        std::fs::write(dir.path().join(path), content).unwrap();
    }

    let mut commit_oids = Vec::new();
    let mut parent: Option<git2::Oid> = None;

    // Commit 0: all five files.
    // Commit 1: modify a.py only.
    // Commit 2: modify b.py only.
    let mutations: [&[(&str, &str)]; 3] = [
        &[
            ("a.py", "x = 1\n"),
            ("b.py", "y = 2\n"),
            ("c.py", "z = 3\n"),
            ("d.py", "w = 4\n"),
            ("e.py", "v = 5\n"),
        ],
        &[("a.py", "x = 11\n")],
        &[("b.py", "y = 22\n")],
    ];

    for (i, batch) in mutations.iter().enumerate() {
        for (path, content) in *batch {
            std::fs::write(dir.path().join(path), content).unwrap();
        }
        let mut index = repo.index().unwrap();
        for name in ["a.py", "b.py", "c.py", "d.py", "e.py"] {
            index.add_path(Path::new(name)).unwrap();
        }
        index.write().unwrap();
        let tree_oid = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_oid).unwrap();

        let parent_commit = parent.map(|p| repo.find_commit(p).unwrap());
        let parents: Vec<&git2::Commit<'_>> = parent_commit.iter().collect();
        let new_oid = repo
            .commit(
                Some("HEAD"),
                &sig,
                &sig,
                &format!("commit {i}"),
                &tree,
                &parents,
            )
            .unwrap();
        commit_oids.push(new_oid);
        parent = Some(new_oid);
    }

    (dir, repo, commit_oids)
}

#[test]
fn import_persistent_roundtrips_cache() {
    let (_dir, repo, _oids) = create_dedup_history();
    let mut store = MemStore::new();
    let cache_dir = tempfile::tempdir().unwrap();
    let known: rustc_hash::FxHashMap<git2::Oid, panproto_vcs::ObjectId> =
        rustc_hash::FxHashMap::default();

    let r1 =
        import_git_repo_persistent(&repo, &mut store, "HEAD", &known, cache_dir.path()).unwrap();
    assert_eq!(r1.commit_count, 3);

    // The cache file must exist and be non-empty; a second import
    // with the same revspec and matching marks must be a no-op.
    let cache_path = cache_dir.path().join(crate::import::BLOB_CACHE_FILE);
    assert!(cache_path.is_file());
    let loaded = load_blob_cache(&cache_path).unwrap();
    assert_eq!(loaded.len(), 7);

    // Atomic save: writing a new cache must replace, not leak tmp.
    save_blob_cache(&cache_path, &loaded).unwrap();
    let tmp = cache_path.with_extension("tmp");
    assert!(!tmp.exists(), "save_blob_cache must rename atomically");
}

#[test]
fn blob_cache_rejects_corrupt_file() {
    let cache_dir = tempfile::tempdir().unwrap();
    let cache_path = cache_dir.path().join("corrupt");
    std::fs::write(&cache_path, "not a valid cache entry\n").unwrap();
    let err = load_blob_cache(&cache_path).unwrap_err();
    assert!(format!("{err}").contains("corrupt"));
}

#[test]
fn blob_cache_missing_is_empty() {
    let cache_dir = tempfile::tempdir().unwrap();
    let cache = load_blob_cache(&cache_dir.path().join("missing")).unwrap();
    assert!(cache.is_empty());
}

#[test]
fn blob_cache_dedupes_unchanged_files_across_commits() {
    let (_dir, repo, _oids) = create_dedup_history();
    let mut store = MemStore::new();
    let mut cache = BlobSchemaCache::default();
    let known: rustc_hash::FxHashMap<git2::Oid, panproto_vcs::ObjectId> =
        rustc_hash::FxHashMap::default();

    let result = import_git_repo_with_cache(&repo, &mut store, "HEAD", &known, &mut cache).unwrap();
    assert_eq!(result.commit_count, 3);

    // After importing all three commits, the blob cache should contain
    // exactly seven unique blob OIDs: five initial blobs plus two new
    // blobs for the modified a.py and b.py. Four of the initial five
    // are fully deduped (c.py, d.py, e.py carry across; a.py and b.py
    // each have an extra version).
    assert_eq!(
        cache.len(),
        7,
        "expected 7 distinct blob OIDs (5 initial + 2 modifications)"
    );

    // Every commit must point at a SchemaTree, not a flat Schema.
    for (_, panproto_id) in &result.oid_map {
        match store.get(panproto_id).unwrap() {
            panproto_vcs::Object::Commit(c) => match store.get(&c.schema_id).unwrap() {
                panproto_vcs::Object::SchemaTree(_) => {}
                other => panic!(
                    "expected commit schema_id to point at schema_tree, got {}",
                    other.type_name()
                ),
            },
            other => panic!("expected commit, got {}", other.type_name()),
        }
    }
}
