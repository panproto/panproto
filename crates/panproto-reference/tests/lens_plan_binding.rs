//! Independent witnesses that compiled lens plans bind the full migration.

use std::collections::{BTreeMap, BTreeSet};

use panproto_reference::lens::GraphLens;
use panproto_reference::migration::Migration;
use panproto_reference::model::SEdge;
use panproto_reference::rel;
use panproto_reference::restrict::{self, NoValueLayer, PlanError};
use panproto_reference::schema::Schema;
use panproto_reference::table::{self, TableBoundaries, TableLens, TablePlanError};

struct OracleWriter(Vec<u8>);

impl OracleWriter {
    const fn new() -> Self {
        Self(Vec::new())
    }

    fn tag(&mut self, value: u8) {
        self.0.push(value);
    }

    fn len(&mut self, value: usize) {
        self.0
            .extend_from_slice(&u64::try_from(value).unwrap_or(u64::MAX).to_le_bytes());
    }

    fn bytes(&mut self, value: &[u8]) {
        self.len(value.len());
        self.0.extend_from_slice(value);
    }

    fn name(&mut self, value: &str) {
        self.bytes(value.as_bytes());
    }

    fn edge(&mut self, edge: &SEdge) {
        self.name(&edge.src);
        self.name(&edge.tgt);
        self.name(&edge.kind);
        match edge.name.as_deref() {
            None => self.tag(0),
            Some(name) => {
                self.tag(1);
                self.name(name);
            }
        }
    }
}

/// An independent copy of the documented private plan-stamp layout.
fn expected_migration_bytes(migration: &Migration) -> Vec<u8> {
    let mut writer = OracleWriter::new();
    writer.bytes(b"panproto-reference/restrict-plan-migration/v1");

    writer.tag(0);
    writer.len(migration.surviving_verts.len());
    for vertex in &migration.surviving_verts {
        writer.name(vertex);
    }

    writer.tag(1);
    writer.len(migration.surviving_edges.len());
    for edge in &migration.surviving_edges {
        writer.edge(edge);
    }

    writer.tag(2);
    writer.len(migration.vertex_remap.len());
    for (source, target) in &migration.vertex_remap {
        writer.name(source);
        writer.name(target);
    }

    writer.tag(3);
    writer.len(migration.edge_remap.len());
    for (source, target) in &migration.edge_remap {
        writer.edge(source);
        writer.edge(target);
    }

    writer.tag(4);
    writer.len(migration.resolver.len());
    for ((source, target), edge) in &migration.resolver {
        writer.name(source);
        writer.name(target);
        writer.edge(edge);
    }

    writer.tag(5);
    writer.len(migration.expansion_path.len());
    for ((source, target), intermediates) in &migration.expansion_path {
        writer.name(source);
        writer.name(target);
        writer.len(intermediates.len());
        for intermediate in intermediates {
            writer.name(intermediate);
        }
    }

    writer.tag(6);
    writer.len(migration.hyper_resolver.len());
    for ((source, labels), (target, label_remap)) in &migration.hyper_resolver {
        writer.name(source);
        writer.len(labels.len());
        for label in labels {
            writer.name(label);
        }
        writer.name(target);
        writer.len(label_remap.len());
        for (source_label, target_label) in label_remap {
            writer.name(source_label);
            writer.name(target_label);
        }
    }

    writer.tag(7);
    writer.len(migration.conditional.len());
    for vertex in &migration.conditional {
        writer.name(vertex);
    }

    writer.tag(8);
    writer.len(migration.transformed.len());
    for vertex in &migration.transformed {
        writer.name(vertex);
    }

    writer.0
}

fn fixture() -> (Schema, Migration, SEdge) {
    let edge = SEdge::new("table", "table.value", "prop", Some("value"));
    let schema = Schema::default()
        .vertex("table", "record")
        .vertex("table.value", "int")
        .edge("table", "table.value", "prop", Some("value"));
    let migration = Migration {
        surviving_verts: schema.vertices.keys().cloned().collect(),
        surviving_edges: schema.edges.clone(),
        ..Migration::default()
    };
    (schema, migration, edge)
}

fn one_mutation_per_field(base: &Migration, edge: &SEdge) -> Vec<(&'static str, Migration)> {
    let mut mutations = Vec::new();

    let mut migration = base.clone();
    migration.surviving_verts.remove("table.value");
    mutations.push(("surviving_verts", migration));

    let mut migration = base.clone();
    migration.surviving_edges.remove(edge);
    mutations.push(("surviving_edges", migration));

    let mut migration = base.clone();
    migration
        .vertex_remap
        .insert("table".to_owned(), "table.value".to_owned());
    mutations.push(("vertex_remap", migration));

    let mut migration = base.clone();
    migration.edge_remap.insert(edge.clone(), edge.clone());
    mutations.push(("edge_remap", migration));

    let mut migration = base.clone();
    migration
        .resolver
        .insert(("table".to_owned(), "table.value".to_owned()), edge.clone());
    mutations.push(("resolver", migration));

    let mut migration = base.clone();
    migration.expansion_path.insert(
        ("table".to_owned(), "table.value".to_owned()),
        vec!["middle".to_owned()],
    );
    mutations.push(("expansion_path", migration));

    let mut migration = base.clone();
    migration.hyper_resolver.insert(
        (
            "source-fan".to_owned(),
            BTreeSet::from(["child".to_owned()]),
        ),
        (
            "target-fan".to_owned(),
            BTreeMap::from([("child".to_owned(), "renamed-child".to_owned())]),
        ),
    );
    mutations.push(("hyper_resolver", migration));

    let mut migration = base.clone();
    migration.conditional.insert("table".to_owned());
    mutations.push(("conditional", migration));

    let mut migration = base.clone();
    migration.transformed.insert("table".to_owned());
    mutations.push(("transformed", migration));

    mutations
}

#[test]
fn graph_plan_stamp_covers_every_migration_field() {
    let (schema, migration, edge) = fixture();
    let plan = match restrict::compile(&schema, &schema, &migration) {
        Ok(plan) => plan,
        Err(error) => panic!("fixture plan must compile: {error:?}"),
    };
    let cloned = migration.clone();
    assert!(
        GraphLens::new(
            &schema,
            &schema,
            &cloned,
            &plan,
            &BTreeMap::new(),
            &NoValueLayer,
        )
        .is_ok(),
        "a semantically identical clone must retain the plan binding"
    );

    let expected = expected_migration_bytes(&migration);
    let mutations = one_mutation_per_field(&migration, &edge);
    assert_eq!(mutations.len(), 9, "the witness must cover all fields");
    for (field, changed) in mutations {
        let found = expected_migration_bytes(&changed);
        assert_ne!(expected, found, "the independent oracle missed {field}");
        assert_eq!(
            GraphLens::new(
                &schema,
                &schema,
                &changed,
                &plan,
                &BTreeMap::new(),
                &NoValueLayer,
            )
            .err(),
            Some(PlanError::MigrationMismatch {
                expected: expected.clone(),
                found,
            }),
            "the graph constructor accepted a changed {field}"
        );
    }
}

#[test]
fn table_plan_refuses_a_different_migration_before_lens_use() {
    let (schema, migration, _) = fixture();
    let value_kinds = BTreeSet::from(["int".to_owned()]);
    let table_kinds = BTreeSet::from(["record".to_owned()]);
    let relational = match rel::classify(&schema, &value_kinds, &table_kinds) {
        Ok(relational) => relational,
        Err(error) => panic!("fixture schema must classify: {error:?}"),
    };
    let plan =
        match table::compile_for_tables(&schema, &schema, &relational, &relational, &migration) {
            Ok(plan) => plan,
            Err(error) => panic!("fixture table plan must compile: {error:?}"),
        };
    let cloned = migration.clone();
    assert!(
        TableLens::new(
            &schema,
            &schema,
            TableBoundaries::new(&relational, &relational),
            &cloned,
            &plan,
            &BTreeMap::new(),
            &NoValueLayer,
        )
        .is_ok(),
        "a semantically identical clone must retain the table-plan binding"
    );

    let mut changed = migration.clone();
    changed.transformed.insert("table".to_owned());
    let expected = expected_migration_bytes(&migration);
    let found = expected_migration_bytes(&changed);
    assert_ne!(expected, found);
    assert_eq!(
        TableLens::new(
            &schema,
            &schema,
            TableBoundaries::new(&relational, &relational),
            &changed,
            &plan,
            &BTreeMap::new(),
            &NoValueLayer,
        )
        .err(),
        Some(TablePlanError::Plan(Box::new(
            PlanError::MigrationMismatch { expected, found }
        ))),
        "the table constructor accepted a plan compiled under another migration"
    );
}
