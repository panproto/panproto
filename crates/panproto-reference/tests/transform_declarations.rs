#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Transform declaration domains, identities, and separate finite carriers.

use std::collections::{BTreeMap, BTreeSet};

use panproto_reference::migration::Migration;
use panproto_reference::model::SEdge;
use panproto_reference::pages::representation::canonical::ComplementShape;
use panproto_reference::restrict::{self, NoValueLayer, PlanError};
use panproto_reference::schema::Schema;

fn writing(source: &str, target: &str, fields: &[&str]) -> Migration {
    Migration {
        surviving_verts: BTreeSet::from([target.to_owned()]),
        vertex_remap: BTreeMap::from([(source.to_owned(), target.to_owned())]),
        transformed: BTreeSet::from([source.to_owned()]),
        output_fields: BTreeMap::from([(
            source.to_owned(),
            fields.iter().map(|field| (*field).to_owned()).collect(),
        )]),
        ..Migration::default()
    }
}

#[test]
fn transform_outputs_follow_target_attribute_declarations() {
    let source = Schema::default().vertex("source", "obj");
    let target = Schema::default()
        .vertex("target", "obj")
        .vertex("value", "int")
        .edge("target", "value", "arbitrary", Some("field"));
    let migration = writing("source", "target", &["field"]);
    assert!(restrict::compile(&source, &target, &migration).is_ok());

    let missing = writing("source", "target", &["missing"]);
    assert_eq!(
        restrict::compile(&source, &target, &missing),
        Err(PlanError::UndeclaredTransformField {
            source_vertex: "source".to_owned(),
            target_vertex: "target".to_owned(),
            field: "missing".to_owned(),
        })
    );
    let unnamed = Schema::default()
        .vertex("target", "obj")
        .vertex("field", "int")
        .edge("target", "field", "arbitrary", None);
    assert!(restrict::compile(&source, &unnamed, &migration).is_ok());
    let structured = target.edge("value", "value", "inner", None);
    assert_eq!(
        restrict::compile(&source, &structured, &migration),
        Err(PlanError::UndeclaredTransformField {
            source_vertex: "source".to_owned(),
            target_vertex: "target".to_owned(),
            field: "field".to_owned(),
        })
    );
}

#[test]
fn transform_output_refusals_keep_anchor_and_write_order() {
    let schema = Schema::default().vertex("a", "obj").vertex("b", "obj");
    let mut migration = writing("b", "b", &["first", "second"]);
    migration.surviving_verts.insert("a".to_owned());
    migration.transformed.insert("a".to_owned());
    migration
        .output_fields
        .insert("a".to_owned(), vec!["z".to_owned(), "a".to_owned()]);
    assert_eq!(
        restrict::compile(&schema, &schema, &migration),
        Err(PlanError::UndeclaredTransformField {
            source_vertex: "a".to_owned(),
            target_vertex: "a".to_owned(),
            field: "z".to_owned(),
        })
    );
    migration.surviving_verts.clear();
    assert!(restrict::compile(&schema, &schema, &migration).is_ok());

    let unknown = writing("absent", "a", &["missing"]);
    assert_eq!(
        restrict::compile(&schema, &schema, &unknown),
        Err(PlanError::UndeclaredTransformField {
            source_vertex: "absent".to_owned(),
            target_vertex: "a".to_owned(),
            field: "missing".to_owned(),
        })
    );
}

#[test]
fn structural_plan_refusals_precede_transform_outputs() {
    let edge = SEdge::new("a", "b", "edge", None);
    let schema = Schema::default()
        .vertex("a", "obj")
        .vertex("b", "obj")
        .edge("a", "b", "edge", None);
    let mut migration = writing("a", "a", &["missing"]);
    migration.surviving_verts.insert("b".to_owned());
    let incoherent = SEdge::new("b", "a", "edge", None);
    migration
        .edge_remap
        .insert(edge.clone(), incoherent.clone());
    assert_eq!(
        restrict::compile(&schema, &schema, &migration),
        Err(PlanError::IncoherentEdgeImage {
            source_edge: Box::new(edge),
            image: Box::new(incoherent),
        })
    );
}

#[test]
fn declaration_refusals_precede_structural_and_output_checks() {
    use panproto_reference::schema::{KeyScope, TargetSet};
    let schema = Schema::default().vertex("a", "obj");
    let mut invalid = schema.clone();
    invalid.target_sets.insert(
        "empty".to_owned(),
        TargetSet {
            members: Vec::new(),
            scope: KeyScope::Global,
        },
    );
    let migration = writing("a", "a", &["missing"]);
    assert!(matches!(
        restrict::compile(&invalid, &invalid, &migration),
        Err(PlanError::SourceDeclarations(_))
    ));
    assert!(matches!(
        restrict::compile(&schema, &invalid, &migration),
        Err(PlanError::TargetDeclarations(_))
    ));
}

#[test]
fn empty_output_metadata_preserves_the_historical_migration_encoding() {
    let namespace = b"panproto-reference/restrict-plan-migration/v1";
    let mut expected = u64::try_from(namespace.len())
        .unwrap()
        .to_le_bytes()
        .to_vec();
    expected.extend_from_slice(namespace);
    for tag in 0..9 {
        expected.push(tag);
        expected.extend_from_slice(&0_u64.to_le_bytes());
    }
    assert_eq!(
        panproto_reference::table::canonical_migration_bytes(&Migration::default()),
        expected
    );
}

#[test]
fn written_fields_bind_plan_and_lens_identities() {
    let schema = Schema::default()
        .vertex("a", "obj")
        .vertex("value", "int")
        .edge("a", "value", "prop", Some("first"))
        .edge("a", "value", "prop", Some("second"));
    let first = writing("a", "a", &["first", "second"]);
    let second = writing("a", "a", &["second", "first"]);
    let plan = restrict::compile(&schema, &schema, &first).unwrap();
    assert!(matches!(
        panproto_reference::lens::GraphLens::new(
            &schema,
            &schema,
            &second,
            &plan,
            &BTreeMap::new(),
            &NoValueLayer,
        ),
        Err(PlanError::MigrationMismatch { .. })
    ));
    assert_ne!(
        panproto_reference::table::canonical_migration_bytes(&first),
        panproto_reference::table::canonical_migration_bytes(&second),
        "ordered output metadata binds table identities"
    );
    assert_ne!(
        panproto_reference::lens::graph_lens_identity(
            &schema,
            &schema,
            &first,
            ComplementShape::Graph
        ),
        panproto_reference::lens::graph_lens_identity(
            &schema,
            &schema,
            &second,
            ComplementShape::Graph
        ),
        "ordered output metadata binds graph and tree identities"
    );
}

#[test]
fn table_interpretations_declare_structured_transform_outputs() {
    let schema = Schema::default()
        .vertex("a", "obj")
        .vertex("value", "structured")
        .vertex("child", "int")
        .edge("a", "value", "prop", Some("field"))
        .edge("value", "child", "inner", Some("child"));
    let mut migration = writing("a", "a", &["field"]);
    migration.surviving_verts = schema.vertices.keys().cloned().collect();
    migration.surviving_edges = schema.edges.clone();
    let boundary = panproto_reference::rel::classify(
        &schema,
        &BTreeSet::from(["structured".to_owned(), "int".to_owned()]),
        &BTreeSet::from(["obj".to_owned()]),
    )
    .unwrap();
    assert!(matches!(
        restrict::compile(&schema, &schema, &migration),
        Err(PlanError::UndeclaredTransformField { .. })
    ));
    assert!(
        panproto_reference::table::compile_for_tables(
            &schema, &schema, &boundary, &boundary, &migration
        )
        .is_ok()
    );
}

#[test]
fn table_functor_refusals_precede_transform_outputs() {
    let source = Schema::default().vertex("a", "obj");
    let target = Schema::default().vertex("value", "int");
    let migration = writing("a", "value", &["f"]);
    let classify = |schema: &Schema| {
        panproto_reference::rel::classify(
            schema,
            &BTreeSet::from(["int".to_owned()]),
            &BTreeSet::from(["obj".to_owned()]),
        )
        .unwrap()
    };
    assert_eq!(
        restrict::compile(&source, &target, &migration),
        Err(PlanError::UndeclaredTransformField {
            source_vertex: "a".to_owned(),
            target_vertex: "value".to_owned(),
            field: "f".to_owned(),
        })
    );
    assert_eq!(
        panproto_reference::table::compile_for_tables(
            &source,
            &target,
            &classify(&source),
            &classify(&target),
            &migration,
        ),
        Err(panproto_reference::table::TablePlanError::Functor(
            Box::new(
                panproto_reference::rel::FunctorError::VertexImageNotATable {
                    vertex: "a".to_owned(),
                    image: "value".to_owned(),
                }
            )
        ))
    );
}

#[test]
fn declared_transform_layers_preserve_other_source_anchors() {
    use panproto_reference::enumerate::lenses::{
        C11AddField, C11DropField, C11OpTerm, C11ReadField,
    };
    use panproto_reference::model::{Graph, Node};
    use panproto_reference::restrict::ValueLayer;
    use panproto_reference::value::Value;

    let layers: [&dyn ValueLayer; 4] = [&C11AddField, &C11DropField, &C11ReadField, &C11OpTerm];
    let old_fields = BTreeMap::from([("old".to_owned(), Value::Int(9))]);
    for fields in [
        BTreeMap::new(),
        BTreeMap::from([("f".to_owned(), Value::Int(1))]),
    ] {
        let mut node = Node::new(0, "value");
        node.fields = fields;
        let source = Graph {
            nodes: [(0, node.clone())].into(),
            ..Graph::default()
        };
        for layer in layers {
            let expected = (node.fields.clone(), node.value.clone());
            assert_eq!(layer.transform(&source, &node), Ok(expected.clone()));
            assert_eq!(
                layer.restore(&node, Some(&old_fields), Some(&None)),
                Ok(expected)
            );
        }
    }
}

#[test]
fn declared_transform_layers_restore_only_their_field_coordinate() {
    use panproto_reference::enumerate::lenses::{
        C11AddField, C11DropField, C11OpTerm, C11ReadField,
    };
    use panproto_reference::model::Node;
    use panproto_reference::restrict::ValueLayer;
    use panproto_reference::value::{Presence, Value};

    for (layer, original_f, view_f, expected_f) in [
        (&C11AddField as &dyn ValueLayer, None, Some(0), None),
        (&C11DropField, Some(1), None, Some(1)),
        (&C11ReadField, Some(1), Some(2), Some(2)),
        (&C11OpTerm, Some(1), Some(0), Some(1)),
    ] {
        let mut original = BTreeMap::from([("other".to_owned(), Value::Int(1))]);
        if let Some(value) = original_f {
            original.insert("f".to_owned(), Value::Int(value));
        }
        let mut view = Node::new(0, "a");
        view.fields.insert("other".to_owned(), Value::Int(2));
        if let Some(value) = view_f {
            view.fields.insert("f".to_owned(), Value::Int(value));
        }
        view.value = Some(Presence::Present(Value::Int(9)));
        let mut expected = BTreeMap::from([("other".to_owned(), Value::Int(2))]);
        if let Some(value) = expected_f {
            expected.insert("f".to_owned(), Value::Int(value));
        }
        assert_eq!(
            layer.restore(
                &view,
                Some(&original),
                Some(&Some(Presence::Present(Value::Int(7)))),
            ),
            Ok((expected, view.value.clone())),
        );
    }
}

#[test]
fn drop_layer_refuses_reinsertion_of_its_erased_field() {
    use panproto_reference::enumerate::lenses::C11DropField;
    use panproto_reference::model::Node;
    use panproto_reference::restrict::{PutFieldRefusal, ValueLayer};
    use panproto_reference::value::Value;

    let mut view = Node::new(0, "a");
    view.fields.insert("f".to_owned(), Value::Int(0));
    let original = BTreeMap::from([("f".to_owned(), Value::Int(1))]);
    for recorded in [None, Some(&original)] {
        assert_eq!(
            C11DropField.restore(&view, recorded, None),
            Err(PutFieldRefusal::TransformNotInvertible {
                field: "f".to_owned(),
            })
        );
    }
}

#[test]
fn declared_transform_carrier_has_separate_exact_counts() {
    use panproto_reference::enumerate::lenses::{
        c11_stages, declared_transform_count_certificate, declared_transform_graphs,
        declared_transform_stages, declared_transform_tables, declared_transform_trees,
    };
    let certificate = declared_transform_count_certificate();
    assert_eq!(c11_stages().len(), 31);
    assert_eq!(declared_transform_stages().len(), certificate.stages);
    assert_eq!(
        [
            declared_transform_graphs().len(),
            declared_transform_trees().len(),
            declared_transform_tables().len()
        ],
        certificate.sources
    );
    assert_eq!(certificate.atomic_cases, 145);
    assert_eq!(certificate.words, 155);
    assert_eq!(certificate.path_cases, 4_495);
}

#[test]
fn undeclared_transform_original_word_scope_remains_fixed() {
    use panproto_reference::enumerate::lenses::{
        c11_fan_schema, c11_graph_state_count, c11_path_count_certificate, c11_stages,
        c11_table_state_count, c11_tree_state_count,
    };
    let stages = c11_stages();
    let mut layer: Vec<Vec<usize>> = vec![Vec::new()];
    let mut all = [0; 3];
    let mut composable = [0; 3];
    let mut affected_words = [0; 3];
    let mut affected_sources = [[0; 3]; 3];
    for length in 0..3 {
        layer = layer
            .iter()
            .flat_map(|prefix| {
                (0..stages.len()).map(move |index| {
                    let mut word = prefix.clone();
                    word.push(index);
                    word
                })
            })
            .collect();
        all[length] = layer.len();
        for word in &layer {
            if !word
                .windows(2)
                .all(|pair| stages[pair[0]].target == stages[pair[1]].source)
            {
                continue;
            }
            composable[length] += 1;
            if !word
                .iter()
                .any(|index| !stages[*index].migration.output_fields.is_empty())
            {
                continue;
            }
            affected_words[length] += 1;
            let source = &stages[word[0]].source;
            assert_ne!(source, &c11_fan_schema());
            affected_sources[length] = [
                affected_sources[length][0] + c11_graph_state_count(source),
                affected_sources[length][1] + c11_tree_state_count(source),
                affected_sources[length][2] + c11_table_state_count(source),
            ];
        }
    }
    let certificate = c11_path_count_certificate();
    assert_eq!(all, certificate.words_by_length);
    assert_eq!(composable, certificate.composable_by_length);
    assert_eq!(affected_words[0], 3);
    assert_eq!(affected_sources[0], [45, 6, 45]);
    println!(
        "original words containing undeclared writes by length: {affected_words:?}; potential graph/tree/table refusal sources by length: {affected_sources:?}"
    );
}
