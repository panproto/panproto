//! Human-readable and machine-readable report generation.
//!
//! Converts a [`CompatReport`] into either a plain-text summary
//! ([`report_text`]) or a JSON value ([`report_json`]).

use std::fmt::Write;

use serde_json::json;

use crate::classify::{BreakingChange, CompatReport, NonBreakingChange};

/// Render a compatibility report as human-readable text.
///
/// The output is suitable for terminal display and includes a
/// compatibility verdict, followed by itemized breaking and
/// non-breaking changes.
#[must_use]
pub fn report_text(compat: &CompatReport) -> String {
    let mut out = String::new();

    if compat.compatible {
        out.push_str("COMPATIBLE: No breaking changes detected.\n");
    } else {
        out.push_str("INCOMPATIBLE: Breaking changes detected.\n");
    }

    if !compat.breaking.is_empty() {
        let _ = writeln!(out, "\nBreaking changes ({}):", compat.breaking.len());
        for (i, change) in compat.breaking.iter().enumerate() {
            let _ = writeln!(out, "  {}. {}", i + 1, format_breaking(change));
        }
    }

    if !compat.non_breaking.is_empty() {
        let _ = writeln!(
            out,
            "\nNon-breaking changes ({}):",
            compat.non_breaking.len()
        );
        for (i, change) in compat.non_breaking.iter().enumerate() {
            let _ = writeln!(out, "  {}. {}", i + 1, format_non_breaking(change));
        }
    }

    if compat.breaking.is_empty() && compat.non_breaking.is_empty() {
        out.push_str("\nNo changes detected.\n");
    }

    out
}

/// Render a compatibility report as a JSON value.
///
/// The JSON structure contains `compatible` (bool), `breaking` (array),
/// and `non_breaking` (array) fields.
#[must_use]
pub fn report_json(compat: &CompatReport) -> serde_json::Value {
    let breaking: Vec<serde_json::Value> = compat.breaking.iter().map(breaking_to_json).collect();

    let non_breaking: Vec<serde_json::Value> = compat
        .non_breaking
        .iter()
        .map(non_breaking_to_json)
        .collect();

    json!({
        "compatible": compat.compatible,
        "breaking": breaking,
        "non_breaking": non_breaking,
        "breaking_count": compat.breaking.len(),
        "non_breaking_count": compat.non_breaking.len(),
    })
}

/// Format a single breaking change for text output.
fn format_breaking(change: &BreakingChange) -> String {
    match change {
        BreakingChange::RemovedVertex { vertex_id } => {
            format!("Removed vertex: {vertex_id}")
        }
        BreakingChange::RemovedEdge {
            src,
            tgt,
            kind,
            name,
        } => {
            let label = name
                .as_deref()
                .map_or(String::new(), |n| format!(" (name: {n})"));
            format!("Removed edge: {src} -> {tgt} [{kind}]{label}")
        }
        BreakingChange::KindChanged {
            vertex_id,
            old_kind,
            new_kind,
        } => {
            format!("Kind changed: {vertex_id} ({old_kind} -> {new_kind})")
        }
        BreakingChange::ConstraintTightened {
            vertex_id,
            sort,
            old_value,
            new_value,
        } => {
            format!("Constraint tightened: {vertex_id}.{sort} ({old_value} -> {new_value})")
        }
        BreakingChange::ConstraintAdded {
            vertex_id,
            sort,
            value,
        } => {
            format!("Constraint added: {vertex_id}.{sort} = {value}")
        }
    }
}

/// Format a single non-breaking change for text output.
fn format_non_breaking(change: &NonBreakingChange) -> String {
    match change {
        NonBreakingChange::AddedVertex { vertex_id } => {
            format!("Added vertex: {vertex_id}")
        }
        NonBreakingChange::AddedEdge {
            src,
            tgt,
            kind,
            name,
        } => {
            let label = name
                .as_deref()
                .map_or(String::new(), |n| format!(" (name: {n})"));
            format!("Added edge: {src} -> {tgt} [{kind}]{label}")
        }
        NonBreakingChange::ConstraintRelaxed {
            vertex_id,
            sort,
            old_value,
            new_value,
        } => {
            format!("Constraint relaxed: {vertex_id}.{sort} ({old_value} -> {new_value})")
        }
        NonBreakingChange::ConstraintRemoved { vertex_id, sort } => {
            format!("Constraint removed: {vertex_id}.{sort}")
        }
    }
}

/// Convert a breaking change to JSON.
fn breaking_to_json(change: &BreakingChange) -> serde_json::Value {
    match change {
        BreakingChange::RemovedVertex { vertex_id } => json!({
            "type": "removed_vertex",
            "vertex_id": vertex_id,
        }),
        BreakingChange::RemovedEdge {
            src,
            tgt,
            kind,
            name,
        } => json!({
            "type": "removed_edge",
            "src": src,
            "tgt": tgt,
            "kind": kind,
            "name": name,
        }),
        BreakingChange::KindChanged {
            vertex_id,
            old_kind,
            new_kind,
        } => json!({
            "type": "kind_changed",
            "vertex_id": vertex_id,
            "old_kind": old_kind,
            "new_kind": new_kind,
        }),
        BreakingChange::ConstraintTightened {
            vertex_id,
            sort,
            old_value,
            new_value,
        } => json!({
            "type": "constraint_tightened",
            "vertex_id": vertex_id,
            "sort": sort,
            "old_value": old_value,
            "new_value": new_value,
        }),
        BreakingChange::ConstraintAdded {
            vertex_id,
            sort,
            value,
        } => json!({
            "type": "constraint_added",
            "vertex_id": vertex_id,
            "sort": sort,
            "value": value,
        }),
    }
}

/// Convert a non-breaking change to JSON.
fn non_breaking_to_json(change: &NonBreakingChange) -> serde_json::Value {
    match change {
        NonBreakingChange::AddedVertex { vertex_id } => json!({
            "type": "added_vertex",
            "vertex_id": vertex_id,
        }),
        NonBreakingChange::AddedEdge {
            src,
            tgt,
            kind,
            name,
        } => json!({
            "type": "added_edge",
            "src": src,
            "tgt": tgt,
            "kind": kind,
            "name": name,
        }),
        NonBreakingChange::ConstraintRelaxed {
            vertex_id,
            sort,
            old_value,
            new_value,
        } => json!({
            "type": "constraint_relaxed",
            "vertex_id": vertex_id,
            "sort": sort,
            "old_value": old_value,
            "new_value": new_value,
        }),
        NonBreakingChange::ConstraintRemoved { vertex_id, sort } => json!({
            "type": "constraint_removed",
            "vertex_id": vertex_id,
            "sort": sort,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_text_compatible() {
        let report = CompatReport {
            breaking: vec![],
            non_breaking: vec![NonBreakingChange::AddedVertex {
                vertex_id: "x".into(),
            }],
            compatible: true,
        };

        let text = report_text(&report);
        assert!(text.contains("COMPATIBLE"));
        assert!(text.contains("Added vertex: x"));
    }

    #[test]
    fn report_text_incompatible() {
        let report = CompatReport {
            breaking: vec![BreakingChange::RemovedVertex {
                vertex_id: "y".into(),
            }],
            non_breaking: vec![],
            compatible: false,
        };

        let text = report_text(&report);
        assert!(text.contains("INCOMPATIBLE"));
        assert!(text.contains("Removed vertex: y"));
    }

    #[test]
    fn report_json_structure() {
        let report = CompatReport {
            breaking: vec![BreakingChange::RemovedVertex {
                vertex_id: "a".into(),
            }],
            non_breaking: vec![NonBreakingChange::AddedVertex {
                vertex_id: "b".into(),
            }],
            compatible: false,
        };

        let json = report_json(&report);
        assert_eq!(json["compatible"], false);
        assert_eq!(json["breaking_count"], 1);
        assert_eq!(json["non_breaking_count"], 1);
        assert_eq!(json["breaking"][0]["type"], "removed_vertex");
        assert_eq!(json["non_breaking"][0]["type"], "added_vertex");
    }

    #[test]
    fn report_json_valid_structure() {
        let report = CompatReport {
            breaking: vec![],
            non_breaking: vec![],
            compatible: true,
        };

        let json = report_json(&report);
        assert!(json.is_object());
        assert!(json["breaking"].is_array());
        assert!(json["non_breaking"].is_array());
    }
}
