//! Snapshot tests for the JSON rendering of [`SchemaDiff`].
//!
//! These tests pin the serialized shape of a diff so that unintentional
//! changes to the wire format surface as review artifacts rather than
//! silent downstream breakage. Review diffs with `cargo insta review`.

use panproto_check::diff::{ConstraintDiff, KindChange, SchemaDiff};
use panproto_schema::{Constraint, Edge};

fn sample_diff() -> SchemaDiff {
    let mut diff = SchemaDiff::default();
    diff.added_vertices.push("post.attachments".to_owned());
    diff.removed_vertices.push("post.legacy_field".to_owned());
    diff.kind_changes.push(KindChange {
        vertex_id: "post.body".to_owned(),
        old_kind: "string".to_owned(),
        new_kind: "object".to_owned(),
    });
    diff.added_edges.push(Edge {
        src: "post".into(),
        tgt: "post.attachments".into(),
        kind: "prop".into(),
        name: Some("attachments".into()),
    });
    diff.modified_constraints.insert(
        "post.body.text".to_owned(),
        ConstraintDiff {
            added: vec![Constraint {
                sort: "maxLength".into(),
                value: "3000".to_owned(),
            }],
            removed: vec![],
            changed: vec![],
        },
    );
    diff
}

#[test]
fn diff_json_shape_is_stable() {
    let diff = sample_diff();
    insta::assert_json_snapshot!(diff);
}
