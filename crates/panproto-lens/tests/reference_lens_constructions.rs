//! Independent construction and complement-free-put evidence for L-lenses-10.
//!
//! The graph and tree verdicts below are computed from the public structural
//! carriers. They do not call a production classifier, get, or complement.
//! The table verdicts and outputs come from the page-owned reference table
//! lens, whose implementation is independent of `panproto-lens`.

#![allow(clippy::expect_used, clippy::too_many_lines, clippy::unwrap_used)]

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::OnceLock;

use panproto_expr::{BuiltinOp, Expr, Literal};
use panproto_gat::{CoercionClass, Name};
use panproto_inst::{
    BoundaryProtocols, CompiledMigration, EdgeId, FInstance, Fan, FieldPresence, FieldTransform,
    GEdge, GInstance, Node, NodeShape, PutDomainError, RowId, TermAssignment, TermScope, Value,
    ViewRef, WInstance,
};
use panproto_lens::{Lens, LensError, graph_lens, table_lens};
use panproto_reference::enumerate::lenses::{
    C11_FIELD, C11_VALUE, C11Layer, C11Stage, LensConstructionCase, LensConstructionCaseCensus,
    TABLE_FIXTURE_TABLE_KIND, TABLE_FIXTURE_VALUE_KIND, TableFixture, WRITER_VALUE_KIND,
    WriterFixture, c11_fan_schema, c11_fan_states, c11_graph_states, c11_stages, c11_tree_states,
    for_each_lens_construction_case, for_each_table_fixture, for_each_writer_fixture,
    lens_construction_case_census_certificate, lens_construction_count_certificate,
    table_fixture_count_certificate, writer_count_certificate,
};
use panproto_reference::migration::Migration as RefMigration;
use panproto_reference::model::{
    Fan as RefFan, Graph as RefGraph, Node as RefNode, NodeShape as RefNodeShape, SEdge as RefEdge,
    Table as RefTable, Tree as RefTree,
};
use panproto_reference::restrict::NoValueLayer;
use panproto_reference::schema::Schema as RefSchema;
use panproto_reference::table::{
    TableBoundaries as RefTableBoundaries, TableLens as RefTableLens,
    TablePutError as RefTablePutError,
};
use panproto_reference::value::{Presence as RefPresence, Value as RefValue};
use panproto_schema::{
    ConstraintSemantics, Edge, Key, KeyPart, KeyRole, KeyScope, Protocol, ScalarKind, Schema,
    SchemaBuilder, canonical_bytes,
};
use proptest::prelude::*;

fn prod_edge(edge: &RefEdge) -> Edge {
    Edge {
        src: edge.src.as_str().into(),
        tgt: edge.tgt.as_str().into(),
        kind: edge.kind.as_str().into(),
        name: edge.name.as_deref().map(Into::into),
    }
}

fn prod_value(value: &RefValue) -> Value {
    match value {
        RefValue::Bool(value) => Value::Bool(*value),
        RefValue::Int(value) => Value::Int(*value),
        RefValue::Str(value) => Value::Str(value.clone()),
        RefValue::Null => Value::Null,
        other => panic!("value outside the bounded lens carrier: {other:?}"),
    }
}

fn ref_value(value: &Value) -> RefValue {
    match value {
        Value::Bool(value) => RefValue::Bool(*value),
        Value::Int(value) => RefValue::Int(*value),
        Value::Str(value) => RefValue::Str(value.clone()),
        Value::Null => RefValue::Null,
        other => panic!("value outside the bounded lens carrier: {other:?}"),
    }
}

fn prod_presence(presence: &RefPresence) -> FieldPresence {
    match presence {
        RefPresence::Present(value) => FieldPresence::Present(prod_value(value)),
        RefPresence::Null => FieldPresence::Null,
        RefPresence::Absent => FieldPresence::Absent,
    }
}

fn ref_presence(presence: &FieldPresence) -> RefPresence {
    match presence {
        FieldPresence::Present(value) => RefPresence::Present(ref_value(value)),
        FieldPresence::Null => RefPresence::Null,
        FieldPresence::Absent => RefPresence::Absent,
    }
}

fn prod_shape(shape: &RefNodeShape) -> NodeShape {
    match shape {
        RefNodeShape::Plain => NodeShape::Plain,
        RefNodeShape::List => NodeShape::List,
        RefNodeShape::XmlElement(tag) => NodeShape::XmlElement {
            tag: tag.as_str().into(),
        },
        RefNodeShape::XmlTextSegment => NodeShape::XmlTextSegment,
        RefNodeShape::HyperEdge => NodeShape::HyperEdge,
    }
}

fn ref_shape(shape: &NodeShape) -> RefNodeShape {
    match shape {
        NodeShape::Plain => RefNodeShape::Plain,
        NodeShape::List => RefNodeShape::List,
        NodeShape::XmlElement { tag } => RefNodeShape::XmlElement(tag.to_string()),
        NodeShape::XmlTextSegment => RefNodeShape::XmlTextSegment,
        NodeShape::HyperEdge => RefNodeShape::HyperEdge,
    }
}

fn prod_node(node: &RefNode) -> Node {
    let mut out = Node::new(node.id, node.anchor.as_str()).with_shape(prod_shape(&node.shape));
    out.value = node.value.as_ref().map(prod_presence);
    out.discriminator = node.discriminator.as_deref().map(Into::into);
    out.extra_fields = node
        .fields
        .iter()
        .map(|(key, value)| (key.clone(), prod_value(value)))
        .collect();
    out.position = node.position;
    out.annotations = node
        .annotations
        .iter()
        .map(|(key, value)| (key.clone(), prod_value(value)))
        .collect();
    out
}

fn ref_node(node: &Node) -> RefNode {
    RefNode {
        id: node.id,
        anchor: node.anchor.to_string(),
        value: node.value.as_ref().map(ref_presence),
        discriminator: node.discriminator.as_ref().map(ToString::to_string),
        fields: node
            .extra_fields
            .iter()
            .map(|(key, value)| (key.clone(), ref_value(value)))
            .collect(),
        position: node.position,
        shape: ref_shape(&node.shape),
        annotations: node
            .annotations
            .iter()
            .map(|(key, value)| (key.clone(), ref_value(value)))
            .collect(),
    }
}

fn prod_graph(graph: &RefGraph) -> GInstance {
    let mut out = GInstance::new();
    for node in graph.nodes.values() {
        out.insert_node(prod_node(node))
            .expect("reference graph node ids are distinct");
    }
    for (id, edge) in &graph.edges {
        out.insert_edge(
            EdgeId(*id),
            GEdge::new(edge.src, edge.tgt, prod_edge(&edge.label)).with_position(edge.position),
        )
        .expect("reference graph edges have present endpoints");
    }
    out
}

fn prod_tree(tree: &RefTree) -> WInstance {
    let root_anchor = tree
        .nodes
        .get(&tree.root)
        .map(|node| node.anchor.clone())
        .unwrap_or_default();
    WInstance::new(
        tree.nodes
            .iter()
            .map(|(id, node)| (*id, prod_node(node)))
            .collect(),
        tree.arcs
            .iter()
            .map(|(parent, child, edge)| (*parent, *child, prod_edge(edge)))
            .collect(),
        tree.fans
            .iter()
            .map(|fan| Fan {
                hyper_edge_id: fan.hyper_edge.clone(),
                parent: fan.parent,
                children: fan.children.clone().into_iter().collect(),
            })
            .collect(),
        tree.root,
        root_anchor.into(),
    )
}

fn ref_tree(tree: &WInstance) -> RefTree {
    RefTree {
        nodes: tree
            .nodes
            .iter()
            .map(|(id, node)| (*id, ref_node(node)))
            .collect(),
        arcs: tree
            .arcs
            .iter()
            .map(|(parent, child, edge)| {
                (
                    *parent,
                    *child,
                    RefEdge::new(&edge.src, &edge.tgt, &edge.kind, edge.name.as_deref()),
                )
            })
            .collect(),
        fans: tree
            .fans
            .iter()
            .map(|fan| RefFan {
                parent: fan.parent,
                hyper_edge: fan.hyper_edge_id.clone(),
                children: fan.children.clone().into_iter().collect(),
            })
            .collect(),
        root: tree.root,
    }
}

fn prod_schema(schema: &RefSchema) -> Schema {
    if *schema == RefSchema::default() {
        return Schema::default();
    }
    let protocol = Protocol::default();
    let mut builder = SchemaBuilder::new(&protocol);
    for (vertex, kind) in &schema.vertices {
        builder = builder
            .vertex(vertex, kind, None)
            .expect("reference vertex is valid");
    }
    for edge in &schema.edges {
        builder = builder
            .edge(&edge.src, &edge.tgt, &edge.kind, edge.name.as_deref())
            .expect("reference edge is valid");
    }
    for (id, hyper) in &schema.hyper_edges {
        builder = builder
            .hyper_edge(
                id,
                &hyper.kind,
                hyper
                    .signature
                    .iter()
                    .map(|(label, vertex)| (label.clone(), vertex.clone()))
                    .collect(),
                &hyper.parent_label,
            )
            .expect("reference hyper-edge is valid");
    }
    assert!(
        schema.constraints.is_empty()
            && schema.required.is_empty()
            && schema.orderings.is_empty()
            && schema.entries.is_empty()
            && schema.keys.is_empty()
            && schema.references.is_empty()
            && schema.target_sets.is_empty(),
        "declarations outside the C11 carrier"
    );
    builder.build().expect("reference schema builds")
}

fn prod_migration(migration: &RefMigration) -> CompiledMigration {
    assert!(migration.conditional.is_empty());
    assert!(migration.transformed.is_empty());
    CompiledMigration {
        surviving_verts: migration
            .surviving_verts
            .iter()
            .cloned()
            .map(Into::into)
            .collect(),
        surviving_edges: migration.surviving_edges.iter().map(prod_edge).collect(),
        vertex_remap: migration
            .vertex_remap
            .iter()
            .map(|(source, target)| (source.clone().into(), target.clone().into()))
            .collect(),
        edge_remap: migration
            .edge_remap
            .iter()
            .map(|(source, target)| (prod_edge(source), prod_edge(target)))
            .collect(),
        resolver: migration
            .resolver
            .iter()
            .map(|((source, target), edge)| {
                (
                    (source.clone().into(), target.clone().into()),
                    prod_edge(edge),
                )
            })
            .collect(),
        expansion_path: migration
            .expansion_path
            .iter()
            .map(|((source, target), path)| {
                (
                    (source.clone().into(), target.clone().into()),
                    path.iter().cloned().map(Into::into).collect(),
                )
            })
            .collect(),
        hyper_resolver: migration
            .hyper_resolver
            .iter()
            .map(|((hyper, labels), (target, remap))| {
                (
                    (
                        hyper.clone().into(),
                        labels.iter().cloned().map(Into::into).collect(),
                    ),
                    (
                        target.clone().into(),
                        remap
                            .iter()
                            .map(|(from, to)| (from.clone().into(), to.clone().into()))
                            .collect(),
                    ),
                )
            })
            .collect(),
        ..CompiledMigration::default()
    }
}

fn production_stage(stage: &C11Stage) -> Lens {
    let mut structural = stage.migration.clone();
    structural.conditional.clear();
    structural.transformed.clear();
    let mut compiled = prod_migration(&structural);
    match stage.layer {
        C11Layer::None => {}
        C11Layer::SurvivalFalse => {
            for vertex in &stage.migration.conditional {
                compiled
                    .conditional_survival
                    .insert(vertex.clone().into(), Expr::Lit(Literal::Bool(false)));
            }
        }
        C11Layer::AddField => {
            for vertex in &stage.migration.transformed {
                compiled.field_transforms.insert(
                    vertex.clone().into(),
                    vec![FieldTransform::AddField {
                        key: C11_FIELD.to_owned(),
                        value: prod_value(&C11_VALUE),
                    }],
                );
            }
        }
        C11Layer::DropField => {
            for vertex in &stage.migration.transformed {
                compiled.field_transforms.insert(
                    vertex.clone().into(),
                    vec![FieldTransform::DropField {
                        key: C11_FIELD.to_owned(),
                    }],
                );
            }
        }
        C11Layer::ReadField => {
            for vertex in &stage.migration.transformed {
                compiled.field_transforms.insert(
                    vertex.clone().into(),
                    vec![FieldTransform::ApplyExpr {
                        key: C11_FIELD.to_owned(),
                        expr: Expr::Var(C11_FIELD.into()),
                        inverse: Some(Expr::Var(C11_FIELD.into())),
                        coercion_class: CoercionClass::Iso,
                    }],
                );
            }
        }
        C11Layer::OpTerm => {
            for vertex in &stage.migration.transformed {
                compiled.op_term_assignments.insert(
                    vertex.clone().into(),
                    vec![TermAssignment::Compute {
                        target: C11_FIELD.to_owned(),
                        scope: TermScope::Row,
                        term: Expr::Lit(Literal::Int(0)),
                        inverse: None,
                        coercion_class: CoercionClass::Projection,
                    }],
                );
            }
        }
    }
    Lens {
        compiled,
        src_schema: prod_schema(&stage.source),
        tgt_schema: prod_schema(&stage.target),
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum ObstructionClass {
    Plan,
    MissingVertex,
    TargetOnlyVertex,
    ConditionalVertex,
    SharedVertexImage,
    TermAssignment,
    NonInvertibleTransform,
    MissingEdge,
    TargetOnlyEdge,
    SharedEdgeImage,
    HyperEdge,
    RequiredField,
    Key,
}

fn vertex_image<'a>(lens: &'a Lens, source: &'a Name) -> &'a Name {
    lens.compiled.vertex_remap.get(source).unwrap_or(source)
}

fn mapped_edge(lens: &Lens, source: &Edge) -> Edge {
    if let Some(image) = lens.compiled.edge_remap.get(source) {
        return image.clone();
    }
    let carried = Edge {
        src: vertex_image(lens, &source.src).clone(),
        tgt: vertex_image(lens, &source.tgt).clone(),
        kind: source.kind.clone(),
        name: source.name.clone(),
    };
    if lens.tgt_schema.edges.contains_key(&carried) {
        return carried;
    }
    if lens
        .src_schema
        .edges_between(source.src.as_str(), source.tgt.as_str())
        .len()
        == 1
        && let Some(resolved) = lens
            .compiled
            .resolver
            .get(&(carried.src.clone(), carried.tgt.clone()))
    {
        return resolved.clone();
    }
    carried
}

fn independent_tree_obstruction(lens: &Lens) -> Option<ObstructionClass> {
    let mut images = BTreeSet::new();
    let mut vertices = lens.src_schema.vertices.keys().collect::<Vec<_>>();
    vertices.sort();
    for source in vertices {
        let image = vertex_image(lens, source);
        if !lens.compiled.surviving_verts.contains(image)
            || !lens.tgt_schema.vertices.contains_key(image)
        {
            return Some(ObstructionClass::MissingVertex);
        }
        if lens.compiled.conditional_survival.contains_key(source) {
            return Some(ObstructionClass::ConditionalVertex);
        }
        if !images.insert(image.clone()) {
            return Some(ObstructionClass::SharedVertexImage);
        }
    }
    if lens
        .tgt_schema
        .vertices
        .keys()
        .any(|target| !images.contains(target))
    {
        return Some(ObstructionClass::TargetOnlyVertex);
    }

    let mut edge_images = BTreeSet::new();
    let mut edges = lens.src_schema.edges.keys().collect::<Vec<_>>();
    edges.sort();
    for source in edges {
        let image = mapped_edge(lens, source);
        if image.src != *vertex_image(lens, &source.src)
            || image.tgt != *vertex_image(lens, &source.tgt)
        {
            return Some(ObstructionClass::Plan);
        }
        if !lens.tgt_schema.edges.contains_key(&image) {
            return Some(ObstructionClass::MissingEdge);
        }
        if !edge_images.insert(image) {
            return Some(ObstructionClass::SharedEdgeImage);
        }
    }
    if lens
        .tgt_schema
        .edges
        .keys()
        .any(|target| !edge_images.contains(target))
    {
        return Some(ObstructionClass::TargetOnlyEdge);
    }
    if !lens.src_schema.hyper_edges.is_empty()
        || !lens.tgt_schema.hyper_edges.is_empty()
        || !lens.compiled.hyper_resolver.is_empty()
    {
        return Some(ObstructionClass::HyperEdge);
    }
    let source_required_without_default = lens.src_schema.required.iter().any(|(vertex, edges)| {
        lens.src_schema.vertices.contains_key(vertex)
            && edges
                .iter()
                .any(|edge| !lens.src_schema.defaults.contains_key(&edge.tgt))
    });
    if source_required_without_default {
        return Some(ObstructionClass::RequiredField);
    }
    if lens
        .src_schema
        .declarations
        .keys
        .iter()
        .any(|(vertex, keys)| lens.src_schema.vertices.contains_key(vertex) && !keys.is_empty())
    {
        return Some(ObstructionClass::Key);
    }
    let target_required_without_default = lens.tgt_schema.required.iter().any(|(vertex, edges)| {
        lens.tgt_schema.vertices.contains_key(vertex)
            && edges
                .iter()
                .any(|edge| !lens.tgt_schema.defaults.contains_key(&edge.tgt))
    });
    if target_required_without_default {
        return Some(ObstructionClass::RequiredField);
    }
    if lens
        .tgt_schema
        .declarations
        .keys
        .iter()
        .any(|(vertex, keys)| lens.tgt_schema.vertices.contains_key(vertex) && !keys.is_empty())
    {
        return Some(ObstructionClass::Key);
    }
    if lens
        .compiled
        .field_transforms
        .values()
        .any(|transforms| !transforms.is_empty())
        || lens
            .compiled
            .op_term_assignments
            .values()
            .any(|assignments| !assignments.is_empty())
    {
        return Some(ObstructionClass::NonInvertibleTransform);
    }
    None
}

fn independent_graph_obstruction(lens: &Lens) -> Option<ObstructionClass> {
    let mut images = BTreeSet::new();
    let mut vertices = lens.src_schema.vertices.keys().collect::<Vec<_>>();
    vertices.sort();
    for source in vertices {
        let image = vertex_image(lens, source);
        if !lens.compiled.surviving_verts.contains(image)
            || !lens.tgt_schema.vertices.contains_key(image)
        {
            return Some(ObstructionClass::MissingVertex);
        }
        if lens.compiled.conditional_survival.contains_key(source) {
            return Some(ObstructionClass::ConditionalVertex);
        }
        if !images.insert(image.clone()) {
            return Some(ObstructionClass::SharedVertexImage);
        }
    }
    if lens
        .tgt_schema
        .vertices
        .keys()
        .any(|target| !images.contains(target))
    {
        return Some(ObstructionClass::TargetOnlyVertex);
    }
    if lens
        .compiled
        .op_term_assignments
        .values()
        .any(|assignments| !assignments.is_empty())
    {
        return Some(ObstructionClass::TermAssignment);
    }
    if lens
        .compiled
        .field_transforms
        .values()
        .any(|transforms| !transforms.is_empty())
    {
        return Some(ObstructionClass::NonInvertibleTransform);
    }

    let mut edge_images = BTreeSet::new();
    let mut edges = lens.src_schema.edges.keys().collect::<Vec<_>>();
    edges.sort();
    for source in edges {
        let endpoints = (
            vertex_image(lens, &source.src),
            vertex_image(lens, &source.tgt),
        );
        let image = mapped_edge(lens, source);
        if image.src != *endpoints.0 || image.tgt != *endpoints.1 {
            return Some(ObstructionClass::Plan);
        }
        if !lens.tgt_schema.edges.contains_key(&image) {
            return Some(ObstructionClass::MissingEdge);
        }
        if !edge_images.insert(image) {
            return Some(ObstructionClass::SharedEdgeImage);
        }
    }
    if lens
        .tgt_schema
        .edges
        .keys()
        .any(|target| !edge_images.contains(target))
    {
        return Some(ObstructionClass::TargetOnlyEdge);
    }
    if !lens.src_schema.hyper_edges.is_empty()
        || !lens.tgt_schema.hyper_edges.is_empty()
        || !lens.compiled.hyper_resolver.is_empty()
    {
        return Some(ObstructionClass::HyperEdge);
    }

    let required_without_default = lens.src_schema.required.iter().any(|(vertex, edges)| {
        lens.src_schema.vertices.contains_key(vertex)
            && edges
                .iter()
                .any(|edge| !lens.src_schema.defaults.contains_key(&edge.tgt))
    });
    if required_without_default {
        return Some(ObstructionClass::RequiredField);
    }
    if lens
        .src_schema
        .declarations
        .keys
        .iter()
        .any(|(vertex, keys)| lens.src_schema.vertices.contains_key(vertex) && !keys.is_empty())
    {
        return Some(ObstructionClass::Key);
    }
    let target_required_without_default = lens.tgt_schema.required.iter().any(|(vertex, edges)| {
        lens.tgt_schema.vertices.contains_key(vertex)
            && edges
                .iter()
                .any(|edge| !lens.tgt_schema.defaults.contains_key(&edge.tgt))
    });
    if target_required_without_default {
        return Some(ObstructionClass::RequiredField);
    }
    if lens
        .tgt_schema
        .declarations
        .keys
        .iter()
        .any(|(vertex, keys)| lens.tgt_schema.vertices.contains_key(vertex) && !keys.is_empty())
    {
        return Some(ObstructionClass::Key);
    }
    None
}

fn identity_lens(schema: &Schema) -> Lens {
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

fn edge(src: &str, tgt: &str, name: &str) -> Edge {
    Edge {
        src: src.into(),
        tgt: tgt.into(),
        kind: "rel".into(),
        name: Some(name.into()),
    }
}

fn plain_schema(vertices: &[&str], edges: &[Edge]) -> Schema {
    let protocol = Protocol::default();
    let mut builder = SchemaBuilder::new(&protocol);
    for vertex in vertices {
        builder = builder
            .vertex(vertex, "record", None)
            .expect("fixture vertex");
    }
    for edge in edges {
        builder = builder
            .edge(&edge.src, &edge.tgt, &edge.kind, edge.name.as_deref())
            .expect("fixture edge");
    }
    builder.build().expect("fixture schema")
}

fn graph_only_witnesses() -> Vec<(&'static str, ObstructionClass, Lens)> {
    let ghost_vertex = Lens {
        compiled: CompiledMigration {
            surviving_verts: [Name::from("ghost")].into(),
            vertex_remap: [(Name::from("a"), Name::from("ghost"))].into(),
            ..CompiledMigration::default()
        },
        src_schema: plain_schema(&["a"], &[]),
        tgt_schema: plain_schema(&["b"], &[]),
    };

    let shared_source = plain_schema(&["a", "b"], &[]);
    let shared_target = plain_schema(&["x"], &[]);
    let shared_vertex = Lens {
        compiled: CompiledMigration {
            surviving_verts: [Name::from("x")].into(),
            vertex_remap: [(Name::from("a"), Name::from("x")), ("b".into(), "x".into())].into(),
            ..CompiledMigration::default()
        },
        src_schema: shared_source,
        tgt_schema: shared_target,
    };

    let source_x = edge("a", "b", "x");
    let source_y = edge("a", "b", "y");
    let target_z = edge("a", "b", "z");
    let shared_edge = Lens {
        compiled: CompiledMigration {
            surviving_verts: [Name::from("a"), Name::from("b")].into(),
            surviving_edges: [target_z.clone()].into(),
            edge_remap: [
                (source_x.clone(), target_z.clone()),
                (source_y.clone(), target_z.clone()),
            ]
            .into(),
            ..CompiledMigration::default()
        },
        src_schema: plain_schema(&["a", "b"], &[source_x, source_y]),
        tgt_schema: plain_schema(&["a", "b"], &[target_z]),
    };

    let required_edge = Edge {
        src: "a".into(),
        tgt: "a.value".into(),
        kind: "prop".into(),
        name: Some("value".into()),
    };
    let protocol = Protocol::default();
    let required_schema = SchemaBuilder::new(&protocol)
        .vertex("a", "record", None)
        .expect("record")
        .vertex("a.value", "string", None)
        .expect("value")
        .edge("a", "a.value", "prop", Some("value"))
        .expect("field")
        .required("a", vec![required_edge.clone()])
        .build()
        .expect("required schema");
    let unconstrained_required_schema =
        plain_schema(&["a", "a.value"], std::slice::from_ref(&required_edge));
    let target_required = Lens {
        compiled: identity_lens(&unconstrained_required_schema).compiled,
        src_schema: unconstrained_required_schema,
        tgt_schema: required_schema.clone(),
    };

    let mut keyed_schema = plain_schema(&["a"], &[]);
    keyed_schema.declarations.keys.insert(
        "a".into(),
        vec![Key {
            id: "by-value".into(),
            parts: vec![KeyPart::Path(Vec::new())],
            role: KeyRole::Unique,
            scope: KeyScope::Global,
            equality: vec![],
        }],
    );
    let unconstrained_keyed_schema = plain_schema(&["a"], &[]);
    let target_keyed = Lens {
        compiled: identity_lens(&unconstrained_keyed_schema).compiled,
        src_schema: unconstrained_keyed_schema,
        tgt_schema: keyed_schema.clone(),
    };

    vec![
        (
            "target-only vertex",
            ObstructionClass::TargetOnlyVertex,
            target_only_vertex_lens(),
        ),
        (
            "target-only edge",
            ObstructionClass::TargetOnlyEdge,
            target_only_edge_lens(),
        ),
        (
            "target-only hyper-edge declaration",
            ObstructionClass::HyperEdge,
            target_only_hyper_lens(),
        ),
        (
            "surviving vertex image absent from the target",
            ObstructionClass::MissingVertex,
            ghost_vertex,
        ),
        (
            "shared vertex image",
            ObstructionClass::SharedVertexImage,
            shared_vertex,
        ),
        (
            "shared edge image",
            ObstructionClass::SharedEdgeImage,
            shared_edge,
        ),
        (
            "required field without a default",
            ObstructionClass::RequiredField,
            identity_lens(&required_schema),
        ),
        (
            "target required field without a default",
            ObstructionClass::RequiredField,
            target_required,
        ),
        (
            "declared key",
            ObstructionClass::Key,
            identity_lens(&keyed_schema),
        ),
        ("target declared key", ObstructionClass::Key, target_keyed),
    ]
}

fn tree_structural_witnesses() -> Vec<(&'static str, ObstructionClass, Lens)> {
    let mut witnesses: Vec<_> = graph_only_witnesses()
        .into_iter()
        .filter(|(_, obstruction, _)| {
            matches!(
                obstruction,
                ObstructionClass::MissingVertex
                    | ObstructionClass::TargetOnlyVertex
                    | ObstructionClass::SharedVertexImage
                    | ObstructionClass::TargetOnlyEdge
                    | ObstructionClass::SharedEdgeImage
                    | ObstructionClass::HyperEdge
                    | ObstructionClass::RequiredField
                    | ObstructionClass::Key
            )
        })
        .collect();

    let source_edge = edge("a", "b", "present-only-in-source");
    let missing_target = Lens {
        compiled: CompiledMigration {
            surviving_verts: [Name::from("a"), Name::from("b")].into(),
            surviving_edges: [source_edge.clone()].into(),
            edge_remap: [(source_edge.clone(), source_edge.clone())].into(),
            ..CompiledMigration::default()
        },
        src_schema: plain_schema(&["a", "b"], &[source_edge]),
        tgt_schema: plain_schema(&["a", "b"], &[]),
    };
    witnesses.push((
        "explicit edge image absent from the target",
        ObstructionClass::MissingEdge,
        missing_target,
    ));
    witnesses
}

fn one_vertex_lens_with_field_transform(transform: FieldTransform) -> Lens {
    let schema = plain_schema(&["a"], &[]);
    let mut lens = identity_lens(&schema);
    lens.compiled
        .field_transforms
        .insert("a".into(), vec![transform]);
    lens
}

fn one_vertex_lens_with_term_assignment(assignment: TermAssignment) -> Lens {
    let schema = plain_schema(&["a"], &[]);
    let mut lens = identity_lens(&schema);
    lens.compiled
        .op_term_assignments
        .insert("a".into(), vec![assignment]);
    lens
}

fn identity_apply(key: &str) -> FieldTransform {
    FieldTransform::ApplyExpr {
        key: key.to_owned(),
        expr: Expr::Var(key.into()),
        inverse: Some(Expr::Var(key.into())),
        coercion_class: CoercionClass::Iso,
    }
}

fn tree_value_witnesses() -> Vec<(&'static str, Lens)> {
    vec![
        (
            "Iso ApplyExpr without an inverse",
            one_vertex_lens_with_field_transform(FieldTransform::ApplyExpr {
                key: "field".to_owned(),
                expr: Expr::Var("field".into()),
                inverse: None,
                coercion_class: CoercionClass::Iso,
            }),
        ),
        (
            "Iso ApplyExpr over __value__",
            one_vertex_lens_with_field_transform(identity_apply("__value__")),
        ),
        (
            "Iso ComputeField",
            one_vertex_lens_with_field_transform(FieldTransform::ComputeField {
                target_key: "computed".to_owned(),
                expr: Expr::Var("source".into()),
                inverse: Some(Expr::Var("computed".into())),
                coercion_class: CoercionClass::Iso,
            }),
        ),
        (
            "nonempty Iso PathTransform",
            one_vertex_lens_with_field_transform(FieldTransform::PathTransform {
                path: vec!["nested".to_owned()],
                inner: Box::new(identity_apply("field")),
            }),
        ),
        (
            "nonempty Iso Case transform list",
            one_vertex_lens_with_field_transform(FieldTransform::Case {
                branches: vec![panproto_inst::CaseBranch {
                    predicate: Expr::Lit(Literal::Bool(true)),
                    transforms: vec![FieldTransform::RenameField {
                        old_key: "field".to_owned(),
                        new_key: "field".to_owned(),
                    }],
                }],
            }),
        ),
        (
            "nontrivial Iso Compute term assignment",
            one_vertex_lens_with_term_assignment(TermAssignment::Compute {
                target: "field".to_owned(),
                scope: TermScope::Row,
                term: Expr::Var("field".into()),
                inverse: Some(Expr::Var("field".into())),
                coercion_class: CoercionClass::Iso,
            }),
        ),
        (
            "nonidentity Iso Rename term assignment",
            one_vertex_lens_with_term_assignment(TermAssignment::Rename {
                old: "old".to_owned(),
                new: "new".to_owned(),
            }),
        ),
        (
            "nonempty Iso Case term-assignment list",
            one_vertex_lens_with_term_assignment(TermAssignment::Case {
                branches: vec![panproto_inst::TermBranch {
                    predicate: Expr::Lit(Literal::Bool(true)),
                    assignments: vec![TermAssignment::Rename {
                        old: "field".to_owned(),
                        new: "field".to_owned(),
                    }],
                }],
            }),
        ),
    ]
}

fn nonidentity_open_field_rename_witness() -> (Lens, WInstance) {
    let schema = plain_schema(&["a"], &[]);
    let mut lens = identity_lens(&schema);
    lens.compiled.field_transforms.insert(
        "a".into(),
        vec![FieldTransform::RenameField {
            old_key: "old".to_owned(),
            new_key: "new".to_owned(),
        }],
    );

    let mut node = Node::new(0, "a");
    node.extra_fields = HashMap::from([
        ("old".to_owned(), Value::Int(1)),
        ("new".to_owned(), Value::Int(2)),
    ]);
    let source = WInstance::new(
        HashMap::from([(0, node)]),
        Vec::new(),
        Vec::new(),
        0,
        "a".into(),
    );
    (lens, source)
}

fn target_only_vertex_lens() -> Lens {
    let source = plain_schema(&["a"], &[]);
    Lens {
        compiled: identity_lens(&source).compiled,
        src_schema: source,
        tgt_schema: plain_schema(&["a", "b"], &[]),
    }
}

fn target_only_edge_lens() -> Lens {
    let source = plain_schema(&["a", "b"], &[]);
    Lens {
        compiled: identity_lens(&source).compiled,
        src_schema: source,
        tgt_schema: plain_schema(&["a", "b"], &[edge("a", "b", "target-only")]),
    }
}

fn target_only_hyper_lens() -> Lens {
    let target = prod_schema(&c11_fan_schema());
    let mut source = target.clone();
    source.hyper_edges.clear();
    Lens {
        compiled: identity_lens(&source).compiled,
        src_schema: source,
        tgt_schema: target,
    }
}

fn resolver_only_edge_relabel_witness() -> (Lens, WInstance) {
    let source_edge = edge("a", "b", "old");
    let target_edge = edge("a", "b", "new");
    let lens = Lens {
        compiled: CompiledMigration {
            surviving_verts: [Name::from("a"), Name::from("b")].into(),
            surviving_edges: [target_edge.clone()].into(),
            resolver: [((Name::from("a"), Name::from("b")), target_edge)].into(),
            ..CompiledMigration::default()
        },
        src_schema: plain_schema(&["a", "b"], std::slice::from_ref(&source_edge)),
        tgt_schema: plain_schema(&["a", "b"], &[edge("a", "b", "new")]),
    };
    let source = WInstance::new(
        HashMap::from([(0, Node::new(0, "a")), (1, Node::new(1, "b"))]),
        vec![(0, 1, source_edge)],
        Vec::new(),
        0,
        "a".into(),
    );
    (lens, source)
}

fn incoherent_resolver_witness() -> Lens {
    let source_edge = edge("a", "b", "old");
    let target_edge = edge("b", "a", "new");
    Lens {
        compiled: CompiledMigration {
            surviving_verts: [Name::from("a"), Name::from("b")].into(),
            surviving_edges: [target_edge.clone()].into(),
            resolver: [((Name::from("a"), Name::from("b")), target_edge.clone())].into(),
            ..CompiledMigration::default()
        },
        src_schema: plain_schema(&["a", "b"], &[source_edge]),
        tgt_schema: plain_schema(&["a", "b"], &[target_edge]),
    }
}

fn boolean_scalar_apply_lens() -> Lens {
    one_vertex_lens_with_field_transform(FieldTransform::ApplyExpr {
        key: "__value__".to_owned(),
        expr: Expr::Builtin(BuiltinOp::Not, vec![Expr::Var("__value__".into())]),
        inverse: Some(Expr::Builtin(
            BuiltinOp::Not,
            vec![Expr::Var("__value__".into())],
        )),
        coercion_class: CoercionClass::Iso,
    })
}

fn identity_apply_with_nonidentity_inverse_lens() -> Lens {
    one_vertex_lens_with_field_transform(FieldTransform::ApplyExpr {
        key: "field".to_owned(),
        expr: Expr::Var("field".into()),
        inverse: Some(Expr::Lit(Literal::Bool(false))),
        coercion_class: CoercionClass::Iso,
    })
}

fn refusing_empty_case_lens() -> Lens {
    one_vertex_lens_with_field_transform(FieldTransform::Case {
        branches: vec![panproto_inst::CaseBranch {
            predicate: Expr::Var("missing".into()),
            transforms: Vec::new(),
        }],
    })
}

fn boolean_source(anchor: &str) -> WInstance {
    let mut node = Node::new(0, anchor);
    node.value = Some(FieldPresence::Present(Value::Bool(true)));
    WInstance::new(
        HashMap::from([(0, node)]),
        Vec::new(),
        Vec::new(),
        0,
        anchor.into(),
    )
}

fn child_scalar_apply_witness() -> (Lens, WInstance) {
    let child_edge = edge("a", "b", "field");
    let schema = plain_schema(&["a", "b"], std::slice::from_ref(&child_edge));
    let mut lens = identity_lens(&schema);
    lens.compiled
        .field_transforms
        .insert("a".into(), vec![identity_apply("field")]);

    let mut child = Node::new(1, "b");
    child.value = Some(FieldPresence::Present(Value::Bool(true)));
    let source = WInstance::new(
        HashMap::from([(0, Node::new(0, "a")), (1, child)]),
        vec![(0, 1, child_edge)],
        Vec::new(),
        0,
        "a".into(),
    );
    (lens, source)
}

fn symmetric_graph_and_table_spans_require_one_canonical_middle() -> usize {
    let mut assertions = 0_usize;
    let forward = plain_schema(&["a", "b"], &[edge("a", "b", "b")]);
    let reverse = plain_schema(&["b", "a"], &[edge("a", "b", "b")]);
    let unequal = plain_schema(&["a", "b"], &[edge("b", "a", "a")]);
    assert_eq!(canonical_bytes(&forward), canonical_bytes(&reverse));
    assert_ne!(canonical_bytes(&forward), canonical_bytes(&unequal));

    assert!(
        panproto_lens::graph_symmetric::from_span(
            identity_lens(&forward),
            identity_lens(&reverse),
        )
        .is_ok()
    );
    assertions += 1;
    assert!(matches!(
        panproto_lens::graph_symmetric::from_span(identity_lens(&forward), identity_lens(&unequal),),
        Err(LensError::NoSharedMiddle { .. })
    ));
    assertions += 1;
    assert!(
        panproto_lens::table_symmetric::from_span(
            identity_lens(&forward),
            identity_lens(&reverse),
        )
        .is_ok()
    );
    assertions += 1;
    assert!(matches!(
        panproto_lens::table_symmetric::from_span(identity_lens(&forward), identity_lens(&unequal),),
        Err(LensError::NoSharedMiddle { .. })
    ));
    assertions += 1;
    assertions
}

fn unit_complement_classifiers_reject_scalar_and_target_only_coordinates() -> usize {
    let mut scenarios = 0_usize;
    let target_vertex = target_only_vertex_lens();
    assert_eq!(
        independent_graph_obstruction(&target_vertex),
        Some(ObstructionClass::TargetOnlyVertex)
    );
    assert_eq!(
        independent_tree_obstruction(&target_vertex),
        Some(ObstructionClass::TargetOnlyVertex)
    );
    assert!(!graph_lens::is_isomorphism(&target_vertex));
    assert!(!target_vertex.is_isomorphism());
    scenarios += 1;

    let target_edge = target_only_edge_lens();
    assert_eq!(
        independent_graph_obstruction(&target_edge),
        Some(ObstructionClass::TargetOnlyEdge)
    );
    assert_eq!(
        independent_tree_obstruction(&target_edge),
        Some(ObstructionClass::TargetOnlyEdge)
    );
    assert!(!graph_lens::is_isomorphism(&target_edge));
    assert!(!target_edge.is_isomorphism());
    scenarios += 1;

    let target_hyper = target_only_hyper_lens();
    assert_eq!(
        independent_graph_obstruction(&target_hyper),
        Some(ObstructionClass::HyperEdge)
    );
    assert_eq!(
        independent_tree_obstruction(&target_hyper),
        Some(ObstructionClass::HyperEdge)
    );
    assert!(!graph_lens::is_isomorphism(&target_hyper));
    assert!(!target_hyper.is_isomorphism());
    scenarios += 1;

    let hyper_schema = prod_schema(&c11_fan_schema());
    let hyper_identity = identity_lens(&hyper_schema);
    let empty_fan_source = WInstance::new(
        HashMap::from([(0, Node::new(0, "a"))]),
        Vec::new(),
        vec![Fan::new("h", 0)],
        0,
        "a".into(),
    );
    assert_eq!(
        independent_tree_obstruction(&hyper_identity),
        Some(ObstructionClass::HyperEdge)
    );
    let (_, empty_fan_complement) = panproto_lens::get(&hyper_identity, &empty_fan_source)
        .expect("zero-child fan is in the tree get domain");
    assert_eq!(empty_fan_complement.dropped_fans, empty_fan_source.fans);
    assert!(!empty_fan_complement.residue_is_trivial());
    assert!(!hyper_identity.is_isomorphism());
    scenarios += 1;

    let scalar = boolean_scalar_apply_lens();
    assert_eq!(
        independent_graph_obstruction(&scalar),
        Some(ObstructionClass::NonInvertibleTransform)
    );
    assert_eq!(
        independent_tree_obstruction(&scalar),
        Some(ObstructionClass::NonInvertibleTransform)
    );
    let scalar_source = boolean_source("a");
    let scalar_graph = GInstance::from_wtype(&scalar_source).expect("one-node graph");
    let (_, graph_complement) =
        graph_lens::get(&scalar, &scalar_graph).expect("Boolean scalar graph get");
    let (_, tree_complement) =
        panproto_lens::get(&scalar, &scalar_source).expect("Boolean scalar tree get");
    assert!(!graph_complement.residue_is_trivial());
    assert!(!tree_complement.residue_is_trivial());
    assert!(!graph_lens::is_isomorphism(&scalar));
    assert!(!scalar.is_isomorphism());
    scenarios += 1;

    let (child_scalar, child_source) = child_scalar_apply_witness();
    assert_eq!(
        independent_graph_obstruction(&child_scalar),
        Some(ObstructionClass::NonInvertibleTransform)
    );
    assert_eq!(
        independent_tree_obstruction(&child_scalar),
        Some(ObstructionClass::NonInvertibleTransform)
    );
    let child_graph = GInstance::from_wtype(&child_source).expect("two-node graph");
    let (_, graph_complement) =
        graph_lens::get(&child_scalar, &child_graph).expect("child-scalar graph get");
    let (_, tree_complement) =
        panproto_lens::get(&child_scalar, &child_source).expect("child-scalar tree get");
    assert!(!graph_complement.residue_is_trivial());
    assert!(!tree_complement.residue_is_trivial());
    assert!(!graph_lens::is_isomorphism(&child_scalar));
    assert!(!child_scalar.is_isomorphism());
    scenarios += 1;

    let bad_inverse = identity_apply_with_nonidentity_inverse_lens();
    assert_eq!(
        independent_graph_obstruction(&bad_inverse),
        Some(ObstructionClass::NonInvertibleTransform)
    );
    assert_eq!(
        independent_tree_obstruction(&bad_inverse),
        Some(ObstructionClass::NonInvertibleTransform)
    );
    let mut bad_inverse_node = Node::new(0, "a");
    bad_inverse_node
        .extra_fields
        .insert("field".to_owned(), Value::Bool(true));
    let bad_inverse_source = WInstance::new(
        HashMap::from([(0, bad_inverse_node)]),
        Vec::new(),
        Vec::new(),
        0,
        "a".into(),
    );
    let bad_inverse_graph =
        GInstance::from_wtype(&bad_inverse_source).expect("one-node field graph");
    let (_, graph_complement) =
        graph_lens::get(&bad_inverse, &bad_inverse_graph).expect("identity forward graph get");
    let (_, tree_complement) =
        panproto_lens::get(&bad_inverse, &bad_inverse_source).expect("identity forward tree get");
    assert!(graph_complement.residue_is_trivial());
    assert!(tree_complement.residue_is_trivial());
    assert!(!graph_lens::is_isomorphism(&bad_inverse));
    assert!(!bad_inverse.is_isomorphism());
    scenarios += 1;

    let refusing_case = refusing_empty_case_lens();
    assert_eq!(
        independent_graph_obstruction(&refusing_case),
        Some(ObstructionClass::NonInvertibleTransform)
    );
    assert_eq!(
        independent_tree_obstruction(&refusing_case),
        Some(ObstructionClass::NonInvertibleTransform)
    );
    assert!(graph_lens::get(&refusing_case, &bad_inverse_graph).is_err());
    assert!(panproto_lens::get(&refusing_case, &bad_inverse_source).is_err());
    assert!(!graph_lens::is_isomorphism(&refusing_case));
    assert!(!refusing_case.is_isomorphism());
    scenarios += 1;

    let identity = identity_lens(&plain_schema(&["a"], &[]));
    let identity_source = boolean_source("a");
    let identity_graph = GInstance::from_wtype(&identity_source).expect("one-node graph");
    let (_, graph_complement) =
        graph_lens::get(&identity, &identity_graph).expect("identity graph get");
    let (_, tree_complement) =
        panproto_lens::get(&identity, &identity_source).expect("identity tree get");
    assert!(graph_lens::is_isomorphism(&identity));
    assert!(identity.is_isomorphism());
    assert!(graph_complement.residue_is_trivial());
    assert!(tree_complement.residue_is_trivial());
    scenarios += 1;
    scenarios
}

fn unique_pair_resolver_relabel_is_a_residue_free_bijection() -> usize {
    let (lens, source) = resolver_only_edge_relabel_witness();
    assert_eq!(independent_graph_obstruction(&lens), None);
    assert_eq!(independent_tree_obstruction(&lens), None);
    assert!(graph_lens::is_isomorphism(&lens));
    assert!(lens.is_isomorphism());

    let source_graph = GInstance::from_wtype(&source).expect("two-node source graph");
    let (graph_view, graph_complement) =
        graph_lens::get(&lens, &source_graph).expect("resolver-only graph get");
    let (tree_view, tree_complement) =
        panproto_lens::get(&lens, &source).expect("resolver-only tree get");
    assert!(graph_complement.residue_is_trivial());
    assert!(tree_complement.residue_is_trivial());

    let graph_restored = graph_lens::put_without_complement(&lens, &graph_view)
        .expect("resolver-only graph complement-free put");
    let tree_restored = panproto_lens::put_without_complement(&lens, &tree_view)
        .expect("resolver-only tree complement-free put");
    assert!(GInstance::identical(&source_graph, &graph_restored));
    assert_eq!(ref_tree(&tree_restored), ref_tree(&source));
    1
}

fn incoherent_unique_pair_resolver_is_rejected() -> usize {
    let lens = incoherent_resolver_witness();
    assert_eq!(
        independent_graph_obstruction(&lens),
        Some(ObstructionClass::Plan)
    );
    assert_eq!(
        independent_tree_obstruction(&lens),
        Some(ObstructionClass::Plan)
    );
    assert!(!graph_lens::is_isomorphism(&lens));
    assert!(!lens.is_isomorphism());
    1
}

fn graph_classifier_and_complement_free_put_agree_with_the_structural_oracle() {
    let certificate = lens_construction_count_certificate();
    let stages = c11_stages();
    assert_eq!(stages.len(), certificate.stage_classifications);
    let mut seen = BTreeSet::new();
    let mut positive_stages = 0_usize;
    let mut redundant_expansion_stages = 0_usize;
    let mut restored = 0_usize;
    for stage in &stages {
        let lens = production_stage(stage);
        let obstruction = independent_graph_obstruction(&lens);
        seen.extend(obstruction);
        if stage.name == "X" {
            let source_edge = lens
                .src_schema
                .edges
                .keys()
                .next()
                .expect("C11 X has one source edge");
            assert!(!lens.compiled.expansion_path.is_empty());
            assert!(
                lens.tgt_schema
                    .edges
                    .contains_key(&mapped_edge(&lens, source_edge)),
                "C11 X has a direct carried target edge before its redundant path"
            );
            assert_eq!(obstruction, Some(ObstructionClass::TargetOnlyEdge));
            assert!(!graph_lens::is_isomorphism(&lens));
            redundant_expansion_stages += 1;
        }
        assert_eq!(
            graph_lens::is_isomorphism(&lens),
            obstruction.is_none(),
            "C11 graph stage {}",
            stage.name
        );
        assert_eq!(
            graph_lens::obstruction_to_isomorphism(&lens).is_none(),
            obstruction.is_none(),
            "C11 graph stage {}",
            stage.name
        );

        if obstruction.is_some() {
            assert!(matches!(
                graph_lens::put_without_complement(&lens, &GInstance::default()),
                Err(LensError::NotAnIsomorphism { .. })
            ));
            continue;
        }
        positive_stages += 1;

        let source_graphs = if stage.source == c11_fan_schema() {
            c11_fan_states()
        } else {
            c11_graph_states(&stage.source)
        };
        let mut admitted = 0_usize;
        for state in source_graphs {
            let source = prod_graph(&state);
            let Ok((view, complement)) = graph_lens::get(&lens, &source) else {
                continue;
            };
            assert!(
                complement.residue_is_trivial(),
                "positive C11 graph stage {} left residue",
                stage.name
            );
            admitted += 1;
            let roundtrip = graph_lens::put_without_complement(&lens, &view)
                .expect("the independent graph oracle classified the lens as lossless");
            assert!(GInstance::identical(&source, &roundtrip));
        }
        assert!(admitted > 0, "positive C11 graph stage {}", stage.name);
        restored += admitted;
    }

    let graph_witnesses = graph_only_witnesses();
    let graph_witness_count = graph_witnesses.len();
    for (name, expected, lens) in graph_witnesses {
        let found = independent_graph_obstruction(&lens);
        assert_eq!(found, Some(expected), "independent witness {name}");
        seen.insert(expected);
        assert!(!graph_lens::is_isomorphism(&lens), "witness {name}");
        assert!(
            graph_lens::obstruction_to_isomorphism(&lens).is_some(),
            "witness {name}"
        );
        assert!(matches!(
            graph_lens::put_without_complement(&lens, &GInstance::default()),
            Err(LensError::NotAnIsomorphism { .. })
        ));
    }

    assert_eq!(
        (positive_stages, redundant_expansion_stages, restored),
        (
            certificate.graph_positive_stages,
            1,
            certificate.graph_restorations,
        )
    );
    assert_eq!(graph_witness_count, certificate.graph_structural_witnesses);
    assert_eq!(
        seen,
        BTreeSet::from([
            ObstructionClass::Plan,
            ObstructionClass::MissingVertex,
            ObstructionClass::TargetOnlyVertex,
            ObstructionClass::ConditionalVertex,
            ObstructionClass::SharedVertexImage,
            ObstructionClass::TermAssignment,
            ObstructionClass::NonInvertibleTransform,
            ObstructionClass::MissingEdge,
            ObstructionClass::TargetOnlyEdge,
            ObstructionClass::SharedEdgeImage,
            ObstructionClass::HyperEdge,
            ObstructionClass::RequiredField,
            ObstructionClass::Key,
        ])
    );
}

fn tree_classifier_and_complement_free_put_agree_with_the_structural_oracle() {
    let certificate = lens_construction_count_certificate();
    let mut seen = BTreeSet::new();
    let mut stage_count = 0_usize;
    let mut positive_stages = 0_usize;
    let mut state_bearing_positive_stages = 0_usize;
    let mut restored = 0_usize;
    let mut open_field_witness_count = 0_usize;
    for stage in c11_stages() {
        stage_count += 1;
        let lens = production_stage(&stage);
        let obstruction = independent_tree_obstruction(&lens);
        seen.extend(obstruction);
        assert_eq!(
            lens.is_isomorphism(),
            obstruction.is_none(),
            "C11 tree stage {}",
            stage.name
        );
        assert_eq!(
            lens.obstruction_to_isomorphism().is_none(),
            obstruction.is_none(),
            "C11 tree stage {}",
            stage.name
        );

        if obstruction.is_some() {
            let empty = WInstance::new(HashMap::new(), Vec::new(), Vec::new(), 0, "".into());
            assert!(matches!(
                panproto_lens::put_without_complement(&lens, &empty),
                Err(LensError::NotAnIsomorphism { .. })
            ));
            continue;
        }
        positive_stages += 1;
        if stage.source == c11_fan_schema() {
            continue;
        }

        let states = c11_tree_states(&stage.source);
        let expected = states.len();
        let mut admitted = 0_usize;
        for state in states {
            let source = prod_tree(&state);
            let Ok((view, complement)) = panproto_lens::get(&lens, &source) else {
                continue;
            };
            assert!(
                complement.residue_is_trivial(),
                "positive C11 tree stage {} left residue",
                stage.name
            );
            admitted += 1;
            let roundtrip = panproto_lens::put_without_complement(&lens, &view)
                .expect("the independent tree oracle classified the lens as lossless");
            assert_eq!(ref_tree(&roundtrip), ref_tree(&source));
        }
        assert_eq!(admitted, expected, "positive C11 tree stage {}", stage.name);
        state_bearing_positive_stages += usize::from(admitted > 0);
        restored += admitted;
    }

    let tree_structural_witnesses = tree_structural_witnesses();
    let tree_structural_witness_count = tree_structural_witnesses.len();
    for (name, expected, lens) in tree_structural_witnesses {
        let found = independent_tree_obstruction(&lens);
        assert_eq!(found, Some(expected), "independent tree witness {name}");
        seen.insert(expected);
        assert!(!lens.is_isomorphism(), "tree witness {name}");
        assert!(
            lens.obstruction_to_isomorphism().is_some(),
            "tree witness {name}"
        );
        let empty = WInstance::new(HashMap::new(), Vec::new(), Vec::new(), 0, "".into());
        assert!(matches!(
            panproto_lens::put_without_complement(&lens, &empty),
            Err(LensError::NotAnIsomorphism { .. })
        ));
    }

    let tree_value_witnesses = tree_value_witnesses();
    let tree_value_witness_count = tree_value_witnesses.len();
    for (name, lens) in tree_value_witnesses {
        let found = independent_tree_obstruction(&lens);
        assert_eq!(
            found,
            Some(ObstructionClass::NonInvertibleTransform),
            "independent tree witness {name}"
        );
        seen.extend(found);
        assert!(!lens.is_isomorphism(), "tree witness {name}");
        assert!(
            lens.obstruction_to_isomorphism().is_some(),
            "tree witness {name}"
        );
        let empty = WInstance::new(HashMap::new(), Vec::new(), Vec::new(), 0, "".into());
        assert!(matches!(
            panproto_lens::put_without_complement(&lens, &empty),
            Err(LensError::NotAnIsomorphism { .. })
        ));
    }

    let rename_name = "nonidentity open-field rename";
    let (rename_lens, rename_source) = nonidentity_open_field_rename_witness();
    let rename_obstruction = independent_tree_obstruction(&rename_lens);
    assert_eq!(
        rename_obstruction,
        Some(ObstructionClass::NonInvertibleTransform),
        "independent tree witness {rename_name}"
    );
    seen.extend(rename_obstruction);
    let (rename_view, rename_complement) =
        panproto_lens::get(&rename_lens, &rename_source).expect("rename witness get");
    assert_eq!(
        rename_view.nodes[&0].extra_fields,
        HashMap::from([("new".to_owned(), Value::Int(1))])
    );
    assert_eq!(
        rename_complement.original_extra_fields.get(&0),
        Some(&rename_source.nodes[&0].extra_fields)
    );
    assert!(!rename_lens.is_isomorphism(), "tree witness {rename_name}");
    assert!(
        rename_lens.obstruction_to_isomorphism().is_some(),
        "tree witness {rename_name}"
    );
    assert!(matches!(
        panproto_lens::put_without_complement(&rename_lens, &rename_view),
        Err(LensError::NotAnIsomorphism { .. })
    ));
    open_field_witness_count += 1;

    assert_eq!(
        (positive_stages, state_bearing_positive_stages, restored,),
        (
            certificate.tree_positive_stages,
            certificate.tree_state_bearing_stages,
            certificate.tree_restorations,
        )
    );
    assert_eq!(stage_count, certificate.stage_classifications);
    assert_eq!(
        tree_structural_witness_count,
        certificate.tree_structural_witnesses
    );
    assert_eq!(tree_value_witness_count, certificate.tree_value_witnesses);
    assert_eq!(
        open_field_witness_count,
        certificate.tree_open_field_witnesses
    );
    assert_eq!(
        seen,
        BTreeSet::from([
            ObstructionClass::Plan,
            ObstructionClass::MissingVertex,
            ObstructionClass::TargetOnlyVertex,
            ObstructionClass::ConditionalVertex,
            ObstructionClass::SharedVertexImage,
            ObstructionClass::NonInvertibleTransform,
            ObstructionClass::MissingEdge,
            ObstructionClass::TargetOnlyEdge,
            ObstructionClass::SharedEdgeImage,
            ObstructionClass::HyperEdge,
            ObstructionClass::RequiredField,
            ObstructionClass::Key,
        ])
    );
}

fn tree_validation_obstructions_are_operational() -> usize {
    let mut witnesses = 0_usize;
    let (_, _, required_lens) = graph_only_witnesses()
        .into_iter()
        .find(|(name, _, _)| *name == "required field without a default")
        .expect("the required-field witness is registered");
    let required_source = WInstance::new(
        HashMap::from([(0, Node::new(0, "a"))]),
        Vec::new(),
        Vec::new(),
        0,
        "a".into(),
    );
    let (required_view, required_complement) = panproto_lens::get(&required_lens, &required_source)
        .expect("tree get does not validate the source");
    assert!(required_complement.residue_is_trivial());
    assert!(matches!(
        panproto_lens::asymmetric::check_put_domain(
            &required_lens,
            &required_view,
            &required_complement,
        ),
        Err(ref errors)
            if matches!(
                errors.as_slice(),
                [PutDomainError::MissingRequiredCell {
                    elem: ViewRef::Node(0),
                    field,
                }] if field == "value"
            )
    ));
    assert_eq!(
        independent_tree_obstruction(&required_lens),
        Some(ObstructionClass::RequiredField)
    );
    assert!(!required_lens.is_isomorphism());
    assert!(matches!(
        panproto_lens::put_without_complement(&required_lens, &required_view),
        Err(LensError::NotAnIsomorphism { .. })
    ));
    witnesses += 1;

    let child = edge("root", "item", "item");
    let protocol = Protocol::default();
    let mut keyed_schema = SchemaBuilder::new(&protocol)
        .vertex("root", "object", None)
        .expect("root")
        .vertex("item", "string", None)
        .expect("item")
        .edge("root", "item", "rel", Some("item"))
        .expect("child edge")
        .build()
        .expect("keyed tree schema");
    keyed_schema.declarations.keys.insert(
        "item".into(),
        vec![Key {
            id: "item-value".into(),
            parts: vec![KeyPart::Path(Vec::new())],
            role: KeyRole::Unique,
            scope: KeyScope::Global,
            equality: vec![],
        }],
    );
    let keyed_lens = identity_lens(&keyed_schema);
    let duplicate = |id| {
        Node::new(id, "item").with_value(FieldPresence::Present(Value::Str("same".to_owned())))
    };
    let keyed_source = WInstance::new(
        HashMap::from([
            (0, Node::new(0, "root")),
            (1, duplicate(1)),
            (2, duplicate(2)),
        ]),
        vec![(0, 1, child.clone()), (0, 2, child)],
        Vec::new(),
        0,
        "root".into(),
    );
    let (keyed_view, keyed_complement) = panproto_lens::get(&keyed_lens, &keyed_source)
        .expect("tree get does not validate duplicate keys");
    assert!(keyed_complement.residue_is_trivial());
    assert!(matches!(
        panproto_lens::asymmetric::check_put_domain(
            &keyed_lens,
            &keyed_view,
            &keyed_complement,
        ),
        Err(ref errors)
            if matches!(
                errors.as_slice(),
                [PutDomainError::DuplicateKey { table, elems, .. }]
                    if table.as_str() == "item"
                        && elems == &[ViewRef::Node(1), ViewRef::Node(2)]
            )
    ));
    assert_eq!(
        independent_tree_obstruction(&keyed_lens),
        Some(ObstructionClass::Key)
    );
    assert!(!keyed_lens.is_isomorphism());
    assert!(matches!(
        panproto_lens::put_without_complement(&keyed_lens, &keyed_view),
        Err(LensError::NotAnIsomorphism { .. })
    ));
    witnesses += 1;
    witnesses
}

fn prod_table(table: &RefTable) -> FInstance {
    let mut out = FInstance::new();
    for (id, (name, cells)) in &table.rows {
        out.insert_row(
            name.as_str(),
            RowId(*id),
            cells
                .iter()
                .map(|(key, value)| (key.clone(), prod_value(value)))
                .collect(),
        )
        .expect("reference row ids are distinct");
    }
    for (edge, source, target) in &table.pairs {
        out.insert_foreign_key(prod_edge(edge), RowId(*source), RowId(*target))
            .expect("reference foreign-key endpoints are present");
    }
    out
}

fn ref_table(table: &FInstance) -> RefTable {
    let mut out = RefTable::default();
    for (name, _) in table.tables() {
        for (id, row) in table.rows(name) {
            out.rows.insert(
                id.0,
                (
                    name.to_string(),
                    row.iter()
                        .map(|(key, value)| (key.clone(), ref_value(value)))
                        .collect(),
                ),
            );
        }
    }
    for (edge, pairs) in table.foreign_keys() {
        for (source, target) in pairs {
            out.pairs.insert((
                RefEdge::new(&edge.src, &edge.tgt, &edge.kind, edge.name.as_deref()),
                source.0,
                target.0,
            ));
        }
    }
    out
}

fn compare_table_classifier_and_put(
    label: &str,
    source_schema: &RefSchema,
    target_schema: &RefSchema,
    migration: &RefMigration,
    sources: &[RefTable],
    value_kind: &str,
    scalar: ScalarKind,
) -> usize {
    let value_kinds = BTreeSet::from([value_kind.to_owned()]);
    let table_kinds = BTreeSet::from([TABLE_FIXTURE_TABLE_KIND.to_owned()]);
    let source_boundary =
        panproto_reference::rel::classify(source_schema, &value_kinds, &table_kinds)
            .expect("reference source is relational");
    let target_boundary =
        panproto_reference::rel::classify(target_schema, &value_kinds, &table_kinds)
            .expect("reference target is relational");
    let plan = panproto_reference::table::compile_for_tables(
        source_schema,
        target_schema,
        &source_boundary,
        &target_boundary,
        migration,
    )
    .expect("reference table plan compiles");
    let constraint_semantics = BTreeMap::new();
    let reference = RefTableLens::new(
        source_schema,
        target_schema,
        RefTableBoundaries::new(&source_boundary, &target_boundary),
        migration,
        &plan,
        &constraint_semantics,
        &NoValueLayer,
    )
    .expect("reference table lens binds to its plan");

    let protocol = Protocol {
        value_kinds: BTreeMap::from([(value_kind.to_owned(), scalar)]),
        non_value_kinds: BTreeSet::from([TABLE_FIXTURE_TABLE_KIND.to_owned()]),
        ..Protocol::default()
    };
    let production = Lens {
        compiled: prod_migration(migration),
        src_schema: prod_schema(source_schema),
        tgt_schema: prod_schema(target_schema),
    };
    let boundaries = BoundaryProtocols {
        src: &protocol,
        tgt: &protocol,
    };

    let reference_isomorphism = panproto_reference::table::is_isomorphism(&reference);
    assert_eq!(
        table_lens::is_isomorphism(&production, boundaries),
        reference_isomorphism,
        "table fixture {label}"
    );
    assert_eq!(
        table_lens::obstruction_to_isomorphism(&production, boundaries).is_none(),
        panproto_reference::table::obstruction_to_isomorphism(&reference).is_none(),
        "table fixture {label}"
    );

    for source in sources {
        let (reference_view, _) = reference.get(source).expect("bounded reference get");
        let production_source = prod_table(source);
        let (production_view, _) = table_lens::get(&production, &production_source, boundaries)
            .expect("bounded production get");
        let reference_put =
            panproto_reference::table::put_without_complement(&reference, &reference_view);
        let production_put =
            table_lens::put_without_complement(&production, &production_view, boundaries);
        match reference_put {
            Ok(reference_source) => {
                assert_eq!(reference_source, *source, "reference table fixture {label}");
                let production_source = production_put
                    .unwrap_or_else(|error| panic!("production table fixture {label}: {error}"));
                assert_eq!(ref_table(&production_source), reference_source);
            }
            Err(RefTablePutError::NotAnIsomorphism { .. }) => {
                assert!(matches!(
                    production_put,
                    Err(LensError::NotAnIsomorphism { .. })
                ));
            }
            Err(error) => panic!("unexpected reference table result for {label}: {error:?}"),
        }
    }
    sources.len()
}

fn table_projection_classifier_and_put_agree_with_the_reference() {
    let certificate = lens_construction_count_certificate();
    let mut projections: BTreeMap<(usize, usize), Vec<RefTable>> = BTreeMap::new();
    for_each_table_fixture(|fixture| {
        projections
            .entry((fixture.columns, fixture.retained))
            .or_default()
            .push(fixture.source);
    });
    let mut projection_lenses = 0_usize;
    let mut compared = 0_usize;
    for ((columns, retained), sources) in projections {
        projection_lenses += 1;
        let fixture = TableFixture {
            columns,
            retained,
            source: RefTable::default(),
        };
        let source_schema = fixture.source_schema();
        let target_schema = fixture.target_schema();
        let migration = fixture.migration();
        compared += compare_table_classifier_and_put(
            &format!("projection-{columns}-{retained}"),
            &source_schema,
            &target_schema,
            &migration,
            &sources,
            TABLE_FIXTURE_VALUE_KIND,
            ScalarKind::Any,
        );
    }
    assert_eq!(projection_lenses, certificate.table_projection_lenses);
    assert_eq!(compared, certificate.table_projection_cases);
    assert_eq!(compared, table_fixture_count_certificate().total);
}

fn table_writer_classifier_and_put_agree_with_the_reference() {
    let certificate = lens_construction_count_certificate();
    let mut writers: BTreeMap<[Option<&'static str>; 2], Vec<RefTable>> = BTreeMap::new();
    for_each_writer_fixture(|fixture| {
        writers
            .entry(fixture.images)
            .or_default()
            .push(fixture.source());
    });
    let mut writer_lenses = 0_usize;
    let mut writer_cases = 0_usize;
    for (images, sources) in writers {
        writer_lenses += 1;
        let fixture = WriterFixture {
            images,
            values: [0, 0],
        };
        let source_schema = fixture.source_schema();
        let target_schema = fixture.target_schema();
        let migration = fixture.migration();
        writer_cases += compare_table_classifier_and_put(
            &format!("writer-{images:?}"),
            &source_schema,
            &target_schema,
            &migration,
            &sources,
            WRITER_VALUE_KIND,
            ScalarKind::Int {
                bits: 64,
                signed: true,
            },
        );
    }
    assert_eq!(writer_lenses, certificate.table_writer_lenses);
    assert_eq!(writer_cases, certificate.table_writer_cases);
    assert_eq!(writer_lenses, writer_count_certificate().0);
    assert_eq!(writer_cases, writer_count_certificate().1);
}

fn table_validation_classifier_boundaries_are_operational() -> usize {
    let mut witnesses = 0_usize;
    let protocol = Protocol {
        value_kinds: BTreeMap::from([(
            "int".to_owned(),
            ScalarKind::Int {
                bits: 64,
                signed: true,
            },
        )]),
        non_value_kinds: BTreeSet::from(["record".to_owned()]),
        constraint_sorts: vec!["required".to_owned()],
        constraint_semantics: BTreeMap::from([(
            "required".to_owned(),
            ConstraintSemantics::RequiredValue,
        )]),
        ..Protocol::default()
    };
    let base = SchemaBuilder::new(&protocol)
        .vertex("t", "record", None)
        .expect("table")
        .vertex("t.a", "int", None)
        .expect("column")
        .edge("t", "t.a", "prop", Some("a"))
        .expect("column edge")
        .build()
        .expect("base table schema");
    let boundary = BoundaryProtocols {
        src: &protocol,
        tgt: &protocol,
    };

    let mut required = base.clone();
    required
        .constraints
        .entry("t.a".into())
        .or_default()
        .push(panproto_schema::Constraint {
            sort: "required".into(),
            value: "true".to_owned(),
        });
    let required_lens = identity_lens(&required);
    let missing_source = FInstance::new()
        .with_rows("t", [(RowId(0), HashMap::new())])
        .expect("row without the required cell");
    let (missing_view, missing_complement) =
        table_lens::get(&required_lens, &missing_source, boundary)
            .expect("table get does not validate its raw source");
    assert!(missing_complement.residue_is_trivial());
    assert!(
        table_lens::obstruction_to_isomorphism(&required_lens, boundary)
            .is_some_and(|detail| detail.contains("required-value constraint"))
    );
    assert!(!table_lens::is_isomorphism(&required_lens, boundary));
    assert!(matches!(
        table_lens::check_put_domain(
            &required_lens,
            &missing_view,
            &missing_complement,
            boundary,
        ),
        Err(ref errors)
            if errors.iter().any(|error| matches!(
                error,
                PutDomainError::MissingRequiredCell {
                    elem: ViewRef::Row(RowId(0)),
                    field,
                } if field == "a"
            ))
    ));
    assert!(matches!(
        table_lens::put_without_complement(&required_lens, &missing_view, boundary),
        Err(LensError::NotAnIsomorphism { .. })
    ));
    witnesses += 1;

    let mut keyed = base.clone();
    keyed.declarations.keys.insert(
        "t".into(),
        vec![Key {
            id: "by-a".into(),
            parts: vec![KeyPart::Path(vec![
                base.edges
                    .keys()
                    .next()
                    .cloned()
                    .expect("the table has one column edge"),
            ])],
            role: KeyRole::Unique,
            scope: KeyScope::Global,
            equality: vec![],
        }],
    );
    let keyed_lens = identity_lens(&keyed);
    let duplicate_source = FInstance::new()
        .with_rows(
            "t",
            [
                (RowId(0), HashMap::from([("a".to_owned(), Value::Int(1))])),
                (RowId(1), HashMap::from([("a".to_owned(), Value::Int(1))])),
            ],
        )
        .expect("duplicate-key rows");
    let (duplicate_view, duplicate_complement) =
        table_lens::get(&keyed_lens, &duplicate_source, boundary)
            .expect("table get does not validate duplicate keys");
    assert!(duplicate_complement.residue_is_trivial());
    assert!(
        table_lens::obstruction_to_isomorphism(&keyed_lens, boundary)
            .is_some_and(|detail| detail.contains("key `by-a`"))
    );
    assert!(!table_lens::is_isomorphism(&keyed_lens, boundary));
    assert!(matches!(
        table_lens::check_put_domain(
            &keyed_lens,
            &duplicate_view,
            &duplicate_complement,
            boundary,
        ),
        Err(ref errors)
            if errors.iter().any(|error| matches!(
                error,
                PutDomainError::DuplicateKey { table, elems, .. }
                    if table.as_str() == "t" && elems.len() == 2
            ))
    ));
    assert!(matches!(
        table_lens::put_without_complement(&keyed_lens, &duplicate_view, boundary),
        Err(LensError::NotAnIsomorphism { .. })
    ));
    witnesses += 1;

    let target_required_lens = Lens {
        compiled: identity_lens(&base).compiled,
        src_schema: base,
        tgt_schema: required,
    };
    assert!(table_lens::is_isomorphism(&target_required_lens, boundary));
    let (target_view, target_complement) =
        table_lens::get(&target_required_lens, &missing_source, boundary)
            .expect("target validation is outside the raw table carrier");
    assert!(target_complement.residue_is_trivial());
    assert_eq!(
        table_lens::put_without_complement(&target_required_lens, &target_view, boundary)
            .expect("the operational table inverse ignores target validation"),
        missing_source
    );
    witnesses += 1;
    witnesses
}

#[test]
fn table_validation_isomorphism_witnesses() {
    table_validation_classifier_boundaries_are_operational();
}

fn selector_index(selector: u64, len: usize) -> usize {
    assert!(len > 0, "a randomized carrier must be nonempty");
    let modulus = u64::try_from(len).expect("bounded carrier length fits u64");
    usize::try_from(selector % modulus).expect("reduced selector fits usize")
}

fn sampled<T>(values: Vec<T>) -> Vec<T> {
    assert!(!values.is_empty(), "a restoration carrier must be nonempty");
    let len = values.len();
    let mut positions = BTreeSet::new();
    for numerator in 0..=8 {
        positions.insert(numerator * (len - 1) / 8);
    }
    values
        .into_iter()
        .enumerate()
        .filter_map(|(index, value)| positions.contains(&index).then_some(value))
        .collect()
}

fn c11_stage_carrier() -> &'static [C11Stage] {
    static STAGES: OnceLock<Vec<C11Stage>> = OnceLock::new();
    STAGES.get_or_init(c11_stages)
}

fn graph_restoration_carrier() -> &'static [(C11Stage, Vec<RefGraph>)] {
    static CASES: OnceLock<Vec<(C11Stage, Vec<RefGraph>)>> = OnceLock::new();
    CASES.get_or_init(|| {
        c11_stages()
            .into_iter()
            .filter_map(|stage| {
                let lens = production_stage(&stage);
                if independent_graph_obstruction(&lens).is_some() {
                    return None;
                }
                let states = if stage.source == c11_fan_schema() {
                    c11_fan_states()
                } else {
                    c11_graph_states(&stage.source)
                };
                Some((stage, sampled(states)))
            })
            .collect()
    })
}

fn tree_restoration_carrier() -> &'static [(C11Stage, Vec<RefTree>)] {
    static CASES: OnceLock<Vec<(C11Stage, Vec<RefTree>)>> = OnceLock::new();
    CASES.get_or_init(|| {
        c11_stages()
            .into_iter()
            .filter_map(|stage| {
                let lens = production_stage(&stage);
                if independent_tree_obstruction(&lens).is_some() || stage.source == c11_fan_schema()
                {
                    return None;
                }
                let states = c11_tree_states(&stage.source);
                (!states.is_empty()).then(|| (stage, sampled(states)))
            })
            .collect()
    })
}

fn table_fixture_carrier() -> &'static [TableFixture] {
    static FIXTURES: OnceLock<Vec<TableFixture>> = OnceLock::new();
    FIXTURES.get_or_init(|| {
        let mut fixtures = Vec::new();
        for_each_table_fixture(|fixture| fixtures.push(fixture));
        fixtures
    })
}

fn writer_fixture_carrier() -> &'static [WriterFixture] {
    static FIXTURES: OnceLock<Vec<WriterFixture>> = OnceLock::new();
    FIXTURES.get_or_init(|| {
        let mut fixtures = Vec::new();
        for_each_writer_fixture(|fixture| fixtures.push(fixture));
        fixtures
    })
}

fn assert_random_span_case(selector: u64) {
    let suffix = selector % 65_521;
    let left = format!("left-{suffix}");
    let right = format!("right-{suffix}");
    let carried = edge(&left, &right, "carried");
    let reversed = edge(&right, &left, "different");
    let middle = plain_schema(&[&left, &right], std::slice::from_ref(&carried));
    let equal_middle = plain_schema(&[&right, &left], &[carried]);
    let unequal_middle = plain_schema(&[&left, &right], &[reversed]);
    assert_eq!(canonical_bytes(&middle), canonical_bytes(&equal_middle));
    assert_ne!(canonical_bytes(&middle), canonical_bytes(&unequal_middle));

    assert!(
        panproto_lens::graph_symmetric::from_span(
            identity_lens(&middle),
            identity_lens(&equal_middle),
        )
        .is_ok()
    );
    assert!(matches!(
        panproto_lens::graph_symmetric::from_span(
            identity_lens(&middle),
            identity_lens(&unequal_middle),
        ),
        Err(LensError::NoSharedMiddle { .. })
    ));
    assert!(
        panproto_lens::table_symmetric::from_span(
            identity_lens(&middle),
            identity_lens(&equal_middle),
        )
        .is_ok()
    );
    assert!(matches!(
        panproto_lens::table_symmetric::from_span(
            identity_lens(&middle),
            identity_lens(&unequal_middle),
        ),
        Err(LensError::NoSharedMiddle { .. })
    ));
}

#[test]
fn tree_validation_isomorphism_witnesses() {
    tree_validation_obstructions_are_operational();
}

fn assert_random_graph_cases(selectors: &[u64]) {
    let stages = c11_stage_carrier();
    let stage = &stages[selector_index(selectors[0], stages.len())];
    let lens = production_stage(stage);
    let expected = independent_graph_obstruction(&lens);
    assert_eq!(graph_lens::is_isomorphism(&lens), expected.is_none());
    assert_eq!(
        graph_lens::obstruction_to_isomorphism(&lens).is_none(),
        expected.is_none()
    );

    let witnesses = graph_only_witnesses();
    let (name, expected, lens) = &witnesses[selector_index(selectors[1], witnesses.len())];
    assert_eq!(
        independent_graph_obstruction(lens),
        Some(*expected),
        "random graph witness {name}"
    );
    assert!(!graph_lens::is_isomorphism(lens));
    assert!(matches!(
        graph_lens::put_without_complement(lens, &GInstance::default()),
        Err(LensError::NotAnIsomorphism { .. })
    ));

    let restorations = graph_restoration_carrier();
    let (stage, sources) = &restorations[selector_index(selectors[2], restorations.len())];
    let source = prod_graph(&sources[selector_index(selectors[3], sources.len())]);
    let lens = production_stage(stage);
    assert_eq!(independent_graph_obstruction(&lens), None);
    let (view, complement) = graph_lens::get(&lens, &source)
        .expect("the independent graph classifier admitted the sampled source");
    assert!(complement.residue_is_trivial());
    let restored = graph_lens::put_without_complement(&lens, &view)
        .expect("an admitted graph isomorphism has complement-free put");
    assert!(GInstance::identical(&source, &restored));
}

fn assert_random_tree_cases(selectors: &[u64]) {
    let stages = c11_stage_carrier();
    let stage = &stages[selector_index(selectors[0], stages.len())];
    let lens = production_stage(stage);
    let expected = independent_tree_obstruction(&lens);
    assert_eq!(lens.is_isomorphism(), expected.is_none());
    assert_eq!(
        lens.obstruction_to_isomorphism().is_none(),
        expected.is_none()
    );

    let mut witnesses = tree_structural_witnesses();
    witnesses.extend(
        tree_value_witnesses()
            .into_iter()
            .map(|(name, lens)| (name, ObstructionClass::NonInvertibleTransform, lens)),
    );
    let (name, expected, lens) = &witnesses[selector_index(selectors[1], witnesses.len())];
    assert_eq!(
        independent_tree_obstruction(lens),
        Some(*expected),
        "random tree witness {name}"
    );
    assert!(!lens.is_isomorphism());
    let empty = WInstance::new(HashMap::new(), Vec::new(), Vec::new(), 0, "".into());
    assert!(matches!(
        panproto_lens::put_without_complement(lens, &empty),
        Err(LensError::NotAnIsomorphism { .. })
    ));

    // This singleton family is counted separately from the structural and
    // value-program witnesses, so exercise it in every generated case.
    let (rename_lens, rename_source) = nonidentity_open_field_rename_witness();
    assert_eq!(
        independent_tree_obstruction(&rename_lens),
        Some(ObstructionClass::NonInvertibleTransform),
        "random tree open-field rename witness"
    );
    let (rename_view, rename_complement) = panproto_lens::get(&rename_lens, &rename_source)
        .expect("random open-field rename witness get");
    assert_eq!(
        rename_view.nodes[&0].extra_fields,
        HashMap::from([("new".to_owned(), Value::Int(1))])
    );
    assert_eq!(
        rename_complement.original_extra_fields.get(&0),
        Some(&rename_source.nodes[&0].extra_fields)
    );
    assert!(!rename_lens.is_isomorphism());
    assert!(matches!(
        panproto_lens::put_without_complement(&rename_lens, &rename_view),
        Err(LensError::NotAnIsomorphism { .. })
    ));

    let restorations = tree_restoration_carrier();
    let (stage, sources) = &restorations[selector_index(selectors[2], restorations.len())];
    let source = prod_tree(&sources[selector_index(selectors[3], sources.len())]);
    let lens = production_stage(stage);
    assert_eq!(independent_tree_obstruction(&lens), None);
    let (view, complement) = panproto_lens::get(&lens, &source)
        .expect("the independent tree classifier admitted the sampled source");
    assert!(complement.residue_is_trivial());
    let restored = panproto_lens::put_without_complement(&lens, &view)
        .expect("an admitted tree isomorphism has complement-free put");
    assert_eq!(ref_tree(&restored), ref_tree(&source));
}

fn assert_random_table_cases(selectors: &[u64]) {
    let projections = table_fixture_carrier();
    let fixture = &projections[selector_index(selectors[0], projections.len())];
    assert_eq!(
        compare_table_classifier_and_put(
            "random projection",
            &fixture.source_schema(),
            &fixture.target_schema(),
            &fixture.migration(),
            std::slice::from_ref(&fixture.source),
            TABLE_FIXTURE_VALUE_KIND,
            ScalarKind::Any,
        ),
        1
    );

    let writers = writer_fixture_carrier();
    let fixture = &writers[selector_index(selectors[1], writers.len())];
    assert_eq!(
        compare_table_classifier_and_put(
            "random writer",
            &fixture.source_schema(),
            &fixture.target_schema(),
            &fixture.migration(),
            &[fixture.source()],
            WRITER_VALUE_KIND,
            ScalarKind::Int {
                bits: 64,
                signed: true,
            },
        ),
        1
    );
}

#[test]
fn reference_lens_constructions() {
    let certificate = lens_construction_count_certificate();
    let mut census = LensConstructionCaseCensus {
        total: 0,
        graph_bundles: 0,
        tree_bundles: 0,
        unit_complement_bundles: 0,
        resolver_cases: 0,
        table_projection_bundles: 0,
        table_writer_bundles: 0,
        span_bundles: 0,
        tree_validation_bundles: 0,
        table_validation_bundles: 0,
    };
    let mut resolver_witnesses = 0_usize;

    for_each_lens_construction_case(|case| {
        census.total += 1;
        match case {
            LensConstructionCase::GraphClassificationsAndRestorations => {
                census.graph_bundles += 1;
                graph_classifier_and_complement_free_put_agree_with_the_structural_oracle();
            }
            LensConstructionCase::TreeClassificationsAndRestorations => {
                census.tree_bundles += 1;
                tree_classifier_and_complement_free_put_agree_with_the_structural_oracle();
            }
            LensConstructionCase::UnitComplementBoundaries => {
                census.unit_complement_bundles += 1;
                assert_eq!(
                    unit_complement_classifiers_reject_scalar_and_target_only_coordinates(),
                    certificate.unit_complement_scenarios
                );
            }
            LensConstructionCase::ResolverRelabel => {
                census.resolver_cases += 1;
                resolver_witnesses += unique_pair_resolver_relabel_is_a_residue_free_bijection();
            }
            LensConstructionCase::ResolverIncoherence => {
                census.resolver_cases += 1;
                resolver_witnesses += incoherent_unique_pair_resolver_is_rejected();
            }
            LensConstructionCase::TableProjectionFixtures => {
                census.table_projection_bundles += 1;
                table_projection_classifier_and_put_agree_with_the_reference();
            }
            LensConstructionCase::TableWriterFixtures => {
                census.table_writer_bundles += 1;
                table_writer_classifier_and_put_agree_with_the_reference();
            }
            LensConstructionCase::SymmetricSpans => {
                census.span_bundles += 1;
                assert_eq!(
                    symmetric_graph_and_table_spans_require_one_canonical_middle(),
                    certificate.span_assertions
                );
            }
            LensConstructionCase::TreeValidationBoundaries => {
                census.tree_validation_bundles += 1;
                assert_eq!(
                    tree_validation_obstructions_are_operational(),
                    certificate.tree_validation_witnesses
                );
            }
            LensConstructionCase::TableValidationBoundaries => {
                census.table_validation_bundles += 1;
                assert_eq!(
                    table_validation_classifier_boundaries_are_operational(),
                    certificate.table_validation_witnesses
                );
            }
        }
    });

    assert_eq!(resolver_witnesses, certificate.resolver_witnesses);
    assert_eq!(census, lens_construction_case_census_certificate());
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn randomized_lens_construction_differential(selectors in any::<[u64; 12]>()) {
        let certificate = lens_construction_count_certificate();
        assert_random_span_case(selectors[0]);
        assert_random_graph_cases(&selectors[1..5]);
        assert_random_tree_cases(&selectors[5..9]);
        assert_random_table_cases(&selectors[9..11]);

        // The focused families are small enough to traverse completely in
        // every generated case. This is stronger than rotating one witness:
        // neither resolver polarity nor a validation boundary can be missed
        // by an unlucky selector distribution.
        assert_eq!(
            unit_complement_classifiers_reject_scalar_and_target_only_coordinates(),
            certificate.unit_complement_scenarios
        );
        assert_eq!(
            unique_pair_resolver_relabel_is_a_residue_free_bijection()
                + incoherent_unique_pair_resolver_is_rejected(),
            certificate.resolver_witnesses
        );
        assert_eq!(
            tree_validation_obstructions_are_operational(),
            certificate.tree_validation_witnesses
        );
        assert_eq!(
            table_validation_classifier_boundaries_are_operational(),
            certificate.table_validation_witnesses
        );
    }
}
