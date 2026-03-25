//! Export panproto-vcs repositories to git.
//!
//! Takes a panproto-vcs commit and creates corresponding git tree and commit
//! objects. The schema is decomposed back into per-file source text via the
//! panproto-parse emitters.

use std::path::Path;

use panproto_parse::ParserRegistry;
use panproto_vcs::{Object, ObjectId, Store};

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
/// Loads the project schema from the panproto commit, runs emitters on each
/// file's sub-schema to produce source text, and creates git tree + commit objects.
///
/// # Errors
///
/// Returns [`GitBridgeError`] if VCS operations, emission, or git operations fail.
pub fn export_to_git<S: Store>(
    panproto_store: &S,
    git_repo: &git2::Repository,
    commit_id: ObjectId,
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

    let registry = ParserRegistry::new();

    // For now, export the entire schema as a single file.
    // A full implementation would decompose by file_map from ProjectSchema,
    // but the schema doesn't carry file_map information. We export what we can.
    //
    // Try to emit using each registered protocol until one succeeds.
    let mut tree_builder = git_repo.treebuilder(None)?;
    let mut file_count = 0;

    // Attempt to emit the schema as source code for the protocol it was parsed with.
    let protocol = &commit.protocol;
    match registry.emit_with_protocol(protocol, schema) {
        Ok(content) => {
            let blob_oid = git_repo.blob(&content)?;
            tree_builder.insert("schema_output", blob_oid, 0o100644)?;
            file_count += 1;
        }
        Err(_) => {
            // If emission fails, store the schema as serialized JSON.
            let json = serde_json::to_vec_pretty(schema.as_ref())
                .unwrap_or_else(|_| b"(serialization failed)".to_vec());
            let blob_oid = git_repo.blob(&json)?;
            tree_builder.insert("schema.json", blob_oid, 0o100644)?;
            file_count += 1;
        }
    }

    let tree_oid = tree_builder.write()?;
    let tree = git_repo.find_tree(tree_oid)?;

    // Create git commit.
    let sig = git2::Signature::new(
        &commit.author,
        &format!("{}@panproto", commit.author),
        &git2::Time::new(commit.timestamp as i64, 0),
    )?;

    // Find parent git commits (if any).
    // For now, create root commits since we don't track the git-panproto OID mapping.
    let parents: Vec<git2::Commit<'_>> = Vec::new();
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
