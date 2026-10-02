#![allow(clippy::unwrap_used, clippy::expect_used)]
//! The table restrict takes time linear in a row's cells up to a
//! logarithmic factor: tracing each source cell to the key that holds its
//! value in the view costs no scan of the row's other keys.

mod common;

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::time::{Duration, Instant};

use panproto_gat::Name;
use panproto_inst::{
    BoundaryProtocols, CompiledMigration, FInstance, FieldTransform, RestrictPlan, RowId, Value,
    functor_restrict_with_plan,
};
use panproto_schema::{Edge, Protocol, ScalarKind, Schema, SchemaBuilder};

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

fn column(name: &str) -> Edge {
    Edge {
        src: "t".into(),
        tgt: format!("t.{name}").into(),
        kind: "prop".into(),
        name: Some(name.into()),
    }
}

/// The table `t` with `width` int columns `c0 ..`, the identity plan over
/// it (with a value transform keeping every column when `keep` is set),
/// and `rows` rows whose cell `ci` holds `i`.
fn wide(width: usize, rows: u32, keep: bool) -> (RestrictPlan, FInstance) {
    let p = protocol();
    let names: Vec<String> = (0..width).map(|i| format!("c{i}")).collect();
    let mut builder = SchemaBuilder::new(&p).vertex("t", "record", None).unwrap();
    for name in &names {
        let vertex = format!("t.{name}");
        builder = builder
            .vertex(&vertex, "int", None)
            .unwrap()
            .edge("t", &vertex, "prop", Some(name))
            .unwrap();
    }
    let schema: Schema = builder.build().unwrap();
    let mut migration = CompiledMigration {
        surviving_verts: schema.vertices.keys().cloned().collect(),
        surviving_edges: names.iter().map(|n| column(n)).collect(),
        ..CompiledMigration::default()
    };
    if keep {
        migration.field_transforms.insert(
            "t".into(),
            vec![FieldTransform::KeepFields {
                keys: names.clone(),
            }],
        );
    }
    let plan = RestrictPlan::compile_for_tables(
        &schema,
        &schema,
        &migration,
        BoundaryProtocols { src: &p, tgt: &p },
    )
    .unwrap();
    let row: HashMap<String, Value> = names
        .iter()
        .zip(0_i64..)
        .map(|(name, i)| (name.clone(), Value::Int(i)))
        .collect();
    let instance = FInstance::new()
        .with_rows("t", (0..rows).map(|id| (RowId(id), row.clone())))
        .unwrap();
    (plan, instance)
}

/// The time of the restrict, after checking that the identity plan keeps
/// every cell and records nothing.
fn restrict_time(plan: &RestrictPlan, instance: &FInstance) -> Duration {
    let start = Instant::now();
    let (view, complement) = functor_restrict_with_plan(instance, plan).unwrap();
    let elapsed = start.elapsed();
    assert_eq!(view.row(RowId(0)), instance.row(RowId(0)));
    assert!(complement.residue_is_trivial(), "{complement:?}");
    elapsed
}

/// The growth, from 250 to 4000 columns, of the restrict's time in units
/// of a linear baseline over the same table (one clone of it).
fn growth_from_250_to_4000_columns(keep: bool) -> f64 {
    let (small_plan, small_table) = wide(250, 20, keep);
    let (large_plan, large_table) = wide(4000, 20, keep);
    let (small, large) = common::sizes_in_units(
        7,
        common::Probe {
            work: || restrict_time(&small_plan, &small_table),
            unit: || drop(std::hint::black_box(small_table.clone())),
        },
        common::Probe {
            work: || restrict_time(&large_plan, &large_table),
            unit: || drop(std::hint::black_box(large_table.clone())),
        },
    );
    let growth = large / small;
    eprintln!(
        "keep {keep}: 250 columns: {small:.1} units; 4000 columns: {large:.1} units; \
         growth {growth:.2}"
    );
    growth
}

// Multiplying the columns by sixteen multiplies the cells by sixteen. Work
// that scans a row's keys for each of its cells grows 256-fold, sixteenfold
// in units of a linear baseline over the same table, less the share of the
// linear work around the scans; work linear in the cells up to a
// logarithmic factor keeps its size in those units. The bound 2.8 lies
// between the two: the linear run measures about 1, and a scan of the kept
// keys per cell alone already measures above 4.

#[test]
fn a_wide_row_is_not_charged_per_cell_and_key() {
    let growth = growth_from_250_to_4000_columns(false);
    assert!(growth < 2.8, "growth {growth:.2}");
}

#[test]
fn keeping_the_columns_of_a_wide_row_is_not_charged_per_cell_and_key() {
    let growth = growth_from_250_to_4000_columns(true);
    assert!(growth < 2.8, "growth {growth:.2}");
}

#[test]
fn every_renamed_cell_of_a_wide_row_is_traced_to_its_source_column() {
    // Renaming every column moves each cell's value to a new key; each
    // source cell is found under its new key, so nothing is dropped or
    // recorded as changed.
    let p = protocol();
    let width = 64;
    let mut src = SchemaBuilder::new(&p).vertex("t", "record", None).unwrap();
    let mut tgt = SchemaBuilder::new(&p).vertex("t", "record", None).unwrap();
    let mut migration = CompiledMigration {
        surviving_verts: BTreeSet::from([Name::from("t")]).into_iter().collect(),
        ..CompiledMigration::default()
    };
    let mut row = HashMap::new();
    let mut expected = HashMap::new();
    for i in 0..width {
        let (old, new) = (format!("c{i}"), format!("d{i}"));
        let (old_v, new_v) = (format!("t.{old}"), format!("t.{new}"));
        src = src
            .vertex(&old_v, "int", None)
            .unwrap()
            .edge("t", &old_v, "prop", Some(&old))
            .unwrap();
        tgt = tgt
            .vertex(&new_v, "int", None)
            .unwrap()
            .edge("t", &new_v, "prop", Some(&new))
            .unwrap();
        migration.surviving_verts.insert(Name::from(new_v.as_str()));
        migration.surviving_edges.insert(column(&new));
        migration
            .vertex_remap
            .insert(Name::from(old_v.as_str()), Name::from(new_v.as_str()));
        migration.edge_remap.insert(column(&old), column(&new));
        row.insert(old, Value::Int(i64::from(i)));
        expected.insert(new, Value::Int(i64::from(i)));
    }
    let plan = RestrictPlan::compile_for_tables(
        &src.build().unwrap(),
        &tgt.build().unwrap(),
        &migration,
        BoundaryProtocols { src: &p, tgt: &p },
    )
    .unwrap();
    let instance = FInstance::new().with_rows("t", [(RowId(0), row)]).unwrap();
    let (view, complement) = functor_restrict_with_plan(&instance, &plan).unwrap();
    assert_eq!(view.row(RowId(0)), Some(&expected));
    assert_eq!(complement.dropped_cells, BTreeMap::new());
    assert_eq!(complement.original_cells, BTreeMap::new());
}
