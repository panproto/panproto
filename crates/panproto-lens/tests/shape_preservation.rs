#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Static shape predicates decline exactly the structural obstructions.

use std::collections::{BTreeSet, HashMap};

use panproto_inst::CompiledMigration;
use panproto_lens::Lens;
use panproto_schema::{
    Cardinality, Edge, OnUnresolved, PathSyntax, Protocol, RefSource, Reference, ResolveMode,
    Schema, SchemaBuilder,
};

fn protocol() -> Protocol {
    Protocol {
        non_value_kinds: BTreeSet::from(["record".to_owned()]),
        ..Protocol::default()
    }
}

fn edge(src: &str, tgt: &str, name: &str) -> Edge {
    Edge {
        src: src.into(),
        tgt: tgt.into(),
        kind: "fk".into(),
        name: Some(name.into()),
    }
}

fn schema(vertices: &[&str], edges: &[Edge]) -> Schema {
    let protocol = protocol();
    let mut builder = SchemaBuilder::new(&protocol);
    for vertex in vertices {
        builder = builder.vertex(vertex, "record", None).unwrap();
    }
    for edge in edges {
        builder = builder
            .edge(
                edge.src.as_str(),
                edge.tgt.as_str(),
                edge.kind.as_str(),
                edge.name.as_deref(),
            )
            .unwrap();
    }
    builder.build().unwrap()
}

#[test]
fn a_dropped_edge_between_survivors_breaks_tree_shape() {
    let source_edge = edge("a", "b", "r");
    let lens = Lens {
        src_schema: schema(&["a", "b"], std::slice::from_ref(&source_edge)),
        tgt_schema: schema(&["a", "b"], &[]),
        compiled: CompiledMigration {
            surviving_verts: ["a".into(), "b".into()].into(),
            ..CompiledMigration::default()
        },
    };
    assert!(!lens.preserves_tree_shape());
}

#[test]
fn contraction_alone_preserves_tree_shape() {
    let left = edge("r", "m", "left");
    let right = edge("m", "v", "right");
    let joined = edge("r", "v", "joined");
    let lens = Lens {
        src_schema: schema(&["r", "m", "v"], &[left, right]),
        tgt_schema: schema(&["r", "v"], std::slice::from_ref(&joined)),
        compiled: CompiledMigration {
            surviving_verts: ["r".into(), "v".into()].into(),
            resolver: std::iter::once((('r'.to_string().into(), "v".into()), joined)).collect(),
            ..CompiledMigration::default()
        },
    };
    assert!(lens.preserves_tree_shape());
}

#[test]
fn expanding_and_hyper_resolving_lenses_do_not_preserve_table_shape() {
    let source_edge = edge("a", "b", "r");
    let first = edge("a", "m", "first");
    let second = edge("m", "b", "second");
    let expanding = Lens {
        src_schema: schema(&["a", "b"], std::slice::from_ref(&source_edge)),
        tgt_schema: schema(&["a", "m", "b"], &[first, second]),
        compiled: CompiledMigration {
            surviving_verts: ["a".into(), "m".into(), "b".into()].into(),
            expansion_path: std::iter::once((
                ('a'.to_string().into(), "b".into()),
                vec!["m".into()],
            ))
            .collect(),
            ..CompiledMigration::default()
        },
    };
    assert!(!expanding.preserves_table_shape());

    let identity_schema = schema(&["a"], &[]);
    let hyper = Lens {
        src_schema: identity_schema.clone(),
        tgt_schema: identity_schema,
        compiled: CompiledMigration {
            surviving_verts: ["a".into()].into(),
            hyper_resolver: std::iter::once((("h".into(), vec![]), ("h2".into(), HashMap::new())))
                .collect(),
            ..CompiledMigration::default()
        },
    };
    assert!(!hyper.preserves_table_shape());
}

#[test]
fn a_parallel_edge_merge_does_not_preserve_table_shape() {
    let left = edge("a", "b", "left");
    let right = edge("a", "b", "right");
    let image = edge("A", "B", "joined");
    let lens = Lens {
        src_schema: schema(&["a", "b"], &[left.clone(), right.clone()]),
        tgt_schema: schema(&["A", "B"], std::slice::from_ref(&image)),
        compiled: CompiledMigration {
            surviving_verts: ["A".into(), "B".into()].into(),
            vertex_remap: [("a".into(), "A".into()), ("b".into(), "B".into())]
                .into_iter()
                .collect(),
            edge_remap: [(left, image.clone()), (right, image)]
                .into_iter()
                .collect(),
            ..CompiledMigration::default()
        },
    };
    assert!(!lens.preserves_table_shape());
}

#[test]
fn a_non_opfibration_merge_does_not_preserve_table_shape() {
    let lifted = edge("a", "z", "r");
    let required = edge("m", "z", "r");
    let mut target = schema(&["m", "z"], std::slice::from_ref(&required));
    target.required.insert("m".into(), vec![required.clone()]);
    let lens = Lens {
        src_schema: schema(&["a", "b", "z"], std::slice::from_ref(&lifted)),
        tgt_schema: target,
        compiled: CompiledMigration {
            surviving_verts: ["m".into(), "z".into()].into(),
            vertex_remap: [
                ("a".into(), "m".into()),
                ("b".into(), "m".into()),
                ("z".into(), "z".into()),
            ]
            .into_iter()
            .collect(),
            edge_remap: std::iter::once((lifted, required)).collect(),
            ..CompiledMigration::default()
        },
    };
    assert!(!lens.preserves_table_shape());
}

#[test]
fn an_explicit_vertex_merge_without_guarded_edges_preserves_table_shape() {
    let lens = Lens {
        src_schema: schema(&["a", "b"], &[]),
        tgt_schema: schema(&["m"], &[]),
        compiled: CompiledMigration {
            surviving_verts: ["m".into()].into(),
            vertex_remap: [("a".into(), "m".into()), ("b".into(), "m".into())]
                .into_iter()
                .collect(),
            ..CompiledMigration::default()
        },
    };
    assert!(lens.preserves_table_shape());
}

#[test]
fn an_identity_lens_preserves_table_shape() {
    let identity = schema(&["a"], &[]);
    let lens = Lens {
        src_schema: identity.clone(),
        tgt_schema: identity,
        compiled: CompiledMigration {
            surviving_verts: ["a".into()].into(),
            ..CompiledMigration::default()
        },
    };

    assert!(lens.preserves_table_shape());
}

#[test]
fn an_implicit_vertex_collision_does_not_preserve_table_shape() {
    let lens = Lens {
        src_schema: schema(&["a", "m"], &[]),
        tgt_schema: schema(&["m"], &[]),
        compiled: CompiledMigration {
            surviving_verts: ["m".into()].into(),
            vertex_remap: std::iter::once(("a".into(), "m".into())).collect(),
            ..CompiledMigration::default()
        },
    };

    assert!(!lens.preserves_table_shape());
}

#[test]
fn an_explicit_self_remap_keeps_an_explicit_vertex_merge_table_shaped() {
    let lens = Lens {
        src_schema: schema(&["a", "m"], &[]),
        tgt_schema: schema(&["m"], &[]),
        compiled: CompiledMigration {
            surviving_verts: ["m".into()].into(),
            vertex_remap: [("a".into(), "m".into()), ("m".into(), "m".into())]
                .into_iter()
                .collect(),
            ..CompiledMigration::default()
        },
    };

    assert!(lens.preserves_table_shape());
}

#[test]
fn a_guarded_fk_has_exactly_one_lift_only_over_its_source_fibre() {
    let guarded = edge("x", "y", "guarded");
    let source = schema(&["a", "x", "y"], std::slice::from_ref(&guarded));
    let mut target = source.clone();
    target.required.insert("x".into(), vec![guarded.clone()]);
    let lens = Lens {
        src_schema: source,
        tgt_schema: target,
        compiled: CompiledMigration {
            surviving_verts: ["a".into(), "x".into(), "y".into()].into(),
            surviving_edges: [guarded].into(),
            ..CompiledMigration::default()
        },
    };

    assert!(lens.preserves_table_shape());
}

#[test]
fn a_nonnullable_many_reference_does_not_guard_an_fk_lift() {
    let lifted = edge("a", "z", "r");
    let target_edge = edge("m", "z", "r");
    let mut target = schema(&["m", "z"], std::slice::from_ref(&target_edge));
    target.declarations.references.insert(
        "many".into(),
        Reference {
            id: "many".into(),
            holder: "m".into(),
            source: RefSource::Path {
                field: Vec::new(),
                syntax: PathSyntax::IdRef,
            },
            target_vertex: "z".into(),
            target_key: None,
            edge: Some(target_edge.clone()),
            cardinality: Cardinality::Many,
            nullable: false,
            on_unresolved: OnUnresolved::Error,
            mode: ResolveMode::AddEdge,
            guard: None,
            null_match: panproto_schema::NullMatch::default(),
            null_vertex: None,
            scope_via: None,
        },
    );
    let lens = Lens {
        src_schema: schema(&["a", "b", "z"], std::slice::from_ref(&lifted)),
        tgt_schema: target,
        compiled: CompiledMigration {
            surviving_verts: ["m".into(), "z".into()].into(),
            vertex_remap: [
                ("a".into(), "m".into()),
                ("b".into(), "m".into()),
                ("z".into(), "z".into()),
            ]
            .into_iter()
            .collect(),
            edge_remap: std::iter::once((lifted, target_edge)).collect(),
            ..CompiledMigration::default()
        },
    };

    assert!(lens.preserves_table_shape());
}
