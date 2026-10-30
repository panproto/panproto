#![allow(clippy::expect_used, clippy::unwrap_used)]
//! Key-role and required-key-part witnesses for graph and table put.

use std::collections::{BTreeMap, BTreeSet};

use panproto_reference::keys::{self, KeyEquality, KeyingError, SqlCollation, SqlPad};
use panproto_reference::lens::{
    GraphLens, LensError, Protolens, ProtolensError, PutDomainError, ViewRef, protolens_schema,
    tree_get, tree_put,
};
use panproto_reference::migration::Migration;
use panproto_reference::model::{GEdge, Graph, Node, NodeShape, SEdge, Table, Tree};
use panproto_reference::rel;
use panproto_reference::restrict::{self, Complement, NoValueLayer, PlanError, RestrictError};
use panproto_reference::schema::{self, DeclarationError, Key, KeyPart, KeyRole, KeyScope, Schema};
use panproto_reference::table::{
    self, TableBoundaries, TableCheckError, TableComplement, TableLens, TablePlanError,
};
use panproto_reference::validation::Semantics;
use panproto_reference::value::{Presence, Value};

fn key(id: &str, role: KeyRole, parts: Vec<KeyPart>) -> Key {
    Key {
        id: id.to_owned(),
        parts,
        role,
        scope: KeyScope::Global,
        equality: Vec::new(),
    }
}

fn duplicate_schema(role: KeyRole) -> Schema {
    let id = SEdge::new("t", "t.id", "prop", Some("id"));
    let mut schema = Schema::default()
        .vertex("t", "record")
        .vertex("t.id", "int")
        .edge("t", "t.id", "prop", Some("id"));
    let mut declared = key("by_id", role, vec![KeyPart::Path(vec![id])]);
    if role == KeyRole::SortKey {
        let within = SEdge::new("root", "t", "items", Some("items"));
        schema
            .vertices
            .insert("root".to_owned(), "record".to_owned());
        schema.edges.insert(within.clone());
        schema.orderings.insert(within, 0);
        declared.scope = KeyScope::Within("root".to_owned());
    }
    schema.keys.insert("t".to_owned(), vec![declared]);
    schema
}

fn identity_migration(schema: &Schema) -> Migration {
    Migration {
        surviving_verts: schema.vertices.keys().cloned().collect(),
        surviving_edges: schema.edges.clone(),
        ..Migration::default()
    }
}

const fn graph_report(errors: Vec<PutDomainError>) -> Result<(), Vec<PutDomainError>> {
    Err(errors)
}

const fn tree_error(error: PutDomainError) -> Result<Tree, LensError> {
    Err(LensError::PutDomain(error))
}

const fn table_report(errors: Vec<PutDomainError>) -> Result<(), TableCheckError> {
    Err(TableCheckError::Domain {
        violations: errors,
        unresolved_collations: BTreeSet::new(),
    })
}

fn duplicate_graph_errors(role: KeyRole) -> Vec<PutDomainError> {
    let schema = duplicate_schema(role);
    let migration = identity_migration(&schema);
    let plan = restrict::compile(&schema, &schema, &migration).unwrap();
    let lens = GraphLens::new(
        &schema,
        &schema,
        &migration,
        &plan,
        &BTreeMap::new(),
        &NoValueLayer,
    )
    .unwrap();
    let mut view = Graph::default();
    view.nodes
        .insert(3, Node::new(3, "t").with_field("id", Value::Int(7)));
    view.nodes
        .insert(8, Node::new(8, "t").with_field("id", Value::Int(7)));
    match lens.check_put_domain(&view, &Complement::default()) {
        Ok(()) => Vec::new(),
        Err(errors) => errors,
    }
}

fn duplicate_table_errors(role: KeyRole) -> Vec<PutDomainError> {
    let schema = duplicate_schema(role);
    let migration = identity_migration(&schema);
    let value_kinds = BTreeSet::from(["int".to_owned()]);
    let table_kinds = BTreeSet::from(["record".to_owned()]);
    let relational = rel::classify(&schema, &value_kinds, &table_kinds).unwrap();
    let plan =
        table::compile_for_tables(&schema, &schema, &relational, &relational, &migration).unwrap();
    let lens = TableLens::new(
        &schema,
        &schema,
        TableBoundaries::new(&relational, &relational),
        &migration,
        &plan,
        &BTreeMap::new(),
        &NoValueLayer,
    )
    .unwrap();
    let mut view = Table::default();
    view.rows.insert(
        3,
        ("t".to_owned(), [("id".to_owned(), Value::Int(7))].into()),
    );
    view.rows.insert(
        8,
        ("t".to_owned(), [("id".to_owned(), Value::Int(7))].into()),
    );
    match table::check_put_domain(&lens, &view, &TableComplement::default()) {
        Ok(()) => Vec::new(),
        Err(TableCheckError::Domain { violations, .. }) => violations,
        Err(other) => panic!("unexpected table-domain result: {other:?}"),
    }
}

#[test]
fn graph_put_enforces_exactly_the_three_uniqueness_roles() {
    for role in [KeyRole::Primary, KeyRole::Unique, KeyRole::Identifier] {
        assert_eq!(
            duplicate_graph_errors(role),
            vec![PutDomainError::DuplicateKey {
                table: "t".to_owned(),
                key: vec![("id".to_owned(), Value::Int(7))],
                elems: vec![ViewRef::Node(3), ViewRef::Node(8)],
            }],
            "{role:?} must enforce uniqueness with the source values"
        );
    }
    for role in [KeyRole::SortKey, KeyRole::Index] {
        assert_eq!(
            duplicate_graph_errors(role),
            Vec::new(),
            "{role:?} must not enforce uniqueness"
        );
    }
}

#[test]
fn table_put_enforces_exactly_the_three_uniqueness_roles() {
    for role in [KeyRole::Primary, KeyRole::Unique, KeyRole::Identifier] {
        assert_eq!(
            duplicate_table_errors(role),
            vec![PutDomainError::DuplicateKey {
                table: "t".to_owned(),
                key: vec![("id".to_owned(), Value::Int(7))],
                elems: vec![ViewRef::Row(3), ViewRef::Row(8)],
            }],
            "{role:?} must enforce uniqueness with the source values"
        );
    }
    for role in [KeyRole::SortKey, KeyRole::Index] {
        assert_eq!(
            duplicate_table_errors(role),
            Vec::new(),
            "{role:?} must not enforce uniqueness"
        );
    }
}

fn empty_path_schema(role: KeyRole) -> Schema {
    let mut schema = Schema::default().vertex("scalar", "str");
    schema.keys.insert(
        "scalar".to_owned(),
        vec![key("own", role, vec![KeyPart::Path(Vec::new())])],
    );
    schema
}

#[test]
fn graph_identifier_empty_path_uses_equal_own_values_for_uniqueness() {
    let schema = empty_path_schema(KeyRole::Identifier);
    let migration = identity_migration(&schema);
    let plan = restrict::compile(&schema, &schema, &migration).unwrap();
    let lens = GraphLens::new(
        &schema,
        &schema,
        &migration,
        &plan,
        &BTreeMap::new(),
        &NoValueLayer,
    )
    .unwrap();
    let same = Value::Str("same".to_owned());
    let view = Graph {
        nodes: [
            (3, Node::new(3, "scalar").with_value(same.clone())),
            (8, Node::new(8, "scalar").with_value(same.clone())),
        ]
        .into(),
        ..Graph::default()
    };

    assert_eq!(
        lens.check_put_domain(&view, &Complement::default()),
        graph_report(vec![PutDomainError::DuplicateKey {
            table: "scalar".to_owned(),
            key: vec![(String::new(), same)],
            elems: vec![ViewRef::Node(3), ViewRef::Node(8)],
        }])
    );
}

#[test]
fn graph_primary_empty_path_requires_exactly_a_present_non_null_own_value() {
    let schema = empty_path_schema(KeyRole::Primary);
    let migration = identity_migration(&schema);
    let plan = restrict::compile(&schema, &schema, &migration).unwrap();
    let lens = GraphLens::new(
        &schema,
        &schema,
        &migration,
        &plan,
        &BTreeMap::new(),
        &NoValueLayer,
    )
    .unwrap();
    let present = Graph {
        nodes: [(5, Node::new(5, "scalar").with_value(Value::Int(7)))].into(),
        ..Graph::default()
    };
    assert_eq!(
        lens.check_put_domain(&present, &Complement::default()),
        Ok(())
    );

    let cases = [
        ("no presence", None),
        ("absent", Some(Presence::Absent)),
        ("explicit null", Some(Presence::Null)),
        ("present null", Some(Presence::Present(Value::Null))),
    ];
    for (label, presence) in cases {
        let mut node = Node::new(5, "scalar");
        node.value = presence;
        let view = Graph {
            nodes: [(5, node)].into(),
            ..Graph::default()
        };
        assert_eq!(
            lens.check_put_domain(&view, &Complement::default()),
            graph_report(vec![PutDomainError::MissingRequiredCell {
                elem: ViewRef::Node(5),
                field: String::new(),
            }]),
            "{label} must not supply an own key value"
        );
    }
}

#[test]
fn table_primary_empty_path_remains_absent_because_rows_have_no_own_value() {
    let schema = empty_path_schema(KeyRole::Primary);
    let migration = identity_migration(&schema);
    let value_kinds = BTreeSet::new();
    let table_kinds = BTreeSet::from(["str".to_owned()]);
    let relational = rel::classify(&schema, &value_kinds, &table_kinds).unwrap();
    let plan =
        table::compile_for_tables(&schema, &schema, &relational, &relational, &migration).unwrap();
    let lens = TableLens::new(
        &schema,
        &schema,
        TableBoundaries::new(&relational, &relational),
        &migration,
        &plan,
        &BTreeMap::new(),
        &NoValueLayer,
    )
    .unwrap();
    let view = Table {
        rows: [(5, ("scalar".to_owned(), BTreeMap::default()))].into(),
        ..Table::default()
    };

    assert_eq!(
        table::check_put_domain(&lens, &view, &TableComplement::default()),
        table_report(vec![PutDomainError::MissingRequiredCell {
            elem: ViewRef::Row(5),
            field: String::new(),
        }])
    );
}

fn required_value_schema() -> (Schema, SEdge) {
    let value = SEdge::new("record", "scalar", "prop", Some("value"));
    let mut schema = Schema::default()
        .vertex("record", "record")
        .vertex("scalar", "str")
        .edge("record", "scalar", "prop", Some("value"));
    schema.constraints.insert(
        "scalar".to_owned(),
        vec![("required".to_owned(), "true".to_owned())],
    );
    (schema, value)
}

fn required_semantics() -> BTreeMap<String, Semantics> {
    BTreeMap::from([("required".to_owned(), Semantics::RequiredValue)])
}

#[test]
fn graph_required_value_uses_only_declared_semantics() {
    let (schema, _) = required_value_schema();
    let migration = identity_migration(&schema);
    let plan = restrict::compile(&schema, &schema, &migration).unwrap();
    let missing_field = Graph {
        nodes: [(3, Node::new(3, "record"))].into(),
        ..Graph::default()
    };
    for semantics in [
        BTreeMap::new(),
        BTreeMap::from([("required".to_owned(), Semantics::Annotation)]),
    ] {
        let inert = GraphLens::new(
            &schema,
            &schema,
            &migration,
            &plan,
            &semantics,
            &NoValueLayer,
        )
        .unwrap();
        assert_eq!(
            inert.check_put_domain(&missing_field, &Complement::default()),
            Ok(())
        );
    }
}

#[test]
fn graph_required_value_checks_every_value_site() {
    let (schema, _) = required_value_schema();
    let migration = identity_migration(&schema);
    let plan = restrict::compile(&schema, &schema, &migration).unwrap();
    let semantics = required_semantics();
    let lens = GraphLens::new(
        &schema,
        &schema,
        &migration,
        &plan,
        &semantics,
        &NoValueLayer,
    )
    .unwrap();
    let present = Graph {
        nodes: [
            (
                3,
                Node::new(3, "record").with_field("value", Value::Str("field".to_owned())),
            ),
            (
                5,
                Node::new(5, "scalar").with_value(Value::Str("own".to_owned())),
            ),
        ]
        .into(),
        ..Graph::default()
    };
    assert_eq!(
        lens.check_put_domain(&present, &Complement::default()),
        Ok(())
    );

    let missing = Graph {
        nodes: [
            (8, Node::new(8, "record")),
            (5, Node::new(5, "scalar")),
            (3, Node::new(3, "record").with_field("value", Value::Null)),
        ]
        .into(),
        ..Graph::default()
    };
    assert_eq!(
        lens.check_put_domain(&missing, &Complement::default()),
        graph_report(vec![
            PutDomainError::MissingRequiredCell {
                elem: ViewRef::Node(3),
                field: "value".to_owned(),
            },
            PutDomainError::MissingRequiredCell {
                elem: ViewRef::Node(5),
                field: String::new(),
            },
            PutDomainError::MissingRequiredCell {
                elem: ViewRef::Node(8),
                field: "value".to_owned(),
            },
        ])
    );

    for presence in [
        Some(Presence::Null),
        Some(Presence::Absent),
        Some(Presence::Present(Value::Null)),
    ] {
        let mut node = Node::new(5, "scalar");
        node.value = presence;
        let view = Graph {
            nodes: [(5, node)].into(),
            ..Graph::default()
        };
        assert_eq!(
            lens.check_put_domain(&view, &Complement::default()),
            graph_report(vec![PutDomainError::MissingRequiredCell {
                elem: ViewRef::Node(5),
                field: String::new(),
            }])
        );
    }
}

#[test]
fn graph_required_value_checks_a_structured_child_own_site() {
    let (schema, value_edge) = required_value_schema();
    let migration = identity_migration(&schema);
    let plan = restrict::compile(&schema, &schema, &migration).unwrap();
    let semantics = required_semantics();
    let lens = GraphLens::new(
        &schema,
        &schema,
        &migration,
        &plan,
        &semantics,
        &NoValueLayer,
    )
    .unwrap();
    let mut structured =
        Node::new(11, "scalar").with_value(Value::List(vec![Value::Int(1), Value::Int(2)]));
    structured.shape = NodeShape::List;
    let mut view = Graph {
        nodes: [(10, Node::new(10, "record")), (11, structured)].into(),
        edges: [(
            12,
            GEdge {
                src: 10,
                tgt: 11,
                label: value_edge,
                position: None,
            },
        )]
        .into(),
    };
    assert_eq!(lens.check_put_domain(&view, &Complement::default()), Ok(()));
    view.nodes.get_mut(&11).unwrap().value = None;
    assert_eq!(
        lens.check_put_domain(&view, &Complement::default()),
        graph_report(vec![PutDomainError::MissingRequiredCell {
            elem: ViewRef::Node(11),
            field: String::new(),
        }])
    );
}

#[test]
fn graph_required_value_suppresses_only_the_exact_realized_prop_site() {
    let a = SEdge::new("record", "scalar", "prop", Some("a"));
    let b = SEdge::new("record", "scalar", "prop", Some("b"));
    let mut schema = Schema::default()
        .vertex("record", "record")
        .vertex("scalar", "str")
        .edge("record", "scalar", "prop", Some("a"))
        .edge("record", "scalar", "prop", Some("b"));
    schema.constraints.insert(
        "scalar".to_owned(),
        vec![("required".to_owned(), "true".to_owned())],
    );
    let migration = identity_migration(&schema);
    let plan = restrict::compile(&schema, &schema, &migration).unwrap();
    let semantics = required_semantics();
    let lens = GraphLens::new(
        &schema,
        &schema,
        &migration,
        &plan,
        &semantics,
        &NoValueLayer,
    )
    .unwrap();
    let view = Graph {
        nodes: [
            (10, Node::new(10, "record")),
            (
                11,
                Node::new(11, "scalar").with_value(Value::Str("child".to_owned())),
            ),
        ]
        .into(),
        edges: [(
            12,
            GEdge {
                src: 10,
                tgt: 11,
                label: a,
                position: None,
            },
        )]
        .into(),
    };

    assert_eq!(
        lens.check_put_domain(&view, &Complement::default()),
        graph_report(vec![PutDomainError::MissingRequiredCell {
            elem: ViewRef::Node(10),
            field: b.column().to_owned(),
        }])
    );
}

#[test]
fn graph_required_value_sites_are_named_props_on_non_fan_nodes_only() {
    let mut schema = Schema::default()
        .vertex("record", "record")
        .vertex("scalar", "str")
        .vertex("other", "record")
        .edge("record", "scalar", "prop", None)
        .edge("record", "scalar", "child", Some("named"));
    schema.constraints.insert(
        "scalar".to_owned(),
        vec![("required".to_owned(), "true".to_owned())],
    );
    let migration = identity_migration(&schema);
    let plan = restrict::compile(&schema, &schema, &migration).unwrap();
    let semantics = required_semantics();
    let lens = GraphLens::new(
        &schema,
        &schema,
        &migration,
        &plan,
        &semantics,
        &NoValueLayer,
    )
    .unwrap();
    let ordinary = Graph {
        nodes: [(3, Node::new(3, "record"))].into(),
        ..Graph::default()
    };
    assert_eq!(
        lens.check_put_domain(&ordinary, &Complement::default()),
        Ok(())
    );

    let mut hyper = Node::new(4, "scalar");
    hyper.shape = NodeShape::HyperEdge;
    let fan_edge = GEdge {
        src: 5,
        tgt: 6,
        label: SEdge::new("scalar", "other", restrict::HYPER_LABEL, Some("x")),
        position: None,
    };
    let fan_boundary = Graph {
        nodes: [
            (4, hyper),
            (5, Node::new(5, "scalar")),
            (6, Node::new(6, "other")),
        ]
        .into(),
        edges: [(7, fan_edge.clone())].into(),
    };
    let mut complement = Complement::default();
    complement.original_edges.insert(7, fan_edge);
    complement.identity = Some(*lens.identity());
    let errors = lens
        .check_put_domain(&fan_boundary, &complement)
        .expect_err("the malformed fan must be outside the put domain");
    assert!(!errors.is_empty());
    assert!(
        errors
            .iter()
            .all(|error| !matches!(error, PutDomainError::MissingRequiredCell { .. }))
    );
}

#[test]
fn table_required_value_checks_named_cells() {
    let (schema, _) = required_value_schema();
    let migration = identity_migration(&schema);
    let value_kinds = BTreeSet::from(["str".to_owned()]);
    let table_kinds = BTreeSet::from(["record".to_owned()]);
    let relational = rel::classify(&schema, &value_kinds, &table_kinds).unwrap();
    let plan =
        table::compile_for_tables(&schema, &schema, &relational, &relational, &migration).unwrap();
    let semantics = required_semantics();
    let lens = TableLens::new(
        &schema,
        &schema,
        TableBoundaries::new(&relational, &relational),
        &migration,
        &plan,
        &semantics,
        &NoValueLayer,
    )
    .unwrap();
    let present = Table {
        rows: [(
            3,
            (
                "record".to_owned(),
                [("value".to_owned(), Value::Str("present".to_owned()))].into(),
            ),
        )]
        .into(),
        ..Table::default()
    };
    assert!(lens.put(&present, &TableComplement::default()).is_ok());
    let missing = Table {
        rows: [
            (8, ("record".to_owned(), BTreeMap::new())),
            (
                3,
                (
                    "record".to_owned(),
                    [("value".to_owned(), Value::Null)].into(),
                ),
            ),
        ]
        .into(),
        ..Table::default()
    };
    assert_eq!(
        table::check_put_domain(&lens, &missing, &TableComplement::default()),
        table_report(vec![
            PutDomainError::MissingRequiredCell {
                elem: ViewRef::Row(3),
                field: "value".to_owned(),
            },
            PutDomainError::MissingRequiredCell {
                elem: ViewRef::Row(8),
                field: "value".to_owned(),
            },
        ])
    );
}

#[test]
fn table_required_value_checks_row_own_sites_and_fk_suppression() {
    let child_a = SEdge::new("parent", "child", "prop", Some("a"));
    let mut linked_schema = Schema::default()
        .vertex("parent", "record")
        .vertex("child", "record")
        .edge("parent", "child", "prop", Some("a"))
        .edge("parent", "child", "prop", Some("b"));
    linked_schema.constraints.insert(
        "child".to_owned(),
        vec![("required".to_owned(), "true".to_owned())],
    );
    let linked_migration = identity_migration(&linked_schema);
    let linked_relational = rel::classify(
        &linked_schema,
        &BTreeSet::new(),
        &BTreeSet::from(["record".to_owned()]),
    )
    .unwrap();
    let linked_plan = table::compile_for_tables(
        &linked_schema,
        &linked_schema,
        &linked_relational,
        &linked_relational,
        &linked_migration,
    )
    .unwrap();
    let semantics = required_semantics();
    let linked_lens = TableLens::new(
        &linked_schema,
        &linked_schema,
        TableBoundaries::new(&linked_relational, &linked_relational),
        &linked_migration,
        &linked_plan,
        &semantics,
        &NoValueLayer,
    )
    .unwrap();
    let mut linked = Table {
        rows: [
            (1, ("parent".to_owned(), BTreeMap::new())),
            (2, ("child".to_owned(), BTreeMap::new())),
        ]
        .into(),
        pairs: BTreeSet::from([(child_a, 1, 2)]),
    };
    assert_eq!(
        table::check_put_domain(&linked_lens, &linked, &TableComplement::default()),
        table_report(vec![
            PutDomainError::MissingRequiredCell {
                elem: ViewRef::Row(1),
                field: "b".to_owned(),
            },
            PutDomainError::MissingRequiredCell {
                elem: ViewRef::Row(2),
                field: String::new(),
            },
        ])
    );
    linked.pairs.clear();
    assert_eq!(
        table::check_put_domain(&linked_lens, &linked, &TableComplement::default()),
        table_report(vec![
            PutDomainError::MissingRequiredCell {
                elem: ViewRef::Row(1),
                field: "a".to_owned(),
            },
            PutDomainError::MissingRequiredCell {
                elem: ViewRef::Row(1),
                field: "b".to_owned(),
            },
            PutDomainError::MissingRequiredCell {
                elem: ViewRef::Row(2),
                field: String::new(),
            },
        ])
    );
}

#[test]
fn tree_required_value_distinguishes_a_field_site_from_a_child_own_site() {
    let (schema, value_edge) = required_value_schema();
    let migration = identity_migration(&schema);
    let plan = restrict::compile(&schema, &schema, &migration).unwrap();
    let semantics = required_semantics();
    let lens = GraphLens::new(
        &schema,
        &schema,
        &migration,
        &plan,
        &semantics,
        &NoValueLayer,
    )
    .unwrap();
    let field_source = Tree {
        nodes: [(
            1,
            Node::new(1, "record").with_field("value", Value::Str("field".to_owned())),
        )]
        .into(),
        arcs: Vec::new(),
        fans: Vec::new(),
        root: 1,
    };
    let (mut field_view, field_complement, field_embedding) =
        tree_get(&lens, &field_source).unwrap().unwrap();
    field_view.nodes.get_mut(&1).unwrap().fields.clear();
    assert_eq!(
        tree_put(&lens, &field_view, &field_embedding, &field_complement),
        tree_error(PutDomainError::MissingRequiredCell {
            elem: ViewRef::Node(1),
            field: "value".to_owned(),
        })
    );

    let child_source = Tree {
        nodes: [
            (1, Node::new(1, "record")),
            (
                2,
                Node::new(2, "scalar").with_value(Value::Str("child".to_owned())),
            ),
        ]
        .into(),
        arcs: vec![(1, 2, value_edge)],
        fans: Vec::new(),
        root: 1,
    };
    let (mut child_view, child_complement, child_embedding) =
        tree_get(&lens, &child_source).unwrap().unwrap();
    child_view.nodes.get_mut(&2).unwrap().value = Some(Presence::Null);
    assert_eq!(
        tree_put(&lens, &child_view, &child_embedding, &child_complement),
        tree_error(PutDomainError::MissingRequiredCell {
            elem: ViewRef::Node(2),
            field: String::new(),
        })
    );
}

#[test]
fn required_value_errors_deduplicate_and_sort_after_duplicate_keys_by_field() {
    let id = SEdge::new("t", "t.id", "prop", Some("id"));
    let a = SEdge::new("t", "t.a", "prop", Some("a"));
    let mut schema = Schema::default()
        .vertex("t", "record")
        .vertex("t.id", "int")
        .vertex("t.a", "int")
        .vertex("t.z", "int")
        .edge("t", "t.id", "prop", Some("id"))
        .edge("t", "t.a", "prop", Some("a"))
        .edge("t", "t.z", "prop", Some("z"));
    schema.keys.insert(
        "t".to_owned(),
        vec![
            key(
                "duplicate-id",
                KeyRole::Unique,
                vec![KeyPart::Path(vec![id])],
            ),
            key("required-a", KeyRole::Primary, vec![KeyPart::Path(vec![a])]),
        ],
    );
    schema.constraints.insert(
        "t.a".to_owned(),
        vec![
            ("required".to_owned(), "one".to_owned()),
            ("required".to_owned(), "two".to_owned()),
        ],
    );
    schema.constraints.insert(
        "t.z".to_owned(),
        vec![("required".to_owned(), "true".to_owned())],
    );
    let migration = identity_migration(&schema);
    let plan = restrict::compile(&schema, &schema, &migration).unwrap();
    let semantics = required_semantics();
    let lens = GraphLens::new(
        &schema,
        &schema,
        &migration,
        &plan,
        &semantics,
        &NoValueLayer,
    )
    .unwrap();
    let view = Graph {
        nodes: [
            (3, Node::new(3, "t").with_field("id", Value::Int(7))),
            (8, Node::new(8, "t").with_field("id", Value::Int(7))),
        ]
        .into(),
        ..Graph::default()
    };

    assert_eq!(
        lens.check_put_domain(&view, &Complement::default()),
        graph_report(vec![
            PutDomainError::DuplicateKey {
                table: "t".to_owned(),
                key: vec![("id".to_owned(), Value::Int(7))],
                elems: vec![ViewRef::Node(3), ViewRef::Node(8)],
            },
            PutDomainError::MissingRequiredCell {
                elem: ViewRef::Node(3),
                field: "a".to_owned(),
            },
            PutDomainError::MissingRequiredCell {
                elem: ViewRef::Node(3),
                field: "z".to_owned(),
            },
            PutDomainError::MissingRequiredCell {
                elem: ViewRef::Node(8),
                field: "a".to_owned(),
            },
            PutDomainError::MissingRequiredCell {
                elem: ViewRef::Node(8),
                field: "z".to_owned(),
            },
        ])
    );
}

fn two_duplicate_keys_schema() -> Schema {
    let z = SEdge::new("t", "t.z", "prop", Some("z"));
    let a = SEdge::new("t", "t.a", "prop", Some("a"));
    let mut schema = Schema::default()
        .vertex("t", "record")
        .vertex("t.z", "int")
        .vertex("t.a", "int")
        .edge("t", "t.z", "prop", Some("z"))
        .edge("t", "t.a", "prop", Some("a"));
    schema.keys.insert(
        "t".to_owned(),
        vec![
            key("by_z", KeyRole::Unique, vec![KeyPart::Path(vec![z])]),
            key("by_a", KeyRole::Unique, vec![KeyPart::Path(vec![a])]),
        ],
    );
    schema
}

fn tied_duplicate_errors(elem: fn(u32) -> ViewRef) -> Vec<PutDomainError> {
    vec![
        PutDomainError::DuplicateKey {
            table: "t".to_owned(),
            key: vec![("a".to_owned(), Value::Int(1))],
            elems: vec![elem(3), elem(8)],
        },
        PutDomainError::DuplicateKey {
            table: "t".to_owned(),
            key: vec![("z".to_owned(), Value::Int(9))],
            elems: vec![elem(3), elem(8)],
        },
    ]
}

#[test]
fn graph_duplicate_key_ties_use_named_canonical_key_order() {
    let schema = two_duplicate_keys_schema();
    let migration = identity_migration(&schema);
    let plan = restrict::compile(&schema, &schema, &migration).unwrap();
    let lens = GraphLens::new(
        &schema,
        &schema,
        &migration,
        &plan,
        &BTreeMap::new(),
        &NoValueLayer,
    )
    .unwrap();
    let view = Graph {
        nodes: [
            (
                3,
                Node::new(3, "t")
                    .with_field("z", Value::Int(9))
                    .with_field("a", Value::Int(1)),
            ),
            (
                8,
                Node::new(8, "t")
                    .with_field("z", Value::Int(9))
                    .with_field("a", Value::Int(1)),
            ),
        ]
        .into(),
        ..Graph::default()
    };

    assert_eq!(
        lens.check_put_domain(&view, &Complement::default()),
        graph_report(tied_duplicate_errors(ViewRef::Node))
    );
}

#[test]
fn table_duplicate_key_ties_use_named_canonical_key_order() {
    let schema = two_duplicate_keys_schema();
    let migration = identity_migration(&schema);
    let value_kinds = BTreeSet::from(["int".to_owned()]);
    let table_kinds = BTreeSet::from(["record".to_owned()]);
    let relational = rel::classify(&schema, &value_kinds, &table_kinds).unwrap();
    let plan =
        table::compile_for_tables(&schema, &schema, &relational, &relational, &migration).unwrap();
    let lens = TableLens::new(
        &schema,
        &schema,
        TableBoundaries::new(&relational, &relational),
        &migration,
        &plan,
        &BTreeMap::new(),
        &NoValueLayer,
    )
    .unwrap();
    let view = Table {
        rows: [
            (
                3,
                (
                    "t".to_owned(),
                    [
                        ("z".to_owned(), Value::Int(9)),
                        ("a".to_owned(), Value::Int(1)),
                    ]
                    .into(),
                ),
            ),
            (
                8,
                (
                    "t".to_owned(),
                    [
                        ("z".to_owned(), Value::Int(9)),
                        ("a".to_owned(), Value::Int(1)),
                    ]
                    .into(),
                ),
            ),
        ]
        .into(),
        ..Table::default()
    };

    assert_eq!(
        table::check_put_domain(&lens, &view, &TableComplement::default()),
        table_report(tied_duplicate_errors(ViewRef::Row))
    );
}

fn required_parts_schema() -> Schema {
    let id = SEdge::new("item", "item.id", "prop", Some("id"));
    let within = SEdge::new("root", "item", "items", Some("items"));
    let mut schema = Schema::default()
        .vertex("root", "record")
        .vertex("item", "record")
        .vertex("item.id", "int")
        .edge("root", "item", "items", Some("items"))
        .edge("item", "item.id", "prop", Some("id"));
    schema.keys.insert(
        "item".to_owned(),
        vec![key(
            "primary",
            KeyRole::Primary,
            vec![KeyPart::Path(vec![id]), KeyPart::Position { within }],
        )],
    );
    schema
}

#[test]
fn graph_primary_requires_every_path_and_position_part_in_part_order() {
    let schema = required_parts_schema();
    let migration = identity_migration(&schema);
    let plan = restrict::compile(&schema, &schema, &migration).unwrap();
    let lens = GraphLens::new(
        &schema,
        &schema,
        &migration,
        &plan,
        &BTreeMap::new(),
        &NoValueLayer,
    )
    .unwrap();
    let view = Graph {
        nodes: [(5, Node::new(5, "item"))].into(),
        ..Graph::default()
    };

    assert_eq!(
        lens.check_put_domain(&view, &Complement::default()),
        graph_report(vec![
            PutDomainError::MissingRequiredCell {
                elem: ViewRef::Node(5),
                field: "id".to_owned(),
            },
            PutDomainError::MissingRequiredCell {
                elem: ViewRef::Node(5),
                field: "items".to_owned(),
            },
        ])
    );
}

#[test]
fn table_primary_requires_every_path_and_position_part_in_part_order() {
    let schema = required_parts_schema();
    let migration = identity_migration(&schema);
    let value_kinds = BTreeSet::from(["int".to_owned()]);
    let table_kinds = BTreeSet::from(["record".to_owned()]);
    let relational = rel::classify(&schema, &value_kinds, &table_kinds).unwrap();
    let plan =
        table::compile_for_tables(&schema, &schema, &relational, &relational, &migration).unwrap();
    let lens = TableLens::new(
        &schema,
        &schema,
        TableBoundaries::new(&relational, &relational),
        &migration,
        &plan,
        &BTreeMap::new(),
        &NoValueLayer,
    )
    .unwrap();
    let view = Table {
        rows: [(5, ("item".to_owned(), BTreeMap::default()))].into(),
        ..Table::default()
    };

    assert_eq!(
        table::check_put_domain(&lens, &view, &TableComplement::default()),
        table_report(vec![
            PutDomainError::MissingRequiredCell {
                elem: ViewRef::Row(5),
                field: "id".to_owned(),
            },
            PutDomainError::MissingRequiredCell {
                elem: ViewRef::Row(5),
                field: "items".to_owned(),
            },
        ])
    );
}

#[test]
fn graph_primary_position_is_present_exactly_within_an_ordinary_edge() {
    let schema = required_parts_schema();
    let migration = identity_migration(&schema);
    let plan = restrict::compile(&schema, &schema, &migration).unwrap();
    let lens = GraphLens::new(
        &schema,
        &schema,
        &migration,
        &plan,
        &BTreeMap::new(),
        &NoValueLayer,
    )
    .unwrap();
    let within = SEdge::new("root", "item", "items", Some("items"));
    let mut view = Graph {
        nodes: [
            (1, Node::new(1, "root")),
            (5, Node::new(5, "item").with_field("id", Value::Int(7))),
        ]
        .into(),
        edges: [(
            11,
            GEdge {
                src: 1,
                tgt: 5,
                label: within,
                position: Some(3),
            },
        )]
        .into(),
    };

    assert_eq!(lens.check_put_domain(&view, &Complement::default()), Ok(()));
    view.edges.clear();
    assert_eq!(
        lens.check_put_domain(&view, &Complement::default()),
        graph_report(vec![PutDomainError::MissingRequiredCell {
            elem: ViewRef::Node(5),
            field: "items".to_owned(),
        }])
    );
}

#[test]
fn table_primary_position_is_present_exactly_within_a_foreign_key_pair() {
    let schema = required_parts_schema();
    let migration = identity_migration(&schema);
    let value_kinds = BTreeSet::from(["int".to_owned()]);
    let table_kinds = BTreeSet::from(["record".to_owned()]);
    let relational = rel::classify(&schema, &value_kinds, &table_kinds).unwrap();
    let plan =
        table::compile_for_tables(&schema, &schema, &relational, &relational, &migration).unwrap();
    let lens = TableLens::new(
        &schema,
        &schema,
        TableBoundaries::new(&relational, &relational),
        &migration,
        &plan,
        &BTreeMap::new(),
        &NoValueLayer,
    )
    .unwrap();
    let within = SEdge::new("root", "item", "items", Some("items"));
    let mut view = Table {
        rows: [
            (1, ("root".to_owned(), BTreeMap::default())),
            (
                5,
                ("item".to_owned(), [("id".to_owned(), Value::Int(7))].into()),
            ),
        ]
        .into(),
        pairs: BTreeSet::from([(within, 1, 5)]),
    };

    assert!(lens.put(&view, &TableComplement::default()).is_ok());
    view.pairs.clear();
    assert_eq!(
        table::check_put_domain(&lens, &view, &TableComplement::default()),
        table_report(vec![PutDomainError::MissingRequiredCell {
            elem: ViewRef::Row(5),
            field: "items".to_owned(),
        }])
    );
}

fn equality_key(id: &str, equality: Vec<KeyEquality>) -> Key {
    Key {
        id: id.to_owned(),
        parts: vec![
            KeyPart::Path(vec![SEdge::new("t", "t.a", "prop", Some("a"))]),
            KeyPart::Path(vec![SEdge::new("t", "t.b", "prop", Some("b"))]),
        ],
        role: KeyRole::Unique,
        scope: KeyScope::Global,
        equality,
    }
}

fn equality_schema(keys: Vec<Key>) -> Schema {
    let mut schema = Schema::default()
        .vertex("t", "record")
        .vertex("t.a", "str")
        .vertex("t.b", "str")
        .edge("t", "t.a", "prop", Some("a"))
        .edge("t", "t.b", "prop", Some("b"));
    schema.keys.insert("t".to_owned(), keys);
    schema
}

fn structurally_bad_migration(schema: &Schema) -> Migration {
    let source = schema.edges.iter().next().unwrap().clone();
    Migration {
        surviving_verts: schema.vertices.keys().cloned().collect(),
        edge_remap: [(source, SEdge::new("t", "t.a", "prop", Some("missing")))].into(),
        ..Migration::default()
    }
}

#[test]
fn declaration_build_refusals_preserve_side_full_vector_and_order() {
    let source = equality_schema(vec![
        equality_key("duplicate", vec![KeyEquality::Representation]),
        equality_key(
            "duplicate",
            vec![
                KeyEquality::Representation,
                KeyEquality::Representation,
                KeyEquality::Representation,
            ],
        ),
    ]);
    let target = equality_schema(vec![equality_key(
        "target_bad",
        vec![KeyEquality::Representation],
    )]);
    let valid = equality_schema(vec![equality_key("valid", Vec::new())]);
    let source_errors = vec![
        DeclarationError::EqualityArity {
            vertex: "t".to_owned(),
            key: "duplicate".to_owned(),
            expected: 2,
            found: 1,
        },
        DeclarationError::DuplicateKeyId {
            vertex: "t".to_owned(),
            key: "duplicate".to_owned(),
        },
        DeclarationError::EqualityArity {
            vertex: "t".to_owned(),
            key: "duplicate".to_owned(),
            expected: 2,
            found: 3,
        },
    ];
    let target_errors = vec![DeclarationError::EqualityArity {
        vertex: "t".to_owned(),
        key: "target_bad".to_owned(),
        expected: 2,
        found: 1,
    }];
    assert_eq!(
        schema::validate_declarations(&source),
        Err(source_errors.clone())
    );
    assert_eq!(
        schema::validate_declarations(&target),
        Err(target_errors.clone())
    );

    let bad_migration = structurally_bad_migration(&valid);
    assert!(matches!(
        restrict::compile(&valid, &valid, &bad_migration),
        Err(PlanError::IncoherentEdgeImage { .. })
    ));
    assert_eq!(
        restrict::compile(&source, &target, &bad_migration),
        Err(PlanError::SourceDeclarations(source_errors.clone()))
    );
    let protolens = Protolens::exact(&source, &target, &bad_migration);
    assert_eq!(
        protolens_schema(&protolens, &source),
        Err(ProtolensError::Plan(PlanError::SourceDeclarations(
            source_errors
        )))
    );
    assert_eq!(
        restrict::compile(&valid, &target, &bad_migration),
        Err(PlanError::TargetDeclarations(target_errors))
    );
}

#[test]
fn empty_exact_and_named_equalities_cross_the_build_boundary() {
    let empty = equality_schema(vec![equality_key("empty", Vec::new())]);
    let exact = equality_schema(vec![equality_key(
        "exact",
        vec![KeyEquality::Representation, KeyEquality::NumericValue],
    )]);
    for schema in [&empty, &exact] {
        let migration = identity_migration(schema);
        let plan = restrict::compile(schema, schema, &migration).unwrap();
        assert_eq!(plan.fates().len(), schema.edges.len());
        assert!(plan.pairs().is_empty());
    }

    let named = equality_schema(vec![equality_key(
        "named",
        vec![
            KeyEquality::Sql {
                collation: SqlCollation::Named("database_default".to_owned()),
                pad: SqlPad::NoPad,
            },
            KeyEquality::Representation,
        ],
    )]);
    let migration = identity_migration(&named);
    restrict::compile(&named, &named, &migration).unwrap();
    let value_kinds = BTreeSet::from(["str".to_owned()]);
    let table_kinds = BTreeSet::from(["record".to_owned()]);
    let relational = rel::classify(&named, &value_kinds, &table_kinds).unwrap();
    let table_plan =
        table::compile_for_tables(&named, &named, &relational, &relational, &migration).unwrap();
    assert_eq!(table_plan.restrict_plan().fates().len(), named.edges.len());
    assert_eq!(table_plan.functor().tables.get("t"), Some(&"t".to_owned()));
    assert!(table_plan.fates().contains_key("t"));

    let graph = Graph {
        nodes: [(
            3,
            Node::new(3, "t")
                .with_field("a", Value::Str("A".to_owned()))
                .with_field("b", Value::Str("B".to_owned())),
        )]
        .into(),
        ..Graph::default()
    };
    assert_eq!(
        keys::buckets(&graph, "t", &named.keys["t"][0]),
        Err(KeyingError::UnsupportedCollation(
            "database_default".to_owned()
        ))
    );
}

#[test]
fn table_plan_wraps_declaration_refusals_before_functor_refusals() {
    let source = equality_schema(vec![equality_key(
        "source_bad",
        vec![KeyEquality::Representation],
    )]);
    let target = equality_schema(vec![equality_key(
        "target_bad",
        vec![KeyEquality::Representation],
    )]);
    let valid = equality_schema(vec![equality_key("valid", Vec::new())]);
    let migration = identity_migration(&valid);
    let bad_source_rel = rel::RelSchema {
        tables: ["t".to_owned()].into(),
        ..rel::RelSchema::default()
    };
    let bad_target_rel = rel::RelSchema::default();
    assert!(matches!(
        table::compile_for_tables(&valid, &valid, &bad_source_rel, &bad_target_rel, &migration),
        Err(TablePlanError::Functor(_))
    ));
    let source_error = schema::validate_declarations(&source).unwrap_err();
    let target_error = schema::validate_declarations(&target).unwrap_err();
    assert_eq!(
        table::compile_for_tables(
            &source,
            &valid,
            &bad_source_rel,
            &bad_target_rel,
            &migration,
        ),
        Err(TablePlanError::Plan(Box::new(
            PlanError::SourceDeclarations(source_error)
        )))
    );
    assert_eq!(
        table::compile_for_tables(
            &valid,
            &target,
            &bad_source_rel,
            &bad_target_rel,
            &migration,
        ),
        Err(TablePlanError::Plan(Box::new(
            PlanError::TargetDeclarations(target_error)
        )))
    );
}

#[test]
fn compiled_plan_reuse_is_refused_at_every_public_execution_boundary() {
    let compiled = Schema::default().vertex("t", "record");
    let other = equality_schema(vec![equality_key("bad", vec![KeyEquality::Representation])]);
    let migration = Migration {
        surviving_verts: ["t".to_owned()].into(),
        conditional: ["t".to_owned()].into(),
        ..Migration::default()
    };
    let plan = restrict::compile(&compiled, &compiled, &migration).unwrap();
    let source_mismatch = PlanError::SourceSchemaMismatch {
        expected: schema::canonical_bytes(&compiled),
        found: schema::canonical_bytes(&other),
    };
    let target_mismatch = PlanError::TargetSchemaMismatch {
        expected: schema::canonical_bytes(&compiled),
        found: schema::canonical_bytes(&other),
    };
    let graph = Graph {
        nodes: [(1, Node::new(1, "t"))].into(),
        ..Graph::default()
    };
    assert_eq!(
        restrict::get_with_plan(&other, &other, &migration, &plan, &NoValueLayer, &graph,),
        Err(RestrictError::Plan(source_mismatch.clone()))
    );
    assert_eq!(
        restrict::get_with_plan(&compiled, &other, &migration, &plan, &NoValueLayer, &graph,),
        Err(RestrictError::Plan(target_mismatch.clone()))
    );
    assert_eq!(
        GraphLens::new(
            &other,
            &other,
            &migration,
            &plan,
            &BTreeMap::new(),
            &NoValueLayer,
        )
        .err(),
        Some(source_mismatch.clone())
    );
    assert_eq!(
        GraphLens::new(
            &compiled,
            &other,
            &migration,
            &plan,
            &BTreeMap::new(),
            &NoValueLayer,
        )
        .err(),
        Some(target_mismatch.clone())
    );

    let table_kinds = BTreeSet::from(["record".to_owned()]);
    let relational = rel::classify(&compiled, &BTreeSet::new(), &table_kinds).unwrap();
    let table_plan =
        table::compile_for_tables(&compiled, &compiled, &relational, &relational, &migration)
            .unwrap();
    assert_eq!(
        TableLens::new(
            &other,
            &other,
            TableBoundaries::new(&relational, &relational),
            &migration,
            &table_plan,
            &BTreeMap::new(),
            &NoValueLayer,
        )
        .err(),
        Some(TablePlanError::Plan(Box::new(source_mismatch)))
    );
    assert_eq!(
        TableLens::new(
            &compiled,
            &other,
            TableBoundaries::new(&relational, &relational),
            &migration,
            &table_plan,
            &BTreeMap::new(),
            &NoValueLayer,
        )
        .err(),
        Some(TablePlanError::Plan(Box::new(target_mismatch)))
    );
}
