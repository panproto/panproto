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
        let Ok(table_plan) =
            RestrictPlan::compile_for_tables(&schema, &schema, &m, protocols(&p))
        else {
            return Ok(());
        };
        let graph_plan =
            RestrictPlan::compile_with_protocols(&schema, &schema, &m, protocols(&p)).unwrap();
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
                let table_rows: BTreeSet<u32> =
                    table_view.row_ids().into_iter().map(|r| r.0).collect();
                prop_assert_eq!(graph_nodes, table_rows);
            }
            (Err(table_error), Err(graph_error)) => {
                prop_assert_eq!(table_error.kind_index(), graph_error.unlocated().kind_index());
            }
            (table, graph) => {
                prop_assert!(
                    false,
                    "table {:?} graph {:?}",
                    table.map(|r| r.0),
                    graph.map(|r| r.0)
                );
            }
        }
    }
}

#[test]
fn a_renamed_column_is_a_dropped_cell() {
    // The column `a` is renamed to `a2` by the edge map: the view row holds
    // no cell named `a`, so the source cell is a dropped cell, and a
    // column rename leaves non-trivial residue, as it does in the graph.
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
    assert_eq!(
        complement.dropped_cells,
        BTreeMap::from([((RowId(0), "a".to_owned()), Value::Int(1))])
    );
    assert_eq!(complement.original_cells, BTreeMap::new());
    assert!(!complement.residue_is_trivial(), "{complement:?}");
    assert_eq!(
        restored_row(
            view.row(RowId(0)).unwrap(),
            &complement,
            RowId(0),
            &["a", "b"]
        ),
        cells(&[("a", Value::Int(1)), ("b", Value::Int(2))])
    );
    let p = protocol();
    let graph_plan = RestrictPlan::compile_with_protocols(&src, &tgt, &m, protocols(&p)).unwrap();
    let (_, graph_complement) =
        graph_restrict_with_plan(&GInstance::from_finstance(&f).unwrap().0, &graph_plan).unwrap();
    assert!(
        !graph_complement.residue_is_trivial(),
        "{graph_complement:?}"
    );
}

#[test]
fn a_cell_renamed_by_a_transform_is_traced_to_its_source_column() {
    let (schema, f) = people();
    // A rename alone: the view row holds no cell named `name`, so each
    // source cell of that column is a dropped cell under it.
    let mut m = identity(&schema);
    m.add_field_rename("person", "name", "label");
    let (view, complement) = functor_restrict_with_plan(&f, &plan(&schema, &schema, &m)).unwrap();
    assert_eq!(
        view.row(RowId(0)).unwrap().get("label"),
        Some(&Value::Str("Ann".into()))
    );
    assert_eq!(
        complement.dropped_cells,
        BTreeMap::from([
            ((RowId(0), "name".to_owned()), Value::Str("Ann".into())),
            ((RowId(1), "name".to_owned()), Value::Str("Bo".into())),
        ])
    );
    assert_eq!(complement.original_cells, BTreeMap::new());
    // A rename, then a change of the renamed value: the source value is
    // recorded under the source column, as a dropped cell, since the view
    // row holds no cell of its name.
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
    assert_eq!(
        complement.dropped_cells,
        BTreeMap::from([
            ((RowId(0), "age".to_owned()), Value::Int(10)),
            ((RowId(1), "age".to_owned()), Value::Int(40)),
        ])
    );
    assert_eq!(complement.original_cells, BTreeMap::new());
    // A rename onto a column the row holds: the view row holds no cell
    // named `a`, so `a` is a dropped cell, and its cell named `b` holds
    // `a`'s value, so `b` is an original cell.
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
        BTreeMap::from([((RowId(0), "a".to_owned()), Value::Int(1))])
    );
    assert_eq!(
        complement.original_cells,
        BTreeMap::from([((RowId(0), "b".to_owned()), Value::Int(2))])
    );
    assert_eq!(
        restored_row(
            view.row(RowId(0)).unwrap(),
            &complement,
            RowId(0),
            &["a", "b"]
        ),
        cells(&[("a", Value::Int(1)), ("b", Value::Int(2))])
    );
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

/// The source row a put rebuilds from the view row and the complement,
/// column by column of the row's source table: `dropped_cells`, else
/// `original_cells`, else the view's cell of that name.
fn restored_row(
    view_row: &HashMap<String, Value>,
    complement: &TableComplement,
    id: RowId,
    columns: &[&str],
) -> HashMap<String, Value> {
    columns
        .iter()
        .filter_map(|column| {
            let key = (id, (*column).to_owned());
            complement
                .dropped_cells
                .get(&key)
                .or_else(|| complement.original_cells.get(&key))
                .or_else(|| view_row.get(*column))
                .map(|value| ((*column).to_owned(), value.clone()))
        })
        .collect()
}

/// `Case [a > 1 => rename a -> tmp, c -> a, tmp -> c]` on the table
/// `t(a, c)`: a swap of the two columns that runs only where `a > 1`.
fn value_dependent_swap() -> (Schema, CompiledMigration) {
    let schema = tables(&[("t", &[("a", "int"), ("c", "int")])], &[]);
    let mut m = identity(&schema);
    let rename = |old: &str, new: &str| panproto_inst::FieldTransform::RenameField {
        old_key: old.into(),
        new_key: new.into(),
    };
    m.field_transforms.insert(
        "t".into(),
        vec![panproto_inst::FieldTransform::Case {
            branches: vec![panproto_inst::CaseBranch {
                predicate: Expr::Builtin(
                    BuiltinOp::Gt,
                    vec![Expr::Var("a".into()), Expr::Lit(Literal::Int(1))],
                ),
                transforms: vec![rename("a", "tmp"), rename("c", "a"), rename("tmp", "c")],
            }],
        }],
    );
    (schema, m)
}

#[test]
fn a_value_dependent_swap_leaves_distinct_table_complements() {
    // The sources {a: 1, c: 2} and {a: 2, c: 1} have one view, {a: 1,
    // c: 2}: the first does not swap and the second does. Which one a put
    // must restore depends on a source value, so no trace of keys through
    // the renames can tell them apart; the per-column comparison
    // does, and the complements differ.
    let (schema, m) = value_dependent_swap();
    let p = protocol();
    let graph_plan =
        RestrictPlan::compile_with_protocols(&schema, &schema, &m, protocols(&p)).unwrap();
    let run = |a: i64, c: i64| {
        let row = cells(&[("a", Value::Int(a)), ("c", Value::Int(c))]);
        let f = FInstance::new()
            .with_rows("t", [(RowId(0), row.clone())])
            .unwrap();
        let (view, complement) =
            functor_restrict_with_plan(&f, &plan(&schema, &schema, &m)).unwrap();
        let view_row = view.row(RowId(0)).unwrap().clone();
        assert_eq!(
            restored_row(&view_row, &complement, RowId(0), &["a", "c"]),
            row,
            "{complement:?}"
        );
        let (graph, _) = GInstance::from_finstance(&f).unwrap();
        let (_, graph_complement) = graph_restrict_with_plan(&graph, &graph_plan).unwrap();
        assert_eq!(
            complement.residue_is_trivial(),
            graph_complement.residue_is_trivial(),
            "table {complement:?}\ngraph {graph_complement:?}"
        );
        (view_row, complement)
    };
    let (unswapped_view, unswapped) = run(1, 2);
    let (swapped_view, swapped) = run(2, 1);
    assert_eq!(unswapped_view, swapped_view);
    assert_eq!(
        unswapped_view,
        cells(&[("a", Value::Int(1)), ("c", Value::Int(2))])
    );
    assert!(unswapped.residue_is_trivial(), "{unswapped:?}");
    assert_eq!(swapped.dropped_cells, BTreeMap::new());
    assert_eq!(
        swapped.original_cells,
        BTreeMap::from([
            ((RowId(0), "a".to_owned()), Value::Int(2)),
            ((RowId(0), "c".to_owned()), Value::Int(1)),
        ])
    );
    assert_ne!(unswapped, swapped);
}

/// The table `t(a, c)` holding one row, id 0, of the given cells.
fn one_row(row: &[(&str, Value)]) -> FInstance {
    FInstance::new()
        .with_rows("t", [(RowId(0), cells(row))])
        .unwrap()
}

/// Whether `result` is the refusal of row `row` at `column` for `reason`.
fn refused_as_nonconforming<T: std::fmt::Debug>(
    result: &Result<T, RestrictError>,
    row: u32,
    column: Option<&str>,
    reason: &panproto_inst::NonConformance,
) -> bool {
    matches!(
        result,
        Err(RestrictError::NonConforming { row: r, column: c, reason: why })
            if *r == RowId(row) && c.as_deref() == column && why == reason
    )
}

#[test]
fn a_partial_row_is_refused_since_no_complement_could_restore_its_absent_cell() {
    // `AddField c := 0` sends {a: 1} and {a: 1, c: 0} to the one view row
    // {a: 1, c: 0}. The second is unchanged, so its complement is empty,
    // and a put from the view must give it back; the first then has no
    // complement that restores it. The table restrict reads total rows
    // only (absent and null cells differ), so it refuses the partial row
    // and keeps the total one.
    let schema = tables(&[("t", &[("a", "int"), ("c", "int")])], &[]);
    let mut m = identity(&schema);
    m.field_transforms.insert(
        "t".into(),
        vec![panproto_inst::FieldTransform::AddField {
            key: "c".into(),
            value: Value::Int(0),
        }],
    );
    let p = protocol();
    let missing = panproto_inst::NonConformance::MissingCell;
    let partial = one_row(&[("a", Value::Int(1))]);
    let total = one_row(&[("a", Value::Int(1)), ("c", Value::Int(0))]);
    let engine = functor_restrict_with_plan(&partial, &plan(&schema, &schema, &m));
    assert!(
        refused_as_nonconforming(&engine, 0, Some("c"), &missing),
        "{engine:?}"
    );
    let validated = functor_restrict_with_complement(&partial, &schema, &schema, &m, protocols(&p));
    assert!(
        refused_as_nonconforming(&validated, 0, Some("c"), &missing),
        "{validated:?}"
    );
    let (view, complement) =
        functor_restrict_with_complement(&total, &schema, &schema, &m, protocols(&p)).unwrap();
    assert_eq!(
        view.row(RowId(0)),
        Some(&cells(&[("a", Value::Int(1)), ("c", Value::Int(0))]))
    );
    assert!(complement.residue_is_trivial(), "{complement:?}");
    // A null cell is a cell: the row holding it is total, and `AddField`
    // leaves a cell the row holds as it is.
    let null = one_row(&[("a", Value::Int(1)), ("c", Value::Null)]);
    let (view, complement) =
        functor_restrict_with_plan(&null, &plan(&schema, &schema, &m)).unwrap();
    assert_eq!(view.row(RowId(0)), null.row(RowId(0)));
    assert!(complement.residue_is_trivial(), "{complement:?}");
}

#[test]
fn the_table_engine_refuses_a_stray_cell_and_a_row_outside_the_tables() {
    // A cell at no column of its table, which `AddField x := 5` would
    // leave unchanged, and so unrecorded, while it adds the cell to a row
    // without it: the engine, which validates nothing, refuses it as the
    // validated restrict's source boundary does.
    let schema = tables(&[("t", &[("a", "int"), ("c", "int")])], &[]);
    let mut m = identity(&schema);
    m.field_transforms.insert(
        "t".into(),
        vec![panproto_inst::FieldTransform::AddField {
            key: "x".into(),
            value: Value::Int(5),
        }],
    );
    let p = protocol();
    let table_plan = plan(&schema, &schema, &m);
    let stray = one_row(&[
        ("a", Value::Int(1)),
        ("c", Value::Int(2)),
        ("x", Value::Int(5)),
    ]);
    let engine = functor_restrict_with_plan(&stray, &table_plan);
    assert!(
        refused_as_nonconforming(
            &engine,
            0,
            Some("x"),
            &panproto_inst::NonConformance::StrayCell
        ),
        "{engine:?}"
    );
    assert!(matches!(
        functor_restrict_with_complement(&stray, &schema, &schema, &m, protocols(&p)),
        Err(RestrictError::Validation {
            side: BoundarySide::Input,
            ..
        })
    ));
    // The least offending column decides between a missing and a stray
    // cell: `a` is missing before `b` is stray, and `b` is stray before
    // `c` is missing.
    let engine = functor_restrict_with_plan(
        &one_row(&[("b", Value::Int(1)), ("c", Value::Int(2))]),
        &table_plan,
    );
    assert!(
        refused_as_nonconforming(
            &engine,
            0,
            Some("a"),
            &panproto_inst::NonConformance::MissingCell
        ),
        "{engine:?}"
    );
    let engine = functor_restrict_with_plan(
        &one_row(&[("a", Value::Int(1)), ("b", Value::Int(2))]),
        &table_plan,
    );
    assert!(
        refused_as_nonconforming(
            &engine,
            0,
            Some("b"),
            &panproto_inst::NonConformance::StrayCell
        ),
        "{engine:?}"
    );
    // A row at a vertex that is no table: the column vertex `t.a`.
    let outside = FInstance::new()
        .with_rows("t.a", [(RowId(0), cells(&[]))])
        .unwrap();
    let engine = functor_restrict_with_plan(&outside, &table_plan);
    assert!(
        refused_as_nonconforming(
            &engine,
            0,
            None,
            &panproto_inst::NonConformance::TableOutside {
                table: "t.a".into()
            }
        ),
        "{engine:?}"
    );
}

#[test]
fn a_nonconforming_row_is_refused_in_ascending_row_order() {
    // Row order decides between a row's conformance and another row's
    // survival predicate, as between two rows' predicates: the least row
    // with a refusal gives it, and a row's conformance comes before its
    // own predicate.
    let schema = tables(&[("t", &[("a", "int"), ("c", "int")])], &[]);
    let mut m = identity(&schema);
    m.add_conditional_survival(
        "t",
        Expr::Builtin(
            BuiltinOp::Gt,
            vec![Expr::Var("a".into()), Expr::Lit(Literal::Int(0))],
        ),
    );
    let table_plan = plan(&schema, &schema, &m);
    let instance = |first: HashMap<String, Value>, second: HashMap<String, Value>| {
        FInstance::new()
            .with_rows("t", [(RowId(0), first), (RowId(1), second)])
            .unwrap()
    };
    let total = cells(&[("a", Value::Int(1)), ("c", Value::Int(2))]);
    // `a` is a string: the predicate fails to evaluate.
    let failing = cells(&[("a", Value::Str("s".into())), ("c", Value::Int(2))]);
    let partial = cells(&[("c", Value::Int(2))]);
    let missing = panproto_inst::NonConformance::MissingCell;
    let first =
        functor_restrict_with_plan(&instance(partial.clone(), failing.clone()), &table_plan);
    assert!(
        refused_as_nonconforming(&first, 0, Some("a"), &missing),
        "{first:?}"
    );
    let second = functor_restrict_with_plan(&instance(failing, partial.clone()), &table_plan);
    assert!(
        matches!(
            second,
            Err(RestrictError::SurvivalPredicateFailed { node: 0, .. })
        ),
        "{second:?}"
    );
    let later = functor_restrict_with_plan(&instance(total, partial), &table_plan);
    assert!(
        refused_as_nonconforming(&later, 1, Some("a"), &missing),
        "{later:?}"
    );
}

/// One value transform of `t(a, c)` from a pool that renames, swaps on a
/// value, drops, overwrites, changes in place and computes a column the
/// table lacks.
fn pooled_transform(choice: u8) -> panproto_inst::FieldTransform {
    use panproto_inst::{CaseBranch, FieldTransform};
    let var = |name: &str| Expr::Var(name.into());
    let int = |n: i64| Expr::Lit(Literal::Int(n));
    let rename = |old: &str, new: &str| FieldTransform::RenameField {
        old_key: old.into(),
        new_key: new.into(),
    };
    match choice {
        0 => value_dependent_swap().1.field_transforms["t"][0].clone(),
        1 => FieldTransform::ComputeField {
            target_key: "k".into(),
            expr: int(1),
            inverse: None,
            coercion_class: panproto_gat::CoercionClass::Opaque,
        },
        2 => FieldTransform::ApplyExpr {
            key: "a".into(),
            expr: Expr::Builtin(BuiltinOp::Mul, vec![var("a"), int(0)]),
            inverse: None,
            coercion_class: panproto_gat::CoercionClass::Opaque,
        },
        3 => FieldTransform::DropField { key: "c".into() },
        4 => rename("a", "c"),
        5 => FieldTransform::Case {
            branches: vec![CaseBranch {
                predicate: Expr::Builtin(BuiltinOp::Gt, vec![var("a"), int(0)]),
                transforms: vec![rename("a", "z")],
            }],
        },
        _ => FieldTransform::AddField {
            key: "c".into(),
            value: Value::Int(0),
        },
    }
}

/// The row of `t(a, c)` that `shape` draws from the cells `a` and `c`:
/// total for 0 to 2, without `a` for 3, without `c` for 4, and with the
/// stray cell `x` for 5; with the refusal the table engine owes it, if any.
fn drawn_row(
    a: i64,
    c: i64,
    shape: u8,
) -> (
    HashMap<String, Value>,
    Option<(&'static str, panproto_inst::NonConformance)>,
) {
    use panproto_inst::NonConformance::{MissingCell, StrayCell};
    match shape {
        3 => (cells(&[("c", Value::Int(c))]), Some(("a", MissingCell))),
        4 => (cells(&[("a", Value::Int(a))]), Some(("c", MissingCell))),
        5 => (
            cells(&[
                ("a", Value::Int(a)),
                ("c", Value::Int(c)),
                ("x", Value::Int(0)),
            ]),
            Some(("x", StrayCell)),
        ),
        _ => (cells(&[("a", Value::Int(a)), ("c", Value::Int(c))]), None),
    }
}

/// The table and graph restricts of `f` along `m` on `t(a, c)`, with the
/// cell residue agreement of
/// `table_and_graph_cell_residues_agree_but_for_a_computed_column`.
fn cell_residues_agree(
    schema: &Schema,
    m: &CompiledMigration,
    f: &FInstance,
) -> Result<(), TestCaseError> {
    let p = protocol();
    let table_plan = RestrictPlan::compile_for_tables(schema, schema, m, protocols(&p)).unwrap();
    let graph_plan =
        RestrictPlan::compile_with_protocols(schema, schema, m, protocols(&p)).unwrap();
    let (graph, _) = GInstance::from_finstance(f).unwrap();
    let table = functor_restrict_with_plan(f, &table_plan);
    let graph = graph_restrict_with_plan(&graph, &graph_plan);
    match (table, graph) {
        (Ok((view, complement)), Ok((_, graph_complement))) => {
            for id in view.row_ids() {
                let (view_row, source_row) = (view.row(id).unwrap(), f.row(id).unwrap());
                prop_assert_eq!(
                    &restored_row(view_row, &complement, id, &["a", "c"]),
                    source_row
                );
                let table_records = complement
                    .dropped_cells
                    .keys()
                    .chain(complement.original_cells.keys())
                    .any(|(r, _)| *r == id);
                let graph_records = graph_complement.original_extra_fields.contains_key(&id.0);
                let extra = view_row.keys().any(|k| k != "a" && k != "c");
                prop_assert!(
                    graph_records == table_records || (graph_records && !table_records && extra),
                    "row {:?}: table {:?}\ngraph {:?}",
                    id,
                    complement,
                    graph_complement
                );
                if graph_records {
                    let snapshot: HashMap<String, Value> = graph_complement.original_extra_fields
                        [&id.0]
                        .iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect();
                    prop_assert_eq!(&snapshot, source_row);
                }
            }
        }
        (Err(table_error), Err(graph_error)) => {
            prop_assert_eq!(
                table_error.kind_index(),
                graph_error.unlocated().kind_index()
            );
        }
        (table, graph) => {
            prop_assert!(
                false,
                "table {:?} graph {:?}",
                table.map(|r| r.0),
                graph.map(|r| r.0)
            );
        }
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// Under value transforms that rename, swap on a value, drop,
    /// overwrite, change in place and compute, on every surviving row: the
    /// table complement rebuilds the source row column by column (so get
    /// paired with it is injective on sources), and the table records the
    /// row's cells exactly when the graph engine records its fields on the
    /// table's graph form, except on a row whose view holds a cell outside
    /// the source table's columns while every source column keeps its name
    /// and value, which the graph records and the table does not. A source
    /// with a row that is not total (a cell missing or stray) is refused at
    /// the least such row, before any comparison.
    #[test]
    fn table_and_graph_cell_residues_agree_but_for_a_computed_column(
        choices in prop::collection::vec(0u8..7, 0..4),
        rows in prop::collection::vec((-2i64..3, -2i64..3, 0u8..6), 0..4),
    ) {
        let schema = tables(&[("t", &[("a", "int"), ("c", "int")])], &[]);
        let mut m = identity(&schema);
        m.field_transforms
            .insert("t".into(), choices.iter().map(|c| pooled_transform(*c)).collect());
        let mut f = FInstance::new();
        // The rows before the first that is not total, and that row's
        // refusal.
        let mut prefix = FInstance::new();
        let mut owed = None;
        for (i, (a, c, shape)) in rows.iter().enumerate() {
            let id = u32::try_from(i).unwrap();
            let (row, refusal) = drawn_row(*a, *c, *shape);
            if owed.is_none() {
                owed = refusal.map(|(column, reason)| (id, column, reason));
            }
            if owed.is_none() {
                prefix = prefix.with_rows("t", [(RowId(id), row.clone())]).unwrap();
            }
            f = f.with_rows("t", [(RowId(id), row)]).unwrap();
        }
        if let Some((id, column, reason)) = owed {
            // Rows are refused in ascending id: a refusal of a total row
            // before it comes first, and otherwise the row's own.
            let p = protocol();
            let table_plan =
                RestrictPlan::compile_for_tables(&schema, &schema, &m, protocols(&p)).unwrap();
            let refused = functor_restrict_with_plan(&f, &table_plan);
            match functor_restrict_with_plan(&prefix, &table_plan) {
                Err(earlier) => prop_assert_eq!(
                    format!("{:?}", refused.map(|r| r.0)),
                    format!("{:?}", Err::<FInstance, _>(earlier))
                ),
                Ok(_) => prop_assert!(
                    refused_as_nonconforming(&refused, id, Some(column), &reason),
                    "{:?}",
                    refused.map(|r| r.0)
                ),
            }
        } else {
            cell_residues_agree(&schema, &m, &f)?;
        }
    }
}

/// The protocol of [`protocol`] with the value kind `struct`, whose cells
/// hold a record value.
fn protocol_with_struct() -> Protocol {
    let mut p = protocol();
    p.value_kinds.insert("struct".to_owned(), ScalarKind::Any);
    p
}

/// `t` with the leaf column `a` (int) and the structured column `addr`
/// (`struct`), whose value has the inner field `x` (int).
fn leaf_and_struct_table(p: &Protocol) -> Schema {
    SchemaBuilder::new(p)
        .vertex("t", "record", None)
        .and_then(|b| b.vertex("t.a", "int", None))
        .and_then(|b| b.vertex("t.addr", "struct", None))
        .and_then(|b| b.vertex("t.addr.x", "int", None))
        .and_then(|b| b.edge("t", "t.a", "prop", Some("a")))
        .and_then(|b| b.edge("t", "t.addr", "prop", Some("addr")))
        .and_then(|b| b.edge("t.addr", "t.addr.x", "prop", Some("x")))
        .and_then(SchemaBuilder::build)
        .unwrap()
}

#[test]
fn a_table_with_a_leaf_and_a_struct_column_reads_both_as_cells() {
    // The structured column is a column of `t` (its target is a value
    // vertex; the edge out of it is inside the one value a cell holds), so
    // a total row holds a cell at both columns, and a row without the
    // structured cell is partial.
    let p = protocol_with_struct();
    let schema = leaf_and_struct_table(&p);
    let rel = panproto_inst::RelSchema::new(&schema, &p).unwrap();
    assert!(rel.columns("t").keys().eq(["a", "addr"]));
    let addr = Value::Unknown(HashMap::from([("x".to_owned(), Value::Int(2))]));
    let row = FInstance::new()
        .with_rows(
            "t",
            [(
                RowId(0),
                cells(&[("a", Value::Int(1)), ("addr", addr.clone())]),
            )],
        )
        .unwrap();
    let m = identity(&schema);
    let (view, complement) =
        functor_restrict_with_plan(&row, &plan_under(&schema, &schema, &m, &p)).unwrap();
    assert_eq!(view, row);
    assert!(complement.residue_is_trivial(), "{complement:?}");

    let partial = FInstance::new()
        .with_rows("t", [(RowId(0), cells(&[("a", Value::Int(1))]))])
        .unwrap();
    let engine = functor_restrict_with_plan(&partial, &plan_under(&schema, &schema, &m, &p));
    assert!(
        refused_as_nonconforming(
            &engine,
            0,
            Some("addr"),
            &panproto_inst::NonConformance::MissingCell
        ),
        "{engine:?}"
    );

    // Dropping the structured column records its whole value.
    let mut dropping = identity(&schema);
    dropping.surviving_verts.remove("t.addr");
    dropping.surviving_verts.remove("t.addr.x");
    dropping
        .surviving_edges
        .retain(|edge| edge.src != "t.addr" && edge.name.as_deref() != Some("addr"));
    let mut tgt = schema.clone();
    tgt.vertices.remove("t.addr");
    tgt.vertices.remove("t.addr.x");
    tgt.edges
        .retain(|edge, _| edge.src != "t.addr" && edge.name.as_deref() != Some("addr"));
    let (view, complement) =
        functor_restrict_with_plan(&row, &plan_under(&schema, &tgt, &dropping, &p)).unwrap();
    assert_eq!(view.row(RowId(0)), Some(&cells(&[("a", Value::Int(1))])));
    assert_eq!(
        complement.dropped_cells,
        BTreeMap::from([((RowId(0), "addr".to_owned()), addr)])
    );
}

/// The table plan of `m` from `src` to `tgt` under the protocol `p`.
fn plan_under(src: &Schema, tgt: &Schema, m: &CompiledMigration, p: &Protocol) -> RestrictPlan {
    RestrictPlan::compile_for_tables(src, tgt, m, protocols(p)).unwrap()
}

#[test]
fn a_validated_restrict_accepts_a_row_with_a_structured_column() {
    // The validated entry point checks both boundaries: the structured
    // column `addr` is a column of `t` as the classification reads it, so
    // a total row passes validation on both sides and comes back as is.
    let p = protocol_with_struct();
    let schema = leaf_and_struct_table(&p);
    let m = identity(&schema);
    let addr = Value::Unknown(HashMap::from([("x".to_owned(), Value::Int(2))]));
    let row = one_row(&[("a", Value::Int(1)), ("addr", addr)]);
    let (view, complement) =
        functor_restrict_with_complement(&row, &schema, &schema, &m, protocols(&p)).unwrap();
    assert_eq!(view, row);
    assert!(complement.residue_is_trivial(), "{complement:?}");
    assert_eq!(
        panproto_inst::validate_table(&row, &schema, &p).status,
        panproto_gat::CheckStatus::Passed
    );

    // The column is declared with its vertex's kind, so a cell of another
    // kind there is mistyped, and a cell the table does not declare is
    // undeclared: both are refused on the input side.
    for (bad, violation) in [
        (
            one_row(&[("a", Value::Int(1)), ("addr", Value::Str("s".into()))]),
            "MistypedAttribute",
        ),
        (
            one_row(&[
                ("a", Value::Int(1)),
                ("addr", Value::Unknown(HashMap::new())),
                ("stray", Value::Int(3)),
            ]),
            "UndeclaredAttribute",
        ),
    ] {
        let error = functor_restrict_with_complement(&bad, &schema, &schema, &m, protocols(&p))
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
        assert!(format!("{error:?}").contains(violation), "{error:?}");
    }
}

#[test]
fn a_table_whose_only_column_is_structured_declares_it() {
    // With no leaf column, the table still declares its structured column,
    // so a stray cell beside it is undeclared rather than unchecked.
    let p = protocol_with_struct();
    let schema = SchemaBuilder::new(&p)
        .vertex("t", "record", None)
        .and_then(|b| b.vertex("t.addr", "struct", None))
        .and_then(|b| b.vertex("t.addr.x", "int", None))
        .and_then(|b| b.edge("t", "t.addr", "prop", Some("addr")))
        .and_then(|b| b.edge("t.addr", "t.addr.x", "prop", Some("x")))
        .and_then(SchemaBuilder::build)
        .unwrap();
    let addr = Value::Unknown(HashMap::from([("x".to_owned(), Value::Int(2))]));
    let total = one_row(&[("addr", addr.clone())]);
    assert_eq!(
        panproto_inst::validate_table(&total, &schema, &p).status,
        panproto_gat::CheckStatus::Passed
    );
    let stray = one_row(&[("addr", addr), ("stray", Value::Int(3))]);
    let report = panproto_inst::validate_table(&stray, &schema, &p);
    assert_ne!(report.status, panproto_gat::CheckStatus::Passed);
    assert!(
        format!("{report:?}").contains("UndeclaredAttribute"),
        "{report:?}"
    );
}

#[test]
fn a_plan_digest_reads_the_kinds_that_classify_columns() {
    // Plans with equal digests give equal reports. Two protocols that
    // differ only in their value and non-value kinds classify the
    // structured column differently, so their reports and digests differ.
    let classifying = protocol_with_struct();
    let schema = leaf_and_struct_table(&classifying);
    let mut silent = classifying.clone();
    silent.value_kinds.clear();
    silent.non_value_kinds.clear();
    let options = panproto_inst::ValidationOptions::default();
    let addr = Value::Unknown(HashMap::from([("x".to_owned(), Value::Int(2))]));
    let row = one_row(&[("a", Value::Int(1)), ("addr", addr)]);
    assert_eq!(
        panproto_inst::validate_table(&row, &schema, &classifying).status,
        panproto_gat::CheckStatus::Passed
    );
    assert_ne!(
        panproto_inst::validate_table(&row, &schema, &silent).status,
        panproto_gat::CheckStatus::Passed
    );
    let digest =
        |p: &Protocol| *panproto_inst::ValidationPlan::compile(&schema, p, &options).digest();
    assert_ne!(digest(&classifying), digest(&silent));
    let mut only_non_value = classifying.clone();
    only_non_value.non_value_kinds.insert("other".to_owned());
    assert_ne!(digest(&classifying), digest(&only_non_value));
}

#[test]
fn the_default_protocol_declares_the_leaf_edges_alone() {
    // A vertex with a primary key is a table under any protocol, the
    // default one included, but the default protocol has no value kinds,
    // so the table it classifies has no column and nothing is declared.
    let keyed = SchemaBuilder::new(&Protocol::default())
        .vertex("t", "record", None)
        .map(|b| {
            b.key(
                "t",
                panproto_schema::Key {
                    id: "pk".into(),
                    parts: vec![],
                    role: panproto_schema::KeyRole::Primary,
                    scope: panproto_schema::KeyScope::Document,
                },
            )
        })
        .and_then(SchemaBuilder::build)
        .unwrap();
    let rel = panproto_inst::RelSchema::new(&keyed, &Protocol::default()).unwrap();
    assert!(rel.is_table("t"));
    assert!(rel.columns("t").is_empty());
    let attrs = panproto_inst::AttributeSchema::from_schema(&keyed, &Protocol::default());
    assert!(attrs.for_vertex("t").is_empty());
    // On a table with a leaf and a structured column, the default protocol
    // declares the leaf column alone, and the classifying one both.
    let classifying = protocol_with_struct();
    let schema = leaf_and_struct_table(&classifying);
    let declared = |p: &Protocol| -> Vec<String> {
        panproto_inst::AttributeSchema::from_schema(&schema, p)
            .for_vertex("t")
            .iter()
            .map(|d| d.name.clone())
            .collect()
    };
    assert_eq!(declared(&Protocol::default()), vec!["a".to_owned()]);
    assert_eq!(
        declared(&classifying),
        vec!["a".to_owned(), "addr".to_owned()]
    );
}
