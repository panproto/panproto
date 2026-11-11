//! Independent finite relational certificate for the graph/table lens square.
//!
//! The oracle in this file is written directly over rows, cells, and relation
//! pairs. It does not call either lens engine or the executable reference lens.
//! Production's graph and table routes are compared with each other and with
//! that oracle on every state and every one-coordinate edit in the bound.

#![allow(clippy::expect_used, clippy::too_many_lines)]

use std::collections::{BTreeMap, BTreeSet, HashMap};

use panproto_inst::{
    BoundaryProtocols, CompiledMigration, ComplementShape, FInstance, GInstance, PutDomainError,
    RowId, Value,
};
use panproto_lens::{
    GraphShape, Lens, LensError, LensShape, graph_get_via_table, graph_lens, graph_put_via_table,
    table_lens,
};
use panproto_reference::{
    enumerate::lenses::{
        RELATIONAL_TABLE_SOURCE_COUNT, for_each_relational_table_source,
        relational_table_source_count_certificate,
    },
    lens::{RelationalTableMigration, relational_table_get, relational_table_put},
    model::{SEdge as RefEdge, Table as RefTable},
    value::Value as RefValue,
};
use panproto_schema::{Edge, Protocol, ScalarKind, Schema, SchemaBuilder};
use proptest::prelude::*;

const MIGRATIONS: usize = 5;
const STATES_PER_MIGRATION: usize = 1_831;
const TOTAL_STATES: usize = MIGRATIONS * STATES_PER_MIGRATION;
const TOTAL_OBSERVATIONS: usize = 61_126;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MigrationKind {
    Identity,
    RenameColumn,
    DropColumn,
    DropRelation,
    DropTable,
}

impl MigrationKind {
    const ALL: [Self; MIGRATIONS] = [
        Self::Identity,
        Self::RenameColumn,
        Self::DropColumn,
        Self::DropRelation,
        Self::DropTable,
    ];

    const fn index(self) -> usize {
        match self {
            Self::Identity => 0,
            Self::RenameColumn => 1,
            Self::DropColumn => 2,
            Self::DropRelation => 3,
            Self::DropTable => 4,
        }
    }

    const fn target_has_b(self) -> bool {
        !matches!(self, Self::DropTable)
    }

    const fn target_has_relation(self) -> bool {
        !matches!(self, Self::DropRelation | Self::DropTable)
    }

    const fn target_a_column(self) -> Option<&'static str> {
        match self {
            Self::RenameColumn => Some("z"),
            Self::DropColumn => None,
            Self::Identity | Self::DropRelation | Self::DropTable => Some("x"),
        }
    }

    const fn target_b_column(self) -> Option<&'static str> {
        if self.target_has_b() { Some("y") } else { None }
    }

    const fn commutes_with_protocol_free_graph(self) -> bool {
        !matches!(self, Self::RenameColumn | Self::DropColumn)
    }

    const fn reference(self) -> RelationalTableMigration {
        match self {
            Self::Identity => RelationalTableMigration::Identity,
            Self::RenameColumn => RelationalTableMigration::RenameColumn,
            Self::DropColumn => RelationalTableMigration::DropColumn,
            Self::DropRelation => RelationalTableMigration::DropRelation,
            Self::DropTable => RelationalTableMigration::DropTable,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExpectedPut {
    Restored,
    ReservedIdCollision,
}

#[derive(Clone, Debug)]
struct EditCase {
    label: String,
    view: FInstance,
    expected: ExpectedPut,
}

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

fn column(table: &str, name: &str) -> Edge {
    Edge {
        src: table.into(),
        tgt: format!("{table}.{name}").into(),
        kind: "column".into(),
        name: Some(name.into()),
    }
}

fn relation() -> Edge {
    Edge {
        src: "a".into(),
        tgt: "b".into(),
        kind: "relation".into(),
        name: Some("ab".into()),
    }
}

fn schema(
    protocol: &Protocol,
    a_column: Option<&str>,
    b_column: Option<&str>,
    with_relation: bool,
) -> Schema {
    let mut builder = SchemaBuilder::new(protocol)
        .vertex("a", "record", None)
        .expect("table a");
    if let Some(name) = a_column {
        let vertex = format!("a.{name}");
        builder = builder
            .vertex(&vertex, "int", None)
            .expect("a column vertex")
            .edge("a", &vertex, "column", Some(name))
            .expect("a column edge");
    }
    if let Some(name) = b_column {
        let vertex = format!("b.{name}");
        builder = builder
            .vertex("b", "record", None)
            .expect("table b")
            .vertex(&vertex, "int", None)
            .expect("b column vertex")
            .edge("b", &vertex, "column", Some(name))
            .expect("b column edge");
    }
    if with_relation {
        builder = builder
            .edge("a", "b", "relation", Some("ab"))
            .expect("a-to-b relation");
    }
    builder.build().expect("relational schema")
}

fn lens(kind: MigrationKind, protocol: &Protocol) -> Lens {
    let source = schema(protocol, Some("x"), Some("y"), true);
    let target = schema(
        protocol,
        kind.target_a_column(),
        kind.target_b_column(),
        kind.target_has_relation(),
    );
    let mut compiled = CompiledMigration {
        surviving_verts: target.vertices.keys().cloned().collect(),
        surviving_edges: target.edges.keys().cloned().collect(),
        ..CompiledMigration::default()
    };
    if kind == MigrationKind::RenameColumn {
        compiled.vertex_remap.insert("a.x".into(), "a.z".into());
        compiled
            .edge_remap
            .insert(column("a", "x"), column("a", "z"));
    }
    Lens {
        src_schema: source,
        tgt_schema: target,
        compiled,
    }
}

fn production_value(value: &RefValue) -> Value {
    match value {
        RefValue::Int(value) => Value::Int(*value),
        other => panic!("value outside the relational carrier: {other:?}"),
    }
}

fn reference_value(value: &Value) -> RefValue {
    match value {
        Value::Int(value) => RefValue::Int(*value),
        other => panic!("value outside the relational carrier: {other:?}"),
    }
}

fn production_edge(edge: &RefEdge) -> Edge {
    Edge {
        src: edge.src.as_str().into(),
        tgt: edge.tgt.as_str().into(),
        kind: edge.kind.as_str().into(),
        name: edge.name.as_deref().map(Into::into),
    }
}

fn reference_edge(edge: &Edge) -> RefEdge {
    RefEdge::new(&edge.src, &edge.tgt, &edge.kind, edge.name.as_deref())
}

fn production_table(table: &RefTable) -> FInstance {
    let mut production = FInstance::new();
    for (id, (name, cells)) in &table.rows {
        production
            .insert_row(
                name.as_str(),
                RowId(*id),
                cells
                    .iter()
                    .map(|(column, value)| (column.clone(), production_value(value)))
                    .collect(),
            )
            .expect("reference row ids are globally distinct");
    }
    for (edge, source, target) in &table.pairs {
        production
            .insert_foreign_key(production_edge(edge), RowId(*source), RowId(*target))
            .expect("reference pairs have present endpoints");
    }
    production
}

fn reference_table(table: &FInstance) -> RefTable {
    let mut reference = RefTable::default();
    for (name, _) in table.tables() {
        for (id, cells) in table.rows(name) {
            reference.rows.insert(
                id.0,
                (
                    name.to_string(),
                    cells
                        .iter()
                        .map(|(column, value)| (column.clone(), reference_value(value)))
                        .collect(),
                ),
            );
        }
    }
    for (edge, source, target) in table.all_pairs() {
        reference
            .pairs
            .insert((reference_edge(&edge), source.0, target.0));
    }
    reference
}

fn for_each_source(mut visit: impl FnMut(&RefTable, &FInstance)) {
    for_each_relational_table_source(|reference| {
        let production = production_table(&reference);
        visit(&reference, &production);
    });
}

fn oracle_get(kind: MigrationKind, source: &RefTable) -> FInstance {
    production_table(&relational_table_get(kind.reference(), source))
}

fn oracle_put(kind: MigrationKind, source: &RefTable, view: &FInstance) -> FInstance {
    production_table(&relational_table_put(
        kind.reference(),
        source,
        &reference_table(view),
    ))
}

const fn toggled(value: Option<&Value>) -> Value {
    match value {
        Some(Value::Int(0)) => Value::Int(1),
        _ => Value::Int(0),
    }
}

fn edit_cases(kind: MigrationKind, source: &FInstance, view: &FInstance) -> Vec<EditCase> {
    let mut cases = vec![EditCase {
        label: "unedited".to_owned(),
        view: view.clone(),
        expected: ExpectedPut::Restored,
    }];

    for (table, column_name) in [("a", kind.target_a_column()), ("b", kind.target_b_column())] {
        let Some(column_name) = column_name else {
            continue;
        };
        for (id, _) in view.rows(table) {
            let mut edited = view.clone();
            let row = edited.row_mut(id).expect("enumerated view row");
            let value = toggled(row.get(column_name));
            row.insert(column_name.to_owned(), value);
            cases.push(EditCase {
                label: format!("cell:{table}:{}:{column_name}", id.0),
                view: edited,
                expected: ExpectedPut::Restored,
            });
        }
    }

    for id in view.row_ids() {
        let mut edited = view.clone();
        edited.remove_row(id).expect("enumerated view row");
        cases.push(EditCase {
            label: format!("delete-row:{}", id.0),
            view: edited,
            expected: ExpectedPut::Restored,
        });
    }

    if kind.target_has_relation() {
        for (src, _) in view.rows("a") {
            for (tgt, _) in view.rows("b") {
                let mut edited = view.clone();
                if edited.contains_pair(&relation(), src, tgt) {
                    assert!(edited.remove_foreign_key(&relation(), src, tgt));
                } else {
                    edited
                        .insert_foreign_key(relation(), src, tgt)
                        .expect("the edit endpoints were enumerated from the view");
                }
                cases.push(EditCase {
                    label: format!("toggle-pair:{}:{}", src.0, tgt.0),
                    view: edited,
                    expected: ExpectedPut::Restored,
                });
            }
        }
    }

    let mut inserted_a = view.clone();
    inserted_a
        .insert_row("a", RowId(10), HashMap::new())
        .expect("the bounded carrier does not use row 10");
    cases.push(EditCase {
        label: "insert-row:a:10".to_owned(),
        view: inserted_a,
        expected: ExpectedPut::Restored,
    });
    if kind.target_has_b() {
        let mut inserted_b = view.clone();
        inserted_b
            .insert_row("b", RowId(11), HashMap::new())
            .expect("the bounded carrier does not use row 11");
        cases.push(EditCase {
            label: "insert-row:b:11".to_owned(),
            view: inserted_b,
            expected: ExpectedPut::Restored,
        });
    }

    if kind == MigrationKind::DropTable {
        for (id, _) in source.rows("b") {
            let mut edited = view.clone();
            edited
                .insert_row("a", id, HashMap::new())
                .expect("the dropped b id is absent from the view");
            cases.push(EditCase {
                label: format!("reserved-id-collision:{}", id.0),
                view: edited,
                expected: ExpectedPut::ReservedIdCollision,
            });
        }
    }
    cases
}

fn assert_reserved_id_collision(error: &LensError, label: &str) {
    assert!(
        matches!(
            error,
            LensError::PutDomain(reason)
                if matches!(reason.as_ref(), PutDomainError::IdCollision { .. })
        ),
        "{label}: expected IdCollision, found {error:?}"
    );
}

#[test]
fn graph_and_table_routes_commute_on_the_independent_relational_carrier() {
    assert_eq!(RELATIONAL_TABLE_SOURCE_COUNT, STATES_PER_MIGRATION);
    assert_eq!(
        relational_table_source_count_certificate(),
        STATES_PER_MIGRATION
    );
    let protocol = protocol();
    let boundaries = BoundaryProtocols {
        src: &protocol,
        tgt: &protocol,
    };
    let lenses: Vec<(MigrationKind, Lens)> = MigrationKind::ALL
        .into_iter()
        .map(|kind| (kind, lens(kind, &protocol)))
        .collect();
    for (kind, lens) in &lenses {
        assert!(
            lens.preserves_table_shape(),
            "{kind:?} belongs to the table-shape fragment"
        );
    }

    let mut states = 0;
    let mut observations = 0;
    let mut observations_by_migration = [0_usize; MIGRATIONS];
    for_each_source(|reference_source, source| {
        for (kind, lens) in &lenses {
            states += 1;
            let context = format!("migration={kind:?}, source={source:?}");
            let expected_view = oracle_get(*kind, reference_source);
            let (source_graph, _) =
                GInstance::from_finstance(source).expect("bounded source graph form");
            if !kind.commutes_with_protocol_free_graph() {
                observations += 1;
                observations_by_migration[kind.index()] += 1;
                assert!(
                    graph_get_via_table(lens, &source_graph, boundaries)
                        .unwrap_or_else(|error| {
                            panic!("{context}: declined table route errored: {error:?}")
                        })
                        .is_none(),
                    "{context}: a structural column fate must decline"
                );
                let (table_view, _) = table_lens::get(lens, source, boundaries)
                    .unwrap_or_else(|error| panic!("{context}: native table get: {error:?}"));
                assert_eq!(table_view, expected_view, "{context}: native table oracle");
                let (graph_view, graph_complement) = graph_lens::get(lens, &source_graph)
                    .unwrap_or_else(|error| panic!("{context}: direct graph get: {error:?}"));
                assert!(
                    graph_put_via_table(lens, &graph_view, &graph_complement, boundaries)
                        .unwrap_or_else(|error| {
                            panic!("{context}: declined table put errored: {error:?}")
                        })
                        .is_none(),
                    "{context}: a structural column fate must decline put"
                );
                continue;
            }
            let (graph_view, graph_complement) = graph_lens::get(lens, &source_graph)
                .unwrap_or_else(|error| panic!("{context}: graph get refused: {error:?}"));
            let (fast_view, fast_complement) = graph_get_via_table(lens, &source_graph, boundaries)
                .unwrap_or_else(|error| panic!("{context}: table get errored: {error:?}"))
                .unwrap_or_else(|| panic!("{context}: table get declined inside its criterion"));
            assert!(
                GInstance::identical(&fast_view, &graph_view),
                "{context}: get views differ"
            );
            assert_eq!(
                GraphShape::complement_divergence(&fast_complement, &graph_complement),
                None,
                "{context}: complement conversion differs; fast={fast_complement:?}; graph={graph_complement:?}"
            );
            // The observational comparison above excludes the identity
            // envelope, so the envelope half of the square is asserted here:
            // the conversion re-stamps the graph envelope that graph get
            // stamps, and both routes take the same node and edge floors.
            assert_eq!(
                fast_complement.identity,
                Some(lens.identity(ComplementShape::Graph)),
                "{context}: converted complement envelope"
            );
            assert_eq!(
                fast_complement.identity, graph_complement.identity,
                "{context}: complement envelopes differ"
            );
            assert_eq!(
                fast_complement.id_floor, graph_complement.id_floor,
                "{context}: complement id floors differ"
            );
            let (observed_view, _) = graph_view
                .try_to_finstance(&lens.tgt_schema)
                .unwrap_or_else(|error| {
                    panic!("{context}: graph view is not relational: {error:?}")
                });
            assert_eq!(observed_view, expected_view, "{context}: oracle get");

            for case in edit_cases(*kind, source, &expected_view) {
                observations += 1;
                observations_by_migration[kind.index()] += 1;
                let label = format!("{context}, edit={}", case.label);
                let (_, mut embedding) = graph_view
                    .try_to_finstance(&lens.tgt_schema)
                    .expect("the graph view was already decoded above");
                let edited_graph = GInstance::from_finstance_along(&case.view, &mut embedding)
                    .unwrap_or_else(|error| panic!("{label}: edited graph form: {error:?}"));
                let direct = graph_lens::put(lens, &edited_graph, &graph_complement);
                let fast = graph_put_via_table(lens, &edited_graph, &graph_complement, boundaries);
                match case.expected {
                    ExpectedPut::Restored => {
                        let direct = direct.unwrap_or_else(|error| {
                            panic!("{label}: direct graph put refused: {error:?}")
                        });
                        let fast = fast
                            .unwrap_or_else(|error| panic!("{label}: table put errored: {error:?}"))
                            .unwrap_or_else(|| {
                                panic!("{label}: table put declined inside the common domain")
                            });
                        assert!(
                            GInstance::identical(&fast, &direct),
                            "{label}: put results differ; fast={fast:?}; direct={direct:?}"
                        );
                        let (observed, _) = direct
                            .try_to_finstance(&lens.src_schema)
                            .unwrap_or_else(|error| {
                                panic!("{label}: restored graph is not relational: {error:?}")
                            });
                        assert_eq!(
                            observed,
                            oracle_put(*kind, reference_source, &case.view),
                            "{label}: oracle put"
                        );
                    }
                    ExpectedPut::ReservedIdCollision => {
                        let direct = direct
                            .as_ref()
                            .expect_err("the independent oracle predicts a direct refusal");
                        assert_reserved_id_collision(direct, &label);
                        let fast = fast
                            .as_ref()
                            .expect_err("the independent oracle predicts a table-route refusal");
                        assert_reserved_id_collision(fast, &label);
                        assert_eq!(
                            format!("{direct:?}"),
                            format!("{fast:?}"),
                            "{label}: refusal details differ"
                        );
                    }
                }
            }
        }
    });

    assert_eq!(states, TOTAL_STATES);
    assert_eq!(
        observations_by_migration,
        [24_957, 1_831, 1_831, 18_837, 13_670]
    );
    assert_eq!(observations, TOTAL_OBSERVATIONS);
    println!(
        "L18 relational certificate: migrations={MIGRATIONS}, states={states}, observations={observations}, per_migration={observations_by_migration:?}"
    );
}

fn generated_source(
    a_rows: &[(u8, Option<i64>)],
    b_rows: &[(u8, Option<i64>)],
    pair_mask: u32,
) -> FInstance {
    let mut source = FInstance::new();
    for (table, column, rows, parity) in [("a", "x", a_rows, 0_u32), ("b", "y", b_rows, 1_u32)] {
        let mut unique = BTreeMap::new();
        for (raw_id, value) in rows {
            unique.insert(100 + u32::from(*raw_id) * 2 + parity, *value);
        }
        for (id, value) in unique {
            let cells = value
                .map(|value| HashMap::from([(column.to_owned(), Value::Int(value))]))
                .unwrap_or_default();
            source
                .insert_row(table, RowId(id), cells)
                .expect("generated ids are disjoint between tables");
        }
    }
    let b_ids: Vec<_> = source.rows("b").into_iter().map(|(id, _)| id).collect();
    let pairs: Vec<_> = source
        .rows("a")
        .into_iter()
        .flat_map(|(source_id, _)| {
            b_ids
                .iter()
                .copied()
                .map(move |target_id| (source_id, target_id))
        })
        .collect();
    for (bit, (source_id, target_id)) in pairs.into_iter().enumerate() {
        if pair_mask & (1 << bit) != 0 {
            source
                .insert_foreign_key(relation(), source_id, target_id)
                .expect("generated relation endpoints are present");
        }
    }
    source
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn randomized_relational_table_square_differential(
        a_rows in prop::collection::vec((0_u8..8, prop::option::of(any::<i64>())), 0..5),
        b_rows in prop::collection::vec((0_u8..8, prop::option::of(any::<i64>())), 0..5),
        pair_mask in any::<u32>(),
        edit_selector in any::<usize>(),
    ) {
        let protocol = protocol();
        let boundaries = BoundaryProtocols {
            src: &protocol,
            tgt: &protocol,
        };
        let source = generated_source(&a_rows, &b_rows, pair_mask);
        let reference_source = reference_table(&source);
        let (source_graph, _) = GInstance::from_finstance(&source)
            .expect("generated relational source has a graph form");

        for kind in MigrationKind::ALL {
            let lens = lens(kind, &protocol);
            let expected_view = oracle_get(kind, &reference_source);
            let context = format!("migration={kind:?}, source={source:?}");
            if !kind.commutes_with_protocol_free_graph() {
                prop_assert!(
                    graph_get_via_table(&lens, &source_graph, boundaries)
                        .map_err(|error| TestCaseError::fail(format!("{context}: {error:?}")))?
                        .is_none(),
                    "{context}: structural cell fate did not decline get"
                );
                let (graph_view, graph_complement) = graph_lens::get(&lens, &source_graph)
                    .map_err(|error| TestCaseError::fail(format!("{context}: {error:?}")))?;
                prop_assert!(
                    graph_put_via_table(&lens, &graph_view, &graph_complement, boundaries)
                        .map_err(|error| TestCaseError::fail(format!("{context}: {error:?}")))?
                        .is_none(),
                    "{context}: structural cell fate did not decline put"
                );
                let (table_view, _) = table_lens::get(&lens, &source, boundaries)
                    .map_err(|error| TestCaseError::fail(format!("{context}: {error:?}")))?;
                prop_assert_eq!(table_view, expected_view, "{}: reference get", context);
                continue;
            }

            let (graph_view, graph_complement) = graph_lens::get(&lens, &source_graph)
                .map_err(|error| TestCaseError::fail(format!("{context}: {error:?}")))?;
            let (fast_view, fast_complement) = graph_get_via_table(
                &lens,
                &source_graph,
                boundaries,
            )
            .map_err(|error| TestCaseError::fail(format!("{context}: {error:?}")))?
            .ok_or_else(|| TestCaseError::fail(format!("{context}: table get declined")))?;
            prop_assert!(GInstance::identical(&fast_view, &graph_view), "{context}: get");
            prop_assert_eq!(
                GraphShape::complement_divergence(&fast_complement, &graph_complement),
                None,
                "{}: complement",
                context
            );
            prop_assert_eq!(
                &fast_complement.identity,
                &graph_complement.identity,
                "{}: envelope",
                context
            );
            prop_assert_eq!(
                &fast_complement.id_floor,
                &graph_complement.id_floor,
                "{}: floor",
                context
            );
            let (observed_view, _) = graph_view
                .try_to_finstance(&lens.tgt_schema)
                .map_err(|error| TestCaseError::fail(format!("{context}: {error:?}")))?;
            prop_assert_eq!(
                &observed_view,
                &expected_view,
                "{}: reference get",
                context
            );

            let cases = edit_cases(kind, &source, &expected_view);
            let case = &cases[edit_selector % cases.len()];
            let label = format!("{context}, edit={}", case.label);
            let (_, mut embedding) = graph_view
                .try_to_finstance(&lens.tgt_schema)
                .map_err(|error| TestCaseError::fail(format!("{label}: {error:?}")))?;
            let edited_graph = GInstance::from_finstance_along(&case.view, &mut embedding)
                .map_err(|error| TestCaseError::fail(format!("{label}: {error:?}")))?;
            let direct = graph_lens::put(&lens, &edited_graph, &graph_complement);
            let fast = graph_put_via_table(
                &lens,
                &edited_graph,
                &graph_complement,
                boundaries,
            );
            match case.expected {
                ExpectedPut::Restored => {
                    let direct = direct
                        .map_err(|error| TestCaseError::fail(format!("{label}: {error:?}")))?;
                    let fast = fast
                        .map_err(|error| TestCaseError::fail(format!("{label}: {error:?}")))?
                        .ok_or_else(|| TestCaseError::fail(format!("{label}: table put declined")))?;
                    prop_assert!(GInstance::identical(&fast, &direct), "{label}: put");
                    let (observed, _) = direct
                        .try_to_finstance(&lens.src_schema)
                        .map_err(|error| TestCaseError::fail(format!("{label}: {error:?}")))?;
                    prop_assert_eq!(
                        observed,
                        oracle_put(kind, &reference_source, &case.view),
                        "{}: reference put",
                        label
                    );
                }
                ExpectedPut::ReservedIdCollision => {
                    let direct = direct.as_ref().expect_err("direct put must refuse");
                    let fast = fast.as_ref().expect_err("table put must refuse");
                    prop_assert!(
                        matches!(
                            direct,
                            LensError::PutDomain(reason)
                                if matches!(reason.as_ref(), PutDomainError::IdCollision { .. })
                        ),
                        "{label}: direct refusal={direct:?}"
                    );
                    prop_assert_eq!(
                        format!("{direct:?}"),
                        format!("{fast:?}"),
                        "{}",
                        label
                    );
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Table edit lenses (L-edits-10)
// ---------------------------------------------------------------------------

mod table_edit_lens_differential {
    use panproto_inst::TableEmbedding;
    use panproto_inst::table_delta::{CellChange, TableChange, TableDelta, TableDeltaParts};
    use panproto_lens::{GraphEditLens, ShapeEditError, TableContext, TableEditLens};
    use panproto_reference::edit::{
        self as ref_edit, TElem, TKey, TPayload, TVal, TableDelta as RefTableDelta, TableEditError,
        TableEditState,
    };
    use panproto_reference::enumerate::edits::{
        TableLetterSchema, table_edit_lens_letter_count, table_edit_lens_letters,
    };

    use super::*;

    const VALUES: [RefValue; 2] = [RefValue::Int(0), RefValue::Int(1)];

    fn production_delta(delta: &RefTableDelta) -> TableDelta {
        let mut parts = TableDeltaParts::default();
        for (side, map) in [(true, &delta.del), (false, &delta.add)] {
            for (element, payload) in map {
                match (element, payload) {
                    (TElem::Row(id), TPayload::Row(table, cells)) => {
                        let row = (
                            table.as_str().into(),
                            cells
                                .iter()
                                .map(|(column, value)| (column.clone(), production_value(value)))
                                .collect(),
                        );
                        if side {
                            parts.del_rows.insert(RowId(*id), row);
                        } else {
                            parts.add_rows.insert(RowId(*id), row);
                        }
                    }
                    (TElem::Pair(edge, src, tgt), TPayload::Pair) => {
                        let pair = (production_edge(edge), RowId(*src), RowId(*tgt));
                        if side {
                            parts.del_pairs.insert(pair);
                        } else {
                            parts.add_pairs.insert(pair);
                        }
                    }
                    other => panic!("cross-sort reference table element: {other:?}"),
                }
            }
        }
        for (key, (old, new)) in &delta.attrs {
            match (key, old, new) {
                (TKey::Cell(row, column), TVal::Cell(old), TVal::Cell(new)) => {
                    parts.cells.insert(
                        (RowId(*row), column.clone()),
                        CellChange {
                            old: old.as_ref().map(production_value),
                            new: new.as_ref().map(production_value),
                        },
                    );
                }
                (TKey::Table(row), TVal::Table(old), TVal::Table(new)) => {
                    parts.tables.insert(
                        RowId(*row),
                        TableChange {
                            old: old.as_str().into(),
                            new: new.as_str().into(),
                        },
                    );
                }
                other => panic!("cross-sort reference table attribute: {other:?}"),
            }
        }
        TableDelta::normalize(parts)
    }

    fn reference_delta(delta: &TableDelta) -> RefTableDelta {
        let mut out = RefTableDelta::identity();
        let row = |table: &panproto_gat::Name, cells: &panproto_inst::Row| {
            TPayload::Row(
                table.to_string(),
                cells
                    .iter()
                    .map(|(column, value)| (column.clone(), reference_value(value)))
                    .collect(),
            )
        };
        for (id, (table, cells)) in delta.del_rows() {
            out.del.insert(TElem::Row(id.0), row(table, cells));
        }
        for (id, (table, cells)) in delta.add_rows() {
            out.add.insert(TElem::Row(id.0), row(table, cells));
        }
        for (edge, src, tgt) in delta.del_pairs() {
            out.del.insert(
                TElem::Pair(reference_edge(edge), src.0, tgt.0),
                TPayload::Pair,
            );
        }
        for (edge, src, tgt) in delta.add_pairs() {
            out.add.insert(
                TElem::Pair(reference_edge(edge), src.0, tgt.0),
                TPayload::Pair,
            );
        }
        for ((row, column), change) in delta.cells() {
            out.attrs.insert(
                TKey::Cell(row.0, column.clone()),
                (
                    TVal::Cell(change.old.as_ref().map(reference_value)),
                    TVal::Cell(change.new.as_ref().map(reference_value)),
                ),
            );
        }
        for (row, change) in delta.tables() {
            out.attrs.insert(
                TKey::Table(row.0),
                (
                    TVal::Table(change.old.to_string()),
                    TVal::Table(change.new.to_string()),
                ),
            );
        }
        out
    }

    fn reference_relation() -> RefEdge {
        reference_edge(&relation())
    }

    /// The tables, columns and relation of the source schema, or of the
    /// target schema of `kind`.
    fn with_letter_schema<T>(
        kind: Option<MigrationKind>,
        run: impl FnOnce(TableLetterSchema<'_>) -> T,
    ) -> T {
        let relations = [reference_relation()];
        let a_column: Vec<&str> = match kind {
            None => vec!["x"],
            Some(kind) => kind.target_a_column().into_iter().collect(),
        };
        let b_column: Vec<&str> = match kind {
            None => vec!["y"],
            Some(kind) => kind.target_b_column().into_iter().collect(),
        };
        let mut tables: Vec<(&str, &[&str])> = vec![("a", a_column.as_slice())];
        if kind.is_none_or(MigrationKind::target_has_b) {
            tables.push(("b", b_column.as_slice()));
        }
        let relations: &[RefEdge] = if kind.is_none_or(MigrationKind::target_has_relation) {
            &relations
        } else {
            &[]
        };
        run(TableLetterSchema {
            tables: &tables,
            relations,
            values: &VALUES,
        })
    }

    fn letters(kind: Option<MigrationKind>, table: &RefTable) -> Vec<RefTableDelta> {
        with_letter_schema(kind, |schema| {
            let letters = table_edit_lens_letters(schema, table);
            assert_eq!(letters.len(), table_edit_lens_letter_count(schema, table));
            letters
        })
    }

    fn context() -> TableContext {
        TableContext {
            src: protocol(),
            tgt: protocol(),
        }
    }

    /// The reference put of the relational carrier: the relational oracle,
    /// refusing an edited view that reuses the id of a source row the view
    /// does not show, as the reserved-id clause of the put domain does.
    fn oracle_state_put(
        kind: MigrationKind,
        source: &RefTable,
        view: &RefTable,
    ) -> Result<RefTable, String> {
        let shown = relational_table_get(kind.reference(), source);
        if view
            .rows
            .keys()
            .any(|id| source.rows.contains_key(id) && !shown.rows.contains_key(id))
        {
            return Err("IdCollision".to_owned());
        }
        Ok(relational_table_put(kind.reference(), source, view))
    }

    fn production_refusal(error: &ShapeEditError) -> String {
        match error {
            ShapeEditError::Apply(_) => "apply".to_owned(),
            ShapeEditError::Put(error)
                if matches!(
                    error.as_ref(),
                    LensError::PutDomain(reason)
                        if matches!(reason.as_ref(), PutDomainError::IdCollision { .. })
                ) =>
            {
                "put IdCollision".to_owned()
            }
            ShapeEditError::PutGetMismatch => "put-get mismatch".to_owned(),
            other => format!("production refusal outside the carrier: {other:?}"),
        }
    }

    fn reference_refusal(error: &TableEditError<String, String>) -> String {
        match error {
            TableEditError::Apply(_) => "apply".to_owned(),
            TableEditError::Get(reason) => format!("get {reason}"),
            TableEditError::Put(reason) => format!("put {reason}"),
            TableEditError::PutGetMismatch => "put-get mismatch".to_owned(),
        }
    }

    #[derive(Debug, PartialEq)]
    enum Outcome {
        Admitted(RefTableDelta, RefTable, RefTable),
        Refused(String),
    }

    /// Translate `letter` on both sides, assert agreement and that a
    /// refusal leaves the production lens as it was, and return the
    /// advanced states when the edit is admitted.
    fn step(
        kind: MigrationKind,
        production: &TableEditLens,
        state: &TableEditState,
        put: bool,
        letter: &RefTableDelta,
    ) -> Option<(TableEditLens, TableEditState)> {
        let get =
            |source: &RefTable| Ok::<_, String>(relational_table_get(kind.reference(), source));
        let put_oracle = |source: &RefTable, view: &RefTable| oracle_state_put(kind, source, view);
        let reference = if put {
            ref_edit::table_edit_put(&get, &put_oracle, state, letter)
        } else {
            ref_edit::table_edit_get::<String, String>(&get, state, letter)
        };
        let mut advanced = production.clone();
        let edit = production_delta(letter);
        let produced = if put {
            advanced.put_edit(&edit)
        } else {
            advanced.get_edit(&edit)
        };
        let production_outcome = match &produced {
            Ok(step) => Outcome::Admitted(
                reference_delta(if put {
                    &step.source_delta
                } else {
                    &step.view_delta
                }),
                reference_table(advanced.source()),
                reference_table(advanced.view()),
            ),
            Err(error) => {
                assert_eq!(advanced.source(), production.source());
                assert_eq!(advanced.view(), production.view());
                assert_eq!(advanced.complement(), production.complement());
                Outcome::Refused(production_refusal(error))
            }
        };
        let reference_outcome = match &reference {
            Ok((delta, next)) => {
                Outcome::Admitted(delta.clone(), next.source.clone(), next.view.clone())
            }
            Err(error) => Outcome::Refused(reference_refusal(error)),
        };
        assert_eq!(
            production_outcome,
            reference_outcome,
            "{kind:?} {} of {letter:?} from {:?}",
            if put { "put" } else { "get" },
            state.source
        );
        match reference {
            Ok((_, next)) if produced.is_ok() => Some((advanced, next)),
            _ => None,
        }
    }

    /// The table edits one sweep compared.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    struct Counts {
        bases: usize,
        get_letters: usize,
        put_letters: usize,
        inverses: usize,
    }

    fn unit(kind: MigrationKind, reference_source: &RefTable, source: &FInstance) -> Counts {
        let protocol = protocol();
        let production = TableEditLens::new(lens(kind, &protocol), source.clone(), context())
            .expect("table get accepts every relational source");
        let state = TableEditState {
            source: reference_source.clone(),
            view: relational_table_get(kind.reference(), reference_source),
        };
        assert_eq!(reference_table(production.view()), state.view);
        let mut counts = Counts {
            bases: 1,
            ..Counts::default()
        };
        for letter in letters(None, reference_source) {
            counts.get_letters += 1;
            if let Some((advanced, next)) = step(kind, &production, &state, false, &letter) {
                counts.inverses += 1;
                step(kind, &advanced, &next, false, &letter.invert());
            }
        }
        for letter in letters(Some(kind), &state.view) {
            counts.put_letters += 1;
            step(kind, &production, &state, true, &letter);
        }
        counts
    }

    /// The closed-form letter census of the relational carrier: bases, get
    /// letters on the sources and put letters on the oracle views.
    const TABLE_EDIT_LENS_CENSUS: (usize, usize, usize) = (9_155, 200_655, 166_227);

    fn census() -> (usize, usize, usize) {
        let (mut bases, mut get, mut put) = (0, 0, 0);
        for_each_relational_table_source(|source| {
            for kind in MigrationKind::ALL {
                bases += 1;
                get += with_letter_schema(None, |schema| {
                    table_edit_lens_letter_count(schema, &source)
                });
                let view = relational_table_get(kind.reference(), &source);
                put += with_letter_schema(Some(kind), |schema| {
                    table_edit_lens_letter_count(schema, &view)
                });
            }
        });
        (bases, get, put)
    }

    #[test]
    fn table_edit_lens_letter_census_is_the_closed_form_over_the_relational_carrier() {
        let census = census();
        println!("table edit lens census: {census:?}");
        assert_eq!(census, TABLE_EDIT_LENS_CENSUS);
    }

    #[test]
    fn table_edit_lens_agrees_with_the_reference_on_the_relational_carrier() {
        let mut counts = Counts::default();
        for_each_source(|reference_source, source| {
            for kind in MigrationKind::ALL {
                let unit = unit(kind, reference_source, source);
                counts.bases += unit.bases;
                counts.get_letters += unit.get_letters;
                counts.put_letters += unit.put_letters;
                counts.inverses += unit.inverses;
            }
        });
        println!("table edit lens relational carrier: {counts:?}");
        assert_eq!(
            (counts.bases, counts.get_letters, counts.put_letters),
            TABLE_EDIT_LENS_CENSUS
        );
    }

    /// The graph edit lens translation of a table edit, read back as a
    /// table delta with the next table source and view, or its refusal.
    fn graph_route(
        lens_kind: MigrationKind,
        source: &FInstance,
        put: bool,
        edit: &TableDelta,
    ) -> Result<(TableDelta, FInstance, FInstance), String> {
        let protocol = protocol();
        let lens = lens(lens_kind, &protocol);
        let tgt_schema = lens.tgt_schema.clone();
        let src_schema = lens.src_schema.clone();
        let (graph, _) = GInstance::from_finstance(source).expect("relational graph form");
        let mut graph_lens = GraphEditLens::new(lens, graph).expect("graph get of the graph form");
        let read = |graph: &GInstance, schema: &Schema| {
            graph
                .try_to_finstance(schema)
                .expect("graph route states are tables")
        };
        let (_, mut source_embedding) = read(graph_lens.source(), &src_schema);
        let (_, mut view_embedding) = read(graph_lens.view(), &tgt_schema);
        let embedding: &mut TableEmbedding = if put {
            &mut view_embedding
        } else {
            &mut source_embedding
        };
        let graph_edit = edit
            .to_graph_delta(embedding)
            .map_err(|_| "apply".to_owned())?;
        let (_, mut before) = if put {
            read(graph_lens.source(), &src_schema)
        } else {
            read(graph_lens.view(), &tgt_schema)
        };
        let step = if put {
            graph_lens.put_edit(&graph_edit)
        } else {
            graph_lens.get_edit(&graph_edit)
        }
        .map_err(|error| production_refusal(&error))?;
        let translated = if put {
            &step.source_delta
        } else {
            &step.view_delta
        };
        let read_back = TableDelta::from_graph_delta(translated, &mut before)
            .expect("a translated delta between table states is a table delta");
        Ok((
            read_back,
            read(graph_lens.source(), &src_schema).0,
            read(graph_lens.view(), &tgt_schema).0,
        ))
    }

    #[test]
    fn graph_and_table_edit_lenses_commute_on_the_relational_carrier() {
        let mut compared = 0_usize;
        for_each_source(|reference_source, source| {
            for kind in MigrationKind::ALL {
                if !kind.commutes_with_protocol_free_graph() {
                    continue;
                }
                let protocol = protocol();
                let table_lens =
                    TableEditLens::new(lens(kind, &protocol), source.clone(), context())
                        .expect("table get accepts every relational source");
                let view = reference_table(table_lens.view());
                for (put, letters) in [
                    (false, letters(None, reference_source)),
                    (true, letters(Some(kind), &view)),
                ] {
                    for letter in letters {
                        compared += 1;
                        let edit = production_delta(&letter);
                        let mut table_route = table_lens.clone();
                        let table = if put {
                            table_route.put_edit(&edit)
                        } else {
                            table_route.get_edit(&edit)
                        }
                        .map(|step| {
                            (
                                if put {
                                    step.source_delta
                                } else {
                                    step.view_delta
                                },
                                table_route.source().clone(),
                                table_route.view().clone(),
                            )
                        })
                        .map_err(|error| production_refusal(&error));
                        let graph = graph_route(kind, source, put, &edit);
                        assert_eq!(
                            graph,
                            table,
                            "{kind:?} {} of {letter:?} from {reference_source:?}",
                            if put { "put" } else { "get" }
                        );
                    }
                }
            }
        });
        println!("graph and table edit lens square: compared={compared}");
        assert!(compared > 0);
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(128))]

        #[test]
        fn randomized_table_edit_lens_differential(
            a_rows in prop::collection::vec((0_u8..8, prop::option::of(0_i64..2)), 0..5),
            b_rows in prop::collection::vec((0_u8..8, prop::option::of(0_i64..2)), 0..5),
            pair_mask in any::<u32>(),
            kind in 0_usize..MIGRATIONS,
            word in prop::collection::vec((any::<bool>(), any::<usize>()), 1..6),
        ) {
            let kind = MigrationKind::ALL[kind];
            let source = generated_source(&a_rows, &b_rows, pair_mask);
            let reference_source = reference_table(&source);
            let protocol = protocol();
            let mut production = TableEditLens::new(lens(kind, &protocol), source, context())
                .expect("table get accepts every generated source");
            let mut state = TableEditState {
                view: relational_table_get(kind.reference(), &reference_source),
                source: reference_source,
            };
            for (put, choice) in &word {
                let letters = if *put {
                    letters(Some(kind), &state.view)
                } else {
                    letters(None, &state.source)
                };
                if letters.is_empty() {
                    continue;
                }
                let letter = &letters[choice % letters.len()];
                if let Some((advanced, next)) = step(kind, &production, &state, *put, letter) {
                    production = advanced;
                    state = next;
                }
            }
        }
    }
}
