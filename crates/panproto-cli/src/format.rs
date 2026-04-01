//! Output formatting helpers for the CLI.
//!
//! Provides functions to format commits and diffs in various styles,
//! mirroring common git output modes (oneline, stat, name-only, etc.).

use panproto_core::{
    check::diff::SchemaDiff,
    schema::Schema,
    vcs::{CommitObject, hash},
};

use crate::cmd::helpers::format_timestamp;

/// Format a commit using a custom format string.
///
/// Supported placeholders:
/// - `%H`: full hash
/// - `%h`: short hash (7 chars)
/// - `%s`: subject (commit message)
/// - `%an`: author name
/// - `%ad`: author date
pub fn format_commit(commit: &CommitObject, fmt: &str) -> miette::Result<String> {
    let id =
        hash::hash_commit(commit).map_err(|e| miette::miette!("failed to hash commit: {e}"))?;

    let result = fmt
        .replace("%H", &id.to_string())
        .replace("%h", &id.short())
        .replace("%s", &commit.message)
        .replace("%an", &commit.author)
        .replace("%ad", &format_timestamp(commit.timestamp));

    Ok(result)
}

/// Format a commit as a single line: `<short_hash> <message>`.
pub fn format_commit_oneline(commit: &CommitObject) -> miette::Result<String> {
    let id =
        hash::hash_commit(commit).map_err(|e| miette::miette!("failed to hash commit: {e}"))?;
    Ok(format!("{} {}", id.short(), commit.message))
}

/// Format a diff as a stat summary (counts of added/removed/modified elements).
pub fn format_diff_stat(diff: &SchemaDiff) -> String {
    let added = diff.added_vertices.len() + diff.added_edges.len();
    let removed = diff.removed_vertices.len() + diff.removed_edges.len();
    let modified = diff.kind_changes.len() + diff.modified_constraints.len();

    let mut parts = Vec::new();
    if added > 0 {
        parts.push(format!("{added} addition(s)"));
    }
    if removed > 0 {
        parts.push(format!("{removed} deletion(s)"));
    }
    if modified > 0 {
        parts.push(format!("{modified} modification(s)"));
    }

    if parts.is_empty() {
        "0 changes".to_owned()
    } else {
        parts.join(", ")
    }
}

/// Format a diff showing only element names, one per line.
pub fn format_diff_name_only(
    diff: &SchemaDiff,
    old_schema: &Schema,
    new_schema: &Schema,
) -> String {
    let mut names = Vec::new();

    for v in &diff.added_vertices {
        names.push(v.clone());
    }
    for v in &diff.removed_vertices {
        names.push(v.clone());
    }
    for kc in &diff.kind_changes {
        names.push(kc.vertex_id.clone());
    }
    for e in &diff.added_edges {
        let label = e.name.as_deref().unwrap_or("");
        names.push(format!("{}->{} {label}", e.src, e.tgt));
    }
    for e in &diff.removed_edges {
        let label = e.name.as_deref().unwrap_or("");
        names.push(format!("{}->{} {label}", e.src, e.tgt));
    }
    for vid in diff.modified_constraints.keys() {
        names.push(format!("{vid} (constraints)"));
    }

    // Suppress unused-variable warnings; schemas are available for
    // richer formatting in future expansions.
    let _ = (old_schema, new_schema);

    names.join("\n")
}

/// Format a diff with A/D/M status markers, one entry per line.
pub fn format_diff_name_status(
    diff: &SchemaDiff,
    old_schema: &Schema,
    new_schema: &Schema,
) -> String {
    let mut lines = Vec::new();

    for v in &diff.added_vertices {
        lines.push(format!("A\t{v}"));
    }
    for v in &diff.removed_vertices {
        lines.push(format!("D\t{v}"));
    }
    for kc in &diff.kind_changes {
        lines.push(format!("M\t{}", kc.vertex_id));
    }
    for e in &diff.added_edges {
        let label = e.name.as_deref().unwrap_or("");
        lines.push(format!("A\t{}->{} {label}", e.src, e.tgt));
    }
    for e in &diff.removed_edges {
        let label = e.name.as_deref().unwrap_or("");
        lines.push(format!("D\t{}->{} {label}", e.src, e.tgt));
    }
    for vid in diff.modified_constraints.keys() {
        lines.push(format!("M\t{vid} (constraints)"));
    }

    let _ = (old_schema, new_schema);

    lines.join("\n")
}
