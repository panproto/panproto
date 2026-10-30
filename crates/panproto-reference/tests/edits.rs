#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Algebraic identities of the edit algebra on tiny deltas.

use std::collections::{BTreeMap, BTreeSet};

use panproto_reference::canonical::{self, Writer};
use panproto_reference::edit::{
    self, AttrKey, AttrVal, EditConflict, GElem, GPayload, GraphDelta, Mismatch, NodeMeta, TElem,
    TKey, TPayload, TVal, TableApplyError, TableDelta, TreeEdit,
};
use panproto_reference::encode::{ParentNameRule, TreeEmbedding};
use panproto_reference::enumerate::edits as carrier;
use panproto_reference::model::{Fan, GEdge, Graph, Node, NodeShape, SEdge, Table, Tree};
use panproto_reference::segment::Digest;
use panproto_reference::temporal::{Offset, Temporal};
use panproto_reference::value::{Presence, Value};

fn node(id: u32, a: &str) -> GPayload {
    GPayload::Node(Node::new(id, a))
}

fn graph() -> Graph {
    let mut g = Graph::default();
    g.nodes.insert(0, Node::new(0, "a"));
    g.nodes.insert(1, Node::new(1, "b"));
    g.edges.insert(
        0,
        GEdge {
            src: 0,
            tgt: 1,
            label: SEdge::new("a", "b", "e", None),
            position: None,
        },
    );
    g
}

fn write_shape(writer: &mut Writer, shape: &NodeShape) {
    match shape {
        NodeShape::Plain => writer.tag(0),
        NodeShape::List => writer.tag(1),
        NodeShape::XmlElement(tag) => {
            writer.tag(2);
            writer.str(tag);
        }
        NodeShape::XmlTextSegment => writer.tag(3),
        NodeShape::HyperEdge => writer.tag(4),
    }
}

fn write_value_map(writer: &mut Writer, values: &BTreeMap<String, Value>) -> Option<()> {
    writer.len(values.len());
    for (name, value) in values {
        writer.str(name);
        canonical::write_value(writer, value)?;
    }
    Some(())
}

fn write_presence(writer: &mut Writer, presence: Option<&Presence>) -> Option<()> {
    match presence {
        None => writer.tag(0),
        Some(Presence::Present(value)) => {
            writer.tag(1);
            canonical::write_value(writer, value)?;
        }
        Some(Presence::Null) => writer.tag(2),
        Some(Presence::Absent) => writer.tag(3),
    }
    Some(())
}

fn write_meta(writer: &mut Writer, meta: &NodeMeta) -> Option<()> {
    writer.opt_name(meta.discriminator.as_deref());
    writer.opt_u32(meta.position);
    write_shape(writer, &meta.shape);
    write_value_map(writer, &meta.annotations)
}

fn write_element(writer: &mut Writer, element: &GElem) {
    match element {
        GElem::Node(id) => {
            writer.tag(0);
            writer.u32(*id);
        }
        GElem::Edge(id) => {
            writer.tag(1);
            writer.u32(*id);
        }
    }
}

fn write_payload(writer: &mut Writer, payload: &GPayload) -> Option<()> {
    match payload {
        GPayload::Node(node) => {
            writer.tag(0);
            canonical::write_node(writer, node)
        }
        GPayload::Edge(edge) => {
            writer.tag(1);
            writer.u32(edge.src);
            writer.u32(edge.tgt);
            writer.edge(&edge.label);
            writer.opt_u32(edge.position);
            Some(())
        }
    }
}

fn write_attr_key(writer: &mut Writer, key: &AttrKey) {
    match key {
        AttrKey::Value(node) => {
            writer.tag(0);
            writer.u32(*node);
        }
        AttrKey::Field(node, field) => {
            writer.tag(1);
            writer.u32(*node);
            writer.str(field);
        }
        AttrKey::Meta(node) => {
            writer.tag(2);
            writer.u32(*node);
        }
        AttrKey::Anchor(node) => {
            writer.tag(3);
            writer.u32(*node);
        }
        AttrKey::EdgeLabel(edge) => {
            writer.tag(4);
            writer.u32(*edge);
        }
        AttrKey::EdgePosition(edge) => {
            writer.tag(5);
            writer.u32(*edge);
        }
    }
}

fn write_optional_value(writer: &mut Writer, value: Option<&Value>) -> Option<()> {
    match value {
        None => writer.tag(0),
        Some(value) => {
            writer.tag(1);
            canonical::write_value(writer, value)?;
        }
    }
    Some(())
}

fn write_attr_value(writer: &mut Writer, value: &AttrVal) -> Option<()> {
    match value {
        AttrVal::Presence(presence) => {
            writer.tag(0);
            write_presence(writer, presence.as_ref())
        }
        AttrVal::Field(value) => {
            writer.tag(1);
            write_optional_value(writer, value.as_ref())
        }
        AttrVal::Meta(meta) => {
            writer.tag(2);
            write_meta(writer, meta)
        }
        AttrVal::Anchor(anchor) => {
            writer.tag(3);
            writer.str(anchor);
            Some(())
        }
        AttrVal::Label(label) => {
            writer.tag(4);
            writer.edge(label);
            Some(())
        }
        AttrVal::Position(position) => {
            writer.tag(5);
            writer.opt_u32(*position);
            Some(())
        }
    }
}

fn write_graph_edge_record(writer: &mut Writer, id: u32, edge: &GEdge) {
    writer.u32(id);
    writer.u32(edge.src);
    writer.u32(edge.tgt);
    writer.edge(&edge.label);
    writer.opt_u32(edge.position);
}

fn write_table_pair_record(writer: &mut Writer, edge: &SEdge, src: u32, tgt: u32) {
    writer.edge(edge);
    writer.u32(src);
    writer.u32(tgt);
}

fn write_tree_fan_record(writer: &mut Writer, fan: &Fan) {
    writer.str(&fan.hyper_edge);
    writer.u32(fan.parent);
    writer.len(fan.children.len());
    for (label, child) in &fan.children {
        writer.str(label);
        writer.u32(*child);
    }
}

/// Injective bytes for distinguishing the bounded test cases.
///
/// This deliberately writes one combined `del`/`add`/`attrs` record. It is not
/// E.3a's separated canonical delta layout and is never used as its oracle.
fn delta_witness_bytes(delta: &GraphDelta) -> Option<Vec<u8>> {
    let mut writer = Writer::default();
    writer.len(delta.del.len());
    for (element, payload) in &delta.del {
        write_element(&mut writer, element);
        write_payload(&mut writer, payload)?;
    }
    writer.len(delta.add.len());
    for (element, payload) in &delta.add {
        write_element(&mut writer, element);
        write_payload(&mut writer, payload)?;
    }
    writer.len(delta.attrs.len());
    for (key, (before, after)) in &delta.attrs {
        write_attr_key(&mut writer, key);
        write_attr_value(&mut writer, before)?;
        write_attr_value(&mut writer, after)?;
    }
    Some(writer.bytes)
}

/// Injective length-delimited words of [`delta_witness_bytes`].
fn sequence_witness_bytes(sequence: &[carrier::GraphDeltaCase]) -> Option<Vec<u8>> {
    let mut writer = Writer::default();
    writer.len(sequence.len());
    for case in sequence {
        writer.raw(&delta_witness_bytes(&case.delta)?);
    }
    Some(writer.bytes)
}

fn attribute_delta(key: AttrKey, before: AttrVal, after: AttrVal) -> GraphDelta {
    let mut delta = GraphDelta::identity();
    delta.attrs.insert(key, (before, after));
    delta
}

fn expected_element_deltas() -> Vec<GraphDelta> {
    let mut deltas = vec![GraphDelta::identity()];
    for anchor in ["a", "b"] {
        for presence in [None, Some(Presence::Present(Value::Int(0)))] {
            let mut payload = Node::new(0, anchor);
            payload.value = presence;
            let payload = GPayload::Node(payload);
            let mut delete = GraphDelta::identity();
            delete.del.insert(GElem::Node(0), payload.clone());
            deltas.push(delete);
            let mut add = GraphDelta::identity();
            add.add.insert(GElem::Node(0), payload);
            deltas.push(add);
        }
    }
    let edge_labels = [
        SEdge::new("a", "a", "e", None),
        SEdge::new("a", "a", "e", Some("x")),
    ];
    for label in &edge_labels {
        let payload = GPayload::Edge(GEdge {
            src: 0,
            tgt: 0,
            label: label.clone(),
            position: None,
        });
        let mut edge_delete = GraphDelta::identity();
        edge_delete.del.insert(GElem::Edge(0), payload.clone());
        deltas.push(edge_delete);
        let mut edge_add = GraphDelta::identity();
        edge_add.add.insert(GElem::Edge(0), payload);
        deltas.push(edge_add);
    }
    deltas
}

fn expected_attribute_deltas() -> Vec<GraphDelta> {
    let mut deltas = Vec::new();
    let edge_labels = [
        SEdge::new("a", "a", "e", None),
        SEdge::new("a", "a", "e", Some("x")),
    ];
    deltas.push(attribute_delta(
        AttrKey::Anchor(0),
        AttrVal::Anchor("a".to_owned()),
        AttrVal::Anchor("b".to_owned()),
    ));
    deltas.push(attribute_delta(
        AttrKey::Anchor(0),
        AttrVal::Anchor("b".to_owned()),
        AttrVal::Anchor("a".to_owned()),
    ));
    let absent_presence = AttrVal::Presence(None);
    let zero_presence = AttrVal::Presence(Some(Presence::Present(Value::Int(0))));
    deltas.push(attribute_delta(
        AttrKey::Value(0),
        absent_presence.clone(),
        zero_presence.clone(),
    ));
    deltas.push(attribute_delta(
        AttrKey::Value(0),
        zero_presence,
        absent_presence,
    ));
    let absent_field = AttrVal::Field(None);
    let zero_field = AttrVal::Field(Some(Value::Int(0)));
    deltas.push(attribute_delta(
        AttrKey::Field(0, "f".to_owned()),
        absent_field.clone(),
        zero_field.clone(),
    ));
    deltas.push(attribute_delta(
        AttrKey::Field(0, "f".to_owned()),
        zero_field,
        absent_field,
    ));
    let meta = |shape| {
        AttrVal::Meta(NodeMeta {
            discriminator: None,
            position: None,
            shape,
            annotations: BTreeMap::new(),
        })
    };
    deltas.push(attribute_delta(
        AttrKey::Meta(0),
        meta(NodeShape::Plain),
        meta(NodeShape::HyperEdge),
    ));
    deltas.push(attribute_delta(
        AttrKey::Meta(0),
        meta(NodeShape::HyperEdge),
        meta(NodeShape::Plain),
    ));
    deltas.push(attribute_delta(
        AttrKey::EdgeLabel(0),
        AttrVal::Label(edge_labels[0].clone()),
        AttrVal::Label(edge_labels[1].clone()),
    ));
    deltas.push(attribute_delta(
        AttrKey::EdgeLabel(0),
        AttrVal::Label(edge_labels[1].clone()),
        AttrVal::Label(edge_labels[0].clone()),
    ));
    deltas.push(attribute_delta(
        AttrKey::EdgePosition(0),
        AttrVal::Position(None),
        AttrVal::Position(Some(0)),
    ));
    deltas.push(attribute_delta(
        AttrKey::EdgePosition(0),
        AttrVal::Position(Some(0)),
        AttrVal::Position(None),
    ));
    deltas
}

fn expected_atomic_deltas() -> Vec<GraphDelta> {
    let mut deltas = expected_element_deltas();
    deltas.extend(expected_attribute_deltas());
    deltas
}

#[test]
fn finite_edit_carrier_has_independent_counts_canonical_graphs_and_injective_delta_witnesses() {
    use carrier::GraphDeltaFamily as Family;

    let graphs = carrier::graphs();
    assert_eq!(
        u64::try_from(graphs.len()).unwrap(),
        carrier::graphs_count_certificate()
    );
    let graph_bytes: BTreeSet<Vec<u8>> = graphs
        .iter()
        .map(|graph| {
            assert!(graph.nodes.len() <= 3);
            assert!(graph.edges.len() <= 1);
            assert_eq!(
                graph.nodes.keys().copied().collect::<Vec<_>>(),
                (0..u32::try_from(graph.nodes.len()).unwrap()).collect::<Vec<_>>()
            );
            assert!(graph.nodes.iter().all(|(id, node)| {
                let mut expected = Node::new(*id, "a");
                expected.value.clone_from(&node.value);
                *node == expected
                    && matches!(
                        node.value.as_ref(),
                        None | Some(Presence::Present(Value::Int(0)))
                    )
            }));
            assert!(graph.edges.iter().all(|(id, edge)| {
                *id == 0
                    && graph.nodes.contains_key(&edge.src)
                    && graph.nodes.contains_key(&edge.tgt)
                    && edge.label == SEdge::new("a", "a", "e", None)
                    && edge.position.is_none()
            }));
            let bytes = canonical::graph_bytes(graph).unwrap();
            assert_eq!(canonical::decode_graph(&bytes).as_ref(), Some(graph));
            bytes
        })
        .collect();
    assert_eq!(graph_bytes.len(), graphs.len());

    let cases = carrier::graph_delta_cases();
    let certificate = carrier::graph_delta_count_certificate();
    assert_eq!(u64::try_from(cases.len()).unwrap(), certificate.total());
    assert_eq!(
        cases
            .iter()
            .map(|case| case.delta.clone())
            .collect::<Vec<_>>(),
        carrier::graph_deltas()
    );
    let actual_delta_witnesses: BTreeSet<Vec<u8>> = cases
        .iter()
        .map(|case| delta_witness_bytes(&case.delta).unwrap())
        .collect();
    assert_eq!(actual_delta_witnesses.len(), cases.len());
    assert_eq!(
        actual_delta_witnesses,
        expected_atomic_deltas()
            .iter()
            .map(|delta| delta_witness_bytes(delta).unwrap())
            .collect()
    );

    let mut counts = BTreeMap::new();
    for case in &cases {
        *counts.entry(case.family).or_insert(0u64) += 1;
    }
    assert_eq!(
        counts,
        BTreeMap::from([
            (Family::Identity, certificate.identity),
            (Family::NodeAdd, certificate.node_add),
            (Family::NodeDelete, certificate.node_delete),
            (Family::EdgeAdd, certificate.edge_add),
            (Family::EdgeDelete, certificate.edge_delete),
            (Family::AnchorSet, certificate.anchor_set),
            (Family::ValueSet, certificate.value_set),
            (Family::FieldSet, certificate.field_set),
            (Family::ShapeSet, certificate.shape_set),
            (Family::EdgeLabelSet, certificate.edge_label_set),
            (Family::EdgePositionSet, certificate.edge_position_set),
        ])
    );
}

#[test]
fn graph_delta_words_have_exact_length_major_lexicographic_membership_and_order() {
    let cases = carrier::graph_delta_cases();
    let sequences = carrier::graph_delta_sequences();
    assert_eq!(
        u64::try_from(sequences.len()).unwrap(),
        carrier::graph_delta_sequences_count_certificate()
    );
    let sequence_witnesses: BTreeSet<Vec<u8>> = sequences
        .iter()
        .map(|sequence| sequence_witness_bytes(sequence).unwrap())
        .collect();
    assert_eq!(sequence_witnesses.len(), sequences.len());
    assert!(sequences[0].is_empty());
    assert_eq!(
        sequences[1..=25],
        cases
            .iter()
            .cloned()
            .map(|case| vec![case])
            .collect::<Vec<_>>()
    );
    assert_eq!(sequences[26], vec![cases[0].clone(), cases[0].clone()]);
    assert_eq!(sequences[27], vec![cases[0].clone(), cases[1].clone()]);
    assert_eq!(sequences[50], vec![cases[0].clone(), cases[24].clone()]);
    assert_eq!(sequences[51], vec![cases[1].clone(), cases[0].clone()]);
    assert_eq!(sequences[650], vec![cases[24].clone(), cases[24].clone()]);
    assert_eq!(
        sequences[651],
        vec![cases[0].clone(), cases[0].clone(), cases[0].clone()]
    );
    assert_eq!(
        sequences.last(),
        Some(&vec![
            cases[24].clone(),
            cases[24].clone(),
            cases[24].clone()
        ])
    );
}

#[test]
fn independent_expected_deletions_are_complete_manual_del_maps() {
    let actual: Vec<_> = expected_element_deltas()
        .into_iter()
        .filter(|delta| !delta.del.is_empty())
        .map(|delta| {
            assert!(delta.add.is_empty());
            assert!(delta.attrs.is_empty());
            let mut entries = delta.del.into_iter();
            let entry = entries.next().expect("a deletion witness has one entry");
            assert_eq!(entries.next(), None);
            entry
        })
        .collect();

    let mut expected = Vec::new();
    for anchor in ["a", "b"] {
        for presence in [None, Some(Presence::Present(Value::Int(0)))] {
            let mut node = Node::new(0, anchor);
            node.value = presence;
            expected.push((GElem::Node(0), GPayload::Node(node)));
        }
    }
    for label in [
        SEdge::new("a", "a", "e", None),
        SEdge::new("a", "a", "e", Some("x")),
    ] {
        expected.push((
            GElem::Edge(0),
            GPayload::Edge(GEdge {
                src: 0,
                tgt: 0,
                label,
                position: None,
            }),
        ));
    }
    assert_eq!(actual, expected);
}

#[test]
fn finite_edit_carrier_retains_both_shape_directions() {
    let shapes: Vec<_> = carrier::graph_delta_cases()
        .into_iter()
        .filter(|case| case.family == carrier::GraphDeltaFamily::ShapeSet)
        .map(|case| {
            let (_, (AttrVal::Meta(before), AttrVal::Meta(after))) =
                case.delta.attrs.into_iter().next().unwrap()
            else {
                panic!("shape-tagged case was not a metadata edit");
            };
            (before.shape, after.shape)
        })
        .collect();
    assert_eq!(shapes.len(), 2);
    assert!(shapes.contains(&(NodeShape::Plain, NodeShape::HyperEdge)));
    assert!(shapes.contains(&(NodeShape::HyperEdge, NodeShape::Plain)));
}

#[test]
fn every_delta_family_tag_names_its_constructor() {
    use carrier::GraphDeltaFamily as Family;

    for case in carrier::graph_delta_cases() {
        let delta = &case.delta;
        let is_identity = delta.del.is_empty() && delta.add.is_empty() && delta.attrs.is_empty();
        let one_add = delta.add.len() == 1 && delta.del.is_empty() && delta.attrs.is_empty();
        let one_delete = delta.del.len() == 1 && delta.add.is_empty() && delta.attrs.is_empty();
        let one_attribute = delta.attrs.len() == 1 && delta.add.is_empty() && delta.del.is_empty();
        let matches_constructor = match case.family {
            Family::Identity => is_identity,
            Family::NodeAdd => one_add && matches!(delta.add.keys().next(), Some(GElem::Node(_))),
            Family::NodeDelete => {
                one_delete && matches!(delta.del.keys().next(), Some(GElem::Node(_)))
            }
            Family::EdgeAdd => one_add && matches!(delta.add.keys().next(), Some(GElem::Edge(_))),
            Family::EdgeDelete => {
                one_delete && matches!(delta.del.keys().next(), Some(GElem::Edge(_)))
            }
            Family::AnchorSet => {
                one_attribute && matches!(delta.attrs.keys().next(), Some(AttrKey::Anchor(_)))
            }
            Family::ValueSet => {
                one_attribute && matches!(delta.attrs.keys().next(), Some(AttrKey::Value(_)))
            }
            Family::FieldSet => {
                one_attribute && matches!(delta.attrs.keys().next(), Some(AttrKey::Field(..)))
            }
            Family::ShapeSet => {
                one_attribute && matches!(delta.attrs.keys().next(), Some(AttrKey::Meta(_)))
            }
            Family::EdgeLabelSet => {
                one_attribute && matches!(delta.attrs.keys().next(), Some(AttrKey::EdgeLabel(_)))
            }
            Family::EdgePositionSet => {
                one_attribute && matches!(delta.attrs.keys().next(), Some(AttrKey::EdgePosition(_)))
            }
        };
        assert!(matches_constructor, "wrong constructor tag: {case:?}");
    }
}

#[test]
fn c8_scope_is_complete_with_its_fixed_parameters() {
    let carrier::C8ScopeStatus::Complete(scope) = carrier::c8_scope_status();
    assert_eq!(scope.vertex_names, ["a", "b"]);
    assert_eq!(scope.edge_kind, "e");
    assert_eq!(scope.column, "f");
    assert_eq!(scope.values, [None, Some(0), Some(1)]);
    assert_eq!(scope.schema_vertices, 2);
    assert_eq!(scope.schema_edges, 2);
    assert_eq!(scope.graph_nodes, 3);
    assert_eq!(scope.graph_edges, 2);
    assert_eq!(scope.table_rows, 3);
    assert_eq!(scope.table_pairs, 2);
    assert_eq!(scope.tree_nodes, 3);
    assert_eq!(scope.sequence_len, 3);
    let schemas = carrier::c8_schemas();
    assert_eq!(
        u64::try_from(schemas.len()).unwrap(),
        carrier::c8_schemas_count_certificate()
    );
    let distinct: BTreeSet<_> = schemas
        .iter()
        .map(|schema| (schema.vertices.clone(), schema.edges.clone()))
        .collect();
    assert_eq!(distinct.len(), schemas.len());
}

#[test]
fn c8_initial_states_match_their_independent_counts() {
    let schemas = carrier::c8_schemas();
    let count = |states: &dyn Fn(&carrier::C8Schema) -> usize| -> u64 {
        schemas
            .iter()
            .map(|schema| u64::try_from(states(schema)).unwrap())
            .sum()
    };
    assert_eq!(
        count(&|schema| carrier::c8_graphs(schema).len()),
        carrier::c8_graph_counts_certificate().states
    );
    assert_eq!(
        count(&|schema| carrier::c8_tables(schema).len()),
        carrier::c8_table_counts_certificate().states
    );
    assert_eq!(
        count(&|schema| carrier::c8_trees(schema).len()),
        carrier::c8_tree_counts_certificate().states
    );
    for schema in &schemas {
        for graph in carrier::c8_graphs(schema) {
            assert!(graph.well_formed());
            assert!(graph.edges.values().all(|edge| {
                schema.edge(
                    &graph.nodes[&edge.src].anchor,
                    &graph.nodes[&edge.tgt].anchor,
                ) == Some(&edge.label)
            }));
        }
        for table in carrier::c8_tables(schema) {
            assert!(table.well_formed() && table.pairs.len() <= 2);
        }
        for tree in carrier::c8_trees(schema) {
            assert!(panproto_reference::encode::tree_to_graph(&tree, None).is_ok());
        }
    }
}

#[test]
fn add_then_delete_cancels_and_delete_then_add_replaces() {
    let mut add = GraphDelta::identity();
    add.add.insert(GElem::Node(5), node(5, "a"));
    let del = add.invert();
    assert_eq!(add.compose(&del), Ok(GraphDelta::identity()));
    let repl = del.compose(&add).unwrap();
    assert_eq!(repl.del.len() + repl.add.len(), 2);
    assert_eq!(add.invert().invert(), add);
}

#[test]
fn composition_is_partial_and_both_bracketings_may_refuse() {
    // a = Add(q), b = Attr(k: q[k] -> n), c = Del(r) with r[k] = n and
    // r != q[k := n]: both bracketings refuse.
    let mut a = GraphDelta::identity();
    a.add.insert(GElem::Node(5), node(5, "a"));
    let mut b = GraphDelta::identity();
    b.attrs.insert(
        AttrKey::Anchor(5),
        (AttrVal::Anchor("a".into()), AttrVal::Anchor("z".into())),
    );
    let mut c = GraphDelta::identity();
    let mut r = Node::new(5, "z");
    r.discriminator = Some("d".into());
    c.del.insert(GElem::Node(5), GPayload::Node(r));
    assert!(a.compose(&b).is_ok() && b.compose(&c).is_ok());
    assert!(a.compose(&b).unwrap().compose(&c).is_err());
    assert!(a.compose(&b.compose(&c).unwrap()).is_err());
    // attr after add then delete is defined, attr before add is not.
    assert!(matches!(b.compose(&a), Err(Mismatch::Elem(_))));
}

#[test]
fn the_differ_applies_and_regularity_holds() {
    let g = graph();
    let mut h = g.clone();
    h.nodes.get_mut(&1).unwrap().anchor = "c".into();
    h.edges.get_mut(&0).unwrap().tgt = 0;
    h.nodes.insert(7, Node::new(7, "a"));
    let d = edit::diff(&g, &h);
    assert_eq!(edit::apply(&d, &g), Ok(h.clone()));
    assert_eq!(edit::diff(&g, &g), GraphDelta::identity());
    assert_eq!(edit::apply(&d.invert(), &h), Ok(g));
    let back = d.compose(&d.invert()).unwrap().compose(&d).unwrap();
    assert_eq!(back, d);
}

#[test]
fn independent_deltas_commute_and_merge() {
    let g = graph();
    let mut a = GraphDelta::identity();
    a.add.insert(GElem::Node(9), node(9, "a"));
    let mut b = GraphDelta::identity();
    b.attrs.insert(
        AttrKey::Field(1, "x".into()),
        (
            AttrVal::Field(None),
            AttrVal::Field(Some(panproto_reference::value::Value::Int(1))),
        ),
    );
    assert!(edit::independence(&a, &b).is_empty());
    let ab = edit::apply(&b, &edit::apply(&a, &g).unwrap()).unwrap();
    let ba = edit::apply(&a, &edit::apply(&b, &g).unwrap()).unwrap();
    assert_eq!(ab, ba);
    assert_eq!(edit::apply(&edit::merge(&a, &b).unwrap(), &g).unwrap(), ab);
    assert!(!edit::independence(&a, &a).is_empty());
    // Deleting a node another delta re-anchors is a conflict.
    let mut del = GraphDelta::identity();
    del.del.insert(GElem::Node(0), node(0, "a"));
    let mut anchor = GraphDelta::identity();
    anchor.attrs.insert(
        AttrKey::Anchor(0),
        (AttrVal::Anchor("a".into()), AttrVal::Anchor("q".into())),
    );
    assert!(!edit::independence(&del, &anchor).is_empty());
    assert_eq!(
        edit::apply(&del, &g),
        Err(edit::ApplyError::DanglingEdges {
            node: 0,
            edges: vec![0],
        })
    );
}

#[test]
fn replacement_deletion_conflicts_with_every_use_of_the_replaced_node() {
    let mut replacement = GraphDelta::identity();
    replacement.del.insert(GElem::Node(0), node(0, "a"));
    replacement.add.insert(GElem::Node(0), node(0, "replaced"));

    let mut field_use = GraphDelta::identity();
    field_use.attrs.insert(
        AttrKey::Field(0, "x".into()),
        (AttrVal::Field(None), AttrVal::Field(Some(Value::Int(1)))),
    );
    assert_eq!(
        edit::independence(&replacement, &field_use),
        vec![EditConflict::DeleteUse {
            deleted: GElem::Node(0),
            user: edit::ElemRef::Attr(AttrKey::Field(0, "x".into())),
        }]
    );
    assert_eq!(
        edit::merge(&replacement, &field_use),
        Err(EditConflict::DeleteUse {
            deleted: GElem::Node(0),
            user: edit::ElemRef::Attr(AttrKey::Field(0, "x".into())),
        })
    );

    let mut edge_use = GraphDelta::identity();
    edge_use.add.insert(
        GElem::Edge(7),
        GPayload::Edge(GEdge {
            src: 0,
            tgt: 1,
            label: SEdge::new("a", "b", "use", None),
            position: None,
        }),
    );
    assert!(
        edit::independence(&replacement, &edge_use).contains(&EditConflict::DeleteUse {
            deleted: GElem::Node(0),
            user: edit::ElemRef::Elem(GElem::Edge(7)),
        })
    );
}

#[test]
fn typed_delta_carriers_refuse_every_cross_sort_payload_and_attribute() {
    let g = graph();
    let edge = g.edges[&0].clone();
    let mut wrong_payload = GraphDelta::identity();
    wrong_payload
        .del
        .insert(GElem::Node(0), GPayload::Edge(edge));
    assert_eq!(
        edit::apply(&wrong_payload, &g),
        Err(edit::ApplyError::ElementTypeMismatch(GElem::Node(0)))
    );

    let mut wrong_attribute = GraphDelta::identity();
    wrong_attribute.attrs.insert(
        AttrKey::Anchor(0),
        (AttrVal::Field(None), AttrVal::Field(Some(Value::Int(3)))),
    );
    assert_eq!(
        edit::apply(&wrong_attribute, &g),
        Err(edit::ApplyError::AttributeTypeMismatch(AttrKey::Anchor(0)))
    );
    assert!(!wrong_payload.is_normal());
    assert!(!wrong_attribute.is_normal());
    assert!(edit::canonical_graph_delta_bytes(&wrong_payload).is_none());
    assert_eq!(
        wrong_payload.compose(&GraphDelta::identity()),
        Err(Mismatch::Elem(GElem::Node(0)))
    );
    assert_eq!(
        GraphDelta::identity().compose(&wrong_attribute),
        Err(Mismatch::Attr(AttrKey::Anchor(0)))
    );

    let mut misfiled = GraphDelta::identity();
    misfiled
        .add
        .insert(GElem::Node(7), GPayload::Node(Node::new(8, "a")));
    assert_eq!(
        edit::apply(&misfiled, &g),
        Err(edit::ApplyError::ElementTypeMismatch(GElem::Node(7)))
    );
}

fn table() -> Table {
    let mut table = Table::default();
    table.rows.insert(
        0,
        ("a".into(), BTreeMap::from([("x".into(), Value::Int(1))])),
    );
    table.rows.insert(1, ("b".into(), BTreeMap::new()));
    table.pairs.insert((SEdge::new("a", "b", "fk", None), 0, 1));
    table
}

#[test]
fn table_apply_reports_exact_missing_stale_duplicate_dangling_and_type_errors() {
    let table = table();
    let row = TPayload::Row("a".into(), BTreeMap::from([("x".into(), Value::Int(1))]));
    let pair = SEdge::new("a", "b", "fk", None);

    let mut missing = TableDelta::identity();
    missing.del.insert(TElem::Row(9), row.clone());
    assert_eq!(
        edit::apply_table(&missing, &table),
        Err(TableApplyError::RowNotFound(9))
    );

    let mut stale = TableDelta::identity();
    stale
        .del
        .insert(TElem::Row(0), TPayload::Row("a".into(), BTreeMap::new()));
    assert_eq!(
        edit::apply_table(&stale, &table),
        Err(TableApplyError::StaleCell {
            row: 0,
            column: "x".into(),
        })
    );

    let mut dangling = TableDelta::identity();
    dangling.del.insert(TElem::Row(0), row);
    assert!(matches!(
        edit::apply_table(&dangling, &table),
        Err(TableApplyError::DanglingForeignKeys { row: 0, pairs }) if pairs == vec![(pair.clone(), 0, 1)]
    ));

    let mut duplicate = TableDelta::identity();
    duplicate
        .add
        .insert(TElem::Row(0), TPayload::Row("a".into(), BTreeMap::new()));
    assert_eq!(
        edit::apply_table(&duplicate, &table),
        Err(TableApplyError::DuplicateRow(0))
    );

    let mut endpoint = TableDelta::identity();
    endpoint
        .add
        .insert(TElem::Pair(pair.clone(), 0, 9), TPayload::Pair);
    assert_eq!(
        edit::apply_table(&endpoint, &table),
        Err(TableApplyError::MissingRowEndpoint { edge: pair, row: 9 })
    );

    let mistyped = SEdge::new("x", "b", "fk", None);
    let mut mistyped_pair = TableDelta::identity();
    mistyped_pair
        .add
        .insert(TElem::Pair(mistyped.clone(), 0, 1), TPayload::Pair);
    assert_eq!(
        edit::apply_table(&mistyped_pair, &table),
        Err(TableApplyError::MissingRowEndpoint {
            edge: mistyped,
            row: 0,
        })
    );

    let kept = SEdge::new("a", "b", "fk", None);
    let mut move_source = TableDelta::identity();
    move_source.attrs.insert(
        TKey::Table(0),
        (TVal::Table("a".into()), TVal::Table("c".into())),
    );
    assert_eq!(
        edit::apply_table(&move_source, &table),
        Err(TableApplyError::MissingRowEndpoint { edge: kept, row: 0 })
    );

    let mut wrong = TableDelta::identity();
    wrong.del.insert(TElem::Row(0), TPayload::Pair);
    assert_eq!(
        edit::apply_table(&wrong, &table),
        Err(TableApplyError::ElementTypeMismatch(TElem::Row(0)))
    );
    let mut wrong_attr = TableDelta::identity();
    wrong_attr.attrs.insert(
        TKey::Cell(0, "x".into()),
        (TVal::Table("a".into()), TVal::Table("b".into())),
    );
    assert_eq!(
        edit::apply_table(&wrong_attr, &table),
        Err(TableApplyError::AttributeTypeMismatch(TKey::Cell(
            0,
            "x".into()
        )))
    );
}

#[test]
fn table_apply_validation_phases_have_fixed_error_precedence() {
    let table = table();
    let row = TPayload::Row("a".into(), BTreeMap::from([("x".into(), Value::Int(1))]));
    let bad_attribute = (
        TKey::Cell(0, "x".into()),
        (TVal::Table("a".into()), TVal::Table("b".into())),
    );

    let mut deletion_before_addition = TableDelta::identity();
    deletion_before_addition
        .del
        .insert(TElem::Row(9), row.clone());
    deletion_before_addition
        .add
        .insert(TElem::Row(8), TPayload::Pair);
    deletion_before_addition
        .attrs
        .insert(bad_attribute.0.clone(), bad_attribute.1.clone());
    assert_eq!(
        edit::apply_table(&deletion_before_addition, &table),
        Err(TableApplyError::RowNotFound(9))
    );

    let mut addition_before_attribute_and_incidence = TableDelta::identity();
    addition_before_attribute_and_incidence
        .del
        .insert(TElem::Row(0), row.clone());
    addition_before_attribute_and_incidence
        .add
        .insert(TElem::Row(8), TPayload::Pair);
    addition_before_attribute_and_incidence
        .attrs
        .insert(bad_attribute.0.clone(), bad_attribute.1.clone());
    assert_eq!(
        edit::apply_table(&addition_before_attribute_and_incidence, &table),
        Err(TableApplyError::ElementTypeMismatch(TElem::Row(8)))
    );

    let mut attribute_before_incidence = TableDelta::identity();
    attribute_before_incidence.del.insert(TElem::Row(0), row);
    attribute_before_incidence
        .attrs
        .insert(bad_attribute.0.clone(), bad_attribute.1);
    assert_eq!(
        edit::apply_table(&attribute_before_incidence, &table),
        Err(TableApplyError::AttributeTypeMismatch(bad_attribute.0))
    );
}

#[test]
fn table_apply_chooses_the_least_key_within_a_validation_phase() {
    let table = table();
    let missing_row = TPayload::Row("a".into(), BTreeMap::new());
    let mut delta = TableDelta::identity();
    delta.del.insert(TElem::Row(9), missing_row.clone());
    delta.del.insert(TElem::Row(8), missing_row);

    assert_eq!(
        edit::apply_table(&delta, &table),
        Err(TableApplyError::RowNotFound(8))
    );
}

#[test]
fn table_apply_deletes_before_adding_a_replacement_row() {
    let mut before = Table::default();
    before.rows.insert(
        7,
        (
            "old".into(),
            BTreeMap::from([("value".into(), Value::Int(1))]),
        ),
    );
    let mut delta = TableDelta::identity();
    delta.del.insert(
        TElem::Row(7),
        TPayload::Row(
            "old".into(),
            BTreeMap::from([("value".into(), Value::Int(1))]),
        ),
    );
    delta.add.insert(
        TElem::Row(7),
        TPayload::Row(
            "new".into(),
            BTreeMap::from([("value".into(), Value::Int(2))]),
        ),
    );

    let mut expected = Table::default();
    expected.rows.insert(
        7,
        (
            "new".into(),
            BTreeMap::from([("value".into(), Value::Int(2))]),
        ),
    );
    assert_eq!(edit::apply_table(&delta, &before), Ok(expected));
}

#[test]
fn table_apply_late_errors_do_not_mutate_the_input() {
    let table = table();
    let saved = table.clone();
    let saved_bytes = canonical::table_bytes(&saved).unwrap();
    let pair = SEdge::new("a", "b", "fk", None);

    let mut duplicate = TableDelta::identity();
    duplicate.attrs.insert(
        TKey::Cell(0, "x".into()),
        (
            TVal::Cell(Some(Value::Int(1))),
            TVal::Cell(Some(Value::Int(2))),
        ),
    );
    duplicate
        .add
        .insert(TElem::Pair(pair.clone(), 0, 1), TPayload::Pair);
    assert_eq!(
        edit::apply_table(&duplicate, &table),
        Err(TableApplyError::DuplicateForeignKey {
            edge: pair,
            src: 0,
            tgt: 1,
        })
    );
    assert_eq!(table, saved);
    assert_eq!(canonical::table_bytes(&table), Some(saved_bytes.clone()));

    let endpoint = SEdge::new("a", "b", "late", None);
    let mut missing_endpoint = TableDelta::identity();
    missing_endpoint.attrs.insert(
        TKey::Cell(0, "x".into()),
        (
            TVal::Cell(Some(Value::Int(1))),
            TVal::Cell(Some(Value::Int(2))),
        ),
    );
    missing_endpoint
        .add
        .insert(TElem::Pair(endpoint.clone(), 0, 9), TPayload::Pair);
    assert_eq!(
        edit::apply_table(&missing_endpoint, &table),
        Err(TableApplyError::MissingRowEndpoint {
            edge: endpoint,
            row: 9,
        })
    );
    assert_eq!(table, saved);
    assert_eq!(canonical::table_bytes(&table), Some(saved_bytes));
}

#[test]
fn graph_apply_reports_every_structural_refusal_variant_directly() {
    let graph = graph();
    let mut missing_node = GraphDelta::identity();
    missing_node.del.insert(GElem::Node(9), node(9, "missing"));
    assert_eq!(
        edit::apply(&missing_node, &graph),
        Err(edit::ApplyError::NodeNotFound(9))
    );

    let mut missing_edge = GraphDelta::identity();
    missing_edge.del.insert(
        GElem::Edge(9),
        GPayload::Edge(GEdge {
            src: 0,
            tgt: 1,
            label: SEdge::new("a", "b", "missing", None),
            position: None,
        }),
    );
    assert_eq!(
        edit::apply(&missing_edge, &graph),
        Err(edit::ApplyError::EdgeNotFound(9))
    );

    let mut duplicate_node = GraphDelta::identity();
    duplicate_node.add.insert(GElem::Node(1), node(1, "b"));
    assert_eq!(
        edit::apply(&duplicate_node, &graph),
        Err(edit::ApplyError::DuplicateNodeId(1))
    );
    let mut duplicate_edge = GraphDelta::identity();
    duplicate_edge
        .add
        .insert(GElem::Edge(0), GPayload::Edge(graph.edges[&0].clone()));
    assert_eq!(
        edit::apply(&duplicate_edge, &graph),
        Err(edit::ApplyError::DuplicateEdge(0))
    );

    let mut missing_endpoint = GraphDelta::identity();
    missing_endpoint.add.insert(
        GElem::Edge(7),
        GPayload::Edge(GEdge {
            src: 0,
            tgt: 99,
            label: SEdge::new("a", "missing", "e", None),
            position: None,
        }),
    );
    assert_eq!(
        edit::apply(&missing_endpoint, &graph),
        Err(edit::ApplyError::MissingEndpoint { edge: 7, node: 99 })
    );

    let mut malformed_fan = GraphDelta::identity();
    let mut fan = Node::new(9, "fan");
    fan.shape = NodeShape::HyperEdge;
    malformed_fan
        .add
        .insert(GElem::Node(9), GPayload::Node(fan));
    assert_eq!(
        edit::apply(&malformed_fan, &graph),
        Err(edit::ApplyError::MalformedFan {
            node: 9,
            reason: panproto_reference::fan::FanViolation::MissingParent,
        })
    );
}

#[test]
fn table_row_surface_conversion_checks_sorted_cells_then_complete_incidence() {
    let mut table = Table::default();
    table.rows.insert(
        0,
        ("t".into(), BTreeMap::from([("b".into(), Value::Int(2))])),
    );
    table.rows.insert(1, ("u".into(), BTreeMap::new()));
    let incoming = (SEdge::new("u", "t", "in", None), 1, 0);
    let outgoing = (SEdge::new("t", "u", "out", None), 0, 1);
    table.pairs.extend([incoming.clone(), outgoing.clone()]);

    let expected_cells = BTreeMap::from([("a".into(), Value::Int(1))]);
    let mut stale_cells = TableDelta::identity();
    stale_cells
        .del
        .insert(TElem::Row(0), TPayload::Row("t".into(), expected_cells));
    assert_eq!(
        edit::apply_table(&stale_cells, &table),
        Err(TableApplyError::StaleCell {
            row: 0,
            column: "a".into(),
        })
    );

    let exact_cells = BTreeMap::from([("b".into(), Value::Int(2))]);
    assert_eq!(
        edit::delete_row_delta(&table, 0, "t", &exact_cells, &BTreeSet::new()),
        Err(TableApplyError::StaleIncidence { row: 0 })
    );
    let incidence = BTreeSet::from([incoming, outgoing]);
    let deletion = edit::delete_row_delta(&table, 0, "t", &exact_cells, &incidence).unwrap();
    assert_eq!(deletion.del.len(), 3);
    let after = edit::apply_table(&deletion, &table).unwrap();
    assert_eq!(after.rows.keys().copied().collect::<Vec<_>>(), vec![1]);
    assert!(after.pairs.is_empty());
}

#[test]
fn graph_apply_uses_the_exact_stale_variant_for_every_attribute_sort() {
    let graph = graph();
    let cases = [
        (
            AttrKey::Value(0),
            AttrVal::Presence(Some(Presence::Null)),
            AttrVal::Presence(None),
            edit::ApplyError::StaleValue { node: 0 },
        ),
        (
            AttrKey::Field(0, "missing".into()),
            AttrVal::Field(Some(Value::Int(1))),
            AttrVal::Field(None),
            edit::ApplyError::StaleField {
                node: 0,
                field: "missing".into(),
            },
        ),
        (
            AttrKey::Meta(0),
            AttrVal::Meta(NodeMeta {
                discriminator: None,
                position: None,
                shape: NodeShape::List,
                annotations: BTreeMap::new(),
            }),
            AttrVal::Meta(NodeMeta {
                discriminator: None,
                position: None,
                shape: NodeShape::Plain,
                annotations: BTreeMap::new(),
            }),
            edit::ApplyError::StaleMeta { node: 0 },
        ),
        (
            AttrKey::Anchor(0),
            AttrVal::Anchor("wrong".into()),
            AttrVal::Anchor("next".into()),
            edit::ApplyError::StaleAnchor { node: 0 },
        ),
        (
            AttrKey::EdgeLabel(0),
            AttrVal::Label(SEdge::new("a", "b", "wrong", None)),
            AttrVal::Label(SEdge::new("a", "b", "next", None)),
            edit::ApplyError::StaleLabel { edge: 0 },
        ),
        (
            AttrKey::EdgePosition(0),
            AttrVal::Position(Some(1)),
            AttrVal::Position(Some(2)),
            edit::ApplyError::StalePosition { edge: 0 },
        ),
    ];
    for (key, old, new, expected) in cases {
        let mut delta = GraphDelta::identity();
        delta.attrs.insert(key, (old, new));
        assert_eq!(edit::apply(&delta, &graph), Err(expected));
    }

    let mut stale_node = GraphDelta::identity();
    stale_node
        .del
        .insert(GElem::Node(0), GPayload::Node(Node::new(0, "wrong")));
    assert_eq!(
        edit::apply(&stale_node, &graph),
        Err(edit::ApplyError::StaleNode { node: 0 })
    );
    let mut stale_edge = GraphDelta::identity();
    let mut wrong_edge = graph.edges[&0].clone();
    wrong_edge.label.kind = "wrong".into();
    stale_edge
        .del
        .insert(GElem::Edge(0), GPayload::Edge(wrong_edge));
    assert_eq!(
        edit::apply(&stale_edge, &graph),
        Err(edit::ApplyError::StaleEdge { edge: 0 })
    );
}

#[test]
fn graph_apply_names_an_absent_attribute_owner_before_staleness() {
    let graph = graph();
    let cases = [
        (
            AttrKey::Anchor(99),
            AttrVal::Anchor("a".into()),
            AttrVal::Anchor("b".into()),
            edit::ApplyError::NodeNotFound(99),
        ),
        (
            AttrKey::EdgeLabel(99),
            AttrVal::Label(SEdge::new("a", "b", "old", None)),
            AttrVal::Label(SEdge::new("a", "b", "new", None)),
            edit::ApplyError::EdgeNotFound(99),
        ),
        (
            AttrKey::EdgePosition(99),
            AttrVal::Position(None),
            AttrVal::Position(Some(0)),
            edit::ApplyError::EdgeNotFound(99),
        ),
    ];
    for (key, old, new, expected) in cases {
        let mut delta = GraphDelta::identity();
        delta.attrs.insert(key, (old, new));
        assert_eq!(edit::apply(&delta, &graph), Err(expected));
    }
}

#[test]
fn table_apply_uses_exact_foreign_key_and_row_attribute_errors() {
    let table = table();
    let pair = SEdge::new("a", "b", "fk", None);

    let absent = SEdge::new("a", "b", "other", None);
    let mut absent_pair = TableDelta::identity();
    absent_pair
        .del
        .insert(TElem::Pair(absent.clone(), 0, 1), TPayload::Pair);
    assert_eq!(
        edit::apply_table(&absent_pair, &table),
        Err(TableApplyError::ForeignKeyNotFound {
            edge: absent,
            src: 0,
            tgt: 1,
        })
    );

    let mut duplicate = TableDelta::identity();
    duplicate
        .add
        .insert(TElem::Pair(pair.clone(), 0, 1), TPayload::Pair);
    assert_eq!(
        edit::apply_table(&duplicate, &table),
        Err(TableApplyError::DuplicateForeignKey {
            edge: pair,
            src: 0,
            tgt: 1,
        })
    );

    let mut missing_row = TableDelta::identity();
    missing_row.attrs.insert(
        TKey::Cell(9, "x".into()),
        (TVal::Cell(None), TVal::Cell(Some(Value::Int(1)))),
    );
    assert_eq!(
        edit::apply_table(&missing_row, &table),
        Err(TableApplyError::RowNotFound(9))
    );

    let mut stale_table = TableDelta::identity();
    stale_table.attrs.insert(
        TKey::Table(0),
        (TVal::Table("wrong".into()), TVal::Table("next".into())),
    );
    assert_eq!(
        edit::apply_table(&stale_table, &table),
        Err(TableApplyError::StaleTable { row: 0 })
    );
}

#[test]
fn table_delta_diff_invert_compose_are_the_same_partial_algebra() {
    let before = table();
    let mut after = before.clone();
    after
        .rows
        .get_mut(&0)
        .unwrap()
        .1
        .insert("x".into(), Value::Int(2));
    after.rows.insert(2, ("b".into(), BTreeMap::new()));
    let delta = edit::diff_table(&before, &after);
    assert_eq!(edit::apply_table(&delta, &before), Ok(after.clone()));
    let inverse = edit::invert_table(&delta);
    assert_eq!(edit::apply_table(&inverse, &after), Ok(before));
    assert_eq!(
        edit::compose_table(&delta, &inverse),
        Ok(TableDelta::identity())
    );
}

fn join_tree() -> Tree {
    Tree {
        nodes: [
            (0, Node::new(0, "root")),
            (1, Node::new(1, "feature")),
            (2, Node::new(2, "feature")),
            (3, Node::new(3, "leaf")),
            (4, Node::new(4, "leaf")),
        ]
        .into_iter()
        .collect(),
        arcs: vec![
            (0, 1, SEdge::new("root", "feature", "child", Some("a"))),
            (0, 2, SEdge::new("root", "feature", "child", Some("b"))),
            (2, 3, SEdge::new("feature", "leaf", "child", Some("c"))),
            (2, 4, SEdge::new("feature", "leaf", "child", Some("d"))),
        ],
        fans: vec![Fan {
            parent: 2,
            hyper_edge: "fan".into(),
            children: BTreeMap::from([("arm".into(), 3)]),
        }],
        root: 0,
    }
}

fn literal_join_graph() -> Graph {
    let mut graph = Graph {
        nodes: join_tree().nodes,
        edges: BTreeMap::new(),
    };
    let mut fan_node = Node::new(5, "fan");
    fan_node.shape = NodeShape::HyperEdge;
    graph.nodes.insert(5, fan_node);
    for (id, src, tgt, label) in [
        (0, 0, 1, SEdge::new("root", "feature", "child", Some("a"))),
        (1, 0, 2, SEdge::new("root", "feature", "child", Some("b"))),
        (2, 2, 3, SEdge::new("feature", "leaf", "child", Some("c"))),
        (3, 2, 4, SEdge::new("feature", "leaf", "child", Some("d"))),
        (
            5,
            5,
            3,
            SEdge::new("fan", "leaf", "hyper_label", Some("arm")),
        ),
    ] {
        graph.edges.insert(
            id,
            GEdge {
                src,
                tgt,
                label,
                position: None,
            },
        );
    }
    graph.edges.insert(
        4,
        GEdge {
            src: 5,
            tgt: 2,
            label: SEdge::new("fan", "feature", "hyper_parent", None),
            position: None,
        },
    );
    graph
}

fn graph_payload(graph: &Graph, element: &GElem) -> GPayload {
    match element {
        GElem::Node(id) => GPayload::Node(graph.nodes[id].clone()),
        GElem::Edge(id) => GPayload::Edge(graph.edges[id].clone()),
    }
}

fn literal_deletions(graph: &Graph, elements: impl IntoIterator<Item = GElem>) -> GraphDelta {
    let mut delta = GraphDelta::identity();
    for element in elements {
        delta
            .del
            .insert(element.clone(), graph_payload(graph, &element));
    }
    delta
}

fn assert_literal_tree_lowering(
    tree: &Tree,
    edit_step: &TreeEdit,
    expected: &GraphDelta,
    expected_floors: (u32, u32),
) {
    let (before, embedding) = panproto_reference::encode::tree_to_graph(tree, None).unwrap();
    let mut followed = embedding.clone();
    let actual = edit::graph_delta_from_tree_edit(tree, &mut followed, edit_step, None).unwrap();
    assert_eq!(&actual, expected);
    assert_eq!((followed.next_node, followed.next_edge), expected_floors);
    assert!(edit::apply(expected, &before).unwrap().well_formed());
    assert_eq!(
        edit::tree_edit_from_graph_delta(tree, &embedding, expected, None),
        Ok(edit_step.clone())
    );
}

fn assert_tree_steps_read_back_exactly(cases: Vec<(Tree, TreeEdit)>) {
    for (tree, expected) in cases {
        let (_, embedding) = panproto_reference::encode::tree_to_graph(&tree, None).unwrap();
        let mut followed = embedding.clone();
        let delta =
            edit::graph_delta_from_tree_edit(&tree, &mut followed, &expected, None).unwrap();
        assert_eq!(
            edit::tree_edit_from_graph_delta(&tree, &embedding, &delta, None),
            Ok(expected)
        );
    }
}

#[test]
fn structural_tree_lowerings_match_independent_literal_deltas() {
    let tree = join_tree();
    let graph = literal_join_graph();
    assert_eq!(
        panproto_reference::encode::tree_to_graph(&tree, None)
            .unwrap()
            .0,
        graph
    );
    assert_literal_tree_lowering(&tree, &TreeEdit::Identity, &GraphDelta::identity(), (6, 6));

    let insert = TreeEdit::InsertNode {
        parent: 1,
        child_id: 6,
        node: Node::new(6, "leaf"),
        edge: SEdge::new("feature", "leaf", "child", Some("new")),
    };
    let mut insert_delta = GraphDelta::identity();
    insert_delta.add.insert(GElem::Node(6), node(6, "leaf"));
    insert_delta.add.insert(
        GElem::Edge(6),
        GPayload::Edge(GEdge {
            src: 1,
            tgt: 6,
            label: SEdge::new("feature", "leaf", "child", Some("new")),
            position: None,
        }),
    );
    assert_literal_tree_lowering(&tree, &insert, &insert_delta, (7, 7));

    let delete = TreeEdit::DeleteNode { id: 4 };
    let delete_delta = literal_deletions(&graph, [GElem::Node(4), GElem::Edge(3)]);
    assert_literal_tree_lowering(&tree, &delete, &delete_delta, (6, 6));

    let move_edit = TreeEdit::MoveSubtree {
        node_id: 3,
        new_parent: 1,
        edge: SEdge::new("feature", "leaf", "child", Some("moved")),
    };
    let mut move_delta = literal_deletions(&graph, [GElem::Edge(2)]);
    move_delta.add.insert(
        GElem::Edge(6),
        GPayload::Edge(GEdge {
            src: 1,
            tgt: 3,
            label: SEdge::new("feature", "leaf", "child", Some("moved")),
            position: None,
        }),
    );
    assert_literal_tree_lowering(&tree, &move_edit, &move_delta, (6, 7));
}

#[test]
fn contract_and_fan_lowerings_match_independent_literal_deltas() {
    let tree = join_tree();
    let graph = literal_join_graph();
    let contract = TreeEdit::ContractNode { id: 2 };
    let mut contract_delta = literal_deletions(
        &graph,
        [
            GElem::Node(2),
            GElem::Node(5),
            GElem::Edge(1),
            GElem::Edge(2),
            GElem::Edge(3),
            GElem::Edge(4),
            GElem::Edge(5),
        ],
    );
    for (id, tgt, name) in [(6, 3, "c"), (7, 4, "d")] {
        contract_delta.add.insert(
            GElem::Edge(id),
            GPayload::Edge(GEdge {
                src: 0,
                tgt,
                label: SEdge::new("root", "leaf", "child", Some(name)),
                position: None,
            }),
        );
    }
    assert_literal_tree_lowering(&tree, &contract, &contract_delta, (6, 8));

    let insert_fan = TreeEdit::InsertFan {
        fan: Fan {
            parent: 1,
            hyper_edge: "second-fan".into(),
            children: BTreeMap::from([("arm".into(), 4)]),
        },
    };
    let mut fan_delta = GraphDelta::identity();
    let mut fan_node = Node::new(6, "second-fan");
    fan_node.shape = NodeShape::HyperEdge;
    fan_delta
        .add
        .insert(GElem::Node(6), GPayload::Node(fan_node));
    for (id, tgt, kind, name) in [
        (6, 1, "hyper_parent", None),
        (7, 4, "hyper_label", Some("arm")),
    ] {
        fan_delta.add.insert(
            GElem::Edge(id),
            GPayload::Edge(GEdge {
                src: 6,
                tgt,
                label: SEdge::new(
                    "second-fan",
                    if tgt == 1 { "feature" } else { "leaf" },
                    kind,
                    name,
                ),
                position: None,
            }),
        );
    }
    assert_literal_tree_lowering(&tree, &insert_fan, &fan_delta, (7, 8));

    let delete_fan = TreeEdit::DeleteFan {
        hyper_edge: "fan".into(),
    };
    let delete_fan_delta =
        literal_deletions(&graph, [GElem::Node(5), GElem::Edge(4), GElem::Edge(5)]);
    assert_literal_tree_lowering(&tree, &delete_fan, &delete_fan_delta, (6, 6));
}

#[test]
fn payload_tree_lowerings_match_independent_literal_deltas() {
    let tree = join_tree();
    let graph = literal_join_graph();
    let relabel = TreeEdit::RelabelNode {
        id: 3,
        new_anchor: "renamed".into(),
    };
    let mut relabel_delta = GraphDelta::identity();
    relabel_delta.attrs.insert(
        AttrKey::Anchor(3),
        (
            AttrVal::Anchor("leaf".into()),
            AttrVal::Anchor("renamed".into()),
        ),
    );
    relabel_delta.attrs.insert(
        AttrKey::EdgeLabel(5),
        (
            AttrVal::Label(SEdge::new("fan", "leaf", "hyper_label", Some("arm"))),
            AttrVal::Label(SEdge::new("fan", "renamed", "hyper_label", Some("arm"))),
        ),
    );
    assert_literal_tree_lowering(&tree, &relabel, &relabel_delta, (6, 6));

    let set = TreeEdit::SetField {
        node_id: 1,
        field: "new".into(),
        value: Value::Bool(true),
    };
    let set_delta = attribute_delta(
        AttrKey::Field(1, "new".into()),
        AttrVal::Field(None),
        AttrVal::Field(Some(Value::Bool(true))),
    );
    assert_literal_tree_lowering(&tree, &set, &set_delta, (6, 6));

    let mut with_field = tree.clone();
    with_field
        .nodes
        .get_mut(&1)
        .unwrap()
        .fields
        .insert("old".into(), Value::Int(7));
    let remove = TreeEdit::RemoveField {
        node_id: 1,
        field: "old".into(),
    };
    let remove_delta = attribute_delta(
        AttrKey::Field(1, "old".into()),
        AttrVal::Field(Some(Value::Int(7))),
        AttrVal::Field(None),
    );
    assert_literal_tree_lowering(&with_field, &remove, &remove_delta, (6, 6));

    let mut value_node = tree.nodes[&3].clone();
    value_node.value = Some(Presence::Present(Value::Int(9)));
    let replace_value = TreeEdit::JoinFeatures {
        primary: 3,
        joined: Vec::new(),
        produce: value_node,
    };
    let value_delta = attribute_delta(
        AttrKey::Value(3),
        AttrVal::Presence(None),
        AttrVal::Presence(Some(Presence::Present(Value::Int(9)))),
    );
    assert_eq!(graph, literal_join_graph());
    assert_literal_tree_lowering(&tree, &replace_value, &value_delta, (6, 6));
}

#[test]
fn tree_reader_keeps_anchor_and_field_only_changes_as_ordered_primitives() {
    let tree = join_tree();
    let (_, embedding) = panproto_reference::encode::tree_to_graph(&tree, None).unwrap();
    let delta = GraphDelta {
        del: BTreeMap::new(),
        add: BTreeMap::new(),
        attrs: BTreeMap::from([
            (
                AttrKey::Field(1, "field".into()),
                (AttrVal::Field(None), AttrVal::Field(Some(Value::Int(7)))),
            ),
            (
                AttrKey::Anchor(1),
                (
                    AttrVal::Anchor("feature".into()),
                    AttrVal::Anchor("renamed".into()),
                ),
            ),
        ]),
    };
    assert_eq!(
        edit::tree_edit_from_graph_delta(&tree, &embedding, &delta, None),
        Ok(TreeEdit::Sequence(vec![
            TreeEdit::RelabelNode {
                id: 1,
                new_anchor: "renamed".into(),
            },
            TreeEdit::SetField {
                node_id: 1,
                field: "field".into(),
                value: Value::Int(7),
            },
        ]))
    );
}

#[test]
fn tree_reader_coalesces_value_or_meta_nodes_after_every_other_step() {
    let tree = join_tree();
    let graph = literal_join_graph();
    let (_, embedding) = panproto_reference::encode::tree_to_graph(&tree, None).unwrap();
    let mut produce = tree.nodes[&1].clone();
    produce.anchor = "coalesced".into();
    produce.value = Some(Presence::Null);
    produce.fields.insert("field".into(), Value::Int(7));
    produce.shape = NodeShape::List;
    let mut delta = literal_deletions(&graph, [GElem::Edge(2)]);
    delta.add.insert(
        GElem::Edge(6),
        GPayload::Edge(GEdge {
            src: 1,
            tgt: 3,
            label: SEdge::new("feature", "leaf", "child", Some("moved")),
            position: None,
        }),
    );
    delta.attrs.extend([
        (
            AttrKey::Value(1),
            (
                AttrVal::Presence(None),
                AttrVal::Presence(Some(Presence::Null)),
            ),
        ),
        (
            AttrKey::Field(1, "field".into()),
            (AttrVal::Field(None), AttrVal::Field(Some(Value::Int(7)))),
        ),
        (
            AttrKey::Meta(1),
            (
                AttrVal::Meta(NodeMeta {
                    discriminator: None,
                    position: None,
                    shape: NodeShape::Plain,
                    annotations: BTreeMap::new(),
                }),
                AttrVal::Meta(NodeMeta {
                    discriminator: None,
                    position: None,
                    shape: NodeShape::List,
                    annotations: BTreeMap::new(),
                }),
            ),
        ),
        (
            AttrKey::Anchor(1),
            (
                AttrVal::Anchor("feature".into()),
                AttrVal::Anchor("coalesced".into()),
            ),
        ),
    ]);
    assert_eq!(
        edit::tree_edit_from_graph_delta(&tree, &embedding, &delta, None),
        Ok(TreeEdit::Sequence(vec![
            TreeEdit::MoveSubtree {
                node_id: 3,
                new_parent: 1,
                edge: SEdge::new("feature", "leaf", "child", Some("moved")),
            },
            TreeEdit::JoinFeatures {
                primary: 1,
                joined: Vec::new(),
                produce,
            },
        ]))
    );
}

#[test]
fn join_and_sequence_lowerings_match_independent_literal_deltas() {
    let tree = join_tree();
    let graph = literal_join_graph();
    let mut produce = tree.nodes[&1].clone();
    produce.anchor = "joined".into();
    let join = TreeEdit::JoinFeatures {
        primary: 1,
        joined: vec![2],
        produce,
    };
    let mut join_delta = literal_deletions(
        &graph,
        [
            GElem::Node(2),
            GElem::Node(5),
            GElem::Edge(1),
            GElem::Edge(2),
            GElem::Edge(3),
            GElem::Edge(4),
            GElem::Edge(5),
        ],
    );
    join_delta.attrs.insert(
        AttrKey::Anchor(1),
        (
            AttrVal::Anchor("feature".into()),
            AttrVal::Anchor("joined".into()),
        ),
    );
    for (id, tgt, name) in [(6, 3, "c"), (7, 4, "d")] {
        join_delta.add.insert(
            GElem::Edge(id),
            GPayload::Edge(GEdge {
                src: 1,
                tgt,
                label: SEdge::new("joined", "leaf", "child", Some(name)),
                position: None,
            }),
        );
    }
    assert_literal_tree_lowering(&tree, &join, &join_delta, (6, 8));
}

#[test]
fn composite_tree_readback_uses_canonical_primitive_order() {
    let tree = join_tree();
    let graph = literal_join_graph();
    let sequence = TreeEdit::Sequence(vec![
        TreeEdit::MoveSubtree {
            node_id: 3,
            new_parent: 1,
            edge: SEdge::new("feature", "leaf", "child", Some("moved")),
        },
        TreeEdit::RelabelNode {
            id: 3,
            new_anchor: "renamed".into(),
        },
        TreeEdit::SetField {
            node_id: 1,
            field: "joined".into(),
            value: Value::Bool(true),
        },
    ]);
    let mut sequence_delta = literal_deletions(&graph, [GElem::Edge(2)]);
    sequence_delta.add.insert(
        GElem::Edge(6),
        GPayload::Edge(GEdge {
            src: 1,
            tgt: 3,
            label: SEdge::new("feature", "leaf", "child", Some("moved")),
            position: None,
        }),
    );
    sequence_delta.attrs.extend([
        (
            AttrKey::Field(1, "joined".into()),
            (
                AttrVal::Field(None),
                AttrVal::Field(Some(Value::Bool(true))),
            ),
        ),
        (
            AttrKey::Anchor(3),
            (
                AttrVal::Anchor("leaf".into()),
                AttrVal::Anchor("renamed".into()),
            ),
        ),
        (
            AttrKey::EdgeLabel(5),
            (
                AttrVal::Label(SEdge::new("fan", "leaf", "hyper_label", Some("arm"))),
                AttrVal::Label(SEdge::new("fan", "renamed", "hyper_label", Some("arm"))),
            ),
        ),
    ]);
    let (_, embedding) = panproto_reference::encode::tree_to_graph(&tree, None).unwrap();
    let mut followed = embedding.clone();
    assert_eq!(
        edit::graph_delta_from_tree_edit(&tree, &mut followed, &sequence, None),
        Ok(sequence_delta.clone())
    );
    assert_eq!((followed.next_node, followed.next_edge), (6, 7));

    let canonical = TreeEdit::Sequence(vec![
        TreeEdit::MoveSubtree {
            node_id: 3,
            new_parent: 1,
            edge: SEdge::new("feature", "leaf", "child", Some("moved")),
        },
        TreeEdit::RelabelNode {
            id: 3,
            new_anchor: "renamed".into(),
        },
        TreeEdit::SetField {
            node_id: 1,
            field: "joined".into(),
            value: Value::Bool(true),
        },
    ]);
    assert_eq!(
        edit::tree_edit_from_graph_delta(&tree, &embedding, &sequence_delta, None),
        Ok(canonical.clone())
    );
    let mut canonical_embedding = embedding;
    assert_eq!(
        edit::graph_delta_from_tree_edit(&tree, &mut canonical_embedding, &canonical, None),
        Ok(sequence_delta)
    );
}

fn assert_invalid_tree_embedding(
    tree: &Tree,
    mut embedding: panproto_reference::encode::TreeEmbedding,
) {
    let unchanged = embedding.clone();
    assert_eq!(
        edit::apply_tree(&TreeEdit::Identity, tree, &mut embedding),
        Err(edit::TreeApplyError::InvalidEmbedding)
    );
    assert_eq!(embedding, unchanged);
}

fn assert_tree_apply_refusal(
    tree: &Tree,
    embedding: &panproto_reference::encode::TreeEmbedding,
    edit_step: &TreeEdit,
    expected: edit::TreeApplyError,
) {
    let mut actual_embedding = embedding.clone();
    assert_eq!(
        edit::apply_tree(edit_step, tree, &mut actual_embedding),
        Err(expected)
    );
    assert_eq!(&actual_embedding, embedding);
}

#[test]
fn tree_embedding_requires_exact_ordered_disjoint_carriers_and_durable_floors() {
    let mut tree = join_tree();
    tree.fans.push(Fan {
        parent: 1,
        hyper_edge: "second-fan".into(),
        children: BTreeMap::from([("arm".into(), 4)]),
    });
    let (_, embedding) = panproto_reference::encode::tree_to_graph(&tree, None).unwrap();
    assert_eq!(
        embedding.arcs.values().copied().collect::<Vec<_>>(),
        vec![0, 1, 2, 3]
    );
    assert_eq!(
        embedding
            .fans
            .iter()
            .map(|(node, _, _)| *node)
            .collect::<Vec<_>>(),
        vec![5, 6]
    );

    let mut missing_arc = embedding.clone();
    missing_arc.arcs.remove(&4);
    assert_invalid_tree_embedding(&tree, missing_arc);
    let mut extra_arc = embedding.clone();
    extra_arc.arcs.insert(99, 99);
    assert_invalid_tree_embedding(&tree, extra_arc);
    let mut nonascending_arcs = embedding.clone();
    nonascending_arcs.arcs.insert(1, embedding.arcs[&2]);
    nonascending_arcs.arcs.insert(2, embedding.arcs[&1]);
    assert_invalid_tree_embedding(&tree, nonascending_arcs);
    let mut duplicate_arc = embedding.clone();
    duplicate_arc.arcs.insert(2, embedding.arcs[&1]);
    assert_invalid_tree_embedding(&tree, duplicate_arc);

    let mut nonascending_fans = embedding.clone();
    nonascending_fans.fans[0].0 = embedding.fans[1].0;
    nonascending_fans.fans[1].0 = embedding.fans[0].0;
    assert_invalid_tree_embedding(&tree, nonascending_fans);
    let mut missing_fan = embedding.clone();
    missing_fan.fans.pop();
    assert_invalid_tree_embedding(&tree, missing_fan);
    let mut duplicate_fan_node = embedding.clone();
    duplicate_fan_node.fans[1].0 = embedding.fans[0].0;
    assert_invalid_tree_embedding(&tree, duplicate_fan_node);
    let mut fan_node_overlaps_tree = embedding.clone();
    fan_node_overlaps_tree.fans[0].0 = tree.root;
    assert_invalid_tree_embedding(&tree, fan_node_overlaps_tree);
    let mut missing_fan_child = embedding.clone();
    missing_fan_child.fans[0].2.remove("arm");
    assert_invalid_tree_embedding(&tree, missing_fan_child);
    let mut extra_fan_child = embedding.clone();
    extra_fan_child.fans[0].2.insert("extra".into(), 99);
    assert_invalid_tree_embedding(&tree, extra_fan_child);
    let mut fan_edge_overlaps_arc = embedding.clone();
    fan_edge_overlaps_arc.fans[0].1 = embedding.arcs[&1];
    assert_invalid_tree_embedding(&tree, fan_edge_overlaps_arc);
    let mut fan_edges_overlap = embedding.clone();
    fan_edges_overlap.fans[1]
        .2
        .insert("arm".into(), embedding.fans[0].1);
    assert_invalid_tree_embedding(&tree, fan_edges_overlap);
    let mut stale_node_floor = embedding.clone();
    stale_node_floor.next_node = embedding.fans[1].0;
    assert_invalid_tree_embedding(&tree, stale_node_floor);
    let mut stale_edge_floor = embedding.clone();
    stale_edge_floor.next_edge = embedding.fans[1].2["arm"];
    assert_invalid_tree_embedding(&tree, stale_edge_floor);
}

#[test]
fn insert_node_relocates_the_complete_fan_suffix_and_roundtrips() {
    let mut tree = join_tree();
    tree.fans.push(Fan {
        parent: 1,
        hyper_edge: "second-fan".into(),
        children: BTreeMap::from([("arm".into(), 4)]),
    });
    let (before_graph, embedding) = panproto_reference::encode::tree_to_graph(&tree, None).unwrap();
    let old_fans = embedding.fans.clone();
    let edit_step = TreeEdit::InsertNode {
        parent: 1,
        child_id: old_fans[0].0,
        node: Node::new(old_fans[0].0, "leaf"),
        edge: SEdge::new("feature", "leaf", "child", Some("inserted")),
    };
    let mut applied_embedding = embedding.clone();
    let after = edit::apply_tree(&edit_step, &tree, &mut applied_embedding).unwrap();
    assert_eq!(
        applied_embedding
            .fans
            .iter()
            .map(|(node, parent, children)| (*node, *parent, children["arm"]))
            .collect::<Vec<_>>(),
        vec![(7, 8, 9), (8, 10, 11)]
    );
    assert_eq!(applied_embedding.arcs[&5], 12);
    assert_eq!(
        (applied_embedding.next_node, applied_embedding.next_edge),
        (9, 13)
    );

    let mut lowered_embedding = embedding.clone();
    let delta =
        edit::graph_delta_from_tree_edit(&tree, &mut lowered_embedding, &edit_step, None).unwrap();
    assert_eq!(lowered_embedding, applied_embedding);
    assert_eq!(
        edit::tree_edit_from_graph_delta(&tree, &embedding, &delta, None),
        Ok(edit_step)
    );
    let after_graph = edit::apply(&delta, &before_graph).unwrap();
    assert_eq!(after.nodes[&5], Node::new(5, "leaf"));
    assert_eq!(
        after_graph.nodes.keys().copied().collect::<Vec<_>>(),
        vec![0, 1, 2, 3, 4, 5, 7, 8]
    );
    assert_eq!(
        after_graph.edges.keys().copied().collect::<Vec<_>>(),
        vec![0, 1, 2, 3, 8, 9, 10, 11, 12]
    );
}

#[test]
fn tree_apply_names_every_node_and_structure_precondition_exactly() {
    let tree = join_tree();
    let (_, embedding) = panproto_reference::encode::tree_to_graph(&tree, None).unwrap();
    let leaf = Node::new(6, "leaf");
    let edge = SEdge::new("feature", "leaf", "child", Some("new"));
    assert_tree_apply_refusal(
        &tree,
        &embedding,
        &TreeEdit::InsertNode {
            parent: 99,
            child_id: 6,
            node: leaf,
            edge: edge.clone(),
        },
        edit::TreeApplyError::ParentNotFound(99),
    );
    assert_tree_apply_refusal(
        &tree,
        &embedding,
        &TreeEdit::InsertNode {
            parent: 1,
            child_id: 1,
            node: Node::new(1, "feature"),
            edge: edge.clone(),
        },
        edit::TreeApplyError::DuplicateNodeId(1),
    );
    assert_tree_apply_refusal(
        &tree,
        &embedding,
        &TreeEdit::InsertNode {
            parent: 1,
            child_id: 6,
            node: Node::new(7, "leaf"),
            edge,
        },
        edit::TreeApplyError::MisfiledNode {
            expected: 6,
            actual: 7,
        },
    );
    assert_tree_apply_refusal(
        &tree,
        &embedding,
        &TreeEdit::DeleteNode { id: 99 },
        edit::TreeApplyError::NodeNotFound(99),
    );
    assert_tree_apply_refusal(
        &tree,
        &embedding,
        &TreeEdit::DeleteNode { id: 0 },
        edit::TreeApplyError::RootOperation(0),
    );
    assert_tree_apply_refusal(
        &tree,
        &embedding,
        &TreeEdit::DeleteNode { id: 2 },
        edit::TreeApplyError::NotLeaf(2),
    );
}

#[test]
fn tree_apply_names_every_cycle_fan_join_and_allocation_refusal_exactly() {
    let tree = join_tree();
    let (_, embedding) = panproto_reference::encode::tree_to_graph(&tree, None).unwrap();
    assert_tree_apply_refusal(
        &tree,
        &embedding,
        &TreeEdit::MoveSubtree {
            node_id: 2,
            new_parent: 3,
            edge: tree.arcs[1].2.clone(),
        },
        edit::TreeApplyError::Cycle { node: 2, parent: 3 },
    );
    assert_tree_apply_refusal(
        &tree,
        &embedding,
        &TreeEdit::JoinFeatures {
            primary: 1,
            joined: vec![1],
            produce: tree.nodes[&1].clone(),
        },
        edit::TreeApplyError::InvalidJoin(1),
    );
    assert_tree_apply_refusal(
        &tree,
        &embedding,
        &TreeEdit::DeleteFan {
            hyper_edge: "absent".into(),
        },
        edit::TreeApplyError::FanNotFound("absent".into()),
    );
    assert_tree_apply_refusal(
        &tree,
        &embedding,
        &TreeEdit::InsertFan {
            fan: Fan {
                parent: 1,
                hyper_edge: "dangling".into(),
                children: BTreeMap::from([("arm".into(), 99)]),
            },
        },
        edit::TreeApplyError::NodeNotFound(99),
    );
    let mut exhausted = embedding.clone();
    exhausted.next_edge = u32::MAX;
    assert_tree_apply_refusal(
        &tree,
        &exhausted,
        &TreeEdit::MoveSubtree {
            node_id: 3,
            new_parent: 1,
            edge: tree.arcs[2].2.clone(),
        },
        edit::TreeApplyError::IdsExhausted,
    );
    let mut node_exhausted = embedding.clone();
    node_exhausted.next_node = u32::MAX;
    assert_tree_apply_refusal(
        &tree,
        &node_exhausted,
        &TreeEdit::InsertFan {
            fan: Fan {
                parent: 1,
                hyper_edge: "exhausted".into(),
                children: BTreeMap::from([("arm".into(), 4)]),
            },
        },
        edit::TreeApplyError::IdsExhausted,
    );
    let mut reserved = tree;
    reserved.nodes.get_mut(&1).unwrap().shape = NodeShape::HyperEdge;
    assert_tree_apply_refusal(
        &reserved,
        &embedding,
        &TreeEdit::Identity,
        edit::TreeApplyError::ReservedVocabulary,
    );
}

#[test]
fn structural_tree_edit_variants_read_back_with_exact_lowerings() {
    let base = join_tree();
    assert_tree_steps_read_back_exactly(vec![
        (base.clone(), TreeEdit::Identity),
        (
            base.clone(),
            TreeEdit::InsertNode {
                parent: 1,
                child_id: 6,
                node: Node::new(6, "leaf"),
                edge: SEdge::new("feature", "leaf", "child", Some("new")),
            },
        ),
        (base.clone(), TreeEdit::DeleteNode { id: 4 }),
        (base.clone(), TreeEdit::ContractNode { id: 2 }),
        (
            base.clone(),
            TreeEdit::MoveSubtree {
                node_id: 3,
                new_parent: 1,
                edge: SEdge::new("feature", "leaf", "child", Some("moved")),
            },
        ),
        (
            base.clone(),
            TreeEdit::InsertFan {
                fan: Fan {
                    parent: 1,
                    hyper_edge: "second-fan".into(),
                    children: BTreeMap::from([("arm".into(), 4)]),
                },
            },
        ),
        (
            base,
            TreeEdit::DeleteFan {
                hyper_edge: "fan".into(),
            },
        ),
    ]);
}

#[test]
fn payload_tree_edit_variants_read_back_with_exact_lowerings() {
    let base = join_tree();
    let mut with_field = base.clone();
    with_field
        .nodes
        .get_mut(&1)
        .unwrap()
        .fields
        .insert("old".into(), Value::Int(7));
    let mut joined = base.nodes[&1].clone();
    joined.anchor = "joined".into();
    let mut value_joined = base.nodes[&3].clone();
    value_joined.value = Some(Presence::Present(Value::Int(9)));
    assert_tree_steps_read_back_exactly(vec![
        (
            base.clone(),
            TreeEdit::RelabelNode {
                id: 3,
                new_anchor: "renamed".into(),
            },
        ),
        (
            base.clone(),
            TreeEdit::SetField {
                node_id: 1,
                field: "new".into(),
                value: Value::Bool(true),
            },
        ),
        (
            with_field,
            TreeEdit::RemoveField {
                node_id: 1,
                field: "old".into(),
            },
        ),
        (
            base.clone(),
            TreeEdit::JoinFeatures {
                primary: 3,
                joined: Vec::new(),
                produce: value_joined,
            },
        ),
        (
            base,
            TreeEdit::JoinFeatures {
                primary: 1,
                joined: vec![2],
                produce: joined,
            },
        ),
    ]);
}

#[test]
fn tree_join_reader_preserves_nonascending_join_order_exactly() {
    let mut tree = join_tree();
    tree.nodes.insert(5, Node::new(5, "leaf"));
    tree.arcs
        .push((1, 5, SEdge::new("feature", "leaf", "child", Some("e"))));
    let (_, embedding) = panproto_reference::encode::tree_to_graph(&tree, None).unwrap();
    let join = TreeEdit::JoinFeatures {
        primary: 0,
        joined: vec![2, 1],
        produce: tree.nodes[&0].clone(),
    };
    let mut followed = embedding.clone();
    let delta = edit::graph_delta_from_tree_edit(&tree, &mut followed, &join, None).unwrap();
    let recovered = edit::tree_edit_from_graph_delta(&tree, &embedding, &delta, None).unwrap();
    assert_eq!(recovered, join);
    let mut recovered_embedding = embedding;
    let after = edit::apply_tree(&recovered, &tree, &mut recovered_embedding).unwrap();
    assert_eq!(
        after
            .arcs
            .iter()
            .map(|(parent, child, _)| (*parent, *child))
            .collect::<Vec<_>>(),
        vec![(0, 3), (0, 4), (0, 5)]
    );
}

#[test]
fn composite_tree_reading_does_not_allocate_for_unchanged_fans() {
    let tree = join_tree();
    let (_, mut embedding) = panproto_reference::encode::tree_to_graph(&tree, None).unwrap();
    embedding.next_node = u32::MAX;
    let composite = TreeEdit::Sequence(vec![
        TreeEdit::SetField {
            node_id: 0,
            field: "root-field".into(),
            value: Value::Int(10),
        },
        TreeEdit::SetField {
            node_id: 1,
            field: "feature-field".into(),
            value: Value::Int(11),
        },
    ]);
    let mut followed = embedding.clone();
    let delta = edit::graph_delta_from_tree_edit(&tree, &mut followed, &composite, None).unwrap();
    assert_eq!(followed.next_node, u32::MAX);
    let recovered = edit::tree_edit_from_graph_delta(&tree, &embedding, &delta, None).unwrap();
    let mut recovered_embedding = embedding;
    let after = edit::apply_tree(&recovered, &tree, &mut recovered_embedding).unwrap();
    let mut expected = tree;
    expected
        .nodes
        .get_mut(&0)
        .unwrap()
        .fields
        .insert("root-field".into(), Value::Int(10));
    expected
        .nodes
        .get_mut(&1)
        .unwrap()
        .fields
        .insert("feature-field".into(), Value::Int(11));
    assert_eq!(after, expected);
    assert_eq!(recovered_embedding.next_node, u32::MAX);
}

#[test]
fn tree_join_preserves_tr14_order_updates_embedding_floors_and_roundtrips_lowering() {
    let tree = join_tree();
    let (graph, embedding) = panproto_reference::encode::tree_to_graph(&tree, None).unwrap();
    let start_embedding = embedding.clone();
    let mut produce = tree.nodes[&1].clone();
    produce.anchor = "merged".into();
    let join = TreeEdit::JoinFeatures {
        primary: 1,
        joined: vec![2],
        produce,
    };

    let mut applied_embedding = embedding.clone();
    let applied = edit::apply_tree(&join, &tree, &mut applied_embedding).unwrap();
    assert_eq!(
        applied
            .arcs
            .iter()
            .map(|(parent, child, _)| (*parent, *child))
            .collect::<Vec<_>>(),
        vec![(0, 1), (1, 3), (1, 4)]
    );
    assert!(applied.fans.is_empty());
    assert_eq!(applied_embedding.next_edge, embedding.next_edge + 2);
    assert!(applied_embedding.next_node >= embedding.next_node);

    let mut lowered_embedding = embedding;
    let delta =
        edit::graph_delta_from_tree_edit(&tree, &mut lowered_embedding, &join, None).unwrap();
    assert_eq!(
        edit::apply(&delta, &graph).unwrap().nodes[&1].anchor,
        "merged"
    );
    assert_eq!(lowered_embedding, applied_embedding);
    assert_eq!(
        edit::tree_edit_from_graph_delta(&tree, &start_embedding, &delta, None),
        Ok(join)
    );
}

#[test]
fn graph_delta_to_tree_edit_inverts_composite_lowerings_and_rejects_nonimages() {
    let tree = join_tree();
    let (graph, embedding) = panproto_reference::encode::tree_to_graph(&tree, None).unwrap();
    let composite = TreeEdit::Sequence(vec![
        TreeEdit::MoveSubtree {
            node_id: 3,
            new_parent: 1,
            edge: SEdge::new("feature", "leaf", "child", Some("moved")),
        },
        TreeEdit::RelabelNode {
            id: 3,
            new_anchor: "renamed".into(),
        },
        TreeEdit::SetField {
            node_id: 1,
            field: "joined".into(),
            value: Value::Bool(true),
        },
    ]);
    let mut expected = tree.clone();
    expected.arcs = vec![
        (0, 1, SEdge::new("root", "feature", "child", Some("a"))),
        (0, 2, SEdge::new("root", "feature", "child", Some("b"))),
        (2, 4, SEdge::new("feature", "leaf", "child", Some("d"))),
        (1, 3, SEdge::new("feature", "leaf", "child", Some("moved"))),
    ];
    expected.nodes.get_mut(&3).unwrap().anchor = "renamed".into();
    expected
        .nodes
        .get_mut(&1)
        .unwrap()
        .fields
        .insert("joined".into(), Value::Bool(true));
    let mut lowered_embedding = embedding.clone();
    let delta =
        edit::graph_delta_from_tree_edit(&tree, &mut lowered_embedding, &composite, None).unwrap();
    let recovered = edit::tree_edit_from_graph_delta(&tree, &embedding, &delta, None).unwrap();
    let mut recovered_embedding = embedding.clone();
    assert_eq!(
        edit::apply_tree(&recovered, &tree, &mut recovered_embedding),
        Ok(expected)
    );

    let old_edge = embedding.arcs[&3];
    let mut below_floor = GraphDelta::identity();
    below_floor.del.insert(
        GElem::Edge(old_edge),
        GPayload::Edge(graph.edges[&old_edge].clone()),
    );
    let mut replacement = graph.edges[&old_edge].clone();
    replacement.src = 1;
    below_floor
        .add
        .insert(GElem::Edge(old_edge), GPayload::Edge(replacement));
    assert!(matches!(
        edit::tree_edit_from_graph_delta(&tree, &embedding, &below_floor, None),
        Err(edit::TreeApplyError::NotTreeEdit { reason }) if reason.contains("below the embedding floor")
    ));

    let mut positioned = GraphDelta::identity();
    positioned.attrs.insert(
        AttrKey::EdgePosition(old_edge),
        (AttrVal::Position(None), AttrVal::Position(Some(0))),
    );
    assert!(matches!(
        edit::tree_edit_from_graph_delta(&tree, &embedding, &positioned, None),
        Err(edit::TreeApplyError::NotTreeEdit { reason }) if reason.contains("edge position")
    ));

    let mut relabelled_arc = GraphDelta::identity();
    let old_label = graph.edges[&old_edge].label.clone();
    let mut new_label = old_label.clone();
    new_label.kind = "other".into();
    relabelled_arc.attrs.insert(
        AttrKey::EdgeLabel(old_edge),
        (AttrVal::Label(old_label), AttrVal::Label(new_label)),
    );
    assert!(matches!(
        edit::tree_edit_from_graph_delta(&tree, &embedding, &relabelled_arc, None),
        Err(edit::TreeApplyError::NotTreeEdit { reason }) if reason.contains("target relabel")
    ));

    let fan_edge = embedding.fans[0].2["arm"];
    let fan_label = graph.edges[&fan_edge].label.clone();
    let mut uninduced = fan_label.clone();
    uninduced.tgt = "not-renamed".into();
    let mut relabelled_fan = GraphDelta::identity();
    relabelled_fan.attrs.insert(
        AttrKey::EdgeLabel(fan_edge),
        (AttrVal::Label(fan_label), AttrVal::Label(uninduced)),
    );
    assert!(matches!(
        edit::tree_edit_from_graph_delta(&tree, &embedding, &relabelled_fan, None),
        Err(edit::TreeApplyError::NotTreeEdit { reason }) if reason.contains("target relabel")
    ));
}

#[test]
fn tree_reader_rejects_root_replacements_even_when_the_payload_is_unchanged() {
    let tree = Tree {
        nodes: BTreeMap::from([(0, Node::new(0, "root"))]),
        arcs: Vec::new(),
        fans: Vec::new(),
        root: 0,
    };
    let (_, embedding) = panproto_reference::encode::tree_to_graph(&tree, None).unwrap();
    for replacement in [Node::new(0, "root"), Node::new(0, "different")] {
        let mut delta = GraphDelta::identity();
        delta.del.insert(GElem::Node(0), node(0, "root"));
        delta
            .add
            .insert(GElem::Node(0), GPayload::Node(replacement));
        assert!(matches!(
            edit::tree_edit_from_graph_delta(&tree, &embedding, &delta, None),
            Err(edit::TreeApplyError::NotTreeEdit { reason })
                if reason.contains("fixed carriers")
        ));
    }
}

#[test]
fn tree_reader_rejects_partial_deletion_of_duplicate_named_fans() {
    let tree = duplicate_fan_tree();
    let (graph, embedding) = literal_duplicate_fan_graph_and_embedding();
    let (fan_node, parent_edge, child_edges) = &embedding.fans[1];
    let mut delta = GraphDelta::identity();
    for element in std::iter::once(GElem::Node(*fan_node)).chain(
        std::iter::once(GElem::Edge(*parent_edge))
            .chain(child_edges.values().copied().map(GElem::Edge)),
    ) {
        let payload = match element {
            GElem::Node(id) => GPayload::Node(graph.nodes[&id].clone()),
            GElem::Edge(id) => GPayload::Edge(graph.edges[&id].clone()),
        };
        delta.del.insert(element, payload);
    }
    assert!(matches!(
        edit::tree_edit_from_graph_delta(&tree, &embedding, &delta, None),
        Err(edit::TreeApplyError::NotTreeEdit { reason })
            if reason.contains("fixed carriers")
    ));
}

#[test]
fn conflict_classifier_has_direct_witnesses_for_all_six_variants() {
    let mut delete_a = GraphDelta::identity();
    let mut delete_b = GraphDelta::identity();
    delete_a.del.insert(GElem::Node(8), node(8, "a"));
    delete_b.del.insert(GElem::Node(8), node(8, "a"));
    assert!(
        edit::independence(&delete_a, &delete_b).contains(&EditConflict::DeleteDelete {
            elem: GElem::Node(8),
        })
    );

    let update_a = attribute_delta(
        AttrKey::Field(0, "x".into()),
        AttrVal::Field(None),
        AttrVal::Field(Some(Value::Int(1))),
    );
    let update_b = attribute_delta(
        AttrKey::Field(0, "x".into()),
        AttrVal::Field(None),
        AttrVal::Field(Some(Value::Int(2))),
    );
    assert!(
        edit::independence(&update_a, &update_b).contains(&EditConflict::UpdateUpdate {
            key: AttrKey::Field(0, "x".into()),
        })
    );

    let mut add_a = GraphDelta::identity();
    let mut add_b = GraphDelta::identity();
    add_a.add.insert(GElem::Node(8), node(8, "a"));
    add_b.add.insert(GElem::Node(8), node(8, "b"));
    assert!(
        edit::independence(&add_a, &add_b).contains(&EditConflict::AddAdd {
            elem: GElem::Node(8),
        })
    );

    let anchor = attribute_delta(
        AttrKey::Anchor(0),
        AttrVal::Anchor("a".into()),
        AttrVal::Anchor("b".into()),
    );
    let mut incident = GraphDelta::identity();
    incident.add.insert(
        GElem::Edge(7),
        GPayload::Edge(GEdge {
            src: 0,
            tgt: 1,
            label: SEdge::new("a", "b", "ordinary", None),
            position: None,
        }),
    );
    assert!(
        edit::independence(&anchor, &incident).contains(&EditConflict::AnchorUse {
            node: 0,
            user: edit::ElemRef::Elem(GElem::Edge(7)),
        })
    );

    let shape = attribute_delta(
        AttrKey::Meta(0),
        AttrVal::Meta(NodeMeta {
            discriminator: None,
            position: None,
            shape: NodeShape::Plain,
            annotations: BTreeMap::new(),
        }),
        AttrVal::Meta(NodeMeta {
            discriminator: None,
            position: None,
            shape: NodeShape::HyperEdge,
            annotations: BTreeMap::new(),
        }),
    );
    let shape_change = edit::ElemRef::Attr(AttrKey::Meta(0));
    assert!(
        edit::independence(&shape, &incident)
            .iter()
            .any(|conflict| {
                matches!(conflict, EditConflict::FanUse { first, second }
            if first == &shape_change || second == &shape_change)
            })
    );
}

#[test]
fn every_benign_conflict_class_depends_on_equal_result_payloads() {
    let mut delete_a = GraphDelta::identity();
    let mut delete_b = GraphDelta::identity();
    delete_a.del.insert(GElem::Node(8), node(8, "a"));
    delete_b.del.insert(GElem::Node(8), node(8, "a"));
    let deletion = EditConflict::DeleteDelete {
        elem: GElem::Node(8),
    };
    assert!(edit::is_benign(&deletion, &delete_a, &delete_b));
    delete_b.del.insert(GElem::Node(8), node(8, "b"));
    assert!(!edit::is_benign(&deletion, &delete_a, &delete_b));

    let update_a = attribute_delta(
        AttrKey::Anchor(0),
        AttrVal::Anchor("a".into()),
        AttrVal::Anchor("same".into()),
    );
    let mut update_b = attribute_delta(
        AttrKey::Anchor(0),
        AttrVal::Anchor("b".into()),
        AttrVal::Anchor("same".into()),
    );
    let update = EditConflict::UpdateUpdate {
        key: AttrKey::Anchor(0),
    };
    assert!(edit::is_benign(&update, &update_a, &update_b));
    update_b.attrs.get_mut(&AttrKey::Anchor(0)).unwrap().1 = AttrVal::Anchor("other".into());
    assert!(!edit::is_benign(&update, &update_a, &update_b));

    let empty_a = GraphDelta::identity();
    let empty_b = GraphDelta::identity();
    assert!(!edit::is_benign(&deletion, &empty_a, &empty_b));
    assert!(!edit::is_benign(&update, &empty_a, &empty_b));
    assert!(!edit::is_benign(
        &EditConflict::AddAdd {
            elem: GElem::Node(8),
        },
        &empty_a,
        &empty_b,
    ));
}

#[test]
fn canonical_edit_bytes_are_normalized_golden_and_domain_separated() {
    // Captured from the external blake3 1.8.7 implementation before the
    // dependency-free reference implementation replaced it. These are fixed
    // application bytes, not values recomputed by the code under test.
    const GRAPH_DIGEST: [u8; 32] = [
        0x1a, 0x38, 0xf1, 0x22, 0x28, 0xc3, 0x38, 0x9b, 0xe6, 0xa5, 0xfb, 0x4c, 0xb9, 0xf2, 0x5f,
        0x73, 0xb3, 0x6d, 0x22, 0x4e, 0xa3, 0x2e, 0x29, 0xf7, 0x8b, 0xa7, 0x75, 0xb3, 0x25, 0xbb,
        0x1e, 0x0a,
    ];
    const TABLE_DIGEST: [u8; 32] = [
        0xe7, 0xf5, 0x06, 0xfe, 0x9a, 0xbe, 0x55, 0x5a, 0xdc, 0xd5, 0x08, 0xf1, 0xc9, 0xc9, 0xeb,
        0x85, 0x20, 0xcc, 0x00, 0x29, 0x36, 0x44, 0x6e, 0x37, 0xb5, 0x61, 0x1f, 0xb7, 0x58, 0x29,
        0x1d, 0x63,
    ];
    const TREE_DIGEST: [u8; 32] = [
        0x52, 0x73, 0xed, 0x46, 0xc3, 0xab, 0x33, 0x67, 0x05, 0xee, 0xbd, 0x32, 0x74, 0x32, 0xdb,
        0x2e, 0x4c, 0xf8, 0x31, 0x4a, 0x35, 0xc2, 0xf9, 0xc2, 0x36, 0xc3, 0x9f, 0x0e, 0x84, 0x0f,
        0xda, 0x2f,
    ];
    const GRAPH_SAME: [u8; 32] = [
        0x57, 0xf7, 0x1d, 0x4c, 0x04, 0x33, 0xbb, 0xb9, 0xc1, 0x15, 0xc6, 0xaf, 0x4d, 0x2c, 0x4e,
        0x85, 0x0b, 0x84, 0xcd, 0x20, 0xe2, 0x50, 0xf3, 0xf3, 0xa4, 0x62, 0x94, 0x5d, 0x6c, 0x2c,
        0x74, 0x75,
    ];
    const TABLE_SAME: [u8; 32] = [
        0xb9, 0x3f, 0x38, 0x7b, 0xc8, 0x7f, 0x4e, 0x55, 0xea, 0x7b, 0x2e, 0x1a, 0xab, 0x25, 0x6c,
        0x71, 0x00, 0xae, 0x44, 0x60, 0x60, 0x6b, 0xf8, 0xac, 0xbc, 0xaa, 0x82, 0xd8, 0x12, 0x89,
        0x5a, 0x0f,
    ];
    const TREE_SAME: [u8; 32] = [
        0x99, 0x78, 0x85, 0xad, 0xc1, 0x59, 0x04, 0x6f, 0x10, 0xd2, 0x96, 0xbb, 0x68, 0xae, 0x87,
        0x26, 0x6b, 0xd1, 0x7c, 0x5b, 0x3d, 0x81, 0x5b, 0xb4, 0x37, 0x13, 0xcf, 0x12, 0x00, 0x87,
        0xf2, 0x5d,
    ];

    assert_eq!(
        edit::canonical_graph_delta_bytes(&GraphDelta::identity()),
        Some(vec![0; 40])
    );
    assert_eq!(
        edit::canonical_table_delta_bytes(&TableDelta::identity()),
        Some(vec![0; 48])
    );
    assert_eq!(
        edit::canonical_tree_edit_bytes(&TreeEdit::Identity),
        Some(vec![0; 8])
    );
    let step = TreeEdit::DeleteNode { id: 7 };
    assert_eq!(
        edit::canonical_tree_edit_bytes(&step),
        edit::canonical_tree_edit_bytes(&TreeEdit::Sequence(vec![
            TreeEdit::Identity,
            TreeEdit::Sequence(vec![step.clone()]),
        ]))
    );
    assert_eq!(
        edit::graph_delta_digest(&GraphDelta::identity()),
        Some(GRAPH_DIGEST)
    );
    assert_eq!(
        edit::table_delta_digest(&TableDelta::identity()),
        Some(TABLE_DIGEST)
    );
    assert_eq!(
        edit::tree_edit_digest(&TreeEdit::Identity),
        Some(TREE_DIGEST)
    );
    assert_eq!(
        Digest::derive(edit::GRAPH_DELTA_CONTEXT, b"same").0,
        GRAPH_SAME
    );
    assert_eq!(
        Digest::derive(edit::TABLE_DELTA_CONTEXT, b"same").0,
        TABLE_SAME
    );
    assert_eq!(
        Digest::derive(edit::TREE_EDIT_CONTEXT, b"same").0,
        TREE_SAME
    );
    assert_eq!(BTreeSet::from([GRAPH_SAME, TABLE_SAME, TREE_SAME]).len(), 3);
}

#[test]
fn canonical_nonidentity_edit_bytes_match_independent_literal_layouts() {
    let relabel = attribute_delta(
        AttrKey::Anchor(0),
        AttrVal::Anchor("a".into()),
        AttrVal::Anchor("b".into()),
    );
    let mut expected_graph = vec![0; 32];
    expected_graph.extend_from_slice(&1u64.to_le_bytes());
    expected_graph.push(3);
    expected_graph.extend_from_slice(&0u32.to_le_bytes());
    expected_graph.push(3);
    expected_graph.extend_from_slice(&1u64.to_le_bytes());
    expected_graph.push(b'a');
    expected_graph.push(3);
    expected_graph.extend_from_slice(&1u64.to_le_bytes());
    expected_graph.push(b'b');
    assert_eq!(
        edit::canonical_graph_delta_bytes(&relabel),
        Some(expected_graph)
    );

    let mut table_change = TableDelta::identity();
    table_change.attrs.insert(
        TKey::Table(0),
        (TVal::Table("a".into()), TVal::Table("b".into())),
    );
    let mut expected_table = vec![0; 40];
    expected_table.extend_from_slice(&1u64.to_le_bytes());
    expected_table.extend_from_slice(&0u32.to_le_bytes());
    expected_table.extend_from_slice(&1u64.to_le_bytes());
    expected_table.push(b'a');
    expected_table.extend_from_slice(&1u64.to_le_bytes());
    expected_table.push(b'b');
    assert_eq!(
        edit::canonical_table_delta_bytes(&table_change),
        Some(expected_table)
    );
    assert_eq!(
        edit::canonical_tree_edit_bytes(&TreeEdit::DeleteNode { id: 7 }),
        Some(vec![1, 0, 0, 0, 0, 0, 0, 0, 2, 7, 0, 0, 0])
    );

    let equal_graph = attribute_delta(
        AttrKey::Anchor(0),
        AttrVal::Anchor("a".into()),
        AttrVal::Anchor("a".into()),
    );
    assert_eq!(edit::canonical_graph_delta_bytes(&equal_graph), None);
    let mut equal_table = TableDelta::identity();
    equal_table.attrs.insert(
        TKey::Table(0),
        (TVal::Table("a".into()), TVal::Table("a".into())),
    );
    assert_eq!(edit::canonical_table_delta_bytes(&equal_table), None);
}

#[test]
fn canonical_graph_delta_covers_every_part_and_attribute_branch_in_order() {
    let deleted_node = Node::new(1, "deleted");
    let added_node = Node::new(2, "added");
    let deleted_edge = GEdge {
        src: 0,
        tgt: 1,
        label: SEdge::new("a", "deleted", "old", None),
        position: None,
    };
    let added_edge = GEdge {
        src: 0,
        tgt: 2,
        label: SEdge::new("a", "added", "new", Some("n")),
        position: Some(4),
    };
    let plain = NodeMeta {
        discriminator: None,
        position: None,
        shape: NodeShape::Plain,
        annotations: BTreeMap::new(),
    };
    let list = NodeMeta {
        shape: NodeShape::List,
        ..plain.clone()
    };
    let attrs = BTreeMap::from([
        (
            AttrKey::Value(0),
            (
                AttrVal::Presence(None),
                AttrVal::Presence(Some(Presence::Present(Value::Bool(true)))),
            ),
        ),
        (
            AttrKey::Field(0, "field".into()),
            (AttrVal::Field(None), AttrVal::Field(Some(Value::Int(7)))),
        ),
        (
            AttrKey::Meta(0),
            (AttrVal::Meta(plain), AttrVal::Meta(list)),
        ),
        (
            AttrKey::Anchor(0),
            (AttrVal::Anchor("a".into()), AttrVal::Anchor("b".into())),
        ),
        (
            AttrKey::EdgeLabel(0),
            (
                AttrVal::Label(SEdge::new("a", "a", "e", None)),
                AttrVal::Label(SEdge::new("b", "a", "e", Some("named"))),
            ),
        ),
        (
            AttrKey::EdgePosition(0),
            (AttrVal::Position(None), AttrVal::Position(Some(3))),
        ),
    ]);
    let delta = GraphDelta {
        del: BTreeMap::from([
            (GElem::Node(1), GPayload::Node(deleted_node.clone())),
            (GElem::Edge(1), GPayload::Edge(deleted_edge.clone())),
        ]),
        add: BTreeMap::from([
            (GElem::Node(2), GPayload::Node(added_node.clone())),
            (GElem::Edge(2), GPayload::Edge(added_edge.clone())),
        ]),
        attrs,
    };
    let mut expected = Writer::default();
    for count in [1, 1, 1, 1, 6] {
        expected.len(count);
    }
    canonical::write_node(&mut expected, &deleted_node).unwrap();
    write_graph_edge_record(&mut expected, 1, &deleted_edge);
    canonical::write_node(&mut expected, &added_node).unwrap();
    write_graph_edge_record(&mut expected, 2, &added_edge);
    for (key, (old, new)) in &delta.attrs {
        write_attr_key(&mut expected, key);
        write_attr_value(&mut expected, old).unwrap();
        write_attr_value(&mut expected, new).unwrap();
    }
    assert_eq!(
        edit::canonical_graph_delta_bytes(&delta),
        Some(expected.bytes)
    );
}

#[test]
fn canonical_table_delta_covers_every_part_and_attribute_branch_in_order() {
    let deleted_cells = BTreeMap::from([("old".into(), Value::Int(1))]);
    let added_cells = BTreeMap::from([("new".into(), Value::Bool(true))]);
    let deleted_pair = SEdge::new("old", "target", "fk", Some("deleted"));
    let added_pair = SEdge::new("new", "target", "fk", Some("added"));
    let delta = TableDelta {
        del: BTreeMap::from([
            (
                TElem::Row(1),
                TPayload::Row("old".into(), deleted_cells.clone()),
            ),
            (TElem::Pair(deleted_pair.clone(), 1, 0), TPayload::Pair),
        ]),
        add: BTreeMap::from([
            (
                TElem::Row(2),
                TPayload::Row("new".into(), added_cells.clone()),
            ),
            (TElem::Pair(added_pair.clone(), 2, 0), TPayload::Pair),
        ]),
        attrs: BTreeMap::from([
            (
                TKey::Cell(0, "cell".into()),
                (TVal::Cell(None), TVal::Cell(Some(Value::Int(3)))),
            ),
            (
                TKey::Table(0),
                (TVal::Table("before".into()), TVal::Table("after".into())),
            ),
        ]),
    };
    let mut expected = Writer::default();
    for count in [1, 1, 1, 1, 1, 1] {
        expected.len(count);
    }
    expected.u32(1);
    expected.str("old");
    write_value_map(&mut expected, &deleted_cells).unwrap();
    expected.u32(2);
    expected.str("new");
    write_value_map(&mut expected, &added_cells).unwrap();
    write_table_pair_record(&mut expected, &deleted_pair, 1, 0);
    write_table_pair_record(&mut expected, &added_pair, 2, 0);
    expected.u32(0);
    expected.str("cell");
    write_optional_value(&mut expected, None).unwrap();
    write_optional_value(&mut expected, Some(&Value::Int(3))).unwrap();
    expected.u32(0);
    expected.str("before");
    expected.str("after");
    assert_eq!(
        edit::canonical_table_delta_bytes(&delta),
        Some(expected.bytes)
    );
}

fn all_primitive_tree_edits() -> TreeEdit {
    TreeEdit::Sequence(vec![
        TreeEdit::InsertNode {
            parent: 0,
            child_id: 11,
            node: Node::new(11, "leaf"),
            edge: SEdge::new("root", "leaf", "child", Some("insert")),
        },
        TreeEdit::DeleteNode { id: 10 },
        TreeEdit::ContractNode { id: 9 },
        TreeEdit::RelabelNode {
            id: 8,
            new_anchor: "renamed".into(),
        },
        TreeEdit::SetField {
            node_id: 7,
            field: "field".into(),
            value: Value::Int(4),
        },
        TreeEdit::RemoveField {
            node_id: 6,
            field: "removed".into(),
        },
        TreeEdit::MoveSubtree {
            node_id: 5,
            new_parent: 0,
            edge: SEdge::new("root", "leaf", "child", Some("move")),
        },
        TreeEdit::InsertFan {
            fan: Fan {
                parent: 0,
                hyper_edge: "inserted-fan".into(),
                children: BTreeMap::from([("arm".into(), 5)]),
            },
        },
        TreeEdit::DeleteFan {
            hyper_edge: "deleted-fan".into(),
        },
        TreeEdit::JoinFeatures {
            primary: 4,
            joined: vec![3, 2],
            produce: Node::new(4, "merged"),
        },
    ])
}

fn independent_all_tree_edit_bytes() -> Vec<u8> {
    let insert_edge = SEdge::new("root", "leaf", "child", Some("insert"));
    let move_edge = SEdge::new("root", "leaf", "child", Some("move"));
    let fan = Fan {
        parent: 0,
        hyper_edge: "inserted-fan".into(),
        children: BTreeMap::from([("arm".into(), 5)]),
    };
    let mut writer = Writer::default();
    writer.len(10);
    writer.tag(1);
    writer.u32(0);
    writer.u32(11);
    canonical::write_node(&mut writer, &Node::new(11, "leaf")).unwrap();
    writer.edge(&insert_edge);
    writer.tag(2);
    writer.u32(10);
    writer.tag(3);
    writer.u32(9);
    writer.tag(4);
    writer.u32(8);
    writer.str("renamed");
    writer.tag(5);
    writer.u32(7);
    writer.str("field");
    canonical::write_value(&mut writer, &Value::Int(4)).unwrap();
    writer.tag(6);
    writer.u32(6);
    writer.str("removed");
    writer.tag(7);
    writer.u32(5);
    writer.u32(0);
    writer.edge(&move_edge);
    writer.tag(8);
    write_tree_fan_record(&mut writer, &fan);
    writer.tag(9);
    writer.str("deleted-fan");
    writer.tag(10);
    writer.u32(4);
    writer.len(2);
    writer.u32(3);
    writer.u32(2);
    canonical::write_node(&mut writer, &Node::new(4, "merged")).unwrap();
    writer.bytes
}

#[test]
fn canonical_tree_edit_covers_every_primitive_tag_in_flattened_order() {
    assert_eq!(
        edit::canonical_tree_edit_bytes(&all_primitive_tree_edits()),
        Some(independent_all_tree_edit_bytes())
    );
}

#[test]
fn canonical_edit_encoders_reject_ill_formed_values_and_cross_sort_parts() {
    let invalid = Value::Temporal(Temporal {
        date: None,
        time: None,
        separator: None,
        offset: Offset::Local,
        infinity: None,
    });
    let graph_delta = attribute_delta(
        AttrKey::Field(0, "invalid".into()),
        AttrVal::Field(None),
        AttrVal::Field(Some(invalid.clone())),
    );
    assert_eq!(edit::canonical_graph_delta_bytes(&graph_delta), None);
    let mut table_delta = TableDelta::identity();
    table_delta.attrs.insert(
        TKey::Cell(0, "invalid".into()),
        (TVal::Cell(None), TVal::Cell(Some(invalid.clone()))),
    );
    assert_eq!(edit::canonical_table_delta_bytes(&table_delta), None);
    let tree_edit = TreeEdit::SetField {
        node_id: 0,
        field: "invalid".into(),
        value: invalid,
    };
    assert_eq!(edit::canonical_tree_edit_bytes(&tree_edit), None);

    let mut wrong_table_payload = TableDelta::identity();
    wrong_table_payload
        .add
        .insert(TElem::Row(0), TPayload::Pair);
    assert_eq!(
        edit::canonical_table_delta_bytes(&wrong_table_payload),
        None
    );
    let mut wrong_table_attribute = TableDelta::identity();
    wrong_table_attribute.attrs.insert(
        TKey::Table(0),
        (TVal::Cell(None), TVal::Cell(Some(Value::Int(1)))),
    );
    assert_eq!(
        edit::canonical_table_delta_bytes(&wrong_table_attribute),
        None
    );
}

#[test]
fn benign_conflicts_and_typed_edit_translation_have_direct_witnesses() {
    let payload = node(8, "a");
    let mut a = GraphDelta::identity();
    let mut b = GraphDelta::identity();
    a.add.insert(GElem::Node(8), payload.clone());
    b.add.insert(GElem::Node(8), payload);
    let conflict = EditConflict::AddAdd {
        elem: GElem::Node(8),
    };
    assert!(edit::is_benign(&conflict, &a, &b));
    b.add.insert(GElem::Node(8), node(8, "different"));
    assert!(!edit::is_benign(&conflict, &a, &b));

    let source = graph();
    let translated = edit::get_edit(
        &|state| Ok::<_, &'static str>((state.clone(), state.nodes.len())),
        &source,
        &a,
    )
    .unwrap();
    assert_eq!(translated.delta, a);
    assert_eq!(translated.complement, 3);

    let put = edit::put_edit(
        &|_, edited, complement| Ok::<_, &'static str>((edited.clone(), complement + 1usize)),
        &source,
        &source,
        &2,
        &a,
    )
    .unwrap();
    assert_eq!(put.delta, a);
    assert_eq!(put.complement, 3);

    let refused = edit::get_edit(
        &|_| Err::<(Graph, ()), _>("lens refused"),
        &source,
        &GraphDelta::identity(),
    );
    assert_eq!(
        refused,
        Err(edit::EditTranslationError::Lens("lens refused"))
    );
}

fn prefix_graph_anchors(state: &Graph, prefix: &str) -> Graph {
    let mut mapped = state.clone();
    for node in mapped.nodes.values_mut() {
        node.anchor = format!("{prefix}{}", node.anchor);
    }
    for edge in mapped.edges.values_mut() {
        edge.label.src = format!("{prefix}{}", edge.label.src);
        edge.label.tgt = format!("{prefix}{}", edge.label.tgt);
    }
    mapped
}

fn strip_graph_anchor_prefix(state: &Graph, prefix: &str) -> Graph {
    let mut mapped = state.clone();
    for node in mapped.nodes.values_mut() {
        node.anchor = node.anchor.strip_prefix(prefix).unwrap().to_owned();
    }
    for edge in mapped.edges.values_mut() {
        edge.label.src = edge.label.src.strip_prefix(prefix).unwrap().to_owned();
        edge.label.tgt = edge.label.tgt.strip_prefix(prefix).unwrap().to_owned();
    }
    mapped
}

#[test]
fn nonidentity_get_and_put_edit_squares_have_explicit_deltas_and_complements() {
    let source = graph();
    let source_delta = GraphDelta {
        del: BTreeMap::new(),
        add: BTreeMap::new(),
        attrs: BTreeMap::from([
            (
                AttrKey::Anchor(0),
                (AttrVal::Anchor("a".into()), AttrVal::Anchor("c".into())),
            ),
            (
                AttrKey::EdgeLabel(0),
                (
                    AttrVal::Label(SEdge::new("a", "b", "e", None)),
                    AttrVal::Label(SEdge::new("c", "b", "e", None)),
                ),
            ),
        ]),
    };
    let view = prefix_graph_anchors(&source, "view:");
    let view_delta = GraphDelta {
        del: BTreeMap::new(),
        add: BTreeMap::new(),
        attrs: BTreeMap::from([
            (
                AttrKey::Anchor(0),
                (
                    AttrVal::Anchor("view:a".into()),
                    AttrVal::Anchor("view:c".into()),
                ),
            ),
            (
                AttrKey::EdgeLabel(0),
                (
                    AttrVal::Label(SEdge::new("view:a", "view:b", "e", None)),
                    AttrVal::Label(SEdge::new("view:c", "view:b", "e", None)),
                ),
            ),
        ]),
    };

    let get = |state: &Graph| {
        Ok::<_, &'static str>((
            prefix_graph_anchors(state, "view:"),
            state.nodes.len() + state.edges.len(),
        ))
    };
    let translated_get = edit::get_edit(&get, &source, &source_delta).unwrap();
    assert_eq!(translated_get.delta, view_delta);
    assert_eq!(translated_get.complement, 3);
    let edited_source = edit::apply(&source_delta, &source).unwrap();
    let edited_view = edit::apply(&translated_get.delta, &view).unwrap();
    assert_eq!(get(&edited_source).unwrap().0, edited_view);

    let put = |_: &Graph, edited_view: &Graph, complement: &usize| {
        Ok::<_, &'static str>((
            strip_graph_anchor_prefix(edited_view, "view:"),
            complement + 7,
        ))
    };
    let translated_put = edit::put_edit(&put, &source, &view, &3, &view_delta).unwrap();
    assert_eq!(translated_put.delta, source_delta);
    assert_eq!(translated_put.complement, 10);
    assert_eq!(
        edit::apply(&translated_put.delta, &source).unwrap(),
        edited_source
    );
}

/// The embedding `tree_to_graph` assigns to [`join_tree`], written out.
fn literal_join_embedding() -> TreeEmbedding {
    TreeEmbedding {
        arcs: BTreeMap::from([(1, 0), (2, 1), (3, 2), (4, 3)]),
        fans: vec![(5, 4, BTreeMap::from([("arm".into(), 5)]))],
        next_node: 6,
        next_edge: 6,
    }
}

fn fan_node(id: u32, hyper_edge: &str) -> Node {
    let mut node = Node::new(id, hyper_edge);
    node.shape = NodeShape::HyperEdge;
    node
}

const fn plain_edge(src: u32, tgt: u32, label: SEdge) -> GEdge {
    GEdge {
        src,
        tgt,
        label,
        position: None,
    }
}

fn fan_parent_label() -> SEdge {
    SEdge::new("fan", "feature", "hyper_parent", None)
}

fn fan_arm_label() -> SEdge {
    SEdge::new("fan", "leaf", "hyper_label", Some("arm"))
}

fn inserted_label() -> SEdge {
    SEdge::new("feature", "leaf", "child", Some("inserted"))
}

fn insert_leaf(parent: u32, child_id: u32) -> TreeEdit {
    TreeEdit::InsertNode {
        parent,
        child_id,
        node: Node::new(child_id, "leaf"),
        edge: inserted_label(),
    }
}

/// Whether `result` is exactly the refusal `NotTreeEdit { reason }`.
fn not_tree_edit(result: &Result<TreeEdit, edit::TreeApplyError>, reason: &str) -> bool {
    *result
        == Err(edit::TreeApplyError::NotTreeEdit {
            reason: reason.to_owned(),
        })
}

/// The literal lowering of `InsertNode { parent: 1, child_id: 5 }` on
/// [`join_tree`]: fan node 5 becomes the tree leaf 5 (its metadata and anchor
/// change), the fan moves to node 6 with fresh edges 6 and 7 replacing 4 and
/// 5, and the new arc is edge 8.
fn fan_relocation_expected_delta() -> GraphDelta {
    let mut expected_delta = GraphDelta::identity();
    expected_delta.del.insert(
        GElem::Edge(4),
        GPayload::Edge(plain_edge(5, 2, fan_parent_label())),
    );
    expected_delta.del.insert(
        GElem::Edge(5),
        GPayload::Edge(plain_edge(5, 3, fan_arm_label())),
    );
    expected_delta
        .add
        .insert(GElem::Node(6), GPayload::Node(fan_node(6, "fan")));
    expected_delta.add.insert(
        GElem::Edge(6),
        GPayload::Edge(plain_edge(6, 2, fan_parent_label())),
    );
    expected_delta.add.insert(
        GElem::Edge(7),
        GPayload::Edge(plain_edge(6, 3, fan_arm_label())),
    );
    expected_delta.add.insert(
        GElem::Edge(8),
        GPayload::Edge(plain_edge(1, 5, inserted_label())),
    );
    expected_delta = replacing(
        expected_delta,
        &literal_join_graph(),
        5,
        Node::new(5, "leaf"),
    );

    expected_delta
}

/// The literal graph after [`fan_relocation_expected_delta`].
fn fan_relocation_expected_graph() -> Graph {
    let mut expected_graph = literal_join_graph();
    expected_graph.nodes.insert(5, Node::new(5, "leaf"));
    expected_graph.nodes.insert(6, fan_node(6, "fan"));
    expected_graph.edges.remove(&4);
    expected_graph.edges.remove(&5);
    expected_graph
        .edges
        .insert(6, plain_edge(6, 2, fan_parent_label()));
    expected_graph
        .edges
        .insert(7, plain_edge(6, 3, fan_arm_label()));
    expected_graph
        .edges
        .insert(8, plain_edge(1, 5, inserted_label()));

    expected_graph
}

// Why the reader's embedding-floor refusal never refuses a lowering, and why
// readback across a fan relocation needs no exception to it.
//
// Fix a tree T and an embedding E that `apply_tree` accepts, so every id E
// uses lies strictly below its floors: tree and fan node ids below
// `E.next_node`, arc and fan edge ids below `E.next_edge`. Let e be any tree
// edit, E' the embedding it leaves, and d = diff(G(T, E), G(T', E')) its
// lowering.
//
// Lemma 1 (ids only grow). Every step draws a new id with `take_tree_id`,
// which returns the current floor and then raises it, and `InsertNode` raises
// `next_node` past its child before drawing. So every id E' holds that E
// does not hold is at least the matching floor of E.
//
// Lemma 2 (kept ids keep their endpoints). A step changes an existing entry
// of the embedding only by removing it or by overwriting it with a freshly
// drawn id: `InsertNode` re-draws the node and every edge id of each
// relocated fan; `ContractNode`, `JoinFeatures` and `MoveSubtree` re-draw the
// arc of every child they re-parent or move. An arc entry that survives
// unchanged has an unchanged parent and child, and a fan entry that survives
// unchanged has an unchanged fan node, parent and children. Its graph edge
// therefore has the same source and target in G(T, E) and G(T', E'). By
// induction this holds across a sequence.
//
// Lemma 3 (no below-floor addition). The differ adds an edge id x either
// when x is absent from G(T, E) or when x is in both graphs with other
// endpoints. In the first case x is not an id of E, so x >= E.next_edge by
// Lemma 1. The second case is excluded by Lemma 2. A fan node d adds is
// absent from G(T, E), so its id is at least E.next_node by Lemma 1.
//
// Hence the floor clause of `check_graph_delta_tree_image` admits every
// lowering. In particular `InsertNode { child_id: 5 }` on [`join_tree`]
// relocates fan node 5 to 6 and re-draws both fan edges as 6 and 7 before
// drawing the new arc as 8, and every added id is at or above the floor 6.
// A delta that instead re-adds edges 4 and 5 in place with a new source is a
// below-floor replacement, outside the image by Lemma 3, and is refused.
#[test]
fn insert_node_over_the_fan_id_lowers_reads_back_and_applies_exactly() {
    let tree = join_tree();
    let embedding = literal_join_embedding();
    assert_eq!(
        panproto_reference::encode::tree_to_graph(&tree, None),
        Ok((literal_join_graph(), embedding.clone()))
    );
    let insert = insert_leaf(1, 5);

    let expected_delta = fan_relocation_expected_delta();
    let expected_embedding = TreeEmbedding {
        arcs: BTreeMap::from([(1, 0), (2, 1), (3, 2), (4, 3), (5, 8)]),
        fans: vec![(6, 6, BTreeMap::from([("arm".into(), 7)]))],
        next_node: 7,
        next_edge: 9,
    };
    let mut expected_tree = join_tree();
    expected_tree.nodes.insert(5, Node::new(5, "leaf"));
    expected_tree.arcs.push((1, 5, inserted_label()));
    let mut lowered_embedding = embedding.clone();
    assert_eq!(
        edit::graph_delta_from_tree_edit(&tree, &mut lowered_embedding, &insert, None),
        Ok(expected_delta.clone())
    );
    assert_eq!(lowered_embedding, expected_embedding);
    assert_eq!(
        edit::apply(&expected_delta, &literal_join_graph()),
        Ok(fan_relocation_expected_graph())
    );
    assert_eq!(
        edit::tree_edit_from_graph_delta(&tree, &embedding, &expected_delta, None),
        Ok(insert.clone())
    );
    let mut applied_embedding = embedding;
    assert_eq!(
        edit::apply_tree(&insert, &tree, &mut applied_embedding),
        Ok(expected_tree)
    );
    assert_eq!(applied_embedding, expected_embedding);
}

#[test]
fn an_in_place_fan_edge_replacement_is_refused_below_the_floor() {
    let tree = join_tree();
    let embedding = literal_join_embedding();
    let mut in_place = fan_relocation_expected_delta();
    for id in [6, 7] {
        in_place.add.remove(&GElem::Edge(id));
    }
    in_place.add.remove(&GElem::Edge(8));
    in_place.add.insert(
        GElem::Edge(4),
        GPayload::Edge(plain_edge(6, 2, fan_parent_label())),
    );
    in_place.add.insert(
        GElem::Edge(5),
        GPayload::Edge(plain_edge(6, 3, fan_arm_label())),
    );
    in_place.add.insert(
        GElem::Edge(6),
        GPayload::Edge(plain_edge(1, 5, inserted_label())),
    );
    assert!(not_tree_edit(
        &edit::tree_edit_from_graph_delta(&tree, &embedding, &in_place, None),
        "an added arc or fan id is below the embedding floor"
    ));
}

#[test]
fn a_sequence_delta_that_skips_ids_reads_as_its_effect() {
    let tree = join_tree();
    let embedding = literal_join_embedding();
    let transient = TreeEdit::Sequence(vec![insert_leaf(1, 5), TreeEdit::DeleteNode { id: 5 }]);
    let mut expected_delta = literal_deletions(
        &literal_join_graph(),
        [GElem::Node(5), GElem::Edge(4), GElem::Edge(5)],
    );
    expected_delta
        .add
        .insert(GElem::Node(6), GPayload::Node(fan_node(6, "fan")));
    expected_delta.add.insert(
        GElem::Edge(6),
        GPayload::Edge(plain_edge(6, 2, fan_parent_label())),
    );
    expected_delta.add.insert(
        GElem::Edge(7),
        GPayload::Edge(plain_edge(6, 3, fan_arm_label())),
    );
    let mut lowered_embedding = embedding.clone();
    assert_eq!(
        edit::graph_delta_from_tree_edit(&tree, &mut lowered_embedding, &transient, None),
        Ok(expected_delta.clone())
    );
    assert_eq!(
        lowered_embedding,
        TreeEmbedding {
            arcs: BTreeMap::from([(1, 0), (2, 1), (3, 2), (4, 3)]),
            fans: vec![(6, 6, BTreeMap::from([("arm".into(), 7)]))],
            next_node: 7,
            next_edge: 9,
        }
    );
    assert_eq!(
        edit::tree_edit_from_graph_delta(&tree, &embedding, &expected_delta, None),
        Ok(TreeEdit::Identity)
    );
    let mut applied_embedding = embedding;
    assert_eq!(
        edit::apply_tree(&transient, &tree, &mut applied_embedding),
        Ok(join_tree())
    );

    let mut mislabelled = expected_delta.clone();
    let mut bogus = fan_parent_label();
    bogus.tgt = "bogus".into();
    mislabelled
        .add
        .insert(GElem::Edge(6), GPayload::Edge(plain_edge(6, 2, bogus)));
    assert!(not_tree_edit(
        &edit::tree_edit_from_graph_delta(&tree, &literal_join_embedding(), &mislabelled, None),
        "edited graph is not a tree: FanLabel { edge: 6 }"
    ));

    let mut relabelled = expected_delta;
    let renamed_arm = SEdge::new("fan", "renamed", "hyper_label", Some("arm"));
    relabelled.add.insert(
        GElem::Edge(7),
        GPayload::Edge(plain_edge(6, 3, renamed_arm)),
    );
    relabelled.attrs.insert(
        AttrKey::Anchor(3),
        (
            AttrVal::Anchor("leaf".into()),
            AttrVal::Anchor("renamed".into()),
        ),
    );
    assert_eq!(
        edit::tree_edit_from_graph_delta(&tree, &literal_join_embedding(), &relabelled, None),
        Ok(TreeEdit::RelabelNode {
            id: 3,
            new_anchor: "renamed".into(),
        })
    );
}

#[test]
fn fan_relocations_read_back_as_suffix_relocations_or_reinsertions() {
    let second_fan = Fan {
        parent: 1,
        hyper_edge: "second-fan".into(),
        children: BTreeMap::from([("arm".into(), 4)]),
    };
    let mut tree = join_tree();
    tree.fans.push(second_fan);
    let embedding = TreeEmbedding {
        arcs: BTreeMap::from([(1, 0), (2, 1), (3, 2), (4, 3)]),
        fans: vec![
            (5, 4, BTreeMap::from([("arm".into(), 5)])),
            (6, 6, BTreeMap::from([("arm".into(), 7)])),
        ],
        next_node: 7,
        next_edge: 8,
    };
    let second_parent = SEdge::new("second-fan", "feature", "hyper_parent", None);
    let second_arm = SEdge::new("second-fan", "leaf", "hyper_label", Some("arm"));
    let mut graph = literal_join_graph();
    graph.nodes.insert(6, fan_node(6, "second-fan"));
    graph
        .edges
        .insert(6, plain_edge(6, 1, second_parent.clone()));
    graph.edges.insert(7, plain_edge(6, 4, second_arm.clone()));
    assert_eq!(
        panproto_reference::encode::tree_to_graph(&tree, None),
        Ok((graph.clone(), embedding.clone()))
    );

    let mut suffix = literal_deletions(&graph, [GElem::Node(6), GElem::Edge(6), GElem::Edge(7)]);
    suffix
        .add
        .insert(GElem::Node(7), GPayload::Node(fan_node(7, "second-fan")));
    suffix.add.insert(
        GElem::Edge(8),
        GPayload::Edge(plain_edge(7, 1, second_parent)),
    );
    suffix
        .add
        .insert(GElem::Edge(9), GPayload::Edge(plain_edge(7, 4, second_arm)));
    assert_eq!(
        edit::tree_edit_from_graph_delta(&tree, &embedding, &suffix, None),
        Ok(TreeEdit::Identity)
    );

    let mut first_only =
        literal_deletions(&graph, [GElem::Node(5), GElem::Edge(4), GElem::Edge(5)]);
    first_only
        .add
        .insert(GElem::Node(7), GPayload::Node(fan_node(7, "fan")));
    first_only.add.insert(
        GElem::Edge(8),
        GPayload::Edge(plain_edge(7, 2, fan_parent_label())),
    );
    first_only.add.insert(
        GElem::Edge(9),
        GPayload::Edge(plain_edge(7, 3, fan_arm_label())),
    );
    assert_eq!(
        edit::tree_edit_from_graph_delta(&tree, &embedding, &first_only, None),
        Ok(TreeEdit::Sequence(vec![
            TreeEdit::DeleteFan {
                hyper_edge: "fan".into(),
            },
            TreeEdit::InsertFan {
                fan: join_tree().fans[0].clone(),
            },
        ]))
    );

    let duplicate = duplicate_fan_tree();
    let (duplicate_graph, duplicate_embedding) = literal_duplicate_fan_graph_and_embedding();
    let mut duplicate_first_only = literal_deletions(
        &duplicate_graph,
        [GElem::Node(5), GElem::Edge(4), GElem::Edge(5)],
    );
    duplicate_first_only
        .add
        .insert(GElem::Node(7), GPayload::Node(fan_node(7, "fan")));
    duplicate_first_only.add.insert(
        GElem::Edge(8),
        GPayload::Edge(plain_edge(7, 2, fan_parent_label())),
    );
    duplicate_first_only.add.insert(
        GElem::Edge(9),
        GPayload::Edge(plain_edge(7, 3, fan_arm_label())),
    );
    assert!(not_tree_edit(
        &edit::tree_edit_from_graph_delta(
            &duplicate,
            &duplicate_embedding,
            &duplicate_first_only,
            None
        ),
        FIXED_CARRIERS
    ));
}

#[test]
fn sibling_insertions_read_back_in_their_arc_order() {
    let tree = join_tree();
    let embedding = literal_join_embedding();
    let script = TreeEdit::Sequence(vec![insert_leaf(1, 7), insert_leaf(1, 6)]);
    let mut expected_delta = GraphDelta::identity();
    expected_delta
        .add
        .insert(GElem::Node(6), GPayload::Node(Node::new(6, "leaf")));
    expected_delta
        .add
        .insert(GElem::Node(7), GPayload::Node(Node::new(7, "leaf")));
    expected_delta.add.insert(
        GElem::Edge(6),
        GPayload::Edge(plain_edge(1, 7, inserted_label())),
    );
    expected_delta.add.insert(
        GElem::Edge(7),
        GPayload::Edge(plain_edge(1, 6, inserted_label())),
    );
    let mut lowered_embedding = embedding.clone();
    assert_eq!(
        edit::graph_delta_from_tree_edit(&tree, &mut lowered_embedding, &script, None),
        Ok(expected_delta.clone())
    );
    let mut expected_tree = join_tree();
    expected_tree.nodes.insert(6, Node::new(6, "leaf"));
    expected_tree.nodes.insert(7, Node::new(7, "leaf"));
    expected_tree.arcs.push((1, 7, inserted_label()));
    expected_tree.arcs.push((1, 6, inserted_label()));
    let reading = TreeEdit::Sequence(vec![
        insert_leaf(1, 6),
        insert_leaf(1, 7),
        TreeEdit::MoveSubtree {
            node_id: 6,
            new_parent: 1,
            edge: inserted_label(),
        },
    ]);
    assert_eq!(
        edit::tree_edit_from_graph_delta(&tree, &embedding, &expected_delta, None),
        Ok(reading.clone())
    );
    let mut reading_embedding = embedding;
    assert_eq!(
        edit::apply_tree(&reading, &tree, &mut reading_embedding),
        Ok(expected_tree)
    );
}

#[test]
fn same_parent_moves_that_keep_the_label_read_back_exactly() {
    let tree = join_tree();
    let embedding = literal_join_embedding();
    let graph = literal_join_graph();
    for (child, old_edge, name, expected_arcs, expected_arc_ids) in [
        (
            4,
            3,
            "d",
            [(0, 1), (0, 2), (2, 3), (2, 4)],
            [(1, 0), (2, 1), (3, 2), (4, 6)],
        ),
        (
            3,
            2,
            "c",
            [(0, 1), (0, 2), (2, 4), (2, 3)],
            [(1, 0), (2, 1), (3, 6), (4, 3)],
        ),
    ] {
        let expected_embedding = TreeEmbedding {
            arcs: BTreeMap::from(expected_arc_ids),
            next_edge: 7,
            ..literal_join_embedding()
        };
        let label = SEdge::new("feature", "leaf", "child", Some(name));
        let step = TreeEdit::MoveSubtree {
            node_id: child,
            new_parent: 2,
            edge: label.clone(),
        };
        let mut expected_delta = literal_deletions(&graph, [GElem::Edge(old_edge)]);
        expected_delta
            .add
            .insert(GElem::Edge(6), GPayload::Edge(plain_edge(2, child, label)));
        let mut lowered_embedding = embedding.clone();
        assert_eq!(
            edit::graph_delta_from_tree_edit(&tree, &mut lowered_embedding, &step, None),
            Ok(expected_delta.clone())
        );
        assert_eq!(
            edit::tree_edit_from_graph_delta(&tree, &embedding, &expected_delta, None),
            Ok(step.clone())
        );
        let mut applied_embedding = embedding.clone();
        let applied = edit::apply_tree(&step, &tree, &mut applied_embedding).unwrap();
        assert_eq!(
            applied
                .arcs
                .iter()
                .map(|(parent, child, _)| (*parent, *child))
                .collect::<Vec<_>>(),
            expected_arcs
        );
        assert_eq!(lowered_embedding, expected_embedding);
        assert_eq!(applied_embedding, expected_embedding);
    }
}

#[test]
fn duplicate_named_fan_relocations_read_back_through_member_deletion() {
    let tree = duplicate_fan_tree();
    let (graph, embedding) = literal_duplicate_fan_graph_and_embedding();
    let script = TreeEdit::Sequence(vec![insert_leaf(0, 6), TreeEdit::DeleteNode { id: 1 }]);
    let mut expected_delta = literal_deletions(
        &graph,
        [
            GElem::Node(1),
            GElem::Edge(0),
            GElem::Edge(6),
            GElem::Edge(7),
        ],
    );
    expected_delta.add.insert(
        GElem::Edge(10),
        GPayload::Edge(plain_edge(0, 6, inserted_label())),
    );
    let expected_delta = replacing(expected_delta, &graph, 6, Node::new(6, "leaf"));
    let mut lowered_embedding = embedding.clone();
    assert_eq!(
        edit::graph_delta_from_tree_edit(&tree, &mut lowered_embedding, &script, None),
        Ok(expected_delta.clone())
    );
    assert_eq!(
        edit::tree_edit_from_graph_delta(&tree, &embedding, &expected_delta, None),
        Ok(TreeEdit::Sequence(vec![
            TreeEdit::DeleteNode { id: 1 },
            insert_leaf(0, 6),
        ]))
    );
}

/// [`join_tree`] with a second fan named like the first, under node 1.
fn duplicate_fan_tree() -> Tree {
    let mut tree = join_tree();
    tree.fans.push(Fan {
        parent: 1,
        hyper_edge: "fan".into(),
        children: BTreeMap::from([("other-arm".into(), 4)]),
    });
    tree
}

/// The graph and embedding of [`duplicate_fan_tree`], written out and
/// checked against the encoder: the second fan is node 6 with parent edge
/// 6 and `other-arm` edge 7.
fn literal_duplicate_fan_graph_and_embedding() -> (Graph, TreeEmbedding) {
    let mut graph = literal_join_graph();
    graph.nodes.insert(6, fan_node(6, "fan"));
    graph.edges.insert(6, plain_edge(6, 1, fan_parent_label()));
    graph.edges.insert(
        7,
        plain_edge(
            6,
            4,
            SEdge::new("fan", "leaf", "hyper_label", Some("other-arm")),
        ),
    );
    let embedding = TreeEmbedding {
        arcs: BTreeMap::from([(1, 0), (2, 1), (3, 2), (4, 3)]),
        fans: vec![
            (5, 4, BTreeMap::from([("arm".into(), 5)])),
            (6, 6, BTreeMap::from([("other-arm".into(), 7)])),
        ],
        next_node: 7,
        next_edge: 8,
    };
    assert_eq!(
        panproto_reference::encode::tree_to_graph(&duplicate_fan_tree(), None),
        Ok((graph.clone(), embedding.clone()))
    );
    (graph, embedding)
}

#[test]
fn a_duplicate_named_fan_removed_through_a_reinserted_member_reads_back() {
    let tree = duplicate_fan_tree();
    let (graph, embedding) = literal_duplicate_fan_graph_and_embedding();
    let first_fan = tree.fans[0].clone();
    let reinsert = TreeEdit::InsertNode {
        parent: 2,
        child_id: 3,
        node: Node::new(3, "leaf"),
        edge: SEdge::new("feature", "leaf", "child", Some("c")),
    };
    let script = TreeEdit::Sequence(vec![
        TreeEdit::DeleteNode { id: 3 },
        reinsert.clone(),
        TreeEdit::InsertFan { fan: first_fan },
    ]);
    let mut expected = literal_deletions(
        &graph,
        [
            GElem::Node(5),
            GElem::Edge(2),
            GElem::Edge(4),
            GElem::Edge(5),
        ],
    );
    expected.add.insert(
        GElem::Edge(8),
        GPayload::Edge(plain_edge(
            2,
            3,
            SEdge::new("feature", "leaf", "child", Some("c")),
        )),
    );
    expected
        .add
        .insert(GElem::Node(7), GPayload::Node(fan_node(7, "fan")));
    expected.add.insert(
        GElem::Edge(9),
        GPayload::Edge(plain_edge(7, 2, fan_parent_label())),
    );
    expected.add.insert(
        GElem::Edge(10),
        GPayload::Edge(plain_edge(7, 3, fan_arm_label())),
    );
    let expected = replacing(expected, &graph, 3, Node::new(3, "leaf"));
    let mut lowered = embedding.clone();
    assert_eq!(
        edit::graph_delta_from_tree_edit(&tree, &mut lowered, &script, None),
        Ok(expected.clone())
    );
    assert_eq!(
        edit::tree_edit_from_graph_delta(&tree, &embedding, &expected, None),
        Ok(script)
    );

    let without_reinsertion = TreeEdit::Sequence(vec![TreeEdit::DeleteNode { id: 3 }, reinsert]);
    let mut lowered = embedding.clone();
    let delta =
        edit::graph_delta_from_tree_edit(&tree, &mut lowered, &without_reinsertion, None).unwrap();
    assert_eq!(
        delta,
        added(
            replacing(
                literal_deletions(
                    &graph,
                    [
                        GElem::Node(5),
                        GElem::Edge(2),
                        GElem::Edge(4),
                        GElem::Edge(5),
                    ],
                ),
                &graph,
                3,
                Node::new(3, "leaf")
            ),
            vec![(
                GElem::Edge(8),
                GPayload::Edge(plain_edge(
                    2,
                    3,
                    SEdge::new("feature", "leaf", "child", Some("c")),
                )),
            )],
        )
    );
    assert_eq!(
        edit::tree_edit_from_graph_delta(&tree, &embedding, &delta, None),
        Ok(without_reinsertion)
    );
}

#[test]
fn a_fan_removed_through_a_member_reads_back_beside_relocated_fans_of_its_name() {
    let tree = Tree {
        fans: vec![
            arm_fan(1, "fan", 4),
            arm_fan(2, "other", 3),
            arm_fan(0, "other", 1),
            arm_fan(0, "fan", 2),
        ],
        ..join_tree()
    };
    let (_, embedding) = panproto_reference::encode::tree_to_graph(&tree, None).unwrap();
    assert_eq!(
        embedding.fans.iter().map(|fan| fan.0).collect::<Vec<_>>(),
        [5, 6, 7, 8]
    );
    let reinsert = TreeEdit::InsertNode {
        parent: 2,
        child_id: 3,
        node: Node::new(3, "leaf"),
        edge: SEdge::new("feature", "leaf", "child", Some("c")),
    };
    let script = TreeEdit::Sequence(vec![
        TreeEdit::DeleteNode { id: 3 },
        reinsert.clone(),
        insert_leaf(0, 7),
    ]);
    let mut lowered = embedding.clone();
    let delta = edit::graph_delta_from_tree_edit(&tree, &mut lowered, &script, None).unwrap();
    assert_eq!(
        lowered.fans.iter().map(|fan| fan.0).collect::<Vec<_>>(),
        [5, 9, 10]
    );
    let read = edit::tree_edit_from_graph_delta(&tree, &embedding, &delta, None);
    assert_eq!(
        read,
        Ok(TreeEdit::Sequence(vec![
            TreeEdit::DeleteNode { id: 3 },
            insert_leaf(0, 7),
            reinsert,
            TreeEdit::MoveSubtree {
                node_id: 7,
                new_parent: 0,
                edge: inserted_label(),
            },
        ]))
    );
    let mut read_embedding = embedding.clone();
    let mut script_embedding = embedding;
    assert_eq!(
        edit::apply_tree(&read.unwrap(), &tree, &mut read_embedding),
        edit::apply_tree(&script, &tree, &mut script_embedding)
    );
}

#[test]
fn tree_reader_refuses_fresh_carriers_with_payloads_no_edit_writes() {
    let tree = join_tree();
    let embedding = literal_join_embedding();
    let mut positioned = GraphDelta::identity();
    positioned
        .add
        .insert(GElem::Node(6), GPayload::Node(Node::new(6, "leaf")));
    positioned.add.insert(
        GElem::Edge(6),
        GPayload::Edge(GEdge {
            position: Some(0),
            ..plain_edge(1, 6, inserted_label())
        }),
    );
    assert!(not_tree_edit(
        &edit::tree_edit_from_graph_delta(&tree, &embedding, &positioned, None),
        "edited graph is not a tree: PositionedEdge { edge: 6 }"
    ));

    let mut fan_label = GraphDelta::identity();
    fan_label
        .add
        .insert(GElem::Node(6), GPayload::Node(fan_node(6, "second-fan")));
    fan_label.add.insert(
        GElem::Edge(6),
        GPayload::Edge(plain_edge(
            6,
            1,
            SEdge::new("second-fan", "not-the-anchor", "hyper_parent", None),
        )),
    );
    fan_label.add.insert(
        GElem::Edge(7),
        GPayload::Edge(plain_edge(
            6,
            4,
            SEdge::new("second-fan", "leaf", "hyper_label", Some("arm")),
        )),
    );
    assert!(not_tree_edit(
        &edit::tree_edit_from_graph_delta(&tree, &embedding, &fan_label, None),
        "edited graph is not a tree: FanLabel { edge: 6 }"
    ));
}

/// A finite alphabet of tree edits over `tree` and its successors: every
/// primitive variant, inserts that take hidden fan-node ids, same-parent moves
/// that keep the arc label, fan deletion and re-insertion, and joins.
fn tree_edit_alphabet(tree: &Tree, wide: bool) -> Vec<TreeEdit> {
    let label = |name: &str| SEdge::new("feature", "leaf", "child", Some(name));
    let parents: &[u32] = if wide { &[0, 1, 2, 3, 4] } else { &[0, 2] };
    let moved: &[u32] = if wide {
        &[1, 2, 3, 4, 5, 6, 7, 8]
    } else {
        &[3, 4, 6]
    };
    let new_parents = if wide { 0..=8 } else { 0..=6 };
    let names: &[&str] = if wide {
        &["c", "d", "inserted", "moved"]
    } else {
        &["c", "d"]
    };
    let mut alphabet = vec![TreeEdit::Identity];
    for parent in parents {
        for child_id in 5..=8 {
            alphabet.push(insert_leaf(*parent, child_id));
        }
    }
    for id in 1..=8 {
        alphabet.push(TreeEdit::DeleteNode { id });
        alphabet.push(TreeEdit::ContractNode { id });
    }
    for node_id in moved {
        for new_parent in new_parents.clone() {
            for name in names {
                alphabet.push(TreeEdit::MoveSubtree {
                    node_id: *node_id,
                    new_parent,
                    edge: label(name),
                });
            }
        }
    }
    for fan in &tree.fans {
        alphabet.push(TreeEdit::DeleteFan {
            hyper_edge: fan.hyper_edge.clone(),
        });
        alphabet.push(TreeEdit::InsertFan { fan: fan.clone() });
    }
    alphabet.extend([
        TreeEdit::RelabelNode {
            id: 3,
            new_anchor: "renamed".into(),
        },
        TreeEdit::SetField {
            node_id: 1,
            field: "f".into(),
            value: Value::Int(1),
        },
        TreeEdit::RemoveField {
            node_id: 1,
            field: "f".into(),
        },
        TreeEdit::InsertFan {
            fan: Fan {
                parent: 0,
                hyper_edge: "third-fan".into(),
                children: BTreeMap::from([("arm".into(), 1)]),
            },
        },
        TreeEdit::JoinFeatures {
            primary: 1,
            joined: vec![2],
            produce: Node::new(1, "joined"),
        },
        TreeEdit::JoinFeatures {
            primary: 0,
            joined: vec![2, 1],
            produce: Node::new(0, "root"),
        },
    ]);
    alphabet
}

/// [`join_tree`], the same with a second fan of another name, and the same
/// with a second fan of the same name.
fn readback_trees() -> Vec<Tree> {
    let mut distinct = join_tree();
    distinct.fans.push(Fan {
        parent: 1,
        hyper_edge: "second-fan".into(),
        children: BTreeMap::from([("arm".into(), 4)]),
    });
    let mut duplicate = join_tree();
    duplicate.fans.push(Fan {
        parent: 1,
        hyper_edge: "fan".into(),
        children: BTreeMap::from([("other-arm".into(), 4)]),
    });
    vec![join_tree(), distinct, duplicate]
}

/// Whether two embeddings have the same carriers: the edge id of every arc,
/// keyed by its child (so the result's tree nodes other than the root), and
/// the node, parent edge and child edges of every fan, in fan order. The
/// floors are not compared: they are allocation state, and two scripts that
/// lower to one delta may leave different floors.
fn same_carriers<I: PartialEq>(left: &TreeEmbedding<I>, right: &TreeEmbedding<I>) -> bool {
    left.arcs == right.arcs && left.fans == right.fans
}

/// Check the readback laws on every script that lowers: the reading exists,
/// it has the script's effect on the tree, and on at most one step it
/// lowers to the script's delta and reaches the carriers of the script's
/// embedding. Returns the number of lowered scripts and every failure.
fn readback_law_failures(
    tree: &Tree,
    scripts: impl Iterator<Item = TreeEdit>,
) -> (usize, Vec<String>) {
    let (_, embedding) = panproto_reference::encode::tree_to_graph(tree, None).unwrap();
    let mut lowered_count = 0;
    let mut failures = Vec::new();
    for script in scripts {
        let mut lowered = embedding.clone();
        let Ok(delta) = edit::graph_delta_from_tree_edit(tree, &mut lowered, &script, None) else {
            continue;
        };
        lowered_count += 1;
        let read = match edit::tree_edit_from_graph_delta(tree, &embedding, &delta, None) {
            Ok(read) => read,
            Err(error) => {
                failures.push(format!("{error:?} on {script:?}"));
                continue;
            }
        };
        let mut read_embedding = embedding.clone();
        let mut script_embedding = embedding.clone();
        if edit::apply_tree(&read, tree, &mut read_embedding)
            != edit::apply_tree(&script, tree, &mut script_embedding)
        {
            failures.push(format!("{read:?} has another effect than {script:?}"));
        }
        if script.steps().len() <= 1 {
            let mut relowered = embedding.clone();
            if edit::graph_delta_from_tree_edit(tree, &mut relowered, &read, None).as_ref()
                != Ok(&delta)
                || !same_carriers(&relowered, &lowered)
            {
                failures.push(format!("{script:?} read inexactly as {read:?}"));
            }
        }
    }
    (lowered_count, failures)
}

#[test]
fn tree_readback_laws_hold_on_every_short_script_over_three_fan_geometries() {
    let mut pair_counts = Vec::new();
    let mut triple_counts = Vec::new();
    for tree in readback_trees() {
        let wide = tree_edit_alphabet(&tree, true);
        let pairs = wide.iter().flat_map(|first| {
            wide.iter()
                .map(|second| TreeEdit::Sequence(vec![first.clone(), second.clone()]))
        });
        let (count, failures) = readback_law_failures(&tree, pairs);
        assert_eq!(failures, Vec::<String>::new());
        pair_counts.push(count);

        let narrow_alphabet = tree_edit_alphabet(&tree, false);
        let narrow = &narrow_alphabet;
        let triples = narrow.iter().flat_map(|first| {
            narrow.iter().flat_map(move |second| {
                narrow.iter().map(move |third| {
                    TreeEdit::Sequence(vec![first.clone(), second.clone(), third.clone()])
                })
            })
        });
        let (count, failures) = readback_law_failures(&tree, triples);
        assert_eq!(failures, Vec::<String>::new());
        triple_counts.push(count);
    }
    println!(
        "tree readback laws: lowered pairs {pair_counts:?}, lowered triples {triple_counts:?}"
    );
    assert_eq!(pair_counts, [8_518, 8_879, 8_885]);
    assert_eq!(triple_counts, [54_301, 62_740, 63_175]);
}

fn arm_fan(parent: u32, hyper_edge: &str, member: u32) -> Fan {
    Fan {
        parent,
        hyper_edge: hyper_edge.into(),
        children: BTreeMap::from([("arm".into(), member)]),
    }
}

/// [`join_tree`] under fan lists that cover every way fans relate: the
/// first fan with each second fan that shares or not its name, its parent
/// and its member, in both orders (once when the two are equal); every
/// order of three fans, two of one name, one under the root with a root
/// child as member; and four fans whose two names each have a fan the
/// other's removal must not touch.
fn fan_geometries() -> Vec<(String, Tree)> {
    let first = arm_fan(2, "fan", 3);
    let mut lists = Vec::new();
    for name in ["fan", "other"] {
        for parent in [2, 1] {
            for member in [3, 4] {
                let second = arm_fan(parent, name, member);
                lists.push(vec![first.clone(), second.clone()]);
                if second != first {
                    lists.push(vec![second, first.clone()]);
                }
            }
        }
    }
    let pool = [first, arm_fan(1, "fan", 4), arm_fan(0, "other", 1)];
    for order in [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ] {
        lists.push(order.iter().map(|index| pool[*index].clone()).collect());
    }
    lists.push(vec![
        arm_fan(1, "fan", 4),
        arm_fan(2, "other", 3),
        arm_fan(0, "other", 1),
        arm_fan(0, "fan", 2),
    ]);
    lists
        .into_iter()
        .map(|fans| {
            let name = fans
                .iter()
                .map(|fan| format!("{}@{}:{}", fan.hyper_edge, fan.parent, fan.children["arm"]))
                .collect::<Vec<_>>()
                .join(",");
            (
                name,
                Tree {
                    fans,
                    ..join_tree()
                },
            )
        })
        .collect()
}

/// The edits that remove, keep, refresh or relocate fans of `tree`: leaf
/// deletion and contraction of every fan-incident node, re-insertion of each
/// under its original arc, insertion over every hidden fan id and the first
/// fresh one, same-arc moves that refresh an arc, a member relabel, a join
/// absorbing a fan parent, and deletion and re-insertion of every fan.
fn fan_script_alphabet(tree: &Tree) -> Vec<TreeEdit> {
    let arc = |src: &str, tgt: &str, name: &str| SEdge::new(src, tgt, "child", Some(name));
    let mut alphabet = vec![
        TreeEdit::Identity,
        TreeEdit::DeleteNode { id: 1 },
        TreeEdit::DeleteNode { id: 3 },
        TreeEdit::DeleteNode { id: 4 },
        TreeEdit::ContractNode { id: 2 },
        TreeEdit::InsertNode {
            parent: 0,
            child_id: 1,
            node: Node::new(1, "feature"),
            edge: arc("root", "feature", "a"),
        },
        TreeEdit::InsertNode {
            parent: 2,
            child_id: 3,
            node: Node::new(3, "leaf"),
            edge: arc("feature", "leaf", "c"),
        },
        TreeEdit::InsertNode {
            parent: 2,
            child_id: 4,
            node: Node::new(4, "leaf"),
            edge: arc("feature", "leaf", "d"),
        },
    ];
    let fan_ids = u32::try_from(tree.fans.len()).unwrap();
    for child_id in 5..=5 + fan_ids {
        alphabet.push(insert_leaf(0, child_id));
    }
    alphabet.extend([
        TreeEdit::MoveSubtree {
            node_id: 1,
            new_parent: 0,
            edge: arc("root", "feature", "a"),
        },
        TreeEdit::MoveSubtree {
            node_id: 3,
            new_parent: 2,
            edge: arc("feature", "leaf", "c"),
        },
        TreeEdit::MoveSubtree {
            node_id: 4,
            new_parent: 2,
            edge: arc("feature", "leaf", "d"),
        },
        TreeEdit::MoveSubtree {
            node_id: 3,
            new_parent: 1,
            edge: arc("feature", "leaf", "c"),
        },
        TreeEdit::RelabelNode {
            id: 3,
            new_anchor: "renamed".into(),
        },
        TreeEdit::JoinFeatures {
            primary: 1,
            joined: vec![2],
            produce: Node::new(1, "joined"),
        },
    ]);
    let names: BTreeSet<_> = tree.fans.iter().map(|fan| fan.hyper_edge.clone()).collect();
    alphabet.extend(
        names
            .into_iter()
            .map(|hyper_edge| TreeEdit::DeleteFan { hyper_edge }),
    );
    for fan in &tree.fans {
        let insert = TreeEdit::InsertFan { fan: fan.clone() };
        if !alphabet.contains(&insert) {
            alphabet.push(insert);
        }
    }
    alphabet
}

#[test]
fn tree_readback_laws_hold_on_every_fan_script_over_shared_name_parent_and_order_geometries() {
    let mut counts = Vec::new();
    let mut failures = Vec::new();
    for (name, tree) in fan_geometries() {
        let alphabet = fan_script_alphabet(&tree);
        let letters = &alphabet;
        let words = letters
            .iter()
            .cloned()
            .chain(letters.iter().flat_map(|first| {
                letters
                    .iter()
                    .map(move |second| TreeEdit::Sequence(vec![first.clone(), second.clone()]))
            }))
            .chain(letters.iter().flat_map(|first| {
                letters.iter().flat_map(move |second| {
                    letters.iter().map(move |third| {
                        TreeEdit::Sequence(vec![first.clone(), second.clone(), third.clone()])
                    })
                })
            }));
        let (count, geometry_failures) = readback_law_failures(&tree, words);
        println!(
            "fan geometry [{name}]: {} letters, {count} lowered scripts, {} failures",
            alphabet.len(),
            geometry_failures.len()
        );
        if let Some(first) = geometry_failures.first() {
            println!("  first failure: {first}");
        }
        failures.extend(geometry_failures);
        counts.push(count);
    }
    println!("fan geometry lowered script counts {counts:?}");
    assert_eq!(failures, Vec::<String>::new());
    assert_eq!(
        counts,
        [
            3_256, 3_947, 3_947, 4_014, 4_014, 4_038, 4_038, 4_629, 4_629, 4_613, 4_613, 4_686,
            4_686, 4_670, 4_670, 6_795, 6_795, 6_795, 6_795, 6_795, 6_795, 9_231
        ]
    );
}

/// A node id of a delta, with each added fan node at or above the node
/// floor named by its rank among those (ascending) instead of its id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum NamedNode {
    Fixed(u32),
    FreshFan(usize),
}

/// An added element under [`NamedNode`] names; an added edge at or above
/// the edge floor is named by its endpoints and label instead of its id.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum NamedElem {
    Node(NamedNode),
    FixedEdge(u32),
    FreshEdge(NamedNode, NamedNode, SEdge),
}

#[derive(Clone, Debug, PartialEq)]
enum NamedPayload {
    Node(Node),
    Edge(NamedNode, NamedNode, SEdge, Option<u32>),
}

/// The added part of `delta` with every fresh id (an added fan node at or
/// above `next_node`, an added edge at or above `next_edge`) replaced by a
/// name that does not mention it, or none when two added elements get one
/// name or a fresh fan node is misfiled. Two deltas with equal deleted and
/// attribute parts and equal named added parts differ by a bijective
/// renaming of fresh ids (map each fresh id to the other delta's id of the
/// same name), and by nothing else.
fn added_up_to_fresh_ids(
    delta: &GraphDelta,
    embedding: &TreeEmbedding,
) -> Option<BTreeMap<NamedElem, NamedPayload>> {
    let fresh_fans: Vec<u32> = delta
        .add
        .iter()
        .filter_map(|(element, payload)| match (element, payload) {
            (GElem::Node(id), GPayload::Node(node))
                if node.shape == NodeShape::HyperEdge && *id >= embedding.next_node =>
            {
                Some(*id)
            }
            _ => None,
        })
        .collect();
    let name = |id: u32| {
        fresh_fans
            .iter()
            .position(|fan| *fan == id)
            .map_or(NamedNode::Fixed(id), NamedNode::FreshFan)
    };
    let mut named = BTreeMap::new();
    for (element, payload) in &delta.add {
        let (key, value) = match (element, payload) {
            (GElem::Node(id), GPayload::Node(node)) => {
                let mut node = node.clone();
                if matches!(name(*id), NamedNode::FreshFan(_)) {
                    if node.id != *id {
                        return None;
                    }
                    node.id = 0;
                }
                (NamedElem::Node(name(*id)), NamedPayload::Node(node))
            }
            (GElem::Edge(id), GPayload::Edge(edge)) => {
                let (src, tgt) = (name(edge.src), name(edge.tgt));
                let key = if *id >= embedding.next_edge {
                    NamedElem::FreshEdge(src, tgt, edge.label.clone())
                } else {
                    NamedElem::FixedEdge(*id)
                };
                (
                    key,
                    NamedPayload::Edge(src, tgt, edge.label.clone(), edge.position),
                )
            }
            _ => return None,
        };
        if named.insert(key, value).is_some() {
            return None;
        }
    }
    Some(named)
}

/// Whether two deltas, each with fresh ids above the floors of its own
/// embedding (the two with the same carriers), differ only by renaming
/// fresh ids.
fn equal_up_to_fresh_ids(
    left: &GraphDelta,
    left_floors: &TreeEmbedding,
    right: &GraphDelta,
    right_floors: &TreeEmbedding,
) -> bool {
    left.del == right.del
        && left.attrs == right.attrs
        && added_up_to_fresh_ids(left, left_floors)
            .is_some_and(|named| Some(named) == added_up_to_fresh_ids(right, right_floors))
}

/// The two steps that relocate the fan suffix from fan node `fan` and
/// change nothing else: a leaf inserted under the root over the fan id, then
/// deleted.
fn relocation_from(tree: &Tree, fan: u32) -> Vec<TreeEdit> {
    vec![
        TreeEdit::InsertNode {
            parent: tree.root,
            child_id: fan,
            node: Node::new(fan, "relocation"),
            edge: SEdge::new(
                &tree.nodes[&tree.root].anchor,
                "relocation",
                "child",
                Some("relocation"),
            ),
        },
        TreeEdit::DeleteNode { id: fan },
    ]
}

/// The soundness oracle: applying `reading` to the tree gives the tree form
/// of the delta's result, and the delta is, up to renaming fresh ids, the
/// lowering of `reading` alone or of `reading` after relocating the fan
/// suffix from some fan of the embedding. The reading is applied and
/// lowered under `allocation`, which has the carriers of `embedding` and
/// floors with room for every id the reading draws: the ids it draws are
/// immaterial up to that renaming, so such floors stand for an id space
/// that does not run out.
fn delta_is_an_image_of_reading(
    tree: &Tree,
    embedding: &TreeEmbedding,
    allocation: &TreeEmbedding,
    graph: &Graph,
    delta: &GraphDelta,
    reading: &TreeEdit,
) -> bool {
    let mut read_embedding = allocation.clone();
    let read_tree = edit::apply_tree(reading, tree, &mut read_embedding).ok();
    let edited_tree = edit::apply(delta, graph).ok().and_then(|edited| {
        panproto_reference::encode::graph_to_tree(&edited, tree.root, &|_| {
            ParentNameRule::Exact(None)
        })
        .ok()
    });
    read_tree.is_some()
        && read_tree == edited_tree
        && std::iter::once(Vec::new())
            .chain(
                embedding
                    .fans
                    .iter()
                    .map(|(fan, _, _)| relocation_from(tree, *fan)),
            )
            .any(|mut steps| {
                steps.push(reading.clone());
                let mut lowered_embedding = allocation.clone();
                edit::graph_delta_from_tree_edit(
                    tree,
                    &mut lowered_embedding,
                    &TreeEdit::Sequence(steps),
                    None,
                )
                .is_ok_and(|lowered| equal_up_to_fresh_ids(delta, embedding, &lowered, allocation))
            })
}

/// `delta` with each fresh fan node id n renamed to `node(n)` and each fresh
/// edge id e to `edge(e)`, in keys, node payloads and edge endpoints.
fn rename_fresh_ids(
    delta: &GraphDelta,
    embedding: &TreeEmbedding,
    node: impl Fn(u32) -> u32,
    edge: impl Fn(u32) -> u32,
) -> GraphDelta {
    let fresh_fan = |id: u32| {
        id >= embedding.next_node
            && matches!(
                delta.add.get(&GElem::Node(id)),
                Some(GPayload::Node(added)) if added.shape == NodeShape::HyperEdge
            )
    };
    let node_id = |id: u32| if fresh_fan(id) { node(id) } else { id };
    let mut renamed = GraphDelta {
        del: delta.del.clone(),
        add: BTreeMap::new(),
        attrs: delta.attrs.clone(),
    };
    for (element, payload) in &delta.add {
        let entry = match (element, payload) {
            (GElem::Node(id), GPayload::Node(added)) => {
                let mut added = added.clone();
                added.id = node_id(added.id);
                (GElem::Node(node_id(*id)), GPayload::Node(added))
            }
            (GElem::Edge(id), GPayload::Edge(added)) => {
                let mut added = added.clone();
                added.src = node_id(added.src);
                added.tgt = node_id(added.tgt);
                let id = if *id >= embedding.next_edge {
                    edge(*id)
                } else {
                    *id
                };
                (GElem::Edge(id), GPayload::Edge(added))
            }
            (element, payload) => (element.clone(), payload.clone()),
        };
        renamed.add.insert(entry.0, entry.1);
    }
    renamed
}

/// `delta` with every fresh id raised by `by`, which keeps the order of the
/// fresh fan nodes and of the fresh edges among all ids short of the
/// maximum (an id that would pass it wraps, still a delta to read).
fn shift_fresh_ids(delta: &GraphDelta, embedding: &TreeEmbedding, by: u32) -> GraphDelta {
    rename_fresh_ids(
        delta,
        embedding,
        |id| id.wrapping_add(by),
        |id| id.wrapping_add(by),
    )
}

/// The deltas one perturbation away from `delta`: one part dropped; one
/// kept element deleted; each attribute change swapped or made a no-op, and
/// a no-op anchor change of the root added; each added payload changed;
/// each two added payloads of one sort swapped; every nonempty subset of
/// the carriers of each fan the delta leaves alone re-drawn under fresh ids;
/// the fresh ids shifted, reversed among the fresh edges, and a fresh fan
/// node moved onto each id the delta deletes.
fn delta_perturbations(
    graph: &Graph,
    embedding: &TreeEmbedding,
    delta: &GraphDelta,
) -> Vec<GraphDelta> {
    let mut out = Vec::new();
    let with = |change: &dyn Fn(&mut GraphDelta)| {
        let mut perturbed = delta.clone();
        change(&mut perturbed);
        perturbed
    };
    for element in delta.del.keys() {
        out.push(with(&|p| {
            p.del.remove(element);
        }));
    }
    for element in delta.add.keys() {
        out.push(with(&|p| {
            p.add.remove(element);
        }));
    }
    for key in delta.attrs.keys() {
        out.push(with(&|p| {
            p.attrs.remove(key);
        }));
    }
    let kept = graph
        .nodes
        .keys()
        .map(|id| GElem::Node(*id))
        .chain(graph.edges.keys().map(|id| GElem::Edge(*id)))
        .filter(|element| !delta.del.contains_key(element));
    for element in kept {
        out.push(with(&|p| {
            p.del
                .insert(element.clone(), graph_payload(graph, &element));
        }));
    }
    for (key, (old, new)) in &delta.attrs {
        for change in [(new, old), (old, old), (new, new)] {
            out.push(with(&|p| {
                p.attrs
                    .insert(key.clone(), (change.0.clone(), change.1.clone()));
            }));
        }
    }
    let root_anchor = AttrVal::Anchor(graph.nodes[&0].anchor.clone());
    out.push(with(&|p| {
        p.attrs
            .entry(AttrKey::Anchor(0))
            .or_insert_with(|| (root_anchor.clone(), root_anchor.clone()));
    }));
    for (element, payload) in &delta.add {
        let mut changed = payload.clone();
        match &mut changed {
            GPayload::Node(node) => node.anchor = "perturbed".into(),
            GPayload::Edge(edge) => edge.label.name = Some("perturbed".into()),
        }
        out.push(with(&|p| {
            p.add.insert(element.clone(), changed.clone());
        }));
    }
    let added: Vec<_> = delta.add.iter().collect();
    for (index, (first, first_payload)) in added.iter().enumerate() {
        for (second, second_payload) in &added[index + 1..] {
            if matches!(first, GElem::Node(_)) == matches!(second, GElem::Node(_)) {
                out.push(with(&|p| {
                    p.add.insert((*first).clone(), (*second_payload).clone());
                    p.add.insert((*second).clone(), (*first_payload).clone());
                }));
            }
        }
    }
    out.extend(fan_redraw_perturbations(graph, embedding, delta));
    out.extend(fresh_id_perturbations(embedding, delta));
    out.retain(|perturbed| perturbed != delta);
    out
}

/// Every nonempty subset of the carriers of each fan that `delta` leaves
/// alone, deleted and re-added under ids above every id the delta adds.
fn fan_redraw_perturbations(
    graph: &Graph,
    embedding: &TreeEmbedding,
    delta: &GraphDelta,
) -> Vec<GraphDelta> {
    let mut out = Vec::new();
    let fresh_node = delta
        .add
        .keys()
        .filter_map(|element| match element {
            GElem::Node(id) => Some(id.wrapping_add(1)),
            GElem::Edge(_) => None,
        })
        .fold(embedding.next_node, u32::max);
    let fresh_edge = delta
        .add
        .keys()
        .filter_map(|element| match element {
            GElem::Edge(id) => Some(id.wrapping_add(1)),
            GElem::Node(_) => None,
        })
        .fold(embedding.next_edge, u32::max);
    for (fan, parent, children) in &embedding.fans {
        let carriers: Vec<GElem> = [GElem::Node(*fan), GElem::Edge(*parent)]
            .into_iter()
            .chain(children.values().map(|edge| GElem::Edge(*edge)))
            .collect();
        if carriers
            .iter()
            .any(|carrier| delta.del.contains_key(carrier))
        {
            continue;
        }
        for mask in 1u32..(1 << carriers.len()) {
            let mut perturbed = delta.clone();
            let new_fan = if mask & 1 == 0 { *fan } else { fresh_node };
            let mut next_edge = fresh_edge;
            for (bit, carrier) in carriers.iter().enumerate() {
                if mask >> bit & 1 == 0 {
                    continue;
                }
                perturbed
                    .del
                    .insert(carrier.clone(), graph_payload(graph, carrier));
                match carrier {
                    GElem::Node(_) => {
                        let mut moved = graph.nodes[fan].clone();
                        moved.id = new_fan;
                        perturbed
                            .add
                            .insert(GElem::Node(new_fan), GPayload::Node(moved));
                    }
                    GElem::Edge(id) => {
                        let mut moved = graph.edges[id].clone();
                        moved.src = new_fan;
                        perturbed
                            .add
                            .insert(GElem::Edge(next_edge), GPayload::Edge(moved));
                        next_edge += 1;
                    }
                }
            }
            out.push(perturbed);
        }
    }
    out
}

/// `delta` with its fresh ids shifted, reversed among the fresh edges, and
/// each fresh fan node moved onto each node id the delta deletes.
fn fresh_id_perturbations(embedding: &TreeEmbedding, delta: &GraphDelta) -> Vec<GraphDelta> {
    let mut out = vec![shift_fresh_ids(delta, embedding, 40)];
    let fresh_edges: Vec<u32> = delta
        .add
        .keys()
        .filter_map(|element| match element {
            GElem::Edge(id) if *id >= embedding.next_edge => Some(*id),
            _ => None,
        })
        .collect();
    out.push(rename_fresh_ids(
        delta,
        embedding,
        |id| id,
        |id| {
            let index = fresh_edges.iter().position(|edge| *edge == id).unwrap();
            fresh_edges[fresh_edges.len() - 1 - index]
        },
    ));
    let fresh_fans: Vec<u32> = delta
        .add
        .iter()
        .filter_map(|(element, payload)| match (element, payload) {
            (GElem::Node(id), GPayload::Node(node))
                if node.shape == NodeShape::HyperEdge && *id >= embedding.next_node =>
            {
                Some(*id)
            }
            _ => None,
        })
        .collect();
    for fan in &fresh_fans {
        for deleted in delta.del.keys() {
            if let GElem::Node(target) = deleted {
                out.push(rename_fresh_ids(
                    delta,
                    embedding,
                    |id| if id == *fan { *target } else { id },
                    |id| id,
                ));
            }
        }
    }
    out
}

/// Counts of a soundness sweep: lowered scripts, perturbed deltas, and
/// accepted perturbed deltas.
#[derive(Debug, Default, PartialEq, Eq)]
struct SoundnessCounts {
    lowered: usize,
    perturbed: usize,
    accepted: usize,
}

/// Read every lowering of `scripts` and every perturbation of it, and check
/// the soundness oracle on each accepted delta (the lowerings themselves
/// included). Returns the counts and every finding.
fn soundness_findings(
    tree: &Tree,
    scripts: impl Iterator<Item = TreeEdit>,
) -> (SoundnessCounts, Vec<String>) {
    let (graph, embedding) = panproto_reference::encode::tree_to_graph(tree, None).unwrap();
    let mut counts = SoundnessCounts::default();
    let mut findings = Vec::new();
    for script in scripts {
        let mut lowered_embedding = embedding.clone();
        let Ok(delta) =
            edit::graph_delta_from_tree_edit(tree, &mut lowered_embedding, &script, None)
        else {
            continue;
        };
        counts.lowered += 1;
        match edit::tree_edit_from_graph_delta(tree, &embedding, &delta, None) {
            Ok(reading)
                if delta_is_an_image_of_reading(
                    tree, &embedding, &embedding, &graph, &delta, &reading,
                ) => {}
            other => findings.push(format!("lowering of {script:?} read as {other:?}")),
        }
        for perturbed in delta_perturbations(&graph, &embedding, &delta) {
            counts.perturbed += 1;
            if let Ok(reading) =
                edit::tree_edit_from_graph_delta(tree, &embedding, &perturbed, None)
            {
                counts.accepted += 1;
                if !delta_is_an_image_of_reading(
                    tree, &embedding, &embedding, &graph, &perturbed, &reading,
                ) {
                    findings.push(format!(
                        "{perturbed:?} (perturbing {script:?}) accepted as {reading:?}"
                    ));
                }
            }
        }
    }
    (counts, findings)
}

/// Every word of length one or two over `alphabet`.
fn words_up_to_two(alphabet: &[TreeEdit]) -> impl Iterator<Item = TreeEdit> + '_ {
    alphabet
        .iter()
        .cloned()
        .chain(alphabet.iter().flat_map(move |first| {
            alphabet
                .iter()
                .map(move |second| TreeEdit::Sequence(vec![first.clone(), second.clone()]))
        }))
}

// The smallest reader soundness case: inserting a leaf over the first fan's
// node id relocates both fans of `fan@2:3,fan@2:3`, and a delta that does
// so but names the first relocated fan's parent edge is not the lowering of
// that insertion (a relocated fan parent edge keeps the name the graph gave
// it). The reader must refuse it, since it checks only the carriers it
// keeps otherwise.
#[test]
fn a_relocated_fan_with_a_renamed_parent_edge_is_not_read_as_its_relocating_insertion() {
    let tree = Tree {
        fans: vec![arm_fan(2, "fan", 3), arm_fan(2, "fan", 3)],
        ..join_tree()
    };
    let (_, embedding) = panproto_reference::encode::tree_to_graph(&tree, None).unwrap();
    let fan = embedding.fans[0].0;
    let insert = TreeEdit::InsertNode {
        parent: 0,
        child_id: fan,
        node: Node::new(fan, "leaf"),
        edge: SEdge::new(&tree.nodes[&0].anchor, "leaf", "child", Some("inserted")),
    };
    let mut lowered_embedding = embedding.clone();
    let delta =
        edit::graph_delta_from_tree_edit(&tree, &mut lowered_embedding, &insert, None).unwrap();
    assert_eq!(
        edit::tree_edit_from_graph_delta(&tree, &embedding, &delta, None),
        Ok(insert)
    );
    let (parent_edge, payload) = delta
        .add
        .iter()
        .find(|(_, payload)| {
            matches!(payload, GPayload::Edge(edge) if edge.label.kind == "hyper_parent")
        })
        .map(|(element, payload)| (element.clone(), payload.clone()))
        .unwrap();
    let GPayload::Edge(mut renamed) = payload else {
        unreachable!("the found payload is an edge");
    };
    renamed.label.name = Some("perturbed".to_owned());
    let mut perturbed = delta;
    perturbed.add.insert(parent_edge, GPayload::Edge(renamed));
    assert!(matches!(
        edit::tree_edit_from_graph_delta(&tree, &embedding, &perturbed, None),
        Err(edit::TreeApplyError::NotTreeEdit { .. })
    ));
}

// Soundness of the reader, checked exhaustively on bounded perturbations
// of lowerings: every delta the reader accepts, a lowering or one
// perturbation away from one, is the lowering of its reading up to renaming
// fresh ids and relocating a whole fan suffix, and has its reading's
// effect. The totality sweeps above check only that lowerings read back.
#[test]
fn every_accepted_perturbation_of_a_short_lowering_is_an_image_of_its_reading() {
    let mut counts = Vec::new();
    let mut findings = Vec::new();
    for (name, tree) in fan_geometries() {
        let alphabet = fan_script_alphabet(&tree);
        let (geometry_counts, geometry_findings) =
            soundness_findings(&tree, words_up_to_two(&alphabet));
        println!(
            "soundness [{name}]: {geometry_counts:?}, {} findings",
            geometry_findings.len()
        );
        if let Some(first) = geometry_findings.first() {
            println!("  first finding: {first}");
        }
        findings.extend(geometry_findings);
        counts.push(geometry_counts);
    }
    for tree in readback_trees() {
        let alphabet = tree_edit_alphabet(&tree, false);
        let (tree_counts, tree_findings) = soundness_findings(&tree, words_up_to_two(&alphabet));
        println!(
            "soundness [readback tree with {} fans]: {tree_counts:?}, {} findings",
            tree.fans.len(),
            tree_findings.len()
        );
        if let Some(first) = tree_findings.first() {
            println!("  first finding: {first}");
        }
        findings.extend(tree_findings);
        counts.push(tree_counts);
    }
    let total = counts
        .iter()
        .fold(SoundnessCounts::default(), |sum, each| SoundnessCounts {
            lowered: sum.lowered + each.lowered,
            perturbed: sum.perturbed + each.perturbed,
            accepted: sum.accepted + each.accepted,
        });
    println!("soundness total: {total:?}, {} findings", findings.len());
    assert_eq!(
        findings.len(),
        0,
        "first findings: {:?}",
        &findings[..findings.len().min(5)]
    );
    let pinned = |lowered, perturbed, accepted| SoundnessCounts {
        lowered,
        perturbed,
        accepted,
    };
    assert_eq!(
        counts,
        [
            pinned(245, 8_590, 1_539),
            pinned(277, 9_822, 1_883),
            pinned(277, 9_814, 1_861),
            pinned(279, 10_293, 1_881),
            pinned(279, 10_286, 1_881),
            pinned(280, 10_178, 1_949),
            pinned(280, 10_163, 1_927),
            pinned(308, 10_997, 2_123),
            pinned(308, 10_997, 2_123),
            pinned(308, 10_852, 2_132),
            pinned(308, 10_844, 2_134),
            pinned(310, 11_342, 2_157),
            pinned(310, 11_335, 2_145),
            pinned(310, 11_189, 2_170),
            pinned(310, 11_174, 2_160),
            pinned(390, 19_276, 3_882),
            pinned(390, 19_270, 3_890),
            pinned(390, 19_248, 3_862),
            pinned(390, 19_228, 3_838),
            pinned(390, 19_236, 3_944),
            pinned(390, 19_222, 3_910),
            pinned(474, 29_854, 6_090),
            pinned(1_534, 40_071, 8_105),
            pinned(1_689, 62_129, 12_508),
            pinned(1_695, 61_241, 11_705),
        ]
    );
    assert_eq!(total, pinned(12_121, 466_651, 91_799));
}

// The reading depends on the values of fresh ids only through the exact
// single-step path. Shifting every fresh id by 40 or by 80 keeps the order
// of the fresh fan nodes and of the fresh edges among all ids, so it keeps
// the result's tree form and the composite reader's inputs; the shifted
// deltas are read alike, with the lowering's effect, and differ from the
// lowering's own reading only where that reading lowers to the delta
// exactly.
#[test]
fn order_preserving_fresh_id_renamings_change_only_exact_single_step_readings() {
    let mut counts = Vec::new();
    let mut failures = Vec::new();
    for (name, tree) in fan_geometries() {
        let (_, embedding) = panproto_reference::encode::tree_to_graph(&tree, None).unwrap();
        let alphabet = fan_script_alphabet(&tree);
        let (mut lowered, mut changed) = (0, 0);
        for script in words_up_to_two(&alphabet) {
            let mut lowered_embedding = embedding.clone();
            let Ok(delta) =
                edit::graph_delta_from_tree_edit(&tree, &mut lowered_embedding, &script, None)
            else {
                continue;
            };
            lowered += 1;
            let read = |delta: &GraphDelta| {
                edit::tree_edit_from_graph_delta(&tree, &embedding, delta, None)
            };
            let effect = |reading: &TreeEdit| {
                let mut followed = embedding.clone();
                edit::apply_tree(reading, &tree, &mut followed)
            };
            let reading = read(&delta).unwrap();
            let shifted = read(&shift_fresh_ids(&delta, &embedding, 40));
            let shifted_again = read(&shift_fresh_ids(&delta, &embedding, 80));
            let Ok(shifted) = shifted else {
                failures.push(format!(
                    "[{name}] shifted lowering of {script:?}: {shifted:?}"
                ));
                continue;
            };
            if shifted_again.as_ref() != Ok(&shifted) || effect(&shifted) != effect(&reading) {
                failures.push(format!(
                    "[{name}] {script:?} read as {shifted:?} and {shifted_again:?}"
                ));
            }
            if shifted != reading {
                changed += 1;
                let mut relowered = embedding.clone();
                if edit::graph_delta_from_tree_edit(&tree, &mut relowered, &reading, None)
                    != Ok(delta.clone())
                {
                    failures.push(format!(
                        "[{name}] {script:?} read as {reading:?}, shifted as {shifted:?}"
                    ));
                }
            }
        }
        counts.push((lowered, changed));
    }
    println!("fresh-id renaming (lowered, changed) per fan geometry: {counts:?}");
    assert_eq!(failures, Vec::<String>::new());
    assert_eq!(
        counts,
        [
            (245, 14),
            (277, 14),
            (277, 14),
            (279, 13),
            (279, 13),
            (280, 13),
            (280, 13),
            (308, 15),
            (308, 15),
            (308, 15),
            (308, 15),
            (310, 14),
            (310, 14),
            (310, 14),
            (310, 14),
            (390, 13),
            (390, 13),
            (390, 13),
            (390, 13),
            (390, 13),
            (390, 13),
            (474, 13),
        ]
    );
}

/// `embedding` with its floors set to `next_node` and `next_edge`.
fn with_floors(embedding: &TreeEmbedding, next_node: u32, next_edge: u32) -> TreeEmbedding {
    TreeEmbedding {
        next_node,
        next_edge,
        ..embedding.clone()
    }
}

/// The node and edge ids applying `edit` takes above the floors of
/// `embedding`, or none when it refuses.
fn ids_taken(tree: &Tree, embedding: &TreeEmbedding, edit: &TreeEdit) -> Option<(u32, u32)> {
    let mut followed = embedding.clone();
    edit::apply_tree(edit, tree, &mut followed).ok()?;
    Some((
        followed.next_node - embedding.next_node,
        followed.next_edge - embedding.next_edge,
    ))
}

/// Counts of a ceiling sweep: scripts lowered under the encoder's floors,
/// (script, floors) pairs tried, lowerings refused for want of ids, lowerings
/// read back, and readings that would themselves run out of ids under the
/// floors their delta was lowered under.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct CeilingCounts {
    lowered: usize,
    floors: usize,
    exhausted: usize,
    read: usize,
    reading_exhausts: usize,
}

/// The room of a ceiling sweep (the most node and edge ids any lowered
/// script or its reading takes under the encoder's floors), its counts and
/// its findings, for [`ceiling_findings`].
struct CeilingSweep {
    room: (u32, u32),
    counts: CeilingCounts,
    findings: Vec<String>,
}

/// Lower each script under the encoder's floors `base`, counting it in the
/// sweep, and widen the sweep's room to the ids it and its reading take.
fn base_lowerings<'a>(
    tree: &Tree,
    base: &TreeEmbedding,
    scripts: &'a [TreeEdit],
    sweep: &mut CeilingSweep,
) -> Vec<(&'a TreeEdit, GraphDelta)> {
    let mut lowerings = Vec::new();
    for script in scripts {
        let mut base_lowered = base.clone();
        let Ok(base_delta) =
            edit::graph_delta_from_tree_edit(tree, &mut base_lowered, script, None)
        else {
            continue;
        };
        sweep.counts.lowered += 1;
        let Ok(base_reading) = edit::tree_edit_from_graph_delta(tree, base, &base_delta, None)
        else {
            sweep
                .findings
                .push(format!("{script:?} does not read back"));
            continue;
        };
        for taken in [
            ids_taken(tree, base, script),
            ids_taken(tree, base, &base_reading),
        ]
        .into_iter()
        .flatten()
        {
            sweep.room = (sweep.room.0.max(taken.0), sweep.room.1.max(taken.1));
        }
        lowerings.push((script, base_delta));
    }
    lowerings
}

/// Law (c) for a single-step script lowered to `delta` under the raised
/// floors `ceiling`, reaching `lowered`, read as `reading`: over unbounded
/// ids the reading lowers to `delta` and reaches the carriers of `lowered`,
/// and (R1) its `u32` lowering under `ceiling` is that delta with those
/// carriers or, short of full room, runs out of ids.
fn reads_exactly_at_the_ceiling(
    tree: &Tree,
    ceiling: &TreeEmbedding,
    delta: &GraphDelta,
    lowered: &TreeEmbedding,
    reading: &TreeEdit,
    full_room: bool,
) -> bool {
    let mut relowered = edit::unbounded_embedding(ceiling);
    let exact = edit::graph_delta_from_tree_edit_unbounded(tree, &mut relowered, reading, None)
        == Ok(Some(delta.clone()))
        && same_carriers(&relowered, &edit::unbounded_embedding(lowered));
    let mut relowered = ceiling.clone();
    let refines = match edit::graph_delta_from_tree_edit(tree, &mut relowered, reading, None) {
        Ok(relowering) => relowering == *delta && same_carriers(&relowered, lowered),
        Err(edit::TreeApplyError::IdsExhausted) => !full_room,
        Err(_) => false,
    };
    exact && refines
}

/// Lower each script under every pair of floors that leaves at most the
/// sweep's room of node and edge ids below `u32::MAX`, and check each
/// lowering: it is refused only by `IdsExhausted` and never with full room,
/// it is the lowering under the encoder's floors up to renaming fresh ids,
/// it reads back (law a), the reading has the script's effect (law b) and,
/// for a script of at most one step, lowers over unbounded ids to the delta
/// and reaches the carriers of the script's embedding (law c), while its
/// `u32` lowering is that one or, short of full room, runs out of ids, the soundness oracle accepts it with the encoder's
/// floors standing for an unbounded id space, and applying the reading
/// under the raised floors either has that effect or refuses with
/// `IdsExhausted`.
fn ceiling_findings(tree: &Tree, scripts: &[TreeEdit]) -> CeilingSweep {
    let (graph, base) = panproto_reference::encode::tree_to_graph(tree, None).unwrap();
    let mut sweep = CeilingSweep {
        room: (0, 0),
        counts: CeilingCounts::default(),
        findings: Vec::new(),
    };
    let lowerings = base_lowerings(tree, &base, scripts, &mut sweep);
    let room = sweep.room;
    for (script, base_delta) in lowerings {
        let effect = edit::apply_tree(script, tree, &mut base.clone());
        for node_room in 0..=room.0 {
            for edge_room in 0..=room.1 {
                sweep.counts.floors += 1;
                let ceiling = with_floors(&base, u32::MAX - node_room, u32::MAX - edge_room);
                let full_room = (node_room, edge_room) == room;
                let at = format!("{script:?} with room ({node_room}, {edge_room})");
                let mut lowered = ceiling.clone();
                let delta = match edit::graph_delta_from_tree_edit(tree, &mut lowered, script, None)
                {
                    Ok(delta) => delta,
                    Err(edit::TreeApplyError::IdsExhausted) if !full_room => {
                        sweep.counts.exhausted += 1;
                        continue;
                    }
                    Err(error) => {
                        sweep
                            .findings
                            .push(format!("{at}: lowering refused with {error:?}"));
                        continue;
                    }
                };
                if !equal_up_to_fresh_ids(&delta, &ceiling, &base_delta, &base) {
                    sweep
                        .findings
                        .push(format!("{at}: lowering differs beyond fresh ids"));
                }
                let reading = match edit::tree_edit_from_graph_delta(tree, &ceiling, &delta, None) {
                    Ok(reading) => reading,
                    Err(error) => {
                        sweep.findings.push(format!("{at}: read as {error:?}"));
                        continue;
                    }
                };
                sweep.counts.read += 1;
                if edit::apply_tree(&reading, tree, &mut base.clone()) != effect {
                    sweep
                        .findings
                        .push(format!("{at}: {reading:?} has another effect"));
                }
                if script.steps().len() <= 1
                    && !reads_exactly_at_the_ceiling(
                        tree, &ceiling, &delta, &lowered, &reading, full_room,
                    )
                {
                    sweep
                        .findings
                        .push(format!("{at}: read inexactly as {reading:?}"));
                }
                if !delta_is_an_image_of_reading(tree, &ceiling, &base, &graph, &delta, &reading) {
                    sweep
                        .findings
                        .push(format!("{at}: not an image of {reading:?}"));
                }
                match edit::apply_tree(&reading, tree, &mut ceiling.clone()) {
                    Err(edit::TreeApplyError::IdsExhausted) if !full_room => {
                        sweep.counts.reading_exhausts += 1;
                    }
                    applied if applied == effect => {}
                    applied => sweep.findings.push(format!(
                        "{at}: {reading:?} applied under the raised floors as {applied:?}"
                    )),
                }
            }
        }
    }
    sweep
}

/// The base sets of the soundness sweep, named: every word of length one
/// or two over [`fan_script_alphabet`] for each of the [`fan_geometries`],
/// and over the narrow [`tree_edit_alphabet`] for each of the
/// [`readback_trees`].
fn short_script_sets() -> Vec<(String, Tree, Vec<TreeEdit>)> {
    let mut sets: Vec<(String, Tree, Vec<TreeEdit>)> = fan_geometries()
        .into_iter()
        .map(|(name, tree)| {
            let words = words_up_to_two(&fan_script_alphabet(&tree)).collect();
            (name, tree, words)
        })
        .collect();
    for tree in readback_trees() {
        let words = words_up_to_two(&tree_edit_alphabet(&tree, false)).collect();
        sets.push((
            format!("readback tree with {} fans", tree.fans.len()),
            tree,
            words,
        ));
    }
    sets
}

// Law (a) holds over every lowering, at every floor: lower every short
// script of every geometry above with each floor anywhere from the
// ceiling down to the most ids any of these scripts or readings takes, and
// read it back. The reading's own ids are immaterial, so a reading that
// needs more ids than its delta's script is still a reading; applying it
// under those floors is what refuses.
#[test]
fn lowerings_read_back_soundly_with_every_floor_near_the_id_ceiling() {
    let sets = short_script_sets();
    let sweeps: Vec<CeilingSweep> = std::thread::scope(|scope| {
        let mut handles = Vec::with_capacity(sets.len());
        for (_, tree, words) in &sets {
            handles.push(scope.spawn(move || ceiling_findings(tree, words)));
        }
        handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect()
    });
    let mut findings = Vec::new();
    let mut counts = Vec::new();
    for ((name, _, _), sweep) in sets.iter().zip(sweeps) {
        println!(
            "ceiling [{name}]: room {:?}, {:?}, {} findings",
            sweep.room,
            sweep.counts,
            sweep.findings.len()
        );
        if let Some(first) = sweep.findings.first() {
            println!("  first finding: {first}");
        }
        findings.extend(sweep.findings);
        counts.push((sweep.room, sweep.counts));
    }
    let total = counts
        .iter()
        .fold(CeilingCounts::default(), |sum, (_, each)| CeilingCounts {
            lowered: sum.lowered + each.lowered,
            floors: sum.floors + each.floors,
            exhausted: sum.exhausted + each.exhausted,
            read: sum.read + each.read,
            reading_exhausts: sum.reading_exhausts + each.reading_exhausts,
        });
    println!("ceiling total: {total:?}, {} findings", findings.len());
    assert_eq!(
        findings.len(),
        0,
        "first findings: {:?}",
        &findings[..findings.len().min(5)]
    );
    let pinned = |room, [lowered, floors, exhausted, read, reading_exhausts]: [usize; 5]| {
        (
            room,
            CeilingCounts {
                lowered,
                floors,
                exhausted,
                read,
                reading_exhausts,
            },
        )
    };
    assert_eq!(
        counts,
        [
            pinned((4, 11), [245, 14_700, 3_469, 11_231, 245]),
            pinned((4, 11), [277, 16_620, 4_294, 12_326, 245]),
            pinned((4, 11), [277, 16_620, 4_294, 12_326, 245]),
            pinned((4, 11), [279, 16_740, 4_365, 12_375, 294]),
            pinned((4, 11), [279, 16_740, 4_365, 12_375, 278]),
            pinned((4, 11), [280, 16_800, 4_350, 12_450, 294]),
            pinned((4, 11), [280, 16_800, 4_350, 12_450, 278]),
            pinned((4, 11), [308, 18_480, 4_570, 13_910, 245]),
            pinned((4, 11), [308, 18_480, 4_570, 13_910, 245]),
            pinned((4, 11), [308, 18_480, 4_555, 13_925, 245]),
            pinned((4, 11), [308, 18_480, 4_555, 13_925, 245]),
            pinned((4, 11), [310, 18_600, 4_626, 13_974, 294]),
            pinned((4, 11), [310, 18_600, 4_626, 13_974, 278]),
            pinned((4, 11), [310, 18_600, 4_611, 13_989, 294]),
            pinned((4, 11), [310, 18_600, 4_611, 13_989, 278]),
            pinned((6, 15), [390, 43_680, 11_083, 32_597, 529]),
            pinned((6, 15), [390, 43_680, 11_083, 32_597, 529]),
            pinned((6, 15), [390, 43_680, 11_083, 32_597, 509]),
            pinned((6, 15), [390, 43_680, 11_083, 32_597, 485]),
            pinned((6, 15), [390, 43_680, 11_083, 32_597, 509]),
            pinned((6, 15), [390, 43_680, 11_083, 32_597, 485]),
            pinned((8, 19), [474, 85_320, 22_163, 63_157, 689]),
            pinned((4, 7), [1_534, 61_360, 15_731, 45_629, 1_727]),
            pinned((4, 11), [1_689, 101_340, 24_695, 76_645, 1_660]),
            pinned((4, 11), [1_695, 101_700, 24_639, 77_061, 1_680]),
        ]
    );
    assert_eq!(
        total,
        pinned((0, 0), [12_121, 875_140, 219_937, 655_203, 12_805]).1
    );
}

/// The letters that name tree node ids at the top of `u32`: a leaf
/// inserted under the root at each of the three highest ids, one under
/// node 2, one under another high node, a high node moved under node 2,
/// and the highest deleted.
fn high_id_letters() -> Vec<TreeEdit> {
    let [top, next, third] = HIGH_IDS;
    vec![
        insert_leaf(0, top),
        insert_leaf(0, next),
        insert_leaf(0, third),
        insert_leaf(2, next),
        insert_leaf(next, third),
        TreeEdit::MoveSubtree {
            node_id: next,
            new_parent: 2,
            edge: inserted_label(),
        },
        TreeEdit::DeleteNode { id: top },
    ]
}

/// The three highest node ids.
const HIGH_IDS: [u32; 3] = [u32::MAX, u32::MAX - 1, u32::MAX - 2];

/// Every word of length one or two over `alphabet` and [`high_id_letters`]
/// with at least one high letter.
fn high_id_words(alphabet: &[TreeEdit]) -> Vec<TreeEdit> {
    let high = high_id_letters();
    let all: Vec<TreeEdit> = alphabet.iter().chain(&high).cloned().collect();
    let mut words: Vec<TreeEdit> = high.clone();
    for first in &all {
        for second in &all {
            if high.contains(first) || high.contains(second) {
                words.push(TreeEdit::Sequence(vec![first.clone(), second.clone()]));
            }
        }
    }
    words
}

/// The short-script sets of [`short_script_sets`] with their words split:
/// the words over the set's alphabet, and the words with a high letter.
fn high_id_script_sets() -> Vec<(String, Tree, Vec<TreeEdit>, Vec<TreeEdit>)> {
    let mut sets: Vec<_> = fan_geometries()
        .into_iter()
        .map(|(name, tree)| {
            let alphabet = fan_script_alphabet(&tree);
            let words = words_up_to_two(&alphabet).collect();
            (name, tree, words, high_id_words(&alphabet))
        })
        .collect();
    for tree in readback_trees() {
        let alphabet = tree_edit_alphabet(&tree, false);
        let words = words_up_to_two(&alphabet).collect();
        sets.push((
            format!("readback tree with {} fans", tree.fans.len()),
            tree,
            words,
            high_id_words(&alphabet),
        ));
    }
    sets
}

/// The graph of `tree` under `embedding`, written from the tree-to-graph
/// layout rather than by the crate: the tree's nodes; each arc at its
/// edge id with its own label; and per fan a node of the fan shape named
/// for the fan, a `hyper_parent` edge to the fan's parent and one
/// `hyper_label` edge per member, named for the member's label, each
/// labelled from the fan's name to its target's anchor. None when a
/// carrier id is not a `u32` id. Floors play no part.
fn graph_of_carriers(tree: &Tree, embedding: &TreeEmbedding<u128>) -> Option<Graph> {
    let mut graph = Graph {
        nodes: tree.nodes.clone(),
        edges: BTreeMap::new(),
    };
    for (parent, child, label) in &tree.arcs {
        let id = u32::try_from(*embedding.arcs.get(child)?).ok()?;
        graph
            .edges
            .insert(id, plain_edge(*parent, *child, label.clone()));
    }
    for (fan, (node_id, parent_edge, member_edges)) in tree.fans.iter().zip(&embedding.fans) {
        let node_id = u32::try_from(*node_id).ok()?;
        graph
            .nodes
            .insert(node_id, fan_node(node_id, &fan.hyper_edge));
        let anchor = |id: &u32| tree.nodes[id].anchor.clone();
        graph.edges.insert(
            u32::try_from(*parent_edge).ok()?,
            plain_edge(
                node_id,
                fan.parent,
                SEdge::new(&fan.hyper_edge, &anchor(&fan.parent), "hyper_parent", None),
            ),
        );
        for (label, member) in &fan.children {
            graph.edges.insert(
                u32::try_from(*member_edges.get(label)?).ok()?,
                plain_edge(
                    node_id,
                    *member,
                    SEdge::new(
                        &fan.hyper_edge,
                        &anchor(member),
                        "hyper_label",
                        Some(label.as_str()),
                    ),
                ),
            );
        }
    }
    Some(graph)
}

/// `reached` with the ids at or above the floors of `start` renamed in
/// order onto the lowest graph ids at or above those floors (fan nodes
/// skipping the tree's node ids), or none when too few remain. The renaming
/// keeps every id below the floors and the order of the fresh fan nodes
/// and of the fresh edges, so the graph it gives is the unbounded graph
/// with its fresh ids renamed.
fn compact_fresh_ids(
    tree: &Tree,
    start: &TreeEmbedding,
    reached: &TreeEmbedding<u128>,
) -> Option<TreeEmbedding<u128>> {
    let node_floor = u128::from(start.next_node);
    let edge_floor = u128::from(start.next_edge);
    let mut free_nodes = (start.next_node..=u32::MAX)
        .filter(|id| !tree.nodes.contains_key(id))
        .map(u128::from);
    let mut nodes = BTreeMap::new();
    for (node, _, _) in &reached.fans {
        if *node >= node_floor {
            nodes.insert(*node, free_nodes.next()?);
        }
    }
    let fresh_edges: BTreeSet<u128> = reached
        .arcs
        .values()
        .chain(
            reached
                .fans
                .iter()
                .flat_map(|(_, parent, children)| std::iter::once(parent).chain(children.values())),
        )
        .filter(|edge| **edge >= edge_floor)
        .copied()
        .collect();
    let mut edges = BTreeMap::new();
    for (rank, edge) in fresh_edges.into_iter().enumerate() {
        let renamed = edge_floor + u128::try_from(rank).ok()?;
        if renamed > u128::from(u32::MAX) {
            return None;
        }
        edges.insert(edge, renamed);
    }
    let node = |id: &u128| *nodes.get(id).unwrap_or(id);
    let edge = |id: &u128| *edges.get(id).unwrap_or(id);
    Some(TreeEmbedding {
        arcs: reached
            .arcs
            .iter()
            .map(|(child, id)| (*child, edge(id)))
            .collect(),
        fans: reached
            .fans
            .iter()
            .map(|(fan, parent, children)| {
                (
                    node(fan),
                    edge(parent),
                    children
                        .iter()
                        .map(|(label, id)| (label.clone(), edge(id)))
                        .collect(),
                )
            })
            .collect(),
        next_node: reached.next_node,
        next_edge: reached.next_edge,
    })
}

/// The step-by-step composite that lowering `steps` from `tree` gives,
/// read from the start and end graphs: the differ, except that a node held
/// by both graphs is a replacement when it changes between a fan node and a
/// tree node, or when it is a tree node that some intermediate tree of the
/// steps lacks (deleted by one step and inserted again by a later one).
fn composite_of_diff(
    before: &Graph,
    after: &Graph,
    tree: &Tree,
    embedding: &TreeEmbedding,
    steps: &[TreeEdit],
) -> GraphDelta {
    let mut gone = BTreeSet::new();
    let mut current = tree.clone();
    let mut reached = edit::unbounded_embedding(embedding);
    for step in steps.iter().flat_map(TreeEdit::steps) {
        let Ok(next) = edit::apply_tree_unbounded(step, &current, &mut reached) else {
            break;
        };
        gone.extend(
            tree.nodes
                .keys()
                .filter(|id| !next.nodes.contains_key(id))
                .copied(),
        );
        current = next;
    }
    let mut delta = edit::diff(before, after);
    for (id, old) in &before.nodes {
        let Some(new) = after.nodes.get(id) else {
            continue;
        };
        let status_changes =
            (old.shape == NodeShape::HyperEdge) != (new.shape == NodeShape::HyperEdge);
        if status_changes || (tree.nodes.contains_key(id) && gone.contains(id)) {
            delta = replacing(delta, before, *id, new.clone());
        }
    }
    delta
}

/// The soundness oracle over unbounded ids: applying `reading` to the tree
/// from the unbounded `embedding` gives the tree form of the delta's
/// result, and the delta is, up to renaming fresh ids, the unbounded
/// lowering of `reading` alone or after relocating the fan suffix from
/// some fan of the embedding. The lowering's fresh ids are compacted first
/// ([`compact_fresh_ids`]), which changes nothing up to that renaming and
/// lets a graph hold a lowering whose ids pass `u32::MAX`.
fn delta_is_an_unbounded_image_of_reading(
    tree: &Tree,
    embedding: &TreeEmbedding,
    graph: &Graph,
    delta: &GraphDelta,
    reading: &TreeEdit,
) -> bool {
    let Some(edited_tree) = edit::apply(delta, graph).ok().and_then(|edited| {
        panproto_reference::encode::graph_to_tree(&edited, tree.root, &|_| {
            ParentNameRule::Exact(None)
        })
        .ok()
    }) else {
        return false;
    };
    let read_tree =
        edit::apply_tree_unbounded(reading, tree, &mut edit::unbounded_embedding(embedding));
    read_tree.as_ref() == Ok(&edited_tree)
        && std::iter::once(Vec::new())
            .chain(
                embedding
                    .fans
                    .iter()
                    .map(|(fan, _, _)| relocation_from(tree, *fan)),
            )
            .any(|mut steps| {
                steps.push(reading.clone());
                let mut reached = edit::unbounded_embedding(embedding);
                let Ok(relocated_tree) = edit::apply_tree_unbounded(
                    &TreeEdit::Sequence(steps.clone()),
                    tree,
                    &mut reached,
                ) else {
                    return false;
                };
                let Some(compacted) = compact_fresh_ids(&relocated_tree, embedding, &reached)
                else {
                    return false;
                };
                let Some(lowered) = graph_of_carriers(&relocated_tree, &compacted) else {
                    return false;
                };
                let composite = composite_of_diff(graph, &lowered, tree, embedding, &steps);
                equal_up_to_fresh_ids(delta, embedding, &composite, embedding)
            })
}

/// The refinement law on one edit: applying and lowering it under the
/// `u32` embedding gives what the unbounded forms give, or refuses with
/// `IdsExhausted`. Returns whether the `u32` lowering ran out, or the
/// violation.
fn refinement_violation(
    tree: &Tree,
    embedding: &TreeEmbedding,
    edit_under: &TreeEdit,
) -> Result<bool, String> {
    let mut applied_embedding = embedding.clone();
    let applied = edit::apply_tree(edit_under, tree, &mut applied_embedding);
    let mut unbounded = edit::unbounded_embedding(embedding);
    let applied_unbounded = edit::apply_tree_unbounded(edit_under, tree, &mut unbounded);
    let apply_refines = match (&applied, &applied_unbounded) {
        (Err(edit::TreeApplyError::IdsExhausted), _) => true,
        (Ok(_), Ok(_)) => {
            applied == applied_unbounded
                && edit::unbounded_embedding(&applied_embedding) == unbounded
        }
        _ => applied == applied_unbounded,
    };
    let lowered = edit::graph_delta_from_tree_edit(tree, &mut embedding.clone(), edit_under, None);
    let lowered_unbounded = edit::graph_delta_from_tree_edit_unbounded(
        tree,
        &mut edit::unbounded_embedding(embedding),
        edit_under,
        None,
    );
    let lower_refines = match (&lowered, &lowered_unbounded) {
        (Err(edit::TreeApplyError::IdsExhausted), _) => true,
        (Ok(delta), Ok(Some(unbounded_delta))) => delta == unbounded_delta,
        (Err(error), Err(unbounded_error)) => error == unbounded_error,
        _ => false,
    };
    if apply_refines && lower_refines {
        Ok(lowered == Err(edit::TreeApplyError::IdsExhausted))
    } else {
        Err(format!(
            "{edit_under:?} under {embedding:?}: u32 {applied:?} / {lowered:?}, unbounded \
             {applied_unbounded:?} / {lowered_unbounded:?}"
        ))
    }
}

/// The deltas one high-id perturbation away from `delta`: each added tree
/// node moved onto each free id of [`HIGH_IDS`] (its arcs and fan edges
/// following it), and a leaf added under the root at each free high id
/// with an arc above every edge id the delta adds.
fn high_id_perturbations(graph: &Graph, delta: &GraphDelta) -> Vec<GraphDelta> {
    let taken = |id: u32| graph.nodes.contains_key(&id) || delta.add.contains_key(&GElem::Node(id));
    let added_tree_nodes: Vec<u32> = delta
        .add
        .iter()
        .filter_map(|(element, payload)| match (element, payload) {
            (GElem::Node(id), GPayload::Node(node)) if node.shape != NodeShape::HyperEdge => {
                Some(*id)
            }
            _ => None,
        })
        .collect();
    let next_edge = delta
        .add
        .keys()
        .filter_map(|element| match element {
            GElem::Edge(id) => Some(id.wrapping_add(1)),
            GElem::Node(_) => None,
        })
        .chain(graph.edges.keys().map(|id| id.wrapping_add(1)))
        .max()
        .unwrap_or(0);
    let mut out = Vec::new();
    for high in HIGH_IDS.into_iter().filter(|id| !taken(*id)) {
        for moved in &added_tree_nodes {
            let mut perturbed = delta.clone();
            perturbed.add = delta
                .add
                .iter()
                .map(|(element, payload)| {
                    let rename = |id: u32| if id == *moved { high } else { id };
                    match (element, payload) {
                        (GElem::Node(id), GPayload::Node(node)) => {
                            let mut node = node.clone();
                            node.id = rename(node.id);
                            (GElem::Node(rename(*id)), GPayload::Node(node))
                        }
                        (GElem::Edge(id), GPayload::Edge(edge)) => {
                            let mut edge = edge.clone();
                            edge.src = rename(edge.src);
                            edge.tgt = rename(edge.tgt);
                            (GElem::Edge(*id), GPayload::Edge(edge))
                        }
                        (element, payload) => (element.clone(), payload.clone()),
                    }
                })
                .collect();
            out.push(perturbed);
        }
        let mut perturbed = delta.clone();
        perturbed.add.insert(GElem::Node(high), node(high, "leaf"));
        perturbed.add.insert(
            GElem::Edge(next_edge),
            GPayload::Edge(plain_edge(
                0,
                high,
                SEdge::new(&graph.nodes[&0].anchor, "leaf", "child", Some("top")),
            )),
        );
        out.push(perturbed);
    }
    out
}

/// Counts of a sweep over unbounded ids: (script, floors) pairs tried,
/// scripts lowered over unbounded ids, unbounded lowerings with an id past
/// `u32` (no graph delta, so no reader input), unbounded lowerings read
/// back, scripts whose `u32`
/// lowering ran out, readings whose `u32` lowering ran out, perturbed
/// deltas and accepted perturbed deltas.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct UnboundedCounts {
    pairs: usize,
    lowered: usize,
    beyond_u32: usize,
    read: usize,
    script_exhausts: usize,
    reading_exhausts: usize,
    perturbed: usize,
    accepted: usize,
}

impl UnboundedCounts {
    const fn plus(self, other: Self) -> Self {
        Self {
            pairs: self.pairs + other.pairs,
            lowered: self.lowered + other.lowered,
            beyond_u32: self.beyond_u32 + other.beyond_u32,
            read: self.read + other.read,
            script_exhausts: self.script_exhausts + other.script_exhausts,
            reading_exhausts: self.reading_exhausts + other.reading_exhausts,
            perturbed: self.perturbed + other.perturbed,
            accepted: self.accepted + other.accepted,
        }
    }
}

/// [`UnboundedCounts`] from its fields in declaration order.
const fn unbounded_counts(
    [
        pairs,
        lowered,
        beyond_u32,
        read,
        script_exhausts,
        reading_exhausts,
        perturbed,
        accepted,
    ]: [usize; 8],
) -> UnboundedCounts {
    UnboundedCounts {
        pairs,
        lowered,
        beyond_u32,
        read,
        script_exhausts,
        reading_exhausts,
        perturbed,
        accepted,
    }
}

/// Which deltas [`unbounded_findings`] reads: every unbounded lowering
/// with all perturbations, with the high-id perturbations or alone, or
/// only the unbounded lowerings whose `u32` lowering runs out (the others
/// are the deltas [`ceiling_findings`] reads).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Reads {
    AllPerturbed,
    HighIdPerturbed,
    Unperturbed,
    BeyondU32Runs,
}

/// Check one unbounded lowering `delta` of `script` under `embedding`: it
/// reads back (law a); the reading has the script's effect over unbounded
/// ids (law b); on at most one step it lowers to the delta and reaches the
/// carriers of the script's unbounded embedding (law c); the unbounded soundness oracle
/// holds; and the reading satisfies the refinement law. Returns whether
/// the reading's `u32` lowering ran out.
fn check_unbounded_lowering(
    tree: &Tree,
    embedding: &TreeEmbedding,
    graph: &Graph,
    script: &TreeEdit,
    delta: &GraphDelta,
    findings: &mut Vec<String>,
) -> Option<bool> {
    let at = format!(
        "{script:?} with floors ({}, {})",
        embedding.next_node, embedding.next_edge
    );
    let reading = match edit::tree_edit_from_graph_delta(tree, embedding, delta, None) {
        Ok(reading) => reading,
        Err(error) => {
            findings.push(format!("{at}: read as {error:?}"));
            return None;
        }
    };
    let mut script_embedding = edit::unbounded_embedding(embedding);
    let effect = edit::apply_tree_unbounded(script, tree, &mut script_embedding);
    let mut read_embedding = edit::unbounded_embedding(embedding);
    if edit::apply_tree_unbounded(&reading, tree, &mut read_embedding) != effect {
        findings.push(format!("{at}: {reading:?} has another effect"));
    }
    if script.steps().len() <= 1 {
        let mut relowered = edit::unbounded_embedding(embedding);
        if edit::graph_delta_from_tree_edit_unbounded(tree, &mut relowered, &reading, None)
            != Ok(Some(delta.clone()))
            || !same_carriers(&relowered, &script_embedding)
        {
            findings.push(format!("{at}: read inexactly as {reading:?}"));
        }
    }
    if !delta_is_an_unbounded_image_of_reading(tree, embedding, graph, delta, &reading) {
        findings.push(format!("{at}: not an unbounded image of {reading:?}"));
    }
    match refinement_violation(tree, embedding, &reading) {
        Ok(exhausts) => Some(exhausts),
        Err(violation) => {
            findings.push(format!("{at}: reading {violation}"));
            None
        }
    }
}

/// Lower each script over unbounded ids under each pair of floors, check
/// the refinement law on the script, and check every lowering a graph
/// delta can hold with [`check_unbounded_lowering`]; then read its
/// perturbations and check the unbounded soundness oracle on each that the
/// reader accepts.
fn unbounded_findings(
    tree: &Tree,
    scripts: &[TreeEdit],
    floors: &[(u32, u32)],
    reads: Reads,
) -> (UnboundedCounts, Vec<String>) {
    let (graph, base) = panproto_reference::encode::tree_to_graph(tree, None).unwrap();
    let mut counts = UnboundedCounts::default();
    let mut findings = Vec::new();
    for script in scripts {
        for (next_node, next_edge) in floors {
            let embedding = with_floors(&base, *next_node, *next_edge);
            counts.pairs += 1;
            let script_exhausts = match refinement_violation(tree, &embedding, script) {
                Ok(exhausts) => exhausts,
                Err(violation) => {
                    findings.push(format!("script {violation}"));
                    false
                }
            };
            counts.script_exhausts += usize::from(script_exhausts);
            let mut lowered = edit::unbounded_embedding(&embedding);
            let Ok(delta) =
                edit::graph_delta_from_tree_edit_unbounded(tree, &mut lowered, script, None)
            else {
                continue;
            };
            counts.lowered += 1;
            let Some(delta) = delta else {
                counts.beyond_u32 += 1;
                continue;
            };
            if reads == Reads::BeyondU32Runs && !script_exhausts {
                continue;
            }
            counts.read += 1;
            if let Some(exhausts) =
                check_unbounded_lowering(tree, &embedding, &graph, script, &delta, &mut findings)
            {
                counts.reading_exhausts += usize::from(exhausts);
            }
            let perturbations = match reads {
                Reads::Unperturbed | Reads::BeyondU32Runs => Vec::new(),
                Reads::HighIdPerturbed => high_id_perturbations(&graph, &delta),
                Reads::AllPerturbed => {
                    let mut all = delta_perturbations(&graph, &embedding, &delta);
                    all.extend(high_id_perturbations(&graph, &delta));
                    all
                }
            };
            for perturbed in perturbations {
                counts.perturbed += 1;
                if let Ok(reading) =
                    edit::tree_edit_from_graph_delta(tree, &embedding, &perturbed, None)
                {
                    counts.accepted += 1;
                    if !delta_is_an_unbounded_image_of_reading(
                        tree, &embedding, &graph, &perturbed, &reading,
                    ) {
                        findings.push(format!(
                            "{perturbed:?} (perturbing {script:?}) accepted as {reading:?}"
                        ));
                    }
                }
            }
        }
    }
    (counts, findings)
}

/// Run `sweep` on every set of [`high_id_script_sets`], one thread per
/// set, print and return the counts per set, and assert no finding.
fn run_unbounded_sweep(
    label: &str,
    sweep: impl Fn(&Tree, &[TreeEdit], &[TreeEdit]) -> (UnboundedCounts, Vec<String>) + Sync,
) -> Vec<UnboundedCounts> {
    let sets = high_id_script_sets();
    let results: Vec<(UnboundedCounts, Vec<String>)> = std::thread::scope(|scope| {
        let sweep = &sweep;
        let mut handles = Vec::with_capacity(sets.len());
        for (_, tree, words, high) in &sets {
            handles.push(scope.spawn(move || sweep(tree, words, high)));
        }
        handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect()
    });
    let mut findings = Vec::new();
    let mut counts = Vec::new();
    for ((name, _, _, _), (set_counts, set_findings)) in sets.iter().zip(results) {
        println!(
            "{label} [{name}]: {set_counts:?}, {} findings",
            set_findings.len()
        );
        if let Some(first) = set_findings.first() {
            println!("  first finding: {first}");
        }
        findings.extend(set_findings);
        counts.push(set_counts);
    }
    let total = counts
        .iter()
        .fold(UnboundedCounts::default(), |sum, each| sum.plus(*each));
    println!("{label} total: {total:?}, {} findings", findings.len());
    assert_eq!(
        findings.len(),
        0,
        "first findings: {:?}",
        &findings[..findings.len().min(5)]
    );
    counts
}

// Soundness, totality, effect and exactness over unbounded ids, with tree
// node ids at the top of `u32` (the gap that hid the two readings that
// exhaust only in `u32`): every short script with a high letter is
// lowered over unbounded ids under the encoder's floors and read back with
// all perturbations, every short script without one with the high-id
// perturbations; the refinement law is checked on every script and
// reading.
#[test]
fn unbounded_readback_laws_hold_with_node_ids_at_the_top_of_u32() {
    let counts = run_unbounded_sweep("high ids", |tree, words, high| {
        let (_, base) = panproto_reference::encode::tree_to_graph(tree, None).unwrap();
        let floors = [(base.next_node, base.next_edge)];
        let (word_counts, mut findings) =
            unbounded_findings(tree, words, &floors, Reads::HighIdPerturbed);
        let (high_counts, high_findings) =
            unbounded_findings(tree, high, &floors, Reads::AllPerturbed);
        findings.extend(high_findings);
        (word_counts.plus(high_counts), findings)
    });
    assert_eq!(
        counts,
        [
            unbounded_counts([702, 390, 5, 385, 53, 40, 6_695, 2_363]),
            unbounded_counts([756, 430, 6, 424, 57, 45, 7_109, 2_582]),
            unbounded_counts([756, 430, 6, 424, 57, 45, 7_109, 2_566]),
            unbounded_counts([756, 432, 6, 426, 57, 45, 7_217, 2_556]),
            unbounded_counts([756, 432, 6, 426, 57, 45, 7_217, 2_564]),
            unbounded_counts([756, 433, 6, 427, 57, 45, 7_188, 2_591]),
            unbounded_counts([756, 433, 6, 427, 57, 45, 7_188, 2_583]),
            unbounded_counts([812, 469, 6, 463, 59, 47, 7_552, 2_829]),
            unbounded_counts([812, 469, 6, 463, 59, 47, 7_552, 2_829]),
            unbounded_counts([812, 469, 6, 463, 59, 47, 7_520, 2_837]),
            unbounded_counts([812, 469, 6, 463, 59, 47, 7_520, 2_837]),
            unbounded_counts([812, 471, 6, 465, 59, 47, 7_628, 2_827]),
            unbounded_counts([812, 471, 6, 465, 59, 47, 7_628, 2_827]),
            unbounded_counts([812, 471, 6, 465, 59, 47, 7_596, 2_835]),
            unbounded_counts([812, 471, 6, 465, 59, 47, 7_596, 2_835]),
            unbounded_counts([930, 567, 11, 556, 68, 53, 10_116, 3_316]),
            unbounded_counts([930, 567, 11, 556, 68, 53, 10_116, 3_370]),
            unbounded_counts([930, 567, 11, 556, 68, 53, 10_116, 3_316]),
            unbounded_counts([930, 567, 11, 556, 68, 53, 10_116, 3_354]),
            unbounded_counts([930, 567, 11, 556, 68, 53, 10_116, 3_390]),
            unbounded_counts([930, 567, 11, 556, 68, 53, 10_116, 3_374]),
            unbounded_counts([1_056, 667, 16, 651, 77, 59, 12_902, 3_868]),
            unbounded_counts([6_806, 1_870, 4, 1_866, 134, 93, 17_028, 9_685]),
            unbounded_counts([7_140, 2_041, 11, 2_030, 146, 101, 20_583, 10_129]),
            unbounded_counts([7_140, 2_047, 11, 2_036, 146, 101, 20_489, 9_943]),
        ]
    );
    assert_eq!(
        counts
            .iter()
            .fold(UnboundedCounts::default(), |sum, each| sum.plus(*each)),
        unbounded_counts([39_456, 16_767, 197, 16_570, 1_778, 1_358, 242_013, 94_206])
    );
}

// The refinement law and the laws over unbounded ids at every floor near
// the ceiling: every short script, with or without a high letter, is
// lowered over unbounded ids with each floor from the ceiling down to the
// sweep room of its set, and its `u32` application and lowering must equal
// the unbounded ones or refuse with `IdsExhausted`. Every unbounded
// lowering a graph delta can hold that a `u32` run cannot produce, and
// every lowering of a script with a high letter, must read back soundly with the script's effect,
// exactly on single steps, and its reading must satisfy the refinement
// law too. The other lowerings are `u32` lowerings, read by the ceiling
// sweep above.
#[test]
fn the_u32_tree_edit_semantics_refines_the_unbounded_one_near_the_id_ceiling() {
    let counts = run_unbounded_sweep("refinement", |tree, words, high| {
        let (_, base) = panproto_reference::encode::tree_to_graph(tree, None).unwrap();
        let mut sweep = CeilingSweep {
            room: (0, 0),
            counts: CeilingCounts::default(),
            findings: Vec::new(),
        };
        base_lowerings(tree, &base, words, &mut sweep);
        let (node_room, edge_room) = sweep.room;
        let floors: Vec<(u32, u32)> = (0..=node_room)
            .flat_map(|node| (0..=edge_room).map(move |edge| (u32::MAX - node, u32::MAX - edge)))
            .collect();
        let (word_counts, mut findings) =
            unbounded_findings(tree, words, &floors, Reads::BeyondU32Runs);
        let (high_counts, high_findings) =
            unbounded_findings(tree, high, &floors, Reads::Unperturbed);
        findings.extend(high_findings);
        (word_counts.plus(high_counts), findings)
    });
    assert_eq!(
        counts,
        [
            unbounded_counts([42_120, 23_400, 2_927, 9_242, 8_862, 4_342, 0, 0]),
            unbounded_counts([45_360, 25_800, 3_471, 10_003, 10_183, 4_955, 0, 0]),
            unbounded_counts([45_360, 25_800, 3_471, 10_003, 10_183, 4_955, 0, 0]),
            unbounded_counts([45_360, 25_920, 3_515, 10_030, 10_214, 4_987, 0, 0]),
            unbounded_counts([45_360, 25_920, 3_511, 10_034, 10_214, 4_999, 0, 0]),
            unbounded_counts([45_360, 25_980, 3_497, 10_033, 10_199, 4_987, 0, 0]),
            unbounded_counts([45_360, 25_980, 3_493, 10_037, 10_199, 4_999, 0, 0]),
            unbounded_counts([48_720, 28_140, 3_577, 10_653, 10_629, 5_260, 0, 0]),
            unbounded_counts([48_720, 28_140, 3_577, 10_653, 10_629, 5_260, 0, 0]),
            unbounded_counts([48_720, 28_140, 3_559, 10_656, 10_614, 5_260, 0, 0]),
            unbounded_counts([48_720, 28_140, 3_559, 10_656, 10_614, 5_260, 0, 0]),
            unbounded_counts([48_720, 28_260, 3_603, 10_683, 10_645, 5_292, 0, 0]),
            unbounded_counts([48_720, 28_260, 3_599, 10_687, 10_645, 5_304, 0, 0]),
            unbounded_counts([48_720, 28_260, 3_585, 10_686, 10_630, 5_292, 0, 0]),
            unbounded_counts([48_720, 28_260, 3_581, 10_690, 10_630, 5_304, 0, 0]),
            unbounded_counts([104_160, 63_504, 9_455, 21_452, 23_312, 10_476, 0, 0]),
            unbounded_counts([104_160, 63_504, 9_433, 21_474, 23_312, 10_489, 0, 0]),
            unbounded_counts([104_160, 63_504, 9_465, 21_442, 23_312, 10_458, 0, 0]),
            unbounded_counts([104_160, 63_504, 9_429, 21_478, 23_312, 10_505, 0, 0]),
            unbounded_counts([104_160, 63_504, 9_433, 21_474, 23_312, 10_469, 0, 0]),
            unbounded_counts([104_160, 63_504, 9_419, 21_488, 23_312, 10_503, 0, 0]),
            unbounded_counts([190_080, 120_060, 20_172, 36_731, 43_795, 17_841, 0, 0]),
            unbounded_counts([272_240, 74_800, 8_913, 20_258, 32_403, 12_137, 0, 0]),
            unbounded_counts([428_400, 122_460, 15_225, 30_590, 48_469, 17_307, 0, 0]),
            unbounded_counts([428_400, 122_820, 15_169, 30_590, 48_393, 17_222, 0, 0]),
        ]
    );
    assert_eq!(
        counts
            .iter()
            .fold(UnboundedCounts::default(), |sum, each| sum.plus(*each)),
        unbounded_counts([
            2_648_120, 1_225_564, 168_638, 401_723, 468_022, 203_863, 0, 0
        ])
    );
}

/// `delta` with every edge id at or above `embedding.next_edge` raised by
/// `edges` and every node id at or above `embedding.next_node` raised by
/// `nodes`, in element ids and in the endpoints of added edges.
fn shift_allocated_ids(
    delta: &GraphDelta,
    embedding: &TreeEmbedding,
    nodes: u32,
    edges: u32,
) -> GraphDelta {
    let node = |id: u32| {
        if id >= embedding.next_node {
            id + nodes
        } else {
            id
        }
    };
    let edge = |id: u32| {
        if id >= embedding.next_edge {
            id + edges
        } else {
            id
        }
    };
    let mut shifted = GraphDelta {
        add: BTreeMap::new(),
        ..delta.clone()
    };
    for (element, payload) in &delta.add {
        match (element, payload.clone()) {
            (GElem::Edge(id), GPayload::Edge(mut added)) => {
                added.src = node(added.src);
                added.tgt = node(added.tgt);
                shifted
                    .add
                    .insert(GElem::Edge(edge(*id)), GPayload::Edge(added));
            }
            (GElem::Node(id), GPayload::Node(mut added)) => {
                added.id = node(*id);
                shifted
                    .add
                    .insert(GElem::Node(added.id), GPayload::Node(added));
            }
            (element, payload) => panic!("cross-sort addition {element:?}: {payload:?}"),
        }
    }
    shifted
}

/// A tree-edit reader, as the room-shift check calls it.
type Reader = dyn Fn(&Tree, &TreeEmbedding, &GraphDelta) -> Result<TreeEdit, edit::TreeApplyError>;

/// The room-shift check of (R2) with the reader `read`: for every
/// lowering over unbounded ids of every word and high-id word of `sets`,
/// from the encoder embedding, raise the edge floor and every edge id the
/// delta allocates, and the node floor and every fan node id it
/// allocates when the result has no tree node at or above the node floor,
/// so that the delta's largest allocated ids sit at `u32::MAX` minus 0, 1,
/// 2 and 5; the reading must be the one at the encoder floors, and never
/// `IdsExhausted`. Returns the counts (lowerings, shifted deltas, those
/// with the node floor raised, readings, readings whose application under
/// the raised floors runs out of `u32` ids) and the findings.
fn room_shift_findings(
    sets: Vec<(String, Tree, Vec<TreeEdit>, Vec<TreeEdit>)>,
    read: &Reader,
) -> ([usize; 5], Vec<String>) {
    let mut counts = [0_usize; 5];
    let mut findings = Vec::new();
    for (name, tree, words, high) in sets {
        let (graph, base) = panproto_reference::encode::tree_to_graph(&tree, None).unwrap();
        for script in words.iter().chain(&high) {
            let mut lowered = edit::unbounded_embedding(&base);
            let Ok(Some(delta)) =
                edit::graph_delta_from_tree_edit_unbounded(&tree, &mut lowered, script, None)
            else {
                continue;
            };
            counts[0] += 1;
            let reading = read(&tree, &base, &delta);
            let allocated = |floor: u32, nodes: bool| {
                delta
                    .add
                    .iter()
                    .filter_map(|(element, payload)| match (element, payload) {
                        (GElem::Edge(id), _) if !nodes => Some(*id),
                        (GElem::Node(id), GPayload::Node(added))
                            if nodes && added.shape == NodeShape::HyperEdge =>
                        {
                            Some(*id)
                        }
                        _ => None,
                    })
                    .fold(floor, u32::max)
            };
            let tree_nodes_below_floor = edit::apply(&delta, &graph)
                .unwrap()
                .nodes
                .iter()
                .all(|(id, added)| added.shape == NodeShape::HyperEdge || *id < base.next_node);
            for room in [0, 1, 2, 5] {
                let edges = u32::MAX - room - allocated(base.next_edge, false);
                let nodes = if tree_nodes_below_floor {
                    u32::MAX - room - allocated(base.next_node, true)
                } else {
                    0
                };
                let shifted = shift_allocated_ids(&delta, &base, nodes, edges);
                let raised = with_floors(&base, base.next_node + nodes, base.next_edge + edges);
                let shifted_reading = read(&tree, &raised, &shifted);
                counts[1] += 1;
                counts[2] += usize::from(nodes > 0);
                counts[3] += usize::from(shifted_reading.is_ok());
                counts[4] += usize::from(shifted_reading.as_ref().is_ok_and(|shifted| {
                    edit::apply_tree(shifted, &tree, &mut raised.clone())
                        == Err(edit::TreeApplyError::IdsExhausted)
                }));
                if shifted_reading != reading
                    || shifted_reading == Err(edit::TreeApplyError::IdsExhausted)
                {
                    findings.push(format!(
                        "{name}: {script:?} with room {room}: {reading:?} became {shifted_reading:?}"
                    ));
                }
            }
        }
    }
    (counts, findings)
}

/// (R2) The reader returns what the reader over unbounded ids returns, so
/// no reading depends on how much `u32` room is left. Edge ids are only
/// drawn, never named by an edit, so raising the edge floor and every edge
/// id the delta allocates by one amount reads the same; so does raising
/// the node floor and the allocated fan node ids, when the result has no
/// tree node at or above the node floor (a tree node id is named by its
/// insertion). [`room_shift_findings`] checks this on the sets of
/// [`high_id_script_sets`]. A reader that took the exact single step only
/// where its `u32` run fits would fail here.
#[test]
fn tree_edit_readings_do_not_depend_on_the_room_left_below_the_id_ceiling() {
    let (counts, findings) =
        room_shift_findings(high_id_script_sets(), &|tree, embedding, delta| {
            edit::tree_edit_from_graph_delta(tree, embedding, delta, None)
        });
    println!(
        "lowered {}, shifted {}, with the node floor raised {}, read {}, readings exhausting in u32 at the raised floors {}",
        counts[0], counts[1], counts[2], counts[3], counts[4]
    );
    assert!(
        findings.is_empty(),
        "{:#?}",
        &findings[..findings.len().min(10)]
    );
    assert_eq!(counts, [16_570, 66_280, 41_132, 66_280, 21_207]);
}

/// The room-shift check fails a reader whose readings depend on the room
/// left: one that returns its single readings wrapped in a one-step
/// sequence (the same edit in effect, another reading) once the edge floor
/// is within two ids of `u32::MAX`.
#[test]
fn the_room_shift_check_fails_a_reader_whose_readings_depend_on_the_room_left() {
    let sets = high_id_script_sets().into_iter().take(1).collect();
    let (_, findings) = room_shift_findings(sets, &|tree, embedding, delta| {
        let reading = edit::tree_edit_from_graph_delta(tree, embedding, delta, None)?;
        Ok(if embedding.next_edge > u32::MAX - 3 {
            TreeEdit::Sequence(vec![reading])
        } else {
            reading
        })
    });
    assert!(!findings.is_empty());
}

/// The ceiling sweep's exactness check fails a reading that is exact only
/// in effect where its `u32` lowering runs out short of full room. With
/// the edge floor at `u32::MAX - 2`, `JoinFeatures {1, [2], "joined"}` on
/// [`join_tree`] lowers in `u32`, re-drawing arcs 3 and 4 at
/// `u32::MAX - 2` and `u32::MAX - 1`. Its contraction-and-moves form has
/// the same effect, but its moves draw past `u32` (over unbounded ids its
/// lowering has no graph form, and in `u32` it runs out), so only the
/// exactness over unbounded ids refuses it; the join itself passes.
#[test]
fn the_ceiling_exactness_check_fails_an_inexact_reading_whose_u32_lowering_runs_out() {
    let tree = join_tree();
    let ceiling = TreeEmbedding {
        next_edge: u32::MAX - 2,
        ..literal_join_embedding()
    };
    let join = TreeEdit::JoinFeatures {
        primary: 1,
        joined: vec![2],
        produce: Node::new(1, "joined"),
    };
    let moved = |child: u32, name: &str| TreeEdit::MoveSubtree {
        node_id: child,
        new_parent: 1,
        edge: SEdge::new("joined", "leaf", "child", Some(name)),
    };
    let contracted = TreeEdit::Sequence(vec![
        TreeEdit::ContractNode { id: 2 },
        moved(3, "c"),
        moved(4, "d"),
        TreeEdit::RelabelNode {
            id: 1,
            new_anchor: "joined".into(),
        },
    ]);
    let mut lowered = ceiling.clone();
    let delta = edit::graph_delta_from_tree_edit(&tree, &mut lowered, &join, None).unwrap();
    let start = edit::unbounded_embedding(&ceiling);
    assert_eq!(
        edit::apply_tree_unbounded(&contracted, &tree, &mut start.clone()),
        edit::apply_tree_unbounded(&join, &tree, &mut start.clone())
    );
    let mut relowered = start;
    assert_eq!(
        edit::graph_delta_from_tree_edit_unbounded(&tree, &mut relowered, &contracted, None),
        Ok(None)
    );
    assert_eq!(
        edit::graph_delta_from_tree_edit(&tree, &mut ceiling.clone(), &contracted, None),
        Err(edit::TreeApplyError::IdsExhausted)
    );
    for full_room in [false, true] {
        assert!(reads_exactly_at_the_ceiling(
            &tree, &ceiling, &delta, &lowered, &join, full_room
        ));
        assert!(!reads_exactly_at_the_ceiling(
            &tree,
            &ceiling,
            &delta,
            &lowered,
            &contracted,
            full_room
        ));
    }
}

fn arm_edges(edge: u32) -> BTreeMap<String, u32> {
    BTreeMap::from([("arm".into(), edge)])
}

/// [`join_tree`] with a second fan of another name under node 1, and its
/// literal embedding.
fn two_fan_tree_and_embedding() -> (Tree, TreeEmbedding) {
    let mut tree = join_tree();
    tree.fans.push(Fan {
        parent: 1,
        hyper_edge: "second-fan".into(),
        children: BTreeMap::from([("arm".into(), 4)]),
    });
    let embedding = TreeEmbedding {
        arcs: BTreeMap::from([(1, 0), (2, 1), (3, 2), (4, 3)]),
        fans: vec![(5, 4, arm_edges(5)), (6, 6, arm_edges(7))],
        next_node: 7,
        next_edge: 8,
    };
    (tree, embedding)
}

fn assert_valid_tree_embedding(tree: &Tree, mut embedding: TreeEmbedding) {
    let unchanged = embedding.clone();
    assert_eq!(
        edit::apply_tree(&TreeEdit::Identity, tree, &mut embedding),
        Ok(tree.clone())
    );
    assert_eq!(embedding, unchanged);
}

fn join_embedding_with_arcs(arcs: &[(u32, u32)]) -> TreeEmbedding {
    TreeEmbedding {
        arcs: arcs.iter().copied().collect(),
        ..literal_join_embedding()
    }
}

fn join_embedding_with_fans(
    fans: Vec<(u32, u32, BTreeMap<String, u32>)>,
    next_node: u32,
    next_edge: u32,
) -> TreeEmbedding {
    TreeEmbedding {
        fans,
        next_node,
        next_edge,
        ..literal_join_embedding()
    }
}

#[test]
fn tr14_identity_refuses_every_literal_embedding_corruption_without_mutation() {
    let tree = join_tree();
    let valid = literal_join_embedding();
    assert_valid_tree_embedding(&tree, valid.clone());
    assert_valid_tree_embedding(
        &tree,
        TreeEmbedding {
            next_node: 7,
            next_edge: 9,
            ..valid
        },
    );
    let corruptions = [
        (
            "missing arcs key",
            join_embedding_with_arcs(&[(1, 0), (2, 1), (3, 2)]),
        ),
        (
            "extra arcs key",
            TreeEmbedding {
                arcs: BTreeMap::from([(0, 6), (1, 0), (2, 1), (3, 2), (4, 3)]),
                next_edge: 7,
                ..literal_join_embedding()
            },
        ),
        (
            "swapped ordinary-edge ids",
            join_embedding_with_arcs(&[(1, 1), (2, 0), (3, 2), (4, 3)]),
        ),
        (
            "duplicate ordinary-edge ids",
            join_embedding_with_arcs(&[(1, 0), (2, 0), (3, 2), (4, 3)]),
        ),
        (
            "fan count below the tree's",
            join_embedding_with_fans(Vec::new(), 6, 6),
        ),
        (
            "fan count above the tree's",
            join_embedding_with_fans(vec![(5, 4, arm_edges(5)), (6, 6, arm_edges(7))], 7, 8),
        ),
        (
            "fan node colliding with a tree node",
            join_embedding_with_fans(vec![(4, 4, arm_edges(5))], 6, 6),
        ),
        (
            "missing fan child-label key",
            join_embedding_with_fans(vec![(5, 4, BTreeMap::new())], 6, 6),
        ),
        (
            "extra fan child-label key",
            join_embedding_with_fans(
                vec![(
                    5,
                    4,
                    BTreeMap::from([("arm".into(), 5), ("extra".into(), 6)]),
                )],
                6,
                7,
            ),
        ),
        (
            "fan parent edge colliding with an ordinary edge",
            join_embedding_with_fans(vec![(5, 0, arm_edges(5))], 6, 6),
        ),
        (
            "fan child edge colliding with an ordinary edge",
            join_embedding_with_fans(vec![(5, 4, arm_edges(0))], 6, 6),
        ),
        (
            "fan child edge colliding with its fan's parent edge",
            join_embedding_with_fans(vec![(5, 4, arm_edges(4))], 6, 6),
        ),
        (
            "next_node equal to the used fan node",
            join_embedding_with_fans(vec![(5, 4, arm_edges(5))], 5, 6),
        ),
        (
            "next_node equal to a used tree node",
            join_embedding_with_fans(vec![(5, 4, arm_edges(5))], 4, 6),
        ),
        (
            "next_edge equal to the used fan edge",
            join_embedding_with_fans(vec![(5, 4, arm_edges(5))], 6, 5),
        ),
        (
            "next_edge equal to a used ordinary edge",
            join_embedding_with_fans(vec![(5, 4, arm_edges(5))], 6, 3),
        ),
    ];
    for (name, corrupted) in corruptions {
        let mut actual = corrupted.clone();
        assert_eq!(
            edit::apply_tree(&TreeEdit::Identity, &tree, &mut actual),
            Err(edit::TreeApplyError::InvalidEmbedding),
            "{name}"
        );
        assert_eq!(actual, corrupted, "{name}");
    }
}

#[test]
fn tr14_identity_refuses_every_two_fan_embedding_corruption_without_mutation() {
    let (two_fans, two_fan_embedding) = two_fan_tree_and_embedding();
    assert_valid_tree_embedding(&two_fans, two_fan_embedding.clone());
    let two_fan_with = |fans: Vec<(u32, u32, BTreeMap<String, u32>)>| TreeEmbedding {
        fans,
        ..two_fan_embedding.clone()
    };
    let two_fan_corruptions = [
        (
            "reversed fan entries",
            two_fan_with(vec![(6, 6, arm_edges(7)), (5, 4, arm_edges(5))]),
        ),
        (
            "duplicate fan nodes",
            two_fan_with(vec![(5, 4, arm_edges(5)), (5, 6, arm_edges(7))]),
        ),
        (
            "fan parent edge colliding with another fan's edge",
            two_fan_with(vec![(5, 4, arm_edges(5)), (6, 5, arm_edges(7))]),
        ),
        (
            "fan child edge colliding with another fan's child edge",
            two_fan_with(vec![(5, 4, arm_edges(5)), (6, 6, arm_edges(5))]),
        ),
        (
            "fan child edge colliding with another fan's parent edge",
            two_fan_with(vec![(5, 4, arm_edges(5)), (6, 6, arm_edges(4))]),
        ),
    ];
    for (name, corrupted) in two_fan_corruptions {
        let mut actual = corrupted.clone();
        assert_eq!(
            edit::apply_tree(&TreeEdit::Identity, &two_fans, &mut actual),
            Err(edit::TreeApplyError::InvalidEmbedding),
            "{name}"
        );
        assert_eq!(actual, corrupted, "{name}");
    }
}

#[test]
fn tree_reader_refuses_deleting_either_duplicate_named_fan_alone() {
    let tree = duplicate_fan_tree();
    let (graph, embedding) = literal_duplicate_fan_graph_and_embedding();
    let first = [GElem::Node(5), GElem::Edge(4), GElem::Edge(5)];
    let second = [GElem::Node(6), GElem::Edge(6), GElem::Edge(7)];
    for alone in [first.clone(), second.clone()] {
        assert!(not_tree_edit(
            &edit::tree_edit_from_graph_delta(
                &tree,
                &embedding,
                &literal_deletions(&graph, alone),
                None
            ),
            FIXED_CARRIERS
        ));
    }
    let both = literal_deletions(&graph, first.into_iter().chain(second));
    assert_eq!(
        edit::tree_edit_from_graph_delta(&tree, &embedding, &both, None),
        Ok(TreeEdit::DeleteFan {
            hyper_edge: "fan".into(),
        })
    );
}

const FIXED_CARRIERS: &str = "the delta's fixed carriers are outside the tree-edit image";

/// A fan node kept while every edge of it is re-drawn to another member:
/// the result has one fan, `fan@2` with member 4, which the reading
/// deletes by name and inserts afresh, so its lowering deletes fan node 5.
/// The delta keeps node 5, which no lowering does (a lowering draws a new
/// fan node for a new fan), and the deletion clause of the fixed-part
/// comparison refuses it although every edge the lowering deletes is
/// deleted.
#[test]
fn tree_reader_refuses_a_fan_node_kept_under_edges_redrawn_to_another_member() {
    let tree = join_tree();
    let embedding = literal_join_embedding();
    let graph = literal_join_graph();
    let mut delta = literal_deletions(&graph, [GElem::Edge(4), GElem::Edge(5)]);
    delta.add.insert(
        GElem::Edge(6),
        GPayload::Edge(plain_edge(5, 2, fan_parent_label())),
    );
    delta.add.insert(
        GElem::Edge(7),
        GPayload::Edge(plain_edge(5, 4, fan_arm_label())),
    );
    let edited = edit::apply(&delta, &graph).unwrap();
    let mut moved_member = join_tree();
    moved_member.fans[0].children.insert("arm".into(), 4);
    assert_eq!(
        panproto_reference::encode::graph_to_tree(&edited, 0, &|_| { ParentNameRule::Exact(None) }),
        Ok(moved_member.clone())
    );
    let reading = TreeEdit::Sequence(vec![
        TreeEdit::DeleteFan {
            hyper_edge: "fan".into(),
        },
        TreeEdit::InsertFan {
            fan: moved_member.fans[0].clone(),
        },
    ]);
    let mut lowered = literal_deletions(&graph, [GElem::Node(5), GElem::Edge(4), GElem::Edge(5)]);
    lowered
        .add
        .insert(GElem::Node(6), GPayload::Node(fan_node(6, "fan")));
    lowered.add.insert(
        GElem::Edge(6),
        GPayload::Edge(plain_edge(6, 2, fan_parent_label())),
    );
    lowered.add.insert(
        GElem::Edge(7),
        GPayload::Edge(plain_edge(6, 4, fan_arm_label())),
    );
    assert_eq!(
        edit::graph_delta_from_tree_edit(&tree, &mut embedding.clone(), &reading, None),
        Ok(lowered.clone())
    );
    assert_eq!(
        edit::tree_edit_from_graph_delta(&tree, &embedding, &lowered, None),
        Ok(reading)
    );
    assert_eq!(
        edit::tree_edit_from_graph_delta(&tree, &embedding, &delta, None),
        Err(edit::TreeApplyError::NotTreeEdit {
            reason: FIXED_CARRIERS.to_owned(),
        })
    );
}

/// No lowering re-draws part of a fan: a fan is re-drawn only by
/// `InsertNode` over a fan id, which re-draws the fan node and every edge
/// of each fan in the suffix. Re-drawing the arm edge, the parent edge, or
/// both edges while keeping fan node 5 leaves the same tree, and each is
/// refused.
#[test]
fn tree_reader_refuses_redrawing_part_of_a_fan() {
    let tree = join_tree();
    let embedding = literal_join_embedding();
    let graph = literal_join_graph();
    assert_eq!(
        panproto_reference::encode::tree_to_graph(&tree, None),
        Ok((graph.clone(), embedding.clone()))
    );
    let parent = GPayload::Edge(plain_edge(5, 2, fan_parent_label()));
    let arm = GPayload::Edge(plain_edge(5, 3, fan_arm_label()));
    let redraws = [
        (
            vec![(GElem::Edge(5), arm.clone())],
            vec![(GElem::Edge(6), arm.clone())],
        ),
        (
            vec![(GElem::Edge(4), parent.clone())],
            vec![(GElem::Edge(6), parent.clone())],
        ),
        (
            vec![
                (GElem::Edge(4), parent.clone()),
                (GElem::Edge(5), arm.clone()),
            ],
            vec![(GElem::Edge(6), parent), (GElem::Edge(7), arm)],
        ),
    ];
    let mut readings = Vec::new();
    for (deleted, added) in redraws {
        let delta = GraphDelta {
            del: deleted.into_iter().collect(),
            add: added.into_iter().collect(),
            attrs: BTreeMap::new(),
        };
        let edited = edit::apply(&delta, &graph).unwrap();
        assert_eq!(
            panproto_reference::encode::graph_to_tree(&edited, 0, &|_| {
                ParentNameRule::Exact(None)
            }),
            Ok(join_tree())
        );
        readings.push(edit::tree_edit_from_graph_delta(
            &tree, &embedding, &delta, None,
        ));
    }
    let refusal = Err(edit::TreeApplyError::NotTreeEdit {
        reason: FIXED_CARRIERS.to_owned(),
    });
    assert_eq!(readings, [refusal.clone(), refusal.clone(), refusal]);
}

/// The first fan of [`duplicate_fan_tree`] is removed through its member 3
/// while the arc of its parent 2 is re-drawn. Node 2 also keeps child 4
/// under its original arc, so removing the fan by contracting 2 would re-draw
/// arc 3, which the delta keeps; the reader removes it through the member
/// instead.
#[test]
fn a_fan_whose_parent_keeps_an_original_child_arc_is_removed_through_its_member() {
    let tree = duplicate_fan_tree();
    let (graph, embedding) = literal_duplicate_fan_graph_and_embedding();
    let b_arc = SEdge::new("root", "feature", "child", Some("b"));
    let c_arc = SEdge::new("feature", "leaf", "child", Some("c"));
    let reinsert = TreeEdit::InsertNode {
        parent: 2,
        child_id: 3,
        node: Node::new(3, "leaf"),
        edge: c_arc.clone(),
    };
    let refresh = TreeEdit::MoveSubtree {
        node_id: 2,
        new_parent: 0,
        edge: b_arc.clone(),
    };
    let script = TreeEdit::Sequence(vec![
        refresh.clone(),
        TreeEdit::DeleteNode { id: 3 },
        reinsert.clone(),
    ]);
    let mut expected = literal_deletions(
        &graph,
        [
            GElem::Node(5),
            GElem::Edge(1),
            GElem::Edge(2),
            GElem::Edge(4),
            GElem::Edge(5),
        ],
    );
    expected.add.insert(
        GElem::Edge(8),
        GPayload::Edge(plain_edge(0, 2, b_arc.clone())),
    );
    expected.add.insert(
        GElem::Edge(9),
        GPayload::Edge(plain_edge(2, 3, c_arc.clone())),
    );
    let expected = replacing(expected, &graph, 3, Node::new(3, "leaf"));
    let mut lowered = embedding.clone();
    assert_eq!(
        edit::graph_delta_from_tree_edit(&tree, &mut lowered, &script, None),
        Ok(expected.clone())
    );
    let reading = TreeEdit::Sequence(vec![
        TreeEdit::DeleteNode { id: 3 },
        reinsert,
        refresh,
        TreeEdit::MoveSubtree {
            node_id: 3,
            new_parent: 2,
            edge: c_arc.clone(),
        },
    ]);
    assert_eq!(
        edit::tree_edit_from_graph_delta(&tree, &embedding, &expected, None),
        Ok(reading.clone())
    );
    let result = Tree {
        arcs: vec![
            (0, 1, SEdge::new("root", "feature", "child", Some("a"))),
            (2, 4, SEdge::new("feature", "leaf", "child", Some("d"))),
            (0, 2, b_arc),
            (2, 3, c_arc),
        ],
        fans: vec![tree.fans[1].clone()],
        ..join_tree()
    };
    let mut read_embedding = embedding;
    assert_eq!(
        edit::apply_tree(&reading, &tree, &mut read_embedding),
        Ok(result)
    );
}

/// The script of the test above, lowered with only two edge ids left above
/// the floor. The script draws two edge ids; its reading re-inserts 3 before
/// re-drawing the arc of 2 and so draws three. The reading's own ids are
/// immaterial (the delta is its image up to renaming fresh ids), so the
/// lowering reads back; applying the reading under this embedding is what
/// runs out of ids.
#[test]
fn a_lowering_reads_back_when_its_reading_needs_more_edge_ids_than_remain() {
    let tree = duplicate_fan_tree();
    let (graph, base) = literal_duplicate_fan_graph_and_embedding();
    let embedding = TreeEmbedding {
        next_edge: u32::MAX - 2,
        ..base.clone()
    };
    let b_arc = SEdge::new("root", "feature", "child", Some("b"));
    let c_arc = SEdge::new("feature", "leaf", "child", Some("c"));
    let reinsert = TreeEdit::InsertNode {
        parent: 2,
        child_id: 3,
        node: Node::new(3, "leaf"),
        edge: c_arc.clone(),
    };
    let refresh = TreeEdit::MoveSubtree {
        node_id: 2,
        new_parent: 0,
        edge: b_arc.clone(),
    };
    let script = TreeEdit::Sequence(vec![
        refresh.clone(),
        TreeEdit::DeleteNode { id: 3 },
        reinsert.clone(),
    ]);
    let mut expected = literal_deletions(
        &graph,
        [
            GElem::Node(5),
            GElem::Edge(1),
            GElem::Edge(2),
            GElem::Edge(4),
            GElem::Edge(5),
        ],
    );
    expected.add.insert(
        GElem::Edge(u32::MAX - 2),
        GPayload::Edge(plain_edge(0, 2, b_arc)),
    );
    expected.add.insert(
        GElem::Edge(u32::MAX - 1),
        GPayload::Edge(plain_edge(2, 3, c_arc.clone())),
    );
    let expected = replacing(expected, &graph, 3, Node::new(3, "leaf"));
    let mut lowered = embedding.clone();
    assert_eq!(
        edit::graph_delta_from_tree_edit(&tree, &mut lowered, &script, None),
        Ok(expected.clone())
    );
    let reading = TreeEdit::Sequence(vec![
        TreeEdit::DeleteNode { id: 3 },
        reinsert,
        refresh,
        TreeEdit::MoveSubtree {
            node_id: 3,
            new_parent: 2,
            edge: c_arc,
        },
    ]);
    assert_eq!(
        edit::tree_edit_from_graph_delta(&tree, &embedding, &expected, None),
        Ok(reading.clone())
    );
    let mut exhausted = embedding;
    assert_eq!(
        edit::apply_tree(&reading, &tree, &mut exhausted),
        Err(edit::TreeApplyError::IdsExhausted)
    );
    let mut roomy = base.clone();
    let mut scripted = base;
    assert_eq!(
        edit::apply_tree(&reading, &tree, &mut roomy),
        edit::apply_tree(&script, &tree, &mut scripted)
    );
}

/// The same on the node floor: the script inserts 5 under 2, which
/// relocates both fans of [`duplicate_fan_tree`], then 6 under the root,
/// drawing two fan node ids. Its reading inserts by depth, 6 first, so
/// it relocates the second fan and then both, drawing three.
#[test]
fn a_lowering_reads_back_when_its_reading_needs_more_fan_node_ids_than_remain() {
    let tree = duplicate_fan_tree();
    let (_, base) = literal_duplicate_fan_graph_and_embedding();
    let embedding = TreeEmbedding {
        next_node: u32::MAX - 2,
        ..base.clone()
    };
    let script = TreeEdit::Sequence(vec![insert_leaf(2, 5), insert_leaf(0, 6)]);
    let mut lowered = embedding.clone();
    let delta = edit::graph_delta_from_tree_edit(&tree, &mut lowered, &script, None).unwrap();
    let mut base_lowered = base.clone();
    let base_delta =
        edit::graph_delta_from_tree_edit(&tree, &mut base_lowered, &script, None).unwrap();
    assert_eq!((base_lowered.next_node, lowered.next_node), (9, u32::MAX));
    assert_eq!(
        delta,
        rename_fresh_ids(&base_delta, &base, |id| id - 7 + (u32::MAX - 2), |id| id)
    );
    let reading = TreeEdit::Sequence(vec![
        insert_leaf(0, 6),
        insert_leaf(2, 5),
        TreeEdit::MoveSubtree {
            node_id: 6,
            new_parent: 0,
            edge: inserted_label(),
        },
    ]);
    assert_eq!(
        edit::tree_edit_from_graph_delta(&tree, &embedding, &delta, None),
        Ok(reading.clone())
    );
    let mut exhausted = embedding;
    assert_eq!(
        edit::apply_tree(&reading, &tree, &mut exhausted),
        Err(edit::TreeApplyError::IdsExhausted)
    );
    let mut roomy = base.clone();
    let mut scripted = base;
    assert_eq!(
        edit::apply_tree(&reading, &tree, &mut roomy),
        edit::apply_tree(&script, &tree, &mut scripted)
    );
}

/// [`join_tree`] without its fan, with its literal graph and embedding.
fn fanless_join_tree_graph_and_embedding() -> (Tree, Graph, TreeEmbedding) {
    let tree = Tree {
        fans: Vec::new(),
        ..join_tree()
    };
    let mut graph = literal_join_graph();
    graph.nodes.remove(&5);
    graph.edges.retain(|id, _| *id < 4);
    let embedding = TreeEmbedding {
        arcs: BTreeMap::from([(1, 0), (2, 1), (3, 2), (4, 3)]),
        fans: Vec::new(),
        next_node: 5,
        next_edge: 4,
    };
    assert_eq!(
        panproto_reference::encode::tree_to_graph(&tree, None),
        Ok((graph.clone(), embedding.clone()))
    );
    (tree, graph, embedding)
}

/// That applying and lowering `reading` under `embedding` refuse for want
/// of `u32` ids.
fn assert_exhausts_in_u32(tree: &Tree, embedding: &TreeEmbedding, reading: &TreeEdit) {
    assert_eq!(
        edit::apply_tree(reading, tree, &mut embedding.clone()),
        Err(edit::TreeApplyError::IdsExhausted)
    );
    assert_eq!(
        edit::graph_delta_from_tree_edit(tree, &mut embedding.clone(), reading, None),
        Err(edit::TreeApplyError::IdsExhausted)
    );
}

/// The lowering of inserting fan `f` (root, `arm` member 1) and then leaf
/// `high` under the root on the fanless [`join_tree`], and the lowering of
/// the two steps in the other order over unbounded ids, which draws the
/// fan at `u32::MAX`, both written out.
fn fan_then_high_node_deltas(high: u32, top: &SEdge) -> (GraphDelta, GraphDelta) {
    let parent_label = SEdge::new("f", "root", "hyper_parent", None);
    let arm_label = SEdge::new("f", "feature", "hyper_label", Some("arm"));
    let mut scripted = GraphDelta::identity();
    let mut relocated = GraphDelta::identity();
    for (delta, fan, [arc, parent, arm]) in [
        (&mut scripted, 5, [6, 4, 5]),
        (&mut relocated, u32::MAX, [4, 5, 6]),
    ] {
        delta
            .add
            .insert(GElem::Node(fan), GPayload::Node(fan_node(fan, "f")));
        delta.add.insert(GElem::Node(high), node(high, "leaf"));
        delta.add.insert(
            GElem::Edge(arc),
            GPayload::Edge(plain_edge(0, high, top.clone())),
        );
        delta.add.insert(
            GElem::Edge(parent),
            GPayload::Edge(plain_edge(fan, 0, parent_label.clone())),
        );
        delta.add.insert(
            GElem::Edge(arm),
            GPayload::Edge(plain_edge(fan, 1, arm_label.clone())),
        );
    }
    (scripted, relocated)
}

/// Embeddings with no graph form, each refused by both id types: a node
/// filed under another id, an arc to an absent node, a fan whose parent is
/// absent, an arc id at the edge floor that the edit itself would remove,
/// and an edit that inserts a node of the fan shape.
#[test]
fn tree_apply_refuses_embeddings_without_a_graph_form_over_both_id_types() {
    let (tree, _, embedding) = fanless_join_tree_graph_and_embedding();
    let mut misfiled = tree.clone();
    misfiled.nodes.insert(4, Node::new(9, "leaf"));
    let mut dangling_arc = tree.clone();
    dangling_arc
        .arcs
        .push((2, 9, SEdge::new("feature", "leaf", "child", Some("e"))));
    let mut dangling_arc_embedding = embedding.clone();
    dangling_arc_embedding.arcs.insert(9, 4);
    dangling_arc_embedding.next_edge = 5;
    let mut dangling_fan = tree.clone();
    dangling_fan.fans.push(arm_fan(9, "fan", 3));
    let mut dangling_fan_embedding = embedding.clone();
    dangling_fan_embedding
        .fans
        .push((5, 4, BTreeMap::from([("arm".into(), 5)])));
    dangling_fan_embedding.next_node = 6;
    dangling_fan_embedding.next_edge = 6;
    let low_edge_floor = TreeEmbedding {
        next_edge: 3,
        ..embedding.clone()
    };
    let mut fan_shaped = Node::new(7, "leaf");
    fan_shaped.shape = NodeShape::HyperEdge;
    let fan_shaped_insert = TreeEdit::InsertNode {
        parent: 0,
        child_id: 7,
        node: fan_shaped,
        edge: inserted_label(),
    };
    let cases = [
        (
            &misfiled,
            &embedding,
            TreeEdit::Identity,
            edit::TreeApplyError::InvalidEmbedding,
        ),
        (
            &dangling_arc,
            &dangling_arc_embedding,
            TreeEdit::Identity,
            edit::TreeApplyError::InvalidEmbedding,
        ),
        (
            &dangling_fan,
            &dangling_fan_embedding,
            TreeEdit::Identity,
            edit::TreeApplyError::InvalidEmbedding,
        ),
        (
            &tree,
            &low_edge_floor,
            TreeEdit::DeleteNode { id: 4 },
            edit::TreeApplyError::InvalidEmbedding,
        ),
        (
            &tree,
            &embedding,
            fan_shaped_insert,
            edit::TreeApplyError::ReservedVocabulary,
        ),
    ];
    for (case_tree, case_embedding, case_edit, expected) in cases {
        assert_tree_apply_refusal(case_tree, case_embedding, &case_edit, expected.clone());
        let mut unbounded = edit::unbounded_embedding(case_embedding);
        assert_eq!(
            edit::apply_tree_unbounded(&case_edit, case_tree, &mut unbounded),
            Err(expected)
        );
        assert_eq!(unbounded, edit::unbounded_embedding(case_embedding));
    }
    let mut after_delete = edit::unbounded_embedding(&low_edge_floor);
    after_delete.arcs.remove(&4);
    let mut deleted = tree.clone();
    deleted.nodes.remove(&4);
    deleted.arcs.retain(|(_, child, _)| *child != 4);
    assert_eq!(
        edit::apply_tree_unbounded(&TreeEdit::Identity, &deleted, &mut after_delete),
        Ok(deleted)
    );
}

/// An edge-label change of an arc, beside the relabel of its target, is
/// not the fan relabel a lowering writes: it is refused for that reason.
#[test]
fn tree_reader_refuses_an_arc_label_change_beside_its_target_relabel() {
    let (tree, graph, embedding) = fanless_join_tree_graph_and_embedding();
    let old = SEdge::new("feature", "leaf", "child", Some("c"));
    let mut new = old.clone();
    new.tgt = "renamed".into();
    let mut delta = GraphDelta::identity();
    delta.attrs.insert(
        AttrKey::Anchor(3),
        (
            AttrVal::Anchor("leaf".into()),
            AttrVal::Anchor("renamed".into()),
        ),
    );
    delta.attrs.insert(
        AttrKey::EdgeLabel(2),
        (AttrVal::Label(old), AttrVal::Label(new)),
    );
    assert!(edit::apply(&delta, &graph).is_ok());
    assert!(not_tree_edit(
        &edit::tree_edit_from_graph_delta(&tree, &embedding, &delta, None),
        "an edge-label change is not the required fan relabel"
    ));
}

/// `u32::MAX + 1`, the node floor after inserting node `u32::MAX` over
/// unbounded ids.
const PAST_U32: u128 = 1 << 32;

// A delta that adds tree node `u32::MAX` is the lowering of its reading
// over unbounded ids, exactly; under `u32` ids the node floor cannot pass
// `u32::MAX`, so applying or lowering the reading refuses with
// `IdsExhausted`, the exhaustion the `u32` refinement permits.
#[test]
fn a_tree_node_at_the_id_maximum_reads_exactly_over_unbounded_ids_and_exhausts_in_u32() {
    let (tree, graph, embedding) = fanless_join_tree_graph_and_embedding();
    let top = SEdge::new("root", "leaf", "child", Some("top"));
    let mut delta = GraphDelta::identity();
    delta
        .add
        .insert(GElem::Node(u32::MAX), node(u32::MAX, "leaf"));
    delta.add.insert(
        GElem::Edge(4),
        GPayload::Edge(plain_edge(0, u32::MAX, top.clone())),
    );
    let reading = TreeEdit::InsertNode {
        parent: 0,
        child_id: u32::MAX,
        node: Node::new(u32::MAX, "leaf"),
        edge: top,
    };
    assert_eq!(
        edit::tree_edit_from_graph_delta(&tree, &embedding, &delta, None),
        Ok(reading.clone())
    );
    let edited_tree = panproto_reference::encode::graph_to_tree(
        &edit::apply(&delta, &graph).unwrap(),
        0,
        &|_| ParentNameRule::Exact(None),
    )
    .unwrap();
    let mut unbounded = edit::unbounded_embedding(&embedding);
    assert_eq!(
        edit::apply_tree_unbounded(&reading, &tree, &mut unbounded.clone()),
        Ok(edited_tree)
    );
    assert_eq!(
        edit::graph_delta_from_tree_edit_unbounded(&tree, &mut unbounded, &reading, None),
        Ok(Some(delta))
    );
    assert_eq!(
        unbounded,
        TreeEmbedding {
            arcs: BTreeMap::from([(1, 0), (2, 1), (3, 2), (4, 3), (u32::MAX, 4)]),
            fans: Vec::new(),
            next_node: PAST_U32,
            next_edge: 5,
        }
    );
    assert_exhausts_in_u32(&tree, &embedding, &reading);
}

// The script inserts a fan and then tree node `u32::MAX - 1`; its reading
// inserts the node first (nodes before fans), so over unbounded ids the
// fan is drawn at `u32::MAX` and the reading's lowering is the script's
// up to renaming fresh ids (effect law b), while under `u32` ids the fan
// draw finds no `u32` floor past `u32::MAX` and refuses, although the
// script applies.
#[test]
fn a_reading_that_draws_a_fan_past_a_high_node_has_the_effect_over_unbounded_ids_only() {
    let (tree, graph, embedding) = fanless_join_tree_graph_and_embedding();
    let fan = Fan {
        parent: 0,
        hyper_edge: "f".into(),
        children: BTreeMap::from([("arm".into(), 1)]),
    };
    let high = u32::MAX - 1;
    let top = SEdge::new("root", "leaf", "child", Some("top"));
    let insert_fan = TreeEdit::InsertFan { fan };
    let insert_high = TreeEdit::InsertNode {
        parent: 0,
        child_id: high,
        node: Node::new(high, "leaf"),
        edge: top.clone(),
    };
    let script = TreeEdit::Sequence(vec![insert_fan.clone(), insert_high.clone()]);
    let (delta, relocated) = fan_then_high_node_deltas(high, &top);
    assert_eq!(
        edit::graph_delta_from_tree_edit(&tree, &mut embedding.clone(), &script, None),
        Ok(delta.clone())
    );
    let reading = TreeEdit::Sequence(vec![insert_high, insert_fan]);
    assert_eq!(
        edit::tree_edit_from_graph_delta(&tree, &embedding, &delta, None),
        Ok(reading.clone())
    );
    let edited_tree = panproto_reference::encode::graph_to_tree(
        &edit::apply(&delta, &graph).unwrap(),
        0,
        &|_| ParentNameRule::Exact(None),
    )
    .unwrap();
    let mut scripted = edit::unbounded_embedding(&embedding);
    assert_eq!(
        edit::apply_tree_unbounded(&script, &tree, &mut scripted),
        Ok(edited_tree.clone())
    );
    let mut unbounded = edit::unbounded_embedding(&embedding);
    assert_eq!(
        edit::apply_tree_unbounded(&reading, &tree, &mut unbounded.clone()),
        Ok(edited_tree)
    );
    assert_eq!(
        edit::graph_delta_from_tree_edit_unbounded(&tree, &mut unbounded, &reading, None),
        Ok(Some(relocated.clone()))
    );
    assert_eq!(
        relocated,
        rename_fresh_ids(
            &delta,
            &embedding,
            |id| if id == 5 { u32::MAX } else { id },
            |id| [5, 6, 4][usize::try_from(id - 4).unwrap()]
        )
    );
    assert_eq!(
        unbounded,
        TreeEmbedding {
            arcs: BTreeMap::from([(1, 0), (2, 1), (3, 2), (4, 3), (high, 4)]),
            fans: vec![(u128::from(u32::MAX), 5, BTreeMap::from([("arm".into(), 6)]))],
            next_node: PAST_U32,
            next_edge: 7,
        }
    );
    assert_eq!(
        scripted,
        TreeEmbedding {
            arcs: BTreeMap::from([(1, 0), (2, 1), (3, 2), (4, 3), (high, 6)]),
            fans: vec![(5, 4, BTreeMap::from([("arm".into(), 5)]))],
            next_node: u128::from(u32::MAX),
            next_edge: 7,
        }
    );
    assert_exhausts_in_u32(&tree, &embedding, &reading);
}

// The unbounded forms stand for the natural numbers only as far as
// `u128` reaches: past `u128::MAX` they refuse with `IdsExhausted` like
// the `u32` forms past `u32::MAX`. No embedding read from a `u32` one
// comes near that floor.
#[test]
fn the_unbounded_forms_run_out_only_past_the_u128_maximum() {
    let (tree, _, embedding) = fanless_join_tree_graph_and_embedding();
    let fan = TreeEdit::InsertFan {
        fan: Fan {
            parent: 0,
            hyper_edge: "f".into(),
            children: BTreeMap::from([("arm".into(), 1)]),
        },
    };
    let mut at_node_maximum = TreeEmbedding {
        next_node: u128::MAX,
        ..edit::unbounded_embedding(&embedding)
    };
    let mut below_node_maximum = TreeEmbedding {
        next_node: u128::MAX - 1,
        ..edit::unbounded_embedding(&embedding)
    };
    assert_eq!(
        edit::apply_tree_unbounded(&fan, &tree, &mut at_node_maximum),
        Err(edit::TreeApplyError::IdsExhausted)
    );
    assert!(edit::apply_tree_unbounded(&fan, &tree, &mut below_node_maximum).is_ok());
    assert_eq!(below_node_maximum.next_node, u128::MAX);
    let mut at_edge_maximum = TreeEmbedding {
        next_edge: u128::MAX,
        ..edit::unbounded_embedding(&embedding)
    };
    assert_eq!(
        edit::graph_delta_from_tree_edit_unbounded(
            &tree,
            &mut at_edge_maximum,
            &insert_leaf(0, 5),
            None
        ),
        Err(edit::TreeApplyError::IdsExhausted)
    );
    assert_eq!(at_edge_maximum.next_edge, u128::MAX);
}

// Law (c) over unbounded ids: a join that re-parents two arcs draws edge
// ids `u32::MAX - 1` and `u32::MAX`. Its lowering is a `u32` delta, though
// the `u32` run cannot draw `u32::MAX` (no floor follows it). The reader
// must read the single step exactly, as over unbounded ids, and not fall
// back to a reconstruction that contracts and moves.
#[test]
fn a_single_step_whose_lowering_needs_the_last_edge_id_reads_back_exactly() {
    let tree = join_tree();
    let graph = literal_join_graph();
    let embedding = TreeEmbedding {
        next_edge: u32::MAX - 1,
        ..literal_join_embedding()
    };
    let join = TreeEdit::JoinFeatures {
        primary: 1,
        joined: vec![2],
        produce: Node::new(1, "joined"),
    };
    let mut expected = literal_deletions(
        &graph,
        [
            GElem::Node(2),
            GElem::Node(5),
            GElem::Edge(1),
            GElem::Edge(2),
            GElem::Edge(3),
            GElem::Edge(4),
            GElem::Edge(5),
        ],
    );
    expected.add.insert(
        GElem::Edge(u32::MAX - 1),
        GPayload::Edge(plain_edge(
            1,
            3,
            SEdge::new("joined", "leaf", "child", Some("c")),
        )),
    );
    expected.add.insert(
        GElem::Edge(u32::MAX),
        GPayload::Edge(plain_edge(
            1,
            4,
            SEdge::new("joined", "leaf", "child", Some("d")),
        )),
    );
    expected.attrs.insert(
        AttrKey::Anchor(1),
        (
            AttrVal::Anchor("feature".into()),
            AttrVal::Anchor("joined".into()),
        ),
    );
    let mut lowered = edit::unbounded_embedding(&embedding);
    assert_eq!(
        edit::graph_delta_from_tree_edit_unbounded(&tree, &mut lowered, &join, None),
        Ok(Some(expected.clone()))
    );
    assert_eq!((lowered.next_node, lowered.next_edge), (6, PAST_U32));
    assert_eq!(
        edit::graph_delta_from_tree_edit(&tree, &mut embedding.clone(), &join, None),
        Err(edit::TreeApplyError::IdsExhausted)
    );
    let reading = edit::tree_edit_from_graph_delta(&tree, &embedding, &expected, None);
    assert_eq!(reading, Ok(join.clone()));
    let mut relowered = edit::unbounded_embedding(&embedding);
    assert_eq!(
        edit::graph_delta_from_tree_edit_unbounded(&tree, &mut relowered, &join, None),
        Ok(Some(expected))
    );
    assert_eq!(relowered, lowered);
}

// Law (c) compares carriers, not floors. Joining 2 and then 4 into 1
// re-parents 4's arc to 1 (drawing edge 7) before 4 goes, a draw no delta
// shows; joining 4 first draws only edge 6 for 3's arc. Both lower to one
// delta, reach the same arcs and fans, and leave floors (6, 8) and (6, 7).
// The reader sees only the delta and reads both as the later order, so no
// reading leaves both floors, and both scripts meet law (c) over `u32` and
// over unbounded ids, in each shared law check.
#[test]
fn joins_that_differ_only_in_a_dead_draw_meet_law_c_on_carriers_with_other_floors() {
    let tree = join_tree();
    let graph = literal_join_graph();
    let embedding = literal_join_embedding();
    let join = |joined: Vec<u32>| TreeEdit::JoinFeatures {
        primary: 1,
        joined,
        produce: Node::new(1, "joined"),
    };
    let (parent_first, descendant_first) = (join(vec![2, 4]), join(vec![4, 2]));
    let mut expected = literal_deletions(
        &graph,
        [
            GElem::Node(2),
            GElem::Node(4),
            GElem::Node(5),
            GElem::Edge(1),
            GElem::Edge(2),
            GElem::Edge(3),
            GElem::Edge(4),
            GElem::Edge(5),
        ],
    );
    expected.add.insert(
        GElem::Edge(6),
        GPayload::Edge(plain_edge(
            1,
            3,
            SEdge::new("joined", "leaf", "child", Some("c")),
        )),
    );
    expected.attrs.insert(
        AttrKey::Anchor(1),
        (
            AttrVal::Anchor("feature".into()),
            AttrVal::Anchor("joined".into()),
        ),
    );
    let arcs = BTreeMap::from([(1, 0), (3, 6)]);
    for (script, floors) in [(&parent_first, (6, 8)), (&descendant_first, (6, 7))] {
        let mut lowered = embedding.clone();
        assert_eq!(
            edit::graph_delta_from_tree_edit(&tree, &mut lowered, script, None),
            Ok(expected.clone()),
            "{script:?}"
        );
        assert_eq!(lowered.arcs, arcs);
        assert_eq!(lowered.fans, Vec::new());
        assert_eq!((lowered.next_node, lowered.next_edge), floors);
        assert_eq!(
            edit::tree_edit_from_graph_delta(&tree, &embedding, &expected, None),
            Ok(parent_first.clone())
        );
        let mut relowered = embedding.clone();
        assert_eq!(
            edit::graph_delta_from_tree_edit(&tree, &mut relowered, &parent_first, None),
            Ok(expected.clone())
        );
        assert!(same_carriers(&relowered, &lowered), "{script:?}");
        assert_eq!(
            relowered == lowered,
            script == &parent_first,
            "{script:?}: the floors differ exactly when the orders do"
        );

        let mut lowered_n = edit::unbounded_embedding(&embedding);
        assert_eq!(
            edit::graph_delta_from_tree_edit_unbounded(&tree, &mut lowered_n, script, None),
            Ok(Some(expected.clone()))
        );
        assert_eq!(
            (lowered_n.next_node, lowered_n.next_edge),
            (u128::from(floors.0), u128::from(floors.1))
        );
        let mut relowered_n = edit::unbounded_embedding(&embedding);
        assert_eq!(
            edit::graph_delta_from_tree_edit_unbounded(
                &tree,
                &mut relowered_n,
                &parent_first,
                None
            ),
            Ok(Some(expected.clone()))
        );
        assert!(same_carriers(&relowered_n, &lowered_n), "{script:?}");

        let mut findings = Vec::new();
        check_unbounded_lowering(&tree, &embedding, &graph, script, &expected, &mut findings);
        assert_eq!(findings, Vec::<String>::new());
    }
    let scripts = [parent_first, descendant_first];
    assert_eq!(
        readback_law_failures(&tree, scripts.iter().cloned()),
        (2, Vec::new())
    );
    let sweep = ceiling_findings(&tree, &scripts);
    assert_eq!(sweep.findings, Vec::<String>::new());
    assert_eq!(sweep.counts.lowered, 2);
}

// The carrier comparison of law (c) ignores the floors and nothing else.
#[test]
fn same_carriers_compares_every_arc_and_fan_id_and_no_floor() {
    let embedding = literal_join_embedding();
    let changed = |change: fn(&mut TreeEmbedding)| {
        let mut other = embedding.clone();
        change(&mut other);
        same_carriers(&embedding, &other)
    };
    assert!(changed(|_| {}));
    assert!(changed(|other| other.next_node += 1));
    assert!(changed(|other| other.next_edge += 1));
    assert!(changed(|other| {
        other.next_node = u32::MAX;
        other.next_edge = u32::MAX;
    }));
    assert!(!changed(|other| {
        other.arcs.insert(3, 7);
    }));
    assert!(!changed(|other| {
        other.arcs.remove(&4);
    }));
    assert!(!changed(|other| {
        other.arcs.insert(6, 7);
    }));
    assert!(!changed(|other| other.fans[0].0 = 6));
    assert!(!changed(|other| other.fans[0].1 = 7));
    assert!(!changed(|other| {
        other.fans[0].2.insert("arm".into(), 7);
    }));
    assert!(!changed(|other| {
        other.fans[0].2.insert("other-arm".into(), 7);
    }));
    assert!(!changed(|other| other.fans.clear()));
    let unbounded = edit::unbounded_embedding(&embedding);
    let mut raised = unbounded.clone();
    raised.next_edge = PAST_U32;
    assert!(same_carriers(&unbounded, &raised));
    raised.fans[0].1 = PAST_U32;
    assert!(!same_carriers(&unbounded, &raised));
}

/// A fan node re-added under its own id is a fresh fan only in name: no
/// lowering re-uses a fan node id, so the node floor refuses it although
/// the result is the same tree.
#[test]
fn tree_reader_refuses_a_fan_redrawn_under_its_own_node_id() {
    let tree = join_tree();
    let embedding = literal_join_embedding();
    let graph = literal_join_graph();
    let mut delta = literal_deletions(&graph, [GElem::Node(5), GElem::Edge(4), GElem::Edge(5)]);
    delta
        .add
        .insert(GElem::Node(5), GPayload::Node(fan_node(5, "fan")));
    delta.add.insert(
        GElem::Edge(6),
        GPayload::Edge(plain_edge(5, 2, fan_parent_label())),
    );
    delta.add.insert(
        GElem::Edge(7),
        GPayload::Edge(plain_edge(5, 3, fan_arm_label())),
    );
    let edited = edit::apply(&delta, &graph).unwrap();
    assert_eq!(
        panproto_reference::encode::graph_to_tree(&edited, 0, &|_| { ParentNameRule::Exact(None) }),
        Ok(join_tree())
    );
    assert!(not_tree_edit(
        &edit::tree_edit_from_graph_delta(&tree, &embedding, &delta, None),
        "an added arc or fan id is below the embedding floor"
    ));
}

const BELOW_FLOOR: &str = "an added arc or fan id is below the embedding floor";

/// [`join_tree`] with a two-member fan under node 2 and two equal fans
/// under node 1, its encoder graph, and its encoder embedding with the
/// floors raised to `next_node` and `next_edge` (the encoder's are 8 and
/// 11, checked).
fn two_member_and_twin_fan_tree(next_node: u32, next_edge: u32) -> (Tree, Graph, TreeEmbedding) {
    let tree = Tree {
        fans: vec![
            Fan {
                parent: 2,
                hyper_edge: "fan".into(),
                children: BTreeMap::from([("arm".into(), 3), ("leg".into(), 4)]),
            },
            arm_fan(1, "other", 2),
            arm_fan(1, "other", 2),
        ],
        ..join_tree()
    };
    let (graph, encoded) = panproto_reference::encode::tree_to_graph(&tree, None).unwrap();
    assert_eq!(
        encoded,
        TreeEmbedding {
            arcs: BTreeMap::from([(1, 0), (2, 1), (3, 2), (4, 3)]),
            fans: vec![
                (5, 4, BTreeMap::from([("arm".into(), 5), ("leg".into(), 6)])),
                (6, 7, arm_edges(8)),
                (7, 9, arm_edges(10)),
            ],
            next_node: 8,
            next_edge: 11,
        }
    );
    (tree, graph, with_floors(&encoded, next_node, next_edge))
}

/// The edge floor is strict at its boundary. With both floors at
/// `u32::MAX`, a delta that deletes node 1 (its arc 0 and both fans under
/// it) and re-draws arc 3 (2 -> 4) as edge `u32::MAX - 1`, one below the
/// edge floor, is refused by the floor; re-drawn as edge `u32::MAX`, at
/// the floor, it reads back as the deletion followed by the same-arc move
/// that re-draws the arc.
#[test]
fn an_arc_redrawn_one_below_the_edge_floor_is_refused_and_at_the_floor_reads_back() {
    let (tree, graph, embedding) = two_member_and_twin_fan_tree(u32::MAX, u32::MAX);
    let redrawn = |id: u32| {
        let mut delta = literal_deletions(
            &graph,
            [
                GElem::Node(1),
                GElem::Edge(0),
                GElem::Node(6),
                GElem::Edge(7),
                GElem::Edge(8),
                GElem::Node(7),
                GElem::Edge(9),
                GElem::Edge(10),
                GElem::Edge(3),
            ],
        );
        delta
            .add
            .insert(GElem::Edge(id), GPayload::Edge(graph.edges[&3].clone()));
        delta
    };
    let below = redrawn(u32::MAX - 1);
    let at = redrawn(u32::MAX);
    for delta in [&below, &at] {
        let edited = edit::apply(delta, &graph).unwrap();
        assert!(
            panproto_reference::encode::graph_to_tree(&edited, 0, &|_| {
                ParentNameRule::Exact(None)
            })
            .is_ok()
        );
    }
    assert!(not_tree_edit(
        &edit::tree_edit_from_graph_delta(&tree, &embedding, &below, None),
        BELOW_FLOOR
    ));
    assert_eq!(
        edit::tree_edit_from_graph_delta(&tree, &embedding, &at, None),
        Ok(TreeEdit::Sequence(vec![
            TreeEdit::DeleteNode { id: 1 },
            TreeEdit::MoveSubtree {
                node_id: 4,
                new_parent: 2,
                edge: SEdge::new("feature", "leaf", "child", Some("d")),
            },
        ]))
    );
}

/// The fan-node floor is strict at its boundary. With the node floor at
/// `u32::MAX`, a delta that re-draws the two-member fan (node 5 and edges
/// 4 to 6) with fresh edges 11 to 13 and fan node `u32::MAX - 1`, one
/// below the floor, is refused by the floor; with fan node `u32::MAX`, at
/// the floor, it reads back as the fan's deletion and re-insertion (the
/// re-drawn fan now follows the two others).
#[test]
fn a_fan_redrawn_one_below_the_node_floor_is_refused_and_at_the_floor_reads_back() {
    let (tree, graph, embedding) = two_member_and_twin_fan_tree(u32::MAX, 11);
    let redrawn = |fan: u32| {
        let mut delta = literal_deletions(
            &graph,
            [
                GElem::Node(5),
                GElem::Edge(4),
                GElem::Edge(5),
                GElem::Edge(6),
            ],
        );
        delta
            .add
            .insert(GElem::Node(fan), GPayload::Node(fan_node(fan, "fan")));
        for (id, old) in [(11, 4), (12, 5), (13, 6)] {
            let mut edge = graph.edges[&old].clone();
            edge.src = fan;
            delta.add.insert(GElem::Edge(id), GPayload::Edge(edge));
        }
        delta
    };
    let below = redrawn(u32::MAX - 1);
    let at = redrawn(u32::MAX);
    for delta in [&below, &at] {
        let edited = edit::apply(delta, &graph).unwrap();
        assert!(
            panproto_reference::encode::graph_to_tree(&edited, 0, &|_| {
                ParentNameRule::Exact(None)
            })
            .is_ok()
        );
    }
    assert!(not_tree_edit(
        &edit::tree_edit_from_graph_delta(&tree, &embedding, &below, None),
        BELOW_FLOOR
    ));
    assert_eq!(
        edit::tree_edit_from_graph_delta(&tree, &embedding, &at, None),
        Ok(TreeEdit::Sequence(vec![
            TreeEdit::DeleteFan {
                hyper_edge: "fan".into()
            },
            TreeEdit::InsertFan {
                fan: tree.fans[0].clone()
            },
        ]))
    );
}

/// A fan kept at node `u32::MAX - 1` while its `arm` edge is re-drawn to
/// another member is no lowering (an edit changing a fan draws a new fan
/// node), and the reader refuses it although the fan's node is a start id
/// next to the ids its reconstruction runs past. The node floor is
/// `u32::MAX`; the delta also adds leaf `u32::MAX`, so the reconstruction
/// (delete the fan, insert the leaf, insert the changed fan) draws the
/// fan's node past `u32`. The comparison writes that node at `u32::MAX`,
/// an id no start element has, so the kept node `u32::MAX - 1` counts as
/// deleted by the reconstruction and kept by the delta.
#[test]
fn a_fan_kept_under_a_redrawn_member_edge_is_refused_when_its_reading_draws_past_u32() {
    let tree = join_tree();
    let fan = u32::MAX - 1;
    let embedding = TreeEmbedding {
        fans: vec![(fan, 4, arm_edges(5))],
        next_node: u32::MAX,
        ..literal_join_embedding()
    };
    assert_eq!(
        edit::apply_tree(&TreeEdit::Identity, &tree, &mut embedding.clone()),
        Ok(join_tree())
    );
    let mut graph = literal_join_graph();
    graph.nodes.remove(&5);
    graph.nodes.insert(fan, fan_node(fan, "fan"));
    graph.edges.get_mut(&4).unwrap().src = fan;
    graph.edges.get_mut(&5).unwrap().src = fan;
    assert_eq!(
        graph_of_carriers(&tree, &edit::unbounded_embedding(&embedding)),
        Some(graph.clone())
    );
    let mut delta = literal_deletions(&graph, [GElem::Edge(5)]);
    delta.add.insert(
        GElem::Edge(6),
        GPayload::Edge(plain_edge(fan, 4, fan_arm_label())),
    );
    delta
        .add
        .insert(GElem::Node(u32::MAX), node(u32::MAX, "leaf"));
    let top = SEdge::new("root", "leaf", "child", Some("top"));
    delta
        .add
        .insert(GElem::Edge(7), GPayload::Edge(plain_edge(0, u32::MAX, top)));
    let edited = edit::apply(&delta, &graph).unwrap();
    let edited_tree =
        panproto_reference::encode::graph_to_tree(&edited, 0, &|_| ParentNameRule::Exact(None))
            .unwrap();
    assert_eq!(edited_tree.fans, vec![arm_fan(2, "fan", 4)]);
    assert!(edited_tree.nodes.contains_key(&u32::MAX));
    assert_eq!(
        edit::tree_edit_from_graph_delta(&tree, &embedding, &delta, None),
        Err(edit::TreeApplyError::NotTreeEdit {
            reason: FIXED_CARRIERS.to_owned(),
        })
    );
}

/// An anchor change whose old anchor is stale, beside the label change of
/// the fan edge into that node with the graph's old label: the label
/// change is not the one the anchor change implies (its old target is the
/// graph's anchor, not the delta's old anchor), so the reader refuses it
/// as outside the image before it tries to apply the delta, which would
/// refuse it as stale.
#[test]
fn tree_reader_refuses_a_fan_relabel_whose_anchor_change_has_another_old_anchor() {
    let tree = join_tree();
    let embedding = literal_join_embedding();
    let graph = literal_join_graph();
    let mut renamed = fan_arm_label();
    renamed.tgt = "renamed".into();
    let mut delta = GraphDelta::identity();
    delta.attrs.insert(
        AttrKey::Anchor(3),
        (
            AttrVal::Anchor("stale".into()),
            AttrVal::Anchor("renamed".into()),
        ),
    );
    delta.attrs.insert(
        AttrKey::EdgeLabel(5),
        (AttrVal::Label(fan_arm_label()), AttrVal::Label(renamed)),
    );
    assert!(edit::apply(&delta, &graph).is_err());
    assert!(not_tree_edit(
        &edit::tree_edit_from_graph_delta(&tree, &embedding, &delta, None),
        "an edge-label change is not the required fan relabel"
    ));
}

/// The unbounded lowering of an edit whose result has a carrier past
/// `u32` returns `Ok(None)` and still advances the embedding to the one
/// tree apply reaches: here the leaf `u32::MAX` lifts the node floor past
/// `u32`, so the fan inserted next is drawn at `2^32`. The same holds when
/// the start embedding already has a carrier past `u32`. An invalid
/// embedding is refused, with the embedding unchanged, exactly as tree
/// apply refuses it.
#[test]
fn the_unbounded_lowering_without_a_graph_form_still_advances_the_embedding() {
    let tree = join_tree();
    let start = edit::unbounded_embedding(&literal_join_embedding());
    let fan = arm_fan(0, "f", 1);
    let script = TreeEdit::Sequence(vec![insert_leaf(1, u32::MAX), TreeEdit::InsertFan { fan }]);
    let mut applied = start.clone();
    let edited = edit::apply_tree_unbounded(&script, &tree, &mut applied).unwrap();
    assert_eq!(
        applied.fans[1],
        (PAST_U32, 7, BTreeMap::from([("arm".into(), 8)]))
    );
    assert_eq!(applied.next_node, PAST_U32 + 1);
    let mut lowered = start.clone();
    assert_eq!(
        edit::graph_delta_from_tree_edit_unbounded(&tree, &mut lowered, &script, None),
        Ok(None)
    );
    assert_eq!(lowered, applied);

    let mut advanced = applied.clone();
    let mut reapplied = applied.clone();
    let leaf = insert_leaf(0, 9);
    let leafed = edit::apply_tree_unbounded(&leaf, &edited, &mut reapplied).unwrap();
    assert!(leafed.nodes.contains_key(&9));
    assert_eq!(
        edit::graph_delta_from_tree_edit_unbounded(&edited, &mut advanced, &leaf, None),
        Ok(None)
    );
    assert_eq!(advanced, reapplied);
    assert_ne!(advanced, applied);

    let low_floor = TreeEmbedding {
        next_edge: 5,
        ..start.clone()
    };
    let mut fan_shaped = tree.clone();
    fan_shaped.nodes.insert(4, fan_node(4, "leaf"));
    for (invalid_tree, corrupt, error) in [
        (&tree, low_floor, edit::TreeApplyError::InvalidEmbedding),
        (&fan_shaped, start, edit::TreeApplyError::ReservedVocabulary),
    ] {
        let mut unchanged = corrupt.clone();
        assert_eq!(
            edit::graph_delta_from_tree_edit_unbounded(invalid_tree, &mut unchanged, &leaf, None),
            Err(error.clone())
        );
        assert_eq!(unchanged, corrupt);
        assert_eq!(
            edit::apply_tree_unbounded(&leaf, invalid_tree, &mut unchanged),
            Err(error)
        );
    }
}

/// Attribute changes no lowering writes: a no-op anchor change, and the
/// label change of a fan edge that the delta also deletes (the relocated
/// copy already carries the new label). Graph application accepts both and
/// leaves the same tree as the lowering, so only the attribute comparison
/// refuses them; without the extra entry each reads back.
#[test]
fn tree_reader_refuses_attribute_changes_no_lowering_writes() {
    let tree = join_tree();
    let embedding = literal_join_embedding();
    let graph = literal_join_graph();
    let refusal = Err(edit::TreeApplyError::NotTreeEdit {
        reason: FIXED_CARRIERS.to_owned(),
    });

    let mut no_op = GraphDelta::identity();
    no_op.attrs.insert(
        AttrKey::Anchor(0),
        (
            AttrVal::Anchor("root".into()),
            AttrVal::Anchor("root".into()),
        ),
    );
    assert_eq!(edit::apply(&no_op, &graph), Ok(literal_join_graph()));
    assert_eq!(
        edit::tree_edit_from_graph_delta(&tree, &embedding, &no_op, None),
        refusal
    );

    let renamed_arm = SEdge::new("fan", "renamed", "hyper_label", Some("arm"));
    let mut relocated = literal_deletions(&graph, [GElem::Node(5), GElem::Edge(4), GElem::Edge(5)]);
    relocated
        .add
        .insert(GElem::Node(6), GPayload::Node(fan_node(6, "fan")));
    relocated.add.insert(
        GElem::Edge(6),
        GPayload::Edge(plain_edge(6, 2, fan_parent_label())),
    );
    relocated.add.insert(
        GElem::Edge(7),
        GPayload::Edge(plain_edge(6, 3, renamed_arm.clone())),
    );
    relocated.attrs.insert(
        AttrKey::Anchor(3),
        (
            AttrVal::Anchor("leaf".into()),
            AttrVal::Anchor("renamed".into()),
        ),
    );
    let relabel = TreeEdit::RelabelNode {
        id: 3,
        new_anchor: "renamed".into(),
    };
    assert_eq!(
        edit::tree_edit_from_graph_delta(&tree, &embedding, &relocated, None),
        Ok(relabel)
    );
    let mut stale_label = relocated;
    stale_label.attrs.insert(
        AttrKey::EdgeLabel(5),
        (AttrVal::Label(fan_arm_label()), AttrVal::Label(renamed_arm)),
    );
    let mut renamed_graph = literal_join_graph();
    renamed_graph.nodes.insert(3, Node::new(3, "renamed"));
    renamed_graph.nodes.remove(&5);
    renamed_graph.edges.remove(&4);
    renamed_graph.edges.remove(&5);
    renamed_graph.nodes.insert(6, fan_node(6, "fan"));
    renamed_graph
        .edges
        .insert(6, plain_edge(6, 2, fan_parent_label()));
    renamed_graph.edges.insert(
        7,
        plain_edge(
            6,
            3,
            SEdge::new("fan", "renamed", "hyper_label", Some("arm")),
        ),
    );
    assert_eq!(edit::apply(&stale_label, &graph), Ok(renamed_graph));
    assert_eq!(
        edit::tree_edit_from_graph_delta(&tree, &embedding, &stale_label, None),
        refusal
    );
}

/// One row of the literal lowering oracle: a start tree, an edit, and the
/// hand-written delta, final embedding, final tree and final graph.
struct LiteralLowering {
    name: &'static str,
    tree: Tree,
    edit: TreeEdit,
    delta: GraphDelta,
    embedding: TreeEmbedding,
    final_tree: Tree,
    final_graph: Graph,
}

fn check_literal_lowering(row: LiteralLowering) {
    let LiteralLowering {
        name,
        tree,
        edit: edit_step,
        delta,
        embedding,
        final_tree,
        final_graph,
    } = row;
    let mut start_graph = literal_join_graph();
    start_graph.nodes.clone_from(&tree.nodes);
    start_graph.nodes.insert(5, fan_node(5, "fan"));
    let start = literal_join_embedding();
    let mut lowered = start.clone();
    assert_eq!(
        edit::graph_delta_from_tree_edit(&tree, &mut lowered, &edit_step, None),
        Ok(delta.clone()),
        "{name}: lowering"
    );
    assert_eq!(lowered, embedding, "{name}: lowered embedding");
    assert_eq!(
        edit::apply(&delta, &start_graph),
        Ok(final_graph),
        "{name}: final graph"
    );
    let mut applied = start.clone();
    assert_eq!(
        edit::apply_tree(&edit_step, &tree, &mut applied),
        Ok(final_tree),
        "{name}: final tree"
    );
    assert_eq!(applied, embedding, "{name}: applied embedding");
    assert_eq!(
        edit::tree_edit_from_graph_delta(&tree, &start, &delta, None),
        Ok(edit_step),
        "{name}: reading"
    );
}

fn join_tree_with(change: impl FnOnce(&mut Tree)) -> Tree {
    let mut tree = join_tree();
    change(&mut tree);
    tree
}

fn join_graph_with(change: impl FnOnce(&mut Graph)) -> Graph {
    let mut graph = literal_join_graph();
    change(&mut graph);
    graph
}

fn join_embedding_with(change: impl FnOnce(&mut TreeEmbedding)) -> TreeEmbedding {
    let mut embedding = literal_join_embedding();
    change(&mut embedding);
    embedding
}

fn child_label(src: &str, name: &str) -> SEdge {
    SEdge::new(src, "leaf", "child", Some(name))
}

/// `delta` with node `id` replaced: deleted with its payload in `graph` and
/// added as `node`, with no attribute change of its own. A step sequence
/// that deletes a node and later inserts one under its id, or a step that
/// gives a tree node the id of a fan node, lowers to such a replacement.
fn replacing(mut delta: GraphDelta, graph: &Graph, id: u32, node: Node) -> GraphDelta {
    delta.attrs.retain(|key, _| match key {
        AttrKey::Value(node)
        | AttrKey::Field(node, _)
        | AttrKey::Meta(node)
        | AttrKey::Anchor(node) => *node != id,
        AttrKey::EdgeLabel(_) | AttrKey::EdgePosition(_) => true,
    });
    delta
        .del
        .insert(GElem::Node(id), graph_payload(graph, &GElem::Node(id)));
    delta.add.insert(GElem::Node(id), GPayload::Node(node));
    delta
}

fn added(mut delta: GraphDelta, elements: Vec<(GElem, GPayload)>) -> GraphDelta {
    delta.add.extend(elements);
    delta
}

fn with_attr(mut delta: GraphDelta, key: AttrKey, old: AttrVal, new: AttrVal) -> GraphDelta {
    delta.attrs.insert(key, (old, new));
    delta
}

fn structural_literal_rows() -> Vec<LiteralLowering> {
    let graph = literal_join_graph();
    vec![
        LiteralLowering {
            name: "identity",
            tree: join_tree(),
            edit: TreeEdit::Identity,
            delta: GraphDelta::identity(),
            embedding: literal_join_embedding(),
            final_tree: join_tree(),
            final_graph: literal_join_graph(),
        },
        LiteralLowering {
            name: "insert node 6",
            tree: join_tree(),
            edit: TreeEdit::InsertNode {
                parent: 1,
                child_id: 6,
                node: Node::new(6, "leaf"),
                edge: child_label("feature", "new"),
            },
            delta: added(
                GraphDelta::identity(),
                vec![
                    (GElem::Node(6), GPayload::Node(Node::new(6, "leaf"))),
                    (
                        GElem::Edge(6),
                        GPayload::Edge(plain_edge(1, 6, child_label("feature", "new"))),
                    ),
                ],
            ),
            embedding: join_embedding_with(|e| {
                e.arcs.insert(6, 6);
                e.next_node = 7;
                e.next_edge = 7;
            }),
            final_tree: join_tree_with(|t| {
                t.nodes.insert(6, Node::new(6, "leaf"));
                t.arcs.push((1, 6, child_label("feature", "new")));
            }),
            final_graph: join_graph_with(|g| {
                g.nodes.insert(6, Node::new(6, "leaf"));
                g.edges
                    .insert(6, plain_edge(1, 6, child_label("feature", "new")));
            }),
        },
        LiteralLowering {
            name: "delete node 4",
            tree: join_tree(),
            edit: TreeEdit::DeleteNode { id: 4 },
            delta: literal_deletions(&graph, [GElem::Node(4), GElem::Edge(3)]),
            embedding: join_embedding_with(|e| {
                e.arcs.remove(&4);
            }),
            final_tree: join_tree_with(|t| {
                t.nodes.remove(&4);
                t.arcs.retain(|(_, child, _)| *child != 4);
            }),
            final_graph: join_graph_with(|g| {
                g.nodes.remove(&4);
                g.edges.remove(&3);
            }),
        },
    ]
}

fn reparenting_literal_rows() -> Vec<LiteralLowering> {
    let graph = literal_join_graph();
    vec![
        LiteralLowering {
            name: "contract node 2",
            tree: join_tree(),
            edit: TreeEdit::ContractNode { id: 2 },
            delta: added(
                literal_deletions(
                    &graph,
                    [2, 5]
                        .map(GElem::Node)
                        .into_iter()
                        .chain((1..=5).map(GElem::Edge)),
                ),
                vec![
                    (
                        GElem::Edge(6),
                        GPayload::Edge(plain_edge(0, 3, child_label("root", "c"))),
                    ),
                    (
                        GElem::Edge(7),
                        GPayload::Edge(plain_edge(0, 4, child_label("root", "d"))),
                    ),
                ],
            ),
            embedding: TreeEmbedding {
                arcs: BTreeMap::from([(1, 0), (3, 6), (4, 7)]),
                fans: Vec::new(),
                next_node: 6,
                next_edge: 8,
            },
            final_tree: join_tree_with(|t| {
                t.nodes.remove(&2);
                t.arcs = vec![
                    (0, 1, SEdge::new("root", "feature", "child", Some("a"))),
                    (0, 3, child_label("root", "c")),
                    (0, 4, child_label("root", "d")),
                ];
                t.fans.clear();
            }),
            final_graph: join_graph_with(|g| {
                g.nodes.remove(&2);
                g.nodes.remove(&5);
                g.edges.retain(|id, _| *id == 0);
                g.edges
                    .insert(6, plain_edge(0, 3, child_label("root", "c")));
                g.edges
                    .insert(7, plain_edge(0, 4, child_label("root", "d")));
            }),
        },
        LiteralLowering {
            name: "move subtree 3",
            tree: join_tree(),
            edit: TreeEdit::MoveSubtree {
                node_id: 3,
                new_parent: 1,
                edge: child_label("feature", "moved"),
            },
            delta: added(
                literal_deletions(&graph, [GElem::Edge(2)]),
                vec![(
                    GElem::Edge(6),
                    GPayload::Edge(plain_edge(1, 3, child_label("feature", "moved"))),
                )],
            ),
            embedding: join_embedding_with(|e| {
                e.arcs.insert(3, 6);
                e.next_edge = 7;
            }),
            final_tree: join_tree_with(|t| {
                t.arcs.retain(|(_, child, _)| *child != 3);
                t.arcs.push((1, 3, child_label("feature", "moved")));
            }),
            final_graph: join_graph_with(|g| {
                g.edges.remove(&2);
                g.edges
                    .insert(6, plain_edge(1, 3, child_label("feature", "moved")));
            }),
        },
    ]
}

fn fan_and_join_literal_rows() -> Vec<LiteralLowering> {
    let graph = literal_join_graph();
    let second_fan = Fan {
        parent: 1,
        hyper_edge: "second-fan".into(),
        children: BTreeMap::from([("arm".into(), 4)]),
    };
    let second_parent = plain_edge(
        6,
        1,
        SEdge::new("second-fan", "feature", "hyper_parent", None),
    );
    let second_arm = plain_edge(
        6,
        4,
        SEdge::new("second-fan", "leaf", "hyper_label", Some("arm")),
    );
    vec![
        LiteralLowering {
            name: "insert fan",
            tree: join_tree(),
            edit: TreeEdit::InsertFan {
                fan: second_fan.clone(),
            },
            delta: added(
                GraphDelta::identity(),
                vec![
                    (GElem::Node(6), GPayload::Node(fan_node(6, "second-fan"))),
                    (GElem::Edge(6), GPayload::Edge(second_parent.clone())),
                    (GElem::Edge(7), GPayload::Edge(second_arm.clone())),
                ],
            ),
            embedding: join_embedding_with(|e| {
                e.fans.push((6, 6, arm_edges(7)));
                e.next_node = 7;
                e.next_edge = 8;
            }),
            final_tree: join_tree_with(|t| t.fans.push(second_fan)),
            final_graph: join_graph_with(|g| {
                g.nodes.insert(6, fan_node(6, "second-fan"));
                g.edges.insert(6, second_parent);
                g.edges.insert(7, second_arm);
            }),
        },
        LiteralLowering {
            name: "delete fan",
            tree: join_tree(),
            edit: TreeEdit::DeleteFan {
                hyper_edge: "fan".into(),
            },
            delta: literal_deletions(&graph, [GElem::Node(5), GElem::Edge(4), GElem::Edge(5)]),
            embedding: join_embedding_with(|e| e.fans.clear()),
            final_tree: join_tree_with(|t| t.fans.clear()),
            final_graph: join_graph_with(|g| {
                g.nodes.remove(&5);
                g.edges.remove(&4);
                g.edges.remove(&5);
            }),
        },
    ]
}

fn join_literal_rows() -> Vec<LiteralLowering> {
    let graph = literal_join_graph();
    vec![LiteralLowering {
        name: "join 2 into 1",
        tree: join_tree(),
        edit: TreeEdit::JoinFeatures {
            primary: 1,
            joined: vec![2],
            produce: Node::new(1, "joined"),
        },
        delta: with_attr(
            added(
                literal_deletions(
                    &graph,
                    [2, 5]
                        .map(GElem::Node)
                        .into_iter()
                        .chain((1..=5).map(GElem::Edge)),
                ),
                vec![
                    (
                        GElem::Edge(6),
                        GPayload::Edge(plain_edge(1, 3, child_label("joined", "c"))),
                    ),
                    (
                        GElem::Edge(7),
                        GPayload::Edge(plain_edge(1, 4, child_label("joined", "d"))),
                    ),
                ],
            ),
            AttrKey::Anchor(1),
            AttrVal::Anchor("feature".into()),
            AttrVal::Anchor("joined".into()),
        ),
        embedding: TreeEmbedding {
            arcs: BTreeMap::from([(1, 0), (3, 6), (4, 7)]),
            fans: Vec::new(),
            next_node: 6,
            next_edge: 8,
        },
        final_tree: join_tree_with(|t| {
            t.nodes.remove(&2);
            t.nodes.insert(1, Node::new(1, "joined"));
            t.arcs = vec![
                (0, 1, SEdge::new("root", "feature", "child", Some("a"))),
                (1, 3, child_label("joined", "c")),
                (1, 4, child_label("joined", "d")),
            ];
            t.fans.clear();
        }),
        final_graph: join_graph_with(|g| {
            g.nodes.remove(&2);
            g.nodes.remove(&5);
            g.nodes.insert(1, Node::new(1, "joined"));
            g.edges.retain(|id, _| *id == 0);
            g.edges
                .insert(6, plain_edge(1, 3, child_label("joined", "c")));
            g.edges
                .insert(7, plain_edge(1, 4, child_label("joined", "d")));
        }),
    }]
}

fn payload_literal_rows() -> Vec<LiteralLowering> {
    let renamed = |node: &mut Node| node.anchor = "renamed".into();
    let with_old_field = |nodes: &mut BTreeMap<u32, Node>| {
        nodes
            .get_mut(&1)
            .unwrap()
            .fields
            .insert("old".into(), Value::Int(7));
    };
    vec![
        LiteralLowering {
            name: "relabel node 3",
            tree: join_tree(),
            edit: TreeEdit::RelabelNode {
                id: 3,
                new_anchor: "renamed".into(),
            },
            delta: with_attr(
                with_attr(
                    GraphDelta::identity(),
                    AttrKey::Anchor(3),
                    AttrVal::Anchor("leaf".into()),
                    AttrVal::Anchor("renamed".into()),
                ),
                AttrKey::EdgeLabel(5),
                AttrVal::Label(fan_arm_label()),
                AttrVal::Label(SEdge::new("fan", "renamed", "hyper_label", Some("arm"))),
            ),
            embedding: literal_join_embedding(),
            final_tree: join_tree_with(|t| renamed(t.nodes.get_mut(&3).unwrap())),
            final_graph: join_graph_with(|g| {
                renamed(g.nodes.get_mut(&3).unwrap());
                g.edges.get_mut(&5).unwrap().label.tgt = "renamed".into();
            }),
        },
        LiteralLowering {
            name: "set field",
            tree: join_tree(),
            edit: TreeEdit::SetField {
                node_id: 1,
                field: "new".into(),
                value: Value::Bool(true),
            },
            delta: attribute_delta(
                AttrKey::Field(1, "new".into()),
                AttrVal::Field(None),
                AttrVal::Field(Some(Value::Bool(true))),
            ),
            embedding: literal_join_embedding(),
            final_tree: join_tree_with(|t| {
                t.nodes
                    .get_mut(&1)
                    .unwrap()
                    .fields
                    .insert("new".into(), Value::Bool(true));
            }),
            final_graph: join_graph_with(|g| {
                g.nodes
                    .get_mut(&1)
                    .unwrap()
                    .fields
                    .insert("new".into(), Value::Bool(true));
            }),
        },
        LiteralLowering {
            name: "remove field",
            tree: join_tree_with(|t| with_old_field(&mut t.nodes)),
            edit: TreeEdit::RemoveField {
                node_id: 1,
                field: "old".into(),
            },
            delta: attribute_delta(
                AttrKey::Field(1, "old".into()),
                AttrVal::Field(Some(Value::Int(7))),
                AttrVal::Field(None),
            ),
            embedding: literal_join_embedding(),
            final_tree: join_tree(),
            final_graph: literal_join_graph(),
        },
    ]
}

fn value_and_sequence_literal_rows() -> Vec<LiteralLowering> {
    let mut valued = Node::new(3, "leaf");
    valued.value = Some(Presence::Present(Value::Int(9)));
    vec![
        LiteralLowering {
            name: "join value into 3",
            tree: join_tree(),
            edit: TreeEdit::JoinFeatures {
                primary: 3,
                joined: Vec::new(),
                produce: valued.clone(),
            },
            delta: attribute_delta(
                AttrKey::Value(3),
                AttrVal::Presence(None),
                AttrVal::Presence(Some(Presence::Present(Value::Int(9)))),
            ),
            embedding: literal_join_embedding(),
            final_tree: join_tree_with(|t| {
                t.nodes.insert(3, valued.clone());
            }),
            final_graph: join_graph_with(|g| {
                g.nodes.insert(3, valued);
            }),
        },
        LiteralLowering {
            name: "move then set field",
            tree: join_tree(),
            edit: TreeEdit::Sequence(vec![
                TreeEdit::MoveSubtree {
                    node_id: 3,
                    new_parent: 1,
                    edge: child_label("feature", "moved"),
                },
                TreeEdit::SetField {
                    node_id: 1,
                    field: "f".into(),
                    value: Value::Int(7),
                },
            ]),
            delta: with_attr(
                added(
                    literal_deletions(&literal_join_graph(), [GElem::Edge(2)]),
                    vec![(
                        GElem::Edge(6),
                        GPayload::Edge(plain_edge(1, 3, child_label("feature", "moved"))),
                    )],
                ),
                AttrKey::Field(1, "f".into()),
                AttrVal::Field(None),
                AttrVal::Field(Some(Value::Int(7))),
            ),
            embedding: join_embedding_with(|e| {
                e.arcs.insert(3, 6);
                e.next_edge = 7;
            }),
            final_tree: join_tree_with(|t| {
                t.arcs.retain(|(_, child, _)| *child != 3);
                t.arcs.push((1, 3, child_label("feature", "moved")));
                t.nodes
                    .get_mut(&1)
                    .unwrap()
                    .fields
                    .insert("f".into(), Value::Int(7));
            }),
            final_graph: join_graph_with(|g| {
                g.edges.remove(&2);
                g.edges
                    .insert(6, plain_edge(1, 3, child_label("feature", "moved")));
                g.nodes
                    .get_mut(&1)
                    .unwrap()
                    .fields
                    .insert("f".into(), Value::Int(7));
            }),
        },
    ]
}

#[test]
fn every_tree_edit_variant_matches_a_hand_assembled_delta_embedding_tree_and_graph() {
    let rows: Vec<_> = structural_literal_rows()
        .into_iter()
        .chain(reparenting_literal_rows())
        .chain(fan_and_join_literal_rows())
        .chain(join_literal_rows())
        .chain(payload_literal_rows())
        .chain(value_and_sequence_literal_rows())
        .collect();
    let names: Vec<_> = rows.iter().map(|row| row.name).collect();
    assert_eq!(
        names,
        [
            "identity",
            "insert node 6",
            "delete node 4",
            "contract node 2",
            "move subtree 3",
            "insert fan",
            "delete fan",
            "join 2 into 1",
            "relabel node 3",
            "set field",
            "remove field",
            "join value into 3",
            "move then set field",
        ]
    );
    for row in rows {
        check_literal_lowering(row);
    }
}

fn assert_tree_lowering_refusal(
    tree: &Tree,
    embedding: &TreeEmbedding,
    edit_step: &TreeEdit,
    expected: &edit::TreeApplyError,
) {
    let mut lowered = embedding.clone();
    assert_eq!(
        edit::graph_delta_from_tree_edit(tree, &mut lowered, edit_step, None).as_ref(),
        Err(expected),
        "lowering {edit_step:?}"
    );
    assert_eq!(&lowered, embedding, "lowering {edit_step:?}");
    let mut applied = embedding.clone();
    assert_eq!(
        edit::apply_tree(edit_step, tree, &mut applied).as_ref(),
        Err(expected),
        "applying {edit_step:?}"
    );
    assert_eq!(&applied, embedding, "applying {edit_step:?}");
}

#[test]
fn tree_lowering_refuses_absent_duplicate_and_misfiled_nodes_exactly_without_mutation() {
    use edit::TreeApplyError as E;
    let tree = join_tree();
    let embedding = literal_join_embedding();
    let leaf = |id: u32| Node::new(id, "leaf");
    let label = || child_label("feature", "new");
    let cases = [
        (insert_leaf(99, 6), E::ParentNotFound(99)),
        (
            TreeEdit::MoveSubtree {
                node_id: 3,
                new_parent: 99,
                edge: label(),
            },
            E::ParentNotFound(99),
        ),
        (TreeEdit::DeleteNode { id: 99 }, E::NodeNotFound(99)),
        (TreeEdit::ContractNode { id: 99 }, E::NodeNotFound(99)),
        (
            TreeEdit::RelabelNode {
                id: 99,
                new_anchor: "x".into(),
            },
            E::NodeNotFound(99),
        ),
        (
            TreeEdit::SetField {
                node_id: 99,
                field: "f".into(),
                value: Value::Int(1),
            },
            E::NodeNotFound(99),
        ),
        (
            TreeEdit::RemoveField {
                node_id: 99,
                field: "f".into(),
            },
            E::NodeNotFound(99),
        ),
        (
            TreeEdit::MoveSubtree {
                node_id: 99,
                new_parent: 1,
                edge: label(),
            },
            E::NodeNotFound(99),
        ),
        (
            TreeEdit::JoinFeatures {
                primary: 99,
                joined: Vec::new(),
                produce: leaf(99),
            },
            E::NodeNotFound(99),
        ),
        (
            TreeEdit::JoinFeatures {
                primary: 1,
                joined: vec![99],
                produce: Node::new(1, "f"),
            },
            E::NodeNotFound(99),
        ),
        (insert_leaf(1, 1), E::DuplicateNodeId(1)),
        (
            TreeEdit::InsertNode {
                parent: 1,
                child_id: 6,
                node: leaf(7),
                edge: label(),
            },
            E::MisfiledNode {
                expected: 6,
                actual: 7,
            },
        ),
        (
            TreeEdit::JoinFeatures {
                primary: 1,
                joined: Vec::new(),
                produce: leaf(2),
            },
            E::MisfiledNode {
                expected: 1,
                actual: 2,
            },
        ),
    ];
    for (edit_step, expected) in &cases {
        assert_tree_lowering_refusal(&tree, &embedding, edit_step, expected);
    }
}

#[test]
fn tree_lowering_refuses_root_leaf_cycle_join_fan_and_id_preconditions_exactly() {
    use edit::TreeApplyError as E;
    let tree = join_tree();
    let embedding = literal_join_embedding();
    let leaf = |id: u32| Node::new(id, "leaf");
    let label = || child_label("feature", "new");
    let cases = [
        (TreeEdit::DeleteNode { id: 0 }, E::RootOperation(0)),
        (TreeEdit::ContractNode { id: 0 }, E::RootOperation(0)),
        (
            TreeEdit::MoveSubtree {
                node_id: 0,
                new_parent: 1,
                edge: label(),
            },
            E::RootOperation(0),
        ),
        (
            TreeEdit::JoinFeatures {
                primary: 1,
                joined: vec![0],
                produce: Node::new(1, "f"),
            },
            E::RootOperation(0),
        ),
        (TreeEdit::DeleteNode { id: 2 }, E::NotLeaf(2)),
        (
            TreeEdit::MoveSubtree {
                node_id: 2,
                new_parent: 3,
                edge: label(),
            },
            E::Cycle { node: 2, parent: 3 },
        ),
        (
            TreeEdit::MoveSubtree {
                node_id: 2,
                new_parent: 2,
                edge: label(),
            },
            E::Cycle { node: 2, parent: 2 },
        ),
        (
            TreeEdit::JoinFeatures {
                primary: 3,
                joined: vec![2],
                produce: leaf(3),
            },
            E::Cycle { node: 2, parent: 3 },
        ),
        (
            TreeEdit::JoinFeatures {
                primary: 1,
                joined: vec![1],
                produce: Node::new(1, "f"),
            },
            E::InvalidJoin(1),
        ),
        (
            TreeEdit::JoinFeatures {
                primary: 1,
                joined: vec![2, 2],
                produce: Node::new(1, "f"),
            },
            E::InvalidJoin(2),
        ),
        (
            TreeEdit::DeleteFan {
                hyper_edge: "absent".into(),
            },
            E::FanNotFound("absent".into()),
        ),
        (
            TreeEdit::InsertFan {
                fan: Fan {
                    parent: 1,
                    hyper_edge: "f".into(),
                    children: BTreeMap::from([("arm".into(), 99)]),
                },
            },
            E::NodeNotFound(99),
        ),
        (
            TreeEdit::InsertFan {
                fan: Fan {
                    parent: 99,
                    hyper_edge: "f".into(),
                    children: BTreeMap::new(),
                },
            },
            E::NodeNotFound(99),
        ),
        (insert_leaf(1, u32::MAX), E::IdsExhausted),
    ];
    for (edit_step, expected) in &cases {
        assert_tree_lowering_refusal(&tree, &embedding, edit_step, expected);
    }
}

#[test]
fn tree_lowering_refuses_exhausted_floors_invalid_embeddings_and_reserved_vocabulary() {
    use edit::TreeApplyError as E;
    let tree = join_tree();
    let embedding = literal_join_embedding();
    let label = || child_label("feature", "new");

    let edge_exhausted = join_embedding_with(|e| e.next_edge = u32::MAX);
    assert_tree_lowering_refusal(
        &tree,
        &edge_exhausted,
        &TreeEdit::MoveSubtree {
            node_id: 3,
            new_parent: 1,
            edge: label(),
        },
        &E::IdsExhausted,
    );
    let node_exhausted = join_embedding_with(|e| e.next_node = u32::MAX);
    assert_tree_lowering_refusal(
        &tree,
        &node_exhausted,
        &TreeEdit::InsertFan {
            fan: Fan {
                parent: 1,
                hyper_edge: "f".into(),
                children: BTreeMap::from([("arm".into(), 4)]),
            },
        },
        &E::IdsExhausted,
    );

    let swapped =
        join_embedding_with(|e| e.arcs = BTreeMap::from([(1, 1), (2, 0), (3, 2), (4, 3)]));
    let set_field = TreeEdit::SetField {
        node_id: 1,
        field: "f".into(),
        value: Value::Int(1),
    };
    assert_tree_lowering_refusal(&tree, &swapped, &set_field, &E::InvalidEmbedding);
    assert_eq!(
        edit::tree_edit_from_graph_delta(&tree, &swapped, &GraphDelta::identity(), None),
        Err(E::InvalidEmbedding)
    );

    let hyper_shaped =
        join_tree_with(|t| t.nodes.get_mut(&1).unwrap().shape = NodeShape::HyperEdge);
    let reserved_arc = join_tree_with(|t| t.arcs[0].2.kind = "hyper_parent".into());
    for reserved in [hyper_shaped, reserved_arc] {
        assert_tree_lowering_refusal(&reserved, &embedding, &set_field, &E::ReservedVocabulary);
        assert_eq!(
            edit::tree_edit_from_graph_delta(&reserved, &embedding, &GraphDelta::identity(), None),
            Err(E::ReservedVocabulary)
        );
    }
}

/// A literal little-endian byte layout, written without the crate's writer.
#[derive(Default)]
struct Bytes(Vec<u8>);

impl Bytes {
    fn tag(mut self, tag: u8) -> Self {
        self.0.push(tag);
        self
    }
    fn u32(mut self, value: u32) -> Self {
        self.0.extend_from_slice(&value.to_le_bytes());
        self
    }
    fn u64(mut self, value: u64) -> Self {
        self.0.extend_from_slice(&value.to_le_bytes());
        self
    }
    fn i64(mut self, value: i64) -> Self {
        self.0.extend_from_slice(&value.to_le_bytes());
        self
    }
    fn str(self, text: &str) -> Self {
        let mut out = self.u64(u64::try_from(text.len()).unwrap());
        out.0.extend_from_slice(text.as_bytes());
        out
    }
    /// A schema edge: source, target, kind, then an optional name.
    fn sedge(self, src: &str, tgt: &str, kind: &str, name: Option<&str>) -> Self {
        let out = self.str(src).str(tgt).str(kind);
        match name {
            None => out.tag(0),
            Some(name) => out.tag(1).str(name),
        }
    }
    /// A node with no value, discriminator, fields, position or annotations
    /// and the plain shape.
    fn bare_node(self, id: u32, anchor: &str) -> Self {
        self.u32(id)
            .str(anchor)
            .tag(0)
            .tag(0)
            .u64(0)
            .tag(0)
            .tag(0)
            .u64(0)
    }
}

fn literal_layout_graph_delta() -> GraphDelta {
    let mut added_node = Node::new(2, "n");
    added_node.value = Some(Presence::Null);
    added_node.discriminator = Some("x".into());
    added_node.fields.insert("k".into(), Value::Int(-1));
    added_node.position = Some(3);
    added_node.shape = NodeShape::List;
    let meta_after = NodeMeta {
        discriminator: Some("dd".into()),
        position: Some(2),
        shape: NodeShape::XmlElement("x".into()),
        annotations: BTreeMap::from([("k".into(), Value::Bool(false))]),
    };
    GraphDelta {
        del: BTreeMap::from([
            (GElem::Node(1), GPayload::Node(Node::new(1, "d"))),
            (
                GElem::Edge(1),
                GPayload::Edge(plain_edge(0, 1, SEdge::new("a", "d", "old", None))),
            ),
        ]),
        add: BTreeMap::from([
            (GElem::Node(2), GPayload::Node(added_node)),
            (
                GElem::Edge(2),
                GPayload::Edge(GEdge {
                    position: Some(4),
                    ..plain_edge(0, 2, SEdge::new("a", "n", "new", Some("m")))
                }),
            ),
        ]),
        attrs: BTreeMap::from([
            (
                AttrKey::Value(0),
                (
                    AttrVal::Presence(None),
                    AttrVal::Presence(Some(Presence::Present(Value::Int(5)))),
                ),
            ),
            (
                AttrKey::Field(0, "f".into()),
                (
                    AttrVal::Field(Some(Value::Bool(true))),
                    AttrVal::Field(None),
                ),
            ),
            (
                AttrKey::Meta(0),
                (
                    AttrVal::Meta(NodeMeta {
                        discriminator: None,
                        position: None,
                        shape: NodeShape::Plain,
                        annotations: BTreeMap::new(),
                    }),
                    AttrVal::Meta(meta_after),
                ),
            ),
            (
                AttrKey::Anchor(0),
                (AttrVal::Anchor("a".into()), AttrVal::Anchor("b".into())),
            ),
            (
                AttrKey::EdgeLabel(0),
                (
                    AttrVal::Label(SEdge::new("a", "b", "e", None)),
                    AttrVal::Label(SEdge::new("a", "b", "e", Some("n"))),
                ),
            ),
            (
                AttrKey::EdgePosition(0),
                (AttrVal::Position(Some(3)), AttrVal::Position(None)),
            ),
        ]),
    }
}

#[test]
fn canonical_graph_delta_matches_a_literal_layout_of_every_part_and_attribute_kind() {
    let delta = literal_layout_graph_delta();
    let expected = Bytes::default()
        .u64(1)
        .u64(1)
        .u64(1)
        .u64(1)
        .u64(6)
        .bare_node(1, "d")
        .u32(1)
        .u32(0)
        .u32(1)
        .sedge("a", "d", "old", None)
        .tag(0)
        .u32(2)
        .str("n")
        .tag(2)
        .tag(1)
        .str("x")
        .u64(1)
        .str("k")
        .tag(1)
        .i64(-1)
        .tag(1)
        .u32(3)
        .tag(1)
        .u64(0)
        .u32(2)
        .u32(0)
        .u32(2)
        .sedge("a", "n", "new", Some("m"))
        .tag(1)
        .u32(4)
        .tag(0)
        .u32(0)
        .tag(0)
        .tag(0)
        .tag(0)
        .tag(1)
        .tag(1)
        .i64(5)
        .tag(1)
        .u32(0)
        .str("f")
        .tag(1)
        .tag(1)
        .tag(0)
        .tag(1)
        .tag(1)
        .tag(0)
        .tag(2)
        .u32(0)
        .tag(2)
        .tag(0)
        .tag(0)
        .tag(0)
        .u64(0)
        .tag(2)
        .tag(1)
        .str("dd")
        .tag(1)
        .u32(2)
        .tag(2)
        .str("x")
        .u64(1)
        .str("k")
        .tag(0)
        .tag(0)
        .tag(3)
        .u32(0)
        .tag(3)
        .str("a")
        .tag(3)
        .str("b")
        .tag(4)
        .u32(0)
        .tag(4)
        .sedge("a", "b", "e", None)
        .tag(4)
        .sedge("a", "b", "e", Some("n"))
        .tag(5)
        .u32(0)
        .tag(5)
        .tag(1)
        .u32(3)
        .tag(5)
        .tag(0);
    assert_eq!(edit::canonical_graph_delta_bytes(&delta), Some(expected.0));
}

#[test]
fn canonical_table_delta_matches_a_literal_layout_of_every_part() {
    let deleted_pair = SEdge::new("t", "u", "fk", Some("p"));
    let added_pair = SEdge::new("u", "t", "fk", None);
    let delta = TableDelta {
        del: BTreeMap::from([
            (
                TElem::Row(1),
                TPayload::Row("t".into(), BTreeMap::from([("c".into(), Value::Int(1))])),
            ),
            (TElem::Pair(deleted_pair, 1, 0), TPayload::Pair),
        ]),
        add: BTreeMap::from([
            (TElem::Row(2), TPayload::Row("u".into(), BTreeMap::new())),
            (TElem::Pair(added_pair, 2, 0), TPayload::Pair),
        ]),
        attrs: BTreeMap::from([
            (
                TKey::Cell(0, "c".into()),
                (TVal::Cell(None), TVal::Cell(Some(Value::Str("s".into())))),
            ),
            (
                TKey::Table(0),
                (TVal::Table("t".into()), TVal::Table("v".into())),
            ),
        ]),
    };
    let expected = Bytes::default()
        .u64(1)
        .u64(1)
        .u64(1)
        .u64(1)
        .u64(1)
        .u64(1)
        .u32(1)
        .str("t")
        .u64(1)
        .str("c")
        .tag(1)
        .i64(1)
        .u32(2)
        .str("u")
        .u64(0)
        .sedge("t", "u", "fk", Some("p"))
        .u32(1)
        .u32(0)
        .sedge("u", "t", "fk", None)
        .u32(2)
        .u32(0)
        .u32(0)
        .str("c")
        .tag(0)
        .tag(1)
        .tag(3)
        .str("s")
        .u32(0)
        .str("t")
        .str("v");
    assert_eq!(edit::canonical_table_delta_bytes(&delta), Some(expected.0));
}

#[test]
fn canonical_tree_edit_matches_a_literal_layout_of_tags_one_through_ten_flattened() {
    let nested = TreeEdit::Sequence(vec![
        TreeEdit::Identity,
        all_primitive_tree_edits(),
        TreeEdit::Sequence(vec![TreeEdit::Identity]),
    ]);
    let expected = Bytes::default()
        .u64(10)
        .tag(1)
        .u32(0)
        .u32(11)
        .bare_node(11, "leaf")
        .sedge("root", "leaf", "child", Some("insert"))
        .tag(2)
        .u32(10)
        .tag(3)
        .u32(9)
        .tag(4)
        .u32(8)
        .str("renamed")
        .tag(5)
        .u32(7)
        .str("field")
        .tag(1)
        .i64(4)
        .tag(6)
        .u32(6)
        .str("removed")
        .tag(7)
        .u32(5)
        .u32(0)
        .sedge("root", "leaf", "child", Some("move"))
        .tag(8)
        .str("inserted-fan")
        .u32(0)
        .u64(1)
        .str("arm")
        .u32(5)
        .tag(9)
        .str("deleted-fan")
        .tag(10)
        .u32(4)
        .u64(2)
        .u32(3)
        .u32(2)
        .bare_node(4, "merged");
    assert_eq!(edit::canonical_tree_edit_bytes(&nested), Some(expected.0));
}

const fn ill_formed_temporal() -> Value {
    Value::Temporal(Temporal {
        date: None,
        time: None,
        separator: None,
        offset: Offset::Local,
        infinity: None,
    })
}

#[test]
fn canonical_graph_delta_refuses_every_ill_typed_or_unnormalized_part() {
    let edge_payload = GPayload::Edge(plain_edge(0, 0, SEdge::new("a", "a", "e", None)));
    let mut refused = Vec::new();
    let with = |change: &dyn Fn(&mut GraphDelta)| {
        let mut delta = GraphDelta::identity();
        change(&mut delta);
        delta
    };
    refused.push(with(&|d| {
        d.add.insert(GElem::Node(0), edge_payload.clone());
    }));
    refused.push(with(&|d| {
        d.add.insert(GElem::Edge(0), node(0, "a"));
    }));
    refused.push(with(&|d| {
        d.del.insert(GElem::Edge(0), node(0, "a"));
    }));
    refused.push(with(&|d| {
        d.attrs.insert(
            AttrKey::Anchor(0),
            (AttrVal::Field(None), AttrVal::Field(Some(Value::Int(1)))),
        );
    }));
    refused.push(with(&|d| {
        d.attrs.insert(
            AttrKey::EdgePosition(0),
            (
                AttrVal::Label(SEdge::new("a", "a", "e", None)),
                AttrVal::Label(SEdge::new("a", "a", "f", None)),
            ),
        );
    }));
    refused.push(with(&|d| {
        d.del.insert(GElem::Node(1), node(1, "a"));
        d.attrs.insert(
            AttrKey::Anchor(1),
            (AttrVal::Anchor("a".into()), AttrVal::Anchor("b".into())),
        );
    }));
    refused.push(with(&|d| {
        d.add.insert(GElem::Node(1), node(1, "a"));
        d.attrs.insert(
            AttrKey::Value(1),
            (
                AttrVal::Presence(None),
                AttrVal::Presence(Some(Presence::Null)),
            ),
        );
    }));
    refused.push(with(&|d| {
        d.del.insert(GElem::Edge(0), edge_payload.clone());
        d.attrs.insert(
            AttrKey::EdgePosition(0),
            (AttrVal::Position(None), AttrVal::Position(Some(1))),
        );
    }));
    for equal in [
        AttrVal::Presence(None),
        AttrVal::Field(Some(Value::Int(1))),
        AttrVal::Meta(NodeMeta {
            discriminator: None,
            position: None,
            shape: NodeShape::Plain,
            annotations: BTreeMap::new(),
        }),
    ] {
        let key = match equal {
            AttrVal::Presence(_) => AttrKey::Value(0),
            AttrVal::Field(_) => AttrKey::Field(0, "f".into()),
            _ => AttrKey::Meta(0),
        };
        refused.push(attribute_delta(key, equal.clone(), equal));
    }
    for (key, equal) in [
        (
            AttrKey::EdgeLabel(0),
            AttrVal::Label(SEdge::new("a", "a", "e", None)),
        ),
        (AttrKey::EdgePosition(0), AttrVal::Position(Some(1))),
    ] {
        refused.push(attribute_delta(key, equal.clone(), equal));
    }
    refused.push(with(&|d| {
        d.del.insert(GElem::Node(1), node(2, "a"));
    }));
    refused.push(with(&|d| {
        d.add.insert(GElem::Node(1), node(2, "a"));
    }));
    assert_eq!(refused.len(), 15);
    for delta in &refused {
        assert_eq!(edit::canonical_graph_delta_bytes(delta), None, "{delta:?}");
        assert_eq!(edit::graph_delta_digest(delta), None, "{delta:?}");
    }
}

#[test]
fn canonical_graph_delta_refuses_an_equal_anchor_change_and_an_attribute_of_an_added_edge() {
    let equal_anchor = attribute_delta(
        AttrKey::Anchor(0),
        AttrVal::Anchor("a".into()),
        AttrVal::Anchor("a".into()),
    );
    let mut added_edge_label = attribute_delta(
        AttrKey::EdgeLabel(0),
        AttrVal::Label(SEdge::new("a", "a", "e", None)),
        AttrVal::Label(SEdge::new("a", "a", "f", None)),
    );
    added_edge_label.add.insert(
        GElem::Edge(0),
        GPayload::Edge(plain_edge(0, 0, SEdge::new("a", "a", "e", None))),
    );
    for delta in [equal_anchor, added_edge_label] {
        assert_eq!(edit::canonical_graph_delta_bytes(&delta), None, "{delta:?}");
        assert_eq!(edit::graph_delta_digest(&delta), None, "{delta:?}");
    }
}

#[test]
fn canonical_graph_delta_refuses_ill_formed_values_nested_in_every_payload_and_attribute() {
    let mut refused = Vec::new();
    let mut temporal_field = Node::new(1, "a");
    temporal_field
        .fields
        .insert("t".into(), ill_formed_temporal());
    let mut temporal_value = Node::new(1, "a");
    temporal_value.value = Some(Presence::Present(ill_formed_temporal()));
    let mut temporal_annotation = Node::new(1, "a");
    temporal_annotation
        .annotations
        .insert("t".into(), ill_formed_temporal());
    for payload in [temporal_field, temporal_value, temporal_annotation] {
        let mut deleted = GraphDelta::identity();
        deleted
            .del
            .insert(GElem::Node(1), GPayload::Node(payload.clone()));
        refused.push(deleted);
        let mut addition = GraphDelta::identity();
        addition.add.insert(GElem::Node(1), GPayload::Node(payload));
        refused.push(addition);
    }
    refused.push(attribute_delta(
        AttrKey::Value(0),
        AttrVal::Presence(None),
        AttrVal::Presence(Some(Presence::Present(ill_formed_temporal()))),
    ));
    refused.push(attribute_delta(
        AttrKey::Meta(0),
        AttrVal::Meta(NodeMeta {
            discriminator: None,
            position: None,
            shape: NodeShape::Plain,
            annotations: BTreeMap::new(),
        }),
        AttrVal::Meta(NodeMeta {
            discriminator: None,
            position: None,
            shape: NodeShape::Plain,
            annotations: BTreeMap::from([("t".into(), ill_formed_temporal())]),
        }),
    ));
    assert_eq!(refused.len(), 8);
    for delta in &refused {
        assert_eq!(edit::canonical_graph_delta_bytes(delta), None, "{delta:?}");
        assert_eq!(edit::graph_delta_digest(delta), None, "{delta:?}");
    }
}

#[test]
fn canonical_table_delta_refuses_every_ill_typed_or_unnormalized_part() {
    let pair = TElem::Pair(SEdge::new("t", "t", "fk", None), 0, 0);
    let row = |cells: BTreeMap<String, Value>| TPayload::Row("t".into(), cells);
    let mut refused = Vec::new();
    let mut cross_payload = TableDelta::identity();
    cross_payload.del.insert(pair, row(BTreeMap::new()));
    refused.push(cross_payload);
    let mut cross_attribute = TableDelta::identity();
    cross_attribute.attrs.insert(
        TKey::Cell(0, "c".into()),
        (TVal::Table("a".into()), TVal::Table("b".into())),
    );
    refused.push(cross_attribute);
    let mut deleted_owner = TableDelta::identity();
    deleted_owner
        .del
        .insert(TElem::Row(0), row(BTreeMap::new()));
    deleted_owner.attrs.insert(
        TKey::Cell(0, "c".into()),
        (TVal::Cell(None), TVal::Cell(Some(Value::Int(1)))),
    );
    refused.push(deleted_owner);
    let mut added_owner = TableDelta::identity();
    added_owner.add.insert(TElem::Row(0), row(BTreeMap::new()));
    added_owner.attrs.insert(
        TKey::Table(0),
        (TVal::Table("a".into()), TVal::Table("b".into())),
    );
    refused.push(added_owner);
    let mut equal_cell = TableDelta::identity();
    equal_cell.attrs.insert(
        TKey::Cell(0, "c".into()),
        (
            TVal::Cell(Some(Value::Int(1))),
            TVal::Cell(Some(Value::Int(1))),
        ),
    );
    refused.push(equal_cell);
    let mut equal_table = TableDelta::identity();
    equal_table.attrs.insert(
        TKey::Table(0),
        (TVal::Table("a".into()), TVal::Table("a".into())),
    );
    refused.push(equal_table);
    for deleted in [true, false] {
        let mut temporal_row = TableDelta::identity();
        let part = if deleted {
            &mut temporal_row.del
        } else {
            &mut temporal_row.add
        };
        part.insert(
            TElem::Row(0),
            row(BTreeMap::from([("t".into(), ill_formed_temporal())])),
        );
        refused.push(temporal_row);
    }
    let mut temporal_old_cell = TableDelta::identity();
    temporal_old_cell.attrs.insert(
        TKey::Cell(0, "c".into()),
        (TVal::Cell(Some(ill_formed_temporal())), TVal::Cell(None)),
    );
    refused.push(temporal_old_cell);
    assert_eq!(refused.len(), 9);
    for delta in &refused {
        assert_eq!(edit::canonical_table_delta_bytes(delta), None, "{delta:?}");
        assert_eq!(edit::table_delta_digest(delta), None, "{delta:?}");
    }
}

#[test]
fn canonical_tree_edit_refuses_misfiled_nodes_and_nested_ill_formed_values() {
    let mut temporal_node = Node::new(6, "leaf");
    temporal_node
        .fields
        .insert("t".into(), ill_formed_temporal());
    let mut temporal_produce = Node::new(1, "feature");
    temporal_produce.value = Some(Presence::Present(ill_formed_temporal()));
    let refused = [
        TreeEdit::InsertNode {
            parent: 1,
            child_id: 6,
            node: Node::new(7, "leaf"),
            edge: inserted_label(),
        },
        TreeEdit::JoinFeatures {
            primary: 1,
            joined: vec![2],
            produce: Node::new(2, "feature"),
        },
        TreeEdit::Sequence(vec![
            TreeEdit::Identity,
            TreeEdit::Sequence(vec![TreeEdit::InsertNode {
                parent: 1,
                child_id: 6,
                node: temporal_node,
                edge: inserted_label(),
            }]),
        ]),
        TreeEdit::Sequence(vec![
            TreeEdit::DeleteNode { id: 4 },
            TreeEdit::JoinFeatures {
                primary: 1,
                joined: Vec::new(),
                produce: temporal_produce,
            },
        ]),
        TreeEdit::Sequence(vec![TreeEdit::Sequence(vec![TreeEdit::SetField {
            node_id: 1,
            field: "t".into(),
            value: ill_formed_temporal(),
        }])]),
    ];
    for edit_step in &refused {
        assert_eq!(
            edit::canonical_tree_edit_bytes(edit_step),
            None,
            "{edit_step:?}"
        );
        assert_eq!(edit::tree_edit_digest(edit_step), None, "{edit_step:?}");
    }
}

fn single_node_graph(anchor: &str) -> Graph {
    Graph {
        nodes: BTreeMap::from([(0, Node::new(0, anchor))]),
        edges: BTreeMap::new(),
    }
}

#[test]
fn single_anchor_lens_commutes_both_edit_squares_with_literal_deltas_and_complements() {
    let get = |state: &Graph| -> Result<(Graph, String), &'static str> {
        let anchor = &state.nodes[&0].anchor;
        Ok((single_node_graph(&format!("view:{anchor}")), anchor.clone()))
    };
    let put =
        |_: &Graph, view: &Graph, complement: &String| -> Result<(Graph, String), &'static str> {
            let anchor = view.nodes[&0]
                .anchor
                .strip_prefix("view:")
                .ok_or("not a view anchor")?;
            Ok((single_node_graph(anchor), format!("{complement}->{anchor}")))
        };
    let source = single_node_graph("a");
    let view = single_node_graph("view:a");
    let edited_source = single_node_graph("b");
    let edited_view = single_node_graph("view:b");
    let source_delta = attribute_delta(
        AttrKey::Anchor(0),
        AttrVal::Anchor("a".into()),
        AttrVal::Anchor("b".into()),
    );
    let view_delta = attribute_delta(
        AttrKey::Anchor(0),
        AttrVal::Anchor("view:a".into()),
        AttrVal::Anchor("view:b".into()),
    );

    let translated_get = edit::get_edit(&get, &source, &source_delta).unwrap();
    assert_eq!(translated_get.delta, view_delta);
    assert_eq!(translated_get.complement, "b");
    assert_eq!(
        edit::apply(&source_delta, &source),
        Ok(edited_source.clone())
    );
    assert_eq!(
        get(&edited_source),
        Ok((edited_view.clone(), "b".to_owned()))
    );
    assert_eq!(edit::apply(&view_delta, &view), Ok(edited_view.clone()));

    let complement = "a".to_owned();
    let translated_put = edit::put_edit(&put, &source, &view, &complement, &view_delta).unwrap();
    assert_eq!(translated_put.delta, source_delta);
    assert_eq!(translated_put.complement, "a->b");
    assert_eq!(
        put(&source, &edited_view, &complement),
        Ok((edited_source.clone(), "a->b".to_owned()))
    );
    assert_eq!(
        edit::apply(&translated_put.delta, &source),
        Ok(edited_source)
    );
}

/// The 25 atomic graph deltas of the finite carrier in their stated order,
/// written out: the identity; for each anchor `a`, `b` and each of no value
/// and the value 0, the node deletion then the node addition; for each label,
/// the edge deletion then the edge addition; then the forward and reverse
/// change of anchor, value, field, shape, edge label and edge position.
fn literal_atomic_alphabet() -> Vec<GraphDelta> {
    let mut alphabet = vec![GraphDelta::identity()];
    for anchor in ["a", "b"] {
        for value in [None, Some(Presence::Present(Value::Int(0)))] {
            let mut payload = Node::new(0, anchor);
            payload.value = value;
            let mut deletion = GraphDelta::identity();
            deletion
                .del
                .insert(GElem::Node(0), GPayload::Node(payload.clone()));
            let mut addition = GraphDelta::identity();
            addition.add.insert(GElem::Node(0), GPayload::Node(payload));
            alphabet.extend([deletion, addition]);
        }
    }
    for name in [None, Some("x")] {
        let payload = GPayload::Edge(plain_edge(0, 0, SEdge::new("a", "a", "e", name)));
        let mut deletion = GraphDelta::identity();
        deletion.del.insert(GElem::Edge(0), payload.clone());
        let mut addition = GraphDelta::identity();
        addition.add.insert(GElem::Edge(0), payload);
        alphabet.extend([deletion, addition]);
    }
    let shape = |shape| {
        AttrVal::Meta(NodeMeta {
            discriminator: None,
            position: None,
            shape,
            annotations: BTreeMap::new(),
        })
    };
    let changes = [
        (
            AttrKey::Anchor(0),
            AttrVal::Anchor("a".into()),
            AttrVal::Anchor("b".into()),
        ),
        (
            AttrKey::Value(0),
            AttrVal::Presence(None),
            AttrVal::Presence(Some(Presence::Present(Value::Int(0)))),
        ),
        (
            AttrKey::Field(0, "f".into()),
            AttrVal::Field(None),
            AttrVal::Field(Some(Value::Int(0))),
        ),
        (
            AttrKey::Meta(0),
            shape(NodeShape::Plain),
            shape(NodeShape::HyperEdge),
        ),
        (
            AttrKey::EdgeLabel(0),
            AttrVal::Label(SEdge::new("a", "a", "e", None)),
            AttrVal::Label(SEdge::new("a", "a", "e", Some("x"))),
        ),
        (
            AttrKey::EdgePosition(0),
            AttrVal::Position(None),
            AttrVal::Position(Some(0)),
        ),
    ];
    for (key, first, second) in changes {
        alphabet.push(attribute_delta(key.clone(), first.clone(), second.clone()));
        alphabet.push(attribute_delta(key, second, first));
    }
    alphabet
}

#[test]
fn graph_delta_sequences_have_literal_membership_and_positions() {
    let alphabet = literal_atomic_alphabet();
    assert_eq!(alphabet.len(), 25);
    let deltas = |word: &[carrier::GraphDeltaCase]| -> Vec<GraphDelta> {
        word.iter().map(|case| case.delta.clone()).collect()
    };
    let sequences = carrier::graph_delta_sequences();
    assert_eq!(sequences.len(), 1 + 25 + 625 + 15_625);
    assert!(sequences[0].is_empty());
    for (i, first) in alphabet.iter().enumerate() {
        assert_eq!(
            deltas(&sequences[1 + i]),
            std::slice::from_ref(first),
            "singleton {i}"
        );
        for (j, second) in alphabet.iter().enumerate() {
            assert_eq!(
                deltas(&sequences[26 + 25 * i + j]),
                [first.clone(), second.clone()],
                "pair ({i}, {j})"
            );
            for (k, third) in alphabet.iter().enumerate() {
                assert_eq!(
                    deltas(&sequences[651 + 625 * i + 25 * j + k]),
                    [first.clone(), second.clone(), third.clone()],
                    "triple ({i}, {j}, {k})"
                );
            }
        }
    }
    assert_eq!(
        sequences.last().map(|word| deltas(word)),
        Some(vec![alphabet[24].clone(); 3])
    );
}
