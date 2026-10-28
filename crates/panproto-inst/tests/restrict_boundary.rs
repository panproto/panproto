#![allow(clippy::unwrap_used, clippy::expect_used)]
//! The boundary of the schema-taking restrict and extend in every shape:
//! the input validated against the source, the output against the target,
//! a failing report returned with its side, and the validated forms over
//! cached plans equal to the schema-taking ones.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use panproto_expr::{BuiltinOp, Expr, Literal};
use panproto_gat::{CheckStatus, Name};
use panproto_inst::{
    BoundaryProtocols, BoundarySide, CompiledMigration, FInstance, GInstance, Node, RestrictError,
    RestrictPlan, RowId, ValidationOptions, ValidationPlan, Value, WInstance,
    functor_extend_validated, functor_restrict_validated, functor_restrict_with_complement,
    graph_extend, graph_extend_validated, graph_restrict, graph_restrict_validated,
    graph_restrict_with_complement, graph_restrict_with_plan, wtype_extend, wtype_restrict,
    wtype_restrict_validated, wtype_restrict_with_synthesis,
};
use panproto_schema::{Edge, Protocol, ScalarKind, Schema, SchemaBuilder};
use proptest::prelude::*;

fn e(src: &str, tgt: &str, name: &str) -> Edge {
    Edge {
        src: src.into(),
        tgt: tgt.into(),
        kind: "prop".into(),
        name: Some(name.into()),
    }
}

fn schema(vertices: &[&str], edges: &[&Edge], required: &[(&str, &Edge)]) -> Schema {
    let protocol = Protocol::default();
    let mut builder = SchemaBuilder::new(&protocol);
    for v in vertices {
        builder = builder.vertex(v, "record", None).unwrap();
    }
    for edge in edges {
        builder = builder
            .edge(&edge.src, &edge.tgt, &edge.kind, edge.name.as_deref())
            .unwrap();
    }
    for (vertex, edge) in required {
        builder = builder.required(vertex, vec![(*edge).clone()]);
    }
    builder.build().unwrap()
}

fn keep(vertices: &[&str]) -> CompiledMigration {
    CompiledMigration {
        surviving_verts: vertices.iter().map(|v| Name::from(*v)).collect(),
        ..CompiledMigration::default()
    }
}

const fn both(protocol: &Protocol) -> BoundaryProtocols<'_> {
    BoundaryProtocols {
        src: protocol,
        tgt: protocol,
    }
}

fn plan_for(schema: &Schema, protocol: &Protocol) -> ValidationPlan {
    ValidationPlan::compile(schema, protocol, &ValidationOptions::default())
}

/// `r -c-> c` where the target requires `r` to have a `c`, and a
/// migration whose predicate drops every `c` with `n > 1`.
fn required_child() -> (Schema, Schema, CompiledMigration) {
    let c = e("r", "c", "c");
    let src = schema(&["r", "c"], &[&c], &[]);
    let tgt = schema(&["r", "c"], &[&c], &[("r", &c)]);
    let mut m = keep(&["r", "c"]);
    m.add_conditional_survival(
        "c",
        Expr::Builtin(
            BuiltinOp::Lte,
            vec![Expr::Var("n".into()), Expr::Lit(Literal::Int(1))],
        ),
    );
    (src, tgt, m)
}

fn rc_graph(n: i64) -> GInstance {
    GInstance::new()
        .with_node(Node::new(0, "r"))
        .with_node(Node::new(1, "c").with_extra_field("n", Value::Int(n)))
        .with_edge(0, 1, e("r", "c", "c"))
}

fn rc_tree(n: i64) -> WInstance {
    WInstance::new(
        [
            (0, Node::new(0, "r")),
            (1, Node::new(1, "c").with_extra_field("n", Value::Int(n))),
        ]
        .into(),
        vec![(0, 1, e("r", "c", "c"))],
        vec![],
        0,
        "r".into(),
    )
}

fn side(result: Result<impl std::fmt::Debug, RestrictError>) -> Option<BoundarySide> {
    match result {
        Err(RestrictError::Validation { side, report }) => {
            assert_ne!(report.status, CheckStatus::Passed);
            Some(side)
        }
        _ => None,
    }
}

#[test]
fn graph_restrict_validates_both_boundaries() {
    let (src, tgt, m) = required_child();
    let p = Protocol::default();
    // A valid input that keeps its child restricts.
    assert!(graph_restrict(&rc_graph(1), &src, &tgt, &m, both(&p)).is_ok());
    // A valid input whose child the predicate drops leaves r without its
    // required child: the output side fails.
    assert_eq!(
        side(graph_restrict(&rc_graph(5), &src, &tgt, &m, both(&p))),
        Some(BoundarySide::Output)
    );
    // An input anchored outside the source fails on the input side.
    let stray = GInstance::new().with_node(Node::new(0, "nowhere"));
    assert_eq!(
        side(graph_restrict(&stray, &src, &tgt, &m, both(&p))),
        Some(BoundarySide::Input)
    );
}

#[test]
fn an_invalid_input_is_a_validation_error_on_the_source_side() {
    let (src, tgt, m) = required_child();
    let p = Protocol::default();
    let stray = rc_tree(1);
    let mut stray = stray;
    stray.nodes.insert(2, Node::new(2, "nowhere"));
    stray.arcs.push((0, 2, e("r", "nowhere", "x")));
    let stray = WInstance::new(stray.nodes, stray.arcs, vec![], 0, "r".into());
    assert_eq!(
        side(wtype_restrict(&stray, &src, &tgt, &m, both(&p))),
        Some(BoundarySide::Input)
    );
}

#[test]
fn an_invalid_output_is_a_validation_error_on_the_target_side() {
    let (src, tgt, m) = required_child();
    let p = Protocol::default();
    assert_eq!(
        side(wtype_restrict(&rc_tree(5), &src, &tgt, &m, both(&p))),
        Some(BoundarySide::Output)
    );
}

#[test]
fn restrict_boundary_errors_carry_the_side_and_report() {
    use panproto_reference::validation::{
        CheckStatus as ReferenceCheckStatus, ValidationBoundarySide, restriction_boundary,
    };

    assert_eq!(
        restriction_boundary(ReferenceCheckStatus::Failed, ReferenceCheckStatus::Failed),
        Err(ValidationBoundarySide::Input),
    );
    assert_eq!(
        restriction_boundary(ReferenceCheckStatus::Passed, ReferenceCheckStatus::Failed),
        Err(ValidationBoundarySide::Output),
    );
    assert_eq!(
        restriction_boundary(ReferenceCheckStatus::Passed, ReferenceCheckStatus::Passed),
        Ok(()),
    );
    let (src, tgt, m) = required_child();
    let p = Protocol::default();
    match graph_restrict(&rc_graph(5), &src, &tgt, &m, both(&p)) {
        Err(RestrictError::Validation { side, report }) => {
            assert_eq!(side, BoundarySide::Output);
            assert_eq!(report.status, CheckStatus::Failed);
            assert!(!report.diagnostics.is_empty());
            assert_eq!(report.shape, panproto_inst::InstanceShapeKind::Graph);
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn graph_restrict_is_the_first_projection() {
    let (src, tgt, m) = required_child();
    let p = Protocol::default();
    let (view, complement) =
        graph_restrict_with_complement(&rc_graph(1), &src, &tgt, &m, both(&p)).unwrap();
    assert!(GInstance::identical(
        &graph_restrict(&rc_graph(1), &src, &tgt, &m, both(&p)).unwrap(),
        &view
    ));
    assert!(complement.identity.is_none());
}

#[test]
fn graph_extend_validates_its_output() {
    let c = e("r", "c", "c");
    let src = schema(&["r", "c"], &[&c], &[]);
    let tgt = schema(&["r", "c"], &[&c], &[("r", &c)]);
    let mut m = keep(&["r", "c"]);
    m.surviving_edges.insert(c);
    let p = Protocol::default();
    assert!(graph_extend(&rc_graph(1), &src, &tgt, &m, both(&p)).is_ok());
    // Without its child, r fails the required edge.
    let lone = GInstance::new().with_node(Node::new(0, "r"));
    assert_eq!(
        side(graph_extend(&lone, &src, &tgt, &m, both(&p))),
        Some(BoundarySide::Output)
    );
    // The tree extend follows the same rule.
    let lone_tree = WInstance::new(
        [(0, Node::new(0, "r"))].into(),
        vec![],
        vec![],
        0,
        "r".into(),
    );
    assert_eq!(
        side(wtype_extend(&lone_tree, &src, &tgt, &m, both(&p))),
        Some(BoundarySide::Output)
    );
}

#[test]
fn graph_extend_validates_its_target() {
    let c = e("r", "c", "c");
    let tgt = schema(&["r", "c"], &[&c], &[]);
    let other = schema(&["q"], &[], &[]);
    let p = Protocol::default();
    let m = keep(&["r", "c"]);
    let src_plan = plan_for(&tgt, &p);
    let tgt_plan = plan_for(&other, &p);
    let migration_plan = RestrictPlan::compile_with_protocols(&tgt, &tgt, &m, both(&p)).unwrap();
    assert!(matches!(
        graph_extend_validated(&rc_graph(1), &src_plan, &tgt_plan, &migration_plan),
        Err(RestrictError::PlanSchemaMismatch {
            side: BoundarySide::Output
        })
    ));
}

#[test]
fn restrict_validated_equals_the_schema_taking_restrict() {
    let (src, tgt, m) = required_child();
    let p = Protocol::default();
    let src_plan = plan_for(&src, &p);
    let tgt_plan = plan_for(&tgt, &p);
    let plan = RestrictPlan::compile_with_protocols(&src, &tgt, &m, both(&p)).unwrap();
    for n in [1, 5] {
        // Graphs.
        let schema_taking = graph_restrict_with_complement(&rc_graph(n), &src, &tgt, &m, both(&p));
        let validated = graph_restrict_validated(&rc_graph(n), &src_plan, &tgt_plan, &plan);
        match (schema_taking, validated) {
            (Ok((a, _)), Ok((b, _))) => assert!(GInstance::identical(&a, &b)),
            (Err(a), Err(b)) => assert_eq!(format!("{a:?}"), format!("{b:?}")),
            (a, b) => panic!("{a:?} against {b:?}"),
        }
        // Trees.
        let schema_taking = wtype_restrict_with_synthesis(&rc_tree(n), &src, &tgt, &m, both(&p));
        let validated = wtype_restrict_validated(&rc_tree(n), &src_plan, &tgt_plan, &plan);
        match (schema_taking, validated) {
            // Trees hold their nodes in a hash map, so they are compared
            // through their serialized form, whose maps are sorted.
            (Ok((a, sa)), Ok((b, sb))) => {
                assert_eq!(
                    serde_json::to_value(&a).unwrap(),
                    serde_json::to_value(&b).unwrap()
                );
                assert_eq!(sa, sb);
            }
            (Err(a), Err(b)) => assert_eq!(format!("{a:?}"), format!("{b:?}")),
            (a, b) => panic!("{a:?} against {b:?}"),
        }
    }
    // Tables.
    let tp = Protocol {
        value_kinds: BTreeMap::from([(
            "int".to_owned(),
            ScalarKind::Int {
                bits: 64,
                signed: true,
            },
        )]),
        non_value_kinds: BTreeSet::from(["record".to_owned()]),
        ..Protocol::default()
    };
    let n = e("t", "t.n", "n");
    let table_schema = SchemaBuilder::new(&tp)
        .vertex("t", "record", None)
        .and_then(|b| b.vertex("t.n", "int", None))
        .and_then(|b| b.edge("t", "t.n", "prop", Some("n")))
        .and_then(SchemaBuilder::build)
        .unwrap();
    let mut tm = keep(&["t", "t.n"]);
    tm.surviving_edges.insert(n);
    let table = FInstance::new()
        .with_rows(
            "t",
            [(RowId(0), HashMap::from([("n".to_owned(), Value::Int(1))]))],
        )
        .unwrap();
    let table_plan =
        RestrictPlan::compile_for_tables(&table_schema, &table_schema, &tm, both(&tp)).unwrap();
    let validation = plan_for(&table_schema, &tp);
    assert_eq!(
        functor_restrict_with_complement(&table, &table_schema, &table_schema, &tm, both(&tp))
            .unwrap(),
        functor_restrict_validated(&table, &validation, &validation, &table_plan).unwrap()
    );
    assert_eq!(
        functor_extend_validated(&table, &validation, &validation, &table_plan).unwrap(),
        table
    );
}

#[test]
fn a_validated_restrict_refuses_plans_for_other_schemas() {
    let (src, tgt, m) = required_child();
    let p = Protocol::default();
    let plan = RestrictPlan::compile(&src, &tgt, &m).unwrap();
    let other = plan_for(&schema(&["q"], &[], &[]), &p);
    assert!(matches!(
        graph_restrict_validated(&rc_graph(1), &other, &plan_for(&tgt, &p), &plan),
        Err(RestrictError::PlanSchemaMismatch {
            side: BoundarySide::Input
        })
    ));
    assert!(matches!(
        graph_restrict_validated(&rc_graph(1), &plan_for(&src, &p), &other, &plan),
        Err(RestrictError::PlanSchemaMismatch {
            side: BoundarySide::Output
        })
    ));
    assert!(matches!(
        wtype_restrict_validated(&rc_tree(1), &other, &other, &plan),
        Err(RestrictError::PlanSchemaMismatch {
            side: BoundarySide::Input
        })
    ));
}

#[test]
fn restricting_the_probe_under_an_unrelated_schema_fails_at_input() {
    let unrelated = schema(&["u"], &[], &[]);
    let p = Protocol::default();
    assert_eq!(
        side(graph_restrict(
            &rc_graph(1),
            &unrelated,
            &unrelated,
            &keep(&["u"]),
            both(&p)
        )),
        Some(BoundarySide::Input)
    );
}

/// What the composition of V9 gives: the input side, then compilation,
/// then the engine, then the output side.
fn composition(
    g: &GInstance,
    src: &Schema,
    tgt: &Schema,
    m: &CompiledMigration,
    p: &Protocol,
) -> Result<GInstance, String> {
    let report = panproto_inst::validate_graph(g, src, p);
    if report.status != CheckStatus::Passed {
        return Err("input".into());
    }
    let plan = RestrictPlan::compile_with_protocols(src, tgt, m, both(p))
        .map_err(|e| format!("compile {e}"))?;
    let (view, _) = graph_restrict_with_plan(g, &plan).map_err(|e| format!("engine {e}"))?;
    if panproto_inst::validate_graph(&view, tgt, p).status != CheckStatus::Passed {
        return Err("output".into());
    }
    Ok(view)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// For every input, the schema-taking restrict is the composition of
    /// input validation, plan compilation, the engine and output
    /// validation. The generator keeps unresolved and transform-failing
    /// migrations: their error is a compilation or engine error.
    #[test]
    fn v9_restrict_boundary(
        ns in prop::collection::vec(-2i64..4, 0..4),
        stray in any::<bool>(),
        failing_transform in any::<bool>(),
        unresolved in any::<bool>(),
        required in any::<bool>(),
        through_m in any::<bool>(),
        incoherent in any::<bool>(),
    ) {
        let c = e("r", "c", "c");
        let src = schema(&["r", "m", "c"], &[&c, &e("r", "m", "m"), &e("m", "c", "mc")], &[]);
        let required_edges: Vec<(&str, &Edge)> = if required { vec![("r", &c)] } else { vec![] };
        let tgt = if unresolved {
            // No r -> c edge in the target: contraction through m and the
            // direct edge are unresolved.
            schema(&["r", "c"], &[], &[])
        } else {
            schema(&["r", "c"], &[&c], &required_edges)
        };
        let mut m = keep(&["r", "c"]);
        m.add_conditional_survival(
            "c",
            Expr::Builtin(BuiltinOp::Lte, vec![Expr::Var("n".into()), Expr::Lit(Literal::Int(1))]),
        );
        if failing_transform {
            m.add_computed_field("c", "y", Expr::Var("missing".into()));
        }
        if incoherent {
            // An edge remap whose image the target does not hold: a
            // compilation error.
            m.edge_remap.insert(c.clone(), e("r", "c", "elsewhere"));
        }
        let mut g = GInstance::new().with_node(Node::new(0, "r"));
        for (i, n) in ns.iter().enumerate() {
            let id = u32::try_from(i + 1).unwrap();
            g = g.with_node(Node::new(id, "c").with_extra_field("n", Value::Int(*n)));
            g = g.with_edge(0, id, c.clone());
        }
        if through_m {
            // A c reached only through a dropped m: a contracted pair, which
            // an unresolved target leaves without an edge.
            g = g
                .with_node(Node::new(50, "m"))
                .with_node(Node::new(51, "c").with_extra_field("n", Value::Int(0)))
                .with_edge(0, 50, e("r", "m", "m"))
                .with_edge(50, 51, e("m", "c", "mc"));
        }
        if stray {
            g = g.with_node(Node::new(99, "stray"));
        }
        let p = Protocol::default();
        let expected = composition(&g, &src, &tgt, &m, &p);
        let actual = graph_restrict(&g, &src, &tgt, &m, both(&p));
        match (&expected, &actual) {
            (Ok(a), Ok(b)) => prop_assert!(GInstance::identical(a, b)),
            (Err(kind), Err(RestrictError::Validation { side, .. })) => {
                let expected_side = if kind == "input" {
                    BoundarySide::Input
                } else {
                    BoundarySide::Output
                };
                prop_assert!(kind == "input" || kind == "output", "{kind}");
                prop_assert_eq!(*side, expected_side);
            }
            (Err(kind), Err(error)) => {
                prop_assert!(
                    kind.starts_with("compile") || kind.starts_with("engine"),
                    "{kind} {error}"
                );
                prop_assert!(kind.ends_with(&error.to_string()), "{kind} against {error}");
            }
            (a, b) => prop_assert!(false, "{:?} against {:?}", a, b),
        }
        // A valid input never fails on the input side.
        if panproto_inst::validate_graph(&g, &src, &p).status == CheckStatus::Passed {
            let input_side = matches!(
                actual,
                Err(RestrictError::Validation { side: BoundarySide::Input, .. })
            );
            prop_assert!(!input_side, "a valid input failed on the input side");
        }
    }
}
