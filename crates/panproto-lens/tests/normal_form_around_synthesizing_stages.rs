//! Normal-form folds with a fusible group between synthesizing stages.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;

use panproto_inst::{CompiledMigration, GInstance, Node, SynthIndex, Value, WInstance};
use panproto_lens::lens_shape::{LensShape, TreeShape};
use panproto_lens::{Lens, LensPath};
use panproto_schema::{Edge, Protocol, Schema, SchemaBuilder};
use proptest::prelude::*;

fn edge(src: &str, tgt: &str, name: &str) -> Edge {
    Edge {
        src: src.into(),
        tgt: tgt.into(),
        kind: "rel".into(),
        name: Some(name.into()),
    }
}

fn schema(vertices: &[&str], edges: &[Edge]) -> Schema {
    let protocol = Protocol::default();
    let mut builder = SchemaBuilder::new(&protocol);
    for vertex in vertices {
        builder = builder.vertex(vertex, "record", None).unwrap();
    }
    for edge in edges {
        builder = builder
            .edge(&edge.src, &edge.tgt, &edge.kind, edge.name.as_deref())
            .unwrap();
    }
    builder.build().unwrap()
}

fn identity(schema: &Schema) -> Lens {
    Lens {
        compiled: CompiledMigration {
            surviving_verts: schema.vertices.keys().cloned().collect(),
            surviving_edges: schema.edges.keys().cloned().collect(),
            ..CompiledMigration::default()
        },
        src_schema: schema.clone(),
        tgt_schema: schema.clone(),
    }
}

fn synthesizing_path() -> LensPath {
    let am = edge("a", "m", "m");
    let mb = edge("m", "b", "b");
    let ab = edge("a", "b", "b");
    let an = edge("a", "n", "n");
    let nb = edge("n", "b", "b");
    let source = schema(&["a", "m", "b"], &[am, mb]);
    let middle = schema(&["a", "b"], std::slice::from_ref(&ab));
    let target = schema(&["a", "n", "b"], &[an, nb]);
    let contraction = Lens {
        compiled: CompiledMigration {
            surviving_verts: ["a".into(), "b".into()].into_iter().collect(),
            resolver: HashMap::from([(("a".into(), "b".into()), ab)]),
            ..CompiledMigration::default()
        },
        src_schema: source,
        tgt_schema: middle.clone(),
    };
    let expansion = Lens {
        compiled: CompiledMigration {
            surviving_verts: ["a".into(), "b".into()].into_iter().collect(),
            expansion_path: HashMap::from([(("a".into(), "b".into()), vec!["n".into()])]),
            ..CompiledMigration::default()
        },
        src_schema: middle.clone(),
        tgt_schema: target,
    };
    LensPath::single(contraction)
        .then(&LensPath::single(identity(&middle)))
        .unwrap()
        .then(&LensPath::single(identity(&middle)))
        .unwrap()
        .then(&LensPath::single(expansion))
        .unwrap()
}

fn source(value: i16) -> WInstance {
    WInstance::new(
        HashMap::from([
            (0, Node::new(0, "a")),
            (1, Node::new(1, "m")),
            (
                2,
                Node::new(2, "b").with_value(panproto_inst::FieldPresence::Present(Value::Int(
                    i64::from(value),
                ))),
            ),
        ]),
        vec![(0, 1, edge("a", "m", "m")), (1, 2, edge("m", "b", "b"))],
        vec![],
        0,
        "a".into(),
    )
}

proptest! {
    #[test]
    fn normal_form_agrees_with_the_folds_around_synthesizing_stages(value in any::<i16>()) {
        let path = synthesizing_path();
        assert_eq!(path.tree_normal_form().iter().map(|group| group.stages.clone()).collect::<Vec<_>>(), vec![0..1, 1..3, 3..4]);
        assert_eq!(path.graph_normal_form().iter().map(|group| group.stages.clone()).collect::<Vec<_>>(), vec![0..1, 1..3, 3..4]);

        let tree_source = source(value);
        let mut atomic_tree_view = tree_source.clone();
        let mut tree_complements = Vec::new();
        for stage in path.stages() {
            let (next, complement) = panproto_lens::get(stage, &atomic_tree_view).unwrap();
            atomic_tree_view = next;
            tree_complements.push(complement);
        }
        let mut atomic_tree_source = atomic_tree_view.clone();
        for (stage, complement) in path.stages().iter().zip(&tree_complements).rev() {
            atomic_tree_source = panproto_lens::put(stage, &atomic_tree_source, complement).unwrap();
        }
        let (normal_tree_view, normal_tree_complement) = path.get(&tree_source).unwrap();
        let normal_tree_source = path.put(&normal_tree_view, &normal_tree_complement).unwrap();
        let empty_index = SynthIndex::default();
        prop_assert!(TreeShape::equivalent(
            &atomic_tree_view,
            &empty_index,
            &normal_tree_view,
            &empty_index,
        ));
        prop_assert!(TreeShape::equivalent(
            &atomic_tree_source,
            &empty_index,
            &normal_tree_source,
            &empty_index,
        ));
        prop_assert!(TreeShape::equivalent(
            &tree_source,
            &empty_index,
            &normal_tree_source,
            &empty_index,
        ));

        let graph_source = GInstance::from_wtype_with_schema(&tree_source, path.src_schema()).unwrap();
        let mut atomic_graph_view = graph_source.clone();
        let mut graph_complements = Vec::new();
        for stage in path.stages() {
            let (next, complement) = panproto_lens::graph_lens::get(stage, &atomic_graph_view).unwrap();
            atomic_graph_view = next;
            graph_complements.push(complement);
        }
        let mut atomic_graph_source = atomic_graph_view.clone();
        for (stage, complement) in path.stages().iter().zip(&graph_complements).rev() {
            atomic_graph_source = panproto_lens::graph_lens::put(stage, &atomic_graph_source, complement).unwrap();
        }
        let (normal_graph_view, normal_graph_complement) = path.get_graph(&graph_source).unwrap();
        let normal_graph_source = path.put_graph(&normal_graph_view, &normal_graph_complement).unwrap();
        prop_assert!(GInstance::identical(&atomic_graph_view, &normal_graph_view));
        prop_assert!(GInstance::identical(&atomic_graph_source, &normal_graph_source));
        prop_assert!(GInstance::identical(&graph_source, &normal_graph_source));

        prop_assert!(normal_graph_view.edges().keys().next().is_some());
    }
}
