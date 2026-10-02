#![allow(clippy::unwrap_used, clippy::expect_used)]
//! The table restrict engine and its boundary: rows survive by their
//! table's fate and their predicate, are transformed as every shape
//! transforms them, keep their ids across merges, and their pairs travel
//! by their foreign key's fate and contract with set semantics; the result
//! agrees with the graph engine on the table's graph form.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use panproto_expr::{BuiltinOp, Expr, Literal};
use panproto_gat::Name;
use panproto_inst::{
    BoundaryProtocols, BoundarySide, CompiledMigration, FInstance, GInstance, RestrictError,
    RestrictPlan, RowId, TableComplement, Value, functor_extend, functor_restrict,
    functor_restrict_with_complement, functor_restrict_with_plan, graph_restrict_with_plan,
};
use panproto_schema::{Edge, Protocol, ScalarKind, Schema, SchemaBuilder};
use proptest::prelude::*;

fn protocol() -> Protocol {
    Protocol {
        value_kinds: BTreeMap::from([
            ("string".to_owned(), ScalarKind::Str),
            (
                "int".to_owned(),
                ScalarKind::Int {
                    bits: 64,
                    signed: true,
                },
            ),
        ]),
        non_value_kinds: BTreeSet::from(["record".to_owned()]),
        ..Protocol::default()
    }
}

const fn protocols(p: &Protocol) -> BoundaryProtocols<'_> {
    BoundaryProtocols { src: p, tgt: p }
}

/// A schema of tables, each with `(column, kind)` columns, and the given
/// relations between tables.
fn tables(spec: &[(&str, &[(&str, &str)])], relations: &[&Edge]) -> Schema {
    let p = protocol();
    let mut builder = SchemaBuilder::new(&p);
    for (table, columns) in spec {
        builder = builder.vertex(table, "record", None).unwrap();
        for (column, kind) in *columns {
            let vertex = format!("{table}.{column}");
            builder = builder
                .vertex(&vertex, kind, None)
                .unwrap()
                .edge(table, &vertex, "prop", Some(column))
                .unwrap();
        }
    }
    for edge in relations {
        builder = builder
            .edge(&edge.src, &edge.tgt, &edge.kind, edge.name.as_deref())
            .unwrap();
    }
    builder.build().unwrap()
}

fn column(table: &str, name: &str) -> Edge {
    Edge {
        src: table.into(),
        tgt: format!("{table}.{name}").into(),
        kind: "prop".into(),
        name: Some(name.into()),
    }
}

fn fk(src: &str, tgt: &str, name: &str) -> Edge {
    Edge {
        src: src.into(),
        tgt: tgt.into(),
        kind: "fk".into(),
        name: Some(name.into()),
    }
}

/// The identity on `schema`, as `panproto_mig::compile` writes it.
fn identity(schema: &Schema) -> CompiledMigration {
    CompiledMigration {
        surviving_verts: schema.vertices.keys().cloned().collect(),
        surviving_edges: schema.edges.keys().cloned().collect(),
        ..CompiledMigration::default()
    }
}

fn cells(pairs: &[(&str, Value)]) -> HashMap<String, Value> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), v.clone()))
        .collect()
}

fn plan(src: &Schema, tgt: &Schema, m: &CompiledMigration) -> RestrictPlan {
    let p = protocol();
    RestrictPlan::compile_for_tables(src, tgt, m, protocols(&p)).unwrap()
}

fn people() -> (Schema, FInstance) {
    let schema = tables(&[("person", &[("name", "string"), ("age", "int")])], &[]);
    let f = FInstance::new()
        .with_rows(
            "person",
            [
                (
                    RowId(0),
                    cells(&[("name", Value::Str("Ann".into())), ("age", Value::Int(10))]),
                ),
                (
                    RowId(1),
                    cells(&[("name", Value::Str("Bo".into())), ("age", Value::Int(40))]),
                ),
            ],
        )
        .unwrap();
    (schema, f)
}

fn adults(m: &mut CompiledMigration) {
    m.add_conditional_survival(
        "person",
        Expr::Builtin(
            BuiltinOp::Gt,
            vec![Expr::Var("age".into()), Expr::Lit(Literal::Int(18))],
        ),
    );
}

#[test]
fn functor_restrict_applies_conditional_survival() {
    let (schema, f) = people();
    let mut m = identity(&schema);
    adults(&mut m);
    let (view, complement) = functor_restrict_with_plan(&f, &plan(&schema, &schema, &m)).unwrap();
    assert_eq!(view.row_ids(), vec![RowId(1)]);
    assert_eq!(
        complement.dropped_rows.keys().copied().collect::<Vec<_>>(),
        vec![RowId(0)]
    );
}

#[test]
fn functor_restrict_applies_field_transforms() {
    let (schema, f) = people();
    let mut m = identity(&schema);
    m.add_field_drop("person", "age");
    let (view, _) = functor_restrict_with_plan(&f, &plan(&schema, &schema, &m)).unwrap();
    assert!(
        view.rows("person")
            .iter()
            .all(|(_, r)| !r.contains_key("age"))
    );
    let mut m = identity(&schema);
    m.add_field_rename("person", "name", "label");
    m.add_field_keep("person", &["label"]);
    let (view, _) = functor_restrict_with_plan(&f, &plan(&schema, &schema, &m)).unwrap();
    assert_eq!(
        view.row(RowId(0)).unwrap(),
        &cells(&[("label", Value::Str("Ann".into()))])
    );
}

#[test]
fn functor_restrict_applies_op_terms_and_value_transforms() {
    let (schema, f) = people();
    let mut m = identity(&schema);
    m.op_term_assignments.insert(
        "person".into(),
        vec![panproto_inst::TermAssignment::Compute {
            target: "age".into(),
            scope: panproto_inst::TermScope::Row,
            term: Expr::Builtin(
                BuiltinOp::Add,
                vec![Expr::Var("age".into()), Expr::Lit(Literal::Int(1))],
            ),
            inverse: None,
            coercion_class: panproto_gat::CoercionClass::Opaque,
        }],
    );
    let (view, _) = functor_restrict_with_plan(&f, &plan(&schema, &schema, &m)).unwrap();
    assert_eq!(
        view.row(RowId(0)).unwrap().get("age"),
        Some(&Value::Int(11))
    );
    // The tree oracle computes the same value at a node holding the cells.
    let tree = panproto_inst::WInstance::new(
        [(
            0,
            panproto_inst::Node::new(0, "person").with_extra_field("age", Value::Int(10)),
        )]
        .into(),
        vec![],
        vec![],
        0,
        "person".into(),
    );
    let (tree_view, _) = panproto_inst::wtype_restrict_with_plan(
        &tree,
        &RestrictPlan::compile(&schema, &schema, &m).unwrap(),
    )
    .unwrap();
    assert_eq!(
        tree_view.nodes[&0].extra_fields.get("age"),
        Some(&Value::Int(11))
    );
}

#[test]
fn functor_restrict_drops_schema_dropped_columns() {
    let src = tables(&[("t", &[("a", "int"), ("b", "int")])], &[]);
    let tgt = tables(&[("t", &[("a", "int")])], &[]);
    let m = CompiledMigration {
        surviving_verts: ["t", "t.a"].iter().map(|v| Name::from(*v)).collect(),
        surviving_edges: [column("t", "a")].into(),
        ..CompiledMigration::default()
    };
    let f = FInstance::new()
        .with_rows(
            "t",
            [(
                RowId(0),
                cells(&[("a", Value::Int(1)), ("b", Value::Int(2))]),
            )],
        )
        .unwrap();
    let (view, complement) = functor_restrict_with_plan(&f, &plan(&src, &tgt, &m)).unwrap();
    assert_eq!(view.row(RowId(0)).unwrap(), &cells(&[("a", Value::Int(1))]));
    assert_eq!(
        complement.dropped_cells,
        BTreeMap::from([((RowId(0), "b".to_owned()), Value::Int(2))])
    );
}

#[test]
fn functor_restrict_remaps_fk_edges_through_edge_image() {
    let src = tables(&[("a", &[]), ("b", &[])], &[&fk("a", "b", "old")]);
    let tgt = tables(&[("a", &[]), ("b", &[])], &[&fk("a", "b", "new")]);
    let mut m = identity(&tgt);
    m.edge_remap
        .insert(fk("a", "b", "old"), fk("a", "b", "new"));
    let f = FInstance::new()
        .with_rows("a", [(RowId(0), HashMap::new())])
        .and_then(|f| f.with_rows("b", [(RowId(1), HashMap::new())]))
        .and_then(|f| f.with_foreign_key(fk("a", "b", "old"), [(RowId(0), RowId(1))]))
        .unwrap();
    let (view, complement) = functor_restrict_with_plan(&f, &plan(&src, &tgt, &m)).unwrap();
    assert_eq!(
        view.pairs(&fk("a", "b", "new")),
        &BTreeSet::from([(RowId(0), RowId(1))])
    );
    assert!(
        complement.original_fk_edges.is_empty(),
        "one preimage maps back"
    );
}

#[test]
fn functor_restrict_contracts_fk_pairs_with_set_semantics() {
    // a -> m -> b with m dropped; two m rows between one a and one b give
    // one contracted pair.
    let src = tables(
        &[("a", &[]), ("m", &[]), ("b", &[])],
        &[&fk("a", "m", "am"), &fk("m", "b", "mb")],
    );
    let tgt = tables(&[("a", &[]), ("b", &[])], &[&fk("a", "b", "ab")]);
    let m = identity(&tgt);
    let f = FInstance::new()
        .with_rows("a", [(RowId(0), HashMap::new())])
        .and_then(|f| {
            f.with_rows(
                "m",
                [(RowId(1), HashMap::new()), (RowId(2), HashMap::new())],
            )
        })
        .and_then(|f| f.with_rows("b", [(RowId(3), HashMap::new())]))
        .and_then(|f| {
            f.with_foreign_key(
                fk("a", "m", "am"),
                [(RowId(0), RowId(1)), (RowId(0), RowId(2))],
            )
        })
        .and_then(|f| {
            f.with_foreign_key(
                fk("m", "b", "mb"),
                [(RowId(1), RowId(3)), (RowId(2), RowId(3))],
            )
        })
        .unwrap();
    let (view, complement) = functor_restrict_with_plan(&f, &plan(&src, &tgt, &m)).unwrap();
    assert_eq!(
        view.pairs(&fk("a", "b", "ab")),
        &BTreeSet::from([(RowId(0), RowId(3))])
    );
    assert_eq!(
        complement.synthesized_fk_pairs,
        BTreeSet::from([(fk("a", "b", "ab"), RowId(0), RowId(3))])
    );
}

#[test]
fn contracted_pairs_are_keyed_by_their_contract_synth_key() {
    // The graph form keys the same pair Contract { src: s.0, tgt: t.0, edge }.
    let src = tables(
        &[("a", &[]), ("m", &[]), ("b", &[])],
        &[&fk("a", "m", "am"), &fk("m", "b", "mb")],
    );
    let tgt = tables(&[("a", &[]), ("b", &[])], &[&fk("a", "b", "ab")]);
    let m = identity(&tgt);
    let f = FInstance::new()
        .with_rows("a", [(RowId(0), HashMap::new())])
        .and_then(|f| f.with_rows("m", [(RowId(1), HashMap::new())]))
        .and_then(|f| f.with_rows("b", [(RowId(2), HashMap::new())]))
        .and_then(|f| f.with_foreign_key(fk("a", "m", "am"), [(RowId(0), RowId(1))]))
        .and_then(|f| f.with_foreign_key(fk("m", "b", "mb"), [(RowId(1), RowId(2))]))
        .unwrap();
    let (_, complement) = functor_restrict_with_plan(&f, &plan(&src, &tgt, &m)).unwrap();
    let (graph, _) = GInstance::from_finstance(&f).unwrap();
    let (_, graph_complement) =
        graph_restrict_with_plan(&graph, &RestrictPlan::compile(&src, &tgt, &m).unwrap()).unwrap();
    let keys: BTreeSet<panproto_inst::SynthKey> = complement
        .synthesized_fk_pairs
        .iter()
        .map(|(edge, s, t)| panproto_inst::SynthKey::Contract {
            src: s.0,
            tgt: t.0,
            edge: edge.clone(),
        })
        .collect();
    assert_eq!(
        keys,
        graph_complement
            .synthesized_edges
            .values()
            .cloned()
            .collect()
    );
}

#[test]
fn a_vertex_merge_keeps_row_ids_and_records_the_source_table() {
    let src = tables(
        &[("a", &[("n", "string")]), ("b", &[("n", "string")])],
        &[&fk("a", "b", "link")],
    );
    let tgt = tables(&[("t", &[("n", "string")])], &[&fk("t", "t", "link")]);
    let mut m = identity(&tgt);
    for table in ["a", "b"] {
        m.vertex_remap.insert(table.into(), "t".into());
        m.vertex_remap
            .insert(format!("{table}.n").into(), "t.n".into());
        m.edge_remap.insert(column(table, "n"), column("t", "n"));
    }
    m.edge_remap
        .insert(fk("a", "b", "link"), fk("t", "t", "link"));
    let n = |v: &str| cells(&[("n", Value::Str(v.into()))]);
    let f = FInstance::new()
        .with_rows("a", [(RowId(0), n("a0")), (RowId(1), n("a1"))])
        .and_then(|f| f.with_rows("b", [(RowId(2), n("b0")), (RowId(3), n("b1"))]))
        .and_then(|f| f.with_foreign_key(fk("a", "b", "link"), [(RowId(1), RowId(2))]))
        .unwrap();
    let (view, complement) = functor_restrict_with_plan(&f, &plan(&src, &tgt, &m)).unwrap();
    assert_eq!(view.row_count("t"), 4);
    assert_eq!(
        view.pairs(&fk("t", "t", "link")),
        &BTreeSet::from([(RowId(1), RowId(2))])
    );
    assert_eq!(
        view.row(RowId(2)).unwrap().get("n"),
        Some(&Value::Str("b0".into()))
    );
    assert_eq!(
        complement.original_tables,
        BTreeMap::from([
            (RowId(0), Name::from("a")),
            (RowId(1), Name::from("a")),
            (RowId(2), Name::from("b")),
            (RowId(3), Name::from("b")),
        ])
    );
}

#[test]
fn an_injective_table_rename_records_no_original_table() {
    let src = tables(&[("a", &[])], &[]);
    let tgt = tables(&[("b", &[])], &[]);
    let mut m = identity(&tgt);
    m.vertex_remap.insert("a".into(), "b".into());
    let f = FInstance::new()
        .with_rows("a", [(RowId(0), HashMap::new())])
        .unwrap();
    let (view, complement) = functor_restrict_with_plan(&f, &plan(&src, &tgt, &m)).unwrap();
    assert_eq!(view.table_of(RowId(0)).map(|t| &**t), Some("b"));
    assert!(complement.original_tables.is_empty());
}

#[test]
fn original_fk_edges_records_exactly_the_pairs_that_would_not_map_back() {
    // Two source keys onto one: each view pair records the keys that
    // carried it; a pair both carried records both.
    let src = tables(&[("a", &[])], &[&fk("a", "a", "x"), &fk("a", "a", "y")]);
    let tgt = tables(&[("a", &[])], &[&fk("a", "a", "z")]);
    let mut m = identity(&tgt);
    m.edge_remap.insert(fk("a", "a", "x"), fk("a", "a", "z"));
    m.edge_remap.insert(fk("a", "a", "y"), fk("a", "a", "z"));
    let f = FInstance::new()
        .with_rows(
            "a",
            [(RowId(0), HashMap::new()), (RowId(1), HashMap::new())],
        )
        .and_then(|f| f.with_foreign_key(fk("a", "a", "x"), [(RowId(0), RowId(1))]))
        .and_then(|f| {
            f.with_foreign_key(
                fk("a", "a", "y"),
                [(RowId(0), RowId(1)), (RowId(1), RowId(0))],
            )
        })
        .unwrap();
    let (_, complement) = functor_restrict_with_plan(&f, &plan(&src, &tgt, &m)).unwrap();
    assert_eq!(
        complement.original_fk_edges,
        BTreeMap::from([
            (
                (fk("a", "a", "z"), RowId(0), RowId(1)),
                BTreeSet::from([fk("a", "a", "x"), fk("a", "a", "y")])
            ),
            (
                (fk("a", "a", "z"), RowId(1), RowId(0)),
                BTreeSet::from([fk("a", "a", "y")])
            ),
        ])
    );
}

#[test]
fn table_complement_fields_serialize_in_contract_order() {
    let json = serde_json::to_string(&TableComplement::empty()).unwrap();
    let order = [
        "dropped_rows",
        "dropped_cells",
        "dropped_fk_pairs",
        "original_fk_edges",
        "original_tables",
        "original_cells",
        "synthesized_fk_pairs",
        "id_floor",
        "identity",
    ];
    let positions: Vec<usize> = order
        .iter()
        .map(|key| json.find(&format!("\"{key}\":")).unwrap())
        .collect();
    assert!(positions.windows(2).all(|w| w[0] < w[1]), "{json}");
    // A complement with data round-trips through JSON.
    let (schema, f) = people();
    let mut m = identity(&schema);
    adults(&mut m);
    m.add_field_drop("person", "name");
    let (_, complement) = functor_restrict_with_plan(&f, &plan(&schema, &schema, &m)).unwrap();
    let back: TableComplement =
        serde_json::from_str(&serde_json::to_string(&complement).unwrap()).unwrap();
    assert_eq!(back, complement);
}

#[test]
fn table_residue_ignores_synthesized_pairs_floor_and_identity() {
    let mut complement = TableComplement::empty();
    complement.id_floor = 9;
    complement
        .synthesized_fk_pairs
        .insert((fk("a", "b", "ab"), RowId(0), RowId(1)));
    assert!(complement.residue_is_trivial());
    assert!(!complement.is_empty());
    complement
        .original_cells
        .insert((RowId(0), "x".into()), Value::Int(1));
    assert!(!complement.residue_is_trivial());
}

#[test]
fn reserved_row_ids_are_the_dropped_rows() {
    let (schema, f) = people();
    let mut m = identity(&schema);
    adults(&mut m);
    let (_, complement) = functor_restrict_with_plan(&f, &plan(&schema, &schema, &m)).unwrap();
    assert_eq!(complement.reserved_row_ids(), BTreeSet::from([RowId(0)]));
    assert_eq!(complement.id_floor, 2);
}

#[test]
fn the_table_engine_requires_a_table_plan() {
    let (schema, f) = people();
    let m = identity(&schema);
    let graph_plan = RestrictPlan::compile(&schema, &schema, &m).unwrap();
    assert!(matches!(
        functor_restrict_with_plan(&f, &graph_plan),
        Err(RestrictError::TablePlanRequired)
    ));
}

#[test]
fn table_restrict_refuses_an_unclassified_kind() {
    let p = protocol();
    let mut b = SchemaBuilder::new(&p).vertex("t", "mystery", None).unwrap();
    b = b.vertex("u", "record", None).unwrap();
    let schema = b.build().unwrap();
    let f = FInstance::new();
    let error =
        functor_restrict(&f, &schema, &schema, &identity(&schema), protocols(&p)).unwrap_err();
    assert!(matches!(error, RestrictError::RelSchema(_)), "{error:?}");
}

#[test]
fn functor_extend_classifies_by_the_target_schema() {
    let p = protocol();
    let (schema, f) = people();
    let extended = functor_extend(&f, &schema, &identity(&schema), &p).unwrap();
    assert_eq!(extended, f);
    let unclassified = Protocol::default();
    let error = functor_extend(&f, &schema, &identity(&schema), &unclassified).unwrap_err();
    assert!(matches!(error, RestrictError::RelSchema(_)), "{error:?}");
}

#[test]
fn functor_restrict_accepts_a_field_rename_inside_a_struct_column() {
    // A column whose value is a record: the edge inside the value is
    // renamed along a vertex rename, which keeps the value's structure.
    let p = Protocol {
        value_kinds: BTreeMap::from([
            ("string".to_owned(), ScalarKind::Str),
            ("struct".to_owned(), ScalarKind::Any),
        ]),
        non_value_kinds: BTreeSet::from(["record".to_owned()]),
        ..Protocol::default()
    };
    let build = |field: &str| {
        SchemaBuilder::new(&p)
            .vertex("t", "record", None)
            .and_then(|b| b.vertex("t.s", "struct", None))
            .and_then(|b| b.vertex(&format!("t.s.{field}"), "string", None))
            .and_then(|b| b.edge("t", "t.s", "prop", Some("s")))
            .and_then(|b| b.edge("t.s", &format!("t.s.{field}"), "prop", Some("f")))
            .and_then(SchemaBuilder::build)
            .unwrap()
    };
    let src = build("old");
    let tgt = build("new");
    let mut m = identity(&tgt);
    m.vertex_remap.insert("t.s.old".into(), "t.s.new".into());
    m.edge_remap.insert(
        Edge {
            src: "t.s".into(),
            tgt: "t.s.old".into(),
            kind: "prop".into(),
            name: Some("f".into()),
        },
        Edge {
            src: "t.s".into(),
            tgt: "t.s.new".into(),
            kind: "prop".into(),
            name: Some("f".into()),
        },
    );
    m.surviving_edges.insert(column("t", "s"));
    let f = FInstance::new()
        .with_rows(
            "t",
            [(
                RowId(0),
                cells(&[(
                    "s",
                    Value::Unknown(HashMap::from([("f".to_owned(), Value::Str("v".into()))])),
                )]),
            )],
        )
        .unwrap();
    let view = functor_restrict(&f, &src, &tgt, &m, protocols(&p)).unwrap();
    assert_eq!(view, f);
}

#[test]
fn table_restrict_rejects_an_invalid_source_and_target() {
    let (schema, _) = people();
    let p = protocol();
    // A row in a table the schema does not declare.
    let stray = FInstance::new()
        .with_rows("ghost", [(RowId(0), HashMap::new())])
        .unwrap();
    let error = functor_restrict_with_complement(
        &stray,
        &schema,
        &schema,
        &identity(&schema),
        protocols(&p),
    )
    .unwrap_err();
    assert!(
        matches!(
            error,
            RestrictError::Validation {
                side: BoundarySide::Input,
                ..
            }
        ),
        "{error:?}"
    );
    // An output a target constraint refuses: the target requires a
    // maximum age the source rows exceed.
    let mut b = SchemaBuilder::new(&p);
    b = b.vertex("person", "record", None).unwrap();
    for (column_name, kind) in [("name", "string"), ("age", "int")] {
        let vertex = format!("person.{column_name}");
        b = b
            .vertex(&vertex, kind, None)
            .unwrap()
            .edge("person", &vertex, "prop", Some(column_name))
            .unwrap();
    }
    let tgt = b.constraint("person.age", "maximum", "18").build().unwrap();
    let (_, f) = people();
    let report = panproto_inst::validate_table(&f, &tgt, &p);
    assert_ne!(
        report.status,
        panproto_gat::CheckStatus::Passed,
        "{report:?}"
    );
    let error =
        functor_restrict_with_complement(&f, &schema, &tgt, &identity(&schema), protocols(&p))
            .unwrap_err();
    assert!(
        matches!(
            error,
            RestrictError::Validation {
                side: BoundarySide::Output,
                ..
            }
        ),
        "{error:?}"
    );
}

#[test]
fn functor_restrict_is_the_first_projection_of_the_plan_engine() {
    let (schema, f) = people();
    let p = protocol();
    let mut m = identity(&schema);
    adults(&mut m);
    let (view, complement) =
        functor_restrict_with_complement(&f, &schema, &schema, &m, protocols(&p)).unwrap();
    let (engine_view, engine_complement) =
        functor_restrict_with_plan(&f, &plan(&schema, &schema, &m)).unwrap();
    assert_eq!(view, engine_view);
    assert_eq!(complement, engine_complement);
    assert_eq!(
        functor_restrict(&f, &schema, &schema, &m, protocols(&p)).unwrap(),
        view
    );
}

#[test]
fn a_plan_restrict_complement_carries_no_envelope() {
    let (schema, f) = people();
    let (_, complement) =
        functor_restrict_with_plan(&f, &plan(&schema, &schema, &identity(&schema))).unwrap();
    assert!(complement.identity.is_none());
}

#[test]
fn the_table_graph_restrict_square_commutes_under_transforms() {
    // Conditional survival, a field transform and an op term: the table
    // engine on the table and the graph engine on its graph form agree.
    let (schema, f) = people();
    let mut m = identity(&schema);
    adults(&mut m);
    m.add_field_rename("person", "name", "label");
    m.add_computed_field(
        "person",
        "older",
        Expr::Builtin(
            BuiltinOp::Add,
            vec![Expr::Var("age".into()), Expr::Lit(Literal::Int(1))],
        ),
    );
    let (table_view, _) = functor_restrict_with_plan(&f, &plan(&schema, &schema, &m)).unwrap();
    let (graph, _) = GInstance::from_finstance(&f).unwrap();
    let p = protocol();
    let graph_plan =
        RestrictPlan::compile_with_protocols(&schema, &schema, &m, protocols(&p)).unwrap();
    let (graph_view, _) = graph_restrict_with_plan(&graph, &graph_plan).unwrap();
    let (expected, _) = GInstance::from_finstance(&table_view).unwrap();
    assert!(
        GInstance::identical(&graph_view, &expected),
        "{graph_view:?}\n{expected:?}"
    );
    assert_eq!(table_view.row_ids(), vec![RowId(1)]);
    assert_eq!(
        table_view.row(RowId(1)).unwrap().get("older"),
        Some(&Value::Int(41))
    );
}

/// Up to three tables `t0..t2`, each with column `c` of kind `int`, and
/// foreign keys between them from a pool; rows and pairs at random.
fn arb_table_case() -> impl Strategy<Value = (Schema, FInstance, CompiledMigration, bool)> {
    (
        prop::collection::vec(any::<bool>(), 3),
        prop::collection::vec((0usize..3, 0usize..3), 0..4),
        prop::collection::vec((0usize..3, -5i64..5), 0..6),
        prop::collection::vec((0usize..6, 0usize..6, 0usize..4), 0..6),
        any::<bool>(),
    )
        .prop_map(|(keep, relation_spec, row_spec, pair_spec, predicate)| {
            let names = ["t0", "t1", "t2"];
            let mut relations: Vec<Edge> = relation_spec
                .iter()
                .map(|(s, t)| fk(names[*s], names[*t], &format!("r{s}{t}")))
                .collect();
            relations.sort();
            relations.dedup();
            let spec: Vec<(&str, &[(&str, &str)])> =
                names.iter().map(|n| (*n, &[("c", "int")][..])).collect();
            let schema = tables(&spec, &relations.iter().collect::<Vec<_>>());
            let mut f = FInstance::new();
            for (i, (table, value)) in row_spec.iter().enumerate() {
                f = f
                    .with_rows(
                        names[*table],
                        [(
                            RowId(u32::try_from(i).unwrap()),
                            cells(&[("c", Value::Int(*value))]),
                        )],
                    )
                    .unwrap();
            }
            for (s, t, r) in pair_spec {
                if let (Some(relation), Some((ts, _)), Some((tt, _))) =
                    (relations.get(r), row_spec.get(s), row_spec.get(t))
                    && relation.src == names[*ts]
                    && relation.tgt == names[*tt]
                {
                    let pair = (
                        RowId(u32::try_from(s).unwrap()),
                        RowId(u32::try_from(t).unwrap()),
                    );
                    if !f.contains_pair(relation, pair.0, pair.1) {
                        f = f.with_foreign_key(relation.clone(), [pair]).unwrap();
                    }
                }
            }
            let mut m = CompiledMigration::default();
            for (i, name) in names.iter().enumerate() {
                if keep[i] {
                    m.surviving_verts.insert((*name).into());
                    m.surviving_verts.insert(format!("{name}.c").into());
                    m.surviving_edges.insert(column(name, "c"));
                }
            }
            for relation in &relations {
                m.surviving_edges.insert(relation.clone());
            }
            if predicate {
                for name in names {
                    m.add_conditional_survival(
                        name,
                        Expr::Builtin(
                            BuiltinOp::Gt,
                            vec![Expr::Var("c".into()), Expr::Lit(Literal::Int(0))],
                        ),
                    );
                }
            }
            let tgt_keep = keep.iter().any(|k| *k);
            (schema, f, m, tgt_keep)
        })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// On the migrations `compile_for_tables` accepts, the table engine
    /// and the graph engine on the table's graph form give the same view;
    /// the others are refused, with the reason in the plan error.
    #[test]
    fn table_and_graph_restrict_agree_on_a_table_shaped_graph(
        (schema, f, m, _) in arb_table_case(),
    ) {
        let p = protocol();
        let Ok(table_plan) = RestrictPlan::compile_for_tables(&schema, &schema, &m, protocols(&p)) else {
            return Ok(());
        };
        let graph_plan = RestrictPlan::compile_with_protocols(&schema, &schema, &m, protocols(&p)).unwrap();
        let (graph, _) = GInstance::from_finstance(&f).unwrap();
        let table = functor_restrict_with_plan(&f, &table_plan);
        let graph_result = graph_restrict_with_plan(&graph, &graph_plan);
        match (table, graph_result) {
            (Ok((table_view, _)), Ok((graph_view, graph_complement))) => {
                // Synthesized pairs of the table are synthesized edges of the
                // graph; compare the views modulo edge ids.
                // The views agree modulo edge ids, except where a contracted
                // pair coincides with a kept direct pair on the same key: the
                // graph holds two edges there and the table one pair (the
                // per-call case dispatch declines).
                let (expected, _) = GInstance::from_finstance(&table_view).unwrap();
                let coincides = graph_complement.synthesized_edges.keys().any(|id| {
                    let synthesized = graph_view.edge(*id).unwrap();
                    graph_view.edges().iter().any(|(other, edge)| {
                        other != id
                            && !graph_complement.synthesized_edges.contains_key(other)
                            && (edge.src, edge.tgt, &edge.edge)
                                == (synthesized.src, synthesized.tgt, &synthesized.edge)
                    })
                });
                prop_assert!(
                    GInstance::equivalent_modulo_edge_ids(&graph_view, &expected) || coincides,
                    "{:?}\n{:?}", graph_view, expected
                );
                let graph_nodes: BTreeSet<u32> = graph_view.nodes().keys().copied().collect();
                let table_rows: BTreeSet<u32> = table_view.row_ids().into_iter().map(|r| r.0).collect();
                prop_assert_eq!(graph_nodes, table_rows);
            }
            (Err(table_error), Err(graph_error)) => {
                prop_assert_eq!(table_error.kind_index(), graph_error.unlocated().kind_index());
            }
            (table, graph) => {
                prop_assert!(false, "table {:?} graph {:?}", table.map(|r| r.0), graph.map(|r| r.0));
            }
        }
    }
}

#[test]
fn a_renamed_column_is_no_dropped_cell() {
    // The column `a` is renamed to `a2` by the edge map: the view holds its
    // value under the new name, so the complement records nothing for it
    // and a bijective rename leaves trivial residue.
    let src = tables(&[("t", &[("a", "int"), ("b", "int")])], &[]);
    let tgt = tables(&[("t", &[("a2", "int"), ("b", "int")])], &[]);
    let m = CompiledMigration {
        surviving_verts: ["t", "t.a2", "t.b"]
            .iter()
            .map(|v| Name::from(*v))
            .collect(),
        surviving_edges: [column("t", "a2"), column("t", "b")].into(),
        vertex_remap: HashMap::from([("t.a".into(), "t.a2".into())]),
        edge_remap: HashMap::from([(column("t", "a"), column("t", "a2"))]),
        ..CompiledMigration::default()
    };
    let f = FInstance::new()
        .with_rows(
            "t",
            [(
                RowId(0),
                cells(&[("a", Value::Int(1)), ("b", Value::Int(2))]),
            )],
        )
        .unwrap();
    let (view, complement) = functor_restrict_with_plan(&f, &plan(&src, &tgt, &m)).unwrap();
    assert_eq!(
        view.row(RowId(0)).unwrap(),
        &cells(&[("a2", Value::Int(1)), ("b", Value::Int(2))])
    );
    assert_eq!(complement.dropped_cells, BTreeMap::new());
    assert_eq!(complement.original_cells, BTreeMap::new());
    assert!(complement.residue_is_trivial(), "{complement:?}");
}

#[test]
fn a_cell_renamed_by_a_transform_is_traced_to_its_source_column() {
    let (schema, f) = people();
    // A rename alone: nothing recorded.
    let mut m = identity(&schema);
    m.add_field_rename("person", "name", "label");
    let (view, complement) = functor_restrict_with_plan(&f, &plan(&schema, &schema, &m)).unwrap();
    assert_eq!(
        view.row(RowId(0)).unwrap().get("label"),
        Some(&Value::Str("Ann".into()))
    );
    assert!(complement.residue_is_trivial(), "{complement:?}");
    // A rename, then a change of the renamed value: the source value is
    // recorded under the source column, and nothing is dropped.
    let mut m = identity(&schema);
    m.add_field_rename("person", "age", "years");
    m.field_transforms.entry("person".into()).or_default().push(
        panproto_inst::FieldTransform::ApplyExpr {
            key: "years".into(),
            expr: Expr::Builtin(
                BuiltinOp::Add,
                vec![Expr::Var("years".into()), Expr::Lit(Literal::Int(1))],
            ),
            inverse: None,
            coercion_class: panproto_gat::CoercionClass::Opaque,
        },
    );
    let (view, complement) = functor_restrict_with_plan(&f, &plan(&schema, &schema, &m)).unwrap();
    assert_eq!(
        view.row(RowId(0)).unwrap().get("years"),
        Some(&Value::Int(11))
    );
    assert_eq!(complement.dropped_cells, BTreeMap::new());
    assert_eq!(
        complement.original_cells,
        BTreeMap::from([
            ((RowId(0), "age".to_owned()), Value::Int(10)),
            ((RowId(1), "age".to_owned()), Value::Int(40)),
        ])
    );
    // A rename onto a column the row holds: that column's own value is
    // gone from the view, so it is a dropped cell, and the moved one is not.
    let two = tables(&[("t", &[("a", "int"), ("b", "int")])], &[]);
    let row = FInstance::new()
        .with_rows(
            "t",
            [(
                RowId(0),
                cells(&[("a", Value::Int(1)), ("b", Value::Int(2))]),
            )],
        )
        .unwrap();
    let mut m = identity(&two);
    m.add_field_rename("t", "a", "b");
    let (view, complement) = functor_restrict_with_plan(&row, &plan(&two, &two, &m)).unwrap();
    assert_eq!(view.row(RowId(0)).unwrap(), &cells(&[("b", Value::Int(1))]));
    assert_eq!(
        complement.dropped_cells,
        BTreeMap::from([((RowId(0), "b".to_owned()), Value::Int(2))])
    );
    assert_eq!(complement.original_cells, BTreeMap::new());
    // A computed value written over a column the row holds is that
    // column's changed value.
    let mut m = identity(&schema);
    m.field_transforms.entry("person".into()).or_default().push(
        panproto_inst::FieldTransform::ComputeField {
            target_key: "age".into(),
            expr: Expr::Lit(Literal::Int(0)),
            inverse: None,
            coercion_class: panproto_gat::CoercionClass::Opaque,
        },
    );
    let (_, complement) = functor_restrict_with_plan(&f, &plan(&schema, &schema, &m)).unwrap();
    assert_eq!(complement.dropped_cells, BTreeMap::new());
    assert_eq!(
        complement.original_cells,
        BTreeMap::from([
            ((RowId(0), "age".to_owned()), Value::Int(10)),
            ((RowId(1), "age".to_owned()), Value::Int(40)),
        ])
    );
}
