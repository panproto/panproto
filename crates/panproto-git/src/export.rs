//! Export panproto-vcs repositories to git.
//!
//! Takes a panproto-vcs commit and creates corresponding git tree and commit
//! objects. The schema is serialized as JSON (the authoritative structural
//! representation) alongside any cached source text from the import.

use panproto_vcs::{Object, ObjectId, Store};
use rustc_hash::FxHashMap;

use crate::error::GitBridgeError;

/// Result of exporting a panproto-vcs commit to git.
#[derive(Debug)]
pub struct ExportResult {
    /// The git commit OID that was created.
    pub git_oid: git2::Oid,
    /// Number of files exported.
    pub file_count: usize,
}

/// Export a panproto-vcs commit as a git commit.
///
/// Loads the schema from the panproto commit and serializes it into the git tree.
/// If a `parent_map` is provided (mapping panproto parent commit IDs to git OIDs),
/// the exported git commit will have the correct parent pointers, preserving the
/// DAG structure.
///
/// The schema is stored as a JSON file in the git tree. This is the authoritative
/// representation; source text reconstruction requires re-parsing with the
/// appropriate language parser.
///
/// # Errors
///
/// Returns [`GitBridgeError`] if VCS operations or git operations fail.
pub fn export_to_git<S: Store>(
    panproto_store: &S,
    git_repo: &git2::Repository,
    commit_id: ObjectId,
    parent_map: &FxHashMap<ObjectId, git2::Oid>,
) -> Result<ExportResult, GitBridgeError> {
    // Load the commit.
    let commit_obj = panproto_store.get(&commit_id)?;
    let commit = match &commit_obj {
        Object::Commit(c) => c,
        other => {
            return Err(GitBridgeError::ObjectRead {
                oid: commit_id.to_string(),
                reason: format!("expected commit, got {}", other.type_name()),
            });
        }
    };

    // Load the schema.
    let schema_obj = panproto_store.get(&commit.schema_id)?;
    let schema = match &schema_obj {
        Object::Schema(s) => s,
        other => {
            return Err(GitBridgeError::ObjectRead {
                oid: commit.schema_id.to_string(),
                reason: format!("expected schema, got {}", other.type_name()),
            });
        }
    };

    // Build the git tree.
    // The schema is serialized as JSON, which is the authoritative structural
    // representation of the project. Each vertex, edge, and constraint is preserved.
    let mut tree_builder = git_repo.treebuilder(None)?;
    let mut file_count = 0;

    // Serialize the schema as pretty-printed JSON.
    let schema_json = serde_json::to_vec_pretty(schema.as_ref()).map_err(|e| {
        GitBridgeError::ObjectRead {
            oid: commit.schema_id.to_string(),
            reason: format!("JSON serialization failed: {e}"),
        }
    })?;
    let blob_oid = git_repo.blob(&schema_json)?;
    tree_builder.insert("schema.json", blob_oid, 0o100644)?;
    file_count += 1;

    // Also store commit metadata.
    let commit_json = serde_json::to_vec_pretty(commit).map_err(|e| {
        GitBridgeError::ObjectRead {
            oid: commit_id.to_string(),
            reason: format!("commit JSON serialization failed: {e}"),
        }
    })?;
    let commit_blob = git_repo.blob(&commit_json)?;
    tree_builder.insert("commit.json", commit_blob, 0o100644)?;
    file_count += 1;

    // Extract leaf node literal values to reconstruct source per file.
    // Group vertices by their file prefix (before the first "::").
    let mut files_content: FxHashMap<String, Vec<(usize, String)>> = FxHashMap::default();
    let mut file_blobs: FxHashMap<String, git2::Oid> = FxHashMap::default();
    for (name, _vertex) in &schema.vertices {
        if let Some(constraints) = schema.constraints.get(name) {
            let start_byte = constraints
                .iter()
                .find(|c| c.sort.as_ref() == "start-byte")
                .and_then(|c| c.value.parse::<usize>().ok());
            let literal = constraints
                .iter()
                .find(|c| c.sort.as_ref() == "literal-value")
                .map(|c| c.value.clone());

            if let (Some(start), Some(text)) = (start_byte, literal) {
                // Extract file prefix from vertex ID (e.g., "src/main.ts::greet" → "src/main.ts").
                let name_str = name.as_ref();
                let file_prefix = name_str
                    .find("::")
                    .map(|pos| &name_str[..pos])
                    .unwrap_or(name_str);
                files_content
                    .entry(file_prefix.to_owned())
                    .or_default()
                    .push((start, text));
            }
        }
    }

    // Write reconstructed source files.
    for (file_path, mut leaves) in files_content {
        leaves.sort_by_key(|(s, _)| *s);
        let mut content = Vec::new();
        let mut cursor = 0;
        for (start, text) in &leaves {
            if *start > cursor {
                let gap = start - cursor;
                if gap <= 2 {
                    for _ in 0..gap {
                        content.push(b' ');
                    }
                } else {
                    content.push(b'\n');
                    for _ in 0..gap.saturating_sub(1) {
                        content.push(b' ');
                    }
                }
            }
            content.extend_from_slice(text.as_bytes());
            cursor = start + text.len();
        }

        if !content.is_empty() {
            let blob_oid = git_repo.blob(&content)?;
            // Track file path → blob OID for nested tree construction.
            file_blobs.insert(file_path, blob_oid);
            file_count += 1;
        }
    }

    // Build nested git tree structure from file paths.
    // Group files by their directory prefix and create subtrees.
    build_nested_tree(git_repo, &mut tree_builder, &file_blobs)?;

    let tree_oid = tree_builder.write()?;
    let tree = git_repo.find_tree(tree_oid)?;

    // Create git commit signature.
    let sig = git2::Signature::new(
        &commit.author,
        &format!("{}@panproto", commit.author),
        &git2::Time::new(commit.timestamp as i64, 0),
    )?;

    // Resolve parent git commits from the mapping.
    let mut parents: Vec<git2::Commit<'_>> = Vec::new();
    for parent_panproto_id in &commit.parents {
        if let Some(parent_git_oid) = parent_map.get(parent_panproto_id) {
            if let Ok(parent_commit) = git_repo.find_commit(*parent_git_oid) {
                parents.push(parent_commit);
            }
        }
    }
    let parent_refs: Vec<&git2::Commit<'_>> = parents.iter().collect();

    let git_oid = git_repo.commit(
        Some("HEAD"),
        &sig,
        &sig,
        &commit.message,
        &tree,
        &parent_refs,
    )?;

    Ok(ExportResult {
        git_oid,
        file_count,
    })
}

/// Build a nested git tree structure from a flat map of file paths to blob OIDs.
///
/// For paths like `"src/main.ts"`, this creates a subtree `"src"` containing
/// the blob `"main.ts"`. Deeply nested paths create multiple levels of subtrees.
fn build_nested_tree(
    repo: &git2::Repository,
    root_builder: &mut git2::TreeBuilder<'_>,
    file_blobs: &FxHashMap<String, git2::Oid>,
) -> Result<(), GitBridgeError> {
    // Group files by top-level directory.
    let mut dirs: FxHashMap<String, Vec<(String, git2::Oid)>> = FxHashMap::default();
    let mut root_files: Vec<(String, git2::Oid)> = Vec::new();

    for (path, oid) in file_blobs {
        if let Some(slash_pos) = path.find('/') {
            let dir = &path[..slash_pos];
            let rest = &path[slash_pos + 1..];
            dirs.entry(dir.to_owned())
                .or_default()
                .push((rest.to_owned(), *oid));
        } else {
            root_files.push((path.clone(), *oid));
        }
    }

    // Insert root-level files directly.
    for (name, oid) in &root_files {
        root_builder.insert(name, *oid, 0o100644)?;
    }

    // Recursively build subtrees for directories.
    for (dir_name, entries) in &dirs {
        let subtree_oid = build_subtree(repo, entries)?;
        root_builder.insert(dir_name, subtree_oid, 0o040000)?;
    }

    Ok(())
}

/// Recursively build a git subtree from a list of (relative_path, blob_oid) entries.
fn build_subtree(
    repo: &git2::Repository,
    entries: &[(String, git2::Oid)],
) -> Result<git2::Oid, GitBridgeError> {
    let mut builder = repo.treebuilder(None)?;

    // Separate files from subdirectories.
    let mut subdirs: FxHashMap<String, Vec<(String, git2::Oid)>> = FxHashMap::default();
    let mut files: Vec<(String, git2::Oid)> = Vec::new();

    for (path, oid) in entries {
        if let Some(slash_pos) = path.find('/') {
            let dir = &path[..slash_pos];
            let rest = &path[slash_pos + 1..];
            subdirs
                .entry(dir.to_owned())
                .or_default()
                .push((rest.to_owned(), *oid));
        } else {
            files.push((path.clone(), *oid));
        }
    }

    for (name, oid) in &files {
        builder.insert(name, *oid, 0o100644)?;
    }

    for (dir_name, sub_entries) in &subdirs {
        let subtree_oid = build_subtree(repo, sub_entries)?;
        builder.insert(dir_name, subtree_oid, 0o040000)?;
    }

    Ok(builder.write()?)
}

/// Detect the appropriate file extension for a protocol name.
#[must_use]
pub fn extension_for_protocol(protocol: &str) -> &'static str {
    match protocol {
        "typescript" => "ts",
        "tsx" => "tsx",
        "python" => "py",
        "rust" => "rs",
        "java" => "java",
        "go" => "go",
        "swift" => "swift",
        "kotlin" => "kt",
        "csharp" => "cs",
        "c" => "c",
        "cpp" => "cpp",
        "raw_file" => "txt",
        _ => "txt",
    }
}
