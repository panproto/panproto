//! Import git repositories into panproto-vcs.
//!
//! Walks the git commit DAG topologically, parses each commit's file tree
//! into a panproto project schema, and creates panproto-vcs commits that
//! preserve authorship, timestamps, and parent structure.

use std::collections::HashMap;
use std::hash::BuildHasher;
use std::path::PathBuf;

use panproto_project::ProjectBuilder;
use panproto_vcs::{CommitObject, Object, ObjectId, Store};
use rustc_hash::FxHashMap;

use crate::error::GitBridgeError;

/// Result of importing a git repository.
#[derive(Debug)]
pub struct ImportResult {
    /// Number of commits imported.
    pub commit_count: usize,
    /// The panproto-vcs object ID of the HEAD commit after import.
    pub head_id: ObjectId,
    /// Mapping from git commit OIDs to panproto-vcs object IDs.
    pub oid_map: Vec<(git2::Oid, ObjectId)>,
}

/// Import a range of git commits into a panproto-vcs store.
///
/// Walks the git commit DAG starting from `revspec` (e.g. "HEAD", "main",
/// "HEAD~10..HEAD") in topological order. For each commit:
///
/// 1. Reads all files from the git tree
/// 2. Parses them into a project schema via `panproto-project`
/// 3. Stores the schema as a panproto-vcs object
/// 4. Creates a panproto-vcs commit preserving author, timestamp, message, parents
///
/// This is a convenience wrapper around [`import_git_repo_incremental`] with
/// an empty `known` map, which re-imports the entire history reachable from
/// `revspec`. For repeated imports against a persistent store, prefer
/// [`import_git_repo_incremental`] to avoid walking already-imported ancestors.
///
/// # Errors
///
/// Returns [`GitBridgeError`] if git operations, parsing, or VCS operations fail.
pub fn import_git_repo<S: Store>(
    git_repo: &git2::Repository,
    panproto_store: &mut S,
    revspec: &str,
) -> Result<ImportResult, GitBridgeError> {
    import_git_repo_incremental(git_repo, panproto_store, revspec, &FxHashMap::default())
}

/// Incrementally import a range of git commits into a panproto-vcs store.
///
/// Like [`import_git_repo`], but skips commits whose git OID appears in
/// `known`. The `known` map provides the panproto-vcs [`ObjectId`] that
/// each already-imported git commit was translated to, so that children
/// of skipped commits can be wired up to the correct panproto parent.
///
/// Skipping is performed via `git2`'s revwalk `hide`, so the walker never
/// visits ancestors of known commits either. This makes repeated imports
/// against a persistent store run in time proportional to the *new*
/// commits, not the full history.
///
/// # Edge cases
///
/// - If `revspec` itself resolves to a commit in `known`, no commits are
///   imported and [`ImportResult::head_id`] is set from the `known` map.
/// - If a new commit has a parent that is neither in `known` nor walked
///   (i.e. the `known` map is inconsistent with the actual DAG), that
///   parent is dropped from the panproto commit's parents, matching the
///   behavior of the non-incremental path.
///
/// # Errors
///
/// Returns [`GitBridgeError`] if git operations, parsing, or VCS operations fail.
pub fn import_git_repo_incremental<S: Store, H: BuildHasher>(
    git_repo: &git2::Repository,
    panproto_store: &mut S,
    revspec: &str,
    known: &HashMap<git2::Oid, ObjectId, H>,
) -> Result<ImportResult, GitBridgeError> {
    // Resolve the revspec to a commit.
    let obj = git_repo.revparse_single(revspec)?;
    let head_commit = obj
        .peel_to_commit()
        .map_err(|e| GitBridgeError::ObjectRead {
            oid: obj.id().to_string(),
            reason: format!("not a commit: {e}"),
        })?;
    let head_git_oid = head_commit.id();

    // Collect new commits in topological order (parents before children),
    // skipping any commit reachable from a `known` entry.
    let mut commits = Vec::new();
    collect_new_ancestors(git_repo, head_git_oid, known, &mut commits)?;

    // Seed the git→panproto map with already-known entries so that new
    // commits can resolve parents that live on the "known" side of the cut.
    let mut git_to_panproto: FxHashMap<git2::Oid, ObjectId> =
        known.iter().map(|(&k, &v)| (k, v)).collect();
    let mut oid_map: Vec<(git2::Oid, ObjectId)> = Vec::new();
    let mut last_id = ObjectId::ZERO;

    for git_oid in &commits {
        let git_commit = git_repo.find_commit(*git_oid)?;
        let tree = git_commit.tree()?;

        // Parse all files in the tree into a project schema.
        let mut project_builder = ProjectBuilder::new();
        walk_git_tree(git_repo, &tree, &PathBuf::new(), &mut project_builder)?;

        // Build the project schema.
        let project = if project_builder.file_count() == 0 {
            // Empty tree (initial commit with no files). Create a minimal schema.
            let proto = panproto_protocols::raw_file::protocol();
            let builder = panproto_schema::SchemaBuilder::new(&proto);

            builder
                .vertex("root", "file", None)
                .map_err(|e| {
                    GitBridgeError::Project(panproto_project::ProjectError::CoproductFailed {
                        reason: format!("empty tree schema: {e}"),
                    })
                })?
                .build()
                .map_err(|e| {
                    GitBridgeError::Project(panproto_project::ProjectError::CoproductFailed {
                        reason: format!("empty tree build: {e}"),
                    })
                })?
        } else {
            project_builder.build()?.schema
        };

        // Store the schema.
        let schema_id = panproto_store.put(&Object::Schema(Box::new(project)))?;

        // Map parent git OIDs to panproto-vcs parent IDs.
        let parents: Vec<ObjectId> = git_commit
            .parent_ids()
            .filter_map(|parent_oid| git_to_panproto.get(&parent_oid).copied())
            .collect();

        // Extract author info.
        let author_sig = git_commit.author();
        let author = author_sig.name().unwrap_or("unknown").to_owned();
        let timestamp = u64::try_from(author_sig.when().seconds()).unwrap_or(0);
        let message = git_commit.message().unwrap_or("(no message)").to_owned();

        // Create panproto-vcs commit.
        let commit = CommitObject::builder(schema_id, "project", &author, &message)
            .parents(parents)
            .timestamp(timestamp)
            .build();

        let commit_id = panproto_store.put(&Object::Commit(commit))?;

        git_to_panproto.insert(*git_oid, commit_id);
        oid_map.push((*git_oid, commit_id));
        last_id = commit_id;
    }

    // Determine the head panproto ID. If no new commits were imported,
    // the requested head must already live in `known`; fall back to that.
    if commits.is_empty() {
        if let Some(&id) = known.get(&head_git_oid) {
            last_id = id;
        }
    }

    // Note: this function does not set any local refs. Naming the result
    // (e.g. `refs/heads/<branch>`) is the caller's responsibility because
    // only the caller knows which branch it is importing.

    Ok(ImportResult {
        commit_count: commits.len(),
        head_id: last_id,
        oid_map,
    })
}

/// Collect ancestor commits in topological order (parents first), skipping
/// any commit reachable from an entry in `known`.
fn collect_new_ancestors<H: BuildHasher>(
    repo: &git2::Repository,
    head: git2::Oid,
    known: &HashMap<git2::Oid, ObjectId, H>,
    result: &mut Vec<git2::Oid>,
) -> Result<(), GitBridgeError> {
    let mut revwalk = repo.revwalk()?;
    revwalk.push(head)?;
    revwalk.set_sorting(git2::Sort::TOPOLOGICAL | git2::Sort::REVERSE)?;

    // Hide known commits and all their ancestors from the walk.
    for git_oid in known.keys() {
        // A known OID may not correspond to a commit reachable from `head`
        // (e.g. leftover mapping from a deleted branch). `hide` errors in
        // that case; ignore so an out-of-date map doesn't break imports.
        let _ = revwalk.hide(*git_oid);
    }

    for oid_result in revwalk {
        result.push(oid_result?);
    }

    Ok(())
}

/// Recursively walk a git tree, adding each file to the project builder.
fn walk_git_tree(
    repo: &git2::Repository,
    tree: &git2::Tree<'_>,
    prefix: &std::path::Path,
    builder: &mut ProjectBuilder,
) -> Result<(), GitBridgeError> {
    for entry in tree {
        let name = entry.name().unwrap_or("(unnamed)");
        let path = prefix.join(name);

        match entry.kind() {
            Some(git2::ObjectType::Blob) => {
                let blob = repo.find_blob(entry.id())?;
                let content = blob.content();
                builder.add_file(&path, content)?;
            }
            Some(git2::ObjectType::Tree) => {
                let subtree = repo.find_tree(entry.id())?;
                walk_git_tree(repo, &subtree, &path, builder)?;
            }
            _ => {
                // Skip submodules, symbolic links, etc.
            }
        }
    }

    Ok(())
}
