//! Mutation-directed witnesses for the T.3/T.4 table-lens contract.

use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{Debug, Write};

use panproto_reference::enumerate::lenses::{
    derived_writer_count_certificate, for_each_derived_writer_fixture,
    for_each_table_complement_construction, last_explicit_writer_count_certificate,
    table_complement_construction_count_certificate,
};
use panproto_reference::fan::FanViolation;
use panproto_reference::keys::{KeyEquality, SqlCollation, SqlPad};
use panproto_reference::lens::{PutDomainError, ViewRef};
use panproto_reference::migration::Migration;
use panproto_reference::model::{Graph, Node, SEdge, Table};
use panproto_reference::pages::representation::canonical::{
    self, ComplementShape, IdentityEnvelope,
};
use panproto_reference::rel::{self, RelSchema};
use panproto_reference::restrict::{
    LayerRefusal, NoValueLayer, PlanError, PredicateRefusal, PutFieldRefusal, RestrictError,
    ValueLayer, ValueLayerSemantics,
};
use panproto_reference::schema::{Key, KeyPart, KeyRole, KeyScope, Schema};
use panproto_reference::table::{
    self, CellFate, DroppedRow, IdentityError, IdentityPolicy, IdentityStatus, RowIdsExhausted,
    TableBoundaries, TableCheckError, TableComplement, TableLens, TableOperationalError, TablePlan,
    TablePlanError, TablePutError,
};
use panproto_reference::validation::Semantics;
use panproto_reference::value::{Presence, Value};

fn must<T, E: Debug>(result: Result<T, E>) -> T {
    match result {
        Ok(value) => value,
        Err(error) => panic!("unexpected error: {error:?}"),
    }
}

fn record_boundary(schema: &Schema) -> RelSchema {
    must(rel::classify(
        schema,
        &BTreeSet::from(["int".to_owned(), "str".to_owned()]),
        &BTreeSet::from(["record".to_owned()]),
    ))
}

fn identity_migration(schema: &Schema) -> Migration {
    Migration {
        surviving_verts: schema.vertices.keys().cloned().collect(),
        surviving_edges: schema.edges.clone(),
        ..Migration::default()
    }
}

fn table_plan(
    source: &Schema,
    target: &Schema,
    migration: &Migration,
) -> (RelSchema, RelSchema, TablePlan) {
    let source_boundary = record_boundary(source);
    let target_boundary = record_boundary(target);
    let plan = must(table::compile_for_tables(
        source,
        target,
        &source_boundary,
        &target_boundary,
        migration,
    ));
    (source_boundary, target_boundary, plan)
}

fn raw_writer(fates: &[CellFate], target: &str) -> Option<String> {
    if let Some(source) = fates.iter().rev().find_map(|fate| match fate {
        CellFate::Rename(source, found) if found == target => Some(source),
        CellFate::Drop(_) | CellFate::Rename(_, _) => None,
    }) {
        return Some(source.clone());
    }
    let displaced = fates.iter().any(|fate| match fate {
        CellFate::Drop(source) | CellFate::Rename(source, _) => source == target,
    });
    (!displaced).then(|| target.to_owned())
}

fn fates_for_t(plan: &TablePlan) -> &[CellFate] {
    plan.fates()
        .get("t")
        .map_or_else(|| panic!("fixture plan has no table t"), Vec::as_slice)
}

fn two_column_schema(columns: &[(&str, &str)]) -> Schema {
    let mut schema = Schema::default().vertex("t", "record");
    for (column, vertex) in columns {
        schema = schema
            .vertex(vertex, "int")
            .edge("t", vertex, "prop", Some(column));
    }
    schema
}

fn fate_plan(
    source_columns: &[(&str, &str)],
    target_columns: &[(&str, &str)],
    images: &[(&str, &str, &str, &str)],
) -> TablePlan {
    let source = two_column_schema(source_columns);
    let target = two_column_schema(target_columns);
    let edge_remap = images
        .iter()
        .map(
            |(source_column, source_vertex, target_column, target_vertex)| {
                (
                    SEdge::new("t", source_vertex, "prop", Some(source_column)),
                    SEdge::new("t", target_vertex, "prop", Some(target_column)),
                )
            },
        )
        .collect();
    let vertex_remap = images
        .iter()
        .map(|(_, source_vertex, _, target_vertex)| {
            ((*source_vertex).to_owned(), (*target_vertex).to_owned())
        })
        .collect();
    let migration = Migration {
        surviving_verts: target.vertices.keys().cloned().collect(),
        vertex_remap,
        edge_remap,
        ..Migration::default()
    };
    table_plan(&source, &target, &migration).2
}

#[test]
fn raw_fate_vectors_determine_last_writer_without_calling_writer_as_oracle() {
    let swap = fate_plan(
        &[("a", "sa"), ("b", "sb")],
        &[("a", "ta"), ("b", "tb")],
        &[("a", "sa", "b", "tb"), ("b", "sb", "a", "ta")],
    );
    let swap_fates = fates_for_t(&swap);
    assert_eq!(raw_writer(swap_fates, "a"), Some("b".to_owned()));
    assert_eq!(raw_writer(swap_fates, "b"), Some("a".to_owned()));
    assert_eq!(swap.writer("t", "a"), raw_writer(swap_fates, "a"));
    assert_eq!(swap.writer("t", "b"), raw_writer(swap_fates, "b"));

    let many = fate_plan(
        &[("a", "sa"), ("b", "sb")],
        &[("x", "tx")],
        &[("a", "sa", "x", "tx"), ("b", "sb", "x", "tx")],
    );
    let many_fates = fates_for_t(&many);
    assert_eq!(raw_writer(many_fates, "x"), Some("b".to_owned()));
    assert_eq!(many.writer("t", "x"), raw_writer(many_fates, "x"));

    let collision = fate_plan(
        &[("a", "sa"), ("b", "sb")],
        &[("b", "tb")],
        &[("a", "sa", "b", "tb"), ("b", "sb", "b", "tb")],
    );
    let collision_fates = fates_for_t(&collision);
    assert_eq!(raw_writer(collision_fates, "b"), Some("a".to_owned()));
    assert_eq!(collision.writer("t", "b"), raw_writer(collision_fates, "b"));
}

fn two_to_one_fixture(
    target_column: &str,
    second_target_column: &str,
) -> (Schema, Schema, Migration, TablePlan) {
    let source = two_column_schema(&[("a", "sa"), ("b", "sb")]);
    let target = two_column_schema(&[(target_column, "tx")]);
    let migration = Migration {
        surviving_verts: target.vertices.keys().cloned().collect(),
        vertex_remap: [
            ("sa".to_owned(), "tx".to_owned()),
            ("sb".to_owned(), "tx".to_owned()),
        ]
        .into(),
        edge_remap: [
            (
                SEdge::new("t", "sa", "prop", Some("a")),
                SEdge::new("t", "tx", "prop", Some(target_column)),
            ),
            (
                SEdge::new("t", "sb", "prop", Some("b")),
                SEdge::new("t", "tx", "prop", Some(second_target_column)),
            ),
        ]
        .into(),
        ..Migration::default()
    };
    let plan = table_plan(&source, &target, &migration).2;
    (source, target, migration, plan)
}

#[derive(Default)]
struct OracleWriter {
    bytes: Vec<u8>,
}

impl OracleWriter {
    fn tag(&mut self, value: u8) {
        self.bytes.push(value);
    }

    fn len(&mut self, value: usize) {
        self.bytes
            .extend_from_slice(&u64::try_from(value).unwrap_or(u64::MAX).to_le_bytes());
    }

    fn raw(&mut self, value: &[u8]) {
        self.len(value.len());
        self.bytes.extend_from_slice(value);
    }

    fn u16(&mut self, value: u16) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn str(&mut self, value: &str) {
        self.raw(value.as_bytes());
    }

    fn edge(&mut self, edge: &SEdge) {
        self.str(&edge.src);
        self.str(&edge.tgt);
        self.str(&edge.kind);
        match edge.name.as_deref() {
            None => self.tag(0),
            Some(name) => {
                self.tag(1);
                self.str(name);
            }
        }
    }
}

#[test]
fn last_writer_is_the_end_to_end_put_writer_for_many_to_one_and_unfated_collisions() {
    let source_instance = Table {
        rows: [(
            1,
            (
                "t".to_owned(),
                [
                    ("a".to_owned(), Value::Int(1)),
                    ("b".to_owned(), Value::Int(2)),
                ]
                .into(),
            ),
        )]
        .into(),
        ..Table::default()
    };

    let (source, target, migration, plan) = two_to_one_fixture("x", "x");
    let source_boundary = record_boundary(&source);
    let target_boundary = record_boundary(&target);
    let lens = must(TableLens::new_at_boundary(
        &source,
        &target,
        TableBoundaries::new(&source_boundary, &target_boundary),
        &migration,
        &plan,
        &BTreeMap::new(),
        &NoValueLayer,
    ));
    let (mut view, complement) = must(lens.get(&source_instance));
    assert_eq!(
        view.rows[&1].1,
        BTreeMap::from([("x".to_owned(), Value::Int(2))])
    );
    if let Some((_, row)) = view.rows.get_mut(&1) {
        row.insert("x".to_owned(), Value::Int(9));
    } else {
        panic!("fixture view has no row 1");
    }
    let restored = must(table::put(&lens, &view, &complement));
    assert_eq!(
        restored.rows[&1].1,
        BTreeMap::from([
            ("a".to_owned(), Value::Int(1)),
            ("b".to_owned(), Value::Int(9)),
        ])
    );

    let (source, target, migration, plan) = two_to_one_fixture("b", "b");
    let source_boundary = record_boundary(&source);
    let target_boundary = record_boundary(&target);
    let lens = must(TableLens::new_at_boundary(
        &source,
        &target,
        TableBoundaries::new(&source_boundary, &target_boundary),
        &migration,
        &plan,
        &BTreeMap::new(),
        &NoValueLayer,
    ));
    let (mut view, complement) = must(lens.get(&source_instance));
    assert_eq!(
        view.rows[&1].1,
        BTreeMap::from([("b".to_owned(), Value::Int(1))])
    );
    if let Some((_, row)) = view.rows.get_mut(&1) {
        row.insert("b".to_owned(), Value::Int(9));
    } else {
        panic!("fixture view has no row 1");
    }
    let restored = must(table::put(&lens, &view, &complement));
    assert_eq!(
        restored.rows[&1].1,
        BTreeMap::from([
            ("a".to_owned(), Value::Int(9)),
            ("b".to_owned(), Value::Int(2)),
        ])
    );
}

fn oracle_migration_bytes(migration: &Migration) -> Vec<u8> {
    let mut writer = OracleWriter::default();
    writer.raw(b"panproto-reference/restrict-plan-migration/v1");
    writer.tag(0);
    writer.len(migration.surviving_verts.len());
    for vertex in &migration.surviving_verts {
        writer.str(vertex);
    }
    writer.tag(1);
    writer.len(migration.surviving_edges.len());
    for edge in &migration.surviving_edges {
        writer.edge(edge);
    }
    writer.tag(2);
    writer.len(migration.vertex_remap.len());
    for (source, target) in &migration.vertex_remap {
        writer.str(source);
        writer.str(target);
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
        writer.str(source);
        writer.str(target);
        writer.edge(edge);
    }
    writer.tag(5);
    writer.len(migration.expansion_path.len());
    for ((source, target), path) in &migration.expansion_path {
        writer.str(source);
        writer.str(target);
        writer.len(path.len());
        for intermediate in path {
            writer.str(intermediate);
        }
    }
    writer.tag(6);
    writer.len(migration.hyper_resolver.len());
    for ((source, labels), (target, remap)) in &migration.hyper_resolver {
        writer.str(source);
        writer.len(labels.len());
        for label in labels {
            writer.str(label);
        }
        writer.str(target);
        writer.len(remap.len());
        for (source_label, target_label) in remap {
            writer.str(source_label);
            writer.str(target_label);
        }
    }
    writer.tag(7);
    writer.len(migration.conditional.len());
    for vertex in &migration.conditional {
        writer.str(vertex);
    }
    writer.tag(8);
    writer.len(migration.transformed.len());
    for vertex in &migration.transformed {
        writer.str(vertex);
    }
    writer.bytes
}

fn oracle_rel_schema_bytes(schema: &RelSchema) -> Vec<u8> {
    let mut writer = OracleWriter::default();
    writer.raw(b"panproto-reference/relation-schema/v1");
    writer.tag(0);
    writer.len(schema.tables.len());
    for table in &schema.tables {
        writer.str(table);
    }
    writer.tag(1);
    writer.len(schema.relations.len());
    for relation in &schema.relations {
        writer.edge(relation);
    }
    writer.tag(2);
    writer.len(schema.columns.len());
    for (table, columns) in &schema.columns {
        writer.str(table);
        writer.len(columns.len());
        for (column, arrow) in columns {
            writer.str(column);
            writer.edge(arrow);
        }
    }
    writer.tag(3);
    writer.len(schema.value_vertices.len());
    for vertex in &schema.value_vertices {
        writer.str(vertex);
    }
    writer.tag(4);
    writer.len(schema.inner_edges.len());
    for edge in &schema.inner_edges {
        writer.edge(edge);
    }
    writer.tag(5);
    writer.len(schema.kinds.len());
    for (vertex, kind) in &schema.kinds {
        writer.str(vertex);
        writer.str(kind);
    }
    writer.bytes
}

fn oracle_value_layer_bytes(semantics: &ValueLayerSemantics) -> Vec<u8> {
    let mut writer = OracleWriter::default();
    writer.tag(0);
    writer.str(semantics.domain);
    writer.tag(1);
    writer.u16(semantics.version);
    writer.tag(2);
    writer.raw(&semantics.canonical_payload);
    writer.bytes
}

struct DescribedIdentity {
    domain: &'static str,
    version: u16,
    payload: &'static [u8],
    semantic_calls: Cell<u32>,
}

impl DescribedIdentity {
    const fn new(domain: &'static str, version: u16, payload: &'static [u8]) -> Self {
        Self {
            domain,
            version,
            payload,
            semantic_calls: Cell::new(0),
        }
    }
}

impl ValueLayer for DescribedIdentity {
    fn canonical_semantics(&self) -> ValueLayerSemantics {
        self.semantic_calls
            .set(self.semantic_calls.get().saturating_add(1));
        ValueLayerSemantics {
            domain: self.domain,
            version: self.version,
            canonical_payload: self.payload.to_vec(),
        }
    }

    fn survives(&self, _: &Graph, _: &Node) -> Result<bool, PredicateRefusal> {
        Ok(true)
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

struct RefusingBaselineTransform;

impl ValueLayer for RefusingBaselineTransform {
    fn canonical_semantics(&self) -> ValueLayerSemantics {
        ValueLayerSemantics {
            domain: "panproto-reference/test/refusing-baseline-transform",
            version: 1,
            canonical_payload: b"kind=test-refusal\0detail=baseline".to_vec(),
        }
    }

    fn survives(&self, _: &Graph, _: &Node) -> Result<bool, PredicateRefusal> {
        Ok(true)
    }

    fn transform(
        &self,
        _: &Graph,
        _: &Node,
    ) -> Result<(BTreeMap<String, Value>, Option<Presence>), LayerRefusal> {
        Err(LayerRefusal {
            kind: "test-refusal".to_owned(),
            detail: "baseline".to_owned(),
        })
    }

    fn restore(
        &self,
        view: &Node,
        _: Option<&BTreeMap<String, Value>>,
        _: Option<&Option<Presence>>,
    ) -> Result<(BTreeMap<String, Value>, Option<Presence>), PutFieldRefusal> {
        Ok((view.fields.clone(), view.value.clone()))
    }
}

#[derive(Clone, Copy, Debug)]
enum CandidateRestore {
    String,
    Refuse,
    NonString,
}

struct StringIntCandidateLayer(CandidateRestore);

impl ValueLayer for StringIntCandidateLayer {
    fn canonical_semantics(&self) -> ValueLayerSemantics {
        let mode = match self.0 {
            CandidateRestore::String => b"string".as_slice(),
            CandidateRestore::Refuse => b"refuse".as_slice(),
            CandidateRestore::NonString => b"non-string".as_slice(),
        };
        ValueLayerSemantics {
            domain: "panproto-reference/test/string-int-candidate",
            version: 1,
            canonical_payload: mode.to_vec(),
        }
    }

    fn survives(&self, _: &Graph, _: &Node) -> Result<bool, PredicateRefusal> {
        Ok(true)
    }

    fn transform(
        &self,
        _: &Graph,
        node: &Node,
    ) -> Result<(BTreeMap<String, Value>, Option<Presence>), LayerRefusal> {
        let mut fields = node.fields.clone();
        if node.anchor == "a"
            && let Some(Value::Str(value)) = fields.get("key")
        {
            let parsed = value.parse::<i64>().map_err(|_| LayerRefusal {
                kind: "invalid-test-integer".to_owned(),
                detail: value.clone(),
            })?;
            fields.insert("key".to_owned(), Value::Int(parsed));
        }
        Ok((fields, node.value.clone()))
    }

    fn restore(
        &self,
        view: &Node,
        _: Option<&BTreeMap<String, Value>>,
        _: Option<&Option<Presence>>,
    ) -> Result<(BTreeMap<String, Value>, Option<Presence>), PutFieldRefusal> {
        if view.anchor != "a" {
            return Ok((view.fields.clone(), view.value.clone()));
        }
        let Some(Value::Int(value)) = view.fields.get("key") else {
            return Err(PutFieldRefusal::TransformNotInvertible {
                field: "key".to_owned(),
            });
        };
        match self.0 {
            CandidateRestore::String => Ok((
                BTreeMap::from([("key".to_owned(), Value::Str(value.to_string()))]),
                view.value.clone(),
            )),
            CandidateRestore::Refuse => Err(PutFieldRefusal::TransformNotInvertible {
                field: "key".to_owned(),
            }),
            CandidateRestore::NonString => Ok((
                BTreeMap::from([("key".to_owned(), Value::Int(*value))]),
                view.value.clone(),
            )),
        }
    }
}

fn construct_identity(
    source: &Schema,
    target: &Schema,
    source_boundary: &RelSchema,
    target_boundary: &RelSchema,
    migration: &Migration,
    layer: &dyn ValueLayer,
) -> IdentityEnvelope {
    let plan = must(table::compile_for_tables(
        source,
        target,
        source_boundary,
        target_boundary,
        migration,
    ));
    *must(TableLens::new(
        source,
        target,
        TableBoundaries::new(source_boundary, target_boundary),
        migration,
        &plan,
        &BTreeMap::new(),
        layer,
    ))
    .identity()
}

fn construct_coordinate_identity(
    source: &Schema,
    target: &Schema,
    source_boundary: &RelSchema,
    target_boundary: &RelSchema,
    migration: &Migration,
    layer: &dyn ValueLayer,
) -> IdentityEnvelope {
    construct_identity(
        source,
        target,
        source_boundary,
        target_boundary,
        migration,
        layer,
    )
}

fn rel_schema_coordinate_mutations(base: &RelSchema) -> Vec<(&'static str, RelSchema)> {
    let relation = SEdge::new("phantom-parent", "phantom-child", "relation", Some("r"));
    let column = SEdge::new("phantom-table", "phantom-value", "prop", Some("c"));
    let inner = SEdge::new("phantom-value", "phantom-inner", "inner", Some("i"));
    let mut mutations = Vec::new();

    let mut changed = base.clone();
    changed.tables.insert("phantom-table".to_owned());
    mutations.push(("tables", changed));

    let mut changed = base.clone();
    changed.relations.insert(relation);
    mutations.push(("relations", changed));

    let mut changed = base.clone();
    changed.columns.insert(
        "phantom-table".to_owned(),
        BTreeMap::from([("c".to_owned(), column)]),
    );
    mutations.push(("columns", changed));

    let mut changed = base.clone();
    changed.value_vertices.insert("phantom-value".to_owned());
    mutations.push(("value_vertices", changed));

    let mut changed = base.clone();
    changed.inner_edges.insert(inner);
    mutations.push(("inner_edges", changed));

    let mut changed = base.clone();
    changed
        .kinds
        .insert("phantom".to_owned(), "kind".to_owned());
    mutations.push(("kinds", changed));

    mutations
}

fn migration_coordinate_mutations(base: &Migration) -> Vec<(&'static str, Migration)> {
    let source_edge = SEdge::new("source", "target", "edge", Some("source"));
    let target_edge = SEdge::new("image", "image-target", "edge", Some("target"));
    let mut mutations = Vec::new();

    let mut changed = base.clone();
    changed.surviving_verts.insert("survivor".to_owned());
    mutations.push(("surviving_verts", changed));

    let mut changed = base.clone();
    changed.surviving_edges.insert(source_edge.clone());
    mutations.push(("surviving_edges", changed));

    let mut changed = base.clone();
    changed
        .vertex_remap
        .insert("source".to_owned(), "image".to_owned());
    mutations.push(("vertex_remap", changed));

    let mut changed = base.clone();
    changed.edge_remap.insert(source_edge, target_edge.clone());
    mutations.push(("edge_remap", changed));

    let mut changed = base.clone();
    changed
        .resolver
        .insert(("source".to_owned(), "target".to_owned()), target_edge);
    mutations.push(("resolver", changed));

    let mut changed = base.clone();
    changed.expansion_path.insert(
        ("source".to_owned(), "target".to_owned()),
        vec!["middle".to_owned()],
    );
    mutations.push(("expansion_path", changed));

    let mut changed = base.clone();
    changed.conditional.insert("conditional".to_owned());
    mutations.push(("conditional", changed));

    let mut changed = base.clone();
    changed.transformed.insert("transformed".to_owned());
    mutations.push(("transformed", changed));

    mutations
}

fn hex(bytes: impl AsRef<[u8]>) -> String {
    let bytes = bytes.as_ref();
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut hex, byte| {
            let _ = write!(hex, "{byte:02x}");
            hex
        })
}

#[test]
fn table_identity_uses_the_exact_domain_separated_envelope_layout() {
    let source = two_column_schema(&[("a", "sa"), ("b", "sb")]);
    let target = two_column_schema(&[("a", "ta"), ("b", "tb")]);
    let migration = Migration {
        surviving_verts: target.vertices.keys().cloned().collect(),
        vertex_remap: [
            ("sa".to_owned(), "tb".to_owned()),
            ("sb".to_owned(), "ta".to_owned()),
        ]
        .into(),
        edge_remap: [
            (
                SEdge::new("t", "sa", "prop", Some("a")),
                SEdge::new("t", "tb", "prop", Some("b")),
            ),
            (
                SEdge::new("t", "sb", "prop", Some("b")),
                SEdge::new("t", "ta", "prop", Some("a")),
            ),
        ]
        .into(),
        ..Migration::default()
    };

    let (source_boundary, target_boundary, plan) = table_plan(&source, &target, &migration);
    assert_eq!(
        oracle_rel_schema_bytes(&source_boundary),
        table::canonical_rel_schema_bytes(&source_boundary)
    );
    assert_eq!(
        oracle_rel_schema_bytes(&target_boundary),
        table::canonical_rel_schema_bytes(&target_boundary)
    );
    assert_eq!(
        oracle_migration_bytes(&migration),
        table::canonical_migration_bytes(&migration)
    );

    // Captured from the external blake3 1.8.7 implementation before its
    // removal. The literal envelope is independent of the encoding traversal
    // checks above and pins each domain-separated 32-byte digest.
    let expected = IdentityEnvelope {
        source_schema: [
            0x14, 0x8e, 0x79, 0xa3, 0xf2, 0xc0, 0x67, 0xd4, 0x9f, 0x39, 0x5e, 0x36, 0xe2, 0x01,
            0xb9, 0x35, 0x46, 0x15, 0xf1, 0x6e, 0x4f, 0xfe, 0xbc, 0xd6, 0xd3, 0xd6, 0xef, 0x9d,
            0xb0, 0xd2, 0xef, 0xf7,
        ],
        target_schema: [
            0x4b, 0x04, 0xe5, 0x4b, 0x12, 0xb7, 0x5a, 0x98, 0x37, 0x59, 0xc7, 0xac, 0xda, 0x1c,
            0xfa, 0x3f, 0x14, 0x8e, 0xbb, 0x78, 0x78, 0xff, 0xee, 0x12, 0xbd, 0xbd, 0x3b, 0x4d,
            0xac, 0x82, 0xc3, 0x48,
        ],
        lens: [
            0x97, 0x70, 0x43, 0xcc, 0xd3, 0x4e, 0xc1, 0x73, 0xc4, 0x0e, 0xbf, 0xf0, 0xd8, 0x24,
            0x23, 0xd8, 0x9d, 0x1e, 0x83, 0xd9, 0x3a, 0xa7, 0xc6, 0xfa, 0x3f, 0x9b, 0xd1, 0x97,
            0xf5, 0x35, 0x6b, 0x8f,
        ],
        shape: ComplementShape::Functor,
        format_version: 1,
    };
    let lens = must(TableLens::new(
        &source,
        &target,
        TableBoundaries::new(&source_boundary, &target_boundary),
        &migration,
        &plan,
        &BTreeMap::new(),
        &NoValueLayer,
    ));
    let actual = *lens.identity();
    assert_eq!(actual, expected);
    assert_eq!(
        hex(canonical::identity(&actual)),
        "0200148e79a3f2c067d49f395e36e201b9354615f16e4ffebcd6d3d6ef9db0d2eff74b04e54b12b75a983759c7acda1cfa3f148ebb7878ffee12bdbd3b4dac82c348977043ccd34ec173c40ebff0d82423d89d1e83d93aa7c6fa3f9bd197f5356b8f020100"
    );
}

#[test]
fn identity_separates_boundaries_migration_and_value_layer_descriptor_fields() {
    let schema = Schema::default().vertex("t", "record");
    let migration = identity_migration(&schema);
    let boundary = record_boundary(&schema);
    let layer = DescribedIdentity::new("identity-domain", 7, b"payload");
    let baseline = construct_identity(&schema, &schema, &boundary, &boundary, &migration, &layer);

    let mut changed_source = boundary.clone();
    changed_source
        .kinds
        .insert("phantom".to_owned(), "source-only".to_owned());
    let source_changed = construct_identity(
        &schema,
        &schema,
        &changed_source,
        &boundary,
        &migration,
        &layer,
    );
    let mut changed_target = boundary.clone();
    changed_target
        .kinds
        .insert("phantom".to_owned(), "target-only".to_owned());
    let target_changed = construct_identity(
        &schema,
        &schema,
        &boundary,
        &changed_target,
        &migration,
        &layer,
    );
    let mut changed_migration = migration.clone();
    changed_migration
        .vertex_remap
        .insert("t".to_owned(), "t".to_owned());
    let migration_changed = construct_identity(
        &schema,
        &schema,
        &boundary,
        &boundary,
        &changed_migration,
        &layer,
    );
    let domain_changed = construct_identity(
        &schema,
        &schema,
        &boundary,
        &boundary,
        &migration,
        &DescribedIdentity::new("identity-domain-2", 7, b"payload"),
    );
    let version_changed = construct_identity(
        &schema,
        &schema,
        &boundary,
        &boundary,
        &migration,
        &DescribedIdentity::new("identity-domain", 8, b"payload"),
    );
    let payload_changed = construct_identity(
        &schema,
        &schema,
        &boundary,
        &boundary,
        &migration,
        &DescribedIdentity::new("identity-domain", 7, b"payload-2"),
    );
    assert_eq!(
        BTreeSet::from([
            baseline.lens,
            source_changed.lens,
            target_changed.lens,
            migration_changed.lens,
            domain_changed.lens,
            version_changed.lens,
            payload_changed.lens,
        ])
        .len(),
        7,
        "each independently varied semantic field must affect identity"
    );
}

#[test]
fn identity_separates_every_rel_schema_and_migration_coordinate_independently() {
    let source = Schema::default();
    let target = Schema::default();
    let boundary = RelSchema::default();
    let migration = Migration::default();
    let layer = NoValueLayer;
    let baseline =
        construct_coordinate_identity(&source, &target, &boundary, &boundary, &migration, &layer);
    let mut lens_digests = BTreeSet::from([baseline.lens]);

    let source_mutations = rel_schema_coordinate_mutations(&boundary);
    assert_eq!(source_mutations.len(), 6);
    for (field, changed_source) in source_mutations {
        let identity = construct_coordinate_identity(
            &source,
            &target,
            &changed_source,
            &boundary,
            &migration,
            &layer,
        );
        assert_ne!(identity, baseline, "source RelSchema::{field} was omitted");
        assert!(
            lens_digests.insert(identity.lens),
            "source RelSchema::{field} collided with another coordinate"
        );
    }

    let target_mutations = rel_schema_coordinate_mutations(&boundary);
    assert_eq!(target_mutations.len(), 6);
    for (field, changed_target) in target_mutations {
        let identity = construct_coordinate_identity(
            &source,
            &target,
            &boundary,
            &changed_target,
            &migration,
            &layer,
        );
        assert_ne!(identity, baseline, "target RelSchema::{field} was omitted");
        assert!(
            lens_digests.insert(identity.lens),
            "target RelSchema::{field} collided with another coordinate"
        );
    }

    let migration_mutations = migration_coordinate_mutations(&migration);
    assert_eq!(migration_mutations.len(), 8);
    for (field, changed_migration) in migration_mutations {
        let identity = construct_coordinate_identity(
            &source,
            &target,
            &boundary,
            &boundary,
            &changed_migration,
            &layer,
        );
        assert_ne!(identity, baseline, "Migration::{field} was omitted");
        assert!(
            lens_digests.insert(identity.lens),
            "Migration::{field} collided with another coordinate"
        );
    }
    assert_eq!(lens_digests.len(), 21);

    // Table plans reject every hyper resolver as non-relational. Prove the
    // otherwise valid migration coordinate separately rather than silently
    // dropping it from the identity inventory.
    let mut hyper = migration.clone();
    hyper.hyper_resolver.insert(
        (
            "source-fan".to_owned(),
            BTreeSet::from(["label".to_owned()]),
        ),
        (
            "target-fan".to_owned(),
            BTreeMap::from([("label".to_owned(), "image-label".to_owned())]),
        ),
    );
    assert!(panproto_reference::restrict::compile(&source, &target, &hyper).is_ok());
    assert_eq!(
        table::compile_for_tables(&source, &target, &boundary, &boundary, &hyper),
        Err(TablePlanError::NotTableRepresentable)
    );
    let hyper_identity = table::table_lens_identity(
        &source,
        &target,
        &boundary,
        &boundary,
        &hyper,
        &layer.canonical_semantics(),
    );
    assert_ne!(
        hyper_identity, baseline,
        "Migration::hyper_resolver was omitted by the independent oracle"
    );
}

#[test]
fn value_layer_descriptor_framing_resists_boundary_shifts() {
    let left = ValueLayerSemantics {
        domain: "a",
        version: 0x6263,
        canonical_payload: b"def".to_vec(),
    };
    let shifted_domain = ValueLayerSemantics {
        domain: "a\0",
        version: 0x6263,
        canonical_payload: b"def".to_vec(),
    };
    let shifted_payload = ValueLayerSemantics {
        domain: "a",
        version: 0x6263,
        canonical_payload: b"\0def".to_vec(),
    };
    assert_ne!(
        oracle_value_layer_bytes(&left),
        oracle_value_layer_bytes(&shifted_domain)
    );
    assert_ne!(
        oracle_value_layer_bytes(&left),
        oracle_value_layer_bytes(&shifted_payload)
    );
    assert_ne!(
        oracle_value_layer_bytes(&shifted_domain),
        oracle_value_layer_bytes(&shifted_payload)
    );
}

#[test]
fn constructors_validate_before_semantics_and_new_at_boundary_is_an_exact_alias() {
    let schema = Schema::default().vertex("t", "record");
    let migration = identity_migration(&schema);
    let (source_boundary, target_boundary, plan) = table_plan(&schema, &schema, &migration);
    let wrong = record_boundary(&Schema::default().vertex("other", "record"));
    let rejected = DescribedIdentity::new("counted", 1, b"rejected");
    assert!(matches!(
        TableLens::new(
            &schema,
            &schema,
            TableBoundaries::new(&wrong, &target_boundary),
            &migration,
            &plan,
            &BTreeMap::new(),
            &rejected,
        ),
        Err(TablePlanError::SourceBoundaryMismatch)
    ));
    assert_eq!(rejected.semantic_calls.get(), 0);

    let target_rejected = DescribedIdentity::new("counted", 1, b"target-rejected");
    assert!(matches!(
        TableLens::new(
            &schema,
            &schema,
            TableBoundaries::new(&source_boundary, &wrong),
            &migration,
            &plan,
            &BTreeMap::new(),
            &target_rejected,
        ),
        Err(TablePlanError::TargetBoundaryMismatch)
    ));
    assert_eq!(target_rejected.semantic_calls.get(), 0);

    let mut changed_migration = migration.clone();
    changed_migration.transformed.insert("t".to_owned());
    let plan_rejected = DescribedIdentity::new("counted", 1, b"plan-rejected");
    assert!(matches!(
        TableLens::new(
            &schema,
            &schema,
            TableBoundaries::new(&source_boundary, &target_boundary),
            &changed_migration,
            &plan,
            &BTreeMap::new(),
            &plan_rejected,
        ),
        Err(TablePlanError::Plan(boxed))
            if matches!(*boxed, PlanError::MigrationMismatch { .. })
    ));
    assert_eq!(plan_rejected.semantic_calls.get(), 0);

    let direct_layer = DescribedIdentity::new("counted", 1, b"accepted");
    let direct = must(TableLens::new(
        &schema,
        &schema,
        TableBoundaries::new(&source_boundary, &target_boundary),
        &migration,
        &plan,
        &BTreeMap::new(),
        &direct_layer,
    ));
    let alias_layer = DescribedIdentity::new("counted", 1, b"accepted");
    let alias = must(TableLens::new_at_boundary(
        &schema,
        &schema,
        TableBoundaries::new(&source_boundary, &target_boundary),
        &migration,
        &plan,
        &BTreeMap::new(),
        &alias_layer,
    ));
    assert_eq!(direct_layer.semantic_calls.get(), 1);
    assert_eq!(alias_layer.semantic_calls.get(), 1);
    assert_eq!(direct.identity(), alias.identity());
}

#[test]
fn raw_and_bound_get_stamp_identically_without_recomputing_bound_semantics() {
    let schema = Schema::default().vertex("t", "record");
    let migration = identity_migration(&schema);
    let (source_boundary, target_boundary, plan) = table_plan(&schema, &schema, &migration);
    let layer = DescribedIdentity::new("counted", 1, b"get");
    let lens = must(TableLens::new(
        &schema,
        &schema,
        TableBoundaries::new(&source_boundary, &target_boundary),
        &migration,
        &plan,
        &BTreeMap::new(),
        &layer,
    ));
    assert_eq!(layer.semantic_calls.get(), 1);
    let (_, bound) = must(lens.get(&Table::default()));
    assert_eq!(layer.semantic_calls.get(), 1);
    let (_, raw) = must(table::get(
        &schema,
        &schema,
        &migration,
        &plan,
        &layer,
        &Table::default(),
    ));
    assert_eq!(layer.semantic_calls.get(), 2);
    assert_eq!(bound.identity, Some(*lens.identity()));
    assert_eq!(raw.identity, bound.identity);
}

#[test]
fn constraint_semantics_are_admission_context_not_identity_material() {
    let schema = Schema::default().vertex("t", "record");
    let migration = identity_migration(&schema);
    let (source_boundary, target_boundary, plan) = table_plan(&schema, &schema, &migration);
    let empty = must(TableLens::new(
        &schema,
        &schema,
        TableBoundaries::new(&source_boundary, &target_boundary),
        &migration,
        &plan,
        &BTreeMap::new(),
        &NoValueLayer,
    ));
    let populated = must(TableLens::new(
        &schema,
        &schema,
        TableBoundaries::new(&source_boundary, &target_boundary),
        &migration,
        &plan,
        &BTreeMap::from([("required".to_owned(), Semantics::RequiredValue)]),
        &NoValueLayer,
    ));
    assert_eq!(empty.identity(), populated.identity());
}

const fn domain_name(error: &PutDomainError) -> &'static str {
    match error {
        PutDomainError::IdCollision { .. } => "IdCollision",
        PutDomainError::SynthesizedEdited { .. } => "SynthesizedEdited",
        PutDomainError::ContractedEdgeInserted { .. } => "ContractedEdgeInserted",
        PutDomainError::UnrepresentableViewEdge { .. } => "UnrepresentableViewEdge",
        PutDomainError::AmbiguousReverseAnchor { .. } => "AmbiguousReverseAnchor",
        PutDomainError::SurvivalPredicateFails { .. } => "SurvivalPredicateFails",
        PutDomainError::DerivedCellEdited { .. } => "DerivedCellEdited",
        PutDomainError::TransformNotInvertible { .. } => "TransformNotInvertible",
        PutDomainError::DuplicateKey { .. } => "DuplicateKey",
        PutDomainError::MissingRequiredCell { .. } => "MissingRequiredCell",
        PutDomainError::MalformedFan { .. } => "MalformedFan",
        PutDomainError::SurvivalPredicateRefused { .. } => "SurvivalPredicateRefused",
        PutDomainError::KeyRefused { .. } => "KeyRefused",
    }
}

const fn refusal_name(refusal: &PredicateRefusal) -> &'static str {
    match refusal {
        PredicateRefusal::AmbiguousChildField { .. } => "AmbiguousChildField",
        PredicateRefusal::StructuralChildNotATree { .. } => "StructuralChildNotATree",
        PredicateRefusal::ValueNotExpressible { .. } => "ValueNotExpressible",
        PredicateRefusal::EvaluationFailed { .. } => "EvaluationFailed",
        PredicateRefusal::NotBoolean { .. } => "NotBoolean",
    }
}

#[test]
fn put_domain_vocabulary_is_exactly_thirteen_variants_with_five_predicate_refusals() {
    let row = ViewRef::Row(1);
    let edge = ViewRef::Pair(Box::new(SEdge::new("a", "b", "fk", Some("b"))), 1, 2);
    let variants = vec![
        PutDomainError::IdCollision { elem: row.clone() },
        PutDomainError::SynthesizedEdited { elem: edge.clone() },
        PutDomainError::ContractedEdgeInserted { elem: edge.clone() },
        PutDomainError::UnrepresentableViewEdge { elem: edge },
        PutDomainError::AmbiguousReverseAnchor {
            elem: row.clone(),
            candidates: vec!["a".to_owned(), "b".to_owned()],
        },
        PutDomainError::SurvivalPredicateFails { elem: row.clone() },
        PutDomainError::DerivedCellEdited {
            elem: row.clone(),
            field: "derived".to_owned(),
        },
        PutDomainError::TransformNotInvertible {
            elem: row.clone(),
            field: "value".to_owned(),
        },
        PutDomainError::DuplicateKey {
            table: "t".to_owned(),
            key: vec![("id".to_owned(), Value::Int(1))],
            elems: vec![row.clone()],
        },
        PutDomainError::MissingRequiredCell {
            elem: row.clone(),
            field: "required".to_owned(),
        },
        PutDomainError::MalformedFan {
            node: 1,
            reason: FanViolation::MissingParent,
        },
        PutDomainError::SurvivalPredicateRefused {
            elem: row,
            refusal: PredicateRefusal::NotBoolean {
                got: "integer".to_owned(),
            },
        },
        PutDomainError::KeyRefused {
            table: "t".to_owned(),
            key: "id".to_owned(),
            reason: panproto_reference::keys::KeyingError::NoCanonicalBytes,
        },
    ];
    assert_eq!(variants.len(), 13);
    assert_eq!(
        variants.iter().map(domain_name).collect::<Vec<_>>(),
        [
            "IdCollision",
            "SynthesizedEdited",
            "ContractedEdgeInserted",
            "UnrepresentableViewEdge",
            "AmbiguousReverseAnchor",
            "SurvivalPredicateFails",
            "DerivedCellEdited",
            "TransformNotInvertible",
            "DuplicateKey",
            "MissingRequiredCell",
            "MalformedFan",
            "SurvivalPredicateRefused",
            "KeyRefused",
        ]
    );
    assert_eq!(
        variants
            .iter()
            .map(|variant| variant.order_key().0)
            .collect::<Vec<_>>(),
        (0..13).collect::<Vec<_>>()
    );

    let predicate_refusals = [
        PredicateRefusal::AmbiguousChildField {
            field: "child".to_owned(),
        },
        PredicateRefusal::StructuralChildNotATree {
            field: "child".to_owned(),
        },
        PredicateRefusal::ValueNotExpressible {
            field: "value".to_owned(),
            value_type: "bytes".to_owned(),
        },
        PredicateRefusal::EvaluationFailed {
            message: "division by zero".to_owned(),
        },
        PredicateRefusal::NotBoolean {
            got: "integer".to_owned(),
        },
    ];
    assert_eq!(predicate_refusals.len(), 5);
    assert_eq!(
        predicate_refusals
            .iter()
            .map(refusal_name)
            .collect::<Vec<_>>(),
        [
            "AmbiguousChildField",
            "StructuralChildNotATree",
            "ValueNotExpressible",
            "EvaluationFailed",
            "NotBoolean",
        ]
    );
}

#[test]
fn domain_check_is_identity_free_while_checked_put_still_checks_identity_first() {
    let schema = Schema::default().vertex("t", "record");
    let migration = identity_migration(&schema);
    let (source_boundary, target_boundary, plan) = table_plan(&schema, &schema, &migration);
    let lens = must(TableLens::new(
        &schema,
        &schema,
        TableBoundaries::new(&source_boundary, &target_boundary),
        &migration,
        &plan,
        &BTreeMap::new(),
        &NoValueLayer,
    ));
    let mut complement = TableComplement::empty();
    let mut wrong = *lens.identity();
    wrong.lens[0] ^= 1;
    complement.identity = Some(wrong);
    assert_eq!(
        table::check_put_domain(&lens, &Table::default(), &complement),
        Ok(())
    );
    assert!(matches!(
        table::put_checked(
            &lens,
            &Table::default(),
            &complement,
            IdentityPolicy::Strict,
        ),
        Err(TablePutError::Identity(IdentityError::Mismatch { .. }))
    ));
}

#[test]
fn baseline_transform_refusal_is_typed_operational_incompleteness() {
    let schema = two_column_schema(&[("x", "sx")]);
    let migration = Migration {
        surviving_verts: schema.vertices.keys().cloned().collect(),
        surviving_edges: schema.edges.clone(),
        transformed: ["t".to_owned()].into(),
        ..Migration::default()
    };
    let (source_boundary, target_boundary, plan) = table_plan(&schema, &schema, &migration);
    let lens = must(TableLens::new(
        &schema,
        &schema,
        TableBoundaries::new(&source_boundary, &target_boundary),
        &migration,
        &plan,
        &BTreeMap::new(),
        &RefusingBaselineTransform,
    ));
    let (mut view, complement) = must(lens.get(&Table::default()));
    view.rows.insert(
        1,
        (
            "t".to_owned(),
            BTreeMap::from([("x".to_owned(), Value::Int(1))]),
        ),
    );
    let expected = TableOperationalError::Transform {
        elem: ViewRef::Row(1),
        refusal: LayerRefusal {
            kind: "test-refusal".to_owned(),
            detail: "baseline".to_owned(),
        },
    };
    assert_eq!(
        table::check_put_domain(&lens, &view, &complement),
        Err(TableCheckError::Operational(expected.clone()))
    );
    assert_eq!(
        table::put(&lens, &view, &complement),
        Err(TablePutError::Operational(expected))
    );
}

#[test]
fn identity_then_operational_then_domain_is_the_checked_put_priority_lattice() {
    let source_schema = Schema::default()
        .vertex("t", "record")
        .vertex("hidden", "record");
    let target_schema = Schema::default().vertex("t", "record");
    let migration = Migration {
        surviving_verts: ["t".to_owned()].into(),
        transformed: ["t".to_owned()].into(),
        ..Migration::default()
    };
    let (source_boundary, target_boundary, plan) =
        table_plan(&source_schema, &target_schema, &migration);
    let lens = must(TableLens::new(
        &source_schema,
        &target_schema,
        TableBoundaries::new(&source_boundary, &target_boundary),
        &migration,
        &plan,
        &BTreeMap::new(),
        &RefusingBaselineTransform,
    ));
    let source = Table {
        rows: [(1, ("hidden".to_owned(), BTreeMap::new()))].into(),
        ..Table::default()
    };
    let (mut view, complement) = must(lens.get(&source));
    assert!(complement.dropped_rows.contains_key(&1));
    view.rows.insert(1, ("t".to_owned(), BTreeMap::new()));
    assert!(
        view.rows
            .keys()
            .any(|row| complement.dropped_rows.contains_key(row)),
        "the fixture must contain a known IdCollision domain violation"
    );
    let operational = TableOperationalError::Transform {
        elem: ViewRef::Row(1),
        refusal: LayerRefusal {
            kind: "test-refusal".to_owned(),
            detail: "baseline".to_owned(),
        },
    };
    assert_eq!(
        table::check_put_domain(&lens, &view, &complement),
        Err(TableCheckError::Operational(operational.clone()))
    );
    assert_eq!(
        table::put(&lens, &view, &complement),
        Err(TablePutError::Operational(operational))
    );

    let mut corrupt = complement;
    let mut wrong = *lens.identity();
    wrong.lens[0] ^= 1;
    corrupt.identity = Some(wrong);
    assert_eq!(
        table::put(&lens, &view, &corrupt),
        Err(TablePutError::Identity(IdentityError::Mismatch {
            expected: Box::new(*lens.identity()),
            found: Box::new(wrong),
        }))
    );
}

#[test]
fn partial_reverse_candidates_union_only_their_named_collations_after_renames() {
    let a_value = SEdge::new("a", "a.str", "prop", Some("old_a"));
    let b_value = SEdge::new("b", "b.str", "prop", Some("old_b"));
    let target_value = SEdge::new("t", "t.str", "prop", Some("x"));
    let mut source = Schema::default()
        .vertex("a", "record")
        .vertex("b", "record")
        .vertex("a.str", "str")
        .vertex("b.str", "str")
        .edge("a", "a.str", "prop", Some("old_a"))
        .edge("b", "b.str", "prop", Some("old_b"));
    source.keys.insert(
        "a".to_owned(),
        vec![Key {
            id: "a-key".to_owned(),
            parts: vec![KeyPart::Path(vec![a_value.clone()])],
            role: KeyRole::Unique,
            scope: KeyScope::Global,
            equality: vec![KeyEquality::Sql {
                collation: SqlCollation::Named("alpha".to_owned()),
                pad: SqlPad::NoPad,
            }],
        }],
    );
    source.keys.insert(
        "b".to_owned(),
        vec![Key {
            id: "b-key".to_owned(),
            parts: vec![KeyPart::Path(vec![b_value.clone()])],
            role: KeyRole::Unique,
            scope: KeyScope::Global,
            equality: vec![KeyEquality::Sql {
                collation: SqlCollation::Named("omega".to_owned()),
                pad: SqlPad::NoPad,
            }],
        }],
    );
    let target = Schema::default()
        .vertex("t", "record")
        .vertex("t.str", "str")
        .edge("t", "t.str", "prop", Some("x"));
    let migration = Migration {
        surviving_verts: target.vertices.keys().cloned().collect(),
        vertex_remap: [
            ("a".to_owned(), "t".to_owned()),
            ("b".to_owned(), "t".to_owned()),
            ("a.str".to_owned(), "t.str".to_owned()),
            ("b.str".to_owned(), "t.str".to_owned()),
        ]
        .into(),
        edge_remap: [(a_value, target_value.clone()), (b_value, target_value)].into(),
        ..Migration::default()
    };
    let (source_boundary, target_boundary, plan) = table_plan(&source, &target, &migration);
    let lens = must(TableLens::new(
        &source,
        &target,
        TableBoundaries::new(&source_boundary, &target_boundary),
        &migration,
        &plan,
        &BTreeMap::new(),
        &NoValueLayer,
    ));
    let view = Table {
        rows: [(
            1,
            (
                "t".to_owned(),
                BTreeMap::from([("x".to_owned(), Value::Str("A".to_owned()))]),
            ),
        )]
        .into(),
        ..Table::default()
    };
    assert_eq!(
        table::check_put_domain(&lens, &view, &TableComplement::empty()),
        Err(TableCheckError::Domain {
            violations: vec![PutDomainError::AmbiguousReverseAnchor {
                elem: ViewRef::Row(1),
                candidates: vec!["a".to_owned(), "b".to_owned()],
            }],
            unresolved_collations: BTreeSet::from(["alpha".to_owned(), "omega".to_owned()]),
        })
    );
}

#[test]
fn scoped_candidate_collations_follow_each_coherent_source_parent_pair() {
    let pa = SEdge::new("p", "a", "child", Some("child"));
    let pb = SEdge::new("p", "b", "child", Some("child"));
    let pt = SEdge::new("p", "t", "child", Some("child"));
    let a_key = SEdge::new("a", "a.key", "prop", Some("key"));
    let b_key = SEdge::new("b", "b.key", "prop", Some("key"));
    let t_key = SEdge::new("t", "t.key", "prop", Some("key"));
    let scoped_key = |id: &str, edge: SEdge, collation: &str| Key {
        id: id.to_owned(),
        parts: vec![KeyPart::Path(vec![edge])],
        role: KeyRole::Unique,
        scope: KeyScope::Within("p".to_owned()),
        equality: vec![KeyEquality::Sql {
            collation: SqlCollation::Named(collation.to_owned()),
            pad: SqlPad::NoPad,
        }],
    };
    let mut source = Schema::default()
        .vertex("p", "record")
        .vertex("a", "record")
        .vertex("b", "record")
        .vertex("a.key", "str")
        .vertex("b.key", "str")
        .edge("p", "a", "child", Some("child"))
        .edge("p", "b", "child", Some("child"))
        .edge("a", "a.key", "prop", Some("key"))
        .edge("b", "b.key", "prop", Some("key"));
    source.keys.insert(
        "a".to_owned(),
        vec![scoped_key("a-key", a_key.clone(), "alpha")],
    );
    source.keys.insert(
        "b".to_owned(),
        vec![scoped_key("b-key", b_key.clone(), "omega")],
    );
    let target = Schema::default()
        .vertex("p", "record")
        .vertex("t", "record")
        .vertex("t.key", "str")
        .edge("p", "t", "child", Some("child"))
        .edge("t", "t.key", "prop", Some("key"));
    let migration = Migration {
        surviving_verts: target.vertices.keys().cloned().collect(),
        vertex_remap: [
            ("a".to_owned(), "t".to_owned()),
            ("b".to_owned(), "t".to_owned()),
            ("a.key".to_owned(), "t.key".to_owned()),
            ("b.key".to_owned(), "t.key".to_owned()),
        ]
        .into(),
        edge_remap: [
            (pa, pt.clone()),
            (pb, pt.clone()),
            (a_key, t_key.clone()),
            (b_key, t_key),
        ]
        .into(),
        ..Migration::default()
    };
    let (source_boundary, target_boundary, plan) = table_plan(&source, &target, &migration);
    let lens = must(TableLens::new(
        &source,
        &target,
        TableBoundaries::new(&source_boundary, &target_boundary),
        &migration,
        &plan,
        &BTreeMap::new(),
        &NoValueLayer,
    ));
    let view = Table {
        rows: [
            (1, ("p".to_owned(), BTreeMap::new())),
            (
                2,
                (
                    "t".to_owned(),
                    BTreeMap::from([("key".to_owned(), Value::Str("value".to_owned()))]),
                ),
            ),
        ]
        .into(),
        pairs: [(pt.clone(), 1, 2)].into(),
    };
    assert_eq!(
        table::check_put_domain(&lens, &view, &TableComplement::empty()),
        Err(TableCheckError::Domain {
            violations: vec![
                PutDomainError::UnrepresentableViewEdge {
                    elem: ViewRef::Pair(Box::new(pt), 1, 2),
                },
                PutDomainError::AmbiguousReverseAnchor {
                    elem: ViewRef::Row(2),
                    candidates: vec!["a".to_owned(), "b".to_owned()],
                },
            ],
            unresolved_collations: BTreeSet::from(["alpha".to_owned(), "omega".to_owned()]),
        })
    );
}

#[test]
fn transformed_candidate_collations_use_restored_source_values_and_skip_refusals() {
    let a_key = SEdge::new("a", "a.key", "prop", Some("key"));
    let b_key = SEdge::new("b", "b.key", "prop", Some("key"));
    let t_key = SEdge::new("t", "t.key", "prop", Some("key"));
    let mut source = Schema::default()
        .vertex("a", "record")
        .vertex("b", "record")
        .vertex("a.key", "str")
        .vertex("b.key", "int")
        .edge("a", "a.key", "prop", Some("key"))
        .edge("b", "b.key", "prop", Some("key"));
    source.keys.insert(
        "a".to_owned(),
        vec![Key {
            id: "a-key".to_owned(),
            parts: vec![KeyPart::Path(vec![a_key.clone()])],
            role: KeyRole::Unique,
            scope: KeyScope::Global,
            equality: vec![KeyEquality::Sql {
                collation: SqlCollation::Named("alpha".to_owned()),
                pad: SqlPad::NoPad,
            }],
        }],
    );
    let target = Schema::default()
        .vertex("t", "record")
        .vertex("t.key", "int")
        .edge("t", "t.key", "prop", Some("key"));
    let migration = Migration {
        surviving_verts: target.vertices.keys().cloned().collect(),
        vertex_remap: [
            ("a".to_owned(), "t".to_owned()),
            ("b".to_owned(), "t".to_owned()),
            ("a.key".to_owned(), "t.key".to_owned()),
            ("b.key".to_owned(), "t.key".to_owned()),
        ]
        .into(),
        edge_remap: [(a_key, t_key.clone()), (b_key, t_key)].into(),
        transformed: ["a".to_owned()].into(),
        ..Migration::default()
    };
    let (source_boundary, target_boundary, plan) = table_plan(&source, &target, &migration);
    let view = Table {
        rows: [(
            1,
            (
                "t".to_owned(),
                BTreeMap::from([("key".to_owned(), Value::Int(7))]),
            ),
        )]
        .into(),
        ..Table::default()
    };
    let violations = vec![PutDomainError::AmbiguousReverseAnchor {
        elem: ViewRef::Row(1),
        candidates: vec!["a".to_owned(), "b".to_owned()],
    }];

    for (mode, expected_collations) in [
        (
            CandidateRestore::String,
            BTreeSet::from(["alpha".to_owned()]),
        ),
        (CandidateRestore::Refuse, BTreeSet::new()),
        (CandidateRestore::NonString, BTreeSet::new()),
    ] {
        let layer = StringIntCandidateLayer(mode);
        let lens = must(TableLens::new(
            &source,
            &target,
            TableBoundaries::new(&source_boundary, &target_boundary),
            &migration,
            &plan,
            &BTreeMap::new(),
            &layer,
        ));
        assert_eq!(
            table::check_put_domain(&lens, &view, &TableComplement::empty()),
            Err(TableCheckError::Domain {
                violations: violations.clone(),
                unresolved_collations: expected_collations,
            }),
            "candidate restore mode {mode:?}"
        );
    }
}

#[test]
fn candidate_collations_exclude_unused_index_non_string_and_absent_parts() {
    let applicable = SEdge::new("t", "t.applicable", "prop", Some("applicable"));
    let index = SEdge::new("t", "t.index", "prop", Some("index"));
    let number = SEdge::new("t", "t.number", "prop", Some("number"));
    let absent = SEdge::new("t", "t.absent", "prop", Some("absent"));
    let unused = SEdge::new("unused", "unused.value", "prop", Some("value"));
    let named_key = |id: &str, edge: SEdge, role: KeyRole, collation: &str| Key {
        id: id.to_owned(),
        parts: vec![KeyPart::Path(vec![edge])],
        role,
        scope: KeyScope::Global,
        equality: vec![KeyEquality::Sql {
            collation: SqlCollation::Named(collation.to_owned()),
            pad: SqlPad::NoPad,
        }],
    };
    let mut schema = Schema::default()
        .vertex("t", "record")
        .vertex("unused", "record")
        .vertex("t.applicable", "str")
        .vertex("t.index", "str")
        .vertex("t.number", "int")
        .vertex("t.absent", "str")
        .vertex("unused.value", "str")
        .edge("t", "t.applicable", "prop", Some("applicable"))
        .edge("t", "t.index", "prop", Some("index"))
        .edge("t", "t.number", "prop", Some("number"))
        .edge("t", "t.absent", "prop", Some("absent"))
        .edge("unused", "unused.value", "prop", Some("value"));
    schema.keys.insert(
        "t".to_owned(),
        vec![
            named_key("applicable", applicable, KeyRole::Unique, "applicable"),
            named_key("index", index, KeyRole::Index, "index-only"),
            named_key("number", number, KeyRole::Unique, "non-string"),
            named_key("absent", absent, KeyRole::Unique, "absent"),
        ],
    );
    schema.keys.insert(
        "unused".to_owned(),
        vec![named_key("unused", unused, KeyRole::Unique, "unused")],
    );
    let migration = identity_migration(&schema);
    let (source_boundary, target_boundary, plan) = table_plan(&schema, &schema, &migration);
    let lens = must(TableLens::new(
        &schema,
        &schema,
        TableBoundaries::new(&source_boundary, &target_boundary),
        &migration,
        &plan,
        &BTreeMap::new(),
        &NoValueLayer,
    ));
    let view = Table {
        rows: [(
            1,
            (
                "t".to_owned(),
                BTreeMap::from([
                    ("applicable".to_owned(), Value::Str("present".to_owned())),
                    ("index".to_owned(), Value::Str("present".to_owned())),
                    ("number".to_owned(), Value::Int(7)),
                ]),
            ),
        )]
        .into(),
        ..Table::default()
    };
    assert_eq!(
        table::check_put_domain(&lens, &view, &TableComplement::empty()),
        Err(TableCheckError::NamedCollationUnresolved {
            names: BTreeSet::from(["applicable".to_owned()]),
        })
    );
}

#[test]
fn complement_layout_helpers_distinguish_residue_from_bookkeeping() {
    let mut complement = TableComplement::empty();
    assert_eq!(
        TableComplement::FIELD_ORDER,
        [
            "dropped_rows",
            "dropped_cells",
            "dropped_fk_pairs",
            "original_fk_edges",
            "original_tables",
            "original_cells",
            "synthesized_fk_pairs",
            "id_floor",
            "identity",
        ]
    );
    let TableComplement {
        dropped_rows,
        dropped_cells,
        dropped_fk_pairs,
        original_fk_edges,
        original_tables,
        original_cells,
        synthesized_fk_pairs,
        id_floor,
        identity,
    } = TableComplement::empty();
    assert!(dropped_rows.is_empty());
    assert!(dropped_cells.is_empty());
    assert!(dropped_fk_pairs.is_empty());
    assert!(original_fk_edges.is_empty());
    assert!(original_tables.is_empty());
    assert!(original_cells.is_empty());
    assert!(synthesized_fk_pairs.is_empty());
    assert_eq!(id_floor, 0);
    assert_eq!(identity, None);
    assert!(complement.is_empty());
    assert!(complement.residue_is_trivial());
    assert!(complement.reserved_row_ids().is_empty());

    complement
        .synthesized_fk_pairs
        .insert((SEdge::new("a", "b", "fk", Some("ab")), 1, 2));
    complement.id_floor = 9;
    complement.identity = Some(IdentityEnvelope {
        source_schema: [1; 32],
        target_schema: [2; 32],
        lens: [3; 32],
        shape: ComplementShape::Functor,
        format_version: 1,
    });
    assert!(!complement.is_empty());
    assert!(complement.residue_is_trivial());

    complement.dropped_rows.insert(
        7,
        DroppedRow {
            table: "hidden".to_owned(),
            row: BTreeMap::from([("x".to_owned(), Value::Int(1))]),
        },
    );
    assert!(!complement.residue_is_trivial());
    assert_eq!(complement.reserved_row_ids(), BTreeSet::from([7]));

    let edge = SEdge::new("a", "b", "fk", Some("ab"));
    let mut residue_cases = Vec::new();
    let mut case = TableComplement::empty();
    case.dropped_cells
        .insert((1, "x".to_owned()), Value::Int(1));
    residue_cases.push(case);
    let mut case = TableComplement::empty();
    case.dropped_fk_pairs
        .insert(edge.clone(), BTreeSet::from([(1, 2)]));
    residue_cases.push(case);
    let mut case = TableComplement::empty();
    case.original_fk_edges
        .insert((edge.clone(), 1, 2), BTreeSet::from([edge]));
    residue_cases.push(case);
    let mut case = TableComplement::empty();
    case.original_tables.insert(1, "a".to_owned());
    residue_cases.push(case);
    let mut case = TableComplement::empty();
    case.original_cells
        .insert((1, "x".to_owned()), Value::Int(1));
    residue_cases.push(case);
    assert!(
        residue_cases
            .iter()
            .all(|case| !case.residue_is_trivial() && case.reserved_row_ids().is_empty())
    );
}

#[test]
fn reference_owned_table_certificates_have_their_exact_cardinalities() {
    let mut constructions = 0;
    for_each_table_complement_construction(|_| constructions += 1);
    assert_eq!(
        constructions,
        table_complement_construction_count_certificate()
    );
    assert_eq!(constructions, 7);

    let mut derived = 0;
    for_each_derived_writer_fixture(|_| derived += 1);
    assert_eq!(derived, derived_writer_count_certificate());
    assert_eq!(derived, 2);
    assert_eq!(last_explicit_writer_count_certificate(), 36);
}

#[test]
fn fresh_row_allocation_starts_at_the_floor_and_avoids_visible_and_hidden_rows() {
    let view = Table {
        rows: [
            (4, ("t".to_owned(), BTreeMap::new())),
            (6, ("t".to_owned(), BTreeMap::new())),
        ]
        .into(),
        ..Table::default()
    };
    let mut complement = TableComplement::empty();
    complement.id_floor = 4;
    complement.dropped_rows.insert(
        5,
        DroppedRow {
            table: "t".to_owned(),
            row: BTreeMap::new(),
        },
    );
    assert_eq!(table::fresh_view_row_id(&view, &complement), Ok(7));
    complement.id_floor = u32::MAX;
    assert_eq!(
        table::fresh_view_row_id(&view, &complement),
        Err(RowIdsExhausted)
    );
}

#[test]
fn identity_mismatch_precedes_the_first_domain_violation() {
    let source = Schema::default()
        .vertex("kept", "record")
        .vertex("hidden", "record");
    let target = Schema::default().vertex("kept", "record");
    let migration = Migration {
        surviving_verts: ["kept".to_owned()].into(),
        ..Migration::default()
    };
    let (source_boundary, target_boundary, plan) = table_plan(&source, &target, &migration);
    let lens = must(TableLens::new_at_boundary(
        &source,
        &target,
        TableBoundaries::new(&source_boundary, &target_boundary),
        &migration,
        &plan,
        &BTreeMap::new(),
        &NoValueLayer,
    ));
    let source_instance = Table {
        rows: [
            (1, ("kept".to_owned(), BTreeMap::new())),
            (2, ("hidden".to_owned(), BTreeMap::new())),
        ]
        .into(),
        ..Table::default()
    };
    let (mut view, mut complement) = must(lens.get(&source_instance));
    view.rows.insert(2, ("kept".to_owned(), BTreeMap::new()));
    let mut wrong = *lens.identity();
    if let Some(first) = wrong.lens.first_mut() {
        *first ^= 1;
    }
    complement.identity = Some(wrong);
    assert_eq!(
        table::put_checked(&lens, &view, &complement, IdentityPolicy::Strict),
        Err(TablePutError::Identity(IdentityError::Mismatch {
            expected: Box::new(*lens.identity()),
            found: Box::new(wrong),
        }))
    );
    complement.identity = None;
    assert_eq!(
        table::put(&lens, &view, &complement),
        Err(TablePutError::Identity(IdentityError::UnboundComplement))
    );
}

#[test]
fn table_construction_and_bare_get_refuse_wrong_endpoint_contexts() {
    let schema = Schema::default().vertex("t", "record");
    let migration = identity_migration(&schema);
    let (source_boundary, target_boundary, plan) = table_plan(&schema, &schema, &migration);
    let wrong_schema = Schema::default().vertex("other", "record");
    let wrong_boundary = record_boundary(&wrong_schema);

    assert!(matches!(
        TableLens::new_at_boundary(
            &schema,
            &schema,
            TableBoundaries::new(&wrong_boundary, &target_boundary),
            &migration,
            &plan,
            &BTreeMap::new(),
            &NoValueLayer,
        ),
        Err(TablePlanError::SourceBoundaryMismatch)
    ));
    assert!(matches!(
        TableLens::new_at_boundary(
            &schema,
            &schema,
            TableBoundaries::new(&source_boundary, &wrong_boundary),
            &migration,
            &plan,
            &BTreeMap::new(),
            &NoValueLayer,
        ),
        Err(TablePlanError::TargetBoundaryMismatch)
    ));

    let mut changed_migration = migration;
    changed_migration.transformed.insert("t".to_owned());
    assert!(matches!(
        table::get(
            &schema,
            &schema,
            &changed_migration,
            &plan,
            &NoValueLayer,
            &Table::default(),
        ),
        Err(RestrictError::Plan(PlanError::MigrationMismatch { .. }))
    ));
}

#[test]
fn checked_domain_vector_and_first_error_have_independent_exact_order() {
    let source = Schema::default()
        .vertex("a", "record")
        .vertex("b", "record");
    let target = Schema::default().vertex("t", "record");
    let migration = Migration {
        surviving_verts: ["t".to_owned()].into(),
        vertex_remap: [
            ("a".to_owned(), "t".to_owned()),
            ("b".to_owned(), "t".to_owned()),
        ]
        .into(),
        ..Migration::default()
    };
    let (source_boundary, target_boundary, plan) = table_plan(&source, &target, &migration);
    let lens = must(TableLens::new_at_boundary(
        &source,
        &target,
        TableBoundaries::new(&source_boundary, &target_boundary),
        &migration,
        &plan,
        &BTreeMap::new(),
        &NoValueLayer,
    ));
    let synthesized = SEdge::new("t", "t", "synth", Some("synth"));
    let unsupported = SEdge::new("t", "t", "other", Some("other"));
    let mut complement = TableComplement::empty();
    complement.identity = Some(*lens.identity());
    complement.dropped_rows.insert(
        1,
        DroppedRow {
            table: "a".to_owned(),
            row: BTreeMap::new(),
        },
    );
    complement.original_tables = [(1, "a".to_owned()), (3, "b".to_owned())].into();
    complement
        .synthesized_fk_pairs
        .insert((synthesized.clone(), 1, 3));
    let view = Table {
        rows: [
            (1, ("t".to_owned(), BTreeMap::new())),
            (3, ("t".to_owned(), BTreeMap::new())),
            (4, ("t".to_owned(), BTreeMap::new())),
        ]
        .into(),
        pairs: [(unsupported.clone(), 1, 3)].into(),
    };
    let expected = vec![
        PutDomainError::IdCollision {
            elem: ViewRef::Row(1),
        },
        PutDomainError::SynthesizedEdited {
            elem: ViewRef::Pair(Box::new(synthesized), 1, 3),
        },
        PutDomainError::UnrepresentableViewEdge {
            elem: ViewRef::Pair(Box::new(unsupported), 1, 3),
        },
        PutDomainError::AmbiguousReverseAnchor {
            elem: ViewRef::Row(4),
            candidates: vec!["a".to_owned(), "b".to_owned()],
        },
    ];
    assert_eq!(
        table::check_put_domain(&lens, &view, &complement),
        Err(TableCheckError::Domain {
            violations: expected.clone(),
            unresolved_collations: BTreeSet::new(),
        })
    );
    let first = expected.first().map_or_else(
        || panic!("fixture has an empty expected report"),
        Clone::clone,
    );
    assert_eq!(
        table::put(&lens, &view, &complement),
        Err(TablePutError::Domain(first))
    );
}

struct HiddenB;

impl ValueLayer for HiddenB {
    fn canonical_semantics(&self) -> ValueLayerSemantics {
        ValueLayerSemantics {
            domain: "panproto-reference/test/hidden-anchor",
            version: 1,
            canonical_payload: b"anchor=b".to_vec(),
        }
    }

    fn survives(&self, _: &Graph, node: &Node) -> Result<bool, PredicateRefusal> {
        Ok(node.anchor != "b")
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
fn reinserted_visible_row_does_not_regain_its_cascaded_hidden_pair() {
    let foreign_key = SEdge::new("a", "b", "fk", Some("b"));
    let schema = Schema::default()
        .vertex("a", "record")
        .vertex("b", "record")
        .edge("a", "b", "fk", Some("b"));
    let migration = Migration {
        surviving_verts: schema.vertices.keys().cloned().collect(),
        surviving_edges: schema.edges.clone(),
        conditional: ["b".to_owned()].into(),
        ..Migration::default()
    };
    let (source_boundary, target_boundary, plan) = table_plan(&schema, &schema, &migration);
    let lens = must(TableLens::new_at_boundary(
        &schema,
        &schema,
        TableBoundaries::new(&source_boundary, &target_boundary),
        &migration,
        &plan,
        &BTreeMap::new(),
        &HiddenB,
    ));
    let source = Table {
        rows: [
            (1, ("a".to_owned(), BTreeMap::new())),
            (2, ("b".to_owned(), BTreeMap::new())),
        ]
        .into(),
        pairs: [(foreign_key.clone(), 1, 2)].into(),
    };
    let (mut view, complement) = must(lens.get(&source));
    view.rows.remove(&1);
    let after_delete = must(table::put(&lens, &view, &complement));
    assert!(!after_delete.pairs.contains(&(foreign_key.clone(), 1, 2)));

    let (mut second_view, second_complement) = must(lens.get(&after_delete));
    second_view
        .rows
        .insert(1, ("a".to_owned(), BTreeMap::new()));
    let after_reinsert = must(table::put(&lens, &second_view, &second_complement));
    assert!(!after_reinsert.pairs.contains(&(foreign_key, 1, 2)));
}

#[test]
fn a_direct_and_contracted_pair_is_read_only_but_restores_its_direct_origin() {
    let am = SEdge::new("a", "m", "fk", Some("am"));
    let mb = SEdge::new("m", "b", "fk", Some("mb"));
    let direct = SEdge::new("a", "b", "fk", Some("ab"));
    let view_edge = SEdge::new("a", "b", "fk", Some("ab"));
    let source_schema = Schema::default()
        .vertex("a", "record")
        .vertex("m", "record")
        .vertex("b", "record")
        .edge("a", "m", "fk", Some("am"))
        .edge("m", "b", "fk", Some("mb"))
        .edge("a", "b", "fk", Some("ab"));
    let target_schema = Schema::default()
        .vertex("a", "record")
        .vertex("b", "record")
        .edge("a", "b", "fk", Some("ab"));
    let migration = Migration {
        surviving_verts: ["a".to_owned(), "b".to_owned()].into(),
        surviving_edges: [direct.clone()].into(),
        resolver: [(("a".to_owned(), "b".to_owned()), view_edge.clone())].into(),
        ..Migration::default()
    };
    let (source_boundary, target_boundary, plan) =
        table_plan(&source_schema, &target_schema, &migration);
    let lens = must(TableLens::new_at_boundary(
        &source_schema,
        &target_schema,
        TableBoundaries::new(&source_boundary, &target_boundary),
        &migration,
        &plan,
        &BTreeMap::new(),
        &NoValueLayer,
    ));
    let source = Table {
        rows: [
            (1, ("a".to_owned(), BTreeMap::new())),
            (2, ("m".to_owned(), BTreeMap::new())),
            (3, ("b".to_owned(), BTreeMap::new())),
        ]
        .into(),
        pairs: [(am, 1, 2), (mb, 2, 3), (direct, 1, 3)].into(),
    };
    let (view, complement) = must(lens.get(&source));
    assert!(
        complement
            .synthesized_fk_pairs
            .contains(&(view_edge.clone(), 1, 3))
    );
    assert_eq!(must(table::put(&lens, &view, &complement)), source);

    let mut removed = view;
    removed.pairs.remove(&(view_edge.clone(), 1, 3));
    assert_eq!(
        table::put(&lens, &removed, &complement),
        Err(TablePutError::Domain(PutDomainError::SynthesizedEdited {
            elem: ViewRef::Pair(Box::new(view_edge), 1, 3),
        }))
    );
}

#[test]
fn recorded_foreign_key_origins_are_validated_before_they_are_trusted() {
    let first = SEdge::new("a", "b", "first", Some("first"));
    let second = SEdge::new("a", "b", "second", Some("second"));
    let image = SEdge::new("a", "b", "joined", Some("joined"));
    let source_schema = Schema::default()
        .vertex("a", "record")
        .vertex("b", "record")
        .edge("a", "b", "first", Some("first"))
        .edge("a", "b", "second", Some("second"));
    let target_schema = Schema::default()
        .vertex("a", "record")
        .vertex("b", "record")
        .edge("a", "b", "joined", Some("joined"));
    let migration = Migration {
        surviving_verts: target_schema.vertices.keys().cloned().collect(),
        edge_remap: [
            (first.clone(), image.clone()),
            (second.clone(), image.clone()),
        ]
        .into(),
        ..Migration::default()
    };
    let (source_boundary, target_boundary, plan) =
        table_plan(&source_schema, &target_schema, &migration);
    let lens = must(TableLens::new_at_boundary(
        &source_schema,
        &target_schema,
        TableBoundaries::new(&source_boundary, &target_boundary),
        &migration,
        &plan,
        &BTreeMap::new(),
        &NoValueLayer,
    ));
    let source = Table {
        rows: [
            (1, ("a".to_owned(), BTreeMap::new())),
            (2, ("b".to_owned(), BTreeMap::new())),
        ]
        .into(),
        pairs: [(first.clone(), 1, 2), (second.clone(), 1, 2)].into(),
    };
    let (view, mut complement) = must(lens.get(&source));
    let pair = (image.clone(), 1, 2);
    assert_eq!(
        complement.original_fk_edges.get(&pair),
        Some(&BTreeSet::from([first, second]))
    );
    assert_eq!(must(table::put(&lens, &view, &complement)), source);

    complement.original_fk_edges.insert(
        pair,
        BTreeSet::from([SEdge::new("x", "y", "bogus", Some("bogus"))]),
    );
    assert_eq!(
        table::put(&lens, &view, &complement),
        Err(TablePutError::Domain(
            PutDomainError::UnrepresentableViewEdge {
                elem: ViewRef::Pair(Box::new(image), 1, 2),
            }
        ))
    );
}

struct KeepWhenOne;

impl ValueLayer for KeepWhenOne {
    fn canonical_semantics(&self) -> ValueLayerSemantics {
        ValueLayerSemantics {
            domain: "panproto-reference/test/field-equals-int-survival",
            version: 1,
            canonical_payload: b"anchor=b\0field=keep\0value=i64:1".to_vec(),
        }
    }

    fn survives(&self, _: &Graph, node: &Node) -> Result<bool, PredicateRefusal> {
        Ok(node.anchor != "b" || node.fields.get("keep") == Some(&Value::Int(1)))
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
fn regenerated_pair_mismatch_is_reported_with_the_survival_failure() {
    let foreign_key = SEdge::new("a", "b", "fk", Some("b"));
    let schema = Schema::default()
        .vertex("a", "record")
        .vertex("b", "record")
        .vertex("int", "int")
        .edge("b", "int", "prop", Some("keep"))
        .edge("a", "b", "fk", Some("b"));
    let migration = Migration {
        surviving_verts: schema.vertices.keys().cloned().collect(),
        surviving_edges: schema.edges.clone(),
        conditional: ["b".to_owned()].into(),
        ..Migration::default()
    };
    let (source_boundary, target_boundary, plan) = table_plan(&schema, &schema, &migration);
    let lens = must(TableLens::new_at_boundary(
        &schema,
        &schema,
        TableBoundaries::new(&source_boundary, &target_boundary),
        &migration,
        &plan,
        &BTreeMap::new(),
        &KeepWhenOne,
    ));
    let source = Table {
        rows: [
            (1, ("a".to_owned(), BTreeMap::new())),
            (
                2,
                (
                    "b".to_owned(),
                    BTreeMap::from([("keep".to_owned(), Value::Int(1))]),
                ),
            ),
        ]
        .into(),
        pairs: [(foreign_key.clone(), 1, 2)].into(),
    };
    let (mut view, complement) = must(lens.get(&source));
    if let Some((_, row)) = view.rows.get_mut(&2) {
        row.insert("keep".to_owned(), Value::Int(0));
    } else {
        panic!("fixture view has no row 2");
    }
    assert_eq!(
        table::check_put_domain(&lens, &view, &complement),
        Err(TableCheckError::Domain {
            violations: vec![
                PutDomainError::UnrepresentableViewEdge {
                    elem: ViewRef::Pair(Box::new(foreign_key), 1, 2),
                },
                PutDomainError::SurvivalPredicateFails {
                    elem: ViewRef::Row(2),
                },
            ],
            unresolved_collations: BTreeSet::new(),
        })
    );
}

struct DoublingOnly;

impl ValueLayer for DoublingOnly {
    fn canonical_semantics(&self) -> ValueLayerSemantics {
        ValueLayerSemantics {
            domain: "panproto-reference/test/integer-scale",
            version: 1,
            canonical_payload: b"field=x\0factor=2".to_vec(),
        }
    }

    fn survives(&self, _: &Graph, _: &Node) -> Result<bool, PredicateRefusal> {
        Ok(true)
    }

    fn transform(
        &self,
        _: &Graph,
        node: &Node,
    ) -> Result<(BTreeMap<String, Value>, Option<Presence>), LayerRefusal> {
        let mut fields = node.fields.clone();
        if let Some(Value::Int(value)) = fields.get("x") {
            fields.insert("x".to_owned(), Value::Int(value * 2));
        }
        Ok((fields, node.value.clone()))
    }

    fn restore(
        &self,
        view: &Node,
        _: Option<&BTreeMap<String, Value>>,
        _: Option<&Option<Presence>>,
    ) -> Result<(BTreeMap<String, Value>, Option<Presence>), PutFieldRefusal> {
        let Some(Value::Int(value)) = view.fields.get("x") else {
            return Err(PutFieldRefusal::TransformNotInvertible {
                field: "x".to_owned(),
            });
        };
        if value % 2 != 0 {
            return Err(PutFieldRefusal::TransformNotInvertible {
                field: "x".to_owned(),
            });
        }
        Ok((
            BTreeMap::from([("x".to_owned(), Value::Int(value / 2))]),
            view.value.clone(),
        ))
    }
}

struct DerivedAndInvertible;

impl ValueLayer for DerivedAndInvertible {
    fn canonical_semantics(&self) -> ValueLayerSemantics {
        ValueLayerSemantics {
            domain: "panproto-reference/test/integer-scale-with-derived-successor",
            version: 1,
            canonical_payload: b"field=x\0factor=2\0derived=derived\0offset=1".to_vec(),
        }
    }

    fn survives(&self, _: &Graph, _: &Node) -> Result<bool, PredicateRefusal> {
        Ok(true)
    }

    fn transform(
        &self,
        _: &Graph,
        node: &Node,
    ) -> Result<(BTreeMap<String, Value>, Option<Presence>), LayerRefusal> {
        let mut fields = node.fields.clone();
        if let Some(Value::Int(value)) = fields.get("x").cloned() {
            fields.insert("x".to_owned(), Value::Int(value * 2));
            fields.insert("derived".to_owned(), Value::Int(value + 1));
        }
        Ok((fields, node.value.clone()))
    }

    fn restore(
        &self,
        view: &Node,
        _: Option<&BTreeMap<String, Value>>,
        _: Option<&Option<Presence>>,
    ) -> Result<(BTreeMap<String, Value>, Option<Presence>), PutFieldRefusal> {
        let Some(Value::Int(value)) = view.fields.get("x") else {
            return Err(PutFieldRefusal::TransformNotInvertible {
                field: "x".to_owned(),
            });
        };
        if value % 2 != 0 {
            return Err(PutFieldRefusal::TransformNotInvertible {
                field: "x".to_owned(),
            });
        }
        let mut fields = BTreeMap::from([("x".to_owned(), Value::Int(value / 2))]);
        if view.fields.contains_key("derived") {
            fields.remove("derived");
        }
        Ok((fields, view.value.clone()))
    }
}

#[test]
fn transformed_cells_recompute_the_baseline_and_use_only_valid_inverses() {
    let schema = two_column_schema(&[("x", "sx")]);
    let migration = Migration {
        surviving_verts: schema.vertices.keys().cloned().collect(),
        surviving_edges: schema.edges.clone(),
        transformed: ["t".to_owned()].into(),
        ..Migration::default()
    };
    let (source_boundary, target_boundary, plan) = table_plan(&schema, &schema, &migration);
    let inverse_lens = must(TableLens::new_at_boundary(
        &schema,
        &schema,
        TableBoundaries::new(&source_boundary, &target_boundary),
        &migration,
        &plan,
        &BTreeMap::new(),
        &DoublingOnly,
    ));
    let source = Table {
        rows: [(
            1,
            ("t".to_owned(), [("x".to_owned(), Value::Int(3))].into()),
        )]
        .into(),
        ..Table::default()
    };
    let (mut inverse_view, inverse_complement) = must(inverse_lens.get(&source));
    assert_eq!(
        inverse_view.rows.get(&1).and_then(|row| row.1.get("x")),
        Some(&Value::Int(6))
    );
    if let Some(row) = inverse_view.rows.get_mut(&1) {
        row.1.insert("x".to_owned(), Value::Int(10));
    }
    let restored = must(table::put(
        &inverse_lens,
        &inverse_view,
        &inverse_complement,
    ));
    assert_eq!(
        restored.rows.get(&1).and_then(|row| row.1.get("x")),
        Some(&Value::Int(5))
    );

    let lens = must(TableLens::new_at_boundary(
        &schema,
        &schema,
        TableBoundaries::new(&source_boundary, &target_boundary),
        &migration,
        &plan,
        &BTreeMap::new(),
        &DerivedAndInvertible,
    ));
    let (mut view, complement) = must(lens.get(&source));
    if let Some(row) = view.rows.get_mut(&1) {
        row.1.insert("x".to_owned(), Value::Int(10));
    }
    assert_eq!(
        table::put(&lens, &view, &complement),
        Err(TablePutError::Domain(PutDomainError::DerivedCellEdited {
            elem: ViewRef::Row(1),
            field: "derived".to_owned(),
        }))
    );

    let mut coherent = view.clone();
    if let Some(row) = coherent.rows.get_mut(&1) {
        row.1.insert("derived".to_owned(), Value::Int(6));
    }
    assert_eq!(
        table::put(&lens, &coherent, &complement),
        Err(TablePutError::Domain(PutDomainError::DerivedCellEdited {
            elem: ViewRef::Row(1),
            field: "derived".to_owned(),
        }))
    );

    let mut invalid = view.clone();
    if let Some(row) = invalid.rows.get_mut(&1) {
        row.1.insert("x".to_owned(), Value::Int(9));
    }
    assert_eq!(
        table::put(&lens, &invalid, &complement),
        Err(TablePutError::Domain(
            PutDomainError::TransformNotInvertible {
                elem: ViewRef::Row(1),
                field: "x".to_owned(),
            }
        ))
    );

    let mut derived = view;
    if let Some(row) = derived.rows.get_mut(&1) {
        row.1.insert("derived".to_owned(), Value::Int(99));
    }
    assert_eq!(
        table::put(&lens, &derived, &complement),
        Err(TablePutError::Domain(PutDomainError::DerivedCellEdited {
            elem: ViewRef::Row(1),
            field: "derived".to_owned(),
        }))
    );
}

#[test]
fn schema_and_forward_stability_violations_are_accumulated_before_sorting() {
    let key_edge = SEdge::new("t", "sx", "prop", Some("x"));
    let mut schema = two_column_schema(&[("x", "sx")]);
    schema.keys.insert(
        "t".to_owned(),
        vec![Key {
            id: "unique_x".to_owned(),
            parts: vec![KeyPart::Path(vec![key_edge])],
            role: KeyRole::Unique,
            scope: KeyScope::Global,
            equality: Vec::new(),
        }],
    );
    let migration = Migration {
        surviving_verts: schema.vertices.keys().cloned().collect(),
        surviving_edges: schema.edges.clone(),
        transformed: ["t".to_owned()].into(),
        ..Migration::default()
    };
    let (source_boundary, target_boundary, plan) = table_plan(&schema, &schema, &migration);
    let lens = must(TableLens::new_at_boundary(
        &schema,
        &schema,
        TableBoundaries::new(&source_boundary, &target_boundary),
        &migration,
        &plan,
        &BTreeMap::new(),
        &DerivedAndInvertible,
    ));
    let source = Table {
        rows: [
            (
                1,
                (
                    "t".to_owned(),
                    BTreeMap::from([("x".to_owned(), Value::Int(3))]),
                ),
            ),
            (
                2,
                (
                    "t".to_owned(),
                    BTreeMap::from([("x".to_owned(), Value::Int(5))]),
                ),
            ),
        ]
        .into(),
        ..Table::default()
    };
    let (mut view, complement) = must(lens.get(&source));
    if let Some((_, row)) = view.rows.get_mut(&1) {
        row.insert("x".to_owned(), Value::Int(10));
    } else {
        panic!("fixture view has no row 1");
    }
    assert_eq!(
        table::check_put_domain(&lens, &view, &complement),
        Err(TableCheckError::Domain {
            violations: vec![
                PutDomainError::DerivedCellEdited {
                    elem: ViewRef::Row(1),
                    field: "derived".to_owned(),
                },
                PutDomainError::DuplicateKey {
                    table: "t".to_owned(),
                    key: vec![("x".to_owned(), Value::Int(5))],
                    elems: vec![ViewRef::Row(1), ViewRef::Row(2)],
                },
            ],
            unresolved_collations: BTreeSet::new(),
        })
    );
}

#[test]
fn complement_free_put_accepts_a_column_swap_and_refuses_loss() {
    let source = two_column_schema(&[("a", "sa"), ("b", "sb")]);
    let target = two_column_schema(&[("a", "ta"), ("b", "tb")]);
    let migration = Migration {
        surviving_verts: target.vertices.keys().cloned().collect(),
        vertex_remap: [
            ("sa".to_owned(), "tb".to_owned()),
            ("sb".to_owned(), "ta".to_owned()),
        ]
        .into(),
        edge_remap: [
            (
                SEdge::new("t", "sa", "prop", Some("a")),
                SEdge::new("t", "tb", "prop", Some("b")),
            ),
            (
                SEdge::new("t", "sb", "prop", Some("b")),
                SEdge::new("t", "ta", "prop", Some("a")),
            ),
        ]
        .into(),
        ..Migration::default()
    };
    let (source_boundary, target_boundary, plan) = table_plan(&source, &target, &migration);
    let lens = must(TableLens::new_at_boundary(
        &source,
        &target,
        TableBoundaries::new(&source_boundary, &target_boundary),
        &migration,
        &plan,
        &BTreeMap::new(),
        &NoValueLayer,
    ));
    let view = Table {
        rows: [(
            1,
            (
                "t".to_owned(),
                [
                    ("a".to_owned(), Value::Int(20)),
                    ("b".to_owned(), Value::Int(10)),
                ]
                .into(),
            ),
        )]
        .into(),
        ..Table::default()
    };
    let source_instance = must(table::put_without_complement(&lens, &view));
    assert_eq!(
        source_instance.rows.get(&1).map(|row| &row.1),
        Some(&BTreeMap::from([
            ("a".to_owned(), Value::Int(10)),
            ("b".to_owned(), Value::Int(20)),
        ]))
    );

    let lossy_target = Schema::default().vertex("t", "record");
    let lossy_migration = Migration {
        surviving_verts: ["t".to_owned()].into(),
        ..Migration::default()
    };
    let (lossy_source_boundary, lossy_target_boundary, lossy_plan) =
        table_plan(&source, &lossy_target, &lossy_migration);
    let lossy = must(TableLens::new_at_boundary(
        &source,
        &lossy_target,
        TableBoundaries::new(&lossy_source_boundary, &lossy_target_boundary),
        &lossy_migration,
        &lossy_plan,
        &BTreeMap::new(),
        &NoValueLayer,
    ));
    assert!(matches!(
        table::put_without_complement(&lossy, &Table::default()),
        Err(TablePutError::NotAnIsomorphism { .. })
    ));
}

#[test]
fn named_collation_is_a_typed_unresolved_result_not_a_silent_pass() {
    let edge = SEdge::new("t", "s", "prop", Some("s"));
    let mut schema = Schema::default()
        .vertex("t", "record")
        .vertex("s", "str")
        .edge("t", "s", "prop", Some("s"));
    schema.keys.insert(
        "t".to_owned(),
        vec![Key {
            id: "named".to_owned(),
            parts: vec![KeyPart::Path(vec![edge])],
            role: KeyRole::Unique,
            scope: KeyScope::Global,
            equality: vec![KeyEquality::Sql {
                collation: SqlCollation::Named("database_default".to_owned()),
                pad: SqlPad::NoPad,
            }],
        }],
    );
    let migration = identity_migration(&schema);
    let (source_boundary, target_boundary, plan) = table_plan(&schema, &schema, &migration);
    let lens = must(TableLens::new_at_boundary(
        &schema,
        &schema,
        TableBoundaries::new(&source_boundary, &target_boundary),
        &migration,
        &plan,
        &BTreeMap::new(),
        &NoValueLayer,
    ));
    let view = Table {
        rows: [(
            1,
            (
                "t".to_owned(),
                [("s".to_owned(), Value::Str("A".to_owned()))].into(),
            ),
        )]
        .into(),
        ..Table::default()
    };
    let mut complement = TableComplement::empty();
    complement.identity = Some(*lens.identity());
    let names = BTreeSet::from(["database_default".to_owned()]);
    assert_eq!(
        table::check_put_domain(&lens, &view, &complement),
        Err(TableCheckError::NamedCollationUnresolved {
            names: names.clone(),
        })
    );
    assert_eq!(
        table::put(&lens, &view, &complement),
        Err(TablePutError::NamedCollationUnresolved { names })
    );
}

#[test]
fn every_applicable_named_collation_in_a_composite_key_is_reported() {
    let first = SEdge::new("t", "s1", "prop", Some("first"));
    let second = SEdge::new("t", "s2", "prop", Some("second"));
    let mut schema = Schema::default()
        .vertex("t", "record")
        .vertex("s1", "str")
        .vertex("s2", "str")
        .edge("t", "s1", "prop", Some("first"))
        .edge("t", "s2", "prop", Some("second"));
    schema.keys.insert(
        "t".to_owned(),
        vec![Key {
            id: "composite".to_owned(),
            parts: vec![KeyPart::Path(vec![first]), KeyPart::Path(vec![second])],
            role: KeyRole::Unique,
            scope: KeyScope::Global,
            equality: vec![
                KeyEquality::Sql {
                    collation: SqlCollation::Named("alpha".to_owned()),
                    pad: SqlPad::NoPad,
                },
                KeyEquality::Sql {
                    collation: SqlCollation::Named("omega".to_owned()),
                    pad: SqlPad::NoPad,
                },
            ],
        }],
    );
    let migration = identity_migration(&schema);
    let (source_boundary, target_boundary, plan) = table_plan(&schema, &schema, &migration);
    let lens = must(TableLens::new_at_boundary(
        &schema,
        &schema,
        TableBoundaries::new(&source_boundary, &target_boundary),
        &migration,
        &plan,
        &BTreeMap::new(),
        &NoValueLayer,
    ));
    let view = Table {
        rows: [(
            1,
            (
                "t".to_owned(),
                BTreeMap::from([
                    ("first".to_owned(), Value::Str("A".to_owned())),
                    ("second".to_owned(), Value::Str("B".to_owned())),
                ]),
            ),
        )]
        .into(),
        ..Table::default()
    };
    let mut complement = TableComplement::empty();
    complement.identity = Some(*lens.identity());
    assert_eq!(
        table::check_put_domain(&lens, &view, &complement),
        Err(TableCheckError::NamedCollationUnresolved {
            names: BTreeSet::from(["alpha".to_owned(), "omega".to_owned()]),
        })
    );
}

#[test]
fn checked_put_returns_the_verified_or_empty_unbound_status() {
    let schema = Schema::default().vertex("t", "record");
    let migration = identity_migration(&schema);
    let (source_boundary, target_boundary, plan) = table_plan(&schema, &schema, &migration);
    let lens = must(TableLens::new_at_boundary(
        &schema,
        &schema,
        TableBoundaries::new(&source_boundary, &target_boundary),
        &migration,
        &plan,
        &BTreeMap::new(),
        &NoValueLayer,
    ));
    let view = Table::default();
    assert_eq!(
        table::put_checked(
            &lens,
            &view,
            &TableComplement::empty(),
            IdentityPolicy::Strict,
        ),
        Ok((Table::default(), IdentityStatus::Unbound))
    );
    let mut stamped = TableComplement::empty();
    stamped.identity = Some(*lens.identity());
    assert_eq!(
        table::put_checked(&lens, &view, &stamped, IdentityPolicy::Strict),
        Ok((Table::default(), IdentityStatus::Verified))
    );
}
