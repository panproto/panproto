//! Exact observations of compiling, composing, inverting and lifting
//! migrations, and of the value-level coercion algebra.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use panproto_expr::{Expr, Literal};
use panproto_gat::{CoercionClass, Name};
use panproto_inst::{BoundaryProtocols, FInstance, RowId, Value, functor_restrict};
use panproto_mig::{
    COERCION_INPUT, ComposeReport, InvertError, Migration, compile, compose, compose_coercions,
    invert, invert_coercion, lift_functor,
};
use panproto_schema::{CoercionSpec, Edge, Protocol, ScalarKind, Schema, SchemaBuilder};

fn protocol() -> Protocol {
    Protocol {
        value_kinds: BTreeMap::from([(
            "int".to_owned(),
            ScalarKind::Int {
                bits: 64,
                signed: true,
            },
        )]),
        non_value_kinds: BTreeSet::from(["record".to_owned()]),
        ..Protocol::default()
    }
}

fn prop(src: &str, tgt: &str, name: &str) -> Edge {
    Edge {
        src: src.into(),
        tgt: tgt.into(),
        kind: "prop".into(),
        name: Some(name.into()),
    }
}

/// Record `T` with int vertices `vertices`, a `prop` edge from `T` to the
/// vertex of each `(name, vertex)` column, and, when `hyper_edge` is given,
/// a hyper-edge over `T` (parent label `parent`) with the `labels`.
fn table(
    vertices: &[&str],
    columns: &[(&str, &str)],
    hyper_edge: Option<(&str, &[(&str, &str)])>,
) -> Schema {
    let protocol = protocol();
    let mut builder = SchemaBuilder::new(&protocol)
        .vertex("T", "record", None)
        .unwrap();
    for v in vertices {
        builder = builder.vertex(v, "int", None).unwrap();
    }
    for (name, vertex) in columns {
        builder = builder.edge("T", vertex, "prop", Some(name)).unwrap();
    }
    if let Some((id, labels)) = hyper_edge {
        let mut signature: HashMap<String, String> = labels
            .iter()
            .map(|(l, v)| ((*l).to_owned(), (*v).to_owned()))
            .collect();
        signature.insert("parent".to_owned(), "T".to_owned());
        builder = builder.hyper_edge(id, "row", signature, "parent").unwrap();
    }
    builder.build().unwrap()
}

fn vertex_identity(vertices: &[&str]) -> Migration {
    let mut m = Migration::empty();
    for v in vertices {
        m.vertex_map.insert((*v).into(), (*v).into());
    }
    m
}

#[test]
fn compile_keeps_only_target_edges_whose_endpoints_both_survive() {
    let src = table(&["c"], &[("c", "c")], None);
    let tgt = table(&["c", "d"], &[("c", "c"), ("d", "d")], None);
    let m = vertex_identity(&["T", "c"]);
    let compiled = compile(&src, &tgt, &m).unwrap();
    assert_eq!(
        compiled.surviving_edges,
        HashSet::from([prop("T", "c", "c")])
    );
    assert_eq!(
        compiled.surviving_verts,
        HashSet::from([Name::from("T"), Name::from("c")])
    );
}

#[test]
fn an_edge_map_entry_that_agrees_with_the_label_rename_compiles() {
    let src = table(&["c"], &[("a", "c")], Some(("H", &[("a", "c")])));
    let tgt = table(&["c"], &[("b", "c")], Some(("H2", &[("b", "c")])));
    let mut m = vertex_identity(&["T", "c"]);
    m.hyper_edge_map.insert("H".into(), "H2".into());
    m.label_map
        .insert((Name::from("H"), Name::from("a")), Name::from("b"));
    m.edge_map.insert(prop("T", "c", "a"), prop("T", "c", "b"));
    let compiled = compile(&src, &tgt, &m).unwrap();
    assert_eq!(
        compiled.edge_remap,
        HashMap::from([(prop("T", "c", "a"), prop("T", "c", "b"))])
    );
}

#[test]
fn a_removed_label_whose_arrow_has_only_a_differently_named_image_compiles() {
    let src = table(&["c"], &[("a", "c")], Some(("H", &[("a", "c")])));
    let tgt = table(&["c"], &[("z", "c")], Some(("H2", &[])));
    let mut m = vertex_identity(&["T", "c"]);
    m.hyper_edge_map.insert("H".into(), "H2".into());
    let compiled = compile(&src, &tgt, &m).unwrap();
    assert!(compiled.edge_remap.is_empty());
    assert_eq!(
        compiled.surviving_edges,
        HashSet::from([prop("T", "c", "z")])
    );
}

fn full_report() -> ComposeReport {
    ComposeReport {
        dropped_vertices: vec![Name::from("v")],
        dropped_edges: vec![prop("a", "b", "e")],
        dropped_hyper_edges: vec![Name::from("h")],
        dropped_resolver_keys: vec![(Name::from("s"), Name::from("t"))],
        dropped_labels: vec![(Name::from("h"), Name::from("l"))],
    }
}

#[test]
fn a_compose_report_is_empty_exactly_when_it_drops_nothing() {
    assert!(ComposeReport::default().is_empty());
    let only = [
        ComposeReport {
            dropped_vertices: vec![Name::from("v")],
            ..ComposeReport::default()
        },
        ComposeReport {
            dropped_edges: vec![prop("a", "b", "e")],
            ..ComposeReport::default()
        },
        ComposeReport {
            dropped_hyper_edges: vec![Name::from("h")],
            ..ComposeReport::default()
        },
        ComposeReport {
            dropped_resolver_keys: vec![(Name::from("s"), Name::from("t"))],
            ..ComposeReport::default()
        },
        ComposeReport {
            dropped_labels: vec![(Name::from("h"), Name::from("l"))],
            ..ComposeReport::default()
        },
    ];
    for report in only {
        assert!(!report.is_empty(), "{report:?}");
    }
    assert!(!full_report().is_empty());
}

#[test]
fn a_compose_report_renders_one_line_per_drop() {
    assert!(ComposeReport::default().to_lines().is_empty());
    assert_eq!(
        full_report().to_lines(),
        vec![
            "compose dropped vertex `v` (removed by second migration)".to_owned(),
            "compose dropped edge `a->b` (removed by second migration)".to_owned(),
            "compose dropped hyper-edge `h` (removed by second migration)".to_owned(),
            "compose dropped resolver entry `(s, t)` (endpoint or edge removed by second migration)"
                .to_owned(),
            "compose dropped label `(h, l)` (hyper-edge removed by second migration)".to_owned(),
        ]
    );
}

/// Several first-step labels renamed onto one: the composite pulls the
/// second step's resolver entry back onto the least of them, on every run
/// and whatever order the label table iterates in.
#[test]
fn a_pulled_back_resolver_label_is_the_least_of_its_renamed_sources() {
    let sources = ["a", "b", "c", "d", "e", "f", "g", "h"];
    for _ in 0..64 {
        let mut m1 = Migration::empty();
        m1.hyper_edge_map.insert("H".into(), "H2".into());
        for l in sources {
            m1.label_map
                .insert((Name::from("H"), Name::from(l)), Name::from("x"));
        }
        let mut m2 = Migration::empty();
        m2.hyper_edge_map.insert("H2".into(), "H3".into());
        m2.hyper_resolver.insert(
            (Name::from("H2"), vec![Name::from("x")]),
            (
                Name::from("H3"),
                HashMap::from([(Name::from("x"), Name::from("y"))]),
            ),
        );
        let composite = compose(&m1, &m2).unwrap();
        assert_eq!(
            composite.hyper_resolver,
            HashMap::from([(
                (Name::from("H"), vec![Name::from("a")]),
                (
                    Name::from("H3"),
                    HashMap::from([(Name::from("a"), Name::from("y"))]),
                ),
            )])
        );
    }
}

#[test]
fn a_bijective_migration_with_a_hyper_edge_inverts() {
    let schema = table(&["c"], &[("c", "c")], Some(("H", &[("c", "c")])));
    let mut m = vertex_identity(&["T", "c"]);
    m.edge_map.insert(prop("T", "c", "c"), prop("T", "c", "c"));
    m.hyper_edge_map.insert("H".into(), "H".into());
    let inverse = invert(&m, &schema, &schema).unwrap();
    assert_eq!(
        inverse.hyper_edge_map,
        HashMap::from([(Name::from("H"), Name::from("H"))])
    );
    assert_eq!(inverse.vertex_map, m.vertex_map);
    assert_eq!(inverse.edge_map, m.edge_map);
}

#[test]
fn two_hyper_edges_onto_one_do_not_invert() {
    let schema = table(&["c"], &[("c", "c")], Some(("H", &[("c", "c")])));
    let mut m = vertex_identity(&["T", "c"]);
    m.edge_map.insert(prop("T", "c", "c"), prop("T", "c", "c"));
    m.hyper_edge_map.insert("H".into(), "H".into());
    m.hyper_edge_map.insert("G".into(), "H".into());
    assert!(matches!(
        invert(&m, &schema, &schema),
        Err(InvertError::HyperEdgeNotBijective { .. })
    ));
}

fn input() -> Expr {
    Expr::Var(COERCION_INPUT.into())
}

fn tagged(tag: i64) -> Expr {
    Expr::List(vec![input(), Expr::Lit(Literal::Int(tag))])
}

#[test]
fn a_composed_coercion_inverts_through_both_inverses() {
    let first = CoercionSpec {
        forward: tagged(1),
        inverse: Some(tagged(-1)),
        class: CoercionClass::Iso,
    };
    let second = CoercionSpec {
        forward: tagged(2),
        inverse: Some(tagged(-2)),
        class: CoercionClass::Iso,
    };
    let composite = compose_coercions(&first, &second);
    let nest = |inner: Expr, tag: i64| Expr::List(vec![inner, Expr::Lit(Literal::Int(tag))]);
    assert_eq!(composite.forward, nest(tagged(1), 2));
    assert_eq!(composite.inverse, Some(nest(tagged(-2), -1)));
    assert_eq!(composite.class, CoercionClass::Iso);

    let one_way = CoercionSpec {
        inverse: None,
        ..second
    };
    assert_eq!(compose_coercions(&first, &one_way).inverse, None);
}

#[test]
fn an_inverted_coercion_swaps_its_terms_and_dualizes_its_class() {
    for (class, reversed) in [
        (CoercionClass::Iso, CoercionClass::Iso),
        (CoercionClass::Retraction, CoercionClass::Projection),
        (CoercionClass::Projection, CoercionClass::Opaque),
        (CoercionClass::Opaque, CoercionClass::Opaque),
    ] {
        let spec = CoercionSpec {
            forward: tagged(1),
            inverse: Some(tagged(-1)),
            class,
        };
        assert_eq!(
            invert_coercion(&spec),
            Some(CoercionSpec {
                forward: tagged(-1),
                inverse: Some(tagged(1)),
                class: reversed,
            }),
            "{class:?}"
        );
    }
}

#[test]
fn lifting_a_table_instance_carries_its_surviving_cells() {
    let src = table(&["T.a", "T.b"], &[("a", "T.a"), ("b", "T.b")], None);
    let tgt = table(&["T.a"], &[("a", "T.a")], None);
    let m = vertex_identity(&["T", "T.a"]);
    let compiled = compile(&src, &tgt, &m).unwrap();
    let p = protocol();
    let protocols = BoundaryProtocols { src: &p, tgt: &p };
    let row = HashMap::from([
        ("a".to_owned(), Value::Int(1)),
        ("b".to_owned(), Value::Int(2)),
    ]);
    let instance = FInstance::new().with_rows("T", [(RowId(0), row)]).unwrap();
    let lifted = lift_functor(&compiled, &src, &tgt, &instance, protocols).unwrap();
    let cells: BTreeMap<String, Value> = lifted
        .row(RowId(0))
        .unwrap()
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    assert_eq!(cells, BTreeMap::from([("a".to_owned(), Value::Int(1))]));
    assert_eq!(
        lifted,
        functor_restrict(&instance, &src, &tgt, &compiled, protocols).unwrap()
    );
}
