//! Scope-level diff reporting.
//!
//! Groups flat vertex/edge changes from a [`SchemaDiff`] by their
//! nearest named program element (function, class, type) using the
//! scope hierarchy encoded in vertex IDs (`file::Class::method::$0`).
//!
//! This is the developer-facing view: "which functions changed?" rather
//! than "which anonymous AST nodes were added?"

use std::collections::HashMap;

use panproto_schema::Schema;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

use crate::diff::SchemaDiff;

/// A scope-level diff report grouping changes by named program element.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ScopeReport {
    /// Changes grouped by their nearest named scope.
    pub scopes: Vec<ScopeChange>,
    /// All named elements with their status (for summary views).
    pub named_elements: Vec<NamedElement>,
}

/// A change at the scope level (function, class, type, etc.).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScopeChange {
    /// Full vertex ID of the scope vertex.
    pub scope_id: String,
    /// Human-readable name (last named segment of the ID).
    pub scope_name: String,
    /// What kind of change occurred at the scope level.
    pub kind: ScopeChangeKind,
    /// Natural-language summary of the change.
    pub summary: String,
    /// Number of anonymous child vertices added within this scope.
    pub anonymous_added: usize,
    /// Number of anonymous child vertices removed within this scope.
    pub anonymous_removed: usize,
    /// Start line (from start-byte constraint), if available.
    pub start_line: Option<usize>,
    /// End line (from end-byte constraint), if available.
    pub end_line: Option<usize>,
}

/// Classification of a scope-level change.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScopeChangeKind {
    /// The entire scope was added.
    Added,
    /// The entire scope was removed.
    Removed,
    /// Named edges to/from the scope changed (API surface change).
    SignatureChanged,
    /// Only anonymous children changed (implementation change).
    BodyModified,
}

/// A named element with its diff status.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NamedElement {
    /// Full vertex ID.
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Vertex kind from the schema.
    pub kind: String,
    /// Diff status of this element.
    pub status: ElementStatus,
    /// Start line, if available.
    pub start_line: Option<usize>,
}

/// Status of a named element in the diff.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ElementStatus {
    /// No changes.
    Unchanged,
    /// Only anonymous children changed.
    BodyModified,
    /// Named edges changed.
    SignatureChanged,
    /// Element was added.
    Added,
    /// Element was removed.
    Removed,
}

/// Returns `true` if a vertex ID segment is anonymous (positional).
fn is_anonymous_segment(segment: &str) -> bool {
    segment.starts_with('$')
}

/// Extract the nearest named scope from a vertex ID.
///
/// Walks the `::` segments from right to left, skipping anonymous `$N`
/// segments, and returns the prefix up to and including the first named
/// segment found.
fn nearest_named_scope(vertex_id: &str) -> &str {
    let segments: Vec<&str> = vertex_id.split("::").collect();
    // Walk backwards to find the last named segment.
    for i in (0..segments.len()).rev() {
        if !is_anonymous_segment(segments[i]) {
            // Return the prefix up to and including this segment.
            // Total length = sum of segment lengths + number of "::" separators * 2.
            let char_len: usize = segments[..=i].iter().map(|s| s.len()).sum::<usize>() + i * 2;
            return &vertex_id[..char_len];
        }
    }
    // All segments are anonymous; return the whole ID.
    vertex_id
}

/// Extract the human-readable name from a scope ID (last named segment).
fn scope_display_name(scope_id: &str) -> &str {
    scope_id
        .rsplit("::")
        .find(|s| !is_anonymous_segment(s))
        .unwrap_or(scope_id)
}

/// Try to extract a line number from a `start-byte` constraint.
///
/// This requires the source bytes to convert byte offset to line number.
fn byte_offset_to_line(offset: usize, source: &[u8]) -> usize {
    memchr::memchr_iter(b'\n', &source[..offset.min(source.len())]).count() + 1
}

/// Extract the start-byte constraint value for a vertex, if present.
fn start_byte_for_vertex(schema: &Schema, vertex_id: &str) -> Option<usize> {
    let constraints = schema.constraints.get(vertex_id)?;
    constraints
        .iter()
        .find(|c| c.sort.as_ref() == "start-byte")
        .and_then(|c| c.value.parse().ok())
}

/// Extract the end-byte constraint value for a vertex, if present.
fn end_byte_for_vertex(schema: &Schema, vertex_id: &str) -> Option<usize> {
    let constraints = schema.constraints.get(vertex_id)?;
    constraints
        .iter()
        .find(|c| c.sort.as_ref() == "end-byte")
        .and_then(|c| c.value.parse().ok())
}

/// Count named (non-anonymous) edge changes per scope.
fn count_named_edge_changes(edges: &[panproto_schema::Edge]) -> FxHashMap<&str, usize> {
    let mut m: FxHashMap<&str, usize> = FxHashMap::default();
    for e in edges {
        if !is_anonymous_segment(e.src.rsplit("::").next().unwrap_or(&e.src)) {
            *m.entry(nearest_named_scope(&e.src)).or_default() += 1;
        }
    }
    m
}

/// Count anonymous vertices in a list.
fn count_anonymous(vertices: &[&str]) -> usize {
    vertices
        .iter()
        .filter(|v| {
            let last = v.rsplit("::").next().unwrap_or(v);
            is_anonymous_segment(last)
        })
        .count()
}

/// Build a scope-level diff report from a flat [`SchemaDiff`].
///
/// Groups vertex additions and removals by their nearest named scope,
/// classifies each scope as added/removed/signature-changed/body-modified,
/// and optionally resolves line numbers from source bytes.
#[must_use]
pub fn report_by_scope(
    diff: &SchemaDiff,
    old_schema: &Schema,
    new_schema: &Schema,
    old_bytes: Option<&[u8]>,
    new_bytes: Option<&[u8]>,
) -> ScopeReport {
    // Collect sets for quick lookup.
    let added_set: rustc_hash::FxHashSet<&str> =
        diff.added_vertices.iter().map(String::as_str).collect();
    let removed_set: rustc_hash::FxHashSet<&str> =
        diff.removed_vertices.iter().map(String::as_str).collect();

    // Group added/removed vertices by nearest named scope.
    let mut scope_added: FxHashMap<&str, Vec<&str>> = FxHashMap::default();
    let mut scope_removed: FxHashMap<&str, Vec<&str>> = FxHashMap::default();

    for v in &diff.added_vertices {
        let scope = nearest_named_scope(v);
        scope_added.entry(scope).or_default().push(v);
    }
    for v in &diff.removed_vertices {
        let scope = nearest_named_scope(v);
        scope_removed.entry(scope).or_default().push(v);
    }

    // Collect all scopes that have changes.
    let mut all_scopes: rustc_hash::FxHashSet<&str> = rustc_hash::FxHashSet::default();
    all_scopes.extend(scope_added.keys());
    all_scopes.extend(scope_removed.keys());

    // Also include scopes with edge changes.
    for edge in &diff.added_edges {
        all_scopes.insert(nearest_named_scope(&edge.src));
        all_scopes.insert(nearest_named_scope(&edge.tgt));
    }
    for edge in &diff.removed_edges {
        all_scopes.insert(nearest_named_scope(&edge.src));
        all_scopes.insert(nearest_named_scope(&edge.tgt));
    }

    // Build edge change sets for named-edge detection.
    let edges_added_for = count_named_edge_changes(&diff.added_edges);
    let edges_removed_for = count_named_edge_changes(&diff.removed_edges);

    // Classify each scope.
    let mut scopes = Vec::new();
    let mut sorted_scopes: Vec<&str> = all_scopes.into_iter().collect();
    sorted_scopes.sort_unstable();

    for &scope_id in &sorted_scopes {
        let added = scope_added.get(scope_id).map_or(&[][..], |v| v.as_slice());
        let removed = scope_removed
            .get(scope_id)
            .map_or(&[][..], |v| v.as_slice());
        let anon_added = count_anonymous(added);
        let anon_removed = count_anonymous(removed);

        let scope_itself_added = added_set.contains(scope_id);
        let scope_itself_removed = removed_set.contains(scope_id);
        let has_named_edge_changes =
            edges_added_for.contains_key(scope_id) || edges_removed_for.contains_key(scope_id);

        let kind = if scope_itself_added {
            ScopeChangeKind::Added
        } else if scope_itself_removed {
            ScopeChangeKind::Removed
        } else if has_named_edge_changes {
            ScopeChangeKind::SignatureChanged
        } else {
            ScopeChangeKind::BodyModified
        };

        let summary = match &kind {
            ScopeChangeKind::Added => format!("{} added", scope_display_name(scope_id)),
            ScopeChangeKind::Removed => format!("{} removed", scope_display_name(scope_id)),
            ScopeChangeKind::SignatureChanged => {
                format!("{} signature changed", scope_display_name(scope_id))
            }
            ScopeChangeKind::BodyModified => {
                let total = anon_added + anon_removed;
                format!(
                    "{} body modified ({total} anonymous node{})",
                    scope_display_name(scope_id),
                    if total == 1 { "" } else { "s" },
                )
            }
        };

        // Resolve line numbers from start-byte/end-byte constraints.
        let (start_line, end_line) = resolve_scope_lines(
            scope_id, &kind, old_schema, new_schema, old_bytes, new_bytes,
        );

        scopes.push(ScopeChange {
            scope_id: scope_id.to_string(),
            scope_name: scope_display_name(scope_id).to_string(),
            kind,
            summary,
            anonymous_added: anon_added,
            anonymous_removed: anon_removed,
            start_line,
            end_line,
        });
    }

    // Build named_elements from both schemas.
    let named_elements = build_named_elements(
        old_schema,
        new_schema,
        &added_set,
        &removed_set,
        &scope_added,
        &scope_removed,
        &edges_added_for,
        &edges_removed_for,
        old_bytes,
        new_bytes,
    );

    ScopeReport {
        scopes,
        named_elements,
    }
}

/// Resolve start/end line numbers for a scope from byte-offset constraints.
fn resolve_scope_lines(
    scope_id: &str,
    kind: &ScopeChangeKind,
    old_schema: &Schema,
    new_schema: &Schema,
    old_bytes: Option<&[u8]>,
    new_bytes: Option<&[u8]>,
) -> (Option<usize>, Option<usize>) {
    let (schema, source) = if *kind == ScopeChangeKind::Removed {
        (old_schema, old_bytes)
    } else {
        (new_schema, new_bytes)
    };
    let start = source.and_then(|bytes| {
        start_byte_for_vertex(schema, scope_id).map(|off| byte_offset_to_line(off, bytes))
    });
    let end = source.and_then(|bytes| {
        end_byte_for_vertex(schema, scope_id).map(|off| byte_offset_to_line(off, bytes))
    });
    (start, end)
}

/// Build the `named_elements` list from both schemas.
#[allow(clippy::too_many_arguments)]
fn build_named_elements(
    old_schema: &Schema,
    new_schema: &Schema,
    added_set: &rustc_hash::FxHashSet<&str>,
    removed_set: &rustc_hash::FxHashSet<&str>,
    scope_added: &FxHashMap<&str, Vec<&str>>,
    scope_removed: &FxHashMap<&str, Vec<&str>>,
    edges_added_for: &FxHashMap<&str, usize>,
    edges_removed_for: &FxHashMap<&str, usize>,
    old_bytes: Option<&[u8]>,
    new_bytes: Option<&[u8]>,
) -> Vec<NamedElement> {
    let mut elements: HashMap<String, NamedElement> = HashMap::new();

    // Collect from new schema.
    for (vid, vertex) in &new_schema.vertices {
        let id_str = vid.to_string();
        let name = id_str.rsplit("::").next().unwrap_or(&id_str).to_string();
        if is_anonymous_segment(&name) {
            continue;
        }

        let status = if added_set.contains(id_str.as_str()) {
            ElementStatus::Added
        } else if edges_added_for.contains_key(id_str.as_str())
            || edges_removed_for.contains_key(id_str.as_str())
        {
            ElementStatus::SignatureChanged
        } else if scope_added.contains_key(id_str.as_str())
            || scope_removed.contains_key(id_str.as_str())
        {
            ElementStatus::BodyModified
        } else {
            ElementStatus::Unchanged
        };

        let start_line = new_bytes.and_then(|bytes| {
            start_byte_for_vertex(new_schema, &id_str).map(|off| byte_offset_to_line(off, bytes))
        });

        elements.insert(
            id_str.clone(),
            NamedElement {
                id: id_str,
                name,
                kind: vertex.kind.to_string(),
                status,
                start_line,
            },
        );
    }

    // Add removed elements from old schema.
    for (vid, vertex) in &old_schema.vertices {
        let id_str = vid.to_string();
        let name = id_str.rsplit("::").next().unwrap_or(&id_str).to_string();
        if is_anonymous_segment(&name) {
            continue;
        }
        if elements.contains_key(&id_str) {
            continue;
        }
        if !removed_set.contains(id_str.as_str()) {
            continue;
        }

        let start_line = old_bytes.and_then(|bytes| {
            start_byte_for_vertex(old_schema, &id_str).map(|off| byte_offset_to_line(off, bytes))
        });

        elements.insert(
            id_str.clone(),
            NamedElement {
                id: id_str,
                name,
                kind: vertex.kind.to_string(),
                status: ElementStatus::Removed,
                start_line,
            },
        );
    }

    let mut result: Vec<NamedElement> = elements.into_values().collect();
    result.sort_by(|a, b| a.id.cmp(&b.id));
    result
}

/// Render a scope report as human-readable text.
#[must_use]
pub fn report_scope_text(report: &ScopeReport) -> String {
    use std::fmt::Write;

    let mut out = String::new();

    if report.scopes.is_empty() {
        out.push_str("No scope-level changes detected.\n");
        return out;
    }

    let _ = writeln!(out, "{} scope(s) changed:\n", report.scopes.len());

    for scope in &report.scopes {
        let line_info = match (scope.start_line, scope.end_line) {
            (Some(s), Some(e)) => format!(" (lines {s}:{e})"),
            (Some(s), None) => format!(" (line {s})"),
            _ => String::new(),
        };

        let _ = writeln!(out, "  {:?} {}{}", scope.kind, scope.scope_id, line_info);
        let _ = writeln!(out, "    {}", scope.summary);
    }

    out
}

/// Render a scope report as a JSON value.
#[must_use]
pub fn report_scope_json(report: &ScopeReport) -> serde_json::Value {
    serde_json::to_value(report).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use panproto_gat::Name;
    use panproto_schema::Vertex;

    fn test_schema(verts: &[(&str, &str)]) -> Schema {
        let mut vertices = HashMap::new();
        for (id, kind) in verts {
            vertices.insert(
                Name::from(*id),
                Vertex {
                    id: Name::from(*id),
                    kind: Name::from(*kind),
                    nsid: None,
                },
            );
        }
        Schema {
            protocol: "test".into(),
            vertices,
            edges: HashMap::new(),
            hyper_edges: HashMap::new(),
            constraints: HashMap::new(),
            required: HashMap::new(),
            nsids: HashMap::new(),
            variants: HashMap::new(),
            orderings: HashMap::new(),
            recursion_points: HashMap::new(),
            spans: HashMap::new(),
            usage_modes: HashMap::new(),
            nominal: HashMap::new(),
            coercions: HashMap::new(),
            mergers: HashMap::new(),
            defaults: HashMap::new(),
            policies: HashMap::new(),
            outgoing: HashMap::new(),
            incoming: HashMap::new(),
            between: HashMap::new(),
        }
    }

    #[test]
    fn nearest_scope_skips_anonymous() {
        assert_eq!(
            nearest_named_scope("file.rs::MyClass::method::$0::$1"),
            "file.rs::MyClass::method"
        );
        assert_eq!(nearest_named_scope("file.rs::MyClass"), "file.rs::MyClass");
        assert_eq!(nearest_named_scope("$0::$1"), "$0::$1");
    }

    #[test]
    fn scope_name_extracts_last_named() {
        assert_eq!(scope_display_name("file.rs::MyClass::method"), "method");
        assert_eq!(scope_display_name("file.rs"), "file.rs");
    }

    #[test]
    fn body_modified_groups_anonymous() {
        let old = test_schema(&[
            ("file.rs::fn_a", "function"),
            ("file.rs::fn_a::$0", "expression_statement"),
            ("file.rs::fn_a::$1", "expression_statement"),
        ]);
        let new = test_schema(&[
            ("file.rs::fn_a", "function"),
            ("file.rs::fn_a::$0", "expression_statement"),
            ("file.rs::fn_a::$1", "expression_statement"),
            ("file.rs::fn_a::$2", "expression_statement"),
        ]);

        let diff = SchemaDiff {
            added_vertices: vec!["file.rs::fn_a::$2".to_string()],
            ..SchemaDiff::default()
        };

        let report = report_by_scope(&diff, &old, &new, None, None);
        assert_eq!(report.scopes.len(), 1);
        assert_eq!(report.scopes[0].scope_id, "file.rs::fn_a");
        assert_eq!(report.scopes[0].kind, ScopeChangeKind::BodyModified);
        assert_eq!(report.scopes[0].anonymous_added, 1);
    }

    #[test]
    fn added_scope_detected() {
        let old = test_schema(&[("file.rs", "source_file")]);
        let new = test_schema(&[("file.rs", "source_file"), ("file.rs::new_fn", "function")]);

        let diff = SchemaDiff {
            added_vertices: vec!["file.rs::new_fn".to_string()],
            ..SchemaDiff::default()
        };

        let report = report_by_scope(&diff, &old, &new, None, None);
        assert_eq!(report.scopes.len(), 1);
        assert_eq!(report.scopes[0].kind, ScopeChangeKind::Added);
    }

    #[test]
    fn removed_scope_detected() {
        let old = test_schema(&[("file.rs", "source_file"), ("file.rs::old_fn", "function")]);
        let new = test_schema(&[("file.rs", "source_file")]);

        let diff = SchemaDiff {
            removed_vertices: vec!["file.rs::old_fn".to_string()],
            ..SchemaDiff::default()
        };

        let report = report_by_scope(&diff, &old, &new, None, None);
        assert_eq!(report.scopes.len(), 1);
        assert_eq!(report.scopes[0].kind, ScopeChangeKind::Removed);
    }

    #[test]
    fn named_elements_lists_all() {
        let old = test_schema(&[
            ("file.rs", "source_file"),
            ("file.rs::fn_a", "function"),
            ("file.rs::fn_a::$0", "statement"),
        ]);
        let new = test_schema(&[
            ("file.rs", "source_file"),
            ("file.rs::fn_a", "function"),
            ("file.rs::fn_a::$0", "statement"),
            ("file.rs::fn_a::$1", "statement"),
        ]);

        let diff = SchemaDiff {
            added_vertices: vec!["file.rs::fn_a::$1".to_string()],
            ..SchemaDiff::default()
        };

        let report = report_by_scope(&diff, &old, &new, None, None);
        // Named elements should include file.rs and fn_a but not $0 or $1
        let names: Vec<&str> = report
            .named_elements
            .iter()
            .map(|e| e.name.as_str())
            .collect();
        assert!(names.contains(&"fn_a"));
        assert!(!names.contains(&"$0"));
        assert!(!names.contains(&"$1"));
    }

    #[test]
    fn empty_diff_produces_empty_report() {
        let schema = test_schema(&[("file.rs", "source_file")]);
        let diff = SchemaDiff::default();
        let report = report_by_scope(&diff, &schema, &schema, None, None);
        assert!(report.scopes.is_empty());
    }

    #[test]
    fn text_report_formats() {
        let old = test_schema(&[
            ("file.rs::fn_a", "function"),
            ("file.rs::fn_a::$0", "statement"),
        ]);
        let new = test_schema(&[
            ("file.rs::fn_a", "function"),
            ("file.rs::fn_a::$0", "statement"),
            ("file.rs::fn_a::$1", "statement"),
        ]);

        let diff = SchemaDiff {
            added_vertices: vec!["file.rs::fn_a::$1".to_string()],
            ..SchemaDiff::default()
        };

        let report = report_by_scope(&diff, &old, &new, None, None);
        let text = report_scope_text(&report);
        assert!(text.contains("fn_a"));
        assert!(text.contains("BodyModified"));
    }
}
