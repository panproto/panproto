//! Independent validation witnesses for the native tree put domain (L.5/L.9).
//!
//! `panproto-reference` supplies every expected report coordinate. The test
//! constructs the production graph and tree routes separately, compares their
//! full ordered reports with that external descriptor, and checks both puts.

#![allow(clippy::expect_used)]

use std::collections::HashMap;

use panproto_inst::{
    CompiledMigration, Fan, FanViolation, GInstance, Node, PutDomainError, Value, ViewRef,
    WInstance,
};
use panproto_lens::graph_lens;
use panproto_lens::{Lens, LensError};
use panproto_reference::enumerate::lenses::{
    TreeValidationCase, for_each_tree_validation_case, tree_validation_case_count_certificate,
};
use panproto_schema::{Edge, Key, KeyPart, KeyRole, KeyScope, Protocol, Schema, SchemaBuilder};
use proptest::prelude::*;

fn edge(src: &str, tgt: &str, name: &str) -> Edge {
    Edge {
        src: src.into(),
        tgt: tgt.into(),
        kind: "rel".into(),
        name: Some(name.into()),
    }
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

fn first_put_error(result: Result<impl Sized, LensError>) -> PutDomainError {
    match result {
        Err(LensError::PutDomain(error)) => *error,
        Err(error) => panic!("expected a typed put-domain refusal, got {error:?}"),
        Ok(_) => panic!("expected put to refuse the edited view"),
    }
}

fn assert_square_refuses(
    lens: &Lens,
    tree_view: &WInstance,
    tree_complement: &panproto_lens::Complement,
    graph_view: &GInstance,
    graph_complement: &panproto_inst::GraphComplement,
    expected: &[PutDomainError],
) {
    let tree_report = panproto_lens::asymmetric::check_put_domain(lens, tree_view, tree_complement)
        .expect_err("tree view is outside the put domain");
    let graph_report = graph_lens::check_put_domain(lens, graph_view, graph_complement)
        .expect_err("graph view is outside the put domain");
    assert_eq!(tree_report, expected);
    assert_eq!(graph_report, expected);
    assert_eq!(tree_report, graph_report, "full ordered reports differ");
    assert_eq!(
        first_put_error(panproto_lens::put(lens, tree_view, tree_complement)),
        expected[0]
    );
    assert_eq!(
        first_put_error(graph_lens::put(lens, graph_view, graph_complement)),
        expected[0]
    );
}

fn required_deletion(expected_elem: u32, expected_field: &str) {
    let relation = edge("a", "b", "b");
    let mut schema = SchemaBuilder::new(&Protocol::default())
        .vertex("a", "record", None)
        .expect("a")
        .vertex("b", "record", None)
        .expect("b")
        .edge("a", "b", "rel", Some("b"))
        .expect("edge")
        .build()
        .expect("schema");
    schema.required.insert("a".into(), vec![relation.clone()]);
    let lens = identity_lens(&schema);
    let source = WInstance::new(
        HashMap::from([(0, Node::new(0, "a")), (1, Node::new(1, "b"))]),
        vec![(0, 1, relation)],
        Vec::new(),
        0,
        "a".into(),
    );
    let source_graph =
        GInstance::from_wtype_with_schema(&source, &schema).expect("schema-valid source graph");
    let (tree_view, tree_complement) = panproto_lens::get(&lens, &source).expect("tree get");
    let (mut graph_view, graph_complement) =
        graph_lens::get(&lens, &source_graph).expect("graph get");
    let edited_tree = WInstance::new(
        HashMap::from([(0, tree_view.nodes[&0].clone())]),
        Vec::new(),
        Vec::new(),
        0,
        "a".into(),
    );
    let required_edge = graph_view
        .edges_sorted()
        .find_map(|(id, edge)| (edge.tgt == 1).then_some(id))
        .expect("required child edge");
    graph_view
        .remove_edge(required_edge)
        .expect("delete required child edge");
    graph_view.remove_node(1).expect("delete required child");
    let expected = [PutDomainError::MissingRequiredCell {
        elem: ViewRef::Node(expected_elem),
        field: expected_field.to_owned(),
    }];
    assert_square_refuses(
        &lens,
        &edited_tree,
        &tree_complement,
        &graph_view,
        &graph_complement,
        &expected,
    );
}

fn duplicate_key_after_edit_values(
    left: i64,
    right: i64,
    expected_table: &str,
    expected_field: &str,
    expected_elems: [u32; 2],
) {
    assert_ne!(left, right, "the source key values must be unique");
    let parent = edge("root", "a", "a");
    let key_field = edge("a", "b", "b");
    let mut schema = SchemaBuilder::new(&Protocol::default())
        .vertex("root", "record", None)
        .expect("root")
        .vertex("a", "record", None)
        .expect("a")
        .vertex("b", "string", None)
        .expect("b")
        .edge("root", "a", "rel", Some("a"))
        .expect("root-a")
        .edge("a", "b", "rel", Some("b"))
        .expect("a-b")
        .build()
        .expect("schema");
    schema.declarations.keys.insert(
        "a".into(),
        vec![Key {
            id: "by-b".into(),
            parts: vec![KeyPart::Path(vec![key_field])],
            role: KeyRole::Primary,
            scope: KeyScope::Global,
        }],
    );
    let lens = identity_lens(&schema);
    let mut first = Node::new(1, "a");
    first.extra_fields.insert("b".to_owned(), Value::Int(left));
    let mut second = Node::new(2, "a");
    second
        .extra_fields
        .insert("b".to_owned(), Value::Int(right));
    let source = WInstance::new(
        HashMap::from([(0, Node::new(0, "root")), (1, first), (2, second)]),
        vec![(0, 1, parent.clone()), (0, 2, parent)],
        Vec::new(),
        0,
        "root".into(),
    );
    let source_graph =
        GInstance::from_wtype_with_schema(&source, &schema).expect("schema-valid source graph");
    let (mut tree_view, tree_complement) = panproto_lens::get(&lens, &source).expect("tree get");
    let (mut graph_view, graph_complement) =
        graph_lens::get(&lens, &source_graph).expect("graph get");
    tree_view
        .nodes
        .get_mut(&2)
        .expect("second key row")
        .extra_fields
        .insert("b".to_owned(), Value::Int(left));
    graph_view
        .node_mut(2)
        .expect("second key node")
        .extra_fields
        .insert("b".to_owned(), Value::Int(left));
    let expected = [PutDomainError::DuplicateKey {
        table: expected_table.into(),
        key: vec![(expected_field.to_owned(), Value::Int(left))],
        elems: expected_elems.into_iter().map(ViewRef::Node).collect(),
    }];
    assert_square_refuses(
        &lens,
        &tree_view,
        &tree_complement,
        &graph_view,
        &graph_complement,
        &expected,
    );
}

fn malformed_fan(expected_node: u32, expected_label: &str) {
    let relation = edge("r", "c", "c");
    let schema = SchemaBuilder::new(&Protocol::default())
        .vertex("r", "record", None)
        .expect("r")
        .vertex("c", "record", None)
        .expect("c")
        .edge("r", "c", "rel", Some("c"))
        .expect("edge")
        .hyper_edge(
            "h",
            "rel",
            HashMap::from([
                ("p".to_owned(), "r".to_owned()),
                ("x".to_owned(), "c".to_owned()),
                ("y".to_owned(), "c".to_owned()),
            ]),
            "p",
        )
        .expect("hyper-edge")
        .build()
        .expect("schema");
    let lens = identity_lens(&schema);
    let source = WInstance::new(
        HashMap::from([
            (0, Node::new(0, "r")),
            (1, Node::new(1, "c")),
            (2, Node::new(2, "c")),
        ]),
        vec![(0, 1, relation.clone()), (0, 2, relation)],
        vec![Fan {
            hyper_edge_id: "h".to_owned(),
            parent: 0,
            children: HashMap::from([("x".to_owned(), 1), ("y".to_owned(), 2)]),
        }],
        0,
        "r".into(),
    );
    let (mut view, complement) = panproto_lens::get(&lens, &source).expect("tree get");
    view.fans[0].children.insert("x".to_owned(), 99);
    let report = panproto_lens::asymmetric::check_put_domain(&lens, &view, &complement)
        .expect_err("fan names an absent child");
    let expected = [PutDomainError::MalformedFan {
        node: expected_node,
        reason: FanViolation::MissingChild {
            label: expected_label.into(),
        },
    }];
    assert_eq!(report, expected, "full ordered native report");
    assert_eq!(
        first_put_error(panproto_lens::put(&lens, &view, &complement)),
        expected[0]
    );
}

#[test]
fn tree_validation_reference_certificate() {
    let mut count = 0;
    for_each_tree_validation_case(|case| {
        count += 1;
        match case {
            TreeValidationCase::RequiredDeletion { elem, field } => {
                required_deletion(elem, field);
            }
            TreeValidationCase::DuplicateKeyAfterEdit {
                table,
                field,
                repeated_value,
                replaced_value,
                elems,
            } => {
                duplicate_key_after_edit_values(
                    repeated_value,
                    replaced_value,
                    table,
                    field,
                    elems,
                );
            }
            TreeValidationCase::MalformedFan {
                node,
                missing_label,
            } => malformed_fan(node, missing_label),
        }
    });
    assert_eq!(count, tree_validation_case_count_certificate());
    assert_eq!(count, 3);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn randomized_tree_validation_square(
        pair in (any::<i16>(), any::<i16>())
            .prop_filter("source keys differ", |(left, right)| left != right),
    ) {
        let (left, right) = pair;
        duplicate_key_after_edit_values(i64::from(left), i64::from(right), "a", "b", [1, 2]);
    }
}
