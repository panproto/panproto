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

/// Error loading a blob-to-schema cache.
#[derive(Debug, thiserror::Error)]
pub enum BlobCacheLoadError {
    /// The cache file exists but could not be parsed.
    #[error("blob cache at {path} is corrupt at line {line}: {reason}")]
    Corrupt {
        /// Display of the cache file path.
        path: String,
        /// 1-based line number of the first malformed entry.
        line: usize,
        /// What went wrong on that line.
        reason: String,
    },

    /// An I/O error occurred while reading the cache.
    #[error("blob cache at {path}: {source}")]
    Io {
        /// Display of the cache file path.
        path: String,
        /// The underlying I/O error.
        #[source]
        source: std::io::Error,
    },
}

/// Load a blob-to-schema cache from a plain-text file.
///
/// File format: one entry per line,
/// `<git_blob_oid> <protocol_name> <file_schema_panproto_id>`. Every
/// entry carries a non-empty protocol slot; a line with a missing or
/// empty protocol is rejected as corrupt so the caller can delete the
/// file and reimport rather than round-trip through an empty slot.
///
/// # Errors
///
/// Returns [`BlobCacheLoadError::Io`] for I/O problems other than a
/// missing file (missing yields an empty cache), and
/// [`BlobCacheLoadError::Corrupt`] if any line cannot be parsed.
pub fn load_blob_cache(path: &Path) -> Result<BlobSchemaCache, BlobCacheLoadError> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(BlobSchemaCache::default());
        }
        Err(source) => {
            return Err(BlobCacheLoadError::Io {
                path: path.display().to_string(),
                source,
            });
        }
    };
    let mut map = BlobSchemaCache::default();
    for (idx, line) in content.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let mut parts = line.split_whitespace();
        let Some(blob_hex) = parts.next() else {
            continue;
        };
        let Some(protocol) = parts.next() else {
            return Err(BlobCacheLoadError::Corrupt {
                path: path.display().to_string(),
                line: idx + 1,
                reason: "missing protocol slot; delete the cache file and reimport".to_owned(),
            });
        };
        let Some(panproto_hex) = parts.next() else {
            return Err(BlobCacheLoadError::Corrupt {
                path: path.display().to_string(),
                line: idx + 1,
                reason: "missing panproto id".to_owned(),
            });
        };
        let blob_oid = git2::Oid::from_str(blob_hex).map_err(|e| BlobCacheLoadError::Corrupt {
            path: path.display().to_string(),
            line: idx + 1,
            reason: format!("bad git oid: {e}"),
        })?;
        let panproto_id =
            panproto_hex
                .parse::<ObjectId>()
                .map_err(|e| BlobCacheLoadError::Corrupt {
                    path: path.display().to_string(),
                    line: idx + 1,
                    reason: format!("bad panproto id: {e}"),
                })?;
        map.insert((blob_oid, protocol.to_owned()), panproto_id);
    }
    Ok(map)
}

/// Persist a blob-to-schema cache atomically.
///
/// Writes to `<path>.tmp` and renames into place, so a crash mid-write
/// cannot leave a partial file that would later parse as corrupt.
///
/// # Errors
///
/// Returns any I/O error encountered while creating parent
/// directories, writing, or renaming.
pub fn save_blob_cache(path: &Path, cache: &BlobSchemaCache) -> std::io::Result<()> {
    use std::io::Write;
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "blob cache path has no parent directory",
        )
    })?;
    std::fs::create_dir_all(parent)?;
    let mut lines: Vec<String> = Vec::with_capacity(cache.len());
    for ((blob, protocol), id) in cache {
        if protocol.is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!(
                    "blob cache entry for {blob} has empty protocol; every entry must carry a protocol name"
                ),
            ));
        }
        lines.push(format!("{blob} {protocol} {id}"));
    }
    lines.sort();
    let body = lines.join("\n") + "\n";
    let tmp = path.with_extension("tmp");
    // Create + write + fsync the temp file so its bytes are on disk
    // before the rename. Then fsync the parent directory so the
    // rename itself is durable; without this, a crash can leave the
    // rename unrecorded even though the payload is on disk.
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(body.as_bytes())?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)?;
    let dir = std::fs::File::open(parent)?;
    dir.sync_all()?;
    Ok(())
}

/// Cache mapping a `(git blob OID, protocol)` pair to the
/// content-addressed [`ObjectId`] of the [`FileSchemaObject`]
/// produced by parsing it.
///
/// Keying on the protocol avoids a cross-protocol collision: the
/// same bytes appearing as `a.py` and `a.txt` parse to different
/// per-file schemas and therefore must hash to different
/// [`ObjectId`]s, which means they must occupy different cache
/// slots.
///
/// A [`BlobSchemaCache`] is the key to making incremental tree-based
/// imports cheap: when a new git commit only changes one file, every
/// other `(blob, protocol)` pair is already in the cache, so the
/// importer reuses the existing [`ObjectId`] and only has to rewrite
/// the tree-node objects on the path from the changed file to the
/// project root.
pub type BlobSchemaCache = FxHashMap<(git2::Oid, String), ObjectId>;

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

/// Import a git repository, persisting the blob-to-schema cache
/// under `cache_dir/<BLOB_CACHE_FILE>` so subsequent imports
/// deduplicate unchanged files without re-parsing them.
///
/// This is the production entry point: `cache_dir` is usually the
/// per-remote panproto cache directory
/// (`$GIT_DIR/panproto-cache/<remote>/`). Pass an empty `known` map
/// for a full import, or the existing git-to-panproto marks for an
/// incremental one.
///
/// # Errors
///
/// Returns [`GitBridgeError`] if git operations, parsing, or VCS
/// operations fail. The cache file is loaded best-effort; a corrupt
/// cache propagates as [`GitBridgeError::BlobCache`] so the caller
/// can choose to delete-and-restart rather than silently re-import.
pub fn import_git_repo_persistent<S: Store, H: BuildHasher>(
    git_repo: &git2::Repository,
    panproto_store: &mut S,
    revspec: &str,
    known: &HashMap<git2::Oid, ObjectId, H>,
    cache_dir: &Path,
) -> Result<ImportResult, GitBridgeError> {
    let cache_path = cache_dir.join(BLOB_CACHE_FILE);
    let mut cache =
        load_blob_cache(&cache_path).map_err(|e| GitBridgeError::BlobCache(e.to_string()))?;
    let result = import_git_repo_with_cache(git_repo, panproto_store, revspec, known, &mut cache)?;
    save_blob_cache(&cache_path, &cache).map_err(|e| GitBridgeError::BlobCache(e.to_string()))?;
    Ok(result)
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
    // Delegate to the cache-aware path with an in-memory cache so
    // every call still gets within-import dedup: two commits that
    // reference the same git blob share a single FileSchemaObject.
    // Production callers that want cross-call dedup should go through
    // [`import_git_repo_persistent`] with a real cache directory.
    let mut cache = BlobSchemaCache::default();
    import_git_repo_with_cache(git_repo, panproto_store, revspec, known, &mut cache)
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
        let name = entry.name().map_err(|_| GitBridgeError::NonUtf8TreeEntry {
            parent: prefix.display().to_string(),
        })?;
        let path = prefix.join(name);

        match entry.kind() {
            Some(git2::ObjectType::Blob) => {
                let blob_oid = entry.id();
                // Probe the cache before parsing; we need the
                // protocol to key the cache, so detect it first.
                let protocol_guess = panproto_project::detect::detect_language(
                    &path,
                    &panproto_parse::ParserRegistry::new(),
                )
                .map_or_else(String::new, ToOwned::to_owned);
                let leaf_id =
                    if let Some(&cached) = blob_cache.get(&(blob_oid, protocol_guess.clone())) {
                        cached
                    } else {
                        let blob = repo.find_blob(blob_oid)?;
                        let content = blob.content();
                        let (schema, protocol) = parse_single_blob(&path, content)?;
                        let file = FileSchemaObject {
                            path: path.display().to_string(),
                            protocol: protocol.clone(),
                            schema,
                            cross_file_edges: Vec::new(),
                        };
                        let id = store.put(&Object::FileSchema(Box::new(file)))?;
                        // Record under the protocol actually used so a
                        // second cache probe for the same (blob, proto)
                        // pair hits even when detection and the parser
                        // disagree (e.g., raw_file fallback).
                        blob_cache.insert((blob_oid, protocol.clone()), id);
                        if protocol != protocol_guess {
                            blob_cache.insert((blob_oid, protocol_guess), id);
                        }
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
