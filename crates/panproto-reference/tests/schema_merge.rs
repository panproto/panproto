//! The schema-merge reference on hand-computed cases.

use panproto_reference::enumerate::schema_merge::{SCHEMAS_COUNT, format_markers, schemas};
use panproto_reference::pages::schema_merge::{
    Conflict, Identified, Schema, Side, may_rename, merge, repository_format, surviving_identity,
};

fn schema(vertices: &[(&str, &str)], edges: &[(&str, &str)]) -> Schema {
    Schema {
        vertices: vertices
            .iter()
            .map(|(id, kind)| ((*id).to_owned(), (*kind).to_owned()))
            .collect(),
        edges: edges
            .iter()
            .map(|(src, tgt)| ((*src).to_owned(), (*tgt).to_owned()))
            .collect(),
    }
}

#[test]
fn the_merge_rule_on_hand_computed_triples() {
    let base = schema(&[("a", "object"), ("b", "object")], &[]);
    // Ours adds an edge at `a`, theirs deletes `a`: a delete-use conflict,
    // and `a` keeps its base kind so the edge keeps its endpoint.
    let ours = schema(&[("a", "object"), ("b", "object")], &[("a", "b")]);
    let theirs = schema(&[("b", "object")], &[]);
    let merged = merge(&base, &ours, &theirs);
    assert_eq!(
        merged.conflicts,
        vec![Conflict::DeleteModifyVertex {
            vertex: "a".to_owned(),
            deleted_by: Side::Theirs,
        }]
    );
    assert_eq!(merged.merged, ours);

    // Without the edge the deletion stands.
    let clean = merge(&base, &base, &theirs);
    assert_eq!(clean.conflicts, vec![]);
    assert_eq!(clean.merged, theirs);

    // Two equal additions are one element and are reported.
    let added = schema(&[("a", "object"), ("b", "string")], &[("a", "b")]);
    let both = merge(&schema(&[("a", "object")], &[]), &added, &added);
    assert_eq!(both.merged, added);
    assert_eq!(
        both.identified,
        vec![
            Identified::Vertex("b".to_owned()),
            Identified::Edge(("a".to_owned(), "b".to_owned())),
        ]
    );

    // Two different kinds conflict and keep the base kind.
    let string = schema(&[("a", "string"), ("b", "object")], &[]);
    let modified = merge(
        &base,
        &string,
        &schema(&[("a", "boolean"), ("b", "object")], &[]),
    );
    assert_eq!(
        modified.conflicts,
        vec![Conflict::BothModifiedVertex {
            vertex: "a".to_owned(),
            ours: "string".to_owned(),
            theirs: "boolean".to_owned(),
        }]
    );
    assert_eq!(modified.merged, base);
}

#[test]
fn swapping_the_branches_swaps_every_conflict() {
    let family = schemas();
    assert_eq!(family.len(), SCHEMAS_COUNT);
    for base in &family {
        for ours in &family {
            for theirs in &family {
                let forward = merge(base, ours, theirs);
                let backward = merge(base, theirs, ours);
                assert_eq!(forward.merged, backward.merged);
                assert_eq!(forward.identified, backward.identified);
                let swapped: Vec<Conflict> =
                    backward.conflicts.iter().map(Conflict::swapped).collect();
                assert_eq!(forward.conflicts, swapped);
            }
        }
    }
}

#[test]
fn surviving_identity_on_hand_computed_pairs() {
    let old = schema(
        &[("a", "object"), ("b", "string")],
        &[("a", "b"), ("a", "a")],
    );
    let new = schema(&[("a", "string"), ("c", "string")], &[("a", "a")]);
    let (vertices, edges) = surviving_identity(&old, &new);
    assert_eq!(
        vertices.into_iter().collect::<Vec<_>>(),
        vec![("a".to_owned(), "a".to_owned())]
    );
    assert_eq!(
        edges.into_iter().collect::<Vec<_>>(),
        vec![("a".to_owned(), "a".to_owned())]
    );
    assert!(may_rename(&old, &new));
    assert!(!may_rename(&old, &old));
}

#[test]
fn the_format_marker_is_one_line_feed_terminated_numeral() {
    assert_eq!(repository_format(None), Some(1));
    assert_eq!(repository_format(Some(b"1\n")), Some(1));
    for refused in [
        &b"1"[..],
        b"2\n",
        b"01\n",
        b" 1\n",
        b"1\n\n",
        b"",
        b"\xff\n",
    ] {
        assert_eq!(repository_format(Some(refused)), None, "{refused:?}");
    }
    let markers = format_markers(3);
    assert_eq!(markers.len(), 1 + 1 + 6 + 36 + 216);
    let accepted = markers
        .iter()
        .filter(|marker| repository_format(marker.as_deref()).is_some())
        .count();
    assert_eq!(accepted, 2);
}
