//! Tests for the git bridge.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::path::Path;

use panproto_vcs::{MemStore, Store};

use crate::import::import_git_repo;

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

    let schema_obj = store.get(&commit.schema_id).unwrap();
    match &schema_obj {
        panproto_vcs::Object::Schema(s) => {
            assert!(
                s.vertices.len() > 5,
                "expected rich project schema, got {} vertices",
                s.vertices.len()
            );
        }
        other => panic!("expected schema, got {}", other.type_name()),
    }
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
