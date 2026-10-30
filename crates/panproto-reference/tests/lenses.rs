#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Worked examples of graph restrict and of the graph lens.

use std::collections::BTreeMap;

use panproto_reference::fan::FanViolation;
use panproto_reference::lens::{GraphLens, LensError, PutDomainError, ViewRef};
use panproto_reference::migration::Migration;
use panproto_reference::model::{Fan, GEdge, Graph, Node, NodeShape, SEdge, Tree};
use panproto_reference::restrict::{
    self, Complement, EdgeFate, EngineError, ErrorKey, LayerRefusal, NoValueLayer, Plan, PlanError,
    PredicateRefusal, PutFieldRefusal, RestrictError, SynthKey, ValueLayer, ValueLayerSemantics,
};
use panproto_reference::schema::{HyperEdge, Schema};
use panproto_reference::value::{Presence, Value};

struct RefusingPredicate;

impl ValueLayer for RefusingPredicate {
    fn canonical_semantics(&self) -> ValueLayerSemantics {
        ValueLayerSemantics {
            domain: "panproto-reference/test-refusing-predicate",
            version: 1,
            canonical_payload: Vec::new(),
        }
    }

    fn survives(&self, _: &Graph, _: &Node) -> Result<bool, PredicateRefusal> {
        Err(PredicateRefusal::AmbiguousChildField {
            field: "child".into(),
        })
    }

    fn transform(
        &self,
        _: &Graph,
        node: &Node,
    ) -> Result<(BTreeMap<String, Value>, Option<Presence>), LayerRefusal> {
        Ok((node.fields.clone(), node.value.clone()))
    }

    fn restore(
        &self,
        view: &Node,
        original_fields: Option<&BTreeMap<String, Value>>,
        original_value: Option<&Option<Presence>>,
    ) -> Result<(BTreeMap<String, Value>, Option<Presence>), PutFieldRefusal> {
        Ok((
            original_fields
                .cloned()
                .unwrap_or_else(|| view.fields.clone()),
            original_value
                .cloned()
                .unwrap_or_else(|| view.value.clone()),
        ))
    }
}

#[test]
fn put_domain_error_census_is_exactly_thirteen_variants_in_structural_order() {
    let node = ViewRef::Node(7);
    let errors = vec![
        PutDomainError::IdCollision { elem: node.clone() },
        PutDomainError::SynthesizedEdited { elem: node.clone() },
        PutDomainError::ContractedEdgeInserted {
            elem: ViewRef::Edge(8),
        },
        PutDomainError::UnrepresentableViewEdge {
            elem: ViewRef::Edge(9),
        },
        PutDomainError::AmbiguousReverseAnchor {
            elem: node.clone(),
            candidates: vec!["a".into(), "b".into()],
        },
        PutDomainError::SurvivalPredicateFails { elem: node.clone() },
        PutDomainError::DerivedCellEdited {
            elem: node.clone(),
            field: "d".into(),
        },
        PutDomainError::TransformNotInvertible {
            elem: node.clone(),
            field: "t".into(),
        },
        PutDomainError::DuplicateKey {
            table: "T".into(),
            key: vec![("k".into(), Value::Str("v".into()))],
            elems: vec![node.clone()],
        },
        PutDomainError::MissingRequiredCell {
            elem: node.clone(),
            field: "r".into(),
        },
        PutDomainError::MalformedFan {
            node: 7,
            reason: FanViolation::MissingParent,
        },
        PutDomainError::SurvivalPredicateRefused {
            elem: node,
            refusal: PredicateRefusal::NotBoolean { got: "null".into() },
        },
        PutDomainError::KeyRefused {
            table: "T".into(),
            key: "k".into(),
            reason: panproto_reference::keys::KeyingError::NoCanonicalBytes,
        },
    ];

    assert_eq!(errors.len(), 13);
    assert_eq!(
        errors
            .iter()
            .map(|error| error.order_key().0)
            .collect::<Vec<_>>(),
        (0..13).collect::<Vec<_>>()
    );
}

#[test]
fn predicate_refusal_census_is_exactly_five_typed_variants() {
    let refusals = [
        PredicateRefusal::AmbiguousChildField { field: "a".into() },
        PredicateRefusal::StructuralChildNotATree { field: "s".into() },
        PredicateRefusal::ValueNotExpressible {
            field: "v".into(),
            value_type: "opaque".into(),
        },
        PredicateRefusal::EvaluationFailed {
            message: "failed".into(),
        },
        PredicateRefusal::NotBoolean { got: "null".into() },
    ];
    assert_eq!(refusals.len(), 5);
}

#[test]
fn graph_get_and_put_preserve_the_exact_typed_predicate_refusal() {
    let schema = Schema::default().vertex("v", "obj");
    let migration = Migration {
        surviving_verts: ["v".to_owned()].into(),
        conditional: ["v".to_owned()].into(),
        ..Migration::default()
    };
    let plan = restrict::compile(&schema, &schema, &migration).unwrap();
    let layer = RefusingPredicate;
    let lens = GraphLens::new(
        &schema,
        &schema,
        &migration,
        &plan,
        &BTreeMap::new(),
        &layer,
    )
    .unwrap();
    let graph = Graph {
        nodes: [(1, Node::new(1, "v"))].into(),
        edges: BTreeMap::new(),
    };
    let refusal = PredicateRefusal::AmbiguousChildField {
        field: "child".into(),
    };

    assert_eq!(
        lens.get(&graph),
        Err(RestrictError::At {
            key: ErrorKey {
                stage: 0,
                id: 1,
                id2: 0,
            },
            error: EngineError::Predicate(refusal.clone()),
        })
    );
    assert_eq!(
        lens.put(&graph, &Complement::default()),
        Err(LensError::PutDomain(
            PutDomainError::SurvivalPredicateRefused {
                elem: ViewRef::Node(1),
                refusal,
            }
        ))
    );
}

fn wrapper() -> (Schema, Schema, Migration, Graph) {
    let source = Schema::default()
        .vertex("r", "obj")
        .vertex("m", "obj")
        .vertex("leaf", "str")
        .edge("r", "m", "prop", Some("m"))
        .edge("m", "m", "prop", Some("m"))
        .edge("m", "leaf", "prop", Some("leaf"));
    let target = Schema::default()
        .vertex("r", "obj")
        .vertex("leaf", "str")
        .edge("r", "leaf", "prop", Some("leaf"));
    let mig = Migration {
        surviving_verts: ["r".to_owned(), "leaf".to_owned()].into(),
        resolver: [(
            ("r".to_owned(), "leaf".to_owned()),
            SEdge::new("r", "leaf", "prop", Some("leaf")),
        )]
        .into(),
        ..Migration::default()
    };
    let mut graph = Graph::default();
    for (id, a) in [(0, "r"), (1, "m"), (2, "m"), (3, "m"), (4, "leaf")] {
        graph.nodes.insert(id, Node::new(id, a));
    }
    let labels = [
        ("r", "m", "m"),
        ("m", "m", "m"),
        ("m", "m", "m"),
        ("m", "leaf", "leaf"),
    ];
    for (idx, (a, b, n)) in labels.iter().enumerate() {
        let idx = u32::try_from(idx).unwrap();
        graph.edges.insert(
            idx,
            GEdge {
                src: idx,
                tgt: idx + 1,
                label: SEdge::new(a, b, "prop", Some(n)),
                position: None,
            },
        );
    }
    (source, target, mig, graph)
}

#[test]
fn a_recursive_dropped_wrapper_contracts_to_one_edge_and_put_restores_it() {
    let (source, target, mig, graph) = wrapper();
    let plan = restrict::compile(&source, &target, &mig).unwrap();
    assert_eq!(plan.mode(), restrict::ContractionMode::Closure);
    let (view, mut c) =
        restrict::get_with_plan(&source, &target, &mig, &plan, &NoValueLayer, &graph).unwrap();
    assert_eq!(view.nodes.keys().copied().collect::<Vec<_>>(), vec![0, 4]);
    assert_eq!(view.edges.len(), 1);
    let (id, e) = view.edges.iter().next().unwrap();
    assert_eq!((*id, e.src, e.tgt), (4, 0, 4));
    assert!(matches!(
        c.synthesized_edges[&4],
        SynthKey::Contract { src: 0, tgt: 4, .. }
    ));
    let lens = GraphLens::new(
        &source,
        &target,
        &mig,
        &plan,
        &BTreeMap::new(),
        &NoValueLayer,
    )
    .unwrap();
    c.identity = Some(*lens.identity());
    let back = lens.put(&view, &c).unwrap();
    assert_eq!(back, graph);
    // Removing the synthesized edge alone is an edit of a synthesized object.
    let mut edited = view.clone();
    edited.edges.clear();
    assert_eq!(
        lens.put(&edited, &c),
        Err(LensError::PutDomain(PutDomainError::SynthesizedEdited {
            elem: ViewRef::Edge(4)
        }))
    );
    // Removing the leaf explains the missing edge; put cascades nothing
    // that the view did not remove.
    let mut no_leaf = view.clone();
    no_leaf.edges.clear();
    no_leaf.nodes.remove(&4);
    let restored = lens.put(&no_leaf, &c).unwrap();
    assert!(!restored.nodes.contains_key(&4));
    assert!(!restored.edges.contains_key(&3));
    // A view edge between r and leaf that is no synthesized edge is a new
    // contracted pair.
    let mut inserted = view.clone();
    inserted.edges.insert(
        9,
        GEdge {
            src: 0,
            tgt: 4,
            label: SEdge::new("r", "leaf", "prop", Some("leaf")),
            position: None,
        },
    );
    let errs = lens.check_put_domain(&inserted, &c).unwrap_err();
    assert!(errs.contains(&PutDomainError::ContractedEdgeInserted {
        elem: ViewRef::Edge(9)
    }));
}

#[test]
fn the_identity_lens_records_nothing_and_put_get_holds_on_edits() {
    let (s, _, _, g) = wrapper();
    let id = Migration {
        surviving_verts: s.vertices.keys().cloned().collect(),
        surviving_edges: s.edges.clone(),
        ..Migration::default()
    };
    let plan = restrict::compile(&s, &s, &id).unwrap();
    let (view, c) = restrict::get_with_plan(&s, &s, &id, &plan, &NoValueLayer, &g).unwrap();
    assert_eq!(view, g);
    assert!(
        c.dropped_nodes.is_empty() && c.original_edges.is_empty() && c.original_anchors.is_empty()
    );
    let lens = GraphLens::new(&s, &s, &id, &plan, &BTreeMap::new(), &NoValueLayer).unwrap();
    let mut v2 = view;
    v2.nodes
        .get_mut(&4)
        .unwrap()
        .fields
        .insert("x".into(), panproto_reference::value::Value::Int(1));
    v2.edges.remove(&3);
    v2.nodes.remove(&4);
    let s2 = lens.put(&v2, &c).unwrap();
    let (again, _) = restrict::get_with_plan(&s, &s, &id, &plan, &NoValueLayer, &s2).unwrap();
    assert_eq!(again, v2);
}

#[test]
fn a_view_node_reusing_a_dropped_id_collides() {
    let (source, target, mig, graph) = wrapper();
    let plan = restrict::compile(&source, &target, &mig).unwrap();
    let (mut view, mut c) =
        restrict::get_with_plan(&source, &target, &mig, &plan, &NoValueLayer, &graph).unwrap();
    view.nodes.insert(2, Node::new(2, "leaf"));
    let lens = GraphLens::new(
        &source,
        &target,
        &mig,
        &plan,
        &BTreeMap::new(),
        &NoValueLayer,
    )
    .unwrap();
    c.identity = Some(*lens.identity());
    assert_eq!(
        lens.put(&view, &c),
        Err(LensError::PutDomain(PutDomainError::IdCollision {
            elem: ViewRef::Node(2)
        }))
    );
}

fn expansion_compilation_fixture(intermediate_count: usize) -> (Schema, Schema, Migration) {
    let source = Schema::default()
        .vertex("a", "obj")
        .vertex("b", "obj")
        .edge("a", "b", "source", Some("ab"));
    let target = Schema::default()
        .vertex("a", "obj")
        .vertex("x", "obj")
        .vertex("b", "obj")
        .edge("a", "x", "hop", None)
        .edge("x", "x", "hop", None)
        .edge("x", "b", "hop", None);
    let migration = Migration {
        surviving_verts: ["a".to_owned(), "b".to_owned()].into(),
        expansion_path: [(
            ("a".to_owned(), "b".to_owned()),
            vec!["x".to_owned(); intermediate_count],
        )]
        .into(),
        ..Migration::default()
    };
    (source, target, migration)
}

#[test]
fn endpoint_resolver_does_not_choose_between_parallel_source_edges() {
    let e = SEdge::new("a", "a", "rel", Some("e"));
    let f = SEdge::new("a", "a", "rel", Some("f"));
    let source = Schema::default()
        .vertex("a", "obj")
        .edge("a", "a", "rel", Some("e"))
        .edge("a", "a", "rel", Some("f"));
    let target = Schema::default()
        .vertex("a", "obj")
        .edge("a", "a", "rel", Some("f"));
    let migration = Migration {
        surviving_verts: ["a".to_owned()].into(),
        surviving_edges: [f.clone()].into(),
        resolver: [(("a".to_owned(), "a".to_owned()), f.clone())].into(),
        ..Migration::default()
    };

    let plan = restrict::compile(&source, &target, &migration).unwrap();
    assert_eq!(plan.fates().get(&e), Some(&EdgeFate::Drop));
    assert_eq!(plan.fates().get(&f), Some(&EdgeFate::Keep(f)));
}

#[test]
fn endpoint_resolver_still_images_a_single_source_edge() {
    let e = SEdge::new("a", "a", "rel", Some("e"));
    let f = SEdge::new("a", "a", "rel", Some("f"));
    let source = Schema::default()
        .vertex("a", "obj")
        .edge("a", "a", "rel", Some("e"));
    let target = Schema::default()
        .vertex("a", "obj")
        .edge("a", "a", "rel", Some("f"));
    let migration = Migration {
        surviving_verts: ["a".to_owned()].into(),
        resolver: [(("a".to_owned(), "a".to_owned()), f.clone())].into(),
        ..Migration::default()
    };

    let plan = restrict::compile(&source, &target, &migration).unwrap();
    assert_eq!(plan.fates().get(&e), Some(&EdgeFate::Keep(f)));
}

#[test]
fn plan_compilation_accepts_65536_hops_and_refuses_65537() {
    {
        let (source, target, migration) = expansion_compilation_fixture(65_535);
        let plan = restrict::compile(&source, &target, &migration).unwrap();
        assert!(matches!(
            plan.fates().values().next(),
            Some(EdgeFate::Expand { labels, .. }) if labels.len() == 65_536
        ));
    }
    let (source, target, migration) = expansion_compilation_fixture(65_536);
    assert_eq!(
        restrict::compile(&source, &target, &migration),
        Err(PlanError::ExpansionTooLong {
            src: "a".to_owned(),
            tgt: "b".to_owned(),
            hops: 65_537,
        })
    );
}

#[test]
fn an_overlong_edge_expansion_is_refused_before_execution() {
    let (source, target, migration) = expansion_compilation_fixture(65_536);
    assert_eq!(
        restrict::compile(&source, &target, &migration),
        Err(PlanError::ExpansionTooLong {
            src: "a".to_owned(),
            tgt: "b".to_owned(),
            hops: 65_537,
        })
    );
}

#[test]
fn an_overlong_pair_expansion_is_refused_before_execution() {
    let source = Schema::default()
        .vertex("a", "obj")
        .vertex("m", "obj")
        .vertex("b", "obj")
        .edge("a", "m", "source", Some("am"))
        .edge("m", "b", "source", Some("mb"));
    let target = Schema::default()
        .vertex("ta", "obj")
        .vertex("x", "obj")
        .vertex("tb", "obj")
        .edge("ta", "x", "hop", None)
        .edge("x", "x", "hop", None)
        .edge("x", "tb", "hop", None);
    let migration = Migration {
        surviving_verts: ["ta".to_owned(), "tb".to_owned()].into(),
        vertex_remap: [
            ("a".to_owned(), "ta".to_owned()),
            ("b".to_owned(), "tb".to_owned()),
        ]
        .into(),
        expansion_path: [(
            ("ta".to_owned(), "tb".to_owned()),
            vec!["x".to_owned(); 65_536],
        )]
        .into(),
        ..Migration::default()
    };
    assert_eq!(
        restrict::compile(&source, &target, &migration),
        Err(PlanError::ExpansionTooLong {
            src: "ta".to_owned(),
            tgt: "tb".to_owned(),
            hops: 65_537,
        })
    );
}

fn fan_fixture() -> (Schema, Schema, Migration, Graph) {
    let mut source = Schema::default()
        .vertex("p", "obj")
        .vertex("a", "obj")
        .vertex("b", "obj");
    source.hyper_edges.insert(
        "h".to_owned(),
        HyperEdge {
            kind: "source".to_owned(),
            signature: [
                ("x".to_owned(), "a".to_owned()),
                ("y".to_owned(), "b".to_owned()),
            ]
            .into(),
            parent_label: "source_parent".to_owned(),
        },
    );
    let mut target = Schema::default().vertex("p", "obj").vertex("a", "obj");
    target.hyper_edges.insert(
        "target_h".to_owned(),
        HyperEdge {
            kind: "target".to_owned(),
            signature: [("z".to_owned(), "a".to_owned())].into(),
            parent_label: "target_parent".to_owned(),
        },
    );
    let migration = Migration {
        surviving_verts: ["p".to_owned(), "a".to_owned()].into(),
        ..Migration::default()
    };
    let mut graph = Graph::default();
    for (id, anchor) in [(0, "p"), (1, "a"), (2, "b"), (3, "h")] {
        graph.nodes.insert(id, Node::new(id, anchor));
    }
    graph.nodes.get_mut(&3).unwrap().shape = NodeShape::HyperEdge;
    for (id, tgt, kind, name) in [
        (10, 0, restrict::HYPER_PARENT, "source_parent"),
        (11, 1, restrict::HYPER_LABEL, "x"),
        (12, 2, restrict::HYPER_LABEL, "y"),
    ] {
        graph.edges.insert(
            id,
            GEdge {
                src: 3,
                tgt,
                label: SEdge::new("h", graph.nodes[&tgt].anchor.as_str(), kind, Some(name)),
                position: None,
            },
        );
    }
    (source, target, migration, graph)
}

fn empty_plan(src: &Schema, tgt: &Schema, migration: &Migration) -> Plan {
    restrict::compile(src, tgt, migration).unwrap()
}

#[test]
fn a_pruned_fan_uses_its_full_shape_and_the_target_parent_label() {
    let (source, target, mut migration, graph) = fan_fixture();
    migration.hyper_resolver.insert(
        ("h".to_owned(), ["x".to_owned(), "y".to_owned()].into()),
        (
            "target_h".to_owned(),
            [("x".to_owned(), "z".to_owned())].into(),
        ),
    );
    let plan = empty_plan(&source, &target, &migration);
    let (view, complement) =
        restrict::get_with_plan(&source, &target, &migration, &plan, &NoValueLayer, &graph)
            .unwrap();
    assert_eq!(view.nodes[&3].anchor, "target_h");
    assert_eq!(view.edges[&10].label.name.as_deref(), Some("target_parent"));
    assert_eq!(view.edges[&11].label.name.as_deref(), Some("z"));
    assert!(!view.edges.contains_key(&12));
    assert!(complement.dropped_edges.contains_key(&12));
}

#[test]
fn a_fan_uses_the_sole_unambiguous_resolver_entry() {
    let (source, target, mut migration, graph) = fan_fixture();
    migration.hyper_resolver.insert(
        ("h".to_owned(), ["not-the-fan-shape".to_owned()].into()),
        (
            "target_h".to_owned(),
            [("x".to_owned(), "z".to_owned())].into(),
        ),
    );
    let plan = empty_plan(&source, &target, &migration);
    let (view, _) =
        restrict::get_with_plan(&source, &target, &migration, &plan, &NoValueLayer, &graph)
            .unwrap();
    assert_eq!(view.nodes[&3].anchor, "target_h");
    assert_eq!(view.edges[&11].label.name.as_deref(), Some("z"));
}

#[test]
fn an_ambiguous_nonmatching_fan_resolver_has_no_fallback() {
    let (source, target, mut migration, graph) = fan_fixture();
    for shape in ["first", "second"] {
        migration.hyper_resolver.insert(
            ("h".to_owned(), [shape.to_owned()].into()),
            ("target_h".to_owned(), BTreeMap::new()),
        );
    }
    let plan = empty_plan(&source, &target, &migration);
    let (view, complement) =
        restrict::get_with_plan(&source, &target, &migration, &plan, &NoValueLayer, &graph)
            .unwrap();
    assert!(!view.nodes.contains_key(&3));
    assert_eq!(complement.dropped_nodes[&3], graph.nodes[&3]);
    assert!(
        [10, 11, 12]
            .into_iter()
            .all(|edge| complement.dropped_edges.contains_key(&edge))
    );
}

#[test]
fn colliding_fan_child_renames_are_refused_at_stage_three() {
    let (source, target, mut migration, graph) = fan_fixture();
    migration.surviving_verts.insert("b".to_owned());
    migration.hyper_resolver.insert(
        ("h".to_owned(), ["x".to_owned(), "y".to_owned()].into()),
        (
            "target_h".to_owned(),
            [
                ("x".to_owned(), "z".to_owned()),
                ("y".to_owned(), "z".to_owned()),
            ]
            .into(),
        ),
    );
    let plan = empty_plan(&source, &target, &migration);
    assert_eq!(
        restrict::get_with_plan(&source, &target, &migration, &plan, &NoValueLayer, &graph,),
        Err(RestrictError::At {
            key: ErrorKey {
                stage: 3,
                id: 3,
                id2: 0,
            },
            error: EngineError::FanReconstructionFailed {
                hyper_edge: "target_h".to_owned(),
                label: "z".to_owned(),
            },
        })
    );
}

#[test]
fn synthesized_nodes_refuse_every_noncanonical_coordinate() {
    let source = Schema::default()
        .vertex("a", "obj")
        .vertex("b", "obj")
        .edge("a", "b", "source", Some("ab"));
    let target = Schema::default()
        .vertex("a", "obj")
        .vertex("x", "obj")
        .vertex("b", "obj")
        .edge("a", "x", "hop", Some("ax"))
        .edge("x", "b", "hop", Some("xb"));
    let migration = Migration {
        surviving_verts: ["a".to_owned(), "b".to_owned()].into(),
        expansion_path: [(("a".to_owned(), "b".to_owned()), vec!["x".to_owned()])].into(),
        ..Migration::default()
    };
    let source_edge = source.edges.iter().next().unwrap().clone();
    let plan = restrict::compile(&source, &target, &migration).unwrap();
    let graph = Graph {
        nodes: [(0, Node::new(0, "a")), (1, Node::new(1, "b"))].into(),
        edges: [(
            0,
            GEdge {
                src: 0,
                tgt: 1,
                label: source_edge,
                position: None,
            },
        )]
        .into(),
    };
    let (view, mut complement) =
        restrict::get_with_plan(&source, &target, &migration, &plan, &NoValueLayer, &graph)
            .unwrap();
    let synthesized = *complement.synthesized_nodes.keys().next().unwrap();
    let lens = GraphLens::new(
        &source,
        &target,
        &migration,
        &plan,
        &BTreeMap::new(),
        &NoValueLayer,
    )
    .unwrap();
    complement.identity = Some(*lens.identity());

    for coordinate in 0..5 {
        let mut edited = view.clone();
        let node = edited.nodes.get_mut(&synthesized).unwrap();
        match coordinate {
            0 => {
                node.fields
                    .insert("field".to_owned(), panproto_reference::value::Value::Int(1));
            }
            1 => node.discriminator = Some("tag".to_owned()),
            2 => node.position = Some(0),
            3 => node.shape = NodeShape::List,
            4 => {
                node.annotations.insert(
                    "annotation".to_owned(),
                    panproto_reference::value::Value::Bool(true),
                );
            }
            _ => unreachable!(),
        }
        assert!(
            lens.check_put_domain(&edited, &complement)
                .unwrap_err()
                .contains(&PutDomainError::SynthesizedEdited {
                    elem: ViewRef::Node(synthesized),
                }),
            "coordinate {coordinate}"
        );
    }
}

#[test]
fn the_fan_domain_is_read_on_the_restorable_part() {
    let mut schema = Schema::default();
    schema.hyper_edges.insert(
        "declared".to_owned(),
        HyperEdge {
            kind: "kind".to_owned(),
            signature: BTreeMap::new(),
            parent_label: "parent".to_owned(),
        },
    );
    let migration = Migration::default();
    let plan = empty_plan(&schema, &schema, &migration);
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
    for (id, anchor) in [(4, "declared"), (9, "unknown")] {
        let mut node = Node::new(id, anchor);
        node.shape = NodeShape::HyperEdge;
        view.nodes.insert(id, node);
    }
    assert_eq!(
        lens.check_put_domain(&view, &Complement::default()),
        Err(vec![
            PutDomainError::AmbiguousReverseAnchor {
                elem: ViewRef::Node(9),
                candidates: Vec::new(),
            },
            // Node 9 has no source anchor, so it is outside the restorable
            // part on which the fan domain is read. Node 4 remains in that
            // part, where the fan-domain check reports its missing parent;
            // no survival predicate is declared by this migration.
            PutDomainError::MalformedFan {
                node: 4,
                reason: FanViolation::MissingParent,
            },
        ])
    );
}

#[test]
fn a_recorded_fan_edge_uses_only_its_governing_resolver_entry() {
    let (source, mut target, mut migration, graph) = fan_fixture();
    target.hyper_edges.get_mut("target_h").unwrap().signature =
        [("q".to_owned(), "a".to_owned())].into();
    migration.hyper_resolver.insert(
        ("h".to_owned(), ["x".to_owned(), "y".to_owned()].into()),
        (
            "target_h".to_owned(),
            [("x".to_owned(), "q".to_owned())].into(),
        ),
    );
    migration.hyper_resolver.insert(
        ("h".to_owned(), ["unrelated".to_owned()].into()),
        (
            "other_target".to_owned(),
            [("x".to_owned(), "z".to_owned())].into(),
        ),
    );
    let plan = empty_plan(&source, &target, &migration);
    let (mut view, mut complement) =
        restrict::get_with_plan(&source, &target, &migration, &plan, &NoValueLayer, &graph)
            .unwrap();
    assert!(complement.original_edges.contains_key(&11));
    assert_eq!(view.edges[&11].label.name.as_deref(), Some("q"));
    view.edges.get_mut(&11).unwrap().label.name = Some("z".to_owned());
    let lens = GraphLens::new(
        &source,
        &target,
        &migration,
        &plan,
        &BTreeMap::new(),
        &NoValueLayer,
    )
    .unwrap();
    complement.identity = Some(*lens.identity());
    assert!(
        lens.check_put_domain(&view, &complement)
            .unwrap_err()
            .contains(&PutDomainError::UnrepresentableViewEdge {
                elem: ViewRef::Edge(11),
            })
    );
}

#[test]
fn tree_put_relocates_a_fan_hidden_under_a_new_tree_node_id() {
    let edge = SEdge::new("r", "a", "prop", Some("a"));
    let mut schema = Schema::default()
        .vertex("r", "obj")
        .vertex("a", "obj")
        .edge("r", "a", "prop", Some("a"));
    schema.hyper_edges.insert(
        "h".to_owned(),
        HyperEdge {
            kind: "kind".to_owned(),
            signature: [("x".to_owned(), "a".to_owned())].into(),
            parent_label: "parent".to_owned(),
        },
    );
    let migration = Migration {
        surviving_verts: ["r".to_owned(), "a".to_owned()].into(),
        surviving_edges: [edge.clone()].into(),
        ..Migration::default()
    };
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
    let source = Tree {
        nodes: [(0, Node::new(0, "r")), (1, Node::new(1, "a"))].into(),
        arcs: vec![(0, 1, edge.clone())],
        fans: vec![Fan {
            parent: 0,
            hyper_edge: "h".to_owned(),
            children: [("x".to_owned(), 1)].into(),
        }],
        root: 0,
    };
    let (mut view, complement, embedding) = panproto_reference::lens::tree_get(&lens, &source)
        .unwrap()
        .unwrap();
    let hidden_fan = embedding.fans[0].0;
    view.nodes.insert(hidden_fan, Node::new(hidden_fan, "a"));
    view.arcs.push((0, hidden_fan, edge));
    let restored =
        panproto_reference::lens::tree_put(&lens, &view, &embedding, &complement).unwrap();
    assert_eq!(restored.nodes[&hidden_fan].anchor, "a");
    assert_eq!(restored.fans, source.fans);
    assert!(
        restored
            .arcs
            .iter()
            .any(|(_, child, _)| *child == hidden_fan)
    );
}
