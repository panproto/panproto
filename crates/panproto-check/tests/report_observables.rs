//! Report output preserves change presence and complete declaration identities.

use panproto_check::classify::{BreakingChange, Classification, CompatReport, NonBreakingChange};
use panproto_check::{report_json, report_text};
use serde_json::json;

#[test]
fn no_changes_message_requires_both_change_lists_to_be_empty() {
    for (removed, added, no_changes) in [
        (false, false, true),
        (true, false, false),
        (false, true, false),
        (true, true, false),
    ] {
        let report = CompatReport {
            breaking: if removed {
                vec![BreakingChange::RemovedVertex {
                    vertex_id: "old".into(),
                }]
            } else {
                vec![]
            },
            non_breaking: if added {
                vec![NonBreakingChange::AddedVertex {
                    vertex_id: "new".into(),
                }]
            } else {
                vec![]
            },
            compatible: !removed,
            classification: if removed {
                Classification::Breaking
            } else if added {
                Classification::BackwardCompatible
            } else {
                Classification::FullyCompatible
            },
        };
        let text = report_text(&report);
        assert_eq!(
            text.contains("\nNo changes detected.\n"),
            no_changes,
            "removed={removed}, added={added}: {text}"
        );
        assert_eq!(text.contains("\nBreaking changes (1):"), removed);
        assert_eq!(text.contains("\nNon-breaking changes (1):"), added);
    }
}

#[test]
fn edge_change_json_preserves_endpoints_kind_and_optional_name() {
    for name in [None, Some(""), Some("μ.field")] {
        let report = CompatReport {
            breaking: vec![],
            non_breaking: vec![
                NonBreakingChange::AddedEdge {
                    src: "source".into(),
                    tgt: "target".into(),
                    kind: "reference".into(),
                    name: name.map(str::to_owned),
                },
                NonBreakingChange::RemovedEdge {
                    src: "former-source".into(),
                    tgt: "former-target".into(),
                    kind: "former-kind".into(),
                    name: name.map(str::to_owned),
                },
            ],
            compatible: true,
            classification: Classification::BackwardCompatible,
        };
        assert_eq!(
            report_json(&report),
            json!({
                "compatible": true,
                "classification": "backward-compatible",
                "breaking": [],
                "breaking_count": 0,
                "non_breaking_count": 2,
                "non_breaking": [
                    {"type": "added_edge", "src": "source", "tgt": "target",
                     "kind": "reference", "name": name},
                    {"type": "removed_edge_non_governed", "src": "former-source",
                     "tgt": "former-target", "kind": "former-kind", "name": name},
                ],
            })
        );
    }
}

#[test]
fn declaration_change_json_preserves_both_kind_and_id() {
    for id in ["", "declaration", "μ.span"] {
        let report = CompatReport {
            breaking: vec![],
            non_breaking: vec![
                NonBreakingChange::AddedHyperEdge { id: id.into() },
                NonBreakingChange::AddedSpan { id: id.into() },
            ],
            compatible: true,
            classification: Classification::BackwardCompatible,
        };
        assert_eq!(
            report_json(&report),
            json!({
                "compatible": true,
                "classification": "backward-compatible",
                "breaking": [],
                "breaking_count": 0,
                "non_breaking_count": 2,
                "non_breaking": [
                    {"type": "added_hyper_edge", "id": id},
                    {"type": "added_span", "id": id},
                ],
            })
        );
    }
}
