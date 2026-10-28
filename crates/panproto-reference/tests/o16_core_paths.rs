#![allow(clippy::expect_used, clippy::unwrap_used)]
//! Direct O16 witnesses for key, resolution, theory, and representation paths.

use std::collections::BTreeMap;

use panproto_reference::exact::{Decimal, DecimalValue};
use panproto_reference::keys::{KeyEquality, SqlCollation, SqlPad};
use panproto_reference::model::{Fan, GEdge, Graph, Node, NodeShape, SEdge, Tree};
use panproto_reference::schema::{HyperEdge, Schema};
use panproto_reference::value::Value;
use panproto_reference::{encode, fan, keys, resolution, theory};

#[test]
fn representation_refinement_preserves_computable_truth_and_named_refusal() {
    assert_eq!(
        keys::representation_refines(&KeyEquality::NumericValue, &Value::Int(7), &Value::Int(7),),
        Some(true),
        "identical represented integers satisfy numeric equality"
    );
    assert_eq!(
        keys::representation_refines(&KeyEquality::NumericValue, &Value::Int(7), &Value::Int(8),),
        Some(true),
        "a false representation antecedent makes the refinement implication true"
    );

    let named = KeyEquality::Sql {
        collation: SqlCollation::Named("database-default".to_owned()),
        pad: SqlPad::NoPad,
    };
    assert_eq!(
        keys::representation_refines(
            &named,
            &Value::Str("same".to_owned()),
            &Value::Str("same".to_owned()),
        ),
        None,
        "represented text under a named collation remains an explicit refusal"
    );
}

#[test]
fn target_set_requires_distinct_members_with_one_arity_and_equality_vector() {
    assert!(
        !keys::target_set(&[]),
        "a target set with no members is not a declaration"
    );

    let representation = vec![KeyEquality::Representation];
    let compatible = vec![
        (
            "left".to_owned(),
            "id".to_owned(),
            1,
            representation.clone(),
        ),
        ("right".to_owned(), "code".to_owned(), 1, representation),
    ];
    assert!(
        keys::target_set(&compatible),
        "distinct declarations with identical key contracts are compatible"
    );

    let duplicate = vec![compatible[0].clone(), compatible[0].clone()];
    assert!(
        !keys::target_set(&duplicate),
        "the same vertex/key declaration cannot occur twice"
    );

    let incompatible = vec![
        compatible[0].clone(),
        (
            "right".to_owned(),
            "code".to_owned(),
            2,
            vec![KeyEquality::NumericValue, KeyEquality::NumericValue],
        ),
    ];
    assert!(
        !keys::target_set(&incompatible),
        "different arities and equality vectors are not compatible"
    );

    let same_arity_different_equality = vec![
        compatible[0].clone(),
        (
            "right".to_owned(),
            "code".to_owned(),
            1,
            vec![KeyEquality::NumericValue],
        ),
    ];
    assert!(
        !keys::target_set(&same_arity_different_equality),
        "equal arity does not hide an unequal equality vector"
    );

    let different_arity_same_equality = vec![
        compatible[0].clone(),
        (
            "right".to_owned(),
            "code".to_owned(),
            2,
            vec![KeyEquality::Representation],
        ),
    ];
    assert!(
        !keys::target_set(&different_arity_same_equality),
        "an equal equality vector does not hide unequal arity"
    );
}

#[test]
fn xml_noncdata_collapses_post_attribute_u0020_only() {
    assert_eq!(
        keys::xml_noncdata("\t alpha\r\n beta  "),
        "\t alpha\r\n beta",
        "attribute processing owns TAB, CR, and LF before this kernel runs"
    );
    assert_eq!(
        keys::xml_noncdata("alpha beta"),
        "alpha beta",
        "an already normalized spelling is fixed"
    );
    assert_eq!(
        keys::xml_noncdata("\u{00a0}alpha\u{00a0}"),
        "\u{00a0}alpha\u{00a0}",
        "non-breaking spaces are not XML whitespace and remain unchanged"
    );
    assert_eq!(
        keys::key_equivalence(
            &KeyEquality::XmlNonCdata,
            &Value::Str(" alpha  beta ".to_owned()),
            &Value::Str("alpha beta".to_owned()),
        ),
        Some(true),
        "the declared equality observes the non-CDATA normalization"
    );
}

#[test]
fn guarded_partition_selects_the_first_representationally_equal_guard() {
    let guards = vec![
        ("z-first".to_owned(), Value::Int(7)),
        ("a-second".to_owned(), Value::Int(7)),
    ];
    assert_eq!(
        resolution::guarded_partition(&guards, &Value::Int(7)),
        Some("z-first"),
        "declaration order, not lexical guard name order, selects the first match"
    );
    assert_eq!(
        resolution::guarded_partition(&guards, &Value::Int(8)),
        None,
        "a value outside every guard partition is refused"
    );

    let distinct_guards = vec![
        ("seven".to_owned(), Value::Int(7)),
        ("eight".to_owned(), Value::Int(8)),
    ];
    assert_eq!(
        resolution::guarded_partition(&distinct_guards, &Value::Int(8)),
        Some("eight"),
        "a later distinct guard remains reachable"
    );

    let scaled = Value::Decimal(Decimal::Finite(
        DecimalValue::parse("1.0").expect("the literal is a decimal"),
    ));
    let minimal = Value::Decimal(Decimal::Finite(
        DecimalValue::parse("1").expect("the literal is a decimal"),
    ));
    let decimal_guards = vec![
        ("scaled".to_owned(), scaled),
        ("minimal".to_owned(), minimal.clone()),
    ];
    assert_eq!(
        resolution::guarded_partition(&decimal_guards, &minimal),
        Some("minimal"),
        "numeric equality does not collapse representation-distinct decimal scales"
    );
}

#[test]
fn inherited_base_uses_the_nearest_present_ancestor() {
    let ancestors = vec![
        Some("https://outer.example/root/".to_owned()),
        None,
        Some("https://inner.example/a/b/".to_owned()),
    ];
    assert_eq!(
        resolution::inherited_base(&ancestors, "../item?q=1"),
        Some("https://inner.example/a/item?q=1".to_owned()),
        "the nearest present base wins over a more distant ancestor"
    );
    assert_eq!(
        resolution::inherited_base(&[None, None], "item"),
        None,
        "a relative reference without an inherited base is undefined"
    );
}

#[test]
fn preserving_registry_is_the_ordered_cartesian_product() {
    let choices = vec![
        vec!["left-a".to_owned(), "left-b".to_owned()],
        vec!["right-1".to_owned(), "right-2".to_owned()],
    ];
    assert_eq!(
        resolution::preserving_registry(&choices),
        vec![
            vec!["left-a".to_owned(), "right-1".to_owned()],
            vec!["left-a".to_owned(), "right-2".to_owned()],
            vec!["left-b".to_owned(), "right-1".to_owned()],
            vec!["left-b".to_owned(), "right-2".to_owned()],
        ],
        "registry choices expand to products rather than returning the input groups"
    );
    assert_eq!(
        resolution::preserving_registry::<String>(&[]),
        vec![Vec::<String>::new()],
        "the empty product contains one empty registry"
    );
    assert_eq!(
        resolution::preserving_registry(&[
            vec!["left".to_owned()],
            Vec::<String>::new(),
            vec!["right".to_owned()],
        ]),
        Vec::<Vec<String>>::new(),
        "one empty factor makes the entire cartesian product empty"
    );
}

#[test]
fn uri_environment_folds_every_directive_in_order() {
    let directives = vec![
        "https://example.test/root/a/".to_owned(),
        "../b/".to_owned(),
        "leaf?version=1".to_owned(),
    ];
    assert_eq!(
        resolution::uri_environment(&directives),
        Some("https://example.test/root/b/leaf?version=1".to_owned()),
        "both relative directives refine the preceding environment"
    );
    assert_eq!(
        resolution::uri_environment(&["relative-only".to_owned()]),
        None,
        "the first directive must establish an absolute environment"
    );
}

#[test]
fn quotient_closes_valid_identifications_and_refuses_either_bad_endpoint() {
    assert_eq!(
        theory::quotient(4, &[(3, 2), (2, 1)]),
        Some(vec![0, 1, 1, 1]),
        "transitive identifications use the least class representative"
    );
    assert_eq!(
        theory::quotient(3, &[(0, 3)]),
        None,
        "an out-of-bounds right endpoint is refused"
    );
    assert_eq!(
        theory::quotient(3, &[(3, 0)]),
        None,
        "an out-of-bounds left endpoint is refused"
    );
}

#[test]
fn reify_accepts_a_literal_tree_and_refuses_reserved_tree_vocabulary() {
    let tree = Tree {
        nodes: BTreeMap::from([(0, Node::new(0, "root"))]),
        arcs: Vec::new(),
        fans: Vec::new(),
        root: 0,
    };
    assert_eq!(
        encode::reify(&tree, None),
        Some(tree),
        "a one-node tree lies in both representation directions"
    );

    let fan_schema = Schema {
        hyper_edges: BTreeMap::from([(
            "membership".to_owned(),
            HyperEdge {
                kind: "hyper".to_owned(),
                signature: BTreeMap::from([("item".to_owned(), "child".to_owned())]),
                parent_label: "owner".to_owned(),
            },
        )]),
        ..Schema::default()
    };
    let fan_tree = Tree {
        nodes: BTreeMap::from([(0, Node::new(0, "parent")), (1, Node::new(1, "child"))]),
        arcs: vec![(
            0,
            1,
            SEdge::new("parent", "child", "contains", Some("item")),
        )],
        fans: vec![Fan {
            parent: 0,
            hyper_edge: "membership".to_owned(),
            children: BTreeMap::from([("item".to_owned(), 1)]),
        }],
        root: 0,
    };
    assert_eq!(
        encode::reify(&fan_tree, Some(&fan_schema)),
        Some(fan_tree),
        "an actual fan round-trips exactly through its graph representation"
    );

    let reserved_arc = Tree {
        nodes: BTreeMap::from([(0, Node::new(0, "parent")), (1, Node::new(1, "child"))]),
        arcs: vec![(
            0,
            1,
            SEdge::new("parent", "child", "hyper_label", Some("item")),
        )],
        fans: Vec::new(),
        root: 0,
    };
    assert_eq!(
        encode::reify(&reserved_arc, None),
        None,
        "a reserved arc kind is refused even between plain tree nodes"
    );

    let mut reserved_root = Node::new(0, "fan");
    reserved_root.shape = NodeShape::HyperEdge;
    let reserved = Tree {
        nodes: BTreeMap::from([(0, reserved_root)]),
        arcs: Vec::new(),
        fans: Vec::new(),
        root: 0,
    };
    assert_eq!(
        encode::reify(&reserved, None),
        None,
        "a tree already using the reserved fan-node shape has no graph reification"
    );
}

fn edge(src: u32, tgt: u32, kind: &str, name: Option<&str>) -> GEdge {
    GEdge {
        src,
        tgt,
        label: SEdge::new("fan", "child", kind, name),
        position: None,
    }
}

#[test]
fn fan_violations_return_every_reason_in_contract_order() {
    let mut fan_node = Node::new(1, "fan");
    fan_node.shape = NodeShape::HyperEdge;
    let graph = Graph {
        nodes: BTreeMap::from([
            (0, Node::new(0, "plain")),
            (1, fan_node),
            (2, Node::new(2, "child")),
            (3, Node::new(3, "child")),
        ]),
        edges: BTreeMap::from([
            (1, edge(0, 2, "hyper_parent", Some("parent"))),
            (2, edge(0, 3, "hyper_label", Some("outside"))),
            (3, edge(2, 1, "ordinary", Some("incoming"))),
            (4, edge(1, 2, "ordinary", Some("foreign"))),
            (5, edge(1, 2, "hyper_parent", Some("parent"))),
            (6, edge(1, 3, "hyper_parent", Some("parent"))),
            (7, edge(1, 2, "hyper_label", Some("z-dup"))),
            (8, edge(1, 3, "hyper_label", Some("z-dup"))),
            (9, edge(1, 2, "hyper_label", Some("a-dup"))),
            (10, edge(1, 3, "hyper_label", Some("a-dup"))),
            (11, edge(1, 2, "hyper_label", Some("b-unknown"))),
            (12, edge(1, 3, "hyper_label", Some("y-unknown"))),
            (13, edge(1, 3, "hyper_label", None)),
        ]),
    };
    assert!(
        graph.well_formed(),
        "the fan fixture is a well-formed graph"
    );

    let schema = Schema {
        hyper_edges: BTreeMap::from([(
            "fan".to_owned(),
            HyperEdge {
                kind: "hyper".to_owned(),
                signature: BTreeMap::from([
                    ("a-dup".to_owned(), "child".to_owned()),
                    ("z-dup".to_owned(), "child".to_owned()),
                ]),
                parent_label: "parent".to_owned(),
            },
        )]),
        ..Schema::default()
    };
    assert_eq!(
        fan::violations_at(&graph, 0, Some(&schema)),
        vec![
            fan::FanViolation::ReservedKindOutsideFan { edge: 1 },
            fan::FanViolation::ReservedKindOutsideFan { edge: 2 },
        ],
        "every reserved-kind edge outside a fan is reported in edge order"
    );
    assert_eq!(
        fan::violations_at(&graph, 1, Some(&schema)),
        vec![
            fan::FanViolation::ForeignIncidentEdge { edge: 3 },
            fan::FanViolation::ForeignIncidentEdge { edge: 4 },
            fan::FanViolation::ForeignIncidentEdge { edge: 13 },
            fan::FanViolation::MultipleParents { edges: vec![5, 6] },
            fan::FanViolation::DuplicateLabel {
                label: "a-dup".to_owned(),
                edges: vec![9, 10],
            },
            fan::FanViolation::DuplicateLabel {
                label: "z-dup".to_owned(),
                edges: vec![7, 8],
            },
            fan::FanViolation::UnknownLabel {
                label: "b-unknown".to_owned(),
            },
            fan::FanViolation::UnknownLabel {
                label: "y-unknown".to_owned(),
            },
        ],
        "incident, parent, duplicate-label, and schema-label defects retain contract order"
    );

    let mut unknown = Node::new(20, "undeclared");
    unknown.shape = NodeShape::HyperEdge;
    let missing = Graph {
        nodes: BTreeMap::from([(20, unknown)]),
        edges: BTreeMap::new(),
    };
    assert_eq!(
        fan::violations_at(&missing, 20, Some(&schema)),
        vec![
            fan::FanViolation::MissingParent,
            fan::FanViolation::UnknownHyperEdge {
                hyper_edge: "undeclared".to_owned(),
            },
        ],
        "missing-parent precedes unknown-hyper-edge and neither defect is dropped"
    );
}
