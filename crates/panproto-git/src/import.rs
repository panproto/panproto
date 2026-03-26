//! Import git repositories into panproto-vcs.
//!
//! Walks the git commit DAG topologically, parses each commit's file tree
//! into a panproto project schema, and creates panproto-vcs commits that
//! preserve authorship, timestamps, and parent structure.

use std::path::PathBuf;

use panproto_project::ProjectBuilder;
use panproto_vcs::{CommitObject, Object, ObjectId, Store};

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
/// # Errors
///
/// Returns [`GitBridgeError`] if git operations, parsing, or VCS operations fail.
pub fn import_git_repo<S: Store>(
    git_repo: &git2::Repository,
    panproto_store: &mut S,
    revspec: &str,
) -> Result<ImportResult, GitBridgeError> {
    // Resolve the revspec to a commit.
    let obj = git_repo.revparse_single(revspec)?;
    let head_commit = obj
        .peel_to_commit()
        .map_err(|e| GitBridgeError::ObjectRead {
            oid: obj.id().to_string(),
            reason: format!("not a commit: {e}"),
        })?;

    // Collect commits in topological order (parents before children).
    let mut commits = Vec::new();
    collect_ancestors(git_repo, head_commit.id(), &mut commits)?;

    // Import each commit.
    let mut oid_map: Vec<(git2::Oid, ObjectId)> = Vec::new();
    let mut git_to_panproto: rustc_hash::FxHashMap<git2::Oid, ObjectId> =
        rustc_hash::FxHashMap::default();
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
        let commit = CommitObject {
            schema_id,
            parents,
            migration_id: None,
            protocol: "project".to_owned(),
            author,
            timestamp,
            message,
            renames: Vec::new(),
            protocol_id: None,
            data_ids: Vec::new(),
            complement_ids: Vec::new(),
            edit_log_ids: Vec::new(),
        };

        let commit_id = panproto_store.put(&Object::Commit(commit))?;

        git_to_panproto.insert(*git_oid, commit_id);
        oid_map.push((*git_oid, commit_id));
        last_id = commit_id;
    }

    // Set HEAD to the last imported commit.
    if !commits.is_empty() {
        panproto_store.set_ref("refs/heads/main", last_id)?;
    }

    Ok(ImportResult {
        commit_count: commits.len(),
        head_id: last_id,
        oid_map,
    })
}

/// Collect all ancestor commits in topological order (parents first).
fn collect_ancestors(
    repo: &git2::Repository,
    head: git2::Oid,
    result: &mut Vec<git2::Oid>,
) -> Result<(), GitBridgeError> {
    let mut revwalk = repo.revwalk()?;
    revwalk.push(head)?;
    revwalk.set_sorting(git2::Sort::TOPOLOGICAL | git2::Sort::REVERSE)?;

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
