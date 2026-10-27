#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]
//! Whole-state edit-oracle differentials for a small named projection carrier.

use std::collections::{BTreeMap, HashMap};

use panproto_inst::{
    CompiledMigration, Complement, GEdge, GInstance, GraphDelta, Node, NodeShape, TreeEdit,
    TreeEmbedding, WInstance,
};
use panproto_lens::{EditLens, Lens, LensShape, TreeShape};
use panproto_reference::edit as reference;
use panproto_reference::enumerate::edits as carrier;
use panproto_reference::model as reference_model;
use panproto_schema::{Edge, Protocol, Schema, SchemaBuilder};

const ROOT: u32 = 0;
const VISIBLE: u32 = 1;
const HIDDEN: u32 = 2;

#[derive(Clone, Debug, PartialEq)]
struct ComplementObs {
    nodes: BTreeMap<u32, reference_model::Node>,
    edges: BTreeMap<u32, reference_model::GEdge>,
}

fn edge(anchor: &str) -> Edge {
    Edge {
        src: "root".into(),
        tgt: anchor.into(),
        kind: "prop".into(),
        name: Some(anchor.into()),
    }
}

fn source() -> WInstance {
    WInstance::new(
        HashMap::from([
            (ROOT, Node::new(ROOT, "root")),
            (VISIBLE, Node::new(VISIBLE, "visible")),
            (HIDDEN, Node::new(HIDDEN, "hidden")),
        ]),
        vec![
            (ROOT, VISIBLE, edge("visible")),
            (ROOT, HIDDEN, edge("hidden")),
        ],
        Vec::new(),
        ROOT,
        "root".into(),
    )
}

fn schema(include_hidden: bool) -> Schema {
    let protocol = Protocol::default();
    let mut builder = SchemaBuilder::new(&protocol)
        .vertex("root", "record", None::<&str>)
        .unwrap()
        .vertex("visible", "scalar", None::<&str>)
        .unwrap()
        .edge("root", "visible", "prop", Some("visible"))
        .unwrap();
    if include_hidden {
        builder = builder
            .vertex("hidden", "scalar", None::<&str>)
            .unwrap()
            .edge("root", "hidden", "prop", Some("hidden"))
            .unwrap();
    }
    builder.entry("root").build().unwrap()
}

fn state_lens() -> Lens {
    let src_schema = schema(true);
    let tgt_schema = schema(false);
    let compiled = CompiledMigration {
        surviving_verts: tgt_schema.vertices.keys().cloned().collect(),
        surviving_edges: tgt_schema.edges.keys().cloned().collect(),
        ..CompiledMigration::default()
    };
    Lens {
        compiled,
        src_schema,
        tgt_schema,
    }
}

fn edit_lens(source: &WInstance) -> EditLens {
    let mut lens = EditLens::from_lens(state_lens(), Protocol::default());
    lens.initialize(source).unwrap();
    lens
}

fn embedding(tree: &WInstance) -> TreeEmbedding {
    GInstance::from_wtype_embedded(tree, None).unwrap().1
}

fn lower(tree: &WInstance, embedding: &TreeEmbedding, edit: &TreeEdit) -> GraphDelta {
    let graph = GInstance::from_wtype_under(tree, embedding, None).unwrap();
    let mut next_embedding = embedding.clone();
    GraphDelta::from_tree_edit(&graph, &mut next_embedding, edit).unwrap()
}

fn node_to_reference(node: &Node) -> reference_model::Node {
    assert!(node.value.is_none());
    assert!(node.discriminator.is_none());
    assert!(node.extra_fields.is_empty());
    assert!(node.position.is_none());
    assert_eq!(node.shape, NodeShape::Plain);
    assert!(node.annotations.is_empty());
    reference_model::Node::new(node.id, node.anchor.as_ref())
}

fn edge_to_reference(edge: &GEdge) -> reference_model::GEdge {
    reference_model::GEdge {
        src: edge.src,
        tgt: edge.tgt,
        label: reference_model::SEdge::new(
            edge.edge.src.as_ref(),
            edge.edge.tgt.as_ref(),
            edge.edge.kind.as_ref(),
            edge.edge.name.as_deref(),
        ),
        position: edge.position,
    }
}

fn graph_to_reference(graph: &GInstance) -> reference_model::Graph {
    reference_model::Graph {
        nodes: graph
            .nodes()
            .iter()
            .map(|(id, node)| (*id, node_to_reference(node)))
            .collect(),
        edges: graph
            .edges_sorted()
            .map(|(id, edge)| (id.0, edge_to_reference(edge)))
            .collect(),
    }
}

fn delta_to_reference(delta: &GraphDelta) -> reference::GraphDelta {
    assert!(delta.attrs().is_empty());
    let mut out = reference::GraphDelta::identity();
    out.del.extend(delta.del_nodes().iter().map(|(id, node)| {
        (
            reference::GElem::Node(*id),
            reference::GPayload::Node(node_to_reference(node)),
        )
    }));
    out.del.extend(delta.del_edges().iter().map(|(id, edge)| {
        (
            reference::GElem::Edge(id.0),
            reference::GPayload::Edge(edge_to_reference(edge)),
        )
    }));
    out.add.extend(delta.add_nodes().iter().map(|(id, node)| {
        (
            reference::GElem::Node(*id),
            reference::GPayload::Node(node_to_reference(node)),
        )
    }));
    out.add.extend(delta.add_edges().iter().map(|(id, edge)| {
        (
            reference::GElem::Edge(id.0),
            reference::GPayload::Edge(edge_to_reference(edge)),
        )
    }));
    out
}

fn project(graph: &reference_model::Graph) -> (reference_model::Graph, ComplementObs) {
    let retained: BTreeMap<_, _> = graph
        .nodes
        .iter()
        .filter(|(_, node)| node.anchor != "hidden")
        .map(|(id, node)| (*id, node.clone()))
        .collect();
    let (retained_edges, dropped_edges): (BTreeMap<_, _>, BTreeMap<_, _>) = graph
        .edges
        .iter()
        .map(|(id, edge)| (*id, edge.clone()))
        .partition(|(_, edge)| {
            retained.contains_key(&edge.src) && retained.contains_key(&edge.tgt)
        });
    let dropped_nodes = graph
        .nodes
        .iter()
        .filter(|(id, _)| !retained.contains_key(id))
        .map(|(id, node)| (*id, node.clone()))
        .collect();
    (
        reference_model::Graph {
            nodes: retained,
            edges: retained_edges,
        },
        ComplementObs {
            nodes: dropped_nodes,
            edges: dropped_edges,
        },
    )
}

fn restore(view: &reference_model::Graph, complement: &ComplementObs) -> reference_model::Graph {
    let mut source = view.clone();
    source.nodes.extend(complement.nodes.clone());
    source.edges.extend(complement.edges.clone());
    source
}

fn complement_to_reference(
    complement: &Complement,
    source_embedding: &TreeEmbedding,
) -> ComplementObs {
    assert!(complement.dropped_fans.is_empty());
    assert!(complement.contraction_choices.is_empty());
    let nodes = complement
        .dropped_nodes
        .iter()
        .map(|(id, node)| (*id, node_to_reference(node)))
        .collect();
    let edges = complement
        .dropped_arcs
        .iter()
        .map(|(parent, child, label)| {
            let id = source_embedding.arcs.get(child).expect("dropped arc id");
            (
                id.0,
                reference_model::GEdge {
                    src: *parent,
                    tgt: *child,
                    label: reference_model::SEdge::new(
                        label.src.as_ref(),
                        label.tgt.as_ref(),
                        label.kind.as_ref(),
                        label.name.as_deref(),
                    ),
                    position: None,
                },
            )
        })
        .collect();
    ComplementObs { nodes, edges }
}

fn edit_to_production(edit: &reference::TreeEdit) -> TreeEdit {
    match edit {
        reference::TreeEdit::Identity => TreeEdit::Identity,
        reference::TreeEdit::DeleteNode { id } => TreeEdit::DeleteNode { id: *id },
        other => panic!("edit outside the translation carrier: {other:?}"),
    }
}

struct TranslationFixture {
    source: WInstance,
    source_embedding: TreeEmbedding,
    reference_source: reference_model::Graph,
    lens: Lens,
    view: WInstance,
    view_embedding: TreeEmbedding,
    reference_view: reference_model::Graph,
    reference_complement: ComplementObs,
}

fn translation_fixture() -> TranslationFixture {
    let source = source();
    let source_embedding = embedding(&source);
    let source_graph = GInstance::from_wtype_under(&source, &source_embedding, None).unwrap();
    let reference_source = graph_to_reference(&source_graph);
    let lens = state_lens();
    let (view, initial_complement) = panproto_lens::get(&lens, &source).unwrap();
    let view_embedding = embedding(&view);
    let view_graph = GInstance::from_wtype_under(&view, &view_embedding, None).unwrap();
    let reference_view = graph_to_reference(&view_graph);
    let (expected_view, reference_complement) = project(&reference_source);
    assert_eq!(reference_view, expected_view);
    assert_eq!(
        complement_to_reference(&initial_complement, &source_embedding),
        reference_complement
    );
    TranslationFixture {
        source,
        source_embedding,
        reference_source,
        lens,
        view,
        view_embedding,
        reference_view,
        reference_complement,
    }
}

fn assert_get_case(fixture: &TranslationFixture, case: &carrier::EditTranslationCase) {
    let name = case.name;
    let edit = edit_to_production(&case.edit);
    let source_delta = lower(&fixture.source, &fixture.source_embedding, &edit);
    let reference_source_delta = delta_to_reference(&source_delta);
    let expected = panproto_reference::edit::get_edit(
        &|state| Ok::<_, &'static str>(project(state)),
        &fixture.reference_source,
        &reference_source_delta,
    )
    .unwrap();

    let mut graph_lens = edit_lens(&fixture.source);
    let graph_actual = graph_lens
        .get_graph_edit(
            &fixture.source,
            &fixture.source_embedding,
            &fixture.view,
            &fixture.view_embedding,
            &source_delta,
        )
        .unwrap_or_else(|error| panic!("graph get translation {name}: {error:?}"));
    assert_eq!(delta_to_reference(&graph_actual), expected.delta, "{name}");
    assert_eq!(
        complement_to_reference(&graph_lens.complement, &fixture.source_embedding),
        expected.complement,
        "graph complement: {name}"
    );

    let mut tree_lens = edit_lens(&fixture.source);
    let tree_actual = tree_lens
        .get_edit(edit.clone())
        .unwrap_or_else(|error| panic!("tree get translation {name}: {error:?}"));
    assert_eq!(
        delta_to_reference(&lower(&fixture.view, &fixture.view_embedding, &tree_actual,)),
        expected.delta,
        "tree lowering: {name}"
    );
    assert_eq!(
        complement_to_reference(&tree_lens.complement, &fixture.source_embedding),
        expected.complement,
        "tree complement: {name}"
    );

    let mut edited_source = fixture.source.clone();
    edit.apply(&mut edited_source).unwrap();
    let (_, whole_complement) = panproto_lens::get(&fixture.lens, &edited_source).unwrap();
    assert!(
        <TreeShape as LensShape>::complement_divergence(&graph_lens.complement, &whole_complement)
            .is_none(),
        "whole-state graph complement: {name}"
    );
    let tree_complement_divergence =
        <TreeShape as LensShape>::complement_divergence(&tree_lens.complement, &whole_complement);
    assert!(
        tree_complement_divergence.is_none(),
        "whole-state tree complement: {name}: {tree_complement_divergence:?}\n\
         incremental={:#?}\nwhole-state={whole_complement:#?}",
        tree_lens.complement,
    );
}

fn assert_put_case(fixture: &TranslationFixture, case: &carrier::EditTranslationCase) {
    let name = case.name;
    let edit = edit_to_production(&case.edit);
    let view_delta = lower(&fixture.view, &fixture.view_embedding, &edit);
    let reference_view_delta = delta_to_reference(&view_delta);
    let expected = panproto_reference::edit::put_edit(
        &|_, edited_view, complement| {
            Ok::<_, &'static str>((restore(edited_view, complement), complement.clone()))
        },
        &fixture.reference_source,
        &fixture.reference_view,
        &fixture.reference_complement,
        &reference_view_delta,
    )
    .unwrap();
    let edited_reference_view =
        panproto_reference::edit::apply(&reference_view_delta, &fixture.reference_view).unwrap();
    assert_eq!(
        expected.delta,
        panproto_reference::edit::diff(
            &fixture.reference_source,
            &restore(&edited_reference_view, &fixture.reference_complement)
        ),
        "explicit differ: {name}"
    );

    let mut production = edit_lens(&fixture.source);
    let translated = production
        .put_edit(edit)
        .unwrap_or_else(|error| panic!("tree put translation {name}: {error:?}"));
    let actual = lower(&fixture.source, &fixture.source_embedding, &translated);
    assert_eq!(delta_to_reference(&actual), expected.delta, "{name}");
    assert_eq!(
        complement_to_reference(&production.complement, &fixture.source_embedding),
        expected.complement,
        "put complement: {name}"
    );
}

#[test]
fn generic_whole_state_edit_oracles_match_actual_tree_lens_translation() {
    let fixture = translation_fixture();
    let cases = carrier::edit_translation_cases();
    assert_eq!(
        u64::try_from(cases.len()).unwrap(),
        carrier::edit_translation_cases_count_certificate()
    );
    for case in &cases {
        match case.direction {
            carrier::EditTranslationDirection::Get => assert_get_case(&fixture, case),
            carrier::EditTranslationDirection::Put => assert_put_case(&fixture, case),
        }
    }
}
