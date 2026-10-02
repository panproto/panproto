#![allow(clippy::unwrap_used, clippy::expect_used)]
//! A hyper-edge label rename reaches the attribute arrow the label names:
//! `compile` lowers it into an ordinary edge remap, and refuses a label the
//! arrows cannot carry. The signature check reads `label_map` by the source
//! label.

use std::collections::HashMap;

use panproto_gat::Name;
use panproto_mig::{ExistenceError, Migration, check_existence, compile};
use panproto_schema::{Edge, Protocol, Schema, SchemaBuilder};

/// A schema with table `T`, value vertices `values`, one `prop` edge from
/// `T` named by each of `columns` to its vertex, and a hyper-edge `H` over
/// `T` (parent label `parent`) and the `labels`.
fn table(
    hyper_edge: &str,
    columns: &[(&str, &str)],
    labels: &[(&str, &str)],
    extra_edges: &[(&str, &str, &str)],
) -> Schema {
    let protocol = Protocol::default();
    let mut builder = SchemaBuilder::new(&protocol)
        .vertex("T", "record", None)
        .unwrap();
    let mut vertices: Vec<&str> = columns.iter().map(|(_, v)| *v).collect();
    vertices.extend(labels.iter().map(|(_, v)| *v));
    vertices.sort_unstable();
    vertices.dedup();
    for v in vertices {
        builder = builder.vertex(v, "string", None).unwrap();
    }
    for (name, vertex) in columns {
        builder = builder.edge("T", vertex, "prop", Some(name)).unwrap();
    }
    for (src, tgt, name) in extra_edges {
        builder = builder.edge(src, tgt, "prop", Some(name)).unwrap();
    }
    let mut signature: HashMap<String, String> = labels
        .iter()
        .map(|(l, v)| ((*l).to_owned(), (*v).to_owned()))
        .collect();
    signature.insert("parent".to_owned(), "T".to_owned());
    builder
        .hyper_edge(hyper_edge, "row", signature, "parent")
        .unwrap()
        .build()
        .unwrap()
}

fn prop(src: &str, tgt: &str, name: &str) -> Edge {
    Edge {
        src: src.into(),
        tgt: tgt.into(),
        kind: "prop".into(),
        name: Some(name.into()),
    }
}

fn migration(vertices: &[(&str, &str)], labels: &[(&str, &str)]) -> Migration {
    let mut m = Migration::empty();
    for (s, t) in vertices {
        m.vertex_map.insert((*s).into(), (*t).into());
    }
    m.hyper_edge_map.insert("H".into(), "H2".into());
    for (s, t) in labels {
        m.label_map
            .insert((Name::from("H"), Name::from(*s)), Name::from(*t));
    }
    m
}

fn mismatch(error: ExistenceError, label: &str) {
    match error {
        ExistenceError::LabelArrowMismatch {
            hyper_edge,
            label: l,
        } => {
            assert_eq!(hyper_edge, "H");
            assert_eq!(l, label);
        }
        other => panic!("expected LabelArrowMismatch on {label}, got {other:?}"),
    }
}

#[test]
fn compile_lowers_a_label_rename_onto_its_attribute_arrow() {
    let src = table("H", &[("a", "c")], &[("a", "c")], &[]);
    let tgt = table("H2", &[("b", "c")], &[("b", "c")], &[]);
    let m = migration(&[("T", "T"), ("c", "c")], &[("a", "b")]);
    let compiled = compile(&src, &tgt, &m).unwrap();
    assert_eq!(
        compiled.edge_remap.get(&prop("T", "c", "a")),
        Some(&prop("T", "c", "b"))
    );
}

#[test]
fn a_label_arrow_mismatch_is_refused_when_the_rename_names_no_target_label() {
    let src = table("H", &[("a", "c")], &[("a", "c")], &[]);
    let tgt = table("H2", &[("b", "c")], &[("b", "c")], &[]);
    let m = migration(&[("T", "T"), ("c", "c")], &[("a", "zzz")]);
    mismatch(compile(&src, &tgt, &m).unwrap_err(), "a");
}

#[test]
fn a_label_arrow_mismatch_is_refused_when_the_target_label_has_no_arrow() {
    let src = table("H", &[("a", "c")], &[("a", "c")], &[]);
    let tgt = table("H2", &[], &[("b", "c")], &[]);
    let m = migration(&[("T", "T"), ("c", "c")], &[("a", "b")]);
    mismatch(compile(&src, &tgt, &m).unwrap_err(), "a");
}

#[test]
fn a_label_arrow_mismatch_is_refused_when_the_edge_map_sends_the_arrow_elsewhere() {
    let src = table("H", &[("a", "c")], &[("a", "c")], &[]);
    let tgt = table("H2", &[("b", "c"), ("d", "c")], &[("b", "c")], &[]);
    let mut m = migration(&[("T", "T"), ("c", "c")], &[("a", "b")]);
    m.edge_map.insert(prop("T", "c", "a"), prop("T", "c", "d"));
    mismatch(compile(&src, &tgt, &m).unwrap_err(), "a");
}

#[test]
fn a_label_arrow_mismatch_is_refused_when_a_removed_label_keeps_its_image() {
    let src = table("H", &[("a", "c")], &[("a", "c")], &[]);
    let tgt = table("H2", &[("a", "c")], &[], &[]);
    let m = migration(&[("T", "T"), ("c", "c")], &[]);
    mismatch(compile(&src, &tgt, &m).unwrap_err(), "a");
}

#[test]
fn a_removed_label_without_a_same_named_image_is_lowered_to_nothing() {
    let src = table("H", &[("a", "c")], &[("a", "c")], &[]);
    let tgt = table("H2", &[], &[], &[]);
    let m = migration(&[("T", "T")], &[]);
    let compiled = compile(&src, &tgt, &m).unwrap();
    assert!(compiled.edge_remap.is_empty());
}

#[test]
fn signature_coherence_reads_label_map_by_source_label() {
    // H = {parent: T, a: c1}, H2 = {parent: T, b: c2}; label a is renamed
    // to b, but c1 is sent to c9, not to c2.
    let src = table("H", &[("z", "c3")], &[("a", "c1")], &[]);
    let tgt = table("H2", &[("x", "c9")], &[("b", "c2")], &[]);
    let mut m = migration(&[("T", "T"), ("c1", "c9")], &[("a", "b")]);
    m.vertex_map.insert("c3".into(), "c2".into());
    // A schema theory with the hyper-edge sort, so the signature check runs.
    let protocol = Protocol {
        schema_theory: "ThHyper".into(),
        ..Protocol::default()
    };
    let registry = HashMap::from([(
        "ThHyper".to_owned(),
        panproto_gat::Theory::new(
            "ThHyper",
            vec![
                panproto_gat::Sort::simple("Vertex"),
                panproto_gat::Sort::simple("HyperEdge"),
            ],
            vec![],
            vec![],
        ),
    )]);
    let report = check_existence(&protocol, &src, &tgt, &m, &registry);
    assert!(
        report.errors.iter().any(|e| matches!(
            e,
            ExistenceError::SignatureCoherence { hyper_edge, label }
                if hyper_edge == "H2" && label == "b"
        )),
        "{:?}",
        report.errors
    );
}

#[test]
fn a_mapped_hyper_edge_without_a_label_rename_compiles_as_if_unmapped() {
    // H and H2 hold the same label `a` over the same arrow: the direct image
    // of the arrow is already its target arrow, so nothing is lowered and
    // the compiled migration, and with it the lens digest, is the one of
    // the migration that maps no hyper-edge.
    let src = table("H", &[("a", "c1")], &[("a", "c1")], &[]);
    let tgt = table("H2", &[("a", "c1")], &[("a", "c1")], &[]);
    let mapped = migration(&[("T", "T"), ("c1", "c1")], &[]);
    let mut unmapped = mapped.clone();
    unmapped.hyper_edge_map.clear();
    let with_map = compile(&src, &tgt, &mapped).unwrap();
    let without_map = compile(&src, &tgt, &unmapped).unwrap();
    assert!(with_map.edge_remap.is_empty(), "{:?}", with_map.edge_remap);
    assert_eq!(with_map.surviving_edges, without_map.surviving_edges);
    assert_eq!(
        panproto_inst::identity::lens_digest(&src, &tgt, &with_map),
        panproto_inst::identity::lens_digest(&src, &tgt, &without_map)
    );
}

#[test]
fn only_the_renamed_labels_of_a_hyper_edge_are_lowered() {
    // Of the labels a and k of H, only a is renamed (to b): its arrow gains
    // an edge remap, while k, whose arrow keeps its direct image, gains none,
    // and the digest differs from the unmapped migration's.
    let src = table(
        "H",
        &[("a", "c"), ("k", "d")],
        &[("a", "c"), ("k", "d")],
        &[],
    );
    let tgt = table(
        "H2",
        &[("b", "c"), ("k", "d")],
        &[("b", "c"), ("k", "d")],
        &[],
    );
    let m = migration(&[("T", "T"), ("c", "c"), ("d", "d")], &[("a", "b")]);
    let compiled = compile(&src, &tgt, &m).unwrap();
    assert_eq!(
        compiled.edge_remap,
        HashMap::from([(prop("T", "c", "a"), prop("T", "c", "b"))])
    );
    let mut unmapped = m.clone();
    unmapped.hyper_edge_map.clear();
    unmapped.label_map.clear();
    let plain = compile(&src, &tgt, &unmapped).unwrap();
    assert_ne!(
        panproto_inst::identity::lens_digest(&src, &tgt, &compiled),
        panproto_inst::identity::lens_digest(&src, &tgt, &plain)
    );
}
