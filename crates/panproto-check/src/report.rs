//! Human-readable and machine-readable report generation.
//!
//! Converts a [`CompatReport`] into either a plain-text summary
//! ([`report_text`]) or a JSON value ([`report_json`]).

use std::fmt::Write;

use serde_json::json;

use crate::classify::{BreakingChange, Classification, CompatReport, NonBreakingChange};

/// Human-readable label for a [`Classification`] tier.
const fn classification_label(c: Classification) -> &'static str {
    match c {
        Classification::FullyCompatible => "fully-compatible",
        Classification::BackwardCompatible => "backward-compatible",
        Classification::Breaking => "breaking",
    }
}

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
    let _ = writeln!(
        out,
        "Classification: {}",
        classification_label(compat.classification)
    );

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
        "classification": classification_label(compat.classification),
        "breaking": breaking,
        "non_breaking": non_breaking,
        "breaking_count": compat.breaking.len(),
        "non_breaking_count": compat.non_breaking.len(),
    })
}

/// Format a single breaking change for text output.
#[allow(clippy::too_many_lines)]
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
        BreakingChange::RemovedVariant {
            vertex_id,
            variant_id,
        } => {
            format!("Removed variant: {vertex_id}/{variant_id}")
        }
        BreakingChange::OrderToUnordered { edge } => {
            format!(
                "Order removed: {} -> {} ({})",
                edge.src, edge.tgt, edge.kind
            )
        }
        BreakingChange::RecursionBroken { mu_id } => {
            format!("Recursion broken: {mu_id}")
        }
        BreakingChange::LinearityTightened {
            edge,
            old_mode,
            new_mode,
        } => {
            format!(
                "Linearity tightened: {} -> {} ({}) {old_mode:?} -> {new_mode:?}",
                edge.src, edge.tgt, edge.kind
            )
        }
        BreakingChange::CoercionClassDowngraded {
            from_kind,
            to_kind,
            old_class,
            new_class,
        } => {
            format!(
                "Coercion class downgraded: ({from_kind} -> {to_kind}) {old_class} -> {new_class}"
            )
        }
        BreakingChange::CoercionRemoved {
            from_kind, to_kind, ..
        } => {
            format!("Coercion removed: ({from_kind} -> {to_kind})")
        }
        BreakingChange::RequiredEdgeAdded {
            vertex_id,
            src,
            tgt,
            kind,
            name,
        } => {
            let label = name
                .as_deref()
                .map_or(String::new(), |n| format!(" (name: {n})"));
            format!("Required edge added on {vertex_id}: {src} -> {tgt} [{kind}]{label}")
        }
        BreakingChange::RequiredEdgeRemoved {
            vertex_id,
            src,
            tgt,
            kind,
            name,
        } => {
            let label = name
                .as_deref()
                .map_or(String::new(), |n| format!(" (name: {n})"));
            format!("Required edge removed on {vertex_id}: {src} -> {tgt} [{kind}]{label}")
        }
        BreakingChange::AddedVariant {
            vertex_id,
            variant_id,
        } => {
            format!("Added variant: {vertex_id}/{variant_id}")
        }
        BreakingChange::ModifiedVariant {
            vertex_id,
            variant_id,
            old_tag,
            new_tag,
        } => {
            format!(
                "Modified variant: {vertex_id}/{variant_id} ({} -> {})",
                old_tag.as_deref().unwrap_or("<none>"),
                new_tag.as_deref().unwrap_or("<none>"),
            )
        }
        BreakingChange::UnorderedToOrdered { edge } => {
            format!("Order added: {} -> {} ({})", edge.src, edge.tgt, edge.kind)
        }
        BreakingChange::RecursionPointAdded { mu_id } => {
            format!("Recursion point added: {mu_id}")
        }
        BreakingChange::RecursionPointModified {
            mu_id,
            old_target,
            new_target,
        } => {
            format!("Recursion point retargeted: {mu_id} ({old_target} -> {new_target})")
        }
        BreakingChange::NsidChanged {
            vertex_id,
            old_nsid,
            new_nsid,
        } => {
            format!("NSID changed: {vertex_id} ({old_nsid} -> {new_nsid})")
        }
        BreakingChange::NsidRemoved { vertex_id } => {
            format!("NSID removed: {vertex_id}")
        }
        BreakingChange::HyperEdgeRemoved { id } => {
            format!("Hyper-edge removed: {id}")
        }
        BreakingChange::HyperEdgeModified { id } => {
            format!("Hyper-edge modified: {id}")
        }
        BreakingChange::SpanRemoved { id } => {
            format!("Span removed: {id}")
        }
        BreakingChange::SpanModified { id } => {
            format!("Span modified: {id}")
        }
        BreakingChange::NominalFlipped {
            vertex_id,
            old_value,
            new_value,
        } => {
            format!("Nominal flag flipped: {vertex_id} ({old_value} -> {new_value})")
        }
        BreakingChange::EnrichmentRemoved { category, key } => {
            format!("Enrichment removed: {category} {key}")
        }
        BreakingChange::EnrichmentModified { category, key } => {
            format!("Enrichment modified: {category} {key}")
        }
        BreakingChange::RenamedVertex { old_id, new_id } => {
            format!("Renamed vertex: {old_id} -> {new_id}")
        }
        BreakingChange::UnclassifiedChange { category, count } => {
            format!("Unclassified change: {category} ({count})")
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
        NonBreakingChange::RemovedEdge {
            src,
            tgt,
            kind,
            name,
        } => {
            let label = name
                .as_deref()
                .map_or(String::new(), |n| format!(" (name: {n})"));
            format!("Removed edge (non-governed): {src} -> {tgt} [{kind}]{label}")
        }
        NonBreakingChange::AddedNsid { vertex_id, nsid } => {
            format!("NSID added: {vertex_id} = {nsid}")
        }
        NonBreakingChange::AddedHyperEdge { id } => {
            format!("Hyper-edge added: {id}")
        }
        NonBreakingChange::AddedSpan { id } => {
            format!("Span added: {id}")
        }
        NonBreakingChange::EnrichmentAdded { category, key } => {
            format!("Enrichment added: {category} {key}")
        }
        NonBreakingChange::LinearityRelaxed {
            edge,
            old_mode,
            new_mode,
        } => {
            format!(
                "Linearity relaxed: {} -> {} ({}) {old_mode:?} -> {new_mode:?}",
                edge.src, edge.tgt, edge.kind
            )
        }
    }
}

/// Convert a breaking change to JSON.
#[allow(clippy::too_many_lines)]
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
        BreakingChange::RemovedVariant {
            vertex_id,
            variant_id,
        } => json!({
            "type": "removed_variant",
            "vertex_id": vertex_id,
            "variant_id": variant_id,
        }),
        BreakingChange::OrderToUnordered { edge } => json!({
            "type": "order_to_unordered",
            "src": edge.src,
            "tgt": edge.tgt,
            "kind": edge.kind,
        }),
        BreakingChange::RecursionBroken { mu_id } => json!({
            "type": "recursion_broken",
            "mu_id": mu_id,
        }),
        BreakingChange::LinearityTightened {
            edge,
            old_mode,
            new_mode,
        } => json!({
            "type": "linearity_tightened",
            "src": edge.src,
            "tgt": edge.tgt,
            "kind": edge.kind,
            "old_mode": format!("{old_mode:?}"),
            "new_mode": format!("{new_mode:?}"),
        }),
        BreakingChange::CoercionClassDowngraded {
            from_kind,
            to_kind,
            old_class,
            new_class,
        } => json!({
            "type": "coercion_class_downgraded",
            "from_kind": from_kind,
            "to_kind": to_kind,
            "old_class": old_class,
            "new_class": new_class,
        }),
        BreakingChange::CoercionRemoved {
            from_kind, to_kind, ..
        } => json!({
            "type": "coercion_removed",
            "from_kind": from_kind,
            "to_kind": to_kind,
        }),
        BreakingChange::RequiredEdgeAdded {
            vertex_id,
            src,
            tgt,
            kind,
            name,
        } => json!({
            "type": "required_edge_added",
            "vertex_id": vertex_id,
            "src": src,
            "tgt": tgt,
            "kind": kind,
            "name": name,
        }),
        BreakingChange::RequiredEdgeRemoved {
            vertex_id,
            src,
            tgt,
            kind,
            name,
        } => json!({
            "type": "required_edge_removed",
            "vertex_id": vertex_id,
            "src": src,
            "tgt": tgt,
            "kind": kind,
            "name": name,
        }),
        BreakingChange::AddedVariant {
            vertex_id,
            variant_id,
        } => json!({
            "type": "added_variant",
            "vertex_id": vertex_id,
            "variant_id": variant_id,
        }),
        BreakingChange::ModifiedVariant {
            vertex_id,
            variant_id,
            old_tag,
            new_tag,
        } => json!({
            "type": "modified_variant",
            "vertex_id": vertex_id,
            "variant_id": variant_id,
            "old_tag": old_tag,
            "new_tag": new_tag,
        }),
        BreakingChange::UnorderedToOrdered { edge } => json!({
            "type": "unordered_to_ordered",
            "src": edge.src,
            "tgt": edge.tgt,
            "kind": edge.kind,
        }),
        BreakingChange::RecursionPointAdded { mu_id } => json!({
            "type": "recursion_point_added",
            "mu_id": mu_id,
        }),
        BreakingChange::RecursionPointModified {
            mu_id,
            old_target,
            new_target,
        } => json!({
            "type": "recursion_point_modified",
            "mu_id": mu_id,
            "old_target": old_target,
            "new_target": new_target,
        }),
        BreakingChange::NsidChanged {
            vertex_id,
            old_nsid,
            new_nsid,
        } => json!({
            "type": "nsid_changed",
            "vertex_id": vertex_id,
            "old_nsid": old_nsid,
            "new_nsid": new_nsid,
        }),
        BreakingChange::NsidRemoved { vertex_id } => json!({
            "type": "nsid_removed",
            "vertex_id": vertex_id,
        }),
        BreakingChange::HyperEdgeRemoved { id } => json!({
            "type": "hyper_edge_removed",
            "id": id,
        }),
        BreakingChange::HyperEdgeModified { id } => json!({
            "type": "hyper_edge_modified",
            "id": id,
        }),
        BreakingChange::SpanRemoved { id } => json!({
            "type": "span_removed",
            "id": id,
        }),
        BreakingChange::SpanModified { id } => json!({
            "type": "span_modified",
            "id": id,
        }),
        BreakingChange::NominalFlipped {
            vertex_id,
            old_value,
            new_value,
        } => json!({
            "type": "nominal_flipped",
            "vertex_id": vertex_id,
            "old_value": old_value,
            "new_value": new_value,
        }),
        BreakingChange::EnrichmentRemoved { category, key } => json!({
            "type": "enrichment_removed",
            "category": category,
            "key": key,
        }),
        BreakingChange::EnrichmentModified { category, key } => json!({
            "type": "enrichment_modified",
            "category": category,
            "key": key,
        }),
        BreakingChange::RenamedVertex { old_id, new_id } => json!({
            "type": "renamed_vertex",
            "old_id": old_id,
            "new_id": new_id,
        }),
        BreakingChange::UnclassifiedChange { category, count } => json!({
            "type": "unclassified_change",
            "category": category,
            "count": count,
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
        NonBreakingChange::RemovedEdge {
            src,
            tgt,
            kind,
            name,
        } => json!({
            "type": "removed_edge_non_governed",
            "src": src,
            "tgt": tgt,
            "kind": kind,
            "name": name,
        }),
        NonBreakingChange::AddedNsid { vertex_id, nsid } => json!({
            "type": "added_nsid",
            "vertex_id": vertex_id,
            "nsid": nsid,
        }),
        NonBreakingChange::AddedHyperEdge { id } => json!({
            "type": "added_hyper_edge",
            "id": id,
        }),
        NonBreakingChange::AddedSpan { id } => json!({
            "type": "added_span",
            "id": id,
        }),
        NonBreakingChange::EnrichmentAdded { category, key } => json!({
            "type": "enrichment_added",
            "category": category,
            "key": key,
        }),
        NonBreakingChange::LinearityRelaxed {
            edge,
            old_mode,
            new_mode,
        } => json!({
            "type": "linearity_relaxed",
            "src": edge.src,
            "tgt": edge.tgt,
            "kind": edge.kind,
            "old_mode": format!("{old_mode:?}"),
            "new_mode": format!("{new_mode:?}"),
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
            classification: Classification::BackwardCompatible,
        };

        let text = report_text(&report);
        assert!(text.contains("COMPATIBLE"));
        assert!(text.contains("backward-compatible"));
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
            classification: Classification::Breaking,
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
            classification: Classification::Breaking,
        };

        let json = report_json(&report);
        assert_eq!(json["compatible"], false);
        assert_eq!(json["classification"], "breaking");
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
            classification: Classification::FullyCompatible,
        };

        let json = report_json(&report);
        assert!(json.is_object());
        assert!(json["breaking"].is_array());
        assert!(json["non_breaking"].is_array());
        assert_eq!(json["classification"], "fully-compatible");
    }
}
