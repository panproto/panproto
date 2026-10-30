//! Counterexamples for reverse anchor lookup.

use std::collections::BTreeMap;

use panproto_reference::migration::Migration;
use panproto_reference::restrict::reverse_anchor;
use panproto_reference::schema::{HyperEdge, Schema};

fn hyper_edge() -> HyperEdge {
    HyperEdge {
        kind: "record".to_owned(),
        signature: BTreeMap::new(),
        parent_label: "parent".to_owned(),
    }
}

#[test]
fn a_target_only_anchor_has_no_reverse_preimage() {
    let mut source = Schema::default().vertex("source_vertex", "object");
    source
        .hyper_edges
        .insert("source_hyper_edge".to_owned(), hyper_edge());
    let migration = Migration {
        surviving_verts: [
            "target_only_vertex".to_owned(),
            "target_only_hyper_edge".to_owned(),
        ]
        .into(),
        ..Migration::default()
    };

    assert_eq!(
        reverse_anchor(&source, &migration, "target_only_vertex"),
        Vec::<String>::new()
    );
    assert_eq!(
        reverse_anchor(&source, &migration, "target_only_hyper_edge"),
        Vec::<String>::new()
    );
    assert_eq!(
        reverse_anchor(&source, &migration, "absent_anchor"),
        Vec::<String>::new()
    );
}

#[test]
fn a_dropped_source_anchor_has_no_reverse_preimage() {
    let source = Schema::default().vertex("dropped_vertex", "object");
    let migration = Migration {
        vertex_remap: [("dropped_vertex".to_owned(), "target_vertex".to_owned())].into(),
        ..Migration::default()
    };

    assert_eq!(
        reverse_anchor(&source, &migration, "target_vertex"),
        Vec::<String>::new()
    );
}

#[test]
fn an_identity_hyper_edge_survives_without_vertex_carrier_membership() {
    let mut source = Schema::default();
    source
        .hyper_edges
        .insert("identity_hyper_edge".to_owned(), hyper_edge());

    assert_eq!(
        reverse_anchor(&source, &Migration::default(), "identity_hyper_edge"),
        vec!["identity_hyper_edge".to_owned()]
    );
}

#[test]
fn resolver_targets_and_multi_entry_identity_fallback_are_preimages_once() {
    let mut source = Schema::default();
    source
        .hyper_edges
        .insert("first_hyper_edge".to_owned(), hyper_edge());
    source
        .hyper_edges
        .insert("second_hyper_edge".to_owned(), hyper_edge());
    let migration = Migration {
        hyper_resolver: [
            (
                ("first_hyper_edge".to_owned(), ["one".to_owned()].into()),
                ("target_hyper_edge".to_owned(), BTreeMap::new()),
            ),
            (
                ("first_hyper_edge".to_owned(), ["two".to_owned()].into()),
                ("target_hyper_edge".to_owned(), BTreeMap::new()),
            ),
            (
                ("first_hyper_edge".to_owned(), ["three".to_owned()].into()),
                ("other_target_hyper_edge".to_owned(), BTreeMap::new()),
            ),
            (
                ("second_hyper_edge".to_owned(), ["one".to_owned()].into()),
                ("target_hyper_edge".to_owned(), BTreeMap::new()),
            ),
        ]
        .into(),
        ..Migration::default()
    };

    assert_eq!(
        reverse_anchor(&source, &migration, "target_hyper_edge"),
        vec![
            "first_hyper_edge".to_owned(),
            "second_hyper_edge".to_owned()
        ]
    );
    assert_eq!(
        reverse_anchor(&source, &migration, "first_hyper_edge"),
        vec!["first_hyper_edge".to_owned()]
    );
    assert_eq!(
        reverse_anchor(&source, &migration, "second_hyper_edge"),
        Vec::<String>::new()
    );
    assert_eq!(
        reverse_anchor(&source, &migration, "other_target_hyper_edge"),
        vec!["first_hyper_edge".to_owned()]
    );
}

#[test]
fn vertex_preimages_precede_identity_hyper_edge_preimages() {
    let mut source = Schema::default().vertex("source_vertex", "object");
    source
        .hyper_edges
        .insert("target_anchor".to_owned(), hyper_edge());
    let migration = Migration {
        surviving_verts: ["target_anchor".to_owned()].into(),
        vertex_remap: [("source_vertex".to_owned(), "target_anchor".to_owned())].into(),
        ..Migration::default()
    };

    assert_eq!(
        reverse_anchor(&source, &migration, "target_anchor"),
        vec!["source_vertex".to_owned(), "target_anchor".to_owned()]
    );
}
