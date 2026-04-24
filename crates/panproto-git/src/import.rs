//! Import git repositories into panproto-vcs.
//!
//! Walks the git commit DAG topologically, parses each commit's file tree
//! into a panproto project schema, and creates panproto-vcs commits that
//! preserve authorship, timestamps, and parent structure.

use std::collections::HashMap;
use std::hash::BuildHasher;
use std::path::{Path, PathBuf};

use panproto_project::ProjectBuilder;
use panproto_vcs::{CommitObject, FileSchemaObject, Object, ObjectId, Store};
use rustc_hash::FxHashMap;

use crate::error::GitBridgeError;

/// Standard on-disk name of the blob-OID to `FileSchema`
/// [`ObjectId`] cache.
pub const BLOB_CACHE_FILE: &str = "blob_to_schema";

/// Load a blob-to-schema cache from a plain-text file.
///
/// File format: one entry per line, `<git_blob_oid> <file_schema_panproto_id>`.
/// Missing or malformed files yield an empty map so the next import
/// acts as a cold start.
#[must_use]
pub fn load_blob_cache(path: &Path) -> BlobSchemaCache {
    let mut map = BlobSchemaCache::default();
    let Ok(content) = std::fs::read_to_string(path) else {
        return map;
    };
    for line in content.lines() {
        let mut parts = line.split_whitespace();
        let Some(blob_hex) = parts.next() else {
            continue;
        };
        let Some(panproto_hex) = parts.next() else {
            continue;
        };
        let Ok(blob_oid) = git2::Oid::from_str(blob_hex) else {
            continue;
        };
        let Ok(panproto_id) = panproto_hex.parse::<ObjectId>() else {
            continue;
        };
        map.insert(blob_oid, panproto_id);
    }
    map
}

/// Persist a blob-to-schema cache by rewriting `path` in full.
///
/// # Errors
///
/// Returns any I/O error encountered while creating parent
/// directories or writing the file.
pub fn save_blob_cache(path: &Path, cache: &BlobSchemaCache) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut lines: Vec<String> = cache
        .iter()
        .map(|(blob, id)| format!("{blob} {id}"))
        .collect();
    lines.sort();
    std::fs::write(path, lines.join("\n") + "\n")
}

/// Cache mapping a git blob OID to the content-addressed
/// [`ObjectId`] of the [`FileSchemaObject`] produced by parsing it.
///
/// A [`BlobSchemaCache`] is the key to making incremental tree-based
/// imports cheap: when a new git commit only changes one file, every
/// other file's blob OID is already in the cache, so the importer
/// reuses the existing `FileSchema` [`ObjectId`] and only has to
/// rewrite the tree-node objects on the path from the changed file
/// to the project root.
pub type BlobSchemaCache = FxHashMap<git2::Oid, ObjectId>;

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

        // Store the schema as a single-leaf tree. This path is only
        // used by the non-deduped importer variant; production imports
        // go through `import_git_repo_with_cache` which emits a proper
        // multi-leaf Merkle tree with blob-OID dedup.
        let schema_id = panproto_vcs::tree::store_schema_as_tree(panproto_store, project)?;

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

/// Import a git repository using per-file content addressing.
///
/// Like [`import_git_repo_incremental`], but stores each commit's
/// project schema as a Merkle tree of [`FileSchemaObject`] leaves
/// keyed by git blob OID. Unchanged files reuse their existing
/// [`FileSchemaObject`] [`ObjectId`] across commits; only the
/// [`panproto_vcs::SchemaTreeObject`] nodes on the path from the
/// changed file to the project root are rewritten.
///
/// The `blob_cache` is read and updated in place; callers should
/// persist it across imports (e.g., under
/// `$GIT_DIR/panproto-cache/<remote>/blob_to_schema`) so repeated
/// imports only parse blobs that are genuinely new.
///
/// # Errors
///
/// Returns [`GitBridgeError`] if git operations, parsing, or VCS
/// operations fail.
pub fn import_git_repo_with_cache<S, H>(
    git_repo: &git2::Repository,
    panproto_store: &mut S,
    revspec: &str,
    known: &HashMap<git2::Oid, ObjectId, H>,
    blob_cache: &mut BlobSchemaCache,
) -> Result<ImportResult, GitBridgeError>
where
    S: Store,
    H: BuildHasher,
{
    let obj = git_repo.revparse_single(revspec)?;
    let head_commit = obj
        .peel_to_commit()
        .map_err(|e| GitBridgeError::ObjectRead {
            oid: obj.id().to_string(),
            reason: format!("not a commit: {e}"),
        })?;
    let head_git_oid = head_commit.id();

    let mut commits = Vec::new();
    collect_new_ancestors(git_repo, head_git_oid, known, &mut commits)?;

    let mut git_to_panproto: FxHashMap<git2::Oid, ObjectId> =
        known.iter().map(|(&k, &v)| (k, v)).collect();
    let mut oid_map: Vec<(git2::Oid, ObjectId)> = Vec::new();
    let mut last_id = ObjectId::ZERO;

    for git_oid in &commits {
        let git_commit = git_repo.find_commit(*git_oid)?;
        let tree = git_commit.tree()?;

        // Collect (path, FileSchema ObjectId) for every blob under the
        // git tree, reusing cached IDs where possible.
        let mut leaves: Vec<(PathBuf, ObjectId)> = Vec::new();
        collect_tree_leaves(
            git_repo,
            &tree,
            Path::new(""),
            panproto_store,
            blob_cache,
            &mut leaves,
        )?;

        // Empty trees (initial commit with no files) get a synthetic
        // single-file leaf so the commit still points at a schema
        // tree rather than a flat schema.
        let root_id = if leaves.is_empty() {
            let proto = panproto_protocols::raw_file::protocol();
            let schema = panproto_schema::SchemaBuilder::new(&proto)
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
                })?;
            let file = FileSchemaObject {
                path: "__empty__".to_owned(),
                protocol: "raw_file".to_owned(),
                schema,
                cross_file_edges: Vec::new(),
            };
            let leaf_id = panproto_store.put(&Object::FileSchema(Box::new(file)))?;
            panproto_vcs::build_tree_from_leaves(
                panproto_store,
                vec![(PathBuf::from("__empty__"), leaf_id)],
            )
            .map_err(GitBridgeError::Vcs)?
        } else {
            panproto_vcs::build_tree_from_leaves(panproto_store, leaves)
                .map_err(GitBridgeError::Vcs)?
        };

        let parents: Vec<ObjectId> = git_commit
            .parent_ids()
            .filter_map(|parent_oid| git_to_panproto.get(&parent_oid).copied())
            .collect();

        let author_sig = git_commit.author();
        let author = author_sig.name().unwrap_or("unknown").to_owned();
        let timestamp = u64::try_from(author_sig.when().seconds()).unwrap_or(0);
        let message = git_commit.message().unwrap_or("(no message)").to_owned();

        let commit = CommitObject::builder(root_id, "project", &author, &message)
            .parents(parents)
            .timestamp(timestamp)
            .build();

        let commit_id = panproto_store.put(&Object::Commit(commit))?;

        git_to_panproto.insert(*git_oid, commit_id);
        oid_map.push((*git_oid, commit_id));
        last_id = commit_id;
    }

    if commits.is_empty() {
        if let Some(&id) = known.get(&head_git_oid) {
            last_id = id;
        }
    }

    Ok(ImportResult {
        commit_count: commits.len(),
        head_id: last_id,
        oid_map,
    })
}

/// Walk a git tree, recording a `(path, FileSchema ObjectId)` leaf
/// for every blob. Parses and stores blobs whose OIDs are not in
/// `blob_cache`, and updates the cache with the resulting IDs.
fn collect_tree_leaves<S: Store>(
    repo: &git2::Repository,
    tree: &git2::Tree<'_>,
    prefix: &Path,
    store: &mut S,
    blob_cache: &mut BlobSchemaCache,
    leaves: &mut Vec<(PathBuf, ObjectId)>,
) -> Result<(), GitBridgeError> {
    for entry in tree {
        let name = entry
            .name()
            .ok_or_else(|| GitBridgeError::NonUtf8TreeEntry {
                parent: prefix.display().to_string(),
            })?;
        let path = prefix.join(name);

        match entry.kind() {
            Some(git2::ObjectType::Blob) => {
                let blob_oid = entry.id();
                let leaf_id = if let Some(&cached) = blob_cache.get(&blob_oid) {
                    cached
                } else {
                    let blob = repo.find_blob(blob_oid)?;
                    let content = blob.content();
                    let (schema, protocol) = parse_single_blob(&path, content)?;
                    let file = FileSchemaObject {
                        path: path.display().to_string(),
                        protocol,
                        schema,
                        cross_file_edges: Vec::new(),
                    };
                    let id = store.put(&Object::FileSchema(Box::new(file)))?;
                    blob_cache.insert(blob_oid, id);
                    id
                };
                leaves.push((path, leaf_id));
            }
            Some(git2::ObjectType::Tree) => {
                let subtree = repo.find_tree(entry.id())?;
                collect_tree_leaves(repo, &subtree, &path, store, blob_cache, leaves)?;
            }
            _ => {}
        }
    }
    Ok(())
}

/// Parse a single git blob into a per-file schema plus the protocol
/// name used to parse it.
///
/// Goes through [`ProjectBuilder`] so the file-to-schema pipeline
/// matches what the full-repo path does.
fn parse_single_blob(
    path: &Path,
    content: &[u8],
) -> Result<(panproto_schema::Schema, String), GitBridgeError> {
    let mut builder = ProjectBuilder::new();
    builder.add_file(path, content)?;
    let schemas = builder.file_schemas().clone();
    let protocols = builder.protocol_map_ref().clone();
    let schema = schemas.into_iter().next().map(|(_, s)| s).ok_or_else(|| {
        GitBridgeError::Project(panproto_project::ProjectError::CoproductFailed {
            reason: "single-blob parse produced no schema".to_owned(),
        })
    })?;
    let protocol = protocols
        .into_iter()
        .next()
        .map_or_else(|| "raw_file".to_owned(), |(_, p)| p);
    Ok((schema, protocol))
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
        let name = entry
            .name()
            .ok_or_else(|| GitBridgeError::NonUtf8TreeEntry {
                parent: prefix.display().to_string(),
            })?;
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
