#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Round-trip laws and boundary cases for asymmetric table lenses.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;

use panproto_expr::{BuiltinOp, Expr, Literal};
use panproto_gat::{CoercionClass, Name};
use panproto_inst::identity::{
    AnyComplement, Encoding, IdentityCarrier, IdentityField, decode_complement,
};
use panproto_inst::{
    BoundaryProtocols, CompiledMigration, ComplementShape, EdgeId, FInstance, FidelityReason,
    FieldTransform, GInstance, IdentityError, Node, NotTableReason, PutDomainError, RestrictError,
    RowId, SynthIndex, TableComplement, TableEmbedding, TableShapeReport, Value, ViewRef,
};
use panproto_lens::asymmetric::ComplementCompose;
use panproto_lens::table_lens;
use panproto_lens::{
    ComplementIdentity, GraphComposite, LawShape, Lens, LensError, LensPath, LensShape, SmallScope,
    TableContext, TableShape,
};
use panproto_schema::{
    Cardinality, ConstraintSemantics, Edge, Key, KeyPart, KeyRole, KeyScope, OnUnresolved,
    PathSyntax, Protocol, RefSource, Reference, ResolveMode, ScalarKind, Schema, SchemaBuilder,
};
use proptest::prelude::*;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TableLensFixture {
    format_version: u16,
    name: String,
    source_protocol: String,
    target_protocol: String,
    class: Option<String>,
    expected_error: Option<String>,
    runner: String,
}

fn fixture_protocol(name: &str) -> Protocol {
    match name {
        "table_test_v1" => protocol(),
        other => panic!("unknown table-lens fixture protocol `{other}`"),
    }
}

fn replay_fixture(fixture: &TableLensFixture) {
    let source_protocol = fixture_protocol(&fixture.source_protocol);
    let target_protocol = fixture_protocol(&fixture.target_protocol);
    assert_eq!(source_protocol, target_protocol);

    match fixture.runner.as_str() {
        "table_drop" => dropped_rows_and_tables_are_complement_residue(),
        "column_drop" => a_dropped_column_is_restored_while_a_kept_cell_changes(),
        "selection" => row_deletion_cascades_into_hidden_pairs(),
        "transform_with_inverse" => an_edit_through_an_invertible_table_transform_pulls_back(),
        "transform_without_inverse" | "transform_not_invertible" => {
            editing_a_noninvertible_table_transform_is_refused();
        }
        "vertex_merge" | "ambiguous_reverse_anchor" => {
            a_row_in_a_merged_table_is_an_ambiguous_reverse_anchor();
        }
        "edge_merge" => recorded_pairs_restore_every_original_carrier(),
        "contraction" | "synthesized_edited" => {
            contracted_pairs_are_read_only_and_round_trip();
        }
        "reinserted_selection" => {
            a_reinserted_row_does_not_regain_its_cascaded_hidden_foreign_key();
        }
        "two_origin_pair" => a_two_origin_pair_is_read_only_and_restores_its_direct_carrier(),
        "id_collision" => a_view_row_cannot_reuse_a_hidden_row_id(),
        "contracted_edge_inserted" => inserting_a_contracted_pair_is_refused(),
        "unrepresentable_view_edge" => a_pair_with_no_source_preimage_is_unrepresentable(),
        "survival_predicate_fails" | "survival_predicate_refused" => {
            table_put_reports_false_and_refused_survival_predicates();
        }
        "derived_cell_edited" => editing_a_derived_table_cell_is_refused(),
        "duplicate_key" => duplicate_key_ties_break_on_column_and_canonical_value_bytes(),
        "missing_required_cell" => a_primary_key_column_is_required(),
        other => panic!("unknown table-lens fixture runner `{other}`"),
    }
}

#[test]
fn the_table_lens_fixture_manifest_is_complete_and_replayable() {
    const REQUIRED_CLASSES: [&str; 10] = [
        "table_drop",
        "column_drop",
        "selection",
        "transform_with_inverse",
        "transform_without_inverse",
        "vertex_merge",
        "edge_merge",
        "contraction",
        "reinserted_selection",
        "two_origin_pair",
    ];
    const TABLE_ERRORS: [&str; 11] = [
        "id_collision",
        "synthesized_edited",
        "contracted_edge_inserted",
        "unrepresentable_view_edge",
        "ambiguous_reverse_anchor",
        "survival_predicate_fails",
        "derived_cell_edited",
        "transform_not_invertible",
        "duplicate_key",
        "missing_required_cell",
        "survival_predicate_refused",
    ];

    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/table_lens");
    let mut manifests = fs::read_dir(&root)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .map(|entry| entry.path().join("manifest.json"))
        .collect::<Vec<_>>();
    manifests.sort();

    let mut names = BTreeSet::new();
    let mut classes = BTreeSet::new();
    let mut errors = BTreeSet::new();
    for path in manifests {
        let fixture: TableLensFixture =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(fixture.format_version, 1);
        assert_eq!(
            path.parent()
                .unwrap()
                .file_name()
                .unwrap()
                .to_str()
                .unwrap(),
            fixture.name
        );
        assert!(names.insert(fixture.name.clone()));
        if let Some(class) = &fixture.class {
            classes.insert(class.clone());
        }
        if let Some(error) = &fixture.expected_error {
            errors.insert(error.clone());
        }
        replay_fixture(&fixture);
    }

    assert_eq!(
        classes,
        REQUIRED_CLASSES.into_iter().map(str::to_owned).collect()
    );
    assert_eq!(
        errors,
        TABLE_ERRORS.into_iter().map(str::to_owned).collect()
    );
}

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

fn required_protocol() -> Protocol {
    let mut protocol = protocol();
    protocol.constraint_sorts.push("required".to_owned());
    protocol
        .constraint_semantics
        .insert("required".to_owned(), ConstraintSemantics::RequiredValue);
    protocol
}

const fn contexts(p: &Protocol) -> BoundaryProtocols<'_> {
    BoundaryProtocols { src: p, tgt: p }
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

fn tables(spec: &[(&str, &[(&str, &str)])], relations: &[&Edge]) -> Schema {
    let p = protocol();
    let mut builder = SchemaBuilder::new(&p);
    for (table, columns) in spec {
        builder = builder.vertex(table, "record", None).unwrap();
        for (name, kind) in *columns {
            builder = builder
                .vertex(&format!("{table}.{name}"), kind, None)
                .unwrap()
                .edge(table, &format!("{table}.{name}"), "prop", Some(name))
                .unwrap();
        }
    }
    for relation in relations {
        builder = builder
            .edge(
                &relation.src,
                &relation.tgt,
                &relation.kind,
                relation.name.as_deref(),
            )
            .unwrap();
    }
    builder.build().unwrap()
}

fn identity(schema: &Schema) -> Lens {
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

fn row(values: &[(&str, Value)]) -> HashMap<String, Value> {
    values
        .iter()
        .map(|(key, value)| ((*key).to_owned(), value.clone()))
        .collect()
}

fn one_column() -> (Protocol, Schema, Lens, FInstance) {
    let p = protocol();
    let schema = tables(&[("t", &[("a", "int")])], &[]);
    let lens = identity(&schema);
    let source = FInstance::new()
        .with_rows("t", [(RowId(0), row(&[("a", Value::Int(1))]))])
        .unwrap();
    (p, schema, lens, source)
}

fn reference_for(edge: &Edge, cardinality: Cardinality, nullable: bool) -> Reference {
    Reference {
        id: "reference".into(),
        holder: edge.src.clone(),
        source: RefSource::Path {
            field: Vec::new(),
            syntax: PathSyntax::IdRef,
        },
        target_vertex: edge.tgt.clone(),
        target_key: None,
        edge: Some(edge.clone()),
        cardinality,
        nullable,
        on_unresolved: OnUnresolved::Error,
        mode: ResolveMode::AddEdge,
    }
}

fn two_lifts(edge: &Edge) -> GInstance {
    GInstance::new()
        .with_node(Node::new(0, edge.src.clone()))
        .with_node(Node::new(1, edge.tgt.clone()))
        .with_node(Node::new(2, edge.tgt.clone()))
        .with_edge_id(EdgeId(7), 0, 1, edge.clone())
        .with_edge_id(EdgeId(9), 0, 2, edge.clone())
}

fn no_lift(edge: &Edge) -> GInstance {
    GInstance::new()
        .with_node(Node::new(0, edge.src.clone()))
        .with_node(Node::new(1, edge.tgt.clone()))
}

#[test]
fn an_fk_is_functional_exactly_when_a_one_reference_declares_it() {
    let edge = fk("holder", "target", "target");
    let base = tables(&[("holder", &[]), ("target", &[])], &[&edge]);
    let multiple = two_lifts(&edge);
    let missing = no_lift(&edge);

    for nullable in [false, true] {
        let mut many = base.clone();
        many.declarations.references.insert(
            "reference".into(),
            reference_for(&edge, Cardinality::Many, nullable),
        );
        assert_eq!(multiple.table_shape(&many), TableShapeReport::Table);
        assert_eq!(missing.table_shape(&many), TableShapeReport::Table);
    }

    let mut optional_one = base.clone();
    optional_one.declarations.references.insert(
        "reference".into(),
        reference_for(&edge, Cardinality::One, true),
    );
    assert!(matches!(
        multiple.table_shape(&optional_one),
        TableShapeReport::NotTable {
            reason: NotTableReason::MultipleLifts { node: 0, edge: found, .. }
        } if found == edge
    ));
    assert_eq!(missing.table_shape(&optional_one), TableShapeReport::Table);

    let mut required_one = base;
    required_one.declarations.references.insert(
        "reference".into(),
        reference_for(&edge, Cardinality::One, false),
    );
    assert!(matches!(
        multiple.table_shape(&required_one),
        TableShapeReport::NotTable {
            reason: NotTableReason::MultipleLifts { node: 0, edge: found, .. }
        } if found == edge
    ));
    assert_eq!(
        missing.table_shape(&required_one),
        TableShapeReport::NotTable {
            reason: NotTableReason::MissingLift { node: 0, edge },
        }
    );
}

#[test]
fn a_schema_without_references_treats_every_fk_as_a_relation() {
    let edge = fk("holder", "target", "target");
    let schema = tables(&[("holder", &[]), ("target", &[])], &[&edge]);
    assert!(schema.declarations.references.is_empty());
    assert_eq!(
        two_lifts(&edge).table_shape(&schema),
        TableShapeReport::Table
    );
    assert_eq!(no_lift(&edge).table_shape(&schema), TableShapeReport::Table);
}

#[test]
fn table_get_stamps_its_complement() {
    let (protocol, _, lens, source) = one_column();
    let (_, complement) = table_lens::get(&lens, &source, contexts(&protocol)).unwrap();
    assert_eq!(
        complement.identity,
        Some(lens.identity(ComplementShape::Functor))
    );
    assert!(
        complement
            .verify(&lens, panproto_inst::IdentityPolicy::Strict)
            .is_ok()
    );
}

#[test]
fn every_get_path_stamps_its_complement() {
    let (protocol, schema, lens, source) = one_column();
    let expected = lens.identity(ComplementShape::Functor);
    let (_, direct) = table_lens::get(&lens, &source, contexts(&protocol)).unwrap();
    assert_eq!(direct.identity.as_ref(), Some(&expected));

    let path = LensPath::single(identity(&schema));
    let (_, path_complement) = path
        .get_table(&source, &[protocol.clone(), protocol])
        .unwrap();
    assert_eq!(path_complement.entries().len(), 1);
    assert_eq!(path_complement.entries()[0].envelope, expected);
    assert_eq!(
        path_complement.entries()[0].complement.identity.as_ref(),
        Some(&expected)
    );
}

fn binary32_table_lens(output: f64) -> (Lens, Protocol) {
    let protocol = Protocol {
        value_kinds: BTreeMap::from([("float32".to_owned(), ScalarKind::Float { bits: 32 })]),
        non_value_kinds: BTreeSet::from(["record".to_owned()]),
        ..Protocol::default()
    };
    let schema = SchemaBuilder::new(&protocol)
        .vertex("row", "record", None)
        .unwrap()
        .vertex("row.x", "float32", None)
        .unwrap()
        .edge("row", "row.x", "column", Some("x"))
        .unwrap()
        .build()
        .unwrap();
    let lens = Lens {
        compiled: CompiledMigration {
            surviving_verts: schema.vertices.keys().cloned().collect(),
            surviving_edges: schema.edges.keys().cloned().collect(),
            field_transforms: HashMap::from([(
                "row".into(),
                vec![FieldTransform::ComputeField {
                    target_key: "x".to_owned(),
                    expr: Expr::Lit(Literal::Float(output)),
                    inverse: None,
                    coercion_class: CoercionClass::Opaque,
                }],
            )]),
            ..CompiledMigration::default()
        },
        src_schema: schema.clone(),
        tgt_schema: schema,
    };
    (lens, protocol)
}

#[test]
fn a_float_that_is_not_a_binary32_is_refused_by_table_get() {
    let (lens, protocol) = binary32_table_lens(0.1);
    let source = FInstance::new()
        .with_rows("row", [(RowId(0), row(&[]))])
        .unwrap();
    let error = table_lens::get(&lens, &source, contexts(&protocol))
        .expect_err("0.1 is not exactly binary32");
    assert!(
        matches!(
            &error,
            LensError::Restrict(restrict)
                if matches!(
                    restrict.unlocated(),
                    RestrictError::ValueFidelity { node: 0, error }
                        if error.site.vertex.as_str() == "row.x"
                            && error.reason == FidelityReason::PrecisionExceeded { max: 9 }
                )
        ),
        "{error:?}"
    );
}

#[test]
fn an_exact_binary32_table_cell_is_restored_by_put() {
    let exact = f64::from(0.1_f32);
    let (lens, protocol) = binary32_table_lens(exact);
    let source = FInstance::new()
        .with_rows("row", [(RowId(0), row(&[("x", Value::Int(7))]))])
        .unwrap();
    let (view, complement) = table_lens::get(&lens, &source, contexts(&protocol)).unwrap();
    assert_eq!(view.row(RowId(0)).unwrap()["x"], Value::Float(exact));
    assert_eq!(
        table_lens::put(&lens, &view, &complement, contexts(&protocol)).unwrap(),
        source
    );
}

fn structural_column_lens(
    source_columns: &[&str],
    target_columns: &[&str],
    renames: &[(&str, &str)],
) -> Lens {
    let source = tables(
        &[(
            "t",
            &source_columns
                .iter()
                .map(|column| (*column, "int"))
                .collect::<Vec<_>>(),
        )],
        &[],
    );
    let target = tables(
        &[(
            "t",
            &target_columns
                .iter()
                .map(|column| (*column, "int"))
                .collect::<Vec<_>>(),
        )],
        &[],
    );
    Lens {
        compiled: CompiledMigration {
            surviving_verts: target.vertices.keys().cloned().collect(),
            surviving_edges: target.edges.keys().cloned().collect(),
            vertex_remap: renames
                .iter()
                .map(|(source, target)| {
                    (format!("t.{source}").into(), format!("t.{target}").into())
                })
                .collect(),
            edge_remap: renames
                .iter()
                .map(|(source, target)| (column("t", source), column("t", target)))
                .collect(),
            ..CompiledMigration::default()
        },
        src_schema: source,
        tgt_schema: target,
    }
}

#[test]
fn a_column_swap_writes_the_swapped_source() {
    let protocol = protocol();
    let lens = structural_column_lens(&["a", "b"], &["a", "b"], &[("a", "b"), ("b", "a")]);
    let source = FInstance::new()
        .with_rows(
            "t",
            [(RowId(0), row(&[("a", Value::Int(1)), ("b", Value::Int(2))]))],
        )
        .unwrap();
    let (mut view, complement) = table_lens::get(&lens, &source, contexts(&protocol)).unwrap();
    assert_eq!(
        view.row(RowId(0)).unwrap(),
        &row(&[("a", Value::Int(2)), ("b", Value::Int(1))])
    );
    view.row_mut(RowId(0))
        .unwrap()
        .insert("a".into(), Value::Int(9));
    assert_eq!(
        table_lens::put(&lens, &view, &complement, contexts(&protocol)).unwrap(),
        FInstance::new()
            .with_rows(
                "t",
                [(RowId(0), row(&[("a", Value::Int(1)), ("b", Value::Int(9))]))],
            )
            .unwrap()
    );
}

#[test]
fn a_many_to_one_rename_writes_the_last_source() {
    let protocol = protocol();
    let lens = structural_column_lens(&["a", "b"], &["c"], &[("a", "c"), ("b", "c")]);
    let source = FInstance::new()
        .with_rows(
            "t",
            [(RowId(0), row(&[("a", Value::Int(1)), ("b", Value::Int(2))]))],
        )
        .unwrap();
    let (mut view, complement) = table_lens::get(&lens, &source, contexts(&protocol)).unwrap();
    assert_eq!(view.row(RowId(0)).unwrap()["c"], Value::Int(2));
    view.row_mut(RowId(0))
        .unwrap()
        .insert("c".into(), Value::Int(9));
    let updated = table_lens::put(&lens, &view, &complement, contexts(&protocol)).unwrap();
    assert_eq!(
        updated.row(RowId(0)).unwrap(),
        &row(&[("a", Value::Int(1)), ("b", Value::Int(9))])
    );
}

#[test]
fn a_transform_fixing_its_input_round_trips_or_refuses() {
    let (protocol, _, mut square, source) = one_column();
    square.compiled.field_transforms.insert(
        "t".into(),
        vec![FieldTransform::ApplyExpr {
            key: "a".into(),
            expr: Expr::Builtin(
                BuiltinOp::Mul,
                vec![Expr::Var("a".into()), Expr::Var("a".into())],
            ),
            inverse: Some(Expr::Lit(Literal::Int(1))),
            coercion_class: CoercionClass::Iso,
        }],
    );
    let (view, complement) = table_lens::get(&square, &source, contexts(&protocol)).unwrap();
    assert_eq!(view.row(RowId(0)).unwrap()["a"], Value::Int(1));
    assert_eq!(
        table_lens::put(&square, &view, &complement, contexts(&protocol)).unwrap(),
        source
    );

    let (_, _, mut absolute, _) = one_column();
    absolute.compiled.field_transforms.insert(
        "t".into(),
        vec![FieldTransform::ApplyExpr {
            key: "a".into(),
            expr: Expr::Builtin(BuiltinOp::Abs, vec![Expr::Var("a".into())]),
            inverse: None,
            coercion_class: CoercionClass::Projection,
        }],
    );
    let negative = FInstance::new()
        .with_rows("t", [(RowId(0), row(&[("a", Value::Int(-1))]))])
        .unwrap();
    let (mut view, complement) =
        table_lens::get(&absolute, &negative, contexts(&protocol)).unwrap();
    view.row_mut(RowId(0))
        .unwrap()
        .insert("a".into(), Value::Int(2));
    assert!(matches!(
        table_lens::put(&absolute, &view, &complement, contexts(&protocol)),
        Err(LensError::PutDomain(error))
            if matches!(*error, PutDomainError::TransformNotInvertible {
                elem: ViewRef::Row(RowId(0)), ref field
            } if field == "a")
    ));
}

#[test]
fn a_rename_into_an_unfated_name_writes_that_name() {
    let protocol = protocol();
    let lens = structural_column_lens(&["b"], &["a"], &[("b", "a")]);
    let source = FInstance::new()
        .with_rows("t", [(RowId(0), row(&[("b", Value::Int(2))]))])
        .unwrap();
    let (mut view, complement) = table_lens::get(&lens, &source, contexts(&protocol)).unwrap();
    view.row_mut(RowId(0))
        .unwrap()
        .insert("a".into(), Value::Int(9));
    let updated = table_lens::put(&lens, &view, &complement, contexts(&protocol)).unwrap();
    assert_eq!(updated.row(RowId(0)).unwrap()["b"], Value::Int(9));
}

fn raw_complement_free_put(
    lens: &Lens,
    view: &FInstance,
    boundary: BoundaryProtocols<'_>,
) -> Result<FInstance, LensError> {
    let mut complement = TableComplement::empty();
    complement.stamp(lens.identity(ComplementShape::Functor), &lens.src_schema);
    table_lens::put(lens, view, &complement, boundary)
}

#[test]
fn table_put_without_complement_agrees_with_the_shape_classifier() {
    let (protocol, schema, lens, source) = one_column();
    let boundary = contexts(&protocol);
    let (view, _) = table_lens::get(&lens, &source, boundary).unwrap();
    assert!(table_lens::is_isomorphism(&lens, boundary));
    assert!(table_lens::obstruction_to_isomorphism(&lens, boundary).is_none());
    let restored = table_lens::put_without_complement(&lens, &view, boundary).unwrap();
    assert_eq!(source, restored);

    let projection = Lens {
        compiled: CompiledMigration {
            surviving_verts: std::iter::once("t".into()).collect(),
            ..CompiledMigration::default()
        },
        src_schema: schema,
        tgt_schema: tables(&[("t", &[])], &[]),
    };
    let (view, _) = table_lens::get(&projection, &source, boundary).unwrap();
    assert!(!table_lens::is_isomorphism(&projection, boundary));
    assert!(matches!(
        table_lens::put_without_complement(&projection, &view, boundary),
        Err(LensError::NotAnIsomorphism { .. })
    ));
}

#[test]
fn table_obstructions_are_reported() {
    let protocol = protocol();
    let relation = fk("right", "left", "left");
    let lens = Lens {
        compiled: CompiledMigration {
            surviving_verts: std::iter::once("middle".into()).collect(),
            ..CompiledMigration::default()
        },
        src_schema: tables(
            &[("right", &[]), ("middle", &[]), ("left", &[])],
            &[&relation],
        ),
        tgt_schema: tables(&[("middle", &[])], &[]),
    };
    let boundary = contexts(&protocol);

    assert!(!table_lens::is_isomorphism(&lens, boundary));
    assert_eq!(
        table_lens::obstruction_to_isomorphism(&lens, boundary).as_deref(),
        Some("table `left` is dropped, so its rows cannot be reconstructed without a complement")
    );
}

#[test]
fn a_target_only_default_is_complement_free_for_tables() {
    let protocol = protocol();
    let source_schema = tables(&[("t", &[("a", "int")])], &[]);
    let target_schema = tables(&[("t", &[("a", "int"), ("b", "int")])], &[]);
    let mut compiled = CompiledMigration {
        surviving_verts: source_schema.vertices.keys().cloned().collect(),
        surviving_edges: source_schema.edges.keys().cloned().collect(),
        ..CompiledMigration::default()
    };
    compiled.field_transforms.insert(
        "t".into(),
        vec![FieldTransform::AddField {
            key: "b".to_owned(),
            value: Value::Int(0),
        }],
    );
    let lens = Lens {
        compiled,
        src_schema: source_schema,
        tgt_schema: target_schema,
    };
    let source = FInstance::new()
        .with_rows("t", [(RowId(0), row(&[("a", Value::Int(1))]))])
        .unwrap();
    let boundary = contexts(&protocol);
    assert!(table_lens::is_isomorphism(&lens, boundary));
    let (view, _) = table_lens::get(&lens, &source, boundary).unwrap();
    assert_eq!(
        view.row(RowId(0)).and_then(|r| r.get("b")),
        Some(&Value::Int(0))
    );
    let restored = table_lens::put_without_complement(&lens, &view, boundary).unwrap();
    assert_eq!(restored, source);
}

#[test]
fn repeated_defaults_on_one_derived_column_are_complement_free() {
    let protocol = protocol();
    let source_schema = tables(&[("t", &[("a", "int")])], &[]);
    let target_schema = tables(&[("t", &[("a", "int"), ("b", "int")])], &[]);
    let mut compiled = CompiledMigration {
        surviving_verts: source_schema.vertices.keys().cloned().collect(),
        surviving_edges: source_schema.edges.keys().cloned().collect(),
        ..CompiledMigration::default()
    };
    compiled.field_transforms.insert(
        "t".into(),
        vec![
            FieldTransform::AddField {
                key: "b".to_owned(),
                value: Value::Int(0),
            },
            FieldTransform::AddField {
                key: "b".to_owned(),
                value: Value::Int(1),
            },
        ],
    );
    let lens = Lens {
        compiled,
        src_schema: source_schema,
        tgt_schema: target_schema,
    };
    let source = FInstance::new()
        .with_rows("t", [(RowId(0), row(&[("a", Value::Int(7))]))])
        .unwrap();
    let boundary = contexts(&protocol);

    assert!(table_lens::is_isomorphism(&lens, boundary));
    let (view, complement) = table_lens::get(&lens, &source, boundary).unwrap();
    assert!(complement.residue_is_trivial());
    assert_eq!(view.row(RowId(0)).unwrap()["b"], Value::Int(0));
    assert_eq!(
        table_lens::put_without_complement(&lens, &view, boundary).unwrap(),
        source
    );
}

#[test]
fn renaming_a_source_column_over_a_derived_column_is_complement_free() {
    let protocol = protocol();
    let source_schema = tables(&[("t", &[("old", "int")])], &[]);
    let target_schema = tables(&[("t", &[("new", "int")])], &[]);
    let mut compiled = CompiledMigration {
        surviving_verts: std::iter::once(Name::from("t")).collect(),
        ..CompiledMigration::default()
    };
    compiled.field_transforms.insert(
        "t".into(),
        vec![
            FieldTransform::AddField {
                key: "new".to_owned(),
                value: Value::Int(0),
            },
            FieldTransform::RenameField {
                old_key: "old".to_owned(),
                new_key: "new".to_owned(),
            },
        ],
    );
    let lens = Lens {
        compiled,
        src_schema: source_schema,
        tgt_schema: target_schema,
    };
    let source = FInstance::new()
        .with_rows("t", [(RowId(0), row(&[("old", Value::Int(7))]))])
        .unwrap();
    let boundary = contexts(&protocol);

    assert!(table_lens::is_isomorphism(&lens, boundary));
    let (view, _) = table_lens::get(&lens, &source, boundary).unwrap();
    assert_eq!(view.row(RowId(0)).unwrap()["new"], Value::Int(7));
    assert_eq!(
        table_lens::put_without_complement(&lens, &view, boundary).unwrap(),
        source
    );
}

#[test]
fn a_fresh_computed_column_does_not_obstruct_complement_free_put() {
    let protocol = protocol();
    let source_schema = tables(&[("t", &[("a", "int")])], &[]);
    let target_schema = tables(&[("t", &[("a", "int"), ("b", "int")])], &[]);
    let mut compiled = CompiledMigration {
        surviving_verts: source_schema.vertices.keys().cloned().collect(),
        surviving_edges: source_schema.edges.keys().cloned().collect(),
        ..CompiledMigration::default()
    };
    compiled.field_transforms.insert(
        "t".into(),
        vec![FieldTransform::ComputeField {
            target_key: "b".to_owned(),
            expr: Expr::Var("a".into()),
            inverse: None,
            coercion_class: CoercionClass::Projection,
        }],
    );
    let lens = Lens {
        compiled,
        src_schema: source_schema,
        tgt_schema: target_schema,
    };
    let source = FInstance::new()
        .with_rows("t", [(RowId(0), row(&[("a", Value::Int(7))]))])
        .unwrap();
    let boundary = contexts(&protocol);

    assert!(table_lens::is_isomorphism(&lens, boundary));
    let (view, complement) = table_lens::get(&lens, &source, boundary).unwrap();
    assert!(complement.residue_is_trivial());
    assert_eq!(view.row(RowId(0)).unwrap()["b"], Value::Int(7));
    assert_eq!(
        table_lens::put_without_complement(&lens, &view, boundary).unwrap(),
        source
    );
}

#[test]
fn a_fresh_computed_isomorphism_preserves_a_retained_source_column() {
    let protocol = protocol();
    let source_schema = tables(&[("t", &[("a", "int")])], &[]);
    let target_schema = tables(&[("t", &[("a", "int"), ("b", "int")])], &[]);
    let mut compiled = CompiledMigration {
        surviving_verts: source_schema.vertices.keys().cloned().collect(),
        surviving_edges: source_schema.edges.keys().cloned().collect(),
        ..CompiledMigration::default()
    };
    compiled.field_transforms.insert(
        "t".into(),
        vec![FieldTransform::ComputeField {
            target_key: "b".to_owned(),
            expr: Expr::Var("a".into()),
            inverse: Some(Expr::Var("b".into())),
            coercion_class: CoercionClass::Iso,
        }],
    );
    let lens = Lens {
        compiled,
        src_schema: source_schema,
        tgt_schema: target_schema,
    };
    let source = FInstance::new()
        .with_rows(
            "t",
            [
                (RowId(0), row(&[("a", Value::Int(7))])),
                (RowId(1), row(&[])),
                (RowId(2), row(&[("a", Value::Null)])),
            ],
        )
        .unwrap();
    let boundary = contexts(&protocol);

    assert!(table_lens::is_isomorphism(&lens, boundary));
    assert!(table_lens::obstruction_to_isomorphism(&lens, boundary).is_none());
    let (view, complement) = table_lens::get(&lens, &source, boundary).unwrap();
    assert!(complement.residue_is_trivial());
    assert_eq!(
        table_lens::put_without_complement(&lens, &view, boundary).unwrap(),
        source
    );
}

#[test]
fn an_optional_computed_replacement_is_not_a_table_isomorphism() {
    let protocol = protocol();
    let source_schema = tables(&[("t", &[("a", "int")])], &[]);
    let target_schema = tables(&[("t", &[("b", "int")])], &[]);
    let replacement = |inverse, coercion_class| {
        let mut compiled = CompiledMigration {
            surviving_verts: std::iter::once(Name::from("t")).collect(),
            ..CompiledMigration::default()
        };
        compiled.field_transforms.insert(
            "t".into(),
            vec![
                FieldTransform::ComputeField {
                    target_key: "b".to_owned(),
                    expr: Expr::Var("a".into()),
                    inverse,
                    coercion_class,
                },
                FieldTransform::DropField {
                    key: "a".to_owned(),
                },
            ],
        );
        Lens {
            compiled,
            src_schema: source_schema.clone(),
            tgt_schema: target_schema.clone(),
        }
    };
    let lossy = replacement(None, CoercionClass::Projection);
    let invertible = replacement(Some(Expr::Var("b".into())), CoercionClass::Iso);
    let source = FInstance::new()
        .with_rows("t", [(RowId(0), row(&[("a", Value::Int(7))]))])
        .unwrap();
    let boundary = contexts(&protocol);

    assert!(!table_lens::is_isomorphism(&lossy, boundary));
    assert!(!table_lens::is_isomorphism(&invertible, boundary));
    let (view, _) = table_lens::get(&invertible, &source, boundary).unwrap();
    assert_eq!(view.row(RowId(0)).unwrap()["b"], Value::Int(7));
    assert_eq!(
        raw_complement_free_put(&invertible, &view, boundary).unwrap(),
        source
    );

    let absent = FInstance::new()
        .with_rows("t", [(RowId(0), row(&[]))])
        .unwrap();
    let (absent_view, _) = table_lens::get(&invertible, &absent, boundary).unwrap();
    assert_ne!(
        raw_complement_free_put(&invertible, &absent_view, boundary).unwrap(),
        absent
    );
}

#[test]
fn an_in_place_computed_isomorphism_restores_its_source_cell() {
    let (protocol, _schema, mut lens, source) = one_column();
    lens.compiled.field_transforms.insert(
        "t".into(),
        vec![FieldTransform::ComputeField {
            target_key: "a".to_owned(),
            expr: Expr::Builtin(
                BuiltinOp::Add,
                vec![Expr::Var("a".into()), Expr::Lit(Literal::Int(1))],
            ),
            inverse: Some(Expr::Builtin(
                BuiltinOp::Sub,
                vec![Expr::Var("a".into()), Expr::Lit(Literal::Int(1))],
            )),
            coercion_class: CoercionClass::Iso,
        }],
    );
    let boundary = contexts(&protocol);

    assert!(table_lens::is_isomorphism(&lens, boundary));
    let (view, _) = table_lens::get(&lens, &source, boundary).unwrap();
    assert_eq!(view.row(RowId(0)).unwrap()["a"], Value::Int(2));
    assert_eq!(
        table_lens::put_without_complement(&lens, &view, boundary).unwrap(),
        source
    );
}

#[test]
fn redundant_transform_and_structural_renames_remain_invertible() {
    let protocol = protocol();
    let source_schema = tables(&[("t", &[("a", "int")])], &[]);
    let target_schema = tables(&[("t", &[("b", "int")])], &[]);
    let mut compiled = CompiledMigration {
        surviving_verts: ["t", "t.b"].into_iter().map(Name::from).collect(),
        surviving_edges: [column("t", "b")].into(),
        vertex_remap: HashMap::from([("t.a".into(), "t.b".into())]),
        edge_remap: HashMap::from([(column("t", "a"), column("t", "b"))]),
        ..CompiledMigration::default()
    };
    compiled.field_transforms.insert(
        "t".into(),
        vec![FieldTransform::RenameField {
            old_key: "a".to_owned(),
            new_key: "b".to_owned(),
        }],
    );
    let lens = Lens {
        compiled,
        src_schema: source_schema,
        tgt_schema: target_schema,
    };
    let source = FInstance::new()
        .with_rows("t", [(RowId(0), row(&[("a", Value::Int(7))]))])
        .unwrap();
    let boundary = contexts(&protocol);

    assert!(table_lens::is_isomorphism(&lens, boundary));
    let (view, _) = table_lens::get(&lens, &source, boundary).unwrap();
    assert_eq!(view.row(RowId(0)).unwrap()["b"], Value::Int(7));
    assert_eq!(
        table_lens::put_without_complement(&lens, &view, boundary).unwrap(),
        source
    );
}

#[test]
fn an_encoded_carrier_must_reach_its_matching_inverse() {
    let protocol = protocol();
    let source_schema = tables(&[("t", &[("a", "int")])], &[]);
    let target_schema = tables(&[("t", &[("b", "int")])], &[]);
    let mut compiled = CompiledMigration {
        surviving_verts: ["t", "t.b"].into_iter().map(Name::from).collect(),
        surviving_edges: [column("t", "b")].into(),
        vertex_remap: HashMap::from([("t.a".into(), "t.b".into())]),
        edge_remap: HashMap::from([(column("t", "a"), column("t", "b"))]),
        ..CompiledMigration::default()
    };
    compiled.field_transforms.insert(
        "t".into(),
        vec![
            FieldTransform::ComputeField {
                target_key: "b".to_owned(),
                expr: Expr::Builtin(
                    BuiltinOp::Add,
                    vec![Expr::Var("a".into()), Expr::Lit(Literal::Int(1))],
                ),
                inverse: Some(Expr::Builtin(
                    BuiltinOp::Sub,
                    vec![Expr::Var("b".into()), Expr::Lit(Literal::Int(1))],
                )),
                coercion_class: CoercionClass::Iso,
            },
            FieldTransform::DropField {
                key: "a".to_owned(),
            },
        ],
    );
    let lens = Lens {
        compiled,
        src_schema: source_schema,
        tgt_schema: target_schema,
    };
    let source = FInstance::new()
        .with_rows("t", [(RowId(0), row(&[("a", Value::Int(1))]))])
        .unwrap();
    let boundary = contexts(&protocol);
    let (view, _) = table_lens::get(&lens, &source, boundary).unwrap();
    assert_eq!(view.row(RowId(0)).unwrap()["b"], Value::Int(2));

    assert!(!table_lens::is_isomorphism(&lens, boundary));
    assert!(matches!(
        table_lens::put(&lens, &view, &TableComplement::empty(), boundary),
        Err(LensError::PutDomain(error))
            if matches!(*error, PutDomainError::DerivedCellEdited {
                elem: ViewRef::Row(RowId(0)), ref field
            } if field == "b")
    ));
}

#[test]
fn structural_column_rename_chains_are_simultaneous() {
    let protocol = protocol();
    let source_schema = tables(&[("t", &[("a", "int"), ("b", "int")])], &[]);
    let target_schema = tables(&[("t", &[("b", "int"), ("c", "int")])], &[]);
    let lens = Lens {
        compiled: CompiledMigration {
            surviving_verts: ["t", "t.b", "t.c"].into_iter().map(Name::from).collect(),
            surviving_edges: [column("t", "b"), column("t", "c")].into(),
            vertex_remap: HashMap::from([
                ("t.a".into(), "t.b".into()),
                ("t.b".into(), "t.c".into()),
            ]),
            edge_remap: HashMap::from([
                (column("t", "a"), column("t", "b")),
                (column("t", "b"), column("t", "c")),
            ]),
            ..CompiledMigration::default()
        },
        src_schema: source_schema,
        tgt_schema: target_schema,
    };
    let source = FInstance::new()
        .with_rows(
            "t",
            [(RowId(0), row(&[("a", Value::Int(1)), ("b", Value::Int(2))]))],
        )
        .unwrap();
    let boundary = contexts(&protocol);

    assert!(table_lens::is_isomorphism(&lens, boundary));
    let (view, _) = table_lens::get(&lens, &source, boundary).unwrap();
    assert_eq!(
        view.row(RowId(0)).unwrap(),
        &row(&[("b", Value::Int(1)), ("c", Value::Int(2))])
    );
    assert_eq!(
        table_lens::put_without_complement(&lens, &view, boundary).unwrap(),
        source
    );
}

#[test]
fn a_nested_path_transform_is_not_a_table_isomorphism() {
    let (protocol, _schema, mut lens, _source) = one_column();
    lens.compiled.field_transforms.insert(
        "t".into(),
        vec![FieldTransform::PathTransform {
            path: vec!["a".to_owned()],
            inner: Box::new(FieldTransform::RenameField {
                old_key: "x".to_owned(),
                new_key: "y".to_owned(),
            }),
        }],
    );
    let boundary = contexts(&protocol);
    assert!(!table_lens::is_isomorphism(&lens, boundary));
    assert!(
        table_lens::obstruction_to_isomorphism(&lens, boundary)
            .is_some_and(|detail| detail.contains("nested path transform"))
    );
}

#[test]
fn table_isomorphism_is_classified_throughout_finite_scope() {
    let protocol = protocol();
    let relation = fk("left", "right", "right");
    let schema = tables(
        &[
            ("left", &[("value", "int")]),
            ("right", &[("value", "int")]),
        ],
        &[&relation],
    );
    let lens = identity(&schema);
    let boundary = contexts(&protocol);
    let context = TableContext {
        src: protocol.clone(),
        tgt: protocol.clone(),
    };
    let sources = TableShape::enumerate_small(
        &schema,
        SmallScope {
            max_nodes: 2,
            max_edges: 1,
        },
        &context,
    );
    assert!(
        sources.iter().any(|source| source.pair_count() == 1),
        "finite table scope must include a foreign-key carrier"
    );
    assert!(table_lens::is_isomorphism(&lens, boundary));
    let identity_round_trips = sources.iter().all(|source| {
        let (view, _) = table_lens::get(&lens, source, boundary).expect("finite table get");
        raw_complement_free_put(&lens, &view, boundary).is_ok_and(|restored| *source == restored)
    });
    assert_eq!(
        table_lens::is_isomorphism(&lens, boundary),
        identity_round_trips
    );

    let source_schema = tables(&[("t", &[("a", "int")])], &[]);
    let target_schema = tables(&[("t", &[])], &[]);
    let projection = Lens {
        compiled: CompiledMigration {
            surviving_verts: std::iter::once("t".into()).collect(),
            ..CompiledMigration::default()
        },
        src_schema: source_schema.clone(),
        tgt_schema: target_schema,
    };
    assert!(!table_lens::is_isomorphism(&projection, boundary));
    let column_sources = TableShape::enumerate_small(
        &source_schema,
        SmallScope {
            max_nodes: 1,
            max_edges: 0,
        },
        &context,
    );
    let column_round_trips = column_sources.iter().all(|source| {
        let (view, _) =
            table_lens::get(&projection, source, boundary).expect("column projection get");
        raw_complement_free_put(&projection, &view, boundary)
            .is_ok_and(|restored| *source == restored)
    });
    assert!(
        !column_round_trips,
        "the finite scope has a populated dropped cell"
    );
    assert_eq!(
        table_lens::is_isomorphism(&projection, boundary),
        column_round_trips
    );
}

#[test]
fn table_foreign_key_classifier_agrees_with_raw_complement_free_put() {
    let protocol = protocol();
    let boundary = contexts(&protocol);
    let context = TableContext {
        src: protocol.clone(),
        tgt: protocol.clone(),
    };
    let source_relation = fk("left", "right", "right");
    let relation_source_schema = tables(
        &[
            ("left", &[("value", "int")]),
            ("right", &[("value", "int")]),
        ],
        &[&source_relation],
    );
    let relation_target_schema = tables(
        &[
            ("left", &[("value", "int")]),
            ("right", &[("value", "int")]),
        ],
        &[],
    );
    let relation_projection = Lens {
        compiled: CompiledMigration {
            surviving_verts: relation_target_schema.vertices.keys().cloned().collect(),
            surviving_edges: relation_target_schema.edges.keys().cloned().collect(),
            ..CompiledMigration::default()
        },
        src_schema: relation_source_schema.clone(),
        tgt_schema: relation_target_schema,
    };
    let relation_sources = TableShape::enumerate_small(
        &relation_source_schema,
        SmallScope {
            max_nodes: 2,
            max_edges: 1,
        },
        &context,
    );
    assert!(
        relation_sources
            .iter()
            .any(|source| source.pair_count() == 1)
    );
    let relation_round_trips = relation_sources.iter().all(|source| {
        let (view, _) = table_lens::get(&relation_projection, source, boundary)
            .expect("foreign-key projection get");
        raw_complement_free_put(&relation_projection, &view, boundary)
            .is_ok_and(|restored| *source == restored)
    });
    assert!(!relation_round_trips, "the finite scope has a dropped pair");
    assert_eq!(
        table_lens::is_isomorphism(&relation_projection, boundary),
        relation_round_trips
    );
}

#[test]
fn table_identity_get_put_round_trips() {
    let (p, _, lens, source) = one_column();
    let (view, complement) = table_lens::get(&lens, &source, contexts(&p)).unwrap();
    assert_eq!(
        table_lens::put(&lens, &view, &complement, contexts(&p)).unwrap(),
        source
    );
    assert_eq!(
        complement
            .identity
            .as_ref()
            .unwrap()
            .complement_format
            .shape,
        ComplementShape::Functor
    );
}

#[test]
fn an_edit_to_a_kept_cell_pulls_back() {
    let (p, _, lens, source) = one_column();
    let (mut view, complement) = table_lens::get(&lens, &source, contexts(&p)).unwrap();
    view.row_mut(RowId(0))
        .unwrap()
        .insert("a".into(), Value::Int(9));
    let updated = table_lens::put(&lens, &view, &complement, contexts(&p)).unwrap();
    assert_eq!(updated.row(RowId(0)).unwrap()["a"], Value::Int(9));
    assert_eq!(
        table_lens::get(&lens, &updated, contexts(&p)).unwrap().0,
        view
    );
}

#[test]
fn a_dropped_column_is_restored_while_a_kept_cell_changes() {
    let p = protocol();
    let src = tables(&[("t", &[("a", "int"), ("b", "int")])], &[]);
    let tgt = tables(&[("t", &[("a", "int")])], &[]);
    let lens = Lens {
        compiled: CompiledMigration {
            surviving_verts: ["t", "t.a"].into_iter().map(Name::from).collect(),
            surviving_edges: [column("t", "a")].into(),
            ..CompiledMigration::default()
        },
        src_schema: src,
        tgt_schema: tgt,
    };
    let source = FInstance::new()
        .with_rows(
            "t",
            [(RowId(0), row(&[("a", Value::Int(1)), ("b", Value::Int(2))]))],
        )
        .unwrap();
    let (mut view, complement) = table_lens::get(&lens, &source, contexts(&p)).unwrap();
    view.row_mut(RowId(0))
        .unwrap()
        .insert("a".into(), Value::Int(3));
    let updated = table_lens::put(&lens, &view, &complement, contexts(&p)).unwrap();
    assert_eq!(updated.row(RowId(0)).unwrap()["a"], Value::Int(3));
    assert_eq!(updated.row(RowId(0)).unwrap()["b"], Value::Int(2));
}

#[test]
fn an_edit_through_a_structural_column_rename_pulls_back() {
    let p = protocol();
    let src = tables(&[("t", &[("a", "int")])], &[]);
    let tgt = tables(&[("t", &[("a2", "int")])], &[]);
    let lens = Lens {
        compiled: CompiledMigration {
            surviving_verts: ["t", "t.a2"].into_iter().map(Name::from).collect(),
            surviving_edges: [column("t", "a2")].into(),
            vertex_remap: HashMap::from([("t.a".into(), "t.a2".into())]),
            edge_remap: HashMap::from([(column("t", "a"), column("t", "a2"))]),
            ..CompiledMigration::default()
        },
        src_schema: src,
        tgt_schema: tgt,
    };
    let source = FInstance::new()
        .with_rows("t", [(RowId(0), row(&[("a", Value::Int(1))]))])
        .unwrap();
    let (mut view, complement) = table_lens::get(&lens, &source, contexts(&p)).unwrap();
    view.row_mut(RowId(0))
        .unwrap()
        .insert("a2".into(), Value::Int(2));
    let updated = table_lens::put(&lens, &view, &complement, contexts(&p)).unwrap();
    assert_eq!(updated.row(RowId(0)).unwrap()["a"], Value::Int(2));
    assert_eq!(
        table_lens::get(&lens, &updated, contexts(&p)).unwrap().0,
        view
    );
}

#[test]
fn a_new_row_pulls_back_through_a_column_rename() {
    let p = protocol();
    let src = tables(&[("t", &[("a", "int")])], &[]);
    let tgt = tables(&[("t", &[("a2", "int")])], &[]);
    let lens = Lens {
        compiled: CompiledMigration {
            surviving_verts: ["t", "t.a2"].into_iter().map(Name::from).collect(),
            surviving_edges: [column("t", "a2")].into(),
            vertex_remap: HashMap::from([("t.a".into(), "t.a2".into())]),
            edge_remap: HashMap::from([(column("t", "a"), column("t", "a2"))]),
            ..CompiledMigration::default()
        },
        src_schema: src,
        tgt_schema: tgt,
    };
    let source = FInstance::new();
    let (mut view, complement) = table_lens::get(&lens, &source, contexts(&p)).unwrap();
    view.insert_row("t", RowId(0), row(&[("a2", Value::Int(7))]))
        .unwrap();
    let updated = table_lens::put(&lens, &view, &complement, contexts(&p)).unwrap();
    assert_eq!(updated.row(RowId(0)).unwrap()["a"], Value::Int(7));
}

#[test]
fn a_view_inserted_row_gets_no_dropped_column() {
    let p = protocol();
    let src = tables(&[("t", &[("a", "int"), ("hidden", "int")])], &[]);
    let tgt = tables(&[("t", &[("a", "int")])], &[]);
    let lens = Lens {
        compiled: CompiledMigration {
            surviving_verts: ["t", "t.a"].into_iter().map(Name::from).collect(),
            surviving_edges: [column("t", "a")].into(),
            ..CompiledMigration::default()
        },
        src_schema: src,
        tgt_schema: tgt,
    };
    let (mut view, complement) = table_lens::get(&lens, &FInstance::new(), contexts(&p)).unwrap();
    view.insert_row("t", RowId(0), row(&[("a", Value::Int(7))]))
        .unwrap();
    let updated = table_lens::put(&lens, &view, &complement, contexts(&p)).unwrap();
    assert_eq!(
        updated.row(RowId(0)).unwrap(),
        &row(&[("a", Value::Int(7))])
    );
    assert_eq!(
        table_lens::get(&lens, &updated, contexts(&p)).unwrap().0,
        view
    );
}

#[test]
fn editing_a_derived_table_cell_is_refused() {
    let (p, schema, mut lens, source) = one_column();
    lens.compiled
        .add_computed_field("t", "a", Expr::Lit(Literal::Int(0)));
    lens.src_schema = schema.clone();
    lens.tgt_schema = schema;
    let (mut view, complement) = table_lens::get(&lens, &source, contexts(&p)).unwrap();
    view.row_mut(RowId(0))
        .unwrap()
        .insert("a".into(), Value::Int(2));
    assert!(matches!(
        table_lens::put(&lens, &view, &complement, contexts(&p)),
        Err(LensError::PutDomain(error))
            if matches!(*error, PutDomainError::DerivedCellEdited {
                elem: ViewRef::Row(RowId(0)), ref field
            } if field == "a")
    ));
}

#[test]
fn editing_a_noninvertible_table_transform_is_refused() {
    let (p, _, mut lens, source) = one_column();
    lens.compiled
        .add_field_expr("t", "a", Expr::Lit(Literal::Int(0)));
    let (mut view, complement) = table_lens::get(&lens, &source, contexts(&p)).unwrap();
    view.row_mut(RowId(0))
        .unwrap()
        .insert("a".into(), Value::Int(2));
    assert!(matches!(
        table_lens::put(&lens, &view, &complement, contexts(&p)),
        Err(LensError::PutDomain(error))
            if matches!(*error, PutDomainError::TransformNotInvertible {
                elem: ViewRef::Row(RowId(0)), ref field
            } if field == "a")
    ));
}

#[test]
fn an_edit_through_an_invertible_table_transform_pulls_back() {
    let (p, _, mut lens, source) = one_column();
    lens.compiled.field_transforms.insert(
        "t".into(),
        vec![FieldTransform::ApplyExpr {
            key: "a".into(),
            expr: Expr::Builtin(
                BuiltinOp::Add,
                vec![Expr::Var("a".into()), Expr::Lit(Literal::Int(1))],
            ),
            inverse: Some(Expr::Builtin(
                BuiltinOp::Sub,
                vec![Expr::Var("a".into()), Expr::Lit(Literal::Int(1))],
            )),
            coercion_class: CoercionClass::Iso,
        }],
    );
    let (mut view, complement) = table_lens::get(&lens, &source, contexts(&p)).unwrap();
    assert_eq!(view.row(RowId(0)).unwrap()["a"], Value::Int(2));
    view.row_mut(RowId(0))
        .unwrap()
        .insert("a".into(), Value::Int(7));

    let updated = table_lens::put(&lens, &view, &complement, contexts(&p)).unwrap();
    assert_eq!(updated.row(RowId(0)).unwrap()["a"], Value::Int(6));
    assert_eq!(
        table_lens::get(&lens, &updated, contexts(&p)).unwrap().0,
        view
    );
}

#[test]
fn an_optional_transformed_cell_may_be_removed() {
    let (p, _, mut lens, source) = one_column();
    lens.compiled.field_transforms.insert(
        "t".into(),
        vec![FieldTransform::ApplyExpr {
            key: "a".into(),
            expr: Expr::Builtin(
                BuiltinOp::Add,
                vec![Expr::Var("a".into()), Expr::Lit(Literal::Int(1))],
            ),
            inverse: Some(Expr::Builtin(
                BuiltinOp::Sub,
                vec![Expr::Var("a".into()), Expr::Lit(Literal::Int(1))],
            )),
            coercion_class: CoercionClass::Iso,
        }],
    );
    let (mut view, complement) = table_lens::get(&lens, &source, contexts(&p)).unwrap();
    view.row_mut(RowId(0)).unwrap().remove("a");

    let updated = table_lens::put(&lens, &view, &complement, contexts(&p)).unwrap();
    assert!(!updated.row(RowId(0)).unwrap().contains_key("a"));
    assert_eq!(
        table_lens::get(&lens, &updated, contexts(&p)).unwrap().0,
        view
    );
}

#[test]
fn table_put_reports_false_and_refused_survival_predicates() {
    let (p, schema, mut lens, source) = one_column();
    lens.compiled.add_conditional_survival(
        "t",
        Expr::Builtin(
            BuiltinOp::Gt,
            vec![Expr::Var("a".into()), Expr::Lit(Literal::Int(0))],
        ),
    );
    lens.src_schema = schema.clone();
    lens.tgt_schema = schema;
    let (view, complement) = table_lens::get(&lens, &source, contexts(&p)).unwrap();

    let mut false_view = view.clone();
    false_view
        .row_mut(RowId(0))
        .unwrap()
        .insert("a".into(), Value::Int(-1));
    assert!(matches!(
        table_lens::put(&lens, &false_view, &complement, contexts(&p)),
        Err(LensError::PutDomain(error))
            if matches!(*error, PutDomainError::SurvivalPredicateFails {
                elem: ViewRef::Row(RowId(0))
            })
    ));

    let mut refused_view = view;
    refused_view
        .row_mut(RowId(0))
        .unwrap()
        .insert("a".into(), Value::Str("not-an-int".into()));
    assert!(matches!(
        table_lens::put(&lens, &refused_view, &complement, contexts(&p)),
        Err(LensError::PutDomain(error))
            if matches!(*error, PutDomainError::SurvivalPredicateRefused {
                elem: ViewRef::Row(RowId(0)), ..
            })
    ));
}

#[test]
fn requiredness_and_keys_are_the_only_cell_domain_constraints() {
    let p = required_protocol();
    let mut schema = SchemaBuilder::new(&p)
        .vertex("t", "record", None)
        .unwrap()
        .vertex("t.a", "int", None)
        .unwrap()
        .edge("t", "t.a", "prop", Some("a"))
        .unwrap()
        .constraint("t.a", "required", "true")
        .build()
        .unwrap();
    schema.declarations.keys.insert(
        "t".into(),
        vec![Key {
            id: "by-a".into(),
            parts: vec![KeyPart::Path(vec![column("t", "a")])],
            role: KeyRole::Primary,
            scope: KeyScope::Global,
        }],
    );
    let lens = identity(&schema);
    let (mut empty_view, empty_complement) =
        table_lens::get(&lens, &FInstance::new(), contexts(&p)).unwrap();
    empty_view
        .insert_row("t", RowId(0), HashMap::new())
        .unwrap();
    assert!(!table_lens::is_isomorphism(&lens, contexts(&p)));
    assert!(
        table_lens::obstruction_to_isomorphism(&lens, contexts(&p))
            .is_some_and(|detail| detail.contains("required-value constraint"))
    );
    assert!(matches!(
        raw_complement_free_put(&lens, &empty_view, contexts(&p)),
        Err(LensError::PutDomain(error))
            if matches!(*error, PutDomainError::MissingRequiredCell { .. })
    ));
    let missing = table_lens::check_put_domain(&lens, &empty_view, &empty_complement, contexts(&p))
        .unwrap_err();
    assert!(missing.iter().any(|error| matches!(
        error,
        PutDomainError::MissingRequiredCell {
            elem: ViewRef::Row(RowId(0)), field
        } if field == "a"
    )));

    let duplicate_source = FInstance::new()
        .with_rows(
            "t",
            [
                (RowId(0), row(&[("a", Value::Int(1))])),
                (RowId(1), row(&[("a", Value::Int(1))])),
            ],
        )
        .unwrap();
    let (duplicate_view, duplicate_complement) =
        table_lens::get(&lens, &duplicate_source, contexts(&p)).unwrap();
    let duplicate =
        table_lens::check_put_domain(&lens, &duplicate_view, &duplicate_complement, contexts(&p))
            .unwrap_err();
    assert!(duplicate.iter().any(|error| matches!(
        error,
        PutDomainError::DuplicateKey { table, elems, .. }
            if table.as_str() == "t" && elems.len() == 2
    )));
}

#[test]
fn table_classifier_ignores_validation_results_that_put_does_not_map() {
    let required = required_protocol();
    let schema = SchemaBuilder::new(&required)
        .vertex("t", "record", None)
        .unwrap()
        .constraint("t", "required", "true")
        .build()
        .unwrap();
    let lens = identity(&schema);
    let source = FInstance::new()
        .with_rows("t", [(RowId(0), HashMap::new())])
        .unwrap();
    let boundary = contexts(&required);
    let (view, _) = table_lens::get(&lens, &source, boundary).unwrap();
    assert!(table_lens::is_isomorphism(&lens, boundary));
    assert_eq!(
        raw_complement_free_put(&lens, &view, boundary).unwrap(),
        source
    );

    let cast = Edge {
        src: "film".into(),
        tgt: "actor".into(),
        kind: "prop".into(),
        name: Some("cast".into()),
    };
    let field_schema = SchemaBuilder::new(&required)
        .vertex("film", "record", None)
        .unwrap()
        .vertex("actor", "record", None)
        .unwrap()
        .edge("film", "actor", "prop", Some("cast"))
        .unwrap()
        .constraint("actor", "required", "true")
        .build()
        .unwrap();
    let field_lens = identity(&field_schema);
    let missing_field = FInstance::new()
        .with_rows("film", [(RowId(0), HashMap::new())])
        .unwrap();
    let (view, _) = table_lens::get(&field_lens, &missing_field, boundary).unwrap();
    assert!(!table_lens::is_isomorphism(&field_lens, boundary));
    assert!(matches!(
        raw_complement_free_put(&field_lens, &view, boundary),
        Err(LensError::PutDomain(error))
            if matches!(&*error, PutDomainError::MissingRequiredCell { field, .. }
                if field == cast.name.as_ref().unwrap().as_str())
    ));

    let protocol = protocol();
    let parent = fk("p", "t", "t");
    let mut position_schema = tables(&[("p", &[]), ("t", &[])], &[&parent]);
    position_schema.orderings.insert(parent.clone(), 0);
    position_schema.declarations.keys.insert(
        "t".into(),
        vec![Key {
            id: "by-position".into(),
            parts: vec![KeyPart::Position { within: parent }],
            role: KeyRole::Unique,
            scope: KeyScope::Global,
        }],
    );
    let position_lens = identity(&position_schema);
    let positioned = FInstance::new()
        .with_rows("t", [(RowId(0), HashMap::new())])
        .unwrap();
    let boundary = contexts(&protocol);
    let (view, _) = table_lens::get(&position_lens, &positioned, boundary).unwrap();
    assert!(table_lens::is_isomorphism(&position_lens, boundary));
    assert_eq!(
        raw_complement_free_put(&position_lens, &view, boundary).unwrap(),
        positioned
    );

    let mut empty_path_schema = tables(&[("t", &[])], &[]);
    empty_path_schema.declarations.keys.insert(
        "t".into(),
        vec![Key {
            id: "by-own-value".into(),
            parts: vec![KeyPart::Path(Vec::new())],
            role: KeyRole::Unique,
            scope: KeyScope::Global,
        }],
    );
    let empty_path_lens = identity(&empty_path_schema);
    let (view, _) = table_lens::get(&empty_path_lens, &positioned, boundary).unwrap();
    assert!(table_lens::is_isomorphism(&empty_path_lens, boundary));
    assert_eq!(
        raw_complement_free_put(&empty_path_lens, &view, boundary).unwrap(),
        positioned
    );
}

#[test]
fn a_primary_key_column_is_required() {
    let p = protocol();
    let mut schema = tables(&[("t", &[("a", "int")])], &[]);
    schema.declarations.keys.insert(
        "t".into(),
        vec![Key {
            id: "primary-a".into(),
            parts: vec![KeyPart::Path(vec![column("t", "a")])],
            role: KeyRole::Primary,
            scope: KeyScope::Global,
        }],
    );
    let lens = identity(&schema);
    let (mut view, complement) = table_lens::get(&lens, &FInstance::new(), contexts(&p)).unwrap();
    view.insert_row("t", RowId(0), HashMap::new()).unwrap();

    let violations =
        table_lens::check_put_domain(&lens, &view, &complement, contexts(&p)).unwrap_err();
    assert!(violations.iter().any(|error| matches!(
        error,
        PutDomainError::MissingRequiredCell {
            elem: ViewRef::Row(RowId(0)), field
        } if field == "a"
    )));
}

#[test]
fn duplicate_key_ties_break_on_column_and_canonical_value_bytes() {
    let p = protocol();
    let mut schema = tables(&[("t", &[("a", "int"), ("b", "string")])], &[]);
    schema.declarations.keys.insert(
        "t".into(),
        vec![
            Key {
                id: "by-b".into(),
                parts: vec![KeyPart::Path(vec![column("t", "b")])],
                role: KeyRole::Unique,
                scope: KeyScope::Global,
            },
            Key {
                id: "by-a".into(),
                parts: vec![KeyPart::Path(vec![column("t", "a")])],
                role: KeyRole::Unique,
                scope: KeyScope::Global,
            },
        ],
    );
    let lens = identity(&schema);
    let source = FInstance::new()
        .with_rows(
            "t",
            [
                (
                    RowId(1),
                    row(&[("a", Value::Int(1)), ("b", Value::Str("x".into()))]),
                ),
                (
                    RowId(0),
                    row(&[("a", Value::Int(1)), ("b", Value::Str("x".into()))]),
                ),
            ],
        )
        .unwrap();
    let (view, complement) = table_lens::get(&lens, &source, contexts(&p)).unwrap();
    assert!(!table_lens::is_isomorphism(&lens, contexts(&p)));
    assert!(
        table_lens::obstruction_to_isomorphism(&lens, contexts(&p))
            .is_some_and(|detail| detail.contains("duplicate key values"))
    );
    assert!(matches!(
        raw_complement_free_put(&lens, &view, contexts(&p)),
        Err(LensError::PutDomain(error))
            if matches!(*error, PutDomainError::DuplicateKey { .. })
    ));
    let violations =
        table_lens::check_put_domain(&lens, &view, &complement, contexts(&p)).unwrap_err();
    let duplicates: Vec<_> = violations
        .iter()
        .filter_map(|error| match error {
            PutDomainError::DuplicateKey { key, elems, .. } => Some((key, elems)),
            _ => None,
        })
        .collect();

    assert_eq!(duplicates.len(), 2);
    assert_eq!(duplicates[0].0[0].0, "a");
    assert_eq!(duplicates[1].0[0].0, "b");
    assert_eq!(
        duplicates[0].1,
        &vec![ViewRef::Row(RowId(0)), ViewRef::Row(RowId(1))]
    );
}

#[test]
fn an_undeclared_optional_cell_may_remain_absent() {
    let (p, _, lens, _) = one_column();
    let (mut view, complement) = table_lens::get(&lens, &FInstance::new(), contexts(&p)).unwrap();
    view.insert_row("t", RowId(0), HashMap::new()).unwrap();
    assert!(table_lens::check_put_domain(&lens, &view, &complement, contexts(&p)).is_ok());
    let source = table_lens::put(&lens, &view, &complement, contexts(&p)).unwrap();
    assert!(source.row(RowId(0)).unwrap().is_empty());
    assert_eq!(
        table_lens::get(&lens, &source, contexts(&p)).unwrap().0,
        view
    );
}

#[test]
fn table_get_refuses_an_undeclared_row_key() {
    let (protocol, _schema, lens, _) = one_column();
    let source = FInstance::new()
        .with_rows(
            "t",
            [(RowId(0), row(&[("a", Value::Int(1)), ("x", Value::Int(2))]))],
        )
        .unwrap();
    assert!(table_lens::get(&lens, &source, contexts(&protocol)).is_err());
}

#[test]
fn an_optional_cell_may_be_removed() {
    let (p, _, lens, source) = one_column();
    let (mut view, complement) = table_lens::get(&lens, &source, contexts(&p)).unwrap();
    view.row_mut(RowId(0)).unwrap().remove("a");

    let updated = table_lens::put(&lens, &view, &complement, contexts(&p)).unwrap();
    assert!(!updated.row(RowId(0)).unwrap().contains_key("a"));
    assert_eq!(
        table_lens::get(&lens, &updated, contexts(&p)).unwrap().0,
        view
    );
}

#[test]
fn an_optional_renamed_cell_may_be_removed() {
    let p = protocol();
    let src = tables(&[("t", &[("a", "int")])], &[]);
    let tgt = tables(&[("t", &[("a2", "int")])], &[]);
    let lens = Lens {
        compiled: CompiledMigration {
            surviving_verts: ["t", "t.a2"].into_iter().map(Name::from).collect(),
            surviving_edges: [column("t", "a2")].into(),
            vertex_remap: HashMap::from([("t.a".into(), "t.a2".into())]),
            edge_remap: HashMap::from([(column("t", "a"), column("t", "a2"))]),
            ..CompiledMigration::default()
        },
        src_schema: src,
        tgt_schema: tgt,
    };
    let source = FInstance::new()
        .with_rows("t", [(RowId(0), row(&[("a", Value::Int(1))]))])
        .unwrap();
    let (mut view, complement) = table_lens::get(&lens, &source, contexts(&p)).unwrap();
    view.row_mut(RowId(0)).unwrap().remove("a2");

    let updated = table_lens::put(&lens, &view, &complement, contexts(&p)).unwrap();
    assert!(!updated.row(RowId(0)).unwrap().contains_key("a"));
    assert_eq!(
        table_lens::get(&lens, &updated, contexts(&p)).unwrap().0,
        view
    );
}

#[test]
fn dropped_rows_and_tables_are_complement_residue() {
    let p = protocol();
    let src = tables(&[("keep", &[("a", "int")]), ("hide", &[("a", "int")])], &[]);
    let tgt = tables(&[("keep", &[("a", "int")])], &[]);
    let lens = Lens {
        compiled: CompiledMigration {
            surviving_verts: ["keep", "keep.a"].into_iter().map(Name::from).collect(),
            surviving_edges: [column("keep", "a")].into(),
            ..CompiledMigration::default()
        },
        src_schema: src,
        tgt_schema: tgt,
    };
    let source = FInstance::new()
        .with_rows("keep", [(RowId(0), row(&[("a", Value::Int(1))]))])
        .unwrap()
        .with_rows("hide", [(RowId(1), row(&[("a", Value::Int(2))]))])
        .unwrap();
    let (view, complement) = table_lens::get(&lens, &source, contexts(&p)).unwrap();
    assert_eq!(
        table_lens::put(&lens, &view, &complement, contexts(&p)).unwrap(),
        source
    );
}

#[test]
fn foreign_key_pairs_round_trip_exactly() {
    let p = protocol();
    let relation = fk("post", "user", "author");
    let schema = tables(
        &[("post", &[("n", "int")]), ("user", &[("n", "int")])],
        &[&relation],
    );
    let lens = identity(&schema);
    let source = FInstance::new()
        .with_rows("user", [(RowId(0), row(&[("n", Value::Int(1))]))])
        .unwrap()
        .with_rows("post", [(RowId(1), row(&[("n", Value::Int(2))]))])
        .unwrap()
        .with_foreign_key(relation, [(RowId(1), RowId(0))])
        .unwrap();
    let (view, complement) = table_lens::get(&lens, &source, contexts(&p)).unwrap();
    assert_eq!(
        table_lens::put(&lens, &view, &complement, contexts(&p)).unwrap(),
        source
    );
}

#[test]
fn a_new_pair_with_two_source_preimages_is_unrepresentable() {
    let p = protocol();
    let x = fk("a", "a", "x");
    let y = fk("a", "a", "y");
    let z = fk("a", "a", "z");
    let src = tables(&[("a", &[])], &[&x, &y]);
    let tgt = tables(&[("a", &[])], &[&z]);
    let mut migration = identity(&tgt).compiled;
    migration.edge_remap.insert(x, z.clone());
    migration.edge_remap.insert(y, z.clone());
    let lens = Lens {
        compiled: migration,
        src_schema: src,
        tgt_schema: tgt,
    };
    let source = FInstance::new()
        .with_rows(
            "a",
            [(RowId(0), HashMap::new()), (RowId(1), HashMap::new())],
        )
        .unwrap();
    let (mut view, complement) = table_lens::get(&lens, &source, contexts(&p)).unwrap();
    view.insert_foreign_key(z, RowId(0), RowId(1)).unwrap();
    assert!(matches!(
        table_lens::put(&lens, &view, &complement, contexts(&p)),
        Err(LensError::PutDomain(error))
            if matches!(*error, PutDomainError::UnrepresentableViewEdge { .. })
    ));
}

#[test]
fn recorded_pairs_restore_every_original_carrier() {
    let p = protocol();
    let x = fk("a", "a", "x");
    let y = fk("a", "a", "y");
    let z = fk("a", "a", "z");
    let src = tables(&[("a", &[])], &[&x, &y]);
    let tgt = tables(&[("a", &[])], &[&z]);
    let mut migration = identity(&tgt).compiled;
    migration.edge_remap.insert(x.clone(), z.clone());
    migration.edge_remap.insert(y.clone(), z);
    let lens = Lens {
        compiled: migration,
        src_schema: src,
        tgt_schema: tgt,
    };
    let source = FInstance::new()
        .with_rows(
            "a",
            [(RowId(0), HashMap::new()), (RowId(1), HashMap::new())],
        )
        .unwrap()
        .with_foreign_key(x.clone(), [(RowId(0), RowId(1))])
        .unwrap()
        .with_foreign_key(y.clone(), [(RowId(0), RowId(1))])
        .unwrap();
    let (view, complement) = table_lens::get(&lens, &source, contexts(&p)).unwrap();
    let restored = table_lens::put(&lens, &view, &complement, contexts(&p)).unwrap();
    assert!(restored.contains_pair(&x, RowId(0), RowId(1)));
    assert!(restored.contains_pair(&y, RowId(0), RowId(1)));
}

#[test]
fn contracted_pairs_are_read_only_and_round_trip() {
    let p = protocol();
    let am = fk("a", "m", "am");
    let mb = fk("m", "b", "mb");
    let ab = fk("a", "b", "ab");
    let src = tables(&[("a", &[]), ("m", &[]), ("b", &[])], &[&am, &mb]);
    let tgt = tables(&[("a", &[]), ("b", &[])], &[&ab]);
    let lens = Lens {
        compiled: identity(&tgt).compiled,
        src_schema: src,
        tgt_schema: tgt,
    };
    let source = FInstance::new()
        .with_rows("a", [(RowId(0), HashMap::new())])
        .unwrap()
        .with_rows("m", [(RowId(1), HashMap::new())])
        .unwrap()
        .with_rows("b", [(RowId(2), HashMap::new())])
        .unwrap()
        .with_foreign_key(am, [(RowId(0), RowId(1))])
        .unwrap()
        .with_foreign_key(mb, [(RowId(1), RowId(2))])
        .unwrap();
    let (mut view, complement) = table_lens::get(&lens, &source, contexts(&p)).unwrap();
    assert_eq!(
        table_lens::put(&lens, &view, &complement, contexts(&p)).unwrap(),
        source
    );
    view.remove_foreign_key(&ab, RowId(0), RowId(2));
    assert!(matches!(
        table_lens::put(&lens, &view, &complement, contexts(&p)),
        Err(LensError::PutDomain(error))
            if matches!(*error, PutDomainError::SynthesizedEdited {
                elem: ViewRef::Pair { .. }
            })
    ));
}

#[test]
fn a_two_origin_pair_is_read_only_and_restores_its_direct_carrier() {
    let p = protocol();
    let am = fk("a", "m", "am");
    let mb = fk("m", "b", "mb");
    let ab = fk("a", "b", "ab");
    let src = tables(&[("a", &[]), ("m", &[]), ("b", &[])], &[&am, &mb, &ab]);
    let tgt = tables(&[("a", &[]), ("b", &[])], &[&ab]);
    let lens = Lens {
        compiled: identity(&tgt).compiled,
        src_schema: src,
        tgt_schema: tgt,
    };
    let source = FInstance::new()
        .with_rows("a", [(RowId(0), HashMap::new())])
        .unwrap()
        .with_rows("m", [(RowId(1), HashMap::new())])
        .unwrap()
        .with_rows("b", [(RowId(2), HashMap::new())])
        .unwrap()
        .with_foreign_key(am, [(RowId(0), RowId(1))])
        .unwrap()
        .with_foreign_key(mb, [(RowId(1), RowId(2))])
        .unwrap()
        .with_foreign_key(ab.clone(), [(RowId(0), RowId(2))])
        .unwrap();
    let (mut view, complement) = table_lens::get(&lens, &source, contexts(&p)).unwrap();
    assert!(
        complement
            .synthesized_fk_pairs
            .contains(&(ab.clone(), RowId(0), RowId(2)))
    );
    assert_eq!(
        table_lens::put(&lens, &view, &complement, contexts(&p)).unwrap(),
        source
    );

    view.remove_foreign_key(&ab, RowId(0), RowId(2));
    assert!(matches!(
        table_lens::put(&lens, &view, &complement, contexts(&p)),
        Err(LensError::PutDomain(error))
            if matches!(*error, PutDomainError::SynthesizedEdited {
                elem: ViewRef::Pair { .. }
            })
    ));
}

#[test]
fn inserting_a_contracted_pair_is_refused() {
    let p = protocol();
    let am = fk("a", "m", "am");
    let mb = fk("m", "b", "mb");
    let ab = fk("a", "b", "ab");
    let src = tables(&[("a", &[]), ("m", &[]), ("b", &[])], &[&am, &mb]);
    let tgt = tables(&[("a", &[]), ("b", &[])], &[&ab]);
    let lens = Lens {
        compiled: identity(&tgt).compiled,
        src_schema: src,
        tgt_schema: tgt,
    };
    let source = FInstance::new()
        .with_rows("a", [(RowId(0), HashMap::new())])
        .unwrap()
        .with_rows("b", [(RowId(2), HashMap::new())])
        .unwrap();
    let (mut view, complement) = table_lens::get(&lens, &source, contexts(&p)).unwrap();
    view.insert_foreign_key(ab, RowId(0), RowId(2)).unwrap();

    assert!(matches!(
        table_lens::put(&lens, &view, &complement, contexts(&p)),
        Err(LensError::PutDomain(error))
            if matches!(*error, PutDomainError::ContractedEdgeInserted {
                elem: ViewRef::Pair { .. }
            })
    ));
}

#[test]
fn a_pair_with_no_source_preimage_is_unrepresentable() {
    let p = protocol();
    let ab = fk("a", "b", "ab");
    let source_schema = tables(&[("a", &[]), ("b", &[])], &[]);
    let target_schema = tables(&[("a", &[]), ("b", &[])], &[&ab]);
    let lens = Lens {
        compiled: identity(&target_schema).compiled,
        src_schema: source_schema,
        tgt_schema: target_schema,
    };
    let source = FInstance::new()
        .with_rows("a", [(RowId(0), HashMap::new())])
        .unwrap()
        .with_rows("b", [(RowId(1), HashMap::new())])
        .unwrap();
    let (mut view, complement) = table_lens::get(&lens, &source, contexts(&p)).unwrap();
    view.insert_foreign_key(ab, RowId(0), RowId(1)).unwrap();

    assert!(matches!(
        table_lens::put(&lens, &view, &complement, contexts(&p)),
        Err(LensError::PutDomain(error))
            if matches!(*error, PutDomainError::UnrepresentableViewEdge {
                elem: ViewRef::Pair { .. }
            })
    ));
}

#[test]
fn a_row_in_a_merged_table_is_an_ambiguous_reverse_anchor() {
    let p = protocol();
    let src = tables(&[("a", &[("n", "int")]), ("b", &[("n", "int")])], &[]);
    let tgt = tables(&[("t", &[("n", "int")])], &[]);
    let mut migration = identity(&tgt).compiled;
    for table in ["a", "b"] {
        migration.vertex_remap.insert(table.into(), "t".into());
        migration
            .vertex_remap
            .insert(format!("{table}.n").into(), "t.n".into());
        migration
            .edge_remap
            .insert(column(table, "n"), column("t", "n"));
    }
    let lens = Lens {
        compiled: migration,
        src_schema: src,
        tgt_schema: tgt,
    };
    let (mut view, complement) = table_lens::get(&lens, &FInstance::new(), contexts(&p)).unwrap();
    view.insert_row("t", RowId(0), row(&[("n", Value::Int(1))]))
        .unwrap();
    assert!(matches!(
        table_lens::put(&lens, &view, &complement, contexts(&p)),
        Err(LensError::PutDomain(error))
            if matches!(*error, PutDomainError::AmbiguousReverseAnchor {
                elem: ViewRef::Row(RowId(0)), ref candidates
            } if candidates.len() == 2)
    ));
}

#[test]
fn a_view_moved_row_keeps_its_new_table() {
    let p = protocol();
    let src = tables(
        &[
            ("a", &[("n", "int")]),
            ("b", &[("n", "int")]),
            ("c", &[("n", "int")]),
        ],
        &[],
    );
    let tgt = tables(&[("t", &[("n", "int")]), ("u", &[("n", "int")])], &[]);
    let mut migration = identity(&tgt).compiled;
    for (from, to) in [("a", "t"), ("b", "t"), ("c", "u")] {
        migration.vertex_remap.insert(from.into(), to.into());
        migration
            .vertex_remap
            .insert(format!("{from}.n").into(), format!("{to}.n").into());
        migration
            .edge_remap
            .insert(column(from, "n"), column(to, "n"));
    }
    let lens = Lens {
        compiled: migration,
        src_schema: src,
        tgt_schema: tgt,
    };
    let source = FInstance::new()
        .with_rows("a", [(RowId(0), row(&[("n", Value::Int(1))]))])
        .unwrap();
    let (mut view, complement) = table_lens::get(&lens, &source, contexts(&p)).unwrap();
    view.move_row(RowId(0), "u").unwrap();
    let restored = table_lens::put(&lens, &view, &complement, contexts(&p)).unwrap();
    assert_eq!(restored.table_of(RowId(0)).map(Name::as_str), Some("c"));
}

#[test]
fn row_deletion_cascades_into_hidden_pairs() {
    let p = protocol();
    let relation = fk("post", "user", "author");
    let src = tables(
        &[("post", &[("n", "int")]), ("user", &[("n", "int")])],
        &[&relation],
    );
    let tgt = tables(&[("post", &[("n", "int")]), ("user", &[("n", "int")])], &[]);
    let lens = Lens {
        compiled: CompiledMigration {
            surviving_verts: src.vertices.keys().cloned().collect(),
            surviving_edges: [column("post", "n"), column("user", "n")].into(),
            ..CompiledMigration::default()
        },
        src_schema: src,
        tgt_schema: tgt,
    };
    let source = FInstance::new()
        .with_rows("user", [(RowId(0), row(&[("n", Value::Int(1))]))])
        .unwrap()
        .with_rows("post", [(RowId(1), row(&[("n", Value::Int(2))]))])
        .unwrap()
        .with_foreign_key(relation.clone(), [(RowId(1), RowId(0))])
        .unwrap();
    let (mut view, complement) = table_lens::get(&lens, &source, contexts(&p)).unwrap();
    view.remove_row(RowId(1));
    let updated = table_lens::put(&lens, &view, &complement, contexts(&p)).unwrap();
    assert!(updated.row(RowId(1)).is_none());
    assert!(updated.pairs(&relation).is_empty());
}

#[test]
fn a_reinserted_row_does_not_regain_its_cascaded_hidden_foreign_key() {
    let p = protocol();
    let relation = fk("keep", "hide", "link");
    let src = tables(&[("keep", &[]), ("hide", &[])], &[&relation]);
    let tgt = tables(&[("keep", &[])], &[]);
    let lens = Lens {
        compiled: CompiledMigration {
            surviving_verts: std::iter::once(Name::from("keep")).collect(),
            ..CompiledMigration::default()
        },
        src_schema: src,
        tgt_schema: tgt,
    };
    let source = FInstance::new()
        .with_rows("keep", [(RowId(0), HashMap::new())])
        .unwrap()
        .with_rows("hide", [(RowId(1), HashMap::new())])
        .unwrap()
        .with_foreign_key(relation.clone(), [(RowId(0), RowId(1))])
        .unwrap();
    let (mut view, complement) = table_lens::get(&lens, &source, contexts(&p)).unwrap();
    view.remove_row(RowId(0));
    let deleted = table_lens::put(&lens, &view, &complement, contexts(&p)).unwrap();
    let (mut next_view, next_complement) = table_lens::get(&lens, &deleted, contexts(&p)).unwrap();
    next_view
        .insert_row("keep", RowId(0), HashMap::new())
        .unwrap();
    let reinserted = table_lens::put(&lens, &next_view, &next_complement, contexts(&p)).unwrap();
    assert!(reinserted.pairs(&relation).is_empty());
}

#[test]
fn fresh_row_ids_avoid_view_and_residue() {
    let view = FInstance::new()
        .with_rows("t", [(RowId(7), HashMap::new())])
        .unwrap();
    let mut complement = TableComplement::empty();
    complement.id_floor = 5;
    complement.dropped_rows.insert(
        RowId(5),
        panproto_inst::DroppedRow {
            table: "t".into(),
            row: HashMap::new(),
        },
    );
    assert_eq!(
        table_lens::fresh_view_row_id(&view, &complement).unwrap(),
        RowId(8)
    );
}

#[test]
fn a_view_row_cannot_reuse_a_hidden_row_id() {
    let p = protocol();
    let src = tables(&[("keep", &[("a", "int")]), ("hide", &[("a", "int")])], &[]);
    let tgt = tables(&[("keep", &[("a", "int")])], &[]);
    let lens = Lens {
        compiled: CompiledMigration {
            surviving_verts: ["keep", "keep.a"].into_iter().map(Name::from).collect(),
            surviving_edges: [column("keep", "a")].into(),
            ..CompiledMigration::default()
        },
        src_schema: src,
        tgt_schema: tgt,
    };
    let source = FInstance::new()
        .with_rows("hide", [(RowId(3), row(&[("a", Value::Int(2))]))])
        .unwrap();
    let (mut view, complement) = table_lens::get(&lens, &source, contexts(&p)).unwrap();
    view.insert_row("keep", RowId(3), row(&[("a", Value::Int(4))]))
        .unwrap();
    assert!(matches!(
        table_lens::put(&lens, &view, &complement, contexts(&p)),
        Err(LensError::PutDomain(error))
            if matches!(*error, panproto_inst::PutDomainError::IdCollision { .. })
    ));
}

#[test]
fn table_complements_form_an_idempotent_partial_monoid() {
    let (p, _, lens, source) = one_column();
    let mut left = table_lens::get(&lens, &source, contexts(&p)).unwrap().1;
    left.dropped_cells
        .insert((RowId(0), "a".into()), Value::Int(1));
    let same = left.compose(&left).unwrap();
    assert_eq!(same.dropped_cells, left.dropped_cells);
    let mut conflict = left.clone();
    conflict.dropped_cells.clear();
    conflict
        .dropped_cells
        .insert((RowId(0), "a".into()), Value::Int(2));
    assert!(left.compose(&conflict).is_err());
}

#[test]
fn table_complement_identity_verifies_and_composition_is_commutative() {
    let (p, _, lens, source) = one_column();
    let (_, base) = table_lens::get(&lens, &source, contexts(&p)).unwrap();
    assert!(
        base.verify(&lens, panproto_inst::IdentityPolicy::Strict)
            .is_ok()
    );
    let mut left = base.clone();
    let mut right = base;
    left.dropped_cells
        .insert((RowId(10), "a".into()), Value::Int(1));
    right
        .dropped_cells
        .insert((RowId(11), "a".into()), Value::Int(2));
    assert_eq!(
        left.compose(&right).unwrap().dropped_cells,
        right.compose(&left).unwrap().dropped_cells
    );
}

#[test]
fn table_compose_follows_the_graph_fusion_rule() {
    let (p, schema, first, _) = one_column();
    let second = identity(&schema);
    assert!(matches!(
        table_lens::compose(&first, &second, &p, &p, &p).unwrap(),
        GraphComposite::Compiled(_)
    ));
}

#[test]
fn a_table_stage_whose_survival_rules_disagree_is_not_fused() {
    let p = protocol();
    let source = tables(&[("a", &[]), ("b", &[])], &[]);
    let middle = tables(&[("c", &[]), ("b", &[])], &[]);
    let target = tables(&[("b", &[])], &[]);
    let first = Lens {
        compiled: CompiledMigration {
            surviving_verts: ["c", "b"].into_iter().map(Name::from).collect(),
            vertex_remap: HashMap::from([("a".into(), "c".into()), ("b".into(), "b".into())]),
            ..CompiledMigration::default()
        },
        src_schema: source,
        tgt_schema: middle.clone(),
    };
    let second = Lens {
        compiled: CompiledMigration {
            surviving_verts: std::iter::once(Name::from("b")).collect(),
            vertex_remap: HashMap::from([("c".into(), "b".into())]),
            ..CompiledMigration::default()
        },
        src_schema: middle,
        tgt_schema: target,
    };

    assert!(matches!(
        table_lens::compose(&first, &second, &p, &p, &p).unwrap(),
        GraphComposite::Sequential(_)
    ));
}

#[test]
fn compiled_and_sequential_table_refusals_agree() {
    let (p, schema, mut first, source) = one_column();
    first.compiled.add_conditional_survival(
        "t",
        Expr::Builtin(
            BuiltinOp::Gt,
            vec![Expr::Var("a".into()), Expr::Lit(Literal::Int(0))],
        ),
    );
    let second = identity(&schema);
    let GraphComposite::Compiled(compiled) =
        table_lens::compose(&first, &second, &p, &p, &p).unwrap()
    else {
        panic!("predicate followed by identity should fuse");
    };
    let (mut compiled_view, compiled_complement) =
        table_lens::get(&compiled, &source, contexts(&p)).unwrap();
    let (middle, first_complement) = table_lens::get(&first, &source, contexts(&p)).unwrap();
    let (mut sequential_view, second_complement) =
        table_lens::get(&second, &middle, contexts(&p)).unwrap();
    compiled_view
        .row_mut(RowId(0))
        .unwrap()
        .insert("a".into(), Value::Int(-1));
    sequential_view
        .row_mut(RowId(0))
        .unwrap()
        .insert("a".into(), Value::Int(-1));

    let compiled_error = table_lens::put(
        &compiled,
        &compiled_view,
        &compiled_complement,
        contexts(&p),
    )
    .unwrap_err();
    let middle =
        table_lens::put(&second, &sequential_view, &second_complement, contexts(&p)).unwrap();
    let sequential_error =
        table_lens::put(&first, &middle, &first_complement, contexts(&p)).unwrap_err();
    assert_eq!(compiled_error.to_string(), sequential_error.to_string());
}

#[test]
fn table_shape_executes_the_generic_law() {
    let (p, _, lens, source) = one_column();
    let context = TableContext {
        src: p.clone(),
        tgt: p,
    };
    let (view, complement) = TableShape::get(&lens, &source, &context).unwrap();
    assert_eq!(
        TableShape::put(&lens, &view, &complement, &context).unwrap(),
        source
    );
    assert!(TableShape::equivalent(
        &source,
        &SynthIndex::default(),
        &source,
        &SynthIndex::default()
    ));
}

#[test]
fn a_table_path_round_trips_and_checks_context_count() {
    let (p, _, lens, source) = one_column();
    let path = LensPath::single(lens);
    assert!(matches!(
        path.table_normal_form(std::slice::from_ref(&p)),
        Err(LensError::PathContextMismatch {
            expected: 2,
            found: 1
        })
    ));
    let (view, complement) = path.get_table(&source, &[p.clone(), p.clone()]).unwrap();
    assert_eq!(
        path.put_table(&view, &complement, &[p.clone(), p]).unwrap(),
        source
    );
}

#[test]
fn the_functor_complement_decoder_accepts_table_complements() {
    let bytes = serde_json::to_vec(&TableComplement::empty()).unwrap();
    let decoded = decode_complement(&bytes, Encoding::Json, ComplementShape::Functor).unwrap();
    assert!(matches!(decoded, AnyComplement::Functor(_)));
    assert!(matches!(
        decode_complement(&bytes, Encoding::Json, ComplementShape::Graph),
        Err(IdentityError::Mismatch {
            field: IdentityField::Format,
            ..
        })
    ));
}

#[test]
fn table_synth_keys_use_the_pair_embedding() {
    let edge = fk("a", "b", "to");
    let mut complement = TableComplement::empty();
    complement
        .synthesized_fk_pairs
        .insert((edge.clone(), RowId(1), RowId(2)));
    let embedding = TableEmbedding {
        pairs: BTreeMap::from([((edge, RowId(1), RowId(2)), EdgeId(9))]),
        next_edge: 10,
    };
    assert!(
        complement
            .synth_index(&embedding)
            .edges
            .contains_key(&EdgeId(9))
    );
}

fn contraction_case(chain_count: usize) -> (Protocol, Lens, FInstance) {
    let protocol = protocol();
    let am = fk("a", "m", "am");
    let mb = fk("m", "b", "mb");
    let ab = fk("a", "b", "ab");
    let source_schema = tables(&[("a", &[]), ("m", &[]), ("b", &[])], &[&am, &mb]);
    let target_schema = tables(&[("a", &[]), ("b", &[])], &[&ab]);
    let lens = Lens {
        compiled: identity(&target_schema).compiled,
        src_schema: source_schema,
        tgt_schema: target_schema,
    };
    let rows = |offset: u32| {
        (0..chain_count).map(move |index| {
            (
                RowId(u32::try_from(index).expect("small generated index") * 3 + offset),
                HashMap::new(),
            )
        })
    };
    let pairs = |source_offset: u32, target_offset: u32| {
        (0..chain_count).map(move |index| {
            let base = u32::try_from(index).expect("small generated index") * 3;
            (RowId(base + source_offset), RowId(base + target_offset))
        })
    };
    let source = FInstance::new()
        .with_rows("a", rows(0))
        .unwrap()
        .with_rows("m", rows(1))
        .unwrap()
        .with_rows("b", rows(2))
        .unwrap()
        .with_foreign_key(am, pairs(0, 1))
        .unwrap()
        .with_foreign_key(mb, pairs(1, 2))
        .unwrap();
    (protocol, lens, source)
}

proptest! {
    #[test]
    fn identity_round_trips_integer_cells(value in any::<i64>()) {
        let (p, _, lens, _) = one_column();
        let source = FInstance::new()
            .with_rows("t", [(RowId(0), row(&[("a", Value::Int(value))]))])
            .unwrap();
        let (view, complement) = table_lens::get(&lens, &source, contexts(&p)).unwrap();
        let restored = table_lens::put(&lens, &view, &complement, contexts(&p)).unwrap();
        prop_assert_eq!(restored, source);
    }

    #[test]
    fn complement_composition_is_associative_on_disjoint_cells(
        a in any::<i64>(), b in any::<i64>(), c in any::<i64>()
    ) {
        let (p, _, lens, source) = one_column();
        let identity = table_lens::get(&lens, &source, contexts(&p))
            .unwrap()
            .1
            .identity;
        let make = |id, value| {
            let mut complement = TableComplement::empty();
            complement.identity.clone_from(&identity);
            complement.dropped_cells.insert((RowId(id), "a".into()), Value::Int(value));
            complement
        };
        let (x, y, z) = (make(0, a), make(1, b), make(2, c));
        let left = x.compose(&y).unwrap().compose(&z).unwrap();
        let right = x.compose(&y.compose(&z).unwrap()).unwrap();
        prop_assert_eq!(left.dropped_cells, right.dropped_cells);
    }

    #[test]
    fn compiled_and_sequential_table_composites_agree(
        values in prop::collection::vec(any::<i16>(), 0..12),
        edits in prop::collection::vec(any::<i16>(), 0..12),
    ) {
        let p = protocol();
        let schema = tables(&[("t", &[("a", "int")])], &[]);
        let first = identity(&schema);
        let second = identity(&schema);
        let compiled = match table_lens::compose(&first, &second, &p, &p, &p)
            .expect("compose")
        {
            GraphComposite::Compiled(compiled) => compiled,
            GraphComposite::Sequential(_) => {
                return Err(TestCaseError::fail("identity table lenses did not fuse"));
            }
        };
        let rows = values.iter().enumerate().map(|(index, value)| {
            (
                RowId(u32::try_from(index).expect("small generated index")),
                row(&[("a", Value::Int(i64::from(*value)))]),
            )
        });
        let source = FInstance::new().with_rows("t", rows).expect("generated rows");
        let (mut compiled_view, compiled_complement) =
            table_lens::get(&compiled, &source, contexts(&p)).expect("compiled get");
        let (middle, first_complement) =
            table_lens::get(&first, &source, contexts(&p)).expect("first get");
        let (mut sequential_view, second_complement) =
            table_lens::get(&second, &middle, contexts(&p)).expect("second get");
        prop_assert_eq!(&compiled_view, &sequential_view);
        for (index, value) in edits.iter().take(values.len()).enumerate() {
            let id = RowId(u32::try_from(index).expect("small generated index"));
            compiled_view
                .row_mut(id)
                .expect("compiled row")
                .insert("a".into(), Value::Int(i64::from(*value)));
            sequential_view
                .row_mut(id)
                .expect("sequential row")
                .insert("a".into(), Value::Int(i64::from(*value)));
        }
        let compiled_source = table_lens::put(
            &compiled,
            &compiled_view,
            &compiled_complement,
            contexts(&p),
        )
        .expect("compiled put");
        let middle = table_lens::put(
            &second,
            &sequential_view,
            &second_complement,
            contexts(&p),
        )
        .expect("second put");
        let sequential_source =
            table_lens::put(&first, &middle, &first_complement, contexts(&p))
                .expect("first put");
        prop_assert_eq!(compiled_source, sequential_source);
    }

    #[test]
    fn graph_and_table_composition_follow_the_identity_table(
        values in prop::collection::vec(any::<i16>(), 0..12),
    ) {
        let protocol = protocol();
        let schema = tables(&[("t", &[("a", "int")])], &[]);
        let lens = identity(&schema);
        let left = table_lens::compose(&identity(&schema), &lens, &protocol, &protocol, &protocol)
            .expect("left identity composition");
        let right = table_lens::compose(&lens, &identity(&schema), &protocol, &protocol, &protocol)
            .expect("right identity composition");
        let GraphComposite::Compiled(left) = left else {
            return Err(TestCaseError::fail("left table identity did not fuse"));
        };
        let GraphComposite::Compiled(right) = right else {
            return Err(TestCaseError::fail("right table identity did not fuse"));
        };
        let source = FInstance::new()
            .with_rows(
                "t",
                values.iter().enumerate().map(|(index, value)| {
                    (
                        RowId(u32::try_from(index).expect("small generated index")),
                        row(&[("a", Value::Int(i64::from(*value)))]),
                    )
                }),
            )
            .expect("generated rows");
        for composite in [&*left, &*right] {
            let (view, complement) = table_lens::get(composite, &source, contexts(&protocol))
                .expect("identity get");
            prop_assert_eq!(&view, &source);
            prop_assert_eq!(
                table_lens::put(composite, &view, &complement, contexts(&protocol))
                    .expect("identity put"),
                source.clone()
            );
        }
    }

    #[test]
    fn table_normal_form_agrees_with_the_folds_around_synthesizing_stages(
        chain_count in 0_usize..8,
    ) {
        let (protocol, contraction, source) = contraction_case(chain_count);
        let target_schema = contraction.tgt_schema.clone();
        let path = LensPath::single(contraction)
            .then(&LensPath::single(identity(&target_schema)))
            .expect("first path boundary")
            .then(&LensPath::single(identity(&target_schema)))
            .expect("second path boundary");
        let protocols = [
            protocol.clone(),
            protocol.clone(),
            protocol.clone(),
            protocol,
        ];

        let mut atomic_view = source.clone();
        let mut atomic_complements = Vec::new();
        for (index, stage) in path.stages().iter().enumerate() {
            let (next, complement) = table_lens::get(
                stage,
                &atomic_view,
                BoundaryProtocols {
                    src: &protocols[index],
                    tgt: &protocols[index + 1],
                },
            )
            .expect("atomic get");
            atomic_view = next;
            atomic_complements.push(complement);
        }
        let (normal_view, normal_complement) = path
            .get_table(&source, &protocols)
            .expect("normal-form get");
        prop_assert_eq!(&normal_view, &atomic_view);
        if chain_count > 0 {
            prop_assert!(
                normal_complement
                    .entries()
                    .iter()
                    .any(|entry| !entry.complement.synthesized_fk_pairs.is_empty())
            );
        }

        let mut atomic_source = atomic_view;
        for index in (0..path.stages().len()).rev() {
            atomic_source = table_lens::put(
                &path.stages()[index],
                &atomic_source,
                &atomic_complements[index],
                BoundaryProtocols {
                    src: &protocols[index],
                    tgt: &protocols[index + 1],
                },
            )
            .expect("atomic put");
        }
        let normal_source = path
            .put_table(&normal_view, &normal_complement, &protocols)
            .expect("normal-form put");
        prop_assert_eq!(&normal_source, &atomic_source);
        prop_assert_eq!(normal_source, source);
    }

    #[test]
    fn every_fusion_preserves_sequential_behaviour_on_tables(
        values in prop::collection::vec(any::<i16>(), 0..12),
    ) {
        let protocol = protocol();
        let schema = tables(&[("t", &[("a", "int")])], &[]);
        let source = FInstance::new()
            .with_rows(
                "t",
                values.iter().enumerate().map(|(index, value)| {
                    (
                        RowId(u32::try_from(index).expect("small generated index")),
                        row(&[("a", Value::Int(i64::from(*value)))]),
                    )
                }),
            )
            .expect("generated rows");
        let mut filtered = identity(&schema);
        filtered.compiled.add_conditional_survival(
            "t",
            Expr::Builtin(
                BuiltinOp::Gte,
                vec![Expr::Var("a".into()), Expr::Lit(Literal::Int(0))],
            ),
        );
        let pairs = [
            (identity(&schema), identity(&schema)),
            (filtered, identity(&schema)),
        ];
        for (first, second) in pairs {
            let composite = table_lens::compose(&first, &second, &protocol, &protocol, &protocol)
                .expect("composition");
            let GraphComposite::Compiled(compiled) = composite else {
                return Err(TestCaseError::fail("fusible table pair did not fuse"));
            };
            let (compiled_view, compiled_complement) = table_lens::get(
                &compiled,
                &source,
                contexts(&protocol),
            )
            .expect("compiled get");
            let (middle, first_complement) =
                table_lens::get(&first, &source, contexts(&protocol)).expect("first get");
            let (sequential_view, second_complement) =
                table_lens::get(&second, &middle, contexts(&protocol)).expect("second get");
            prop_assert_eq!(&compiled_view, &sequential_view);
            let compiled_source = table_lens::put(
                &compiled,
                &compiled_view,
                &compiled_complement,
                contexts(&protocol),
            )
                .expect("compiled put");
            let middle = table_lens::put(
                &second,
                &sequential_view,
                &second_complement,
                contexts(&protocol),
            )
            .expect("second put");
            let sequential_source = table_lens::put(
                &first,
                &middle,
                &first_complement,
                contexts(&protocol),
            )
            .expect("first put");
            prop_assert_eq!(&compiled_source, &sequential_source);
            prop_assert_eq!(compiled_source, source.clone());
        }
    }

    #[test]
    fn table_lens_path_laws(values in prop::collection::vec(any::<i16>(), 0..12)) {
        let p = protocol();
        let schema = tables(&[("t", &[("a", "int")])], &[]);
        let first = LensPath::single(identity(&schema));
        let second = LensPath::single(identity(&schema));
        let path = first.then(&second).expect("path");
        let rows = values.iter().enumerate().map(|(index, value)| {
            (
                RowId(u32::try_from(index).expect("small generated index")),
                row(&[("a", Value::Int(i64::from(*value)))]),
            )
        });
        let source = FInstance::new().with_rows("t", rows).expect("generated rows");
        let protocols = [p.clone(), p.clone(), p];
        let (view, complement) = path.get_table(&source, &protocols).expect("path get");
        let restored = path
            .put_table(&view, &complement, &protocols)
            .expect("path put");
        prop_assert_eq!(restored, source);
    }
}
