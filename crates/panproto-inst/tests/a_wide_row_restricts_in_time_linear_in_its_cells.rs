#![allow(clippy::unwrap_used, clippy::expect_used)]
//! The table restrict takes time linear in a row's cells up to a
//! logarithmic factor: comparing each source cell with the view's cell of
//! its name, through the value transforms and cell fates that move and
//! keep keys, costs no scan of the row's other keys.

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

/// What the plan over the wide table does to its columns.
#[derive(Clone, Copy, Debug)]
enum Fate {
    /// The identity: no transform and no cell fate runs, so no key is
    /// tracked at all.
    Identity,
    /// A value transform keeps every column, so every key is tracked
    /// through the kept set.
    Keep,
    /// Every column `ci` is renamed to `di`, so every cell is moved by a
    /// cell fate, and each source cell is a dropped cell, since the view
    /// row holds no cell of its column's name.
    Rename,
}

/// The source table `t` with `width` int columns `c0 ..`, the plan over it
/// that `fate` names (onto the same table, or onto columns `d0 ..` for
/// [`Fate::Rename`]), `rows` rows whose cell `ci` holds `i`, and the row
/// the view holds for each of them.
fn wide(width: usize, rows: u32, fate: Fate) -> (RestrictPlan, FInstance, HashMap<String, Value>) {
    let p = protocol();
    let table = |prefix: &str| {
        let mut builder = SchemaBuilder::new(&p).vertex("t", "record", None).unwrap();
        for i in 0..width {
            let (name, vertex) = (format!("{prefix}{i}"), format!("t.{prefix}{i}"));
            builder = builder
                .vertex(&vertex, "int", None)
                .unwrap()
                .edge("t", &vertex, "prop", Some(&name))
                .unwrap();
        }
        builder.build().unwrap()
    };
    let src: Schema = table("c");
    let target_prefix = if matches!(fate, Fate::Rename) {
        "d"
    } else {
        "c"
    };
    let tgt: Schema = table(target_prefix);
    let mut migration = CompiledMigration {
        surviving_verts: tgt.vertices.keys().cloned().collect(),
        surviving_edges: (0..width)
            .map(|i| column(&format!("{target_prefix}{i}")))
            .collect(),
        ..CompiledMigration::default()
    };
    match fate {
        Fate::Identity => {}
        Fate::Keep => {
            migration.field_transforms.insert(
                "t".into(),
                vec![FieldTransform::KeepFields {
                    keys: (0..width).map(|i| format!("c{i}")).collect(),
                }],
            );
        }
        Fate::Rename => {
            for i in 0..width {
                let (old, new) = (format!("c{i}"), format!("d{i}"));
                migration.vertex_remap.insert(
                    Name::from(format!("t.{old}").as_str()),
                    Name::from(format!("t.{new}").as_str()),
                );
                migration.edge_remap.insert(column(&old), column(&new));
            }
        }
    }
    let plan = RestrictPlan::compile_for_tables(
        &src,
        &tgt,
        &migration,
        BoundaryProtocols { src: &p, tgt: &p },
    )
    .unwrap();
    let cells = |prefix: &str| -> HashMap<String, Value> {
        (0..width)
            .zip(0_i64..)
            .map(|(i, v)| (format!("{prefix}{i}"), Value::Int(v)))
            .collect()
    };
    let row = cells("c");
    let instance = FInstance::new()
        .with_rows("t", (0..rows).map(|id| (RowId(id), row.clone())))
        .unwrap();
    (plan, instance, cells(target_prefix))
}

/// The time of the restrict, after checking that the plan maps the first row to `expected` and that
/// the complement records exactly the cells that disagree with the view's cell of their name: none
/// under [`Fate::Identity`] and [`Fate::Keep`], and every source cell, as a dropped cell, under
/// [`Fate::Rename`].
fn restrict_time(
    plan: &RestrictPlan,
    instance: &FInstance,
    expected: &HashMap<String, Value>,
    fate: Fate,
) -> Duration {
    let start = Instant::now();
    let (view, complement) = functor_restrict_with_plan(instance, plan).unwrap();
    let elapsed = start.elapsed();
    assert_eq!(view.row(RowId(0)), Some(expected));
    if matches!(fate, Fate::Rename) {
        let mut cleared = complement.clone();
        cleared.dropped_cells.clear();
        assert!(cleared.residue_is_trivial(), "{cleared:?}");
        let cells: usize = instance
            .row_ids()
            .into_iter()
            .map(|id| instance.row(id).unwrap().len())
            .sum();
        assert_eq!(complement.dropped_cells.len(), cells);
        for ((id, column), value) in &complement.dropped_cells {
            assert_eq!(instance.row(*id).unwrap().get(column), Some(value));
        }
    } else {
        assert!(complement.residue_is_trivial(), "{complement:?}");
    }
    elapsed
}

/// The growth, from 250 to 4000 columns, of the restrict's time in units
/// of a linear baseline over the same table (one clone of it).
fn growth_from_250_to_4000_columns(fate: Fate) -> f64 {
    let (small_plan, small_table, small_row) = wide(250, 20, fate);
    let (large_plan, large_table, large_row) = wide(4000, 20, fate);
    let (small, large) = common::sizes_in_units(
        7,
        common::Probe {
            work: || restrict_time(&small_plan, &small_table, &small_row, fate),
            unit: || drop(std::hint::black_box(small_table.clone())),
        },
        common::Probe {
            work: || restrict_time(&large_plan, &large_table, &large_row, fate),
            unit: || drop(std::hint::black_box(large_table.clone())),
        },
    );
    let growth = large / small;
    eprintln!(
        "{fate:?}: 250 columns: {small:.1} units; 4000 columns: {large:.1} units; \
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
//
// Each fate guards a different path. The identity tracks no key, so it
// guards only the path on which no transform or cell fate runs; keeping
// every column tracks each key through the kept set; renaming every
// column moves each cell by a cell fate and records each source cell.

#[test]
fn a_wide_row_is_not_charged_per_cell_and_key() {
    let growth = growth_from_250_to_4000_columns(Fate::Identity);
    assert!(growth < 2.8, "growth {growth:.2}");
}

#[test]
fn keeping_the_columns_of_a_wide_row_is_not_charged_per_cell_and_key() {
    let growth = growth_from_250_to_4000_columns(Fate::Keep);
    assert!(growth < 2.8, "growth {growth:.2}");
}

#[test]
fn renaming_the_columns_of_a_wide_row_is_not_charged_per_cell_and_key() {
    let growth = growth_from_250_to_4000_columns(Fate::Rename);
    assert!(growth < 2.8, "growth {growth:.2}");
}

#[test]
fn every_renamed_cell_of_a_wide_row_is_traced_to_its_source_column() {
    // Renaming every column moves each cell's value to a new key; the view
    // row holds no cell named after any source column, so each source cell
    // is recorded under its source column as a dropped cell, with
    // its source value, and none as changed.
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
    assert_eq!(
        complement.dropped_cells,
        (0..width)
            .map(|i| ((RowId(0), format!("c{i}")), Value::Int(i64::from(i))))
            .collect::<BTreeMap<_, _>>()
    );
    assert_eq!(complement.original_cells, BTreeMap::new());
}
