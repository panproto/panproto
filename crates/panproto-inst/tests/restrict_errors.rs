#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Errors of the restrict engines: the graph engine locates every error it
//! raises, the least key is the first error, plan compilation and boundary
//! refusals carry no key, and every error projects onto its detail.

use panproto_expr::{Expr, Literal};
use panproto_gat::Name;
use panproto_inst::segment::ErrorKey;
use panproto_inst::{
    BoundarySide, CompiledMigration, ErrorDetail, GInstance, Node, RestrictError, RestrictPlan,
    graph_restrict_with_plan,
};
use panproto_schema::{Edge, Protocol, Schema, SchemaBuilder};
use proptest::prelude::*;

fn schema(vertices: &[&str], edges: &[&Edge]) -> Schema {
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
    builder.build().unwrap()
}

fn e(src: &str, tgt: &str, name: &str) -> Edge {
    Edge {
        src: src.into(),
        tgt: tgt.into(),
        kind: "prop".into(),
        name: Some(name.into()),
    }
}

fn keep(vertices: &[&str]) -> CompiledMigration {
    CompiledMigration {
        surviving_verts: vertices.iter().map(|v| Name::from(*v)).collect(),
        ..CompiledMigration::default()
    }
}

fn run(
    g: &GInstance,
    src: &Schema,
    tgt: &Schema,
    m: &CompiledMigration,
) -> Result<GInstance, RestrictError> {
    let plan = RestrictPlan::compile(src, tgt, m)?;
    graph_restrict_with_plan(g, &plan).map(|(view, _)| view)
}

/// The fixture of the precedence tests: nodes anchored at `bad` fail their
/// survival predicate (it reads an unbound name), nodes anchored at `worse`
/// fail a transform (it reads an unbound name), and contracted pairs from
/// `p` through dropped `m` to `q` have no target edge while those to `q2`
/// have two.
fn precedence_schemas() -> (Schema, Schema, CompiledMigration) {
    let src = schema(
        &["bad", "worse", "p", "m", "q", "q2"],
        &[&e("p", "m", "pm"), &e("m", "q", "mq"), &e("m", "q2", "mq2")],
    );
    let tgt = schema(
        &["bad", "worse", "p", "q", "q2"],
        &[&e("p", "q2", "a"), &e("p", "q2", "b")],
    );
    let mut m = keep(&["bad", "worse", "p", "q", "q2"]);
    m.add_conditional_survival("bad", Expr::Var("unbound".into()));
    m.add_computed_field("worse", "out", Expr::Var("unbound".into()));
    (src, tgt, m)
}

#[test]
fn graph_restrict_returns_the_least_error() {
    let (src, tgt, m) = precedence_schemas();
    // Node 3 fails its predicate (kind 30), node 9 its transform (kind 9):
    // the node decides, not the kind.
    let g = GInstance::new()
        .with_node(Node::new(9, "worse"))
        .with_node(Node::new(3, "bad"));
    let error = run(&g, &src, &tgt, &m).unwrap_err();
    assert_eq!(
        error.key(),
        Some(ErrorKey {
            stage: 0,
            id: 3,
            id2: 0,
            kind: 30
        })
    );
    // A stage 0 error comes before a stage 2 one, whatever the ids.
    let g = GInstance::new()
        .with_node(Node::new(0, "p"))
        .with_node(Node::new(1, "m"))
        .with_node(Node::new(2, "q"))
        .with_node(Node::new(50, "worse"))
        .with_edge(0, 1, e("p", "m", "pm"))
        .with_edge(1, 2, e("m", "q", "mq"));
    let error = run(&g, &src, &tgt, &m).unwrap_err();
    assert_eq!(error.key().map(|k| (k.stage, k.id)), Some((0, 50)));
}

#[test]
fn error_keys_order_by_stage_then_ids_then_kind() {
    let key = |stage, id, id2, kind| ErrorKey {
        stage,
        id,
        id2,
        kind,
    };
    let mut keys = vec![
        key(2, 1, 5, 1),
        key(0, 9, 0, 9),
        key(2, 2, 3, 0),
        key(0, 3, 0, 30),
        key(1, 0, 0, 13),
        key(5, 0, 0, 29),
    ];
    keys.sort();
    assert_eq!(
        keys,
        vec![
            key(0, 3, 0, 30),
            key(0, 9, 0, 9),
            key(1, 0, 0, 13),
            key(2, 1, 5, 1),
            key(2, 2, 3, 0),
            key(5, 0, 0, 29),
        ]
    );
}

#[test]
fn stage_one_and_stage_two_no_edge_found_carry_different_keys() {
    // Stage 1: an edge r -> c whose expansion path (no intermediates) has
    // no target edge for its one hop (r, c).
    let src = schema(&["r", "c"], &[&e("r", "c", "c")]);
    let tgt = schema(&["r", "c"], &[]);
    let mut m = keep(&["r", "c"]);
    m.expansion_path.insert(("r".into(), "c".into()), vec![]);
    let g = GInstance::new()
        .with_node(Node::new(0, "r"))
        .with_node(Node::new(1, "c"))
        .with_edge(0, 1, e("r", "c", "c"));
    let edge_error = run(&g, &src, &tgt, &m).unwrap_err();
    // Stage 2: the pair (r, c) through a dropped m, with no target edge.
    let src2 = schema(&["r", "m", "c"], &[&e("r", "m", "m"), &e("m", "c", "c")]);
    let g2 = GInstance::new()
        .with_node(Node::new(0, "r"))
        .with_node(Node::new(1, "m"))
        .with_node(Node::new(2, "c"))
        .with_edge(0, 1, e("r", "m", "m"))
        .with_edge(1, 2, e("m", "c", "c"));
    let pair_error = run(&g2, &src2, &tgt, &keep(&["r", "c"])).unwrap_err();
    let payload = |error: &RestrictError| match error.unlocated() {
        RestrictError::NoEdgeFound { src, tgt } => (src.clone(), tgt.clone()),
        other => panic!("unexpected {other:?}"),
    };
    assert_eq!(payload(&edge_error), payload(&pair_error));
    assert_eq!(
        edge_error.key(),
        Some(ErrorKey {
            stage: 1,
            id: 0,
            id2: 0,
            kind: 0
        })
    );
    assert_eq!(
        pair_error.key(),
        Some(ErrorKey {
            stage: 2,
            id: 0,
            id2: 2,
            kind: 0
        })
    );
}

#[test]
fn graph_engine_errors_are_located_and_never_nested() {
    let (src, tgt, m) = precedence_schemas();
    let g = GInstance::new().with_node(Node::new(4, "bad"));
    let error = run(&g, &src, &tgt, &m).unwrap_err();
    let RestrictError::At { key, error: inner } = &error else {
        panic!("not located: {error:?}");
    };
    assert!(!matches!(**inner, RestrictError::At { .. }));
    assert_eq!(error.unlocated().to_string(), error.to_string());
    assert_eq!(error.kind_index(), inner.kind_index());
    assert_eq!(key.kind, inner.kind_index());
}

#[test]
fn plan_compilation_errors_carry_no_key() {
    let src = schema(&["r", "c"], &[&e("r", "c", "c")]);
    let tgt = schema(&["r", "c", "z"], &[&e("r", "z", "z")]);
    let mut m = keep(&["r", "c", "z"]);
    m.edge_remap.insert(e("r", "c", "c"), e("r", "z", "z"));
    let error = RestrictPlan::compile(&src, &tgt, &m).unwrap_err();
    assert_eq!(error.key(), None);
    assert!(std::ptr::eq(error.unlocated(), &raw const error));
}

#[test]
fn a_restrict_error_detail_carries_its_error_key() {
    let (src, tgt, m) = precedence_schemas();
    let g = GInstance::new().with_node(Node::new(4, "bad"));
    let error = run(&g, &src, &tgt, &m).unwrap_err();
    match error.detail() {
        ErrorDetail::Restrict { key, kind, message } => {
            assert_eq!(key, error.key());
            assert_eq!(kind, "survival_predicate_failed");
            assert_eq!(message, error.unlocated().to_string());
        }
        other => panic!("unexpected {other:?}"),
    }
    // A boundary failure projects onto its report.
    let report = panproto_inst::validate_graph(
        &GInstance::new().with_node(Node::new(0, "nowhere")),
        &src,
        &Protocol::default(),
    );
    let boundary = RestrictError::Validation {
        side: BoundarySide::Input,
        report: Box::new(report.clone()),
    };
    assert_eq!(boundary.detail(), ErrorDetail::Validation { report });
    // The appended variants have their snake-case kinds.
    let kinds = [
        (RestrictError::TablePlanRequired, "table_plan_required"),
        (
            RestrictError::IdsExhausted {
                kind: panproto_inst::segment::IdKind::Edge,
            },
            "ids_exhausted",
        ),
        (
            RestrictError::AmbiguousChildField {
                node: 1,
                field: "x".into(),
            },
            "ambiguous_child_field",
        ),
        (
            RestrictError::StructuralChildNotATree {
                node: 1,
                field: "x".into(),
            },
            "structural_child_not_a_tree",
        ),
        (
            RestrictError::ValueNotExpressible {
                node: 1,
                field: "x".into(),
                value_type: "token".into(),
            },
            "value_not_expressible",
        ),
        (
            RestrictError::PlanSchemaMismatch {
                side: BoundarySide::Output,
            },
            "plan_schema_mismatch",
        ),
        (
            RestrictError::NotTableRepresentable { reason: "r".into() },
            "not_table_representable",
        ),
        (
            RestrictError::ExpansionTooLong {
                src: "a".into(),
                tgt: "b".into(),
                hops: 65_537,
            },
            "expansion_too_long",
        ),
        (
            RestrictError::NonConforming {
                row: panproto_inst::RowId(0),
                column: Some("c".into()),
                reason: panproto_inst::NonConformance::MissingCell,
            },
            "non_conforming",
        ),
    ];
    for (error, kind) in kinds {
        match error.detail() {
            ErrorDetail::Restrict { kind: k, key, .. } => {
                assert_eq!(k, kind);
                assert_eq!(key, None);
            }
            other => panic!("unexpected {other:?}"),
        }
    }
}

#[test]
fn a_nonconforming_row_is_displayed_by_its_number_and_column_name() {
    let at_cell = RestrictError::NonConforming {
        row: panproto_inst::RowId(3),
        column: Some("c".into()),
        reason: panproto_inst::NonConformance::MissingCell,
    };
    assert_eq!(
        at_cell.to_string(),
        "row 3 does not conform to the source schema at column `c`: the row holds no cell at a \
         column of its table"
    );
    let at_row = RestrictError::NonConforming {
        row: panproto_inst::RowId(4),
        column: None,
        reason: panproto_inst::NonConformance::TableOutside {
            table: "ghost".into(),
        },
    };
    assert_eq!(
        at_row.to_string(),
        "row 4 does not conform to the source schema: `ghost` is no table of the schema"
    );
}

#[test]
fn adjunction_nonconforming_displays_a_sentence() {
    // The Kan extensions refuse a non-conforming input with a sentence that
    // names the row by its number and the column by its name, as the table
    // restrict does, never with the Debug forms of either.
    let at_cell = panproto_inst::AdjunctionError::NonConforming {
        row: panproto_inst::RowId(3),
        column: Some("c".into()),
        reason: panproto_inst::NonConformance::StrayCell,
    };
    assert_eq!(
        at_cell.to_string(),
        "row 3 does not conform to the schema at column `c`: the row holds a cell at no column \
         of its table"
    );
    let at_row = panproto_inst::AdjunctionError::NonConforming {
        row: panproto_inst::RowId(4),
        column: None,
        reason: panproto_inst::NonConformance::NestedLabeledNull,
    };
    assert_eq!(
        at_row.to_string(),
        "row 4 does not conform to the schema: the cell holds a labelled null inside a value"
    );
}

#[test]
fn a_restrict_error_fits_the_result_size_clippy_allows() {
    let size = std::mem::size_of::<RestrictError>();
    println!("size_of::<RestrictError>() = {size}");
    assert!(size <= 128, "{size}");
}

/// What an element of the precedence fixture is.
#[derive(Clone, Copy, Debug)]
enum Element {
    /// A node whose predicate fails.
    Bad,
    /// A node whose transform fails.
    Worse,
    /// A pair p -> m -> q with no target edge.
    NoEdgePair,
    /// A pair p -> m -> q2 with two target edges.
    AmbiguousPair,
    /// A node with no error.
    Fine,
}

fn element() -> impl Strategy<Value = Element> {
    prop_oneof![
        Just(Element::Bad),
        Just(Element::Worse),
        Just(Element::NoEdgePair),
        Just(Element::AmbiguousPair),
        Just(Element::Fine),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// The error the engine returns is the least key over the errors each
    /// element raises, built element by element; the elements are placed at
    /// shuffled ids so that node order and kind order disagree, and the
    /// least of the least keys of any split of the elements into two
    /// segments is the same key.
    #[test]
    fn error_key_minimum_is_the_whole_state_first_error(
        elements in prop::collection::vec(element(), 1..8),
        offsets in prop::collection::vec(0u32..1000, 8),
        split in prop::collection::vec(any::<bool>(), 8),
    ) {
        let (src, tgt, m) = precedence_schemas();
        let mut g = GInstance::new();
        let mut expected: Vec<(ErrorKey, bool)> = Vec::new();
        let mut used = std::collections::BTreeSet::new();
        for (i, element) in elements.iter().enumerate() {
            // Three distinct ids per element, spread by the offsets.
            let base = offsets[i] * 3 + u32::try_from(i).unwrap() * 3000;
            if !used.insert(base) {
                continue;
            }
            match element {
                Element::Bad | Element::Worse | Element::Fine => {
                    let anchor = match element {
                        Element::Bad => "bad",
                        Element::Worse => "worse",
                        _ => "p",
                    };
                    g = g.with_node(Node::new(base, anchor));
                    let kind = match element {
                        Element::Bad => Some(30),
                        Element::Worse => Some(9),
                        _ => None,
                    };
                    if let Some(kind) = kind {
                        expected.push((ErrorKey { stage: 0, id: base, id2: 0, kind }, split[i]));
                    }
                }
                Element::NoEdgePair | Element::AmbiguousPair => {
                    let (end, label, kind) = match element {
                        Element::NoEdgePair => ("q", e("m", "q", "mq"), 0),
                        _ => ("q2", e("m", "q2", "mq2"), 1),
                    };
                    g = g
                        .with_node(Node::new(base, "p"))
                        .with_node(Node::new(base + 1, "m"))
                        .with_node(Node::new(base + 2, end));
                    g = g.with_edge(base, base + 1, e("p", "m", "pm"));
                    g = g.with_edge(base + 1, base + 2, label);
                    expected.push((ErrorKey { stage: 2, id: base, id2: base + 2, kind }, split[i]));
                }
            }
        }
        let result = run(&g, &src, &tgt, &m);
        let least = expected.iter().map(|(k, _)| *k).min();
        prop_assert_eq!(result.as_ref().err().and_then(RestrictError::key), least);
        let segment_least = |side: bool| {
            expected
                .iter()
                .filter(|(_, s)| *s == side)
                .map(|(k, _)| *k)
                .min()
        };
        let of_segments = [segment_least(false), segment_least(true)].into_iter().flatten().min();
        prop_assert_eq!(of_segments, least);
    }

    /// Every error the graph engine returns is located, at a key whose kind
    /// is the inner error's.
    #[test]
    fn every_graph_engine_error_has_a_key(
        elements in prop::collection::vec(element(), 1..6),
    ) {
        let (src, tgt, m) = precedence_schemas();
        let mut g = GInstance::new();
        for (i, element) in elements.iter().enumerate() {
            let base = u32::try_from(i).unwrap() * 3;
            match element {
                Element::Bad => g = g.with_node(Node::new(base, "bad")),
                Element::Worse => g = g.with_node(Node::new(base, "worse")),
                Element::Fine => g = g.with_node(Node::new(base, "p")),
                Element::NoEdgePair | Element::AmbiguousPair => {
                    let (end, label) = match element {
                        Element::NoEdgePair => ("q", e("m", "q", "mq")),
                        _ => ("q2", e("m", "q2", "mq2")),
                    };
                    g = g
                        .with_node(Node::new(base, "p"))
                        .with_node(Node::new(base + 1, "m"))
                        .with_node(Node::new(base + 2, end))
                        .with_edge(base, base + 1, e("p", "m", "pm"))
                        .with_edge(base + 1, base + 2, label);
                }
            }
        }
        if let Err(error) = run(&g, &src, &tgt, &m) {
            let key = error.key();
            prop_assert!(key.is_some(), "{error:?}");
            let nested = matches!(error.unlocated(), RestrictError::At { .. });
            prop_assert!(!nested, "nested: {:?}", error);
            prop_assert_eq!(key.map(|k| k.kind), Some(error.unlocated().kind_index()));
        }
    }
}

#[test]
fn a_literal_predicate_needs_no_children() {
    // A predicate with no free variables binds nothing and evaluates.
    let s = schema(&["a"], &[]);
    let mut m = keep(&["a"]);
    m.add_conditional_survival("a", Expr::Lit(Literal::Bool(false)));
    let view = run(&GInstance::new().with_node(Node::new(0, "a")), &s, &s, &m).unwrap();
    assert_eq!(view.node_count(), 0);
}

/// The variant names of the enum `name` declared in `source`, in order.
fn declared_variants(source: &str, name: &str) -> Vec<String> {
    let start = source
        .find(&format!("pub enum {name} {{"))
        .expect("the enum is declared");
    let body = &source[start..];
    let end = body.find("\n}\n").expect("the enum is closed");
    body[..end]
        .lines()
        .skip(1)
        .filter_map(|line| {
            let rest = line.strip_prefix("    ")?;
            rest.chars().next()?.is_ascii_uppercase().then(|| {
                rest.chars()
                    .take_while(char::is_ascii_alphanumeric)
                    .collect()
            })
        })
        .collect()
}

#[test]
fn every_kind_index_is_its_variants_declaration_index() {
    // The arms of `kind_index` give each variant its position in the
    // declaration of `RestrictError`, every variant has an arm, and `At`,
    // whose index is its inner error's, holds a position no arm returns.
    let variants = declared_variants(include_str!("../src/error.rs"), "RestrictError");
    let plan = include_str!("../src/restrict_plan.rs");
    let start = plan.find("pub fn kind_index(&self) -> u8 {").unwrap();
    let body = &plan[start..];
    let body = &body[..body.find("\n    }\n").unwrap()];
    let mut arms: Vec<(String, Option<usize>)> = Vec::new();
    for line in body.lines() {
        let Some(rest) = line.trim().strip_prefix("Self::") else {
            continue;
        };
        let name: String = rest
            .chars()
            .take_while(char::is_ascii_alphanumeric)
            .collect();
        let index = rest
            .rsplit("=> ")
            .next()
            .and_then(|n| n.trim_end_matches(',').parse().ok());
        arms.push((name, index));
    }
    let names: Vec<&str> = arms.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(
        names,
        variants.iter().map(String::as_str).collect::<Vec<_>>(),
        "the arms are not the variants in declaration order"
    );
    for (position, (name, index)) in arms.iter().enumerate() {
        if name == "At" {
            assert_eq!(*index, None);
            assert!(arms.iter().all(|(_, i)| *i != Some(position)));
        } else {
            assert_eq!(*index, Some(position), "{name}");
        }
    }
    assert_eq!(
        variants.last().map(String::as_str),
        Some("NullIdsExhausted")
    );
}
