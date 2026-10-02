#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Round-trip laws and boundary cases for asymmetric table lenses.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;

use panproto_expr::{BuiltinOp, Expr, Literal};
use panproto_gat::{CoercionClass, Name};
use panproto_inst::identity::{AnyComplement, Encoding, IdentityField, decode_complement};
use panproto_inst::{
    BoundaryProtocols, CompiledMigration, ComplementShape, EdgeId, FInstance, FieldTransform,
    IdentityError, PutDomainError, RowId, SynthIndex, TableComplement, TableEmbedding, Value,
    ViewRef,
};
use panproto_lens::asymmetric::ComplementCompose;
use panproto_lens::table_lens;
use panproto_lens::{
    ComplementIdentity, GraphComposite, Lens, LensError, LensPath, LensShape, TableContext,
    TableShape,
};
use panproto_schema::{
    ConstraintSemantics, Edge, Key, KeyPart, KeyRole, KeyScope, Protocol, ScalarKind, Schema,
    SchemaBuilder,
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
