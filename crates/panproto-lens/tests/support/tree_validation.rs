//! Reusable L.5/L.9 tree-validation assertions.

#![allow(clippy::expect_used)]

use std::collections::HashMap;

use panproto_inst::{
    CompiledMigration, Fan, FanViolation, GInstance, Node, PutDomainError, Value, ViewRef,
    WInstance,
};
use panproto_lens::graph_lens;
use panproto_lens::{Lens, LensError};
use panproto_reference::enumerate::lenses::TreeValidationCase;
use panproto_schema::{Edge, Key, KeyPart, KeyRole, KeyScope, Protocol, Schema, SchemaBuilder};

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

fn required_deletion(expected_elem: u32, child: u32, expected_field: &str) {
    assert_ne!(expected_elem, child, "the parent and child ids must differ");
    let relation = edge("a", "b", expected_field);
    let mut schema = SchemaBuilder::new(&Protocol::default())
        .vertex("a", "record", None)
        .expect("a")
        .vertex("b", "record", None)
        .expect("b")
        .edge("a", "b", "rel", Some(expected_field))
        .expect("edge")
        .build()
        .expect("schema");
    schema.required.insert("a".into(), vec![relation.clone()]);
    let lens = identity_lens(&schema);
    let source = WInstance::new(
        HashMap::from([
            (expected_elem, Node::new(expected_elem, "a")),
            (child, Node::new(child, "b")),
        ]),
        vec![(expected_elem, child, relation)],
        Vec::new(),
        expected_elem,
        "a".into(),
    );
    let source_graph =
        GInstance::from_wtype_with_schema(&source, &schema).expect("schema-valid source graph");
    let (tree_view, tree_complement) = panproto_lens::get(&lens, &source).expect("tree get");
    let (mut graph_view, graph_complement) =
        graph_lens::get(&lens, &source_graph).expect("graph get");
    let edited_tree = WInstance::new(
        HashMap::from([(expected_elem, tree_view.nodes[&expected_elem].clone())]),
        Vec::new(),
        Vec::new(),
        expected_elem,
        "a".into(),
    );
    let required_edge = graph_view
        .edges_sorted()
        .find_map(|(id, edge)| (edge.tgt == child).then_some(id))
        .expect("required child edge");
    graph_view
        .remove_edge(required_edge)
        .expect("delete required child edge");
    graph_view
        .remove_node(child)
        .expect("delete required child");
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

#[allow(clippy::too_many_arguments)]
fn duplicate_key_after_edit_values(
    left: i64,
    right: i64,
    root: u32,
    first_elem: u32,
    second_elem: u32,
    expected_table: &str,
    expected_field: &str,
    expected_elems: [u32; 2],
) {
    assert_ne!(left, right, "the source key values must be unique");
    assert_eq!(
        expected_elems,
        [first_elem, second_elem],
        "the independent expected report must preserve ascending element order"
    );
    assert!(
        root != first_elem && root != second_elem && first_elem != second_elem,
        "the root and both keyed elements must have distinct ids"
    );
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
    let mut first = Node::new(first_elem, "a");
    first.extra_fields.insert("b".to_owned(), Value::Int(left));
    let mut second = Node::new(second_elem, "a");
    second
        .extra_fields
        .insert("b".to_owned(), Value::Int(right));
    let source = WInstance::new(
        HashMap::from([
            (root, Node::new(root, "root")),
            (first_elem, first),
            (second_elem, second),
        ]),
        vec![
            (root, first_elem, parent.clone()),
            (root, second_elem, parent),
        ],
        Vec::new(),
        root,
        "root".into(),
    );
    let source_graph =
        GInstance::from_wtype_with_schema(&source, &schema).expect("schema-valid source graph");
    let (mut tree_view, tree_complement) = panproto_lens::get(&lens, &source).expect("tree get");
    let (mut graph_view, graph_complement) =
        graph_lens::get(&lens, &source_graph).expect("graph get");
    tree_view
        .nodes
        .get_mut(&second_elem)
        .expect("second key row")
        .extra_fields
        .insert("b".to_owned(), Value::Int(left));
    graph_view
        .node_mut(second_elem)
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

fn malformed_fan(
    expected_node: u32,
    first_child: u32,
    second_child: u32,
    absent_child: u32,
    expected_label: &str,
) {
    assert!(
        expected_node != first_child
            && expected_node != second_child
            && expected_node != absent_child
            && first_child != second_child
            && first_child != absent_child
            && second_child != absent_child,
        "the fan parent, present children and absent child must have distinct ids"
    );
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
            (expected_node, Node::new(expected_node, "r")),
            (first_child, Node::new(first_child, "c")),
            (second_child, Node::new(second_child, "c")),
        ]),
        vec![
            (expected_node, first_child, relation.clone()),
            (expected_node, second_child, relation),
        ],
        vec![Fan {
            hyper_edge_id: "h".to_owned(),
            parent: expected_node,
            children: HashMap::from([
                ("x".to_owned(), first_child),
                ("y".to_owned(), second_child),
            ]),
        }],
        expected_node,
        "r".into(),
    );
    let (mut view, complement) = panproto_lens::get(&lens, &source).expect("tree get");
    view.fans[0]
        .children
        .insert(expected_label.to_owned(), absent_child);
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

/// Assert one independently enumerated L.5/L.9 validation boundary.
pub fn assert_tree_validation_case(case: TreeValidationCase) {
    match case {
        TreeValidationCase::RequiredDeletion { elem, field } => {
            required_deletion(elem, 1, field);
        }
        TreeValidationCase::DuplicateKeyAfterEdit {
            table,
            field,
            repeated_value,
            replaced_value,
            elems,
        } => duplicate_key_after_edit_values(
            repeated_value,
            replaced_value,
            0,
            elems[0],
            elems[1],
            table,
            field,
            elems,
        ),
        TreeValidationCase::MalformedFan {
            node,
            missing_label,
        } => malformed_fan(node, 1, 2, 99, missing_label),
    }
}

/// Assert one generated case from every L.5/L.9 validation family.
pub fn assert_randomized_tree_validation(
    left: i64,
    right: i64,
    base: u32,
    required_field: &str,
    missing_label: &str,
) {
    required_deletion(base, base + 1, required_field);
    duplicate_key_after_edit_values(
        left,
        right,
        base,
        base + 1,
        base + 2,
        "a",
        "b",
        [base + 1, base + 2],
    );
    malformed_fan(base, base + 1, base + 2, base + 3, missing_label);
}
